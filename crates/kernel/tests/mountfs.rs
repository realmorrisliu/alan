//! `MountFs` — the kernel namespace presented as one aP [`FileServer`], so a
//! single client (the shell, the engine) reaches a whole assembled namespace
//! (`/proc`, `/agent`, `/mnt/llm`) through one transport. Paths that cross a
//! mount are delegated to the backing tree (through `Resolved::call`, so the
//! mount's access is enforced); paths above the mounts are synthetic directories
//! that list their child mount points.

use std::collections::HashMap;
use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
};
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

/// A backing server with one stream file `out` (blocks at the live edge) under a
/// directory root, used to prove a parked tail does not freeze the namespace.
struct StreamFs {
    stream: Stream,
    fids: tokio::sync::Mutex<HashMap<Fid, bool>>, // fid -> is the `out` stream
}

impl StreamFs {
    /// The server plus a direct handle to its stream, so a test can append to it
    /// (unblocking a parked reader) without going back through the namespace.
    fn new() -> (Self, Stream) {
        let stream = Stream::new();
        (
            Self {
                stream: stream.clone(),
                fids: tokio::sync::Mutex::new(HashMap::new()),
            },
            stream,
        )
    }
}

#[async_trait::async_trait]
impl FileServer for StreamFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let (is_stream, kind) = match names {
            [] => (false, FileKind::Dir),
            [n] if n == "out" => (true, FileKind::Stream),
            _ => return Err(ErrorCode::NotFound),
        };
        self.fids.lock().await.insert(newfid, is_stream);
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Stream,
            version: 0,
            path: 0,
        })
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if *self.fids.lock().await.get(&fid).unwrap_or(&false) {
            Ok(self.stream.read(offset, count).await)
        } else {
            Err(ErrorCode::Unsupported)
        }
    }
    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: FileKind::Stream,
                version: 0,
                path: 0,
            },
            length: self.stream.len().await,
            writable: false,
        })
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

#[tokio::test]
async fn a_parked_stream_read_does_not_freeze_the_namespace() {
    use std::time::Duration;

    // /s is a stream server (blocking live edge); /data is a MemFs. A read parked at
    // /s/out must not hold the MountFs lock, or a concurrent op deadlocks — the M2
    // "tail output while submitting input" workflow.
    let (streamfs, stream) = StreamFs::new();
    let mut ns = Namespace::new();
    ns.mount(
        "/s",
        InProcessTransport::new(Arc::new(streamfs)),
        Access::ReadWrite,
    );
    ns.mount("/data", memfs(), Access::ReadWrite);
    let fs = Arc::new(MountFs::new(ns));

    // A tail parks at the live edge of /s/out.
    let tailer = fs.clone();
    let parked = tokio::spawn(async move {
        tailer
            .walk(Fid::ROOT, Fid(1), &["s".into(), "out".into()])
            .await
            .unwrap();
        tailer.open(Fid(1), OpenMode::Read).await.unwrap();
        tailer.read(Fid(1), 0, 64).await.unwrap()
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(
        !parked.is_finished(),
        "the tail should be parked at the live edge"
    );

    // A concurrent op on the same MountFs must complete despite the parked read.
    let concurrent = tokio::time::timeout(Duration::from_millis(500), async {
        fs.walk(Fid::ROOT, Fid(2), &["data".into(), "greeting".into()])
            .await
            .unwrap();
        fs.open(Fid(2), OpenMode::Read).await.unwrap();
        fs.read(Fid(2), 0, 64).await.unwrap()
    })
    .await
    .expect("a concurrent op must not be blocked by a parked stream read");
    assert_eq!(concurrent, b"hi");

    // Appending unblocks the parked tail.
    stream.append(b"live").await;
    let got = tokio::time::timeout(Duration::from_millis(500), parked)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, b"live");
}

#[tokio::test]
async fn two_wrappers_sharing_a_backing_server_do_not_collide_backing_fids() {
    // Two MountFs over namespaces that share the SAME ProcFs transport (as child
    // namespaces cloning /proc would). Backing fids must be unique across wrappers,
    // or the second wrapper's walk reuses a live fid and the shared server rejects
    // it with BadRequest.
    let shared = InProcessTransport::new(Arc::new(ProcFs::new()));
    let mut ns1 = Namespace::new();
    ns1.mount("/proc", shared.clone(), Access::ReadWrite);
    let mut ns2 = Namespace::new();
    ns2.mount("/proc", shared.clone(), Access::ReadWrite);
    let fs1 = MountFs::new(ns1);
    let fs2 = MountFs::new(ns2);

    // fs1 holds a live delegated fid on the shared ProcFs.
    fs1.walk(Fid::ROOT, Fid(1), &["proc".into(), "clone".into()])
        .await
        .unwrap();
    // fs2's first walk must not collide with it.
    fs2.walk(Fid::ROOT, Fid(1), &["proc".into(), "clone".into()])
        .await
        .unwrap();
}

#[tokio::test]
async fn remove_of_root_is_refused_and_preserves_the_handle() {
    let fs = MountFs::new(ns());
    assert_eq!(fs.remove(Fid::ROOT).await, Err(ErrorCode::Unsupported));
    // The root anchor survives, so the handle still resolves absolute paths.
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "greeting".into()])
        .await
        .unwrap();
}

#[tokio::test]
async fn a_synthetic_parent_resolves_under_a_broad_mount() {
    // A broad `/` mount (a MemFs with no `mnt` entry) plus a deeper `/mnt/llm`.
    let mut ns = Namespace::new();
    ns.mount("/", memfs(), Access::ReadWrite);
    ns.mount("/mnt/llm", memfs(), Access::ReadWrite);
    let fs = MountFs::new(ns);

    // Walking to /mnt one component at a time must yield the synthetic parent of
    // /mnt/llm, not NotFound from the broad mount lacking `mnt`.
    assert_eq!(read_lines(&fs, &["mnt"], Fid(1)).await, vec!["llm"]);
    // And the deeper mount is still reachable through it.
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["mnt".into(), "llm".into(), "greeting".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(2), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(2), 0, 64).await.unwrap(), b"hi");
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
