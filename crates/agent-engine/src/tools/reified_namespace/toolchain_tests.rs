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
