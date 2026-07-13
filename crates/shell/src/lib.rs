//! Alan Shell — the aP-only client.
//!
//! Every builtin is generic file IO over aP: `ls` (walk a directory + read its
//! entries), `cat` (open + read a finite snapshot), `write`/`echo >` (open +
//! write + clunk), `tail` (a live session of blocking reads), and `spawn`
//! (clone-via-open on `/proc/clone`). There is **no agent-specific command** and
//! no `attach` sugar — an agent is just files under `/agent/<pid>`, reached with
//! the same builtins (ADR-0025 D3). The shell depends only on [`alan_ap`]; it
//! never links a server or backend, and it addresses a whole assembled namespace
//! (`/proc`, `/agent`, `/mnt/llm`) through one transport — in v1 the kernel's
//! namespace presented as one aP server (`alan-kernel::MountFs`).

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{
    ErrorCode, Fid, FileKind, InProcessTransport, Offset, OpenMode, Qid, Request, Response,
};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// A process-global fid allocator. aP fid state lives in the server keyed only by
/// [`Fid`], so two shells over the same transport (two tabs on one namespace) must
/// never draw the same number or one would clobber the other's open file. A single
/// global sequence guarantees uniqueness across every shell in the process.
static NEXT_FID: AtomicU64 = AtomicU64::new(1);

/// The shell's view of one mounted namespace, addressed by absolute path.
#[derive(Clone)]
pub struct Shell {
    fs: InProcessTransport,
}

impl Shell {
    /// Build a shell over a mounted file tree (in v1, the kernel's assembled
    /// namespace presented as one aP server).
    pub fn new(fs: InProcessTransport) -> Self {
        Self { fs }
    }

    /// Draw a fresh fid unique across the whole process (see [`NEXT_FID`]).
    fn alloc_fid(&self) -> Fid {
        Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed))
    }

    /// Walk an absolute path, binding a fresh fid at its target and returning that
    /// fid with the target's qid (whose `kind` distinguishes dir/file/stream).
    async fn walk_to(&self, path: &str) -> Result<(Fid, Qid), ErrorCode> {
        let names = split_path(path);
        let fid = self.alloc_fid();
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names,
            })
            .await?
        {
            Response::Walk { qid } => Ok((fid, qid)),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        match self.fs.call(Request::Open { fid, mode }).await? {
            Response::Open { qid } => Ok(qid),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn read_at(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self.fs.call(Request::Read { fid, offset, count }).await? {
            Response::Read { data } => Ok(data),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn length(&self, fid: Fid) -> Result<u64, ErrorCode> {
        match self.fs.call(Request::Stat { fid }).await? {
            Response::Stat { stat } => Ok(stat.length),
            _ => Err(ErrorCode::Io),
        }
    }

    /// Release a fid, propagating a commit-time error. On a commit-on-clunk
    /// endpoint (a document file, `/proc/clone`) the `Clunk` *is* the commit, so a
    /// rejected/malformed document surfaces here — the success path must not
    /// swallow it.
    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        match self.fs.call(Request::Clunk { fid }).await? {
            Response::Clunk => Ok(()),
            _ => Err(ErrorCode::Io),
        }
    }

    /// Best-effort release for the *cleanup* path: the builtin already failed, so a
    /// clunk error is irrelevant and must not mask the original error.
    async fn clunk_quietly(&self, fid: Fid) {
        let _ = self.fs.call(Request::Clunk { fid }).await;
    }

    /// Write the whole buffer, looping on short writes: a server may legally accept
    /// only a prefix per call, so committing after one `Write` could truncate. A
    /// count of zero (no progress) or one larger than the bytes offered is a
    /// protocol error — a buggy/hostile service must not spin the loop or crash the
    /// shell by indexing past the buffer.
    async fn write_all(&self, fid: Fid, data: &[u8]) -> Result<(), ErrorCode> {
        // An intentionally empty document must still reach the server as one
        // zero-length write, so a commit-on-clunk endpoint marks the fid written
        // and commits the empty value instead of silently dropping it.
        if data.is_empty() {
            return match self
                .fs
                .call(Request::Write {
                    fid,
                    offset: 0,
                    data: Vec::new(),
                })
                .await?
            {
                Response::Write { .. } => Ok(()),
                _ => Err(ErrorCode::Io),
            };
        }
        let mut offset: Offset = 0;
        let mut remaining = data;
        while !remaining.is_empty() {
            let accepted = match self
                .fs
                .call(Request::Write {
                    fid,
                    offset,
                    data: remaining.to_vec(),
                })
                .await?
            {
                Response::Write { count } => count as usize,
                _ => return Err(ErrorCode::Io),
            };
            if accepted == 0 || accepted > remaining.len() {
                return Err(ErrorCode::Io);
            }
            offset += accepted as u64;
            remaining = &remaining[accepted..];
        }
        Ok(())
    }

    /// `cat path` — read a finite snapshot of a file. A flat file is read to EOF;
    /// a **stream** is bounded by its current length so `cat` never blocks at the
    /// live edge (that is `tail`'s job).
    pub async fn cat(&self, path: &str) -> Result<Vec<u8>, ErrorCode> {
        let (fid, qid) = self.walk_to(path).await?;
        let result = self.cat_body(fid, qid.kind).await;
        self.clunk_quietly(fid).await;
        result
    }

    async fn cat_body(&self, fid: Fid, kind: FileKind) -> Result<Vec<u8>, ErrorCode> {
        self.open(fid, OpenMode::Read).await?;
        let mut out = Vec::new();
        if kind == FileKind::Stream {
            // Snapshot: read only up to the length observed now, so a read never
            // lands on the (blocking) live edge.
            let len = self.length(fid).await?;
            while (out.len() as u64) < len {
                let want = (len - out.len() as u64).min(4096) as u32;
                let chunk = self.read_at(fid, out.len() as u64, want).await?;
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
        } else {
            // A finite file returns empty at EOF.
            loop {
                let chunk = self.read_at(fid, out.len() as u64, 4096).await?;
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
        }
        Ok(out)
    }

    /// `ls path` — list a directory's entries. The target must be a directory; a
    /// regular file is rejected rather than having its bytes read as entries.
    pub async fn ls(&self, path: &str) -> Result<Vec<String>, ErrorCode> {
        let (fid, qid) = self.walk_to(path).await?;
        let result = self.ls_body(fid, qid.kind).await;
        self.clunk_quietly(fid).await;
        result
    }

    async fn ls_body(&self, fid: Fid, kind: FileKind) -> Result<Vec<String>, ErrorCode> {
        if kind != FileKind::Dir {
            return Err(ErrorCode::NotDirectory);
        }
        self.open(fid, OpenMode::Read).await?;
        let mut bytes = Vec::new();
        loop {
            let chunk = self.read_at(fid, bytes.len() as u64, 4096).await?;
            if chunk.is_empty() {
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes).map_err(|_| ErrorCode::Io)?;
        Ok(text
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// `echo data > path` — write a document and commit it on clunk. A commit-time
    /// rejection (the server discards a malformed document at clunk) surfaces as an
    /// error rather than a false success.
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<(), ErrorCode> {
        let (fid, _) = self.walk_to(path).await?;
        match self.write_body(fid, data).await {
            // The write reached the commit step: the clunk *is* the commit.
            Ok(()) => self.clunk(fid).await,
            Err(e) => {
                self.clunk_quietly(fid).await;
                Err(e)
            }
        }
    }

    async fn write_body(&self, fid: Fid, data: &[u8]) -> Result<(), ErrorCode> {
        self.open(fid, OpenMode::Write).await?;
        self.write_all(fid, data).await
    }

    /// `tail path` — open a live tail session: repeated blocking reads that advance
    /// a held offset, keeping the fid open so a multi-append stream is fully
    /// observed (not just its first chunk). Close it with [`Tail::close`].
    pub async fn tail(&self, path: &str) -> Result<Tail, ErrorCode> {
        self.tail_from(path, 0).await
    }

    /// Open a live tail at a caller-held offset, for example after reconnecting
    /// to the same stream through a new attachment.
    pub async fn tail_from(&self, path: &str, offset: Offset) -> Result<Tail, ErrorCode> {
        let (fid, _) = self.walk_to(path).await?;
        if let Err(e) = self.open(fid, OpenMode::Read).await {
            self.clunk_quietly(fid).await;
            return Err(e);
        }
        Ok(Tail {
            fs: self.fs.clone(),
            fid,
            offset,
        })
    }

    /// `spawn` — launch a process via clone-via-open on `/proc/clone`: open the
    /// clone file (returns the pending pid), write the exec spec, and clunk to
    /// commit. Pure aP, no side API. `/proc` is a mount in the namespace, so the
    /// path is `/proc/clone`, not `/clone`. Returns the new pid.
    pub async fn spawn(&self, exec_spec: &str) -> Result<String, ErrorCode> {
        let (fid, _) = self.walk_to("/proc/clone").await?;
        match self.spawn_body(fid, exec_spec).await {
            // Clunk commits the spawn: a malformed exec spec is rejected here, so a
            // failed commit must fail spawn rather than return a bogus pid.
            Ok(pid) => {
                self.clunk(fid).await?;
                Ok(pid)
            }
            Err(e) => {
                self.clunk_quietly(fid).await;
                Err(e)
            }
        }
    }

    async fn spawn_body(&self, fid: Fid, exec_spec: &str) -> Result<String, ErrorCode> {
        self.open(fid, OpenMode::ReadWrite).await?;
        let pid = String::from_utf8(self.read_at(fid, 0, 64).await?).map_err(|_| ErrorCode::Io)?;
        self.write_all(fid, exec_spec.as_bytes()).await?;
        Ok(pid)
    }
}

/// Error returned by the line-oriented stdio driver.
#[derive(Debug)]
pub enum DriverError {
    /// The underlying aP file operation failed.
    Protocol(ErrorCode),
    /// Reading stdin or writing stdout failed.
    Io(std::io::Error),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(code) => write!(f, "aP operation failed: {code:?}"),
            Self::Io(err) => write!(f, "stdio failed: {err}"),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<ErrorCode> for DriverError {
    fn from(value: ErrorCode) -> Self {
        Self::Protocol(value)
    }
}

impl From<std::io::Error> for DriverError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

enum LineCommand {
    Ls(String),
    Cat(String),
    Echo { path: String, data: Vec<u8> },
    Write { path: String, data: Vec<u8> },
    Tail(String),
    Spawn(String),
    Exit,
    Empty,
}

/// Minimal line-oriented shell driver.
///
/// The driver is intentionally only a composition layer over generic builtins:
/// it parses text commands into `ls`, `cat`, `echo >`, `write`, `tail`, and
/// `spawn` calls. It contains no agent-specific command or attach mode; talking
/// to an agent is still just `tail /agent/<pid>/io/output` plus
/// `echo ... > /agent/<pid>/io/input`.
pub struct StdioDriver {
    shell: Shell,
}

struct TailTask {
    shutdown: oneshot::Sender<()>,
    handle: JoinHandle<()>,
}

impl StdioDriver {
    /// Build a line driver over an aP-only [`Shell`].
    pub fn new(shell: Shell) -> Self {
        Self { shell }
    }

    /// Run the read-eval-print loop over caller-provided async stdin/stdout.
    ///
    /// `tail` commands start independent tasks that forward bytes to the same
    /// stdout writer while this loop keeps accepting input. The first rich
    /// renderer can later give those streams separate panes; this driver keeps
    /// the M1/M2 proof deliberately line-oriented.
    pub async fn run<R, W>(&self, input: R, mut output: W) -> Result<(), DriverError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut lines = input.lines();
        let (tail_tx, mut tail_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let mut tail_tasks: Vec<TailTask> = Vec::new();

        loop {
            tokio::select! {
                line = lines.next_line() => {
                    let Some(line) = line? else { break; };
                    if self
                        .handle_line(&line, &mut output, &tail_tx, &mut tail_tasks)
                        .await?
                    {
                        break;
                    }
                }
                Some(bytes) = tail_rx.recv(), if !tail_tasks.is_empty() => {
                    output.write_all(&bytes).await?;
                    output.flush().await?;
                }
            }
        }

        for task in tail_tasks {
            let _ = task.shutdown.send(());
            let _ = task.handle.await;
        }
        while let Ok(bytes) = tail_rx.try_recv() {
            output.write_all(&bytes).await?;
        }
        output.flush().await?;
        Ok(())
    }

    async fn handle_line<W>(
        &self,
        line: &str,
        output: &mut W,
        tail_tx: &mpsc::UnboundedSender<Vec<u8>>,
        tail_tasks: &mut Vec<TailTask>,
    ) -> Result<bool, DriverError>
    where
        W: AsyncWrite + Unpin,
    {
        let command = match parse_line(line) {
            Ok(command) => command,
            Err(message) => {
                output
                    .write_all(format!("error: {message}\n").as_bytes())
                    .await?;
                output.flush().await?;
                return Ok(false);
            }
        };

        match command {
            LineCommand::Empty => {}
            LineCommand::Exit => return Ok(true),
            LineCommand::Ls(path) => match self.shell.ls(&path).await {
                Ok(entries) => {
                    for entry in entries {
                        output.write_all(entry.as_bytes()).await?;
                        output.write_all(b"\n").await?;
                    }
                }
                Err(err) => write_protocol_error(output, err).await?,
            },
            LineCommand::Cat(path) => match self.shell.cat(&path).await {
                Ok(bytes) => output.write_all(&bytes).await?,
                Err(err) => write_protocol_error(output, err).await?,
            },
            LineCommand::Echo { path, data } | LineCommand::Write { path, data } => {
                if let Err(err) = self.shell.write(&path, &data).await {
                    write_protocol_error(output, err).await?;
                }
            }
            LineCommand::Tail(path) => {
                let shell = self.shell.clone();
                let tx = tail_tx.clone();
                let (shutdown, mut shutdown_rx) = oneshot::channel();
                let handle = tokio::spawn(async move {
                    let mut tail = match shell.tail(&path).await {
                        Ok(tail) => tail,
                        Err(err) => {
                            let _ = tx.send(format!("error: {err:?}\n").into_bytes());
                            return;
                        }
                    };
                    loop {
                        tokio::select! {
                            biased;
                            _ = &mut shutdown_rx => break,
                            result = tail.read(4096) => {
                                match result {
                                    Ok(bytes) if bytes.is_empty() => break,
                                    Ok(bytes) => {
                                        if tx.send(bytes).is_err() {
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = tx.send(format!("error: {err:?}\n").into_bytes());
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    let _ = tail.close().await;
                });
                tail_tasks.push(TailTask { shutdown, handle });
            }
            LineCommand::Spawn(exec_spec) => match self.shell.spawn(&exec_spec).await {
                Ok(pid) => {
                    output.write_all(pid.trim().as_bytes()).await?;
                    output.write_all(b"\n").await?;
                }
                Err(err) => write_protocol_error(output, err).await?,
            },
        }
        output.flush().await?;
        Ok(false)
    }
}

async fn write_protocol_error<W>(output: &mut W, err: ErrorCode) -> Result<(), DriverError>
where
    W: AsyncWrite + Unpin,
{
    output
        .write_all(format!("error: {err:?}\n").as_bytes())
        .await?;
    Ok(())
}

fn parse_line(line: &str) -> Result<LineCommand, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(LineCommand::Empty);
    }
    if matches!(trimmed, "exit" | "quit") {
        return Ok(LineCommand::Exit);
    }
    if let Some(path) = trimmed.strip_prefix("ls ") {
        return Ok(LineCommand::Ls(non_empty_path(path)?));
    }
    if let Some(path) = trimmed.strip_prefix("cat ") {
        return Ok(LineCommand::Cat(non_empty_path(path)?));
    }
    if let Some(path) = trimmed.strip_prefix("tail ") {
        return Ok(LineCommand::Tail(non_empty_path(path)?));
    }
    if let Some(exec_spec) = trimmed.strip_prefix("spawn ") {
        let exec_spec = exec_spec.trim();
        if exec_spec.is_empty() {
            return Err("spawn requires an exec spec".to_string());
        }
        return Ok(LineCommand::Spawn(exec_spec.to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("write ") {
        let Some((path, data)) = rest.trim().split_once(' ') else {
            return Err("write requires a path and data".to_string());
        };
        return Ok(LineCommand::Write {
            path: non_empty_path(path)?,
            data: data.as_bytes().to_vec(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("echo ") {
        let Some((data, path)) = rest.rsplit_once('>') else {
            return Err("echo syntax is: echo <data> > <path>".to_string());
        };
        return Ok(LineCommand::Echo {
            path: non_empty_path(path)?,
            data: data.trim_end().as_bytes().to_vec(),
        });
    }
    Err(format!("unknown command: {trimmed}"))
}

fn non_empty_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() {
        Err("path is required".to_string())
    } else {
        Ok(path.to_string())
    }
}

/// A live tail over a file/stream: repeated blocking reads that advance a held
/// offset, keeping one fid open until [`close`](Tail::close). This is how a
/// multi-append stream (an agent's `io/output` emitting several records) is fully
/// followed — a single read would see only the first append.
pub struct Tail {
    fs: InProcessTransport,
    fid: Fid,
    offset: Offset,
}

impl Tail {
    /// Block until bytes at the current offset exist, return them, and advance the
    /// offset past them. On a stream at the live edge this parks until the next
    /// append; on a closed/drained stream it returns empty.
    pub async fn read(&mut self, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let data = match self
            .fs
            .call(Request::Read {
                fid: self.fid,
                offset: self.offset,
                count,
            })
            .await?
        {
            Response::Read { data } => data,
            _ => return Err(ErrorCode::Io),
        };
        self.offset += data.len() as u64;
        Ok(data)
    }

    /// The offset the next [`read`](Tail::read) will resume from.
    pub fn offset(&self) -> Offset {
        self.offset
    }

    /// Release the tail's fid.
    pub async fn close(self) -> Result<(), ErrorCode> {
        match self.fs.call(Request::Clunk { fid: self.fid }).await? {
            Response::Clunk => Ok(()),
            _ => Err(ErrorCode::Io),
        }
    }
}

/// Split an absolute path into its non-empty components. `/` → `[]`.
fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
