//! `MountFs` — the kernel namespace presented as one aP [`FileServer`], so a
//! single client (the shell, the engine) reaches a whole assembled namespace
//! (`/proc`, `/agent`, `/mnt/llm`) through one transport. Paths that cross a
//! mount are delegated to the backing tree (through `Resolved::call`, so the
//! mount's access is enforced); paths above the mounts are synthetic directories
//! that list their child mount points.

use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};

fn memfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

fn procfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(ProcFs::new()))
}

/// A namespace with `/proc` (ProcFs) and `/data` (MemFs), both read-write.
fn ns() -> Namespace {
    let mut ns = Namespace::new();
    ns.mount("/proc", procfs(), Access::ReadWrite);
    ns.mount("/data", memfs(), Access::ReadWrite);
    ns
}

async fn read_lines(fs: &MountFs, path: &[&str], fid: Fid) -> Vec<String> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    let bytes = fs.read(fid, 0, 4096).await.unwrap();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn root_lists_its_mount_points_as_a_synthetic_directory() {
    let fs = MountFs::new(ns());
    let mut entries = read_lines(&fs, &[], Fid(1)).await;
    entries.sort();
    assert_eq!(entries, vec!["data", "proc"]);
}

#[tokio::test]
async fn walk_crosses_a_mount_into_the_backing_tree() {
    let fs = MountFs::new(ns());
    // /proc/clone resolves to ProcFs's clone file (a Clone-kind node).
    let qid = fs
        .walk(Fid::ROOT, Fid(1), &["proc".into(), "clone".into()])
        .await
        .unwrap();
    assert_eq!(qid.kind, FileKind::Clone);
}

#[tokio::test]
async fn read_delegates_to_the_backing_file() {
    let fs = MountFs::new(ns());
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "greeting".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(1), 0, 64).await.unwrap(), b"hi");
}

#[tokio::test]
async fn spawn_through_proc_clone_works_across_the_mount() {
    let fs = MountFs::new(ns());
    // Open /proc/clone, read the pending pid, write the exec spec, clunk to commit.
    fs.walk(Fid::ROOT, Fid(1), &["proc".into(), "clone".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(fs.read(Fid(1), 0, 64).await.unwrap()).unwrap();
    fs.write(Fid(1), 0, br#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    fs.clunk(Fid(1)).await.unwrap();

    // The process is now public: /proc/<pid>/status reads "running".
    let status = read_lines(&fs, &["proc", &pid, "status"], Fid(2)).await;
    assert_eq!(status, vec!["running"]);
}

#[tokio::test]
async fn an_unmounted_path_is_not_found() {
    let fs = MountFs::new(ns());
    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["nope".into()]).await,
        Err(ErrorCode::NotFound)
    );
}

#[tokio::test]
async fn a_read_only_mount_denies_a_write_open() {
    let mut ns = Namespace::new();
    ns.mount("/ro", memfs(), Access::ReadOnly);
    let fs = MountFs::new(ns);
    // The mount enforces access: a write-intent open is refused even though the
    // backing node is writable.
    fs.walk(Fid::ROOT, Fid(1), &["ro".into(), "submit".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
}

#[tokio::test]
async fn a_nested_mount_appears_through_synthetic_parents() {
    let mut ns = Namespace::new();
    ns.mount("/mnt/llm", memfs(), Access::ReadWrite);
    let fs = MountFs::new(ns);

    // Root lists the first component of the deep mount.
    assert_eq!(read_lines(&fs, &[], Fid(1)).await, vec!["mnt"]);
    // /mnt is a synthetic directory listing its child mount point.
    assert_eq!(read_lines(&fs, &["mnt"], Fid(2)).await, vec!["llm"]);
    // /mnt/llm/greeting reaches the backing tree.
    fs.walk(
        Fid::ROOT,
        Fid(3),
        &["mnt".into(), "llm".into(), "greeting".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(3), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(3), 0, 64).await.unwrap(), b"hi");
}

#[tokio::test]
async fn clunk_propagates_a_backing_commit_error() {
    // MemFs's `/submit` validates its document at clunk; a non-JSON body is
    // rejected at commit. MountFs must surface that error, not swallow it, so a
    // commit-on-clunk write through a mount can fail.
    let fs = MountFs::new(ns());
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "submit".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Write).await.unwrap();
    fs.write(Fid(1), 0, b"not json").await.unwrap();
    assert_eq!(fs.clunk(Fid(1)).await, Err(ErrorCode::BadRequest));
}

#[tokio::test]
async fn a_clunked_fid_is_released() {
    let fs = MountFs::new(ns());
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "greeting".into()])
        .await
        .unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    // The fid is free again: walking onto it succeeds rather than BadRequest.
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "greeting".into()])
        .await
        .unwrap();
}
