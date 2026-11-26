use alan_agent_protocol::{ConfinedTrustPosture, Op, Submission};
use anyhow::Result;
use tracing::warn;

use super::super::transition::{NamespaceAgentFiles, RuntimeLoopState};
use super::super::{queue_publication, task_budget, turn_support, ui_surfaces};
use super::{QueuedRuntimeItem, RuntimeSubmissionQueues, best_effort_durability_warning};
use crate::policy::TrustPosture;

impl RuntimeSubmissionQueues {
    /// Handle settled session-level controls (`ResetSession` and `SetTrustPosture`).
    ///
    /// Must run before input queue admission so controls work cleanly even while
    /// the queue is paused or a previous turn left a pending interaction.
    pub(super) async fn handle_session_controls(
        &mut self,
        state: &mut RuntimeLoopState,
        submission: &Submission,
        rollouts_dir: Option<&std::path::Path>,
        rollout_cwd: Option<&std::path::Path>,
        model_name: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) -> bool {
        match &submission.op {
            Op::SetTrustPosture {
                posture,
                confined_trusted,
            } => {
                let files = state.agent_files();
                let current = state.runtime_config.trust_posture;
                if current == TrustPosture::HostAccountTrusted {
                    if let Err(err) = ui_surfaces::warning(
                        &files,
                        "Trust posture HostAccountTrusted cannot be modified via runtime control.",
                    )
                    .await
                    {
                        warn!(error = %err, "Failed to emit trust posture refusal notice");
                    }
                    return true;
                }
                let next = match ConfinedTrustPosture::resolve(*posture, *confined_trusted) {
                    ConfinedTrustPosture::Governed => TrustPosture::Governed,
                    ConfinedTrustPosture::ConfinedSmart => TrustPosture::ConfinedSmart,
                    ConfinedTrustPosture::ConfinedTrusted => TrustPosture::ConfinedTrusted,
                };
                let is_downgrade = next != current && next.is_equal_or_narrower_than(current);
                state.runtime_config.trust_posture = next;
                state.environment.set_trust_posture(next);
                if is_downgrade {
                    state.machine.invalidate_approvals_on_posture_downgrade();
                    state.environment.bump_posture_authority_revision();
                }
                let message = match next {
                    TrustPosture::ConfinedTrusted => {
                        "Trust posture set to ConfinedTrusted (confined trust enabled; OS sandbox active)."
                    }
                    TrustPosture::ConfinedSmart => {
                        "Trust posture set to ConfinedSmart (adaptive confined smart review enabled)."
                    }
                    TrustPosture::Governed => {
                        "Trust posture set to Governed (confirmation prompts enabled)."
                    }
                    TrustPosture::HostAccountTrusted => {
                        unreachable!()
                    }
                };
                if let Err(err) = ui_surfaces::warning(&files, message).await {
                    warn!(error = %err, "Failed to emit trust posture update notice");
                }
                let configured_profile = if next == TrustPosture::ConfinedSmart {
                    state
                        .runtime_config
                        .smart_evaluator_profile
                        .clone()
                        .or_else(|| state.runtime_config.guardian_profile.clone())
                } else {
                    None
                };
                let system1_snapshot =
                    state
                        .machine
                        .set_system1_evaluator_metadata(configured_profile, None, None);
                super::super::smart_approval::publish_system1_snapshot(&files, &system1_snapshot)
                    .await;
                task_budget::publish_state(state).await;
                true
            }
            Op::ResetSession => {
                self.execute_reset_session(
                    state,
                    rollouts_dir,
                    rollout_cwd,
                    model_name,
                    reasoning_effort,
                )
                .await;
                true
            }
            _ => false,
        }
    }

    async fn execute_reset_session(
        &mut self,
        state: &mut RuntimeLoopState,
        rollouts_dir: Option<&std::path::Path>,
        rollout_cwd: Option<&std::path::Path>,
        model_name: &str,
        reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) {
        if let Err(err) = state
            .environment
            .async_workstreams()
            .quiesce_for_reset(std::time::Duration::from_secs(5))
            .await
        {
            warn!(error = %format!("{err:#}"), "Failed to quiesce active async workstreams before session reset");
        }
        let files = state.agent_files();
        let host_mount_requests = state.environment.host_mount_requests();
        if let Err(err) = turn_support::reset_turn_after_cancelling_host_mounts(
            &mut state.machine,
            &files,
            &host_mount_requests,
        )
        .await
        {
            warn!(error = %format!("{err:#}"), "Failed to cancel pending requests before session reset");
        }

        if let Some(environment) = &self.environment {
            let cleared = {
                let mut bindings = environment.model_bindings.lock().await;
                if let Some(authority) = &bindings.authority
                    && matches!(authority.capture_initial().await, Ok(None))
                {
                    bindings.confirmed = None;
                    bindings.captured.clear();
                    true
                } else {
                    false
                }
            };
            if cleared {
                self.outer_queue
                    .lock()
                    .expect("input queue poisoned")
                    .confirmed_binding = None;
            }
        }

        let effective_model = self
            .outer_queue
            .lock()
            .expect("input queue poisoned")
            .confirmed_binding
            .as_ref()
            .map(|binding| binding.callable_binding.model.clone())
            .unwrap_or_else(|| model_name.to_owned());

        let old_recorder = self.recorder.take();
        self.reset_queue_after_session_reset(&files, old_recorder.as_ref())
            .await;
        state.prompt_cache.reset_memory_bootstrap();
        state.runtime_config.ephemeral_context = true;
        state.runtime_config.project_startup_context = None;
        state.prompt_cache.set_project_ledger_section(None);

        match state
            .machine
            .reset_session(
                &state.process_path(),
                &effective_model,
                rollouts_dir,
                rollout_cwd,
                reasoning_effort,
            )
            .await
        {
            Ok(next_recorder) => {
                self.recorder = next_recorder;
            }
            Err(err) => {
                self.recorder = None;
                let message = if state.runtime_config.durability_required {
                    format!("Session reset without persistent rollout: {err:#}")
                } else {
                    best_effort_durability_warning(&err)
                };
                if let Err(publish_err) = ui_surfaces::warning(&files, message).await {
                    warn!(error = %publish_err, "Failed to emit session reset durability warning");
                }
            }
        }

        if let Err(err) = task_budget::publish_wait(state).await {
            warn!(error = %format!("{err:#}"), "Failed to publish task budget wait after session reset");
        }
        if let Err(err) = task_budget::publish_inherited_wait(state).await {
            warn!(error = %format!("{err:#}"), "Failed to publish inherited budget wait after session reset");
        }
        if let Err(err) = ui_surfaces::plan_updated(&files, None, Vec::new()).await {
            warn!(error = %err, "Failed to clear plan snapshot after session reset");
        }
        if let Err(err) = ui_surfaces::turn_completed(&files, false).await {
            warn!(error = %err, "Failed to publish idle activity after session reset");
        }
        if let Err(err) = ui_surfaces::warning(&files, "Started a fresh session.").await {
            warn!(error = %err, "Failed to emit fresh session notice");
        }
        let system1_snapshot = state.machine.system1_snapshot();
        super::super::smart_approval::publish_system1_snapshot(&files, &system1_snapshot).await;
        task_budget::publish_state(state).await;
        self.observe_models().await;
    }

    async fn reset_queue_after_session_reset(
        &mut self,
        files: &NamespaceAgentFiles,
        old_recorder: Option<&crate::rollout::RolloutRecorder>,
    ) {
        let mut discarded_ids = Vec::new();
        {
            let mut queue = self.outer_queue.lock().expect("input queue poisoned");
            let mut kept_deferred = std::collections::VecDeque::new();
            while let Some(item) = queue.pending.pop_front() {
                match item {
                    QueuedRuntimeItem::Submission(sub) => {
                        discarded_ids.push(sub.id);
                    }
                    deferred @ QueuedRuntimeItem::Deferred(_) => {
                        kept_deferred.push_back(deferred);
                    }
                }
            }
            queue.pending = kept_deferred;
            while let Some(sub) = queue.inband.pop_front() {
                discarded_ids.push(sub.id);
            }
            while let Some(sub) = queue.buffered_inband_submissions.pop_front() {
                discarded_ids.push(sub.id);
            }
            queue.queued_next_turn_inputs.clear();
            queue.active_submission_ids.clear();
            queue.active_cancel_requested = false;
            queue.paused = false;
            queue.recovered = false;
            queue.queue_uncertain_ids.clear();
            queue.pending_binding_rejections.clear();
            for id in &discarded_ids {
                queue.settled_ids.insert(id.clone());
                queue.bindings.remove(id);
            }
        }

        if !discarded_ids.is_empty() {
            if let Err(err) = crate::agent_machine::input_queue::persist_input_removals(
                old_recorder,
                &discarded_ids,
            )
            .await
            {
                warn!(error = %format!("{err:#}"), "Failed to persist discarded queue removals on session reset");
            }
            for id in &discarded_ids {
                let _: Result<()> = super::queue_controls::publish_cancelled_input(
                    files,
                    id,
                    "Discarded by session reset.",
                )
                .await;
            }
        }

        queue_publication::observe(&self.outer_queue).await;
    }
}
