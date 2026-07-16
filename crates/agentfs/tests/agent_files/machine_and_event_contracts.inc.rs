
#[tokio::test]
async fn machine_status_is_read_only_state() {
    let fs = AgentFs::new();
    assert_eq!(
        read_text(&fs, &["machine", "status"], Fid(1)).await,
        "running"
    );
    // status is read-only state (D7): it cannot be set by a free-text data write;
    // lifecycle changes go through machine/ctl instead.
    fs.walk(Fid::ROOT, Fid(2), &["machine".into(), "status".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
}

#[tokio::test]
async fn machine_ui_subtree_exposes_default_snapshots() {
    let fs = AgentFs::new();
    let machine = read_text(&fs, &["machine"], Fid(1)).await;
    assert!(machine.lines().any(|line| line == "ui"), "machine lists ui");

    let ui = read_text(&fs, &["machine", "ui"], Fid(2)).await;
    let mut entries: Vec<&str> = ui.lines().collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["activity", "events", "notice", "plan", "thinking"]
    );

    assert!(
        read_text(&fs, &["machine", "ui", "activity"], Fid(3))
            .await
            .contains("\"state\":\"idle\"")
    );
    assert!(
        read_text(&fs, &["machine", "ui", "plan"], Fid(4))
            .await
            .contains("\"items\":[]")
    );
    assert!(
        read_text(&fs, &["machine", "ui", "notice"], Fid(5))
            .await
            .contains("\"kind\":\"none\"")
    );
}

#[tokio::test]
async fn machine_ui_events_stream_supports_offset_resume() {
    let fs = AgentFs::new();
    let first = br#"{"type":"notice","snapshot":{"version":1,"kind":"warning","message":"one"}}"#;
    let second = br#"{"type":"notice","snapshot":{"version":1,"kind":"warning","message":"two"}}"#;
    write_doc(&fs, &["machine", "ui", "events"], Fid(1), first)
        .await
        .unwrap();
    write_doc(&fs, &["machine", "ui", "events"], Fid(2), b"\n")
        .await
        .unwrap();
    write_doc(&fs, &["machine", "ui", "events"], Fid(3), second)
        .await
        .unwrap();
    write_doc(&fs, &["machine", "ui", "events"], Fid(4), b"\n")
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(5),
        &["machine".into(), "ui".into(), "events".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(5), OpenMode::Read).await.unwrap();
    let offset = (first.len() + 1) as u64;
    let resumed = String::from_utf8(fs.read(Fid(5), offset, 4096).await.unwrap()).unwrap();
    assert!(
        resumed.contains("\"message\":\"two\""),
        "resumed={resumed:?}"
    );
    assert_eq!(
        fs.stat(Fid(5)).await.unwrap().length,
        (first.len() + second.len() + 2) as u64
    );
}

#[tokio::test]
async fn requests_events_stream_announces_new_requests() {
    use std::sync::Arc;
    use std::time::Duration;
    let fs = Arc::new(AgentFs::new());

    // A watcher tails requests/events before any request exists.
    let watcher = fs.clone();
    let handle = tokio::spawn(async move {
        watcher
            .walk(Fid::ROOT, Fid(9), &["requests".into(), "events".into()])
            .await
            .unwrap();
        watcher.open(Fid(9), OpenMode::Read).await.unwrap();
        watcher.read(Fid(9), 0, 4096).await.unwrap()
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "requests/events blocks until a request appears"
    );

    // Creating a request (clone-via-open) announces it.
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let rec = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8(rec).unwrap().contains("created:"));
}

#[tokio::test]
async fn io_events_is_scoped_to_io_not_the_aggregate() {
    let fs = AgentFs::new();
    // A tape write goes to the aggregate `events` but NOT to io/events.
    write_doc(&fs, &["machine", "tape"], Fid(1), b"turn-1\n")
        .await
        .unwrap();
    assert!(read_text(&fs, &["events"], Fid(2)).await.contains("tape:"));

    // io/events only carries io output/input. Read non-blocking: it has no tape
    // record. (An output write does land here.)
    write_doc(&fs, &["io", "output"], Fid(3), b"hi")
        .await
        .unwrap();
    let io_events = read_text(&fs, &["io", "events"], Fid(4)).await;
    assert!(io_events.contains("output:"), "io/events carries io output");
    assert!(
        !io_events.contains("tape:"),
        "io/events is not the aggregate (no tape records)"
    );
}

#[tokio::test]
async fn request_options_is_a_file() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(
        &fs,
        &["requests", &id, "options"],
        Fid(2),
        b"[\"approve\",\"reject\"]",
    )
    .await
    .unwrap();
    assert_eq!(
        read_text(&fs, &["requests", &id, "options"], Fid(3)).await,
        "[\"approve\",\"reject\"]"
    );
}

#[tokio::test]
async fn context_and_children_dirs_are_walkable() {
    let fs = AgentFs::new();
    let root = read_text(&fs, &[], Fid(1)).await;
    assert!(
        root.lines().any(|l| l == "context"),
        "root lists context: {root:?}"
    );
    assert!(root.lines().any(|l| l == "children"), "root lists children");
    fs.walk(Fid::ROOT, Fid(2), &["context".into()])
        .await
        .expect("context walks");
    fs.walk(Fid::ROOT, Fid(3), &["children".into()])
        .await
        .expect("children walks");
}

#[tokio::test]
async fn opening_the_root_for_write_is_rejected() {
    let fs = AgentFs::new();
    // The root is a read-only directory; a write-intent open must be denied
    // rather than silently succeeding via the ROOT fast-path.
    assert_eq!(
        fs.open(Fid::ROOT, OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        fs.open(Fid::ROOT, OpenMode::ReadWrite).await,
        Err(ErrorCode::NoAccess)
    );
    // A read-intent open of the root still works.
    fs.open(Fid::ROOT, OpenMode::Read).await.unwrap();
}

#[tokio::test]
async fn reading_a_released_fid_is_not_found() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    // The fid is gone: reading it is NotFound, not a NoAccess authority error.
    assert_eq!(fs.read(Fid(1), 0, 64).await, Err(ErrorCode::NotFound));
    // An fid that was never walked is likewise NotFound.
    assert_eq!(fs.read(Fid(99), 0, 64).await, Err(ErrorCode::NotFound));
}

#[tokio::test]
async fn answering_a_terminal_request_is_rejected() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // Answering settles the request (response write, committed on clunk).
    write_doc(&fs, &["requests", &id, "response"], Fid(2), b"approved")
        .await
        .unwrap();
    assert_eq!(
        read_text(&fs, &["requests", &id, "status"], Fid(3)).await,
        "answered"
    );

    // A second response to the now-terminal request is refused — request-status
    // integrity (agent-file-layout-contract): a decided yield is not overwritten.
    assert_eq!(
        write_doc(&fs, &["requests", &id, "response"], Fid(4), b"rejected").await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        read_text(&fs, &["requests", &id, "response"], Fid(5)).await,
        "approved"
    );
}

#[tokio::test]
async fn stat_reports_container_event_stream_lengths() {
    let fs = AgentFs::new();
    // Create a request and an action so their event streams have content.
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    fs.walk(Fid::ROOT, Fid(2), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap();

    fs.walk(Fid::ROOT, Fid(3), &["requests".into(), "events".into()])
        .await
        .unwrap();
    let req_len = fs.stat(Fid(3)).await.unwrap().length;
    fs.walk(Fid::ROOT, Fid(4), &["actions".into(), "events".into()])
        .await
        .unwrap();
    let act_len = fs.stat(Fid(4)).await.unwrap().length;

    // stat must report the real stream length, not 0.
    assert_eq!(req_len, "created:r0\n".len() as u64);
    assert_eq!(act_len, "created:a0\n".len() as u64);
}

#[tokio::test]
async fn machine_ctl_carries_runtime_tape_commands() {
    let fs = AgentFs::new();
    // machine/ctl is the agent-runtime control surface (compact/rollback);
    // semantics belong to the engine, so the file server records the command.
    let listing = read_text(&fs, &["machine"], Fid(1)).await;
    assert!(
        listing.lines().any(|l| l == "ctl"),
        "machine lists ctl: {listing:?}"
    );
    write_doc(&fs, &["machine", "ctl"], Fid(2), b"compact")
        .await
        .unwrap();
    write_doc(&fs, &["machine", "ctl"], Fid(3), b"rollback")
        .await
        .unwrap();
    let events = read_text(&fs, &["events"], Fid(4)).await;
    assert!(
        events.contains("ctl:compact"),
        "compact recorded: {events:?}"
    );
    assert!(events.contains("ctl:rollback"), "rollback recorded");
    // An empty command is malformed.
    fs.walk(Fid::ROOT, Fid(5), &["machine".into(), "ctl".into()])
        .await
        .unwrap();
    fs.open(Fid(5), OpenMode::Write).await.unwrap();
    assert_eq!(fs.write(Fid(5), 0, b"").await, Err(ErrorCode::BadRequest));
}

#[tokio::test]
async fn ordinary_data_writes_do_not_invoke_control_semantics() {
    let fs = AgentFs::new();

    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let request_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    fs.walk(Fid::ROOT, Fid(2), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap();
    let action_id = String::from_utf8(fs.read(Fid(2), 0, 64).await.unwrap()).unwrap();

    write_doc(&fs, &["io", "output"], Fid(3), b"assistant text")
        .await
        .unwrap();
    write_doc(
        &fs,
        &["machine", "tape"],
        Fid(4),
        b"{\"role\":\"assistant\"}\n",
    )
    .await
    .unwrap();
    write_doc(
        &fs,
        &["requests", &request_id, "prompt"],
        Fid(5),
        b"need input",
    )
    .await
    .unwrap();
    write_doc(
        &fs,
        &["actions", &action_id, "status"],
        Fid(6),
        b"completed",
    )
    .await
    .unwrap();

    assert_eq!(
        read_text(&fs, &["machine", "status"], Fid(7)).await,
        "running"
    );
    assert_eq!(
        read_text(&fs, &["requests", &request_id, "status"], Fid(8)).await,
        "pending"
    );
    assert_eq!(
        read_text(&fs, &["actions", &action_id, "status"], Fid(9)).await,
        "completed"
    );

    let events = read_text(&fs, &["events"], Fid(10)).await;
    assert!(
        events.contains("output:"),
        "output write recorded: {events:?}"
    );
    assert!(events.contains("tape:"), "tape write recorded: {events:?}");
    assert!(
        events.contains(&format!("request:{request_id}")),
        "request write recorded in aggregate stream: {events:?}"
    );
    assert!(
        events.contains(&format!("action:{action_id}")),
        "action write recorded in aggregate stream: {events:?}"
    );
    assert!(
        !events.contains("ctl:"),
        "ordinary data writes must not be interpreted as control commands: {events:?}"
    );

    let request_events = read_text(&fs, &["requests", "events"], Fid(11)).await;
    assert!(
        request_events.contains(&format!("{request_id}:prompt")),
        "request prompt write recorded: {request_events:?}"
    );
    let action_events = read_text(&fs, &["actions", "events"], Fid(12)).await;
    assert!(
        action_events.contains(&format!("{action_id}:status")),
        "action status write recorded: {action_events:?}"
    );
}

#[tokio::test]
async fn the_events_stream_is_watchable_by_blocking_read() {
    use std::sync::Arc;
    use std::time::Duration;
    let fs = Arc::new(AgentFs::new());

    let watcher = fs.clone();
    let handle = tokio::spawn(async move {
        watcher
            .walk(Fid::ROOT, Fid(9), &["events".to_string()])
            .await
            .unwrap();
        watcher.open(Fid(9), OpenMode::Read).await.unwrap();
        watcher.read(Fid(9), 0, 4096).await.unwrap()
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !handle.is_finished(),
        "watcher blocks until an event is written"
    );

    write_doc(&fs, &["io", "output"], Fid(1), b"hi")
        .await
        .unwrap();
    let rec = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8(rec).unwrap().contains("output:"));
}
async fn qid_version(fs: &AgentFs, path: &[&str], fid: Fid) -> u32 {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.unwrap();
    fs.stat(fid).await.unwrap().qid.version
}

#[tokio::test]
async fn qid_versions_bump_when_dirs_and_fields_change() {
    let fs = AgentFs::new();

    // requests/ dir version bumps when a request is created.
    let r0 = qid_version(&fs, &["requests"], Fid(1)).await;
    fs.walk(Fid::ROOT, Fid(2), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(2), 0, 64).await.unwrap()).unwrap();
    let r1 = qid_version(&fs, &["requests"], Fid(3)).await;
    assert_eq!(r1, r0 + 1, "requests/ listing changed");

    // requests/<id>/status version bumps when answering settles the request.
    let s0 = qid_version(&fs, &["requests", &id, "status"], Fid(4)).await;
    write_doc(&fs, &["requests", &id, "response"], Fid(5), b"approved")
        .await
        .unwrap();
    let s1 = qid_version(&fs, &["requests", &id, "status"], Fid(6)).await;
    assert_eq!(s1, s0 + 1, "answering changed status");

    // A stream's qid version stays stable (freshness is the read offset).
    write_doc(&fs, &["io", "output"], Fid(7), b"hi")
        .await
        .unwrap();
    assert_eq!(qid_version(&fs, &["io", "output"], Fid(8)).await, 0);
}

#[tokio::test]
async fn an_empty_response_still_settles_the_request() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // An intentionally empty answer: a zero-byte write then clunk. The commit
    // trigger is write intent, not a non-empty buffer, so the request settles.
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["requests".into(), id.clone(), "response".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();

    assert_eq!(
        read_text(&fs, &["requests", &id, "response"], Fid(3)).await,
        ""
    );
    assert_eq!(
        read_text(&fs, &["requests", &id, "status"], Fid(4)).await,
        "answered"
    );
}

#[tokio::test]
async fn machine_ctl_read_returns_in_band_help() {
    let fs = AgentFs::new();
    let help = read_text(&fs, &["machine", "ctl"], Fid(1)).await;
    assert!(help.contains("compact"), "ctl help lists compact: {help:?}");
    assert!(help.contains("rollback"), "ctl help lists rollback");
}
