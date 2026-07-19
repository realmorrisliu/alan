use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alan_agent_protocol::YieldKind;
use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;

use super::{ChildRuntimePause, ChildRuntimeResult, ChildRuntimeStatus};
use crate::runtime::child_runs::ChildRunRegistry;
use crate::runtime::transition::NamespaceAgentFiles;
use crate::runtime::{
    AgentProcessLifecycle, ChildRunStatus, ChildRunTerminationMode, ChildRunTerminationRequest,
    NamespaceRuntimeEnvironment, RuntimeController, RuntimeStartupMetadata,
};

const MAX_OBSERVED_CHILD_WARNINGS: usize = 32;
const MAX_OBSERVED_CHILD_WARNING_CHARS: usize = 512;
const MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Inputs retained after a delegated Child Run has been launched and registered.
pub(crate) struct DelegatedChildRunSupervision {
    pub(crate) runtime: Option<RuntimeController>,
    pub(crate) startup_metadata: RuntimeStartupMetadata,
    pub(crate) child_run_id: String,
    pub(crate) child_run_registry: ChildRunRegistry,
    pub(crate) timeout: Option<Duration>,
    pub(crate) process_lifecycle: Arc<dyn AgentProcessLifecycle>,
    pub(crate) process_environment: NamespaceRuntimeEnvironment,
    pub(crate) agent_files: NamespaceAgentFiles,
    pub(crate) process_pid: String,
}

#[derive(Debug)]
struct ObservedChildTerminalEvent {
    output_text: String,
    turn_summary: Option<String>,
    structured_output: Option<serde_json::Value>,
    warnings: Vec<String>,
    error_message: Option<String>,
    pause: Option<ChildRuntimePause>,
    status: ChildRuntimeStatus,
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
    activity: alan_agent_protocol::UiActivitySnapshot,
    notice: alan_agent_protocol::UiNoticeSnapshot,
}

/// Supervises one delegated Child Run from authoritative Process and AgentFS files.
pub(crate) struct DelegatedChildRunSupervisor {
    runtime: Option<RuntimeController>,
    startup_metadata: RuntimeStartupMetadata,
    child_run_id: String,
    child_run_registry: ChildRunRegistry,
    timeout: Option<Duration>,
    process_lifecycle: Arc<dyn AgentProcessLifecycle>,
    process_environment: NamespaceRuntimeEnvironment,
    agent_files: NamespaceAgentFiles,
    process_pid: String,
}

impl DelegatedChildRunSupervisor {
    pub(crate) fn new(input: DelegatedChildRunSupervision) -> Self {
        Self {
            runtime: input.runtime,
            startup_metadata: input.startup_metadata,
            child_run_id: input.child_run_id,
            child_run_registry: input.child_run_registry,
            timeout: input.timeout,
            process_lifecycle: input.process_lifecycle,
            process_environment: input.process_environment,
            agent_files: input.agent_files,
            process_pid: input.process_pid,
        }
    }

    async fn observe_files(&self) -> Result<ChildFileObservation> {
        let process_environment = &self.process_environment;
        let agent_files = &self.agent_files;
        let pid = self.process_pid.as_str();
        let timeout = Duration::from_secs(1);
        let process_exit_code = process_environment.read_process_exit_code(pid).await?;
        let (process_output_offset, process_io_events_offset) =
            process_environment.read_process_io_offsets(pid).await?;
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
            process_exited: process_exit_code.is_some(),
            process_exit_code,
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
            activity,
            notice,
        })
    }

    #[cfg(test)]
    pub(crate) async fn join(mut self) -> Result<ChildRuntimeResult> {
        let observed = match self.wait_for_terminal_event(None).await? {
            ChildRuntimeWaitOutcome::Observed(observed) => observed,
            ChildRuntimeWaitOutcome::Cancelled => return Ok(self.cancelled_result()),
        };

        self.finish_after_observed_terminal_event(observed).await
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
            .startup_metadata
            .warnings
            .iter()
            .cloned()
            .chain(observed.warnings)
        {
            push_bounded_child_warning(&mut warnings, warning);
        }
        self.finish_runtime_and_process(&observed.status).await;
        let output_text = observed.output_text;
        let rollout_fallback_text = if output_text.trim().is_empty() {
            read_latest_assistant_text_from_rollout(self.startup_metadata.rollout_path.as_deref())
                .await
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
            process_path: self.startup_metadata.process_path.clone(),
            child_run_id: Some(self.child_run_id.clone()),
            rollout_path: self.startup_metadata.rollout_path.clone(),
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
        for warning in self.startup_metadata.warnings.iter().cloned() {
            push_bounded_child_warning(&mut warnings, warning);
        }
        ChildRuntimeResult {
            status: ChildRuntimeStatus::Cancelled,
            process_path: self.startup_metadata.process_path.clone(),
            child_run_id: Some(self.child_run_id.clone()),
            rollout_path: self.startup_metadata.rollout_path.clone(),
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

        let mut output_text = String::new();
        let mut warnings = Vec::new();
        let mut latest_liveness_at = Instant::now();
        let started_at = Instant::now();
        let wall_clock_cap = self.timeout.map(|timeout| timeout.saturating_mul(4));
        let file_poll_interval = self
            .timeout
            .map(|timeout| (timeout / 4).min(MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL))
            .unwrap_or(MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL)
            .max(Duration::from_millis(10));
        let mut last_file_observation = None;

        loop {
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
            if observation.process_exited {
                let exit_code = observation.process_exit_code.unwrap_or(1);
                if exit_code == 130 {
                    return Ok(ChildRuntimeWaitOutcome::Observed(
                        self.externally_stopped_observed_event(&observation.output_text, &warnings),
                    ));
                }
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    file_terminal_observation(
                        observation.output_text,
                        warnings,
                        if exit_code == 0 {
                            ChildRuntimeStatus::Completed
                        } else {
                            ChildRuntimeStatus::Failed
                        },
                        (exit_code != 0)
                            .then(|| format!("Child Agent Process exited with code {exit_code}")),
                        None,
                    ),
                ));
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
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    file_terminal_observation(
                        observation.output_text,
                        warnings,
                        ChildRuntimeStatus::Paused,
                        None,
                        Some(ChildRuntimePause {
                            request_id: request_id.clone(),
                            kind,
                        }),
                    ),
                ));
            }
            if observation.activity.state == alan_agent_protocol::UiActivityState::Idle
                && observation.ui_events_offset > 0
            {
                let status = if observation.terminal_error.is_some() {
                    ChildRuntimeStatus::Failed
                } else {
                    ChildRuntimeStatus::Completed
                };
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    file_terminal_observation(
                        observation.output_text,
                        warnings,
                        status,
                        observation.terminal_error,
                        None,
                    ),
                ));
            }
            last_file_observation = Some(observation);

            if let Some(request) = self
                .child_run_registry
                .termination_request(&self.child_run_id)
            {
                match request.mode {
                    ChildRunTerminationMode::Graceful => self.shutdown_runtime_task().await,
                    ChildRunTerminationMode::Forceful => self.abort_runtime_task().await,
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
                self.abort_runtime_task().await;
                return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                    "Delegated Child Run wall-clock cap exceeded",
                    &output_text,
                    &warnings,
                )));
            }

            if let Some(timeout) = self.timeout {
                let deadline = latest_liveness_at + timeout;
                let idle_remaining = deadline.saturating_duration_since(Instant::now());
                if let Some(cancel) = cancel {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            self.terminate_runtime().await;
                            return Ok(ChildRuntimeWaitOutcome::Cancelled);
                        }
                        _ = tokio::time::sleep(idle_remaining) => {
                            self.abort_runtime_task().await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                                "Delegated Child Run idle timed out",
                                &output_text,
                                &warnings,
                            )));
                        }
                        _ = tokio::time::sleep(file_poll_interval) => continue,
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(idle_remaining) => {
                            self.abort_runtime_task().await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(timed_out_observation(
                                "Delegated Child Run idle timed out",
                                &output_text,
                                &warnings,
                            )));
                        }
                        _ = tokio::time::sleep(file_poll_interval) => continue,
                    }
                }
            } else if let Some(cancel) = cancel {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        self.terminate_runtime().await;
                        return Ok(ChildRuntimeWaitOutcome::Cancelled);
                    }
                    _ = tokio::time::sleep(file_poll_interval) => continue,
                }
            } else {
                tokio::time::sleep(file_poll_interval).await;
            }
        }
    }

    async fn terminate_runtime(&mut self) {
        self.shutdown_runtime_task().await;
        self.terminate_process_and_reconcile().await;
    }

    async fn finish_runtime_and_process(&mut self, status: &ChildRuntimeStatus) {
        self.shutdown_runtime_task().await;
        self.process_lifecycle
            .finish(child_runtime_process_exit_code(status))
            .await;
        self.reconcile_exited_process().await;
    }

    async fn shutdown_runtime_task(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let _ = runtime.shutdown().await;
        }
    }

    async fn abort_runtime_task(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.abort().await;
        }
    }

    async fn terminate_process_and_reconcile(&self) {
        let environment = &self.process_environment;
        let pid = self.process_pid.as_str();
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.process_lifecycle.finish(exit_code).await;
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
            return;
        }
        let _ = environment
            .write_process_control_for_pid(pid, "cancel")
            .await;
        let exit_code = environment
            .read_process_exit_code(pid)
            .await
            .ok()
            .flatten()
            .unwrap_or(130);
        self.process_lifecycle.finish(exit_code).await;
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    async fn reconcile_exited_process(&self) {
        let environment = &self.process_environment;
        let pid = self.process_pid.as_str();
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
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

    #[cfg(test)]
    pub(crate) fn set_timeout_for_test(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    #[cfg(test)]
    pub(crate) fn process_environment_for_test(&self) -> &NamespaceRuntimeEnvironment {
        &self.process_environment
    }

    #[cfg(test)]
    pub(crate) fn process_pid_for_test(&self) -> &str {
        &self.process_pid
    }
}

fn push_bounded_child_warning(warnings: &mut Vec<String>, warning: String) {
    while warnings.len() >= MAX_OBSERVED_CHILD_WARNINGS {
        warnings.remove(0);
    }
    warnings.push(truncate_child_text_with_suffix(
        &warning,
        MAX_OBSERVED_CHILD_WARNING_CHARS,
        "...",
    ));
}

fn truncate_child_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let suffix_len = suffix.chars().count();
    if max_chars <= suffix_len {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_len))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}

fn parse_child_structured_output(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .or_else(|| parse_last_json_fenced_block(trimmed))
}

fn file_terminal_observation(
    output_text: String,
    warnings: Vec<String>,
    status: ChildRuntimeStatus,
    error_message: Option<String>,
    pause: Option<ChildRuntimePause>,
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        structured_output: parse_child_structured_output(&output_text),
        output_text,
        turn_summary: None,
        warnings,
        error_message,
        pause,
        status,
    }
}

fn timed_out_observation(
    message: &str,
    output_text: &str,
    warnings: &[String],
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        output_text: output_text.to_string(),
        turn_summary: None,
        structured_output: parse_child_structured_output(output_text),
        warnings: warnings.to_vec(),
        error_message: Some(message.to_string()),
        pause: None,
        status: ChildRuntimeStatus::TimedOut,
    }
}

fn terminated_observation(
    request: ChildRunTerminationRequest,
    output_text: &str,
    warnings: &[String],
) -> ObservedChildTerminalEvent {
    ObservedChildTerminalEvent {
        output_text: output_text.to_string(),
        turn_summary: None,
        structured_output: parse_child_structured_output(output_text),
        warnings: warnings.to_vec(),
        error_message: Some(format!(
            "Delegated Child Run terminated by {} with {:?} mode: {}",
            request.actor, request.mode, request.reason
        )),
        pause: None,
        status: ChildRuntimeStatus::Terminated,
    }
}

fn child_run_status_for_runtime_status(status: ChildRuntimeStatus) -> ChildRunStatus {
    match status {
        ChildRuntimeStatus::Completed => ChildRunStatus::Completed,
        ChildRuntimeStatus::Paused => ChildRunStatus::Failed,
        ChildRuntimeStatus::Cancelled => ChildRunStatus::Cancelled,
        ChildRuntimeStatus::TimedOut => ChildRunStatus::TimedOut,
        ChildRuntimeStatus::Terminated => ChildRunStatus::Terminated,
        ChildRuntimeStatus::Failed => ChildRunStatus::Failed,
    }
}

fn child_runtime_process_exit_code(status: &ChildRuntimeStatus) -> i32 {
    match status {
        ChildRuntimeStatus::Completed => 0,
        ChildRuntimeStatus::TimedOut => 124,
        ChildRuntimeStatus::Cancelled | ChildRuntimeStatus::Terminated => 130,
        ChildRuntimeStatus::Paused | ChildRuntimeStatus::Failed => 1,
    }
}

async fn read_latest_assistant_text_from_rollout(rollout_path: Option<&Path>) -> Option<String> {
    let rollout_path = rollout_path?;
    let contents = tokio::fs::read_to_string(rollout_path).await.ok()?;
    extract_latest_assistant_text_from_rollout(contents.as_str())
}

fn extract_latest_assistant_text_from_rollout(contents: &str) -> Option<String> {
    let mut last_text = None;

    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(object) = value.as_object() else {
            continue;
        };
        if object.get("type").and_then(serde_json::Value::as_str) != Some("message") {
            continue;
        }
        if object.get("role").and_then(serde_json::Value::as_str) != Some("assistant") {
            continue;
        }

        let direct_content = object
            .get("content")
            .and_then(serde_json::Value::as_str)
            .and_then(non_empty_trimmed);
        if direct_content.is_some() {
            last_text = direct_content;
            continue;
        }

        let nested_parts = object
            .get("message")
            .and_then(|message| message.get("parts"))
            .and_then(serde_json::Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| {
                        if part.get("type").and_then(serde_json::Value::as_str) == Some("text") {
                            part.get("text")
                                .and_then(serde_json::Value::as_str)
                                .map(str::trim)
                                .filter(|text| !text.is_empty())
                                .map(ToOwned::to_owned)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|parts| !parts.is_empty())
            .map(|parts| parts.join("\n"));
        if nested_parts.is_some() {
            last_text = nested_parts;
        }
    }

    last_text
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_last_json_fenced_block(text: &str) -> Option<serde_json::Value> {
    let mut remainder = text;
    let mut last_match = None;

    while let Some(start_idx) = remainder.find("```") {
        let fence_remainder = &remainder[start_idx + 3..];
        let Some(newline_idx) = fence_remainder.find('\n') else {
            break;
        };
        let info_string = fence_remainder[..newline_idx].trim().to_ascii_lowercase();
        let content_start = start_idx + 3 + newline_idx + 1;
        let content_remainder = &remainder[content_start..];
        let Some(end_idx) = content_remainder.find("```") else {
            break;
        };
        if info_string.is_empty() || info_string == "json" {
            last_match = Some(content_remainder[..end_idx].trim().to_string());
        }
        remainder = &content_remainder[end_idx + 3..];
    }

    last_match.and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
