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

use std::path::Path;

/// Available sandbox enforcement backends, in order of strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxBackendKind {
    /// macOS Seatbelt (`sandbox-exec`).
    Seatbelt,
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
            SandboxBackendKind::Landlock => "landlock",
            SandboxBackendKind::WorkspacePathGuard => "workspace_path_guard",
        }
    }

    /// Whether this backend enforces confinement at the OS level.
    pub const fn is_os_enforced(self) -> bool {
        matches!(
            self,
            SandboxBackendKind::Seatbelt | SandboxBackendKind::Landlock
        )
    }

    /// Safe-degradation rule: bash and network may proceed without prompting
    /// only when an OS-enforced backend is active. Without one, they MUST
    /// escalate — "sandbox unavailable" is never "sandbox off".
    pub const fn allows_unattended_bash_and_network(self) -> bool {
        self.is_os_enforced()
    }
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

/// Detect the strongest available backend for the host.
///
/// Conservative by design: returns an OS backend only when its tooling is
/// detected, otherwise the path-guard fallback.
pub fn detect_backend() -> SandboxBackendKind {
    if cfg!(target_os = "macos") && seatbelt_available() {
        SandboxBackendKind::Seatbelt
    } else if cfg!(target_os = "linux") && landlock_available() {
        SandboxBackendKind::Landlock
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
/// the workspace (plus the temp dir) and denies outbound network.
///
/// Uses an allow-by-default base then denies the two effects we care about
/// (network and out-of-workspace writes) and re-allows writes to the workspace
/// and temp locations. This keeps process exec, dynamic linking, and reads
/// working while still blocking network and writes that escape the workspace —
/// which is what the auto-approve boundary relies on.
pub fn seatbelt_profile(workspace_root: &Path, allow_network: bool) -> String {
    // sandbox-exec evaluates real (symlink-resolved) paths, so the subpath
    // rules must use canonical paths (e.g. /var -> /private/var on macOS).
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
    // Kernel-deny writes to protected subpaths, placed *after* the workspace allow
    // so the more-specific deny wins (SBPL: last match wins). This confines them
    // independently of command syntax — covering nested forms the path-guard
    // parser cannot see, e.g. `bash -lc 'echo x > .git/config'`.
    let protected_denies = super::sandbox::PROTECTED_SUBPATHS
        .iter()
        .map(|sub| {
            let path = sbpl_quote(&format!("{}/{sub}", canonical_root.trim_end_matches('/')));
            format!("(deny file-write* (subpath {path}))")
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Network is denied by default; an approved network call runs with it allowed
    // (still filesystem-confined) so approval is not futile.
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
         {protected_denies}\n\
         (allow file-write-data (literal \"/dev/null\") (literal \"/dev/stdout\") (literal \"/dev/stderr\") (literal \"/dev/tty\") (literal \"/dev/dtracehelper\"))\n"
    )
}

/// Apply a Landlock ruleset to the current (child) process: allow reads
/// everywhere, restrict writes to the workspace and temp directories, and deny
/// all outbound/listening TCP network access (Landlock ABI v4, best-effort).
///
/// Intended to run in a `pre_exec` hook so it confines the spawned shell, not
/// the daemon. Returns an `io::Error` (fail-closed) when enforcement fails.
#[cfg(target_os = "linux")]
pub fn apply_landlock(workspace_root: &Path, allow_network: bool) -> std::io::Result<()> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, Ruleset, RulesetAttr,
        RulesetCreatedAttr, path_beneath_rules,
    };

    let fs_abi = ABI::V1;
    let net_abi = ABI::V4;
    let mut writable = vec![workspace_root.to_path_buf()];
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn os_backends_are_enforced_and_allow_unattended() {
        assert!(SandboxBackendKind::Seatbelt.is_os_enforced());
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
        assert_eq!(SandboxBackendKind::Landlock.name(), "landlock");
        assert_eq!(
            SandboxBackendKind::WorkspacePathGuard.name(),
            "workspace_path_guard"
        );
    }

    #[test]
    fn seatbelt_profile_confines_writes_and_denies_network() {
        let profile = seatbelt_profile(&PathBuf::from("/work/space"), false);
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(deny file-write*)"));
        assert!(profile.contains("(allow file-write* (subpath \"/work/space\"))"));
    }

    #[test]
    fn seatbelt_profile_permits_network_when_approved() {
        // An approved network call runs with network allowed (still fs-confined).
        let approved = seatbelt_profile(&PathBuf::from("/work/space"), true);
        assert!(!approved.contains("(deny network*)"));
        assert!(approved.contains("(deny file-write*)"));
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
        let profile = seatbelt_profile(workspace.path(), false);
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
                cmd.pre_exec(move || super::apply_landlock(&root, false));
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
            cmd.pre_exec(move || super::apply_landlock(&workspace_path, false));
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
                | SandboxBackendKind::Landlock
                | SandboxBackendKind::WorkspacePathGuard
        ));
    }
}
