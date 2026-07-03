//! Session state management.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::error;

use alan_agent_protocol::{
    CompactionAttemptSnapshot, CompactionMode, CompactionReason, CompactionRequestMetadata,
    CompactionResult, CompactionTrigger, MemoryFlushAttemptSnapshot,
};

use crate::approval::{
    RUNTIME_CONFIRMATION_CONTROL_SOURCE, RUNTIME_CONFIRMATION_CONTROL_VERSION,
    is_runtime_confirmation_checkpoint_type, runtime_confirmation_checkpoint_prefix,
    runtime_confirmation_control_kind,
};
use crate::rollout::{
    CompactedItem, ContextItemRecord, EffectRecord, EventRecord, ReferenceContextSnapshotRecord,
    RolloutItem, RolloutRecorder, build_durable_tool_payload,
};
use crate::tape::{ContextItem, ContextItemsDelta, Tape};

/// Warning emitted when rollback succeeds but remains in-memory only.
pub const ROLLBACK_NON_DURABLE_WARNING: &str =
    "Rollback is in-memory only and will not survive runtime restart.";

/// Structured outcome for an in-memory rollback request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RollbackOutcome {
    /// Number of logical user turns actually removed.
    pub removed_turns: u32,
    /// Number of tape messages removed by the rollback.
    pub removed_messages: usize,
}

/// Server-managed continuation state for Responses-compatible providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesContinuationState {
    pub provider: String,
    pub last_response_id: String,
    pub boundary_message_count: usize,
    pub reference_context_revision: u64,
}

/// Represents a conversation/task session
#[derive(Debug)]
pub struct Session {
    /// Session ID
    pub id: String,
    /// Conversation history and summary
    pub tape: Tape,
    /// Optional recorder for persistence
    pub recorder: Option<RolloutRecorder>,
    /// Whether a sourcing task has been started in this session
    pub has_active_task: bool,
    /// Session-scoped client-provided dynamic tools exposed to the model.
    pub dynamic_tools: HashMap<String, alan_agent_protocol::DynamicToolSpec>,
    /// Session-scoped negotiated client capabilities for adaptive UI emission.
    pub client_capabilities: alan_agent_protocol::ClientCapabilities,
    /// Latest effect record by idempotency key (used for side-effect dedupe).
    effect_index: HashMap<String, EffectRecord>,
    /// Last prompt snapshot fingerprint written to rollout (used to skip duplicates).
    last_turn_context_snapshot_fingerprint: Option<String>,
    /// Monotonic user turn ordinal (never decremented by rollback/compaction).
    user_turn_ordinal: u64,
    /// Consecutive compaction degradation/failure count.
    compaction_failure_streak: u32,
    /// Latest persisted compaction attempt snapshot.
    latest_compaction_attempt: Option<CompactionAttemptSnapshot>,
    /// Latest persisted memory-flush attempt snapshot.
    latest_memory_flush_attempt: Option<MemoryFlushAttemptSnapshot>,
    /// Whether the current automatic compaction cycle already attempted a silent memory flush.
    auto_memory_flush_attempted_in_cycle: bool,
    /// Responses API continuation state, used when chaining via `previous_response_id`.
    responses_continuation: Option<ResponsesContinuationState>,
}

pub use crate::tape::{Message, MessageRole};

impl Session {
    const RESPONSES_CONTINUATION_EVENT_TYPE: &'static str = "responses_continuation";

    fn responses_continuation_from_event_records(
        event_records: &[EventRecord],
    ) -> Option<ResponsesContinuationState> {
        event_records.iter().fold(None, |_, event| {
            if event.event_type != Self::RESPONSES_CONTINUATION_EVENT_TYPE {
                return None;
            }

            if event
                .payload
                .get("cleared")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                return None;
            }

            let provider = event
                .payload
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let last_response_id = event
                .payload
                .get("last_response_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let boundary_message_count = event
                .payload
                .get("boundary_message_count")
                .and_then(serde_json::Value::as_u64)?
                as usize;
            let reference_context_revision = event
                .payload
                .get("reference_context_revision")
                .and_then(serde_json::Value::as_u64)?;

            Some(ResponsesContinuationState {
                provider,
                last_response_id,
                boundary_message_count,
                reference_context_revision,
            })
        })
    }

    fn runtime_confirmation_control_checkpoint(
        payload: &serde_json::Value,
    ) -> Option<(&str, &str)> {
        let checkpoint_id = payload
            .get("checkpoint_id")
            .and_then(serde_json::Value::as_str)?;
        let checkpoint_type = payload
            .get("checkpoint_type")
            .and_then(serde_json::Value::as_str)?;
        let choice = payload.get("choice").and_then(serde_json::Value::as_str)?;

        if !is_runtime_confirmation_checkpoint_type(checkpoint_type) {
            return None;
        }
        if !matches!(choice, "approve" | "reject") {
            return None;
        }
        let prefix = runtime_confirmation_checkpoint_prefix(checkpoint_type)?;
        if !checkpoint_id.starts_with(prefix) {
            return None;
        }

        Some((checkpoint_id, checkpoint_type))
    }

    fn has_runtime_confirmation_control_kind_and_version(
        payload: &serde_json::Value,
        checkpoint_type: &str,
    ) -> bool {
        let marker = payload.get("__alan_internal_control");
        let marker_kind = marker
            .and_then(|value| value.get("kind"))
            .and_then(serde_json::Value::as_str);
        let marker_version = marker
            .and_then(|value| value.get("version"))
            .and_then(serde_json::Value::as_u64);

        marker_kind == runtime_confirmation_control_kind(checkpoint_type)
            && marker_version == Some(RUNTIME_CONFIRMATION_CONTROL_VERSION)
    }

    fn runtime_confirmation_control_source(payload: &serde_json::Value) -> Option<&str> {
        payload
            .get("__alan_internal_control")
            .and_then(|marker| marker.get("source"))
            .and_then(serde_json::Value::as_str)
    }

    fn is_runtime_confirmation_control_payload(payload: &serde_json::Value) -> bool {
        let Some((_, checkpoint_type)) = Self::runtime_confirmation_control_checkpoint(payload)
        else {
            return false;
        };

        Self::has_runtime_confirmation_control_kind_and_version(payload, checkpoint_type)
            && Self::runtime_confirmation_control_source(payload)
                == Some(RUNTIME_CONFIRMATION_CONTROL_SOURCE)
    }

    fn is_legacy_runtime_confirmation_control_payload_for_restore(
        payload: &serde_json::Value,
        known_checkpoints: &HashMap<String, String>,
    ) -> bool {
        let Some((checkpoint_id, checkpoint_type)) =
            Self::runtime_confirmation_control_checkpoint(payload)
        else {
            return false;
        };

        known_checkpoints.get(checkpoint_id).map(String::as_str) == Some(checkpoint_type)
            && Self::has_runtime_confirmation_control_kind_and_version(payload, checkpoint_type)
            && Self::runtime_confirmation_control_source(payload).is_none()
    }

    fn normalize_runtime_confirmation_control_payload_for_restore(
        payload: &mut serde_json::Value,
        known_checkpoints: &HashMap<String, String>,
    ) -> bool {
        let Some((checkpoint_id, checkpoint_type)) =
            Self::runtime_confirmation_control_checkpoint(payload)
        else {
            return false;
        };
        let has_known_checkpoint =
            known_checkpoints.get(checkpoint_id).map(String::as_str) == Some(checkpoint_type);
        if !has_known_checkpoint {
            return false;
        }

        if Self::is_runtime_confirmation_control_payload(payload) {
            return true;
        }
        if !Self::is_legacy_runtime_confirmation_control_payload_for_restore(
            payload,
            known_checkpoints,
        ) {
            return false;
        }

        if let Some(marker) = payload
            .get_mut("__alan_internal_control")
            .and_then(serde_json::Value::as_object_mut)
        {
            marker.insert(
                "source".to_string(),
                serde_json::Value::String(RUNTIME_CONFIRMATION_CONTROL_SOURCE.to_string()),
            );
        }
        true
    }

    fn is_runtime_confirmation_control_parts(parts: &[crate::tape::ContentPart]) -> bool {
        parts.iter().any(|part| {
            matches!(
                part,
                crate::tape::ContentPart::Structured { data }
                    if Self::is_runtime_confirmation_control_payload(data)
            )
        })
    }

    fn is_runtime_confirmation_control_message(message: &Message) -> bool {
        match message {
            Message::User { parts } => Self::is_runtime_confirmation_control_parts(parts),
            _ => false,
        }
    }

    fn normalize_runtime_confirmation_control_message_for_restore(
        message: &mut Message,
        known_checkpoints: &HashMap<String, String>,
    ) -> bool {
        let Message::User { parts } = message else {
            return false;
        };

        let mut is_control = false;
        for part in parts.iter_mut() {
            match part {
                crate::tape::ContentPart::Structured { data } => {
                    is_control |= Self::normalize_runtime_confirmation_control_payload_for_restore(
                        data,
                        known_checkpoints,
                    );
                }
                crate::tape::ContentPart::Text { text } => {
                    if let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(text.trim())
                        && Self::normalize_runtime_confirmation_control_payload_for_restore(
                            &mut payload,
                            known_checkpoints,
                        )
                    {
                        *part = crate::tape::ContentPart::structured(payload);
                        is_control = true;
                    }
                }
                _ => {}
            }
        }
        is_control
    }

    fn normalize_runtime_confirmation_control_content_for_restore(
        content: &str,
        known_checkpoints: &HashMap<String, String>,
    ) -> Option<serde_json::Value> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut payload = serde_json::from_str::<serde_json::Value>(trimmed).ok()?;
        if Self::normalize_runtime_confirmation_control_payload_for_restore(
            &mut payload,
            known_checkpoints,
        ) {
            return Some(payload);
        }
        None
    }

    fn turn_ordinal_from_effect_idempotency_key(key: &str) -> Option<u64> {
        let payload = key.strip_prefix("run:")?;
        let marker_index = payload.rfind(":turn:")?;
        let tail = &payload[(marker_index + ":turn:".len())..];
        let turn_segment = tail.split(':').next()?;
        turn_segment.parse::<u64>().ok()
    }

    fn legacy_compaction_attempt_id(event: &EventRecord, item_index: usize) -> Option<String> {
        let payload = serde_json::to_vec(&event.payload).ok()?;
        let mut hasher = Sha256::new();
        hasher.update(event.event_type.as_bytes());
        hasher.update(b"\n");
        hasher.update(event.timestamp.as_bytes());
        hasher.update(b"\n");
        hasher.update(item_index.to_be_bytes());
        hasher.update(b"\n");
        hasher.update(payload);
        Some(format!("legacy-{}", hex::encode(hasher.finalize())))
    }

    fn legacy_compaction_attempt_from_event(
        event: &EventRecord,
        item_index: usize,
    ) -> Option<CompactionAttemptSnapshot> {
        if event.event_type != "compaction_attempt" {
            return None;
        }

        let payload = &event.payload;
        let mode = serde_json::from_value::<CompactionMode>(payload.get("mode")?.clone()).ok()?;
        let trigger =
            serde_json::from_value::<CompactionTrigger>(payload.get("trigger")?.clone()).ok()?;
        let reason =
            serde_json::from_value::<CompactionReason>(payload.get("reason")?.clone()).ok()?;
        let result =
            serde_json::from_value::<CompactionResult>(payload.get("result")?.clone()).ok()?;
        let retry_count = payload
            .get("retry_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0);
        let failure_streak = payload
            .get("failure_streak")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        let reference_context_revision = payload
            .get("reference_context_revision")
            .and_then(serde_json::Value::as_u64);
        let focus = payload
            .get("focus")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string);

        Some(CompactionAttemptSnapshot {
            attempt_id: Self::legacy_compaction_attempt_id(event, item_index)?,
            submission_id: None,
            request: CompactionRequestMetadata {
                mode,
                trigger,
                reason,
                focus,
            },
            result,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: None,
            output_messages: None,
            input_prompt_tokens: None,
            output_prompt_tokens: None,
            retry_count,
            tape_mutated: matches!(
                result,
                CompactionResult::Success | CompactionResult::Retry | CompactionResult::Degraded
            ),
            warning_message: None,
            error_message: payload
                .get("error")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string),
            failure_streak,
            reference_context_revision_before: reference_context_revision,
            reference_context_revision_after: reference_context_revision,
            timestamp: event.timestamp.clone(),
        })
    }

    fn latest_compaction_attempt_from_rollout_items_internal(
        items: &[RolloutItem],
    ) -> Option<CompactionAttemptSnapshot> {
        let mut latest: Option<(usize, CompactionAttemptSnapshot)> = None;
        let mut pending_tape_mutating_attempts: HashMap<
            String,
            (usize, CompactionAttemptSnapshot),
        > = HashMap::new();
        for (item_index, item) in items.iter().enumerate() {
            match item {
                RolloutItem::CompactionAttempt(attempt) => {
                    Self::track_compaction_attempt(
                        &mut latest,
                        &mut pending_tape_mutating_attempts,
                        item_index,
                        attempt.clone(),
                    );
                }
                RolloutItem::Compacted(compacted) => {
                    if let Some(attempt) = Self::take_completed_compaction_attempt(
                        &mut pending_tape_mutating_attempts,
                        compacted,
                        latest.as_ref().map(|(latest_index, _)| *latest_index),
                    ) {
                        latest = Some((item_index, attempt));
                    }
                }
                RolloutItem::Event(event) => {
                    if let Some(attempt) =
                        Self::legacy_compaction_attempt_from_event(event, item_index)
                    {
                        Self::track_compaction_attempt(
                            &mut latest,
                            &mut pending_tape_mutating_attempts,
                            item_index,
                            attempt,
                        );
                    }
                }
                _ => {}
            }
        }
        latest.map(|(_, attempt)| attempt)
    }

    fn latest_memory_flush_attempt_from_rollout_items_internal(
        items: &[RolloutItem],
    ) -> Option<MemoryFlushAttemptSnapshot> {
        items.iter().rev().find_map(|item| match item {
            RolloutItem::MemoryFlushAttempt(attempt) => Some(attempt.clone()),
            _ => None,
        })
    }

    fn track_compaction_attempt(
        latest: &mut Option<(usize, CompactionAttemptSnapshot)>,
        pending_tape_mutating_attempts: &mut HashMap<String, (usize, CompactionAttemptSnapshot)>,
        item_index: usize,
        attempt: CompactionAttemptSnapshot,
    ) {
        if attempt.tape_mutated {
            pending_tape_mutating_attempts
                .insert(attempt.attempt_id.clone(), (item_index, attempt));
        } else {
            *latest = Some((item_index, attempt));
        }
    }

    fn take_completed_compaction_attempt(
        pending_tape_mutating_attempts: &mut HashMap<String, (usize, CompactionAttemptSnapshot)>,
        compacted: &CompactedItem,
        latest_index: Option<usize>,
    ) -> Option<CompactionAttemptSnapshot> {
        if let Some(attempt_id) = compacted.attempt_id.as_deref() {
            let (attempt_index, attempt) = pending_tape_mutating_attempts.remove(attempt_id)?;
            if latest_index.is_some_and(|latest_index| latest_index > attempt_index) {
                return None;
            }
            return Some(attempt);
        }

        if pending_tape_mutating_attempts.len() != 1 {
            return None;
        }

        let attempt_id = pending_tape_mutating_attempts.keys().next()?.clone();
        let attempt_index = pending_tape_mutating_attempts
            .get(&attempt_id)
            .map(|(item_index, _)| *item_index)?;
        if latest_index.is_some_and(|latest_index| latest_index > attempt_index) {
            return None;
        }

        pending_tape_mutating_attempts
            .remove(&attempt_id)
            .map(|(_, attempt)| attempt)
    }

    fn stabilize_recovered_compacted_item_link(
        mut compacted: Option<CompactedItem>,
        latest_attempt: Option<&CompactionAttemptSnapshot>,
    ) -> Option<CompactedItem> {
        if let Some(compacted) = compacted.as_mut()
            && compacted.attempt_id.is_none()
            && let Some(attempt) = latest_attempt
            && attempt.tape_mutated
        {
            compacted.attempt_id = Some(attempt.attempt_id.clone());
        }
        compacted
    }

    /// Create a new session without persistence
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tape: Tape::new(),
            recorder: None,
            has_active_task: false,
            dynamic_tools: HashMap::new(),
            client_capabilities: alan_agent_protocol::ClientCapabilities::default(),
            effect_index: HashMap::new(),
            last_turn_context_snapshot_fingerprint: None,
            user_turn_ordinal: 0,
            compaction_failure_streak: 0,
            latest_compaction_attempt: None,
            latest_memory_flush_attempt: None,
            auto_memory_flush_attempted_in_cycle: false,
            responses_continuation: None,
        }
    }

    pub(crate) async fn new_with_recorder_options(
        session_id: Option<&str>,
        model: &str,
        sessions_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        let id = session_id
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let recorder = match sessions_dir {
            Some(dir) => {
                RolloutRecorder::new_in_dir_with_cwd_and_reasoning_effort(
                    &id,
                    model,
                    dir,
                    rollout_cwd,
                    reasoning_effort,
                )
                .await?
            }
            None => {
                RolloutRecorder::new_with_cwd_and_reasoning_effort(
                    &id,
                    model,
                    rollout_cwd,
                    reasoning_effort,
                )
                .await?
            }
        };

        Ok(Self {
            id,
            tape: Tape::new(),
            recorder: Some(recorder),
            has_active_task: false,
            dynamic_tools: HashMap::new(),
            client_capabilities: alan_agent_protocol::ClientCapabilities::default(),
            effect_index: HashMap::new(),
            last_turn_context_snapshot_fingerprint: None,
            user_turn_ordinal: 0,
            compaction_failure_streak: 0,
            latest_compaction_attempt: None,
            latest_memory_flush_attempt: None,
            auto_memory_flush_attempted_in_cycle: false,
            responses_continuation: None,
        })
    }

    /// Create a new session with recorder for persistence
    pub async fn new_with_recorder(model: &str) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(None, model, None, None, None).await
    }

    /// Create a new session with recorder under a specific sessions directory.
    pub async fn new_with_recorder_in_dir(
        model: &str,
        sessions_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(None, model, Some(sessions_dir), None, None).await
    }

    /// Create a new session with a specific ID and recorder
    pub async fn new_with_id_and_recorder(session_id: &str, model: &str) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(Some(session_id), model, None, None, None).await
    }

    /// Create a new session with a specific ID and recorder in a specific sessions directory.
    pub async fn new_with_id_and_recorder_in_dir(
        session_id: &str,
        model: &str,
        sessions_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(Some(session_id), model, Some(sessions_dir), None, None)
            .await
    }

    /// Load a session from a rollout file
    pub async fn load_from_rollout(path: &PathBuf, model: &str) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(path, None, model, None, None, None).await
    }

    /// Load a session from a rollout file while overriding the session ID for new persistence.
    pub async fn load_from_rollout_with_id(
        path: &PathBuf,
        session_id: &str,
        model: &str,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(path, Some(session_id), model, None, None, None).await
    }

    /// Load a session from a rollout file, writing future persistence to a specific sessions dir.
    pub async fn load_from_rollout_in_dir(
        path: &PathBuf,
        model: &str,
        sessions_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(path, None, model, Some(sessions_dir), None, None).await
    }

    /// Load a session from a rollout file with an explicit session ID and sessions dir.
    pub async fn load_from_rollout_in_dir_with_id(
        path: &PathBuf,
        session_id: &str,
        model: &str,
        sessions_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(
            path,
            Some(session_id),
            model,
            Some(sessions_dir),
            None,
            None,
        )
        .await
    }

    pub(crate) async fn load_from_rollout_with_recorder_cwd(
        path: &PathBuf,
        session_id_override: Option<&str>,
        model: &str,
        sessions_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(
            path,
            session_id_override,
            model,
            sessions_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await
    }

    async fn load_from_rollout_impl(
        path: &PathBuf,
        session_id_override: Option<&str>,
        model: &str,
        sessions_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        let items = RolloutRecorder::load_history(path).await?;

        // Use a hashed storage key when the host provides an external session identifier.
        let session_id = if let Some(session_id_override) = session_id_override {
            let mut hasher = Sha256::new();
            hasher.update(session_id_override.as_bytes());
            hex::encode(hasher.finalize())
        } else {
            items
                .first()
                .and_then(|item| match item {
                    RolloutItem::SessionMeta(meta) => Some(meta.session_id.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        };

        // Create a new session with recorder
        let mut session = Self::new_with_recorder_options(
            Some(&session_id),
            model,
            sessions_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await?;

        let recovered_latest_compaction_attempt =
            Self::latest_compaction_attempt_from_rollout_items_internal(&items);
        let recovered_latest_memory_flush_attempt =
            Self::latest_memory_flush_attempt_from_rollout_items_internal(&items);
        let mut context_items: Vec<ContextItem> = Vec::new();
        let mut fallback_tool_calls: Vec<crate::rollout::ToolCallRecord> = Vec::new();
        let mut compaction_attempt_records: Vec<CompactionAttemptSnapshot> = Vec::new();
        let mut memory_flush_attempt_records: Vec<MemoryFlushAttemptSnapshot> = Vec::new();
        let mut recovered_compaction: Option<CompactedItem> = None;
        let mut effect_records: Vec<EffectRecord> = Vec::new();
        let mut event_records: Vec<EventRecord> = Vec::new();
        let mut has_tool_message_content = false;
        let known_runtime_confirmation_checkpoints = items
            .iter()
            .filter_map(|item| match item {
                RolloutItem::Checkpoint(checkpoint)
                    if is_runtime_confirmation_checkpoint_type(&checkpoint.checkpoint_type) =>
                {
                    Some((
                        checkpoint.checkpoint_id.clone(),
                        checkpoint.checkpoint_type.clone(),
                    ))
                }
                _ => None,
            })
            .collect::<HashMap<_, _>>();

        // Replay messages from history
        for (item_index, item) in items.into_iter().enumerate() {
            match item {
                RolloutItem::Message(msg) => {
                    if let Some(mut message) = msg.message {
                        if message.is_context() {
                            continue;
                        }
                        let is_control_message =
                            Self::normalize_runtime_confirmation_control_message_for_restore(
                                &mut message,
                                &known_runtime_confirmation_checkpoints,
                            );
                        if message.is_user() && !is_control_message {
                            session.user_turn_ordinal = session.user_turn_ordinal.saturating_add(1);
                        }
                        if message.is_tool() {
                            has_tool_message_content = true;
                        }
                        session.tape.push(message);
                        continue;
                    }

                    let role = match msg.role.as_str() {
                        "user" => MessageRole::User,
                        "assistant" => MessageRole::Assistant,
                        "tool" => MessageRole::Tool,
                        "system" => MessageRole::System,
                        "context" => MessageRole::Context,
                        _ => MessageRole::User,
                    };

                    let content = msg.content.unwrap_or_default();
                    let normalized_control_payload = if matches!(role, MessageRole::User) {
                        Self::normalize_runtime_confirmation_control_content_for_restore(
                            &content,
                            &known_runtime_confirmation_checkpoints,
                        )
                    } else {
                        None
                    };
                    if matches!(role, MessageRole::Tool) && !content.trim().is_empty() {
                        has_tool_message_content = true;
                    }
                    if matches!(role, MessageRole::Context) {
                        continue;
                    }
                    if matches!(role, MessageRole::User) && normalized_control_payload.is_none() {
                        session.user_turn_ordinal = session.user_turn_ordinal.saturating_add(1);
                    }

                    let message = match role {
                        MessageRole::User => match normalized_control_payload {
                            Some(payload) => {
                                Message::user_parts(vec![crate::tape::ContentPart::structured(
                                    payload,
                                )])
                            }
                            None => Message::user(&content),
                        },
                        MessageRole::Assistant => Message::assistant(&content),
                        MessageRole::Tool => {
                            // Try to parse content as structured JSON, fall back to text
                            let tool_id = msg.tool_name.unwrap_or_default();
                            match serde_json::from_str::<serde_json::Value>(content.trim()) {
                                Ok(payload) => Message::tool_structured(&tool_id, payload),
                                Err(_) => Message::tool_text(&tool_id, &content),
                            }
                        }
                        MessageRole::System => Message::system(&content),
                        MessageRole::Context => unreachable!(),
                    };
                    session.tape.push(message);
                }
                RolloutItem::TurnContext(ctx) => {
                    context_items = ctx
                        .context_items
                        .into_iter()
                        .map(|item| ContextItem {
                            id: item.id,
                            kind: item.kind,
                            title: item.title,
                            content: item.content,
                            fingerprint: item.fingerprint,
                        })
                        .collect();
                }
                RolloutItem::Compacted(compacted) => {
                    session.tape.set_summary(compacted.message.clone());
                    recovered_compaction = Some(compacted);
                }
                RolloutItem::CompactionAttempt(attempt) => {
                    compaction_attempt_records.push(attempt);
                }
                RolloutItem::MemoryFlushAttempt(attempt) => {
                    memory_flush_attempt_records.push(attempt);
                }
                RolloutItem::ToolCall(tool_call) => fallback_tool_calls.push(tool_call),
                RolloutItem::Effect(effect) => effect_records.push(effect),
                RolloutItem::Event(event) => {
                    if let Some(attempt) =
                        Self::legacy_compaction_attempt_from_event(&event, item_index)
                    {
                        compaction_attempt_records.push(attempt);
                    } else {
                        event_records.push(event);
                    }
                }
                _ => {} // Skip other item types during loading
            }
        }

        // Backward compatibility: older rollouts recorded tool messages with null content.
        // In that case, recover tool payloads from tool_call records.
        if !has_tool_message_content {
            for tool_call in fallback_tool_calls {
                session
                    .tape
                    .push(Message::tool_structured(&tool_call.name, tool_call.result));
            }
        }

        if !session.tape.is_empty() {
            session.has_active_task = true;
        }

        if !context_items.is_empty() {
            let _ = session.tape.apply_context_items(context_items);
        }
        recovered_compaction = Self::stabilize_recovered_compacted_item_link(
            recovered_compaction,
            recovered_latest_compaction_attempt.as_ref(),
        );
        session.latest_compaction_attempt = recovered_latest_compaction_attempt;
        session.latest_memory_flush_attempt = recovered_latest_memory_flush_attempt;
        session.responses_continuation =
            Self::responses_continuation_from_event_records(&event_records);

        for effect in &effect_records {
            session
                .effect_index
                .insert(effect.idempotency_key.clone(), effect.clone());
        }
        if let Some(max_effect_turn) = effect_records
            .iter()
            .filter_map(|effect| {
                Self::turn_ordinal_from_effect_idempotency_key(&effect.idempotency_key)
            })
            .max()
        {
            session.user_turn_ordinal = session.user_turn_ordinal.max(max_effect_turn);
        }

        let recovered_messages = session.tape.messages().to_vec();
        if (!recovered_messages.is_empty()
            || recovered_compaction.is_some()
            || !compaction_attempt_records.is_empty()
            || !memory_flush_attempt_records.is_empty()
            || !effect_records.is_empty()
            || !event_records.is_empty())
            && let Some(recorder) = session.recorder.as_ref()
        {
            for message in recovered_messages {
                if let Err(err) = recorder.record_tape_message_nowait(&message) {
                    error!(error = %err, "Failed to re-persist recovered message");
                }
            }
            for attempt in compaction_attempt_records {
                if let Err(err) = recorder.record_compaction_attempt_nowait(attempt) {
                    error!(error = %err, "Failed to re-persist recovered compaction attempt");
                }
            }
            for attempt in memory_flush_attempt_records {
                if let Err(err) = recorder.record_memory_flush_attempt_nowait(attempt) {
                    error!(error = %err, "Failed to re-persist recovered memory flush attempt");
                }
            }
            if let Some(compacted) = recovered_compaction
                && let Err(err) = recorder.record_compacted_item_nowait(compacted)
            {
                error!(error = %err, "Failed to re-persist recovered summary");
            }
            for effect in effect_records {
                if let Err(err) = recorder.record_effect_nowait(effect) {
                    error!(error = %err, "Failed to re-persist recovered effect");
                }
            }
            for event in event_records {
                if let Err(err) = recorder.record_event_item_nowait(event) {
                    error!(error = %err, "Failed to re-persist recovered event");
                }
            }
            if let Err(err) = recorder.flush().await {
                error!(error = %err, "Failed to flush recovered rollout state");
            }
        }

        Ok(session)
    }

    /// Add a user message to the session
    pub fn add_user_message(&mut self, content: &str) {
        self.add_user_message_parts(vec![crate::tape::ContentPart::text(content)]);
    }

    fn add_user_message_parts_internal(
        &mut self,
        parts: Vec<crate::tape::ContentPart>,
        count_as_turn: bool,
    ) {
        if count_as_turn {
            self.user_turn_ordinal = self.user_turn_ordinal.saturating_add(1);
        }
        let message = Message::User { parts };
        self.tape.push(message.clone());

        // Record to persistence if available (enqueue to recorder writer queue)
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_tape_message_nowait(&message)
        {
            error!(error = %err, "Failed to record user message");
        }
    }

    /// Add a user message with rich content parts to the session
    pub fn add_user_message_parts(&mut self, parts: Vec<crate::tape::ContentPart>) {
        self.add_user_message_parts_internal(parts, true);
    }

    /// Add a synthetic user control message without incrementing turn ordinal.
    pub fn add_user_control_message_parts(&mut self, parts: Vec<crate::tape::ContentPart>) {
        self.add_user_message_parts_internal(parts, false);
    }

    /// Add an assistant message to the session
    pub fn add_assistant_message(&mut self, content: &str, thinking: Option<&str>) {
        self.add_assistant_message_with_reasoning(content, thinking, None, &[]);
    }

    /// Add an assistant message to the session with full reasoning metadata.
    pub fn add_assistant_message_with_reasoning(
        &mut self,
        content: &str,
        thinking: Option<&str>,
        thinking_signature: Option<&str>,
        redacted_thinking: &[String],
    ) {
        let mut parts = Vec::new();
        if let Some(t) = thinking
            && !t.is_empty()
        {
            let part = match thinking_signature {
                Some(sig) if !sig.trim().is_empty() => {
                    crate::tape::ContentPart::thinking_with_signature(t, sig)
                }
                _ => crate::tape::ContentPart::thinking(t),
            };
            parts.push(part);
        }
        for block in redacted_thinking {
            if !block.trim().is_empty() {
                parts.push(crate::tape::ContentPart::redacted_thinking(block.clone()));
            }
        }
        parts.push(crate::tape::ContentPart::text(content));
        let message = Message::Assistant {
            parts,
            tool_requests: vec![],
        };
        self.tape.push(message.clone());

        // Record to persistence if available (enqueue to recorder writer queue)
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_tape_message_nowait(&message)
        {
            error!(error = %err, "Failed to record assistant message");
        }
    }

    /// Add an assistant message with tool calls to the session
    pub fn add_assistant_message_with_tool_calls(
        &mut self,
        content: &str,
        tool_calls: Vec<crate::tape::ToolRequest>,
        thinking: Option<&str>,
    ) {
        self.add_assistant_message_with_tool_calls_and_reasoning(
            content,
            tool_calls,
            thinking,
            None,
            &[],
        );
    }

    /// Add an assistant message with tool calls and full reasoning metadata.
    pub fn add_assistant_message_with_tool_calls_and_reasoning(
        &mut self,
        content: &str,
        tool_calls: Vec<crate::tape::ToolRequest>,
        thinking: Option<&str>,
        thinking_signature: Option<&str>,
        redacted_thinking: &[String],
    ) {
        let mut parts = Vec::new();
        if let Some(t) = thinking
            && !t.is_empty()
        {
            let part = match thinking_signature {
                Some(sig) if !sig.trim().is_empty() => {
                    crate::tape::ContentPart::thinking_with_signature(t, sig)
                }
                _ => crate::tape::ContentPart::thinking(t),
            };
            parts.push(part);
        }
        for block in redacted_thinking {
            if !block.trim().is_empty() {
                parts.push(crate::tape::ContentPart::redacted_thinking(block.clone()));
            }
        }
        if !content.is_empty() {
            parts.push(crate::tape::ContentPart::text(content));
        }
        let message = Message::Assistant {
            parts,
            tool_requests: tool_calls,
        };
        self.tape.push(message.clone());

        // Record to persistence if available (enqueue to recorder writer queue)
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_tape_message_nowait(&message)
        {
            error!(error = %err, "Failed to record assistant message");
        }
    }

    /// Add a tool message to the session.
    /// Keeps full payload on tape; truncation is handled at LLM projection boundaries.
    ///
    /// # Arguments
    /// * `tool_call_id` - The ID of the tool call this message is responding to
    /// * `name` - The name of the tool that was called
    /// * `payload` - The result payload from the tool execution
    pub fn add_tool_message(
        &mut self,
        tool_call_id: &str,
        _name: &str,
        payload: serde_json::Value,
    ) {
        // Keep the live payload on tape (source of truth for the active runtime),
        // but persist a durable redacted/truncated view to rollout.
        let message = Message::tool_multi(vec![crate::tape::ToolResponse {
            id: tool_call_id.to_string(),
            content: Self::tool_payload_to_content_parts(payload.clone()),
        }]);
        self.tape.push(message.clone());

        let durable_message = {
            let durable_payload = build_durable_tool_payload(&payload);
            Message::tool_multi(vec![crate::tape::ToolResponse {
                id: tool_call_id.to_string(),
                content: Self::tool_payload_to_content_parts(durable_payload.payload),
            }])
        };

        // Record to persistence if available (enqueue to recorder writer queue)
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_tape_message_nowait(&durable_message)
        {
            error!(error = %err, "Failed to record tool message");
        }
    }

    fn tool_payload_to_content_parts(payload: serde_json::Value) -> Vec<crate::tape::ContentPart> {
        if let Ok(part) = serde_json::from_value::<crate::tape::ContentPart>(payload.clone()) {
            return vec![part];
        }

        if let Ok(parts) = serde_json::from_value::<Vec<crate::tape::ContentPart>>(payload.clone())
            && !parts.is_empty()
        {
            return parts;
        }

        match payload {
            serde_json::Value::Object(mut map) => {
                if let Some(content_parts_value) = map.remove("content_parts") {
                    match serde_json::from_value::<Vec<crate::tape::ContentPart>>(
                        content_parts_value.clone(),
                    ) {
                        Ok(mut parts) if !parts.is_empty() => {
                            if !map.is_empty() {
                                parts.push(crate::tape::ContentPart::structured(
                                    serde_json::Value::Object(map),
                                ));
                            }
                            return parts;
                        }
                        Ok(_) | Err(_) => {}
                    }
                    map.insert("content_parts".to_string(), content_parts_value);
                    return vec![crate::tape::ContentPart::structured(
                        serde_json::Value::Object(map),
                    )];
                }

                vec![crate::tape::ContentPart::structured(
                    serde_json::Value::Object(map),
                )]
            }
            other => vec![crate::tape::ContentPart::structured(other)],
        }
    }

    fn tool_response_content_to_payload(
        content: &[crate::tape::ContentPart],
    ) -> Option<serde_json::Value> {
        if content.is_empty() {
            return None;
        }
        if content.len() == 1
            && let crate::tape::ContentPart::Structured { data } = &content[0]
        {
            return Some(data.clone());
        }
        if content.len() == 1 {
            return serde_json::to_value(&content[0]).ok();
        }
        serde_json::to_value(content)
            .ok()
            .map(|parts| serde_json::json!({ "content_parts": parts }))
    }

    /// Lookup a previously recorded tool payload by tool call ID.
    pub fn tool_payload_by_call_id(&self, tool_call_id: &str) -> Option<serde_json::Value> {
        self.tape.messages().iter().rev().find_map(|message| {
            message.tool_responses().iter().rev().find_map(|response| {
                if response.id == tool_call_id {
                    Self::tool_response_content_to_payload(&response.content)
                } else {
                    None
                }
            })
        })
    }

    pub fn responses_continuation(&self) -> Option<&ResponsesContinuationState> {
        self.responses_continuation.as_ref()
    }

    pub fn mark_responses_continuation(
        &mut self,
        provider: &str,
        response_id: &str,
        boundary_message_count: usize,
        reference_context_revision: u64,
    ) {
        let provider = provider.trim();
        let response_id = response_id.trim();
        if provider.is_empty() || response_id.is_empty() {
            return;
        }

        let state = ResponsesContinuationState {
            provider: provider.to_string(),
            last_response_id: response_id.to_string(),
            boundary_message_count,
            reference_context_revision,
        };
        self.responses_continuation = Some(state.clone());
        self.record_event(
            Self::RESPONSES_CONTINUATION_EVENT_TYPE,
            serde_json::json!({
                "provider": state.provider,
                "last_response_id": state.last_response_id,
                "boundary_message_count": state.boundary_message_count,
                "reference_context_revision": state.reference_context_revision,
                "cleared": false,
            }),
        );
    }

    pub fn clear_responses_continuation(&mut self, reason: &str) {
        let Some(previous) = self.responses_continuation.take() else {
            return;
        };
        self.record_event(
            Self::RESPONSES_CONTINUATION_EVENT_TYPE,
            serde_json::json!({
                "provider": previous.provider,
                "last_response_id": previous.last_response_id,
                "boundary_message_count": previous.boundary_message_count,
                "reference_context_revision": previous.reference_context_revision,
                "cleared": true,
                "reason": reason,
            }),
        );
    }

    /// Clear the session state (but keep the recorder)
    pub fn clear(&mut self) {
        self.tape.clear();
        self.has_active_task = false;
        self.last_turn_context_snapshot_fingerprint = None;
        self.clear_responses_continuation("session_cleared");
    }

    /// Roll back the last `num_turns` user turns from in-memory context.
    ///
    /// This mutation is intentionally non-durable: recovery from persisted rollout
    /// history does not re-apply rollback markers to session state.
    ///
    /// A "turn" is approximated as one user message plus any following assistant/tool
    /// messages until the next user message.
    pub fn rollback_last_turns(&mut self, requested_turns: u32) -> RollbackOutcome {
        if requested_turns == 0 {
            return RollbackOutcome {
                removed_turns: 0,
                removed_messages: 0,
            };
        }

        let messages = self.tape.messages();
        if messages.is_empty() {
            return RollbackOutcome {
                removed_turns: 0,
                removed_messages: 0,
            };
        }

        let mut user_turns_seen = 0_u32;
        let mut remove_from = messages.len();

        for (idx, msg) in messages.iter().enumerate().rev() {
            remove_from = idx;
            if matches!(msg, Message::User { .. })
                && !Self::is_runtime_confirmation_control_message(msg)
            {
                user_turns_seen += 1;
                if user_turns_seen >= requested_turns {
                    break;
                }
            }
        }

        if user_turns_seen == 0 {
            return RollbackOutcome {
                removed_turns: 0,
                removed_messages: 0,
            };
        }

        let removed_messages = messages.len().saturating_sub(remove_from);
        let retained = messages[..remove_from].to_vec();
        self.tape.replace(retained);
        self.tape.clear_summary();
        self.clear_responses_continuation("rollback");
        if self.tape.messages().is_empty() {
            self.has_active_task = false;
        }

        self.record_event(
            "session_rollback",
            serde_json::json!({
                "requested_turns": requested_turns,
                "removed_turns": user_turns_seen,
                "removed_messages": removed_messages,
                "durable": false,
                "scope": "in_memory",
                "warning": ROLLBACK_NON_DURABLE_WARNING
            }),
        );

        RollbackOutcome {
            removed_turns: user_turns_seen,
            removed_messages,
        }
    }

    /// Record a tool call to persistence (enqueue only; background writer performs IO)
    pub fn record_tool_call(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
    ) {
        self.record_tool_call_with_audit(name, arguments, result, success, None);
    }

    /// Record a tool call with governance/execution-backend audit metadata.
    pub fn record_tool_call_with_audit(
        &self,
        name: &str,
        arguments: serde_json::Value,
        result: serde_json::Value,
        success: bool,
        audit: Option<alan_agent_protocol::ToolDecisionAudit>,
    ) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) =
                recorder.record_tool_call_nowait_with_audit(name, arguments, result, success, audit)
        {
            error!(error = %err, "Failed to record tool call");
        }
    }

    /// Record an effect state transition and update in-memory dedupe index.
    pub fn record_effect(&mut self, effect: EffectRecord) {
        self.effect_index
            .insert(effect.idempotency_key.clone(), effect.clone());
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_effect_nowait(effect)
        {
            error!(error = %err, "Failed to record effect");
        }
    }

    /// Lookup latest effect record by idempotency key.
    pub fn effect_by_idempotency_key(&self, key: &str) -> Option<EffectRecord> {
        self.effect_index.get(key).cloned()
    }

    /// Count user turns currently present on the tape.
    pub fn user_turn_count(&self) -> usize {
        self.tape
            .messages()
            .iter()
            .filter(|message| message.is_user())
            .count()
    }

    /// Monotonic user turn ordinal for idempotency key derivation.
    pub fn user_turn_ordinal(&self) -> u64 {
        self.user_turn_ordinal
    }

    /// Latest persisted compaction attempt, if any.
    pub fn latest_compaction_attempt(&self) -> Option<&CompactionAttemptSnapshot> {
        self.latest_compaction_attempt.as_ref()
    }

    /// Latest persisted memory-flush attempt, if any.
    pub fn latest_memory_flush_attempt(&self) -> Option<&MemoryFlushAttemptSnapshot> {
        self.latest_memory_flush_attempt.as_ref()
    }

    pub fn note_compaction_failure(&mut self) -> u32 {
        self.compaction_failure_streak = self.compaction_failure_streak.saturating_add(1);
        self.compaction_failure_streak
    }

    pub fn reset_compaction_failure_streak(&mut self) {
        self.compaction_failure_streak = 0;
    }

    pub fn auto_memory_flush_attempted_in_cycle(&self) -> bool {
        self.auto_memory_flush_attempted_in_cycle
    }

    pub fn note_auto_memory_flush_attempt(&mut self) {
        self.auto_memory_flush_attempted_in_cycle = true;
    }

    pub fn reset_auto_memory_flush_cycle(&mut self) {
        self.auto_memory_flush_attempted_in_cycle = false;
    }

    /// Record a checkpoint to persistence (enqueue only; background writer performs IO)
    pub fn record_checkpoint(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
    ) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) =
                recorder.record_checkpoint_nowait(checkpoint_id, checkpoint_type, summary, choice)
        {
            error!(error = %err, "Failed to record checkpoint");
        }
    }

    /// Record an event to persistence (enqueue only; background writer performs IO)
    pub fn record_event(&self, event_type: &str, payload: serde_json::Value) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_event_nowait(event_type, payload)
        {
            error!(error = %err, event_type = %event_type, "Failed to record event");
        }
    }

    /// Record a compaction summary to persistence (enqueue only; background writer performs IO)
    pub fn record_summary(&self, summary: &str) {
        self.record_compaction(CompactedItem::new(summary));
    }

    /// Record a compaction outcome to persistence (enqueue only; background writer performs IO)
    ///
    /// This low-level API persists only the compacted summary item. For tape-mutating compaction
    /// results, prefer [`Session::persist_compaction_observation`] so related rollout items are
    /// flushed together.
    pub(crate) fn record_compaction(&self, compacted: CompactedItem) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.record_compacted_item_nowait(compacted)
        {
            error!(error = %err, "Failed to record compaction outcome");
        }
    }

    /// Record a compaction attempt and its optional compacted summary in one persisted batch.
    pub async fn persist_compaction_observation(
        &mut self,
        attempt: CompactionAttemptSnapshot,
        compacted: Option<CompactedItem>,
    ) -> anyhow::Result<()> {
        let Some(recorder) = self.recorder.as_ref() else {
            self.latest_compaction_attempt = Some(attempt);
            return Ok(());
        };
        let latest_attempt = attempt.clone();
        let mut items = vec![RolloutItem::CompactionAttempt(attempt)];
        if let Some(compacted) = compacted {
            items.push(RolloutItem::Compacted(compacted));
        }
        recorder.persist_batch(items).await?;
        self.latest_compaction_attempt = Some(latest_attempt);
        Ok(())
    }

    /// Record a memory-flush attempt to persistence.
    pub async fn persist_memory_flush_attempt(
        &mut self,
        attempt: MemoryFlushAttemptSnapshot,
    ) -> anyhow::Result<()> {
        let Some(recorder) = self.recorder.as_ref() else {
            self.latest_memory_flush_attempt = Some(attempt);
            return Ok(());
        };
        let latest_attempt = attempt.clone();
        recorder.record_memory_flush_attempt(attempt).await?;
        self.latest_memory_flush_attempt = Some(latest_attempt);
        Ok(())
    }

    /// Record turn context snapshot to persistence (enqueue only; background writer performs IO)
    #[allow(clippy::too_many_arguments)]
    pub fn record_turn_context(
        &self,
        model: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
        system_prompt: &str,
        context_items: &[ContextItem],
        tools: &[String],
        memory_enabled: bool,
        active_skills: &[String],
    ) {
        let Some(recorder) = self.recorder.clone() else {
            return;
        };

        let items: Vec<ContextItemRecord> = context_items
            .iter()
            .map(|item| ContextItemRecord {
                id: item.id.clone(),
                kind: item.kind.clone(),
                title: item.title.clone(),
                content: item.content.clone(),
                fingerprint: item.fingerprint.clone(),
            })
            .collect();
        let tools = tools.to_vec();
        let active_skills = active_skills.to_vec();
        if let Err(err) = recorder.record_turn_context_nowait(
            model,
            reasoning_effort,
            system_prompt,
            items,
            tools,
            memory_enabled,
            active_skills,
            None,
        ) {
            error!(error = %err, "Failed to record turn context");
        }
    }

    /// Record turn context snapshot only when the observed prompt context changed.
    /// Returns `true` if a snapshot was recorded, `false` if it was skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn record_turn_context_if_changed(
        &mut self,
        model: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
        system_prompt: &str,
        context_items: &[ContextItem],
        tools: &[String],
        memory_enabled: bool,
        active_skills: &[String],
        context_delta: &ContextItemsDelta,
    ) -> bool {
        let fingerprint = fingerprint_turn_context_observation(
            model,
            reasoning_effort,
            system_prompt,
            context_items,
            tools,
            memory_enabled,
            active_skills,
        );

        if !context_delta.changed
            && self.last_turn_context_snapshot_fingerprint.as_deref() == Some(fingerprint.as_str())
        {
            return false;
        }

        self.last_turn_context_snapshot_fingerprint = Some(fingerprint);
        let Some(recorder) = self.recorder.clone() else {
            return true;
        };

        let items: Vec<ContextItemRecord> = context_items
            .iter()
            .map(|item| ContextItemRecord {
                id: item.id.clone(),
                kind: item.kind.clone(),
                title: item.title.clone(),
                content: item.content.clone(),
                fingerprint: item.fingerprint.clone(),
            })
            .collect();
        let tools = tools.to_vec();
        let active_skills = active_skills.to_vec();
        let reference_context = Some(ReferenceContextSnapshotRecord {
            revision: self.tape.context_revision(),
            changed: context_delta.changed,
            reordered: context_delta.reordered,
            added: context_delta.added_ids.len(),
            updated: context_delta.updated_ids.len(),
            removed: context_delta.removed_ids.len(),
        });
        if let Err(err) = recorder.record_turn_context_nowait(
            model,
            reasoning_effort,
            system_prompt,
            items,
            tools,
            memory_enabled,
            active_skills,
            reference_context,
        ) {
            error!(error = %err, "Failed to record turn context");
        }
        true
    }

    /// Flush pending writes to disk and wait for the writer queue to drain.
    pub async fn flush(&self) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = recorder.flush().await
        {
            error!(error = %err, "Failed to flush rollout recorder");
        }
    }

    /// Get the rollout file path if recorder is available
    pub fn rollout_path(&self) -> Option<&PathBuf> {
        self.recorder.as_ref().map(|r| r.path())
    }
}

/// Truncate a JSON payload to prevent context overflow
/// Recursively truncates large string values while preserving structure
#[cfg(test)]
fn truncate_payload(payload: serde_json::Value, max_size: usize) -> serde_json::Value {
    let payload_str = payload.to_string();
    if payload_str.len() <= max_size {
        return payload;
    }

    match payload {
        serde_json::Value::Object(map) => {
            let mut truncated = serde_json::Map::new();
            let mut current_size = 0;

            for (key, value) in map {
                // Always include critical fields
                let is_critical = matches!(key.as_str(), "success" | "error" | "url" | "title");

                if is_critical {
                    truncated.insert(key, value);
                    continue;
                }

                // For content/aggregated_content fields, truncate aggressively
                let processed_value = if key == "content" || key == "aggregated_content" {
                    if let serde_json::Value::String(s) = &value {
                        let truncated_str = truncate_text(s, max_size / 4);
                        serde_json::Value::String(truncated_str)
                    } else {
                        value
                    }
                } else {
                    truncate_payload(value, max_size / 2)
                };

                let value_str = processed_value.to_string();
                if current_size + value_str.len() < max_size * 3 / 4 {
                    truncated.insert(key, processed_value);
                    current_size += value_str.len();
                } else {
                    truncated.insert(
                        "_truncated".to_string(),
                        serde_json::Value::String("Additional fields omitted".to_string()),
                    );
                    break;
                }
            }

            serde_json::Value::Object(truncated)
        }
        serde_json::Value::Array(arr) => {
            let arr_len = arr.len();
            let mut truncated = Vec::new();
            let mut current_size = 0;

            for item in arr {
                let processed = truncate_payload(item, max_size / arr_len.max(1));
                let item_str = processed.to_string();

                if current_size + item_str.len() < max_size * 3 / 4 {
                    truncated.push(processed);
                    current_size += item_str.len();
                } else {
                    truncated.push(serde_json::json!({
                        "_note": "Additional array items omitted"
                    }));
                    break;
                }
            }

            serde_json::Value::Array(truncated)
        }
        serde_json::Value::String(s) => {
            if s.len() > max_size / 10 {
                serde_json::Value::String(truncate_text(&s, max_size / 10))
            } else {
                serde_json::Value::String(s)
            }
        }
        other => other,
    }
}

/// Truncate text to a maximum length, adding ellipsis if truncated
#[cfg(test)]
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    format!("{}...[truncated]", truncated)
}

fn fingerprint_turn_context_observation(
    model: &str,
    reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    system_prompt: &str,
    context_items: &[ContextItem],
    tools: &[String],
    memory_enabled: bool,
    active_skills: &[String],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update(b"\n");
    hasher.update(
        reasoning_effort
            .map(alan_agent_protocol::ReasoningEffort::as_str)
            .unwrap_or("unset")
            .as_bytes(),
    );
    hasher.update(b"\n");
    hasher.update(system_prompt.as_bytes());
    hasher.update(b"\n");
    hasher.update(if memory_enabled { b"1" } else { b"0" });
    hasher.update(b"\n");

    for item in context_items {
        hasher.update(item.id.as_bytes());
        hasher.update(b"\n");
        hasher.update(item.fingerprint.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"--tools--\n");
    for tool in tools {
        hasher.update(tool.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(b"--skills--\n");
    for skill in active_skills {
        hasher.update(skill.as_bytes());
        hasher.update(b"\n");
    }

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Recover the latest compaction attempt from rollout items, including legacy event records.
pub fn latest_compaction_attempt_from_rollout_items(
    items: &[RolloutItem],
) -> Option<CompactionAttemptSnapshot> {
    Session::latest_compaction_attempt_from_rollout_items_internal(items)
}

/// Recover the latest memory-flush attempt from rollout items.
pub fn latest_memory_flush_attempt_from_rollout_items(
    items: &[RolloutItem],
) -> Option<MemoryFlushAttemptSnapshot> {
    Session::latest_memory_flush_attempt_from_rollout_items_internal(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rollout::{
        CheckpointRecord, CompactedItem, EffectRecord, EffectStatus, EventRecord, MessageRecord,
        RolloutItem, RolloutRecorder, SessionMeta,
    };
    use crate::tape::{ContentPart, ToolResponse};
    use alan_agent_protocol::{
        CompactionAttemptSnapshot, CompactionMode, CompactionReason, CompactionRequestMetadata,
        CompactionResult, CompactionTrigger, MemoryFlushAttemptSnapshot, MemoryFlushResult,
        MemoryFlushSkipReason,
    };
    use tempfile::TempDir;

    #[test]
    fn test_session_new() {
        let session = Session::new();
        assert!(!session.id.is_empty());
        assert!(session.tape.messages().is_empty());
        assert!(session.recorder.is_none());
    }

    #[test]
    fn test_session_default() {
        let session = Session::default();
        assert!(!session.id.is_empty());
        assert!(session.tape.messages().is_empty());
    }

    #[test]
    fn test_add_user_message() {
        let mut session = Session::new();
        session.add_user_message("Hello, agent!");

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role(), MessageRole::User);
        assert_eq!(messages[0].text_content(), "Hello, agent!");
        assert_eq!(session.user_turn_ordinal(), 1);
    }

    #[test]
    fn test_user_turn_ordinal_is_monotonic_across_rollback() {
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_user_message("u2");
        assert_eq!(session.user_turn_ordinal(), 2);

        let removed = session.rollback_last_turns(1);
        assert!(removed.removed_messages > 0);
        assert_eq!(removed.removed_turns, 1);
        assert_eq!(session.user_turn_count(), 1);
        assert_eq!(session.user_turn_ordinal(), 2);

        session.add_user_message("u3");
        assert_eq!(session.user_turn_count(), 2);
        assert_eq!(session.user_turn_ordinal(), 3);
    }

    #[test]
    fn test_user_control_message_does_not_increment_turn_ordinal() {
        let mut session = Session::new();
        session.add_user_message("u1");
        assert_eq!(session.user_turn_ordinal(), 1);

        session.add_user_control_message_parts(vec![ContentPart::structured(
            serde_json::json!({"choice":"approve"}),
        )]);

        assert_eq!(session.user_turn_count(), 2);
        assert_eq!(session.user_turn_ordinal(), 1);
    }

    #[test]
    fn test_add_assistant_message() {
        let mut session = Session::new();
        session.add_assistant_message("I can help you!", None);

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role(), MessageRole::Assistant);
        assert_eq!(messages[0].text_content(), "I can help you!");
    }

    #[test]
    fn test_add_tool_message() {
        let mut session = Session::new();
        let payload = serde_json::json!({"result": "success"});
        session.add_tool_message("call_123", "search_tool", payload.clone());

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role(), MessageRole::Tool);
        let responses = messages[0].tool_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].id, "call_123");
    }

    #[test]
    fn test_add_tool_message_accepts_content_parts_payload() {
        let mut session = Session::new();
        let payload = serde_json::json!({
            "content_parts": [
                {"type": "text", "text": "hello"},
                {"type": "attachment", "hash": "abc123", "mime_type": "image/png", "metadata": {"w": 10, "h": 10}}
            ]
        });
        session.add_tool_message("call_123", "capture", payload);

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        let responses = messages[0].tool_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].id, "call_123");
        assert!(matches!(
            responses[0].content.first(),
            Some(ContentPart::Text { text }) if text == "hello"
        ));
        assert!(matches!(
            responses[0].content.get(1),
            Some(ContentPart::Attachment { hash, mime_type, .. })
            if hash == "abc123" && mime_type == "image/png"
        ));
    }

    #[test]
    fn test_add_tool_message_accepts_content_parts_array_payload() {
        let mut session = Session::new();
        let payload = serde_json::json!([
            {"type": "text", "text": "part-a"},
            {"type": "structured", "data": {"k": "v"}}
        ]);
        session.add_tool_message("call_124", "custom", payload);

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        let responses = messages[0].tool_responses();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].content.len(), 2);
        assert!(matches!(
            responses[0].content.first(),
            Some(ContentPart::Text { text }) if text == "part-a"
        ));
        assert!(matches!(
            responses[0].content.get(1),
            Some(ContentPart::Structured { data }) if data["k"] == "v"
        ));
    }

    #[test]
    fn test_responses_continuation_can_be_marked_and_cleared() {
        let mut session = Session::new();
        session.mark_responses_continuation("openai_responses", "resp_123", 2, 7);

        let continuation = session.responses_continuation().expect("continuation");
        assert_eq!(continuation.provider, "openai_responses");
        assert_eq!(continuation.last_response_id, "resp_123");
        assert_eq!(continuation.boundary_message_count, 2);
        assert_eq!(continuation.reference_context_revision, 7);

        session.clear_responses_continuation("test");
        assert!(session.responses_continuation().is_none());
    }

    #[test]
    fn test_multiple_messages() {
        let mut session = Session::new();
        session.add_user_message("First");
        session.add_assistant_message("Second", None);
        session.add_user_message("Third");

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role(), MessageRole::User);
        assert_eq!(messages[1].role(), MessageRole::Assistant);
        assert_eq!(messages[2].role(), MessageRole::User);
    }

    #[test]
    fn test_clear_session() {
        let mut session = Session::new();
        session.add_user_message("Test");

        session.clear();

        assert!(session.tape.messages().is_empty());
    }

    #[test]
    fn test_message_role_serialization() {
        let roles = vec![
            (MessageRole::System, "\"system\""),
            (MessageRole::Context, "\"context\""),
            (MessageRole::User, "\"user\""),
            (MessageRole::Assistant, "\"assistant\""),
            (MessageRole::Tool, "\"tool\""),
        ];

        for (role, expected) in roles {
            let json = serde_json::to_string(&role).unwrap();
            assert_eq!(json, expected);

            let deserialized: MessageRole = serde_json::from_str(expected).unwrap();
            assert!(std::mem::discriminant(&deserialized) == std::mem::discriminant(&role));
        }
    }

    #[test]
    fn test_message_serialization() {
        let message = Message::user("Hello");

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("Hello"));
        assert!(json.contains("user"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text_content(), "Hello");
    }

    #[test]
    fn test_message_serialization_with_tool() {
        let message = Message::Tool {
            responses: vec![ToolResponse {
                id: "web_search".to_string(),
                content: vec![ContentPart::structured(
                    serde_json::json!({"result": "found"}),
                )],
            }],
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("web_search"));
        assert!(json.contains("found"));

        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tool_responses()[0].id, "web_search");
    }

    #[test]
    fn test_session_rollout_path_without_recorder() {
        let session = Session::new();
        assert!(session.rollout_path().is_none());
    }

    #[test]
    fn test_session_has_active_task_defaults_false() {
        let session = Session::new();
        assert!(!session.has_active_task);
    }

    #[test]
    fn test_session_clear_resets_active_task() {
        let mut session = Session::new();
        session.has_active_task = true;
        session.clear();
        assert!(!session.has_active_task);
    }

    #[test]
    fn test_load_from_rollout_sets_active_task() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-123","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();
            assert!(session.has_active_task);
        });
    }

    #[test]
    fn test_load_from_rollout_in_dir_with_id_overrides_persisted_session_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-legacy.jsonl");

            let content = r#"{"type":"session_meta","session_id":"legacy-runtime-id","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir_with_id(
                &rollout_path,
                "daemon-session-id",
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            let storage_key = crate::rollout::session_storage_key("daemon-session-id");
            assert_eq!(session.id, storage_key);
            let persisted_path = session
                .rollout_path()
                .expect("session should create a new recorder path");
            let filename = persisted_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("rollout path should have a file name");
            assert!(filename.ends_with(&format!("-{storage_key}.jsonl")));
            let persisted_items = RolloutRecorder::load_history(persisted_path).await.unwrap();
            let persisted_session_id = persisted_items.into_iter().find_map(|item| match item {
                RolloutItem::SessionMeta(meta) => Some(meta.session_id),
                _ => None,
            });
            assert_eq!(persisted_session_id.as_deref(), Some(storage_key.as_str()));
            assert_eq!(session.tape.messages().len(), 1);
        });
    }

    #[test]
    fn test_load_from_rollout_with_recorder_cwd_persists_runtime_tool_cwd() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-cwd.jsonl");
            let resumed_cwd = temp_dir.path().join("workspace/src");
            tokio::fs::create_dir_all(&resumed_cwd).await.unwrap();

            let content = r#"{"type":"session_meta","session_id":"legacy-runtime-id","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp/original","model":"gemini-2.0-flash"}
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_with_recorder_cwd(
                &rollout_path,
                Some("daemon-session-id"),
                "gemini-2.0-flash",
                Some(temp_dir.path()),
                Some(resumed_cwd.as_path()),
                Some(alan_agent_protocol::ReasoningEffort::High),
            )
            .await
            .unwrap();

            let persisted_items = RolloutRecorder::load_history(
                session
                    .rollout_path()
                    .expect("session should create a new recorder path"),
            )
            .await
            .unwrap();
            let persisted_meta = persisted_items.into_iter().find_map(|item| match item {
                RolloutItem::SessionMeta(meta) => Some(meta),
                _ => None,
            });

            assert_eq!(
                persisted_meta.as_ref().map(|meta| meta.cwd.as_str()),
                Some(resumed_cwd.to_string_lossy().as_ref())
            );
            assert_eq!(
                persisted_meta.as_ref().and_then(|meta| meta.reasoning_effort),
                Some(alan_agent_protocol::ReasoningEffort::High)
            );
        });
    }

    #[test]
    fn test_load_from_rollout_restores_summary_and_tool_message() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-summary.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-456","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"compacted","message":"Prior summary","timestamp":"2026-01-29T14:30:53Z"}
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
{"type":"tool_call","name":"memory_search","arguments":{"query":"alpha"},"result":{"ok":true},"success":true,"timestamp":"2026-01-29T14:30:57Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            let messages = session.tape.messages_for_prompt();
            assert!(messages[0].text_content().contains("Prior summary"));

            let tool_message = messages
                .iter()
                .find(|m| m.is_tool())
                .expect("tool message missing");
            let responses = tool_message.tool_responses();
            assert_eq!(responses.len(), 1);
            assert_eq!(responses[0].id, "memory_search");
        });
    }

    #[test]
    fn test_load_from_rollout_parses_tool_message_json_payload() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-tool-message.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-tool-msg","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"message","role":"tool","content":"{\"ok\":true}","tool_name":"call_abc","timestamp":"2026-01-29T14:30:56Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            let tool_messages: Vec<&Message> = session
                .tape
                .messages()
                .iter()
                .filter(|m| m.is_tool())
                .collect();
            assert_eq!(tool_messages.len(), 1);
            let responses = tool_messages[0].tool_responses();
            assert_eq!(responses[0].id, "call_abc");
        });
    }

    #[test]
    fn test_load_from_rollout_does_not_duplicate_when_tool_message_has_payload() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-tool-no-dup.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-tool-no-dup","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"message","role":"tool","content":"{\"ok\":true}","tool_name":"call_abc","timestamp":"2026-01-29T14:30:56Z"}
{"type":"tool_call","name":"web_search","arguments":{"query":"test"},"result":{"ok":true},"success":true,"timestamp":"2026-01-29T14:30:57Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            let tool_messages: Vec<&Message> = session
                .tape
                .messages()
                .iter()
                .filter(|m| m.is_tool())
                .collect();
            assert_eq!(tool_messages.len(), 1);
            let responses = tool_messages[0].tool_responses();
            assert_eq!(responses[0].id, "call_abc");
        });
    }

    #[test]
    fn test_load_from_rollout_prefers_rich_message_payload_when_available() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-rich-message.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-rich-rollout".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "assistant".to_string(),
                    content: Some("final answer".to_string()),
                    tool_name: None,
                    message: Some(Message::Assistant {
                        parts: vec![
                            ContentPart::thinking("internal reasoning"),
                            ContentPart::text("final answer"),
                        ],
                        tool_requests: vec![crate::tape::ToolRequest {
                            id: "call_123".to_string(),
                            name: "web_search".to_string(),
                            arguments: serde_json::json!({"query":"alan"}),
                        }],
                    }),
                    timestamp: "2026-01-29T14:30:56Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(session.tape.messages().len(), 1);
            let message = &session.tape.messages()[0];
            assert_eq!(
                message.thinking_content().as_deref(),
                Some("internal reasoning")
            );
            assert_eq!(message.non_thinking_text_content(), "final answer");
            assert_eq!(message.tool_requests().len(), 1);
            assert_eq!(message.tool_requests()[0].name, "web_search");
        });
    }

    #[test]
    fn test_load_from_rollout_does_not_count_runtime_confirmation_control_messages_as_turns() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-control-turn-ordinal.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-control-turn-ordinal".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some("run task".to_string()),
                    tool_name: None,
                    message: Some(Message::User {
                        parts: vec![ContentPart::text("run task")],
                    }),
                    timestamp: "2026-01-29T14:30:53Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"tool_escalation_call-1\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\"}".to_string(),
                    ),
                    tool_name: None,
                    message: Some(Message::User {
                        parts: vec![ContentPart::structured(serde_json::json!({
                            "checkpoint_id": "tool_escalation_call-1",
                            "checkpoint_type": "tool_escalation",
                            "choice": "approve",
                            "__alan_internal_control": {
                                "kind": "tool_escalation_confirmation",
                                "version": 1
                            }
                        }))],
                    }),
                    timestamp: "2026-01-29T14:30:54Z".to_string(),
                }),
                RolloutItem::Checkpoint(CheckpointRecord {
                    checkpoint_id: "tool_escalation_call-1".to_string(),
                    checkpoint_type: "tool_escalation".to_string(),
                    summary: "approve side effect".to_string(),
                    choice: Some("approved".to_string()),
                    knowledge_root: None,
                    timestamp: "2026-01-29T14:30:54Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some("next task".to_string()),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:55Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"effect_replay_call-2\",\"checkpoint_type\":\"effect_replay_confirmation\",\"choice\":\"reject\",\"__alan_internal_control\":{\"kind\":\"effect_replay_confirmation\",\"version\":1,\"source\":\"runtime/submission_handlers\"}}"
                            .to_string(),
                    ),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:56Z".to_string(),
                }),
                RolloutItem::Checkpoint(CheckpointRecord {
                    checkpoint_id: "effect_replay_call-2".to_string(),
                    checkpoint_type: "effect_replay_confirmation".to_string(),
                    summary: "reject side effect".to_string(),
                    choice: Some("rejected".to_string()),
                    knowledge_root: None,
                    timestamp: "2026-01-29T14:30:56Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let mut session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.user_turn_ordinal(),
                2,
                "only non-control user messages should increment turn ordinal during recovery"
            );
            assert_eq!(session.user_turn_count(), 4);
            let removed = session.rollback_last_turns(2);
            assert_eq!(
                removed.removed_messages, 4,
                "legacy control messages should be normalized during recovery so rollback ignores them"
            );
            assert_eq!(removed.removed_turns, 2);
        });
    }

    #[test]
    fn test_load_from_rollout_counts_legacy_shaped_payload_without_checkpoint_match() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir
                .path()
                .join("rollout-legacy-control-without-checkpoint.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-legacy-control-without-checkpoint".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"tool_escalation_call-9\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\",\"__alan_internal_control\":{\"kind\":\"tool_escalation_confirmation\",\"version\":1}}"
                            .to_string(),
                    ),
                    tool_name: None,
                    message: Some(Message::User {
                        parts: vec![ContentPart::structured(serde_json::json!({
                            "checkpoint_id": "tool_escalation_call-9",
                            "checkpoint_type": "tool_escalation",
                            "choice": "approve",
                            "__alan_internal_control": {
                                "kind": "tool_escalation_confirmation",
                                "version": 1
                            }
                        }))],
                    }),
                    timestamp: "2026-01-29T14:30:53Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.user_turn_ordinal(),
                1,
                "legacy-shaped payloads without a matching checkpoint should count as normal user turns"
            );
            assert_eq!(session.user_turn_count(), 1);
        });
    }

    #[test]
    fn test_load_from_rollout_counts_strict_control_payload_without_checkpoint_match() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir
                .path()
                .join("rollout-strict-control-without-checkpoint.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-strict-control-without-checkpoint".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"tool_escalation_call-11\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\",\"__alan_internal_control\":{\"kind\":\"tool_escalation_confirmation\",\"version\":1,\"source\":\"runtime/submission_handlers\"}}"
                            .to_string(),
                    ),
                    tool_name: None,
                    message: Some(Message::User {
                        parts: vec![ContentPart::structured(serde_json::json!({
                            "checkpoint_id": "tool_escalation_call-11",
                            "checkpoint_type": "tool_escalation",
                            "choice": "approve",
                            "__alan_internal_control": {
                                "kind": "tool_escalation_confirmation",
                                "version": 1,
                                "source": "runtime/submission_handlers"
                            }
                        }))],
                    }),
                    timestamp: "2026-01-29T14:30:53Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.user_turn_ordinal(),
                1,
                "strict control payloads without a matching checkpoint should count as normal user turns"
            );
            assert_eq!(session.user_turn_count(), 1);
        });
    }

    #[test]
    fn test_load_from_rollout_counts_user_payloads_without_internal_control_marker() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-user-payload-turn-ordinal.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-user-payload-turn-ordinal".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"custom-id\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"approve\"}"
                            .to_string(),
                    ),
                    tool_name: None,
                    message: Some(Message::User {
                        parts: vec![ContentPart::structured(serde_json::json!({
                            "checkpoint_id": "custom-id",
                            "checkpoint_type": "tool_escalation",
                            "choice": "approve",
                        }))],
                    }),
                    timestamp: "2026-01-29T14:30:53Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some(
                        "{\"checkpoint_id\":\"manual-id\",\"checkpoint_type\":\"tool_escalation\",\"choice\":\"reject\"}"
                            .to_string(),
                    ),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:54Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.user_turn_ordinal(),
                2,
                "user payloads without internal control markers should count as turns"
            );
            assert_eq!(session.user_turn_count(), 2);
        });
    }

    #[test]
    fn test_load_from_rollout_preserves_turn_ordinal_across_repeated_recovery() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-turn-ordinal-recovery.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "test-turn-ordinal-recovery".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some("task one".to_string()),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:53Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "assistant".to_string(),
                    content: Some("ack".to_string()),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:54Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some("task two".to_string()),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:30:55Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";

            tokio::fs::write(&rollout_path, content).await.unwrap();

            let first = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();
            assert_eq!(first.user_turn_ordinal(), 2);
            drop(first);

            let second = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();
            assert_eq!(
                second.user_turn_ordinal(),
                2,
                "recovered history should preserve monotonic turn ordinal across repeated recovery"
            );
        });
    }

    #[test]
    fn test_load_from_rollout_preserves_turn_ordinal_floor_from_effect_keys_after_compaction() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-compaction-turn-floor.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-compaction-floor".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::Compacted(CompactedItem {
                    message: "Older turns compacted".to_string(),
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
                    timestamp: "2026-01-29T14:31:00Z".to_string(),
                }),
                RolloutItem::Message(MessageRecord {
                    role: "user".to_string(),
                    content: Some("latest visible turn".to_string()),
                    tool_name: None,
                    message: None,
                    timestamp: "2026-01-29T14:31:01Z".to_string(),
                }),
                RolloutItem::Effect(EffectRecord {
                    effect_id: "ef-compaction".to_string(),
                    run_id: "sess-compaction-floor".to_string(),
                    tool_call_id: "call-1".to_string(),
                    idempotency_key: "run:sess-compaction-floor:turn:7:fp-1".to_string(),
                    effect_type: "file".to_string(),
                    request_fingerprint: "fp-1".to_string(),
                    result_digest: Some("digest-1".to_string()),
                    result_payload: Some(serde_json::json!({"ok": true})),
                    status: EffectStatus::Applied,
                    applied_at: Some("2026-01-29T14:31:02Z".to_string()),
                    reason: None,
                    dedupe_hit: false,
                    timestamp: "2026-01-29T14:31:02Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.user_turn_ordinal(),
                7,
                "effect idempotency keys should preserve turn ordinal floor after compaction"
            );
            assert_eq!(session.user_turn_count(), 1);
        });
    }

    #[test]
    fn test_load_from_rollout_preserves_generic_event_records_across_recovery() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-events.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-events".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::Event(EventRecord {
                    event_type: "custom_event".to_string(),
                    payload: serde_json::json!({
                        "phase": "testing",
                        "value": 5
                    }),
                    timestamp: "2026-01-29T14:31:00Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();
            session.flush().await;

            let recovered_path = session
                .rollout_path()
                .expect("recovered session should have rollout path")
                .clone();
            let recovered_items = RolloutRecorder::load_history(&recovered_path)
                .await
                .unwrap();

            let event = recovered_items.into_iter().find_map(|item| match item {
                RolloutItem::Event(event) if event.event_type == "custom_event" => Some(event),
                _ => None,
            });

            let event = event.expect("expected recovered custom event");
            assert_eq!(event.payload["phase"], "testing");
            assert_eq!(event.payload["value"], 5);
            assert_eq!(event.timestamp, "2026-01-29T14:31:00Z");
        });
    }

    #[test]
    fn test_load_from_rollout_migrates_legacy_compaction_attempt_events() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir
                .path()
                .join("rollout-legacy-compaction-event.jsonl");

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-legacy-compaction-event".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::Event(EventRecord {
                    event_type: "compaction_attempt".to_string(),
                    payload: serde_json::json!({
                        "mode": "manual",
                        "trigger": "manual",
                        "reason": "explicit_request",
                        "focus": "preserve todos",
                        "retry_count": 2,
                        "result": "failure",
                        "error": "context window exceeded",
                        "failure_streak": 3,
                        "reference_context_revision": 7
                    }),
                    timestamp: "2026-01-29T14:31:00Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            let latest = session
                .latest_compaction_attempt()
                .expect("expected latest compaction attempt to be recovered");
            let latest_attempt_id = latest.attempt_id.clone();
            assert_eq!(latest.request.mode, CompactionMode::Manual);
            assert_eq!(latest.request.trigger, CompactionTrigger::Manual);
            assert_eq!(latest.request.reason, CompactionReason::ExplicitRequest);
            assert_eq!(latest.request.focus.as_deref(), Some("preserve todos"));
            assert_eq!(latest.result, CompactionResult::Failure);
            assert_eq!(latest.retry_count, 2);
            assert_eq!(
                latest.error_message.as_deref(),
                Some("context window exceeded")
            );
            assert_eq!(latest.failure_streak, Some(3));
            assert_eq!(latest.reference_context_revision_before, Some(7));

            session.flush().await;
            let recovered_path = session
                .rollout_path()
                .expect("recovered session should have rollout path")
                .clone();
            let recovered_items = RolloutRecorder::load_history(&recovered_path)
                .await
                .unwrap();

            let attempt = recovered_items.into_iter().find_map(|item| match item {
                RolloutItem::CompactionAttempt(attempt) => Some(attempt),
                _ => None,
            });

            let attempt = attempt.expect("expected recovered compaction attempt item");
            assert_eq!(attempt.attempt_id, latest_attempt_id);
            assert_eq!(attempt.result, CompactionResult::Failure);
            assert_eq!(attempt.retry_count, 2);
            assert_eq!(attempt.request.focus.as_deref(), Some("preserve todos"));
        });
    }

    #[test]
    fn test_load_from_rollout_repersists_memory_flush_attempt_records() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-memory-flush-attempt.jsonl");

            let attempt = MemoryFlushAttemptSnapshot {
                attempt_id: "flush-123".to_string(),
                compaction_mode: CompactionMode::AutoPreTurn,
                pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
                result: MemoryFlushResult::Success,
                skip_reason: None,
                source_messages: Some(7),
                output_path: Some(".alan/memory/daily/2026-03-03.md".to_string()),
                warning_message: None,
                error_message: None,
                timestamp: "2026-03-03T10:00:00Z".to_string(),
            };

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-memory-flush-attempt".to_string(),
                    started_at: "2026-03-03T09:59:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::MemoryFlushAttempt(attempt.clone()),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(session.latest_memory_flush_attempt(), Some(&attempt));

            session.flush().await;
            let recovered_path = session
                .rollout_path()
                .expect("recovered session should have rollout path")
                .clone();
            let recovered_items = RolloutRecorder::load_history(&recovered_path)
                .await
                .unwrap();

            let persisted = recovered_items.into_iter().find_map(|item| match item {
                RolloutItem::MemoryFlushAttempt(snapshot) => Some(snapshot),
                _ => None,
            });

            assert_eq!(persisted, Some(attempt));
        });
    }

    #[test]
    fn test_latest_compaction_attempt_from_legacy_rollout_is_stable_across_reads() {
        let items = vec![
            RolloutItem::SessionMeta(SessionMeta {
                session_id: "sess-legacy-compaction-event".to_string(),
                started_at: "2026-01-29T14:30:52Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gemini-2.0-flash".to_string(),
                reasoning_effort: None,
            }),
            RolloutItem::Event(EventRecord {
                event_type: "compaction_attempt".to_string(),
                payload: serde_json::json!({
                    "mode": "manual",
                    "trigger": "manual",
                    "reason": "explicit_request",
                    "focus": "preserve todos",
                    "retry_count": 2,
                    "result": "failure",
                    "error": "context window exceeded",
                    "failure_streak": 3,
                    "reference_context_revision": 7
                }),
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            }),
        ];

        let first = latest_compaction_attempt_from_rollout_items(&items)
            .expect("expected first legacy compaction attempt");
        let second = latest_compaction_attempt_from_rollout_items(&items)
            .expect("expected second legacy compaction attempt");

        assert_eq!(first.attempt_id, second.attempt_id);
        assert_eq!(first.result, CompactionResult::Failure);
        assert_eq!(first.retry_count, 2);
    }

    #[test]
    fn test_load_from_rollout_restores_latest_compaction_attempt_item_when_summary_is_persisted() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-compaction-attempt.jsonl");

            let attempt = CompactionAttemptSnapshot {
                attempt_id: "attempt-123".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::AutoPreTurn,
                    trigger: CompactionTrigger::Auto,
                    reason: CompactionReason::WindowPressure,
                    focus: None,
                },
                result: CompactionResult::Retry,
                pressure_level: None,
                memory_flush_attempt_id: None,
                input_messages: Some(18),
                output_messages: Some(5),
                input_prompt_tokens: Some(1500),
                output_prompt_tokens: Some(480),
                retry_count: 1,
                tape_mutated: true,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: Some(4),
                reference_context_revision_after: Some(4),
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            };

            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-compaction-attempt".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::CompactionAttempt(attempt.clone()),
                RolloutItem::Compacted(CompactedItem {
                    message: "Summary after retry".to_string(),
                    attempt_id: Some(attempt.attempt_id.clone()),
                    trigger: Some(CompactionTrigger::Auto),
                    reason: Some(CompactionReason::WindowPressure),
                    focus: None,
                    input_messages: Some(18),
                    output_messages: Some(5),
                    input_tokens: Some(1500),
                    output_tokens: Some(480),
                    duration_ms: Some(42),
                    retry_count: Some(1),
                    result: Some(CompactionResult::Retry),
                    reference_context_revision: Some(4),
                    timestamp: "2026-01-29T14:31:01Z".to_string(),
                }),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(session.latest_compaction_attempt(), Some(&attempt));
        });
    }

    #[test]
    fn test_latest_compaction_attempt_from_rollout_matches_summary_by_attempt_id() {
        let completed_attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-complete".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve tasks".to_string()),
            },
            result: CompactionResult::Success,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: Some(5),
            input_prompt_tokens: Some(1500),
            output_prompt_tokens: Some(480),
            retry_count: 0,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(4),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let incomplete_retry = CompactionAttemptSnapshot {
            attempt_id: "attempt-retry".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::AutoPreTurn,
                trigger: CompactionTrigger::Auto,
                reason: CompactionReason::WindowPressure,
                focus: None,
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(24),
            output_messages: Some(8),
            input_prompt_tokens: Some(1800),
            output_prompt_tokens: Some(500),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(5),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:32:00Z".to_string(),
        };
        let items = [
            RolloutItem::CompactionAttempt(completed_attempt.clone()),
            RolloutItem::CompactionAttempt(incomplete_retry),
            RolloutItem::Compacted(CompactedItem {
                message: "Summary after retry".to_string(),
                attempt_id: Some(completed_attempt.attempt_id.clone()),
                trigger: Some(CompactionTrigger::Manual),
                reason: Some(CompactionReason::ExplicitRequest),
                focus: Some("preserve tasks".to_string()),
                input_messages: Some(18),
                output_messages: Some(5),
                input_tokens: Some(1500),
                output_tokens: Some(480),
                duration_ms: Some(42),
                retry_count: Some(0),
                result: Some(CompactionResult::Success),
                reference_context_revision: Some(4),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
            }),
        ];

        assert_eq!(
            latest_compaction_attempt_from_rollout_items(&items),
            Some(completed_attempt)
        );
    }

    #[test]
    fn test_latest_compaction_attempt_from_rollout_does_not_let_legacy_summary_override_newer_attempt()
     {
        let completed_attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-complete".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve tasks".to_string()),
            },
            result: CompactionResult::Success,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: Some(5),
            input_prompt_tokens: Some(1500),
            output_prompt_tokens: Some(480),
            retry_count: 0,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(4),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let failure = CompactionAttemptSnapshot {
            attempt_id: "attempt-failure".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: None,
            },
            result: CompactionResult::Failure,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: None,
            input_prompt_tokens: Some(1400),
            output_prompt_tokens: None,
            retry_count: 1,
            tape_mutated: false,
            warning_message: Some("Preserving existing context".to_string()),
            error_message: Some("synthetic failure".to_string()),
            failure_streak: Some(1),
            reference_context_revision_before: Some(5),
            reference_context_revision_after: None,
            timestamp: "2026-01-29T14:32:00Z".to_string(),
        };
        let items = [
            RolloutItem::CompactionAttempt(completed_attempt),
            RolloutItem::CompactionAttempt(failure.clone()),
            RolloutItem::Compacted(CompactedItem {
                message: "Legacy summary".to_string(),
                attempt_id: None,
                trigger: Some(CompactionTrigger::Manual),
                reason: Some(CompactionReason::ExplicitRequest),
                focus: Some("preserve tasks".to_string()),
                input_messages: Some(18),
                output_messages: Some(5),
                input_tokens: Some(1500),
                output_tokens: Some(480),
                duration_ms: Some(42),
                retry_count: Some(0),
                result: Some(CompactionResult::Success),
                reference_context_revision: Some(4),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
            }),
        ];

        assert_eq!(
            latest_compaction_attempt_from_rollout_items(&items),
            Some(failure)
        );
    }

    #[test]
    fn test_latest_compaction_attempt_from_rollout_ignores_incomplete_tape_mutation() {
        let failure = CompactionAttemptSnapshot {
            attempt_id: "attempt-failure".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: None,
            },
            result: CompactionResult::Failure,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: None,
            input_prompt_tokens: Some(1400),
            output_prompt_tokens: None,
            retry_count: 0,
            tape_mutated: false,
            warning_message: Some("Preserving existing context".to_string()),
            error_message: Some("synthetic failure".to_string()),
            failure_streak: Some(1),
            reference_context_revision_before: Some(4),
            reference_context_revision_after: None,
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let incomplete_retry = CompactionAttemptSnapshot {
            attempt_id: "attempt-retry".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::AutoPreTurn,
                trigger: CompactionTrigger::Auto,
                reason: CompactionReason::WindowPressure,
                focus: None,
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(24),
            output_messages: Some(8),
            input_prompt_tokens: Some(1800),
            output_prompt_tokens: Some(500),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(5),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:32:00Z".to_string(),
        };
        let items = [
            RolloutItem::CompactionAttempt(failure.clone()),
            RolloutItem::CompactionAttempt(incomplete_retry),
        ];

        assert_eq!(
            latest_compaction_attempt_from_rollout_items(&items),
            Some(failure)
        );
    }

    #[test]
    fn test_latest_compaction_attempt_from_rollout_does_not_let_linked_summary_override_newer_attempt()
     {
        let completed_attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-complete".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve tasks".to_string()),
            },
            result: CompactionResult::Success,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: Some(5),
            input_prompt_tokens: Some(1500),
            output_prompt_tokens: Some(480),
            retry_count: 0,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(4),
            reference_context_revision_after: Some(5),
            timestamp: "2026-01-29T14:31:00Z".to_string(),
        };
        let failure = CompactionAttemptSnapshot {
            attempt_id: "attempt-failure".to_string(),
            submission_id: None,
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: None,
            },
            result: CompactionResult::Failure,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(18),
            output_messages: None,
            input_prompt_tokens: Some(1400),
            output_prompt_tokens: None,
            retry_count: 1,
            tape_mutated: false,
            warning_message: Some("Preserving existing context".to_string()),
            error_message: Some("synthetic failure".to_string()),
            failure_streak: Some(1),
            reference_context_revision_before: Some(5),
            reference_context_revision_after: None,
            timestamp: "2026-01-29T14:32:00Z".to_string(),
        };
        let items = [
            RolloutItem::CompactionAttempt(completed_attempt),
            RolloutItem::CompactionAttempt(failure.clone()),
            RolloutItem::Compacted(CompactedItem {
                message: "Summary after retry".to_string(),
                attempt_id: Some("attempt-complete".to_string()),
                trigger: Some(CompactionTrigger::Manual),
                reason: Some(CompactionReason::ExplicitRequest),
                focus: Some("preserve tasks".to_string()),
                input_messages: Some(18),
                output_messages: Some(5),
                input_tokens: Some(1500),
                output_tokens: Some(480),
                duration_ms: Some(42),
                retry_count: Some(0),
                result: Some(CompactionResult::Success),
                reference_context_revision: Some(4),
                timestamp: "2026-01-29T14:31:01Z".to_string(),
            }),
        ];

        assert_eq!(
            latest_compaction_attempt_from_rollout_items(&items),
            Some(failure)
        );
    }

    #[test]
    fn test_load_from_rollout_repersists_legacy_summary_with_stable_attempt_link() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-legacy-summary.jsonl");

            let completed_attempt = CompactionAttemptSnapshot {
                attempt_id: "attempt-complete".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::Manual,
                    trigger: CompactionTrigger::Manual,
                    reason: CompactionReason::ExplicitRequest,
                    focus: Some("preserve tasks".to_string()),
                },
                result: CompactionResult::Success,
                pressure_level: None,
                memory_flush_attempt_id: None,
                input_messages: Some(18),
                output_messages: Some(5),
                input_prompt_tokens: Some(1500),
                output_prompt_tokens: Some(480),
                retry_count: 0,
                tape_mutated: true,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: Some(4),
                reference_context_revision_after: Some(5),
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            };
            let incomplete_retry = CompactionAttemptSnapshot {
                attempt_id: "attempt-retry".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::AutoPreTurn,
                    trigger: CompactionTrigger::Auto,
                    reason: CompactionReason::WindowPressure,
                    focus: None,
                },
                result: CompactionResult::Retry,
                pressure_level: None,
                memory_flush_attempt_id: None,
                input_messages: Some(24),
                output_messages: Some(8),
                input_prompt_tokens: Some(1800),
                output_prompt_tokens: Some(500),
                retry_count: 1,
                tape_mutated: true,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: Some(5),
                reference_context_revision_after: Some(5),
                timestamp: "2026-01-29T14:32:00Z".to_string(),
            };
            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-legacy-summary".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::CompactionAttempt(completed_attempt.clone()),
                RolloutItem::Compacted(CompactedItem {
                    message: "Legacy summary".to_string(),
                    attempt_id: None,
                    trigger: Some(CompactionTrigger::Manual),
                    reason: Some(CompactionReason::ExplicitRequest),
                    focus: Some("preserve tasks".to_string()),
                    input_messages: Some(18),
                    output_messages: Some(5),
                    input_tokens: Some(1500),
                    output_tokens: Some(480),
                    duration_ms: Some(42),
                    retry_count: Some(0),
                    result: Some(CompactionResult::Success),
                    reference_context_revision: Some(4),
                    timestamp: "2026-01-29T14:31:01Z".to_string(),
                }),
                RolloutItem::CompactionAttempt(incomplete_retry),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(
                session.latest_compaction_attempt(),
                Some(&completed_attempt)
            );

            session.flush().await;
            let recovered_path = session
                .rollout_path()
                .expect("recovered session should have rollout path")
                .clone();
            let recovered_items = RolloutRecorder::load_history(&recovered_path)
                .await
                .unwrap();
            let persisted_compacted = recovered_items.iter().find_map(|item| match item {
                RolloutItem::Compacted(compacted) => Some(compacted),
                _ => None,
            });

            assert_eq!(
                persisted_compacted.and_then(|item| item.attempt_id.as_deref()),
                Some("attempt-complete")
            );

            let reloaded = Session::load_from_rollout_in_dir(
                &recovered_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();
            assert_eq!(
                reloaded.latest_compaction_attempt(),
                Some(&completed_attempt)
            );
        });
    }

    #[test]
    fn test_load_from_rollout_does_not_let_repersisted_linked_summary_override_newer_failure() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-linked-summary.jsonl");

            let completed_attempt = CompactionAttemptSnapshot {
                attempt_id: "attempt-complete".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::Manual,
                    trigger: CompactionTrigger::Manual,
                    reason: CompactionReason::ExplicitRequest,
                    focus: Some("preserve tasks".to_string()),
                },
                result: CompactionResult::Success,
                pressure_level: None,
                memory_flush_attempt_id: None,
                input_messages: Some(18),
                output_messages: Some(5),
                input_prompt_tokens: Some(1500),
                output_prompt_tokens: Some(480),
                retry_count: 0,
                tape_mutated: true,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: Some(4),
                reference_context_revision_after: Some(5),
                timestamp: "2026-01-29T14:31:00Z".to_string(),
            };
            let failure = CompactionAttemptSnapshot {
                attempt_id: "attempt-failure".to_string(),
                submission_id: None,
                request: CompactionRequestMetadata {
                    mode: CompactionMode::Manual,
                    trigger: CompactionTrigger::Manual,
                    reason: CompactionReason::ExplicitRequest,
                    focus: None,
                },
                result: CompactionResult::Failure,
                pressure_level: None,
                memory_flush_attempt_id: None,
                input_messages: Some(18),
                output_messages: None,
                input_prompt_tokens: Some(1400),
                output_prompt_tokens: None,
                retry_count: 1,
                tape_mutated: false,
                warning_message: Some("Preserving existing context".to_string()),
                error_message: Some("synthetic failure".to_string()),
                failure_streak: Some(1),
                reference_context_revision_before: Some(5),
                reference_context_revision_after: None,
                timestamp: "2026-01-29T14:32:00Z".to_string(),
            };
            let items = [
                RolloutItem::SessionMeta(SessionMeta {
                    session_id: "sess-linked-summary".to_string(),
                    started_at: "2026-01-29T14:30:52Z".to_string(),
                    cwd: "/tmp".to_string(),
                    model: "gemini-2.0-flash".to_string(),
                    reasoning_effort: None,
                }),
                RolloutItem::CompactionAttempt(completed_attempt.clone()),
                RolloutItem::Compacted(CompactedItem {
                    message: "Summary after retry".to_string(),
                    attempt_id: Some(completed_attempt.attempt_id.clone()),
                    trigger: Some(CompactionTrigger::Manual),
                    reason: Some(CompactionReason::ExplicitRequest),
                    focus: Some("preserve tasks".to_string()),
                    input_messages: Some(18),
                    output_messages: Some(5),
                    input_tokens: Some(1500),
                    output_tokens: Some(480),
                    duration_ms: Some(42),
                    retry_count: Some(0),
                    result: Some(CompactionResult::Success),
                    reference_context_revision: Some(4),
                    timestamp: "2026-01-29T14:31:01Z".to_string(),
                }),
                RolloutItem::CompactionAttempt(failure.clone()),
            ];

            let content = items
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .join("\n")
                + "\n";
            tokio::fs::write(&rollout_path, content).await.unwrap();

            let session = Session::load_from_rollout_in_dir(
                &rollout_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(session.latest_compaction_attempt(), Some(&failure));

            session.flush().await;
            let recovered_path = session
                .rollout_path()
                .expect("recovered session should have rollout path")
                .clone();
            let reloaded = Session::load_from_rollout_in_dir(
                &recovered_path,
                "gemini-2.0-flash",
                temp_dir.path(),
            )
            .await
            .unwrap();

            assert_eq!(reloaded.latest_compaction_attempt(), Some(&failure));
        });
    }

    #[test]
    fn test_empty_message_content() {
        let mut session = Session::new();
        session.add_user_message("");

        let messages = session.tape.messages();
        assert_eq!(messages[0].text_content(), "");
    }

    #[test]
    fn test_unicode_message_content() {
        let mut session = Session::new();
        session.add_user_message("你好，世界！🌍");

        let messages = session.tape.messages();
        assert_eq!(messages[0].text_content(), "你好，世界！🌍");
    }

    #[test]
    fn test_record_tool_call() {
        let session = Session::new();
        let args = serde_json::json!({"query": "test"});
        let result = serde_json::json!({"status": "ok"});

        // Should not panic without recorder
        session.record_tool_call("search_tool", args.clone(), result.clone(), true);
    }

    #[test]
    fn test_record_effect_updates_lookup_index() {
        let mut session = Session::new();
        let effect = EffectRecord {
            effect_id: "ef-1".to_string(),
            run_id: session.id.clone(),
            tool_call_id: "call-1".to_string(),
            idempotency_key: "idem-1".to_string(),
            effect_type: "file".to_string(),
            request_fingerprint: "fp-1".to_string(),
            result_digest: None,
            result_payload: None,
            status: EffectStatus::Unknown,
            applied_at: None,
            reason: Some("pending".to_string()),
            dedupe_hit: false,
            timestamp: "2026-03-03T10:00:00Z".to_string(),
        };

        session.record_effect(effect);

        let restored = session.effect_by_idempotency_key("idem-1").unwrap();
        assert_eq!(restored.effect_id, "ef-1");
        assert_eq!(restored.status, EffectStatus::Unknown);
    }

    #[test]
    fn test_load_from_rollout_restores_latest_effect_record() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-effect-index.jsonl");

            let content = r#"{"type":"session_meta","session_id":"sess-effect","started_at":"2026-03-03T10:00:00Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"effect","effect_id":"ef-1","run_id":"sess-effect","tool_call_id":"call-1","idempotency_key":"idem-1","effect_type":"file","request_fingerprint":"fp-1","status":"unknown","dedupe_hit":false,"timestamp":"2026-03-03T10:00:01Z"}
{"type":"effect","effect_id":"ef-1","run_id":"sess-effect","tool_call_id":"call-1","idempotency_key":"idem-1","effect_type":"file","request_fingerprint":"fp-1","result_digest":"digest-1","status":"applied","applied_at":"2026-03-03T10:00:02Z","dedupe_hit":false,"timestamp":"2026-03-03T10:00:02Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session =
                Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                    .await
                    .unwrap();

            let effect = session.effect_by_idempotency_key("idem-1").unwrap();
            assert_eq!(effect.status, EffectStatus::Applied);
            assert_eq!(effect.result_digest.as_deref(), Some("digest-1"));

            let persisted_items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
            let persisted_effects: Vec<_> = persisted_items
                .into_iter()
                .filter_map(|item| match item {
                    RolloutItem::Effect(effect) => Some(effect),
                    _ => None,
                })
                .collect();
            assert_eq!(
                persisted_effects.len(),
                2,
                "recovered effects should be re-persisted to protect future recoveries"
            );
            assert!(matches!(
                persisted_effects.last(),
                Some(effect) if effect.status == EffectStatus::Applied
            ));
        });
    }

    #[test]
    fn test_record_checkpoint() {
        let session = Session::new();

        // Should not panic without recorder
        session.record_checkpoint(
            "cp-123",
            "supplier_list",
            "Test checkpoint",
            Some("approve"),
        );
    }

    #[tokio::test]
    async fn test_flush() {
        let session = Session::new();
        // Should not panic without recorder
        session.flush().await;
    }

    #[test]
    fn test_add_user_message_with_tool_name() {
        let mut session = Session::new();
        session.add_user_message("Hello");
        let messages = session.tape.messages();
        assert!(messages[0].is_user());
    }

    #[test]
    fn test_record_event() {
        let session = Session::new();
        // Should not panic without recorder
        session.record_event("test_event", serde_json::json!({"key": "value"}));
    }

    #[test]
    fn test_record_summary() {
        let session = Session::new();
        // Should not panic without recorder
        session.record_summary("Test summary");
    }

    #[test]
    fn test_latest_memory_flush_attempt_from_rollout_items_returns_latest_attempt() {
        let first = MemoryFlushAttemptSnapshot {
            attempt_id: "flush-1".to_string(),
            compaction_mode: CompactionMode::AutoPreTurn,
            pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
            result: MemoryFlushResult::Skipped,
            skip_reason: Some(MemoryFlushSkipReason::ReadOnlyMemoryDir),
            source_messages: Some(4),
            output_path: None,
            warning_message: Some("memory dir is read-only".to_string()),
            error_message: None,
            timestamp: "2026-03-03T10:00:00Z".to_string(),
        };
        let second = MemoryFlushAttemptSnapshot {
            attempt_id: "flush-2".to_string(),
            compaction_mode: CompactionMode::AutoPreTurn,
            pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
            result: MemoryFlushResult::Success,
            skip_reason: None,
            source_messages: Some(8),
            output_path: Some(".alan/memory/daily/2026-03-03.md".to_string()),
            warning_message: None,
            error_message: None,
            timestamp: "2026-03-03T10:05:00Z".to_string(),
        };
        let items = [
            RolloutItem::MemoryFlushAttempt(first),
            RolloutItem::MemoryFlushAttempt(second.clone()),
        ];

        assert_eq!(
            latest_memory_flush_attempt_from_rollout_items(&items),
            Some(second)
        );
    }

    #[tokio::test]
    async fn test_persist_compaction_attempt_updates_latest_and_rollout() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-123".to_string(),
            submission_id: Some("sub-456".to_string()),
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve todos".to_string()),
            },
            result: CompactionResult::Success,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(10),
            output_messages: Some(3),
            input_prompt_tokens: Some(800),
            output_prompt_tokens: Some(250),
            retry_count: 0,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(2),
            reference_context_revision_after: Some(2),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
        };

        session
            .persist_compaction_observation(attempt.clone(), None)
            .await
            .unwrap();
        assert_eq!(session.latest_compaction_attempt(), Some(&attempt));

        let rollout_path = session.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let persisted = items.into_iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(snapshot) => Some(snapshot),
            _ => None,
        });

        assert_eq!(persisted, Some(attempt));
    }

    #[tokio::test]
    async fn test_persist_memory_flush_attempt_updates_latest_and_rollout() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        let attempt = MemoryFlushAttemptSnapshot {
            attempt_id: "flush-123".to_string(),
            compaction_mode: CompactionMode::AutoPreTurn,
            pressure_level: alan_agent_protocol::CompactionPressureLevel::Soft,
            result: MemoryFlushResult::Success,
            skip_reason: None,
            source_messages: Some(7),
            output_path: Some(".alan/memory/daily/2026-03-03.md".to_string()),
            warning_message: None,
            error_message: None,
            timestamp: "2026-03-03T10:00:00Z".to_string(),
        };

        session
            .persist_memory_flush_attempt(attempt.clone())
            .await
            .unwrap();
        assert_eq!(session.latest_memory_flush_attempt(), Some(&attempt));

        let rollout_path = session.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let persisted = items.into_iter().find_map(|item| match item {
            RolloutItem::MemoryFlushAttempt(snapshot) => Some(snapshot),
            _ => None,
        });

        assert_eq!(persisted, Some(attempt));
    }

    #[tokio::test]
    async fn test_persist_compaction_observation_batches_attempt_and_summary() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        let attempt = CompactionAttemptSnapshot {
            attempt_id: "attempt-batched".to_string(),
            submission_id: Some("sub-batched".to_string()),
            request: CompactionRequestMetadata {
                mode: CompactionMode::Manual,
                trigger: CompactionTrigger::Manual,
                reason: CompactionReason::ExplicitRequest,
                focus: Some("preserve blockers".to_string()),
            },
            result: CompactionResult::Retry,
            pressure_level: None,
            memory_flush_attempt_id: None,
            input_messages: Some(10),
            output_messages: Some(3),
            input_prompt_tokens: Some(800),
            output_prompt_tokens: Some(250),
            retry_count: 1,
            tape_mutated: true,
            warning_message: None,
            error_message: None,
            failure_streak: None,
            reference_context_revision_before: Some(2),
            reference_context_revision_after: Some(2),
            timestamp: "2026-03-03T10:00:00Z".to_string(),
        };
        let compacted = CompactedItem {
            message: "Summary after retry".to_string(),
            attempt_id: Some(attempt.attempt_id.clone()),
            trigger: Some(CompactionTrigger::Manual),
            reason: Some(CompactionReason::ExplicitRequest),
            focus: Some("preserve blockers".to_string()),
            input_messages: Some(10),
            output_messages: Some(3),
            input_tokens: Some(800),
            output_tokens: Some(250),
            duration_ms: Some(35),
            retry_count: Some(1),
            result: Some(CompactionResult::Retry),
            reference_context_revision: Some(2),
            timestamp: "2026-03-03T10:00:01Z".to_string(),
        };

        session
            .persist_compaction_observation(attempt.clone(), Some(compacted.clone()))
            .await
            .unwrap();

        let rollout_path = session.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let persisted_attempt = items.iter().find_map(|item| match item {
            RolloutItem::CompactionAttempt(snapshot) => Some(snapshot),
            _ => None,
        });
        let persisted_compacted = items.iter().find_map(|item| match item {
            RolloutItem::Compacted(compacted) => Some(compacted),
            _ => None,
        });

        assert_eq!(session.latest_compaction_attempt(), Some(&attempt));
        assert_eq!(persisted_attempt, Some(&attempt));
        assert_eq!(
            persisted_compacted.map(|item| item.attempt_id.as_deref()),
            Some(Some("attempt-batched"))
        );
        assert_eq!(
            persisted_compacted.map(|item| item.message.as_str()),
            Some(compacted.message.as_str())
        );
    }

    #[test]
    fn test_record_turn_context() {
        let session = Session::new();
        let context_items = vec![ContextItem {
            id: "ctx-1".to_string(),
            kind: "customer".to_string(),
            title: "Customer Profile".to_string(),
            content: "Test content".to_string(),
            fingerprint: "abc123".to_string(),
        }];
        // Should not panic without recorder
        session.record_turn_context(
            "gemini-2.0-flash",
            Some(alan_agent_protocol::ReasoningEffort::Low),
            "System prompt",
            &context_items,
            &["tool1".to_string(), "tool2".to_string()],
            true,
            &["skill1".to_string()],
        );
    }

    #[test]
    fn test_record_turn_context_if_changed_dedupes_identical_snapshots() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let mut session =
                Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
                    .await
                    .unwrap();

            let context_items = vec![ContextItem {
                id: "ctx-1".to_string(),
                kind: "customer".to_string(),
                title: "Customer Profile".to_string(),
                content: "Test content".to_string(),
                fingerprint: "abc123".to_string(),
            }];

            let unchanged = ContextItemsDelta::default();
            assert!(session.record_turn_context_if_changed(
                "gemini-2.0-flash",
                None,
                "System prompt",
                &context_items,
                &["tool1".to_string()],
                true,
                &["skill1".to_string()],
                &unchanged,
            ));

            assert!(!session.record_turn_context_if_changed(
                "gemini-2.0-flash",
                None,
                "System prompt",
                &context_items,
                &["tool1".to_string()],
                true,
                &["skill1".to_string()],
                &unchanged,
            ));

            // A tool list change should still record even when reference context is unchanged.
            assert!(session.record_turn_context_if_changed(
                "gemini-2.0-flash",
                None,
                "System prompt",
                &context_items,
                &["tool1".to_string(), "tool2".to_string()],
                true,
                &["skill1".to_string()],
                &unchanged,
            ));

            session.flush().await;

            let rollout_path = session.rollout_path().unwrap().clone();
            let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
            let turn_context_count = items
                .into_iter()
                .filter(|item| matches!(item, RolloutItem::TurnContext(_)))
                .count();
            assert_eq!(turn_context_count, 2);
        });
    }

    #[test]
    fn test_record_turn_context_if_changed_records_reasoning_effort_changes() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let mut session =
                Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
                    .await
                    .unwrap();

            let context_items = vec![ContextItem {
                id: "ctx-1".to_string(),
                kind: "customer".to_string(),
                title: "Customer Profile".to_string(),
                content: "Test content".to_string(),
                fingerprint: "abc123".to_string(),
            }];
            let unchanged = ContextItemsDelta::default();

            assert!(session.record_turn_context_if_changed(
                "gemini-2.0-flash",
                Some(alan_agent_protocol::ReasoningEffort::Low),
                "System prompt",
                &context_items,
                &["tool1".to_string()],
                true,
                &["skill1".to_string()],
                &unchanged,
            ));
            assert!(session.record_turn_context_if_changed(
                "gemini-2.0-flash",
                Some(alan_agent_protocol::ReasoningEffort::High),
                "System prompt",
                &context_items,
                &["tool1".to_string()],
                true,
                &["skill1".to_string()],
                &unchanged,
            ));

            session.flush().await;

            let rollout_path = session.rollout_path().unwrap().clone();
            let efforts = RolloutRecorder::load_history(&rollout_path)
                .await
                .unwrap()
                .into_iter()
                .filter_map(|item| match item {
                    RolloutItem::TurnContext(ctx) => Some(ctx.reasoning_effort),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                efforts,
                vec![
                    Some(alan_agent_protocol::ReasoningEffort::Low),
                    Some(alan_agent_protocol::ReasoningEffort::High)
                ]
            );
        });
    }

    #[test]
    fn test_add_tool_message_persists_redacted_tool_payload_with_tool_call_id() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use tokio::time::{Duration, Instant, sleep};

            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let mut session =
                Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
                    .await
                    .unwrap();

            session.add_tool_message(
                "call_789",
                "web_search",
                serde_json::json!({
                    "ok": true,
                    "headers": {
                        "set-cookie": "session=secret-cookie",
                        "content-type": "application/json"
                    }
                }),
            );

            let rollout_path = session.rollout_path().unwrap().clone();
            let start = Instant::now();
            let mut found = false;
            while start.elapsed() < Duration::from_secs(1) {
                if let Ok(content) = tokio::fs::read_to_string(&rollout_path).await
                    && content.contains("\"role\":\"tool\"")
                    && content.contains("\"tool_name\":\"call_789\"")
                    && content.contains("\\\"set-cookie\\\":\\\"[REDACTED]\\\"")
                    && !content.contains("secret-cookie")
                {
                    found = true;
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
            assert!(
                found,
                "expected tool message with payload and tool_call_id to be persisted"
            );
        });
    }

    #[tokio::test]
    async fn test_flush_waits_for_queued_rollout_writes() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();

        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.record_event("evt", serde_json::json!({"ok": true}));
        session.flush().await;

        let rollout_path = session.rollout_path().unwrap().clone();
        let content = tokio::fs::read_to_string(&rollout_path).await.unwrap();
        let user_pos = content.find("\"content\":\"u1\"").unwrap();
        let assistant_pos = content.find("\"content\":\"a1\"").unwrap();
        let event_pos = content.find("\"event_type\":\"evt\"").unwrap();
        assert!(user_pos < assistant_pos);
        assert!(assistant_pos < event_pos);
    }

    #[tokio::test]
    async fn test_rollback_records_non_durable_audit_marker() {
        let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();

        let mut session = Session::new_with_recorder_in_dir("gemini-2.0-flash", temp_dir.path())
            .await
            .unwrap();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.add_user_message("u2");
        session.add_assistant_message("a2", None);

        let removed = session.rollback_last_turns(1);
        assert_eq!(removed.removed_turns, 1);
        assert_eq!(removed.removed_messages, 2);
        session.flush().await;

        let rollout_path = session.rollout_path().unwrap().clone();
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        let rollback_event = items.into_iter().find_map(|item| match item {
            RolloutItem::Event(event) if event.event_type == "session_rollback" => Some(event),
            _ => None,
        });

        let event = rollback_event.expect("expected session_rollback event");
        assert_eq!(event.payload["requested_turns"], serde_json::json!(1));
        assert_eq!(event.payload["removed_turns"], serde_json::json!(1));
        assert_eq!(event.payload["removed_messages"], serde_json::json!(2));
        assert_eq!(event.payload["durable"], serde_json::json!(false));
        assert_eq!(event.payload["scope"], serde_json::json!("in_memory"));
        assert_eq!(
            event.payload["warning"],
            serde_json::json!(ROLLBACK_NON_DURABLE_WARNING)
        );
    }

    // Tests for payload truncation

    #[test]
    fn test_truncate_payload_small_payload_unchanged() {
        let payload = serde_json::json!({
            "success": true,
            "url": "https://example.com",
            "title": "Example"
        });
        let result = truncate_payload(payload.clone(), 1000);
        assert_eq!(result, payload);
    }

    #[test]
    fn test_truncate_text() {
        let text = "This is a long text that needs to be truncated";
        let truncated = truncate_text(text, 20);
        assert!(truncated.contains("...[truncated]"));
        assert!(truncated.len() < text.len() + 15); // +15 for "...[truncated]"
    }

    #[test]
    fn test_truncate_text_short() {
        let text = "Short";
        let truncated = truncate_text(text, 100);
        assert_eq!(truncated, text);
    }

    #[test]
    fn test_truncate_payload_large_content() {
        let large_content = "a".repeat(10000);
        let payload = serde_json::json!({
            "success": true,
            "url": "https://example.com",
            "content": large_content
        });
        let result = truncate_payload(payload, 5000);
        let result_str = result.to_string();
        assert!(result_str.len() < 6000); // Should be significantly reduced
        assert!(result_str.contains("...[truncated]"));
    }

    #[test]
    fn test_truncate_payload_preserves_critical_fields() {
        let large_content = "x".repeat(5000);
        let payload = serde_json::json!({
            "success": false,
            "error": "Some error",
            "url": "https://example.com",
            "title": "Test Title",
            "content": large_content
        });
        let result = truncate_payload(payload, 2000);
        // Critical fields should be preserved
        assert_eq!(result["success"], false);
        assert_eq!(result["error"], "Some error");
        assert_eq!(result["url"], "https://example.com");
        assert_eq!(result["title"], "Test Title");
    }

    #[test]
    fn test_add_tool_message_preserves_large_payload_on_tape() {
        let mut session = Session::new();
        let large_content = "x".repeat(50000);
        let payload = serde_json::json!({
            "success": true,
            "content": large_content
        });

        session.add_tool_message("call_456", "test_tool", payload);

        let messages = session.tape.messages();
        assert_eq!(messages.len(), 1);
        let responses = messages[0].tool_responses();
        assert_eq!(responses.len(), 1);

        // Tape should keep the full payload; projection handles truncation.
        let response_str = serde_json::to_string(&responses[0].content).unwrap();
        assert!(
            response_str.len() > 50000,
            "Payload should stay full on tape, got {} chars",
            response_str.len()
        );
    }

    // Additional truncation tests for better coverage

    #[test]
    fn test_truncate_payload_array() {
        let payload = serde_json::json!([
            {"id": 1, "content": "First item"},
            {"id": 2, "content": "Second item"},
            {"id": 3, "content": "Third item"}
        ]);
        // Small max_size to trigger truncation
        let result = truncate_payload(payload.clone(), 100);
        // Result should be an array
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        // Should contain items but may have truncation note
        assert!(!arr.is_empty());
    }

    #[test]
    fn test_truncate_payload_nested_object() {
        let payload = serde_json::json!({
            "level1": {
                "level2": {
                    "data": "x".repeat(5000)
                }
            }
        });
        let result = truncate_payload(payload, 1000);
        // Should preserve structure but truncate content
        assert!(result.get("level1").is_some());
    }

    #[test]
    fn test_truncate_payload_string_only() {
        let payload = serde_json::Value::String("a".repeat(5000));
        let result = truncate_payload(payload, 1000);
        // String should be truncated
        let result_str = result.as_str().unwrap();
        assert!(result_str.len() < 5000);
        assert!(result_str.contains("...[truncated]"));
    }

    #[test]
    fn test_truncate_payload_aggregated_content() {
        let large_content = "b".repeat(5000);
        let payload = serde_json::json!({
            "success": true,
            "aggregated_content": large_content
        });
        let result = truncate_payload(payload, 2000);
        // aggregated_content should be truncated
        let content = result["aggregated_content"].as_str().unwrap();
        assert!(content.len() < 5000);
        assert!(content.contains("...[truncated]"));
    }

    #[test]
    fn test_truncate_payload_array_truncation_note() {
        // Create a large array that will trigger truncation
        let items: Vec<serde_json::Value> = (0..100)
            .map(|i| serde_json::json!({"id": i, "data": "x".repeat(100)}))
            .collect();
        let payload = serde_json::Value::Array(items);
        let result = truncate_payload(payload, 500);
        // Should contain truncation note in one of the items
        let arr = result.as_array().unwrap();
        let has_note = arr.iter().any(|item| {
            item.get("_note")
                .and_then(|n| n.as_str())
                .map(|s| s.contains("omitted"))
                .unwrap_or(false)
        });
        assert!(has_note, "Should have truncation note in array items");
    }

    #[test]
    fn test_truncate_payload_object_truncated_field() {
        // Create an object with many large fields
        let mut map = serde_json::Map::new();
        for i in 0..50 {
            map.insert(
                format!("field{}", i),
                serde_json::Value::String("y".repeat(200)),
            );
        }
        let payload = serde_json::Value::Object(map);
        let result = truncate_payload(payload, 1000);
        // Should have _truncated field
        assert!(
            result.get("_truncated").is_some(),
            "Should have _truncated field for omitted fields"
        );
    }

    #[test]
    fn test_truncate_payload_mixed_types() {
        let payload = serde_json::json!({
            "string": "test",
            "number": 42,
            "bool": true,
            "null": null,
            "array": [1, 2, 3],
            "nested": {"key": "value"}
        });
        let result = truncate_payload(payload.clone(), 1000);
        // All types should be preserved
        assert_eq!(result["string"], "test");
        assert_eq!(result["number"], 42);
        assert_eq!(result["bool"], true);
        assert!(result["null"].is_null());
        assert!(result["array"].is_array());
        assert!(result["nested"].is_object());
    }

    #[test]
    fn test_load_from_rollout_with_turn_context() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-context.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-789","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"turn_context","model":"gemini-2.0-flash","system_prompt":"You are a helpful assistant","context_items":[{"id":"ctx-1","kind":"customer","title":"Profile","content":"Test content","fingerprint":"fp123"}],"tools":["search","analyze"],"memory_enabled":true,"active_skills":["onboarding"],"timestamp":"2026-01-29T14:30:54Z"}
{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            assert!(session.has_active_task);
            // Context items should be restored
            let ctx_items = session.tape.context_items();
            assert!(!ctx_items.is_empty());
            assert_eq!(ctx_items[0].id, "ctx-1");
            assert_eq!(ctx_items[0].kind, "customer");
        });
    }

    #[test]
    fn test_load_from_rollout_system_and_context_roles() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-roles.jsonl");

            let content = r#"{"type":"session_meta","session_id":"test-roles","started_at":"2026-01-29T14:30:52Z","cwd":"/tmp","model":"gemini-2.0-flash"}
{"type":"message","role":"system","content":"System prompt","tool_name":null,"timestamp":"2026-01-29T14:30:53Z"}
{"type":"message","role":"context","content":"Context info","tool_name":null,"timestamp":"2026-01-29T14:30:54Z"}
{"type":"message","role":"assistant","content":"Assistant reply","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
{"type":"message","role":"unknown_role","content":"Fallback test","tool_name":null,"timestamp":"2026-01-29T14:30:56Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            let messages = session.tape.messages();
            // Context messages should be skipped
            assert_eq!(messages.len(), 3); // system, assistant, unknown (falls back to user)
            assert_eq!(messages[0].role(), MessageRole::System);
            assert_eq!(messages[1].role(), MessageRole::Assistant);
            // Unknown role falls back to User
            assert_eq!(messages[2].role(), MessageRole::User);
            assert_eq!(messages[2].text_content(), "Fallback test");
        });
    }

    #[test]
    fn test_load_from_rollout_without_session_meta() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new_in(std::env::temp_dir()).unwrap();
            let rollout_path = temp_dir.path().join("rollout-no-meta.jsonl");

            // No session_meta, should generate new UUID
            let content = r#"{"type":"message","role":"user","content":"Hello","tool_name":null,"timestamp":"2026-01-29T14:30:55Z"}
"#;

            tokio::fs::write(&rollout_path, content).await.unwrap();
            let session = Session::load_from_rollout_in_dir(&rollout_path, "gemini-2.0-flash", temp_dir.path())
                .await
                .unwrap();

            // Should have generated a new UUID
            assert!(!session.id.is_empty());
            assert!(session.has_active_task);
        });
    }

    #[test]
    fn test_rollback_last_turns_removes_latest_turn_messages() {
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.add_tool_message("call1", "web_search", serde_json::json!({"ok": true}));
        session.add_user_message("u2");
        session.add_assistant_message("a2", None);

        let removed = session.rollback_last_turns(1);

        assert_eq!(removed.removed_turns, 1);
        assert_eq!(removed.removed_messages, 2);
        let messages = session.tape.messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text_content(), "u1");
        assert_eq!(messages[1].text_content(), "a1");
        assert!(messages[2].is_tool());
    }

    #[test]
    fn test_rollback_last_turns_clears_all_when_request_exceeds_history() {
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.has_active_task = true;

        let removed = session.rollback_last_turns(10);

        assert_eq!(removed.removed_turns, 1);
        assert_eq!(removed.removed_messages, 2);
        assert!(session.tape.messages().is_empty());
        assert!(!session.has_active_task);
    }

    #[test]
    fn test_rollback_last_turns_ignores_control_user_messages_for_turn_boundaries() {
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.add_user_control_message_parts(vec![ContentPart::structured(serde_json::json!({
            "checkpoint_id": "tool_escalation_call-1",
            "checkpoint_type": "tool_escalation",
            "choice": "approve",
            "__alan_internal_control": {
                "kind": "tool_escalation_confirmation",
                "version": 1,
                "source": "runtime/submission_handlers"
            }
        }))]);
        session.add_assistant_message("a2", None);

        let removed = session.rollback_last_turns(1);

        assert_eq!(
            removed.removed_messages, 4,
            "rollback should anchor on the real user turn, not synthetic control messages"
        );
        assert_eq!(removed.removed_turns, 1);
        assert!(session.tape.messages().is_empty());
    }

    #[test]
    fn test_rollback_last_turns_ignores_effect_replay_control_messages_for_turn_boundaries() {
        let mut session = Session::new();
        session.add_user_message("u1");
        session.add_assistant_message("a1", None);
        session.add_user_control_message_parts(vec![ContentPart::structured(serde_json::json!({
            "checkpoint_id": "effect_replay_call-1",
            "checkpoint_type": "effect_replay_confirmation",
            "choice": "approve",
            "__alan_internal_control": {
                "kind": "effect_replay_confirmation",
                "version": 1,
                "source": "runtime/submission_handlers"
            }
        }))]);
        session.add_assistant_message("a2", None);

        let removed = session.rollback_last_turns(1);

        assert_eq!(
            removed.removed_messages, 4,
            "rollback should ignore effect replay control messages the same way as policy controls"
        );
        assert_eq!(removed.removed_turns, 1);
        assert!(session.tape.messages().is_empty());
    }
}
