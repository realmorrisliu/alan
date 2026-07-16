use super::*;

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
fn os_backend_active_matches_detection() {
    assert_eq!(os_backend_active(), detect_backend().is_os_enforced());
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
