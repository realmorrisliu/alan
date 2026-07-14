//! `MountFs` — the kernel namespace presented as one aP [`FileServer`], so a
//! single client (the shell, the engine) reaches a whole assembled namespace
//! (`/proc`, `/agent`, `/mnt/llm`) through one transport. Paths that cross a
//! mount are delegated to the backing tree (through `Resolved::call`, so the
//! mount's access is enforced); paths above the mounts are synthetic directories
//! that list their child mount points.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
};
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, ProcFs};
use tokio::sync::Notify;

fn memfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

fn procfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(ProcFs::new()))
}

fn createfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(CreateFs::default()))
}

fn static_filefs(content: &'static [u8]) -> InProcessTransport {
    InProcessTransport::new(Arc::new(StaticFileFs {
        content,
        fids: tokio::sync::Mutex::new(HashMap::new()),
    }))
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

async fn read_file(fs: &MountFs, path: &[&str], fid: Fid) -> Vec<u8> {
    let names: Vec<String> = path.iter().map(|s| s.to_string()).collect();
    fs.walk(Fid::ROOT, fid, &names).await.unwrap();
    fs.open(fid, OpenMode::Read).await.unwrap();
    fs.read(fid, 0, 4096).await.unwrap()
}

#[derive(Clone)]
enum CreateNode {
    Root,
    File(String),
}

#[derive(Default)]
struct CreateFs {
    files: tokio::sync::Mutex<HashMap<String, Vec<u8>>>,
    fids: tokio::sync::Mutex<HashMap<Fid, CreateNode>>,
    create_gate: Option<Arc<Notify>>,
    create_started: Option<Arc<AtomicUsize>>,
}

impl CreateFs {
    fn gated(create_gate: Arc<Notify>, create_started: Arc<AtomicUsize>) -> Self {
        Self {
            create_gate: Some(create_gate),
            create_started: Some(create_started),
            ..Self::default()
        }
    }
}

#[async_trait::async_trait]
impl FileServer for CreateFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let node = match names {
            [] => CreateNode::Root,
            [name] if self.files.lock().await.contains_key(name) => CreateNode::File(name.clone()),
            _ => return Err(ErrorCode::NotFound),
        };
        let kind = match node {
            CreateNode::Root => FileKind::Dir,
            CreateNode::File(_) => FileKind::File,
        };
        self.fids.lock().await.insert(newfid, node);
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let kind = match self.fids.lock().await.get(&fid) {
            Some(CreateNode::Root) => FileKind::Dir,
            Some(CreateNode::File(_)) => FileKind::File,
            None if fid == Fid::ROOT => FileKind::Dir,
            None => return Err(ErrorCode::NotFound),
        };
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match self.fids.lock().await.get(&fid) {
            Some(CreateNode::Root) => {
                let mut names = self.files.lock().await.keys().cloned().collect::<Vec<_>>();
                names.sort();
                names.join("\n").into_bytes()
            }
            None if fid == Fid::ROOT => {
                let mut names = self.files.lock().await.keys().cloned().collect::<Vec<_>>();
                names.sort();
                names.join("\n").into_bytes()
            }
            Some(CreateNode::File(name)) => self
                .files
                .lock()
                .await
                .get(name)
                .cloned()
                .ok_or(ErrorCode::NotFound)?,
            None => return Err(ErrorCode::NotFound),
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let name = match self.fids.lock().await.get(&fid) {
            Some(CreateNode::File(name)) => name.clone(),
            Some(CreateNode::Root) => return Err(ErrorCode::IsDirectory),
            None => return Err(ErrorCode::NotFound),
        };
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        let mut files = self.files.lock().await;
        let bytes = files.get_mut(&name).ok_or(ErrorCode::NotFound)?;
        if bytes.len() < end {
            bytes.resize(end, 0);
        }
        bytes[start..end].copy_from_slice(data);
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let (kind, length, writable) = match self.fids.lock().await.get(&fid) {
            Some(CreateNode::Root) => (FileKind::Dir, 0, true),
            None if fid == Fid::ROOT => (FileKind::Dir, 0, true),
            Some(CreateNode::File(name)) => {
                let length = self
                    .files
                    .lock()
                    .await
                    .get(name)
                    .map(|bytes| bytes.len() as u64)
                    .ok_or(ErrorCode::NotFound)?;
                (FileKind::File, length, true)
            }
            None => return Err(ErrorCode::NotFound),
        };
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind,
                version: 0,
                path: 0,
            },
            length,
            executable: false,
            writable,
        })
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if kind != FileKind::File || name.is_empty() || name.contains('/') || name.contains('\n') {
            return Err(ErrorCode::BadRequest);
        }
        let is_root =
            matches!(self.fids.lock().await.get(&fid), Some(CreateNode::Root)) || fid == Fid::ROOT;
        if !is_root {
            return Err(ErrorCode::NotDirectory);
        }
        if let Some(started) = &self.create_started {
            started.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(gate) = &self.create_gate {
            gate.notified().await;
        }
        let mut files = self.files.lock().await;
        if files.contains_key(name) {
            return Err(ErrorCode::BadRequest);
        }
        files.insert(name.to_string(), Vec::new());
        self.fids
            .lock()
            .await
            .insert(newfid, CreateNode::File(name.to_string()));
        Ok(Qid {
            kind: FileKind::File,
            version: 0,
            path: 0,
        })
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let Some(CreateNode::File(name)) = self.fids.lock().await.remove(&fid) else {
            return Err(ErrorCode::Unsupported);
        };
        self.files
            .lock()
            .await
            .remove(&name)
            .ok_or(ErrorCode::NotFound)?;
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid != Fid::ROOT {
            self.fids.lock().await.remove(&fid);
        }
        Ok(())
    }
}

/// A tiny read-only server with one file at `value`.
struct StaticFileFs {
    content: &'static [u8],
    fids: tokio::sync::Mutex<HashMap<Fid, bool>>, // fid -> is `value`
}

#[async_trait::async_trait]
impl FileServer for StaticFileFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let (is_value, kind) = match names {
            [] => (false, FileKind::Dir),
            [name] if name == "value" => (true, FileKind::File),
            _ => return Err(ErrorCode::NotFound),
        };
        self.fids.lock().await.insert(newfid, is_value);
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) {
            return Err(ErrorCode::NoAccess);
        }
        let kind = if *self.fids.lock().await.get(&fid).unwrap_or(&false) {
            FileKind::File
        } else {
            FileKind::Dir
        };
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        if !*self.fids.lock().await.get(&fid).unwrap_or(&false) {
            return Ok(b"value".to_vec());
        }
        let start = (offset as usize).min(self.content.len());
        let end = self.content.len().min(start + count as usize);
        Ok(self.content[start..end].to_vec())
    }

    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: FileKind::File,
                version: 0,
                path: 0,
            },
            length: self.content.len() as u64,
            executable: false,
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
async fn create_delegates_to_the_backing_tree_and_binds_newfid() {
    let mut ns = Namespace::new();
    ns.mount("/create", createfs(), Access::ReadWrite);
    let fs = MountFs::new(ns);
    fs.walk(Fid::ROOT, Fid(1), &["create".into()])
        .await
        .unwrap();
    let qid = fs
        .create(Fid(1), Fid(2), "created", FileKind::File)
        .await
        .unwrap();
    assert_eq!(qid.kind, FileKind::File);
    fs.open(Fid(2), OpenMode::Write).await.unwrap();
    fs.write(Fid(2), 0, b"created through mount").await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();

    let entries = read_lines(&fs, &["create"], Fid(3)).await;
    assert!(
        entries.iter().any(|entry| entry == "created"),
        "{entries:?}"
    );
    fs.walk(Fid::ROOT, Fid(4), &["create".into(), "created".into()])
        .await
        .unwrap();
    fs.open(Fid(4), OpenMode::Read).await.unwrap();
    assert_eq!(
        fs.read(Fid(4), 0, 64).await.unwrap(),
        b"created through mount"
    );
}

#[tokio::test]
async fn create_reserves_newfid_before_forwarding_to_the_backing_tree() {
    let gate = Arc::new(Notify::new());
    let started = Arc::new(AtomicUsize::new(0));
    let backing = Arc::new(CreateFs::gated(gate.clone(), started.clone()));
    let mut ns = Namespace::new();
    ns.mount(
        "/create",
        InProcessTransport::new(backing),
        Access::ReadWrite,
    );
    let fs = Arc::new(MountFs::new(ns));
    fs.walk(Fid::ROOT, Fid(1), &["create".into()])
        .await
        .unwrap();

    let first = {
        let fs = fs.clone();
        tokio::spawn(async move { fs.create(Fid(1), Fid(2), "first", FileKind::File).await })
    };
    while started.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }

    assert_eq!(
        fs.create(Fid(1), Fid(2), "second", FileKind::File).await,
        Err(ErrorCode::BadRequest),
        "the caller-visible newfid is reserved before backing create runs"
    );
    assert_eq!(
        started.load(Ordering::SeqCst),
        1,
        "the failed create must not reach the backing file server"
    );

    gate.notify_waiters();
    first.await.unwrap().unwrap();
    let entries = read_lines(&fs, &["create"], Fid(3)).await;
    assert!(entries.iter().any(|entry| entry == "first"), "{entries:?}");
    assert!(
        !entries.iter().any(|entry| entry == "second"),
        "the failed create must not leave a backing child behind: {entries:?}"
    );
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
async fn a_read_only_mount_denies_create() {
    let mut ns = Namespace::new();
    ns.mount("/ro", createfs(), Access::ReadOnly);
    let fs = MountFs::new(ns);
    fs.walk(Fid::ROOT, Fid(1), &["ro".into()]).await.unwrap();
    assert_eq!(
        fs.create(Fid(1), Fid(2), "created", FileKind::File).await,
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
async fn live_namespace_mount_is_visible_to_future_walks() {
    let live = LiveNamespace::new(Namespace::new());
    let fs = MountFs::from_live_namespace(live.clone());

    assert_eq!(
        fs.walk(Fid::ROOT, Fid(1), &["mnt".into(), "project".into()])
            .await,
        Err(ErrorCode::NotFound)
    );

    live.mount("/mnt/project", static_filefs(b"mounted"), Access::ReadWrite);

    assert_eq!(
        read_file(&fs, &["mnt", "project", "value"], Fid(2)).await,
        b"mounted"
    );
    assert_eq!(read_lines(&fs, &["mnt"], Fid(3)).await, vec!["project"]);
}

#[tokio::test]
async fn live_namespace_mutation_bumps_synthetic_directory_qids() {
    let live = LiveNamespace::new(Namespace::new());
    let fs = MountFs::from_live_namespace(live.clone());

    live.mount("/mnt/old", static_filefs(b"old"), Access::ReadWrite);
    let before_walk = fs.walk(Fid::ROOT, Fid(1), &["mnt".into()]).await.unwrap();
    let before_stat = fs.stat(Fid(1)).await.unwrap();
    assert_eq!(before_walk.version, live.generation());
    assert_eq!(before_stat.qid.version, live.generation());

    live.mount("/mnt/project", static_filefs(b"new"), Access::ReadWrite);

    let after_stat = fs.stat(Fid(1)).await.unwrap();
    assert_ne!(after_stat.qid.version, before_stat.qid.version);
    assert_eq!(after_stat.qid.version, live.generation());
    let mut entries = read_lines(&fs, &["mnt"], Fid(2)).await;
    entries.sort();
    assert_eq!(entries, vec!["old", "project"]);
}

#[tokio::test]
async fn live_namespace_replace_mount_does_not_accumulate_duplicate_descriptions() {
    let live = LiveNamespace::new(Namespace::new());
    let fs = MountFs::from_live_namespace(live.clone());

    live.replace_mount("/mnt/project", static_filefs(b"old"), Access::ReadWrite);
    live.replace_mount("/mnt/project", static_filefs(b"new"), Access::ReadOnly);

    assert_eq!(
        live.describe(),
        vec![("/mnt/project".to_string(), Access::ReadOnly)]
    );
    assert_eq!(
        read_file(&fs, &["mnt", "project", "value"], Fid(1)).await,
        b"new"
    );
    fs.walk(
        Fid::ROOT,
        Fid(2),
        &["mnt".into(), "project".into(), "value".into()],
    )
    .await
    .unwrap();
    assert_eq!(
        fs.open(Fid(2), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
}

#[tokio::test]
async fn live_namespace_replacement_preserves_already_open_fids() {
    let live = LiveNamespace::new(Namespace::new());
    let fs = MountFs::from_live_namespace(live.clone());

    live.replace_mount("/mnt/project", static_filefs(b"old"), Access::ReadWrite);
    fs.walk(
        Fid::ROOT,
        Fid(1),
        &["mnt".into(), "project".into(), "value".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();

    live.replace_mount("/mnt/project", static_filefs(b"new"), Access::ReadWrite);

    assert_eq!(fs.read(Fid(1), 0, 4096).await.unwrap(), b"old");
    assert_eq!(
        read_file(&fs, &["mnt", "project", "value"], Fid(2)).await,
        b"new"
    );
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
            executable: false,
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

/// A read-only directory server whose root lists a fixed set of file names, so a
/// union of two of them (with different names) proves listing merge.
struct DirFs {
    entries: Vec<String>,
    fids: tokio::sync::Mutex<HashMap<Fid, bool>>, // fid -> is the root dir
}

impl DirFs {
    fn transport(entries: &[&str]) -> InProcessTransport {
        InProcessTransport::new(Arc::new(DirFs {
            entries: entries.iter().map(|s| s.to_string()).collect(),
            fids: tokio::sync::Mutex::new(HashMap::new()),
        }))
    }
}

#[async_trait::async_trait]
impl FileServer for DirFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let (is_root, kind) = match names {
            [] => (true, FileKind::Dir),
            [n] if self.entries.contains(n) => (false, FileKind::File),
            _ => return Err(ErrorCode::NotFound),
        };
        self.fids.lock().await.insert(newfid, is_root);
        Ok(Qid {
            kind,
            version: 0,
            path: 0,
        })
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0,
        })
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let is_root = *self.fids.lock().await.get(&fid).unwrap_or(&false);
        let bytes = if is_root {
            self.entries.join("\n").into_bytes()
        } else {
            Vec::new()
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }
    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            },
            length: 0,
            executable: false,
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
async fn a_union_mount_lists_all_contributors_merged() {
    // Two servers mounted at the same prefix (a union, as `/bin` is assembled by
    // bind). Listing must merge both contributors' entries, deduplicated.
    let mut ns = Namespace::new();
    ns.mount("/bin", DirFs::transport(&["ls", "cat"]), Access::ReadWrite);
    ns.mount(
        "/bin",
        DirFs::transport(&["grep", "cat"]),
        Access::ReadWrite,
    );
    let fs = MountFs::new(ns);

    let mut entries = read_lines(&fs, &["bin"], Fid(1)).await;
    entries.sort();
    assert_eq!(entries, vec!["cat", "grep", "ls"]);
}

/// A directory server whose reads fail with `Io`, to prove a union merge surfaces
/// a contributor's operational failure instead of masking it as a partial listing.
struct FailReadDirFs;

#[async_trait::async_trait]
impl FileServer for FailReadDirFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names {
            [] => Ok(Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            }),
            _ => Err(ErrorCode::NotFound),
        }
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0,
        })
    }
    async fn read(&self, _: Fid, _: Offset, _: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Io)
    }
    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 0,
            },
            length: 0,
            executable: false,
            writable: false,
        })
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

#[tokio::test]
async fn a_union_listing_surfaces_a_contributor_failure() {
    // One contributor's read fails operationally (Io). The merge must surface it,
    // not silently return only the other contributor's entries.
    let mut ns = Namespace::new();
    ns.mount("/bin", DirFs::transport(&["ls"]), Access::ReadWrite);
    ns.mount(
        "/bin",
        InProcessTransport::new(Arc::new(FailReadDirFs)),
        Access::ReadWrite,
    );
    let fs = MountFs::new(ns);

    fs.walk(Fid::ROOT, Fid(1), &["bin".into()]).await.unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(1), 0, 4096).await, Err(ErrorCode::Io));
}

#[tokio::test]
async fn a_relative_walk_from_a_backing_file_is_rejected() {
    // `/data/greeting` is a file. A non-empty walk descending from it must be
    // NotDirectory, not a re-resolution that could traverse a non-directory.
    let fs = MountFs::new(ns());
    fs.walk(Fid::ROOT, Fid(1), &["data".into(), "greeting".into()])
        .await
        .unwrap();
    assert_eq!(
        fs.walk(Fid(1), Fid(2), &["child".into()]).await,
        Err(ErrorCode::NotDirectory)
    );
}

#[tokio::test]
async fn a_backed_directory_lists_its_mount_point_children() {
    // A broad `/` mount (MemFs: greeting/clone/submit) plus a deeper `/mnt/llm`.
    // Listing `/` must show the backing entries AND the `mnt` mount point, or the
    // deeper mount is reachable but invisible in its parent's listing.
    let mut ns = Namespace::new();
    ns.mount("/", memfs(), Access::ReadWrite);
    ns.mount("/mnt/llm", memfs(), Access::ReadWrite);
    let fs = MountFs::new(ns);

    let mut entries = read_lines(&fs, &[], Fid(1)).await;
    entries.sort();
    assert_eq!(entries, vec!["clone", "greeting", "mnt", "submit"]);
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

/// A backing server whose `walk` blocks until `gate` is notified. Every other
/// method is trivial. Used to prove `MountFs::walk` does not hold the namespace
/// lock across the forwarded backing walk.
struct GatedFs {
    gate: Arc<Notify>,
}

#[async_trait::async_trait]
impl FileServer for GatedFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, _names: &[String]) -> Result<Qid, ErrorCode> {
        // Block inside the forwarded walk until the test releases the gate.
        self.gate.notified().await;
        Ok(Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0,
        })
    }
    async fn open(&self, _: Fid, _: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0,
        })
    }
    async fn read(&self, _: Fid, _: Offset, _: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }
    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }
    async fn stat(&self, _: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, _: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

/// Regression: a walk whose backing tree blocks must NOT hold the namespace
/// lock, so a concurrent MountFs operation can still make progress. On the old
/// code (walk held the state lock across the forwarded call) the concurrent
/// walk deadlocked on the lock and this test timed out.
#[tokio::test]
async fn walk_does_not_hold_the_namespace_lock_across_the_backing_walk() {
    let gate = Arc::new(Notify::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/gated",
        InProcessTransport::new(Arc::new(GatedFs { gate: gate.clone() })),
        Access::ReadWrite,
    );
    ns.mount("/data", memfs(), Access::ReadWrite);
    let fs = Arc::new(MountFs::new(ns));

    // Task 1 walks into the gated tree; its forwarded backing walk blocks.
    let gated = {
        let fs = fs.clone();
        tokio::spawn(async move { fs.walk(Fid::ROOT, Fid(1), &["gated".into()]).await })
    };
    // Give task 1 a chance to enter the forwarded (blocked) backing walk.
    tokio::task::yield_now().await;

    // A concurrent MountFs walk needs the namespace lock. If task 1 held it
    // across the blocked backing walk, this would deadlock. It must complete;
    // then we release the gate so task 1 finishes too.
    let concurrent = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        fs.walk(Fid::ROOT, Fid(2), &["data".into()]),
    )
    .await
    .expect("concurrent walk must not be blocked by the gated walk")
    .expect("walk /data");
    assert_eq!(concurrent.kind, FileKind::Dir);

    gate.notify_one();
    let gated = tokio::time::timeout(std::time::Duration::from_secs(5), gated)
        .await
        .expect("gated walk should finish after the gate is released")
        .expect("join gated task")
        .expect("gated walk");
    assert_eq!(gated.kind, FileKind::Dir);
}
