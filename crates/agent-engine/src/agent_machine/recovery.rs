use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use alan_agent_protocol::{CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot};
use tracing::error;

use super::{
    AgentMachine, HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE, HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE,
    PendingHostMountRequest, ResponsesContinuationState, runtime_control,
};
use crate::rollout::{CompactedItem, EffectRecord, EventRecord, RolloutItem, RolloutRecorder};
use crate::tape::ContextItem;

impl AgentMachine {
    fn pending_host_mounts_from_event_records(
        event_records: &[EventRecord],
    ) -> Vec<PendingHostMountRequest> {
        let mut pending = BTreeMap::new();
        for event in event_records {
            match event.event_type.as_str() {
                HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE => {
                    if let Ok(request) =
                        serde_json::from_value::<PendingHostMountRequest>(event.payload.clone())
                    {
                        pending.insert(request.request_id.clone(), request);
                    }
                }
                HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE => {
                    if let Some(request_id) = event
                        .payload
                        .get("request_id")
                        .and_then(serde_json::Value::as_str)
                    {
                        pending.remove(request_id);
                    }
                }
                _ => {}
            }
        }
        pending.into_values().collect()
    }

    fn responses_continuation_from_event_records(
        event_records: &[EventRecord],
    ) -> Option<ResponsesContinuationState> {
        event_records.iter().fold(None, |_, event| {
            if event.event_type != runtime_control::RESPONSES_CONTINUATION_EVENT_TYPE {
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

    /// Load a machine from a rollout file, writing future persistence to a specific rollouts dir.
    #[cfg(test)]
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
                    if message.is_user()
                        && !runtime_control::is_runtime_confirmation_control_message(&message)
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

        if !machine.tape_is_empty() {
            machine.activate_task();
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
        for pending in Self::pending_host_mounts_from_event_records(&event_records) {
            machine.set_host_mount_request(pending);
        }

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

        let recovered_messages = machine.messages().to_vec();
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
