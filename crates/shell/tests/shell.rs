//! Alan Shell builtins over aP (introduce-alan-shell §3, §5.1). The shell is an
//! aP-only client: every builtin is generic file IO (walk/open/read/write/clunk
//! and clone-via-open for spawn) with no agent-specific command. These tests run
//! the builtins against an in-memory echo file server (the M1 milestone — input
//! echoed back through files, no LLM) and against the kernel's `/proc` for spawn.

use std::collections::HashMap;
use std::sync::Arc;

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat, Stream,
};
use alan_kernel::ProcFs;
use alan_shell::Shell;

/// A tiny read-write echo server: a `buf` byte file and a `stream` stream file.
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
        Ok(Qid {
            kind: FileKind::File,
            version: 0,
            path: 0,
        })
    }
    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Ok(Qid {
            kind: FileKind::File,
            version: 0,
            path: 0,
        })
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
    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: FileKind::File,
                version: 0,
                path: 0,
            },
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
async fn tail_blocks_until_new_bytes_then_returns_them() {
    use std::time::Duration;

    let transport = EchoFs::transport();
    let shell = Arc::new(Shell::new(transport));
    let watcher = shell.clone();
    let handle = tokio::spawn(async move { watcher.tail("/stream", 0).await.unwrap() });

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!handle.is_finished(), "tail should block at the live edge");

    shell.write("/stream", b"streamed").await.unwrap();
    let got = tokio::time::timeout(Duration::from_millis(500), handle)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, b"streamed");
}

#[tokio::test]
async fn spawn_launches_a_process_via_clone_over_ap_only() {
    // The aP-only shell launches a process through /proc/clone with no side API.
    let shell = Shell::new(InProcessTransport::new(Arc::new(ProcFs::new())));
    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
        .await
        .unwrap();

    // The new pid is now a public /proc entry whose status reads "running".
    assert_eq!(
        shell.cat(&format!("/{pid}/status")).await.unwrap(),
        b"running\n"
    );
}
