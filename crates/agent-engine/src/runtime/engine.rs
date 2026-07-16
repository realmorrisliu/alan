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
#[path = "engine_tests.rs"]
mod tests;
