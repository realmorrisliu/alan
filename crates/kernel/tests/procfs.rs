//! `/proc` synthetic device (substrate §7.1) and spawn via clone-via-open
//! (§7.1a). `/proc` renders the process table as files: a `clone` file plus a
//! directory per pid (`status`, `parent`, `credentials`, `exit`, `ctl`, `io/`).
//! Process creation is pure aP — open `/proc/clone` (a pending pid, not yet
//! public), write the exec spec, and `clunk` to commit — so an aP-only client
//! needs no side API to launch a process.

use alan_ap::{ErrorCode, Fid, FileServer, OpenMode};
use alan_kernel::ProcFs;

fn proc() -> ProcFs {
    ProcFs::new()
}

/// Spawn a process via clone-via-open using a distinct fid base; returns its pid.
async fn spawn(fs: &ProcFs, clone_fid: Fid) -> String {
    fs.walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .unwrap();
    fs.open(clone_fid, OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(clone_fid, 0, 64).await.unwrap()).unwrap();
    fs.write(clone_fid, 0, br#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    fs.clunk(clone_fid).await.unwrap();
    pid
}

async fn read_at(fs: &ProcFs, names: &[&str], fid: Fid) -> Result<Vec<u8>, ErrorCode> {
    let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await?;
    fs.open(fid, OpenMode::Read).await?;
    fs.read(fid, 0, 4096).await
}

#[tokio::test]
async fn empty_proc_lists_only_clone() {
    let fs = proc();
    let listing = read_at(&fs, &[], Fid(1)).await.unwrap();
    let text = String::from_utf8(listing).unwrap();
    assert_eq!(text.lines().collect::<Vec<_>>(), vec!["clone"]);
}

#[tokio::test]
async fn spawn_via_clone_open_write_clunk_makes_a_public_process() {
    let fs = proc();

    // open /proc/clone → the pending pid is returned by reading the clone fid.
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(fs.read(Fid(10), 0, 64).await.unwrap()).unwrap();
    assert!(!pid_name.is_empty());

    // The pending slot is not yet visible in public /proc.
    let before = String::from_utf8(read_at(&fs, &[], Fid(11)).await.unwrap()).unwrap();
    assert!(
        !before.lines().any(|l| l == pid_name),
        "pending slot is fid-private"
    );

    // Write the exec spec (commit-on-clunk) and clunk to start.
    fs.write(Fid(10), 0, br#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(fs.clunk(Fid(10)).await, Ok(()));

    // Now /proc/<pid> is public and its status reads "running".
    let after = String::from_utf8(read_at(&fs, &[], Fid(12)).await.unwrap()).unwrap();
    assert!(
        after.lines().any(|l| l == pid_name),
        "committed process is public: {after:?}"
    );

    let status =
        String::from_utf8(read_at(&fs, &[&pid_name, "status"], Fid(13)).await.unwrap()).unwrap();
    assert_eq!(status.trim(), "running");
}

#[tokio::test]
async fn a_malformed_exec_spec_is_rejected_at_clunk_and_leaks_nothing() {
    let fs = proc();

    fs.walk(Fid::ROOT, Fid(20), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(20), OpenMode::ReadWrite).await.unwrap();
    let pid_name = String::from_utf8(fs.read(Fid(20), 0, 64).await.unwrap()).unwrap();

    // Truncated exec spec: rejected at the commit point, not before.
    fs.write(Fid(20), 0, b"{ truncated").await.unwrap();
    assert_eq!(fs.clunk(Fid(20)).await, Err(ErrorCode::BadRequest));

    // The fid-private slot was discarded; public /proc never shows it.
    let listing = String::from_utf8(read_at(&fs, &[], Fid(21)).await.unwrap()).unwrap();
    assert!(
        !listing.lines().any(|l| l == pid_name),
        "rejected spawn leaks nothing"
    );
}

// /proc/<pid>/io/output is wired to the process output stream, not Unsupported:
// reading an empty live output blocks (stream semantics) rather than erroring
// (PR #574 review).
#[tokio::test]
async fn proc_output_serves_the_stream() {
    use std::time::Duration;
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;

    fs.walk(Fid::ROOT, Fid(11), &[pid, "io".into(), "output".into()])
        .await
        .unwrap();
    fs.open(Fid(11), OpenMode::Read).await.unwrap();
    // Empty output stream → the read blocks; it must NOT return Unsupported.
    let r = tokio::time::timeout(Duration::from_millis(30), fs.read(Fid(11), 0, 64)).await;
    assert!(
        r.is_err(),
        "reading io/output should block on the stream, not error"
    );
}

// Spawning requires write intent: opening /proc/clone read-only is rejected, and
// a ctl write needs write authority (PR #574 review).
#[tokio::test]
async fn write_surfaces_require_write_intent() {
    let fs = proc();

    // Read-only open of clone cannot allocate a (would-be leaked) pending slot.
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(10), OpenMode::Read).await,
        Err(ErrorCode::NoAccess)
    );

    // ctl opened read-only cannot cancel the process.
    let pid = spawn(&fs, Fid(20)).await;
    fs.walk(Fid::ROOT, Fid(21), &[pid.clone(), "ctl".into()])
        .await
        .unwrap();
    fs.open(Fid(21), OpenMode::Read).await.unwrap();
    assert_eq!(
        fs.write(Fid(21), 0, b"cancel").await,
        Err(ErrorCode::NoAccess)
    );
    // Still running — the read-only cancel did not take effect.
    fs.walk(Fid::ROOT, Fid(22), &[pid, "status".into()])
        .await
        .unwrap();
    fs.open(Fid(22), OpenMode::Read).await.unwrap();
    assert_eq!(
        String::from_utf8(fs.read(Fid(22), 0, 64).await.unwrap())
            .unwrap()
            .trim(),
        "running"
    );
}

// walk rejects reused/reserved newfids; open rejects reopening a live fid — so a
// retry cannot clobber a pending clone slot (PR #574 review).
#[tokio::test]
async fn fid_reuse_and_reopen_are_rejected() {
    let fs = proc();
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    // Reusing a live fid is rejected, not a silent clobber.
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()]).await,
        Err(ErrorCode::BadRequest)
    );
    // Reopening a live fid before clunk is rejected.
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    assert_eq!(
        fs.open(Fid(10), OpenMode::ReadWrite).await,
        Err(ErrorCode::BadRequest)
    );
}

// The clone exec-spec write honors byte offsets, so out-of-order chunks build the
// addressed document (PR #574 review).
#[tokio::test]
async fn clone_exec_spec_write_honors_offset() {
    let fs = proc();
    fs.walk(Fid::ROOT, Fid(10), &["clone".to_string()])
        .await
        .unwrap();
    fs.open(Fid(10), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(Fid(10), 0, 64).await.unwrap()).unwrap();
    // Write the tail first (at offset 14), then the head (offset 0).
    fs.write(Fid(10), 14, br#""/bin/agent","args":[]}"#)
        .await
        .unwrap();
    fs.write(Fid(10), 0, br#"{"executable":"#).await.unwrap();
    assert_eq!(fs.clunk(Fid(10)).await, Ok(()));
    // Committed cleanly → the process is public.
    let listing = String::from_utf8(read_at(&fs, &[], Fid(11)).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|l| l == pid),
        "offset-assembled spec spawned the process"
    );
}

// stat reports the readable byte length, so clients can size reads (PR #574).
#[tokio::test]
async fn stat_reports_readable_length() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    fs.walk(Fid::ROOT, Fid(11), &[pid, "status".into()])
        .await
        .unwrap();
    let st = fs.stat(Fid(11)).await.unwrap();
    // status reads "running\n" (8 bytes); stat must not report 0.
    assert_eq!(st.length, 8, "stat length matches the bytes read returns");
}

// The pre-bound /proc root fid can be opened directly (no redundant empty walk),
// matching SrvFs and the reference server (PR #574 review).
#[tokio::test]
async fn root_fid_is_openable_directly() {
    let fs = proc();
    fs.open(Fid::ROOT, OpenMode::Read)
        .await
        .expect("root fid opens directly");
    let listing = String::from_utf8(fs.read(Fid::ROOT, 0, 64).await.unwrap()).unwrap();
    assert!(
        listing.lines().any(|l| l == "clone"),
        "root listing is readable via the root fid"
    );
}

// /proc/<pid>/namespace renders the process's mounted capability set
// (PR #574 review).
#[tokio::test]
async fn proc_exposes_the_process_namespace() {
    let fs = proc();
    let pid = spawn(&fs, Fid(10)).await;
    // The file exists and is listed in the process directory.
    let dir = String::from_utf8(read_at(&fs, &[&pid], Fid(11)).await.unwrap()).unwrap();
    assert!(
        dir.lines().any(|l| l == "namespace"),
        "namespace is listed: {dir:?}"
    );
    // And it reads (empty for a system-spawned process with an empty namespace).
    read_at(&fs, &[&pid, "namespace"], Fid(12))
        .await
        .expect("namespace is readable");
}
