//! Alan Shell builtins over aP (introduce-alan-shell §3, §5.1). The shell is an
//! aP-only client: every builtin is generic file IO (walk/open/read/write/clunk
//! and clone-via-open for spawn) with no agent-specific command. These tests run
//! the builtins against an in-memory echo file server (the M1 milestone — input
//! echoed back through files, no LLM), against a partial-write server (short-write
//! handling), and against a real assembled namespace (`MountFs` over `/proc` and a
//! data mount) so path resolution crosses mounts as it will in production.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Request,
    Response, Stat, Stream,
};
use alan_kernel::{
    Access, Credentials, MountFs, Namespace, ProcFs, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_shell::{Shell, StdioDriver};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

static NEXT_TEST_FID: AtomicU64 = AtomicU64::new(500_000);

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
            executable: false,
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

struct CloseTrackingStreamFs {
    stream: Stream,
    fids: tokio::sync::Mutex<HashMap<Fid, bool>>,
    opens: Arc<AtomicU64>,
    clunks: Arc<AtomicU64>,
}

impl CloseTrackingStreamFs {
    fn new(opens: Arc<AtomicU64>, clunks: Arc<AtomicU64>) -> Self {
        Self {
            stream: Stream::new(),
            fids: tokio::sync::Mutex::new(HashMap::new()),
            opens,
            clunks,
        }
    }
}

#[async_trait::async_trait]
impl FileServer for CloseTrackingStreamFs {
    async fn walk(&self, _fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        match names {
            [] => {
                self.fids.lock().await.insert(newfid, false);
                Ok(qid(FileKind::Dir))
            }
            [name] if name == "stream" => {
                self.fids.lock().await.insert(newfid, true);
                Ok(qid(FileKind::Stream))
            }
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let is_stream = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if is_stream {
            self.opens.fetch_add(1, Ordering::Relaxed);
            Ok(qid(FileKind::Stream))
        } else {
            Ok(qid(FileKind::Dir))
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let is_stream = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        if is_stream {
            Ok(self.stream.read(offset, count).await)
        } else {
            Ok(b"stream".to_vec())
        }
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let is_stream = *self
            .fids
            .lock()
            .await
            .get(&fid)
            .ok_or(ErrorCode::NotFound)?;
        Ok(Stat {
            name: String::new(),
            qid: qid(if is_stream {
                FileKind::Stream
            } else {
                FileKind::Dir
            }),
            length: if is_stream {
                self.stream.len().await
            } else {
                b"stream".len() as u64
            },
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
        if self.fids.lock().await.remove(&fid).is_some() {
            self.clunks.fetch_add(1, Ordering::Relaxed);
        }
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

#[tokio::test]
async fn stdio_driver_runs_line_commands() {
    let shell = Shell::new(EchoFs::transport());
    let output = run_stdio_script(
        shell,
        b"ls /\necho hello from stdio > /buf\ncat /buf\nexit\n",
    )
    .await;

    assert!(
        output.contains("buf\n"),
        "ls output should include buf: {output:?}"
    );
    assert!(
        output.contains("stream\n"),
        "ls output should include stream: {output:?}"
    );
    assert!(
        output.contains("hello from stdio"),
        "cat should print data written by echo: {output:?}"
    );
}

#[derive(Clone)]
struct ArgvRunner;

#[async_trait::async_trait]
impl ProcessRunner for ArgvRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        if invocation.exec.executable != "/bin/argv" {
            return ProcessOutcome::exited(127, b"wrong executable\n");
        }
        ProcessOutcome::exited(0, format!("{}\n", invocation.exec.args.join("|")))
    }
}

fn command_shell() -> Shell {
    let procfs = ProcFs::new().with_runner(Arc::new(ArgvRunner));
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/argv",
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadOnly,
    );
    let spawner = procfs.for_spawner(None, namespace.clone(), Credentials::user("shell-test"));
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(spawner)),
        Access::ReadWrite,
    );
    Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))))
}

#[tokio::test]
async fn generic_command_execution_collects_proc_output_and_exit() {
    let shell = command_shell();
    let result = shell
        .run("/bin/argv", &["first".to_string(), "two words".to_string()])
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, b"first|two words\n");
    assert_eq!(
        shell
            .cat(&format!("/proc/{}/status", result.pid))
            .await
            .unwrap(),
        b"exited\n"
    );
}

#[tokio::test]
async fn stdio_driver_parses_quoted_argv_for_generic_bin_commands() {
    let output = run_stdio_script(
        command_shell(),
        b"argv first 'two words' \"three words\"\nexit\n",
    )
    .await;
    assert!(
        output.contains("first|two words|three words\n"),
        "{output:?}"
    );
}

#[tokio::test]
async fn stdio_driver_tails_stream_while_accepting_input() {
    let shell = Shell::new(EchoFs::transport());
    let driver = StdioDriver::new(shell);
    let (mut client, server) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server);

    let driver_task = tokio::spawn(async move {
        driver
            .run(BufReader::new(server_read), server_write)
            .await
            .expect("stdio driver should run");
    });

    client.write_all(b"tail /stream\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    client
        .write_all(b"echo streamed while typing > /stream\n")
        .await
        .unwrap();

    let output = read_until(&mut client, "streamed while typing").await;
    assert!(
        output.contains("streamed while typing"),
        "tail output should print while the driver still accepts input: {output:?}"
    );

    client.write_all(b"exit\n").await.unwrap();
    driver_task.await.unwrap();
}

#[tokio::test]
async fn stdio_driver_closes_tail_fids_on_exit() {
    let opens = Arc::new(AtomicU64::new(0));
    let clunks = Arc::new(AtomicU64::new(0));
    let fs = Arc::new(CloseTrackingStreamFs::new(
        Arc::clone(&opens),
        Arc::clone(&clunks),
    ));
    let shell = Shell::new(InProcessTransport::new(fs));
    let driver = StdioDriver::new(shell);
    let (mut client, server) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server);

    let driver_task = tokio::spawn(async move {
        driver
            .run(BufReader::new(server_read), server_write)
            .await
            .expect("stdio driver should run");
    });

    client.write_all(b"tail /stream\n").await.unwrap();
    for _ in 0..50 {
        if opens.load(Ordering::Relaxed) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(opens.load(Ordering::Relaxed), 1, "tail fid opened");

    client.write_all(b"exit\n").await.unwrap();
    driver_task.await.unwrap();
    assert_eq!(clunks.load(Ordering::Relaxed), 1, "tail fid was closed");
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
            executable: false,
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
            executable: false,
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
            executable: false,
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

#[tokio::test]
async fn m2_stdio_driver_talks_to_agentfs_llmfs_agent_with_generic_builtins() {
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    let mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: "north star response".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: Some("stop".to_string()),
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let recorded = mock.clone();
    llmfs.register_connection("default", Box::new(mock));

    let procfs = Arc::new(ProcFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(procfs.clone()),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/root",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();
    assert_eq!(
        shell.cat(&format!("/proc/{pid}/status")).await.unwrap(),
        b"running\n"
    );

    let agent_root_path = "/agent/root".to_string();
    let agent_task = tokio::spawn(run_one_file_agent_turn(
        root.clone(),
        agent_root_path.clone(),
    ));

    let driver = StdioDriver::new(shell);
    let (mut client, server) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server);
    let driver_task = tokio::spawn(async move {
        driver
            .run(BufReader::new(server_read), server_write)
            .await
            .expect("stdio driver should run");
    });

    client
        .write_all(format!("tail {agent_root_path}/io/output\n").as_bytes())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    client
        .write_all(format!("echo hello through files > {agent_root_path}/io/input\n").as_bytes())
        .await
        .unwrap();

    let output = read_until(&mut client, "north star response").await;
    assert!(
        output.contains("north star response"),
        "agent response should stream back through shell tail: {output:?}"
    );

    client.write_all(b"exit\n").await.unwrap();
    driver_task.await.unwrap();
    agent_task.await.unwrap();

    let requests = recorded.recorded_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].messages[0].content, "hello through files");
}

async fn run_one_file_agent_turn(root: InProcessTransport, agent_root_path: String) {
    let shell = Shell::new(root.clone());
    let mut input = shell
        .tail(&format!("{agent_root_path}/io/input"))
        .await
        .expect("agent should tail io/input");
    let frame = input.read(4096).await.expect("read input frame");
    input.close().await.expect("close input tail");
    let message = parse_agent_input_frame(&frame);
    let response = generate_once(root, &message).await;
    shell
        .write(&format!("{agent_root_path}/io/output"), response.as_bytes())
        .await
        .expect("agent should write io/output");
}

async fn generate_once(root: InProcessTransport, message: &str) -> String {
    let shell = Shell::new(root.clone());
    let gen_id = open_llm_generation(&root, "/mnt/llm/connections/default/clone").await;
    let data_path = format!("/mnt/llm/connections/default/{gen_id}/data");
    let request = serde_json::json!({
        "version": 1,
        "messages": [
            {"role": "user", "content": message}
        ],
        "tools": []
    });
    shell
        .write(&data_path, request.to_string().as_bytes())
        .await
        .expect("commit llm request");

    let events_path = format!("/mnt/llm/connections/default/{gen_id}/events");
    let mut events = shell.tail(&events_path).await.expect("tail llm events");
    let mut pending = String::new();
    let mut response = String::new();
    loop {
        let bytes = events.read(4096).await.expect("read llm event");
        if bytes.is_empty() {
            break;
        }
        pending.push_str(std::str::from_utf8(&bytes).expect("llm events are utf8"));
        while let Some(newline) = pending.find('\n') {
            let line = pending[..newline].to_string();
            pending.drain(..=newline);
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(&line).expect("json event");
            if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                response.push_str(text);
            }
            if value.get("done").and_then(|v| v.as_bool()) == Some(true) {
                events.close().await.expect("close llm events");
                return response;
            }
        }
    }
    events.close().await.expect("close llm events");
    response
}

async fn open_llm_generation(root: &InProcessTransport, path: &str) -> String {
    let fid = Fid(NEXT_TEST_FID.fetch_add(1, Ordering::Relaxed));
    root.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: fid,
        names: path_names(path),
    })
    .await
    .expect("walk llm clone");
    root.call(Request::Open {
        fid,
        mode: OpenMode::ReadWrite,
    })
    .await
    .expect("open llm clone");
    let gen_id = match root
        .call(Request::Read {
            fid,
            offset: 0,
            count: 64,
        })
        .await
        .expect("read llm generation id")
    {
        Response::Read { data } => String::from_utf8(data).expect("generation id utf8"),
        other => panic!("unexpected llm clone read response: {other:?}"),
    };
    root.call(Request::Clunk { fid })
        .await
        .expect("clunk llm clone");
    gen_id
}

fn parse_agent_input_frame(frame: &[u8]) -> String {
    let newline = frame
        .iter()
        .position(|b| *b == b'\n')
        .expect("framed input has length prefix");
    let len: usize = std::str::from_utf8(&frame[..newline])
        .expect("length prefix utf8")
        .parse()
        .expect("length prefix parses");
    let start = newline + 1;
    let end = start + len;
    String::from_utf8(frame[start..end].to_vec()).expect("message utf8")
}

fn path_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

async fn run_stdio_script(shell: Shell, script: &[u8]) -> String {
    let driver = StdioDriver::new(shell);
    let (client, server) = tokio::io::duplex(4096);
    let (mut client_read, mut client_write) = tokio::io::split(client);
    let (server_read, server_write) = tokio::io::split(server);
    let driver_task = tokio::spawn(async move {
        driver
            .run(BufReader::new(server_read), server_write)
            .await
            .expect("stdio driver should run");
    });
    client_write.write_all(script).await.unwrap();
    drop(client_write);
    let mut out = Vec::new();
    client_read.read_to_end(&mut out).await.unwrap();
    driver_task.await.unwrap();
    String::from_utf8(out).unwrap()
}

async fn read_until<R>(reader: &mut R, needle: &str) -> String
where
    R: AsyncRead + Unpin,
{
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut out = Vec::new();
        let mut buf = [0; 256];
        loop {
            let n = reader.read(&mut buf).await.expect("read driver output");
            assert!(n > 0, "driver output closed before {needle:?}");
            out.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&out).to_string();
            if text.contains(needle) {
                return text;
            }
        }
    })
    .await
    .expect("timed out reading driver output")
}
