use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_editfs::{EditFs, ExecutionPolicy};

async fn read_text(fs: &EditFs, path: &[&str], fid: Fid) -> String {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    let bytes = fs.read(fid, 0, 4096).await.expect("read");
    fs.clunk(fid).await.expect("clunk");
    String::from_utf8(bytes).expect("utf8")
}

async fn write_doc(fs: &EditFs, path: &[&str], fid: Fid, bytes: &[u8]) -> Result<(), ErrorCode> {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Write).await?;
    fs.write(fid, 0, bytes).await?;
    fs.clunk(fid).await
}

async fn event_len(fs: &EditFs, fid: Fid) -> u64 {
    fs.walk(Fid::ROOT, fid, &["event".into()])
        .await
        .expect("walk event");
    let stat = fs.stat(fid).await.expect("stat event");
    fs.clunk(fid).await.expect("clunk event");
    stat.length
}

#[tokio::test]
async fn root_lists_editable_buffer_files() {
    let fs = EditFs::new();
    let root = read_text(&fs, &[], Fid(1)).await;
    for entry in ["body", "tag", "addr", "ctl", "event"] {
        assert!(root.lines().any(|line| line == entry), "{root}");
    }
}

#[tokio::test]
async fn body_and_tag_commit_on_clunk_and_emit_events() {
    let fs = EditFs::new();

    write_doc(&fs, &["body"], Fid(1), b"run tests")
        .await
        .unwrap();
    write_doc(&fs, &["tag"], Fid(2), b"Exec Save")
        .await
        .unwrap();

    assert_eq!(read_text(&fs, &["body"], Fid(3)).await, "run tests");
    assert_eq!(read_text(&fs, &["tag"], Fid(4)).await, "Exec Save");

    let events = read_text(&fs, &["event"], Fid(5)).await;
    assert!(events.contains(r#""type":"edit""#), "{events}");
    assert!(events.contains(r#""file":"body""#), "{events}");
    assert!(events.contains(r#""file":"tag""#), "{events}");
}

#[tokio::test]
async fn readwrite_edits_preserve_existing_body_and_tag_bytes() {
    let fs = EditFs::new();
    write_doc(&fs, &["body"], Fid(1), b"abcdef").await.unwrap();
    write_doc(&fs, &["tag"], Fid(2), b"status-line")
        .await
        .unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["body".into()]).await.unwrap();
    fs.open(Fid(3), OpenMode::ReadWrite).await.unwrap();
    fs.write(Fid(3), 2, b"XY").await.unwrap();
    fs.clunk(Fid(3)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(4), &["tag".into()]).await.unwrap();
    fs.open(Fid(4), OpenMode::ReadWrite).await.unwrap();
    fs.write(Fid(4), 7, b"row").await.unwrap();
    fs.clunk(Fid(4)).await.unwrap();

    assert_eq!(read_text(&fs, &["body"], Fid(5)).await, "abXYef");
    assert_eq!(read_text(&fs, &["tag"], Fid(6)).await, "status-rowe");
}

#[tokio::test]
async fn invalid_utf8_is_rejected_without_changing_visible_content() {
    let fs = EditFs::new();
    write_doc(&fs, &["body"], Fid(1), b"stable").await.unwrap();

    let err = write_doc(&fs, &["body"], Fid(2), &[0xff, 0xfe])
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);
    assert_eq!(read_text(&fs, &["body"], Fid(3)).await, "stable");
}

#[tokio::test]
async fn addr_selects_a_revision_bound_body_range() {
    let fs = EditFs::new();
    write_doc(&fs, &["body"], Fid(1), b"alpha beta")
        .await
        .unwrap();

    write_doc(&fs, &["addr"], Fid(2), b"rev:1 6..10")
        .await
        .unwrap();

    assert_eq!(
        read_text(&fs, &["addr"], Fid(3)).await,
        "rev:1 addr:1 6..10"
    );
    let events = read_text(&fs, &["event"], Fid(4)).await;
    assert!(events.contains(r#""type":"address""#), "{events}");
    assert!(events.contains(r#""start":6"#), "{events}");
}

#[tokio::test]
async fn addr_write_rejects_non_current_body_revision() {
    let fs = EditFs::new();
    write_doc(&fs, &["body"], Fid(1), b"alpha").await.unwrap();

    let future = write_doc(&fs, &["addr"], Fid(2), b"rev:2 0..5")
        .await
        .unwrap_err();
    assert_eq!(future, ErrorCode::BadRequest);
    assert_eq!(read_text(&fs, &["addr"], Fid(3)).await, "rev:0 addr:0 0..0");

    write_doc(&fs, &["body"], Fid(4), b"alpha beta")
        .await
        .unwrap();
    let stale = write_doc(&fs, &["addr"], Fid(5), b"rev:1 0..5")
        .await
        .unwrap_err();
    assert_eq!(stale, ErrorCode::BadRequest);
    assert_eq!(read_text(&fs, &["addr"], Fid(6)).await, "rev:0 addr:0 0..0");
}

#[tokio::test]
async fn stale_addr_is_rejected_when_exec_consumes_it() {
    let fs = EditFs::with_execution_policy(ExecutionPolicy::AcceptAll);
    write_doc(&fs, &["body"], Fid(1), b"first").await.unwrap();
    write_doc(&fs, &["addr"], Fid(2), b"rev:1 0..5")
        .await
        .unwrap();
    write_doc(&fs, &["body"], Fid(3), b"second").await.unwrap();

    let err = write_doc(&fs, &["ctl"], Fid(4), b"exec rev:1 addr:1 0..5")
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);

    let events = read_text(&fs, &["event"], Fid(5)).await;
    assert!(!events.contains(r#""type":"exec""#), "{events}");
}

#[tokio::test]
async fn exec_rejects_stale_address_revision_after_another_selection() {
    let fs = EditFs::with_execution_policy(ExecutionPolicy::AcceptAll);
    write_doc(&fs, &["body"], Fid(1), b"alpha beta")
        .await
        .unwrap();
    write_doc(&fs, &["addr"], Fid(2), b"rev:1 0..5")
        .await
        .unwrap();
    let first_snapshot = read_text(&fs, &["addr"], Fid(3)).await;
    assert_eq!(first_snapshot, "rev:1 addr:1 0..5");

    write_doc(&fs, &["addr"], Fid(4), b"rev:1 6..10")
        .await
        .unwrap();
    let err = write_doc(&fs, &["ctl"], Fid(5), b"exec rev:1 addr:1 0..5")
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);

    let events = read_text(&fs, &["event"], Fid(6)).await;
    assert!(!events.contains(r#""type":"exec""#), "{events}");
}

#[tokio::test]
async fn exec_records_accepted_and_denied_policy_outcomes() {
    let accepted = EditFs::with_execution_policy(ExecutionPolicy::AcceptAll);
    write_doc(&accepted, &["body"], Fid(1), b"cargo test")
        .await
        .unwrap();
    write_doc(&accepted, &["addr"], Fid(2), b"rev:1 0..10")
        .await
        .unwrap();
    write_doc(&accepted, &["ctl"], Fid(3), b"exec rev:1 addr:1 0..10")
        .await
        .unwrap();
    let accepted_events = read_text(&accepted, &["event"], Fid(4)).await;
    assert!(
        accepted_events.contains(r#""type":"exec""#),
        "{accepted_events}"
    );
    assert!(
        accepted_events.contains(r#""status":"accepted""#),
        "{accepted_events}"
    );
    assert!(
        accepted_events.contains(r#""command":"cargo test""#),
        "{accepted_events}"
    );

    let denied = EditFs::new();
    write_doc(&denied, &["body"], Fid(11), b"rm -rf /")
        .await
        .unwrap();
    write_doc(&denied, &["addr"], Fid(12), b"rev:1 0..8")
        .await
        .unwrap();
    let err = write_doc(&denied, &["ctl"], Fid(13), b"exec rev:1 addr:1 0..8")
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::NoAccess);
    let denied_events = read_text(&denied, &["event"], Fid(14)).await;
    assert!(
        denied_events.contains(r#""type":"exec""#),
        "{denied_events}"
    );
    assert!(
        denied_events.contains(r#""status":"denied""#),
        "{denied_events}"
    );
}

#[tokio::test]
async fn event_read_blocks_until_activity_is_appended() {
    let fs = Arc::new(EditFs::new());
    let start = event_len(&fs, Fid(1)).await;

    fs.walk(Fid::ROOT, Fid(2), &["event".into()])
        .await
        .expect("walk event");
    fs.open(Fid(2), OpenMode::Read).await.expect("open event");

    let reader = {
        let fs = fs.clone();
        tokio::spawn(async move { fs.read(Fid(2), start, 4096).await })
    };
    assert!(
        tokio::time::timeout(Duration::from_millis(30), reader)
            .await
            .is_err(),
        "event read should block at the live edge"
    );

    write_doc(&fs, &["tag"], Fid(3), b"status").await.unwrap();
    let event = read_text(&fs, &["event"], Fid(4)).await;
    assert!(event.contains(r#""file":"tag""#), "{event}");
}
