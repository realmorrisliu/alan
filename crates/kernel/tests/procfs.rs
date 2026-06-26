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
