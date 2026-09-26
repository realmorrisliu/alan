//! Queue checkpoints use the Machine's existing ordered rollout writer.
use super::*;
use crate::rollout::{EventRecord, RolloutItem, RolloutRecorder};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

const EVENT: &str = "input_queue_checkpoint";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueueCheckpoint {
    version: u16,
    active: Option<UiSubmission>,
    cwd: Option<std::path::PathBuf>,
    pending: Vec<Submission>,
    next_turn: Vec<Submission>,
    paused: bool,
    unknown: Vec<UiSubmission>,
    recovery_warning: Option<String>,
}

impl TurnInputBroker {
    pub(crate) fn attach_recorder(&self, recorder: RolloutRecorder) {
        *self.inner.recorder.lock().expect("queue recorder poisoned") = Some(recorder);
    }

    /// Confirm state persistence before exposing admission or starting effects.
    pub(crate) async fn persist(&self) -> Result<()> {
        let recorder = self
            .inner
            .recorder
            .lock()
            .expect("queue recorder poisoned")
            .clone();
        let Some(recorder) = recorder else {
            return Ok(());
        };
        let (revision, snapshot, write) = {
            let mut persisted = self
                .inner
                .persisted
                .lock()
                .expect("queue persistence poisoned");
            // ponytail: checkpoints copy pending inputs; switch to an input journal if queues become large.
            let snapshot = {
                let state = self.state();
                serde_json::to_value(QueueCheckpoint {
                    version: 1,
                    active: state.active_submission.clone(),
                    cwd: state.checkpoint_cwd.clone(),
                    pending: state
                        .buffered
                        .iter()
                        .chain(&state.inband)
                        .chain(state.outer.iter().filter_map(|item| match item {
                            QueuedRuntimeItem::Submission(input) => Some(input),
                            QueuedRuntimeItem::Deferred(_) => None,
                        }))
                        .filter(|input| matches!(input.op, Op::Input { .. } | Op::Turn { .. }))
                        .cloned()
                        .collect(),
                    next_turn: state.next_turn.iter().cloned().collect(),
                    paused: state.paused,
                    unknown: state.unknown_inputs.clone(),
                    recovery_warning: state.recovery_warning.clone(),
                })?
            };
            if persisted.1.as_ref() == Some(&snapshot) {
                return Ok(());
            }
            persisted.0 += 1;
            persisted.1 = None;
            // Enqueue under the short lock to preserve snapshot order, but never hold it across IO.
            let write = recorder.enqueue_persist_batch(vec![RolloutItem::Event(EventRecord {
                event_type: EVENT.into(),
                payload: snapshot.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            })]);
            (persisted.0, snapshot, write)
        };
        write.await.context("persist input queue checkpoint")?;
        let mut persisted = self
            .inner
            .persisted
            .lock()
            .expect("queue persistence poisoned");
        if persisted.0 == revision {
            persisted.1 = Some(snapshot);
        }
        Ok(())
    }

    pub(crate) fn restore_from_events(&self, events: &[EventRecord]) -> Result<()> {
        let Some(event) = events.iter().rev().find(|event| event.event_type == EVENT) else {
            let mut state = self.state();
            state.paused = true;
            state.recovery_warning = Some("Input queue recovery records are missing; prior pending work is unknown. Continue or discard explicitly.".into());
            return Ok(());
        };
        let saved: QueueCheckpoint = serde_json::from_value(event.payload.clone())
            .context("invalid input queue recovery record")?;
        ensure!(
            saved.version == 1,
            "unsupported input queue checkpoint version"
        );
        ensure!(
            saved.cwd.is_some() || (saved.pending.is_empty() && saved.next_turn.is_empty()),
            "pending input recovery is missing Process cwd"
        );
        if let Some(cwd) = &saved.cwd {
            ensure!(cwd.is_absolute(), "recovered Process cwd must be absolute");
        }
        let mut ids = std::collections::HashSet::new();
        for input in saved.pending.iter().chain(&saved.next_turn) {
            ensure!(
                !input.id.is_empty() && ids.insert(input.id.as_str()),
                "duplicate or empty recovered input ID"
            );
            ensure!(
                matches!(input.op, Op::Input { .. } | Op::Turn { .. }),
                "recovery record contains a control operation"
            );
        }
        if let Some(active) = &saved.active {
            ensure!(
                !ids.contains(active.submission_id.as_str()),
                "active input also appears in the pending queue"
            );
        }
        let mut state = self.state();
        state.outer = saved
            .pending
            .into_iter()
            .map(QueuedRuntimeItem::Submission)
            .collect();
        state.next_turn = saved.next_turn.into();
        state.checkpoint_cwd = saved.cwd;
        state.unknown_inputs = saved.unknown;
        state.unknown_inputs.extend(saved.active);
        state.paused = saved.paused
            || !state.outer.is_empty()
            || !state.next_turn.is_empty()
            || !state.unknown_inputs.is_empty();
        state.recovery_warning = saved.recovery_warning;
        if state.paused && state.recovery_warning.is_none() {
            state.recovery_warning =
                Some("Recovered input queue is paused; continue or discard explicitly.".into());
        }
        Ok(())
    }

    // Durable snapshot only; the Process Tool execution binding remains the live cwd owner.
    pub(crate) fn checkpoint_cwd(&self, cwd: std::path::PathBuf) {
        self.state().checkpoint_cwd = Some(cwd);
    }

    pub(crate) fn recovered_cwd(&self) -> Option<std::path::PathBuf> {
        self.state().checkpoint_cwd.clone()
    }

    pub(crate) fn unknown_inputs(&self) -> Vec<UiSubmission> {
        self.state().unknown_inputs.clone()
    }

    pub(crate) fn recovery_warning(&self) -> Option<String> {
        self.state().recovery_warning.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_machine::AgentMachine;
    use alan_agent_protocol::{ContentPart, InputIntent};

    fn input(id: &str, intent: InputIntent) -> Submission {
        Submission::with_id_and_intent(
            id,
            Op::Input {
                parts: vec![ContentPart::text("identical body")],
                mode: InputMode::FollowUp,
            },
            intent,
        )
    }

    #[tokio::test]
    async fn disk_recovery_pauses_ordered_inputs_and_never_replays_active_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut machine = AgentMachine::new_with_recorder_options(
            "/proc/1",
            "model",
            Some(dir.path()),
            None,
            None,
        )
        .await
        .unwrap();
        let queue = machine.input_broker();
        machine.accept_submission_identity(UiSubmission {
            submission_id: "active".into(),
            intent: InputIntent::Command,
        });
        queue.checkpoint_cwd("/mnt/project/subdir".into());
        queue.push_outer_submission(input("first", InputIntent::Command));
        queue.push_outer_submission(input("second", InputIntent::Agent));
        queue.persist().await.unwrap();
        let source = machine.rollout_path().unwrap().clone();
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&source, "/proc/2", "model", dir.path())
                .await
                .unwrap();
        let restored = recovered.input_broker();
        assert_eq!(
            restored.recovered_cwd().unwrap(),
            std::path::Path::new("/mnt/project/subdir")
        );
        assert!(restored.is_paused());
        assert!(restored.pop_outer().is_none());
        assert!(restored.activity_snapshot().active_submission.is_none());
        assert_eq!(restored.unknown_inputs()[0].submission_id, "active");
        let pending = restored.activity_snapshot().pending_submissions;
        assert_eq!(
            pending
                .iter()
                .map(|i| i.submission_id.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert_eq!(pending[0].intent, InputIntent::Command);
        assert_eq!(pending[1].intent, InputIntent::Agent);
        restored.continue_queue().unwrap();
        let Some(QueuedRuntimeItem::Submission(first)) = restored.pop_outer() else {
            panic!("missing recovered command")
        };
        assert_eq!(first.id, "first");
        assert_eq!(
            alan_agent_protocol::parts_to_text(match &first.op {
                Op::Input { parts, .. } => parts,
                _ => unreachable!(),
            }),
            "identical body"
        );
        assert!(!restored.interrupt("second").unwrap());
        restored.discard_queue().unwrap();
        restored.persist().await.unwrap();
        let again = AgentMachine::load_from_rollout_in_dir(
            recovered.rollout_path().unwrap(),
            "/proc/3",
            "model",
            dir.path(),
        )
        .await
        .unwrap();
        assert!(
            again
                .input_broker()
                .activity_snapshot()
                .pending_submissions
                .is_empty()
        );
        assert!(again.input_broker().unknown_inputs().is_empty());
        assert!(!again.input_broker().is_paused());
    }

    #[tokio::test]
    async fn missing_queue_records_remain_visible_across_recovery_until_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let machine = AgentMachine::new_with_recorder_options(
            "/proc/1",
            "model",
            Some(dir.path()),
            None,
            None,
        )
        .await
        .unwrap();
        let recovered = AgentMachine::load_from_rollout_in_dir(
            machine.rollout_path().unwrap(),
            "/proc/2",
            "model",
            dir.path(),
        )
        .await
        .unwrap();
        let again = AgentMachine::load_from_rollout_in_dir(
            recovered.rollout_path().unwrap(),
            "/proc/3",
            "model",
            dir.path(),
        )
        .await
        .unwrap();
        assert!(again.input_broker().is_paused());
        assert!(
            again
                .input_broker()
                .recovery_warning()
                .unwrap()
                .contains("missing")
        );
        again.input_broker().continue_queue().unwrap();
        assert!(again.input_broker().recovery_warning().is_none());
    }

    #[tokio::test]
    async fn pending_recovery_without_cwd_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let machine = AgentMachine::new_with_recorder_options(
            "/proc/1",
            "model",
            Some(dir.path()),
            None,
            None,
        )
        .await
        .unwrap();
        let queue = machine.input_broker();
        queue.push_outer_submission(input("pending", InputIntent::Command));
        queue.persist().await.unwrap();
        let error = AgentMachine::load_from_rollout_in_dir(
            machine.rollout_path().unwrap(),
            "/proc/2",
            "model",
            dir.path(),
        )
        .await
        .unwrap_err();
        assert!(format!("{error:#}").contains("missing Process cwd"));
    }

    #[test]
    fn corrupt_queue_checkpoint_is_not_silently_ignored() {
        let queue = TurnInputBroker::default();
        let event = EventRecord {
            event_type: EVENT.into(),
            payload: serde_json::json!({"version":99}),
            timestamp: String::new(),
        };
        assert!(queue.restore_from_events(&[event]).is_err());
    }
}
