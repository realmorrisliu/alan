//! Alan Shell builtins over aP (introduce-alan-shell §3, §5.1). The shell is an
//! aP-only client: every builtin is generic file IO (walk/open/read/write/clunk
//! and clone-via-open for spawn) with no agent-specific command. These tests run
//! the builtins against an in-memory echo file server (the M1 milestone — input
//! echoed back through files, no LLM), against a partial-write server (short-write
//! handling), and against a real assembled namespace (`MountFs` over `/proc` and a
//! data mount) so path resolution crosses mounts as it will in production.

use std::collections::HashMap;
use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;

/// A tiny read-write echo server: a `buf` byte file and a `stream` stream file
/// under a directory root. Each node reports its true [`FileKind`], so the shell's
/// dir/stream-aware builtins behave as they would against a real server.
struct EchoFs {
    state: tokio::sync::Mutex<EchoState>,
}

struct EchoState {
    buf: Vec<u8>,
    stream: Stream,
    fids: HashMap<Fid, EchoNode>,
}

#[derive(Clone, Copy)]
enum EchoNode {
    Root,
    Buf,
    StreamFile,
}

impl EchoNode {
    fn kind(self) -> FileKind {
        match self {
            EchoNode::Root => FileKind::Dir,
            EchoNode::Buf => FileKind::File,
            EchoNode::StreamFile => FileKind::Stream,
        }
    }
}

impl EchoFs {
    fn new() -> Self {
        Self {
            state: tokio::sync::Mutex::new(EchoState {
                buf: Vec::new(),
                stream: Stream::new(),
                fids: HashMap::new(),
            }),
        }
    }
    fn transport() -> InProcessTransport {
        InProcessTransport::new(Arc::new(Self::new()))
    }
}

fn qid(kind: FileKind) -> Qid {
    Qid {
        kind,
        version: 0,
        path: 0,
    }
}

#[async_trait::async_trait]
impl FileServer for EchoFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let node = match names {
            [] => EchoNode::Root,
            [n] if n == "buf" => EchoNode::Buf,
            [n] if n == "stream" => EchoNode::StreamFile,
            _ => return Err(ErrorCode::NotFound),
        };
        self.state.lock().await.fids.insert(newfid, node);
        Ok(qid(node.kind()))
    }
    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = *self
            .state
            .lock()
            .await
            .fids
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        Ok(qid(node.kind()))
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let node = *self
            .state
            .lock()
            .await
            .fids
            .get(&fid)
            .unwrap_or(&EchoNode::Root);
        let bytes = match node {
            EchoNode::Root => b"buf\nstream".to_vec(),
            EchoNode::Buf => self.state.lock().await.buf.clone(),
            EchoNode::StreamFile => {
                let stream = self.state.lock().await.stream.clone();
                return Ok(stream.read(offset, count).await);
            }
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }
    async fn write(&self, fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        match state.fids.get(&fid).copied() {
            Some(EchoNode::Buf) => {
                state.buf.extend_from_slice(data);
                Ok(data.len() as u32)
            }
            Some(EchoNode::StreamFile) => {
                let stream = state.stream.clone();
                drop(state);
                stream.append(data).await;
                Ok(data.len() as u32)
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }
    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = *state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let length = match node {
            EchoNode::Root => b"buf\nstream".len() as u64,
            EchoNode::Buf => state.buf.len() as u64,
            EchoNode::StreamFile => state.stream.len().await,
        };
        Ok(Stat {
            name: String::new(),
            qid: qid(node.kind()),
            length,
            writable: true,
        })
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.state.lock().await.fids.remove(&fid);
        Ok(())
    }
}

#[tokio::test]
async fn m1_input_is_echoed_back_through_files() {
    // M1: write into a file, read it back — the whole round trip is files.
    let shell = Shell::new(EchoFs::transport());
    shell.write("/buf", b"hello shell").await.unwrap();
    assert_eq!(shell.cat("/buf").await.unwrap(), b"hello shell");
}

#[tokio::test]
async fn ls_lists_a_directory() {
    let shell = Shell::new(EchoFs::transport());
    assert_eq!(
        shell.ls("/").await.unwrap(),
        vec!["buf".to_string(), "stream".to_string()]
    );
}

#[tokio::test]
async fn ls_rejects_a_non_directory() {
    // Pointed at a regular file, `ls` fails rather than reporting the file's bytes
    // as directory entries.
    let shell = Shell::new(EchoFs::transport());
    assert_eq!(shell.ls("/buf").await, Err(ErrorCode::NotDirectory));
}

#[tokio::test]
async fn cat_snapshots_a_stream_without_blocking_at_the_live_edge() {
    // A stream with retained bytes: `cat` returns the snapshot and does not block
    // waiting for more (that is `tail`'s job).
    let shell = Shell::new(EchoFs::transport());
    shell.write("/stream", b"retained").await.unwrap();
    let snapshot =
        tokio::time::timeout(std::time::Duration::from_millis(500), shell.cat("/stream"))
            .await
            .expect("cat must not block on a stream")
            .unwrap();
    assert_eq!(snapshot, b"retained");
}

#[tokio::test]
async fn tail_follows_multiple_appends() {
    use std::time::Duration;

    let transport = EchoFs::transport();
    let shell = Arc::new(Shell::new(transport));

    // A tail session blocks at the live edge until bytes arrive, then keeps reading
    // subsequent appends from the advancing offset.
    let watcher = shell.clone();
    let handle = tokio::spawn(async move {
        let mut tail = watcher.tail("/stream").await.unwrap();
        let first = tail.read(65536).await.unwrap();
        let second = tail.read(65536).await.unwrap();
        tail.close().await.unwrap();
        (first, second)
    });

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!handle.is_finished(), "tail should block at the live edge");

    shell.write("/stream", b"first").await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    shell.write("/stream", b"second").await.unwrap();

    let (first, second) = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first, b"first");
    assert_eq!(
        second, b"second",
        "the tail keeps reading past the first chunk"
    );
}

/// A server whose writable `buf` file accepts at most 3 bytes per `write`, to
/// exercise the shell's short-write loop.
struct ChunkFs {
    state: tokio::sync::Mutex<ChunkState>,
}
struct ChunkState {
    buf: Vec<u8>,
    fids: HashMap<Fid, bool>, // fid -> is the `buf` file (vs root dir)
}

#[async_trait::async_trait]
impl FileServer for ChunkFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let (is_file, kind) = match names {
            [] => (false, FileKind::Dir),
            [n] if n == "buf" => (true, FileKind::File),
            _ => return Err(ErrorCode::NotFound),
        };
        self.state.lock().await.fids.insert(newfid, is_file);
        Ok(qid(kind))
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(qid(FileKind::File))
    }
    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        if *state.fids.get(&fid).unwrap_or(&false) {
            let bytes = &state.buf;
            let start = (offset as usize).min(bytes.len());
            let end = bytes.len().min(start + count as usize);
            Ok(bytes[start..end].to_vec())
        } else {
            Err(ErrorCode::Unsupported)
        }
    }
    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        if !*state.fids.get(&fid).unwrap_or(&false) {
            return Err(ErrorCode::Unsupported);
        }
        // Accept at most 3 bytes per call (a legal short write), honoring offset.
        let n = data.len().min(3);
        let start = offset as usize;
        let end = start + n;
        if state.buf.len() < end {
            state.buf.resize(end, 0);
        }
        state.buf[start..end].copy_from_slice(&data[..n]);
        Ok(n as u32)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: qid(FileKind::File),
            length: self.state.lock().await.buf.len() as u64,
            writable: true,
        })
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.state.lock().await.fids.remove(&fid);
        Ok(())
    }
}

/// A server whose `buf` file over-reports its write count (accepted > offered),
/// to prove the shell rejects it instead of panicking on an out-of-range slice.
struct LiarFs;

#[async_trait::async_trait]
impl FileServer for LiarFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names {
            [n] if n == "buf" => Ok(qid(FileKind::File)),
            _ => Err(ErrorCode::NotFound),
        }
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(qid(FileKind::File))
    }
    async fn read(&self, _: Fid, _: Offset, _: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }
    async fn write(&self, _fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        // Claim one more byte than was offered — an illegal, buffer-overrunning count.
        Ok(data.len() as u32 + 1)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: qid(FileKind::File),
            length: 0,
            writable: true,
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

/// A server whose `buf` file rejects every write, so a test can prove a write was
/// actually issued (an empty document must still reach the server).
struct RejectWriteFs;

#[async_trait::async_trait]
impl FileServer for RejectWriteFs {
    async fn walk(&self, _fid: Fid, _newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names {
            [n] if n == "buf" => Ok(qid(FileKind::File)),
            _ => Err(ErrorCode::NotFound),
        }
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(qid(FileKind::File))
    }
    async fn read(&self, _: Fid, _: Offset, _: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }
    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::BadRequest)
    }
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: qid(FileKind::File),
            length: 0,
            writable: true,
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
async fn write_of_empty_document_still_issues_a_write() {
    // The server rejects every write. If the shell silently skipped the empty write
    // it would return Ok (clunk succeeds); instead it must surface the rejection,
    // proving an empty document reaches the server rather than being dropped.
    let shell = Shell::new(InProcessTransport::new(Arc::new(RejectWriteFs)));
    assert_eq!(shell.write("/buf", b"").await, Err(ErrorCode::BadRequest));
}

#[tokio::test]
async fn write_rejects_an_over_reported_count() {
    let shell = Shell::new(InProcessTransport::new(Arc::new(LiarFs)));
    // The server claims to accept more than offered; the shell must error, not panic.
    assert_eq!(shell.write("/buf", b"abc").await, Err(ErrorCode::Io));
}

#[tokio::test]
async fn write_handles_short_writes() {
    let fs = InProcessTransport::new(Arc::new(ChunkFs {
        state: tokio::sync::Mutex::new(ChunkState {
            buf: Vec::new(),
            fids: HashMap::new(),
        }),
    }));
    let shell = Shell::new(fs);
    // 8 bytes with a 3-byte-per-write cap: the shell must loop until all land.
    shell.write("/buf", b"abcdefgh").await.unwrap();
    assert_eq!(shell.cat("/buf").await.unwrap(), b"abcdefgh");
}

/// A real assembled namespace: `/proc` (ProcFs) and `/data` (MemFs) unioned by
/// `MountFs` and presented to the shell as one transport.
fn namespace_shell() -> Shell {
    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(ProcFs::new())),
        Access::ReadWrite,
    );
    ns.mount(
        "/data",
        InProcessTransport::new(Arc::new(MemFs::new())),
        Access::ReadWrite,
    );
    Shell::new(InProcessTransport::new(Arc::new(MountFs::new(ns))))
}

#[tokio::test]
async fn shell_resolves_paths_across_a_real_namespace() {
    let shell = namespace_shell();
    // `cat` reaches a file under a mount.
    assert_eq!(shell.cat("/data/greeting").await.unwrap(), b"hi");
    // `ls /` lists the namespace's mount points (a synthetic directory).
    let mut root = shell.ls("/").await.unwrap();
    root.sort();
    assert_eq!(root, vec!["data".to_string(), "proc".to_string()]);
}

#[tokio::test]
async fn write_surfaces_a_commit_on_clunk_rejection() {
    // MemFs's `/submit` validates its document at clunk; a non-JSON body is
    // rejected at commit, and the shell must surface that, not report success.
    let shell = namespace_shell();
    assert_eq!(
        shell.write("/data/submit", b"not json").await,
        Err(ErrorCode::BadRequest)
    );
}

#[tokio::test]
async fn spawn_surfaces_a_malformed_exec_spec() {
    // /proc/clone commits the process at clunk; a malformed exec spec is discarded
    // with a commit-time error, which spawn must propagate instead of a bogus pid.
    let shell = namespace_shell();
    assert_eq!(shell.spawn("not json").await, Err(ErrorCode::BadRequest));
}

#[tokio::test]
async fn spawn_launches_a_process_through_proc_clone_across_the_mount() {
    // The aP-only shell launches a process through `/proc/clone` (a mount in the
    // namespace) with no side API, then reads its status back across the mount.
    let shell = namespace_shell();
    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(
        shell.cat(&format!("/proc/{pid}/status")).await.unwrap(),
        b"running\n"
    );
}
