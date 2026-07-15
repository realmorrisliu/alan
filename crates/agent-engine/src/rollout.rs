//! AgentMachine persistence using JSONL format (similar to Codex rollout files)

use alan_agent_protocol::{
    CompactionAttemptSnapshot, CompactionReason, CompactionResult, CompactionTrigger,
    MemoryFlushAttemptSnapshot,
};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, warn};

const DURABLE_PAYLOAD_MAX_STRING_CHARS: usize = 512;
const DURABLE_PAYLOAD_MAX_ARRAY_ITEMS: usize = 32;
const DURABLE_PAYLOAD_MAX_OBJECT_FIELDS: usize = 64;
const DURABLE_PREVIEW_MAX_CHARS: usize = 160;

/// Types of items recorded in the rollout
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RolloutItem {
    AgentMachineMeta(AgentMachineMeta),
    Message(MessageRecord),
    TurnContext(TurnContextItem),
    CompactionAttempt(CompactionAttemptSnapshot),
    MemoryFlushAttempt(MemoryFlushAttemptSnapshot),
    Compacted(CompactedItem),
    ToolCall(ToolCallRecord),
    Effect(EffectRecord),
    Checkpoint(CheckpointRecord),
    Event(EventRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMachineMeta {
    /// Stable identity of this rollout evidence file.
    pub rollout_id: String,
    /// AgentFS path of the Agent Process that produced this rollout.
    pub process_path: String,
    pub started_at: String, // ISO 8601
    pub cwd: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
}

pub fn process_storage_key(process_path: &str) -> String {
    hex::encode(Sha256::digest(process_path.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRecord {
    pub role: String, // user, assistant, tool
    pub content: Option<String>,
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<crate::tape::Message>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContextItem {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    pub system_prompt: String,
    pub context_items: Vec<ContextItemRecord>,
    pub tools: Vec<String>,
    pub memory_enabled: bool,
    pub active_skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_context: Option<ReferenceContextSnapshotRecord>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceContextSnapshotRecord {
    pub revision: u64,
    pub changed: bool,
    pub reordered: bool,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItemRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactedItem {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<CompactionTrigger>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<CompactionReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<CompactionResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_context_revision: Option<u64>,
    pub timestamp: String,
}

impl CompactedItem {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            attempt_id: None,
            trigger: None,
            reason: None,
            focus: None,
            input_messages: None,
            output_messages: None,
            input_tokens: None,
            output_tokens: None,
            duration_ms: None,
            retry_count: None,
            result: None,
            reference_context_revision: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redaction: Option<ToolPayloadRedactionSummary>,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<alan_agent_protocol::ToolDecisionAudit>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ToolPayloadRedactionSummary {
    #[serde(default)]
    pub redacted_fields: usize,
    #[serde(default)]
    pub truncated_values: usize,
}

#[derive(Debug, Clone)]
pub struct DurableToolPayload {
    pub payload: Value,
    pub digest: String,
    pub preview: Option<String>,
    pub redaction: Option<ToolPayloadRedactionSummary>,
}

pub fn build_durable_tool_payload(payload: &Value) -> DurableToolPayload {
    let mut summary = ToolPayloadRedactionSummary::default();
    let redacted_payload = crate::evidence::redact_evidence_payload(payload);
    let projection_preview = crate::evidence::bounded_projection_preview(&redacted_payload);
    let mut durable_payload = sanitize_payload_for_rollout(&redacted_payload, &mut summary);
    if let (Some(preview), Some(object)) = (projection_preview, durable_payload.as_object_mut()) {
        object.insert("preview".to_string(), Value::String(preview.to_string()));
        if preview
            .chars()
            .nth(DURABLE_PAYLOAD_MAX_STRING_CHARS)
            .is_some()
        {
            summary.truncated_values = summary.truncated_values.saturating_sub(1);
        }
    }
    let digest = sha256_hex(&canonicalize_json(&durable_payload).to_string());
    let preview = payload_preview(&durable_payload);
    let redaction =
        (summary.redacted_fields > 0 || summary.truncated_values > 0).then_some(summary);

    DurableToolPayload {
        payload: durable_payload,
        digest,
        preview,
        redaction,
    }
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(entry) = map.get(key) {
                    sorted.insert(key.clone(), canonicalize_json(entry));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn normalize_sensitive_key(key: &str) -> String {
    key.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = normalize_sensitive_key(key);
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxyauthorization"
            | "cookie"
            | "setcookie"
            | "apikey"
            | "xapikey"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "bearertoken"
            | "clientsecret"
            | "password"
            | "passwd"
            | "passphrase"
            | "token"
            | "secret"
    ) || normalized.contains("apikey")
        || ["token", "secret", "password", "passwd", "passphrase"]
            .iter()
            .any(|suffix| normalized.ends_with(suffix))
}

fn truncate_string_for_rollout(text: &str, summary: &mut ToolPayloadRedactionSummary) -> String {
    let mut chars = text.chars();
    let preview: String = chars
        .by_ref()
        .take(DURABLE_PAYLOAD_MAX_STRING_CHARS)
        .collect();
    if chars.next().is_none() {
        return text.to_string();
    }

    summary.truncated_values += 1;
    format!("{preview}...[truncated]")
}

fn sanitize_payload_for_rollout(
    payload: &Value,
    summary: &mut ToolPayloadRedactionSummary,
) -> Value {
    match payload {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();

            let omitted = keys.len().saturating_sub(DURABLE_PAYLOAD_MAX_OBJECT_FIELDS);
            let mut sanitized = Map::new();
            for key in keys.into_iter().take(DURABLE_PAYLOAD_MAX_OBJECT_FIELDS) {
                let value = map.get(key).expect("key from map iteration must exist");
                if is_sensitive_key(key) {
                    summary.redacted_fields += 1;
                    sanitized.insert(
                        key.clone(),
                        Value::String("[REDACTED reason=secret_key]".to_string()),
                    );
                } else {
                    sanitized.insert(key.clone(), sanitize_payload_for_rollout(value, summary));
                }
            }

            if omitted > 0 {
                summary.truncated_values += omitted;
                sanitized.insert(
                    "_truncated".to_string(),
                    Value::String(format!("{omitted} additional field(s) omitted")),
                );
            }

            Value::Object(sanitized)
        }
        Value::Array(items) => {
            let omitted = items.len().saturating_sub(DURABLE_PAYLOAD_MAX_ARRAY_ITEMS);
            let mut sanitized: Vec<Value> = items
                .iter()
                .take(DURABLE_PAYLOAD_MAX_ARRAY_ITEMS)
                .map(|item| sanitize_payload_for_rollout(item, summary))
                .collect();

            if omitted > 0 {
                summary.truncated_values += omitted;
                sanitized.push(serde_json::json!({
                    "_truncated": format!("{omitted} additional item(s) omitted")
                }));
            }

            Value::Array(sanitized)
        }
        Value::String(text) => Value::String(truncate_string_for_rollout(text, summary)),
        _ => payload.clone(),
    }
}

fn payload_preview(value: &Value) -> Option<String> {
    let mut preview = match value {
        Value::Null => return None,
        Value::String(text) => text.trim().to_string(),
        Value::Object(map) => {
            if let Some(error) = map.get("error").and_then(Value::as_str) {
                format!("error: {}", error.trim())
            } else if let Some(status) = map.get("status").and_then(Value::as_str) {
                status.trim().to_string()
            } else {
                value.to_string()
            }
        }
        _ => value.to_string(),
    };

    if preview.is_empty() {
        return None;
    }

    if preview.chars().count() > DURABLE_PREVIEW_MAX_CHARS {
        preview = preview
            .chars()
            .take(DURABLE_PREVIEW_MAX_CHARS)
            .collect::<String>();
        preview.push_str("...");
    }

    Some(preview)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    Applied,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectRecord {
    pub effect_id: String,
    pub process_path: String,
    pub tool_call_id: String,
    pub idempotency_key: String,
    pub effect_type: String,
    pub request_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_payload: Option<serde_json::Value>,
    pub status: EffectStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub dedupe_hit: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointRecord {
    pub checkpoint_id: String,
    pub checkpoint_type: String,
    pub summary: String,
    pub choice: Option<String>, // approved, modified, rejected
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_root: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

fn checkpoint_record(
    checkpoint_id: &str,
    checkpoint_type: &str,
    summary: &str,
    choice: Option<&str>,
    knowledge_root: Option<&str>,
) -> CheckpointRecord {
    CheckpointRecord {
        checkpoint_id: checkpoint_id.to_string(),
        checkpoint_type: checkpoint_type.to_string(),
        summary: summary.to_string(),
        choice: choice.map(str::to_string),
        knowledge_root: knowledge_root.map(str::to_string),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

/// Commands for the background writer task
enum RolloutCmd {
    Record(Box<RolloutItem>),
    PersistBatch {
        items: Vec<RolloutItem>,
        ack: oneshot::Sender<Result<()>>,
    },
    Flush {
        ack: Option<oneshot::Sender<Result<()>>>,
    },
}

/// Persistent recorder for machine history
#[derive(Debug)]
pub struct RolloutRecorder {
    tx: mpsc::UnboundedSender<RolloutCmd>,
    rollout_id: String,
    rollout_path: PathBuf,
}

impl RolloutRecorder {
    fn message_record_from_tape_message(message: &crate::tape::Message) -> MessageRecord {
        let role = match message {
            crate::tape::Message::User { .. } => "user",
            crate::tape::Message::Assistant { .. } => "assistant",
            crate::tape::Message::Tool { .. } => "tool",
            crate::tape::Message::System { .. } => "system",
            crate::tape::Message::Context { .. } => "context",
        }
        .to_string();

        let content = match message {
            crate::tape::Message::Assistant { .. } => {
                let text = message.non_thinking_text_content();
                if text.is_empty() { None } else { Some(text) }
            }
            _ => {
                let text = message.text_content();
                if text.is_empty() { None } else { Some(text) }
            }
        };

        let tool_name = match message {
            crate::tape::Message::Tool { responses } => responses
                .first()
                .map(|response| response.id.trim().to_string())
                .filter(|id| !id.is_empty()),
            _ => None,
        };

        MessageRecord {
            role,
            content,
            tool_name,
            message: Some(message.clone()),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Create a new recorder for a machine under a specific rollouts directory.
    pub async fn new_in_dir(
        process_path: &str,
        model: &str,
        rollouts_dir: &std::path::Path,
    ) -> anyhow::Result<Self> {
        Self::new_in_dir_with_cwd_and_reasoning_effort(
            process_path,
            model,
            rollouts_dir,
            None,
            None,
        )
        .await
    }

    /// Create a new recorder under a specific rollouts directory and capture the Alan OS cwd in
    /// machine metadata.
    pub async fn new_in_dir_with_cwd(
        process_path: &str,
        model: &str,
        rollouts_dir: &std::path::Path,
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<Self> {
        Self::new_in_dir_with_cwd_and_reasoning_effort(process_path, model, rollouts_dir, cwd, None)
            .await
    }

    pub async fn new_in_dir_with_cwd_and_reasoning_effort(
        process_path: &str,
        model: &str,
        rollouts_dir: &std::path::Path,
        cwd: Option<&std::path::Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        let rollout_id = uuid::Uuid::new_v4().to_string();
        let rollout_path = Self::build_rollout_path_in_dir(&rollout_id, rollouts_dir).await?;
        Self::new_with_rollout_path(
            &rollout_id,
            process_path,
            model,
            rollout_path,
            cwd,
            reasoning_effort,
        )
        .await
    }

    async fn new_with_rollout_path(
        rollout_id: &str,
        process_path: &str,
        model: &str,
        rollout_path: PathBuf,
        cwd: Option<&std::path::Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        // Create the file
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&rollout_path)
            .await?;

        let (tx, mut rx) = mpsc::unbounded_channel::<RolloutCmd>();
        let _path = rollout_path.clone();

        // Spawn background writer task
        tokio::spawn(async move {
            let mut writer = BufWriter::new(file);

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    RolloutCmd::Record(item) => {
                        if let Err(e) = Self::write_item(&mut writer, &item).await {
                            error!(?e, "Failed to write rollout item");
                        }
                    }
                    RolloutCmd::PersistBatch { items, ack } => {
                        let persist_result =
                            Self::persist_items_and_flush(&mut writer, &items).await;
                        if let Err(err) = persist_result.as_ref() {
                            error!(?err, "Failed to persist rollout batch");
                        }
                        let _ = ack.send(persist_result);
                    }
                    RolloutCmd::Flush { ack } => {
                        let flush_result = Self::flush_writer(&mut writer).await;
                        if let Err(err) = flush_result.as_ref() {
                            error!(?err, "Failed to flush rollout file");
                        }
                        if let Some(ack) = ack {
                            let _ = ack.send(flush_result);
                        }
                    }
                }
            }

            // Final flush when channel closes
            if let Err(e) = Self::flush_writer(&mut writer).await {
                error!(?e, "Failed to flush rollout file on shutdown");
            }
        });

        let recorder = Self {
            tx,
            rollout_id: rollout_id.to_string(),
            rollout_path: rollout_path.clone(),
        };

        // Record machine metadata
        let meta = AgentMachineMeta {
            rollout_id: rollout_id.to_string(),
            process_path: process_path.to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            cwd: cwd
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string()),
            model: model.to_string(),
            reasoning_effort,
        };
        recorder.record_nowait(RolloutItem::AgentMachineMeta(meta))?;
        recorder.flush().await?;

        debug!(?rollout_path, "RolloutRecorder created");
        Ok(recorder)
    }

    /// Record an item
    pub fn record_nowait(&self, item: RolloutItem) -> Result<()> {
        if self.tx.send(RolloutCmd::Record(Box::new(item))).is_err() {
            warn!("Rollout channel closed, cannot record item");
            return Err(anyhow!("Rollout channel closed, cannot record item"));
        }
        Ok(())
    }

    /// Record an item (enqueue only, no flush wait).
    pub async fn record(&self, item: RolloutItem) -> Result<()> {
        self.record_nowait(item)
    }

    /// Persist a batch of items atomically with a single flush acknowledgement.
    pub async fn persist_batch(&self, items: Vec<RolloutItem>) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        if self
            .tx
            .send(RolloutCmd::PersistBatch { items, ack: ack_tx })
            .is_err()
        {
            warn!("Rollout channel closed, cannot persist batch");
            return Err(anyhow!("Rollout channel closed, cannot persist batch"));
        }
        ack_rx.await.map_err(|_| {
            warn!("Rollout writer dropped before batch persistence ack");
            anyhow!("Rollout writer dropped before batch persistence ack")
        })?
    }

    /// Enqueue a flush request without waiting for the writer to drain.
    pub fn flush_nowait(&self) -> Result<()> {
        if self.tx.send(RolloutCmd::Flush { ack: None }).is_err() {
            warn!("Rollout channel closed, cannot flush");
            return Err(anyhow!("Rollout channel closed, cannot flush"));
        }
        Ok(())
    }

    /// Record a message
    pub async fn record_message(
        &self,
        role: &str,
        content: Option<&str>,
        tool_name: Option<&str>,
    ) -> Result<()> {
        let item = RolloutItem::Message(MessageRecord {
            role: role.to_string(),
            content: content.map(|s| s.to_string()),
            tool_name: tool_name.map(|s| s.to_string()),
            message: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record(item).await?;
        // Ensure message records are persisted promptly so rollouts stay in sync
        // with the file-backed renderer during long-running turns.
        self.flush().await?;
        Ok(())
    }

    /// Record a message by enqueuing to the writer queue without spawning.
    pub fn record_message_nowait(
        &self,
        role: &str,
        content: Option<&str>,
        tool_name: Option<&str>,
    ) -> Result<()> {
        let item = RolloutItem::Message(MessageRecord {
            role: role.to_string(),
            content: content.map(|s| s.to_string()),
            tool_name: tool_name.map(|s| s.to_string()),
            message: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a rich tape message.
    pub async fn record_tape_message(&self, message: &crate::tape::Message) -> Result<()> {
        let item = RolloutItem::Message(Self::message_record_from_tape_message(message));
        self.record(item).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a rich tape message without waiting on IO completion.
    pub fn record_tape_message_nowait(&self, message: &crate::tape::Message) -> Result<()> {
        let item = RolloutItem::Message(Self::message_record_from_tape_message(message));
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a turn context snapshot
    #[allow(
        clippy::too_many_arguments,
        reason = "arguments map directly to the durable turn-context record fields"
    )]
    pub async fn record_turn_context(
        &self,
        model: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
        system_prompt: &str,
        context_items: Vec<ContextItemRecord>,
        tools: Vec<String>,
        memory_enabled: bool,
        active_skills: Vec<String>,
        reference_context: Option<ReferenceContextSnapshotRecord>,
    ) -> Result<()> {
        let item = RolloutItem::TurnContext(TurnContextItem {
            model: model.to_string(),
            reasoning_effort,
            system_prompt: system_prompt.to_string(),
            context_items,
            tools,
            memory_enabled,
            active_skills,
            reference_context,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record(item).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a turn context snapshot without waiting on IO completion.
    #[allow(
        clippy::too_many_arguments,
        reason = "arguments map directly to the durable turn-context record fields"
    )]
    pub fn record_turn_context_nowait(
        &self,
        model: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
        system_prompt: &str,
        context_items: Vec<ContextItemRecord>,
        tools: Vec<String>,
        memory_enabled: bool,
        active_skills: Vec<String>,
        reference_context: Option<ReferenceContextSnapshotRecord>,
    ) -> Result<()> {
        let item = RolloutItem::TurnContext(TurnContextItem {
            model: model.to_string(),
            reasoning_effort,
            system_prompt: system_prompt.to_string(),
            context_items,
            tools,
            memory_enabled,
            active_skills,
            reference_context,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a compaction summary
    pub async fn record_compacted(&self, message: &str) -> Result<()> {
        let item = RolloutItem::Compacted(CompactedItem::new(message));
        self.record(item).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a structured compaction attempt.
    pub async fn record_compaction_attempt(
        &self,
        attempt: CompactionAttemptSnapshot,
    ) -> Result<()> {
        self.record(RolloutItem::CompactionAttempt(attempt)).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a structured compaction attempt without waiting on IO completion.
    pub fn record_compaction_attempt_nowait(
        &self,
        attempt: CompactionAttemptSnapshot,
    ) -> Result<()> {
        self.record_nowait(RolloutItem::CompactionAttempt(attempt))?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a structured memory-flush attempt.
    pub async fn record_memory_flush_attempt(
        &self,
        attempt: MemoryFlushAttemptSnapshot,
    ) -> Result<()> {
        self.record(RolloutItem::MemoryFlushAttempt(attempt))
            .await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a structured memory-flush attempt without waiting on IO completion.
    pub fn record_memory_flush_attempt_nowait(
        &self,
        attempt: MemoryFlushAttemptSnapshot,
    ) -> Result<()> {
        self.record_nowait(RolloutItem::MemoryFlushAttempt(attempt))?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a compaction outcome with optional audit metadata.
    pub async fn record_compacted_item(&self, compacted: CompactedItem) -> Result<()> {
        self.record(RolloutItem::Compacted(compacted)).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a compaction summary without waiting on IO completion.
    pub fn record_compacted_nowait(&self, message: &str) -> Result<()> {
        self.record_compacted_item_nowait(CompactedItem::new(message))?;
        Ok(())
    }

    /// Record a compaction outcome without waiting on IO completion.
    pub fn record_compacted_item_nowait(&self, compacted: CompactedItem) -> Result<()> {
        self.record_nowait(RolloutItem::Compacted(compacted))?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a tool call
    pub async fn record_tool_call(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
    ) -> Result<()> {
        self.record_tool_call_with_audit(name, arguments, result, success, None)
            .await
    }

    /// Record a tool call with audit metadata.
    pub async fn record_tool_call_with_audit(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
        audit: Option<alan_agent_protocol::ToolDecisionAudit>,
    ) -> Result<()> {
        let durable_arguments = build_durable_tool_payload(&arguments);
        let durable_result = build_durable_tool_payload(&result);
        let item = RolloutItem::ToolCall(ToolCallRecord {
            name: name.to_string(),
            arguments: durable_arguments.payload,
            result: durable_result.payload,
            result_digest: Some(durable_result.digest),
            result_preview: durable_result.preview,
            redaction: durable_result.redaction,
            success,
            audit,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record(item).await?;
        self.flush().await?; // Important events are flushed immediately
        Ok(())
    }

    /// Record a tool call without waiting on IO completion.
    pub fn record_tool_call_nowait(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
    ) -> Result<()> {
        self.record_tool_call_nowait_with_audit(name, arguments, result, success, None)
    }

    /// Record a tool call with audit metadata without waiting on IO completion.
    pub fn record_tool_call_nowait_with_audit(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
        audit: Option<alan_agent_protocol::ToolDecisionAudit>,
    ) -> Result<()> {
        let durable_arguments = build_durable_tool_payload(&arguments);
        let durable_result = build_durable_tool_payload(&result);
        let item = RolloutItem::ToolCall(ToolCallRecord {
            name: name.to_string(),
            arguments: durable_arguments.payload,
            result: durable_result.payload,
            result_digest: Some(durable_result.digest),
            result_preview: durable_result.preview,
            redaction: durable_result.redaction,
            success,
            audit,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record an effect record.
    pub async fn record_effect(&self, effect: EffectRecord) -> Result<()> {
        self.record(RolloutItem::Effect(effect)).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record an effect record without waiting on IO completion.
    pub fn record_effect_nowait(&self, effect: EffectRecord) -> Result<()> {
        self.record_nowait(RolloutItem::Effect(effect))?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a checkpoint
    pub async fn record_checkpoint(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
    ) -> Result<()> {
        let item = RolloutItem::Checkpoint(checkpoint_record(
            checkpoint_id,
            checkpoint_type,
            summary,
            choice,
            None,
        ));
        self.record(item).await?;
        self.flush().await?; // Important events are flushed immediately
        Ok(())
    }

    /// Record a checkpoint with the content-addressed knowledge root it names.
    pub async fn record_checkpoint_with_knowledge_root(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
        knowledge_root: &str,
    ) -> Result<()> {
        let item = RolloutItem::Checkpoint(checkpoint_record(
            checkpoint_id,
            checkpoint_type,
            summary,
            choice,
            Some(knowledge_root),
        ));
        self.record(item).await?;
        self.flush().await?; // Important events are flushed immediately
        Ok(())
    }

    /// Record a checkpoint without waiting on IO completion.
    pub fn record_checkpoint_nowait(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
    ) -> Result<()> {
        let item = RolloutItem::Checkpoint(checkpoint_record(
            checkpoint_id,
            checkpoint_type,
            summary,
            choice,
            None,
        ));
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a checkpoint with a content-addressed knowledge root without
    /// waiting on IO completion.
    pub fn record_checkpoint_with_knowledge_root_nowait(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
        knowledge_root: &str,
    ) -> Result<()> {
        let item = RolloutItem::Checkpoint(checkpoint_record(
            checkpoint_id,
            checkpoint_type,
            summary,
            choice,
            Some(knowledge_root),
        ));
        self.record_nowait(item)?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Record a generic event
    pub async fn record_event(&self, event_type: &str, payload: serde_json::Value) -> Result<()> {
        self.record_event_item(EventRecord {
            event_type: event_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await
    }

    /// Record a generic event without waiting on IO completion.
    pub fn record_event_nowait(&self, event_type: &str, payload: serde_json::Value) -> Result<()> {
        self.record_event_item_nowait(EventRecord {
            event_type: event_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Record a generic event with a prebuilt item.
    pub async fn record_event_item(&self, event: EventRecord) -> Result<()> {
        self.record(RolloutItem::Event(event)).await?;
        self.flush().await?;
        Ok(())
    }

    /// Record a generic event item without waiting on IO completion.
    pub fn record_event_item_nowait(&self, event: EventRecord) -> Result<()> {
        self.record_nowait(RolloutItem::Event(event))?;
        self.flush_nowait()?;
        Ok(())
    }

    /// Flush pending writes to disk
    pub async fn flush(&self) -> Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        if self
            .tx
            .send(RolloutCmd::Flush { ack: Some(ack_tx) })
            .is_err()
        {
            warn!("Rollout channel closed, cannot flush");
            return Err(anyhow!("Rollout channel closed, cannot flush"));
        }
        ack_rx.await.map_err(|_| {
            warn!("Rollout writer dropped before flush ack");
            anyhow!("Rollout writer dropped before flush ack")
        })?
    }

    /// Load history from a rollout file
    pub async fn load_history(path: &PathBuf) -> anyhow::Result<Vec<RolloutItem>> {
        let content = fs::read(path).await?;
        let ends_with_record_delimiter = content.ends_with(b"\n");
        let mut items = Vec::new();
        let mut lines = content.split(|byte| *byte == b'\n').enumerate().peekable();

        while let Some((index, line_bytes)) = lines.next() {
            let is_unterminated_tail = lines.peek().is_none() && !ends_with_record_delimiter;
            let line = match std::str::from_utf8(line_bytes) {
                Ok(line) => line,
                Err(err) if is_unterminated_tail && err.error_len().is_none() => {
                    warn!(
                        path = %path.display(),
                        line = index + 1,
                        error = %err,
                        "Ignoring torn trailing rollout record with incomplete UTF-8"
                    );
                    break;
                }
                Err(err) => {
                    return Err(anyhow!(err)).with_context(|| {
                        format!(
                            "invalid UTF-8 in current rollout record at {}:{}",
                            path.display(),
                            index + 1
                        )
                    });
                }
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<RolloutItem>(line) {
                Ok(item) => items.push(item),
                Err(err) if is_unterminated_tail && err.is_eof() => {
                    warn!(
                        path = %path.display(),
                        line = index + 1,
                        error = %err,
                        "Ignoring torn trailing rollout record"
                    );
                    break;
                }
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "invalid current rollout record at {}:{}",
                            path.display(),
                            index + 1
                        )
                    });
                }
            }
        }

        Ok(items)
    }

    /// Get the path to the rollout file
    pub fn path(&self) -> &PathBuf {
        &self.rollout_path
    }

    /// Identity of the execution-evidence rollout written by this recorder.
    pub fn rollout_id(&self) -> &str {
        &self.rollout_id
    }

    async fn build_rollout_path_in_dir(
        rollout_id: &str,
        rollouts_dir: &std::path::Path,
    ) -> anyhow::Result<PathBuf> {
        fs::create_dir_all(rollouts_dir).await?;

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let filename = format!("rollout-{}-{}.jsonl", timestamp, rollout_id);
        Ok(rollouts_dir.join(filename))
    }

    /// Write a single item to the writer
    async fn write_item<W: AsyncWrite + Unpin>(
        writer: &mut W,
        item: &RolloutItem,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(item)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        Ok(())
    }

    async fn persist_items_and_flush<W: AsyncWrite + Unpin>(
        writer: &mut W,
        items: &[RolloutItem],
    ) -> anyhow::Result<()> {
        for item in items {
            Self::write_item(writer, item).await?;
        }
        Self::flush_writer(writer).await
    }

    async fn flush_writer<W: AsyncWrite + Unpin>(writer: &mut W) -> anyhow::Result<()> {
        writer.flush().await?;
        Ok(())
    }
}

impl Clone for RolloutRecorder {
    fn clone(&self) -> Self {
        // Create a new channel for the cloned recorder
        // This is a limitation - cloned recorders share the same file but have separate channels
        // In practice, only one recorder should be used per machine
        Self {
            tx: self.tx.clone(),
            rollout_id: self.rollout_id.clone(),
            rollout_path: self.rollout_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::path::Path;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tempfile::TempDir;
    use tokio::io::AsyncWrite;
    use tokio::time::{Duration, Instant, sleep};

    #[derive(Default)]
    struct FlushFailWriter {
        buffer: Vec<u8>,
    }

    impl AsyncWrite for FlushFailWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            self.buffer.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("synthetic flush failure")))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn test_rollout_recorder_creation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let recorder =
                RolloutRecorder::new_in_dir("/proc/123", "gemini-2.0-flash", temp_dir.path()).await;
            assert!(recorder.is_ok());

            let recorder = recorder.unwrap();
            let path = recorder.path();
            assert!(
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("rollout-")
            );
            assert!(path.to_string_lossy().ends_with(".jsonl"));
            let items = RolloutRecorder::load_history(path).await.unwrap();
            assert!(matches!(
                items.first(),
                Some(RolloutItem::AgentMachineMeta(meta)) if meta.process_path == "/proc/123"
            ));

            // Clean up - remove the created file
            let _ = fs::remove_file(path).await;
        });
    }

    #[test]
    fn test_rollout_recorder_creation_records_explicit_cwd() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let explicit_cwd = Path::new("/mnt/source/src");

            let recorder = RolloutRecorder::new_in_dir_with_cwd(
                "test-machine-cwd",
                "gemini-2.0-flash",
                temp_dir.path(),
                Some(explicit_cwd),
            )
            .await
            .unwrap();

            let items = RolloutRecorder::load_history(recorder.path())
                .await
                .unwrap();
            match &items[0] {
                RolloutItem::AgentMachineMeta(meta) => {
                    assert_eq!(meta.cwd, "/mnt/source/src");
                }
                _ => panic!("Expected AgentMachineMeta"),
            }

            let _ = fs::remove_file(recorder.path()).await;
        });
    }

    #[test]
    fn test_record_message_flushes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let recorder = RolloutRecorder::new_in_dir(
                "test-machine-flush",
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();
            recorder
                .record_message("user", Some("Hello"), None)
                .await
                .unwrap();

            let start = Instant::now();
            let mut found = false;
            while start.elapsed() < Duration::from_secs(1) {
                if let Ok(content) = fs::read_to_string(recorder.path()).await
                    && content.contains("\"type\":\"message\"")
                    && content.contains("Hello")
                {
                    found = true;
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }

            assert!(found, "Expected message to be flushed to rollout file");
        });
    }

    #[tokio::test]
    async fn test_record_tape_message_persists_rich_message() {
        let temp_dir = TempDir::new().unwrap();
        let recorder =
            RolloutRecorder::new_in_dir("test-rich-message", "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

        let message = crate::tape::Message::Assistant {
            parts: vec![
                crate::tape::ContentPart::thinking("internal reasoning"),
                crate::tape::ContentPart::text("final answer"),
            ],
            tool_requests: vec![],
        };
        recorder.record_tape_message(&message).await.unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let restored = items.into_iter().find_map(|item| match item {
            RolloutItem::Message(msg) => msg.message,
            _ => None,
        });

        let restored = restored.expect("expected rich message payload");
        assert_eq!(restored.non_thinking_text_content(), "final answer");
        assert_eq!(
            restored.thinking_content().as_deref(),
            Some("internal reasoning")
        );
    }

    #[tokio::test]
    async fn test_record_checkpoint_persists_knowledge_root() {
        let temp_dir = TempDir::new().unwrap();
        let recorder = RolloutRecorder::new_in_dir(
            "test-root-checkpoint",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();

        recorder
            .record_checkpoint_with_knowledge_root(
                "turn-1",
                "turn_completed",
                "turn completed",
                None,
                "sha256:abc123",
            )
            .await
            .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let checkpoint = items.into_iter().find_map(|item| match item {
            RolloutItem::Checkpoint(checkpoint) => Some(checkpoint),
            _ => None,
        });

        let checkpoint = checkpoint.expect("checkpoint should be persisted");
        assert_eq!(checkpoint.checkpoint_id, "turn-1");
        assert_eq!(checkpoint.knowledge_root.as_deref(), Some("sha256:abc123"));
    }

    #[tokio::test]
    async fn test_load_history() {
        let temp_dir = TempDir::new().unwrap();
        let recorder = RolloutRecorder::new_in_dir("/proc/7", "gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        recorder
            .record_tape_message(&crate::tape::Message::user("Hello"))
            .await
            .unwrap();
        recorder
            .record_tool_call(
                "test_tool",
                serde_json::json!({}),
                serde_json::json!({}),
                true,
            )
            .await
            .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        assert_eq!(items.len(), 3);

        // Verify first item is machine meta
        match &items[0] {
            RolloutItem::AgentMachineMeta(meta) => {
                assert_eq!(meta.process_path, "/proc/7");
                assert!(!meta.rollout_id.is_empty());
                assert_eq!(meta.model, "gemini-2.0-flash");
            }
            _ => panic!("Expected AgentMachineMeta"),
        }

        // Verify second item is message
        match &items[1] {
            RolloutItem::Message(msg) => {
                assert_eq!(msg.role, "user");
                assert_eq!(msg.content, Some("Hello".to_string()));
                assert!(msg.tool_name.is_none());
            }
            _ => panic!("Expected Message"),
        }

        // Verify third item is tool call
        match &items[2] {
            RolloutItem::ToolCall(tool) => {
                assert_eq!(tool.name, "test_tool");
                assert!(tool.success);
            }
            _ => panic!("Expected ToolCall"),
        }
    }

    #[tokio::test]
    async fn test_persist_batch_writes_compaction_attempt_and_summary_together() {
        let temp_dir = TempDir::new().unwrap();
        let recorder = RolloutRecorder::new_in_dir(
            "test-compaction-batch",
            "gemini-2.0-flash",
            temp_dir.path(),
        )
        .await
        .unwrap();
        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-123".to_string(),
            submission_id: Some("sub-456".to_string()),
            request: alan_agent_protocol::CompactionRequestMetadata {
                mode: alan_agent_protocol::CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve todos".to_string()),
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(12),
            output_messages: Some(4),
            input_prompt_tokens: Some(900),
            output_prompt_tokens: Some(300),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(3),
            reference_context_revision_after: Some(3),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let compacted = CompactedItem {
            message: "Summary after retry".to_string(),
            attempt_id: Some(attempt.attempt_id.clone()),
            trigger: Some(CompactionTrigger::Manual),
            reason: Some(CompactionReason::ExplicitRequest),
            focus: Some("preserve todos".to_string()),
            input_messages: Some(12),
            output_messages: Some(4),
            input_tokens: Some(900),
            output_tokens: Some(300),
            duration_ms: Some(42),
            retry_count: Some(1),
            result: Some(CompactionResult::Retry),
            reference_context_revision: Some(3),
            timestamp: "2026-01-29T14:31:01Z".to_string(),
        };

        recorder
            .persist_batch(vec![
                RolloutItem::CompactionAttempt(attempt.clone()),
                RolloutItem::Compacted(compacted.clone()),
            ])
            .await
            .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let persisted_attempt = items.iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(attempt) => Some(attempt),
            _ => None,
        });
        let persisted_compacted = items.iter().find_map(|item| match item {
            RolloutItem::Compacted(compacted) => Some(compacted),
            _ => None,
        });

        assert_eq!(persisted_attempt, Some(&attempt));
        assert_eq!(
            persisted_compacted.map(|item| item.attempt_id.as_deref()),
            Some(Some("attempt-123"))
        );
        assert_eq!(
            persisted_compacted.map(|item| item.message.as_str()),
            Some("Summary after retry")
        );
    }

    #[tokio::test]
    async fn test_persist_items_and_flush_propagates_flush_error() {
        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-flush-failure".to_string(),
            submission_id: None,
            request: alan_agent_protocol::CompactionRequestMetadata {
                mode: alan_agent_protocol::CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: None,
            },
            result: CompactionResult::Failure,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(4),
            output_messages: None,
            input_prompt_tokens: Some(256),
            output_prompt_tokens: None,
            retry_count: 0,
            tape_mutated: false,
            warning_message: None,
            error_message: Some("synthetic".to_string()),
            failure_streak: Some(1),
            reference_context_revision_before: Some(2),
            reference_context_revision_after: None,
            timestamp: "2026-03-18T00:00:00Z".to_string(),
        };
        let mut writer = FlushFailWriter::default();

        let err = RolloutRecorder::persist_items_and_flush(
            &mut writer,
            &[RolloutItem::CompactionAttempt(attempt)],
        )
        .await
        .expect_err("flush failure should be returned to the caller");

        assert!(err.to_string().contains("synthetic flush failure"));
        assert!(
            !writer.buffer.is_empty(),
            "writer should receive bytes before flush fails"
        );
    }

    #[tokio::test]
    async fn test_flush_writer_propagates_flush_error() {
        let mut writer = FlushFailWriter::default();

        let err = RolloutRecorder::flush_writer(&mut writer)
            .await
            .expect_err("flush failure should be returned to the caller");

        assert!(err.to_string().contains("synthetic flush failure"));
    }

    #[tokio::test]
    async fn test_load_history_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.jsonl");

        fs::write(&file_path, "").await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_load_history_with_empty_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("with_empty_lines.jsonl");

        let content = r#"
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}

{"type":"message","role":"assistant","content":"Hi!","tool_name":null,"timestamp":"2026-01-29T14:30:56Z"}
"#;

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_load_history_with_invalid_lines() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("with_invalid.jsonl");

        let content = r#"{"type":"message","role":"user","content":"Valid","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
this is not valid json
{"type":"message","role":"assistant","content":"Also valid","tool_name":null,"timestamp":"2026-01-29T14:30:56Z"}
"#;

        fs::write(&file_path, content).await.unwrap();

        let error = RolloutRecorder::load_history(&file_path).await.unwrap_err();
        assert!(error.to_string().contains("invalid current rollout record"));
    }

    #[tokio::test]
    async fn test_load_history_ignores_torn_trailing_json_record() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("with_torn_tail.jsonl");
        let content = concat!(
            "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
            "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
            "{\"type\":\"message\",\"role\":\"assistant\",\"content\":"
        );

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn test_load_history_ignores_torn_trailing_utf8_record() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("with_torn_utf8_tail.jsonl");
        let mut content = concat!(
            "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
            "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
            "{\"type\":\"message\",\"role\":\"assistant\",\"content\":\""
        )
        .as_bytes()
        .to_vec();
        content.extend_from_slice(&[0xe2, 0x82]);

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn test_load_history_rejects_non_torn_invalid_trailing_record() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("with_invalid_tail.jsonl");
        let content = concat!(
            "{\"type\":\"message\",\"role\":\"user\",\"content\":\"Valid\",",
            "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
            "not json"
        );

        fs::write(&file_path, content).await.unwrap();

        let error = RolloutRecorder::load_history(&file_path).await.unwrap_err();
        assert!(error.to_string().contains("invalid current rollout record"));
    }

    #[tokio::test]
    async fn test_load_history_accepts_valid_final_record_without_newline() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("valid_without_newline.jsonl");
        let content = concat!(
            "{\"type\":\"message\",\"role\":\"user\",\"content\":\"First\",",
            "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:55Z\"}\n",
            "{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"Second\",",
            "\"tool_name\":null,\"timestamp\":\"2026-01-29T14:30:56Z\"}"
        );

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 2);
    }

    #[tokio::test]
    async fn test_load_history_file_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.jsonl");

        let result = RolloutRecorder::load_history(&file_path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_rollout_item_serialization() {
        let meta = RolloutItem::AgentMachineMeta(AgentMachineMeta {
            rollout_id: "rollout-123".to_string(),
            process_path: "/proc/7".to_string(),
            started_at: "2026-01-29T14:30:52Z".to_string(),
            cwd: "/test".to_string(),
            model: "gemini-test".to_string(),
            reasoning_effort: None,
        });

        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("agent_machine_meta"));
        assert!(json.contains("rollout-123"));
        assert!(json.contains("/proc/7"));
        assert!(json.contains("gemini-test"));

        let deserialized: RolloutItem = serde_json::from_str(&json).unwrap();
        match deserialized {
            RolloutItem::AgentMachineMeta(m) => assert_eq!(m.rollout_id, "rollout-123"),
            _ => panic!("Expected AgentMachineMeta"),
        }
    }

    #[test]
    fn test_message_record_serialization() {
        let msg = MessageRecord {
            role: "user".to_string(),
            content: Some("Hello".to_string()),
            tool_name: None,
            message: None,
            timestamp: "2026-01-29T14:30:55Z".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello"));

        let deserialized: MessageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, "user");
        assert_eq!(deserialized.content, Some("Hello".to_string()));
        assert!(deserialized.message.is_none());
    }

    #[test]
    fn test_turn_context_item_serialization() {
        let ctx = TurnContextItem {
            model: "gemini-2.0-flash".to_string(),
            reasoning_effort: None,
            system_prompt: "System".to_string(),
            context_items: vec![ContextItemRecord {
                id: "onboarding".to_string(),
                kind: "static".to_string(),
                title: "Onboarding".to_string(),
                content: "Steps".to_string(),
                fingerprint: "abcd1234".to_string(),
            }],
            tools: vec!["web_search".to_string()],
            memory_enabled: true,
            active_skills: vec!["skill-1".to_string()],
            reference_context: Some(ReferenceContextSnapshotRecord {
                revision: 3,
                changed: true,
                reordered: false,
                added: 1,
                updated: 0,
                removed: 0,
            }),
            timestamp: "2026-01-29T14:30:56Z".to_string(),
        };

        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("gemini-2.0-flash"));
        assert!(json.contains("onboarding"));

        let deserialized: TurnContextItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, "gemini-2.0-flash");
        assert_eq!(deserialized.context_items[0].id, "onboarding");
        assert_eq!(
            deserialized.reference_context.as_ref().map(|r| r.revision),
            Some(3)
        );
    }

    #[tokio::test]
    async fn test_record_turn_context_persists_reasoning_effort() {
        let temp_dir = TempDir::new().unwrap();
        let recorder =
            RolloutRecorder::new_in_dir("machine-123", "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

        recorder
            .record_turn_context(
                "gemini-2.0-flash",
                Some(alan_agent_protocol::ReasoningEffort::High),
                "System",
                vec![],
                vec!["web_search".to_string()],
                true,
                vec![],
                None,
            )
            .await
            .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let persisted_effort = items.into_iter().find_map(|item| match item {
            RolloutItem::TurnContext(ctx) => ctx.reasoning_effort,
            _ => None,
        });
        assert_eq!(
            persisted_effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
    }

    #[test]
    fn test_turn_context_item_deserializes_without_reference_context_metadata() {
        let json = r#"{
            "model":"gemini-2.0-flash",
            "system_prompt":"System",
            "context_items":[],
            "tools":["web_search"],
            "memory_enabled":true,
            "active_skills":[],
            "timestamp":"2026-01-29T14:30:56Z"
        }"#;

        let deserialized: TurnContextItem = serde_json::from_str(json).unwrap();
        assert!(deserialized.reference_context.is_none());
    }

    #[test]
    fn test_compacted_item_serialization() {
        let item = CompactedItem {
            message: "Summary".to_string(),
            attempt_id: Some("attempt-123".to_string()),
            trigger: Some(CompactionTrigger::Manual),
            reason: Some(CompactionReason::ExplicitRequest),
            focus: Some("preserve todos".to_string()),
            input_messages: Some(24),
            output_messages: Some(8),
            input_tokens: Some(1200),
            output_tokens: Some(400),
            duration_ms: Some(35),
            retry_count: Some(1),
            result: Some(CompactionResult::Retry),
            reference_context_revision: Some(3),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };

        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("Summary"));
        assert!(json.contains("\"manual\""));
        assert!(json.contains("\"explicit_request\""));
        assert!(json.contains("\"preserve todos\""));

        let deserialized: CompactedItem = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.message, "Summary");
        assert_eq!(deserialized.attempt_id.as_deref(), Some("attempt-123"));
        assert_eq!(deserialized.trigger, Some(CompactionTrigger::Manual));
        assert_eq!(deserialized.reason, Some(CompactionReason::ExplicitRequest));
        assert_eq!(deserialized.focus.as_deref(), Some("preserve todos"));
        assert_eq!(deserialized.reference_context_revision, Some(3));
    }

    #[test]
    fn test_compaction_attempt_item_serialization() {
        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-123".to_string(),
            submission_id: Some("sub-456".to_string()),
            request: alan_agent_protocol::CompactionRequestMetadata {
                mode: alan_agent_protocol::CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve todos".to_string()),
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(12),
            output_messages: Some(4),
            input_prompt_tokens: Some(900),
            output_prompt_tokens: Some(300),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(3),
            reference_context_revision_after: Some(3),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };

        let json = serde_json::to_string(&RolloutItem::CompactionAttempt(attempt)).unwrap();
        assert!(json.contains("\"compaction_attempt\""));
        assert!(json.contains("\"attempt-123\""));
        assert!(json.contains("\"retry\""));

        let deserialized: RolloutItem = serde_json::from_str(&json).unwrap();
        match deserialized {
            RolloutItem::CompactionAttempt(snapshot) => {
                assert_eq!(snapshot.attempt_id, "attempt-123");
                assert_eq!(snapshot.submission_id.as_deref(), Some("sub-456"));
                assert_eq!(snapshot.result, CompactionResult::Retry);
                assert_eq!(snapshot.retry_count, 1);
            }
            other => panic!("expected compaction attempt item, got {other:?}"),
        }
    }

    #[test]
    fn test_tool_call_record_serialization() {
        let tool = ToolCallRecord {
            name: "web_search".to_string(),
            arguments: serde_json::json!({"query": "test"}),
            result: serde_json::json!({"found": 5}),
            result_digest: Some("digest-1".to_string()),
            result_preview: Some("found".to_string()),
            redaction: Some(ToolPayloadRedactionSummary {
                redacted_fields: 1,
                truncated_values: 0,
            }),
            success: true,
            audit: None,
            timestamp: "2026-01-29T14:31:02Z".to_string(),
        };

        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("web_search"));
        assert!(json.contains("true"));

        let deserialized: ToolCallRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "web_search");
        assert!(deserialized.success);
        assert_eq!(deserialized.result_digest.as_deref(), Some("digest-1"));
        assert_eq!(deserialized.result_preview.as_deref(), Some("found"));
        assert_eq!(
            deserialized.redaction,
            Some(ToolPayloadRedactionSummary {
                redacted_fields: 1,
                truncated_values: 0,
            })
        );
    }

    #[test]
    fn test_build_durable_tool_payload_redacts_sensitive_headers() {
        let durable = build_durable_tool_payload(&serde_json::json!({
            "github_token": "github-secret",
            "session_token": "machine-secret",
            "status": 200,
            "headers": {
                "set-cookie": "machine=secret",
                "authorization": "Bearer top-secret",
                "content-type": "application/json"
            }
        }));

        assert_eq!(
            durable.payload["github_token"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            durable.payload["session_token"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            durable.payload["headers"]["set-cookie"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            durable.payload["headers"]["authorization"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            durable.payload["headers"]["content-type"],
            serde_json::json!("application/json")
        );
        assert_eq!(
            durable.redaction,
            Some(ToolPayloadRedactionSummary {
                redacted_fields: 4,
                truncated_values: 0,
            })
        );
    }

    #[test]
    fn test_build_durable_tool_payload_redacts_secrets_embedded_in_strings() {
        let durable = build_durable_tool_payload(&serde_json::json!({
            "output": "api_key=embedded-secret\nAuthorization: Bearer embedded-token"
        }));

        let output = durable.payload["output"]
            .as_str()
            .expect("output should remain a string");
        assert!(!output.contains("embedded-secret"));
        assert!(!output.contains("embedded-token"));
        assert!(output.contains("[REDACTED reason=secret_key]"));
        assert!(!durable.digest.is_empty());
        assert!(
            durable
                .preview
                .as_deref()
                .is_none_or(|preview| !preview.contains("embedded-secret"))
        );
    }

    #[test]
    fn test_build_durable_tool_payload_truncates_large_strings() {
        let durable = build_durable_tool_payload(&serde_json::json!({
            "body": "x".repeat(2000)
        }));

        let body = durable.payload["body"]
            .as_str()
            .expect("body should stay a string");
        assert!(body.contains("...[truncated]"));
        assert_eq!(
            durable.redaction,
            Some(ToolPayloadRedactionSummary {
                redacted_fields: 0,
                truncated_values: 1,
            })
        );
    }

    #[test]
    fn test_build_durable_tool_payload_preserves_bounded_evidence_projection_preview() {
        let projection = crate::evidence::project_evidence_payload(
            &serde_json::json!({
                "output": "x".repeat(crate::evidence::MAX_INLINE_EVIDENCE_BYTES + 1)
            }),
            None,
            Vec::new(),
            Some("reference_unresolvable".to_string()),
        );
        let original_preview = projection["preview"].as_str().unwrap();
        assert!(original_preview.len() > DURABLE_PAYLOAD_MAX_STRING_CHARS);

        let durable = build_durable_tool_payload(&projection);

        assert_eq!(durable.payload["preview"], projection["preview"]);
        assert_eq!(
            durable.payload["truncation"]["preview_bytes"],
            serde_json::json!(original_preview.len())
        );
        assert!(durable.redaction.is_none());
    }

    #[test]
    fn test_effect_record_serialization() {
        let effect = EffectRecord {
            effect_id: "ef-1".to_string(),
            process_path: "/proc/7".to_string(),
            tool_call_id: "call-1".to_string(),
            idempotency_key: "idem-1".to_string(),
            effect_type: "file".to_string(),
            request_fingerprint: "fp-1".to_string(),
            result_digest: Some("digest-1".to_string()),
            result_payload: Some(serde_json::json!({"ok": true})),
            status: EffectStatus::Applied,
            applied_at: Some("2026-03-03T10:00:00Z".to_string()),
            reason: None,
            dedupe_hit: false,
            timestamp: "2026-03-03T10:00:01Z".to_string(),
        };

        let json = serde_json::to_string(&effect).unwrap();
        assert!(json.contains("ef-1"));
        assert!(json.contains("applied"));

        let deserialized: EffectRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.effect_id, "ef-1");
        assert_eq!(deserialized.status, EffectStatus::Applied);
    }

    #[tokio::test]
    async fn test_record_tool_call_redacts_sensitive_values_before_persisting() {
        let temp_dir = TempDir::new().unwrap();
        let recorder =
            RolloutRecorder::new_in_dir("test-tool-redaction", "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

        recorder
            .record_tool_call_with_audit(
                "web_fetch",
                serde_json::json!({
                    "headers": {
                        "authorization": "Bearer top-secret"
                    }
                }),
                serde_json::json!({
                    "headers": {
                        "set-cookie": "machine=super-secret"
                    }
                }),
                true,
                None,
            )
            .await
            .unwrap();

        let items = RolloutRecorder::load_history(recorder.path())
            .await
            .unwrap();
        let tool_call = items.into_iter().find_map(|item| match item {
            RolloutItem::ToolCall(record) => Some(record),
            _ => None,
        });

        let record = tool_call.expect("tool call should be persisted");
        assert_eq!(
            record.arguments["headers"]["authorization"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert_eq!(
            record.result["headers"]["set-cookie"],
            serde_json::json!("[REDACTED reason=secret_key]")
        );
        assert!(
            record.result_digest.is_some(),
            "durable tool call should persist a result digest"
        );
    }

    #[test]
    fn test_checkpoint_record_serialization() {
        let cp = CheckpointRecord {
            checkpoint_id: "cp-123".to_string(),
            checkpoint_type: "supplier_list".to_string(),
            summary: "Found 5 suppliers".to_string(),
            choice: Some("approved".to_string()),
            knowledge_root: Some("sha256:abc123".to_string()),
            timestamp: "2026-01-29T14:35:00Z".to_string(),
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(json.contains("cp-123"));
        assert!(json.contains("approved"));
        assert!(json.contains("sha256:abc123"));

        let deserialized: CheckpointRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.checkpoint_id, "cp-123");
        assert_eq!(deserialized.choice, Some("approved".to_string()));
        assert_eq!(
            deserialized.knowledge_root.as_deref(),
            Some("sha256:abc123")
        );
    }

    #[test]
    fn test_checkpoint_record_without_choice() {
        let cp = CheckpointRecord {
            checkpoint_id: "cp-456".to_string(),
            checkpoint_type: "requirements".to_string(),
            summary: "Requirements gathered".to_string(),
            choice: None,
            knowledge_root: None,
            timestamp: "2026-01-29T14:36:00Z".to_string(),
        };

        let json = serde_json::to_string(&cp).unwrap();
        assert!(json.contains("cp-456"));
        assert!(json.contains("null"));
        assert!(
            !json.contains("knowledge_root"),
            "missing root stays omitted for legacy-compatible checkpoints: {json}"
        );

        let deserialized: CheckpointRecord = serde_json::from_str(&json).unwrap();
        assert!(deserialized.choice.is_none());
        assert!(deserialized.knowledge_root.is_none());
    }

    #[test]
    fn test_event_record_serialization() {
        let event = EventRecord {
            event_type: "thinking".to_string(),
            payload: serde_json::json!({"message": "Analyzing..."}),
            timestamp: "2026-01-29T14:37:00Z".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("thinking"));
        assert!(json.contains("Analyzing..."));

        let deserialized: EventRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_type, "thinking");
    }

    #[test]
    fn test_rollout_recorder_clone() {
        // Just test that Clone is implemented correctly
        let _temp_dir = TempDir::new().unwrap();
        let _path = _temp_dir.path().join("test.jsonl");

        // Create a minimal recorder for testing clone
        // Note: We can't easily create a full recorder without async, but we can verify the types
        fn check_clone<T: Clone>(_: T) {}

        // Check that RolloutItem implements Clone
        let item = RolloutItem::Message(MessageRecord {
            role: "user".to_string(),
            content: Some("test".to_string()),
            tool_name: None,
            message: None,
            timestamp: "2026-01-29T14:30:55Z".to_string(),
        });
        check_clone(item);
    }

    #[tokio::test]
    async fn test_load_history_checkpoint_item() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("checkpoint.jsonl");

        let content = r#"{"type":"checkpoint","checkpoint_id":"cp-test","checkpoint_type":"supplier_list","summary":"Test summary","choice":"approved","timestamp":"2026-01-29T14:35:00Z"}"#;

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 1);

        match &items[0] {
            RolloutItem::Checkpoint(cp) => {
                assert_eq!(cp.checkpoint_id, "cp-test");
                assert_eq!(cp.checkpoint_type, "supplier_list");
                assert_eq!(cp.summary, "Test summary");
                assert_eq!(cp.choice, Some("approved".to_string()));
            }
            _ => panic!("Expected Checkpoint"),
        }
    }

    #[tokio::test]
    async fn test_load_history_event_item() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("event.jsonl");

        let content = r#"{"type":"event","event_type":"thinking","payload":{"message":"Thinking..."},"timestamp":"2026-01-29T14:37:00Z"}"#;

        fs::write(&file_path, content).await.unwrap();

        let items = RolloutRecorder::load_history(&file_path).await.unwrap();
        assert_eq!(items.len(), 1);

        match &items[0] {
            RolloutItem::Event(evt) => {
                assert_eq!(evt.event_type, "thinking");
            }
            _ => panic!("Expected Event"),
        }
    }
}
