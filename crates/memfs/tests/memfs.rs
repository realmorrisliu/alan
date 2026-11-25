use alan_ap::{ErrorCode, Fid, FileKind, FileServer, OpenMode};
use alan_memfs::MemFs;

async fn create_file(fs: &MemFs, name: &str, fid: Fid, bytes: &[u8]) {
    fs.create(Fid::ROOT, fid, name, FileKind::File)
        .await
        .expect("create");
    fs.open(fid, OpenMode::Write).await.expect("open");
    fs.write(fid, 0, bytes).await.expect("write");
    fs.clunk(fid).await.expect("clunk");
}

async fn read_file(fs: &MemFs, name: &str, fid: Fid) -> Vec<u8> {
    fs.walk(Fid::ROOT, fid, &[name.to_string()])
        .await
        .expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    fs.read(fid, 0, 4096).await.expect("read")
}

#[tokio::test]
async fn memory_files_are_plain_file_views_over_checkpoint_roots() {
    let fs = MemFs::new();
    create_file(&fs, "facts", Fid(1), b"alpha\nbeta\n").await;

    assert_eq!(read_file(&fs, "facts", Fid(2)).await, b"alpha\nbeta\n");
    assert_eq!(fs.materialize("facts").await.unwrap(), b"alpha\nbeta\n");

    let root = fs.checkpoint_root("facts").await.unwrap();
    assert!(root.as_str().starts_with("sha256:"));
}

#[tokio::test]
async fn identical_memory_content_gets_the_same_checkpoint_root() {
    let fs = MemFs::new();
    create_file(&fs, "a", Fid(1), b"shared").await;
    create_file(&fs, "b", Fid(2), b"shared").await;

    assert_eq!(
        fs.checkpoint_root("a").await.unwrap(),
        fs.checkpoint_root("b").await.unwrap()
    );
}

#[tokio::test]
async fn removing_a_memory_file_removes_its_namespace_authority() {
    let fs = MemFs::new();
    create_file(&fs, "scratch", Fid(1), b"temporary").await;

    fs.walk(Fid::ROOT, Fid(2), &["scratch".into()])
        .await
        .unwrap();
    fs.remove(Fid(2)).await.unwrap();

    assert_eq!(
        fs.walk(Fid::ROOT, Fid(3), &["scratch".into()]).await,
        Err(ErrorCode::NotFound)
    );
    assert_eq!(fs.materialize("scratch").await, Err(ErrorCode::NotFound));
}

#[tokio::test]
async fn stale_writer_clunk_after_remove_does_not_resurrect_file() {
    let fs = MemFs::new();
    create_file(&fs, "scratch", Fid(1), b"temporary").await;

    fs.walk(Fid::ROOT, Fid(2), &["scratch".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"stale").await.unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["scratch".into()])
        .await
        .unwrap();
    fs.remove(Fid(3)).await.unwrap();

    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::NotFound));
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(4), &["scratch".into()]).await,
        Err(ErrorCode::NotFound)
    );
    assert_eq!(fs.materialize("scratch").await, Err(ErrorCode::NotFound));
}

#[tokio::test]
async fn stale_writer_clunk_after_recreate_does_not_overwrite_new_file() {
    let fs = MemFs::new();
    create_file(&fs, "scratch", Fid(1), b"temporary").await;

    fs.walk(Fid::ROOT, Fid(2), &["scratch".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"stale").await.unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["scratch".into()])
        .await
        .unwrap();
    fs.remove(Fid(3)).await.unwrap();
    create_file(&fs, "scratch", Fid(4), b"fresh").await;

    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::NotFound));
    assert_eq!(read_file(&fs, "scratch", Fid(5)).await, b"fresh");
}

#[tokio::test]
async fn read_write_range_writes_preserve_unwritten_bytes() {
    let fs = MemFs::new();
    create_file(&fs, "facts", Fid(1), b"abcdef").await;

    fs.walk(Fid::ROOT, Fid(2), &["facts".into()]).await.unwrap();
    fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap();
    fs.write(Fid(2), 2, b"XY").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    assert_eq!(read_file(&fs, "facts", Fid(3)).await, b"abXYef");
}

#[tokio::test]
async fn durable_home_can_resume_a_file_from_its_root_hash() {
    let fs = MemFs::new();
    create_file(&fs, "continuity", Fid(1), b"remember this").await;
    let root = fs.checkpoint_root("continuity").await.unwrap();

    fs.walk(Fid::ROOT, Fid(2), &["continuity".into()])
        .await
        .unwrap();
    fs.remove(Fid(2)).await.unwrap();
    assert_eq!(fs.materialize("continuity").await, Err(ErrorCode::NotFound));

    fs.restore_checkpoint("continuity", root).await.unwrap();
    assert_eq!(read_file(&fs, "continuity", Fid(3)).await, b"remember this");
}

#[tokio::test]
async fn stale_writer_clunk_after_restore_does_not_overwrite_checkpoint() {
    let fs = MemFs::new();
    create_file(&fs, "continuity", Fid(1), b"old").await;
    create_file(&fs, "snapshot", Fid(2), b"restored").await;
    let restored_root = fs.checkpoint_root("snapshot").await.unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["continuity".into()])
        .await
        .unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, b"stale").await.unwrap();

    fs.restore_checkpoint("continuity", restored_root)
        .await
        .unwrap();

    assert_eq!(fs.clunk(Fid(3)).await, Err(ErrorCode::NotFound));
    assert_eq!(read_file(&fs, "continuity", Fid(4)).await, b"restored");
}

#[tokio::test]
async fn ephemeral_home_does_not_resume_roots_it_did_not_persist() {
    let durable = MemFs::new();
    create_file(&durable, "continuity", Fid(1), b"remember this").await;
    let root = durable.checkpoint_root("continuity").await.unwrap();

    let ephemeral = MemFs::new();
    assert_eq!(
        ephemeral.restore_checkpoint("continuity", root).await,
        Err(ErrorCode::NotFound)
    );
}

#[tokio::test]
async fn memfs_recall_and_search_surfaces_exercise_bm25_cjk_and_are_read_only() {
    let fs = MemFs::new();
    create_file(
        &fs,
        "prefs.md",
        Fid(1),
        "用户偏好：回答尽量使用中文，工具栈使用 Rust。".as_bytes(),
    )
    .await;
    create_file(
        &fs,
        "cargo.md",
        Fid(2),
        "排查 Rust 项目里的 Cargo 报错需要检查 Cargo.lock。".as_bytes(),
    )
    .await;

    let hits = fs.search("用户偏好用哪种语言？", 5).await.unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].id, "prefs.md");

    // Walk read-only virtual `recall/<query>` and `search/<query>` surfaces.
    fs.walk(
        Fid::ROOT,
        Fid(10),
        &["recall".into(), "Rust 项目里的 Cargo 报错".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(10), OpenMode::Read).await.unwrap();
    let body = String::from_utf8(fs.read(Fid(10), 0, 8192).await.unwrap()).unwrap();
    fs.clunk(Fid(10)).await.unwrap();
    assert!(body.contains("cargo.md"), "{body}");

    fs.walk(Fid::ROOT, Fid(11), &["search".into(), "偏好".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(11), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    fs.open(Fid(11), OpenMode::Read).await.unwrap();
    let search_body = String::from_utf8(fs.read(Fid(11), 0, 8192).await.unwrap()).unwrap();
    fs.clunk(Fid(11)).await.unwrap();
    assert!(search_body.contains("prefs.md"), "{search_body}");

    // Reserved virtual names cannot be overwritten by file creation.
    assert_eq!(
        fs.create(Fid::ROOT, Fid(12), "recall", FileKind::File)
            .await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn memfs_checkpoint_history_supports_versioned_revert() {
    let fs = MemFs::new();
    create_file(&fs, "notes.md", Fid(1), b"version one").await;

    fs.walk(Fid::ROOT, Fid(2), &["notes.md".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"version two").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    let history = fs.checkpoint_history("notes.md").await.unwrap();
    assert!(history.len() >= 2, "{history:?}");
    assert_eq!(fs.materialize("notes.md").await.unwrap(), b"version two");

    let reverted_root = fs.revert_previous("notes.md").await.unwrap();
    assert_eq!(fs.checkpoint_root("notes.md").await.unwrap(), reverted_root);
    assert_eq!(fs.materialize("notes.md").await.unwrap(), b"version one");

    // Can also jump explicitly back to v2 by checkpoint root hash.
    let v2_root = history.last().unwrap().root.clone();
    fs.revert_to_checkpoint("notes.md", &v2_root).await.unwrap();
    assert_eq!(fs.materialize("notes.md").await.unwrap(), b"version two");
}

#[tokio::test]
async fn failed_multi_chunk_write_never_commits_partial_bytes_on_clunk() {
    let fs = MemFs::new();
    create_file(&fs, "notes.md", Fid(1), b"original content").await;

    fs.walk(Fid::ROOT, Fid(2), &["notes.md".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"partial prefix").await.unwrap();
    assert_eq!(
        fs.write(Fid(2), 1 << 20, b"overflow").await,
        Err(ErrorCode::BadRequest)
    );
    fs.clunk(Fid(2)).await.unwrap();

    assert_eq!(
        fs.materialize("notes.md").await.unwrap(),
        b"original content"
    );
}

#[tokio::test]
async fn read_only_memfs_allows_recall_and_denies_all_mutations() {
    use std::sync::Arc;

    let fs = Arc::new(MemFs::new());
    fs.commit_file(
        "decisions.md",
        "使用 Rust memfs 单写者治理来防止并发覆盖。".as_bytes(),
    )
    .await
    .unwrap();

    let ro = fs.read_only();
    assert_eq!(ro.list_files_via_root().await, b"decisions.md");
    ro.walk(Fid::ROOT, Fid(1), &["decisions.md".into()])
        .await
        .unwrap();
    let stat = ro.stat(Fid(1)).await.unwrap();
    assert!(!stat.writable);
    assert_eq!(
        ro.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        ro.open(Fid(1), OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess)
    );
    ro.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(
        ro.write(Fid(1), 0, b"tamper").await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(ro.remove(Fid(1)).await, Err(ErrorCode::NoAccess));
    ro.clunk(Fid(1)).await.unwrap();

    assert_eq!(
        ro.create(Fid::ROOT, Fid(2), "rogue.md", FileKind::File)
            .await,
        Err(ErrorCode::NoAccess)
    );

    // Child can still query recall/<query> over the read-only view.
    ro.walk(Fid::ROOT, Fid(3), &["recall".into(), "单写者治理".into()])
        .await
        .unwrap();
    ro.open(Fid(3), OpenMode::Read).await.unwrap();
    let hits = String::from_utf8(ro.read(Fid(3), 0, 8192).await.unwrap()).unwrap();
    ro.clunk(Fid(3)).await.unwrap();
    assert!(hits.contains("decisions.md"), "{hits}");
}

#[tokio::test]
async fn commit_files_atomic_and_revert_to_revision_are_all_or_nothing() {
    let fs = MemFs::new();
    let rev1 = fs.commit_file("summary.md", b"rev1").await.unwrap();

    // Invalid batch (contains reserved name "recall") leaves existing files untouched.
    assert_eq!(
        fs.commit_files_atomic(vec![
            ("summary.md".to_string(), b"should-not-apply".to_vec()),
            ("recall".to_string(), b"invalid".to_vec()),
        ])
        .await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(fs.materialize("summary.md").await.unwrap(), b"rev1");

    let committed = fs
        .commit_files_atomic(vec![
            ("summary.md".to_string(), b"rev2".to_vec()),
            ("proposals.json".to_string(), b"[]".to_vec()),
        ])
        .await
        .unwrap();
    assert_eq!(committed.len(), 2);
    assert_eq!(fs.list_files().await, vec!["proposals.json", "summary.md"]);
    assert_eq!(fs.materialize("summary.md").await.unwrap(), b"rev2");

    fs.revert_to_revision("summary.md", rev1.revision)
        .await
        .unwrap();
    assert_eq!(fs.materialize("summary.md").await.unwrap(), b"rev1");

    fs.delete_file("proposals.json").await.unwrap();
    assert_eq!(fs.list_files().await, vec!["summary.md"]);
}

#[async_trait::async_trait]
trait ReadOnlyRootHelper {
    async fn list_files_via_root(&self) -> Vec<u8>;
}

#[async_trait::async_trait]
impl ReadOnlyRootHelper for alan_memfs::ReadOnlyMemFs {
    async fn list_files_via_root(&self) -> Vec<u8> {
        self.read(Fid::ROOT, 0, 4096).await.unwrap()
    }
}
