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

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use alan_llm::{GenerationRequest, LlmProvider};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::Mutex as AsyncMutex;

/// The neutral request document written to a Generation's `data` file. Minimal
/// for this slice; the full versioned wire DTO is deferred.
#[derive(Deserialize)]
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

/// One Generation's projected surfaces.
struct Generation {
    connection: String,
    events: Stream,
    status: StdMutex<&'static str>,
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
    /// For a fid that opened a `clone` file: the allocated Generation id.
    clone_gen: Option<String>,
    /// Buffered request document for a `data` fid (commit-on-clunk).
    write_buf: Vec<u8>,
}

impl LlmFid {
    fn at(node: Node) -> Self {
        Self {
            node,
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
                } else if state.gens.get(name).is_some_and(|g| &g.connection == conn) {
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
        let mut node = self.node_of(fid)?;
        for name in names {
            node = self.child(&node, name)?;
        }
        let qid = qid_of(&node);
        self.state
            .lock()
            .unwrap()
            .fids
            .insert(newfid, LlmFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = self.node_of(fid)?;
        // Clone-via-open: allocate a fresh Generation under the connection.
        if let Node::Clone(conn) = &node {
            let mut state = self.state.lock().unwrap();
            let id = format!("g{}", state.next_gen);
            state.next_gen += 1;
            state.gens.insert(
                id.clone(),
                Arc::new(Generation {
                    connection: conn.clone(),
                    events: Stream::new(),
                    status: StdMutex::new("open"),
                }),
            );
            if let Some(f) = state.fids.get_mut(&fid) {
                f.clone_gen = Some(id);
            }
        }
        Ok(qid_of(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        // An opened clone fid reads back the allocated Generation id.
        let (node, clone_gen) = {
            let state = self.state.lock().unwrap();
            let f = state.fids.get(&fid);
            (
                f.map(|f| f.node.clone()).or(if fid == Fid::ROOT {
                    Some(Node::Root)
                } else {
                    None
                }),
                f.and_then(|f| f.clone_gen.clone()),
            )
        };
        if let Some(id) = clone_gen {
            return Ok(slice(id.into_bytes(), offset, count));
        }
        let node = node.ok_or(ErrorCode::NotFound)?;

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

    async fn write(&self, fid: Fid, _offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let node = self.node_of(fid)?;
        match node {
            // Request document: buffer until clunk (commit-on-clunk).
            Node::GenData(_) => {
                let mut state = self.state.lock().unwrap();
                state
                    .fids
                    .get_mut(&fid)
                    .ok_or(ErrorCode::NotFound)?
                    .write_buf
                    .extend_from_slice(data);
                Ok(data.len() as u32)
            }
            Node::GenCtl(id) => {
                if data == b"abort" {
                    let state = self.state.lock().unwrap();
                    if let Some(g) = state.gens.get(&id) {
                        *g.status.lock().unwrap() = "aborted";
                    }
                    Ok(data.len() as u32)
                } else {
                    Err(ErrorCode::BadRequest)
                }
            }
            _ => Err(ErrorCode::Unsupported),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid)?;
        Ok(Stat {
            name: String::new(),
            qid: qid_of(&node),
            length: 0,
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
        // Take the fid; if it was a `data` write, commit-on-clunk starts the
        // Generation. Collect what we need, then release the state lock before
        // awaiting the provider.
        let commit = {
            let mut state = self.state.lock().unwrap();
            let Some(f) = state.fids.remove(&fid) else {
                return Err(ErrorCode::NotFound);
            };
            match f.node {
                Node::GenData(id) if !f.write_buf.is_empty() => {
                    let generation = state.gens.get(&id).cloned();
                    let conn = generation
                        .as_ref()
                        .and_then(|g| state.connections.get(&g.connection).cloned());
                    Some((f.write_buf, generation, conn))
                }
                _ => None,
            }
        };

        let Some((buf, Some(generation), Some(conn))) = commit else {
            return Ok(());
        };

        // Parse the request document; a malformed document is a commit-time error.
        let doc: RequestDoc = serde_json::from_slice(&buf).map_err(|_| ErrorCode::BadRequest)?;
        let mut request = GenerationRequest::new().with_user_message(doc.user);
        if let Some(system) = doc.system {
            request = request.with_system_prompt(system);
        }

        let mut rx = {
            let mut provider = conn.provider.lock().await;
            provider
                .generate_stream(request)
                .await
                .map_err(|_| ErrorCode::Io)?
        };
        *generation.status.lock().unwrap() = "running";

        // Drain the provider stream into the Generation's events file.
        let events = generation.events.clone();
        let generation = generation.clone();
        tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                if let Some(text) = chunk.text {
                    let record = serde_json::json!({ "text": text }).to_string();
                    events.append(format!("{record}\n").as_bytes()).await;
                }
                if chunk.is_finished {
                    events.append(b"{\"done\":true}\n").await;
                    *generation.status.lock().unwrap() = "done";
                    break;
                }
            }
        });
        Ok(())
    }
}

impl LlmFs {
    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().unwrap();
        let bytes = match node {
            Node::Root => b"connections".to_vec(),
            Node::ConnectionsDir => {
                let mut names: Vec<_> = state.connections.keys().cloned().collect();
                names.sort();
                names.join("\n").into_bytes()
            }
            Node::Connection(_) => b"clone".to_vec(),
            Node::Gen(_) => b"data\nevents\nctl\nstatus".to_vec(),
            Node::GenStatus(id) => {
                let g = state.gens.get(id).ok_or(ErrorCode::NotFound)?;
                format!("{}\n", g.status.lock().unwrap()).into_bytes()
            }
            // clone, data, ctl, events are open/write/stream surfaces, not read here.
            _ => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }
}

fn qid_of(node: &Node) -> Qid {
    let (kind, path) = match node {
        Node::Root | Node::ConnectionsDir | Node::Connection(_) | Node::Gen(_) => {
            (FileKind::Dir, 0)
        }
        Node::Clone(_) => (FileKind::Clone, 1),
        Node::GenEvents(_) => (FileKind::Stream, 2),
        Node::GenData(_) | Node::GenCtl(_) | Node::GenStatus(_) => (FileKind::File, 3),
    };
    Qid {
        kind,
        version: 0,
        path,
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Clone(_) | Node::GenData(_) | Node::GenCtl(_))
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
