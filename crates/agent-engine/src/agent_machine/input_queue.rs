//! Machine-owned in-turn admission, shared with the Process input pump.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard},
};

use alan_agent_protocol::{
    InputMode, Op, Submission, UiActivitySnapshot, UiActivityState, UiSubmission,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

mod persistence;

const MAX_BROKERED_INBAND_USER_INPUTS: usize = 16;

#[derive(Debug, Clone)]
pub(crate) struct TurnInputBroker {
    inner: Arc<TurnInputBrokerInner>,
}

#[derive(Debug)]
struct TurnInputBrokerInner {
    state: Mutex<MachineInputQueues>,
    notify: Notify,
    activity_notify: Notify,
    recorder: Mutex<Option<crate::rollout::RolloutRecorder>>,
    persisted: Mutex<(u64, Option<serde_json::Value>)>,
}

impl Default for TurnInputBroker {
    fn default() -> Self {
        Self {
            inner: Arc::new(TurnInputBrokerInner {
                state: Mutex::new(MachineInputQueues::default()),
                notify: Notify::new(),
                activity_notify: Notify::new(),
                recorder: Mutex::new(None),
                persisted: Mutex::new((0, None)),
            }),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct MachineInputQueues {
    pub(super) active_submission: Option<UiSubmission>,
    ui_activity: UiActivitySnapshot,
    activity_events: VecDeque<UiActivitySnapshot>,
    pub(super) inband: VecDeque<Submission>,
    pub(super) buffered: VecDeque<Submission>,
    pub(super) next_turn: VecDeque<Submission>,
    outer: VecDeque<QueuedRuntimeItem>,
    pub(super) paused: bool,
    unknown_inputs: Vec<UiSubmission>,
    checkpoint_cwd: Option<std::path::PathBuf>,
    recovery_warning: Option<String>,
}

#[derive(Debug)]
pub(crate) enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::DeferredRuntimeAction),
}

impl TurnInputBroker {
    pub(super) fn state(&self) -> MutexGuard<'_, MachineInputQueues> {
        self.inner
            .state
            .lock()
            .expect("Machine input queue poisoned")
    }

    pub(crate) fn activity_snapshot(&self) -> UiActivitySnapshot {
        let state = self.state();
        Self::project_activity(&state)
    }

    fn project_activity(state: &MachineInputQueues) -> UiActivitySnapshot {
        let pending = state
            .buffered
            .iter()
            .chain(&state.inband)
            .chain(state.outer.iter().filter_map(|item| match item {
                QueuedRuntimeItem::Submission(s) => Some(s),
                _ => None,
            }))
            .chain(&state.next_turn)
            .filter(|s| matches!(s.op, Op::Input { .. } | Op::Turn { .. }))
            .map(UiSubmission::from)
            .collect();
        UiActivitySnapshot {
            active_submission: state.active_submission.clone(),
            pending_submissions: pending,
            queue_paused: state.paused,
            ..state.ui_activity.clone()
        }
    }

    pub(crate) fn record_activity(&self) {
        let mut state = self.state();
        let snapshot = Self::project_activity(&state);
        state.activity_events.push_back(snapshot);
        drop(state);
        self.inner.activity_notify.notify_one();
    }

    pub(crate) fn set_ui_activity(&self, activity: UiActivityState) {
        let mut state = self.state();
        state.ui_activity.state = activity;
        if activity == UiActivityState::Idle {
            state.ui_activity.started_at_ms = None;
        } else if state.ui_activity.started_at_ms.is_none() {
            state.ui_activity.started_at_ms = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            );
        }
        drop(state);
        self.record_activity();
    }

    pub(crate) async fn activity_changed(&self) {
        self.inner.activity_notify.notified().await;
    }

    pub(crate) fn drain_activity_events(&self) -> VecDeque<UiActivitySnapshot> {
        std::mem::take(&mut self.state().activity_events)
    }

    pub(crate) fn pop_outer(&self) -> Option<QueuedRuntimeItem> {
        let mut state = self.state();
        if state.paused {
            return None;
        }
        state.outer.pop_front()
    }

    pub(crate) fn pop_outer_deferred(&self) -> Option<QueuedRuntimeItem> {
        let mut state = self.state();
        let index = state
            .outer
            .iter()
            .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))?;
        state.outer.remove(index)
    }

    pub(crate) fn push_outer_submission(&self, submission: Submission) {
        let mut state = self.state();
        let index = state
            .outer
            .iter()
            .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))
            .unwrap_or(state.outer.len());
        state
            .outer
            .insert(index, QueuedRuntimeItem::Submission(submission));
    }

    pub(crate) fn push_outer_front(&self, submission: Submission) {
        self.state()
            .outer
            .push_front(QueuedRuntimeItem::Submission(submission));
    }

    pub(crate) fn push_outer_deferred(&self, action: super::DeferredRuntimeAction) {
        self.state()
            .outer
            .push_back(QueuedRuntimeItem::Deferred(action));
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.state().paused
    }

    /// Returns whether the target is currently executing. Unknown targets change no state.
    pub(crate) fn interrupt(&self, id: &str) -> anyhow::Result<bool> {
        let mut state = self.state();
        let active = state
            .active_submission
            .as_ref()
            .is_some_and(|input| input.submission_id == id);
        let mut removed = false;
        if !active {
            state.outer.retain(|item| {
                let keep = !matches!(item, QueuedRuntimeItem::Submission(s) if s.id == id);
                removed |= !keep;
                keep
            });
            let MachineInputQueues {
                inband,
                buffered,
                next_turn,
                ..
            } = &mut *state;
            for queue in [inband, buffered, next_turn] {
                queue.retain(|s| {
                    let keep = s.id != id;
                    removed |= !keep;
                    keep
                });
            }
        }
        anyhow::ensure!(active || removed, "unknown or settled submission: {id}");
        state.paused = true;
        Ok(active)
    }

    pub(crate) fn continue_queue(&self) -> anyhow::Result<()> {
        let mut state = self.state();
        anyhow::ensure!(state.paused, "input queue is not paused");
        anyhow::ensure!(
            state.active_submission.is_none(),
            "active input has not settled yet"
        );
        state.paused = false;
        state.unknown_inputs.clear();
        state.recovery_warning = None;
        Ok(())
    }

    pub(crate) fn discard_queue(&self) -> anyhow::Result<Vec<Submission>> {
        let mut state = self.state();
        anyhow::ensure!(state.paused, "input queue is not paused");
        anyhow::ensure!(
            state.active_submission.is_none(),
            "active input has not settled yet"
        );
        let mut discarded = Vec::new();
        state.outer.retain(|item| match item {
            QueuedRuntimeItem::Submission(s) => {
                discarded.push(s.clone());
                false
            }
            QueuedRuntimeItem::Deferred(_) => true,
        });
        discarded.extend(state.buffered.drain(..));
        discarded.extend(state.inband.drain(..));
        discarded.extend(state.next_turn.drain(..));
        state.paused = false;
        state.unknown_inputs.clear();
        state.recovery_warning = None;
        Ok(discarded)
    }

    pub(crate) async fn push(&self, submission: Submission) -> bool {
        let mut state = self.state();
        let guard = &mut state.inband;
        if is_brokered_input(&submission.op)
            && guard
                .iter()
                .filter(|queued| is_brokered_input(&queued.op))
                .count()
                >= MAX_BROKERED_INBAND_USER_INPUTS
        {
            return false;
        }
        guard.push_back(submission);
        drop(state);
        self.inner.notify.notify_one();
        true
    }

    pub(crate) async fn recv(&self, cancel: &CancellationToken) -> Option<Submission> {
        loop {
            if cancel.is_cancelled() {
                return None;
            }
            if let Some(submission) = self.try_pop().await {
                return Some(submission);
            }

            tokio::select! {
                _ = cancel.cancelled() => return None,
                _ = self.inner.notify.notified() => {}
            }
        }
    }

    #[cfg(test)]
    pub(crate) async fn clear(&self) {
        self.state().inband.clear();
    }

    pub(crate) async fn drain(&self) -> VecDeque<Submission> {
        std::mem::take(&mut self.state().inband)
    }

    pub(crate) async fn try_recv(&self) -> Option<Submission> {
        self.try_pop().await
    }

    async fn try_pop(&self) -> Option<Submission> {
        let mut state = self.state();
        if state.paused {
            let index = state
                .inband
                .iter()
                .position(|s| matches!(s.op, Op::Resume { .. }))?;
            return state.inband.remove(index);
        }
        state.inband.pop_front()
    }
}

pub(crate) fn is_brokered_input(op: &Op) -> bool {
    matches!(
        op,
        Op::Input {
            mode: InputMode::Steer | InputMode::FollowUp,
            ..
        }
    )
}

impl super::AgentMachine {
    pub(crate) fn input_broker(&self) -> TurnInputBroker {
        self.transition_state.input_broker.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_receiver_leaves_the_accepted_input_for_the_machine() {
        let machine = super::super::AgentMachine::new();
        let broker = machine.input_broker();
        let submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("printf next")],
            mode: InputMode::FollowUp,
        });
        let id = submission.id.clone();
        assert!(broker.push(submission).await);
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(broker.recv(&cancel).await.is_none());
        let retained = machine.input_broker().try_recv().await.unwrap();
        assert_eq!(retained.id, id);
        assert!(broker.try_recv().await.is_none());
    }
    #[tokio::test]
    async fn targeted_interrupt_preserves_later_inputs_until_continue_or_discard() {
        let mut machine = super::super::AgentMachine::new();
        let queue = machine.input_broker();
        let input = |id: &str| {
            Submission::with_id_and_intent(
                id,
                Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(id)],
                    mode: InputMode::FollowUp,
                },
                alan_agent_protocol::InputIntent::Command,
            )
        };
        queue.push_outer_submission(input("outer"));
        machine.push_buffered_inband_submission(input("buffered"));
        queue.push(input("inband")).await;
        machine.queue_next_turn_submission(input("deferred"));
        machine.accept_submission("active");
        assert!(queue.interrupt("unknown").is_err());
        assert!(!queue.is_paused());
        assert!(queue.interrupt("active").unwrap());
        assert!(queue.pop_outer().is_none());
        assert!(queue.try_recv().await.is_none());
        assert!(
            queue.continue_queue().is_err(),
            "active cancellation must settle first"
        );
        machine.reset_turn();
        assert_eq!(machine.buffered_inband_user_input_count(), 1);
        machine.finish_submission();
        queue.continue_queue().unwrap();
        let Some(QueuedRuntimeItem::Submission(first)) = queue.pop_outer() else {
            panic!("lost input")
        };
        assert_eq!(first.id, "outer");
        assert!(!queue.interrupt("inband").unwrap());
        assert!(
            queue.interrupt("inband").is_err(),
            "settled target cannot affect later work"
        );
        let discarded = queue.discard_queue().unwrap();
        assert_eq!(
            discarded.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["buffered", "deferred"]
        );
        assert!(queue.pop_outer().is_none());
        assert!(queue.try_recv().await.is_none());
        assert!(machine.take_next_turn_commands().is_empty());
        assert!(!queue.is_paused());
    }
    #[test]
    fn activity_projection_retains_intent_order_and_paused_queue_without_command_bodies() {
        let mut machine = super::super::AgentMachine::new();
        let queue = machine.input_broker();
        machine.accept_submission_identity(UiSubmission {
            submission_id: "active".into(),
            intent: alan_agent_protocol::InputIntent::ForceAgent,
        });
        for id in ["first", "second"] {
            queue.push_outer_submission(Submission::with_id_and_intent(
                id,
                Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text(
                        "private command body",
                    )],
                    mode: InputMode::FollowUp,
                },
                alan_agent_protocol::InputIntent::Command,
            ));
        }
        queue.set_ui_activity(UiActivityState::Running);
        let running = queue.activity_snapshot();
        assert_eq!(running.version, 2);
        assert_eq!(
            running.active_submission.as_ref().unwrap().intent,
            alan_agent_protocol::InputIntent::ForceAgent
        );
        assert_eq!(
            running
                .pending_submissions
                .iter()
                .map(|s| s.submission_id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(
            !serde_json::to_string(&running)
                .unwrap()
                .contains("private command body")
        );
        queue.interrupt("active").unwrap();
        machine.finish_submission();
        queue.set_ui_activity(UiActivityState::Idle);
        let paused = queue.activity_snapshot();
        assert!(paused.queue_paused);
        assert!(paused.active_submission.is_none());
        assert_eq!(paused.pending_submissions, running.pending_submissions);
        assert!(
            queue
                .drain_activity_events()
                .iter()
                .any(|e| e.state == UiActivityState::Running && e.active_submission.is_some())
        );
    }
}
