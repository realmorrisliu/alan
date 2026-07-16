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
