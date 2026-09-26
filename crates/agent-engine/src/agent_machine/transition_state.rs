use std::collections::{HashMap, VecDeque};

use super::AgentMachine;
use crate::approval::{PendingConfirmation, PendingStructuredInputRequest};
use crate::skills::ActiveSkillEnvelope;
use crate::tape::ContentPart;
use alan_agent_protocol::{PlanItem, Submission};
use serde::{Deserialize, Serialize};

#[path = "submission_queue.rs"]
mod submission_queue;

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
    pub(crate) explicit_command: bool,
}

/// Best-effort work retained by Machine until the outer Process loop can run it.
#[derive(Debug, Clone)]
pub(crate) enum DeferredRuntimeAction {
    TurnMemoryPromotion(crate::runtime::TurnMemoryPromotionJob),
}

#[derive(Debug, Default)]
pub(super) struct MachineTransitionState {
    pub(super) input_broker: super::input_queue::TurnInputBroker,

    pending: HashMap<String, PendingYield>,
    pending_tool_replay_batches: HashMap<String, PendingToolReplayBatch>,
    /// Insertion order tracking for all pending items
    pending_order: Vec<String>,
    turn_activity: TurnActivityState,

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
    #[cfg(test)]
    pub(crate) fn accept_submission(&mut self, submission_id: impl Into<String>) {
        self.accept_submission_identity(alan_agent_protocol::UiSubmission {
            submission_id: submission_id.into(),
            intent: alan_agent_protocol::InputIntent::Agent,
        });
    }

    pub(crate) fn accept_submission_identity(
        &mut self,
        identity: alan_agent_protocol::UiSubmission,
    ) {
        self.transition_state.input_broker.state().active_submission = Some(identity);
        self.transition_state.input_broker.record_activity();
    }

    pub(crate) fn finish_submission(&mut self) {
        self.transition_state.input_broker.state().active_submission = None;
        self.transition_state.input_broker.record_activity();
    }

    pub(crate) fn current_submission_identity(&self) -> Option<alan_agent_protocol::UiSubmission> {
        self.transition_state
            .input_broker
            .state()
            .active_submission
            .clone()
    }

    pub(crate) fn current_submission_id(&self) -> Option<String> {
        self.current_submission_identity()
            .map(|input| input.submission_id)
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
        if !self.transition_state.input_broker.is_paused() {
            self.transition_state.input_broker.state().buffered.clear();
        }
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

    /// Drain all buffered inband submissions.
    pub(crate) fn drain_buffered_inband_submissions(&mut self) -> VecDeque<Submission> {
        std::mem::take(&mut self.transition_state.input_broker.state().buffered)
    }

    /// Push a submission to the buffered inband submissions queue.
    pub(crate) fn push_buffered_inband_submission(&mut self, submission: Submission) {
        self.transition_state
            .input_broker
            .state()
            .buffered
            .push_back(submission);
    }

    /// Pop a submission from the buffered inband submissions queue.
    pub(crate) fn pop_buffered_inband_submission(&mut self) -> Option<Submission> {
        self.transition_state
            .input_broker
            .state()
            .buffered
            .pop_front()
    }

    /// Count user input submissions in the buffered queue
    pub(crate) fn buffered_inband_user_input_count(&self) -> usize {
        self.transition_state
            .input_broker
            .state()
            .buffered
            .iter()
            .filter(|submission| matches!(submission.op, alan_agent_protocol::Op::Input { .. }))
            .count()
    }

    /// Clear buffered inband submissions and return the count
    pub(crate) fn clear_buffered_inband_submissions(&mut self) -> usize {
        let count = self.transition_state.input_broker.state().buffered.len();
        self.transition_state.input_broker.state().buffered.clear();
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
                explicit_command: !resume_with_generation,
            },
        );
    }

    pub(crate) fn set_tool_replay_continuation(&mut self, checkpoint_id: &str, generate: bool) {
        if let Some(batch) = self
            .transition_state
            .pending_tool_replay_batches
            .get_mut(checkpoint_id)
        {
            batch.resume_with_generation = generate;
        }
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepted_submission_identity_is_machine_owned() {
        let mut machine = AgentMachine::new();
        assert_eq!(machine.current_submission_id(), None);

        machine.accept_submission("sub-1");
        assert_eq!(machine.current_submission_id().as_deref(), Some("sub-1"));
        machine.add_user_message("request");
        machine.add_assistant_message("response", None);
        assert_eq!(machine.messages()[0].submission_id(), Some("sub-1"));
        assert_eq!(machine.messages()[1].submission_id(), Some("sub-1"));
        assert_eq!(
            serde_json::to_value(&machine.messages()[0]).unwrap()["submission_id"],
            "sub-1"
        );

        machine.finish_submission();
        assert_eq!(machine.current_submission_id(), None);
    }

    #[test]
    fn guardian_breaker_trips_on_three_consecutive_denials() {
        let mut state = AgentMachine::new();
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(state.record_guardian_review(true)); // third consecutive trips
    }

    #[test]
    fn guardian_breaker_resets_on_allow() {
        let mut state = AgentMachine::new();
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(false)); // reset
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(state.record_guardian_review(true)); // three consecutive again
    }

    #[test]
    fn guardian_breaker_trips_on_ten_denials_in_window() {
        let mut state = AgentMachine::new();
        // Interleave allow/deny so it never hits 3 consecutive, but reaches 10
        // denials within the rolling window.
        let mut tripped = false;
        for _ in 0..10 {
            state.record_guardian_review(false);
            tripped = state.record_guardian_review(true);
        }
        assert!(tripped);
    }

    #[test]
    fn test_confirmation_set_and_pending() {
        let mut state = AgentMachine::new();
        state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string(), "reject".to_string()],
        });

        let latest = state.pending_confirmation().unwrap();
        assert_eq!(latest.checkpoint_id, "cp-1");

        // take_pending removes it
        let taken = state.take_pending("cp-1").unwrap();
        assert!(matches!(taken, PendingYield::Confirmation(_)));
        assert!(state.pending_confirmation().is_none());
    }

    #[test]
    fn test_clear_resets_pending_interactions() {
        let mut state = AgentMachine::new();
        state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string()],
        });
        state.reset_turn();
        assert!(state.pending_confirmation().is_none());
        assert!(!state.has_pending_interaction());
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
    }

    #[test]
    fn test_turn_activity_state_roundtrip_and_clear() {
        let mut state = AgentMachine::new();
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));

        state.set_turn_activity(TurnActivityState::Running);
        assert!(matches!(state.turn_activity(), TurnActivityState::Running));

        state.set_turn_activity(TurnActivityState::Paused);
        assert!(matches!(state.turn_activity(), TurnActivityState::Paused));

        state.reset_turn();
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
        assert_eq!(state.compactions_this_turn(), 0);
    }

    #[test]
    fn test_clear_preserves_plan_snapshot() {
        let mut state = AgentMachine::new();
        state.set_plan_snapshot(
            Some("Keep the current plan".to_string()),
            vec![PlanItem {
                id: "plan-1".to_string(),
                content: "Run delegated review".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        );

        state.reset_turn();

        let snapshot = state.plan_snapshot().expect("plan snapshot should persist");
        assert_eq!(
            snapshot.explanation.as_deref(),
            Some("Keep the current plan")
        );
        assert_eq!(snapshot.items.len(), 1);
    }

    #[test]
    fn test_clear_plan_snapshot_removes_latest_plan() {
        let mut state = AgentMachine::new();
        state.set_plan_snapshot(
            Some("Drop the current plan".to_string()),
            vec![PlanItem {
                id: "plan-1".to_string(),
                content: "Cancelled work".to_string(),
                status: alan_agent_protocol::PlanItemStatus::Pending,
            }],
        );

        state.clear_plan_snapshot();

        assert!(state.plan_snapshot().is_none());
    }

    #[test]
    fn test_active_skills_roundtrip_and_clear() {
        let mut state = AgentMachine::new();
        state.set_active_skills(vec![crate::skills::ActiveSkillEnvelope::available(
            crate::skills::SkillMetadata {
                id: "deploy".to_string(),
                package_id: Some("skill:deploy".to_string()),
                name: "Deploy".to_string(),
                description: "Deploy service".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/deploy/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: crate::skills::SkillScope::Descriptor,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: crate::skills::SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/deploy/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: Default::default(),
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            crate::skills::SkillActivationReason::ExplicitMention {
                mention: "deploy".to_string(),
            },
        )]);

        assert_eq!(state.active_skills().len(), 1);
        assert_eq!(state.active_skills()[0].metadata.id, "deploy");

        state.reset_turn();
        assert!(state.active_skills().is_empty());
    }

    #[test]
    fn test_active_turn_message_start_tracks_turn_start_and_compaction() {
        let mut state = AgentMachine::new();
        assert_eq!(state.active_turn_message_start(), None);

        state.begin_turn(5);
        assert_eq!(state.active_turn_message_start(), Some(5));

        state.note_tape_compaction(2);
        assert_eq!(state.active_turn_message_start(), Some(3));

        state.note_tape_compaction(10);
        assert_eq!(state.active_turn_message_start(), Some(0));

        state.reset_turn();
        assert_eq!(state.active_turn_message_start(), None);
    }

    #[test]
    fn dropped_plan_boundary_does_not_become_active_after_compaction() {
        let mut state = AgentMachine::new();
        state.begin_turn(2);
        state.set_plan_snapshot(Some("old plan".to_string()), Vec::new());
        state.begin_turn(5);

        state.note_tape_compaction(5);

        assert_eq!(state.active_turn_message_start(), Some(0));
        assert!(!state.plan_snapshot_is_from_active_turn());
    }

    #[test]
    fn test_auto_mid_turn_compaction_budget_and_growth_guard() {
        let mut state = AgentMachine::new();
        assert!(state.can_auto_mid_turn_compact(4_000, 8_192));

        state.record_auto_mid_turn_compaction(3_200);
        assert_eq!(state.compactions_this_turn(), 1);
        assert!(!state.can_auto_mid_turn_compact(3_300, 8_192));
        assert!(state.can_auto_mid_turn_compact(3_600, 8_192));

        state.record_auto_mid_turn_compaction(3_400);
        assert_eq!(state.compactions_this_turn(), 2);
        assert!(!state.can_auto_mid_turn_compact(3_700, 8_192));
        assert!(state.can_auto_mid_turn_compact(7_980, 8_192));

        state.reset_turn();
        assert!(state.can_auto_mid_turn_compact(4_000, 8_192));
    }

    #[test]
    fn test_auto_mid_turn_compaction_emergency_helper() {
        assert!(is_auto_mid_turn_compaction_emergency(4_000, 4_128));
        assert!(!is_auto_mid_turn_compaction_emergency(4_000, 4_400));
        assert!(!is_auto_mid_turn_compaction_emergency(4_000, 0));
    }

    #[test]
    fn test_latest_pending_key_tracks_cross_type_insertion_order() {
        let mut state = AgentMachine::new();
        state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "manual".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string()],
        });
        assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));

        state.set_structured_input(PendingStructuredInputRequest {
            request_id: "input-1".to_string(),
            title: "Input".to_string(),
            prompt: "Value?".to_string(),
            questions: vec![],
        });
        assert_eq!(state.latest_pending_key().as_deref(), Some("input-1"));

        let _ = state.take_pending("input-1");
        assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));
    }

    #[test]
    fn test_turn_state_buffers_inband_submissions_fifo() {
        let mut state = AgentMachine::new();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Resume {
                request_id: "latest".to_string(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    serde_json::json!({"choice": "approve"}),
                )],
            },
        });

        assert_eq!(state.buffered_inband_user_input_count(), 1);
        assert_eq!(
            state
                .pop_buffered_inband_submission()
                .as_ref()
                .map(|s| s.id.as_str()),
            Some("s1")
        );
        assert_eq!(
            state
                .pop_buffered_inband_submission()
                .as_ref()
                .map(|s| s.id.as_str()),
            Some("s2")
        );
        assert!(state.pop_buffered_inband_submission().is_none());
    }

    #[test]
    fn test_turn_state_drain_buffered_inband_submissions_preserves_order() {
        let mut state = AgentMachine::new();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Resume {
                request_id: "latest".to_string(),
                content: vec![alan_agent_protocol::ContentPart::structured(
                    serde_json::json!({"choice": "approve"}),
                )],
            },
        });

        let drained = state.drain_buffered_inband_submissions();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained.front().map(|s| s.id.as_str()), Some("s1"));
        assert_eq!(drained.back().map(|s| s.id.as_str()), Some("s2"));
        assert!(state.pop_buffered_inband_submission().is_none());
    }

    #[test]
    fn test_clear_buffered_inband_submissions_returns_count() {
        let mut state = AgentMachine::new();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("two")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });

        let count = state.clear_buffered_inband_submissions();
        assert_eq!(count, 2);
        assert!(state.pop_buffered_inband_submission().is_none());
    }

    #[test]
    fn test_tool_replay_batch_roundtrip() {
        let mut state = AgentMachine::new();
        let tool_calls = vec![
            NormalizedToolCall {
                id: "call-1".to_string(),
                name: "web_search".to_string(),
                arguments: json!({"query": "rust"}),
            },
            NormalizedToolCall {
                id: "call-2".to_string(),
                name: "memory_write".to_string(),
                arguments: json!({"key": "test", "value": "data"}),
            },
        ];

        state.set_tool_replay_batch("cp-1", tool_calls, true);

        let retrieved = state.take_tool_replay_batch("cp-1").unwrap();
        assert!(retrieved.resume_with_generation);
        assert_eq!(retrieved.tool_calls.len(), 2);
        assert_eq!(retrieved.tool_calls[0].id, "call-1");
        assert_eq!(retrieved.tool_calls[1].id, "call-2");

        // Should be removed after take
        assert!(state.take_tool_replay_batch("cp-1").is_none());
    }
}
