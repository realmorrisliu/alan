//! agentfs as the read-write file backing of the agent process's state
//! (`refactor-engine-namespace-native` §4): the agent writes io/output, the
//! tape, requests and actions as files; the shell writes io/input; consumers
//! read/tail. No `EventEnvelope` on the path — everything is aP file IO.

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
