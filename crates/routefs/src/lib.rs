//! alan-routefs — typed message routing as an aP file server.
//!
//! A sender writes one typed message document to `send` and commits it by
//! clunking the fid. Routefs matches the complete message against plain rule
//! files in deterministic name order, delivers it to a destination port stream,
//! and appends the routing decision to `log`. The sender emits a result type; it
//! does not name the receiving actor.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream, VersionTable,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Canonical `/srv` handle name for the routefs server.
pub const SRV_HANDLE: &str = "route";
/// Canonical mount path for routefs.
pub const MOUNT_PATH: &str = "/mnt/route";
/// Default port for unrouted messages.
pub const DEAD_LETTER_PORT: &str = "dead-letter";

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

/// A plain JSON rule file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSpec {
    pub version: u16,
    /// Message type to match. If omitted, any type matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    /// Substring required in the message content. If omitted, content is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
    /// Destination port name under `ports/`.
    pub port: String,
    /// Human-readable reason, retained in the log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RuleSpec {
    /// Rule matching a message type.
    pub fn for_type(message_type: impl Into<String>, port: impl Into<String>) -> Self {
        Self {
            version: 1,
            match_type: Some(message_type.into()),
            content_contains: None,
            port: port.into(),
            reason: None,
        }
    }

    /// Attach a content substring condition.
    pub fn with_content_contains(mut self, content: impl Into<String>) -> Self {
        self.content_contains = Some(content.into());
        self
    }

    /// Attach an inspectable reason.
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != 1 || !valid_name(&self.port) {
            return Err(ErrorCode::BadRequest);
        }
        if self.match_type.as_deref().is_some_and(str::is_empty)
            || self.content_contains.as_deref().is_some_and(str::is_empty)
        {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }

    fn matches(&self, message: &MessageDocument) -> bool {
        let type_matches = self
            .match_type
            .as_deref()
            .is_none_or(|expected| expected == message.message_type);
        let content_matches = self
            .content_contains
            .as_deref()
            .is_none_or(|needle| message.content.contains(needle));
        type_matches && content_matches
    }
}

#[derive(Debug, Clone)]
struct MessageDocument {
    message_type: String,
    content: String,
    raw: serde_json::Value,
}

impl MessageDocument {
    fn parse(bytes: &[u8]) -> Result<Self, ErrorCode> {
        let raw: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let object = raw.as_object().ok_or(ErrorCode::BadRequest)?;
        let version = object
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or(ErrorCode::BadRequest)?;
        let message_type = object
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or(ErrorCode::BadRequest)?
            .to_string();
        let content = object
            .get("content")
            .and_then(serde_json::Value::as_str)
            .ok_or(ErrorCode::BadRequest)?
            .to_string();
        if version != 1 || message_type.is_empty() {
            return Err(ErrorCode::BadRequest);
        }
        Ok(Self {
            message_type,
            content,
            raw,
        })
    }
}

#[derive(Serialize)]
struct RoutedRecord<'a> {
    version: u16,
    port: &'a str,
    rule: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    message: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
struct RuleEntry {
    spec: RuleSpec,
    source: Vec<u8>,
}

struct State {
    rules: BTreeMap<String, RuleEntry>,
    pending_rules: BTreeSet<String>,
    ports: BTreeMap<String, Stream>,
    log: Stream,
    versions: VersionTable,
    fids: HashMap<Fid, RouteFid>,
}

struct RouteFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
}

#[derive(Debug, Clone)]
enum Node {
    Root,
    Send,
    RulesDir,
    Rule(String),
    PortsDir,
    Port(String),
    Log,
}

/// The message routing file server.
pub struct RouteFs {
    state: Mutex<State>,
}

impl Default for RouteFs {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteFs {
    /// Create an empty routefs with only the dead-letter port.
    pub fn new() -> Self {
        let mut ports = BTreeMap::new();
        ports.insert(DEAD_LETTER_PORT.to_string(), Stream::new());
        Self {
            state: Mutex::new(State {
                rules: BTreeMap::new(),
                pending_rules: BTreeSet::new(),
                ports,
                log: Stream::new(),
                versions: VersionTable::new(),
                fids: HashMap::new(),
            }),
        }
    }

    /// Install or replace a rule without going through the aP file surface.
    /// Tests and bootstrap code may use this; the rule remains readable as
    /// `rules/<name>`.
    pub async fn install_rule(
        &self,
        name: impl Into<String>,
        spec: RuleSpec,
    ) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let name = name.into();
        state.install_rule(name, spec)
    }
}

impl State {
    fn node_of(&self, fid: Fid) -> Result<Node, ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(Node::Root);
        }
        self.fids
            .get(&fid)
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn qid(&self, node: &Node) -> Qid {
        let (kind, path) = node_identity(node);
        Qid {
            kind,
            version: self.versions.get(path),
            path,
        }
    }

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        match node {
            Node::Root => match name {
                "send" => Ok(Node::Send),
                "rules" => Ok(Node::RulesDir),
                "ports" => Ok(Node::PortsDir),
                "log" => Ok(Node::Log),
                _ => Err(ErrorCode::NotFound),
            },
            Node::RulesDir => {
                if self.rules.contains_key(name) {
                    Ok(Node::Rule(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            Node::PortsDir => {
                if self.ports.contains_key(name) {
                    Ok(Node::Port(name.to_string()))
                } else {
                    Err(ErrorCode::NotFound)
                }
            }
            _ => Err(ErrorCode::NotDirectory),
        }
    }

    fn computed_bytes(&self, node: &Node) -> Result<Vec<u8>, ErrorCode> {
        let bytes = match node {
            Node::Root => b"send\nrules\nports\nlog".to_vec(),
            Node::Send => b"# routefs send: write one message JSON document, then clunk\n".to_vec(),
            Node::RulesDir => listing(self.rules.keys()),
            Node::PortsDir => listing(self.ports.keys()),
            Node::Rule(name) => self
                .rules
                .get(name)
                .ok_or(ErrorCode::NotFound)?
                .source
                .clone(),
            Node::Port(_) | Node::Log => return Err(ErrorCode::Unsupported),
        };
        Ok(bytes)
    }

    fn stream_for(&self, node: &Node) -> Option<Stream> {
        match node {
            Node::Port(name) => self.ports.get(name).cloned(),
            Node::Log => Some(self.log.clone()),
            _ => None,
        }
    }

    fn install_rule(&mut self, name: String, spec: RuleSpec) -> Result<(), ErrorCode> {
        if !valid_name(&name) {
            return Err(ErrorCode::BadRequest);
        }
        if self.pending_rules.contains(&name) {
            return Err(ErrorCode::BadRequest);
        }
        spec.validate()?;
        let source = rule_source(&spec)?;
        let port = spec.port.clone();
        self.rules.insert(name.clone(), RuleEntry { spec, source });
        self.ensure_port(&port);
        self.versions.bump(node_identity(&Node::RulesDir).1);
        self.versions.bump(node_identity(&Node::Rule(name)).1);
        Ok(())
    }

    fn ensure_port(&mut self, port: &str) {
        if !self.ports.contains_key(port) {
            self.ports.insert(port.to_string(), Stream::new());
            self.versions.bump(node_identity(&Node::PortsDir).1);
        }
    }

    async fn route_message(&mut self, bytes: &[u8]) -> Result<(), ErrorCode> {
        let message = MessageDocument::parse(bytes)?;

        let decision = self
            .rules
            .iter()
            .find(|(_, rule)| rule.spec.matches(&message))
            .map(|(name, rule)| {
                (
                    rule.spec.port.clone(),
                    name.clone(),
                    rule.spec.reason.clone(),
                )
            })
            .unwrap_or_else(|| {
                (
                    DEAD_LETTER_PORT.to_string(),
                    "dead-letter".to_string(),
                    Some("no matching rule".to_string()),
                )
            });
        let (port, rule_name, reason) = decision;
        self.ensure_port(&port);
        let record = RoutedRecord {
            version: 1,
            port: &port,
            rule: &rule_name,
            reason: reason.as_deref(),
            message: &message.raw,
        };
        let bytes = routed_record_bytes(&record)?;
        let stream = self.ports.get(&port).ok_or(ErrorCode::NotFound)?.clone();
        stream.append(&bytes).await;
        self.log.append(&bytes).await;
        Ok(())
    }

    fn commit_pending_rule(&mut self, name: String, spec: RuleSpec) -> Result<(), ErrorCode> {
        self.pending_rules.remove(&name);
        self.install_rule(name, spec)
    }

    fn abandon_pending_rule(&mut self, name: &str) {
        self.pending_rules.remove(name);
    }
}

#[async_trait]
impl FileServer for RouteFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = state.node_of(fid)?;
        for name in names {
            node = state.child(&node, name)?;
        }
        let qid = state.qid(&node);
        state.fids.insert(newfid, RouteFid::at(node));
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        if fid != Fid::ROOT && state.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        if matches!(mode, OpenMode::Write | OpenMode::ReadWrite) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        if fid != Fid::ROOT {
            let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(state.qid(&node))
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let (node, stream) = {
            let state = self.state.lock().await;
            if fid != Fid::ROOT {
                let f = state.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
                if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                    return Err(ErrorCode::NoAccess);
                }
            }
            let node = state.node_of(fid)?;
            let stream = state.stream_for(&node);
            (node, stream)
        };
        if let Some(stream) = stream {
            return Ok(stream.read(offset, count).await);
        }
        let state = self.state.lock().await;
        Ok(slice(state.computed_bytes(&node)?, offset, count))
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let f = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if !matches!(f.mode, Some(OpenMode::Write | OpenMode::ReadWrite)) {
            return Err(ErrorCode::NoAccess);
        }
        if !matches!(node, Node::Send | Node::Rule(_)) {
            return Err(ErrorCode::Unsupported);
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_DOC_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        if f.write_buf.len() < end {
            f.write_buf.resize(end, 0);
        }
        f.write_buf[start..end].copy_from_slice(data);
        f.wrote = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let length = match &node {
            Node::Port(_) | Node::Log => {
                state
                    .stream_for(&node)
                    .ok_or(ErrorCode::NotFound)?
                    .len()
                    .await
            }
            other => state.computed_bytes(other)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: state.qid(&node),
            length,
            writable: is_writable(&node),
        })
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if kind != FileKind::File || !valid_name(name) {
            return Err(ErrorCode::BadRequest);
        }
        let mut state = self.state.lock().await;
        if newfid == Fid::ROOT || state.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        if !matches!(state.node_of(fid)?, Node::RulesDir)
            || state.rules.contains_key(name)
            || state.pending_rules.contains(name)
        {
            return Err(ErrorCode::BadRequest);
        }
        state.pending_rules.insert(name.to_string());
        let node = Node::Rule(name.to_string());
        let qid = state.qid(&node);
        state.fids.insert(newfid, RouteFid::at(node));
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let node = state.node_of(fid)?;
        let Node::Rule(name) = node else {
            return Err(ErrorCode::Unsupported);
        };
        let removed = state.rules.remove(&name).is_some();
        let reserved = state.pending_rules.remove(&name);
        if !removed && !reserved {
            return Err(ErrorCode::NotFound);
        }
        state.versions.bump(node_identity(&Node::RulesDir).1);
        state.fids.remove(&fid);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let mut state = self.state.lock().await;
        let f = state.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        match f.node {
            Node::Send if f.wrote => state.route_message(&f.write_buf).await,
            Node::Rule(name) if f.wrote => {
                let spec: RuleSpec =
                    serde_json::from_slice(&f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
                state.commit_pending_rule(name, spec)
            }
            Node::Rule(name) => {
                state.abandon_pending_rule(&name);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl RouteFid {
    fn at(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
        }
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Send | Node::Rule(_))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\n')
}

fn listing<'a>(names: impl Iterator<Item = &'a String>) -> Vec<u8> {
    names.cloned().collect::<Vec<_>>().join("\n").into_bytes()
}

fn rule_source(spec: &RuleSpec) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec(spec).map_err(|_| ErrorCode::BadRequest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn routed_record_bytes(record: &RoutedRecord<'_>) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec(record).map_err(|_| ErrorCode::BadRequest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn node_identity(node: &Node) -> (FileKind, u64) {
    let (kind, key) = match node {
        Node::Root => (FileKind::Dir, "/".to_string()),
        Node::Send => (FileKind::File, "send".to_string()),
        Node::RulesDir => (FileKind::Dir, "rules".to_string()),
        Node::Rule(name) => (FileKind::File, format!("rules/{name}")),
        Node::PortsDir => (FileKind::Dir, "ports".to_string()),
        Node::Port(name) => (FileKind::Stream, format!("ports/{name}")),
        Node::Log => (FileKind::Stream, "log".to_string()),
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    (kind, h.finish())
}

fn slice(bytes: Vec<u8>, offset: Offset, count: u32) -> Vec<u8> {
    let start = (offset as usize).min(bytes.len());
    let end = bytes.len().min(start + count as usize);
    bytes[start..end].to_vec()
}
