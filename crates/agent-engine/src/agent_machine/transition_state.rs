use std::collections::{HashMap, VecDeque};

use super::AgentMachine;
use crate::approval::{PendingConfirmation, PendingStructuredInputRequest};
use crate::skills::ActiveSkillEnvelope;
use crate::tape::ContentPart;
use alan_agent_protocol::{PlanItem, Submission};
use serde::{Deserialize, Serialize};

const MAX_QUEUED_NEXT_TURN_INPUTS: usize = 16;
const AUTO_MID_TURN_COMPACTION_LIMIT: u32 = 2;
const AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS: usize = 256;

pub(crate) const HOST_MOUNT_REQUEST_WAITING_EVENT_TYPE: &str = "host_mount_request_waiting";
pub(crate) const HOST_MOUNT_REQUEST_WAIT_CLEARED_EVENT_TYPE: &str =
    "host_mount_request_wait_cleared";
pub(crate) const HOST_MOUNT_REQUEST_TERMINAL_EVENT_TYPE: &str = "host_mount_request_terminal";

pub(crate) fn is_auto_mid_turn_compaction_emergency(
    estimated_prompt_tokens: usize,
    context_window_tokens: usize,
) -> bool {
    context_window_tokens > 0
        && estimated_prompt_tokens
            >= context_window_tokens.saturating_sub(AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS)
}

#[derive(Debug, Clone)]
pub(crate) enum PendingYield {
    Confirmation(PendingConfirmation),
    StructuredInput(PendingStructuredInputRequest),
    HostMount(PendingHostMountRequest),
}

/// Durable logical wait state for one Host Mount Service request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PendingHostMountRequest {
    pub(crate) request_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) namespace_path: String,
    pub(crate) access: String,
    pub(crate) reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) request_events_offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TurnActivityState {
    #[default]
    Idle,
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanSnapshot {
    pub explanation: Option<String>,
    pub items: Vec<PlanItem>,
}

/// Tool call normalized at the Machine transition boundary.
#[derive(Debug, Clone)]
pub(crate) struct NormalizedToolCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingToolReplayBatch {
    pub(crate) tool_calls: Vec<NormalizedToolCall>,
    pub(crate) resume_with_generation: bool,
}

/// Best-effort work retained by Machine until the outer Process loop can run it.
#[derive(Debug, Clone)]
pub(crate) enum DeferredRuntimeAction {
    TurnMemoryPromotion(crate::runtime::TurnMemoryPromotionJob),
}

#[derive(Debug, Clone, Default)]
pub(super) struct MachineTransitionState {
    /// Identifier of the submission currently accepted by this Machine.
    current_submission_id: Option<String>,
    related_submission_ids: Vec<String>,
    /// Cancellation performed by the transition, independent of later control signals.
    submission_cancelled: bool,
    pending: HashMap<String, PendingYield>,
    pending_tool_replay_batches: HashMap<String, PendingToolReplayBatch>,
    /// Insertion order tracking for all pending items
    pending_order: Vec<String>,
    turn_activity: TurnActivityState,
    /// Ordinary queued work and pause state survive reset_turn, like next-turn input.
    pub(super) input_queue: std::sync::Arc<std::sync::Mutex<super::input_queue::MachineInputQueue>>,
    /// Number of automatic mid-turn compactions already performed in the active turn.
    compactions_this_turn: u32,
    /// Prompt token estimate immediately after the most recent mid-turn compaction.
    last_compaction_prompt_tokens: Option<usize>,
    /// Tape message index where the current logical turn started.
    active_turn_message_start: Option<usize>,
    /// Active skills resolved for the current turn.
    active_skills: Vec<ActiveSkillEnvelope>,
    /// Optional request-control intent scoped to the active logical turn.
    active_turn_request_control_intent: crate::RequestControlIntent,
    /// Latest explicit plan/progress state published during the current machine.
    plan_snapshot: Option<PlanSnapshot>,
    /// Turn boundary active when the latest plan snapshot was published.
    plan_snapshot_turn_start: Option<usize>,
    /// Tape message count when the latest plan snapshot was published.
    plan_snapshot_message_count: Option<usize>,
    /// Best-effort follow-up work queued after a turn completes.
    deferred_runtime_actions: VecDeque<DeferredRuntimeAction>,
    /// Guardian rejection circuit breaker: consecutive denials and a rolling
    /// window of recent review outcomes (true = denied) in the active turn.
    guardian_consecutive_denials: u32,
    guardian_recent_reviews: VecDeque<bool>,
}

/// Guardian rejection circuit-breaker thresholds (Codex parity).
const GUARDIAN_MAX_CONSECUTIVE_DENIALS: u32 = 3;
const GUARDIAN_DENIAL_WINDOW: usize = 50;
const GUARDIAN_MAX_DENIALS_IN_WINDOW: usize = 10;

impl AgentMachine {
    pub(crate) fn accept_submission(&mut self, submission_id: impl Into<String>) {
        self.transition_state.current_submission_id = Some(submission_id.into());
        self.transition_state.submission_cancelled = false;
        self.transition_state.related_submission_ids.clear();
        self.sync_active_submission_ids();
    }

    pub(crate) fn finish_submission(&mut self) {
        if self.has_pending_interaction() {
            return;
        }
        self.transition_state.current_submission_id = None;
        self.transition_state.submission_cancelled = false;
        self.transition_state.related_submission_ids.clear();
        self.sync_active_submission_ids();
    }

    fn sync_active_submission_ids(&self) {
        let mut queue = self
            .transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned");
        queue
            .active_submission_ids
            .clone_from(&self.transition_state.related_submission_ids);
        queue
            .active_submission_ids
            .extend(self.transition_state.current_submission_id.clone());
    }

    pub(crate) fn mark_submission_cancelled(&mut self) {
        self.transition_state.submission_cancelled = true;
    }

    pub(crate) fn submission_was_cancelled(&self) -> bool {
        self.transition_state.submission_cancelled
    }

    pub(crate) fn current_submission_id(&self) -> Option<&str> {
        self.transition_state.current_submission_id.as_deref()
    }

    /// Steering joins the active work; the eventual answer belongs to every participant.
    pub(crate) fn accept_steering_submission(&mut self, submission_id: String) {
        if let Some(previous) = self
            .transition_state
            .current_submission_id
            .replace(submission_id)
            && !self
                .transition_state
                .related_submission_ids
                .contains(&previous)
        {
            self.transition_state.related_submission_ids.push(previous);
        }
        self.sync_active_submission_ids();
    }

    pub(crate) fn related_submission_ids(&self) -> &[String] {
        &self.transition_state.related_submission_ids
    }

    /// Record a guardian review outcome (true = denied). Returns true when the
    /// rejection circuit breaker trips (≥3 consecutive, or ≥10 denials within
    /// the last 50 reviews this turn). A non-denial resets the consecutive count.
    pub(crate) fn record_guardian_review(&mut self, denied: bool) -> bool {
        if denied {
            self.transition_state.guardian_consecutive_denials = self
                .transition_state
                .guardian_consecutive_denials
                .saturating_add(1);
        } else {
            self.transition_state.guardian_consecutive_denials = 0;
        }
        self.transition_state
            .guardian_recent_reviews
            .push_back(denied);
        while self.transition_state.guardian_recent_reviews.len() > GUARDIAN_DENIAL_WINDOW {
            self.transition_state.guardian_recent_reviews.pop_front();
        }
        let denials_in_window = self
            .transition_state
            .guardian_recent_reviews
            .iter()
            .filter(|d| **d)
            .count();
        self.transition_state.guardian_consecutive_denials >= GUARDIAN_MAX_CONSECUTIVE_DENIALS
            || denials_in_window >= GUARDIAN_MAX_DENIALS_IN_WINDOW
    }

    pub(crate) fn has_pending_interaction(&self) -> bool {
        !self.transition_state.pending.is_empty()
    }

    pub(crate) fn pending_request_ids(&self) -> Vec<String> {
        self.transition_state.pending_order.clone()
    }

    pub(crate) fn reset_turn(&mut self) {
        let cleared_host_mount_requests = self
            .transition_state
            .pending
            .values()
            .filter_map(|pending| match pending {
                PendingYield::HostMount(pending) => Some(pending.request_id.clone()),
                PendingYield::Confirmation(_) | PendingYield::StructuredInput(_) => None,
            })
            .collect::<Vec<_>>();
        for request_id in cleared_host_mount_requests {
            self.record_event(
                HOST_MOUNT_REQUEST_WAIT_CLEARED_EVENT_TYPE,
                serde_json::json!({
                    "request_id": request_id,
                    "reason": "turn_reset",
                }),
            );
        }
        self.transition_state.pending.clear();
        self.transition_state.pending_tool_replay_batches.clear();
        self.transition_state.pending_order.clear();
        self.transition_state.turn_activity = TurnActivityState::Idle;
        self.transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned")
            .buffered_inband_submissions
            .clear();
        self.transition_state.active_turn_message_start = None;
        self.transition_state.active_skills.clear();
        self.transition_state.active_turn_request_control_intent =
            crate::RequestControlIntent::default();
        self.reset_auto_mid_turn_compaction_state();
        self.transition_state.guardian_consecutive_denials = 0;
        self.transition_state.guardian_recent_reviews.clear();
    }

    pub(crate) fn clear_plan_snapshot(&mut self) {
        self.transition_state.plan_snapshot = None;
        self.transition_state.plan_snapshot_turn_start = None;
        self.transition_state.plan_snapshot_message_count = None;
    }

    pub(crate) fn reset_auto_mid_turn_compaction_state(&mut self) {
        self.transition_state.compactions_this_turn = 0;
        self.transition_state.last_compaction_prompt_tokens = None;
    }

    /// Queue `next_turn` input parts. Returns `Some(new_len)` on success, `None` on overflow.
    pub(crate) fn queue_next_turn_input(&mut self, parts: Vec<ContentPart>) -> Option<usize> {
        let mut queue = self
            .transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned");
        if queue.queued_next_turn_inputs.len() >= MAX_QUEUED_NEXT_TURN_INPUTS {
            return None;
        }
        queue
            .queued_next_turn_inputs
            .push_back((self.transition_state.current_submission_id.clone(), parts));
        Some(queue.queued_next_turn_inputs.len())
    }

    /// Drain queued `next_turn` input parts in FIFO order.
    pub(crate) fn drain_next_turn_inputs(&mut self) -> VecDeque<Vec<ContentPart>> {
        let inputs = std::mem::take(
            &mut self
                .transition_state
                .input_queue
                .lock()
                .expect("input queue poisoned")
                .queued_next_turn_inputs,
        );
        let parts = inputs
            .into_iter()
            .map(|(id, parts)| {
                if let Some(id) = id
                    && self.current_submission_id() != Some(id.as_str())
                    && !self.transition_state.related_submission_ids.contains(&id)
                {
                    self.transition_state.related_submission_ids.push(id);
                }
                parts
            })
            .collect();
        self.sync_active_submission_ids();
        parts
    }

    /// Number of queued `next_turn` payloads.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "queue count is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn queued_next_turn_input_count(&self) -> usize {
        self.transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned")
            .queued_next_turn_inputs
            .len()
    }

    /// Drain all buffered inband submissions.
    pub(crate) fn drain_buffered_inband_submissions(&mut self) -> VecDeque<Submission> {
        std::mem::take(
            &mut self
                .transition_state
                .input_queue
                .lock()
                .expect("input queue poisoned")
                .buffered_inband_submissions,
        )
    }

    /// Push a submission to the buffered inband submissions queue.
    pub(crate) fn push_buffered_inband_submission(&mut self, submission: Submission) {
        self.transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned")
            .buffered_inband_submissions
            .push_back(submission);
    }

    /// Pop a submission from the buffered inband submissions queue.
    pub(crate) fn pop_buffered_inband_submission(&mut self) -> Option<Submission> {
        self.transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned")
            .buffered_inband_submissions
            .pop_front()
    }

    /// Count user input submissions in the buffered queue
    pub(crate) fn buffered_inband_user_input_count(&self) -> usize {
        self.transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned")
            .buffered_inband_submissions
            .iter()
            .filter(|submission| matches!(submission.op, alan_agent_protocol::Op::Input { .. }))
            .count()
    }

    /// Clear buffered inband submissions and return the count
    pub(crate) fn clear_buffered_inband_submissions(&mut self) -> usize {
        let mut queue = self
            .transition_state
            .input_queue
            .lock()
            .expect("input queue poisoned");
        let count = queue.buffered_inband_submissions.len();
        queue.buffered_inband_submissions.clear();
        count
    }

    /// Get the latest pending key across all pending types
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "pending-key inspection is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn latest_pending_key(&self) -> Option<String> {
        self.transition_state.pending_order.last().cloned()
    }

    pub(crate) fn set_turn_activity(&mut self, activity: TurnActivityState) {
        self.transition_state.turn_activity = activity;
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "activity inspection is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn turn_activity(&self) -> TurnActivityState {
        self.transition_state.turn_activity
    }

    pub(crate) fn is_turn_active(&self) -> bool {
        !matches!(self.transition_state.turn_activity, TurnActivityState::Idle)
    }

    pub(crate) fn begin_turn(&mut self, tape_message_count: usize) {
        self.transition_state.active_turn_message_start = Some(tape_message_count);
    }

    pub(crate) fn active_turn_message_start(&self) -> Option<usize> {
        self.transition_state.active_turn_message_start
    }

    pub(crate) fn set_active_turn_request_control_intent(
        &mut self,
        intent: crate::RequestControlIntent,
    ) {
        self.transition_state.active_turn_request_control_intent = intent;
    }

    pub(crate) fn active_turn_request_control_intent(&self) -> crate::RequestControlIntent {
        self.transition_state.active_turn_request_control_intent
    }

    pub(crate) fn note_tape_compaction(&mut self, retention_start: usize) {
        if let Some(active_turn_message_start) =
            &mut self.transition_state.active_turn_message_start
        {
            *active_turn_message_start = active_turn_message_start.saturating_sub(retention_start);
        }
        if self
            .transition_state
            .plan_snapshot_turn_start
            .is_some_and(|start| start < retention_start)
        {
            self.transition_state.plan_snapshot_turn_start = None;
        } else if let Some(plan_snapshot_turn_start) =
            &mut self.transition_state.plan_snapshot_turn_start
        {
            *plan_snapshot_turn_start -= retention_start;
        }
        if self
            .transition_state
            .plan_snapshot_message_count
            .is_some_and(|count| count < retention_start)
        {
            self.transition_state.plan_snapshot_message_count = None;
        } else if let Some(plan_snapshot_message_count) =
            &mut self.transition_state.plan_snapshot_message_count
        {
            *plan_snapshot_message_count -= retention_start;
        }
    }

    pub(crate) fn note_resumed_user_input(&mut self) {
        self.transition_state.plan_snapshot_turn_start = None;
    }

    pub(crate) fn set_active_skills(&mut self, active_skills: Vec<ActiveSkillEnvelope>) {
        self.transition_state.active_skills = active_skills;
    }

    pub(crate) fn active_skills(&self) -> &[ActiveSkillEnvelope] {
        &self.transition_state.active_skills
    }

    pub(crate) fn set_plan_snapshot(&mut self, explanation: Option<String>, items: Vec<PlanItem>) {
        self.transition_state.plan_snapshot = Some(PlanSnapshot { explanation, items });
        self.transition_state.plan_snapshot_turn_start =
            self.transition_state.active_turn_message_start;
        self.transition_state.plan_snapshot_message_count = None;
    }

    pub(crate) fn set_plan_snapshot_at_message_count(
        &mut self,
        explanation: Option<String>,
        items: Vec<PlanItem>,
        tape_message_count: usize,
    ) {
        self.set_plan_snapshot(explanation, items);
        self.transition_state.plan_snapshot_message_count = Some(tape_message_count);
    }

    pub(crate) fn plan_snapshot(&self) -> Option<&PlanSnapshot> {
        self.transition_state.plan_snapshot.as_ref()
    }

    pub(crate) fn plan_snapshot_is_from_active_turn(&self) -> bool {
        self.transition_state.active_turn_message_start.is_some()
            && self.transition_state.plan_snapshot_turn_start
                == self.transition_state.active_turn_message_start
    }

    pub(crate) fn plan_snapshot_postdates_message(&self, message_index: usize) -> bool {
        self.transition_state
            .plan_snapshot_message_count
            .is_some_and(|count| count > message_index)
    }

    pub(crate) fn push_deferred_runtime_action(&mut self, action: DeferredRuntimeAction) {
        self.transition_state
            .deferred_runtime_actions
            .push_back(action);
    }

    pub(crate) fn drain_deferred_runtime_actions(&mut self) -> VecDeque<DeferredRuntimeAction> {
        std::mem::take(&mut self.transition_state.deferred_runtime_actions)
    }

    pub(crate) fn can_auto_mid_turn_compact(
        &self,
        estimated_prompt_tokens: usize,
        context_window_tokens: usize,
    ) -> bool {
        if is_auto_mid_turn_compaction_emergency(estimated_prompt_tokens, context_window_tokens) {
            return true;
        }

        if self.transition_state.compactions_this_turn >= AUTO_MID_TURN_COMPACTION_LIMIT {
            return false;
        }

        if let Some(last_prompt_tokens) = self.transition_state.last_compaction_prompt_tokens
            && estimated_prompt_tokens
                <= last_prompt_tokens.saturating_add(AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS)
        {
            return false;
        }

        true
    }

    pub(crate) fn record_auto_mid_turn_compaction(&mut self, output_prompt_tokens: usize) {
        self.transition_state.compactions_this_turn = self
            .transition_state
            .compactions_this_turn
            .saturating_add(1);
        self.transition_state.last_compaction_prompt_tokens = Some(output_prompt_tokens);
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "compaction-count inspection is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn compactions_this_turn(&self) -> u32 {
        self.transition_state.compactions_this_turn
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "confirmation adapter is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn set_confirmation(&mut self, pending: PendingConfirmation) {
        self.set_confirmation_for_request(pending.checkpoint_id.clone(), pending);
    }

    pub(crate) fn set_confirmation_for_request(
        &mut self,
        request_id: impl Into<String>,
        pending: PendingConfirmation,
    ) {
        let key = request_id.into();
        self.transition_state
            .pending
            .insert(key.clone(), PendingYield::Confirmation(pending));
        push_latest_key(&mut self.transition_state.pending_order, key);
    }

    pub(crate) fn pending_confirmation(&self) -> Option<PendingConfirmation> {
        self.transition_state
            .pending_order
            .iter()
            .rev()
            .find_map(|key| match self.transition_state.pending.get(key) {
                Some(PendingYield::Confirmation(value)) => Some(value.clone()),
                _ => None,
            })
    }

    pub(crate) fn set_tool_replay_batch(
        &mut self,
        checkpoint_id: impl Into<String>,
        tool_calls: Vec<NormalizedToolCall>,
        resume_with_generation: bool,
    ) {
        self.transition_state.pending_tool_replay_batches.insert(
            checkpoint_id.into(),
            PendingToolReplayBatch {
                tool_calls,
                resume_with_generation,
            },
        );
    }

    pub(crate) fn take_tool_replay_batch(
        &mut self,
        checkpoint_id: &str,
    ) -> Option<PendingToolReplayBatch> {
        self.transition_state
            .pending_tool_replay_batches
            .remove(checkpoint_id)
    }

    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "structured-input adapter is exposed for the adjacent turn-state tests"
        )
    )]
    pub(crate) fn set_structured_input(&mut self, pending: PendingStructuredInputRequest) {
        self.set_structured_input_for_request(pending.request_id.clone(), pending);
    }

    pub(crate) fn set_structured_input_for_request(
        &mut self,
        request_id: impl Into<String>,
        pending: PendingStructuredInputRequest,
    ) {
        let key = request_id.into();
        self.transition_state
            .pending
            .insert(key.clone(), PendingYield::StructuredInput(pending));
        push_latest_key(&mut self.transition_state.pending_order, key);
    }

    pub(crate) fn set_host_mount_request(&mut self, pending: PendingHostMountRequest) {
        let key = pending.request_id.clone();
        self.transition_state
            .pending
            .insert(key.clone(), PendingYield::HostMount(pending));
        push_latest_key(&mut self.transition_state.pending_order, key);
    }

    pub(crate) fn pending_host_mount(&self, request_id: &str) -> Option<PendingHostMountRequest> {
        match self.transition_state.pending.get(request_id) {
            Some(PendingYield::HostMount(pending)) => Some(pending.clone()),
            _ => None,
        }
    }

    pub(crate) fn pending_yield(&self, request_id: &str) -> Option<&PendingYield> {
        self.transition_state.pending.get(request_id)
    }

    pub(crate) fn pending_tool_replay_batch(
        &self,
        checkpoint_id: &str,
    ) -> Option<&PendingToolReplayBatch> {
        self.transition_state
            .pending_tool_replay_batches
            .get(checkpoint_id)
    }

    /// Unified lookup: take any pending item by request_id.
    pub(crate) fn take_pending(&mut self, request_id: &str) -> Option<PendingYield> {
        let item = self.transition_state.pending.remove(request_id)?;
        remove_key(&mut self.transition_state.pending_order, request_id);
        Some(item)
    }
}

fn push_latest_key(order: &mut Vec<String>, key: String) {
    remove_key(order, &key);
    order.push(key);
}

fn remove_key(order: &mut Vec<String>, key: &str) {
    if let Some(pos) = order.iter().position(|existing| existing == key) {
        order.remove(pos);
    }
}

#[cfg(test)]
mod tests;
