use super::*;

#[test]
fn default_execution_substrate_includes_command_path_directories() {
    let substrate = default_execution_substrate();
    for path in std::env::split_paths(LINUX_REIFIED_COMMAND_PATH) {
        assert!(
            substrate
                .iter()
                .any(|mount| mount.namespace_path == path && mount.host_path == path)
        );
    }
}

#[cfg(unix)]
#[test]
fn command_path_preserves_order_duplicates_and_alias_spelling() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("first")).unwrap();
    std::fs::create_dir(temp.path().join("second")).unwrap();
    std::os::unix::fs::symlink("first", temp.path().join("alias")).unwrap();
    let substrate = [ReifiedExecutionSubstrateMount::new(
        "/opt/runtime",
        temp.path(),
    )];
    let path = "/opt/runtime/second:/opt/runtime/alias:/opt/runtime/first:/opt/runtime/second";
    assert_eq!(
        validate_linux_command_path(Some(path.into()), &substrate).unwrap(),
        path
    );
}

#[cfg(unix)]
#[test]
fn command_path_rejects_unset_empty_relative_parent_nul_and_non_utf8() {
    use std::os::unix::ffi::OsStringExt;
    let substrate = default_execution_substrate();
    for path in [
        None,
        Some("".into()),
        Some(":/bin".into()),
        Some("/bin:".into()),
        Some("bin".into()),
        Some("/bin/../sbin".into()),
        Some("/bin\0".into()),
        Some(std::ffi::OsString::from_vec(vec![0xff])),
    ] {
        assert!(
            validate_linux_command_path(path.clone(), &substrate).is_err(),
            "{path:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn command_path_rejects_existing_unprojected_directory() {
    let temp = tempfile::tempdir().unwrap();
    let reason =
        validate_linux_command_path(Some(temp.path().as_os_str().into()), &[]).unwrap_err();
    assert!(reason.contains("outside the reified execution substrate"));
}

#[cfg(unix)]
#[test]
fn command_path_rejects_unprojected_entries_before_and_after_creation() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let path = missing.as_os_str().to_os_string();
    assert!(validate_linux_command_path(Some(path.clone()), &[]).is_err());
    std::fs::create_dir(&missing).unwrap();
    assert!(validate_linux_command_path(Some(path), &[]).is_err());
}

#[cfg(unix)]
#[test]
fn command_path_rejects_dangling_and_absolute_remapped_aliases() {
    let runtime = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(runtime.path().join("bin")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("missing"),
        runtime.path().join("dangling"),
    )
    .unwrap();
    std::os::unix::fs::symlink(runtime.path().join("bin"), runtime.path().join("absolute"))
        .unwrap();
    std::os::unix::fs::symlink("absolute", runtime.path().join("chain")).unwrap();
    std::os::unix::fs::symlink("absent", runtime.path().join("relative-dangling")).unwrap();
    std::os::unix::fs::symlink("cycle", runtime.path().join("cycle")).unwrap();
    let substrate = [ReifiedExecutionSubstrateMount::new(
        "/opt/runtime",
        runtime.path(),
    )];
    for name in [
        "dangling",
        "dangling/child",
        "absolute",
        "chain",
        "relative-dangling",
        "relative-dangling/child",
        "cycle",
    ] {
        for suffix in ["", "/"] {
            assert!(
                validate_linux_command_path(
                    Some(format!("/opt/runtime/{name}{suffix}").into()),
                    &substrate
                )
                .is_err(),
                "{name}{suffix}"
            );
        }
    }
    assert!(validate_linux_command_path(Some("/opt/runtime/absent".into()), &substrate).is_ok());
}

#[cfg(unix)]
#[test]
fn command_path_rejects_alias_escape_and_file_entries() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
    std::fs::write(temp.path().join("file"), "not a directory").unwrap();
    let substrate = [ReifiedExecutionSubstrateMount::new(
        "/opt/runtime",
        temp.path(),
    )];
    assert!(
        validate_linux_command_path(Some("/opt/runtime/escape".into()), &substrate)
            .unwrap_err()
            .contains("absolute PATH alias")
    );
    assert!(validate_linux_command_path(Some("/opt/runtime/file".into()), &substrate).is_err());
}

#[cfg(unix)]
#[test]
fn command_path_checks_aliases_in_identical_host_and_namespace_roots() {
    let runtime = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = canonicalize_existing_host_path(runtime.path());
    std::fs::create_dir(root.join("bin")).unwrap();
    std::os::unix::fs::symlink(root.join("bin"), root.join("absolute")).unwrap();
    std::os::unix::fs::symlink("missing", runtime.path().join("dangling")).unwrap();
    std::os::unix::fs::symlink(outside.path(), runtime.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(root.join("bin"), outside.path().join("bridge")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("bridge"), root.join("external-chain")).unwrap();
    std::os::unix::fs::symlink("../outside", root.join("parent-alias")).unwrap();
    let substrate = [ReifiedExecutionSubstrateMount::new(&root, &root)];
    let path = root.join("absolute").into_os_string();
    assert!(validate_linux_command_path(Some(path), &substrate).is_ok());
    for name in [
        "dangling",
        "dangling/child",
        "escape",
        "external-chain",
        "parent-alias",
    ] {
        let path = root.join(name).into_os_string();
        assert!(
            validate_linux_command_path(Some(path), &substrate).is_err(),
            "{name}"
        );
    }
}
