//! Alan Shell — the aP-only client.
//!
//! Every builtin is generic file IO over aP: `ls` (walk + read a directory),
//! `cat` (open + read), `write`/`echo >` (open + write + clunk), `tail`
//! (blocking read from an offset), and `spawn` (clone-via-open on `/proc/clone`).
//! There is **no agent-specific command** and no `attach` sugar — an agent is
//! just files under `/agent/<pid>`, reached with the same builtins (ADR-0025 D3).
//! The shell depends only on [`alan_ap`]; it never links a server or backend.

use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{ErrorCode, Fid, InProcessTransport, Offset, OpenMode, Request, Response};

/// The shell's view of one mounted namespace, addressed by absolute path.
pub struct Shell {
    fs: InProcessTransport,
    next_fid: AtomicU64,
}

impl Shell {
    /// Build a shell over a mounted file tree (in v1, the kernel's assembled
    /// namespace presented as one aP server).
    pub fn new(fs: InProcessTransport) -> Self {
        // Fid 0 is the well-known root; client fids start above it.
        Self {
            fs,
            next_fid: AtomicU64::new(1),
        }
    }

    fn alloc_fid(&self) -> Fid {
        Fid(self.next_fid.fetch_add(1, Ordering::Relaxed))
    }

    /// Walk an absolute path, binding and returning a fresh fid at its target.
    async fn walk_to(&self, path: &str) -> Result<Fid, ErrorCode> {
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
            Response::Walk { .. } => Ok(fid),
            _ => Err(ErrorCode::Io),
        }
    }

    async fn read_once(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        match self.fs.call(Request::Read { fid, offset, count }).await? {
            Response::Read { data } => Ok(data),
            _ => Err(ErrorCode::Io),
        }
    }

    /// `cat path` — read a finite file in full (reads until end of file).
    pub async fn cat(&self, path: &str) -> Result<Vec<u8>, ErrorCode> {
        let fid = self.walk_to(path).await?;
        self.fs
            .call(Request::Open {
                fid,
                mode: OpenMode::Read,
            })
            .await?;
        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let chunk = self.read_once(fid, offset, 4096).await?;
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            out.extend_from_slice(&chunk);
        }
        self.fs.call(Request::Clunk { fid }).await?;
        Ok(out)
    }

    /// `ls path` — list a directory's entries.
    pub async fn ls(&self, path: &str) -> Result<Vec<String>, ErrorCode> {
        let listing = self.cat(path).await?;
        let text = String::from_utf8(listing).map_err(|_| ErrorCode::Io)?;
        Ok(text
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// `echo data > path` — write a document and commit it on clunk.
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<(), ErrorCode> {
        let fid = self.walk_to(path).await?;
        self.fs
            .call(Request::Open {
                fid,
                mode: OpenMode::Write,
            })
            .await?;
        self.fs
            .call(Request::Write {
                fid,
                offset: 0,
                data: data.to_vec(),
            })
            .await?;
        self.fs.call(Request::Clunk { fid }).await?;
        Ok(())
    }

    /// `tail path` — read from `offset`, blocking at the live edge until new
    /// bytes arrive (the watch builtin; observation is a blocking read).
    pub async fn tail(&self, path: &str, offset: Offset) -> Result<Vec<u8>, ErrorCode> {
        let fid = self.walk_to(path).await?;
        self.fs
            .call(Request::Open {
                fid,
                mode: OpenMode::Read,
            })
            .await?;
        let data = self.read_once(fid, offset, 65536).await?;
        self.fs.call(Request::Clunk { fid }).await?;
        Ok(data)
    }

    /// `spawn` — launch a process via clone-via-open on `/proc/clone`: open the
    /// clone file (returns the pending pid), write the exec spec, and clunk to
    /// commit. Pure aP, no side API. Returns the new pid.
    pub async fn spawn(&self, exec_spec: &str) -> Result<String, ErrorCode> {
        let fid = self.walk_to("/clone").await?;
        self.fs
            .call(Request::Open {
                fid,
                mode: OpenMode::ReadWrite,
            })
            .await?;
        let pid =
            String::from_utf8(self.read_once(fid, 0, 64).await?).map_err(|_| ErrorCode::Io)?;
        self.fs
            .call(Request::Write {
                fid,
                offset: 0,
                data: exec_spec.as_bytes().to_vec(),
            })
            .await?;
        self.fs.call(Request::Clunk { fid }).await?;
        Ok(pid)
    }
}

/// Split an absolute path into its non-empty components. `/` → `[]`.
fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
