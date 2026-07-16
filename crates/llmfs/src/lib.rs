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
//! This file-server boundary owns provider introspection, mounted Connections,
//! its versioned request/event DTOs, and connection-local metering and limits.
//! Provider-specific request construction remains behind `alan-llm`; callers
//! interact only through the mounted file tree.
//!
//! A Generation moves through a small lifecycle: `open` (allocated, awaiting the
//! request) → `running` (provider streaming) → a terminal state (`done`,
//! `error`, `rejected`, or `aborted`). Every path that ends a Generation writes a
//! terminal record to `events` and a terminal `status`, so a consumer tailing
//! `events` at the live edge never blocks forever.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex as StdMutex};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use alan_llm::LlmProvider;
use async_trait::async_trait;

mod generation;
mod provider_catalog;
mod request_wire;

use generation::{Connection, Generation, abort_generation, commit_request, generation_status_doc};
use provider_catalog::{
    connection_capabilities_doc, connection_profile_doc, is_known_provider, known_provider_names,
    provider_capabilities_doc, provider_capabilities_for_name, provider_models_doc,
    provider_status_doc,
};

/// Cap on a buffered request document, so a hostile writer cannot exhaust the
/// server before the commit-time validation runs.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB
const RETAIN_TERMINAL_GENERATIONS_PER_CONNECTION: usize = 16;

/// Agent-visible metadata for a callable Connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProfile {
    pub provider: String,
    pub model: String,
    pub credential_ref: String,
}

impl ConnectionProfile {
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        credential_ref: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            credential_ref: credential_ref.into(),
        }
    }
}

/// Per-Connection llmfs enforcement limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConnectionLimits {
    pub max_generations: Option<u64>,
}

impl ConnectionLimits {
    pub fn max_generations(max_generations: u64) -> Self {
        Self {
            max_generations: Some(max_generations),
        }
    }
}

/// What a fid points at within the llmfs tree.
#[derive(Clone)]
enum Node {
    Root,
    ProvidersDir,
    Provider(String),
    ProviderModels(String),
    ProviderCapabilities(String),
    ProviderStatus(String),
    ConnectionsDir,
    Connection(String),
    ConnectionProvider(String),
    ConnectionProfile(String),
    ConnectionMeter(String),
    ConnectionCapabilities(String),
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
    /// For an opened `events` fid: keep the stream reachable even if the owning
    /// Generation is later removed from the connection listing.
    events: Option<Stream>,
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
            events: None,
            clone_gen: None,
            write_buf: Vec::new(),
        }
    }
}

struct State {
    connections: HashMap<String, Arc<Connection>>,
    gens: HashMap<String, Arc<Generation>>,
    fids: HashMap<(u64, Fid), LlmFid>,
    next_gen: u64,
    next_view: u64,
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
                .map(|generation| generation.version())
                .unwrap_or(0),
            Node::ConnectionsDir
            | Node::Connection(_)
            | Node::ConnectionProvider(_)
            | Node::ConnectionProfile(_)
            | Node::ConnectionCapabilities(_) => self.listing_version,
            Node::ConnectionMeter(conn) => self
                .connections
                .get(conn)
                .map(|connection| connection.meter_version())
                .unwrap_or(self.listing_version),
            Node::Root
            | Node::ProvidersDir
            | Node::Provider(_)
            | Node::ProviderModels(_)
            | Node::ProviderCapabilities(_)
            | Node::ProviderStatus(_)
            | Node::Clone(_) => 0,
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
    state: Arc<StdMutex<State>>,
    allowed_connections: Option<Arc<HashSet<String>>>,
    view_id: u64,
}

impl Default for LlmFs {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmFs {
    pub fn new() -> Self {
        Self {
            state: Arc::new(StdMutex::new(State {
                connections: HashMap::new(),
                gens: HashMap::new(),
                fids: HashMap::new(),
                next_gen: 0,
                next_view: 1,
                listing_version: 0,
            })),
            allowed_connections: None,
            view_id: 0,
        }
    }

    /// Create a capability-narrowed view exposing only one callable Connection.
    ///
    /// The view shares live Connection and Generation state with this server,
    /// while walks and listings structurally hide every other Connection.
    pub fn connection_view(&self, name: impl Into<String>) -> Self {
        let name = name.into();
        let allowed_connections = if self.connection_visible(&name) {
            HashSet::from([name])
        } else {
            HashSet::new()
        };
        let view_id = {
            let mut state = self.state.lock().unwrap();
            let view_id = state.next_view;
            state.next_view = state
                .next_view
                .checked_add(1)
                .expect("LLM file-server view identifier space exhausted");
            view_id
        };
        Self {
            state: self.state.clone(),
            allowed_connections: Some(Arc::new(allowed_connections)),
            view_id,
        }
    }

    fn connection_visible(&self, name: &str) -> bool {
        self.allowed_connections
            .as_ref()
            .is_none_or(|allowed| allowed.contains(name))
    }

    /// Register a callable connection backed by an `alan-llm` provider. (In the
    /// full server, connections are assembled from provider + model + credential;
    /// this slice takes a ready provider.)
    pub fn register_connection(&self, name: &str, provider: Box<dyn LlmProvider>) {
        let provider_name = provider.provider_name().to_string();
        self.register_connection_inner(
            name,
            provider_name,
            None,
            None,
            ConnectionLimits::default(),
            provider,
        );
    }

    /// Register a callable connection with explicit profile metadata. The
    /// credential reference is agent-visible metadata only; plaintext credentials
    /// stay outside llmfs and are resolved by the host before constructing the
    /// provider.
    pub fn register_connection_profile(
        &self,
        name: &str,
        profile: ConnectionProfile,
        provider: Box<dyn LlmProvider>,
    ) {
        self.register_connection_inner(
            name,
            profile.provider,
            Some(profile.model),
            Some(profile.credential_ref),
            ConnectionLimits::default(),
            provider,
        );
    }

    pub fn register_connection_profile_with_limits(
        &self,
        name: &str,
        profile: ConnectionProfile,
        limits: ConnectionLimits,
        provider: Box<dyn LlmProvider>,
    ) {
        self.register_connection_inner(
            name,
            profile.provider,
            Some(profile.model),
            Some(profile.credential_ref),
            limits,
            provider,
        );
    }

    /// Publish another name for an existing callable Connection.
    pub fn register_connection_alias(&self, alias: &str, target: &str) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().unwrap();
        let connection = state
            .connections
            .get(target)
            .cloned()
            .ok_or(ErrorCode::NotFound)?;
        state.connections.insert(alias.to_string(), connection);
        state.listing_version += 1;
        Ok(())
    }

    pub async fn unregister_connection(&self, name: &str) {
        let active = {
            let mut state = self.state.lock().unwrap();
            if state.connections.remove(name).is_none() {
                return;
            }
            let mut terminal = Vec::new();
            let mut active = Vec::new();
            for (id, generation) in &state.gens {
                if generation.connection_name() != name {
                    continue;
                }
                if generation.is_terminal() {
                    terminal.push(id.clone());
                } else {
                    active.push((id.clone(), generation.clone()));
                }
            }
            for id in terminal {
                state.gens.remove(&id);
            }
            state.listing_version += 1;
            active
        };

        for (_, generation) in &active {
            let _ = abort_generation(generation).await;
        }
        if !active.is_empty() {
            let mut state = self.state.lock().unwrap();
            for (id, _) in active {
                state.gens.remove(&id);
            }
            state.listing_version += 1;
        }
    }

    fn register_connection_inner(
        &self,
        name: &str,
        provider_name: String,
        model: Option<String>,
        credential_ref: Option<String>,
        limits: ConnectionLimits,
        provider: Box<dyn LlmProvider>,
    ) {
        let capabilities = provider_capabilities_for_name(&provider_name);
        let mut state = self.state.lock().unwrap();
        state.connections.insert(
            name.to_string(),
            Arc::new(Connection::new(
                provider_name,
                model,
                credential_ref,
                capabilities,
                limits,
                provider,
            )),
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
            .get(&(self.view_id, fid))
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        let state = self.state.lock().unwrap();
        match node {
            Node::Root if name == "connections" => Ok(Node::ConnectionsDir),
            Node::Root if name == "providers" => Ok(Node::ProvidersDir),
            Node::ProvidersDir if is_known_provider(name) => Ok(Node::Provider(name.to_string())),
            Node::Provider(provider) => match name {
                "models" => Ok(Node::ProviderModels(provider.clone())),
                "capabilities" => Ok(Node::ProviderCapabilities(provider.clone())),
                "status" => Ok(Node::ProviderStatus(provider.clone())),
                _ => Err(ErrorCode::NotFound),
            },
            Node::ConnectionsDir
                if self.connection_visible(name) && state.connections.contains_key(name) =>
            {
                Ok(Node::Connection(name.to_string()))
            }
            Node::ConnectionsDir => Err(ErrorCode::NotFound),
            Node::Connection(conn) => {
                if name == "clone" {
                    Ok(Node::Clone(conn.clone()))
                } else if name == "provider" {
                    Ok(Node::ConnectionProvider(conn.clone()))
                } else if name == "profile" {
                    Ok(Node::ConnectionProfile(conn.clone()))
                } else if name == "meter" {
                    Ok(Node::ConnectionMeter(conn.clone()))
                } else if name == "capabilities" {
                    Ok(Node::ConnectionCapabilities(conn.clone()))
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
        let newfid = (self.view_id, newfid);
        if state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, LlmFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().unwrap();
        let fid_key = (self.view_id, fid);
        // A fid opens once: a second open would allocate a second Generation on a
        // clone file or re-establish intent, so reject it.
        if state.fids.get(&fid_key).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        let node = if fid == Fid::ROOT {
            Node::Root
        } else {
            state
                .fids
                .get(&fid_key)
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
            connection.try_reserve_generation()?;
            let id = format!("g{}", state.next_gen);
            let sequence = state.next_gen;
            state.next_gen += 1;
            state.listing_version += 1;
            state.gens.insert(
                id.clone(),
                Arc::new(Generation::new(connection, conn.clone(), sequence)),
            );
            if let Some(f) = state.fids.get_mut(&fid_key) {
                f.clone_gen = Some(id);
            }
            reap_terminal_generations(&mut state, conn);
        }

        let opened_events = if let Node::GenEvents(id) = &node {
            Some(state.gens.get(id).ok_or(ErrorCode::NotFound)?.events())
        } else {
            None
        };
        let qid = state.qid(&node);
        if let Some(f) = state.fids.get_mut(&fid_key) {
            f.mode = Some(mode);
            f.events = opened_events;
        }
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        // Reads need read authority from a successful read-open (ROOT is the
        // pre-bound anchor and is always readable).
        let (node, clone_gen, opened_events) = {
            let state = self.state.lock().unwrap();
            if fid == Fid::ROOT {
                (Node::Root, None, None)
            } else {
                let f = state
                    .fids
                    .get(&(self.view_id, fid))
                    .ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
                (f.node.clone(), f.clone_gen.clone(), f.events.clone())
            }
        };
        // An opened clone fid reads back the allocated Generation id.
        if let Some(id) = clone_gen {
            return Ok(slice(id.into_bytes(), offset, count));
        }

        // Stream node: clone the Stream out, then read without holding the lock.
        if let Node::GenEvents(id) = &node {
            let events = if let Some(events) = opened_events {
                events
            } else {
                let state = self.state.lock().unwrap();
                state.gens.get(id).ok_or(ErrorCode::NotFound)?.events()
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
            let fid = (self.view_id, fid);
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

        abort_generation(&generation).await?;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        // Resolve the qid and length under the lock; for the `events` stream, clone
        // it out and await its length *without* the lock held.
        let (qid, len, writable) = {
            let state = self.state.lock().unwrap();
            let (node, opened_events) = if fid == Fid::ROOT {
                (Node::Root, None)
            } else {
                let f = state
                    .fids
                    .get(&(self.view_id, fid))
                    .ok_or(ErrorCode::NotFound)?;
                (f.node.clone(), f.events.clone())
            };
            let qid = state.qid(&node);
            let len = match &node {
                Node::GenEvents(id) => {
                    if let Some(events) = opened_events {
                        Len::Events(events)
                    } else {
                        match state.gens.get(id) {
                            Some(generation) => Len::Events(generation.events()),
                            None => Len::Now(0),
                        }
                    }
                }
                other => Len::Now(
                    computed_bytes(&state, other, self.allowed_connections.as_deref())
                        .map(|b| b.len() as u64)
                        .unwrap_or(0),
                ),
            };
            let writable = is_writable(&node);
            (qid, len, writable)
        };
        let length = match len {
            Len::Now(n) => n,
            Len::Events(s) => s.len().await,
        };
        Ok(Stat {
            name: String::new(),
            qid,
            length,
            executable: false,
            writable,
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
            let Some(f) = state.fids.remove(&(self.view_id, fid)) else {
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
                    let generation = state.gens.get(&id).cloned().ok_or(ErrorCode::BadRequest)?;
                    Some((f.write_buf, generation))
                }
                _ => None,
            }
        };

        let Some((buf, generation)) = commit else {
            return Ok(());
        };

        commit_request(buf, generation).await
    }
}

impl LlmFs {
    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().unwrap();
        computed_bytes(&state, node, self.allowed_connections.as_deref())
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
fn computed_bytes(
    state: &State,
    node: &Node,
    allowed_connections: Option<&HashSet<String>>,
) -> Result<Vec<u8>, ErrorCode> {
    let bytes = match node {
        Node::Root => b"connections\nproviders".to_vec(),
        Node::ProvidersDir => known_provider_names().join("\n").into_bytes(),
        Node::Provider(_) => b"models\ncapabilities\nstatus".to_vec(),
        Node::ProviderModels(provider) => provider_models_doc(provider).into_bytes(),
        Node::ProviderCapabilities(provider) => {
            provider_capabilities_doc(provider, provider_capabilities_for_name(provider))
                .into_bytes()
        }
        Node::ProviderStatus(provider) => provider_status_doc(provider).into_bytes(),
        Node::ConnectionsDir => {
            let mut names: Vec<_> = state
                .connections
                .keys()
                .filter(|name| allowed_connections.is_none_or(|allowed| allowed.contains(*name)))
                .cloned()
                .collect();
            names.sort();
            names.join("\n").into_bytes()
        }
        // A connection lists `clone` plus its allocated Generation ids, so a
        // permitted observer can discover live/finished Generations as files.
        Node::Connection(conn) => {
            let mut names = vec![
                "clone".to_string(),
                "provider".to_string(),
                "profile".to_string(),
                "meter".to_string(),
                "capabilities".to_string(),
            ];
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
        Node::ConnectionProvider(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            format!("{}\n", connection.provider_name).into_bytes()
        }
        Node::ConnectionProfile(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection_profile_doc(
                conn,
                &connection.provider_name,
                connection.model.as_deref(),
                connection.credential_ref.as_deref(),
            )
            .into_bytes()
        }
        Node::ConnectionMeter(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection.meter_doc(conn).into_bytes()
        }
        Node::ConnectionCapabilities(conn) => {
            let connection = state.connections.get(conn).ok_or(ErrorCode::NotFound)?;
            connection_capabilities_doc(conn, &connection.provider_name, connection.capabilities)
                .into_bytes()
        }
        Node::Gen(_) => b"data\nevents\nctl\nstatus".to_vec(),
        Node::GenStatus(id) => {
            let g = state.gens.get(id).ok_or(ErrorCode::NotFound)?;
            generation_status_doc(id, g).into_bytes()
        }
        // clone, data, ctl, events are open/write/stream surfaces, not read here.
        _ => return Err(ErrorCode::Unsupported),
    };
    Ok(bytes)
}

fn reap_terminal_generations(state: &mut State, connection: &str) {
    let mut terminal_generations = state
        .gens
        .iter()
        .filter(|(_, generation)| {
            generation.connection_name() == connection && generation.is_terminal()
        })
        .map(|(id, generation)| (id.clone(), generation.sequence()))
        .collect::<Vec<_>>();
    if terminal_generations.len() <= RETAIN_TERMINAL_GENERATIONS_PER_CONNECTION {
        return;
    }

    terminal_generations.sort_by_key(|(_, sequence)| *sequence);
    let remove_count = terminal_generations.len() - RETAIN_TERMINAL_GENERATIONS_PER_CONNECTION;
    for (id, _) in terminal_generations.into_iter().take(remove_count) {
        state.gens.remove(&id);
        state.listing_version += 1;
    }
}

/// The kind and a server-unique identity key for a node (so distinct connections
/// and Generations get distinct qids).
fn node_identity(node: &Node) -> (FileKind, String) {
    match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::ProvidersDir => (FileKind::Dir, "providers".to_string()),
        Node::Provider(provider) => (FileKind::Dir, format!("providers/{provider}")),
        Node::ProviderModels(provider) => (FileKind::File, format!("providers/{provider}/models")),
        Node::ProviderCapabilities(provider) => {
            (FileKind::File, format!("providers/{provider}/capabilities"))
        }
        Node::ProviderStatus(provider) => (FileKind::File, format!("providers/{provider}/status")),
        Node::ConnectionsDir => (FileKind::Dir, "connections".to_string()),
        Node::Connection(c) => (FileKind::Dir, format!("connections/{c}")),
        Node::ConnectionProvider(c) => (FileKind::File, format!("connections/{c}/provider")),
        Node::ConnectionProfile(c) => (FileKind::File, format!("connections/{c}/profile")),
        Node::ConnectionMeter(c) => (FileKind::File, format!("connections/{c}/meter")),
        Node::ConnectionCapabilities(c) => {
            (FileKind::File, format!("connections/{c}/capabilities"))
        }
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

fn render_json_doc(value: serde_json::Value) -> String {
    let mut rendered = serde_json::to_string(&value).expect("serialize llmfs introspection doc");
    rendered.push('\n');
    rendered
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
