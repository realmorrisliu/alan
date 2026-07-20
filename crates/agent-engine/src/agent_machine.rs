//! AgentMachine state management.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::error;

use alan_agent_protocol::{CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot};

use crate::rollout::{
    CompactedItem, ContextItemRecord, EffectRecord, ReferenceContextSnapshotRecord, RolloutItem,
    RolloutRecorder, build_durable_tool_payload,
};
use crate::tape::{ContextItem, ContextItemsDelta, Tape};

mod recovery;
mod runtime_control;
mod transition_state;

use runtime_control::{ResponsesContinuationState, RollbackOutcome};
use transition_state::MachineTransitionState;
pub(crate) use transition_state::{
    DeferredRuntimeAction, HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE,
    HOST_MOUNT_REQUEST_WAIT_CLEARED_EVENT_TYPE, HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE,
    NormalizedToolCall, PendingHostMountRequest, PendingYield, TurnActivityState,
    is_auto_mid_turn_compaction_emergency,
};

pub use recovery::{
    latest_compaction_attempt_from_rollout_items, latest_memory_flush_attempt_from_rollout_items,
};

/// Warning emitted when rollback succeeds but remains in-memory only.
pub const ROLLBACK_NON_DURABLE_WARNING: &str =
    "Rollback is in-memory only and will not survive runtime restart.";

/// Transition state for one Agent Process.
#[derive(Debug)]
pub(crate) struct AgentMachine {
    /// Conversation history and summary
    tape: Tape,
    /// Optional recorder for persistence
    recorder: Option<RolloutRecorder>,
    /// Per-Process durable-record discriminator used by generated Memory Store paths.
    memory_record_id: String,
    /// Whether a sourcing task has been started in this machine
    has_active_task: bool,
    /// All in-memory state local to an accepted submission and logical turn.
    transition_state: MachineTransitionState,
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
    /// Create a new machine without persistence
    pub fn new() -> Self {
        Self {
            tape: Tape::new(),
            recorder: None,
            memory_record_id: uuid::Uuid::new_v4().to_string(),
            has_active_task: false,
            transition_state: MachineTransitionState::default(),
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

    pub(crate) fn messages(&self) -> &[Message] {
        self.tape.messages()
    }

    #[cfg(test)]
    pub(crate) fn messages_for_prompt(&self) -> Vec<Message> {
        self.tape.messages_for_prompt()
    }

    pub(crate) fn prompt_view(&self) -> crate::tape::PromptContextView {
        self.tape.prompt_view()
    }

    pub(crate) fn tape_summary(&self) -> Option<&str> {
        self.tape.summary()
    }

    pub(crate) fn tape_len(&self) -> usize {
        self.tape.len()
    }

    pub(crate) fn tape_is_empty(&self) -> bool {
        self.tape.is_empty()
    }

    pub(crate) fn estimated_prompt_tokens(&self) -> usize {
        self.tape.estimated_prompt_tokens()
    }

    pub(crate) fn context_items(&self) -> &[ContextItem] {
        self.tape.context_items()
    }

    pub(crate) fn last_context_delta(&self) -> &ContextItemsDelta {
        self.tape.last_context_delta()
    }

    pub(crate) fn context_revision(&self) -> u64 {
        self.tape.context_revision()
    }

    pub(crate) fn compaction_count(&self) -> usize {
        self.tape.compaction_count()
    }

    pub(crate) fn compaction_retention_start(&self, keep_last: usize) -> usize {
        self.tape.compaction_retention_start(keep_last)
    }

    pub(crate) fn compact_tape(&mut self, summary: String, keep_last: usize) {
        self.tape.compact(summary, keep_last);
    }

    #[cfg(test)]
    pub(crate) fn push_tape_message(&mut self, message: Message) {
        self.tape.push(message);
    }

    #[cfg(test)]
    pub(crate) fn set_tape_summary(&mut self, summary: String) {
        self.tape.set_summary(summary);
    }

    #[cfg(test)]
    pub(crate) fn apply_context_items_for_test(&mut self, items: Vec<ContextItem>) {
        let _ = self.tape.apply_context_items(items);
    }

    pub(crate) fn activate_task(&mut self) {
        self.has_active_task = true;
    }

    pub(crate) fn clear_active_task(&mut self) {
        self.has_active_task = false;
    }

    #[cfg(test)]
    pub(crate) fn has_active_task(&self) -> bool {
        self.has_active_task
    }

    pub(crate) fn rollout_id(&self) -> Option<&str> {
        self.recorder.as_ref().map(RolloutRecorder::rollout_id)
    }

    pub(crate) fn is_durable(&self) -> bool {
        self.recorder.is_some()
    }

    pub(crate) async fn flush_recorder(&self) -> anyhow::Result<()> {
        match self.recorder.as_ref() {
            Some(recorder) => recorder.flush().await,
            None => Ok(()),
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
            transition_state: MachineTransitionState::default(),
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
    #[cfg(test)]
    pub async fn new_with_recorder_in_dir(
        process_path: &str,
        model: &str,
        rollouts_dir: &Path,
    ) -> anyhow::Result<Self> {
        Self::new_with_recorder_options(process_path, model, Some(rollouts_dir), None, None).await
    }

    /// Add a user message to the machine
    #[cfg(test)]
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
    #[cfg(test)]
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

    #[cfg(test)]
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
            runtime_control::RESPONSES_CONTINUATION_EVENT_TYPE,
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
            runtime_control::RESPONSES_CONTINUATION_EVENT_TYPE,
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
    #[cfg(test)]
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
                && !runtime_control::is_runtime_confirmation_control_message(msg)
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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
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
    #[cfg(test)]
    pub fn record_summary(&self, summary: &str) {
        self.record_compaction(CompactedItem::new(summary));
    }

    /// Record a compaction outcome to persistence (enqueue only; background writer performs IO)
    ///
    /// This low-level API persists only the compacted summary item. For tape-mutating compaction
    /// results, prefer [`AgentMachine::persist_compaction_observation`] so related rollout items are
    /// flushed together.
    #[cfg(test)]
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
    #[cfg(test)]
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

#[cfg(test)]
#[path = "agent_machine_tests.rs"]
mod tests;
