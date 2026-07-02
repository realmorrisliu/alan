//! alan-llmfs — the LLM file server (add-llm-file-server, the minimal callable
//! slice brought into the Plan 9 core).
//!
//! It serves callable **Connections** and models a **Generation** as a
//! clone-via-open directory: a caller opens `connections/<conn>/clone`
//! (allocating a fresh Generation), writes one neutral request document to
//! `data` (committed on clunk), and reads a typed token stream from `events`.
//! `ctl` aborts and `status` reports progress. This realizes ADR-0024's core
//! framing — *an LLM is a typed stream a process reads* — as files, wrapping
//! `alan-llm` providers and speaking aP.
//!
//! This slice is deliberately minimal (the rest of add-llm-file-server —
//! provider introspection, connection management, the versioned wire DTO,
//! metering/rate-limiting/cost — stays deferred so the "core" does not absorb a
//! whole product surface).
//!
//! A Generation moves through a small lifecycle: `open` (allocated, awaiting the
//! request) → `running` (provider streaming) → a terminal state (`done`,
//! `error`, `rejected`, or `aborted`). Every path that ends a Generation writes a
//! terminal record to `events` and a terminal `status`, so a consumer tailing
//! `events` at the live edge never blocks forever.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use alan_llm::{GenerationRequest, LlmProvider, StreamChunk};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::{Mutex as AsyncMutex, Notify};

/// Cap on a buffered request document, so a hostile writer cannot exhaust the
/// server before the commit-time validation runs.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// The neutral request document written to a Generation's `data` file. Minimal
/// for this slice; the full versioned wire DTO is deferred. Unknown fields are
/// rejected so an unsupported request (e.g. `tools`, `temperature`) fails at the
/// commit boundary instead of silently running a different prompt.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestDoc {
    #[serde(default)]
    system: Option<String>,
    user: String,
}

/// A callable connection: a provider behind an async lock so a Generation can
/// hold it across `generate_stream`.
struct Connection {
    provider: AsyncMutex<Box<dyn LlmProvider>>,
}

/// A Generation's lifecycle status.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GenStatus {
    Open,
    Running,
    Done,
    Error,
    Rejected,
    Aborted,
}

impl GenStatus {
    fn as_str(self) -> &'static str {
        match self {
            GenStatus::Open => "open",
            GenStatus::Running => "running",
            GenStatus::Done => "done",
            GenStatus::Error => "error",
            GenStatus::Rejected => "rejected",
            GenStatus::Aborted => "aborted",
        }
    }
    fn is_terminal(self) -> bool {
        matches!(
            self,
            GenStatus::Done | GenStatus::Error | GenStatus::Rejected | GenStatus::Aborted
        )
    }
}

/// One Generation's projected surfaces and lifecycle.
struct Generation {
    /// The connection captured at allocation, so a later `register_connection`
    /// replacing the name cannot reroute this Generation's request.
    connection: Arc<Connection>,
    /// The connection name, for directory membership under `connections/<conn>`.
    connection_name: String,
    events: Stream,
    status: StdMutex<GenStatus>,
    /// qid version, bumped on every status change so a cached `status`/dir qid
    /// goes stale.
    version: AtomicU32,
    /// Signals the drain task to stop promptly on abort.
    abort: Arc<Notify>,
    /// Serializes every `events` append and terminal transition, so a `ctl` abort
    /// and the drain task cannot interleave — no chunk or `done` record is ever
    /// written after the Generation is aborted.
    finalize: AsyncMutex<()>,
}

impl Generation {
    fn status(&self) -> GenStatus {
        *self.status.lock().unwrap()
    }
    fn connection_name(&self) -> String {
        self.connection_name.clone()
    }
    /// Move to a terminal (or running) status unless already terminal, bumping the
    /// version. Returns whether the transition happened.
    fn advance(&self, to: GenStatus) -> bool {
        let mut s = self.status.lock().unwrap();
        if s.is_terminal() {
            return false;
        }
        *s = to;
        self.version.fetch_add(1, Ordering::Relaxed);
        true
    }
    /// Claim the single initial transition out of `Open` (to `Running` on commit,
    /// or `Rejected` on a malformed request). Atomic compare-and-set: exactly one
    /// caller wins, so two concurrent commits cannot both reach the provider.
    fn claim(&self, to: GenStatus) -> bool {
        let mut s = self.status.lock().unwrap();
        if *s != GenStatus::Open {
            return false;
        }
        *s = to;
        self.version.fetch_add(1, Ordering::Relaxed);
        true
    }
}

/// What a fid points at within the llmfs tree.
#[derive(Clone)]
enum Node {
    Root,
    ConnectionsDir,
    Connection(String),
    Clone(String),
    Gen(String),
    GenData(String),
    GenEvents(String),
    GenCtl(String),
    GenStatus(String),
}

struct LlmFid {
    node: Node,
    /// The open mode, once opened. `None` means walked-but-not-opened.
    mode: Option<OpenMode>,
    /// For a fid that opened a `clone` file: the allocated Generation id.
    clone_gen: Option<String>,
    /// Buffered request document for a `data` fid (commit-on-clunk).
    write_buf: Vec<u8>,
}

impl LlmFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            clone_gen: None,
            write_buf: Vec::new(),
        }
    }
}

struct State {
    connections: HashMap<String, Arc<Connection>>,
    gens: HashMap<String, Arc<Generation>>,
    fids: HashMap<Fid, LlmFid>,
    next_gen: u64,
    /// Version of directory listings (`connections/`, a connection's contents),
    /// bumped when a Generation is allocated so cached directory qids go stale.
    listing_version: u32,
}

impl State {
    /// The qid for a node, with its server-unique path and current version.
    fn qid(&self, node: &Node) -> Qid {
        let (kind, key) = node_identity(node);
        let version = match node {
            Node::Gen(id)
            | Node::GenData(id)
            | Node::GenEvents(id)
            | Node::GenCtl(id)
            | Node::GenStatus(id) => self
                .gens
                .get(id)
                .map(|g| g.version.load(Ordering::Relaxed))
                .unwrap_or(0),
            Node::ConnectionsDir | Node::Connection(_) => self.listing_version,
            Node::Root | Node::Clone(_) => 0,
        };
        Qid {
            kind,
            version,
            path: hash_path(&key),
        }
    }
}

/// The LLM file server.
pub struct LlmFs {
    state: StdMutex<State>,
}

impl Default for LlmFs {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmFs {
    pub fn new() -> Self {
        Self {
            state: StdMutex::new(State {
                connections: HashMap::new(),
                gens: HashMap::new(),
                fids: HashMap::new(),
                next_gen: 0,
                listing_version: 0,
            }),
        }
    }

    /// Register a callable connection backed by an `alan-llm` provider. (In the
    /// full server, connections are assembled from provider + model + credential;
    /// this slice takes a ready provider.)
    pub fn register_connection(&self, name: &str, provider: Box<dyn LlmProvider>) {
        let mut state = self.state.lock().unwrap();
        state.connections.insert(
            name.to_string(),
            Arc::new(Connection {
                provider: AsyncMutex::new(provider),
            }),
        );
        // The `connections/` listing changed: bump its qid version so a cached
        // directory listing goes stale and the new endpoint is seen.
        state.listing_version += 1;
    }

    fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        let state = self.state.lock().unwrap();
        state
            .fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        let state = self.state.lock().unwrap();
        match node {
            Node::Root if name == "connections" => Ok(Node::ConnectionsDir),
            Node::ConnectionsDir if state.connections.contains_key(name) => {
                Ok(Node::Connection(name.to_string()))
            }
            Node::Connection(conn) => {
                if name == "clone" {
                    Ok(Node::Clone(conn.clone()))
                } else if state
                    .gens
                    .get(name)
                    .is_some_and(|g| &g.connection_name() == conn)
                {
                    Ok(Node::Gen(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            Node::Gen(id) => match name {
                "data" => Ok(Node::GenData(id.clone())),
                "events" => Ok(Node::GenEvents(id.clone())),
                "ctl" => Ok(Node::GenCtl(id.clone())),
                "status" => Ok(Node::GenStatus(id.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            _ => Err(ErrorCode::NotDirectory),
        }
    }
}

#[async_trait]
impl FileServer for LlmFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        // Never rebind the root. (Resolution below re-locks per step; the binding
        // is checked-and-inserted atomically at the end.)
        if newfid == Fid::ROOT {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = self.node_of(fid)?;
        for name in names {
            node = self.child(&node, name)?;
        }
        // Check-and-insert under a single lock hold: two concurrent walks that
        // chose the same `newfid` cannot both pass and clobber a live fid (e.g. a
        // write-open `data` fid that already buffered a request).
        let mut state = self.state.lock().unwrap();
        if state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, LlmFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().unwrap();
        // A fid opens once: a second open would allocate a second Generation on a
        // clone file or re-establish intent, so reject it.
        if state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            state
                .fids
                .get(&fid)
                .map(|f| f.node.clone())
                .ok_or(ErrorCode::NotFound)?
        };

        // Dial-time access check: an intent the node cannot service fails here, not
        // later as `Unsupported` on read/write. A write intent needs a writable
        // node; a read intent needs a readable node (`data`/`ctl` are write-only
        // sinks with no readable surface).
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        if matches!(mode, OpenMode::Read | OpenMode::ReadWrite) && !is_readable(&node) {
            return Err(ErrorCode::NoAccess);
        }

        // Clone-via-open allocates a fresh Generation *and* the caller must read the
        // fid back to learn its id, so it requires ReadWrite: a read-only observer
        // can't allocate, and a write-only open can't strand a Generation whose id
        // it could never read.
        if let Node::Clone(conn) = &node {
            if !matches!(mode, OpenMode::ReadWrite) {
                return Err(ErrorCode::NoAccess);
            }
            let connection = state
                .connections
                .get(conn)
                .cloned()
                .ok_or(ErrorCode::NotFound)?;
            let id = format!("g{}", state.next_gen);
            state.next_gen += 1;
            state.listing_version += 1;
            state.gens.insert(
                id.clone(),
                Arc::new(Generation {
                    connection,
                    connection_name: conn.clone(),
                    events: Stream::new(),
                    status: StdMutex::new(GenStatus::Open),
                    version: AtomicU32::new(0),
                    abort: Arc::new(Notify::new()),
                    finalize: AsyncMutex::new(()),
                }),
            );
            if let Some(f) = state.fids.get_mut(&fid) {
                f.clone_gen = Some(id);
            }
        }

        let qid = state.qid(&node);
        if let Some(f) = state.fids.get_mut(&fid) {
            f.mode = Some(mode);
        }
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        // Reads need read authority from a successful read-open (ROOT is the
        // pre-bound anchor and is always readable).
        let (node, clone_gen) = {
            let state = self.state.lock().unwrap();
            if fid == Fid::ROOT {
                (Node::Root, None)
            } else {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
                (f.node.clone(), f.clone_gen.clone())
            }
        };
        // An opened clone fid reads back the allocated Generation id.
        if let Some(id) = clone_gen {
            return Ok(slice(id.into_bytes(), offset, count));
        }

        // Stream node: clone the Stream out, then read without holding the lock.
        if let Node::GenEvents(id) = &node {
            let events = {
                let state = self.state.lock().unwrap();
                state
                    .gens
                    .get(id)
                    .ok_or(ErrorCode::NotFound)?
                    .events
                    .clone()
            };
            return Ok(events.read(offset, count).await);
        }

        let bytes = self.computed_bytes(&node)?;
        Ok(slice(bytes, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        // Phase 1, under the lock: check write intent, resolve the node, and either
        // buffer a `data` write or extract the Generation for a `ctl` command. The
        // lock is released before any await (a `MutexGuard` is not `Send`, and the
        // ctl path appends to `events`).
        let generation = {
            let mut state = self.state.lock().unwrap();
            let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
                return Err(ErrorCode::NoAccess);
            }
            match f.node.clone() {
                // Request document: buffer at the caller's offset until clunk
                // (commit-on-clunk), honoring out-of-order/retried writes.
                Node::GenData(_) => {
                    let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
                    let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
                    if end > MAX_DOC_BYTES {
                        return Err(ErrorCode::BadRequest);
                    }
                    let buf = &mut state
                        .fids
                        .get_mut(&fid)
                        .ok_or(ErrorCode::NotFound)?
                        .write_buf;
                    if buf.len() < end {
                        buf.resize(end, 0);
                    }
                    buf[start..end].copy_from_slice(data);
                    return Ok(data.len() as u32);
                }
                Node::GenCtl(id) => {
                    // Accept newline-terminated commands (`echo abort > ctl`).
                    if String::from_utf8_lossy(data).trim() != "abort" {
                        return Err(ErrorCode::BadRequest);
                    }
                    state.gens.get(&id).cloned().ok_or(ErrorCode::NotFound)?
                }
                _ => return Err(ErrorCode::Unsupported),
            }
        };

        // Finalize under the per-Generation lock so this abort and the drain task
        // cannot interleave: once aborted, no further chunk or `done` record is
        // written. Aborting a terminal Generation is refused (settled status).
        {
            let _guard = generation.finalize.lock().await;
            if generation.status().is_terminal() {
                return Err(ErrorCode::BadRequest);
            }
            generation.events.append(b"{\"aborted\":true}\n").await;
            generation.advance(GenStatus::Aborted);
        }
        // Wake a running drain task (or a pending provider startup) so it stops
        // promptly. `notify_one` stores a permit if no waiter is parked yet, so an
        // abort that arrives before the drain reaches `notified()` is not lost.
        generation.abort.notify_one();
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid)?;
        // Resolve the qid and length under the lock; for the `events` stream, clone
        // it out and await its length *without* the lock held.
        let (qid, len) = {
            let state = self.state.lock().unwrap();
            let qid = state.qid(&node);
            let len = match &node {
                Node::GenEvents(id) => match state.gens.get(id) {
                    Some(g) => Len::Events(g.events.clone()),
                    None => Len::Now(0),
                },
                other => Len::Now(
                    computed_bytes(&state, other)
                        .map(|b| b.len() as u64)
                        .unwrap_or(0),
                ),
            };
            (qid, len)
        };
        let length = match len {
            Len::Now(n) => n,
            Len::Events(s) => s.len().await,
        };
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            writable: is_writable(&node),
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        // Take the fid; a `data` write commits the request on clunk. Collect what
        // we need, then release the state lock before awaiting the provider.
        let commit = {
            let mut state = self.state.lock().unwrap();
            let Some(f) = state.fids.remove(&fid) else {
                return Err(ErrorCode::NotFound);
            };
            match f.node {
                // Only a *write-opened* data fid commits a request; a walked or
                // read-only data fid is just released. Otherwise an observer could
                // clunk an empty data fid and wrongly reject the Generation the real
                // writer is about to start.
                Node::GenData(id)
                    if matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) =>
                {
                    let generation = state.gens.get(&id).cloned();
                    Some((f.write_buf, generation))
                }
                _ => None,
            }
        };

        let Some((buf, Some(generation))) = commit else {
            return Ok(());
        };

        // Parse the request first (pure): an empty or invalid document is malformed.
        let doc: Result<RequestDoc, ()> = if buf.is_empty() {
            Err(())
        } else {
            serde_json::from_slice(&buf).map_err(|_| ())
        };
        let doc = match doc {
            Ok(doc) => doc,
            Err(()) => {
                // Reject only if we still own the initial transition (under the
                // finalize lock, so a racing abort can't also append a terminal
                // record): a malformed second commit cannot clobber a Generation a
                // concurrent valid commit already started.
                let _guard = generation.finalize.lock().await;
                if generation.claim(GenStatus::Rejected) {
                    generation.events.append(b"{\"rejected\":true}\n").await;
                }
                return Err(ErrorCode::BadRequest);
            }
        };

        // Reserve the Generation *before* awaiting the provider: the single
        // `open`→`running` transition. A concurrent data commit (or a post-abort
        // revive) fails here, so only one request ever reaches the provider.
        if !generation.claim(GenStatus::Running) {
            return Err(ErrorCode::BadRequest);
        }

        let mut request = GenerationRequest::new().with_user_message(doc.user);
        if let Some(system) = doc.system {
            request = request.with_system_prompt(system);
        }

        // Start the provider stream, but race it against an abort: a `ctl` abort
        // during startup drops the in-flight `generate_stream` future (cancelling
        // the provider request) instead of paying for a stream nobody will read. A
        // startup failure is terminal (error).
        let rx = {
            let mut provider = generation.connection.provider.lock().await;
            tokio::select! {
                biased;
                _ = generation.abort.notified() => {
                    // Aborted during startup: ctl already recorded the terminal
                    // state; drop the provider future and do not stream.
                    return Ok(());
                }
                result = provider.generate_stream(request) => result,
            }
        };
        let mut rx = match rx {
            Ok(rx) => rx,
            Err(_) => {
                self.fail(&generation, GenStatus::Error, "error").await;
                return Err(ErrorCode::Io);
            }
        };

        // An abort that landed just as startup finished also wins.
        if generation.status() == GenStatus::Aborted {
            return Ok(());
        }

        // Drain the provider stream into the Generation's events file.
        let events = generation.events.clone();
        let abort = generation.abort.clone();
        let drain_gen = generation.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = abort.notified() => break, // aborted: status/record already set
                    chunk = rx.recv() => match chunk {
                        Some(chunk) => {
                            // Serialize with `ctl` abort: hold the finalize lock while
                            // checking status and appending, so a concurrent abort
                            // cannot let a chunk or `done` slip in after it.
                            let _guard = drain_gen.finalize.lock().await;
                            if drain_gen.status().is_terminal() {
                                break; // aborted (or already finished) while we waited
                            }
                            if let Some(record) = chunk_record(&chunk) {
                                events.append(format!("{record}\n").as_bytes()).await;
                            }
                            if chunk.is_finished {
                                // A finished chunk carrying a `stream_error` reason is
                                // an upstream failure, not success: map it to a
                                // terminal error, not `done`.
                                let errored = chunk
                                    .finish_reason
                                    .as_deref()
                                    .is_some_and(|r| r.starts_with("stream_error"));
                                if errored {
                                    let reason = chunk.finish_reason.clone().unwrap_or_default();
                                    let record = serde_json::json!({ "error": reason }).to_string();
                                    events.append(format!("{record}\n").as_bytes()).await;
                                    drain_gen.advance(GenStatus::Error);
                                } else {
                                    events.append(b"{\"done\":true}\n").await;
                                    drain_gen.advance(GenStatus::Done);
                                }
                                break;
                            }
                        }
                        None => {
                            // The provider stream closed before a finished chunk:
                            // convert it to a terminal error so a tailing reader
                            // does not block at the live edge forever.
                            let _guard = drain_gen.finalize.lock().await;
                            if drain_gen.advance(GenStatus::Error) {
                                events.append(b"{\"error\":\"stream closed\"}\n").await;
                            }
                            break;
                        }
                    }
                }
            }
        });
        Ok(())
    }
}

impl LlmFs {
    /// Record a terminal failure (rejected/error) on a Generation before returning
    /// the commit error, so an observer of `status`/`events` sees a terminal state.
    async fn fail(&self, generation: &Generation, status: GenStatus, tag: &str) {
        // Under the finalize lock so a racing abort can't also append a terminal
        // record: only the winner of the status transition writes one.
        let _guard = generation.finalize.lock().await;
        if generation.advance(status) {
            generation
                .events
                .append(format!("{{\"{tag}\":true}}\n").as_bytes())
                .await;
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().unwrap();
        computed_bytes(&state, node)
    }
}

/// The stat length for a node: for `events` the caller awaits `Stream::len`; every
/// other surface has a synchronously-computable length.
enum Len {
    Now(u64),
    Events(Stream),
}

/// Render a readable node's bytes from already-locked state (so both `read` and
/// `stat`'s length use one definition).
fn computed_bytes(state: &State, node: &Node) -> Result<Vec<u8>, ErrorCode> {
    let bytes = match node {
        Node::Root => b"connections".to_vec(),
        Node::ConnectionsDir => {
            let mut names: Vec<_> = state.connections.keys().cloned().collect();
            names.sort();
            names.join("\n").into_bytes()
        }
        // A connection lists `clone` plus its allocated Generation ids, so a
        // permitted observer can discover live/finished Generations as files.
        Node::Connection(conn) => {
            let mut names = vec!["clone".to_string()];
            let mut ids: Vec<_> = state
                .gens
                .iter()
                .filter(|(_, g)| &g.connection_name() == conn)
                .map(|(id, _)| id.clone())
                .collect();
            ids.sort();
            names.extend(ids);
            names.join("\n").into_bytes()
        }
        Node::Gen(_) => b"data\nevents\nctl\nstatus".to_vec(),
        Node::GenStatus(id) => {
            let g = state.gens.get(id).ok_or(ErrorCode::NotFound)?;
            format!("{}\n", g.status().as_str()).into_bytes()
        }
        // clone, data, ctl, events are open/write/stream surfaces, not read here.
        _ => return Err(ErrorCode::Unsupported),
    };
    Ok(bytes)
}

/// The kind and a server-unique identity key for a node (so distinct connections
/// and Generations get distinct qids).
fn node_identity(node: &Node) -> (FileKind, String) {
    match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::ConnectionsDir => (FileKind::Dir, "connections".to_string()),
        Node::Connection(c) => (FileKind::Dir, format!("connections/{c}")),
        Node::Clone(c) => (FileKind::Clone, format!("connections/{c}/clone")),
        Node::Gen(id) => (FileKind::Dir, format!("gen/{id}")),
        Node::GenData(id) => (FileKind::File, format!("gen/{id}/data")),
        Node::GenEvents(id) => (FileKind::Stream, format!("gen/{id}/events")),
        Node::GenCtl(id) => (FileKind::File, format!("gen/{id}/ctl")),
        Node::GenStatus(id) => (FileKind::File, format!("gen/{id}/status")),
    }
}

fn hash_path(key: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

/// Build one `events` record from the meaningful fields of a stream chunk, so a
/// non-text chunk (thinking, usage, finish metadata, tool-call delta) is not
/// dropped. Returns `None` for a chunk with nothing to record.
fn chunk_record(chunk: &StreamChunk) -> Option<String> {
    let mut map = serde_json::Map::new();
    let put = |m: &mut serde_json::Map<String, serde_json::Value>, k: &str, v: &Option<String>| {
        if let Some(s) = v {
            m.insert(k.to_string(), serde_json::Value::String(s.clone()));
        }
    };
    put(&mut map, "text", &chunk.text);
    put(&mut map, "thinking", &chunk.thinking);
    put(&mut map, "thinking_signature", &chunk.thinking_signature);
    put(&mut map, "redacted_thinking", &chunk.redacted_thinking);
    put(&mut map, "finish_reason", &chunk.finish_reason);
    put(
        &mut map,
        "provider_response_id",
        &chunk.provider_response_id,
    );
    put(
        &mut map,
        "provider_response_status",
        &chunk.provider_response_status,
    );
    if let Some(seq) = chunk.sequence_number {
        map.insert("sequence_number".to_string(), seq.into());
    }
    if let Some(u) = &chunk.usage {
        map.insert(
            "usage".to_string(),
            serde_json::json!({
                "prompt_tokens": u.prompt_tokens,
                "cached_prompt_tokens": u.cached_prompt_tokens,
                "completion_tokens": u.completion_tokens,
                "total_tokens": u.total_tokens,
                "reasoning_tokens": u.reasoning_tokens,
            }),
        );
    }
    if let Some(tc) = &chunk.tool_call_delta {
        map.insert(
            "tool_call".to_string(),
            serde_json::json!({
                "index": tc.index,
                "id": tc.id,
                "name": tc.name,
                "arguments_delta": tc.arguments_delta,
                "arguments": tc.arguments,
            }),
        );
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map).to_string())
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Clone(_) | Node::GenData(_) | Node::GenCtl(_))
}

/// Whether a node has a readable surface. `data` and `ctl` are write-only sinks;
/// `clone` is readable (the caller reads the allocated id back).
fn is_readable(node: &Node) -> bool {
    !matches!(node, Node::GenData(_) | Node::GenCtl(_))
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
