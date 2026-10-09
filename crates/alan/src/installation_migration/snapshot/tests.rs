use super::*;

#[test]
fn staging_preserves_bytes_modes_and_empty_directories_but_excludes_runtime_scratch() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("agent-runtime/cache")).unwrap();
    fs::create_dir_all(source.join("agent-runtime/tmp")).unwrap();
    fs::create_dir_all(source.join("memory/stores/empty")).unwrap();
    fs::write(source.join("memory/stores/data"), [0, 255, 3]).unwrap();
    fs::set_permissions(
        source.join("memory/stores/data"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    fs::set_permissions(
        source.join("memory/stores/empty"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    fs::write(source.join("agent-runtime/cache/generated"), "discard").unwrap();
    let before = Snapshot::read(&source, true).unwrap();
    let destination = temp.path().join("stage");
    before.stage(&source, &destination).unwrap();
    assert_eq!(Snapshot::read(&destination, false).unwrap(), before);
    assert!(!destination.join("agent-runtime/cache").exists());
    assert!(!destination.join("agent-runtime/tmp").exists());
    assert!(source.join("agent-runtime/cache/generated").exists());
    assert!(before.stage(&source, &destination).is_err());
}

#[test]
fn changed_source_and_unrecognized_staging_are_retained() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data"), "before").unwrap();
    let snapshot = Snapshot::read(&source, false).unwrap();
    fs::write(source.join("data"), "after").unwrap();
    let destination = temp.path().join("stage");
    assert!(snapshot.stage(&source, &destination).is_err());
    fs::write(destination.join("authored.patch"), "keep").unwrap();
    assert!(snapshot.discard_partial(&destination).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("authored.patch")).unwrap(),
        "keep"
    );
}

#[test]
fn symlinks_and_missing_roots_have_distinct_results() {
    let temp = tempfile::tempdir().unwrap();
    assert!(
        !Snapshot::read(&temp.path().join("absent"), false)
            .unwrap()
            .present()
    );
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(temp.path(), &link).unwrap();
    assert!(Snapshot::read(&link, false).is_err());
}

#[test]
fn loaded_inventory_rejects_escaping_paths_and_missing_parents() {
    let file = serde_json::json!({"directory": false, "mode": 384, "owner": 0, "group": 0, "length": 0, "digest": "a".repeat(64), "metadata_digest": "b".repeat(64)});
    let directory = serde_json::json!({"directory": true, "mode": 448, "owner": 0, "group": 0, "length": 0, "digest": "", "metadata_digest": "b".repeat(64)});
    for path in ["../escape", "/absolute", "missing/child"] {
        let mut entries = serde_json::Map::new();
        entries.insert(String::new(), directory.clone());
        entries.insert(path.into(), file.clone());
        let snapshot: Snapshot =
            serde_json::from_value(serde_json::Value::Object(entries)).unwrap();
        assert!(snapshot.validate_inventory().is_err(), "{path}");
    }
}

#[test]
fn staging_preserves_extended_permissions_and_detects_attribute_changes() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    let file = source.join("authored");
    fs::write(&file, "kept").unwrap();
    set_test_attribute(&file, "private metadata");
    #[cfg(target_os = "macos")]
    assert!(
        Command::new("chmod")
            .args(["+a", "everyone allow read"])
            .arg(&file)
            .status()
            .unwrap()
            .success()
    );
    #[cfg(target_os = "linux")]
    assert!(Command::new("python3").args(["-c",
        "import os,struct,sys; entries=[(1,6,0xffffffff),(2,4,os.getuid()+1),(4,0,0xffffffff),(16,4,0xffffffff),(32,0,0xffffffff)]; acl=struct.pack('<I',2)+b''.join(struct.pack('<HHI',*e) for e in entries); os.setxattr(sys.argv[1],'system.posix_acl_access',acl)"])
        .arg(&file).status().unwrap().success());
    let before = Snapshot::read(&source, false).unwrap();
    let destination = temp.path().join("stage");
    before.stage(&source, &destination).unwrap();
    assert_eq!(Snapshot::read(&destination, false).unwrap(), before);
    assert!(
        !serde_json::to_string(&before)
            .unwrap()
            .contains("private metadata")
    );
    set_test_attribute(&destination.join("authored"), "new metadata");
    assert_ne!(Snapshot::read(&destination, false).unwrap(), before);
}

fn set_test_attribute(path: &Path, value: &str) {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("xattr")
        .args(["-w", "user.alan-migration-test", value])
        .arg(path)
        .status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("python3").args(["-c",
        "import os,sys; os.setxattr(sys.argv[1], 'user.alan-migration-test', sys.argv[2].encode())"])
        .arg(path).arg(value).status();
    assert!(status.unwrap().success());
}
