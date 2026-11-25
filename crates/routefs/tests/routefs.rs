use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, MountFs, Namespace, SrvFs};
use alan_routefs::{
    DEAD_LETTER_PORT, LOOP_GUARD_RULE_NAME, MAX_HANDOFF_HOPS, MOUNT_PATH, ROOT_PORT, RouteFs,
    RouteHandoffKind, RouteHandoffMessage, RoutedRecord, RuleSpec, SRV_HANDLE,
};
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
async fn rule_create_reserves_the_name_until_clunk() {
    let fs = RouteFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["rules".into()]).await.unwrap();
    fs.create(Fid(1), Fid(2), "10-results", FileKind::File)
        .await
        .unwrap();
    assert_eq!(
        fs.create(Fid(1), Fid(3), "10-results", FileKind::File)
            .await,
        Err(ErrorCode::BadRequest),
        "a pending rule create reserves its name"
    );

    let rule = serde_json::to_vec(&json!({
        "version": 1,
        "match_type": "result",
        "port": "review"
    }))
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, &rule).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    assert_eq!(
        fs.create(Fid(1), Fid(4), "10-results", FileKind::File)
            .await,
        Err(ErrorCode::BadRequest),
        "a committed rule keeps the name reserved"
    );
    fs.clunk(Fid(1)).await.unwrap();
}

#[tokio::test]
async fn abandoned_rule_create_releases_the_reserved_name() {
    let fs = RouteFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["rules".into()]).await.unwrap();
    fs.create(Fid(1), Fid(2), "10-results", FileKind::File)
        .await
        .unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    fs.create(Fid(1), Fid(3), "10-results", FileKind::File)
        .await
        .unwrap();
    fs.clunk(Fid(3)).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
}

#[tokio::test]
async fn invalid_rule_json_releases_the_reserved_name() {
    let fs = RouteFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["rules".into()]).await.unwrap();
    fs.create(Fid(1), Fid(2), "10-results", FileKind::File)
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"{not-json").await.unwrap();
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));

    fs.create(Fid(1), Fid(3), "10-results", FileKind::File)
        .await
        .unwrap();
    fs.clunk(Fid(3)).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
}

#[tokio::test]
async fn stale_rule_fid_does_not_release_a_replacement_reservation() {
    let fs = RouteFs::new();
    create_rule(
        &fs,
        "10-results",
        Fid(1),
        json!({"version":1,"match_type":"result","port":"review"}),
    )
    .await;

    fs.walk(Fid::ROOT, Fid(2), &["rules".into(), "10-results".into()])
        .await
        .unwrap();
    fs.walk(Fid::ROOT, Fid(3), &["rules".into(), "10-results".into()])
        .await
        .unwrap();
    fs.remove(Fid(3)).await.unwrap();

    fs.walk(Fid::ROOT, Fid(4), &["rules".into()]).await.unwrap();
    fs.create(Fid(4), Fid(5), "10-results", FileKind::File)
        .await
        .unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    assert_eq!(
        fs.create(Fid(4), Fid(6), "10-results", FileKind::File)
            .await,
        Err(ErrorCode::BadRequest),
        "only the pending create fid may release its reservation"
    );
    fs.clunk(Fid(5)).await.unwrap();
    fs.clunk(Fid(4)).await.unwrap();
}

#[tokio::test]
async fn stale_rule_fid_writes_after_recreate_are_rejected() {
    let fs = RouteFs::new();
    create_rule(
        &fs,
        "10-results",
        Fid(1),
        json!({"version":1,"match_type":"result","port":"old"}),
    )
    .await;

    fs.walk(Fid::ROOT, Fid(2), &["rules".into(), "10-results".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    let stale_rule =
        serde_json::to_vec(&json!({"version":1,"match_type":"result","port":"stale"})).unwrap();
    fs.write(Fid(2), 0, &stale_rule).await.unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["rules".into(), "10-results".into()])
        .await
        .unwrap();
    fs.walk(Fid::ROOT, Fid(4), &["rules".into(), "10-results".into()])
        .await
        .unwrap();
    fs.remove(Fid(4)).await.unwrap();
    create_rule(
        &fs,
        "10-results",
        Fid(5),
        json!({"version":1,"match_type":"result","port":"fresh"}),
    )
    .await;

    assert_eq!(
        fs.open(Fid(3), OpenMode::Write).await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(fs.clunk(Fid(2)).await, Err(ErrorCode::BadRequest));

    write_doc(&fs, &["send"], Fid(6), &[&message("result", "done")])
        .await
        .unwrap();
    let fresh = read_text(&fs, &["ports", "fresh"], Fid(7)).await;
    assert!(fresh.contains(r#""port":"fresh""#), "{fresh}");
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

#[tokio::test]
async fn mounted_routefs_allows_rule_creation_through_mnt_route() {
    let srv = Arc::new(SrvFs::new());
    let routefs = Arc::new(RouteFs::new());
    srv.post(
        SRV_HANDLE,
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    )
    .await;
    let (handle, access) = srv.lookup(SRV_HANDLE).await.expect("route handle");
    let mut ns = Namespace::new();
    ns.mount("/srv", InProcessTransport::new(srv), Access::ReadOnly);
    ns.mount(MOUNT_PATH, handle, access);
    let fs = Arc::new(MountFs::new(ns));

    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["mnt".into(), "route".into(), "rules".into()],
    )
    .await
    .unwrap();
    fs.create(Fid(1), Fid(2), "10-results", FileKind::File)
        .await
        .unwrap();
    let rule = serde_json::to_vec(&json!({
        "version": 1,
        "match_type": "result",
        "port": "review"
    }))
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, &rule).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();

    let shell = Shell::new(InProcessTransport::new(fs));
    shell
        .write("/mnt/route/send", &message("result", "needs review"))
        .await
        .unwrap();
    let routed = String::from_utf8(shell.cat("/mnt/route/ports/review").await.unwrap()).unwrap();
    assert!(routed.contains(r#""rule":"10-results""#), "{routed}");
    assert!(routed.contains(r#""type":"result""#), "{routed}");
}

#[tokio::test]
async fn workstream_handoff_messages_route_to_root_port_and_audit_log() {
    let fs = Arc::new(RouteFs::new_with_workstream_handoff_rules());
    let mut ns = Namespace::new();
    ns.mount(MOUNT_PATH, InProcessTransport::new(fs), Access::ReadWrite);
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    let ports = shell.ls("/mnt/route/ports").await.unwrap();
    assert!(ports.contains(&ROOT_PORT.to_string()), "{ports:?}");
    assert!(ports.contains(&DEAD_LETTER_PORT.to_string()), "{ports:?}");

    for (hop, kind, target) in [
        (0, RouteHandoffKind::RerouteRequest, Some("stream-b")),
        (1, RouteHandoffKind::CrossStreamDependency, Some("stream-c")),
        (
            MAX_HANDOFF_HOPS,
            RouteHandoffKind::HandoffResult,
            Some("root"),
        ),
    ] {
        let mut msg = RouteHandoffMessage::new(
            kind,
            "stream-a",
            format!("corr-{hop}"),
            format!("handoff payload {hop}"),
        )
        .with_hop_count(hop);
        if let Some(target) = target {
            msg = msg.with_suggested_target(target);
        }
        let bytes = serde_json::to_vec(&msg).unwrap();
        shell.write("/mnt/route/send", &bytes).await.unwrap();
    }

    let root_stream = String::from_utf8(shell.cat("/mnt/route/ports/root").await.unwrap()).unwrap();
    let log_stream = String::from_utf8(shell.cat("/mnt/route/log").await.unwrap()).unwrap();
    assert_eq!(root_stream, log_stream);

    let records: Vec<RoutedRecord> = root_stream
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(
        records[0].handoff_message().unwrap().kind,
        RouteHandoffKind::RerouteRequest
    );
    assert_eq!(
        records[1].handoff_message().unwrap().kind,
        RouteHandoffKind::CrossStreamDependency
    );
    assert_eq!(
        records[2].handoff_message().unwrap().hop_count,
        MAX_HANDOFF_HOPS
    );
    assert!(!records.iter().any(RoutedRecord::is_dead_letter));
}

#[tokio::test]
async fn handoff_exceeding_max_hops_routes_to_dead_letter_and_logs_escalation() {
    let fs = Arc::new(RouteFs::new());
    fs.install_workstream_handoff_rules().await.unwrap();
    let mut ns = Namespace::new();
    ns.mount(MOUNT_PATH, InProcessTransport::new(fs), Access::ReadWrite);
    let shell = Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))));

    let looping = RouteHandoffMessage::new(
        RouteHandoffKind::RerouteRequest,
        "stream-b",
        "corr-loop",
        "back to stream-a again",
    )
    .with_suggested_target("stream-a")
    .with_hop_count(MAX_HANDOFF_HOPS + 1);

    shell
        .write("/mnt/route/send", &serde_json::to_vec(&looping).unwrap())
        .await
        .unwrap();

    let root_stream = String::from_utf8(shell.cat("/mnt/route/ports/root").await.unwrap()).unwrap();
    assert!(
        root_stream.is_empty(),
        "looping handoff must bypass root port: {root_stream}"
    );

    let dead = String::from_utf8(
        shell
            .cat(&format!("/mnt/route/ports/{DEAD_LETTER_PORT}"))
            .await
            .unwrap(),
    )
    .unwrap();
    let log = String::from_utf8(shell.cat("/mnt/route/log").await.unwrap()).unwrap();
    assert_eq!(dead, log);

    let record: RoutedRecord = serde_json::from_str(dead.trim()).unwrap();
    assert!(record.is_dead_letter());
    assert!(record.is_loop_escalation());
    assert_eq!(record.rule, LOOP_GUARD_RULE_NAME);
    assert!(
        record
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("exceeds max_handoff_hops=2")),
        "{record:?}"
    );
    assert_eq!(
        record.handoff_message().unwrap().hop_count,
        MAX_HANDOFF_HOPS + 1
    );
}

#[tokio::test]
async fn invalid_handoff_messages_are_rejected_and_legacy_hop_overflow_dead_letters() {
    let fs = RouteFs::new_with_workstream_handoff_rules();

    for bad in [
        json!({"version":2,"kind":"reroute_request","source_stream":"a","hop_count":0,"correlation_id":"c1","payload":"p"}),
        json!({"version":1,"kind":"reroute_request","source_stream":"","hop_count":0,"correlation_id":"c1","payload":"p"}),
        json!({"version":1,"kind":"reroute_request","source_stream":"a/b","hop_count":0,"correlation_id":"c1","payload":"p"}),
        json!({"version":1,"kind":"reroute_request","source_stream":"a","suggested_target":"","hop_count":0,"correlation_id":"c1","payload":"p"}),
        json!({"version":1,"kind":"reroute_request","source_stream":"a","hop_count":0,"correlation_id":"","payload":"p"}),
        json!({"version":1,"kind":"reroute_request","type":"handoff_result","source_stream":"a","hop_count":0,"correlation_id":"c1","payload":"p"}),
        json!({"version":1,"type":"reroute_request","content":"hi","hop_count":-1}),
    ] {
        assert_eq!(
            write_doc(
                &fs,
                &["send"],
                Fid(10),
                &[&serde_json::to_vec(&bad).unwrap()]
            )
            .await,
            Err(ErrorCode::BadRequest),
            "expected BadRequest for {bad}"
        );
    }

    let legacy_overflow = json!({
        "version": 1,
        "type": "reroute_request",
        "content": "looping legacy message",
        "hop_count": 999
    });
    write_doc(
        &fs,
        &["send"],
        Fid(11),
        &[&serde_json::to_vec(&legacy_overflow).unwrap()],
    )
    .await
    .unwrap();
    let dead = read_text(&fs, &["ports", DEAD_LETTER_PORT], Fid(12)).await;
    let record: RoutedRecord = serde_json::from_str(dead.trim()).unwrap();
    assert!(record.is_loop_escalation());
}
