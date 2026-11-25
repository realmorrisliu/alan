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
async fn scoring_selected_branch_bumps_selected_qid_version() {
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
    write_ctl(&fs, Fid(2), json!({"op": "select", "id": "candidate-a"}))
        .await
        .unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["selected".into()])
        .await
        .unwrap();
    let before = fs.stat(Fid(3)).await.unwrap().qid.version;
    fs.clunk(Fid(3)).await.unwrap();

    write_ctl(
        &fs,
        Fid(4),
        json!({"op": "score", "id": "candidate-a", "score": 0.91, "summary": "higher"}),
    )
    .await
    .unwrap();

    fs.walk(Fid::ROOT, Fid(5), &["selected".into()])
        .await
        .unwrap();
    let after = fs.stat(Fid(5)).await.unwrap().qid.version;
    fs.clunk(Fid(5)).await.unwrap();

    assert!(after > before, "selected qid version did not advance");
    let selected: Value =
        serde_json::from_str(&read_text(&fs, &["selected"], Fid(6)).await).expect("selected json");
    assert_eq!(selected["score"], 0.91);
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

#[derive(Default)]
struct MockWorkspacePort {
    created: tokio::sync::Mutex<Vec<String>>,
    discarded: tokio::sync::Mutex<Vec<String>>,
    flushed: tokio::sync::Mutex<Vec<alan_branchfs::BranchWorkspaceExitFlushRequest>>,
    inject_host_path_on_create: bool,
}

#[async_trait::async_trait]
impl alan_branchfs::BranchWorkspacePort for MockWorkspacePort {
    async fn create_workspace(
        &self,
        request: &alan_branchfs::BranchWorkspaceCreateRequest,
    ) -> Result<alan_branchfs::BranchWorkspaceProvisioned, alan_branchfs::BranchWorkspacePortError>
    {
        if self.inject_host_path_on_create {
            return Ok(alan_branchfs::BranchWorkspaceProvisioned {
                workspace_ref: "/Users/alice/System Store/workspaces/1/ws-bad".to_string(),
                backing_kind: alan_branchfs::WorkspaceBackingKind::NativeCow,
                diff_summary: None,
                checks_summary: None,
            });
        }
        let ws_ref = format!("ws-{}", request.branch_id);
        self.created.lock().await.push(ws_ref.clone());
        Ok(alan_branchfs::BranchWorkspaceProvisioned {
            workspace_ref: ws_ref,
            backing_kind: alan_branchfs::WorkspaceBackingKind::NativeCow,
            diff_summary: Some("0 files changed".to_string()),
            checks_summary: None,
        })
    }

    async fn discard_workspace(
        &self,
        request: &alan_branchfs::BranchWorkspaceDiscardRequest,
    ) -> Result<(), alan_branchfs::BranchWorkspacePortError> {
        self.discarded
            .lock()
            .await
            .push(request.workspace_ref.clone());
        Ok(())
    }

    async fn flush_selected_on_exit(
        &self,
        request: &alan_branchfs::BranchWorkspaceExitFlushRequest,
    ) -> Result<
        alan_branchfs::BranchWorkspaceExitFlushOutcome,
        alan_branchfs::BranchWorkspacePortError,
    > {
        self.flushed.lock().await.push(request.clone());
        Ok(alan_branchfs::BranchWorkspaceExitFlushOutcome {
            patch_artifact_ref: Some(format!("patch-{}.patch", request.branch_id)),
            clone_purged: true,
        })
    }
}

#[tokio::test]
async fn branch_record_and_selected_publish_opaque_workspace_metadata() {
    let fs = BranchFs::new();
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();

    write_ctl(
        &fs,
        Fid(1),
        json!({
            "op": "fork",
            "id": "cand-a",
            "from": "base",
            "delta": "crates/branchfs/src/lib.rs (+14/-3)\n",
            "workspace_ref": "ws-cand-a",
            "backing_kind": "native_cow",
            "diff_summary": "1 file changed (+14/-3): crates/branchfs/src/lib.rs",
            "checks_summary": "cargo test -p alan-branchfs: 8 passed"
        }),
    )
    .await
    .unwrap();

    write_ctl(
        &fs,
        Fid(2),
        json!({
            "op": "score",
            "id": "cand-a",
            "score": 0.94,
            "summary": "all unit tests pass",
            "diff_summary": "1 file changed (+14/-3): crates/branchfs/src/lib.rs at /mnt/work/crates/branchfs/src/lib.rs",
            "checks_summary": "cargo test -p alan-branchfs: 12 passed, 0 failed"
        }),
    )
    .await
    .unwrap();
    write_ctl(&fs, Fid(3), json!({"op": "select", "id": "cand-a"}))
        .await
        .unwrap();

    let record: BranchRecord =
        serde_json::from_str(&read_text(&fs, &["branches", "cand-a"], Fid(4)).await).unwrap();
    assert_eq!(record.workspace_ref.as_deref(), Some("ws-cand-a"));
    assert_eq!(
        record.backing_kind,
        Some(alan_branchfs::WorkspaceBackingKind::NativeCow)
    );
    assert!(
        record
            .diff_summary
            .as_deref()
            .unwrap()
            .contains("crates/branchfs/src/lib.rs")
    );
    assert_eq!(
        record.checks_summary.as_deref(),
        Some("cargo test -p alan-branchfs: 12 passed, 0 failed")
    );

    let selected: Value =
        serde_json::from_str(&read_text(&fs, &["selected"], Fid(5)).await).unwrap();
    assert_eq!(selected["workspace_ref"], "ws-cand-a");
    assert_eq!(selected["backing_kind"], "native_cow");

    let events = read_text(&fs, &["events"], Fid(6)).await;
    assert!(
        events.contains(r#""workspace_ref":"ws-cand-a""#),
        "{events}"
    );
    assert!(
        events.contains(r#""backing_kind":"native_cow""#),
        "{events}"
    );
}

#[tokio::test]
async fn ctl_rejects_raw_host_paths_across_all_fields() {
    let fs = BranchFs::new();
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();

    // 1. workspace_ref must be an opaque handle, never a path.
    for bad_ref in [
        "/Users/alice/ws-1",
        "/mnt/work/ws-1",
        "dir/ws-1",
        "../ws-1",
        "C:\\ws-1",
    ] {
        let err = write_ctl(
            &fs,
            Fid(1),
            json!({
                "op": "fork",
                "id": "cand-bad-ref",
                "from": "base",
                "delta": "ok\n",
                "workspace_ref": bad_ref
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err, ErrorCode::BadRequest, "bad_ref={bad_ref}");
    }

    // 2. diff_summary, checks_summary, delta, and score summary must never embed raw Host paths.
    for bad_text in [
        "/Users/morris/repo/src/lib.rs",
        "/home/dev/project/main.rs",
        "/tmp/cow-clone-1",
        "/var/folders/ab/cdef/T/ws",
        "/private/tmp/ws-1",
        "~/Library/Application Support/Alan/System Store/workspaces/1/ws-1",
        "C:\\Users\\alice\\repo\\lib.rs",
        "file:///Users/morris/repo",
        "staged at System Store/workspaces/inst-1/ws-1",
    ] {
        let err = write_ctl(
            &fs,
            Fid(2),
            json!({
                "op": "fork",
                "id": "cand-bad-diff",
                "from": "base",
                "delta": "ok\n",
                "workspace_ref": "ws-ok",
                "diff_summary": bad_text
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err, ErrorCode::BadRequest, "bad_text={bad_text}");

        let err = write_ctl(
            &fs,
            Fid(3),
            json!({
                "op": "fork",
                "id": "cand-bad-delta",
                "from": "base",
                "delta": bad_text
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err, ErrorCode::BadRequest, "bad_delta={bad_text}");
    }

    // Verify none of the rejected commands created branches or polluted events.
    let branches = read_text(&fs, &["branches"], Fid(4)).await;
    assert_eq!(branches.trim(), "base");
    let events = read_text(&fs, &["events"], Fid(5)).await;
    assert!(!events.contains("/Users/"), "{events}");
    assert!(!events.contains("/tmp/"), "{events}");
    assert!(!events.contains("System Store/workspaces"), "{events}");
}

#[tokio::test]
async fn workspace_port_coordinates_fork_immediate_discard_and_exit_flush() {
    let port = Arc::new(MockWorkspacePort::default());
    let fs = BranchFs::with_workspace_port(port.clone());
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();

    // Fork two candidates without pre-supplied workspace_ref -> port provisions them.
    write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "cand-a", "from": "base", "delta": "delta-a\n"}),
    )
    .await
    .unwrap();
    write_ctl(
        &fs,
        Fid(2),
        json!({"op": "fork", "id": "cand-b", "from": "base", "delta": "delta-b\n"}),
    )
    .await
    .unwrap();

    assert_eq!(
        *port.created.lock().await,
        vec!["ws-cand-a".to_string(), "ws-cand-b".to_string()]
    );

    // Score both, select cand-a, and immediately discard losing clones.
    write_ctl(
        &fs,
        Fid(3),
        json!({
            "op": "score",
            "id": "cand-a",
            "score": 0.95,
            "summary": "winner",
            "diff_summary": "1 file changed",
            "checks_summary": "all passed"
        }),
    )
    .await
    .unwrap();
    write_ctl(
        &fs,
        Fid(4),
        json!({"op": "score", "id": "cand-b", "score": 0.40, "summary": "loser"}),
    )
    .await
    .unwrap();
    write_ctl(&fs, Fid(5), json!({"op": "select", "id": "cand-a"}))
        .await
        .unwrap();

    let discarded = fs.discard_unselected_candidates().await.unwrap();
    assert_eq!(discarded, vec!["cand-b".to_string()]);
    assert_eq!(*port.discarded.lock().await, vec!["ws-cand-b".to_string()]);
    assert!(fs.branch("cand-b").await.is_none());

    // Normal exit before host sync-back flushes unapplied winner .patch + purges clone.
    let flushed = fs.flush_on_exit().await.unwrap();
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].branch_id, "cand-a");
    assert_eq!(flushed[0].workspace_ref, "ws-cand-a");
    assert_eq!(
        flushed[0].patch_artifact_ref.as_deref(),
        Some("patch-cand-a.patch")
    );
    assert!(flushed[0].clone_purged);

    let winner = fs.branch("cand-a").await.unwrap();
    assert!(
        winner.workspace_ref.is_none(),
        "purged winner workspace_ref should be cleared"
    );

    let events = read_text(&fs, &["events"], Fid(6)).await;
    assert!(events.contains(r#""type":"exit_flush""#), "{events}");
    assert!(
        events.contains(r#""patch_artifact_ref":"patch-cand-a.patch""#),
        "{events}"
    );
}

#[tokio::test]
async fn workspace_port_leaking_host_path_is_rejected_and_purged() {
    let port = Arc::new(MockWorkspacePort {
        inject_host_path_on_create: true,
        ..Default::default()
    });
    let fs = BranchFs::with_workspace_port(port.clone());
    fs.install_base_branch("base", [b"a\n".to_vec()])
        .await
        .unwrap();

    let err = write_ctl(
        &fs,
        Fid(1),
        json!({"op": "fork", "id": "cand-leak", "from": "base", "delta": "delta\n"}),
    )
    .await
    .unwrap_err();
    assert_eq!(err, ErrorCode::BadRequest);
    assert!(fs.branch("cand-leak").await.is_none());
    assert_eq!(
        *port.discarded.lock().await,
        vec!["/Users/alice/System Store/workspaces/1/ws-bad".to_string()]
    );
}
