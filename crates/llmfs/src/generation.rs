//! Callable Connection state and Generation execution lifecycle.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use alan_ap::{ErrorCode, Stream};
use alan_llm::{LlmProvider, ProviderCapabilities, StreamChunk, TokenUsage, ToolCallDelta};
use serde::Serialize;
use tokio::sync::{Mutex as AsyncMutex, Notify};

use super::request_wire::WireRequestDocV2;
use super::{ConnectionLimits, render_json_doc};

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
pub(super) struct Connection {
    provider: AsyncMutex<Box<dyn LlmProvider>>,
    pub(super) provider_name: String,
    pub(super) model: Option<String>,
    pub(super) credential_ref: Option<String>,
    pub(super) capabilities: ProviderCapabilities,
    limits: ConnectionLimits,
    generation_starts: AtomicU64,
    total_tokens: AtomicU64,
    total_cost_microusd: AtomicU64,
    meter_version: AtomicU32,
}

impl Connection {
    pub(super) fn new(
        provider_name: String,
        model: Option<String>,
        credential_ref: Option<String>,
        capabilities: ProviderCapabilities,
        limits: ConnectionLimits,
        provider: Box<dyn LlmProvider>,
    ) -> Self {
        Self {
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
        }
    }

    pub(super) fn try_reserve_generation(&self) -> Result<(), ErrorCode> {
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

    pub(super) fn meter_version(&self) -> u32 {
        self.meter_version.load(Ordering::Relaxed)
    }

    pub(super) fn meter_doc(&self, connection: &str) -> String {
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
    pub(super) fn is_terminal(self) -> bool {
        matches!(
            self,
            GenStatus::Done | GenStatus::Error | GenStatus::Rejected | GenStatus::Aborted
        )
    }
}

/// One Generation's projected surfaces and lifecycle.
pub(super) struct Generation {
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
    pub(super) fn new(connection: Arc<Connection>, connection_name: String, sequence: u64) -> Self {
        Self {
            connection,
            connection_name,
            sequence,
            events: Stream::new(),
            status: StdMutex::new(GenStatus::Open),
            token_usage: StdMutex::new(None),
            version: AtomicU32::new(0),
            abort: Arc::new(Notify::new()),
            finalize: AsyncMutex::new(()),
        }
    }

    fn status(&self) -> GenStatus {
        *self.status.lock().unwrap()
    }

    pub(super) fn is_terminal(&self) -> bool {
        self.status().is_terminal()
    }
    fn token_usage(&self) -> Option<TokenUsage> {
        *self.token_usage.lock().unwrap()
    }
    pub(super) fn connection_name(&self) -> String {
        self.connection_name.clone()
    }
    pub(super) fn sequence(&self) -> u64 {
        self.sequence
    }
    pub(super) fn version(&self) -> u32 {
        self.version.load(Ordering::Relaxed)
    }

    pub(super) fn events(&self) -> Stream {
        self.events.clone()
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

pub(super) async fn commit_request(
    buf: Vec<u8>,
    generation: Arc<Generation>,
) -> Result<(), ErrorCode> {
    // Parse the request first (pure): an empty or invalid document is malformed.
    let doc: Result<WireRequestDocV2, ()> = if buf.is_empty() {
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

    let request = match doc.into_generation_request(generation.connection.capabilities) {
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
            fail_generation(&generation, GenStatus::Error, "error").await;
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

async fn fail_generation(generation: &Generation, status: GenStatus, tag: &str) {
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

pub(super) async fn abort_generation(generation: &Generation) -> Result<(), ErrorCode> {
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

pub(super) fn generation_status_doc(id: &str, generation: &Generation) -> String {
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
