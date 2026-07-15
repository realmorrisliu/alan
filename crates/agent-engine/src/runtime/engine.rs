//! Agent Runtime - Core execution engine.

use super::agent_loop::{
    DeferredRuntimeActionExit, handle_submission_with_cancel,
    run_deferred_runtime_action_with_cancel,
};
use super::turn_driver::{
    NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL, TurnInputBroker, drive_turn_submission_with_cancel,
    is_turn_inband_submission, namespace_pending_resume_submission, should_drive_turn_submission,
};
use super::turn_state::TurnState;
use super::{NamespaceRuntimeEnvironment, RuntimeConfig, RuntimeLoopState};
use crate::agent_machine::AgentMachine;
use alan_agent_protocol::{Event, InputMode, Submission};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Queues for managing submissions.
///
/// There are two submission queues in the agent runtime:
/// Requeue leftover inband submissions from turn state and broker to the outer queue.
async fn requeue_leftover_inband_submissions(
    broker: &TurnInputBroker,
    turn_state: &mut TurnState,
    queued_submissions: &mut VecDeque<QueuedRuntimeItem>,
) -> usize {
    let broker_drained = broker.drain().await;
    let turn_drained = turn_state.drain_buffered_inband_submissions();
    let count = broker_drained.len() + turn_drained.len();
    for submission in turn_drained {
        push_submission_ahead_of_deferred(queued_submissions, submission);
    }
    for submission in broker_drained {
        push_submission_ahead_of_deferred(queued_submissions, submission);
    }
    count
}

/// 1. The `outer_queue` - cross-turn queue for submissions that are not in the active turn.
/// 2. The `active_turn_broker` - channel for in-turn submissions during active turn execution.
enum QueuedRuntimeItem {
    Submission(Submission),
    Deferred(super::agent_loop::DeferredRuntimeAction),
}

fn push_submission_ahead_of_deferred(
    outer_queue: &mut VecDeque<QueuedRuntimeItem>,
    submission: Submission,
) {
    let insertion_index = outer_queue
        .iter()
        .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))
        .unwrap_or(outer_queue.len());
    outer_queue.insert(insertion_index, QueuedRuntimeItem::Submission(submission));
}

fn should_requeue_deferred_action(
    requeue_requested: bool,
    exit: DeferredRuntimeActionExit,
) -> bool {
    requeue_requested && matches!(exit, DeferredRuntimeActionExit::Cancelled)
}

fn preserves_paused_terminal_state(result: &Result<()>, has_pending_interaction: bool) -> bool {
    result.is_ok() && has_pending_interaction
}

async fn read_pending_namespace_resume_submission(
    state: &RuntimeLoopState,
) -> Option<Result<Submission>> {
    if !state.turn_state.has_pending_interaction() {
        return None;
    }

    match namespace_pending_resume_submission(state).await {
        Ok(Some(submission)) => Some(Ok(submission)),
        Ok(None) => None,
        Err(err) => Some(Err(err)),
    }
}

async fn read_pending_namespace_control_submission(
    namespace: &super::NamespaceRuntimeEnvironment,
) -> Option<Result<Submission>> {
    match namespace.read_next_machine_control_submission().await {
        Ok(Some(submission)) => Some(Ok(submission)),
        Ok(None) => None,
        Err(err) => Some(Err(err)),
    }
}

#[derive(Default)]
struct RuntimeSubmissionQueues {
    /// Cross-turn queue for submissions.
    outer_queue: VecDeque<QueuedRuntimeItem>,
    /// The broker that queues in-turn submissions.
    active_turn_broker: TurnInputBroker,
}

impl RuntimeSubmissionQueues {
    fn pop_outer(&mut self) -> Option<QueuedRuntimeItem> {
        self.outer_queue.pop_front()
    }

    fn pop_outer_deferred(&mut self) -> Option<QueuedRuntimeItem> {
        let deferred_index = self
            .outer_queue
            .iter()
            .position(|item| matches!(item, QueuedRuntimeItem::Deferred(_)))?;
        self.outer_queue.remove(deferred_index)
    }

    fn push_outer_submission(&mut self, submission: Submission) {
        push_submission_ahead_of_deferred(&mut self.outer_queue, submission);
    }

    fn push_outer_deferred(&mut self, action: super::agent_loop::DeferredRuntimeAction) {
        self.outer_queue
            .push_back(QueuedRuntimeItem::Deferred(action));
    }

    async fn requeue_active_turn_leftovers(&mut self, turn_state: &mut TurnState) -> usize {
        requeue_leftover_inband_submissions(
            &self.active_turn_broker,
            turn_state,
            &mut self.outer_queue,
        )
        .await
    }
}

/// Effective durability state for a runtime machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentMachineDurabilityState {
    /// Whether the active machine has a persistent recorder attached.
    pub durable: bool,
    /// Whether startup required durability instead of allowing in-memory fallback.
    pub required: bool,
}

/// Metadata produced once runtime startup completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeStartupMetadata {
    /// Authoritative lifecycle path of the launched Agent Process.
    pub process_path: String,
    /// AgentFS projection path of the launched Agent Process.
    pub agent_path: String,
    /// Identity of the fresh rollout produced by this process, when durable.
    pub rollout_id: Option<String>,
    pub rollout_path: Option<PathBuf>,
    pub durability: AgentMachineDurabilityState,
    pub execution_backend: String,
    pub request_controls: crate::ResolvedRequestControls,
    pub warnings: Vec<String>,
}

struct AgentMachineStartupOutcome {
    machine: AgentMachine,
    metadata: RuntimeStartupMetadata,
}

fn best_effort_durability_warning(err: &anyhow::Error) -> String {
    format!("AgentMachine is running without persistent recorder; using in-memory mode: {err}")
}

fn current_execution_backend() -> String {
    crate::tools::active_backend_name().to_string()
}

#[cfg(test)]
pub(crate) fn runtime_host_capabilities(
    _config: &AgentProcessConfig,
    tools: &crate::tools::ToolRegistry,
) -> crate::skills::SkillHostCapabilities {
    runtime_host_capabilities_for_tools(tools.list_tools().into_iter().map(str::to_string))
}

pub(crate) fn runtime_host_capabilities_for_tools(
    tools: impl IntoIterator<Item = String>,
) -> crate::skills::SkillHostCapabilities {
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    crate::skills::build_skill_host_capabilities_with_path_dirs(tools, path_dirs, true)
}

#[cfg(test)]
fn runtime_host_capabilities_with_path_dirs<I, P>(
    _config: &AgentProcessConfig,
    tools: &crate::tools::ToolRegistry,
    path_dirs: I,
) -> crate::skills::SkillHostCapabilities
where
    I: IntoIterator<Item = P>,
    P: AsRef<std::path::Path>,
{
    crate::skills::build_skill_host_capabilities_with_path_dirs(
        tools.list_tools().into_iter().map(str::to_string),
        path_dirs,
        true,
    )
}

async fn create_persistent_machine(
    process_path: &str,
    model: &str,
    rollouts_dir: Option<&std::path::PathBuf>,
    rollout_cwd: Option<&std::path::Path>,
    reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
) -> anyhow::Result<AgentMachine> {
    AgentMachine::new_with_recorder_options(
        process_path,
        model,
        rollouts_dir.map(|dir| dir.as_path()),
        rollout_cwd,
        reasoning_effort,
    )
    .await
}

struct AgentMachineLaunchContext<'a> {
    process_path: &'a str,
    agent_path: &'a str,
    model: &'a str,
}

async fn initialize_agent_machine(
    launch: AgentMachineLaunchContext<'_>,
    recovery_rollout_path: Option<&std::path::PathBuf>,
    rollouts_dir: Option<&std::path::PathBuf>,
    durability_required: bool,
    rollout_cwd: Option<&std::path::Path>,
    request_controls: crate::ResolvedRequestControls,
) -> anyhow::Result<AgentMachineStartupOutcome> {
    let mut warnings = Vec::new();
    let reasoning_effort = request_controls.reasoning_effort();

    let machine = if let Some(path) = recovery_rollout_path {
        let load_result = AgentMachine::load_from_rollout_with_recorder_cwd(
            path,
            launch.process_path,
            launch.model,
            rollouts_dir.map(|dir| dir.as_path()),
            rollout_cwd,
            reasoning_effort,
        )
        .await;

        match load_result {
            Ok(machine) => machine,
            Err(err) => {
                if durability_required {
                    return Err(anyhow::anyhow!(
                        "Strict durability required: failed to load persisted machine from {}: {}",
                        path.display(),
                        err
                    ));
                }

                warn!(
                    error = %err,
                    path = %path.display(),
                    "Failed to load machine from rollout; creating fresh persistent machine"
                );
                match create_persistent_machine(
                    launch.process_path,
                    launch.model,
                    rollouts_dir,
                    rollout_cwd,
                    reasoning_effort,
                )
                .await
                {
                    Ok(machine) => machine,
                    Err(create_err) => {
                        warn!(
                            error = %create_err,
                            "Failed to create a persistent machine after rollout recovery; using an in-memory machine"
                        );
                        warnings.push(best_effort_durability_warning(&create_err));
                        AgentMachine::new()
                    }
                }
            }
        }
    } else {
        match create_persistent_machine(
            launch.process_path,
            launch.model,
            rollouts_dir,
            rollout_cwd,
            reasoning_effort,
        )
        .await
        {
            Ok(machine) => machine,
            Err(err) => {
                if durability_required {
                    return Err(anyhow::anyhow!(
                        "Strict durability required: failed to create persistent machine: {}",
                        err
                    ));
                }

                warn!(error = %err, "Failed to create persistent machine; using in-memory machine");
                warnings.push(best_effort_durability_warning(&err));
                AgentMachine::new()
            }
        }
    };

    Ok(AgentMachineStartupOutcome {
        metadata: RuntimeStartupMetadata {
            process_path: launch.process_path.to_string(),
            agent_path: launch.agent_path.to_string(),
            rollout_id: machine
                .recorder
                .as_ref()
                .map(|recorder| recorder.rollout_id().to_string()),
            rollout_path: machine.rollout_path().cloned(),
            durability: AgentMachineDurabilityState {
                durable: machine.recorder.is_some(),
                required: durability_required,
            },
            execution_backend: current_execution_backend(),
            request_controls,
            warnings,
        },
        machine,
    })
}

/// Handle for communicating with an agent runtime
#[derive(Clone)]
pub struct RuntimeHandle {
    pub submission_tx: mpsc::Sender<Submission>,
    /// Shutdown signal sender for graceful shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl RuntimeHandle {
    /// Request graceful shutdown of the runtime
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(ref tx) = self.shutdown_tx {
            tx.send(()).await.map_err(|_| {
                anyhow::anyhow!("Failed to send shutdown signal - runtime may already be stopped")
            })?;
            info!("Shutdown signal sent to runtime");
            Ok(())
        } else {
            Err(anyhow::anyhow!("Shutdown channel not available"))
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub core_config: crate::config::Config,
    pub runtime_config: RuntimeConfig,
    explicit_runtime_overrides: ExplicitRuntimeOverrides,
}

#[derive(Debug, Clone, Copy, Default)]
struct ExplicitRuntimeOverrides {
    model: bool,
    max_tool_loops: bool,
    tool_repeat_limit: bool,
    llm_request_timeout_secs: bool,
    prompt_snapshot_enabled: bool,
    prompt_snapshot_max_chars: bool,
    context_window_tokens: bool,
    compaction_soft_trigger_ratio: bool,
    compaction_hard_trigger_ratio: bool,
    request_control_intent: bool,
    streaming_mode: bool,
    partial_stream_recovery_mode: bool,
    durability_required: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        let runtime_config = RuntimeConfig::default();
        Self {
            core_config: crate::config::Config::default(),
            runtime_config,
            explicit_runtime_overrides: ExplicitRuntimeOverrides::default(),
        }
    }
}

impl From<crate::config::Config> for AgentConfig {
    fn from(config: crate::config::Config) -> Self {
        let runtime_config = RuntimeConfig::from(&config);
        Self {
            core_config: config,
            runtime_config,
            explicit_runtime_overrides: ExplicitRuntimeOverrides::default(),
        }
    }
}

impl AgentConfig {
    /// Override the effective model for this launch across agent-root overlays.
    pub fn set_model_override(&mut self, model: impl Into<String>) {
        self.core_config.set_effective_model(model);
        sync_runtime_context_window_budget(&self.core_config, &mut self.runtime_config);
        sync_runtime_request_control_intent(&self.core_config, &mut self.runtime_config);
        self.explicit_runtime_overrides.model = true;
    }

    /// Override named model reasoning effort for this launch across overlays.
    pub fn set_model_reasoning_effort_override(
        &mut self,
        model_reasoning_effort: Option<alan_agent_protocol::ReasoningEffort>,
    ) {
        self.core_config.model_reasoning_effort = model_reasoning_effort;
        self.runtime_config.request_control_intent =
            crate::RequestControlIntent::reasoning_effort(model_reasoning_effort);
        self.explicit_runtime_overrides.request_control_intent = true;
    }

    /// Override streaming mode for this runtime launch, preserving it across agent-root overlays.
    pub fn set_streaming_mode_override(&mut self, streaming_mode: crate::config::StreamingMode) {
        self.core_config.streaming_mode = streaming_mode;
        self.runtime_config.streaming_mode = streaming_mode;
        self.explicit_runtime_overrides.streaming_mode = true;
    }

    /// Override partial stream recovery mode for this launch across agent-root overlays.
    pub fn set_partial_stream_recovery_mode_override(
        &mut self,
        partial_stream_recovery_mode: crate::config::PartialStreamRecoveryMode,
    ) {
        self.core_config.partial_stream_recovery_mode = partial_stream_recovery_mode;
        self.runtime_config.partial_stream_recovery_mode = partial_stream_recovery_mode;
        self.explicit_runtime_overrides.partial_stream_recovery_mode = true;
    }

    /// Override machine durability requirement for this launch across agent-root overlays.
    pub fn set_durability_required_override(&mut self, durability_required: bool) {
        self.core_config.durability.required = durability_required;
        self.runtime_config.durability_required = durability_required;
        self.explicit_runtime_overrides.durability_required = true;
    }

    pub fn refresh_runtime_derived_fields(&mut self) {
        sync_runtime_context_window_budget(&self.core_config, &mut self.runtime_config);
        sync_runtime_request_control_intent(&self.core_config, &mut self.runtime_config);
    }

    pub fn with_definition_overlays(
        &self,
        overlay_paths: &[std::path::PathBuf],
    ) -> anyhow::Result<Self> {
        let mut merge_base_core_config = self.core_config.clone();
        if self.explicit_runtime_overrides.request_control_intent {
            merge_base_core_config.model_reasoning_effort = None;
        }

        let mut core_config = merge_base_core_config.with_definition_overlays(overlay_paths)?;
        let mut runtime_config = merge_runtime_config_from_core_overlay(
            &merge_base_core_config,
            &core_config,
            &self.runtime_config,
            self.explicit_runtime_overrides,
        );
        self.reapply_explicit_runtime_overrides(&mut core_config, &mut runtime_config);

        Ok(Self {
            core_config,
            runtime_config,
            explicit_runtime_overrides: self.explicit_runtime_overrides,
        })
    }

    pub fn with_definition_overlay_content(
        &self,
        content: &str,
        source: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let mut merge_base_core_config = self.core_config.clone();
        if self.explicit_runtime_overrides.request_control_intent {
            merge_base_core_config.model_reasoning_effort = None;
        }
        let mut core_config =
            merge_base_core_config.with_definition_overlay_content(content, source)?;
        let mut runtime_config = merge_runtime_config_from_core_overlay(
            &merge_base_core_config,
            &core_config,
            &self.runtime_config,
            self.explicit_runtime_overrides,
        );
        self.reapply_explicit_runtime_overrides(&mut core_config, &mut runtime_config);
        Ok(Self {
            core_config,
            runtime_config,
            explicit_runtime_overrides: self.explicit_runtime_overrides,
        })
    }

    fn reapply_explicit_runtime_overrides(
        &self,
        core_config: &mut crate::config::Config,
        runtime_config: &mut RuntimeConfig,
    ) {
        if self.explicit_runtime_overrides.model {
            core_config.set_effective_model(self.core_config.effective_model().to_string());
            sync_runtime_context_window_budget(core_config, runtime_config);
            sync_runtime_request_control_intent(core_config, runtime_config);
        }
        if self.explicit_runtime_overrides.request_control_intent {
            self.runtime_config
                .request_control_intent
                .apply_to_config(core_config);
            runtime_config.request_control_intent = self.runtime_config.request_control_intent;
        }
        if self.explicit_runtime_overrides.streaming_mode {
            core_config.streaming_mode = self.runtime_config.streaming_mode;
            runtime_config.streaming_mode = self.runtime_config.streaming_mode;
        }
        if self.explicit_runtime_overrides.partial_stream_recovery_mode {
            core_config.partial_stream_recovery_mode =
                self.runtime_config.partial_stream_recovery_mode;
            runtime_config.partial_stream_recovery_mode =
                self.runtime_config.partial_stream_recovery_mode;
        }
        if self.explicit_runtime_overrides.durability_required {
            core_config.durability.required = self.runtime_config.durability_required;
            runtime_config.durability_required = self.runtime_config.durability_required;
        }
    }
}

fn sync_runtime_context_window_budget(
    core_config: &crate::config::Config,
    runtime_config: &mut RuntimeConfig,
) {
    runtime_config.context_window_tokens = core_config.effective_context_window_tokens();
}

fn sync_runtime_request_control_intent(
    core_config: &crate::config::Config,
    runtime_config: &mut RuntimeConfig,
) {
    runtime_config.request_control_intent = crate::RequestControlIntent::from_config(core_config);
}

fn merge_runtime_config_from_core_overlay(
    base_core_config: &crate::config::Config,
    overlaid_core_config: &crate::config::Config,
    current_runtime_config: &RuntimeConfig,
    explicit_runtime_overrides: ExplicitRuntimeOverrides,
) -> RuntimeConfig {
    let base_runtime = RuntimeConfig::from(base_core_config);
    let overlaid_runtime = RuntimeConfig::from(overlaid_core_config);
    let mut merged_runtime = current_runtime_config.clone();

    macro_rules! sync_if_unmodified {
        ($field:ident) => {
            if !explicit_runtime_overrides.$field && merged_runtime.$field == base_runtime.$field {
                merged_runtime.$field = overlaid_runtime.$field;
            }
        };
    }

    sync_if_unmodified!(max_tool_loops);
    sync_if_unmodified!(tool_repeat_limit);
    sync_if_unmodified!(llm_request_timeout_secs);
    sync_if_unmodified!(prompt_snapshot_enabled);
    sync_if_unmodified!(prompt_snapshot_max_chars);
    sync_if_unmodified!(context_window_tokens);
    sync_if_unmodified!(compaction_soft_trigger_ratio);
    sync_if_unmodified!(compaction_hard_trigger_ratio);
    sync_if_unmodified!(request_control_intent);
    sync_if_unmodified!(streaming_mode);
    sync_if_unmodified!(partial_stream_recovery_mode);
    sync_if_unmodified!(durability_required);

    merged_runtime
}

/// Host inputs for starting one Agent Process runtime.
#[derive(Debug, Clone)]
pub struct AgentProcessConfig {
    /// Agent execution configuration.
    pub agent_config: AgentConfig,
    /// Source used before applying the explicit Agent Definition descriptor.
    pub core_config_source: crate::ConfigSourceKind,
    /// Process namespace, descriptors, credentials, Host Mounts, and Alan OS cwd.
    pub launch_context: crate::ProcessLaunchContext,
    /// Durable service backing selected by the Host; never exposed as Process identity.
    pub store_bindings: Option<crate::AgentRuntimeStoreBindings>,
    /// Memory Service backing paired with the explicit Memory Store descriptor.
    pub memory_store_backing: Option<std::path::PathBuf>,
    /// Optional execution record used to recover Agent Machine state for a new Process.
    pub recovery_rollout_path: Option<std::path::PathBuf>,
    /// Optional host factory for applying approved mount grants to the live namespace.
    pub mount_grant_applicator_factory: Option<Arc<dyn super::MountGrantApplicatorFactory>>,
}

impl Default for AgentProcessConfig {
    fn default() -> Self {
        Self {
            agent_config: AgentConfig::default(),
            core_config_source: crate::ConfigSourceKind::Default,
            launch_context: crate::ProcessLaunchContext::root(),
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

impl From<crate::config::Config> for AgentProcessConfig {
    fn from(config: crate::config::Config) -> Self {
        Self {
            agent_config: AgentConfig::from(config),
            core_config_source: crate::ConfigSourceKind::Default,
            launch_context: crate::ProcessLaunchContext::root(),
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

impl From<crate::LoadedConfig> for AgentProcessConfig {
    fn from(loaded: crate::LoadedConfig) -> Self {
        Self {
            agent_config: AgentConfig::from(loaded.config),
            core_config_source: loaded.source,
            launch_context: crate::ProcessLaunchContext::root(),
            store_bindings: None,
            memory_store_backing: None,
            recovery_rollout_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

/// Runtime controller for managing a spawned agent runtime
pub struct RuntimeController {
    /// Handle for communicating with the runtime
    pub handle: RuntimeHandle,
    /// Join handle for the main runtime task (Option to allow take on abort)
    task_handle: Option<JoinHandle<()>>,
    /// Runtime readiness channel
    ready_rx: Option<oneshot::Receiver<std::result::Result<RuntimeStartupMetadata, String>>>,
    /// Cached startup metadata for repeated readiness checks and child-launch introspection.
    startup_metadata: Option<RuntimeStartupMetadata>,
}

impl RuntimeController {
    /// Returns true if the runtime task has already exited.
    pub fn is_finished(&self) -> bool {
        self.task_handle
            .as_ref()
            .map(tokio::task::JoinHandle::is_finished)
            .unwrap_or(true)
    }

    /// Wait until the runtime has completed startup.
    pub async fn wait_until_ready(&mut self) -> Result<RuntimeStartupMetadata> {
        if let Some(metadata) = self.startup_metadata.clone() {
            return Ok(metadata);
        }

        let Some(ready_rx) = self.ready_rx.take() else {
            return Ok(RuntimeStartupMetadata {
                process_path: String::new(),
                agent_path: String::new(),
                rollout_id: None,
                rollout_path: None,
                durability: AgentMachineDurabilityState {
                    durable: true,
                    required: false,
                },
                execution_backend: current_execution_backend(),
                request_controls: crate::ResolvedRequestControls::default(),
                warnings: Vec::new(),
            });
        };

        match ready_rx.await {
            Ok(Ok(metadata)) => {
                self.startup_metadata = Some(metadata.clone());
                Ok(metadata)
            }
            Ok(Err(message)) => Err(anyhow::anyhow!(message)),
            Err(_) => Err(anyhow::anyhow!(
                "Runtime stopped before signaling startup readiness"
            )),
        }
    }

    /// Shutdown the runtime gracefully and wait for it to complete
    ///
    /// First sends shutdown signal, then waits up to 10s for graceful shutdown.
    /// If timeout occurs, the task is explicitly aborted and awaited to ensure
    /// the runtime is truly stopped.
    pub async fn shutdown(mut self) -> Result<()> {
        // No longer need readiness signal once shutdown starts.
        self.ready_rx.take();

        // Send shutdown signal
        if let Some(ref tx) = self.handle.shutdown_tx
            && tx.send(()).await.is_err()
        {
            warn!("Shutdown channel closed - runtime may already be stopped");
        }

        // Close submission channel to stop accepting new work
        drop(self.handle.submission_tx);

        // Wait for the main task to complete with timeout
        let timeout = tokio::time::Duration::from_secs(10);

        // Use &mut handle so we don't consume it on timeout
        if let Some(ref mut handle) = self.task_handle {
            match tokio::time::timeout(timeout, &mut *handle).await {
                Ok(Ok(())) => {
                    info!("Runtime task completed gracefully");
                    Ok(())
                }
                Ok(Err(e)) => {
                    // Task panicked
                    Err(anyhow::anyhow!("Runtime task panicked: {}", e))
                }
                Err(_) => {
                    // Timeout - explicitly abort the task
                    warn!("Runtime shutdown timeout, aborting task");
                    handle.abort();
                    // Wait for the aborted task to complete
                    match tokio::time::timeout(Duration::from_secs(5), handle).await {
                        Ok(_) => {
                            info!("Runtime task aborted successfully");
                            Ok(())
                        }
                        Err(_) => Err(anyhow::anyhow!("Runtime shutdown timeout and abort failed")),
                    }
                }
            }
        } else {
            Err(anyhow::anyhow!("Task handle not available"))
        }
    }

    /// Abort the runtime immediately without waiting for graceful shutdown
    ///
    /// This takes ownership of the task handles and aborts them immediately.
    /// Use this when you need to guarantee the runtime stops.
    pub async fn abort(mut self) {
        // No longer need readiness signal once abort starts.
        self.ready_rx.take();

        // Send shutdown signal first (best effort)
        if let Some(ref tx) = self.handle.shutdown_tx {
            let _ = tx.try_send(());
        }

        // Take and abort the runtime task.
        if let Some(handle) = self.task_handle.take() {
            handle.abort();
            // Wait for the task to actually stop
            let _ = tokio::time::timeout(Duration::from_secs(5), handle).await;
        }
    }
}

/// Apply the Process Launch Context's explicit Host Mount authority to Tool execution.
pub fn configure_runtime_tool_execution_binding(
    config: &AgentProcessConfig,
    tools: &mut crate::tools::ToolRegistry,
) -> Result<()> {
    if !config.launch_context.host_mounts.is_empty() {
        let scratch_dir = config
            .store_bindings
            .as_ref()
            .map(|stores| stores.tmp.clone())
            .context(
                "Agent Process with Host Mounts requires Agent Runtime Service store bindings",
            )?;
        tools.set_default_execution_binding(
            crate::tools::ToolExecutionBinding::from_launch_context(
                &config.launch_context,
                scratch_dir,
            )?,
        );
    }

    Ok(())
}

pub fn effective_core_config_for_runtime(
    config: &AgentProcessConfig,
) -> Result<crate::config::Config> {
    let resolved_agent_definition = crate::ResolvedAgentDefinition::from_launch_context(
        &config.launch_context,
        &config.agent_config.core_config.resolved_skill_overrides(),
        config.core_config_source,
    )?;
    let agent_config = resolved_agent_definition.apply_to_agent_config(&config.agent_config)?;
    let mut core_config = agent_config.core_config.clone();
    if let Some(memory_store) = config.memory_store_backing.as_ref() {
        let memory_descriptor = config
            .launch_context
            .descriptor(crate::MEMORY_STORE_DESCRIPTOR)
            .context("Agent Runtime Service memory backing requires a Memory Store descriptor")?;
        anyhow::ensure!(
            memory_descriptor.path == "/memory",
            "Memory Store descriptor must reference /memory"
        );
        core_config.memory.store_dir = Some(memory_store.clone());
    } else {
        core_config.memory.store_dir = None;
    }
    crate::resolve_runtime_request_controls(
        &core_config,
        crate::provider_capabilities_for_config(&core_config),
        agent_config.runtime_config.request_control_intent,
    )?;

    Ok(core_config)
}

/// Start Agent Execution Engine over an already-assembled Process namespace.
///
/// This entry point does not create Kernel, `/proc`, `/srv`, AgentFS, or system
/// services. Alan OS Host owns those and supplies the complete environment.
pub fn spawn_with_namespace_environment(
    config: AgentProcessConfig,
    namespace: super::NamespaceRuntimeEnvironment,
    host_capabilities: crate::skills::SkillHostCapabilities,
    generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    spawn_with_prepared_runtime_environment(
        config,
        namespace,
        host_capabilities,
        generation_capabilities,
    )
}

fn spawn_with_prepared_runtime_environment(
    config: AgentProcessConfig,
    environment: NamespaceRuntimeEnvironment,
    host_capabilities: crate::skills::SkillHostCapabilities,
    _generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    let (sub_tx, mut sub_rx) = mpsc::channel::<Submission>(32);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (ready_tx, ready_rx) =
        oneshot::channel::<std::result::Result<RuntimeStartupMetadata, String>>();

    let resolved_agent_definition = crate::ResolvedAgentDefinition::from_launch_context(
        &config.launch_context,
        &config.agent_config.core_config.resolved_skill_overrides(),
        config.core_config_source,
    )?;
    let agent_config = resolved_agent_definition.apply_to_agent_config(&config.agent_config)?;
    let mut core_config = agent_config.core_config.clone();
    if let Some(memory_store) = config.memory_store_backing.as_ref() {
        let memory_descriptor = config
            .launch_context
            .descriptor(crate::MEMORY_STORE_DESCRIPTOR)
            .context("Agent Runtime Service memory backing requires a Memory Store descriptor")?;
        anyhow::ensure!(
            memory_descriptor.path == "/memory",
            "Memory Store descriptor must reference /memory"
        );
        core_config.memory.store_dir = Some(memory_store.clone());
    } else {
        core_config.memory.store_dir = None;
    }

    let mut runtime_config = agent_config.runtime_config;
    runtime_config.store_bindings = config.store_bindings.clone();
    runtime_config.memory_store_backing = config.memory_store_backing.clone();
    runtime_config.policy_engine =
        if let Some(tree) = resolved_agent_definition.descriptor_tree.as_ref() {
            crate::policy::PolicyEngine::load_for_governance_from_file_tree(
                tree,
                &runtime_config.governance,
            )
        } else {
            crate::policy::PolicyEngine::load_for_governance_with_default_policy_path(
                resolved_agent_definition.root_dir.as_deref(),
                resolved_agent_definition.policy_path.as_deref(),
                &runtime_config.governance,
            )
        };
    let prompt_cache_persona_dirs = resolved_agent_definition.persona_dirs.clone();
    if core_config.memory.enabled
        && let Some(memory_dir) = core_config.memory.store_dir.as_deref()
        && let Err(err) = crate::prompts::ensure_memory_store_layout_at(memory_dir)
    {
        warn!(
            path = %memory_dir.display(),
            error = %err,
            "Failed to initialize Memory Store layout; continuing without bootstrap writes"
        );
    }
    let rollouts_dir = config
        .store_bindings
        .as_ref()
        .map(|stores| stores.rollouts.clone());
    let rollout_cwd = std::path::PathBuf::from(&config.launch_context.cwd);
    let recovery_rollout_path = config.recovery_rollout_path;
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let mut prompt_cache =
        super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            resolved_agent_definition.capability_view.clone(),
            resolved_agent_definition.skill_overrides.clone(),
            prompt_cache_persona_dirs.clone(),
            host_capabilities,
        );
    prompt_cache.set_fixed_definition_persona_section(resolved_agent_definition.persona_context);
    prompt_cache.set_memory_store_dir(
        core_config
            .memory
            .enabled
            .then(|| core_config.memory.store_dir.clone())
            .flatten(),
    );

    // Spawn the main runtime task
    let task_handle = tokio::spawn(async move {
        let process_path = match environment.process_path() {
            Ok(path) => path,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let agent_path = environment.agent_path().to_string();
        let model = core_config.effective_model().to_string();
        let machine_request_controls = match crate::resolve_runtime_request_controls(
            &core_config,
            generation_capabilities,
            runtime_config.request_control_intent,
        ) {
            Ok(controls) => controls,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let startup = match initialize_agent_machine(
            AgentMachineLaunchContext {
                process_path: &process_path,
                agent_path: &agent_path,
                model: &model,
            },
            recovery_rollout_path.as_ref(),
            rollouts_dir.as_ref(),
            runtime_config.durability_required,
            Some(rollout_cwd.as_path()),
            machine_request_controls,
        )
        .await
        {
            Ok(startup) => startup,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
        let machine = startup.machine;

        // Build agent loop state
        let mut state = RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment,
            core_config,
            runtime_config,
            definition_persona_dirs: prompt_cache_persona_dirs.clone(),
            prompt_cache,
            turn_state: super::TurnState::default(),
        };
        match super::ui_surfaces::initialize(state.namespace_environment()).await {
            Ok(()) => {}
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };

        info!(
            process_path = %state.process_path(),
            agent_path = %state.agent_path(),
            "Agent runtime started"
        );
        let _ = ready_tx.send(Ok(startup.metadata));

        // Main event loop with graceful shutdown support and interruptible submissions.
        let mut submissions_closed = false;
        let mut shutdown_requested = false;

        let mut queues = RuntimeSubmissionQueues::default();
        let input_environment = state.namespace_environment().clone();
        let (namespace_input_tx, mut namespace_input_rx) = mpsc::channel(1);
        let namespace_input_task = tokio::spawn(async move {
            loop {
                let submission = input_environment
                    .read_next_input_submission(InputMode::FollowUp)
                    .await;
                if namespace_input_tx.send(submission).await.is_err() {
                    break;
                }
            }
        });

        loop {
            let queued_item = if shutdown_requested {
                queues.pop_outer_deferred()
            } else if let Some(queued_item) = queues.pop_outer() {
                Some(queued_item)
            } else if let Some(namespace_resume) =
                read_pending_namespace_resume_submission(&state).await
            {
                match namespace_resume {
                    Ok(submission) => Some(QueuedRuntimeItem::Submission(submission)),
                    Err(err) => {
                        error!(
                            error = %format!("{err:#}"),
                            "Failed to read namespace answered request response"
                        );
                        None
                    }
                }
            } else if let Some(namespace_control) =
                read_pending_namespace_control_submission(state.namespace_environment()).await
            {
                match namespace_control {
                    Ok(submission) => Some(QueuedRuntimeItem::Submission(submission)),
                    Err(err) => {
                        error!(
                            error = %format!("{err:#}"),
                            "Failed to read namespace machine/ctl command"
                        );
                        None
                    }
                }
            } else if submissions_closed {
                None
            } else {
                let namespace_control = state.namespace_environment().clone();
                let poll_pending_namespace_response = state.turn_state.has_pending_interaction();
                tokio::select! {
                    submission = sub_rx.recv() => submission.map(QueuedRuntimeItem::Submission),
                    namespace_submission = namespace_input_rx.recv() => {
                        match namespace_submission {
                            Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                            Some(Err(err)) => {
                                error!(error = %format!("{err:#}"), "Failed to read namespace io/input frame");
                                None
                            }
                            None => None,
                        }
                    }
                    _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                        match read_pending_namespace_control_submission(&namespace_control).await {
                            Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                            Some(Err(err)) => {
                                error!(
                                    error = %format!("{err:#}"),
                                    "Failed to read namespace machine/ctl command"
                                );
                                None
                            }
                            None if poll_pending_namespace_response => {
                                match read_pending_namespace_resume_submission(&state).await {
                                    Some(Ok(submission)) => Some(QueuedRuntimeItem::Submission(submission)),
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace answered request response"
                                        );
                                        None
                                    }
                                    None => None,
                                }
                            }
                            None => None,
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        shutdown_requested = true;
                        submissions_closed = true;
                        None
                    }
                }
            };

            let Some(queued_item) = queued_item else {
                if shutdown_requested || submissions_closed {
                    if shutdown_requested {
                        info!(
                            process_path = %state.namespace_environment().agent_path(),
                            "Shutdown signal received, stopping runtime"
                        );
                    }
                    break;
                }
                continue;
            };

            match queued_item {
                QueuedRuntimeItem::Submission(submission) => {
                    debug!(?submission.id, "Received submission");
                    let drive_as_turn_submission = should_drive_turn_submission(&submission.op);
                    state.current_submission_id = Some(submission.id.clone());

                    let cancel = CancellationToken::new();
                    let mut emit = |_event: Event| async {};

                    let broker_for_submission = queues.active_turn_broker.clone();
                    let namespace_control = state.namespace_environment().clone();
                    let namespace_heartbeat = state.namespace_environment().clone();
                    let mut submission_fut: std::pin::Pin<
                        Box<dyn std::future::Future<Output = Result<()>> + Send + '_>,
                    > = if drive_as_turn_submission {
                        Box::pin(drive_turn_submission_with_cancel(
                            &mut state,
                            submission,
                            &broker_for_submission,
                            &mut emit,
                            &cancel,
                        ))
                    } else {
                        Box::pin(handle_submission_with_cancel(
                            &mut state, submission, &mut emit, &cancel,
                        ))
                    };
                    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
                    heartbeat_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

                    loop {
                        tokio::select! {
                            result = &mut submission_fut => {
                                drop(submission_fut);
                                let terminal_ui_result = match &result {
                                    Ok(()) if preserves_paused_terminal_state(
                                        &result,
                                        state.turn_state.has_pending_interaction(),
                                    ) => Ok(()),
                                    Ok(()) => super::ui_surfaces::turn_completed(
                                        &namespace_heartbeat,
                                        false,
                                    )
                                    .await,
                                    Err(err) => super::ui_surfaces::turn_failed(
                                        &namespace_heartbeat,
                                        &format!("Error handling submission: {err}"),
                                    )
                                    .await,
                                };
                                if let Err(err) = terminal_ui_result {
                                    warn!(error = %err, "Failed to write terminal runtime state");
                                }
                                if drive_as_turn_submission {
                                    let _ = queues
                                        .requeue_active_turn_leftovers(&mut state.turn_state)
                                        .await;
                                }
                                if let Err(e) = result {
                                    let error_msg = format!("Error handling submission: {}", e);
                                    error!(error = %error_msg);
                                }
                                queues.outer_queue.extend(
                                    state
                                        .turn_state
                                        .drain_deferred_runtime_actions()
                                        .into_iter()
                                        .map(QueuedRuntimeItem::Deferred),
                                );
                                state.current_submission_id = None;
                                break;
                            }
                            incoming = sub_rx.recv(), if !submissions_closed => {
                                match incoming {
                                    Some(incoming) => {
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else if drive_as_turn_submission
                                            && is_turn_inband_submission(&incoming.op)
                                        {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    None => {
                                        submissions_closed = true;
                                        cancel.cancel();
                                    }
                                }
                            }
                            namespace_submission = namespace_input_rx.recv() => {
                                match namespace_submission {
                                    Some(Ok(incoming)) => {
                                        if drive_as_turn_submission && is_turn_inband_submission(&incoming.op) {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        let error_msg = format!("Failed to read namespace io/input frame: {err:#}");
                                        error!(error = %error_msg);
                                        let _ = super::ui_surfaces::warning(
                                            &namespace_heartbeat,
                                            error_msg,
                                        ).await;
                                    }
                                    None => {}
                                }
                            }
                            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                                match read_pending_namespace_control_submission(&namespace_control).await {
                                    Some(Ok(incoming)) => {
                                        // A machine/ctl interrupt must cancel the
                                        // running generation/tool immediately, like
                                        // an Op::Interrupt arriving on sub_rx.
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else if drive_as_turn_submission && is_turn_inband_submission(&incoming.op) {
                                            if !queues.active_turn_broker.push(incoming.clone()).await {
                                                queues.push_outer_submission(incoming);
                                            }
                                        } else {
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        let error_msg = format!("Failed to read namespace machine/ctl command: {err:#}");
                                        error!(error = %error_msg);
                                        let _ = super::ui_surfaces::warning(
                                            &namespace_heartbeat,
                                            error_msg,
                                        ).await;
                                    }
                                    None => {}
                                }
                            }
                            _ = heartbeat_interval.tick() => {
                                if let Err(err) = super::ui_surfaces::heartbeat(&namespace_heartbeat).await {
                                    warn!(error = %err, "Failed to write runtime activity heartbeat");
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                shutdown_requested = true;
                                submissions_closed = true;
                                cancel.cancel();
                            }
                        }

                        if shutdown_requested {
                            continue;
                        }
                    }
                }
                QueuedRuntimeItem::Deferred(action) => {
                    let action_for_requeue = action.clone();
                    let mut requeue_if_cancelled = false;
                    let cancel = CancellationToken::new();
                    let namespace_control = state.namespace_environment().clone();
                    let mut action_fut = Box::pin(run_deferred_runtime_action_with_cancel(
                        &mut state, action, &cancel,
                    ));

                    loop {
                        tokio::select! {
                            exit = &mut action_fut => {
                                drop(action_fut);
                                if should_requeue_deferred_action(requeue_if_cancelled, exit) {
                                    queues.push_outer_deferred(action_for_requeue);
                                }
                                break;
                            }
                            incoming = sub_rx.recv(), if !submissions_closed => {
                                match incoming {
                                    Some(incoming) => {
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else {
                                            requeue_if_cancelled = true;
                                            cancel.cancel();
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    None => {
                                        submissions_closed = true;
                                    }
                                }
                            }
                            namespace_submission = namespace_input_rx.recv() => {
                                match namespace_submission {
                                    Some(Ok(incoming)) => {
                                        requeue_if_cancelled = true;
                                        cancel.cancel();
                                        queues.push_outer_submission(incoming);
                                    }
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace io/input frame during deferred action"
                                        );
                                    }
                                    None => {}
                                }
                            }
                            _ = tokio::time::sleep(NAMESPACE_PENDING_RESPONSE_POLL_INTERVAL) => {
                                match read_pending_namespace_control_submission(&namespace_control).await {
                                    Some(Ok(incoming)) => {
                                        // Mirror the sub_rx arm: a machine/ctl
                                        // interrupt just cancels the deferred
                                        // action; other control ops preempt and
                                        // requeue it.
                                        if matches!(incoming.op, alan_agent_protocol::Op::Interrupt) {
                                            cancel.cancel();
                                        } else {
                                            requeue_if_cancelled = true;
                                            cancel.cancel();
                                            queues.push_outer_submission(incoming);
                                        }
                                    }
                                    Some(Err(err)) => {
                                        error!(
                                            error = %format!("{err:#}"),
                                            "Failed to read namespace machine/ctl command during deferred action"
                                        );
                                    }
                                    None => {}
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                shutdown_requested = true;
                                submissions_closed = true;
                            }
                        }
                    }
                }
            }
        }

        namespace_input_task.abort();
        let _ = namespace_input_task.await;
        info!(
            process_path = %state.namespace_environment().agent_path(),
            "Agent runtime stopped"
        );
        state.machine.flush().await;
    });

    Ok(RuntimeController {
        handle: RuntimeHandle {
            submission_tx: sub_tx,
            shutdown_tx: Some(shutdown_tx),
        },
        task_handle: Some(task_handle),
        ready_rx: Some(ready_rx),
        startup_metadata: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{agent_loop::DeferredRuntimeAction, memory_promotion};
    use alan_agent_protocol::{ContentPart, Op};
    use alan_ap::InProcessTransport;
    use alan_llm::{
        GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk,
        TokenUsage, ToolCallDelta,
    };
    use anyhow::anyhow;
    use async_trait::async_trait;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct PackageTestTool {
        name: &'static str,
        description: &'static str,
    }

    impl crate::tools::Tool for PackageTestTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            self.description
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        fn execute(
            &self,
            _arguments: serde_json::Value,
            _ctx: &crate::tools::ToolContext,
        ) -> crate::tools::ToolResult {
            Box::pin(async { Ok(serde_json::json!({"ok": true})) })
        }
    }

    fn single_file_fs(name: &str, bytes: &[u8]) -> Arc<alan_ap::reference::MemFs> {
        Arc::new(alan_ap::reference::MemFs::with_read_only_file(name, bytes))
    }

    fn namespace_environment_for_test() -> NamespaceRuntimeEnvironment {
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        )));
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
    }

    fn write_agent_overlay(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    fn make_deferred_action_for_test() -> DeferredRuntimeAction {
        let temp = TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory-store");

        let mut machine = AgentMachine::new();
        machine.add_user_message("My name is Morris.");

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(0);

        let mut core_config = crate::Config::default();
        core_config.memory.enabled = true;
        core_config.memory.store_dir = Some(memory_dir);
        let runtime_config = RuntimeConfig::from(&core_config);

        let state = RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            core_config,
            runtime_config,
            definition_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        };

        memory_promotion::build_turn_memory_promotion_job(&state, "queue ordering test")
            .map(DeferredRuntimeAction::TurnMemoryPromotion)
            .expect("build deferred memory promotion job")
    }

    fn queue_item_kinds(queue: &VecDeque<QueuedRuntimeItem>) -> Vec<&'static str> {
        queue
            .iter()
            .map(|item| match item {
                QueuedRuntimeItem::Submission(_) => "submission",
                QueuedRuntimeItem::Deferred(_) => "deferred",
            })
            .collect()
    }

    #[tokio::test]
    async fn namespace_discovery_ignores_incomplete_tool_packages() {
        let manifest = crate::runtime::ToolPackageManifest::from_tool(
            &PackageTestTool {
                name: "hidden",
                description: "Hidden Tool",
            },
            30,
        )
        .unwrap();
        let manifest_fs = single_file_fs("manifest", &serde_json::to_vec(&manifest).unwrap());

        let mut mounts = alan_kernel::Namespace::new();
        mounts.mount(
            "/bin/ordinary",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadOnly,
        );
        mounts.mount(
            "/lib/exec/hidden",
            InProcessTransport::new(manifest_fs),
            alan_kernel::Access::ReadOnly,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(mounts)));
        let environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        assert!(
            environment
                .discover_tool_packages()
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn namespace_discovery_rejects_invalid_mounted_manifest() {
        let mut mounts = alan_kernel::Namespace::new();
        mounts.mount(
            "/bin/broken",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadOnly,
        );
        mounts.mount(
            "/lib/exec/broken",
            InProcessTransport::new(single_file_fs("manifest", b"{}")),
            alan_kernel::Access::ReadOnly,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(mounts)));
        let environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        assert!(environment.discover_tool_packages().await.is_err());
    }

    #[test]
    fn test_should_requeue_deferred_action_only_after_cancelled_exit() {
        assert!(should_requeue_deferred_action(
            true,
            DeferredRuntimeActionExit::Cancelled
        ));
        assert!(!should_requeue_deferred_action(
            true,
            DeferredRuntimeActionExit::Completed
        ));
        assert!(!should_requeue_deferred_action(
            false,
            DeferredRuntimeActionExit::Cancelled
        ));
    }

    #[test]
    fn paused_submission_keeps_its_terminal_ui_state() {
        assert!(preserves_paused_terminal_state(&Ok(()), true));
        assert!(!preserves_paused_terminal_state(&Ok(()), false));
        assert!(!preserves_paused_terminal_state(
            &Err(anyhow!("failed")),
            true
        ));
    }

    fn mock_generation_response(content: impl Into<String>) -> GenerationResponse {
        GenerationResponse {
            content: content.into(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }
    }

    async fn wait_for_ui_turn_completion(
        tail: &mut alan_shell::Tail,
        timeout: Duration,
    ) -> Vec<alan_agent_protocol::UiEvent> {
        tokio::time::timeout(timeout, async {
            let mut pending = String::new();
            let mut events = Vec::new();
            let mut saw_running = false;
            loop {
                pending.push_str(&String::from_utf8(tail.read(4096).await.unwrap()).unwrap());
                while let Some(newline) = pending.find('\n') {
                    let line = pending[..newline].to_string();
                    pending.drain(..=newline);
                    let event: alan_agent_protocol::UiEvent = serde_json::from_str(&line).unwrap();
                    if let alan_agent_protocol::UiEvent::Activity { snapshot } = &event {
                        saw_running |= matches!(
                            snapshot.state,
                            alan_agent_protocol::UiActivityState::Running
                        );
                        if saw_running
                            && matches!(snapshot.state, alan_agent_protocol::UiActivityState::Idle)
                        {
                            events.push(event);
                            return events;
                        }
                    }
                    events.push(event);
                }
            }
        })
        .await
        .expect("turn UI stream did not reach idle")
    }

    fn response_stream(response: GenerationResponse) -> tokio::sync::mpsc::Receiver<StreamChunk> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            if !response.content.is_empty()
                || response
                    .thinking
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                || response
                    .thinking_signature
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
                || !response.redacted_thinking.is_empty()
            {
                let mut redacted = response.redacted_thinking.into_iter();
                let _ = tx
                    .send(StreamChunk {
                        text: (!response.content.is_empty()).then_some(response.content),
                        thinking: response.thinking,
                        thinking_signature: response.thinking_signature,
                        redacted_thinking: redacted.next(),
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
                for redacted in redacted {
                    let _ = tx
                        .send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: Some(redacted),
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: false,
                            finish_reason: None,
                        })
                        .await;
                }
            }

            let tool_calls = response.tool_calls;
            for (index, tool_call) in tool_calls.iter().enumerate() {
                let arguments =
                    serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: Some(ToolCallDelta {
                            index,
                            id: tool_call.id.clone(),
                            name: Some(tool_call.name.clone()),
                            arguments_delta: Some(arguments.clone()),
                            arguments: Some(arguments),
                        }),
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
            }

            let finish_reason = response.finish_reason.unwrap_or_else(|| {
                if tool_calls.is_empty() {
                    "stop".to_string()
                } else {
                    "tool_calls".to_string()
                }
            });
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: response.usage,
                    provider_response_id: response.provider_response_id,
                    provider_response_status: response.provider_response_status,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some(finish_reason),
                })
                .await;
        });
        rx
    }

    struct ShutdownDrainMemoryPromotionProvider {
        call_count: Arc<Mutex<usize>>,
        deferred_delay: Duration,
    }

    #[async_trait]
    impl LlmProvider for ShutdownDrainMemoryPromotionProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            let current_call = {
                let mut guard = self.call_count.lock().unwrap();
                let current = *guard;
                *guard += 1;
                current
            };

            match current_call {
                0 => Ok(mock_generation_response("Noted.")),
                1 => {
                    tokio::time::sleep(self.deferred_delay).await;
                    Ok(mock_generation_response(
                        serde_json::json!({
                            "writes": [
                                {
                                    "kind": "user_identity",
                                    "target": "USER.md",
                                    "confidence": "high",
                                    "disposition": "promote_now",
                                    "observation": "Name: Morris",
                                    "evidence": ["My name is Morris."],
                                    "promotion_rationale": "Direct user-stated stable identity detail."
                                }
                            ]
                        })
                        .to_string(),
                    ))
                }
                _ => Ok(mock_generation_response(
                    serde_json::json!({ "writes": [] }).to_string(),
                )),
            }
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Err(anyhow!(
                "ShutdownDrainMemoryPromotionProvider does not implement chat"
            ))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            Ok(response_stream(self.generate(request).await?))
        }

        fn provider_name(&self) -> &'static str {
            "shutdown_drain_memory_promotion"
        }
    }

    #[test]
    fn test_push_outer_submission_inserts_before_existing_deferred_actions() {
        let mut queues = RuntimeSubmissionQueues::default();

        let first_submission = Submission::new(Op::Interrupt);
        let second_submission = Submission::new(Op::CompactWithOptions { focus: None });
        let first_submission_id = first_submission.id.clone();
        let second_submission_id = second_submission.id.clone();

        queues.push_outer_submission(first_submission);
        queues.push_outer_deferred(make_deferred_action_for_test());
        queues.push_outer_deferred(make_deferred_action_for_test());
        queues.push_outer_submission(second_submission);

        assert_eq!(
            queue_item_kinds(&queues.outer_queue),
            vec!["submission", "submission", "deferred", "deferred"]
        );

        let queued_submission_ids = queues
            .outer_queue
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(submission) => Some(submission.id.clone()),
                QueuedRuntimeItem::Deferred(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            queued_submission_ids,
            vec![first_submission_id, second_submission_id]
        );
    }

    #[tokio::test]
    async fn test_requeue_active_turn_leftovers_inserts_before_existing_deferred_actions() {
        let mut queues = RuntimeSubmissionQueues::default();
        queues.push_outer_deferred(make_deferred_action_for_test());

        let mut turn_state = TurnState::default();
        let buffered_submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
            mode: alan_agent_protocol::InputMode::FollowUp,
        });
        let buffered_submission_id = buffered_submission.id.clone();
        turn_state.push_buffered_inband_submission(buffered_submission);

        let requeued = queues.requeue_active_turn_leftovers(&mut turn_state).await;

        assert_eq!(requeued, 1);
        assert_eq!(
            queue_item_kinds(&queues.outer_queue),
            vec!["submission", "deferred"]
        );

        match queues.outer_queue.front() {
            Some(QueuedRuntimeItem::Submission(submission)) => {
                assert_eq!(submission.id, buffered_submission_id);
            }
            _ => panic!("expected buffered submission at queue front"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_runtime_shutdown_drains_deferred_memory_promotion_actions() {
        let temp = TempDir::new().unwrap();
        let system_store = temp.path().join("system-store");
        let memory_dir = system_store.join("memory");
        crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
        let store_bindings = crate::AgentRuntimeStoreBindings {
            rollouts: system_store.join("rollouts"),
            checkpoints: system_store.join("checkpoints"),
            cache: system_store.join("cache"),
            tmp: system_store.join("tmp"),
            metadata: system_store.join("metadata"),
        };
        for path in [
            &store_bindings.rollouts,
            &store_bindings.checkpoints,
            &store_bindings.cache,
            &store_bindings.tmp,
            &store_bindings.metadata,
        ] {
            std::fs::create_dir_all(path).unwrap();
        }

        let mut core_config = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core_config.memory.enabled = true;
        core_config.memory.store_dir = Some(memory_dir.clone());
        core_config.streaming_mode = crate::config::StreamingMode::Off;

        let mut agent_config = crate::AgentConfig::from(core_config);
        agent_config.runtime_config.streaming_mode = crate::config::StreamingMode::Off;

        let config = AgentProcessConfig {
            agent_config,
            launch_context: crate::ProcessLaunchContext::root().with_descriptor(
                crate::MEMORY_STORE_DESCRIPTOR,
                crate::ProcessDescriptor::new("/memory").unwrap(),
            ),
            store_bindings: Some(store_bindings),
            memory_store_backing: Some(memory_dir.clone()),
            ..AgentProcessConfig::default()
        };
        let call_count = Arc::new(Mutex::new(0));
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(ShutdownDrainMemoryPromotionProvider {
                call_count: Arc::clone(&call_count),
                deferred_delay: Duration::from_millis(100),
            }),
        );
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(agentfs),
            alan_kernel::Access::ReadWrite,
        );
        namespace.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let mut output = shell.tail("/agent/1/io/output").await.unwrap();
        let environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let generation_capabilities =
            crate::provider_capabilities_for_config(&config.agent_config.core_config);
        let mut controller = spawn_with_namespace_environment(
            config,
            environment,
            crate::skills::SkillHostCapabilities::default(),
            generation_capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();

        let submission = Submission::new(Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("My name is Morris.")],
            context: None,
        });
        controller
            .handle
            .submission_tx
            .send(submission.clone())
            .await
            .unwrap();

        let output = tokio::time::timeout(Duration::from_secs(15), output.read(4096))
            .await
            .expect("turn output did not arrive")
            .unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "Noted.");

        controller.shutdown().await.unwrap();

        let user_memory =
            tokio::fs::read_to_string(memory_dir.join(crate::prompts::MEMORY_USER_FILENAME))
                .await
                .unwrap();
        assert!(
            user_memory.contains("Name: Morris"),
            "provider_calls={}, user_memory={user_memory:?}",
            *call_count.lock().unwrap()
        );
    }

    #[tokio::test]
    async fn test_spawn_with_namespace_environment_reaches_ready_without_store_bindings() {
        let core_config = crate::Config::default();
        let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
        let config = AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..AgentProcessConfig::default()
        };
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let namespace_environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let mut controller = spawn_with_namespace_environment(
            config,
            namespace_environment,
            crate::skills::SkillHostCapabilities::default(),
            generation_capabilities,
        )
        .unwrap();
        let ready = controller.wait_until_ready().await.unwrap();

        assert_eq!(ready.process_path, "/proc/1");
        assert_eq!(ready.agent_path, "/agent/1");
        assert!(ready.rollout_id.is_none());
        assert!(!ready.durability.durable);
        controller.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_namespace_io_input_frame_drives_runtime_turn_without_api_submission() {
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        let mock = MockLlmProvider::new().with_responses(vec![
            GenerationResponse {
                content: "first namespace response".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            },
            GenerationResponse {
                content: "second namespace response".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            },
        ]);
        let mock_probe = mock.clone();
        llmfs.register_connection("default", Box::new(mock));

        let procfs = Arc::new(alan_kernel::ProcFs::new());
        let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(procfs.clone()));
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/proc",
            alan_ap::InProcessTransport::new(procfs),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/agent",
            alan_ap::InProcessTransport::new(agent_root.clone()),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        for path in ["/bin", "/lib", "/man", "/mnt"] {
            ns.mount(
                path,
                alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                alan_kernel::Access::ReadOnly,
            );
        }
        for name in ["read_file", "write_file", "search_files", "run_command"] {
            let manifest = crate::runtime::ToolPackageManifest::from_tool(
                &PackageTestTool {
                    name,
                    description: "Host-mounted test Tool",
                },
                30,
            )
            .unwrap();
            ns.mount(
                &format!("/bin/{name}"),
                alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
                alan_kernel::Access::ReadOnly,
            );
            ns.mount(
                &format!("/lib/exec/{name}"),
                alan_ap::InProcessTransport::new(Arc::new(
                    alan_ap::reference::MemFs::with_read_only_file(
                        "manifest",
                        serde_json::to_vec(&manifest).unwrap(),
                    ),
                )),
                alan_kernel::Access::ReadOnly,
            );
        }
        let live_namespace = alan_kernel::LiveNamespace::new(ns);
        let root = alan_ap::InProcessTransport::new(Arc::new(
            alan_kernel::MountFs::from_live_namespace(live_namespace.clone()),
        ));
        let bootstrap_shell = alan_shell::Shell::new(root.clone());
        let pid = bootstrap_shell
            .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
            .await
            .unwrap();
        assert_eq!(pid, "1");
        agent_root.bind_process(pid.clone(), agentfs).await;
        agent_root.set_root_process(pid).await;
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(client_stream);
        let (server_read, server_write) = tokio::io::split(server_stream);
        let attachment_mount = Arc::new(alan_kernel::MountFs::from_live_namespace(live_namespace));
        let server_task = tokio::spawn(alan_ap::export_file_server(
            attachment_mount,
            tokio::io::BufReader::new(server_read),
            server_write,
        ));
        let imported = Arc::new(alan_ap::ImportedFileServer::new(
            tokio::io::BufReader::new(client_read),
            client_write,
        ));
        let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(imported));

        let mut core_config = crate::Config::default();
        core_config.memory.enabled = false;
        let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
        let store = TempDir::new().unwrap();
        let store_bindings = crate::AgentRuntimeStoreBindings {
            rollouts: store.path().join("rollouts"),
            checkpoints: store.path().join("checkpoints"),
            cache: store.path().join("cache"),
            tmp: store.path().join("tmp"),
            metadata: store.path().join("metadata"),
        };
        for path in [
            &store_bindings.rollouts,
            &store_bindings.checkpoints,
            &store_bindings.cache,
            &store_bindings.tmp,
            &store_bindings.metadata,
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        let config = AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            store_bindings: Some(store_bindings),
            ..AgentProcessConfig::default()
        };
        let namespace_environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let mut controller = spawn_with_namespace_environment(
            config,
            namespace_environment,
            crate::skills::SkillHostCapabilities::default(),
            generation_capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();

        let mut ui_events = shell.tail("/agent/1/machine/ui/events").await.unwrap();
        shell
            .write("/agent/1/io/input", b"hello through files")
            .await
            .unwrap();

        wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;

        let output = String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap();
        assert_eq!(output, "first namespace response");

        shell
            .write("/agent/1/io/input", b"second input through files")
            .await
            .unwrap();
        wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;
        let output = String::from_utf8(shell.cat("/agent/1/io/output").await.unwrap()).unwrap();
        assert_eq!(output, "first namespace responsesecond namespace response");
        assert_eq!(mock_probe.recorded_requests().len(), 2);

        controller.shutdown().await.unwrap();
        drop(shell);
        server_task.abort();
    }

    #[tokio::test]
    async fn test_outer_idle_reads_answered_namespace_request_response() {
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(agentfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let namespace_environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let request_id = namespace_environment
            .write_request(crate::runtime::agent_loop::NamespaceRequestRecord::new(
                "structured_input",
                "Provide the missing value",
            ))
            .await
            .unwrap();
        let mut turn_state = TurnState::default();
        turn_state.set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id: request_id.clone(),
            title: "Missing value".to_string(),
            prompt: "Provide the missing value".to_string(),
            questions: Vec::new(),
        });
        let state = RuntimeLoopState {
            machine: crate::AgentMachine::new(),
            current_submission_id: None,
            environment: namespace_environment,
            core_config: crate::Config::default(),
            runtime_config: RuntimeConfig::default(),
            definition_persona_dirs: Vec::new(),
            prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        };

        assert!(
            read_pending_namespace_resume_submission(&state)
                .await
                .is_none(),
            "unanswered request should not create a resume submission"
        );

        shell
            .write(
                &format!("/agent/1/requests/{request_id}/response"),
                br#"{"answers":[{"question_id":"q1","value":"from file"}]}"#,
            )
            .await
            .unwrap();

        let submission = read_pending_namespace_resume_submission(&state)
            .await
            .expect("answered namespace request should be observed")
            .unwrap();
        match submission.op {
            Op::Resume {
                request_id: resumed_id,
                content,
            } => {
                assert_eq!(resumed_id, request_id);
                assert_eq!(
                    content,
                    vec![ContentPart::structured(serde_json::json!({
                        "answers": [{"question_id": "q1", "value": "from file"}]
                    }))]
                );
            }
            other => panic!("expected Op::Resume from namespace response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_namespace_machine_ctl_drives_runtime_submission_without_api_submission() {
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(MockLlmProvider::new().with_response(GenerationResponse {
                content: "hello from namespace runtime".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: Some("stop".to_string()),
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            })),
        );

        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/agent/1",
            alan_ap::InProcessTransport::new(agentfs),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            alan_ap::InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());

        let core_config = crate::Config::default();
        let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
        let config = AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..AgentProcessConfig::default()
        };
        let namespace_environment =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let mut controller = spawn_with_namespace_environment(
            config,
            namespace_environment,
            crate::skills::SkillHostCapabilities::default(),
            generation_capabilities,
        )
        .unwrap();
        controller.wait_until_ready().await.unwrap();

        let mut ui_events = shell.tail("/agent/1/machine/ui/events").await.unwrap();
        shell
            .write("/agent/1/io/input", b"hello through files")
            .await
            .unwrap();

        wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;

        shell
            .write("/agent/1/machine/ctl", b"rollback")
            .await
            .unwrap();

        let rollback_notice = tokio::time::timeout(Duration::from_secs(5), async {
            let mut pending = String::new();
            'events: loop {
                pending.push_str(&String::from_utf8(ui_events.read(4096).await.unwrap()).unwrap());
                while let Some(newline) = pending.find('\n') {
                    let line = pending[..newline].to_string();
                    pending.drain(..=newline);
                    let event: alan_agent_protocol::UiEvent = serde_json::from_str(&line).unwrap();
                    if let alan_agent_protocol::UiEvent::Notice { snapshot } = event
                        && snapshot.kind == alan_agent_protocol::UiNoticeKind::Rollback
                    {
                        break 'events snapshot.message;
                    }
                }
            }
        })
        .await
        .expect("namespace machine/ctl should drive rollback submission");
        assert_eq!(rollback_notice, "rolled back 1 turns");

        controller.shutdown().await.unwrap();
    }

    #[test]
    fn test_agent_runtime_config_default() {
        let config = AgentProcessConfig::default();
        assert_eq!(config.launch_context.cwd, "/");
        assert!(config.launch_context.host_mounts.is_empty());
        assert!(config.launch_context.descriptors.is_empty());
        assert!(config.store_bindings.is_none());
        assert!(config.memory_store_backing.is_none());
    }

    #[test]
    fn runtime_tool_binding_uses_host_mount_when_process_cwd_is_virtual() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        let mut launch_context = crate::ProcessLaunchContext::root();
        launch_context.namespace.mount(
            "/mnt/source",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        launch_context = launch_context.with_host_mount(
            crate::HostMountGrant::new("/mnt/source", &source, alan_kernel::Access::ReadWrite)
                .unwrap(),
        );
        let store_root = temp.path().join("system-store");
        let config = AgentProcessConfig {
            launch_context,
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: store_root.join("rollouts"),
                checkpoints: store_root.join("checkpoints"),
                cache: store_root.join("cache"),
                tmp: store_root.join("tmp"),
                metadata: store_root.join("metadata"),
            }),
            ..AgentProcessConfig::default()
        };
        let mut tools = crate::tools::ToolRegistry::new();

        configure_runtime_tool_execution_binding(&config, &mut tools).unwrap();

        let binding = tools
            .default_execution_binding()
            .expect("an explicit Host Mount must create a runtime Tool binding");
        assert_eq!(binding.cwd, dunce::canonicalize(&source).unwrap());
        assert_eq!(binding.namespace_cwd, PathBuf::from("/mnt/source"));
        assert_eq!(config.launch_context.cwd, "/");
        assert_eq!(binding.host_mounts, config.launch_context.host_mounts);
    }

    #[test]
    fn package_projection_alone_does_not_require_runtime_tool_binding() {
        let mut launch_context = crate::ProcessLaunchContext::root();
        launch_context.add_package_reference(
            crate::ProcessPackageReference::new(
                "example",
                "a".repeat(64),
                crate::ProcessPackageKind::Installed,
                "/lib/pkg/example",
                Vec::new(),
                alan_ap::InProcessTransport::new(std::sync::Arc::new(
                    alan_ap::reference::MemFs::new(),
                )),
            )
            .unwrap(),
        );
        let config = AgentProcessConfig {
            launch_context,
            store_bindings: None,
            ..AgentProcessConfig::default()
        };
        let mut tools = crate::tools::ToolRegistry::new();

        configure_runtime_tool_execution_binding(&config, &mut tools).unwrap();

        assert!(tools.default_execution_binding().is_none());
    }

    #[test]
    fn test_runtime_host_capabilities_enable_delegated_support_for_top_level_runtime() {
        let config = AgentProcessConfig::default();
        let tools = crate::tools::ToolRegistry::new();

        let capabilities = runtime_host_capabilities(&config, &tools);

        assert!(capabilities.supports_delegated_skill_invocation());
        assert!(capabilities.tools.contains("invoke_delegated_skill"));
    }

    #[test]
    fn test_runtime_host_capabilities_include_host_path_executables() {
        let temp = tempfile::TempDir::new().unwrap();
        let executable_path = {
            #[cfg(windows)]
            {
                temp.path().join("demo.cmd")
            }

            #[cfg(not(windows))]
            {
                temp.path().join("demo")
            }
        };
        std::fs::write(&executable_path, "echo demo\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable_path, permissions).unwrap();
        }

        let capabilities = runtime_host_capabilities_with_path_dirs(
            &AgentProcessConfig::default(),
            &crate::tools::ToolRegistry::new(),
            [temp.path()],
        );

        assert!(capabilities.supports_required_tool("demo"));
    }

    #[test]
    fn test_agent_runtime_config_from_core_config() {
        let core_config = crate::config::Config::default();
        let runtime_config = AgentProcessConfig::from(core_config);

        assert_eq!(runtime_config.launch_context.cwd, "/");
        assert!(runtime_config.launch_context.host_mounts.is_empty());
        assert!(runtime_config.store_bindings.is_none());
    }

    #[test]
    fn test_agent_runtime_config_clone() {
        let config = AgentProcessConfig::default();
        let cloned = config.clone();
        assert_eq!(config.launch_context.cwd, cloned.launch_context.cwd);
        assert_eq!(
            config.launch_context.host_mounts,
            cloned.launch_context.host_mounts
        );
    }

    #[test]
    fn test_agent_runtime_config_debug() {
        let config = AgentProcessConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("AgentProcessConfig"));
        assert!(debug_str.contains("launch_context"));
        assert!(!debug_str.contains("workspace_id"));
    }

    #[test]
    fn test_agent_runtime_handle_clone() {
        let (sub_tx, _sub_rx) = mpsc::channel(10);

        let handle = RuntimeHandle {
            submission_tx: sub_tx,
            shutdown_tx: None,
        };

        let cloned = handle.clone();
        // Both handles should share the same channels
        drop(cloned);
        drop(handle);
    }

    #[test]
    fn test_agent_runtime_handle_fields() {
        let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);

        let handle = RuntimeHandle {
            submission_tx: sub_tx,
            shutdown_tx: None,
        };

        // Verify handle can be created
        assert!(!handle.submission_tx.is_closed());
    }

    #[tokio::test]
    async fn test_agent_runtime_handle_shutdown_without_channel() {
        let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);
        let handle = RuntimeHandle {
            submission_tx: sub_tx,
            shutdown_tx: None,
        };

        let result = handle.shutdown().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_config_with_definition_overlays_updates_unmodified_runtime_fields() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        write_agent_overlay(
            &overlay_path,
            r#"
	tool_repeat_limit = 9
	prompt_snapshot_enabled = true
	"#,
        );

        let base = AgentConfig::from(crate::Config::default());
        let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

        assert_eq!(merged.core_config.tool_repeat_limit, 9);
        assert!(merged.core_config.prompt_snapshot_enabled);
        assert_eq!(merged.runtime_config.tool_repeat_limit, 9);
        assert!(merged.runtime_config.prompt_snapshot_enabled);
    }

    #[test]
    fn test_agent_config_with_definition_overlays_updates_unmodified_reasoning_effort() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        write_agent_overlay(
            &overlay_path,
            r#"
model_reasoning_effort = "high"
"#,
        );

        let base = AgentConfig::from(crate::Config::default());
        let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

        assert_eq!(
            merged.core_config.model_reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
        assert_eq!(
            merged
                .runtime_config
                .request_control_intent
                .reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
    }

    #[test]
    fn test_agent_config_with_definition_overlays_preserves_runtime_overrides() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        write_agent_overlay(
            &overlay_path,
            r#"
	tool_repeat_limit = 9
	streaming_mode = "off"
	model_reasoning_effort = "high"
	"#,
        );

        let mut base = AgentConfig::from(crate::Config::default());
        base.runtime_config.tool_repeat_limit = 42;
        base.set_model_override("gpt-5-mini");
        base.set_streaming_mode_override(crate::config::StreamingMode::On);
        base.set_model_reasoning_effort_override(Some(alan_agent_protocol::ReasoningEffort::Low));

        let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

        assert_eq!(merged.core_config.openai_responses_model, "gpt-5-mini");
        assert_eq!(merged.core_config.tool_repeat_limit, 9);
        assert_eq!(
            merged.core_config.streaming_mode,
            crate::config::StreamingMode::On
        );
        assert_eq!(
            merged.core_config.model_reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
        assert_eq!(
            merged.core_config.effective_context_window_tokens(),
            crate::Config::for_openai_responses("sk-test", None, Some("gpt-5-mini"))
                .effective_context_window_tokens()
        );
        assert_eq!(merged.runtime_config.tool_repeat_limit, 42);
        assert_eq!(
            merged.runtime_config.context_window_tokens,
            crate::Config::for_openai_responses("sk-test", None, Some("gpt-5-mini"))
                .effective_context_window_tokens()
        );
        assert_eq!(
            merged.runtime_config.streaming_mode,
            crate::config::StreamingMode::On
        );
        assert_eq!(
            merged
                .runtime_config
                .request_control_intent
                .reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
    }

    #[test]
    fn test_set_model_override_refreshes_runtime_context_window_budget() {
        let mut config = AgentConfig::from(crate::Config::for_openai_responses(
            "sk-test",
            None,
            Some("gpt-5.4"),
        ));
        assert_eq!(config.runtime_config.context_window_tokens, 1_050_000);

        config.set_model_override("gpt-5-mini");

        assert_eq!(config.core_config.effective_model(), "gpt-5-mini");
        assert_eq!(config.runtime_config.context_window_tokens, 400_000);
    }

    #[test]
    fn test_agent_config_with_definition_overlays_preserves_marked_same_value_runtime_overrides() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        write_agent_overlay(
            &overlay_path,
            r#"
streaming_mode = "off"
partial_stream_recovery_mode = "off"
[durability]
required = true
"#,
        );

        let mut base = AgentConfig::from(crate::Config::default());
        base.set_streaming_mode_override(crate::config::StreamingMode::Auto);
        base.set_partial_stream_recovery_mode_override(
            crate::config::PartialStreamRecoveryMode::ContinueOnce,
        );
        base.set_durability_required_override(false);

        let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

        assert_eq!(
            merged.core_config.streaming_mode,
            crate::config::StreamingMode::Auto
        );
        assert_eq!(
            merged.runtime_config.streaming_mode,
            crate::config::StreamingMode::Auto
        );
        assert_eq!(
            merged.core_config.partial_stream_recovery_mode,
            crate::config::PartialStreamRecoveryMode::ContinueOnce
        );
        assert_eq!(
            merged.runtime_config.partial_stream_recovery_mode,
            crate::config::PartialStreamRecoveryMode::ContinueOnce
        );
        assert!(!merged.core_config.durability.required);
        assert!(!merged.runtime_config.durability_required);
    }

    #[tokio::test]
    async fn test_agent_runtime_handle_shutdown_with_channel() {
        let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        let handle = RuntimeHandle {
            submission_tx: sub_tx,
            shutdown_tx: Some(shutdown_tx),
        };

        // Shutdown should send signal
        let result = handle.shutdown().await;
        assert!(result.is_ok());

        // Verify shutdown signal was sent
        let signal = shutdown_rx.recv().await;
        assert!(signal.is_some());
    }

    #[test]
    fn test_agent_runtime_config_recovery_rollout_path() {
        let temp = TempDir::new().unwrap();
        let rollout_path = temp.path().join("rollout.jsonl");

        let config = AgentProcessConfig {
            recovery_rollout_path: Some(rollout_path.clone()),
            ..Default::default()
        };

        assert_eq!(config.recovery_rollout_path, Some(rollout_path));
    }

    #[tokio::test]
    async fn test_initialize_agent_machine_from_rollout_preserves_current_process_cwd() {
        let temp = TempDir::new().unwrap();
        let process_cwd = std::path::Path::new("/mnt/source/src");
        let recovered_rollouts = temp.path().join("recovered-rollouts");
        let mut source =
            AgentMachine::new_with_recorder_in_dir("/proc/41", "gemini-2.0-flash", temp.path())
                .await
                .unwrap();
        source.add_user_message("Hello");
        source.flush().await;
        let rollout_path = source.rollout_path().unwrap().clone();
        drop(source);

        let startup = initialize_agent_machine(
            AgentMachineLaunchContext {
                process_path: "/proc/42",
                agent_path: "/agent/42",
                model: "gemini-2.0-flash",
            },
            Some(&rollout_path),
            Some(&recovered_rollouts),
            true,
            Some(process_cwd),
            crate::ResolvedRequestControls {
                reasoning: alan_agent_protocol::ReasoningControls {
                    effort: Some(alan_agent_protocol::ReasoningEffort::Medium),
                },
                source: crate::RequestControlSource::AgentMachineOverride,
                diagnostics: Vec::new(),
            },
        )
        .await
        .unwrap();

        assert_eq!(startup.metadata.process_path, "/proc/42");
        assert_eq!(startup.metadata.agent_path, "/agent/42");
        assert!(startup.metadata.rollout_id.is_some());

        let persisted_path = startup
            .metadata
            .rollout_path
            .clone()
            .expect("recovered machine should create a new rollout recorder");
        let persisted_items = crate::rollout::RolloutRecorder::load_history(&persisted_path)
            .await
            .unwrap();
        let persisted_meta = persisted_items.into_iter().find_map(|item| match item {
            crate::rollout::RolloutItem::AgentMachineMeta(meta) => Some(meta),
            _ => None,
        });

        assert_eq!(
            persisted_meta.as_ref().map(|meta| meta.cwd.as_str()),
            Some("/mnt/source/src")
        );
        assert_eq!(
            persisted_meta
                .as_ref()
                .map(|meta| meta.process_path.as_str()),
            Some("/proc/42")
        );
        assert_eq!(
            persisted_meta
                .as_ref()
                .and_then(|meta| meta.reasoning_effort),
            Some(alan_agent_protocol::ReasoningEffort::Medium)
        );

        drop(startup);
        let _ = tokio::fs::remove_file(persisted_path).await;
    }

    #[test]
    fn test_should_drive_turn_submission() {
        // steer/follow_up should be driven as turn
        assert!(should_drive_turn_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("test")],
            mode: alan_agent_protocol::InputMode::Steer,
        }));
        assert!(should_drive_turn_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("test")],
            mode: alan_agent_protocol::InputMode::FollowUp,
        }));
        // next_turn should be queue-only, not immediate execution.
        assert!(!should_drive_turn_submission(&Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("test")],
            mode: alan_agent_protocol::InputMode::NextTurn,
        }));

        // Turn should be driven as turn
        assert!(should_drive_turn_submission(&Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("test")],
            context: None,
        }));

        // Other ops should not be driven as turn
        assert!(!should_drive_turn_submission(&Op::CompactWithOptions {
            focus: None,
        }));
        assert!(!should_drive_turn_submission(&Op::CompactWithOptions {
            focus: Some("preserve todos".to_string()),
        }));
        assert!(!should_drive_turn_submission(&Op::Rollback { turns: 1 }));
        assert!(!should_drive_turn_submission(&Op::Interrupt));
        assert!(!should_drive_turn_submission(&Op::Resume {
            request_id: "req-123".to_string(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({})
            )],
        }));
    }
}
