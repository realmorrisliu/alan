//! OS sandbox backend selection, capability detection, and safe degradation.
//!
//! This module provides the backend abstraction that future OS-level
//! enforcement plugs into (macOS Seatbelt, Linux Landlock+seccomp), plus the
//! security-critical degradation rule: when no enforcing backend is available,
//! bash/network operations MUST escalate rather than auto-approve.
//!
//! The bash execution path (`Sandbox::build_confined_command`) confines the
//! shell per backend: macOS wraps it in `sandbox-exec` with the generated
//! Seatbelt profile; Linux applies a Landlock ruleset in a `pre_exec` hook
//! (`apply_landlock`). Both confine writes to the host_mount + temp and deny
//! network by default (Seatbelt `network*`, Landlock ABI v4 TCP). An *approved*
//! network call (capability == Network reaching execution) runs with network
//! permitted (still filesystem-confined), so reviewer/human approval is not
//! futile. Detection is conservative: an OS backend is reported only when its
//! tooling is present, otherwise the path-guard fallback (under which bash must
//! not auto-run — the policy escalates it).

mod seatbelt;

use std::fmt;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::OnceLock;

pub(crate) use seatbelt::read_denylist_excluding_writable_roots;
pub use seatbelt::seatbelt_profile;

/// Available sandbox enforcement backends, in order of strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    /// macOS Seatbelt (`sandbox-exec`).
    Seatbelt,
    /// Linux user/mount namespace reification (full read isolation).
    LinuxReifiedNamespace,
    /// Linux Landlock (filesystem) paired with seccomp/namespace (network).
    Landlock,
    /// Best-effort in-process host_mount path guard (no OS enforcement).
    HostMountPathGuard,
}

impl SandboxBackendKind {
    /// Stable name used in audit metadata and diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            SandboxBackendKind::Seatbelt => "seatbelt",
            SandboxBackendKind::LinuxReifiedNamespace => "linux_reified_namespace",
            SandboxBackendKind::Landlock => "landlock",
            SandboxBackendKind::HostMountPathGuard => "host_mount_path_guard",
        }
    }

    /// Whether native subprocess paths are host-projected or namespace-reified.
    pub const fn path_mode(self) -> &'static str {
        match self {
            SandboxBackendKind::LinuxReifiedNamespace => "reified_namespace_paths",
            SandboxBackendKind::Seatbelt
            | SandboxBackendKind::Landlock
            | SandboxBackendKind::HostMountPathGuard => "projected_host_paths",
        }
    }

    /// Whether this backend enforces confinement at the OS level.
    pub const fn is_os_enforced(self) -> bool {
        matches!(
            self,
            SandboxBackendKind::Seatbelt
                | SandboxBackendKind::LinuxReifiedNamespace
                | SandboxBackendKind::Landlock
        )
    }

    /// Safe-degradation rule: bash and network may proceed without prompting
    /// only when an OS-enforced backend is active. Without one, they MUST
    /// escalate — "sandbox unavailable" is never "sandbox off".
    pub const fn allows_unattended_bash_and_network(self) -> bool {
        self.is_os_enforced()
    }

    /// Whether the backend confines bash strongly enough to run shell wrappers
    /// and reviewer-route escalated bash: the host_mount filesystem boundary AND
    /// network are kernel-enforced (Seatbelt). Landlock does not qualify because
    /// network confinement is kernel-conditional, and Linux reified namespace
    /// does not qualify until protected subpaths are carved out of the writable
    /// host_mount mount. Conservative backends keep the full shape parser and
    /// route escalated bash to a human.
    ///
    /// NOTE: protected subpaths (`.git`/`.alan`/`.agents`) are NOT kernel-confined
    /// (denying `.git` breaks git itself). Their integrity rests on the path-guard
    /// parser blocking direct + shell-wrapper-nested tampering; program-internal
    /// writes by approved code (git porcelain, a reviewer-approved test runner)
    /// are trusted — see the residual-gap audit.
    pub const fn permits_autonomous_bash(self) -> bool {
        matches!(self, SandboxBackendKind::Seatbelt)
    }
}

/// Availability state for a single Linux reification requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxReificationCapability {
    available: bool,
    reason: Option<String>,
}

impl LinuxReificationCapability {
    /// Mark the capability as available.
    pub fn available() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    /// Mark the capability as unavailable with an auditable reason.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            reason: Some(reason.into()),
        }
    }

    /// Whether this requirement is currently available.
    pub const fn is_available(&self) -> bool {
        self.available
    }

    /// The unavailable reason, when present.
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    fn audit_value(&self) -> String {
        match (self.available, self.reason()) {
            (true, _) => "available".to_string(),
            (false, Some(reason)) => format!("unavailable({reason})"),
            (false, None) => "unavailable".to_string(),
        }
    }
}

/// Overall status for the Linux reified namespace backend probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxReificationStatus {
    /// All required filesystem and network requirements are available.
    Available,
    /// Filesystem reification can be attempted, but network confinement is missing.
    Degraded,
    /// One or more required namespace or mount requirements are missing.
    Unavailable,
}

impl LinuxReificationStatus {
    /// Stable audit label.
    pub const fn as_str(self) -> &'static str {
        match self {
            LinuxReificationStatus::Available => "available",
            LinuxReificationStatus::Degraded => "degraded",
            LinuxReificationStatus::Unavailable => "unavailable",
        }
    }
}

impl fmt::Display for LinuxReificationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Capability report for the Linux reified namespace backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxReificationCapabilityReport {
    /// Linux host requirement.
    pub linux_host: LinuxReificationCapability,
    /// Unprivileged user namespace creation.
    pub user_namespace: LinuxReificationCapability,
    /// Mount namespace creation.
    pub mount_namespace: LinuxReificationCapability,
    /// PID namespace creation for timeout cleanup of all descendants.
    pub pid_namespace: LinuxReificationCapability,
    /// Bind-mount support inside the new namespace.
    pub bind_mount: LinuxReificationCapability,
    /// Read-only remount support for bound paths.
    pub read_only_remount: LinuxReificationCapability,
    /// Private scratch/tmp mount support.
    pub scratch_tmp_mount: LinuxReificationCapability,
    /// Network confinement support for network-denied commands.
    pub network_confinement: LinuxReificationCapability,
}

/// Requirement states used to build a Linux reification capability report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxReificationCapabilities {
    pub linux_host: LinuxReificationCapability,
    pub user_namespace: LinuxReificationCapability,
    pub mount_namespace: LinuxReificationCapability,
    pub pid_namespace: LinuxReificationCapability,
    pub bind_mount: LinuxReificationCapability,
    pub read_only_remount: LinuxReificationCapability,
    pub scratch_tmp_mount: LinuxReificationCapability,
    pub network_confinement: LinuxReificationCapability,
}

/// Selection readiness for the Linux reified namespace backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxReifiedNamespaceBackendReadiness {
    pub capability_report: LinuxReificationCapabilityReport,
    pub runner_smoke: LinuxReificationCapability,
    pub toolchain_smoke: LinuxReificationCapability,
    pub selected_backend: SandboxBackendKind,
}

impl LinuxReifiedNamespaceBackendReadiness {
    /// Stable fields for startup/debug audits.
    pub fn audit_fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = self.capability_report.audit_fields();
        fields.extend([
            ("runner_smoke", self.runner_smoke.audit_value()),
            ("toolchain_smoke", self.toolchain_smoke.audit_value()),
            ("selected_backend", self.selected_backend.name().to_string()),
            ("path_mode", self.selected_backend.path_mode().to_string()),
        ]);
        fields
    }
}

impl LinuxReificationCapabilityReport {
    /// Build a report from explicit requirement states.
    pub fn new(capabilities: LinuxReificationCapabilities) -> Self {
        Self {
            linux_host: capabilities.linux_host,
            user_namespace: capabilities.user_namespace,
            mount_namespace: capabilities.mount_namespace,
            pid_namespace: capabilities.pid_namespace,
            bind_mount: capabilities.bind_mount,
            read_only_remount: capabilities.read_only_remount,
            scratch_tmp_mount: capabilities.scratch_tmp_mount,
            network_confinement: capabilities.network_confinement,
        }
    }

    /// Stable backend name reported in audit metadata.
    pub const fn backend_name(&self) -> &'static str {
        SandboxBackendKind::LinuxReifiedNamespace.name()
    }

    /// Overall probe status.
    pub fn status(&self) -> LinuxReificationStatus {
        let fs_requirements_available = self.linux_host.is_available()
            && self.user_namespace.is_available()
            && self.mount_namespace.is_available()
            && self.pid_namespace.is_available()
            && self.bind_mount.is_available()
            && self.read_only_remount.is_available()
            && self.scratch_tmp_mount.is_available();

        if fs_requirements_available && self.network_confinement.is_available() {
            LinuxReificationStatus::Available
        } else if fs_requirements_available {
            LinuxReificationStatus::Degraded
        } else {
            LinuxReificationStatus::Unavailable
        }
    }

    /// Whether the backend is selectable for network-denied native subprocesses.
    pub fn is_selectable(&self) -> bool {
        matches!(self.status(), LinuxReificationStatus::Available)
    }

    /// List all missing requirements and their reasons.
    pub fn unavailable_reasons(&self) -> Vec<String> {
        self.capabilities()
            .into_iter()
            .filter(|(_, capability)| !capability.is_available())
            .map(|(name, capability)| {
                let reason = capability.reason().unwrap_or("no reason reported");
                format!("{name}: {reason}")
            })
            .collect()
    }

    /// Stable key/value fields for audit snapshots and diagnostics.
    pub fn audit_fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = vec![
            ("backend", self.backend_name().to_string()),
            ("status", self.status().to_string()),
        ];
        fields.extend(
            self.capabilities()
                .into_iter()
                .map(|(name, capability)| (name, capability.audit_value())),
        );
        fields
    }

    fn capabilities(&self) -> [(&'static str, &LinuxReificationCapability); 8] {
        [
            ("linux_host", &self.linux_host),
            ("user_namespace", &self.user_namespace),
            ("mount_namespace", &self.mount_namespace),
            ("pid_namespace", &self.pid_namespace),
            ("bind_mount", &self.bind_mount),
            ("read_only_remount", &self.read_only_remount),
            ("scratch_tmp_mount", &self.scratch_tmp_mount),
            ("network_confinement", &self.network_confinement),
        ]
    }
}

impl fmt::Display for LinuxReificationCapabilityReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let capabilities = self
            .capabilities()
            .into_iter()
            .map(|(name, capability)| format!("{name}={}", capability.audit_value()))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            formatter,
            "{}: {} ({capabilities})",
            self.backend_name(),
            self.status()
        )
    }
}

/// Probe Linux reified namespace support.
///
/// This is intentionally separate from `detect_backend()`: the backend state is
/// auditable now, but selection remains on the existing Seatbelt/Landlock/path
/// guard chain until a runner is implemented and smoke-gated.
pub fn probe_linux_reification() -> LinuxReificationCapabilityReport {
    probe_linux_reification_for_host()
}

/// Choose the best Linux backend when reification is considered.
///
/// `detect_backend()` does not call this yet; it is the pure fallback ordering
/// used by the probe tests and the later opt-in selection slice.
pub fn preferred_linux_backend_with_reification(
    report: &LinuxReificationCapabilityReport,
    landlock_is_available: bool,
) -> SandboxBackendKind {
    preferred_linux_backend_with_reification_and_runner(
        report,
        &LinuxReificationCapability::available(),
        landlock_is_available,
    )
}

/// Choose a Linux backend from capability and runner-smoke evidence.
pub fn preferred_linux_backend_with_reification_and_runner(
    report: &LinuxReificationCapabilityReport,
    runner_smoke: &LinuxReificationCapability,
    landlock_is_available: bool,
) -> SandboxBackendKind {
    let toolchain_smoke = LinuxReificationCapability::available();
    preferred_linux_backend_with_reification_runner_and_toolchain(
        report,
        runner_smoke,
        &toolchain_smoke,
        landlock_is_available,
    )
}

fn preferred_linux_backend_with_reification_runner_and_toolchain(
    report: &LinuxReificationCapabilityReport,
    runner_smoke: &LinuxReificationCapability,
    toolchain_smoke: &LinuxReificationCapability,
    landlock_is_available: bool,
) -> SandboxBackendKind {
    if report.is_selectable() && runner_smoke.is_available() && toolchain_smoke.is_available() {
        SandboxBackendKind::LinuxReifiedNamespace
    } else if landlock_is_available {
        SandboxBackendKind::Landlock
    } else {
        SandboxBackendKind::HostMountPathGuard
    }
}

/// Cached readiness for selecting the Linux reified namespace backend.
pub fn linux_reified_namespace_backend_readiness() -> LinuxReifiedNamespaceBackendReadiness {
    static READINESS: OnceLock<LinuxReifiedNamespaceBackendReadiness> = OnceLock::new();
    READINESS
        .get_or_init(probe_linux_reified_namespace_backend_readiness)
        .clone()
}

#[cfg(not(target_os = "linux"))]
fn probe_linux_reified_namespace_backend_readiness() -> LinuxReifiedNamespaceBackendReadiness {
    let capability_report = probe_linux_reification();
    let runner_smoke = LinuxReificationCapability::unavailable("not a linux host");
    let toolchain_smoke = LinuxReificationCapability::unavailable("not a linux host");
    let selected_backend = preferred_linux_backend_with_reification_runner_and_toolchain(
        &capability_report,
        &runner_smoke,
        &toolchain_smoke,
        false,
    );
    LinuxReifiedNamespaceBackendReadiness {
        capability_report,
        runner_smoke,
        toolchain_smoke,
        selected_backend,
    }
}

#[cfg(target_os = "linux")]
fn probe_linux_reified_namespace_backend_readiness() -> LinuxReifiedNamespaceBackendReadiness {
    let capability_report = probe_linux_reification();
    let runner_smoke = if capability_report.is_selectable() {
        super::reified_namespace::smoke_linux_reified_namespace_runner()
    } else {
        LinuxReificationCapability::unavailable("capability probe did not select reification")
    };
    let toolchain_smoke = if runner_smoke.is_available() {
        super::reified_namespace::smoke_linux_reified_namespace_user_path()
    } else {
        LinuxReificationCapability::unavailable("runner smoke did not pass")
    };
    let selected_backend = preferred_linux_backend_with_reification_runner_and_toolchain(
        &capability_report,
        &runner_smoke,
        &toolchain_smoke,
        landlock_available(),
    );
    LinuxReifiedNamespaceBackendReadiness {
        capability_report,
        runner_smoke,
        toolchain_smoke,
        selected_backend,
    }
}

#[cfg(not(target_os = "linux"))]
fn probe_linux_reification_for_host() -> LinuxReificationCapabilityReport {
    let reason = "not a linux host";
    LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
        linux_host: LinuxReificationCapability::unavailable(reason),
        user_namespace: LinuxReificationCapability::unavailable(reason),
        mount_namespace: LinuxReificationCapability::unavailable(reason),
        pid_namespace: LinuxReificationCapability::unavailable(reason),
        bind_mount: LinuxReificationCapability::unavailable(reason),
        read_only_remount: LinuxReificationCapability::unavailable(reason),
        scratch_tmp_mount: LinuxReificationCapability::unavailable(reason),
        network_confinement: LinuxReificationCapability::unavailable(reason),
    })
}

#[cfg(target_os = "linux")]
const TRUSTED_LINUX_PROBE_PATH: &str = "/usr/sbin:/usr/bin:/sbin:/bin";

#[cfg(target_os = "linux")]
fn probe_linux_reification_for_host() -> LinuxReificationCapabilityReport {
    let linux_host = LinuxReificationCapability::available();
    let user_namespace = run_linux_probe_command(
        "user namespace",
        linux_unshare_command(&["--user", "--map-root-user"]),
    );
    let mount_namespace = if user_namespace.is_available() {
        run_linux_probe_command(
            "mount namespace",
            linux_unshare_command(&["--user", "--map-root-user", "--mount"]),
        )
    } else {
        LinuxReificationCapability::unavailable("requires available user namespace")
    };

    let pid_namespace = if user_namespace.is_available() {
        run_linux_probe_command(
            "pid namespace",
            linux_unshare_command(&[
                "--user",
                "--map-root-user",
                "--pid",
                "--fork",
                "--kill-child=SIGKILL",
            ]),
        )
    } else {
        LinuxReificationCapability::unavailable("requires available user namespace")
    };

    let bind_mount = if mount_namespace.is_available() {
        run_mount_probe(
            "bind mount",
            "\"$ALAN_PROBE_MOUNT_BIN\" --make-rprivate / && \
             \"$ALAN_PROBE_MOUNT_BIN\" --bind \"$1\" \"$2\" && \
             test -f \"$2/probe-file\"",
        )
    } else {
        LinuxReificationCapability::unavailable("requires available mount namespace")
    };

    let read_only_remount = if mount_namespace.is_available() {
        run_mount_probe(
            "read-only remount",
            "\"$ALAN_PROBE_MOUNT_BIN\" --make-rprivate / && \
             \"$ALAN_PROBE_MOUNT_BIN\" --bind \"$1\" \"$2\" && \
             \"$ALAN_PROBE_MOUNT_BIN\" -o remount,bind,ro \"$2\" && \
             if \"$ALAN_PROBE_SHELL_BIN\" -c 'printf x > \"$1/probe-file\"' sh \"$2\" 2>/dev/null; then exit 1; fi",
        )
    } else {
        LinuxReificationCapability::unavailable("requires available mount namespace")
    };

    let scratch_tmp_mount = if mount_namespace.is_available() {
        run_scratch_tmp_probe()
    } else {
        LinuxReificationCapability::unavailable("requires available mount namespace")
    };

    let network_confinement = if mount_namespace.is_available() {
        run_linux_probe_command(
            "network namespace",
            linux_unshare_command(&["--user", "--map-root-user", "--mount", "--net"]),
        )
    } else {
        LinuxReificationCapability::unavailable("requires available mount namespace")
    };

    LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
        linux_host,
        user_namespace,
        mount_namespace,
        pid_namespace,
        bind_mount,
        read_only_remount,
        scratch_tmp_mount,
        network_confinement,
    })
}

#[cfg(target_os = "linux")]
fn linux_unshare_command(flags: &[&str]) -> Result<std::process::Command, String> {
    let mut command = std::process::Command::new(resolve_trusted_linux_helper(
        "unshare",
        &["/usr/bin/unshare", "/bin/unshare"],
    )?);
    command.args(flags).arg("true");
    command.env("PATH", TRUSTED_LINUX_PROBE_PATH);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn run_mount_probe(label: &str, script: &str) -> LinuxReificationCapability {
    let probe_root = match create_linux_probe_tree(label) {
        Ok(probe_root) => probe_root,
        Err(reason) => return LinuxReificationCapability::unavailable(reason),
    };

    let command = linux_unshare_shell_command(
        script,
        &[probe_root.source.as_path(), probe_root.target.as_path()],
    );
    run_linux_probe_command(label, command)
}

#[cfg(target_os = "linux")]
fn run_scratch_tmp_probe() -> LinuxReificationCapability {
    let probe_root = match create_linux_probe_tree("scratch tmp mount") {
        Ok(probe_root) => probe_root,
        Err(reason) => return LinuxReificationCapability::unavailable(reason),
    };

    let command = linux_unshare_shell_command(
        "\"$ALAN_PROBE_MOUNT_BIN\" --make-rprivate / && \
         \"$ALAN_PROBE_MOUNT_BIN\" -t tmpfs tmpfs \"$1\" && \
         printf ok > \"$1/probe-file\" && \
         test -f \"$1/probe-file\"",
        &[probe_root.scratch.as_path()],
    );
    run_linux_probe_command("scratch tmp mount", command)
}

#[cfg(target_os = "linux")]
fn linux_unshare_shell_command(
    script: &str,
    args: &[&Path],
) -> Result<std::process::Command, String> {
    let unshare = resolve_trusted_linux_helper("unshare", &["/usr/bin/unshare", "/bin/unshare"])?;
    let shell = resolve_trusted_linux_helper("sh", &["/bin/sh", "/usr/bin/sh"])?;
    let mount = resolve_trusted_linux_helper("mount", &["/usr/bin/mount", "/bin/mount"])?;
    let mut command = std::process::Command::new(unshare);
    command
        .env("PATH", TRUSTED_LINUX_PROBE_PATH)
        .env("ALAN_PROBE_MOUNT_BIN", mount)
        .env("ALAN_PROBE_SHELL_BIN", &shell)
        .args(["--user", "--map-root-user", "--mount"])
        .arg(shell)
        .args(["-c", script])
        .arg("alan-linux-reification-probe")
        .args(args);
    Ok(command)
}

#[cfg(target_os = "linux")]
fn run_linux_probe_command(
    label: &str,
    command: Result<std::process::Command, String>,
) -> LinuxReificationCapability {
    match command {
        Ok(command) => run_linux_probe(label, command),
        Err(reason) => LinuxReificationCapability::unavailable(reason),
    }
}

#[cfg(target_os = "linux")]
fn run_linux_probe(label: &str, mut command: std::process::Command) -> LinuxReificationCapability {
    match command.output() {
        Ok(output) if output.status.success() => LinuxReificationCapability::available(),
        Ok(output) => LinuxReificationCapability::unavailable(format!(
            "{label} probe failed: {}",
            linux_probe_failure(&output)
        )),
        Err(err) => {
            LinuxReificationCapability::unavailable(format!("{label} probe failed to start: {err}"))
        }
    }
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
fn linux_probe_failure(output: &std::process::Output) -> String {
    let status = output
        .status
        .code()
        .map(|code| format!("exit {code}"))
        .unwrap_or_else(|| "terminated by signal".to_string());
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        status
    } else {
        format!("{status}: {stderr}")
    }
}

#[cfg(target_os = "linux")]
struct LinuxProbeTree {
    root: PathBuf,
    source: PathBuf,
    target: PathBuf,
    scratch: PathBuf,
}

#[cfg(target_os = "linux")]
impl Drop for LinuxProbeTree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(target_os = "linux")]
fn create_linux_probe_tree(label: &str) -> Result<LinuxProbeTree, String> {
    let normalized_label = label
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let root = std::env::temp_dir().join(format!(
        "alan-linux-reification-{normalized_label}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let target = root.join("target");
    let scratch = root.join("scratch");
    std::fs::create_dir_all(&source)
        .and_then(|()| std::fs::create_dir_all(&target))
        .and_then(|()| std::fs::create_dir_all(&scratch))
        .map_err(|err| format!("{label} probe setup failed: {err}"))?;
    std::fs::write(source.join("probe-file"), b"probe")
        .map_err(|err| format!("{label} probe setup failed: {err}"))?;

    Ok(LinuxProbeTree {
        root,
        source,
        target,
        scratch,
    })
}

/// Whether an OS-enforced sandbox backend is active on this host. The policy
/// consults this for safe degradation: without one, bash must not auto-run.
pub fn os_backend_active() -> bool {
    detect_backend().is_os_enforced()
}

/// Name of the active execution backend for audits, rollouts, and snapshots (the
/// real `seatbelt`/`landlock` when one is enforcing, else the path guard).
pub fn active_backend_name() -> &'static str {
    detect_backend().name()
}

/// Path semantics for the active execution backend.
pub fn active_backend_path_mode() -> &'static str {
    detect_backend().path_mode()
}

/// Detect the strongest projection backend, ignoring Linux reification.
pub fn detect_projection_backend() -> SandboxBackendKind {
    if cfg!(target_os = "macos") && seatbelt_available() {
        SandboxBackendKind::Seatbelt
    } else if cfg!(target_os = "linux") && landlock_available() {
        SandboxBackendKind::Landlock
    } else {
        SandboxBackendKind::HostMountPathGuard
    }
}

/// Detect the strongest available backend for the host.
///
/// Conservative by design: returns an OS backend only when its tooling is
/// detected, otherwise the path-guard fallback.
pub fn detect_backend() -> SandboxBackendKind {
    if cfg!(target_os = "macos") && seatbelt_available() {
        SandboxBackendKind::Seatbelt
    } else if cfg!(target_os = "linux") {
        linux_reified_namespace_backend_readiness().selected_backend
    } else {
        SandboxBackendKind::HostMountPathGuard
    }
}

#[cfg(target_os = "macos")]
fn seatbelt_available() -> bool {
    Path::new("/usr/bin/sandbox-exec").exists()
}

#[cfg(not(target_os = "macos"))]
fn seatbelt_available() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn landlock_available() -> bool {
    // Landlock requires a sufficiently recent kernel with the LSM enabled.
    // Conservatively detect the presence of the Landlock LSM in the kernel's
    // advertised LSM list; absence means degrade to the path guard.
    std::fs::read_to_string("/sys/kernel/security/lsm")
        .map(|lsms| lsms.split(',').any(|lsm| lsm.trim() == "landlock"))
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn landlock_available() -> bool {
    false
}

/// Apply a Landlock ruleset to the current (child) process: allow reads
/// everywhere, restrict writes to writable roots and temp directories, and deny
/// all outbound/listening TCP network access (Landlock ABI v4, best-effort).
///
/// Intended to run in a `pre_exec` hook so it confines the spawned shell, not
/// the host Process. Returns an `io::Error` (fail-closed) when enforcement fails.
#[cfg(target_os = "linux")]
pub fn apply_landlock(
    writable_roots: &[PathBuf],
    read_denylist: &[PathBuf],
    allow_network: bool,
) -> std::io::Result<()> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr,
        RulesetCreatedAttr, path_beneath_rules,
    };

    // Handle the highest filesystem ABI the crate knows so newer rights (e.g.
    // `LANDLOCK_ACCESS_FS_TRUNCATE` from ABI v3, `IOCTL_DEV` from v5) are also
    // restricted to the host_mount — Landlock leaves any *unhandled* right
    // allowed, so a V1-only ruleset would let `truncate(2)` escape the host_mount.
    // `CompatLevel::BestEffort` degrades gracefully on older kernels.
    let fs_abi = ABI::V5;
    let net_abi = ABI::V4;
    // Landlock's allow-list model cannot express a read denylist while reads are
    // otherwise allowed everywhere. P1 threads the value for signature stability.
    let _ = read_denylist;

    let mut writable = writable_roots.to_vec();
    for extra in [
        "/tmp",
        "/var/tmp",
        // Standard writable device files must stay writable so ordinary commands
        // (e.g. `echo > /dev/null`) are not broken by confinement. Block-device
        // writes are denied earlier by the policy red line, not here.
        "/dev/null",
        "/dev/zero",
        "/dev/full",
        "/dev/tty",
        "/dev/random",
        "/dev/urandom",
    ] {
        writable.push(std::path::PathBuf::from(extra));
    }
    if let Ok(tmpdir) = std::env::var("TMPDIR")
        && !tmpdir.is_empty()
    {
        writable.push(std::path::PathBuf::from(tmpdir));
    }
    writable.retain(|path| path.exists());

    let to_io = |err| std::io::Error::other(format!("landlock enforcement failed: {err}"));
    // Best-effort: on kernels without Landlock network support the net handling
    // degrades rather than erroring; `confines_network()` is the authority the
    // policy consults before treating network as contained.
    let mut ruleset = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(fs_abi))
        .map_err(to_io)?;
    if !allow_network {
        // Handle (and, by adding no net rules, deny) all TCP bind/connect.
        ruleset = ruleset
            .handle_access(AccessNet::from_all(net_abi))
            .map_err(to_io)?;
    }
    ruleset
        .create()
        .map_err(to_io)?
        .add_rules(path_beneath_rules(["/"], AccessFs::from_read(fs_abi)))
        .map_err(to_io)?
        .add_rules(path_beneath_rules(&writable, AccessFs::from_all(fs_abi)))
        .map_err(to_io)?
        .restrict_self()
        .map_err(to_io)?;
    Ok(())
}

/// Whether the active backend confines outbound network at the OS level. The
/// policy consults this before letting a network escalation be reviewer-judged
/// (vs forced to a human when no network backstop exists).
pub fn confines_network() -> bool {
    match detect_backend() {
        // Seatbelt profile denies `network*`.
        SandboxBackendKind::Seatbelt => true,
        // Reified namespace selection requires network confinement.
        SandboxBackendKind::LinuxReifiedNamespace => true,
        // Landlock confines network only on kernels exposing ABI v4 net access.
        SandboxBackendKind::Landlock => landlock_supports_network(),
        SandboxBackendKind::HostMountPathGuard => false,
    }
}

#[cfg(target_os = "linux")]
fn landlock_supports_network() -> bool {
    use landlock::{ABI, Access, AccessNet, Ruleset, RulesetAttr};
    // Probe: a strict ruleset that handles network access only succeeds when the
    // kernel actually supports Landlock network rules (ABI v4+).
    Ruleset::default()
        .handle_access(AccessNet::from_all(ABI::V4))
        .and_then(|ruleset| ruleset.create())
        .is_ok()
}

#[cfg(not(target_os = "linux"))]
fn landlock_supports_network() -> bool {
    false
}

#[cfg(test)]
#[path = "sandbox_backend_tests.rs"]
mod tests;
