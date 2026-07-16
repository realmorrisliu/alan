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
    let guard = SandboxBackendKind::HostMountPathGuard;
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
        SandboxBackendKind::HostMountPathGuard.name(),
        "host_mount_path_guard"
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
        SandboxBackendKind::HostMountPathGuard.path_mode(),
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
    let host_mount_root = PathBuf::from("/work/space");
    let writable_roots = vec![host_mount_root.clone()];
    let profile = seatbelt_profile(&writable_roots, &[], false);
    assert_eq!(
        profile,
        pre_refactor_single_host_mount_profile(&host_mount_root, false)
    );
    assert!(!profile.contains("(deny file-read*"));
}

#[test]
fn seatbelt_profile_emits_read_denies_when_configured() {
    let writable_roots = vec![PathBuf::from("/work/space")];
    let read_denylist = vec![PathBuf::from("/secret"), PathBuf::from("/home/me/.netrc")];
    let profile = seatbelt_profile(&writable_roots, &read_denylist, false);
    assert!(profile.contains("(deny file-read* (literal \"/secret\") (subpath \"/secret\"))"));
    assert!(
        profile.contains(
            "(deny file-read* (literal \"/home/me/.netrc\") (subpath \"/home/me/.netrc\"))"
        )
    );
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
fn seatbelt_enforces_host_mount_write_boundary_on_macos() {
    if !super::seatbelt_available() {
        return; // sandbox-exec not present; nothing to enforce
    }
    let host_mount = tempfile::tempdir().unwrap();
    let writable_roots = vec![host_mount.path().to_path_buf()];
    let profile = seatbelt_profile(&writable_roots, &[], false);
    let canonical_host_mount = std::fs::canonicalize(host_mount.path()).unwrap();

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

    // In-host_mount write succeeds. If it fails, this environment cannot run
    // `sandbox-exec` (e.g. a restricted CI runner) — skip rather than fail.
    let inside_file = canonical_host_mount.join("inside.txt");
    let ok = run(format!("echo hi > {}", inside_file.display()));
    if !ok.success() || !inside_file.exists() {
        return;
    }

    // Out-of-host_mount write (under HOME, outside host_mount and temp roots)
    // is blocked by the kernel regardless of command syntax.
    let escape_file =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".alan_seatbelt_escape_test");
    let _ = std::fs::remove_file(&escape_file);
    let blocked = run(format!("echo hi > {}", escape_file.display()));
    assert!(
        !blocked.success(),
        "out-of-host_mount write should be denied"
    );
    assert!(
        !escape_file.exists(),
        "kernel must prevent the out-of-host_mount file from being created"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn landlock_enforces_host_mount_write_boundary_on_linux() {
    if !super::landlock_available() {
        return; // kernel without Landlock; nothing to enforce
    }
    use std::os::unix::process::CommandExt;
    let host_mount = tempfile::tempdir().unwrap();
    let host_mount_path = host_mount.path().to_path_buf();
    let escape_file =
        std::path::PathBuf::from(std::env::var("HOME").unwrap()).join(".alan_landlock_escape_test");
    let _ = std::fs::remove_file(&escape_file);

    let run = |script: String| {
        let root = host_mount_path.clone();
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(script);
        // SAFETY: pre_exec runs in the forked child; the closure owns root
        // and performs only the bounded Landlock setup before exec.
        unsafe {
            cmd.pre_exec(move || super::apply_landlock(std::slice::from_ref(&root), &[], false));
        }
        cmd.status().unwrap()
    };

    // If the in-host_mount write fails, this environment cannot apply
    // Landlock (e.g. a restricted runner) — skip rather than fail.
    let inside = host_mount.path().join("inside.txt");
    if !run(format!("echo hi > {}", inside.display())).success() || !inside.exists() {
        return;
    }

    let blocked = run(format!("echo hi > {}", escape_file.display()));
    assert!(
        !blocked.success(),
        "out-of-host_mount write should be denied"
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
    let host_mount = tempfile::tempdir().unwrap();
    let host_mount_path = host_mount.path().to_path_buf();
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(probe);
    // SAFETY: pre_exec runs in the forked child; the closure owns the mount
    // path and performs only the bounded Landlock setup before exec.
    unsafe {
        cmd.pre_exec(move || {
            super::apply_landlock(std::slice::from_ref(&host_mount_path), &[], false)
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
            | SandboxBackendKind::HostMountPathGuard
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
        SandboxBackendKind::HostMountPathGuard
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
        SandboxBackendKind::HostMountPathGuard
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
fn linux_reification_selection_requires_toolchain_smoke() {
    let complete = complete_linux_reification_report();
    let runner_smoke = available_capability();
    let toolchain_smoke = unavailable_capability("current PATH cannot be preserved");

    assert_eq!(
        preferred_linux_backend_with_reification_runner_and_toolchain(
            &complete,
            &runner_smoke,
            &toolchain_smoke,
            true
        ),
        SandboxBackendKind::Landlock
    );
    assert_eq!(
        preferred_linux_backend_with_reification_runner_and_toolchain(
            &complete,
            &runner_smoke,
            &toolchain_smoke,
            false
        ),
        SandboxBackendKind::HostMountPathGuard
    );
    assert_eq!(
        preferred_linux_backend_with_reification_runner_and_toolchain(
            &complete,
            &runner_smoke,
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
        toolchain_smoke: unavailable_capability("current PATH cannot be preserved"),
        selected_backend: SandboxBackendKind::Landlock,
    };

    let fields = readiness.audit_fields();

    assert!(fields.contains(&(
        "runner_smoke",
        "unavailable(runner smoke failed)".to_string()
    )));
    assert!(fields.contains(&(
        "toolchain_smoke",
        "unavailable(current PATH cannot be preserved)".to_string()
    )));
    assert!(fields.contains(&("selected_backend", "landlock".to_string())));
    assert!(fields.contains(&("path_mode", "projected_host_paths".to_string())));
}

fn pre_refactor_single_host_mount_profile(host_mount_root: &Path, allow_network: bool) -> String {
    let canonical_root = canonical_string(host_mount_root);
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
