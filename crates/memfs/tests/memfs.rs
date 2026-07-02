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
