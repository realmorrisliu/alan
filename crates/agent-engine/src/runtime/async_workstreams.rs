//! In-instance asynchronous child workstream tracking and clean shutdown quiesce.
//!
//! Root dispatches named child workstreams onto background Tokio tasks backed by
//! child Agent Processes (`/proc/<pid>`, `/agent/<pid>`) so Root stays interactive
//! while children execute. On actual foreground exit (or `/new` session reset),
//! active children are gracefully interrupted, flushed to durable checkpoints,
//! and stopped without spawning any background daemon.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use alan_agent_protocol::{SpawnSpec, UiActivityState};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::agent_machine::pending_resume::ResumeTarget;

use super::child_agents::{ChildLaunchRuntime, spawn_child_runtime_cancellable};
use super::child_runs::ChildRunRegistry;
use super::delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus};
use super::memory_proposals::{commit_memory_proposals_over_ap, extract_memory_proposals};
use super::project_workstreams::{WorkstreamPersistence, WorkstreamWriteMode};
use super::subagent_tool::{
    ActionProgressTracker, PreparedChildKind, SlotExecutionError, TrackedWorkstream,
    finalize_action_record, finalize_tracked_workstream,
};
use super::transition::{
    ChildEventWake, ChildEventWatchers, NamespaceAgentFiles, NamespaceProcessFiles,
};
use super::write_isolation::{SharedWriteGuard, WorkspaceMergeHandoff};

const MAX_RECENT_TERMINAL_WORKSTREAMS: usize = 64;
const DEFAULT_GRACEFUL_QUIESCE_CAP: Duration = Duration::from_secs(3);
const MIN_FORCE_QUIESCE_WAIT: Duration = Duration::from_millis(500);
const DISCARD_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Lifecycle state of an in-instance asynchronously dispatched child workstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AsyncWorkstreamStatus {
    Running,
    Completed,
    Paused,
    Cancelled,
    TimedOut,
    Terminated,
    Failed,
}

impl AsyncWorkstreamStatus {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Paused => "paused",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Terminated => "terminated",
            Self::Failed => "failed",
        }
    }
}

impl From<&ChildRuntimeStatus> for AsyncWorkstreamStatus {
    fn from(status: &ChildRuntimeStatus) -> Self {
        match status {
            ChildRuntimeStatus::Completed => Self::Completed,
            ChildRuntimeStatus::Paused => Self::Paused,
            ChildRuntimeStatus::Cancelled => Self::Cancelled,
            ChildRuntimeStatus::TimedOut => Self::TimedOut,
            ChildRuntimeStatus::Terminated => Self::Terminated,
            ChildRuntimeStatus::Failed => Self::Failed,
        }
    }
}

/// Snapshot of an active or recently completed in-instance async workstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncWorkstreamInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_name: Option<String>,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub persistence: WorkstreamPersistence,
    pub child_run_id: String,
    pub child_pid: String,
    pub process_path: String,
    pub agent_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    pub state: AsyncWorkstreamStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_mode: Option<WorkstreamWriteMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_handoff: Option<WorkspaceMergeHandoff>,
    pub started_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
}

/// Phase-1 child Process launch result returned before Phase-2 background completion runs.
pub(in crate::runtime) struct LaunchedAsyncChild {
    pub(in crate::runtime) child_pid: String,
    pub(in crate::runtime) child_run_id: String,
    pub(in crate::runtime) process_path: String,
    pub(in crate::runtime) agent_path: String,
    pub(in crate::runtime) child_agent_files: NamespaceAgentFiles,
    pub(in crate::runtime) child_process_files: NamespaceProcessFiles,
    pub(in crate::runtime) completion:
        Pin<Box<dyn Future<Output = Result<ChildRuntimeResult>> + Send>>,
}

/// Launch hook separating Phase-1 child Process startup from Phase-2 background execution.
pub(in crate::runtime) type AsyncChildLaunchFn = Arc<
    dyn Fn(
            ChildLaunchRuntime,
            SpawnSpec,
            CancellationToken,
            Option<ResumeTarget>,
            Option<ActionProgressTracker>,
        ) -> Pin<Box<dyn Future<Output = Result<LaunchedAsyncChild>> + Send>>
        + Send
        + Sync,
>;

/// Default production async child launcher backed by `/proc/clone` + `/agent/<pid>`.
pub(in crate::runtime) fn default_async_child_launch_fn() -> AsyncChildLaunchFn {
    Arc::new(|runtime, spec, cancel, resume, tracker| {
        Box::pin(async move {
            if cancel.is_cancelled() {
                bail!("Async child workstream launch cancelled");
            }
            let controller =
                spawn_child_runtime_cancellable(runtime, spec, &cancel, resume.as_ref()).await?;
            let child_pid = controller.process_pid().to_string();
            let child_run_id = controller.child_run_id().to_string();
            let process_path = format!("/proc/{child_pid}");
            let agent_path = format!("/agent/{child_pid}");
            let child_agent_files = controller.agent_files();
            let child_process_files = controller.process_files();
            if let Some(tracker) = tracker {
                tracker
                    .mark_started(&child_pid, &child_run_id, &process_path)
                    .await;
            }
            let completion: Pin<Box<dyn Future<Output = Result<ChildRuntimeResult>> + Send>> =
                Box::pin(async move { controller.join_until_cancelled(&cancel).await });
            Ok(LaunchedAsyncChild {
                child_pid,
                child_run_id,
                process_path,
                agent_path,
                child_agent_files,
                child_process_files,
                completion,
            })
        })
    })
}

/// Parameters for registering and spawning Phase-2 background completion.
pub(in crate::runtime) struct AsyncWorkstreamRegistration {
    pub(in crate::runtime) stream_name: Option<String>,
    pub(in crate::runtime) role: String,
    pub(in crate::runtime) scope: Option<String>,
    pub(in crate::runtime) persistence: WorkstreamPersistence,
    pub(in crate::runtime) action_id: Option<String>,
    pub(in crate::runtime) kind: PreparedChildKind,
    pub(in crate::runtime) label: String,
    pub(in crate::runtime) tracked: Option<TrackedWorkstream>,
    pub(in crate::runtime) rollouts_dir: Option<PathBuf>,
    pub(in crate::runtime) parent_agent_files: NamespaceAgentFiles,
    pub(in crate::runtime) child_run_registry: ChildRunRegistry,
    pub(in crate::runtime) child_cancel: CancellationToken,
    pub(in crate::runtime) write_guard: Option<SharedWriteGuard>,
    pub(in crate::runtime) launched: LaunchedAsyncChild,
}

struct ActiveEntry {
    info: AsyncWorkstreamInfo,
    cancel: CancellationToken,
    agent_files: NamespaceAgentFiles,
    process_files: NamespaceProcessFiles,
    join_handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct RegistryState {
    closed: bool,
    active: HashMap<String, ActiveEntry>,
    recent_terminal: VecDeque<AsyncWorkstreamInfo>,
}

/// Process-local registry of in-instance asynchronously running child workstreams.
#[derive(Clone, Default)]
pub struct AsyncWorkstreamRegistry {
    inner: Arc<Mutex<RegistryState>>,
    active_changed: Arc<tokio::sync::Notify>,
}

impl std::fmt::Debug for AsyncWorkstreamRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        f.debug_struct("AsyncWorkstreamRegistry")
            .field("closed", &state.closed)
            .field("active_count", &state.active.len())
            .field("recent_terminal_count", &state.recent_terminal.len())
            .finish()
    }
}

impl AsyncWorkstreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot all currently running async workstreams ordered deterministically.
    pub fn active(&self) -> Vec<AsyncWorkstreamInfo> {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        let mut items = state
            .active
            .values()
            .map(|entry| entry.info.clone())
            .collect::<Vec<_>>();
        sort_workstream_infos(&mut items);
        items
    }

    /// Snapshot all active and recently terminal async workstreams.
    pub fn list(&self) -> Vec<AsyncWorkstreamInfo> {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        let mut items = state
            .active
            .values()
            .map(|entry| entry.info.clone())
            .chain(state.recent_terminal.iter().cloned())
            .collect::<Vec<_>>();
        sort_workstream_infos(&mut items);
        items
    }

    /// Look up the currently running async workstream by exact `stream_name`.
    pub fn find_active_by_stream_name(&self, stream_name: &str) -> Option<AsyncWorkstreamInfo> {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        state
            .active
            .values()
            .find(|entry| entry.info.stream_name.as_deref() == Some(stream_name))
            .map(|entry| entry.info.clone())
    }

    /// Whether a named async workstream is currently active in-instance.
    pub fn is_stream_active(&self, stream_name: &str) -> bool {
        self.find_active_by_stream_name(stream_name).is_some()
    }

    /// Number of currently running async workstreams.
    pub fn active_count(&self) -> usize {
        self.inner
            .lock()
            .expect("async workstream registry poisoned")
            .active
            .len()
    }

    /// Wait until no async workstreams remain active without polling.
    pub async fn wait_until_idle(&self) {
        loop {
            let notified = self.active_changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.active_count() == 0 {
                return;
            }
            notified.await;
        }
    }

    /// Whether permanent shutdown has closed this registry to new background workstreams.
    pub fn is_closed(&self) -> bool {
        self.inner
            .lock()
            .expect("async workstream registry poisoned")
            .closed
    }

    /// Request cancellation of a single running named workstream.
    pub fn cancel_stream(&self, stream_name: &str) -> bool {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        let Some(entry) = state
            .active
            .values()
            .find(|entry| entry.info.stream_name.as_deref() == Some(stream_name))
        else {
            return false;
        };
        entry.cancel.cancel();
        true
    }

    /// Request cancellation of all currently active async workstreams without waiting.
    pub fn cancel_all(&self) {
        let state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        for entry in state.active.values() {
            entry.cancel.cancel();
        }
    }

    /// Register a Phase-1 launched child Process and spawn its Phase-2 completion task.
    pub(in crate::runtime) fn register_and_spawn(
        &self,
        reg: AsyncWorkstreamRegistration,
    ) -> Result<AsyncWorkstreamInfo> {
        let AsyncWorkstreamRegistration {
            stream_name,
            role,
            scope,
            persistence,
            action_id,
            kind,
            label,
            tracked,
            rollouts_dir,
            parent_agent_files,
            child_run_registry,
            child_cancel,
            write_guard,
            launched,
        } = reg;

        let (write_mode, worktree_ref, isolated_workspace) = match &kind {
            PreparedChildKind::AdHocSubagent {
                write_isolation: Some(iso),
                ..
            } => (
                Some(iso.write_mode),
                iso.isolated_workspace
                    .as_ref()
                    .map(|ctx| ctx.allocation.workspace_ref.clone()),
                iso.isolated_workspace.clone(),
            ),
            _ => (None, None, None),
        };
        let entry_id = launched.child_run_id.clone();
        let info = AsyncWorkstreamInfo {
            stream_name: stream_name.clone(),
            role,
            scope,
            persistence,
            child_run_id: launched.child_run_id,
            child_pid: launched.child_pid,
            process_path: launched.process_path,
            agent_path: launched.agent_path,
            action_id: action_id.clone(),
            state: AsyncWorkstreamStatus::Running,
            summary: None,
            rollout_ref: None,
            write_mode,
            worktree_ref,
            merge_handoff: None,
            started_at_ms: now_ms(),
            finished_at_ms: None,
        };

        {
            let mut state = self
                .inner
                .lock()
                .expect("async workstream registry poisoned");
            if state.closed {
                child_cancel.cancel();
                bail!("Foreground instance is shutting down; cannot dispatch async workstream.");
            }
            if let Some(name) = stream_name.as_deref()
                && state
                    .active
                    .values()
                    .any(|entry| entry.info.stream_name.as_deref() == Some(name))
            {
                child_cancel.cancel();
                bail!("Workstream '{name}' is already running.");
            }
            state.active.insert(
                entry_id.clone(),
                ActiveEntry {
                    info: info.clone(),
                    cancel: child_cancel,
                    agent_files: launched.child_agent_files,
                    process_files: launched.child_process_files,
                    join_handle: None,
                },
            );
        }

        let registry = self.clone();
        let completion = launched.completion;
        let fallback_process_path = info.process_path.clone();
        let fallback_run_id = info.child_run_id.clone();
        let task_entry_id = entry_id.clone();
        let handle = tokio::spawn(async move {
            let _write_guard = write_guard;
            let result = match completion.await {
                Ok(result) => result,
                Err(error) => {
                    let mut failed = ChildRuntimeResult::cancelled_before_launch();
                    failed.status = ChildRuntimeStatus::Failed;
                    failed.process_path = fallback_process_path;
                    failed.child_run_id = Some(fallback_run_id);
                    failed.error_message = Some(format!("Async workstream failed: {error:#}"));
                    failed
                }
            };

            let output_reference = finalize_action_record(
                &parent_agent_files,
                action_id.as_deref(),
                &kind,
                &label,
                &result,
            )
            .await;

            if let (Some(run_id), Some(reference)) =
                (result.child_run_id.as_deref(), output_reference.as_ref())
            {
                child_run_registry.set_state_ref(run_id, reference.clone());
            }

            let updated_workstream = finalize_tracked_workstream(
                tracked.as_ref(),
                rollouts_dir.as_deref(),
                &result,
                output_reference.as_ref(),
            )
            .await;

            let summary = non_empty_trimmed(&result.output_text)
                .or_else(|| result.error_message.clone())
                .map(|text| truncate_summary(&text, 240));
            let rollout_ref = updated_workstream
                .as_ref()
                .and_then(|record| {
                    record
                        .last_checkpoint_ref
                        .clone()
                        .or_else(|| record.rollout_ref.clone())
                })
                .or_else(|| {
                    result
                        .rollout_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                });
            let merge_handoff = if let Some(ctx) = isolated_workspace.as_ref() {
                ctx.collect_handoff(rollout_ref.as_deref()).await
            } else {
                None
            };
            let proposals =
                extract_memory_proposals(result.structured_output.as_ref(), rollout_ref.as_deref());
            if !proposals.is_empty()
                && let Err(err) = commit_memory_proposals_over_ap(
                    parent_agent_files.root_transport(),
                    "/mnt/mem",
                    &proposals,
                    None,
                    None,
                )
                .await
            {
                warn!(%err, "Failed to commit background child memory_proposals");
            }
            let terminal_status = AsyncWorkstreamStatus::from(&result.status);
            registry.mark_terminal(
                &task_entry_id,
                terminal_status,
                summary,
                rollout_ref,
                merge_handoff,
            );
        });

        let mut state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        if let Some(entry) = state.active.get_mut(&entry_id) {
            entry.join_handle = Some(handle);
        }
        Ok(info)
    }

    fn mark_terminal(
        &self,
        entry_id: &str,
        status: AsyncWorkstreamStatus,
        summary: Option<String>,
        rollout_ref: Option<String>,
        merge_handoff: Option<WorkspaceMergeHandoff>,
    ) {
        let mut state = self
            .inner
            .lock()
            .expect("async workstream registry poisoned");
        let Some(mut entry) = state.active.remove(entry_id) else {
            return;
        };
        entry.info.state = status;
        entry.info.summary = summary;
        entry.info.rollout_ref = rollout_ref;
        entry.info.merge_handoff = merge_handoff;
        entry.info.finished_at_ms = Some(now_ms());
        push_recent_terminal(&mut state.recent_terminal, entry.info);
        drop(state);
        self.active_changed.notify_waiters();
    }

    /// Gracefully quiesce all active child workstreams, flush checkpoints, stop child
    /// Agent Processes, and permanently close this registry for foreground shutdown.
    pub async fn quiesce_and_flush_all(&self, timeout: Duration) -> Result<()> {
        self.quiesce_inner(timeout, true).await
    }

    /// Gracefully quiesce all active child workstreams for `/new` session reset while
    /// leaving the registry open for subsequent turns in the new session.
    pub async fn quiesce_for_reset(&self, timeout: Duration) -> Result<()> {
        self.quiesce_inner(timeout, false).await
    }

    async fn quiesce_inner(&self, timeout: Duration, close_permanently: bool) -> Result<()> {
        struct QuiesceTarget {
            entry_id: String,
            child_pid: String,
            agent_files: NamespaceAgentFiles,
            process_files: NamespaceProcessFiles,
            cancel: CancellationToken,
            join_handle: Option<JoinHandle<()>>,
        }

        let targets = {
            let mut state = self
                .inner
                .lock()
                .expect("async workstream registry poisoned");
            if close_permanently {
                state.closed = true;
            }
            if state.active.is_empty() {
                return Ok(());
            }
            state
                .active
                .iter_mut()
                .map(|(entry_id, entry)| QuiesceTarget {
                    entry_id: entry_id.clone(),
                    child_pid: entry.info.child_pid.clone(),
                    agent_files: entry.agent_files.clone(),
                    process_files: entry.process_files.clone(),
                    cancel: entry.cancel.clone(),
                    join_handle: entry.join_handle.take(),
                })
                .collect::<Vec<_>>()
        };

        let grace_timeout = timeout.min(DEFAULT_GRACEFUL_QUIESCE_CAP);
        let force_timeout = timeout
            .saturating_sub(grace_timeout)
            .max(MIN_FORCE_QUIESCE_WAIT);

        // Step 1: Gracefully interrupt each child's Agent Machine so its controller.shutdown()
        // flushes RolloutRecorder checkpoints before exiting /proc/<pid>.
        let mut stop_helpers = Vec::with_capacity(targets.len());
        let mut pending_joins = Vec::with_capacity(targets.len());
        for target in targets {
            let pid = target.child_pid.clone();
            let agent_files = target.agent_files.clone();
            let process_files = target.process_files.clone();
            let cancel = target.cancel.clone();
            stop_helpers.push(tokio::spawn(async move {
                graceful_stop_child(&pid, &agent_files, &process_files, &cancel, grace_timeout)
                    .await;
            }));
            pending_joins.push((target.entry_id, target.cancel, target.join_handle));
        }

        let grace_deadline = Instant::now() + grace_timeout;
        let mut remaining_joins = Vec::new();
        for (entry_id, cancel, handle) in pending_joins {
            let Some(mut handle) = handle else {
                continue;
            };
            let rem = grace_deadline.saturating_duration_since(Instant::now());
            if rem.is_zero() {
                remaining_joins.push((entry_id, cancel, handle));
                continue;
            }
            match tokio::time::timeout(rem, &mut handle).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    warn!(entry_id = %entry_id, %error, "Async workstream task panicked during shutdown");
                }
                Err(_) => remaining_joins.push((entry_id, cancel, handle)),
            }
        }

        for helper in stop_helpers {
            helper.abort();
        }

        // Step 2: Force-cancel any straggler via child_cancel -> /proc/<pid>/ctl "cancel".
        if !remaining_joins.is_empty() {
            for (_, cancel, _) in &remaining_joins {
                cancel.cancel();
            }
            let force_deadline = Instant::now() + force_timeout;
            for (entry_id, _, mut handle) in remaining_joins {
                let rem = force_deadline.saturating_duration_since(Instant::now());
                let finished = if rem.is_zero() {
                    handle.is_finished()
                } else {
                    match tokio::time::timeout(rem, &mut handle).await {
                        Ok(Ok(())) => true,
                        Ok(Err(error)) => {
                            warn!(entry_id = %entry_id, %error, "Async workstream task panicked on force cancel");
                            true
                        }
                        Err(_) => false,
                    }
                };
                if !finished {
                    warn!(entry_id = %entry_id, "Timed out waiting for async workstream force cancel; aborting task");
                    handle.abort();
                    let _ = handle.await;
                    self.mark_terminal(
                        &entry_id,
                        AsyncWorkstreamStatus::Cancelled,
                        Some("Cancelled on foreground shutdown".to_string()),
                        None,
                        None,
                    );
                }
            }
        }

        Ok(())
    }
}

async fn graceful_stop_child(
    pid: &str,
    agent_files: &NamespaceAgentFiles,
    process_files: &NamespaceProcessFiles,
    cancel: &CancellationToken,
    grace_timeout: Duration,
) {
    if matches!(process_files.read_process_exit_code(pid).await, Ok(Some(_))) {
        return;
    }

    let mut watchers = ChildEventWatchers::spawn(process_files, agent_files, pid);

    if let Err(error) = cancel_pending_child_requests(agent_files).await {
        warn!(pid = %pid, %error, "Failed to cancel pending child requests before interrupt");
    }

    if agent_files.write_machine_ctl("interrupt").await.is_err() {
        // Synthetic unit tests or unmounted /agent/<pid> views cannot accept machine/ctl;
        // fall back immediately to child_cancel instead of burning the full grace window.
        cancel.cancel();
        return;
    }

    let deadline = tokio::time::Instant::now() + grace_timeout;
    let mut last_discard_at: Option<Instant> = None;
    while tokio::time::Instant::now() < deadline && !cancel.is_cancelled() {
        watchers.mark_events_seen();
        if matches!(process_files.read_process_exit_code(pid).await, Ok(Some(_))) {
            return;
        }
        let _ = cancel_pending_child_requests(agent_files).await;
        let mut discard_retry_deadline = None;
        if let Ok(activity) = agent_files.read_ui_activity_snapshot().await
            && activity.state == UiActivityState::Paused
        {
            if last_discard_at.is_none_or(|prev| prev.elapsed() >= DISCARD_RETRY_INTERVAL) {
                let _ = agent_files.write_machine_ctl("queue-v1 discard").await;
                last_discard_at = Some(Instant::now());
            }
            if let Some(last) = last_discard_at {
                let rem = DISCARD_RETRY_INTERVAL.saturating_sub(last.elapsed());
                discard_retry_deadline = Some(tokio::time::Instant::now() + rem);
            }
        }

        tokio::select! {
            biased;
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep_until(deadline) => return,
            wake = watchers.next_wake(), if watchers.has_open_stream() => {
                match wake {
                    ChildEventWake::ProcessExited(Ok(_)) => return,
                    ChildEventWake::ProcessExited(Err(_)) | ChildEventWake::AgentEvents => {}
                    ChildEventWake::StreamsClosed => {
                        if matches!(process_files.read_process_exit_code(pid).await, Ok(Some(_))) {
                            return;
                        }
                        cancel.cancel();
                        return;
                    }
                }
            }
            _ = async {
                match discard_retry_deadline {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {}
        }
    }
}

async fn cancel_pending_child_requests(agent_files: &NamespaceAgentFiles) -> Result<()> {
    let ids = agent_files.request_ids().await?;
    while let Some(req_id) = agent_files.pending_request_id(&ids).await? {
        agent_files
            .cancel_request(&req_id)
            .await
            .with_context(|| format!("cancel child request {req_id}"))?;
    }
    Ok(())
}

fn push_recent_terminal(queue: &mut VecDeque<AsyncWorkstreamInfo>, info: AsyncWorkstreamInfo) {
    while queue.len() >= MAX_RECENT_TERMINAL_WORKSTREAMS {
        queue.pop_front();
    }
    queue.push_back(info);
}

fn sort_workstream_infos(items: &mut [AsyncWorkstreamInfo]) {
    items.sort_by(|left, right| {
        left.started_at_ms
            .cmp(&right.started_at_ms)
            .then_with(|| left.stream_name.cmp(&right.stream_name))
            .then_with(|| left.child_run_id.cmp(&right.child_run_id))
    });
}

fn non_empty_trimmed(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn truncate_summary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[allow(
    clippy::too_many_arguments,
    reason = "Async background workstream launch threads registry, store paths, child launch runtime, and cancellation handles explicitly"
)]
pub(in crate::runtime) async fn launch_background_workstream_slot(
    async_workstreams: &AsyncWorkstreamRegistry,
    rollouts_dir: Option<PathBuf>,
    parent_agent_files: NamespaceAgentFiles,
    child_run_registry: ChildRunRegistry,
    kind: PreparedChildKind,
    label: String,
    action_id: Option<String>,
    tracked: Option<TrackedWorkstream>,
    child_runtime: ChildLaunchRuntime,
    spec: SpawnSpec,
    resume: Option<ResumeTarget>,
    launch_cancel: &CancellationToken,
    async_launch_fn: AsyncChildLaunchFn,
) -> std::result::Result<AsyncWorkstreamInfo, SlotExecutionError> {
    if launch_cancel.is_cancelled() || async_workstreams.is_closed() {
        return Err(SlotExecutionError::SpawnOrJoin(anyhow::anyhow!(
            "Async workstream launch cancelled before startup"
        )));
    }
    let (role, scope, persistence, stream_name, write_isolation) = match &kind {
        PreparedChildKind::AdHocSubagent {
            request,
            write_isolation,
            ..
        } => (
            request.role.clone(),
            request.scope.clone(),
            request.effective_persistence(),
            request.stream_name.clone(),
            write_isolation.clone(),
        ),
        PreparedChildKind::DelegatedSkill { request } => (
            request.skill_id.clone(),
            None,
            WorkstreamPersistence::OneOff,
            None,
            None,
        ),
    };
    let owner_label = stream_name.as_deref().unwrap_or(role.as_str()).to_string();
    let write_guard = if let Some(iso) = write_isolation.as_ref()
        && !iso.shared_write_grants.is_empty()
    {
        Some(
            iso.queue
                .acquire_grants_fifo(&iso.shared_write_grants, &owner_label, launch_cancel)
                .await
                .map_err(SlotExecutionError::SpawnOrJoin)?,
        )
    } else {
        None
    };
    let tracker = action_id.clone().map(|id| {
        ActionProgressTracker::new(parent_agent_files.clone(), id, label.clone())
            .with_workstream_metadata(stream_name.clone(), Some(persistence), true)
    });
    let background_cancel = CancellationToken::new();
    let bg_cancel_for_launch = background_cancel.clone();
    let launched = tokio::select! {
        res = async_launch_fn(child_runtime, spec, bg_cancel_for_launch, resume, tracker) => {
            res.map_err(SlotExecutionError::SpawnOrJoin)?
        }
        _ = launch_cancel.cancelled() => {
            background_cancel.cancel();
            return Err(SlotExecutionError::SpawnOrJoin(anyhow::anyhow!(
                "Async workstream launch cancelled during startup"
            )));
        }
    };
    async_workstreams
        .register_and_spawn(AsyncWorkstreamRegistration {
            stream_name,
            role,
            scope,
            persistence,
            action_id,
            kind,
            label,
            tracked,
            rollouts_dir,
            parent_agent_files,
            child_run_registry,
            child_cancel: background_cancel,
            write_guard,
            launched,
        })
        .map_err(SlotExecutionError::SpawnOrJoin)
}

#[cfg(test)]
#[path = "async_workstreams_tests.rs"]
mod tests;
