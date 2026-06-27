//! aP wire messages: the request/response pairs every operation reduces to.
//!
//! These are the bytes a transport carries (§5.7). The in-process fast path
//! ([`crate::InProcessTransport`]) dispatches the same `Request`/`Response`
//! values without serializing them; a future wire transport serializes them
//! unchanged. Keeping operations as explicit data — rather than method calls —
//! is what makes "a dumb byte transport could carry every operation" checkable.
//!
//! Fid allocation is client-driven (9P-faithful): `walk` and `create` carry the
//! `newfid` the client picked, `open` operates on an existing fid, and `clunk`
//! releases a fid (§5.2). Clone-via-open (§5.4) needs no dedicated operation:
//! the caller `open`s a [`FileKind::Clone`](crate::FileKind) file and `read`s it
//! to learn the allocated resource's name, then `walk`s to that name.

use serde::{Deserialize, Serialize};

use crate::{Fid, FileKind, Offset, OpenMode, Qid, Stat};

/// One aP operation request. Inputs are fids, paths (name components), byte
/// buffers, offsets, and counts — nothing in-process-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Request {
    /// Walk `names` from `fid`, binding `newfid` to the destination file.
    Walk {
        fid: Fid,
        newfid: Fid,
        names: Vec<String>,
    },
    /// Open `fid` for the given access intent. On a clone file this allocates a
    /// new resource (§5.4); its name is then read from `fid`.
    Open { fid: Fid, mode: OpenMode },
    /// Read up to `count` bytes from `offset`. On a stream this blocks until new
    /// bytes are available at or after `offset` (§5.3).
    Read {
        fid: Fid,
        offset: Offset,
        count: u32,
    },
    /// Write `data` at `offset`. Document entry points commit on `clunk`, not on
    /// a partial write (commit-on-clunk).
    Write {
        fid: Fid,
        offset: Offset,
        data: Vec<u8>,
    },
    /// Return metadata for `fid`.
    Stat { fid: Fid },
    /// Create child `name` of `kind` under directory `fid`, binding `newfid` to it.
    Create {
        fid: Fid,
        newfid: Fid,
        name: String,
        kind: FileKind,
    },
    /// Remove the file `fid` refers to, then release `fid`.
    Remove { fid: Fid },
    /// Release `fid`. For commit-on-clunk document writes, this is the commit point.
    Clunk { fid: Fid },
}

/// One aP operation response. Every output is owned and serializable (no borrows
/// into server memory), so it survives a byte transport unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum Response {
    Walk { qid: Qid },
    Open { qid: Qid },
    Read { data: Vec<u8> },
    Write { count: u32 },
    Stat { stat: Stat },
    Create { qid: Qid },
    Remove,
    Clunk,
}
