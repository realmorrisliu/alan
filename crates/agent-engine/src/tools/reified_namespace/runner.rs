//! Shared setup/evidence for synchronous probes and cancellable runtime execution.
use super::*;

impl LinuxReifiedNamespaceRunner {
    pub(super) fn run_inner(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let (temp_root, command_spec) = self.prepare(plan)?;
        let (command, capture) = self.captured_command(&temp_root, &command_spec)?;
        let output = run_linux_reified_command_with_capture(command, timeout, Some(capture))
            .map_err(|error| self.error(error.to_string(), command_spec.audit_fields()))?;

        self.finish(&temp_root, &command_spec, output)
    }

    pub(super) async fn run_async_inner(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let runner = self.clone();
        let plan = plan.clone();
        // Only trusted setup/probes run on the blocking worker. Dropping this
        // future can never leave a user command running on that worker.
        let (temp_root, command_spec) = tokio::task::spawn_blocking(move || runner.prepare(&plan))
            .await
            .map_err(|error| self.error(format!("runner setup failed: {error}"), Vec::new()))??;
        let (command, capture) = self.captured_command(&temp_root, &command_spec)?;
        let output = super::super::sandbox::command_process::output_with_capture(
            command.into(),
            timeout,
            Some(capture),
        )
        .await
        .map_err(|error| self.error(format!("{error:#}"), command_spec.audit_fields()))?;
        self.finish(&temp_root, &command_spec, output)
    }

    fn captured_command(
        &self,
        temp: &ReifiedRunnerTemp,
        spec: &ReifiedNamespaceCommandSpec,
    ) -> Result<(Command, (std::fs::File, std::fs::File)), ReifiedNamespaceRunError> {
        // Named FIFOs can be bind-mounted and preserve stream semantics when
        // commands reopen /dev/stdout or /dev/stderr (including with O_TRUNC).
        let (stdout, stdout_reader) = capture_pipe(&temp.parent.join("stdout"))
            .map_err(|error| self.error(error.to_string(), spec.audit_fields()))?;
        let (stderr, stderr_reader) = capture_pipe(&temp.parent.join("stderr"))
            .map_err(|error| self.error(error.to_string(), spec.audit_fields()))?;
        let mut command = spec.command();
        command.stdin(Stdio::null()).stdout(stdout).stderr(stderr);
        Ok((command, (stdout_reader, stderr_reader)))
    }

    fn prepare(
        &self,
        plan: &ReifiedNamespacePlan,
    ) -> Result<(ReifiedRunnerTemp, ReifiedNamespaceCommandSpec), ReifiedNamespaceRunError> {
        if plan.argv.is_empty() {
            return Err(self.error("argv must not be empty", Vec::new()));
        }

        let report = probe_linux_reification();
        let fallback_backend = preferred_linux_backend_with_reification(
            &report,
            matches!(self.fallback_backend, SandboxBackendKind::Landlock),
        );
        if !linux_reification_report_supports_plan(&report, plan.network) {
            return Err(ReifiedNamespaceRunError::new(
                format!(
                    "capability probe did not select reification: {}",
                    linux_reification_unavailable_reasons_for_plan(&report, plan.network)
                        .join("; ")
                ),
                if matches!(fallback_backend, SandboxBackendKind::LinuxReifiedNamespace) {
                    self.fallback_backend
                } else {
                    fallback_backend
                },
                report.audit_fields(),
            ));
        }

        let temp_root = ReifiedRunnerTemp::create(plan)
            .map_err(|err| self.error(format!("create reified root failed: {err}"), Vec::new()))?;
        let command_spec = build_linux_reified_namespace_command(plan, &temp_root)
            .map_err(|err| self.error(err, Vec::new()))?;
        Ok((temp_root, command_spec))
    }

    fn finish(
        &self,
        temp_root: &ReifiedRunnerTemp,
        command_spec: &ReifiedNamespaceCommandSpec,
        output: Output,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        if setup_marker_was_written(&temp_root.setup_marker) {
            return Ok(ExecResult {
                stdout,
                stderr,
                exit_code,
            });
        }

        let reason = if stderr.contains(SETUP_FAILURE_PREFIX) {
            stderr.trim().to_string()
        } else {
            format!("namespace setup failed before command execution: exit_code={exit_code}")
        };
        Err(self.error(reason, command_spec.audit_fields()))
    }

    fn error(
        &self,
        reason: impl Into<String>,
        mut audit_fields: Vec<(&'static str, String)>,
    ) -> ReifiedNamespaceRunError {
        audit_fields.extend([
            (
                "backend",
                SandboxBackendKind::LinuxReifiedNamespace.name().to_string(),
            ),
            ("status", "unavailable".to_string()),
            ("fallback_backend", self.fallback_backend.name().to_string()),
        ]);
        ReifiedNamespaceRunError::new(reason, self.fallback_backend, audit_fields)
    }
}

fn capture_pipe(path: &Path) -> std::io::Result<(std::fs::File, std::fs::File)> {
    use std::os::fd::AsRawFd;
    use std::os::unix::{ffi::OsStrExt, fs::OpenOptionsExt};
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    // SAFETY: name is a valid NUL-terminated path in our private directory.
    if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // Establish both endpoints without blocking on the other endpoint's open.
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let writer = std::fs::OpenOptions::new().write(true).open(path)?;
    // SAFETY: reader owns this open descriptor; synchronous readers need blocking IO.
    if unsafe { libc::fcntl(reader.as_raw_fd(), libc::F_SETFL, 0) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok((writer, reader))
}

#[cfg(target_os = "linux")]
pub(super) const LINUX_REIFIED_NAMESPACE_SCRIPT: &str = r#"
set -u
PATH='/usr/sbin:/usr/bin:/sbin:/bin'
export PATH
fail() {
  printf '%s %s\n' 'alan reified namespace setup failed:' "$*" >&2
  exit 125
}

root="$1"; shift
setup_marker="$1"; shift
mount_bin="$1"; shift
printf_bin="$1"; shift
chroot_bin="$1"; shift
namespace_shell="$1"; shift
namespace_setpriv="$1"; shift
command_script="$1"; shift

"$mount_bin" --make-rprivate / || fail "make root private"
"$mount_bin" --bind "$root" "$root" || fail "bind root"
"$mount_bin" -o remount,bind,ro "$root" || fail "remount root read-only"

scratch_tmp="$1"; shift
scratch_destination="${root}${scratch_tmp}"
"$mount_bin" -t tmpfs tmpfs "$scratch_destination" || fail "mount scratch tmp"

mount_count="$1"; shift
while [ "$mount_count" -gt 0 ]; do
  namespace_path="$1"; shift
  host_path="$1"; shift
  access="$1"; shift
  destination="${root}${namespace_path}"
  if [ ! -e "$destination" ]; then
    if [ -f "$host_path" ]; then
      mkdir -p "${destination%/*}" && : > "$destination" || fail "prepare file ${namespace_path}"
    else
      mkdir -p "$destination" || fail "prepare directory ${namespace_path}"
    fi
  fi
  "$mount_bin" --rbind "$host_path" "$destination" || fail "bind mount ${namespace_path}"
  if [ "$access" = "read_only" ]; then
    readonly_tree "$destination" || fail "remount ${namespace_path} recursively read-only"
  fi
  mount_count=$((mount_count - 1))
done

substrate_count="$1"; shift
while [ "$substrate_count" -gt 0 ]; do
  namespace_path="$1"; shift
  host_path="$1"; shift
  destination="${root}${namespace_path}"
  "$mount_bin" --rbind "$host_path" "$destination" || fail "bind substrate ${namespace_path}"
  readonly_tree "$destination" || fail "remount substrate ${namespace_path} recursively read-only"
  substrate_count=$((substrate_count - 1))
done

"$mount_bin" --bind /dev/null "${root}/dev/null" || fail "bind /dev/null"
"$mount_bin" --bind /proc/self/fd/0 "${root}/dev/stdin" || fail "bind /dev/stdin"
"$mount_bin" --bind /proc/self/fd/1 "${root}/dev/stdout" || fail "bind /dev/stdout"
"$mount_bin" --bind /proc/self/fd/2 "${root}/dev/stderr" || fail "bind /dev/stderr"
"$mount_bin" -t proc -o ro,nosuid,nodev,noexec proc "${root}/proc" || fail "mount private proc"

cwd="$1"; shift
command_path="$1"; shift
private_env="$1"; shift
private_output="$1"; shift
mkdir -m 700 "${root}${private_env}" || fail "create private environment"
for name in home cargo rustup tmp cache target; do
  mkdir -m 700 "${root}${private_env}/${name}" || fail "create private ${name}"
done
exec "$chroot_bin" "$root" "$namespace_shell" -c "$command_script" alan-reified-command "$cwd" "$command_path" "$private_env" "$private_output" "$namespace_setpriv" "$namespace_shell" "$@" 3>"$setup_marker"
"#;

#[cfg(target_os = "linux")]
pub(super) const LINUX_REIFIED_COMMAND_SCRIPT: &str = r#"
cd "$1" || exit 126; shift
PATH="$1"; shift
private_env="$1"; shift
private_output="$1"; shift
HOME="${private_env}/home"
CARGO_HOME="${private_env}/cargo"
RUSTUP_HOME="${private_env}/rustup"
TMPDIR="${private_env}/tmp"
XDG_CACHE_HOME="${private_env}/cache"
RUSTUP_AUTO_INSTALL=0
export PATH HOME CARGO_HOME RUSTUP_HOME TMPDIR XDG_CACHE_HOME RUSTUP_AUTO_INSTALL
if [ "$private_output" = 1 ]; then
  CARGO_TARGET_DIR="${private_env}/target"
  export CARGO_TARGET_DIR
fi
setpriv_bin="$1"; shift
shell_bin="$1"; shift
exec "$setpriv_bin" --no-new-privs --bounding-set=-all --inh-caps=-all --ambient-caps=-all "$shell_bin" -c 'printf "%s\n" ok >&3 || exit 125; exec 3>&-; exec "$@"' alan-reified-command "$@"
"#;
