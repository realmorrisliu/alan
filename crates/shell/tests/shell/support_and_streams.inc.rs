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
    Access, Credentials, MountFs, Namespace, Pid, ProcFs, ProcessInvocation, ProcessOutcome,
    ProcessRunner,
};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_shell::{BoundedListError, Shell, StdioDriver};
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
async fn bounded_ls_limits_entries_and_encoded_bytes() {
    let shell = Shell::new(EchoFs::transport());
    assert_eq!(
        shell.ls_bounded("/", 1, 1024).await,
        Err(BoundedListError::LimitExceeded)
    );
    assert_eq!(
        shell.ls_bounded("/", 2, 3).await,
        Err(BoundedListError::LimitExceeded)
    );
    assert_eq!(
        shell.ls_bounded("/", 2, 10).await.unwrap(),
        vec!["buf".to_string(), "stream".to_string()]
    );
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

async fn command_shell() -> Shell {
    let procfs = ProcFs::new();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/argv",
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadOnly,
    );
    let bootstrap = procfs.for_spawner(
        None,
        namespace.clone(),
        Credentials::user("shell-test"),
    );
    bootstrap
        .walk(Fid::ROOT, Fid(499_000), &["clone".to_string()])
        .await
        .unwrap();
    bootstrap
        .open(Fid(499_000), OpenMode::ReadWrite)
        .await
        .unwrap();
    let parent_pid = String::from_utf8(bootstrap.read(Fid(499_000), 0, 64).await.unwrap())
        .unwrap()
        .parse::<u64>()
        .unwrap();
    bootstrap
        .write(
            Fid(499_000),
            0,
            br#"{"executable":"/bin/argv","args":[],"namespace":{"mounts":[{"path":"/bin/argv","access":"ro"}]}}"#,
        )
        .await
        .unwrap();
    bootstrap.clunk(Fid(499_000)).await.unwrap();

    let procfs = procfs.with_runner(Arc::new(ArgvRunner));
    let spawner = procfs.for_spawner(
        Some(Pid(parent_pid)),
        namespace.clone(),
        Credentials::user("shell-test"),
    );
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(spawner)),
        Access::ReadWrite,
    );
    Shell::new(InProcessTransport::new(Arc::new(MountFs::new(namespace))))
}

#[tokio::test]
async fn generic_command_execution_collects_proc_output_and_exit() {
    let shell = command_shell().await;
    let result = shell
        .run("/bin/argv", &["first".to_string(), "two words".to_string()])
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.output, b"first|two words\n");
    assert_eq!(
        shell
            .cat(&format!("/proc/{}/namespace", result.pid))
            .await
            .unwrap(),
        b"/bin/argv ro"
    );
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
        command_shell().await,
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
