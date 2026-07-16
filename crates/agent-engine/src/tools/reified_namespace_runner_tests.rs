use super::*;

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
#[cfg(not(target_os = "linux"))]
#[test]
fn linux_runner_reports_non_linux_unavailable_without_ambient_execution() {
    let plan = ReifiedNamespacePlan::primary_mount(
        "/host/host_mount",
        "/host/host_mount",
        vec!["sh".to_string(), "-c".to_string(), "pwd".to_string()],
        NetworkPosture::Deny,
    )
    .unwrap();
    let runner =
        LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::HostMountPathGuard);

    let error = runner.run(&plan).unwrap_err();

    assert_eq!(
        error.reason,
        "linux reified namespace runner is only available on Linux"
    );
    assert_eq!(
        error.fallback_backend,
        SandboxBackendKind::HostMountPathGuard
    );
    assert!(
        error
            .audit_fields
            .contains(&("backend", "linux_reified_namespace".to_string()))
    );
    assert!(
        error
            .audit_fields
            .contains(&("fallback_backend", "host_mount_path_guard".to_string()))
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
        network_confinement: LinuxReificationCapability::unavailable("network namespaces disabled"),
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
    let runner = LinuxReifiedNamespaceRunner::with_fallback_backend(SandboxBackendKind::Landlock);
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

    let host_mount = tempfile::tempdir().unwrap();
    let readonly = tempfile::tempdir().unwrap();
    let secret_home = tempfile::tempdir().unwrap();
    std::fs::write(host_mount.path().join("visible.txt"), "visible").unwrap();
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
                host_mount.path(),
                ReifiedMountAccess::ReadWrite,
            ),
            ReifiedMountDeclaration::host(
                "/mnt/docs",
                readonly.path(),
                ReifiedMountAccess::ReadOnly,
            ),
        ],
        host_mount.path(),
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
        std::fs::read_to_string(host_mount.path().join("writable.txt")).unwrap(),
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

    let host_mount = tempfile::tempdir().unwrap();
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
    let plan = ReifiedNamespacePlan::primary_mount(
        host_mount.path(),
        host_mount.path(),
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
    let host_mount = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let substrate = tempfile::tempdir().unwrap();
    let plan = ReifiedNamespacePlan::derive(
        ReifiedNamespacePlanInput::new(
            vec![
                ReifiedMountDeclaration::host(
                    "/mnt/project",
                    host_mount.path(),
                    ReifiedMountAccess::ReadWrite,
                ),
                ReifiedMountDeclaration::host(
                    "/mnt/docs",
                    docs.path(),
                    ReifiedMountAccess::ReadOnly,
                ),
            ],
            host_mount.path(),
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
        build_linux_reified_namespace_command_with_helpers(&plan, &temp_root, &helpers).unwrap();

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
    let host_mount = tempfile::tempdir().unwrap();
    let plan = ReifiedNamespacePlan::primary_mount(
        host_mount.path(),
        host_mount.path(),
        vec!["sh".to_string(), "-c".to_string(), "true".to_string()],
        NetworkPosture::Allow,
    )
    .unwrap();
    let temp_root = ReifiedRunnerTemp::create(&plan).unwrap();

    let helpers = test_linux_setup_helpers();
    let command =
        build_linux_reified_namespace_command_with_helpers(&plan, &temp_root, &helpers).unwrap();

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
