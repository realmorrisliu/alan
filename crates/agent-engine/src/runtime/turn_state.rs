use std::collections::{HashMap, VecDeque};

use super::agent_loop::{DeferredRuntimeAction, NormalizedToolCall};
use crate::approval::{PendingConfirmation, PendingDynamicToolCall, PendingStructuredInputRequest};
use crate::skills::ActiveSkillEnvelope;
use crate::tape::ContentPart;
use alan_agent_protocol::{PlanItem, Submission};

const MAX_QUEUED_NEXT_TURN_INPUTS: usize = 16;
const AUTO_MID_TURN_COMPACTION_LIMIT: u32 = 2;
const AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS: usize = 256;

pub(super) fn is_auto_mid_turn_compaction_emergency(
    estimated_prompt_tokens: usize,
    context_window_tokens: usize,
) -> bool {
    context_window_tokens > 0
        && estimated_prompt_tokens
            >= context_window_tokens.saturating_sub(AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS)
}

#[derive(Debug, Clone)]
pub(super) enum PendingYield {
    Confirmation(PendingConfirmation),
    StructuredInput(PendingStructuredInputRequest),
    DynamicToolCall(PendingDynamicToolCall),
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

#[derive(Debug, Clone, Default)]
pub(crate) struct TurnState {
    pending: HashMap<String, PendingYield>,
    pending_tool_replay_batches: HashMap<String, Vec<NormalizedToolCall>>,
    /// Insertion order tracking for all pending items
    pending_order: Vec<String>,
    turn_activity: TurnActivityState,
    /// Submissions buffered during turn execution that need to be requeued
    /// after the turn completes (e.g., user input during tool execution).
    buffered_inband_submissions: VecDeque<Submission>,
    /// Queued context for `InputMode::NextTurn`.
    queued_next_turn_inputs: VecDeque<Vec<ContentPart>>,
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
    /// Latest explicit plan/progress state published during the current session.
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

impl TurnState {
    /// Record a guardian review outcome (true = denied). Returns true when the
    /// rejection circuit breaker trips (≥3 consecutive, or ≥10 denials within
    /// the last 50 reviews this turn). A non-denial resets the consecutive count.
    pub(crate) fn record_guardian_review(&mut self, denied: bool) -> bool {
        if denied {
            self.guardian_consecutive_denials = self.guardian_consecutive_denials.saturating_add(1);
        } else {
            self.guardian_consecutive_denials = 0;
        }
        self.guardian_recent_reviews.push_back(denied);
        while self.guardian_recent_reviews.len() > GUARDIAN_DENIAL_WINDOW {
            self.guardian_recent_reviews.pop_front();
        }
        let denials_in_window = self.guardian_recent_reviews.iter().filter(|d| **d).count();
        self.guardian_consecutive_denials >= GUARDIAN_MAX_CONSECUTIVE_DENIALS
            || denials_in_window >= GUARDIAN_MAX_DENIALS_IN_WINDOW
    }

    pub(crate) fn has_pending_interaction(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn pending_request_ids(&self) -> Vec<String> {
        self.pending_order.clone()
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.pending_tool_replay_batches.clear();
        self.pending_order.clear();
        self.turn_activity = TurnActivityState::Idle;
        self.buffered_inband_submissions.clear();
        self.active_turn_message_start = None;
        self.active_skills.clear();
        self.active_turn_request_control_intent = crate::RequestControlIntent::default();
        self.reset_auto_mid_turn_compaction_state();
        self.guardian_consecutive_denials = 0;
        self.guardian_recent_reviews.clear();
    }

    pub(crate) fn clear_plan_snapshot(&mut self) {
        self.plan_snapshot = None;
        self.plan_snapshot_turn_start = None;
        self.plan_snapshot_message_count = None;
    }

    pub(crate) fn reset_auto_mid_turn_compaction_state(&mut self) {
        self.compactions_this_turn = 0;
        self.last_compaction_prompt_tokens = None;
    }

    /// Queue `next_turn` input parts. Returns `Some(new_len)` on success, `None` on overflow.
    pub(crate) fn queue_next_turn_input(&mut self, parts: Vec<ContentPart>) -> Option<usize> {
        if self.queued_next_turn_inputs.len() >= MAX_QUEUED_NEXT_TURN_INPUTS {
            return None;
        }
        self.queued_next_turn_inputs.push_back(parts);
        Some(self.queued_next_turn_inputs.len())
    }

    /// Drain queued `next_turn` input parts in FIFO order.
    pub(crate) fn drain_next_turn_inputs(&mut self) -> VecDeque<Vec<ContentPart>> {
        std::mem::take(&mut self.queued_next_turn_inputs)
    }

    /// Number of queued `next_turn` payloads.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn queued_next_turn_input_count(&self) -> usize {
        self.queued_next_turn_inputs.len()
    }

    /// Drain all buffered inband submissions.
    pub(crate) fn drain_buffered_inband_submissions(&mut self) -> VecDeque<Submission> {
        std::mem::take(&mut self.buffered_inband_submissions)
    }

    /// Push a submission to the buffered inband submissions queue.
    pub(crate) fn push_buffered_inband_submission(&mut self, submission: Submission) {
        self.buffered_inband_submissions.push_back(submission);
    }

    /// Pop a submission from the buffered inband submissions queue.
    pub(crate) fn pop_buffered_inband_submission(&mut self) -> Option<Submission> {
        self.buffered_inband_submissions.pop_front()
    }

    /// Count user input submissions in the buffered queue
    pub(crate) fn buffered_inband_user_input_count(&self) -> usize {
        self.buffered_inband_submissions
            .iter()
            .filter(|submission| matches!(submission.op, alan_agent_protocol::Op::Input { .. }))
            .count()
    }

    /// Clear buffered inband submissions and return the count
    pub(crate) fn clear_buffered_inband_submissions(&mut self) -> usize {
        let count = self.buffered_inband_submissions.len();
        self.buffered_inband_submissions.clear();
        count
    }

    /// Get the latest pending key across all pending types
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn latest_pending_key(&self) -> Option<String> {
        self.pending_order.last().cloned()
    }

    pub(crate) fn set_turn_activity(&mut self, activity: TurnActivityState) {
        self.turn_activity = activity;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn turn_activity(&self) -> TurnActivityState {
        self.turn_activity
    }

    pub(crate) fn is_turn_active(&self) -> bool {
        !matches!(self.turn_activity, TurnActivityState::Idle)
    }

    pub(crate) fn begin_turn(&mut self, tape_message_count: usize) {
        self.active_turn_message_start = Some(tape_message_count);
    }

    pub(crate) fn active_turn_message_start(&self) -> Option<usize> {
        self.active_turn_message_start
    }

    pub(crate) fn set_active_turn_request_control_intent(
        &mut self,
        intent: crate::RequestControlIntent,
    ) {
        self.active_turn_request_control_intent = intent;
    }

    pub(crate) fn active_turn_request_control_intent(&self) -> crate::RequestControlIntent {
        self.active_turn_request_control_intent
    }

    pub(crate) fn note_tape_compaction(&mut self, retention_start: usize) {
        if let Some(active_turn_message_start) = &mut self.active_turn_message_start {
            *active_turn_message_start = active_turn_message_start.saturating_sub(retention_start);
        }
        if self
            .plan_snapshot_turn_start
            .is_some_and(|start| start < retention_start)
        {
            self.plan_snapshot_turn_start = None;
        } else if let Some(plan_snapshot_turn_start) = &mut self.plan_snapshot_turn_start {
            *plan_snapshot_turn_start -= retention_start;
        }
        if self
            .plan_snapshot_message_count
            .is_some_and(|count| count < retention_start)
        {
            self.plan_snapshot_message_count = None;
        } else if let Some(plan_snapshot_message_count) = &mut self.plan_snapshot_message_count {
            *plan_snapshot_message_count -= retention_start;
        }
    }

    pub(crate) fn note_resumed_user_input(&mut self) {
        self.plan_snapshot_turn_start = None;
    }

    pub(crate) fn set_active_skills(&mut self, active_skills: Vec<ActiveSkillEnvelope>) {
        self.active_skills = active_skills;
    }

    pub(crate) fn active_skills(&self) -> &[ActiveSkillEnvelope] {
        &self.active_skills
    }

    pub(crate) fn set_plan_snapshot(&mut self, explanation: Option<String>, items: Vec<PlanItem>) {
        self.plan_snapshot = Some(PlanSnapshot { explanation, items });
        self.plan_snapshot_turn_start = self.active_turn_message_start;
        self.plan_snapshot_message_count = None;
    }

    pub(crate) fn set_plan_snapshot_at_message_count(
        &mut self,
        explanation: Option<String>,
        items: Vec<PlanItem>,
        tape_message_count: usize,
    ) {
        self.set_plan_snapshot(explanation, items);
        self.plan_snapshot_message_count = Some(tape_message_count);
    }

    pub(crate) fn plan_snapshot(&self) -> Option<&PlanSnapshot> {
        self.plan_snapshot.as_ref()
    }

    pub(crate) fn plan_snapshot_is_from_active_turn(&self) -> bool {
        self.active_turn_message_start.is_some()
            && self.plan_snapshot_turn_start == self.active_turn_message_start
    }

    pub(crate) fn plan_snapshot_postdates_message(&self, message_index: usize) -> bool {
        self.plan_snapshot_message_count
            .is_some_and(|count| count > message_index)
    }

    pub(crate) fn push_deferred_runtime_action(&mut self, action: DeferredRuntimeAction) {
        self.deferred_runtime_actions.push_back(action);
    }

    pub(crate) fn drain_deferred_runtime_actions(&mut self) -> VecDeque<DeferredRuntimeAction> {
        std::mem::take(&mut self.deferred_runtime_actions)
    }

    pub(crate) fn can_auto_mid_turn_compact(
        &self,
        estimated_prompt_tokens: usize,
        context_window_tokens: usize,
    ) -> bool {
        if is_auto_mid_turn_compaction_emergency(estimated_prompt_tokens, context_window_tokens) {
            return true;
        }

        if self.compactions_this_turn >= AUTO_MID_TURN_COMPACTION_LIMIT {
            return false;
        }

        if let Some(last_prompt_tokens) = self.last_compaction_prompt_tokens
            && estimated_prompt_tokens
                <= last_prompt_tokens.saturating_add(AUTO_MID_TURN_COMPACTION_MIN_GROWTH_TOKENS)
        {
            return false;
        }

        true
    }

    pub(crate) fn record_auto_mid_turn_compaction(&mut self, output_prompt_tokens: usize) {
        self.compactions_this_turn = self.compactions_this_turn.saturating_add(1);
        self.last_compaction_prompt_tokens = Some(output_prompt_tokens);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn compactions_this_turn(&self) -> u32 {
        self.compactions_this_turn
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_confirmation(&mut self, pending: PendingConfirmation) {
        self.set_confirmation_for_request(pending.checkpoint_id.clone(), pending);
    }

    pub(crate) fn set_confirmation_for_request(
        &mut self,
        request_id: impl Into<String>,
        pending: PendingConfirmation,
    ) {
        let key = request_id.into();
        self.pending
            .insert(key.clone(), PendingYield::Confirmation(pending));
        push_latest_key(&mut self.pending_order, key);
    }

    pub(crate) fn pending_confirmation(&self) -> Option<PendingConfirmation> {
        self.pending_order
            .iter()
            .rev()
            .find_map(|key| match self.pending.get(key) {
                Some(PendingYield::Confirmation(value)) => Some(value.clone()),
                _ => None,
            })
    }

    pub(crate) fn set_tool_replay_batch(
        &mut self,
        checkpoint_id: impl Into<String>,
        tool_calls: Vec<NormalizedToolCall>,
    ) {
        self.pending_tool_replay_batches
            .insert(checkpoint_id.into(), tool_calls);
    }

    pub(crate) fn take_tool_replay_batch(
        &mut self,
        checkpoint_id: &str,
    ) -> Option<Vec<NormalizedToolCall>> {
        self.pending_tool_replay_batches.remove(checkpoint_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_structured_input(&mut self, pending: PendingStructuredInputRequest) {
        self.set_structured_input_for_request(pending.request_id.clone(), pending);
    }

    pub(crate) fn set_structured_input_for_request(
        &mut self,
        request_id: impl Into<String>,
        pending: PendingStructuredInputRequest,
    ) {
        let key = request_id.into();
        self.pending
            .insert(key.clone(), PendingYield::StructuredInput(pending));
        push_latest_key(&mut self.pending_order, key);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_dynamic_tool_call(&mut self, pending: PendingDynamicToolCall) {
        self.set_dynamic_tool_call_for_request(pending.call_id.clone(), pending);
    }

    pub(crate) fn set_dynamic_tool_call_for_request(
        &mut self,
        request_id: impl Into<String>,
        pending: PendingDynamicToolCall,
    ) {
        let key = request_id.into();
        self.pending
            .insert(key.clone(), PendingYield::DynamicToolCall(pending));
        push_latest_key(&mut self.pending_order, key);
    }

    /// Unified lookup: take any pending item by request_id.
    pub(super) fn take_pending(&mut self, request_id: &str) -> Option<PendingYield> {
        let item = self.pending.remove(request_id)?;
        remove_key(&mut self.pending_order, request_id);
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
    fn guardian_breaker_trips_on_three_consecutive_denials() {
        let mut state = TurnState::default();
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(state.record_guardian_review(true)); // third consecutive trips
    }

    #[test]
    fn guardian_breaker_resets_on_allow() {
        let mut state = TurnState::default();
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(false)); // reset
        assert!(!state.record_guardian_review(true));
        assert!(!state.record_guardian_review(true));
        assert!(state.record_guardian_review(true)); // three consecutive again
    }

    #[test]
    fn guardian_breaker_trips_on_ten_denials_in_window() {
        let mut state = TurnState::default();
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
        let mut state = TurnState::default();
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
    fn test_clear_resets_all_pending_types() {
        let mut state = TurnState::default();
        state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp".to_string(),
            checkpoint_type: "tool_escalation".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string()],
        });
        state.set_dynamic_tool_call(PendingDynamicToolCall {
            call_id: "d1".to_string(),
            tool_name: "lookup".to_string(),
            arguments: json!({"id":"1"}),
        });
        state.clear();
        assert!(state.pending_confirmation().is_none());
        assert!(!state.has_pending_interaction());
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
    }

    #[test]
    fn test_turn_activity_state_roundtrip_and_clear() {
        let mut state = TurnState::default();
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));

        state.set_turn_activity(TurnActivityState::Running);
        assert!(matches!(state.turn_activity(), TurnActivityState::Running));

        state.set_turn_activity(TurnActivityState::Paused);
        assert!(matches!(state.turn_activity(), TurnActivityState::Paused));

        state.clear();
        assert!(matches!(state.turn_activity(), TurnActivityState::Idle));
        assert_eq!(state.compactions_this_turn(), 0);
    }

    #[test]
    fn test_clear_preserves_plan_snapshot() {
        let mut state = TurnState::default();
        state.set_plan_snapshot(
            Some("Keep the current plan".to_string()),
            vec![PlanItem {
                id: "plan-1".to_string(),
                content: "Run delegated review".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        );

        state.clear();

        let snapshot = state.plan_snapshot().expect("plan snapshot should persist");
        assert_eq!(
            snapshot.explanation.as_deref(),
            Some("Keep the current plan")
        );
        assert_eq!(snapshot.items.len(), 1);
    }

    #[test]
    fn test_clear_plan_snapshot_removes_latest_plan() {
        let mut state = TurnState::default();
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
        let mut state = TurnState::default();
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
                scope: crate::skills::SkillScope::Repo,
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

        state.clear();
        assert!(state.active_skills().is_empty());
    }

    #[test]
    fn test_active_turn_message_start_tracks_turn_start_and_compaction() {
        let mut state = TurnState::default();
        assert_eq!(state.active_turn_message_start(), None);

        state.begin_turn(5);
        assert_eq!(state.active_turn_message_start(), Some(5));

        state.note_tape_compaction(2);
        assert_eq!(state.active_turn_message_start(), Some(3));

        state.note_tape_compaction(10);
        assert_eq!(state.active_turn_message_start(), Some(0));

        state.clear();
        assert_eq!(state.active_turn_message_start(), None);
    }

    #[test]
    fn dropped_plan_boundary_does_not_become_active_after_compaction() {
        let mut state = TurnState::default();
        state.begin_turn(2);
        state.set_plan_snapshot(Some("old plan".to_string()), Vec::new());
        state.begin_turn(5);

        state.note_tape_compaction(5);

        assert_eq!(state.active_turn_message_start(), Some(0));
        assert!(!state.plan_snapshot_is_from_active_turn());
    }

    #[test]
    fn test_auto_mid_turn_compaction_budget_and_growth_guard() {
        let mut state = TurnState::default();
        assert!(state.can_auto_mid_turn_compact(4_000, 8_192));

        state.record_auto_mid_turn_compaction(3_200);
        assert_eq!(state.compactions_this_turn(), 1);
        assert!(!state.can_auto_mid_turn_compact(3_300, 8_192));
        assert!(state.can_auto_mid_turn_compact(3_600, 8_192));

        state.record_auto_mid_turn_compaction(3_400);
        assert_eq!(state.compactions_this_turn(), 2);
        assert!(!state.can_auto_mid_turn_compact(3_700, 8_192));
        assert!(state.can_auto_mid_turn_compact(7_980, 8_192));

        state.clear();
        assert!(state.can_auto_mid_turn_compact(4_000, 8_192));
    }

    #[test]
    fn test_auto_mid_turn_compaction_emergency_helper() {
        assert!(is_auto_mid_turn_compaction_emergency(4_000, 4_128));
        assert!(!is_auto_mid_turn_compaction_emergency(4_000, 4_400));
        assert!(!is_auto_mid_turn_compaction_emergency(4_000, 0));
    }

    #[test]
    fn test_take_pending_removes_dynamic_tool_call() {
        let mut state = TurnState::default();
        state.set_dynamic_tool_call(PendingDynamicToolCall {
            call_id: "d1".to_string(),
            tool_name: "lookup".to_string(),
            arguments: json!({"id":"1"}),
        });

        let taken = state.take_pending("d1").unwrap();
        assert!(matches!(taken, PendingYield::DynamicToolCall(_)));
        assert!(!state.has_pending_interaction());
    }

    #[test]
    fn test_latest_pending_key_tracks_cross_type_insertion_order() {
        let mut state = TurnState::default();
        state.set_confirmation(PendingConfirmation {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "manual".to_string(),
            summary: "Approve?".to_string(),
            details: json!({}),
            options: vec!["approve".to_string()],
        });
        assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));

        state.set_dynamic_tool_call(PendingDynamicToolCall {
            call_id: "dyn-1".to_string(),
            tool_name: "lookup".to_string(),
            arguments: json!({"id":"1"}),
        });
        assert_eq!(state.latest_pending_key().as_deref(), Some("dyn-1"));

        let _ = state.take_pending("dyn-1");
        assert_eq!(state.latest_pending_key().as_deref(), Some("cp-1"));
    }

    #[test]
    fn test_turn_state_buffers_inband_submissions_fifo() {
        let mut state = TurnState::default();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
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
        let mut state = TurnState::default();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
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
        let mut state = TurnState::default();
        state.push_buffered_inband_submission(Submission {
            id: "s1".to_string(),
            op: alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("one")],
                mode: alan_agent_protocol::InputMode::Steer,
            },
        });
        state.push_buffered_inband_submission(Submission {
            id: "s2".to_string(),
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
    fn test_queue_next_turn_inputs_fifo_and_drain() {
        let mut state = TurnState::default();
        assert_eq!(
            state.queue_next_turn_input(vec![ContentPart::text("ctx-1")]),
            Some(1)
        );
        assert_eq!(
            state.queue_next_turn_input(vec![ContentPart::text("ctx-2")]),
            Some(2)
        );
        assert_eq!(state.queued_next_turn_input_count(), 2);

        let drained = state.drain_next_turn_inputs();
        assert_eq!(drained.len(), 2);
        assert_eq!(alan_agent_protocol::parts_to_text(&drained[0]), "ctx-1");
        assert_eq!(alan_agent_protocol::parts_to_text(&drained[1]), "ctx-2");
        assert_eq!(state.queued_next_turn_input_count(), 0);
    }

    #[test]
    fn test_queue_next_turn_inputs_overflow_is_rejected() {
        let mut state = TurnState::default();
        for _ in 0..MAX_QUEUED_NEXT_TURN_INPUTS {
            assert!(
                state
                    .queue_next_turn_input(vec![ContentPart::text("queued")])
                    .is_some()
            );
        }
        assert!(
            state
                .queue_next_turn_input(vec![ContentPart::text("overflow")])
                .is_none()
        );
    }

    #[test]
    fn test_tool_replay_batch_roundtrip() {
        let mut state = TurnState::default();
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

        state.set_tool_replay_batch("cp-1", tool_calls.clone());

        let retrieved = state.take_tool_replay_batch("cp-1").unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0].id, "call-1");
        assert_eq!(retrieved[1].id, "call-2");

        // Should be removed after take
        assert!(state.take_tool_replay_batch("cp-1").is_none());
    }
}
