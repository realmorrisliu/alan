//! AgentMachine state management.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::error;

use alan_agent_protocol::{CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot};

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

/// Transition state for one Agent Process.
#[derive(Debug)]
pub struct AgentMachine {
    /// Conversation history and summary
    pub tape: Tape,
    /// Optional recorder for persistence
    pub recorder: Option<RolloutRecorder>,
    /// Per-Process durable-record discriminator used by generated Memory Store paths.
    memory_record_id: String,
    /// Whether a sourcing task has been started in this machine
    pub has_active_task: bool,
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

impl AgentMachine {
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

    fn turn_ordinal_from_effect_idempotency_key(key: &str) -> Option<u64> {
        let tail = key.strip_prefix("machine:turn:")?;
        let turn_segment = tail.split(':').next()?;
        turn_segment.parse::<u64>().ok()
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

    /// Create a new machine without persistence
    pub fn new() -> Self {
        Self {
            tape: Tape::new(),
            recorder: None,
            memory_record_id: uuid::Uuid::new_v4().to_string(),
            has_active_task: false,
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
        process_path: &str,
        model: &str,
        rollouts_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        let rollouts_dir = rollouts_dir
            .ok_or_else(|| anyhow::anyhow!("Agent Process has no rollout store binding"))?;
        let recorder = RolloutRecorder::new_in_dir_with_cwd_and_reasoning_effort(
            process_path,
            model,
            rollouts_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await?;
        let memory_record_id = recorder.rollout_id().to_string();

        Ok(Self {
            tape: Tape::new(),
            recorder: Some(recorder),
            memory_record_id,
            has_active_task: false,
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

    /// Create a new machine with recorder under a specific rollouts directory.
    pub async fn new_with_recorder_in_dir(
        process_path: &str,
        model: &str,
        rollouts_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(process_path, model, Some(rollouts_dir), None, None).await
    }

    /// Load a machine from a rollout file
    pub async fn load_from_rollout(
        path: &PathBuf,
        process_path: &str,
        model: &str,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(path, process_path, model, None, None, None).await
    }

    /// Load a machine from a rollout file, writing future persistence to a specific rollouts dir.
    pub async fn load_from_rollout_in_dir(
        path: &PathBuf,
        process_path: &str,
        model: &str,
        rollouts_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(path, process_path, model, Some(rollouts_dir), None, None)
            .await
    }

    pub(crate) async fn load_from_rollout_with_recorder_cwd(
        path: &PathBuf,
        process_path: &str,
        model: &str,
        rollouts_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        Self::load_from_rollout_impl(
            path,
            process_path,
            model,
            rollouts_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await
    }

    async fn load_from_rollout_impl(
        path: &PathBuf,
        process_path: &str,
        model: &str,
        rollouts_dir: Option<&Path>,
        rollout_cwd: Option<&Path>,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> anyhow::Result<Self> {
        let items = RolloutRecorder::load_history(path).await?;
        if !matches!(items.first(), Some(RolloutItem::AgentMachineMeta(_))) {
            anyhow::bail!("rollout does not begin with current Agent Machine metadata");
        }

        // Recovery is explicitly scoped to the newly launched Agent Process. The
        // source rollout is evidence, not a globally addressable execution identity.
        let mut machine = Self::new_with_recorder_options(
            process_path,
            model,
            rollouts_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await?;

        let recovered_latest_compaction_attempt =
            Self::latest_compaction_attempt_from_rollout_items_internal(&items);
        let recovered_latest_memory_flush_attempt =
            Self::latest_memory_flush_attempt_from_rollout_items_internal(&items);
        let mut context_items: Vec<ContextItem> = Vec::new();
        let mut compaction_attempt_records: Vec<CompactionAttemptSnapshot> = Vec::new();
        let mut memory_flush_attempt_records: Vec<MemoryFlushAttemptSnapshot> = Vec::new();
        let mut recovered_compaction: Option<CompactedItem> = None;
        let mut effect_records: Vec<EffectRecord> = Vec::new();
        let mut event_records: Vec<EventRecord> = Vec::new();

        // Replay messages from history
        for item in items {
            match item {
                RolloutItem::Message(msg) => {
                    let Some(message) = msg.message else {
                        anyhow::bail!(
                            "rollout contains a message without the current rich message record"
                        );
                    };
                    if message.is_context() {
                        continue;
                    }
                    if message.is_user() && !Self::is_runtime_confirmation_control_message(&message)
                    {
                        machine.user_turn_ordinal = machine.user_turn_ordinal.saturating_add(1);
                    }
                    machine.tape.push(message);
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
                    machine.tape.set_summary(compacted.message.clone());
                    recovered_compaction = Some(compacted);
                }
                RolloutItem::CompactionAttempt(attempt) => {
                    compaction_attempt_records.push(attempt);
                }
                RolloutItem::MemoryFlushAttempt(attempt) => {
                    memory_flush_attempt_records.push(attempt);
                }
                RolloutItem::ToolCall(_) => {}
                RolloutItem::Effect(effect) => effect_records.push(effect),
                RolloutItem::Event(event) => event_records.push(event),
                _ => {} // Skip other item types during loading
            }
        }

        if !machine.tape.is_empty() {
            machine.has_active_task = true;
        }

        if !context_items.is_empty() {
            let _ = machine.tape.apply_context_items(context_items);
        }
        recovered_compaction = Self::stabilize_recovered_compacted_item_link(
            recovered_compaction,
            recovered_latest_compaction_attempt.as_ref(),
        );
        machine.latest_compaction_attempt = recovered_latest_compaction_attempt;
        machine.latest_memory_flush_attempt = recovered_latest_memory_flush_attempt;
        machine.responses_continuation =
            Self::responses_continuation_from_event_records(&event_records);

        for effect in &effect_records {
            machine
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
            machine.user_turn_ordinal = machine.user_turn_ordinal.max(max_effect_turn);
        }

        let recovered_messages = machine.tape.messages().to_vec();
        if (!recovered_messages.is_empty()
            || recovered_compaction.is_some()
            || !compaction_attempt_records.is_empty()
            || !memory_flush_attempt_records.is_empty()
            || !effect_records.is_empty()
            || !event_records.is_empty())
            && let Some(recorder) = machine.recorder.as_ref()
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

        Ok(machine)
    }

    /// Add a user message to the machine
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

    /// Add a user message with rich content parts to the machine
    pub fn add_user_message_parts(&mut self, parts: Vec<crate::tape::ContentPart>) {
        self.add_user_message_parts_internal(parts, true);
    }

    /// Add a synthetic user control message without incrementing turn ordinal.
    pub fn add_user_control_message_parts(&mut self, parts: Vec<crate::tape::ContentPart>) {
        self.add_user_message_parts_internal(parts, false);
    }

    /// Add an assistant message to the machine
    pub fn add_assistant_message(&mut self, content: &str, thinking: Option<&str>) {
        self.add_assistant_message_with_reasoning(content, thinking, None, &[]);
    }

    /// Add an assistant message to the machine with full reasoning metadata.
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

    /// Add an assistant message with tool calls to the machine
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

    /// Add a tool message to the machine.
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
        self.tape.push(message);

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

    /// Clear the machine state (but keep the recorder)
    pub fn clear(&mut self) {
        self.tape.clear();
        self.has_active_task = false;
        self.last_turn_context_snapshot_fingerprint = None;
        self.clear_responses_continuation("machine_cleared");
    }

    /// Roll back the last `num_turns` user turns from in-memory context.
    ///
    /// This mutation is intentionally non-durable: recovery from persisted rollout
    /// history does not re-apply rollback markers to machine state.
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
            "machine_rollback",
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
        self.record_checkpoint_with_optional_knowledge_root(
            checkpoint_id,
            checkpoint_type,
            summary,
            choice,
            None,
        );
    }

    /// Record a checkpoint to persistence with an optional content-addressed
    /// knowledge root.
    pub fn record_checkpoint_with_optional_knowledge_root(
        &self,
        checkpoint_id: &str,
        checkpoint_type: &str,
        summary: &str,
        choice: Option<&str>,
        knowledge_root: Option<&str>,
    ) {
        if let Some(recorder) = self.recorder.as_ref()
            && let Err(err) = match knowledge_root {
                Some(root) => recorder.record_checkpoint_with_knowledge_root_nowait(
                    checkpoint_id,
                    checkpoint_type,
                    summary,
                    choice,
                    root,
                ),
                None => recorder.record_checkpoint_nowait(
                    checkpoint_id,
                    checkpoint_type,
                    summary,
                    choice,
                ),
            }
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
    /// results, prefer [`AgentMachine::persist_compaction_observation`] so related rollout items are
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
    #[allow(
        clippy::too_many_arguments,
        reason = "arguments map directly to the durable turn-context record fields"
    )]
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
    #[allow(
        clippy::too_many_arguments,
        reason = "arguments map directly to the durable turn-context record fields"
    )]
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

    /// Stable discriminator for this Agent Process's generated memory records.
    pub(crate) fn memory_record_id(&self) -> &str {
        &self.memory_record_id
    }
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

impl Default for AgentMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// Recover the latest compaction attempt from current rollout records.
pub fn latest_compaction_attempt_from_rollout_items(
    items: &[RolloutItem],
) -> Option<CompactionAttemptSnapshot> {
    AgentMachine::latest_compaction_attempt_from_rollout_items_internal(items)
}

/// Recover the latest memory-flush attempt from rollout items.
pub fn latest_memory_flush_attempt_from_rollout_items(
    items: &[RolloutItem],
) -> Option<MemoryFlushAttemptSnapshot> {
    AgentMachine::latest_memory_flush_attempt_from_rollout_items_internal(items)
}

#[cfg(test)]
#[path = "agent_machine_tests.rs"]
mod tests;
