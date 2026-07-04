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
//! (`apply_landlock`). Both confine writes to the workspace + temp and deny
//! network by default (Seatbelt `network*`, Landlock ABI v4 TCP). An *approved*
//! network call (capability == Network reaching execution) runs with network
//! permitted (still filesystem-confined), so reviewer/human approval is not
//! futile. Detection is conservative: an OS backend is reported only when its
//! tooling is present, otherwise the path-guard fallback (under which bash must
//! not auto-run — the policy escalates it).

use std::fmt;
#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// Available sandbox enforcement backends, in order of strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    /// macOS Seatbelt (`sandbox-exec`).
    Seatbelt,
    /// Linux user/mount namespace reification (full read isolation).
    LinuxReifiedNamespace,
    /// Linux Landlock (filesystem) paired with seccomp/namespace (network).
    Landlock,
    /// Best-effort in-process workspace path guard (no OS enforcement).
    WorkspacePathGuard,
}

impl SandboxBackendKind {
    /// Stable name used in audit metadata and diagnostics.
    pub const fn name(self) -> &'static str {
        match self {
            SandboxBackendKind::Seatbelt => "seatbelt",
            SandboxBackendKind::LinuxReifiedNamespace => "linux_reified_namespace",
            SandboxBackendKind::Landlock => "landlock",
            SandboxBackendKind::WorkspacePathGuard => "workspace_path_guard",
        }
    }

    /// Whether native subprocess paths are host-projected or namespace-reified.
    pub const fn path_mode(self) -> &'static str {
        match self {
            SandboxBackendKind::LinuxReifiedNamespace => "reified_namespace_paths",
            SandboxBackendKind::Seatbelt
            | SandboxBackendKind::Landlock
            | SandboxBackendKind::WorkspacePathGuard => "projected_host_paths",
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
    /// and reviewer-route escalated bash: the workspace filesystem boundary AND
    /// network are kernel-enforced (Seatbelt). Landlock does not qualify because
    /// network confinement is kernel-conditional, and Linux reified namespace
    /// does not qualify until protected subpaths are carved out of the writable
    /// workspace mount. Conservative backends keep the full shape parser and
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
    pub selected_backend: SandboxBackendKind,
}

impl LinuxReifiedNamespaceBackendReadiness {
    /// Stable fields for startup/debug audits.
    pub fn audit_fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = self.capability_report.audit_fields();
        fields.extend([
            ("runner_smoke", self.runner_smoke.audit_value()),
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
    if report.is_selectable() && runner_smoke.is_available() {
        SandboxBackendKind::LinuxReifiedNamespace
    } else if landlock_is_available {
        SandboxBackendKind::Landlock
    } else {
        SandboxBackendKind::WorkspacePathGuard
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
    let selected_backend = preferred_linux_backend_with_reification_and_runner(
        &capability_report,
        &runner_smoke,
        false,
    );
    LinuxReifiedNamespaceBackendReadiness {
        capability_report,
        runner_smoke,
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
    let selected_backend = preferred_linux_backend_with_reification_and_runner(
        &capability_report,
        &runner_smoke,
        landlock_available(),
    );
    LinuxReifiedNamespaceBackendReadiness {
        capability_report,
        runner_smoke,
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

/// Name of the active execution backend for audits/sessions/snapshots (the
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
        SandboxBackendKind::WorkspacePathGuard
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
        SandboxBackendKind::WorkspacePathGuard
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

/// Generate a macOS Seatbelt (SBPL) profile that confines filesystem writes to
/// writable roots (plus the temp dir) and denies outbound network.
///
/// Uses an allow-by-default base then denies the two effects we care about
/// (network and out-of-workspace writes) and re-allows writes to the workspace
/// and temp locations. This keeps process exec, dynamic linking, and reads
/// working while still blocking network and writes that escape writable roots —
/// which is what the auto-approve boundary relies on.
pub fn seatbelt_profile(
    writable_roots: &[PathBuf],
    read_denylist: &[PathBuf],
    allow_network: bool,
) -> String {
    // sandbox-exec evaluates real (symlink-resolved) paths, so the subpath
    // rules must use canonical paths (e.g. /var -> /private/var on macOS).
    let tmpdir = std::env::var("TMPDIR").ok();
    let mut writable = writable_roots
        .iter()
        .map(|root| sbpl_quote(&canonical_string(root)))
        .collect::<Vec<_>>();
    writable.extend([
        sbpl_quote("/tmp"),
        sbpl_quote("/private/tmp"),
        sbpl_quote("/private/var/folders"),
    ]);
    if let Some(tmpdir) = tmpdir.as_deref().filter(|value| !value.is_empty()) {
        writable.push(sbpl_quote(
            canonical_string(Path::new(tmpdir.trim_end_matches('/'))).as_str(),
        ));
    }
    let write_allows = writable
        .iter()
        .map(|path| format!("(allow file-write* (subpath {path}))"))
        .collect::<Vec<_>>()
        .join("\n");
    let read_denylist = read_denylist_excluding_writable_roots(read_denylist, writable_roots);
    let read_denies = read_denylist
        .iter()
        .map(|path| {
            let path = sbpl_quote(&canonical_string(path));
            format!("(deny file-read* (literal {path}) (subpath {path}))\n")
        })
        .collect::<String>();
    // NOTE: we do NOT kernel-deny the protected subpaths (`.git`/`.alan`/
    // `.agents`). The kernel cannot distinguish a tool's tampering from the
    // legitimate program-internal writes those dirs are designed for — denying
    // `.git` breaks `git` itself (init/add/commit all write `.git`), and denying
    // `.alan` breaks the agent's own `.alan/memory`. Protected-subpath tampering is
    // instead blocked by the path-guard parser (direct + shell-wrapper-nested path
    // writes), which leaves program-internal writes (git porcelain, memory) intact.
    // The OS sandbox's role here is the workspace + network boundary.
    let network_rule = if allow_network {
        ""
    } else {
        "(deny network*)\n"
    };
    format!(
        "(version 1)\n\
         (allow default)\n\
         {network_rule}\
         {read_denies}\
         (deny file-write*)\n\
         {write_allows}\n\
         (allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n"
    )
}

pub(crate) fn read_denylist_excluding_writable_roots(
    read_denylist: &[PathBuf],
    writable_roots: &[PathBuf],
) -> Vec<PathBuf> {
    read_denylist
        .iter()
        .filter(|deny_path| !read_deny_matches_any_writable_root(deny_path, writable_roots))
        .cloned()
        .collect()
}

fn read_deny_matches_any_writable_root(deny_path: &Path, writable_roots: &[PathBuf]) -> bool {
    let deny_variants = comparable_path_variants(deny_path);
    writable_roots.iter().any(|writable_root| {
        let writable_variants = comparable_path_variants(writable_root);
        deny_variants
            .iter()
            .any(|deny| writable_variants.iter().any(|writable| writable == deny))
    })
}

/// Apply a Landlock ruleset to the current (child) process: allow reads
/// everywhere, restrict writes to writable roots and temp directories, and deny
/// all outbound/listening TCP network access (Landlock ABI v4, best-effort).
///
/// Intended to run in a `pre_exec` hook so it confines the spawned shell, not
/// the daemon. Returns an `io::Error` (fail-closed) when enforcement fails.
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
    // restricted to the workspace — Landlock leaves any *unhandled* right
    // allowed, so a V1-only ruleset would let `truncate(2)` escape the workspace.
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
        SandboxBackendKind::WorkspacePathGuard => false,
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

/// Quote a path for inclusion in an SBPL literal/subpath form.
fn sbpl_quote(path: &str) -> String {
    format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Resolve a path to its canonical (symlink-free) string, falling back to the
/// lexical path when it cannot be canonicalized (e.g. it does not exist yet).
fn canonical_string(path: &Path) -> String {
    std::fs::canonicalize(path)
        .map(|resolved| resolved.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn comparable_path_variants(path: &Path) -> Vec<PathBuf> {
    let mut variants = vec![lexically_normalize_path(path)];
    if let Ok(canonical) = std::fs::canonicalize(path)
        && !variants.contains(&canonical)
    {
        variants.push(canonical);
    }
    variants
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn os_backends_are_enforced_and_allow_unattended() {
        assert!(SandboxBackendKind::Seatbelt.is_os_enforced());
        assert!(SandboxBackendKind::LinuxReifiedNamespace.is_os_enforced());
        assert!(SandboxBackendKind::Landlock.is_os_enforced());
        assert!(SandboxBackendKind::Seatbelt.allows_unattended_bash_and_network());
    }

    #[test]
    fn path_guard_degrades_safely() {
        let guard = SandboxBackendKind::WorkspacePathGuard;
        assert!(!guard.is_os_enforced());
        // The security-critical rule: no OS backend => bash/network escalate.
        assert!(!guard.allows_unattended_bash_and_network());
    }

    #[test]
    fn backend_names_are_stable() {
        assert_eq!(SandboxBackendKind::Seatbelt.name(), "seatbelt");
        assert_eq!(
            SandboxBackendKind::LinuxReifiedNamespace.name(),
            "linux_reified_namespace"
        );
        assert_eq!(SandboxBackendKind::Landlock.name(), "landlock");
        assert_eq!(
            SandboxBackendKind::WorkspacePathGuard.name(),
            "workspace_path_guard"
        );
    }

    #[test]
    fn backend_path_modes_are_stable() {
        assert_eq!(
            SandboxBackendKind::Seatbelt.path_mode(),
            "projected_host_paths"
        );
        assert_eq!(
            SandboxBackendKind::Landlock.path_mode(),
            "projected_host_paths"
        );
        assert_eq!(
            SandboxBackendKind::WorkspacePathGuard.path_mode(),
            "projected_host_paths"
        );
        assert_eq!(
            SandboxBackendKind::LinuxReifiedNamespace.path_mode(),
            "reified_namespace_paths"
        );
    }

    #[test]
    fn seatbelt_profile_confines_writes_and_denies_network() {
        let writable_roots = vec![PathBuf::from("/work/space")];
        let profile = seatbelt_profile(&writable_roots, &[], false);
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("(allow file-write* (subpath \"/work/space\"))"));
    }

    #[test]
    fn seatbelt_profile_permits_network_when_approved() {
        // An approved network call runs with network allowed (still fs-confined).
        let writable_roots = vec![PathBuf::from("/work/space")];
        let approved = seatbelt_profile(&writable_roots, &[], true);
        assert!(!approved.contains("(deny network*)"));
        assert!(approved.contains("(deny file-write*)"));
    }

    #[test]
    fn seatbelt_profile_single_root_matches_pre_refactor_profile() {
        let workspace_root = PathBuf::from("/work/space");
        let writable_roots = vec![workspace_root.clone()];
        let profile = seatbelt_profile(&writable_roots, &[], false);
        assert_eq!(
            profile,
            pre_refactor_single_workspace_profile(&workspace_root, false)
        );
        assert!(!profile.contains("(deny file-read*"));
    }

    #[test]
    fn seatbelt_profile_emits_read_denies_when_configured() {
        let writable_roots = vec![PathBuf::from("/work/space")];
        let read_denylist = vec![PathBuf::from("/secret"), PathBuf::from("/home/me/.netrc")];
        let profile = seatbelt_profile(&writable_roots, &read_denylist, false);
        assert!(profile.contains("(deny file-read* (literal \"/secret\") (subpath \"/secret\"))"));
        assert!(profile.contains(
            "(deny file-read* (literal \"/home/me/.netrc\") (subpath \"/home/me/.netrc\"))"
        ));
    }

    #[test]
    fn seatbelt_profile_omits_exact_writable_root_read_denies() {
        let writable_roots = vec![PathBuf::from("/Users/alice/.alan")];
        let read_denylist = vec![
            PathBuf::from("/Users/alice"),
            PathBuf::from("/Users/alice/.alan"),
            PathBuf::from("/Users/alice/.alan-dev"),
            PathBuf::from("/Users/alice/.ssh"),
        ];
        let profile = seatbelt_profile(&writable_roots, &read_denylist, false);

        assert!(profile.contains("(deny file-read* (literal \"/Users/alice\")"));
        assert!(!profile.contains("(deny file-read* (literal \"/Users/alice/.alan\")"));
        assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.alan-dev\")"));
        assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.ssh\")"));
    }

    #[test]
    fn seatbelt_profile_preserves_parent_read_denies_for_nested_writable_roots() {
        let writable_roots = vec![PathBuf::from("/Users/alice/.ssh/project")];
        let read_denylist = vec![PathBuf::from("/Users/alice/.ssh")];
        let profile = seatbelt_profile(&writable_roots, &read_denylist, false);

        assert!(profile.contains("(deny file-read* (literal \"/Users/alice/.ssh\")"));
    }

    #[test]
    fn os_backend_active_matches_detection() {
        assert_eq!(os_backend_active(), detect_backend().is_os_enforced());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn seatbelt_enforces_workspace_write_boundary_on_macos() {
        if !super::seatbelt_available() {
            return; // sandbox-exec not present; nothing to enforce
        }
        let workspace = tempfile::tempdir().unwrap();
        let writable_roots = vec![workspace.path().to_path_buf()];
        let profile = seatbelt_profile(&writable_roots, &[], false);
        let canonical_workspace = std::fs::canonicalize(workspace.path()).unwrap();

        let run = |script: String| {
            std::process::Command::new("/usr/bin/sandbox-exec")
                .arg("-p")
                .arg(&profile)
                .arg("sh")
                .arg("-c")
                .arg(script)
                .status()
                .unwrap()
        };

        // In-workspace write succeeds. If it fails, this environment cannot run
        // `sandbox-exec` (e.g. a restricted CI runner) — skip rather than fail.
        let inside_file = canonical_workspace.join("inside.txt");
        let ok = run(format!("echo hi > {}", inside_file.display()));
        if !ok.success() || !inside_file.exists() {
            return;
        }

        // Out-of-workspace write (under HOME, outside workspace and temp roots)
        // is blocked by the kernel regardless of command syntax.
        let escape_file = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join(".alan_seatbelt_escape_test");
        let _ = std::fs::remove_file(&escape_file);
        let blocked = run(format!("echo hi > {}", escape_file.display()));
        assert!(
            !blocked.success(),
            "out-of-workspace write should be denied"
        );
        assert!(
            !escape_file.exists(),
            "kernel must prevent the out-of-workspace file from being created"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn landlock_enforces_workspace_write_boundary_on_linux() {
        if !super::landlock_available() {
            return; // kernel without Landlock; nothing to enforce
        }
        use std::os::unix::process::CommandExt;
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_path_buf();
        let escape_file = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join(".alan_landlock_escape_test");
        let _ = std::fs::remove_file(&escape_file);

        let run = |script: String| {
            let root = workspace_path.clone();
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(script);
            unsafe {
                cmd.pre_exec(move || {
                    super::apply_landlock(std::slice::from_ref(&root), &[], false)
                });
            }
            cmd.status().unwrap()
        };

        // If the in-workspace write fails, this environment cannot apply
        // Landlock (e.g. a restricted runner) — skip rather than fail.
        let inside = workspace.path().join("inside.txt");
        if !run(format!("echo hi > {}", inside.display())).success() || !inside.exists() {
            return;
        }

        let blocked = run(format!("echo hi > {}", escape_file.display()));
        assert!(
            !blocked.success(),
            "out-of-workspace write should be denied"
        );
        assert!(!escape_file.exists());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn landlock_confines_network_on_linux() {
        if !super::landlock_available() || !super::landlock_supports_network() {
            return; // kernel without Landlock network support; nothing to enforce
        }
        use std::os::unix::process::CommandExt;
        let probe = "python3 -c 'import socket; socket.setdefaulttimeout(3); \
             socket.create_connection((\"1.1.1.1\",80)); print(\"CONNECTED\")' \
             2>/dev/null || echo BLOCKED";

        // Control: unconfined, the probe must connect (otherwise the VM has no
        // outbound network / no python3 and the test is meaningless — skip).
        let control = std::process::Command::new("sh")
            .arg("-c")
            .arg(probe)
            .output()
            .unwrap();
        if !String::from_utf8_lossy(&control.stdout).contains("CONNECTED") {
            return;
        }

        // Confined: the same probe under Landlock net confinement must be blocked.
        let workspace = tempfile::tempdir().unwrap();
        let workspace_path = workspace.path().to_path_buf();
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(probe);
        unsafe {
            cmd.pre_exec(move || {
                super::apply_landlock(std::slice::from_ref(&workspace_path), &[], false)
            });
        }
        let output = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("BLOCKED") && !stdout.contains("CONNECTED"),
            "TCP connect should be denied under Landlock net confinement, got: {stdout}"
        );
        assert!(super::confines_network());
    }

    #[test]
    fn detect_backend_returns_a_known_kind() {
        // Detection must always yield a usable backend (possibly the fallback).
        let kind = detect_backend();
        assert!(matches!(
            kind,
            SandboxBackendKind::Seatbelt
                | SandboxBackendKind::LinuxReifiedNamespace
                | SandboxBackendKind::Landlock
                | SandboxBackendKind::WorkspacePathGuard
        ));
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn detect_backend_does_not_select_linux_reified_namespace_on_non_linux() {
        assert_ne!(detect_backend(), SandboxBackendKind::LinuxReifiedNamespace);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detect_backend_matches_linux_reified_namespace_readiness() {
        assert_eq!(
            detect_backend(),
            linux_reified_namespace_backend_readiness().selected_backend
        );
    }

    #[test]
    fn linux_reification_report_formats_audit_fields() {
        let report = LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: available_capability(),
            user_namespace: available_capability(),
            mount_namespace: available_capability(),
            pid_namespace: available_capability(),
            bind_mount: available_capability(),
            read_only_remount: available_capability(),
            scratch_tmp_mount: available_capability(),
            network_confinement: unavailable_capability("network namespace unavailable"),
        });

        assert_eq!(report.backend_name(), "linux_reified_namespace");
        assert_eq!(report.status(), LinuxReificationStatus::Degraded);
        assert_eq!(
            report.audit_fields(),
            vec![
                ("backend", "linux_reified_namespace".to_string()),
                ("status", "degraded".to_string()),
                ("linux_host", "available".to_string()),
                ("user_namespace", "available".to_string()),
                ("mount_namespace", "available".to_string()),
                ("pid_namespace", "available".to_string()),
                ("bind_mount", "available".to_string()),
                ("read_only_remount", "available".to_string()),
                ("scratch_tmp_mount", "available".to_string()),
                (
                    "network_confinement",
                    "unavailable(network namespace unavailable)".to_string()
                ),
            ]
        );
        assert_eq!(
            report.to_string(),
            "linux_reified_namespace: degraded (linux_host=available, \
             user_namespace=available, mount_namespace=available, pid_namespace=available, \
             bind_mount=available, read_only_remount=available, scratch_tmp_mount=available, \
             network_confinement=unavailable(network namespace unavailable))"
        );
    }

    #[test]
    fn linux_reification_report_lists_unavailable_reasons() {
        let report = LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: unavailable_capability("not a linux host"),
            user_namespace: unavailable_capability("not a linux host"),
            mount_namespace: unavailable_capability("requires available user namespace"),
            pid_namespace: unavailable_capability("requires available user namespace"),
            bind_mount: unavailable_capability("requires available mount namespace"),
            read_only_remount: available_capability(),
            scratch_tmp_mount: available_capability(),
            network_confinement: unavailable_capability("network namespace unavailable"),
        });

        assert_eq!(
            report.unavailable_reasons(),
            vec![
                "linux_host: not a linux host".to_string(),
                "user_namespace: not a linux host".to_string(),
                "mount_namespace: requires available user namespace".to_string(),
                "pid_namespace: requires available user namespace".to_string(),
                "bind_mount: requires available mount namespace".to_string(),
                "network_confinement: network namespace unavailable".to_string(),
            ]
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn linux_reification_probe_reports_non_linux_unavailable() {
        let report = probe_linux_reification();

        assert_eq!(report.status(), LinuxReificationStatus::Unavailable);
        assert_eq!(
            report.unavailable_reasons(),
            vec![
                "linux_host: not a linux host".to_string(),
                "user_namespace: not a linux host".to_string(),
                "mount_namespace: not a linux host".to_string(),
                "pid_namespace: not a linux host".to_string(),
                "bind_mount: not a linux host".to_string(),
                "read_only_remount: not a linux host".to_string(),
                "scratch_tmp_mount: not a linux host".to_string(),
                "network_confinement: not a linux host".to_string(),
            ]
        );
    }

    #[test]
    fn linux_reification_fallback_prefers_safe_backends() {
        let complete = complete_linux_reification_report();
        assert_eq!(
            preferred_linux_backend_with_reification(&complete, true),
            SandboxBackendKind::LinuxReifiedNamespace
        );

        let incomplete = LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: available_capability(),
            user_namespace: unavailable_capability("user namespaces disabled"),
            mount_namespace: available_capability(),
            pid_namespace: available_capability(),
            bind_mount: available_capability(),
            read_only_remount: available_capability(),
            scratch_tmp_mount: available_capability(),
            network_confinement: available_capability(),
        });
        assert_eq!(
            preferred_linux_backend_with_reification(&incomplete, true),
            SandboxBackendKind::Landlock
        );
        assert_eq!(
            preferred_linux_backend_with_reification(&incomplete, false),
            SandboxBackendKind::WorkspacePathGuard
        );

        let degraded = LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: available_capability(),
            user_namespace: available_capability(),
            mount_namespace: available_capability(),
            pid_namespace: available_capability(),
            bind_mount: available_capability(),
            read_only_remount: available_capability(),
            scratch_tmp_mount: available_capability(),
            network_confinement: unavailable_capability("network confinement unavailable"),
        });
        assert_eq!(
            preferred_linux_backend_with_reification(&degraded, true),
            SandboxBackendKind::Landlock
        );
    }

    #[test]
    fn linux_reification_selection_requires_runner_smoke() {
        let complete = complete_linux_reification_report();
        let runner_smoke = unavailable_capability("runner smoke failed");

        assert_eq!(
            preferred_linux_backend_with_reification_and_runner(&complete, &runner_smoke, true),
            SandboxBackendKind::Landlock
        );
        assert_eq!(
            preferred_linux_backend_with_reification_and_runner(&complete, &runner_smoke, false),
            SandboxBackendKind::WorkspacePathGuard
        );
        assert_eq!(
            preferred_linux_backend_with_reification_and_runner(
                &complete,
                &available_capability(),
                true
            ),
            SandboxBackendKind::LinuxReifiedNamespace
        );
    }

    #[test]
    fn linux_reification_readiness_audit_names_selected_backend_and_path_mode() {
        let readiness = LinuxReifiedNamespaceBackendReadiness {
            capability_report: complete_linux_reification_report(),
            runner_smoke: unavailable_capability("runner smoke failed"),
            selected_backend: SandboxBackendKind::Landlock,
        };

        let fields = readiness.audit_fields();

        assert!(fields.contains(&(
            "runner_smoke",
            "unavailable(runner smoke failed)".to_string()
        )));
        assert!(fields.contains(&("selected_backend", "landlock".to_string())));
        assert!(fields.contains(&("path_mode", "projected_host_paths".to_string())));
    }

    fn pre_refactor_single_workspace_profile(workspace_root: &Path, allow_network: bool) -> String {
        let canonical_root = canonical_string(workspace_root);
        let root = sbpl_quote(&canonical_root);
        let tmpdir = std::env::var("TMPDIR").ok();
        let mut writable = vec![
            root,
            sbpl_quote("/tmp"),
            sbpl_quote("/private/tmp"),
            sbpl_quote("/private/var/folders"),
        ];
        if let Some(tmpdir) = tmpdir.as_deref().filter(|value| !value.is_empty()) {
            writable.push(sbpl_quote(
                canonical_string(Path::new(tmpdir.trim_end_matches('/'))).as_str(),
            ));
        }
        let write_allows = writable
            .iter()
            .map(|path| format!("(allow file-write* (subpath {path}))"))
            .collect::<Vec<_>>()
            .join("\n");
        let network_rule = if allow_network {
            ""
        } else {
            "(deny network*)\n"
        };
        format!(
            "(version 1)\n\
             (allow default)\n\
             {network_rule}\
             (deny file-write*)\n\
             {write_allows}\n\
             (allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n"
        )
    }

    fn available_capability() -> LinuxReificationCapability {
        LinuxReificationCapability::available()
    }

    fn unavailable_capability(reason: &str) -> LinuxReificationCapability {
        LinuxReificationCapability::unavailable(reason)
    }

    fn complete_linux_reification_report() -> LinuxReificationCapabilityReport {
        LinuxReificationCapabilityReport::new(LinuxReificationCapabilities {
            linux_host: available_capability(),
            user_namespace: available_capability(),
            mount_namespace: available_capability(),
            pid_namespace: available_capability(),
            bind_mount: available_capability(),
            read_only_remount: available_capability(),
            scratch_tmp_mount: available_capability(),
            network_confinement: available_capability(),
        })
    }
}
