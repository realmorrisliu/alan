//! Pure planning model for Linux reified namespace execution.
//!
//! This module does not create namespaces or perform mounts. It translates the
//! host-backed mount authority that the native runner will consume into a stable
//! plan that can be tested on any host.

#[cfg(target_os = "linux")]
use std::io::Read;
#[cfg(any(target_os = "linux", all(test, unix)))]
use std::os::unix::fs::PermissionsExt;
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Child, Command, Output, Stdio};
#[cfg(target_os = "linux")]
use std::thread::JoinHandle;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::Instant;

use thiserror::Error;

use super::sandbox::{ExecResult, NetworkPosture};
#[cfg(test)]
use super::sandbox_backend::LinuxReificationCapabilities;
#[cfg(any(test, target_os = "linux"))]
use super::sandbox_backend::{LinuxReificationCapability, LinuxReificationCapabilityReport};
use super::sandbox_backend::{SandboxBackendKind, detect_backend};
#[cfg(target_os = "linux")]
use super::sandbox_backend::{preferred_linux_backend_with_reification, probe_linux_reification};

/// Default namespace path for the workspace seed mount.
pub const DEFAULT_WORKSPACE_NAMESPACE_PATH: &str = "/mnt/workspace";

/// Default namespace path for the private scratch/tmp mount.
pub const DEFAULT_SCRATCH_TMP_NAMESPACE_PATH: &str = "/tmp";

/// Access mode for a declared host-backed mount in the reified namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReifiedMountAccess {
    ReadOnly,
    ReadWrite,
}

impl ReifiedMountAccess {
    /// Stable audit label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }

    /// Whether the mount permits writes.
    pub const fn is_writable(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

/// Source behind a namespace declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReifiedMountSource {
    /// A host path that can be bind-mounted into the native subprocess view.
    Host(PathBuf),
    /// A pure Alan OS/aP file tree that is not exposed as a native filesystem path.
    Virtual,
}

/// Mount declaration supplied by sandbox authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedMountDeclaration {
    pub namespace_path: PathBuf,
    pub source: ReifiedMountSource,
    pub access: ReifiedMountAccess,
}

impl ReifiedMountDeclaration {
    /// Declare a host-backed namespace mount.
    pub fn host(
        namespace_path: impl Into<PathBuf>,
        host_path: impl Into<PathBuf>,
        access: ReifiedMountAccess,
    ) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            source: ReifiedMountSource::Host(host_path.into()),
            access,
        }
    }

    /// Declare a virtual Alan OS mount that must be excluded from native reification.
    pub fn virtual_mount(namespace_path: impl Into<PathBuf>) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            source: ReifiedMountSource::Virtual,
            access: ReifiedMountAccess::ReadOnly,
        }
    }
}

/// Host-backed mount included in the reified subprocess view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedHostMount {
    pub namespace_path: PathBuf,
    pub host_path: PathBuf,
    pub access: ReifiedMountAccess,
}

/// Read-only host path needed to execute common programs inside the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedExecutionSubstrateMount {
    pub namespace_path: PathBuf,
    pub host_path: PathBuf,
}

impl ReifiedExecutionSubstrateMount {
    /// Declare a read-only execution substrate mount.
    pub fn new(namespace_path: impl Into<PathBuf>, host_path: impl Into<PathBuf>) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            host_path: host_path.into(),
        }
    }
}

/// Private scratch/tmp mount inside the reified view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedScratchTmpMount {
    pub namespace_path: PathBuf,
}

/// Pure plan consumed by the future Linux namespace runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedNamespacePlan {
    pub declared_host_mounts: Vec<ReifiedHostMount>,
    pub execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub scratch_tmp: ReifiedScratchTmpMount,
    pub network: NetworkPosture,
}

impl ReifiedNamespacePlan {
    /// Derive a plan from namespace mount authority and a projected host cwd.
    pub fn derive(input: ReifiedNamespacePlanInput) -> Result<Self, ReifiedNamespacePlanError> {
        let mut declared_host_mounts = Vec::new();
        let mut virtual_namespace_paths = Vec::new();
        for declaration in input.declarations {
            validate_namespace_path(&declaration.namespace_path)?;
            match declaration.source {
                ReifiedMountSource::Host(host_path) => {
                    let host_path =
                        validate_and_normalize_host_source_path(&host_path, "host mount")?;
                    declared_host_mounts.push(ReifiedHostMount {
                        namespace_path: declaration.namespace_path,
                        host_path,
                        access: declaration.access,
                    });
                }
                ReifiedMountSource::Virtual => {
                    virtual_namespace_paths.push(declaration.namespace_path);
                }
            }
        }
        let mut execution_substrate = input.execution_substrate;
        for mount in &mut execution_substrate {
            validate_namespace_path(&mount.namespace_path)?;
            mount.host_path =
                validate_and_normalize_host_source_path(&mount.host_path, "execution substrate")?;
        }
        validate_namespace_path(&input.scratch_tmp_namespace_path)?;
        validate_absolute_path(&input.cwd, "cwd")?;
        let projected_cwd = canonicalize_existing_host_path(&input.cwd);

        let mut namespace_paths = declared_host_mounts
            .iter()
            .map(|mount| mount.namespace_path.as_path())
            .collect::<Vec<_>>();
        namespace_paths.extend(virtual_namespace_paths.iter().map(|path| path.as_path()));
        namespace_paths.extend(
            execution_substrate
                .iter()
                .map(|mount| mount.namespace_path.as_path()),
        );
        namespace_paths.push(input.scratch_tmp_namespace_path.as_path());
        validate_no_overlapping_namespace_paths(&namespace_paths)?;
        validate_no_mixed_access_host_mount_overlap(&declared_host_mounts)?;
        validate_no_writable_mount_over_execution_substrate(
            &declared_host_mounts,
            &execution_substrate,
        )?;

        let cwd = translate_host_path_with_mounts(&declared_host_mounts, &projected_cwd)
            .ok_or_else(|| ReifiedNamespacePlanError::CwdOutsideView {
                cwd: input.cwd.clone(),
            })?;

        Ok(Self {
            declared_host_mounts,
            execution_substrate,
            cwd,
            argv: input.argv,
            scratch_tmp: ReifiedScratchTmpMount {
                namespace_path: input.scratch_tmp_namespace_path,
            },
            network: input.network,
        })
    }

    /// Build a plan for the default workspace seed mount.
    pub fn workspace_seed(
        workspace_root: impl Into<PathBuf>,
        cwd: impl Into<PathBuf>,
        argv: Vec<String>,
        network: NetworkPosture,
    ) -> Result<Self, ReifiedNamespacePlanError> {
        let workspace_root = workspace_root.into();
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                DEFAULT_WORKSPACE_NAMESPACE_PATH,
                workspace_root,
                ReifiedMountAccess::ReadWrite,
            )],
            cwd,
            argv,
            network,
        );
        Self::derive(input)
    }

    /// Translate a projected host path into the reified namespace view.
    pub fn translate_projected_host_path(&self, host_path: &Path) -> Option<PathBuf> {
        if !host_path.is_absolute() || contains_parent_component(host_path) {
            return None;
        }

        let projected_host_path = canonicalize_existing_host_path(host_path);
        translate_host_path_with_mounts(&self.declared_host_mounts, &projected_host_path)
    }
}

/// Input used to derive a reified namespace plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedNamespacePlanInput {
    pub declarations: Vec<ReifiedMountDeclaration>,
    pub cwd: PathBuf,
    pub argv: Vec<String>,
    pub network: NetworkPosture,
    pub execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    pub scratch_tmp_namespace_path: PathBuf,
}

impl ReifiedNamespacePlanInput {
    /// Create input with the default Linux execution substrate and scratch mount.
    pub fn new(
        declarations: Vec<ReifiedMountDeclaration>,
        cwd: impl Into<PathBuf>,
        argv: Vec<String>,
        network: NetworkPosture,
    ) -> Self {
        Self {
            declarations,
            cwd: cwd.into(),
            argv,
            network,
            execution_substrate: default_execution_substrate(),
            scratch_tmp_namespace_path: PathBuf::from(DEFAULT_SCRATCH_TMP_NAMESPACE_PATH),
        }
    }

    /// Override the read-only execution substrate list.
    pub fn with_execution_substrate(
        mut self,
        execution_substrate: Vec<ReifiedExecutionSubstrateMount>,
    ) -> Self {
        self.execution_substrate = execution_substrate;
        self
    }

    /// Override the scratch/tmp namespace path.
    pub fn with_scratch_tmp_namespace_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.scratch_tmp_namespace_path = path.into();
        self
    }
}

/// Stable default execution substrate. Each entry is mounted read-only by the runner.
pub fn default_execution_substrate() -> Vec<ReifiedExecutionSubstrateMount> {
    [
        ("/bin", "/bin"),
        ("/sbin", "/sbin"),
        ("/usr/bin", "/usr/bin"),
        ("/usr/sbin", "/usr/sbin"),
        ("/usr/local/bin", "/usr/local/bin"),
        ("/usr/local/sbin", "/usr/local/sbin"),
        ("/lib", "/lib"),
        ("/lib64", "/lib64"),
        ("/usr/lib", "/usr/lib"),
        ("/usr/lib64", "/usr/lib64"),
        ("/usr/local/lib", "/usr/local/lib"),
        ("/usr/local/lib64", "/usr/local/lib64"),
        ("/etc/ssl", "/etc/ssl"),
        ("/etc/hosts", "/etc/hosts"),
        ("/etc/resolv.conf", "/etc/resolv.conf"),
    ]
    .into_iter()
    .map(|(namespace_path, host_path)| {
        ReifiedExecutionSubstrateMount::new(namespace_path, host_path)
    })
    .collect()
}

/// Errors produced while deriving a pure reified namespace plan.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ReifiedNamespacePlanError {
    #[error("{kind} path must be absolute: {path:?}")]
    RelativePath { kind: &'static str, path: PathBuf },
    #[error("{kind} path must not contain '..': {path:?}")]
    ParentPath { kind: &'static str, path: PathBuf },
    #[error("namespace path must not be root")]
    RootNamespacePath,
    #[error("{kind} path must not be root")]
    RootHostSourcePath { kind: &'static str },
    #[error("namespace mount path overlaps another mount: {child:?} shadows {parent:?}")]
    NamespaceMountOverlap { parent: PathBuf, child: PathBuf },
    #[error(
        "read-only host mount {read_only_host_path:?} overlaps writable mount {writable_host_path:?}"
    )]
    ReadOnlyHostMountOverlapsWritableMount {
        read_only_host_path: PathBuf,
        writable_host_path: PathBuf,
    },
    #[error(
        "writable host mount {writable_host_path:?} overlaps execution substrate {substrate_host_path:?}"
    )]
    WritableHostMountOverlapsExecutionSubstrate {
        writable_host_path: PathBuf,
        substrate_host_path: PathBuf,
    },
    #[error("cwd is outside the reified host mount view: {cwd:?}")]
    CwdOutsideView { cwd: PathBuf },
}

/// Runner abstraction for an opt-in reified namespace backend.
pub trait ReifiedNamespaceRunner: std::fmt::Debug + Send + Sync {
    /// Run the command described by a reified namespace plan.
    fn run(&self, plan: &ReifiedNamespacePlan) -> Result<ExecResult, ReifiedNamespaceRunError>;
}

/// Opt-in Linux implementation backed by unprivileged user/mount namespaces.
#[derive(Debug, Clone)]
pub struct LinuxReifiedNamespaceRunner {
    fallback_backend: SandboxBackendKind,
}

impl Default for LinuxReifiedNamespaceRunner {
    fn default() -> Self {
        Self {
            fallback_backend: detect_backend(),
        }
    }
}

impl LinuxReifiedNamespaceRunner {
    /// Create a runner that reports the supplied backend when reification cannot run.
    pub const fn with_fallback_backend(fallback_backend: SandboxBackendKind) -> Self {
        Self { fallback_backend }
    }

    /// Backend that callers should use when this opt-in runner reports unavailable.
    pub const fn fallback_backend(&self) -> SandboxBackendKind {
        self.fallback_backend
    }

    /// Run the command and terminate the Linux runner PID namespace if the timeout expires.
    pub fn run_with_timeout(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        self.run_inner(plan, timeout)
    }
}

impl ReifiedNamespaceRunner for LinuxReifiedNamespaceRunner {
    fn run(&self, plan: &ReifiedNamespacePlan) -> Result<ExecResult, ReifiedNamespaceRunError> {
        self.run_with_timeout(plan, None)
    }
}

/// Error returned when reified namespace execution cannot run safely.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error(
    "linux_reified_namespace unavailable: {reason}; fallback_backend={}",
    fallback_backend.name()
)]
pub struct ReifiedNamespaceRunError {
    pub reason: String,
    pub fallback_backend: SandboxBackendKind,
    pub audit_fields: Vec<(&'static str, String)>,
}

impl ReifiedNamespaceRunError {
    fn new(
        reason: impl Into<String>,
        fallback_backend: SandboxBackendKind,
        audit_fields: Vec<(&'static str, String)>,
    ) -> Self {
        Self {
            reason: reason.into(),
            fallback_backend,
            audit_fields,
        }
    }
}

/// Command line produced for the Linux helper path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReifiedNamespaceCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl ReifiedNamespaceCommandSpec {
    #[cfg(target_os = "linux")]
    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command.env_clear();
        command.env("PATH", TRUSTED_LINUX_SETUP_PATH);
        command
    }
}

/// Smoke-check the actual Linux reified namespace runner.
#[cfg(target_os = "linux")]
pub(crate) fn smoke_linux_reified_namespace_runner() -> LinuxReificationCapability {
    match smoke_linux_reified_namespace_runner_inner() {
        Ok(()) => LinuxReificationCapability::available(),
        Err(reason) => LinuxReificationCapability::unavailable(reason),
    }
}

/// Smoke-check that selecting reified mode will not hide user PATH toolchains.
#[cfg(target_os = "linux")]
pub(crate) fn smoke_linux_reified_namespace_user_path() -> LinuxReificationCapability {
    match reified_namespace_user_path_unavailable_reason(std::env::var_os("PATH")) {
        Some(reason) => LinuxReificationCapability::unavailable(reason),
        None => LinuxReificationCapability::available(),
    }
}

#[cfg(target_os = "linux")]
fn reified_namespace_user_path_unavailable_reason(
    path: Option<std::ffi::OsString>,
) -> Option<String> {
    let visible_roots = reified_command_path_roots();
    reified_namespace_user_path_unavailable_reason_with_roots(
        path,
        &visible_roots,
        std::ffi::OsString::from(LINUX_REIFIED_COMMAND_PATH),
    )
}

#[cfg(target_os = "linux")]
fn reified_command_path_roots() -> Vec<PathBuf> {
    std::env::split_paths(LINUX_REIFIED_COMMAND_PATH)
        .map(|path| canonicalize_existing_host_path(&path))
        .collect()
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn reified_namespace_user_path_unavailable_reason_with_roots(
    path: Option<std::ffi::OsString>,
    visible_roots: &[PathBuf],
    reified_command_path: std::ffi::OsString,
) -> Option<String> {
    let Some(path) = path else {
        return Some(
            "current PATH is unset; preserve actual PATH/order before selecting \
             linux_reified_namespace"
                .to_string(),
        );
    };
    let visible_roots = visible_roots
        .iter()
        .map(|root| canonicalize_existing_host_path(root))
        .collect::<Vec<_>>();
    let mut unsupported = Vec::new();
    let mut current_executable_entries = Vec::new();

    for entry in std::env::split_paths(&path) {
        if entry.as_os_str().is_empty() {
            return Some(
                "current PATH contains an empty component for current-directory lookup; preserve \
                 actual PATH/order before selecting linux_reified_namespace"
                    .to_string(),
            );
        }
        if !entry.is_absolute() {
            unsupported.push(format!("relative PATH entry {}", entry.display()));
            continue;
        }

        let entry = canonicalize_existing_host_path(&entry);
        if !path_directory_has_executables(&entry) {
            continue;
        }
        if visible_roots
            .iter()
            .any(|root| entry == *root || entry.starts_with(root))
        {
            push_unique_path(&mut current_executable_entries, entry);
            continue;
        }

        unsupported.push(entry.display().to_string());
        if unsupported.len() >= 3 {
            break;
        }
    }

    if !unsupported.is_empty() {
        return Some(format!(
            "current PATH has executable entries outside the reified execution substrate: {}; \
             preserve user PATH/toolchain mounts before selecting linux_reified_namespace",
            unsupported.join(", ")
        ));
    }

    let reified_executable_entries = executable_path_entries(&reified_command_path);
    if current_executable_entries != reified_executable_entries {
        return Some(format!(
            "current PATH executable entry order differs from the reified command PATH: \
             current=[{}], reified=[{}]; preserve actual PATH/order before selecting \
             linux_reified_namespace",
            format_path_entries(&current_executable_entries),
            format_path_entries(&reified_executable_entries)
        ));
    }

    None
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn path_directory_has_executables(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return true;
    };
    entries.filter_map(Result::ok).any(|entry| {
        std::fs::metadata(entry.path())
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn executable_path_entries(path: &std::ffi::OsString) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for entry in std::env::split_paths(path) {
        if entry.as_os_str().is_empty() || !entry.is_absolute() {
            continue;
        }
        let entry = canonicalize_existing_host_path(&entry);
        if path_directory_has_executables(&entry) {
            push_unique_path(&mut entries, entry);
        }
    }
    entries
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn push_unique_path(entries: &mut Vec<PathBuf>, entry: PathBuf) {
    if !entries.contains(&entry) {
        entries.push(entry);
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn format_path_entries(entries: &[PathBuf]) -> String {
    if entries.is_empty() {
        return "<none>".to_string();
    }
    entries
        .iter()
        .map(|entry| entry.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(target_os = "linux")]
fn smoke_linux_reified_namespace_runner_inner() -> Result<(), String> {
    let workspace = ReifiedSmokeWorkspace::create()
        .map_err(|err| format!("create runner smoke workspace failed: {err}"))?;
    let runner = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock);

    let deny_plan = ReifiedNamespacePlan::workspace_seed(
        workspace.root.as_path(),
        workspace.root.as_path(),
        vec![
            "sh".to_string(),
            "-c".to_string(),
            "test -d /mnt/workspace && test ! -e /home".to_string(),
        ],
        NetworkPosture::Deny,
    )
    .map_err(|err| format!("build runner smoke plan failed: {err}"))?;
    run_linux_reified_smoke_plan(&runner, &deny_plan, "network-denied")?;

    let allow_script = if Path::new("/etc/resolv.conf").exists() {
        "test -f /etc/resolv.conf"
    } else {
        "true"
    };
    let allow_plan = ReifiedNamespacePlan::workspace_seed(
        workspace.root.as_path(),
        workspace.root.as_path(),
        vec!["sh".to_string(), "-c".to_string(), allow_script.to_string()],
        NetworkPosture::Allow,
    )
    .map_err(|err| format!("build runner network-allow smoke plan failed: {err}"))?;
    run_linux_reified_smoke_plan(&runner, &allow_plan, "network-allow")
}

#[cfg(target_os = "linux")]
fn run_linux_reified_smoke_plan(
    runner: &LinuxReifiedNamespaceRunner,
    plan: &ReifiedNamespacePlan,
    label: &str,
) -> Result<(), String> {
    match runner.run(plan) {
        Ok(result) if result.exit_code == 0 => Ok(()),
        Ok(result) => Err(format!(
            "runner {label} smoke command failed: exit_code={} stderr={}",
            result.exit_code,
            result.stderr.trim()
        )),
        Err(err) => Err(format!("runner {label} smoke unavailable: {}", err.reason)),
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct ReifiedSmokeWorkspace {
    root: PathBuf,
}

#[cfg(target_os = "linux")]
impl ReifiedSmokeWorkspace {
    fn create() -> std::io::Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "alan-reified-smoke-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
}

#[cfg(target_os = "linux")]
impl Drop for ReifiedSmokeWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(target_os = "linux")]
const SETUP_FAILURE_PREFIX: &str = "alan reified namespace setup failed:";

#[cfg(target_os = "linux")]
const TRUSTED_LINUX_SETUP_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin";

#[cfg(any(target_os = "linux", test))]
macro_rules! linux_reified_command_path {
    () => {
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    };
}

#[cfg(any(target_os = "linux", test))]
const LINUX_REIFIED_COMMAND_PATH: &str = linux_reified_command_path!();

#[cfg(target_os = "linux")]
const LINUX_REIFIED_NAMESPACE_SCRIPT: &str = concat!(
    r#"
set -u
PATH='"#,
    linux_reified_command_path!(),
    r#"'
export PATH
fail() {
  printf '%s %s\n' 'alan reified namespace setup failed:' "$*" >&2
  exit 125
}

root="$1"; shift
setup_marker="$1"; shift
mount_bin="$1"; shift
chroot_bin="$1"; shift
namespace_shell="$1"; shift
namespace_setpriv="$1"; shift

"$mount_bin" --make-rprivate / || fail "make root private"
"$mount_bin" --bind "$root" "$root" || fail "bind root"
"$mount_bin" -o remount,bind,ro "$root" || fail "remount root read-only"

mount_count="$1"; shift
while [ "$mount_count" -gt 0 ]; do
  namespace_path="$1"; shift
  host_path="$1"; shift
  access="$1"; shift
  destination="${root}${namespace_path}"
  "$mount_bin" --bind "$host_path" "$destination" || fail "bind mount ${namespace_path}"
  if [ "$access" = "read_only" ]; then
    "$mount_bin" -o remount,bind,ro "$destination" || fail "remount ${namespace_path} read-only"
  fi
  mount_count=$((mount_count - 1))
done

substrate_count="$1"; shift
while [ "$substrate_count" -gt 0 ]; do
  namespace_path="$1"; shift
  host_path="$1"; shift
  destination="${root}${namespace_path}"
  "$mount_bin" --bind "$host_path" "$destination" || fail "bind substrate ${namespace_path}"
  "$mount_bin" -o remount,bind,ro "$destination" || fail "remount substrate ${namespace_path} read-only"
  substrate_count=$((substrate_count - 1))
done

"$mount_bin" --bind /dev/null "${root}/dev/null" || fail "bind /dev/null"
"$mount_bin" --bind /proc/self/fd/0 "${root}/dev/stdin" || fail "bind /dev/stdin"
"$mount_bin" --bind /proc/self/fd/1 "${root}/dev/stdout" || fail "bind /dev/stdout"
"$mount_bin" --bind /proc/self/fd/2 "${root}/dev/stderr" || fail "bind /dev/stderr"

scratch_tmp="$1"; shift
scratch_destination="${root}${scratch_tmp}"
"$mount_bin" -t tmpfs tmpfs "$scratch_destination" || fail "mount scratch tmp"

cwd="$1"; shift
"$chroot_bin" "$root" "$namespace_shell" -c 'cd "$1" || exit 126; shift; setpriv_bin="$1"; shift; shell_bin="$1"; shift; exec "$setpriv_bin" --no-new-privs --bounding-set=-all --inh-caps=-all --ambient-caps=-all "$shell_bin" -c '"'"'printf "%s\n" ok >&3 || exit 125; exec 3>&-; exec "$@"'"'"' alan-reified-command "$@"' alan-reified-command "$cwd" "$namespace_setpriv" "$namespace_shell" "$@" 3>"$setup_marker"
"#,
);

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
struct LinuxSetupHelpers {
    unshare: PathBuf,
    host_shell: PathBuf,
    mount: PathBuf,
    chroot: PathBuf,
    namespace_shell: PathBuf,
    namespace_setpriv: PathBuf,
}

#[cfg(target_os = "linux")]
impl LinuxSetupHelpers {
    fn resolve() -> Result<Self, String> {
        Ok(Self {
            unshare: resolve_trusted_linux_helper(
                "unshare",
                &["/usr/bin/unshare", "/bin/unshare"],
            )?,
            host_shell: resolve_trusted_linux_helper("sh", &["/bin/sh", "/usr/bin/sh"])?,
            mount: resolve_trusted_linux_helper("mount", &["/usr/bin/mount", "/bin/mount"])?,
            chroot: resolve_trusted_linux_helper(
                "chroot",
                &[
                    "/usr/sbin/chroot",
                    "/usr/bin/chroot",
                    "/sbin/chroot",
                    "/bin/chroot",
                ],
            )?,
            namespace_shell: resolve_trusted_linux_helper("sh", &["/bin/sh", "/usr/bin/sh"])?,
            namespace_setpriv: resolve_trusted_linux_helper(
                "setpriv",
                &["/usr/bin/setpriv", "/bin/setpriv"],
            )?,
        })
    }
}

#[cfg(not(target_os = "linux"))]
impl LinuxReifiedNamespaceRunner {
    fn run_inner(
        &self,
        _plan: &ReifiedNamespacePlan,
        _timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
        Err(ReifiedNamespaceRunError::new(
            "linux reified namespace runner is only available on Linux",
            self.fallback_backend,
            vec![
                (
                    "backend",
                    SandboxBackendKind::LinuxReifiedNamespace.name().to_string(),
                ),
                ("status", "unavailable".to_string()),
                ("reason", "not a linux host".to_string()),
                ("fallback_backend", self.fallback_backend.name().to_string()),
            ],
        ))
    }
}

#[cfg(target_os = "linux")]
impl LinuxReifiedNamespaceRunner {
    fn run_inner(
        &self,
        plan: &ReifiedNamespacePlan,
        timeout: Option<Duration>,
    ) -> Result<ExecResult, ReifiedNamespaceRunError> {
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
        let output = run_linux_reified_command(command_spec.command(), timeout).map_err(|err| {
            let reason = if err.kind() == std::io::ErrorKind::TimedOut {
                err.to_string()
            } else {
                format!("failed to run unshare: {err}")
            };
            self.error(reason, command_spec.audit_fields())
        })?;

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

#[cfg(target_os = "linux")]
fn run_linux_reified_command(
    mut command: Command,
    timeout: Option<Duration>,
) -> std::io::Result<Output> {
    let Some(limit) = timeout else {
        return command.output();
    };

    run_linux_reified_command_with_timeout(command, limit)
}

#[cfg(target_os = "linux")]
fn run_linux_reified_command_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> std::io::Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.process_group(0);
    let mut child = command.spawn()?;
    let stdout_reader = child.stdout.take().map(read_child_pipe);
    let stderr_reader = child.stderr.take().map(read_child_pipe);

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            return Err(timeout_reified_command(
                &mut child,
                stdout_reader,
                stderr_reader,
                timeout,
            ));
        }
        let remaining = timeout.saturating_sub(started.elapsed());
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    };
    let (stdout, stderr) = collect_child_pipes_before_deadline(
        stdout_reader,
        stderr_reader,
        &mut child,
        started,
        timeout,
    )?;

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(target_os = "linux")]
fn read_child_pipe<R>(mut pipe: R) -> JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut output = Vec::new();
        pipe.read_to_end(&mut output)?;
        Ok(output)
    })
}

#[cfg(target_os = "linux")]
fn join_child_pipe(
    handle: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
) -> std::io::Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };
    handle
        .join()
        .map_err(|_| std::io::Error::other("child output reader panicked"))?
}

#[cfg(target_os = "linux")]
fn collect_child_pipes_before_deadline(
    stdout_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    child: &mut Child,
    started: Instant,
    timeout: Duration,
) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut stdout_reader = stdout_reader;
    let mut stderr_reader = stderr_reader;
    loop {
        let stdout_done = stdout_reader.as_ref().is_none_or(JoinHandle::is_finished);
        let stderr_done = stderr_reader.as_ref().is_none_or(JoinHandle::is_finished);
        if stdout_done && stderr_done {
            return Ok((
                join_child_pipe(stdout_reader.take())?,
                join_child_pipe(stderr_reader.take())?,
            ));
        }

        if started.elapsed() >= timeout {
            return Err(timeout_reified_command(
                child,
                stdout_reader.take(),
                stderr_reader.take(),
                timeout,
            ));
        }

        let remaining = timeout.saturating_sub(started.elapsed());
        std::thread::sleep(remaining.min(Duration::from_millis(20)));
    }
}

#[cfg(target_os = "linux")]
fn timeout_reified_command(
    child: &mut Child,
    stdout_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr_reader: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    timeout: Duration,
) -> std::io::Error {
    let _ = kill_child_process_group(child);
    let _ = child.wait();
    join_finished_child_pipe(stdout_reader);
    join_finished_child_pipe(stderr_reader);
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("Command execution timed out after {}s", timeout.as_secs()),
    )
}

#[cfg(target_os = "linux")]
fn join_finished_child_pipe(handle: Option<JoinHandle<std::io::Result<Vec<u8>>>>) {
    let Some(handle) = handle else {
        return;
    };
    if handle.is_finished() {
        let _ = join_child_pipe(Some(handle));
    }
}

#[cfg(target_os = "linux")]
fn kill_child_process_group(child: &mut std::process::Child) -> std::io::Result<()> {
    let pgid = -(child.id() as libc::pid_t);
    let result = unsafe { libc::kill(pgid, libc::SIGKILL) };
    if result == 0 { Ok(()) } else { child.kill() }
}

#[cfg(any(test, target_os = "linux"))]
fn linux_reification_report_supports_plan(
    report: &LinuxReificationCapabilityReport,
    network: NetworkPosture,
) -> bool {
    report.linux_host.is_available()
        && report.user_namespace.is_available()
        && report.mount_namespace.is_available()
        && report.pid_namespace.is_available()
        && report.bind_mount.is_available()
        && report.read_only_remount.is_available()
        && report.scratch_tmp_mount.is_available()
        && (matches!(network, NetworkPosture::Allow) || report.network_confinement.is_available())
}

#[cfg(any(test, target_os = "linux"))]
fn linux_reification_unavailable_reasons_for_plan(
    report: &LinuxReificationCapabilityReport,
    network: NetworkPosture,
) -> Vec<String> {
    let mut reasons = Vec::new();
    push_missing_linux_reification_requirement(&mut reasons, "linux_host", &report.linux_host);
    push_missing_linux_reification_requirement(
        &mut reasons,
        "user_namespace",
        &report.user_namespace,
    );
    push_missing_linux_reification_requirement(
        &mut reasons,
        "mount_namespace",
        &report.mount_namespace,
    );
    push_missing_linux_reification_requirement(
        &mut reasons,
        "pid_namespace",
        &report.pid_namespace,
    );
    push_missing_linux_reification_requirement(&mut reasons, "bind_mount", &report.bind_mount);
    push_missing_linux_reification_requirement(
        &mut reasons,
        "read_only_remount",
        &report.read_only_remount,
    );
    push_missing_linux_reification_requirement(
        &mut reasons,
        "scratch_tmp_mount",
        &report.scratch_tmp_mount,
    );
    if matches!(network, NetworkPosture::Deny) {
        push_missing_linux_reification_requirement(
            &mut reasons,
            "network_confinement",
            &report.network_confinement,
        );
    }
    reasons
}

#[cfg(any(test, target_os = "linux"))]
fn push_missing_linux_reification_requirement(
    reasons: &mut Vec<String>,
    name: &'static str,
    capability: &LinuxReificationCapability,
) {
    if !capability.is_available() {
        let reason = capability.reason().unwrap_or("no reason reported");
        reasons.push(format!("{name}: {reason}"));
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct ReifiedRunnerTemp {
    parent: PathBuf,
    root: PathBuf,
    setup_marker: PathBuf,
}

#[cfg(target_os = "linux")]
impl ReifiedRunnerTemp {
    fn create(plan: &ReifiedNamespacePlan) -> std::io::Result<Self> {
        let mut last_error = None;
        for base in reified_runner_temp_base_candidates() {
            let base = canonicalize_existing_host_path(&base);
            for _ in 0..8 {
                let parent = base.join(format!(
                    "alan-reified-runner-{}-{}",
                    std::process::id(),
                    uuid::Uuid::new_v4()
                ));
                if temp_parent_is_exposed_to_writable_mount(&parent, &plan.declared_host_mounts) {
                    continue;
                }

                let root = parent.join("root");
                match std::fs::create_dir_all(&root) {
                    Ok(()) => {
                        return Ok(Self {
                            setup_marker: parent.join("setup-ok"),
                            parent,
                            root,
                        });
                    }
                    Err(err) => last_error = Some(err),
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "no reified runner temp directory outside writable mounts",
            )
        }))
    }
}

#[cfg(target_os = "linux")]
impl Drop for ReifiedRunnerTemp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.parent);
    }
}

#[cfg(target_os = "linux")]
fn build_linux_reified_namespace_command(
    plan: &ReifiedNamespacePlan,
    temp_root: &ReifiedRunnerTemp,
) -> Result<ReifiedNamespaceCommandSpec, String> {
    let helpers = LinuxSetupHelpers::resolve()?;
    build_linux_reified_namespace_command_with_helpers(plan, temp_root, &helpers)
}

#[cfg(target_os = "linux")]
fn build_linux_reified_namespace_command_with_helpers(
    plan: &ReifiedNamespacePlan,
    temp_root: &ReifiedRunnerTemp,
    helpers: &LinuxSetupHelpers,
) -> Result<ReifiedNamespaceCommandSpec, String> {
    if plan.argv.is_empty() {
        return Err("argv must not be empty".to_string());
    }
    prepare_reified_root(plan, &temp_root.root)?;

    let mut args = vec![
        "--user".to_string(),
        "--map-root-user".to_string(),
        "--mount".to_string(),
        "--pid".to_string(),
        "--fork".to_string(),
        "--kill-child=SIGKILL".to_string(),
    ];
    if matches!(plan.network, NetworkPosture::Deny) {
        args.push("--net".to_string());
    }
    args.extend([
        "--".to_string(),
        helpers.host_shell.display().to_string(),
        "-c".to_string(),
        LINUX_REIFIED_NAMESPACE_SCRIPT.to_string(),
        "alan-reified-runner".to_string(),
        temp_root.root.display().to_string(),
        temp_root.setup_marker.display().to_string(),
        helpers.mount.display().to_string(),
        helpers.chroot.display().to_string(),
        helpers.namespace_shell.display().to_string(),
        helpers.namespace_setpriv.display().to_string(),
        plan.declared_host_mounts.len().to_string(),
    ]);
    for mount in &plan.declared_host_mounts {
        args.push(mount.namespace_path.display().to_string());
        args.push(mount.host_path.display().to_string());
        args.push(mount.access.as_str().to_string());
    }

    let execution_substrate = existing_execution_substrate(plan);
    args.push(execution_substrate.len().to_string());
    for mount in &execution_substrate {
        args.push(mount.namespace_path.display().to_string());
        args.push(mount.host_path.display().to_string());
    }

    args.push(plan.scratch_tmp.namespace_path.display().to_string());
    args.push(plan.cwd.display().to_string());
    args.extend(plan.argv.iter().cloned());

    Ok(ReifiedNamespaceCommandSpec {
        program: helpers.unshare.display().to_string(),
        args,
    })
}

#[cfg(target_os = "linux")]
fn resolve_trusted_linux_helper(name: &str, candidates: &[&str]) -> Result<PathBuf, String> {
    candidates
        .iter()
        .map(Path::new)
        .find(|path| {
            std::fs::metadata(path)
                .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "trusted linux helper {name} not found in {}",
                candidates.join(", ")
            )
        })
}

#[cfg(target_os = "linux")]
fn setup_marker_was_written(path: &Path) -> bool {
    matches!(std::fs::read(path), Ok(contents) if contents == b"ok\n")
}

#[cfg(target_os = "linux")]
impl ReifiedNamespaceCommandSpec {
    fn audit_fields(&self) -> Vec<(&'static str, String)> {
        vec![
            ("program", self.program.clone()),
            ("arg_count", self.args.len().to_string()),
        ]
    }
}

#[cfg(target_os = "linux")]
fn existing_execution_substrate(
    plan: &ReifiedNamespacePlan,
) -> Vec<ReifiedExecutionSubstrateMount> {
    plan.execution_substrate
        .iter()
        .filter(|mount| mount.host_path.exists())
        .cloned()
        .collect()
}

#[cfg(target_os = "linux")]
fn reified_runner_temp_base_candidates() -> Vec<PathBuf> {
    let mut candidates = vec![std::env::temp_dir(), PathBuf::from("/var/tmp")];
    if !candidates
        .iter()
        .any(|candidate| candidate == Path::new("/tmp"))
    {
        candidates.push(PathBuf::from("/tmp"));
    }
    candidates
}

#[cfg(target_os = "linux")]
fn temp_parent_is_exposed_to_writable_mount(parent: &Path, mounts: &[ReifiedHostMount]) -> bool {
    mounts
        .iter()
        .filter(|mount| mount.access.is_writable())
        .any(|mount| paths_overlap(parent, &mount.host_path))
}

#[cfg(target_os = "linux")]
fn prepare_reified_root(plan: &ReifiedNamespacePlan, root: &Path) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|err| format!("create root failed: {err}"))?;

    prepare_standard_device_destinations(root)?;
    for mount in &plan.declared_host_mounts {
        prepare_mount_destination(root, &mount.namespace_path, &mount.host_path, true)?;
    }
    for mount in existing_execution_substrate(plan) {
        prepare_mount_destination(root, &mount.namespace_path, &mount.host_path, false)?;
    }
    let scratch_destination = namespace_path_under_root(root, &plan.scratch_tmp.namespace_path)?;
    std::fs::create_dir_all(&scratch_destination)
        .map_err(|err| format!("create scratch tmp mountpoint failed: {err}"))?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn prepare_standard_device_destinations(root: &Path) -> Result<(), String> {
    let dev_dir = root.join("dev");
    std::fs::create_dir_all(&dev_dir)
        .map_err(|err| format!("create /dev mountpoint parent failed: {err}"))?;
    for name in ["null", "stdin", "stdout", "stderr"] {
        let destination = dev_dir.join(name);
        std::fs::File::create(&destination)
            .map_err(|err| format!("create /dev/{name} mountpoint failed: {err}"))?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn prepare_mount_destination(
    root: &Path,
    namespace_path: &Path,
    host_path: &Path,
    required: bool,
) -> Result<(), String> {
    let metadata = match std::fs::metadata(host_path) {
        Ok(metadata) => metadata,
        Err(err) if required => {
            return Err(format!(
                "host mount source {} unavailable: {err}",
                host_path.display()
            ));
        }
        Err(_) => return Ok(()),
    };
    let destination = namespace_path_under_root(root, namespace_path)?;
    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "create mountpoint parent {} failed: {err}",
                    parent.display()
                )
            })?;
        }
        std::fs::File::create(&destination)
            .map_err(|err| format!("create mountpoint {} failed: {err}", destination.display()))?;
    } else {
        std::fs::create_dir_all(&destination)
            .map_err(|err| format!("create mountpoint {} failed: {err}", destination.display()))?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn namespace_path_under_root(root: &Path, namespace_path: &Path) -> Result<PathBuf, String> {
    if !namespace_path.is_absolute() || contains_parent_component(namespace_path) {
        return Err(format!(
            "invalid namespace path {}",
            namespace_path.display()
        ));
    }
    let mut destination = root.to_path_buf();
    for component in namespace_path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => destination.push(part),
            Component::ParentDir | Component::Prefix(_) => {
                return Err(format!(
                    "invalid namespace path {}",
                    namespace_path.display()
                ));
            }
        }
    }
    Ok(destination)
}

fn translate_host_path_with_mounts(
    mounts: &[ReifiedHostMount],
    host_path: &Path,
) -> Option<PathBuf> {
    if !host_path.is_absolute() || contains_parent_component(host_path) {
        return None;
    }

    let mount = mounts
        .iter()
        .filter(|mount| host_path.starts_with(&mount.host_path))
        .max_by_key(|mount| mount.host_path.components().count())?;
    let suffix = host_path.strip_prefix(&mount.host_path).ok()?;
    if suffix.as_os_str().is_empty() {
        Some(mount.namespace_path.clone())
    } else {
        Some(mount.namespace_path.join(suffix))
    }
}

fn validate_no_overlapping_namespace_paths(
    paths: &[&Path],
) -> Result<(), ReifiedNamespacePlanError> {
    for (index, &left) in paths.iter().enumerate() {
        for &right in &paths[index + 1..] {
            if left == right {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: left.to_path_buf(),
                    child: right.to_path_buf(),
                });
            }
            if right.starts_with(left) {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: left.to_path_buf(),
                    child: right.to_path_buf(),
                });
            }
            if left.starts_with(right) {
                return Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                    parent: right.to_path_buf(),
                    child: left.to_path_buf(),
                });
            }
        }
    }
    Ok(())
}

fn validate_no_mixed_access_host_mount_overlap(
    mounts: &[ReifiedHostMount],
) -> Result<(), ReifiedNamespacePlanError> {
    for (index, left) in mounts.iter().enumerate() {
        for right in &mounts[index + 1..] {
            if !paths_overlap(&left.host_path, &right.host_path) {
                continue;
            }

            match (left.access.is_writable(), right.access.is_writable()) {
                (true, false) => {
                    return Err(
                        ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                            read_only_host_path: right.host_path.clone(),
                            writable_host_path: left.host_path.clone(),
                        },
                    );
                }
                (false, true) => {
                    return Err(
                        ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                            read_only_host_path: left.host_path.clone(),
                            writable_host_path: right.host_path.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn validate_no_writable_mount_over_execution_substrate(
    mounts: &[ReifiedHostMount],
    substrate: &[ReifiedExecutionSubstrateMount],
) -> Result<(), ReifiedNamespacePlanError> {
    for mount in mounts.iter().filter(|mount| mount.access.is_writable()) {
        for substrate_mount in substrate {
            if paths_overlap(&mount.host_path, &substrate_mount.host_path) {
                return Err(
                    ReifiedNamespacePlanError::WritableHostMountOverlapsExecutionSubstrate {
                        writable_host_path: mount.host_path.clone(),
                        substrate_host_path: substrate_mount.host_path.clone(),
                    },
                );
            }
        }
    }
    Ok(())
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn validate_namespace_path(path: &Path) -> Result<(), ReifiedNamespacePlanError> {
    validate_absolute_path(path, "namespace")?;
    if path == Path::new("/") {
        return Err(ReifiedNamespacePlanError::RootNamespacePath);
    }
    Ok(())
}

fn validate_and_normalize_host_source_path(
    path: &Path,
    kind: &'static str,
) -> Result<PathBuf, ReifiedNamespacePlanError> {
    validate_absolute_path(path, kind)?;
    let normalized = canonicalize_existing_host_path(path);
    if normalized == Path::new("/") {
        return Err(ReifiedNamespacePlanError::RootHostSourcePath { kind });
    }
    Ok(normalized)
}

fn canonicalize_existing_host_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = dunce::canonicalize(path) {
        return canonical;
    }

    let mut ancestor = path.parent();
    while let Some(parent) = ancestor {
        if let Ok(canonical_parent) = dunce::canonicalize(parent)
            && let Ok(suffix) = path.strip_prefix(parent)
        {
            return canonical_parent.join(suffix);
        }
        ancestor = parent.parent();
    }

    path.to_path_buf()
}

fn validate_absolute_path(
    path: &Path,
    kind: &'static str,
) -> Result<(), ReifiedNamespacePlanError> {
    if !path.is_absolute() {
        return Err(ReifiedNamespacePlanError::RelativePath {
            kind,
            path: path.to_path_buf(),
        });
    }
    if contains_parent_component(path) {
        return Err(ReifiedNamespacePlanError::ParentPath {
            kind,
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

fn contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_argv() -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), "pwd".to_string()]
    }

    #[cfg(target_os = "linux")]
    fn test_linux_setup_helpers() -> LinuxSetupHelpers {
        LinuxSetupHelpers {
            unshare: PathBuf::from("/usr/bin/unshare"),
            host_shell: PathBuf::from("/bin/sh"),
            mount: PathBuf::from("/usr/bin/mount"),
            chroot: PathBuf::from("/usr/sbin/chroot"),
            namespace_shell: PathBuf::from("/bin/sh"),
            namespace_setpriv: PathBuf::from("/usr/bin/setpriv"),
        }
    }

    #[test]
    fn workspace_seed_mount_translates_cwd_to_namespace_path() {
        let plan = ReifiedNamespacePlan::workspace_seed(
            "/host/workspace",
            "/host/workspace/src",
            shell_argv(),
            NetworkPosture::Deny,
        )
        .unwrap();

        assert_eq!(plan.declared_host_mounts.len(), 1);
        assert_eq!(
            plan.declared_host_mounts[0],
            ReifiedHostMount {
                namespace_path: PathBuf::from(DEFAULT_WORKSPACE_NAMESPACE_PATH),
                host_path: PathBuf::from("/host/workspace"),
                access: ReifiedMountAccess::ReadWrite,
            }
        );
        assert_eq!(plan.cwd, PathBuf::from("/mnt/workspace/src"));
        assert_eq!(plan.argv, shell_argv());
        assert_eq!(plan.network, NetworkPosture::Deny);
        assert_eq!(
            plan.scratch_tmp.namespace_path,
            PathBuf::from(DEFAULT_SCRATCH_TMP_NAMESPACE_PATH)
        );
        assert!(
            plan.execution_substrate
                .iter()
                .any(|mount| mount.namespace_path == Path::new("/bin"))
        );
    }

    #[test]
    fn extra_read_write_mount_is_preserved_and_translatable() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/deps",
                    "/host/deps",
                    ReifiedMountAccess::ReadWrite,
                ),
            ],
            "/host/deps/pkg",
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.cwd, PathBuf::from("/mnt/deps/pkg"));
        assert_eq!(
            plan.declared_host_mounts[1].access,
            ReifiedMountAccess::ReadWrite
        );
        assert_eq!(
            plan.translate_projected_host_path(Path::new("/host/project/src/lib.rs")),
            Some(PathBuf::from("/mnt/project/src/lib.rs"))
        );
    }

    #[test]
    fn read_only_host_mount_is_not_writable() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/docs",
                "/host/docs",
                ReifiedMountAccess::ReadOnly,
            )],
            "/host/docs/manual",
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.cwd, PathBuf::from("/mnt/docs/manual"));
        assert_eq!(
            plan.declared_host_mounts[0].access,
            ReifiedMountAccess::ReadOnly
        );
        assert!(!plan.declared_host_mounts[0].access.is_writable());
    }

    #[test]
    fn execution_substrate_scratch_and_network_are_separate_plan_fields() {
        let substrate = vec![
            ReifiedExecutionSubstrateMount::new("/bin", "/host/bin"),
            ReifiedExecutionSubstrateMount::new("/usr/lib", "/host/lib"),
        ];
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                "/host/project",
                ReifiedMountAccess::ReadWrite,
            )],
            "/host/project",
            vec!["sh".to_string()],
            NetworkPosture::Allow,
        )
        .with_execution_substrate(substrate.clone())
        .with_scratch_tmp_namespace_path("/run/alan-tmp");
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.execution_substrate, substrate);
        assert_eq!(
            plan.scratch_tmp.namespace_path,
            PathBuf::from("/run/alan-tmp")
        );
        assert_eq!(plan.network, NetworkPosture::Allow);
    }

    #[test]
    fn default_execution_substrate_includes_dns_resolver_config() {
        let substrate = default_execution_substrate();

        assert!(substrate.iter().any(|mount| {
            mount.namespace_path == Path::new("/etc/resolv.conf")
                && mount.host_path == Path::new("/etc/resolv.conf")
        }));
    }

    #[test]
    fn default_execution_substrate_includes_trusted_path_directories() {
        let substrate = default_execution_substrate();

        for path in std::env::split_paths(LINUX_REIFIED_COMMAND_PATH) {
            assert!(
                substrate.iter().any(|mount| {
                    mount.namespace_path == path.as_path() && mount.host_path == path.as_path()
                }),
                "missing trusted PATH substrate {}",
                path.display()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn user_path_smoke_allows_executable_dirs_under_visible_roots() {
        let temp = tempfile::tempdir().unwrap();
        let visible_root = temp.path().join("visible");
        let visible_bin = visible_root.join("bin");
        write_executable(&visible_bin.join("cargo"));
        let path = std::env::join_paths([visible_bin.as_path()]).unwrap();

        assert_eq!(
            reified_namespace_user_path_unavailable_reason_with_roots(
                Some(path.clone()),
                std::slice::from_ref(&visible_root),
                path,
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn user_path_smoke_rejects_unset_path() {
        let temp = tempfile::tempdir().unwrap();
        let visible_bin = temp.path().join("bin");
        write_executable(&visible_bin.join("sh"));
        let reified_path = std::env::join_paths([visible_bin.as_path()]).unwrap();

        let reason = reified_namespace_user_path_unavailable_reason_with_roots(
            None,
            &[temp.path().to_path_buf()],
            reified_path,
        )
        .expect("unset PATH should block default selection");

        assert!(reason.contains("current PATH is unset"));
        assert!(reason.contains("preserve actual PATH/order"));
    }

    #[cfg(unix)]
    #[test]
    fn user_path_smoke_rejects_empty_path_entry() {
        let temp = tempfile::tempdir().unwrap();
        let visible_bin = temp.path().join("bin");
        write_executable(&visible_bin.join("sh"));
        let current_path = std::ffi::OsString::from(format!(":{}", visible_bin.display()));
        let reified_path = std::env::join_paths([visible_bin.as_path()]).unwrap();

        let reason = reified_namespace_user_path_unavailable_reason_with_roots(
            Some(current_path),
            &[temp.path().to_path_buf()],
            reified_path,
        )
        .expect("empty PATH entries should block default selection");

        assert!(reason.contains("empty component"));
        assert!(reason.contains("current-directory lookup"));
    }

    #[cfg(unix)]
    #[test]
    fn user_path_smoke_rejects_executable_dirs_outside_visible_roots() {
        let temp = tempfile::tempdir().unwrap();
        let visible_root = temp.path().join("visible");
        let user_bin = temp.path().join("home/alice/.cargo/bin");
        write_executable(&visible_root.join("bin/sh"));
        write_executable(&user_bin.join("cargo"));
        let path = std::env::join_paths([visible_root.join("bin"), user_bin.clone()]).unwrap();
        let reified_path = std::env::join_paths([visible_root.join("bin")]).unwrap();

        let reason = reified_namespace_user_path_unavailable_reason_with_roots(
            Some(path),
            &[visible_root],
            reified_path,
        )
        .expect("user-local executable PATH entry should block reified default selection");

        assert!(reason.contains(user_bin.to_string_lossy().as_ref()));
        assert!(reason.contains("preserve user PATH/toolchain mounts"));
    }

    #[cfg(unix)]
    #[test]
    fn user_path_smoke_rejects_reified_path_order_changes() {
        let temp = tempfile::tempdir().unwrap();
        let usr_bin = temp.path().join("usr/bin");
        let local_bin = temp.path().join("usr/local/bin");
        write_executable(&usr_bin.join("cargo"));
        write_executable(&local_bin.join("cargo"));
        let current_path = std::env::join_paths([usr_bin.as_path(), local_bin.as_path()]).unwrap();
        let reified_path = std::env::join_paths([local_bin.as_path(), usr_bin.as_path()]).unwrap();

        let reason = reified_namespace_user_path_unavailable_reason_with_roots(
            Some(current_path),
            &[temp.path().to_path_buf()],
            reified_path,
        )
        .expect("reified PATH reordering should block default selection");

        assert!(reason.contains("current PATH executable entry order differs"));
        assert!(reason.contains("preserve actual PATH/order"));
    }

    #[test]
    fn virtual_mounts_are_excluded_from_native_plan() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::virtual_mount("/agent"),
                ReifiedMountDeclaration::virtual_mount("/proc"),
                ReifiedMountDeclaration::virtual_mount("/srv"),
                ReifiedMountDeclaration::virtual_mount("/mnt/llm"),
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
            ],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.declared_host_mounts.len(), 1);
        assert_eq!(
            plan.declared_host_mounts[0].namespace_path,
            PathBuf::from("/mnt/project")
        );
    }

    #[test]
    fn longest_host_mount_wins_during_path_translation() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/vendor",
                    "/host/project/vendor",
                    ReifiedMountAccess::ReadWrite,
                ),
            ],
            "/host/project/vendor/crate",
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.cwd, PathBuf::from("/mnt/vendor/crate"));
        assert_eq!(
            plan.translate_projected_host_path(Path::new("/host/project/vendor/crate/Cargo.toml")),
            Some(PathBuf::from("/mnt/vendor/crate/Cargo.toml"))
        );
    }

    #[test]
    fn read_only_child_host_mount_covered_by_writable_parent_is_rejected() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/vendor",
                    "/host/project/vendor",
                    ReifiedMountAccess::ReadOnly,
                ),
            ],
            "/host/project/vendor",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(
                ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                    read_only_host_path: PathBuf::from("/host/project/vendor"),
                    writable_host_path: PathBuf::from("/host/project"),
                }
            )
        );
    }

    #[test]
    fn writable_child_host_mount_under_read_only_parent_is_rejected() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadOnly,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/cache",
                    "/host/project/cache",
                    ReifiedMountAccess::ReadWrite,
                ),
            ],
            "/host/project/cache",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(
                ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                    read_only_host_path: PathBuf::from("/host/project"),
                    writable_host_path: PathBuf::from("/host/project/cache"),
                }
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_host_mount_sources_are_normalized_before_overlap_checks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        let vendor = workspace.join("vendor");
        let workspace_link = temp_dir.path().join("workspace-link");
        std::fs::create_dir_all(&vendor).unwrap();
        std::os::unix::fs::symlink(&workspace, &workspace_link).unwrap();

        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    &workspace_link,
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host("/mnt/vendor", &vendor, ReifiedMountAccess::ReadOnly),
            ],
            workspace_link.join("vendor"),
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(
                ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                    read_only_host_path: dunce::canonicalize(&vendor).unwrap(),
                    writable_host_path: dunce::canonicalize(&workspace).unwrap(),
                }
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_projected_host_paths_are_normalized_before_translation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        let src = workspace.join("src");
        let lib = src.join("lib.rs");
        let workspace_link = temp_dir.path().join("workspace-link");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(&lib, b"mod test;").unwrap();
        std::os::unix::fs::symlink(&workspace, &workspace_link).unwrap();

        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                &workspace_link,
                ReifiedMountAccess::ReadWrite,
            )],
            workspace_link.join("src"),
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(plan.cwd, PathBuf::from("/mnt/project/src"));
        assert_eq!(
            plan.declared_host_mounts[0].host_path,
            dunce::canonicalize(&workspace).unwrap()
        );
        assert_eq!(
            plan.translate_projected_host_path(&workspace_link.join("src/lib.rs")),
            Some(PathBuf::from("/mnt/project/src/lib.rs"))
        );
        assert_eq!(
            plan.translate_projected_host_path(&workspace_link.join("generated/new.rs")),
            Some(PathBuf::from("/mnt/project/generated/new.rs"))
        );
    }

    #[test]
    fn projected_host_path_translation_rejects_relative_and_parent_paths_before_normalization() {
        let current_dir = std::env::current_dir().unwrap();
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                &current_dir,
                ReifiedMountAccess::ReadWrite,
            )],
            &current_dir,
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert!(Path::new("Cargo.toml").exists());
        assert_eq!(
            plan.translate_projected_host_path(Path::new("Cargo.toml")),
            None
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let workspace = temp_dir.path().join("workspace");
        let src = workspace.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(workspace.join("Cargo.toml"), b"[package]\n").unwrap();
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/workspace",
                &workspace,
                ReifiedMountAccess::ReadWrite,
            )],
            &workspace,
            shell_argv(),
            NetworkPosture::Deny,
        );
        let plan = ReifiedNamespacePlan::derive(input).unwrap();

        assert_eq!(
            plan.translate_projected_host_path(&workspace.join("src/../Cargo.toml")),
            None
        );
    }

    #[test]
    fn overlapping_namespace_mounts_are_rejected() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/project/vendor",
                    "/host/vendor-cache",
                    ReifiedMountAccess::ReadOnly,
                ),
            ],
            "/host/project/vendor",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                parent: PathBuf::from("/mnt/project"),
                child: PathBuf::from("/mnt/project/vendor"),
            })
        );
    }

    #[test]
    fn virtual_mount_cannot_overlap_host_mount_namespace_path() {
        let input = ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    "/host/project",
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::virtual_mount("/mnt/project/agent"),
            ],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                parent: PathBuf::from("/mnt/project"),
                child: PathBuf::from("/mnt/project/agent"),
            })
        );
    }

    #[test]
    fn host_mount_cannot_overlap_execution_substrate_namespace_path() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/bin",
                "/host/bin-overlay",
                ReifiedMountAccess::ReadOnly,
            )],
            "/host/bin-overlay",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                parent: PathBuf::from("/bin"),
                child: PathBuf::from("/bin"),
            })
        );
    }

    #[test]
    fn host_mount_cannot_overlap_scratch_tmp_namespace_path() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/tmp/project",
                "/host/project",
                ReifiedMountAccess::ReadWrite,
            )],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::NamespaceMountOverlap {
                parent: PathBuf::from("/tmp"),
                child: PathBuf::from("/tmp/project"),
            })
        );
    }

    #[test]
    fn host_mount_source_must_not_be_root() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/root",
                "/",
                ReifiedMountAccess::ReadWrite,
            )],
            "/",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::RootHostSourcePath { kind: "host mount" })
        );
    }

    #[test]
    fn execution_substrate_source_must_not_be_root() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                "/host/project",
                ReifiedMountAccess::ReadOnly,
            )],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        )
        .with_execution_substrate(vec![ReifiedExecutionSubstrateMount::new(
            "/run/host-root",
            "/",
        )]);

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::RootHostSourcePath {
                kind: "execution substrate"
            })
        );
    }

    #[test]
    fn writable_host_mount_cannot_overlap_execution_substrate_source() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/tools",
                "/usr",
                ReifiedMountAccess::ReadWrite,
            )],
            "/usr/bin",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(
                ReifiedNamespacePlanError::WritableHostMountOverlapsExecutionSubstrate {
                    writable_host_path: PathBuf::from("/usr"),
                    substrate_host_path: PathBuf::from("/usr/bin"),
                }
            )
        );
    }

    #[test]
    fn out_of_view_cwd_is_rejected() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/mnt/project",
                "/host/project",
                ReifiedMountAccess::ReadWrite,
            )],
            "/host/elsewhere",
            shell_argv(),
            NetworkPosture::Deny,
        );
        let error = ReifiedNamespacePlan::derive(input).unwrap_err();

        assert_eq!(
            error,
            ReifiedNamespacePlanError::CwdOutsideView {
                cwd: PathBuf::from("/host/elsewhere")
            }
        );
    }

    #[test]
    fn namespace_paths_must_be_absolute_and_non_root() {
        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "mnt/project",
                "/host/project",
                ReifiedMountAccess::ReadWrite,
            )],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        );

        assert!(matches!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::RelativePath {
                kind: "namespace",
                ..
            })
        ));

        let input = ReifiedNamespacePlanInput::new(
            vec![ReifiedMountDeclaration::host(
                "/",
                "/host/project",
                ReifiedMountAccess::ReadWrite,
            )],
            "/host/project",
            shell_argv(),
            NetworkPosture::Deny,
        );
        assert_eq!(
            ReifiedNamespacePlan::derive(input),
            Err(ReifiedNamespacePlanError::RootNamespacePath)
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn linux_runner_reports_non_linux_unavailable_without_ambient_execution() {
        let plan = ReifiedNamespacePlan::workspace_seed(
            "/host/workspace",
            "/host/workspace",
            shell_argv(),
            NetworkPosture::Deny,
        )
        .unwrap();
        let runner = LinuxReifiedNamespaceRunner::with_fallback_backend(
            SandboxBackendKind::WorkspacePathGuard,
        );

        let error = runner.run(&plan).unwrap_err();

        assert_eq!(
            error.reason,
            "linux reified namespace runner is only available on Linux"
        );
        assert_eq!(
            error.fallback_backend,
            SandboxBackendKind::WorkspacePathGuard
        );
        assert!(
            error
                .audit_fields
                .contains(&("backend", "linux_reified_namespace".to_string()))
        );
        assert!(
            error
                .audit_fields
                .contains(&("fallback_backend", "workspace_path_guard".to_string()))
        );
    }

    #[test]
    fn reification_report_selection_requires_network_only_for_denied_plans() {
        let report = LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: LinuxReificationCapability::available(),
            user_namespace: LinuxReificationCapability::available(),
            mount_namespace: LinuxReificationCapability::available(),
            pid_namespace: LinuxReificationCapability::available(),
            bind_mount: LinuxReificationCapability::available(),
            read_only_remount: LinuxReificationCapability::available(),
            scratch_tmp_mount: LinuxReificationCapability::available(),
            network_confinement: LinuxReificationCapability::unavailable(
                "network namespaces disabled",
            ),
        });

        assert!(linux_reification_report_supports_plan(
            &report,
            NetworkPosture::Allow
        ));
        assert!(!linux_reification_report_supports_plan(
            &report,
            NetworkPosture::Deny
        ));
        assert_eq!(
            linux_reification_unavailable_reasons_for_plan(&report, NetworkPosture::Allow),
            Vec::<String>::new()
        );
        assert_eq!(
            linux_reification_unavailable_reasons_for_plan(&report, NetworkPosture::Deny),
            vec!["network_confinement: network namespaces disabled".to_string()]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_command_clears_inherited_environment() {
        let output = ReifiedNamespaceCommandSpec {
            program: "/usr/bin/env".to_string(),
            args: Vec::new(),
        }
        .command()
        .output()
        .unwrap();

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("PATH={TRUSTED_LINUX_SETUP_PATH}\n")
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_timeout_kills_child_process_group() {
        let temp_dir = tempfile::tempdir().unwrap();
        let marker = temp_dir.path().join("leaked-after-timeout");
        let output = run_linux_reified_command(
            ReifiedNamespaceCommandSpec {
                program: "/bin/sh".to_string(),
                args: vec![
                    "-c".to_string(),
                    format!("(sleep 2; printf leaked > {}) & wait", marker.display()),
                ],
            }
            .command(),
            Some(Duration::from_millis(50)),
        );

        let error = output.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            !marker.exists(),
            "timeout should kill the shell and its sleeping child"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_timeout_covers_output_drain_after_parent_exit() {
        let started = Instant::now();
        let output = run_linux_reified_command(
            ReifiedNamespaceCommandSpec {
                program: "/bin/sh".to_string(),
                args: vec!["-c".to_string(), "sleep 2 &".to_string()],
            }
            .command(),
            Some(Duration::from_millis(50)),
        );

        let error = output.unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "timeout should remain active while stdout/stderr readers wait for EOF"
        );
    }

    #[cfg(target_os = "linux")]
    fn linux_reified_runner_ready_for_smoke() -> bool {
        let readiness = crate::tools::linux_reified_namespace_backend_readiness();
        // These smoke plans use explicit executables, so they validate runner
        // capability even when the default backend selection rejects the current
        // user PATH/toolchain shape.
        if readiness.capability_report.is_selectable() && readiness.runner_smoke.is_available() {
            return true;
        }

        eprintln!(
            "skipping linux reified namespace smoke: audit={:?}",
            readiness.audit_fields()
        );
        false
    }

    #[cfg(target_os = "linux")]
    fn run_linux_reified_smoke(plan: &ReifiedNamespacePlan) -> ExecResult {
        let runner =
            LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock);
        runner.run(plan).unwrap_or_else(|err| {
            panic!(
                "linux reified namespace smoke failed to start: {err}; audit={:?}",
                err.audit_fields
            )
        })
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_smoke_enforces_mount_visibility_and_access() {
        if !linux_reified_runner_ready_for_smoke() {
            return;
        }

        let workspace = tempfile::tempdir().unwrap();
        let readonly = tempfile::tempdir().unwrap();
        let secret_home = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("visible.txt"), "visible").unwrap();
        std::fs::write(readonly.path().join("readonly.txt"), "readonly").unwrap();

        let script = r#"
set -eu
secret_home="$1"
test -f /mnt/project/visible.txt
test "$(cat /mnt/project/visible.txt)" = visible
printf changed > /mnt/project/writable.txt
test "$(cat /mnt/project/writable.txt)" = changed
test -f /mnt/docs/readonly.txt
test "$(cat /mnt/docs/readonly.txt)" = readonly
if sh -c 'printf blocked > /mnt/docs/blocked.txt'; then
  exit 42
fi
test ! -e "$secret_home"
"#;
        let plan = ReifiedNamespacePlan::derive(ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    workspace.path(),
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/docs",
                    readonly.path(),
                    ReifiedMountAccess::ReadOnly,
                ),
            ],
            workspace.path(),
            vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                script.to_string(),
                "alan-reified-fs-smoke".to_string(),
                secret_home.path().display().to_string(),
            ],
            NetworkPosture::Deny,
        ))
        .unwrap();

        let result = run_linux_reified_smoke(&plan);

        assert_eq!(
            result.exit_code, 0,
            "stdout={} stderr={}",
            result.stdout, result.stderr
        );
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("writable.txt")).unwrap(),
            "changed"
        );
        assert!(
            !readonly.path().join("blocked.txt").exists(),
            "read-only mount accepted a write"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_smoke_denies_network_connections() {
        if !linux_reified_runner_ready_for_smoke() {
            return;
        }

        let workspace = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_listener = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let accepted_by_thread = std::sync::Arc::clone(&accepted);
        let stop_listener_by_thread = std::sync::Arc::clone(&stop_listener);
        let listener_thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
            while std::time::Instant::now() < deadline
                && !stop_listener_by_thread.load(std::sync::atomic::Ordering::SeqCst)
            {
                match listener.accept() {
                    Ok((_stream, _addr)) => {
                        accepted_by_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => return,
                }
            }
        });

        let script = r#"
set -u
port="$1"
if ! command -v bash >/tmp/bash-path; then
  exit 77
fi
if ! command -v timeout >/tmp/timeout-path; then
  exit 77
fi
if timeout 2 bash -c "cat < /dev/tcp/127.0.0.1/${port}" >/tmp/network-out 2>/tmp/network-err; then
  exit 42
fi
exit 0
"#;
        let plan = ReifiedNamespacePlan::workspace_seed(
            workspace.path(),
            workspace.path(),
            vec![
                "/bin/sh".to_string(),
                "-c".to_string(),
                script.to_string(),
                "alan-reified-network-smoke".to_string(),
                port.to_string(),
            ],
            NetworkPosture::Deny,
        )
        .unwrap();

        let result = run_linux_reified_smoke(&plan);
        stop_listener.store(true, std::sync::atomic::Ordering::SeqCst);
        listener_thread.join().unwrap();

        assert!(
            !accepted.load(std::sync::atomic::Ordering::SeqCst),
            "network-denied reified command connected to the host listener"
        );
        if result.exit_code == 77 {
            eprintln!("skipping denied-network smoke assertion: bash or timeout unavailable");
            return;
        }
        assert_eq!(
            result.exit_code, 0,
            "stdout={} stderr={}",
            result.stdout, result.stderr
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_command_uses_unshare_mount_chroot_and_network_namespace() {
        let workspace = tempfile::tempdir().unwrap();
        let docs = tempfile::tempdir().unwrap();
        let substrate = tempfile::tempdir().unwrap();
        let plan = ReifiedNamespacePlan::derive(
            ReifiedNamespacePlanInput::new(
                vec![
                    ReifiedMountDeclaration::host(
                        "/mnt/project",
                        workspace.path(),
                        ReifiedMountAccess::ReadWrite,
                    ),
                    ReifiedMountDeclaration::host(
                        "/mnt/docs",
                        docs.path(),
                        ReifiedMountAccess::ReadOnly,
                    ),
                ],
                workspace.path(),
                vec!["sh".to_string(), "-c".to_string(), "pwd".to_string()],
                NetworkPosture::Deny,
            )
            .with_execution_substrate(vec![ReifiedExecutionSubstrateMount::new(
                "/bin",
                substrate.path(),
            )]),
        )
        .unwrap();
        let temp_root = ReifiedRunnerTemp::create(&plan).unwrap();

        let helpers = test_linux_setup_helpers();
        let command =
            build_linux_reified_namespace_command_with_helpers(&plan, &temp_root, &helpers)
                .unwrap();

        assert_eq!(command.program, "/usr/bin/unshare");
        assert!(command.args.contains(&"--user".to_string()));
        assert!(command.args.contains(&"--map-root-user".to_string()));
        assert!(command.args.contains(&"--mount".to_string()));
        assert!(command.args.contains(&"--pid".to_string()));
        assert!(command.args.contains(&"--fork".to_string()));
        assert!(command.args.contains(&"--kill-child=SIGKILL".to_string()));
        assert!(command.args.contains(&"--net".to_string()));
        assert!(
            command
                .args
                .iter()
                .any(|arg| arg.contains("remount,bind,ro \"$root\""))
        );
        assert!(command.args.contains(&"/mnt/project".to_string()));
        assert!(command.args.contains(&"/mnt/docs".to_string()));
        assert!(command.args.contains(&"read_only".to_string()));
        assert!(temp_root.root.join("dev/null").exists());
        assert!(command.args.contains(&"/bin".to_string()));
        assert!(command.args.contains(&"/mnt/project".to_string()));
        let script = command
            .args
            .iter()
            .find(|arg| arg.contains("alan reified namespace setup failed"))
            .unwrap();
        assert!(script.contains(&format!("PATH='{LINUX_REIFIED_COMMAND_PATH}'")));
        assert!(script.contains("\"$mount_bin\" --make-rprivate / || fail \"make root private\""));
        assert!(!script.contains("--make-rprivate / 2>/dev/null || true"));
        assert!(script.contains("\"$mount_bin\" --bind /dev/null \"${root}/dev/null\""));
        assert!(script.contains("\"$mount_bin\" --bind /proc/self/fd/0 \"${root}/dev/stdin\""));
        assert!(script.contains("\"$mount_bin\" --bind /proc/self/fd/1 \"${root}/dev/stdout\""));
        assert!(script.contains("\"$mount_bin\" --bind /proc/self/fd/2 \"${root}/dev/stderr\""));
        assert!(script.contains("\"$mount_bin\" --bind \"$host_path\" \"$destination\""));
        assert!(script.contains("\"$chroot_bin\" \"$root\" \"$namespace_shell\""));
        assert!(!script.contains("chroot \"$root\""));
        assert!(!script.contains("exec setpriv --no-new-privs"));
        assert!(script.contains("exec \"$setpriv_bin\" --no-new-privs"));
        assert!(script.contains("--bounding-set=-all"));
        assert!(script.contains("--inh-caps=-all"));
        assert!(script.contains("--ambient-caps=-all"));
        assert!(script.contains("printf \"%s\\n\" ok >&3"));
        assert!(script.contains("exec 3>&-; exec \"$@\""));
        assert!(script.contains("3>\"$setup_marker\""));
        assert!(command.args.contains(&"/bin/sh".to_string()));
        assert!(command.args.contains(&"/usr/bin/mount".to_string()));
        assert!(command.args.contains(&"/usr/sbin/chroot".to_string()));
        assert!(command.args.contains(&"/usr/bin/setpriv".to_string()));
        assert!(temp_root.root.join("dev/stdin").exists());
        assert!(temp_root.root.join("dev/stdout").exists());
        assert!(temp_root.root.join("dev/stderr").exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_command_allows_network_by_omitting_network_namespace() {
        let workspace = tempfile::tempdir().unwrap();
        let plan = ReifiedNamespacePlan::workspace_seed(
            workspace.path(),
            workspace.path(),
            vec!["sh".to_string(), "-c".to_string(), "true".to_string()],
            NetworkPosture::Allow,
        )
        .unwrap();
        let temp_root = ReifiedRunnerTemp::create(&plan).unwrap();

        let helpers = test_linux_setup_helpers();
        let command =
            build_linux_reified_namespace_command_with_helpers(&plan, &temp_root, &helpers)
                .unwrap();

        assert!(!command.args.contains(&"--net".to_string()));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_temp_parent_must_not_overlap_writable_mounts() {
        let mounts = vec![
            ReifiedHostMount {
                namespace_path: PathBuf::from("/mnt/tmp"),
                host_path: PathBuf::from("/tmp"),
                access: ReifiedMountAccess::ReadWrite,
            },
            ReifiedHostMount {
                namespace_path: PathBuf::from("/mnt/docs"),
                host_path: PathBuf::from("/var/docs"),
                access: ReifiedMountAccess::ReadOnly,
            },
        ];

        assert!(temp_parent_is_exposed_to_writable_mount(
            Path::new("/tmp/alan-reified-runner-1"),
            &mounts
        ));
        assert!(!temp_parent_is_exposed_to_writable_mount(
            Path::new("/var/docs/alan-reified-runner-1"),
            &mounts
        ));
        assert!(!temp_parent_is_exposed_to_writable_mount(
            Path::new("/var/tmp/alan-reified-runner-1"),
            &mounts
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_runner_setup_marker_requires_success_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let marker = temp_dir.path().join("setup-ok");

        assert!(!setup_marker_was_written(&marker));

        std::fs::write(&marker, b"").unwrap();
        assert!(!setup_marker_was_written(&marker));

        std::fs::write(&marker, b"ok\n").unwrap();
        assert!(setup_marker_was_written(&marker));
    }

    #[cfg(unix)]
    fn write_executable(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }
}
