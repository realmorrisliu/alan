use super::*;

#[cfg(unix)]
fn running_as_root() -> bool {
    // SAFETY: geteuid takes no pointers and has no caller-side preconditions.
    unsafe { libc::geteuid() == 0 }
}

#[tokio::test]
async fn reads_host_file() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("notes")).unwrap();
    std::fs::write(temp.path().join("notes/today.txt"), "hello").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    let qid = fs
        .walk(
            Fid::ROOT,
            Fid(1),
            &["notes".to_string(), "today.txt".to_string()],
        )
        .await
        .unwrap();
    assert_eq!(qid.kind, FileKind::File);
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let bytes = fs.read(Fid(1), 0, 1024).await.unwrap();
    assert_eq!(bytes, b"hello");
}

#[tokio::test]
async fn ranged_reads_do_not_require_full_file_buffering() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("large.txt"), b"0123456789abcdef").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["large.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let bytes = fs.read(Fid(1), 10, 3).await.unwrap();
    assert_eq!(bytes, b"abc");
}

#[cfg(unix)]
#[tokio::test]
async fn walk_rejects_fifo_without_blocking() {
    use std::os::unix::ffi::OsStrExt;

    let temp = tempfile::tempdir().unwrap();
    let fifo = temp.path().join("pipe");
    let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: fifo_path is a live NUL-terminated C string and the mode contains valid bits.
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    let walked = tokio::time::timeout(
        Duration::from_millis(100),
        fs.walk(Fid::ROOT, Fid(1), &["pipe".to_string()]),
    )
    .await
    .unwrap();
    assert_eq!(walked.unwrap_err(), ErrorCode::Unsupported);
}

#[tokio::test]
async fn lists_directory_in_stable_order() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("b.txt"), "").unwrap();
    std::fs::write(temp.path().join("a.txt"), "").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    let listing = fs.read(Fid::ROOT, 0, 1024).await.unwrap();
    assert_eq!(String::from_utf8(listing).unwrap(), "a.txt\nb.txt");

    let repeated = fs.read(Fid::ROOT, 0, 1024).await.unwrap();
    assert_eq!(String::from_utf8(repeated).unwrap(), "a.txt\nb.txt");
}

#[tokio::test]
async fn writes_creates_and_removes_files() {
    let temp = tempfile::tempdir().unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.create(Fid::ROOT, Fid(1), "draft.txt", FileKind::File)
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"draft").await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("draft.txt")).unwrap(),
        b"draft"
    );

    fs.walk(Fid::ROOT, Fid(2), &["draft.txt".to_string()])
        .await
        .unwrap();
    fs.remove(Fid(2)).await.unwrap();
    assert!(!temp.path().join("draft.txt").exists());
}

#[tokio::test]
async fn rejects_sparse_writes_beyond_buffer_limit() {
    let temp = tempfile::tempdir().unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.create(Fid::ROOT, Fid(1), "draft.txt", FileKind::File)
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    let offset = Offset::try_from(MAX_BUFFERED_FILE_BYTES + 1).unwrap();
    let err = fs.write(Fid(1), offset, b"x").await.unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);
}

#[tokio::test]
async fn readwrite_rejects_seed_files_beyond_buffer_limit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.txt");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(MAX_BUFFERED_FILE_BYTES as u64 + 1)
        .unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["large.txt".to_string()])
        .await
        .unwrap();
    let err = fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);
}

#[cfg(unix)]
#[tokio::test]
async fn write_intent_open_rejects_host_file_without_write_permission() {
    use std::os::unix::fs::PermissionsExt;

    if running_as_root() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("readonly.txt");
    std::fs::write(&path, "readonly").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["readonly.txt".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
        ErrorCode::NoAccess
    );

    fs.walk(Fid::ROOT, Fid(2), &["readonly.txt".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap_err(),
        ErrorCode::NoAccess
    );
}

#[cfg(unix)]
#[tokio::test]
async fn write_open_allows_host_file_without_read_permission() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("writeonly.txt");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["writeonly.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"new").await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
}

#[cfg(unix)]
#[tokio::test]
async fn stat_allows_host_file_without_read_permission() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("writeonly.txt");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["writeonly.txt".to_string()])
        .await
        .unwrap();
    let stat = fs.stat(Fid(1)).await.unwrap();

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(stat.name, "writeonly.txt");
    assert_eq!(stat.qid.kind, FileKind::File);
    assert_eq!(stat.length, 3);
    assert!(stat.writable);
}

#[cfg(unix)]
#[tokio::test]
async fn write_intent_open_rejects_multiply_linked_host_file() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = temp.path().join("linked.txt");
    let outside_alias = outside.path().join("alias.txt");
    std::fs::write(&path, "linked").unwrap();
    std::fs::hard_link(&path, &outside_alias).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["linked.txt".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
        ErrorCode::NoAccess
    );

    fs.walk(Fid::ROOT, Fid(2), &["linked.txt".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap_err(),
        ErrorCode::NoAccess
    );
    assert_eq!(std::fs::read_to_string(outside_alias).unwrap(), "linked");
}

#[cfg(unix)]
#[tokio::test]
async fn clunk_rejects_multiply_linked_host_file_created_after_open() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    let outside_alias = outside.path().join("alias.txt");
    std::fs::write(&path, "safe").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"changed").await.unwrap();
    std::fs::hard_link(&path, &outside_alias).unwrap();

    assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "safe");
    assert_eq!(std::fs::read_to_string(outside_alias).unwrap(), "safe");
}

#[tokio::test]
async fn clunk_rejects_host_file_replaced_after_open() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("target.txt");
    std::fs::write(&path, "safe").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"changed").await.unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, "external").unwrap();

    assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
}

#[cfg(unix)]
#[tokio::test]
async fn failed_clunk_staging_preserves_original_host_file() {
    use std::os::unix::fs::PermissionsExt;

    if running_as_root() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let mounted = temp.path().join("mounted");
    std::fs::create_dir(&mounted).unwrap();
    let path = mounted.join("target.txt");
    std::fs::write(&path, "safe").unwrap();
    let fs = HostDirFs::new(&mounted, HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["target.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"changed").await.unwrap();
    std::fs::set_permissions(&mounted, std::fs::Permissions::from_mode(0o555)).unwrap();

    assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
    std::fs::set_permissions(&mounted, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "safe");
}

#[test]
fn qid_versions_include_full_modified_timestamp() {
    let first = qid_version_from_duration(Duration::new(1, 42));
    let same_nanos_later_second = qid_version_from_duration(Duration::new(2, 42));
    let same_second_later_nanos = qid_version_from_duration(Duration::new(1, 43));

    assert_ne!(first, same_nanos_later_second);
    assert_ne!(first, same_second_later_nanos);
}

#[tokio::test]
async fn read_only_rejects_mutation() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("a.txt"), "a").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["a.txt".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await.unwrap_err(),
        ErrorCode::NoAccess
    );
    assert_eq!(
        fs.create(Fid::ROOT, Fid(2), "b.txt", FileKind::File)
            .await
            .unwrap_err(),
        ErrorCode::NoAccess
    );
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    symlink(
        outside.path().join("secret.txt"),
        temp.path().join("secret-link"),
    )
    .unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["secret-link".to_string()])
            .await
            .unwrap_err(),
        ErrorCode::NoAccess
    );
}

#[cfg(unix)]
#[tokio::test]
async fn read_rejects_symlink_replacement_after_open() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("notes.txt"), "safe").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["notes.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    std::fs::remove_file(temp.path().join("notes.txt")).unwrap();
    symlink(
        outside.path().join("secret.txt"),
        temp.path().join("notes.txt"),
    )
    .unwrap();

    assert_eq!(
        fs.read(Fid(1), 0, 1024).await.unwrap_err(),
        ErrorCode::NoAccess
    );
}

#[cfg(unix)]
#[tokio::test]
async fn clunk_rejects_symlink_replacement_before_commit() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_secret = outside.path().join("secret.txt");
    std::fs::write(temp.path().join("draft.txt"), "safe").unwrap();
    std::fs::write(&outside_secret, "secret").unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["draft.txt".to_string()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"changed").await.unwrap();
    std::fs::remove_file(temp.path().join("draft.txt")).unwrap();
    symlink(&outside_secret, temp.path().join("draft.txt")).unwrap();

    assert_eq!(fs.clunk(Fid(1)).await.unwrap_err(), ErrorCode::NoAccess);
    assert_eq!(std::fs::read_to_string(outside_secret).unwrap(), "secret");
}

#[cfg(unix)]
#[tokio::test]
async fn create_rejects_symlink_parent_replacement() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("dir")).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["dir".to_string()])
        .await
        .unwrap();
    std::fs::remove_dir(temp.path().join("dir")).unwrap();
    symlink(outside.path(), temp.path().join("dir")).unwrap();

    assert_eq!(
        fs.create(Fid(1), Fid(2), "escaped.txt", FileKind::File)
            .await
            .unwrap_err(),
        ErrorCode::NoAccess
    );
    assert!(!outside.path().join("escaped.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn remove_unlinks_symlink_entry_without_removing_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.txt");
    let alias = temp.path().join("alias.txt");
    std::fs::write(&target, "target").unwrap();
    symlink("target.txt", &alias).unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadWrite).unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["alias.txt".to_string()])
        .await
        .unwrap();
    fs.remove(Fid(1)).await.unwrap();

    assert!(!alias.exists());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "target");
}

#[tokio::test]
async fn rejects_parent_traversal_components() {
    let temp = tempfile::tempdir().unwrap();
    let fs = HostDirFs::new(temp.path(), HostDirAccess::ReadOnly).unwrap();

    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["..".to_string()])
            .await
            .unwrap_err(),
        ErrorCode::BadRequest
    );
}
