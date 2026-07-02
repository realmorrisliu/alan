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

use std::path::{Component, Path, PathBuf};

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

    /// Whether the backend confines bash strongly enough to run shell wrappers
    /// and reviewer-route escalated bash: the workspace filesystem boundary AND
    /// network are kernel-enforced (Seatbelt). Landlock does not qualify — it
    /// cannot guarantee network confinement on older kernels — so it keeps the
    /// full shape parser and routes escalated bash to a human.
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
                | SandboxBackendKind::Landlock
                | SandboxBackendKind::WorkspacePathGuard
        ));
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
}
