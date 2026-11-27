//! Single-owner [`FileServerActor`] implementation for `alan-routefs`.
//!
//! All routing state, rule tables, fid buffers, and stream handles live inside
//! [`RouteFsActor`] without any internal `tokio::sync::Mutex`. Blocking
//! `FileKind::Stream` reads on `ports/<name>` and `log` are offloaded outside
//! the actor mailbox via [`ActorOutcome::ReadStream`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use alan_ap::{
    ActorOutcome, ErrorCode, Fid, FileKind, FileServerActor, Offset, OpenMode, Qid, Stat, Stream,
    VersionTable,
};
use serde::Serialize;

use crate::handoff::{
    LOOP_GUARD_RULE_NAME, MAX_HANDOFF_HOPS, RouteHandoffMessage, parse_optional_hop_count,
    workstream_handoff_rules,
};
use crate::{DEAD_LETTER_PORT, ROOT_PORT, RuleSpec};

const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB

#[derive(Debug, Clone)]
pub(crate) struct MessageDocument {
    pub(crate) message_type: String,
    pub(crate) content: String,
    hop_count: Option<u8>,
    raw: serde_json::Value,
}

impl MessageDocument {
    fn parse(bytes: &[u8]) -> Result<Self, ErrorCode> {
        let raw: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| ErrorCode::BadRequest)?;
        let object = raw.as_object().ok_or(ErrorCode::BadRequest)?;
        if object.contains_key("kind") {
            let handoff: RouteHandoffMessage =
                serde_json::from_value(raw.clone()).map_err(|_| ErrorCode::BadRequest)?;
            handoff.validate()?;
            if let Some(explicit_type) = object.get("type")
                && explicit_type.as_str() != Some(handoff.kind.as_str())
            {
                return Err(ErrorCode::BadRequest);
            }
            return Ok(Self {
                message_type: handoff.kind.as_str().to_string(),
                content: handoff.payload,
                hop_count: Some(handoff.hop_count),
                raw,
            });
        }
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
        let hop_count = parse_optional_hop_count(object)?;
        if version != 1 || message_type.is_empty() {
            return Err(ErrorCode::BadRequest);
        }
        Ok(Self {
            message_type,
            content,
            hop_count,
            raw,
        })
    }
}

#[derive(Serialize)]
struct RoutedRecordRef<'a> {
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

struct RouteFid {
    node: Node,
    mode: Option<OpenMode>,
    write_buf: Vec<u8>,
    wrote: bool,
    pending_rule_create: bool,
    observed_rule_version: Option<u32>,
}

impl RouteFid {
    fn at(node: Node, qid: Qid) -> Self {
        let observed_rule_version = matches!(node, Node::Rule(_)).then_some(qid.version);
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
            pending_rule_create: false,
            observed_rule_version,
        }
    }

    fn pending_rule(node: Node) -> Self {
        Self {
            node,
            mode: None,
            write_buf: Vec::new(),
            wrote: false,
            pending_rule_create: true,
            observed_rule_version: None,
        }
    }
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

/// Single-owner synchronous [`FileServerActor`] owning all `routefs` state.
///
/// Spawn this actor directly via [`alan_ap::spawn_file_actor`] or
/// [`crate::RouteFs::spawn`] to execute message routing inside a crash-isolated
/// Tokio task without shared `tokio::sync::Mutex` locks.
pub struct RouteFsActor {
    rules: BTreeMap<String, RuleEntry>,
    pending_rules: BTreeSet<String>,
    ports: BTreeMap<String, Stream>,
    log: Stream,
    versions: VersionTable,
    fids: HashMap<Fid, RouteFid>,
}

impl Default for RouteFsActor {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteFsActor {
    /// Create an empty `RouteFsActor` with the default `dead-letter` and `root` ports.
    pub fn new() -> Self {
        let mut ports = BTreeMap::new();
        ports.insert(DEAD_LETTER_PORT.to_string(), Stream::new());
        ports.insert(ROOT_PORT.to_string(), Stream::new());
        Self {
            rules: BTreeMap::new(),
            pending_rules: BTreeSet::new(),
            ports,
            log: Stream::new(),
            versions: VersionTable::new(),
            fids: HashMap::new(),
        }
    }

    /// Create a `RouteFsActor` pre-populated with default workstream handoff
    /// rules routing `reroute_request`, `cross_stream_dependency`, and
    /// `handoff_result` to `/mnt/route/ports/root`.
    pub fn new_with_workstream_handoff_rules() -> Self {
        let mut actor = Self::new();
        let _ = actor.install_workstream_handoff_rules();
        actor
    }

    /// Install the default inspectable workstream handoff rules routing typed
    /// cross-stream messages to `/mnt/route/ports/root`.
    pub fn install_workstream_handoff_rules(&mut self) -> Result<(), ErrorCode> {
        for (name, spec) in workstream_handoff_rules() {
            self.install_rule(name, spec)?;
        }
        Ok(())
    }

    /// Install or replace a rule directly on the actor state before or during
    /// execution; the rule remains readable as `rules/<name>`.
    pub fn install_rule(
        &mut self,
        name: impl Into<String>,
        spec: RuleSpec,
    ) -> Result<(), ErrorCode> {
        let name = name.into();
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

    fn current_rule_version(&self, name: &str) -> Option<u32> {
        self.rules.contains_key(name).then(|| {
            self.versions
                .get(node_identity(&Node::Rule(name.to_string())).1)
        })
    }

    fn ensure_current_rule_fid(&self, f: &RouteFid, name: &str) -> Result<(), ErrorCode> {
        if f.pending_rule_create {
            return Ok(());
        }
        let observed = f.observed_rule_version.ok_or(ErrorCode::BadRequest)?;
        let current = self.current_rule_version(name).ok_or(ErrorCode::NotFound)?;
        if observed != current {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }

    fn ensure_port(&mut self, port: &str) {
        if !self.ports.contains_key(port) {
            self.ports.insert(port.to_string(), Stream::new());
            self.versions.bump(node_identity(&Node::PortsDir).1);
        }
    }

    fn route_message(&mut self, bytes: &[u8]) -> Result<(), ErrorCode> {
        let message = MessageDocument::parse(bytes)?;

        let decision = if let Some(hop_count) =
            message.hop_count.filter(|&hops| hops > MAX_HANDOFF_HOPS)
        {
            (
                DEAD_LETTER_PORT.to_string(),
                LOOP_GUARD_RULE_NAME.to_string(),
                Some(format!(
                    "handoff hop_count {hop_count} exceeds max_handoff_hops={MAX_HANDOFF_HOPS}; escalated to {DEAD_LETTER_PORT}"
                )),
            )
        } else {
            self.rules
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
                })
        };
        let (port, rule_name, reason) = decision;
        self.ensure_port(&port);
        let record = RoutedRecordRef {
            version: 1,
            port: &port,
            rule: &rule_name,
            reason: reason.as_deref(),
            message: &message.raw,
        };
        let bytes = routed_record_bytes(&record)?;
        let stream = self.ports.get(&port).ok_or(ErrorCode::NotFound)?.clone();
        stream.append_sync(&bytes);
        self.log.append_sync(&bytes);
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

impl FileServerActor for RouteFsActor {
    fn walk(&mut self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if newfid == Fid::ROOT || self.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        let mut node = self.node_of(fid)?;
        for name in names {
            node = self.child(&node, name)?;
        }
        let qid = self.qid(&node);
        self.fids.insert(newfid, RouteFid::at(node, qid));
        Ok(qid)
    }

    fn open(&mut self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = self.node_of(fid)?;
        if fid != Fid::ROOT && self.fids.get(&fid).is_some_and(|f| f.mode.is_some()) {
            return Err(ErrorCode::BadRequest);
        }
        if is_write_mode(mode) && !is_writable(&node) {
            return Err(ErrorCode::NoAccess);
        }
        if is_write_mode(mode)
            && let Node::Rule(name) = &node
        {
            let f = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            self.ensure_current_rule_fid(f, name)?;
        }
        if fid != Fid::ROOT {
            let f = self.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
            f.mode = Some(mode);
        }
        Ok(self.qid(&node))
    }

    fn read(&mut self, fid: Fid, offset: Offset, count: u32) -> Result<ActorOutcome, ErrorCode> {
        if fid != Fid::ROOT {
            let f = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
            if !matches!(f.mode, Some(OpenMode::Read | OpenMode::ReadWrite)) {
                return Err(ErrorCode::NoAccess);
            }
        }
        let node = self.node_of(fid)?;
        if let Some(stream) = self.stream_for(&node) {
            return Ok(ActorOutcome::ReadStream {
                stream,
                offset,
                count,
            });
        }
        Ok(ActorOutcome::Ready(slice(
            self.computed_bytes(&node)?,
            offset,
            count,
        )))
    }

    fn write(&mut self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let node = self.node_of(fid)?;
        let f = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        if !f.mode.is_some_and(is_write_mode) {
            return Err(ErrorCode::NoAccess);
        }
        if !matches!(node, Node::Send | Node::Rule(_)) {
            return Err(ErrorCode::Unsupported);
        }
        if let Node::Rule(name) = &node {
            self.ensure_current_rule_fid(f, name)?;
        }
        let start = usize::try_from(offset).map_err(|_| ErrorCode::BadRequest)?;
        let end = start.checked_add(data.len()).ok_or(ErrorCode::BadRequest)?;
        if end > MAX_DOC_BYTES {
            return Err(ErrorCode::BadRequest);
        }
        let f = self.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        if f.write_buf.len() < end {
            f.write_buf.resize(end, 0);
        }
        f.write_buf[start..end].copy_from_slice(data);
        f.wrote = true;
        Ok(data.len() as u32)
    }

    fn stat(&mut self, fid: Fid) -> Result<Stat, ErrorCode> {
        let node = self.node_of(fid)?;
        let length = match &node {
            Node::Port(_) | Node::Log => self
                .stream_for(&node)
                .ok_or(ErrorCode::NotFound)?
                .len_sync(),
            other => self.computed_bytes(other)?.len() as u64,
        };
        Ok(Stat {
            name: String::new(),
            qid: self.qid(&node),
            length,
            executable: false,
            writable: is_writable(&node),
        })
    }

    fn create(
        &mut self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if kind != FileKind::File || !valid_name(name) {
            return Err(ErrorCode::BadRequest);
        }
        if newfid == Fid::ROOT || self.fids.contains_key(&newfid) {
            return Err(ErrorCode::BadRequest);
        }
        if !matches!(self.node_of(fid)?, Node::RulesDir)
            || self.rules.contains_key(name)
            || self.pending_rules.contains(name)
        {
            return Err(ErrorCode::BadRequest);
        }
        self.pending_rules.insert(name.to_string());
        let node = Node::Rule(name.to_string());
        let qid = self.qid(&node);
        self.fids.insert(newfid, RouteFid::pending_rule(node));
        Ok(qid)
    }

    fn remove(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        let f = self.fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let node = f.node.clone();
        let Node::Rule(name) = node else {
            return Err(ErrorCode::Unsupported);
        };
        if f.pending_rule_create {
            if !self.pending_rules.remove(&name) {
                return Err(ErrorCode::NotFound);
            }
        } else {
            self.ensure_current_rule_fid(f, &name)?;
            self.rules.remove(&name).ok_or(ErrorCode::NotFound)?;
        }
        self.versions.bump(node_identity(&Node::RulesDir).1);
        self.fids.remove(&fid);
        Ok(())
    }

    fn clunk(&mut self, fid: Fid) -> Result<(), ErrorCode> {
        if fid == Fid::ROOT {
            return Ok(());
        }
        let f = self.fids.remove(&fid).ok_or(ErrorCode::NotFound)?;
        match f.node {
            Node::Send if f.wrote => self.route_message(&f.write_buf),
            Node::Rule(name) if f.wrote && f.pending_rule_create => {
                let spec: RuleSpec = match serde_json::from_slice(&f.write_buf) {
                    Ok(spec) => spec,
                    Err(_) => {
                        self.abandon_pending_rule(&name);
                        return Err(ErrorCode::BadRequest);
                    }
                };
                self.commit_pending_rule(name, spec)
            }
            Node::Rule(ref name) if f.wrote => {
                self.ensure_current_rule_fid(&f, name)?;
                let spec: RuleSpec =
                    serde_json::from_slice(&f.write_buf).map_err(|_| ErrorCode::BadRequest)?;
                self.install_rule(name.clone(), spec)
            }
            Node::Rule(name) if f.pending_rule_create => {
                self.abandon_pending_rule(&name);
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

fn is_writable(node: &Node) -> bool {
    matches!(node, Node::Send | Node::Rule(_))
}

fn is_write_mode(mode: OpenMode) -> bool {
    matches!(
        mode,
        OpenMode::Write | OpenMode::WriteExclusive | OpenMode::ReadWrite
    )
}

pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('/') && !name.contains('\n')
}

fn listing<'a>(names: impl Iterator<Item = &'a String>) -> Vec<u8> {
    names.cloned().collect::<Vec<_>>().join("\n").into_bytes()
}

pub(crate) fn rule_source(spec: &RuleSpec) -> Result<Vec<u8>, ErrorCode> {
    let mut bytes = serde_json::to_vec(spec).map_err(|_| ErrorCode::BadRequest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn routed_record_bytes(record: &RoutedRecordRef<'_>) -> Result<Vec<u8>, ErrorCode> {
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
