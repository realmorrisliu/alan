use alan_agentfs::AgentFs;
use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};

async fn write_doc(fs: &AgentFs, path: &[&str], fid: Fid, data: &[u8]) -> Result<(), ErrorCode> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Write).await?;
    fs.write(fid, 0, data).await?;
    fs.clunk(fid).await
}

async fn read_text(fs: &AgentFs, path: &[&str], fid: Fid) -> String {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    String::from_utf8(fs.read(fid, 0, 65536).await.unwrap()).unwrap()
}

/// Parse the length-framed `io/input` stream (`<len>\n<payload>` per message)
/// into the sequence of message payloads.
fn parse_input_frames(raw: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let nl = i + raw[i..]
            .iter()
            .position(|&b| b == b'\n')
            .expect("frame header");
        let len: usize = std::str::from_utf8(&raw[i..nl]).unwrap().parse().unwrap();
        let start = nl + 1;
        out.push(String::from_utf8(raw[start..start + len].to_vec()).unwrap());
        i = start + len;
    }
    out
}

async fn read_input_messages(fs: &AgentFs, fid: Fid) -> Vec<String> {
    fs.walk(Fid::ROOT, fid, &["io".into(), "input".into()])
        .await
        .unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    parse_input_frames(&fs.read(fid, 0, 65536).await.unwrap())
}

#[tokio::test]
async fn shell_writes_input_and_agent_reads_it() {
    let fs = AgentFs::new();
    // The shell (or parent) writes a message into the agent's input.
    write_doc(&fs, &["io", "input"], Fid(1), b"hello agent")
        .await
        .unwrap();
    // The agent reads it back as a single framed message.
    assert_eq!(read_input_messages(&fs, Fid(2)).await, vec!["hello agent"]);
}

#[tokio::test]
async fn io_input_preserves_message_boundaries() {
    let fs = AgentFs::new();
    // Two messages committed before the agent drains must stay distinguishable —
    // not collapse into one "helloworld" byte run.
    write_doc(&fs, &["io", "input"], Fid(1), b"hello")
        .await
        .unwrap();
    write_doc(&fs, &["io", "input"], Fid(2), b"world")
        .await
        .unwrap();
    assert_eq!(
        read_input_messages(&fs, Fid(3)).await,
        vec!["hello", "world"]
    );
    // A payload containing a newline is still one message (length-framed, not
    // newline-delimited).
    write_doc(&fs, &["io", "input"], Fid(4), b"a\nb")
        .await
        .unwrap();
    assert_eq!(
        read_input_messages(&fs, Fid(5)).await,
        vec!["hello", "world", "a\nb"]
    );
}

#[tokio::test]
async fn agent_writes_output_and_it_is_readable() {
    let fs = AgentFs::new();
    // The agent appends assistant text to its output (two writes).
    fs.walk(Fid::ROOT, Fid(1), &["io".into(), "output".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"Hello ").await.unwrap();
    fs.write(Fid(1), 0, b"world").await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();

    assert_eq!(
        read_text(&fs, &["io", "output"], Fid(2)).await,
        "Hello world"
    );
    // Every output write records into the aggregate events stream.
    assert!(
        read_text(&fs, &["events"], Fid(3))
            .await
            .contains("output:")
    );
}

#[tokio::test]
async fn tape_is_append_only_and_readable() {
    let fs = AgentFs::new();
    write_doc(&fs, &["machine", "tape"], Fid(1), b"turn-1\n")
        .await
        .unwrap();
    write_doc(&fs, &["machine", "tape"], Fid(2), b"turn-2\n")
        .await
        .unwrap();
    assert_eq!(
        read_text(&fs, &["machine", "tape"], Fid(3)).await,
        "turn-1\nturn-2\n"
    );
}

#[tokio::test]
async fn machine_tape_is_backed_by_a_content_addressed_checkpoint() {
    let fs = AgentFs::new();
    let empty_root = fs.current_tape_checkpoint().await;

    write_doc(&fs, &["machine", "tape"], Fid(1), b"turn-1\n")
        .await
        .unwrap();
    write_doc(&fs, &["machine", "tape"], Fid(2), b"turn-2\n")
        .await
        .unwrap();

    let file_view = read_text(&fs, &["machine", "tape"], Fid(3)).await;
    assert_eq!(file_view, "turn-1\nturn-2\n");
    assert_eq!(
        fs.materialize_tape_checkpoint().await.unwrap(),
        file_view.as_bytes()
    );
    fs.verify_tape_checkpoint().await.unwrap();

    let checkpoint = read_text(&fs, &["machine", "checkpoints", "current"], Fid(4)).await;
    let current_root = fs.current_tape_checkpoint().await;
    assert_ne!(current_root, empty_root);
    assert_eq!(checkpoint.trim(), current_root.as_str());
}

#[tokio::test]
async fn a_yield_is_a_request_opened_by_the_agent_and_answered_by_a_consumer() {
    let fs = AgentFs::new();

    // The agent opens a yield via clone-via-open and writes its kind/prompt.
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(&fs, &["requests", &id, "kind"], Fid(2), b"confirmation")
        .await
        .unwrap();
    write_doc(&fs, &["requests", &id, "prompt"], Fid(3), b"approve?")
        .await
        .unwrap();

    assert_eq!(
        read_text(&fs, &["requests", &id, "kind"], Fid(4)).await,
        "confirmation"
    );
    assert_eq!(
        read_text(&fs, &["requests", &id, "status"], Fid(5)).await,
        "pending"
    );

    // A consumer answers by writing the response (committed on clunk), which
    // settles the request (agent-file-layout-contract).
    write_doc(&fs, &["requests", &id, "response"], Fid(6), b"approved")
        .await
        .unwrap();
    assert_eq!(
        read_text(&fs, &["requests", &id, "response"], Fid(7)).await,
        "approved"
    );
    assert_eq!(
        read_text(&fs, &["requests", &id, "status"], Fid(8)).await,
        "answered"
    );
}

#[tokio::test]
async fn a_tool_call_is_an_action_the_agent_records() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // A freshly opened action is running until the agent records its outcome.
    assert_eq!(
        read_text(&fs, &["actions", &id, "status"], Fid(2)).await,
        "running"
    );
    write_doc(&fs, &["actions", &id, "name"], Fid(3), b"read")
        .await
        .unwrap();
    write_doc(&fs, &["actions", &id, "status"], Fid(4), b"completed")
        .await
        .unwrap();
    assert_eq!(
        read_text(&fs, &["actions", &id, "name"], Fid(5)).await,
        "read"
    );
    assert_eq!(
        read_text(&fs, &["actions", &id, "status"], Fid(6)).await,
        "completed"
    );
}

#[tokio::test]
async fn read_write_field_range_edits_preserve_existing_bytes() {
    let fs = AgentFs::new();

    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let request_id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(&fs, &["requests", &request_id, "prompt"], Fid(2), b"abcdef")
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(3),
        &["requests".into(), request_id.clone(), "prompt".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::ReadWrite).await.unwrap();
    fs.write(Fid(3), 2, b"XY").await.unwrap();
    fs.clunk(Fid(3)).await.unwrap();

    assert_eq!(
        read_text(&fs, &["requests", &request_id, "prompt"], Fid(4)).await,
        "abXYef"
    );

    fs.walk(Fid::ROOT, Fid(5), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(5), OpenMode::ReadWrite).await.unwrap();
    let action_id = String::from_utf8(fs.read(Fid(5), 0, 64).await.unwrap()).unwrap();
    write_doc(&fs, &["actions", &action_id, "result"], Fid(6), b"tool")
        .await
        .unwrap();

    fs.walk(
        Fid::ROOT,
        Fid(7),
        &["actions".into(), action_id.clone(), "result".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(7), OpenMode::ReadWrite).await.unwrap();
    fs.write(Fid(7), 4, b" output").await.unwrap();
    fs.clunk(Fid(7)).await.unwrap();

    assert_eq!(
        read_text(&fs, &["actions", &action_id, "result"], Fid(8)).await,
        "tool output"
    );
}

#[tokio::test]
async fn an_action_exposes_all_documented_fields() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();

    // The contract records result, approval state, and a process reference in
    // addition to name/status/output — all reachable through actions/<id>/.
    let listing = read_text(&fs, &["actions", &id], Fid(2)).await;
    let mut entries: Vec<&str> = listing.lines().collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["approval", "name", "output", "process", "result", "status"]
    );
    write_doc(&fs, &["actions", &id, "result"], Fid(3), b"{\"ok\":true}")
        .await
        .unwrap();
    write_doc(&fs, &["actions", &id, "approval"], Fid(4), b"approved")
        .await
        .unwrap();
    write_doc(&fs, &["actions", &id, "process"], Fid(5), b"/proc/42")
        .await
        .unwrap();
    assert_eq!(
        read_text(&fs, &["actions", &id, "result"], Fid(6)).await,
        "{\"ok\":true}"
    );
    assert_eq!(
        read_text(&fs, &["actions", &id, "approval"], Fid(7)).await,
        "approved"
    );
    assert_eq!(
        read_text(&fs, &["actions", &id, "process"], Fid(8)).await,
        "/proc/42"
    );
}

#[tokio::test]
async fn actions_help_describes_projection_retention_and_redaction_in_band() {
    let fs = AgentFs::new();
    let help = read_text(&fs, &["actions", "help"], Fid(1)).await;

    assert!(help.contains("evidence_projection"));
    assert!(help.contains("namespace reference"));
    assert!(help.contains("evidence_retention_expired"));
    assert!(help.contains("[REDACTED reason=<class>]"));
}

#[tokio::test]
async fn action_output_remains_readable_from_content_store_after_process_exit() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(
        &fs,
        &["actions", &id, "output"],
        Fid(2),
        b"durable tool evidence",
    )
    .await
    .unwrap();
    write_doc(&fs, &["actions", &id, "process"], Fid(3), b"/proc/42")
        .await
        .unwrap();
    write_doc(&fs, &["actions", &id, "status"], Fid(4), b"completed")
        .await
        .unwrap();

    assert_eq!(
        read_text(&fs, &["actions", &id, "output"], Fid(5)).await,
        "durable tool evidence"
    );
}

#[tokio::test]
async fn action_output_is_immutable_after_its_first_durable_write() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(
        &fs,
        &["actions", &id, "output"],
        Fid(2),
        b"original evidence",
    )
    .await
    .unwrap();

    let rewrite = write_doc(
        &fs,
        &["actions", &id, "output"],
        Fid(3),
        b"replacement evidence",
    )
    .await;

    assert_eq!(rewrite, Err(ErrorCode::NoAccess));
    assert_eq!(
        read_text(&fs, &["actions", &id, "output"], Fid(4)).await,
        "original evidence"
    );
}

#[tokio::test]
async fn expired_action_output_returns_structured_retention_record() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["actions".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    write_doc(
        &fs,
        &["actions", &id, "output"],
        Fid(2),
        b"retained until policy expiry",
    )
    .await
    .unwrap();

    fs.expire_action_output_for_retention(&id, "age_limit")
        .await
        .unwrap();

    let expired = read_text(&fs, &["actions", &id, "output"], Fid(3)).await;
    assert!(expired.contains("\"type\":\"evidence_retention_expired\""));
    assert!(expired.contains("\"cause\":\"age_limit\""));
}

#[tokio::test]
async fn machine_tape_holds_an_exclusive_write_lease() {
    let fs = AgentFs::new();
    // One writer holds machine/tape open for write (the generating engine).
    fs.walk(Fid::ROOT, Fid(1), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();

    // A second write-open of the tape is refused while the lease is held.
    fs.walk(Fid::ROOT, Fid(2), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );

    // Readers are not excluded — a tail during generation still works.
    fs.walk(Fid::ROOT, Fid(3), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();

    // Releasing the writer releases the lease; a new writer may then take it.
    fs.clunk(Fid(1)).await.unwrap();
    fs.walk(Fid::ROOT, Fid(4), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    fs.open(Fid(4), OpenMode::Write).await.unwrap();
}

#[tokio::test]
async fn writing_without_write_intent_is_rejected() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["io".into(), "output".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(fs.write(Fid(1), 0, b"x").await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn opening_a_read_only_node_for_write_is_rejected() {
    let fs = AgentFs::new();
    // `events` is read-only; a write-intent open fails at dial time.
    fs.walk(Fid::ROOT, Fid(1), &["events".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
}

#[tokio::test]
async fn reading_without_read_open_is_rejected() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    // No open: read is denied (read authority not established).
    assert_eq!(fs.read(Fid(1), 0, 64).await, Err(ErrorCode::NoAccess));
}

#[tokio::test]
async fn distinct_files_get_distinct_qids() {
    let fs = AgentFs::new();
    let out = fs
        .walk(Fid::ROOT, Fid(1), &["io".into(), "output".into()])
        .await
        .unwrap();
    let tape = fs
        .walk(Fid::ROOT, Fid(2), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    let events = fs
        .walk(Fid::ROOT, Fid(3), &["events".into()])
        .await
        .unwrap();
    assert_ne!(out.path, tape.path);
    assert_ne!(out.path, events.path);
    assert_ne!(tape.path, events.path);
}

#[tokio::test]
async fn clone_allocation_requires_write_intent() {
    let fs = AgentFs::new();
    // Clone-via-open requires ReadWrite (you allocate *and* read the id back);
    // neither a read-only nor a write-only open may allocate.
    fs.walk(Fid::ROOT, Fid(1), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Read).await,
        Err(ErrorCode::NoAccess)
    );
    fs.walk(Fid::ROOT, Fid(2), &["requests".into(), "clone".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    // No request id was created — the dir lists only its fixed entries.
    let listing = read_text(&fs, &["requests"], Fid(3)).await;
    let mut entries: Vec<&str> = listing.lines().collect();
    entries.sort();
    assert_eq!(entries, vec!["clone", "events"], "no rN entry leaked");
}
