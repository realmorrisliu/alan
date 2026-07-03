use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, MountFs, Namespace, SrvFs};
use alan_routefs::{DEAD_LETTER_PORT, MOUNT_PATH, RouteFs, RuleSpec, SRV_HANDLE};
use alan_shell::Shell;
use serde_json::json;

async fn read_text(fs: &RouteFs, path: &[&str], fid: Fid) -> String {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await.expect("walk");
    fs.open(fid, OpenMode::Read).await.expect("open");
    let bytes = fs.read(fid, 0, 4096).await.expect("read");
    fs.clunk(fid).await.expect("clunk");
    String::from_utf8(bytes).expect("utf8")
}

async fn write_doc(
    fs: &RouteFs,
    path: &[&str],
    fid: Fid,
    chunks: &[&[u8]],
) -> Result<(), ErrorCode> {
    let names = path.iter().map(|name| name.to_string()).collect::<Vec<_>>();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Write).await?;
    let mut offset = 0;
    for chunk in chunks {
        fs.write(fid, offset, chunk).await?;
        offset += chunk.len() as u64;
    }
    fs.clunk(fid).await
}

async fn create_rule(fs: &RouteFs, name: &str, fid: Fid, rule: serde_json::Value) {
    fs.walk(Fid::ROOT, fid, &["rules".into()])
        .await
        .expect("walk rules");
    fs.create(fid, Fid(fid.0 + 100), name, FileKind::File)
        .await
        .expect("create rule");
    let bytes = serde_json::to_vec(&rule).expect("serialize rule");
    fs.open(Fid(fid.0 + 100), OpenMode::Write)
        .await
        .expect("open rule");
    fs.write(Fid(fid.0 + 100), 0, &bytes)
        .await
        .expect("write rule");
    fs.clunk(Fid(fid.0 + 100)).await.expect("commit rule");
    fs.clunk(fid).await.expect("clunk rules dir");
}

fn message(message_type: &str, content: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "version": 1,
        "type": message_type,
        "content": content,
    }))
    .expect("serialize message")
}

#[tokio::test]
async fn typed_message_routes_by_rule_on_send_clunk() {
    let fs = RouteFs::new();
    create_rule(
        &fs,
        "10-patches",
        Fid(1),
        json!({"version":1,"match_type":"patch","port":"review"}),
    )
    .await;

    let message = message("patch", "diff --git");
    let split = message.split_at(12);
    write_doc(&fs, &["send"], Fid(2), &[split.0, split.1])
        .await
        .unwrap();

    let routed = read_text(&fs, &["ports", "review"], Fid(3)).await;
    assert!(routed.contains(r#""port":"review""#), "{routed}");
    assert!(routed.contains(r#""rule":"10-patches""#), "{routed}");
    assert!(routed.contains(r#""type":"patch""#), "{routed}");
}

#[tokio::test]
async fn routed_record_preserves_the_full_message_document() {
    let fs = RouteFs::new();
    fs.install_rule("10-results", RuleSpec::for_type("result", "review"))
        .await
        .unwrap();

    write_doc(
        &fs,
        &["send"],
        Fid(1),
        &[&serde_json::to_vec(&json!({
            "version": 1,
            "type": "result",
            "content": "needs_human_judgment",
            "result_id": "res-42",
            "payload": {
                "status": "blocked",
                "artifacts": ["patch.diff", "report.json"]
            },
            "metadata": {
                "producer": "root-agent",
                "confidence": 0.62
            }
        }))
        .unwrap()],
    )
    .await
    .unwrap();

    let routed = read_text(&fs, &["ports", "review"], Fid(2)).await;
    let record: serde_json::Value = serde_json::from_str(routed.trim()).unwrap();
    assert_eq!(record["message"]["result_id"], json!("res-42"));
    assert_eq!(record["message"]["payload"]["status"], json!("blocked"));
    assert_eq!(
        record["message"]["payload"]["artifacts"][1],
        json!("report.json")
    );
    assert_eq!(
        record["message"]["metadata"]["producer"],
        json!("root-agent")
    );
    assert_eq!(record["message"]["metadata"]["confidence"], json!(0.62));
}

#[tokio::test]
async fn send_does_not_route_before_clunk() {
    let fs = Arc::new(RouteFs::new());
    fs.install_rule("10-patches", RuleSpec::for_type("patch", "review"))
        .await
        .unwrap();

    fs.walk(Fid::ROOT, Fid(1), &["send".into()]).await.unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, &message("patch", "diff"))
        .await
        .unwrap();

    let reader = {
        let fs = fs.clone();
        tokio::spawn(async move { read_text(&fs, &["ports", "review"], Fid(2)).await })
    };
    assert!(
        tokio::time::timeout(Duration::from_millis(30), reader)
            .await
            .is_err(),
        "port read should block until send fid is clunked"
    );

    fs.clunk(Fid(1)).await.unwrap();
    let routed = read_text(&fs, &["ports", "review"], Fid(3)).await;
    assert!(routed.contains(r#""type":"patch""#), "{routed}");
}

#[tokio::test]
async fn rule_files_are_cat_readable_and_match_by_content() {
    let fs = RouteFs::new();
    create_rule(
        &fs,
        "00-human-review",
        Fid(1),
        json!({
            "version":1,
            "match_type":"result",
            "content_contains":"needs_human_judgment",
            "port":"human-inbox",
            "reason":"approval remains explicit through requests/<id>/response"
        }),
    )
    .await;

    let rule_text = read_text(&fs, &["rules", "00-human-review"], Fid(2)).await;
    assert!(rule_text.contains("needs_human_judgment"), "{rule_text}");
    assert!(rule_text.contains("human-inbox"), "{rule_text}");

    write_doc(
        &fs,
        &["send"],
        Fid(3),
        &[&message("result", "status=needs_human_judgment")],
    )
    .await
    .unwrap();
    let routed = read_text(&fs, &["ports", "human-inbox"], Fid(4)).await;
    assert!(routed.contains("approval remains explicit"), "{routed}");
}

#[tokio::test]
async fn no_match_routes_to_dead_letter_and_log_records_decision() {
    let fs = RouteFs::new();
    write_doc(&fs, &["send"], Fid(1), &[&message("citation", "source")])
        .await
        .unwrap();

    let dead = read_text(&fs, &["ports", DEAD_LETTER_PORT], Fid(2)).await;
    assert!(dead.contains(r#""port":"dead-letter""#), "{dead}");
    assert!(dead.contains(r#""rule":"dead-letter""#), "{dead}");

    let log = read_text(&fs, &["log"], Fid(3)).await;
    assert_eq!(log, dead);
}

#[tokio::test]
async fn deterministic_rule_order_picks_lexically_first_match() {
    let fs = RouteFs::new();
    fs.install_rule("20-generic", RuleSpec::for_type("patch", "apply-tool"))
        .await
        .unwrap();
    fs.install_rule(
        "10-review",
        RuleSpec::for_type("patch", "review-agent").with_reason("review first"),
    )
    .await
    .unwrap();

    write_doc(&fs, &["send"], Fid(1), &[&message("patch", "diff")])
        .await
        .unwrap();

    let routed = read_text(&fs, &["ports", "review-agent"], Fid(2)).await;
    assert!(routed.contains(r#""rule":"10-review""#), "{routed}");
}

#[tokio::test]
async fn routefs_posts_under_srv_and_mounts_at_canonical_path() {
    let srv = Arc::new(SrvFs::new());
    let routefs = Arc::new(RouteFs::new());
    srv.post(
        SRV_HANDLE,
        InProcessTransport::new(routefs.clone()),
        Access::ReadWrite,
    )
    .await;

    let (handle, access) = srv.lookup(SRV_HANDLE).await.expect("route handle");
    assert_eq!(access, Access::ReadWrite);

    let mut ns = Namespace::new();
    ns.mount(MOUNT_PATH, handle, access);
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    let root = String::from_utf8(shell.cat(MOUNT_PATH).await.unwrap()).unwrap();
    for entry in ["send", "rules", "ports", "log"] {
        assert!(root.lines().any(|line| line == entry), "{root}");
    }
}
