#[path = "supervisor_helpers.rs"]
mod helpers;

use std::time::{Duration, Instant};

use alan_agent_protocol::{AgentExecutableResult, AgentExecutableStatus, YieldKind};
use anyhow::{Context, Result, bail};
use tokio_util::sync::CancellationToken;

use super::{ChildRuntimePause, ChildRuntimeResult, ChildRuntimeStatus};
use crate::runtime::child_runs::ChildRunRegistry;
use crate::runtime::transition::{
    ChildEventWake, ChildEventWatchers, NamespaceAgentFiles, NamespaceProcessFiles,
};
use crate::runtime::{ChildRunStatus, ChildRunTerminationMode};
use helpers::{
    agent_result_matches_exit_code, child_run_status_for_runtime_status, file_terminal_observation,
    inherited_pause_from, invalid_agent_result_observation, parse_child_structured_output,
    push_bounded_child_warning, read_latest_assistant_text_from_rollout, terminated_observation,
    timed_out_observation,
};

/// Inputs retained after a delegated Child Run has been launched and registered.
pub(crate) struct DelegatedChildRunSupervision {
    pub(crate) startup: ChildProcessStartup,
    pub(crate) child_run_id: String,
    pub(crate) child_run_registry: ChildRunRegistry,
    pub(crate) timeout: Option<Duration>,
    pub(crate) agent_files: NamespaceAgentFiles,
    pub(crate) process_files: NamespaceProcessFiles,
    pub(crate) process_pid: String,
    /// The durable stop this launch was started to answer, when it is a reconnect.
    pub(crate) resume_target: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ChildProcessStartup {
    pub(crate) process_path: String,
    pub(crate) rollout_path: Option<std::path::PathBuf>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug)]
pub(super) struct ObservedChildTerminalEvent {
    pub(super) output_text: String,
    pub(super) turn_summary: Option<String>,
    pub(super) structured_output: Option<serde_json::Value>,
    pub(super) warnings: Vec<String>,
    pub(super) error_message: Option<String>,
    pub(super) pause: Option<ChildRuntimePause>,
    pub(super) status: ChildRuntimeStatus,
}

enum ChildRuntimeWaitOutcome {
    Observed(ObservedChildTerminalEvent),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildFileObservation {
    process_exited: bool,
    process_exit_code: Option<i32>,
    output_text: String,
    process_output_offset: u64,
    process_io_events_offset: u64,
    request_ids: Vec<String>,
    pending_request_id: Option<String>,
    request_events_offset: u64,
    action_ids: Vec<String>,
    action_events_offset: u64,
    ui_events_offset: u64,
    terminal_error: Option<String>,
    process_result: Option<AgentExecutableResult>,
    activity: alan_agent_protocol::UiActivitySnapshot,
    notice: alan_agent_protocol::UiNoticeSnapshot,
}

/// Supervises one delegated Child Run from authoritative Process and AgentFS files.
pub(crate) struct DelegatedChildRunSupervisor {
    startup: ChildProcessStartup,
    child_run_id: String,
    child_run_registry: ChildRunRegistry,
    timeout: Option<Duration>,
    agent_files: NamespaceAgentFiles,
    process_files: NamespaceProcessFiles,
    process_pid: String,
    resume_target: Option<String>,
}

impl DelegatedChildRunSupervisor {
    pub(crate) fn new(input: DelegatedChildRunSupervision) -> Self {
        Self {
            startup: input.startup,
            child_run_id: input.child_run_id,
            child_run_registry: input.child_run_registry,
            timeout: input.timeout,
            agent_files: input.agent_files,
            process_files: input.process_files,
            process_pid: input.process_pid,
            resume_target: input.resume_target,
        }
    }

    pub(crate) fn process_pid(&self) -> &str {
        &self.process_pid
    }

    pub(crate) fn child_run_id(&self) -> &str {
        &self.child_run_id
    }

    pub(crate) fn agent_files(&self) -> NamespaceAgentFiles {
        self.agent_files.clone()
    }

    pub(crate) fn process_files(&self) -> NamespaceProcessFiles {
        self.process_files.clone()
    }

    /// A resumed Process republishes the stop it is about to answer. That pause is
    /// not this run's outcome: the answer is already on its way, and its own run
    /// timeout bounds the wait.
    fn awaits_resume_answer(&self, checkpoint_id: Option<&str>, process_exited: bool) -> bool {
        // A live Process can still apply the answer; one that exited already
        // published its outcome, and waiting on it would never end.
        if process_exited {
            return false;
        }
        matches!(
            (self.resume_target.as_deref(), checkpoint_id),
            (Some(target), Some(checkpoint)) if target == checkpoint
        )
    }

    async fn observe_files(&self) -> Result<ChildFileObservation> {
        let process_files = &self.process_files;
        let agent_files = &self.agent_files;
        let pid = self.process_pid.as_str();
        let timeout = Duration::from_secs(1);
        if let Some(exit_code) = process_files.read_process_exit_code(pid).await? {
            return self.observe_exited_process(exit_code).await;
        }

        let live_observation: Result<ChildFileObservation> = async {
            let (process_output_offset, process_io_events_offset) =
                process_files.read_process_io_offsets(pid).await?;
            let activity = tokio::time::timeout(timeout, agent_files.read_ui_activity_snapshot())
                .await
                .context("observe child activity timed out")??;
            let output_text = tokio::time::timeout(timeout, agent_files.read_assistant_output())
                .await
                .context("observe child output timed out")??;
            let ui_events_offset = tokio::time::timeout(timeout, agent_files.ui_events_offset())
                .await
                .context("observe child UI events offset timed out")??;
            let notice = tokio::time::timeout(timeout, agent_files.read_ui_notice_snapshot())
                .await
                .context("observe child notice timed out")??;
            let request_ids = tokio::time::timeout(timeout, agent_files.request_ids())
                .await
                .context("observe child requests timed out")??;
            let pending_request_id =
                tokio::time::timeout(timeout, agent_files.pending_request_id(&request_ids))
                    .await
                    .context("observe child pending request timed out")??;
            let request_events_offset =
                tokio::time::timeout(timeout, agent_files.request_events_offset())
                    .await
                    .context("observe child request stream offset timed out")??;
            let action_ids = tokio::time::timeout(timeout, agent_files.action_ids())
                .await
                .context("observe child actions timed out")??;
            let action_events_offset =
                tokio::time::timeout(timeout, agent_files.action_events_offset())
                    .await
                    .context("observe child action stream offset timed out")??;
            Ok(ChildFileObservation {
                process_exited: false,
                process_exit_code: None,
                output_text,
                process_output_offset,
                process_io_events_offset,
                request_ids,
                pending_request_id,
                request_events_offset,
                action_ids,
                action_events_offset,
                ui_events_offset,
                terminal_error: if notice.kind == alan_agent_protocol::UiNoticeKind::Error {
                    Some(notice.message.clone())
                } else {
                    None
                },
                process_result: None,
                activity,
                notice,
            })
        }
        .await;

        match live_observation {
            Ok(observation) => Ok(observation),
            Err(error) => match process_files.read_process_exit_code(pid).await? {
                Some(exit_code) => self.observe_exited_process(exit_code).await,
                None => Err(error),
            },
        }
    }

    async fn observe_exited_process(&self, exit_code: i32) -> Result<ChildFileObservation> {
        let process_files = &self.process_files;
        let pid = self.process_pid.as_str();
        let (process_output_offset, process_io_events_offset) =
            process_files.read_process_io_offsets(pid).await?;
        let process_output = process_files.read_process_output(pid).await?;
        let process_result = AgentExecutableResult::from_process_output(&process_output)
            .ok()
            .filter(|result| agent_result_matches_exit_code(Some(exit_code), result));
        let activity = match process_result.as_ref().map(|result| result.status) {
            Some(AgentExecutableStatus::Paused) => {
                alan_agent_protocol::UiActivitySnapshot::paused(None)
            }
            _ => alan_agent_protocol::UiActivitySnapshot::idle(),
        };
        let notice = process_result
            .as_ref()
            .and_then(|result| result.error_message.as_ref())
            .map(|message| {
                alan_agent_protocol::UiNoticeSnapshot::new(
                    alan_agent_protocol::UiNoticeKind::Error,
                    message.clone(),
                )
            })
            .unwrap_or_default();
        let output_text = process_result
            .as_ref()
            .map(|result| result.output_text.clone())
            .unwrap_or_else(|| String::from_utf8_lossy(&process_output).into_owned());
        let pending_request_id = process_result
            .as_ref()
            .and_then(|result| result.pause.as_ref())
            .map(|pause| pause.request_id.clone());
        let request_ids = pending_request_id.iter().cloned().collect();
        let terminal_error = process_result
            .as_ref()
            .and_then(|result| result.error_message.clone());
        Ok(ChildFileObservation {
            process_exited: true,
            process_exit_code: Some(exit_code),
            output_text,
            process_output_offset,
            process_io_events_offset,
            request_ids,
            pending_request_id,
            request_events_offset: 0,
            action_ids: Vec::new(),
            action_events_offset: 0,
            ui_events_offset: 0,
            terminal_error,
            process_result,
            activity,
            notice,
        })
    }

    pub(crate) async fn join_until_cancelled(
        mut self,
        cancel: &CancellationToken,
    ) -> Result<ChildRuntimeResult> {
        match self.wait_for_terminal_event(Some(cancel)).await? {
            ChildRuntimeWaitOutcome::Observed(observed) => {
                self.finish_after_observed_terminal_event(observed).await
            }
            ChildRuntimeWaitOutcome::Cancelled => Ok(self.cancelled_result()),
        }
    }

    async fn finish_after_observed_terminal_event(
        &mut self,
        observed: ObservedChildTerminalEvent,
    ) -> Result<ChildRuntimeResult> {
        let mut warnings = Vec::new();
        for warning in self
            .startup
            .warnings
            .iter()
            .cloned()
            .chain(observed.warnings)
        {
            push_bounded_child_warning(&mut warnings, warning);
        }
        // A paused child may stay alive waiting for an answer; only await
        // `/proc/<pid>/wait` exit for terminal outcomes.
        if observed.status != ChildRuntimeStatus::Paused {
            self.await_process_terminal().await;
        }
        let output_text = observed.output_text;
        let rollout_fallback_text = if output_text.trim().is_empty() {
            read_latest_assistant_text_from_rollout(self.startup.rollout_path.as_deref()).await
        } else {
            None
        };
        let output_text = if output_text.trim().is_empty() {
            rollout_fallback_text.unwrap_or(output_text)
        } else {
            output_text
        };
        let structured_output = observed
            .structured_output
            .or_else(|| parse_child_structured_output(output_text.as_str()));
        let child_status = child_run_status_for_runtime_status(observed.status.clone());
        self.child_run_registry.mark_terminal(
            &self.child_run_id,
            child_status,
            observed.error_message.clone(),
        );

        Ok(ChildRuntimeResult {
            status: observed.status,
            process_path: self.startup.process_path.clone(),
            child_run_id: Some(self.child_run_id.clone()),
            rollout_path: self.startup.rollout_path.clone(),
            output_text,
            turn_summary: observed.turn_summary,
            structured_output,
            warnings,
            error_message: observed.error_message,
            pause: observed.pause,
            child_run: self.child_run_registry.get(&self.child_run_id),
        })
    }

    fn cancelled_result(&self) -> ChildRuntimeResult {
        self.child_run_registry
            .mark_terminal(&self.child_run_id, ChildRunStatus::Cancelled, None);
        let mut warnings = Vec::new();
        for warning in self.startup.warnings.iter().cloned() {
            push_bounded_child_warning(&mut warnings, warning);
        }
        ChildRuntimeResult {
            status: ChildRuntimeStatus::Cancelled,
            process_path: self.startup.process_path.clone(),
            child_run_id: Some(self.child_run_id.clone()),
            rollout_path: self.startup.rollout_path.clone(),
            output_text: String::new(),
            turn_summary: None,
            structured_output: None,
            warnings,
            error_message: None,
            pause: None,
            child_run: self.child_run_registry.get(&self.child_run_id),
        }
    }

    async fn wait_for_terminal_event(
        &mut self,
        cancel: Option<&CancellationToken>,
    ) -> Result<ChildRuntimeWaitOutcome> {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            self.terminate_runtime().await;
            return Ok(ChildRuntimeWaitOutcome::Cancelled);
        }

        let mut watchers =
            ChildEventWatchers::spawn(&self.process_files, &self.agent_files, &self.process_pid);
        let mut output_text = String::new();
        let mut warnings = Vec::new();
        let mut latest_liveness_at = Instant::now();
        let started_at = Instant::now();
        let wall_clock_cap = self.timeout.map(|timeout| timeout.saturating_mul(4));
        let mut last_file_observation = None;
        let mut streams_closed_observed = false;

        loop {
            watchers.mark_events_seen();
            let observation = self.observe_files().await?;
            if last_file_observation.as_ref() != Some(&observation) {
                latest_liveness_at = Instant::now();
                self.child_run_registry.observe_progress(
                    &self.child_run_id,
                    "agentfs",
                    Some(format!(
                        "process={:?} exit={:?} activity={:?} output={} output_offset={} io_offset={} requests={} request_offset={} actions={} action_offset={} ui_offset={}",
                        if observation.process_exited { "exited" } else { "running" },
                        observation.process_exit_code,
                        observation.activity.state,
                        observation.output_text.len(),
                        observation.process_output_offset,
                        observation.process_io_events_offset,
                        observation.request_ids.len(),
                        observation.request_events_offset,
                        observation.action_ids.len(),
                        observation.action_events_offset,
                        observation.ui_events_offset,
                    )),
                );
                if last_file_observation
                    .as_ref()
                    .is_none_or(|previous: &ChildFileObservation| {
                        previous.notice != observation.notice
                    })
                    && observation.notice.kind == alan_agent_protocol::UiNoticeKind::Warning
                    && !observation.notice.message.is_empty()
                {
                    push_bounded_child_warning(&mut warnings, observation.notice.message.clone());
                }
            }
            output_text.clone_from(&observation.output_text);

            if let Some(outcome) = self
                .evaluate_observation(&observation, &mut warnings)
                .await?
            {
                return Ok(outcome);
            }
            last_file_observation = Some(observation);

            let term_notified = self.child_run_registry.termination_notified();
            tokio::pin!(term_notified);
            term_notified.as_mut().enable();

            if let Some(request) = self
                .child_run_registry
                .termination_request(&self.child_run_id)
            {
                match request.mode {
                    ChildRunTerminationMode::Graceful | ChildRunTerminationMode::Forceful => {
                        self.terminate_process_and_reconcile().await
                    }
                }
                return Ok(ChildRuntimeWaitOutcome::Observed(terminated_observation(
                    request,
                    &output_text,
                    &warnings,
                )));
            }

            if let Some(cap) = wall_clock_cap
                && started_at.elapsed() >= cap
            {
                self.terminate_process_and_reconcile().await;
                return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                    "Delegated Child Run wall-clock cap exceeded",
                    &output_text,
                    &warnings,
                )));
            }

            let next_timeout = self.timeout.map(|timeout| {
                let idle_remaining =
                    (latest_liveness_at + timeout).saturating_duration_since(Instant::now());
                let cap_remaining = wall_clock_cap
                    .map(|cap| (started_at + cap).saturating_duration_since(Instant::now()));
                match cap_remaining {
                    Some(cap_rem) if cap_rem <= idle_remaining => (cap_rem, true),
                    _ => (idle_remaining, false),
                }
            });
            if let Some((remaining, is_cap)) = next_timeout
                && remaining.is_zero()
            {
                self.terminate_process_and_reconcile().await;
                let message = if is_cap {
                    "Delegated Child Run wall-clock cap exceeded"
                } else {
                    "Delegated Child Run idle timed out"
                };
                return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                    message,
                    &output_text,
                    &warnings,
                )));
            }

            let timeout_deadline = next_timeout.map(|(rem, _)| tokio::time::Instant::now() + rem);
            let timeout_is_cap = next_timeout.is_some_and(|(_, is_cap)| is_cap);

            tokio::select! {
                biased;
                _ = async {
                    match cancel {
                        Some(token) => token.cancelled().await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.terminate_runtime().await;
                    return Ok(ChildRuntimeWaitOutcome::Cancelled);
                }
                _ = &mut term_notified => continue,
                _ = async {
                    match timeout_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {
                    self.terminate_process_and_reconcile().await;
                    let message = if timeout_is_cap {
                        "Delegated Child Run wall-clock cap exceeded"
                    } else {
                        "Delegated Child Run idle timed out"
                    };
                    return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                        message,
                        &output_text,
                        &warnings,
                    )));
                }
                wake = watchers.next_wake(), if watchers.has_open_stream() => {
                    match wake {
                        ChildEventWake::ProcessExited(_) | ChildEventWake::AgentEvents => continue,
                        ChildEventWake::StreamsClosed => {
                            if streams_closed_observed {
                                bail!(
                                    "Child Agent Process {} event and wait streams closed before terminal state",
                                    self.process_pid
                                );
                            }
                            streams_closed_observed = true;
                            continue;
                        }
                    }
                }
            }
        }
    }

    async fn evaluate_observation(
        &self,
        observation: &ChildFileObservation,
        warnings: &mut Vec<String>,
    ) -> Result<Option<ChildRuntimeWaitOutcome>> {
        if let Some(result) = observation.process_result.as_ref() {
            for warning in &result.warnings {
                push_bounded_child_warning(warnings, warning.clone());
            }
            // A paused child whose request is an inherited stop must keep
            // its ancestor identity, or the parent degrades it back to the
            // old uncontinuable pause.
            let pause_inherited = match result.pause.as_ref() {
                Some(pause)
                    if matches!(&pause.kind, YieldKind::Custom(kind)
                        if kind == crate::approval::INHERITED_BUDGET_CHECKPOINT_TYPE) =>
                {
                    let details = match pause.payload.clone() {
                        Some(details) => Some(details),
                        None => {
                            self.agent_files
                                .read_request_details(&pause.request_id)
                                .await?
                        }
                    };
                    details.map(|details| inherited_pause_from(&details))
                }
                _ => None,
            };
            let (status, error_message, pause) = match result.status {
                AgentExecutableStatus::Completed => (ChildRuntimeStatus::Completed, None, None),
                AgentExecutableStatus::Paused => (
                    ChildRuntimeStatus::Paused,
                    None,
                    result.pause.as_ref().map(|pause| ChildRuntimePause {
                        request_id: pause.request_id.clone(),
                        kind: pause.kind.clone(),
                        inherited: pause_inherited.clone(),
                    }),
                ),
                AgentExecutableStatus::Failed => (
                    ChildRuntimeStatus::Failed,
                    result
                        .error_message
                        .clone()
                        .or_else(|| Some("Child Agent Process reported failure".to_string())),
                    None,
                ),
            };
            let observed = pause
                .as_ref()
                .and_then(|pause| pause.inherited.as_ref())
                .map(|inherited| inherited.checkpoint_id.clone());
            if self.awaits_resume_answer(observed.as_deref(), observation.process_exited) {
                return Ok(None);
            }
            return Ok(Some(ChildRuntimeWaitOutcome::Observed(
                file_terminal_observation(
                    result.output_text.clone(),
                    std::mem::take(warnings),
                    status,
                    error_message,
                    pause,
                ),
            )));
        }

        if observation.process_exited {
            let exit_code = observation.process_exit_code.unwrap_or(1);
            if exit_code == 130 {
                return Ok(Some(ChildRuntimeWaitOutcome::Observed(
                    self.externally_stopped_observed_event(&observation.output_text, warnings),
                )));
            }
            return Ok(Some(ChildRuntimeWaitOutcome::Observed(
                invalid_agent_result_observation(
                    exit_code,
                    observation.output_text.clone(),
                    std::mem::take(warnings),
                ),
            )));
        }

        if observation.activity.state == alan_agent_protocol::UiActivityState::Paused
            && let Some(request_id) = observation.pending_request_id.as_ref()
        {
            let kind = self.agent_files.read_request_kind(request_id).await?;
            let kind = match kind.as_str() {
                "confirmation" => YieldKind::Confirmation,
                "structured_input" => YieldKind::StructuredInput,
                other => YieldKind::Custom(other.to_string()),
            };
            let inherited = if matches!(
                &kind,
                YieldKind::Custom(name)
                    if name == crate::approval::INHERITED_BUDGET_CHECKPOINT_TYPE
            ) {
                self.agent_files
                    .read_request_details(request_id)
                    .await?
                    .map(|details| inherited_pause_from(&details))
            } else {
                None
            };
            let observed = inherited
                .as_ref()
                .map(|inherited| inherited.checkpoint_id.clone());
            if self.awaits_resume_answer(observed.as_deref(), observation.process_exited) {
                return Ok(None);
            }
            return Ok(Some(ChildRuntimeWaitOutcome::Observed(
                file_terminal_observation(
                    observation.output_text.clone(),
                    std::mem::take(warnings),
                    ChildRuntimeStatus::Paused,
                    None,
                    Some(ChildRuntimePause {
                        request_id: request_id.clone(),
                        kind,
                        inherited,
                    }),
                ),
            )));
        }

        if observation.activity.state == alan_agent_protocol::UiActivityState::Idle
            && observation.ui_events_offset > 0
        {
            let status = if observation.terminal_error.is_some() {
                ChildRuntimeStatus::Failed
            } else {
                ChildRuntimeStatus::Completed
            };
            return Ok(Some(ChildRuntimeWaitOutcome::Observed(
                file_terminal_observation(
                    observation.output_text.clone(),
                    std::mem::take(warnings),
                    status,
                    observation.terminal_error.clone(),
                    None,
                ),
            )));
        }

        Ok(None)
    }

    async fn terminate_runtime(&self) {
        self.terminate_process_and_reconcile().await;
    }

    async fn terminate_process_and_reconcile(&self) {
        let process_files = &self.process_files;
        let pid = self.process_pid.as_str();
        if let Ok(Some(exit_code)) = process_files.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
            return;
        }
        let _ = process_files
            .write_process_control_for_pid(pid, "cancel")
            .await;
        if let Ok(Some(exit_code)) = process_files.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    async fn await_process_terminal(&self) {
        let _ = tokio::time::timeout(
            Duration::from_secs(5),
            self.process_files.wait_for_process_exit(&self.process_pid),
        )
        .await;
        self.reconcile_exited_process().await;
    }

    async fn reconcile_exited_process(&self) {
        let process_files = &self.process_files;
        let pid = self.process_pid.as_str();
        if let Ok(Some(exit_code)) = process_files.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    fn externally_stopped_observed_event(
        &self,
        output_text: &str,
        warnings: &[String],
    ) -> ObservedChildTerminalEvent {
        ObservedChildTerminalEvent {
            output_text: output_text.to_string(),
            turn_summary: None,
            structured_output: parse_child_structured_output(output_text),
            warnings: warnings.to_vec(),
            error_message: Some(
                "Delegated Child Run terminated through external /proc/<pid>/ctl process control"
                    .to_string(),
            ),
            pause: None,
            status: ChildRuntimeStatus::Terminated,
        }
    }
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
