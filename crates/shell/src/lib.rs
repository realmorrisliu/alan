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

use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{
    ErrorCode, Fid, FileKind, InProcessTransport, Offset, OpenMode, Qid, Request, Response,
};

/// A process-global fid allocator. aP fid state lives in the server keyed only by
/// [`Fid`], so two shells over the same transport (two tabs on one namespace) must
/// never draw the same number or one would clobber the other's open file. A single
/// global sequence guarantees uniqueness across every shell in the process.
static NEXT_FID: AtomicU64 = AtomicU64::new(1);

/// The shell's view of one mounted namespace, addressed by absolute path.
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

    /// Best-effort release: used on every exit path so a failed builtin never
    /// leaks the fid it opened.
    async fn clunk_quietly(&self, fid: Fid) {
        let _ = self.fs.call(Request::Clunk { fid }).await;
    }

    /// Write the whole buffer, looping on short writes: a server may legally accept
    /// only a prefix per call, so committing after one `Write` could truncate. No
    /// forward progress is a protocol error rather than an infinite loop.
    async fn write_all(&self, fid: Fid, data: &[u8]) -> Result<(), ErrorCode> {
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
            if accepted == 0 {
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

    /// `echo data > path` — write a document and commit it on clunk.
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<(), ErrorCode> {
        let (fid, _) = self.walk_to(path).await?;
        let result = self.write_body(fid, data).await;
        self.clunk_quietly(fid).await;
        result
    }

    async fn write_body(&self, fid: Fid, data: &[u8]) -> Result<(), ErrorCode> {
        self.open(fid, OpenMode::Write).await?;
        self.write_all(fid, data).await
    }

    /// `tail path` — open a live tail session: repeated blocking reads that advance
    /// a held offset, keeping the fid open so a multi-append stream is fully
    /// observed (not just its first chunk). Close it with [`Tail::close`].
    pub async fn tail(&self, path: &str) -> Result<Tail, ErrorCode> {
        let (fid, _) = self.walk_to(path).await?;
        if let Err(e) = self.open(fid, OpenMode::Read).await {
            self.clunk_quietly(fid).await;
            return Err(e);
        }
        Ok(Tail {
            fs: self.fs.clone(),
            fid,
            offset: 0,
        })
    }

    /// `spawn` — launch a process via clone-via-open on `/proc/clone`: open the
    /// clone file (returns the pending pid), write the exec spec, and clunk to
    /// commit. Pure aP, no side API. `/proc` is a mount in the namespace, so the
    /// path is `/proc/clone`, not `/clone`. Returns the new pid.
    pub async fn spawn(&self, exec_spec: &str) -> Result<String, ErrorCode> {
        let (fid, _) = self.walk_to("/proc/clone").await?;
        let result = self.spawn_body(fid, exec_spec).await;
        self.clunk_quietly(fid).await;
        result
    }

    async fn spawn_body(&self, fid: Fid, exec_spec: &str) -> Result<String, ErrorCode> {
        self.open(fid, OpenMode::ReadWrite).await?;
        let pid = String::from_utf8(self.read_at(fid, 0, 64).await?).map_err(|_| ErrorCode::Io)?;
        self.write_all(fid, exec_spec.as_bytes()).await?;
        Ok(pid)
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
