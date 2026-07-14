//! aP value types: the wire-shaped vocabulary shared by every operation.
//!
//! All types here are plain serializable data (ADR-0024 D5): fids, qids,
//! offsets, file kinds, open modes, error codes, and stat. None of them borrow
//! or carry an in-process-only handle, so each can cross a byte transport
//! unchanged.

use serde::{Deserialize, Serialize};

/// A fid is a handle to one interaction with a file server (§5.2). `walk`/`open`
/// allocate it, `clunk` releases it. Fids are scoped to one client/server
/// connection and are not global capabilities (the namespace is — D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Fid(pub u64);

impl Fid {
    /// The well-known fid naming a server's root, pre-bound by convention so an
    /// aP-only client can `walk` from it without a separate attach operation. It
    /// is never allocated by `walk`/`create` nor released by `clunk`.
    pub const ROOT: Fid = Fid(0);
}

/// A byte offset into a file or stream. Streams are resumable from a
/// caller-held offset (§5.3).
pub type Offset = u64;

/// What kind of file a qid names. Typing of stream *records* is a convention a
/// consumer interprets, not a kernel/protocol schema; this only distinguishes
/// the file *kind*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// A directory: `walk`able, its `read` yields child entries.
    Dir,
    /// A flat byte file with a length.
    File,
    /// A byte/offset stream with retained history: `read` blocks until new bytes
    /// are available and resumes from a caller-held offset (§5.3).
    Stream,
    /// A clone file: `open` allocates a new resource and returns its handle
    /// (§5.4), e.g. `/proc/clone` or an `llmfs` connection `clone`.
    Clone,
}

/// The unique identity of a file at a point in time (the 9P qid analog): its
/// kind, a version, and a server-unique path number.
///
/// `version` bumps when the file's observable content changes, so a client
/// comparing cached qid/version pairs detects a change instead of serving stale
/// data. Servers track this with [`VersionTable`](crate::VersionTable). A
/// [`Stream`](crate::Stream) file is the exception: its freshness is the read
/// offset (history is retained and reads resume by offset), so its qid `version`
/// is stable and need not bump per append.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Qid {
    pub kind: FileKind,
    pub version: u32,
    pub path: u64,
}

/// How a fid is opened. Authority comes from the namespace mount (read-only vs
/// read-write); `OpenMode` is the per-fid intent within granted authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenMode {
    Read,
    Write,
    ReadWrite,
}

/// File metadata returned by `stat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stat {
    pub name: String,
    pub qid: Qid,
    /// Byte length for files; for streams, the highest offset currently retained.
    pub length: u64,
    /// Whether a regular file is executable.
    #[serde(default)]
    pub executable: bool,
    /// Whether this fid's mount grants write authority.
    pub writable: bool,
}

/// Wire-shaped error codes. The *phase* of a failure is determined by which
/// operation returns it (§5.5): a denied/absent/rate-limited `open` is a
/// dial-time failure; a malformed document at `clunk`/`write` is a commit-time
/// failure; a failure after a stream has begun surfaces as a terminal record in
/// the stream, not as an op error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The named file/path does not exist (dial-time).
    #[error("not found")]
    NotFound,
    /// Access denied by mount/access rights (dial-time).
    #[error("no access")]
    NoAccess,
    /// The server is rate limiting this dial (dial-time).
    #[error("rate limited")]
    RateLimited,
    /// A committed document was malformed or truncated (commit-time).
    #[error("bad request")]
    BadRequest,
    /// A directory operation was attempted on a non-directory.
    #[error("not a directory")]
    NotDirectory,
    /// A byte operation was attempted on a directory.
    #[error("is a directory")]
    IsDirectory,
    /// The server does not support this operation on this file.
    #[error("unsupported")]
    Unsupported,
    /// An underlying I/O or backend failure.
    #[error("io error")]
    Io,
}
