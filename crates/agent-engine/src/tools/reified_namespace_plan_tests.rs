use super::*;

fn shell_argv() -> Vec<String> {
    vec!["sh".to_string(), "-c".to_string(), "pwd".to_string()]
}

#[test]
fn primary_mount_mount_translates_cwd_to_namespace_path() {
    let plan = ReifiedNamespacePlan::primary_mount(
        "/host/host_mount",
        "/host/host_mount/src",
        shell_argv(),
        NetworkPosture::Deny,
    )
    .unwrap();

    assert_eq!(plan.declared_host_mounts.len(), 1);
    assert_eq!(
        plan.declared_host_mounts[0],
        ReifiedHostMount {
            namespace_path: PathBuf::from(DEFAULT_PRIMARY_MOUNT_NAMESPACE_PATH),
            host_path: PathBuf::from("/host/host_mount"),
            access: ReifiedMountAccess::ReadWrite,
        }
    );
    assert_eq!(plan.cwd, PathBuf::from("/mnt/source/src"));
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
            ReifiedMountDeclaration::host("/mnt/deps", "/host/deps", ReifiedMountAccess::ReadWrite),
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
    let host_mount = temp_dir.path().join("host_mount");
    let vendor = host_mount.join("vendor");
    let host_mount_link = temp_dir.path().join("host_mount-link");
    std::fs::create_dir_all(&vendor).unwrap();
    std::os::unix::fs::symlink(&host_mount, &host_mount_link).unwrap();

    let input = ReifiedNamespacePlanInput::new(
        vec![
            ReifiedMountDeclaration::host(
                "/mnt/project",
                &host_mount_link,
                ReifiedMountAccess::ReadWrite,
            ),
            ReifiedMountDeclaration::host("/mnt/vendor", &vendor, ReifiedMountAccess::ReadOnly),
        ],
        host_mount_link.join("vendor"),
        shell_argv(),
        NetworkPosture::Deny,
    );

    assert_eq!(
        ReifiedNamespacePlan::derive(input),
        Err(
            ReifiedNamespacePlanError::ReadOnlyHostMountOverlapsWritableMount {
                read_only_host_path: dunce::canonicalize(&vendor).unwrap(),
                writable_host_path: dunce::canonicalize(&host_mount).unwrap(),
            }
        )
    );
}

#[cfg(unix)]
#[test]
fn symlinked_projected_host_paths_are_normalized_before_translation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let host_mount = temp_dir.path().join("host_mount");
    let src = host_mount.join("src");
    let lib = src.join("lib.rs");
    let host_mount_link = temp_dir.path().join("host_mount-link");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(&lib, b"mod test;").unwrap();
    std::os::unix::fs::symlink(&host_mount, &host_mount_link).unwrap();

    let input = ReifiedNamespacePlanInput::new(
        vec![ReifiedMountDeclaration::host(
            "/mnt/project",
            &host_mount_link,
            ReifiedMountAccess::ReadWrite,
        )],
        host_mount_link.join("src"),
        shell_argv(),
        NetworkPosture::Deny,
    );
    let plan = ReifiedNamespacePlan::derive(input).unwrap();

    assert_eq!(plan.cwd, PathBuf::from("/mnt/project/src"));
    assert_eq!(
        plan.declared_host_mounts[0].host_path,
        dunce::canonicalize(&host_mount).unwrap()
    );
    assert_eq!(
        plan.translate_projected_host_path(&host_mount_link.join("src/lib.rs")),
        Some(PathBuf::from("/mnt/project/src/lib.rs"))
    );
    assert_eq!(
        plan.translate_projected_host_path(&host_mount_link.join("generated/new.rs")),
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
    let host_mount = temp_dir.path().join("host_mount");
    let src = host_mount.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(host_mount.join("Cargo.toml"), b"[package]\n").unwrap();
    let input = ReifiedNamespacePlanInput::new(
        vec![ReifiedMountDeclaration::host(
            "/mnt/source",
            &host_mount,
            ReifiedMountAccess::ReadWrite,
        )],
        &host_mount,
        shell_argv(),
        NetworkPosture::Deny,
    );
    let plan = ReifiedNamespacePlan::derive(input).unwrap();

    assert_eq!(
        plan.translate_projected_host_path(&host_mount.join("src/../Cargo.toml")),
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
