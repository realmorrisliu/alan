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
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use alan_ap::{ErrorCode, Fid, FileKind, FileServer, Offset, OpenMode, Qid, Stat, Stream};
use alan_llm::{
    CompatibilityTier, GenerationRequest, LlmProvider, Message, MessageRole, ProviderCapabilities,
    ReasoningControls, ReasoningEffort, StreamChunk, TokenUsage, ToolCall, ToolCallDelta,
    ToolDefinition, factory::ProviderType,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, Notify};

/// Cap on a buffered request document, so a hostile writer cannot exhaust the
/// server before the commit-time validation runs.
const MAX_DOC_BYTES: usize = 1 << 20; // 1 MiB
const RETAIN_TERMINAL_GENERATIONS_PER_CONNECTION: usize = 16;

/// The neutral request document written to a Generation's `data` file.
///
/// The versioned DTO is owned by llmfs and mapped into `alan-llm` at the file
/// server boundary. The version discriminator is required and only version 1
/// is currently supported.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequestDocV1 {
    version: u16,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    messages: Vec<WireMessage>,
    #[serde(default)]
    tools: Vec<WireToolDefinition>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<i32>,
    #[serde(default)]
    reasoning: WireReasoningControls,
    #[serde(default)]
    extra_params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireMessage {
    role: WireMessageRole,
    content: String,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    thinking_signature: Option<String>,
    #[serde(default)]
    redacted_thinking: Option<Vec<String>>,
    #[serde(default)]
    tool_calls: Option<Vec<WireToolCall>>,
    #[serde(default)]
    tool_call_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum WireMessageRole {
    System,
    User,
    Assistant,
    Tool,
    Context,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireToolCall {
    #[serde(default)]
    id: Option<String>,
    name: String,
    arguments: serde_json::Value,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireReasoningControls {
    #[serde(default)]
    effort: Option<ReasoningEffort>,
}

#[derive(Serialize)]
struct WireStreamEventV1 {
    version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rejected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    aborted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    redacted_thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_response_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<WireTokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call: Option<WireToolCallDelta>,
}

#[derive(Serialize)]
struct WireTokenUsage {
    prompt_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cached_prompt_tokens: Option<i32>,
    completion_tokens: i32,
    total_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_tokens: Option<i32>,
}

#[derive(Serialize)]
struct WireToolCallDelta {
    index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments_delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

impl WireRequestDocV1 {
    fn into_generation_request(self) -> Result<GenerationRequest, ()> {
        if self.version != 1 || self.messages.is_empty() {
            return Err(());
        }
        Ok(GenerationRequest {
            system_prompt: self.system,
            messages: self.messages.into_iter().map(Into::into).collect(),
            tools: self.tools.into_iter().map(Into::into).collect(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            reasoning: ReasoningControls {
                effort: self.reasoning.effort,
            },
            extra_params: self.extra_params.into_iter().collect(),
        })
    }
}

impl From<WireMessage> for Message {
    fn from(value: WireMessage) -> Self {
        Self {
            role: value.role.into(),
            content: value.content,
            thinking: value.thinking,
            thinking_signature: value.thinking_signature,
            redacted_thinking: value.redacted_thinking,
            tool_calls: value
                .tool_calls
                .map(|tool_calls| tool_calls.into_iter().map(Into::into).collect()),
            tool_call_id: value.tool_call_id,
        }
    }
}

impl From<WireMessageRole> for MessageRole {
    fn from(value: WireMessageRole) -> Self {
        match value {
            WireMessageRole::System => MessageRole::System,
            WireMessageRole::User => MessageRole::User,
            WireMessageRole::Assistant => MessageRole::Assistant,
            WireMessageRole::Tool => MessageRole::Tool,
            WireMessageRole::Context => MessageRole::Context,
        }
    }
}

impl From<WireToolDefinition> for ToolDefinition {
    fn from(value: WireToolDefinition) -> Self {
        Self {
            name: value.name,
            description: value.description,
            parameters: value.parameters,
        }
    }
}

impl From<WireToolCall> for ToolCall {
    fn from(value: WireToolCall) -> Self {
        Self {
            id: value.id,
            name: value.name,
            arguments: value.arguments,
        }
    }
}

impl From<TokenUsage> for WireTokenUsage {
    fn from(value: TokenUsage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            cached_prompt_tokens: value.cached_prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
            reasoning_tokens: value.reasoning_tokens,
        }
    }
}

impl From<&ToolCallDelta> for WireToolCallDelta {
    fn from(value: &ToolCallDelta) -> Self {
        Self {
            index: value.index,
            id: value.id.clone(),
            name: value.name.clone(),
            arguments_delta: value.arguments_delta.clone(),
            arguments: value.arguments.clone(),
        }
    }
}

impl WireStreamEventV1 {
    fn empty() -> Self {
        Self {
            version: 1,
            done: None,
            error: None,
            rejected: None,
            aborted: None,
            text: None,
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            sequence_number: None,
            usage: None,
            tool_call: None,
        }
    }

    fn done() -> Self {
        Self {
            done: Some(true),
            ..Self::empty()
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            error: Some(message.into()),
            ..Self::empty()
        }
    }

    fn rejected() -> Self {
        Self {
            rejected: Some(true),
            ..Self::empty()
        }
    }

    fn aborted() -> Self {
        Self {
            aborted: Some(true),
            ..Self::empty()
        }
    }

    fn has_payload(&self) -> bool {
        self.done.is_some()
            || self.error.is_some()
            || self.rejected.is_some()
            || self.aborted.is_some()
            || self.text.is_some()
            || self.thinking.is_some()
            || self.thinking_signature.is_some()
            || self.redacted_thinking.is_some()
            || self.finish_reason.is_some()
            || self.provider_response_id.is_some()
            || self.provider_response_status.is_some()
            || self.sequence_number.is_some()
            || self.usage.is_some()
            || self.tool_call.is_some()
    }
}

fn event_line(event: WireStreamEventV1) -> Vec<u8> {
    let mut line = serde_json::to_string(&event)
        .expect("serialize llmfs stream event")
        .into_bytes();
    line.push(b'\n');
    line
}

/// A callable connection: a provider behind an async lock so a Generation can
/// hold it across `generate_stream`.
struct Connection {
    provider: AsyncMutex<Box<dyn LlmProvider>>,
    provider_name: String,
    model: Option<String>,
    credential_ref: Option<String>,
    capabilities: ProviderCapabilities,
    limits: ConnectionLimits,
    generation_starts: AtomicU64,
    total_tokens: AtomicU64,
    total_cost_microusd: AtomicU64,
    meter_version: AtomicU32,
}

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

impl Connection {
    fn try_reserve_generation(&self) -> Result<(), ErrorCode> {
        loop {
            let current = self.generation_starts.load(Ordering::Relaxed);
            if self
                .limits
                .max_generations
                .is_some_and(|max| current >= max)
            {
                return Err(ErrorCode::NoAccess);
            }
            if self
                .generation_starts
                .compare_exchange(current, current + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                self.meter_version.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        }
    }

    fn record_token_delta(&self, delta: u64) {
        if delta == 0 {
            return;
        }
        self.total_tokens.fetch_add(delta, Ordering::Relaxed);
        self.meter_version.fetch_add(1, Ordering::Relaxed);
    }

    fn meter_version(&self) -> u32 {
        self.meter_version.load(Ordering::Relaxed)
    }

    fn meter_doc(&self, connection: &str) -> String {
        render_json_doc(serde_json::json!({
            "version": 1,
            "connection": connection,
            "limits": {
                "max_generations": self.limits.max_generations,
            },
            "meter": {
                "generation_starts": self.generation_starts.load(Ordering::Relaxed),
                "total_tokens": self.total_tokens.load(Ordering::Relaxed),
                "total_cost_microusd": self.total_cost_microusd.load(Ordering::Relaxed),
                "currency": "USD",
            },
        }))
    }
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
    sequence: u64,
    events: Stream,
    status: StdMutex<GenStatus>,
    token_usage: StdMutex<Option<TokenUsage>>,
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
    fn token_usage(&self) -> Option<TokenUsage> {
        *self.token_usage.lock().unwrap()
    }
    fn connection_name(&self) -> String {
        self.connection_name.clone()
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn record_usage(&self, usage: TokenUsage) {
        let previous_total = {
            let mut token_usage = self.token_usage.lock().unwrap();
            let previous_total = token_usage
                .map(|usage| usage.total_tokens.max(0) as u64)
                .unwrap_or(0);
            *token_usage = Some(usage);
            previous_total
        };
        let next_total = usage.total_tokens.max(0) as u64;
        self.connection
            .record_token_delta(next_total.saturating_sub(previous_total));
        self.version.fetch_add(1, Ordering::Relaxed);
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
                .map(|g| g.version.load(Ordering::Relaxed))
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
                if generation.status().is_terminal() {
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
            Arc::new(Connection {
                provider: AsyncMutex::new(provider),
                provider_name,
                model,
                credential_ref,
                capabilities,
                limits,
                generation_starts: AtomicU64::new(0),
                total_tokens: AtomicU64::new(0),
                total_cost_microusd: AtomicU64::new(0),
                meter_version: AtomicU32::new(0),
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
            .get(&(self.view_id, fid))
            .map(|f| f.node.clone())
            .ok_or(ErrorCode::NotFound)
    }

    fn child(&self, node: &Node, name: &str) -> Result<Node, ErrorCode> {
        let state = self.state.lock().unwrap();
        match node {
            Node::Root if name == "connections" => Ok(Node::ConnectionsDir),
            Node::Root if name == "providers" => Ok(Node::ProvidersDir),
            Node::ProvidersDir if provider_type_for_name(name).is_some() => {
                Ok(Node::Provider(name.to_string()))
            }
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
                Arc::new(Generation {
                    connection,
                    connection_name: conn.clone(),
                    sequence,
                    events: Stream::new(),
                    status: StdMutex::new(GenStatus::Open),
                    token_usage: StdMutex::new(None),
                    version: AtomicU32::new(0),
                    abort: Arc::new(Notify::new()),
                    finalize: AsyncMutex::new(()),
                }),
            );
            if let Some(f) = state.fids.get_mut(&fid_key) {
                f.clone_gen = Some(id);
            }
            reap_terminal_generations(&mut state, conn);
        }

        let opened_events = if let Node::GenEvents(id) = &node {
            Some(
                state
                    .gens
                    .get(id)
                    .ok_or(ErrorCode::NotFound)?
                    .events
                    .clone(),
            )
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
                            Some(g) => Len::Events(g.events.clone()),
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

        // Parse the request first (pure): an empty or invalid document is malformed.
        let doc: Result<WireRequestDocV1, ()> = if buf.is_empty() {
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
                    generation
                        .events
                        .append(&event_line(WireStreamEventV1::rejected()))
                        .await;
                }
                return Err(ErrorCode::BadRequest);
            }
        };

        let request = match doc.into_generation_request() {
            Ok(request) => request,
            Err(()) => {
                let _guard = generation.finalize.lock().await;
                if generation.claim(GenStatus::Rejected) {
                    generation
                        .events
                        .append(&event_line(WireStreamEventV1::rejected()))
                        .await;
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
                            if let Some(usage) = chunk.usage {
                                drain_gen.record_usage(usage);
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
                                    events
                                        .append(&event_line(WireStreamEventV1::error(reason)))
                                        .await;
                                    drain_gen.advance(GenStatus::Error);
                                } else {
                                    events.append(&event_line(WireStreamEventV1::done())).await;
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
                                events
                                    .append(&event_line(WireStreamEventV1::error("stream closed")))
                                    .await;
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
            let event = match tag {
                "rejected" => WireStreamEventV1::rejected(),
                "error" => WireStreamEventV1::error("error"),
                _ => WireStreamEventV1::error(tag),
            };
            generation.events.append(&event_line(event)).await;
        }
    }

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
            generation.connection_name() == connection && generation.status().is_terminal()
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

async fn abort_generation(generation: &Generation) -> Result<(), ErrorCode> {
    // Finalize under the per-Generation lock so this abort and the drain task
    // cannot interleave: once aborted, no further chunk or `done` record is
    // written. Aborting a terminal Generation is refused (settled status).
    {
        let _guard = generation.finalize.lock().await;
        if generation.status().is_terminal() {
            return Err(ErrorCode::BadRequest);
        }
        generation
            .events
            .append(&event_line(WireStreamEventV1::aborted()))
            .await;
        generation.advance(GenStatus::Aborted);
    }
    // Wake a running drain task (or a pending provider startup) so it stops
    // promptly. `notify_one` stores a permit if no waiter is parked yet, so an
    // abort that arrives before the drain reaches `notified()` is not lost.
    generation.abort.notify_one();
    Ok(())
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

fn known_provider_names() -> Vec<String> {
    let mut names = vec![
        "anthropic_messages".to_string(),
        "chatgpt".to_string(),
        "google_gemini_generate_content".to_string(),
        "openai_chat_completions".to_string(),
        "openai_chat_completions_compatible".to_string(),
        "openai_responses".to_string(),
        "openrouter".to_string(),
    ];
    names.sort();
    names
}

fn provider_type_for_name(name: &str) -> Option<ProviderType> {
    match name {
        "google_gemini_generate_content" => Some(ProviderType::GoogleGeminiGenerateContent),
        "chatgpt" => Some(ProviderType::ChatgptResponses),
        // Test providers report `mock`, but the engine-backed smoke path uses
        // them as Responses-compatible connections.
        "mock" => Some(ProviderType::OpenAiResponses),
        "openai_responses" => Some(ProviderType::OpenAiResponses),
        "openai_chat_completions" => Some(ProviderType::OpenAiChatCompletions),
        "openai_chat_completions_compatible" => Some(ProviderType::OpenAiChatCompletionsCompatible),
        "openrouter" => Some(ProviderType::OpenRouter),
        "anthropic_messages" => Some(ProviderType::AnthropicMessages),
        _ => None,
    }
}

fn provider_capabilities_for_name(name: &str) -> ProviderCapabilities {
    provider_type_for_name(name)
        .map(ProviderType::capabilities)
        .unwrap_or_else(unknown_provider_capabilities)
}

fn unknown_provider_capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        supports_streaming_text: true,
        supports_streaming_tool_calls: false,
        supports_provider_response_id: false,
        supports_provider_response_status: false,
        supports_reasoning_text: false,
        supports_reasoning_signature: false,
        supports_reasoning_effort_control: false,
        supports_redacted_thinking: false,
        supports_multimodal_input: false,
        supports_document_input: false,
        supports_cached_token_usage: false,
        supports_server_managed_continuation: false,
        supports_background_execution: false,
        supports_retrieve_cancel: false,
        supports_provider_compaction: false,
        instruction_role: alan_llm::InstructionRole::System,
        compatibility_tier: CompatibilityTier::TierCBestEffortCompatible,
    }
}

fn provider_capabilities_doc(provider: &str, capabilities: ProviderCapabilities) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "capabilities": capabilities,
    }))
}

fn connection_capabilities_doc(
    connection: &str,
    provider: &str,
    capabilities: ProviderCapabilities,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        "capabilities": capabilities,
    }))
}

fn connection_profile_doc(
    connection: &str,
    provider: &str,
    model: Option<&str>,
    credential_ref: Option<&str>,
) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "connection": connection,
        "provider": provider,
        "model": model,
        "credential_ref": credential_ref,
    }))
}

fn generation_status_doc(id: &str, generation: &Generation) -> String {
    let status = generation.status();
    let usage = generation.token_usage();
    let tokens = usage
        .map(|usage| {
            serde_json::json!({
                "available": true,
                "prompt_tokens": usage.prompt_tokens,
                "cached_prompt_tokens": usage.cached_prompt_tokens,
                "completion_tokens": usage.completion_tokens,
                "total_tokens": usage.total_tokens,
                "reasoning_tokens": usage.reasoning_tokens,
            })
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "available": false,
                "prompt_tokens": 0,
                "cached_prompt_tokens": null,
                "completion_tokens": 0,
                "total_tokens": 0,
                "reasoning_tokens": null,
            })
        });
    render_json_doc(serde_json::json!({
        "version": 1,
        "generation": id,
        "connection": generation.connection_name(),
        "status": status.as_str(),
        "progress": {
            "phase": status.as_str(),
            "terminal": status.is_terminal(),
        },
        "tokens": tokens,
        "cost": {
            "currency": "USD",
            "amount_microusd": 0,
            "metered": false,
        },
    }))
}

fn provider_models_doc(provider: &str) -> String {
    let catalog = provider_model_catalog(provider);
    let models = catalog
        .map(|catalog| {
            catalog
                .models
                .iter()
                .map(|model| {
                    serde_json::json!({
                        "slug": model.slug,
                        "family": model.family,
                        "context_window_tokens": model.context_window_tokens,
                        "supports_reasoning": model.supports_reasoning,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "source": catalog.map(|catalog| catalog.source).unwrap_or("unknown"),
        "default_model": catalog.map(|catalog| catalog.default_model),
        "models": models,
    }))
}

fn provider_status_doc(provider: &str) -> String {
    render_json_doc(serde_json::json!({
        "version": 1,
        "provider": provider,
        "status": "available",
        "callable": false,
        "has_model_catalog": provider_model_catalog(provider).is_some(),
    }))
}

#[derive(Clone, Copy)]
struct ProviderModel {
    slug: &'static str,
    family: &'static str,
    context_window_tokens: u32,
    supports_reasoning: bool,
}

#[derive(Clone, Copy)]
struct ProviderModelCatalog {
    default_model: &'static str,
    source: &'static str,
    models: &'static [ProviderModel],
}

const OPENAI_GPT_MODELS: &[ProviderModel] = &[
    ProviderModel {
        slug: "gpt-5.4",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.4-pro",
        family: "gpt-5",
        context_window_tokens: 1_050_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.2",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.2-pro",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5.1",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5-mini",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-5-nano",
        family: "gpt-5",
        context_window_tokens: 400_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-oss-120b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "gpt-oss-20b",
        family: "gpt-oss",
        context_window_tokens: 131_072,
        supports_reasoning: true,
    },
];

const OPENAI_COMPATIBLE_MODELS: &[ProviderModel] = &[
    ProviderModel {
        slug: "qwen3.5-plus",
        family: "qwen3.5",
        context_window_tokens: 1_000_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "minimax-m2.5",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "minimax-m2.5-highspeed",
        family: "minimax-m2.5",
        context_window_tokens: 204_800,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "glm-5",
        family: "glm-5",
        context_window_tokens: 200_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "kimi-k2.5",
        family: "kimi-k2.5",
        context_window_tokens: 250_000,
        supports_reasoning: true,
    },
    ProviderModel {
        slug: "deepseek-chat",
        family: "deepseek-v3",
        context_window_tokens: 128_000,
        supports_reasoning: false,
    },
    ProviderModel {
        slug: "deepseek-reasoner",
        family: "deepseek-r1",
        context_window_tokens: 128_000,
        supports_reasoning: true,
    },
];

const CHATGPT_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "gpt-5.3-codex",
    family: "gpt-5-codex",
    context_window_tokens: 400_000,
    supports_reasoning: true,
}];

const GEMINI_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "gemini-2.0-flash",
    family: "gemini-2.0",
    context_window_tokens: 1_048_576,
    supports_reasoning: false,
}];

const ANTHROPIC_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "claude-3-5-sonnet-latest",
    family: "claude-3.5-sonnet",
    context_window_tokens: 200_000,
    supports_reasoning: false,
}];

const OPENROUTER_MODELS: &[ProviderModel] = &[ProviderModel {
    slug: "moonshotai/kimi-k2.6",
    family: "kimi-k2.6",
    context_window_tokens: 256_000,
    supports_reasoning: true,
}];

fn provider_model_catalog(provider: &str) -> Option<ProviderModelCatalog> {
    match provider {
        "openai_responses" => Some(ProviderModelCatalog {
            default_model: "gpt-5.4",
            source: "bundled-openai-responses",
            models: OPENAI_GPT_MODELS,
        }),
        "openai_chat_completions" => Some(ProviderModelCatalog {
            default_model: "gpt-5.4",
            source: "bundled-openai-chat-completions",
            models: OPENAI_GPT_MODELS,
        }),
        "openai_chat_completions_compatible" => Some(ProviderModelCatalog {
            default_model: "qwen3.5-plus",
            source: "bundled-openai-compatible",
            models: OPENAI_COMPATIBLE_MODELS,
        }),
        "chatgpt" => Some(ProviderModelCatalog {
            default_model: "gpt-5.3-codex",
            source: "bundled-chatgpt",
            models: CHATGPT_MODELS,
        }),
        "google_gemini_generate_content" => Some(ProviderModelCatalog {
            default_model: "gemini-2.0-flash",
            source: "bundled-gemini",
            models: GEMINI_MODELS,
        }),
        "anthropic_messages" => Some(ProviderModelCatalog {
            default_model: "claude-3-5-sonnet-latest",
            source: "bundled-anthropic",
            models: ANTHROPIC_MODELS,
        }),
        "openrouter" => Some(ProviderModelCatalog {
            default_model: "moonshotai/kimi-k2.6",
            source: "bundled-openrouter",
            models: OPENROUTER_MODELS,
        }),
        _ => None,
    }
}

fn render_json_doc(value: serde_json::Value) -> String {
    let mut rendered = serde_json::to_string(&value).expect("serialize llmfs introspection doc");
    rendered.push('\n');
    rendered
}

/// Build one `events` record from the meaningful fields of a stream chunk, so a
/// non-text chunk (thinking, usage, finish metadata, tool-call delta) is not
/// dropped. Returns `None` for a chunk with nothing to record.
fn chunk_record(chunk: &StreamChunk) -> Option<String> {
    let event = WireStreamEventV1 {
        done: None,
        error: None,
        rejected: None,
        aborted: None,
        text: chunk.text.clone(),
        thinking: chunk.thinking.clone(),
        thinking_signature: chunk.thinking_signature.clone(),
        redacted_thinking: chunk.redacted_thinking.clone(),
        finish_reason: chunk.finish_reason.clone(),
        provider_response_id: chunk.provider_response_id.clone(),
        provider_response_status: chunk.provider_response_status.clone(),
        sequence_number: chunk.sequence_number,
        usage: chunk.usage.map(Into::into),
        tool_call: chunk.tool_call_delta.as_ref().map(Into::into),
        ..WireStreamEventV1::empty()
    };
    if !event.has_payload() {
        None
    } else {
        Some(serde_json::to_string(&event).expect("serialize llmfs stream event"))
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
