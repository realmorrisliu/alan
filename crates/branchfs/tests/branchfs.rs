use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_branchfs::{BranchFs, BranchRecord, BranchStatus};
use serde_json::{Value, json};

async fn read_text(fs: &BranchFs, path: &[&str], fid: Fid) -> String {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    let bytes = fs.read(fid, 0, 4096).await.expect("read");
    fs.clunk(fid).await.expect("clunk");
    String::from_utf8(bytes).expect("utf8")
}

async fn write_doc(fs: &BranchFs, path: &[&str], fid: Fid, bytes: &[u8]) -> Result<(), ErrorCode> {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Write).await?;
    fs.write(fid, 0, bytes).await?;
    fs.clunk(fid).await
}

async fn write_ctl(fs: &BranchFs, fid: Fid, command: Value) -> Result<(), ErrorCode> {
    write_doc(fs, &["ctl"], fid, command.to_string().as_bytes()).await
}

async fn event_len(fs: &BranchFs, fid: Fid) -> u64 {
    fs.walk(Fid::ROOT, fid, &["events".into()])
        .await
        .expect("walk events");
    let stat = fs.stat(fid).await.expect("stat events");
    fs.clunk(fid).await.expect("clunk events");
    stat.length
}

#[tokio::test]
async fn root_lists_branch_files() {
    let fs = BranchFs::new();
    let root = read_text(&fs, &[], Fid(1)).await;
    for entry in ["ctl", "branches", "selected", "events"] {
        assert!(root.lines().any(|line| line == entry), "{root}");
    }
}

#[tokio::test]
async fn base_branch_is_visible_as_json() {
    let fs = BranchFs::new();
    let root = fs
        .install_base_branch("base", [b"record-1\n".to_vec()])
        .await
        .unwrap();

    let branches = read_text(&fs, &["branches"], Fid(1)).await;
    assert_eq!(branches.trim(), "base");

    let branch: BranchRecord =
        serde_json::from_str(&read_text(&fs, &["branches", "base"], Fid(2)).await).unwrap();
    assert_eq!(branch.id, "base");
    assert_eq!(branch.base, None);
    assert_eq!(branch.root, root);
    assert_eq!(branch.status, BranchStatus::Active);
}

#[tokio::test]
async fn fork_creates_candidate_with_shared_base_blocks_and_event() {
    let fs = BranchFs::new();
    let base = fs
        .install_base_branch("base", [b"a\n".to_vec(), b"b\n".to_vec()])
        .await
        .unwrap();
    let initial_blocks = fs.block_count().await;
    let initial_nodes = fs.node_count().await;

    write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "candidate-a", "from": "base", "delta": "c\n"}),
    )
    .await
    .unwrap();

    assert_eq!(fs.block_count().await, initial_blocks + 1);
    assert_eq!(fs.node_count().await, initial_nodes + 1);

    let candidate: BranchRecord =
        serde_json::from_str(&read_text(&fs, &["branches", "candidate-a"], Fid(2)).await).unwrap();
    assert_eq!(candidate.id, "candidate-a");
    assert_eq!(candidate.base.as_deref(), Some("base"));
    assert_ne!(candidate.root, base);

    let events = read_text(&fs, &["events"], Fid(3)).await;
    assert!(events.contains(r#""type":"fork""#), "{events}");
    assert!(events.contains(r#""id":"candidate-a""#), "{events}");
}

#[tokio::test]
async fn fork_rejects_unknown_source_branch() {
    let fs = BranchFs::new();
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();

    let err = write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "candidate-a", "from": "missing", "delta": "b\n"}),
    )
    .await
    .unwrap_err();

    assert_eq!(err, ErrorCode::NotFound);
    let branches = read_text(&fs, &["branches"], Fid(2)).await;
    assert!(!branches.lines().any(|line| line == "candidate-a"));
}

#[tokio::test]
async fn score_and_select_are_explicit_and_inspectable() {
    let fs = BranchFs::new();
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();
    write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "candidate-a", "from": "base", "delta": "b\n"}),
    )
    .await
    .unwrap();

    write_ctl(
        &fs,
        Fid(2),
        json!({"op": "score", "id": "candidate-a", "score": 0.82, "summary": "best trace"}),
    )
    .await
    .unwrap();
    write_ctl(&fs, Fid(3), json!({"op": "select", "id": "candidate-a"}))
        .await
        .unwrap();

    let branch: BranchRecord =
        serde_json::from_str(&read_text(&fs, &["branches", "candidate-a"], Fid(4)).await).unwrap();
    assert_eq!(branch.score, Some(0.82));
    assert_eq!(branch.summary.as_deref(), Some("best trace"));
    assert_eq!(branch.status, BranchStatus::Selected);

    let selected: Value =
        serde_json::from_str(&read_text(&fs, &["selected"], Fid(5)).await).expect("selected json");
    assert_eq!(selected["id"], "candidate-a");
    assert_eq!(selected["score"], 0.82);

    let events = read_text(&fs, &["events"], Fid(6)).await;
    assert!(events.contains(r#""type":"score""#), "{events}");
    assert!(events.contains(r#""type":"select""#), "{events}");
}

#[tokio::test]
async fn discard_hides_branch_but_retains_event() {
    let fs = BranchFs::new();
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();
    write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "candidate-a", "from": "base", "delta": "b\n"}),
    )
    .await
    .unwrap();

    write_ctl(&fs, Fid(2), json!({"op": "discard", "id": "candidate-a"}))
        .await
        .unwrap();

    let branches = read_text(&fs, &["branches"], Fid(3)).await;
    assert!(!branches.lines().any(|line| line == "candidate-a"));
    let err = fs
        .walk(
            Fid::ROOT,
            Fid(4),
            &["branches".into(), "candidate-a".into()],
        )
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::NotFound);

    let events = read_text(&fs, &["events"], Fid(5)).await;
    assert!(events.contains(r#""type":"discard""#), "{events}");
    assert!(events.contains(r#""id":"candidate-a""#), "{events}");
}

#[tokio::test]
async fn event_read_blocks_until_branch_activity_is_appended() {
    let fs = Arc::new(BranchFs::new());
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();
    let start = event_len(&fs, Fid(1)).await;

    fs.walk(Fid::ROOT, Fid(2), &["events".into()])
        .await
        .expect("walk events");
    fs.open(Fid(2), OpenMode::Read).await.expect("open events");

    let reader = {
        let fs = fs.clone();
        tokio::spawn(async move { fs.read(Fid(2), start, 4096).await })
    };
    assert!(
        tokio::time::timeout(Duration::from_millis(30), reader)
            .await
            .is_err(),
        "events read should block at the live edge"
    );

    write_ctl(
        &fs,
        Fid(3),
        json!({"op": "fork", "id": "candidate-a", "from": "base", "delta": "b\n"}),
    )
    .await
    .unwrap();
    let events = read_text(&fs, &["events"], Fid(4)).await;
    assert!(events.contains(r#""type":"fork""#), "{events}");
}
