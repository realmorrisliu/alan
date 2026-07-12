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
use crate::{agent_machine::AgentMachine, llm::LlmClient};
use alan_agent_protocol::{Event, InputMode, Submission};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

static NEXT_RUNTIME_NAMESPACE_FID: AtomicU64 = AtomicU64::new(120_000);
const LLM_SRV_HANDLE: &str = "llm";
const SRV_MOUNT: &str = "/srv";
const LLM_MOUNT: &str = "/mnt/llm";

struct RuntimeLlmProvider {
    client: LlmClient,
}

impl RuntimeLlmProvider {
    fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl LlmProvider for RuntimeLlmProvider {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.client.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        self.client.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.client.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        self.client.provider_name()
    }
}

enum RuntimeEnvironmentBootstrap {
    Ready(NamespaceRuntimeEnvironment),
    NamespaceRoot {
        llm_client: LlmClient,
        tools: crate::tools::ToolRegistry,
        mount_grant_applicator_factory: Option<Arc<dyn super::MountGrantApplicatorFactory>>,
    },
}

impl RuntimeEnvironmentBootstrap {
    async fn into_environment(self) -> Result<NamespaceRuntimeEnvironment> {
        match self {
            Self::Ready(environment) => Ok(environment),
            Self::NamespaceRoot {
                llm_client,
                tools,
                mount_grant_applicator_factory,
            } => {
                build_root_namespace_environment(llm_client, tools, mount_grant_applicator_factory)
                    .await
            }
        }
    }
}

fn derived_soft_trigger_ratio(hard_trigger_ratio: f32) -> f32 {
    hard_trigger_ratio * 0.9
}

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

async fn read_namespace_input_submission(
    namespace: super::NamespaceRuntimeEnvironment,
    mode: InputMode,
) -> Option<Result<Submission>> {
    Some(namespace.read_next_input_submission(mode).await)
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

pub(crate) fn runtime_host_capabilities(
    config: &WorkspaceRuntimeConfig,
    tools: &crate::tools::ToolRegistry,
) -> crate::skills::SkillHostCapabilities {
    runtime_host_capabilities_for_tools(config, tools.list_tools().into_iter().map(str::to_string))
}

pub(crate) fn runtime_host_capabilities_for_tools(
    config: &WorkspaceRuntimeConfig,
    tools: impl IntoIterator<Item = String>,
) -> crate::skills::SkillHostCapabilities {
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    crate::skills::build_skill_host_capabilities_with_path_dirs(
        tools,
        path_dirs,
        config.launch_root_dir.is_none(),
    )
}

#[cfg(test)]
fn runtime_host_capabilities_with_path_dirs<I, P>(
    config: &WorkspaceRuntimeConfig,
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
        config.launch_root_dir.is_none(),
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

    pub fn with_agent_root_overlays(
        &self,
        overlay_paths: &[std::path::PathBuf],
    ) -> anyhow::Result<Self> {
        let mut merge_base_core_config = self.core_config.clone();
        if self.explicit_runtime_overrides.request_control_intent {
            merge_base_core_config.model_reasoning_effort = None;
        }

        let mut core_config = merge_base_core_config.with_agent_root_overlays(overlay_paths)?;
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

    /// Apply persisted configuration state to this agent config
    ///
    /// This is called when loading a workspace from disk to restore its
    /// original behavior settings (provider, model, timeouts, etc.)
    pub fn apply_persisted_state(&mut self, persisted: &crate::WorkspaceConfigState) {
        use crate::PersistedLlmProvider;
        use crate::config::LlmProvider;

        // Restore runtime behavior settings
        // All fields are Option<T> to distinguish "not set" from "set to 0"
        if let Some(max_tool_loops) = persisted.max_tool_loops {
            self.runtime_config.max_tool_loops = max_tool_loops;
        }
        if let Some(tool_repeat_limit) = persisted.tool_repeat_limit {
            self.runtime_config.tool_repeat_limit = tool_repeat_limit;
        }
        if let Some(llm_timeout_secs) = persisted.llm_timeout_secs {
            self.runtime_config.llm_request_timeout_secs = llm_timeout_secs as u64;
            self.core_config.llm_request_timeout_secs = llm_timeout_secs;
        }
        if let Some(tool_timeout_secs) = persisted.tool_timeout_secs {
            self.core_config.tool_timeout_secs = tool_timeout_secs;
        }
        if let Some(temp) = persisted.temperature {
            self.runtime_config.temperature = temp;
        }
        if let Some(max_tokens) = persisted.max_tokens {
            self.runtime_config.max_tokens = max_tokens;
        }
        if let Some(compaction_hard_trigger_ratio) = persisted.compaction_hard_trigger_ratio {
            self.runtime_config.compaction_hard_trigger_ratio = compaction_hard_trigger_ratio;
        }
        if let Some(compaction_soft_trigger_ratio) = persisted.compaction_soft_trigger_ratio {
            if compaction_soft_trigger_ratio < self.runtime_config.compaction_hard_trigger_ratio {
                self.runtime_config.compaction_soft_trigger_ratio = compaction_soft_trigger_ratio;
            } else {
                self.runtime_config.compaction_soft_trigger_ratio =
                    derived_soft_trigger_ratio(self.runtime_config.compaction_hard_trigger_ratio);
                warn!(
                    persisted_soft_trigger_ratio = compaction_soft_trigger_ratio,
                    persisted_hard_trigger_ratio = ?persisted.compaction_hard_trigger_ratio,
                    effective_hard_trigger_ratio = self.runtime_config.compaction_hard_trigger_ratio,
                    "Ignoring invalid persisted soft compaction threshold and deriving it from the hard threshold"
                );
            }
        } else if persisted.compaction_hard_trigger_ratio.is_some() {
            self.runtime_config.compaction_soft_trigger_ratio =
                derived_soft_trigger_ratio(self.runtime_config.compaction_hard_trigger_ratio);
        }
        if let Some(streaming_mode) = persisted.streaming_mode {
            self.runtime_config.streaming_mode = streaming_mode;
        }
        if let Some(partial_stream_recovery_mode) = persisted.partial_stream_recovery_mode {
            self.runtime_config.partial_stream_recovery_mode = partial_stream_recovery_mode;
        }
        if let Some(governance) = persisted.governance.clone() {
            self.runtime_config.governance = governance;
        }

        // Restore LLM provider and model
        if let Some(provider) = persisted.llm_provider {
            self.core_config.llm_provider = match provider {
                PersistedLlmProvider::GoogleGeminiGenerateContent => {
                    LlmProvider::GoogleGeminiGenerateContent
                }
                PersistedLlmProvider::Chatgpt => LlmProvider::Chatgpt,
                PersistedLlmProvider::OpenAiResponses => LlmProvider::OpenAiResponses,
                PersistedLlmProvider::OpenAiChatCompletions => LlmProvider::OpenAiChatCompletions,
                PersistedLlmProvider::OpenAiChatCompletionsCompatible => {
                    LlmProvider::OpenAiChatCompletionsCompatible
                }
                PersistedLlmProvider::OpenRouter => LlmProvider::OpenRouter,
                PersistedLlmProvider::AnthropicMessages => LlmProvider::AnthropicMessages,
            };
        }

        // Restore model based on provider
        if let Some(ref model) = persisted.llm_model {
            match self.core_config.llm_provider {
                LlmProvider::GoogleGeminiGenerateContent => {
                    self.core_config.google_gemini_generate_content_model = model.clone()
                }
                LlmProvider::Chatgpt => self.core_config.chatgpt_model = model.clone(),
                LlmProvider::OpenAiResponses => {
                    self.core_config.openai_responses_model = model.clone()
                }
                LlmProvider::OpenAiChatCompletions => {
                    self.core_config.openai_chat_completions_model = model.clone()
                }
                LlmProvider::OpenAiChatCompletionsCompatible => {
                    self.core_config.openai_chat_completions_compatible_model = model.clone()
                }
                LlmProvider::OpenRouter => self.core_config.openrouter_model = model.clone(),
                LlmProvider::AnthropicMessages => {
                    self.core_config.anthropic_messages_model = model.clone()
                }
            }
        }

        if let Some(context_window_tokens) = persisted.context_window_tokens {
            self.runtime_config.context_window_tokens = context_window_tokens;
        } else {
            self.refresh_runtime_derived_fields();
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

/// Combined config for spawning a runtime within a workspace
#[derive(Debug, Clone)]
pub struct WorkspaceRuntimeConfig {
    /// Agent capabilities (reusable across workspaces)
    pub agent_config: AgentConfig,
    /// Source used to resolve the default agent configuration before workspace overlays.
    pub core_config_source: crate::ConfigSourceKind,
    /// Optional named agent root to resolve on top of the default workspace agent.
    pub agent_name: Option<String>,
    /// Workspace identifier
    pub workspace_id: String,
    /// Workspace root directory for tool cwd/sandbox context
    pub workspace_root_dir: Option<std::path::PathBuf>,
    /// Workspace `.alan` directory for agent overlays, memory, and rollouts.
    pub workspace_alan_dir: Option<std::path::PathBuf>,
    /// Optional execution record used to recover Agent Machine state for a new Process.
    pub recovery_rollout_path: Option<std::path::PathBuf>,
    /// Optional explicit child launch root layered on top of the resolved workspace/default roots.
    pub launch_root_dir: Option<std::path::PathBuf>,
    /// Optional default cwd override for the runtime tool context.
    pub default_cwd_override: Option<std::path::PathBuf>,
    /// Optional alan home-path override for agent-root resolution in advanced hosts/tests.
    pub agent_home_paths: Option<crate::AlanHomePaths>,
    /// Optional host-selected ChatGPT auth storage path shared with provider auth flows.
    pub chatgpt_auth_storage_path: Option<std::path::PathBuf>,
    /// Optional host factory for applying approved mount grants to the live namespace.
    pub mount_grant_applicator_factory: Option<Arc<dyn super::MountGrantApplicatorFactory>>,
}

impl Default for WorkspaceRuntimeConfig {
    fn default() -> Self {
        Self {
            agent_config: AgentConfig::default(),
            core_config_source: crate::ConfigSourceKind::Default,
            agent_name: None,
            workspace_id: format!(
                "workspace-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            ),
            workspace_root_dir: None,
            workspace_alan_dir: None,
            recovery_rollout_path: None,
            launch_root_dir: None,
            default_cwd_override: None,
            agent_home_paths: None,
            chatgpt_auth_storage_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

impl From<crate::config::Config> for WorkspaceRuntimeConfig {
    fn from(config: crate::config::Config) -> Self {
        Self {
            agent_config: AgentConfig::from(config),
            core_config_source: crate::ConfigSourceKind::Default,
            agent_name: None,
            workspace_id: format!(
                "workspace-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            ),
            workspace_root_dir: None,
            workspace_alan_dir: None,
            recovery_rollout_path: None,
            launch_root_dir: None,
            default_cwd_override: None,
            agent_home_paths: None,
            chatgpt_auth_storage_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

impl From<crate::LoadedConfig> for WorkspaceRuntimeConfig {
    fn from(loaded: crate::LoadedConfig) -> Self {
        Self {
            agent_config: AgentConfig::from(loaded.config),
            core_config_source: loaded.source,
            agent_name: None,
            workspace_id: format!(
                "workspace-{}",
                uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
            ),
            workspace_root_dir: None,
            workspace_alan_dir: None,
            recovery_rollout_path: None,
            launch_root_dir: None,
            default_cwd_override: None,
            agent_home_paths: None,
            chatgpt_auth_storage_path: None,
            mount_grant_applicator_factory: None,
        }
    }
}

impl WorkspaceRuntimeConfig {
    /// Apply persisted configuration state (delegates to agent_config)
    pub fn apply_persisted_state(&mut self, persisted: &crate::WorkspaceConfigState) {
        self.agent_config.apply_persisted_state(persisted);
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

/// A renderer-host view of one mounted runtime namespace.
#[derive(Clone)]
pub struct RuntimeNamespaceSurface {
    root: InProcessTransport,
    agent_path: String,
}

impl RuntimeNamespaceSurface {
    fn new(root: InProcessTransport, agent_path: impl Into<String>) -> Self {
        Self {
            root,
            agent_path: agent_path.into(),
        }
    }

    /// Root aP transport for the mounted namespace.
    pub fn root_transport(&self) -> InProcessTransport {
        self.root.clone()
    }

    /// Concrete agent path for the launched root agent, for example `/agent/1`.
    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }
}

/// Local runtime launch handle for renderer hosts.
pub struct RuntimeNamespaceLaunch {
    pub controller: RuntimeController,
    pub surface: RuntimeNamespaceSurface,
}

/// Spawn a new agent runtime and return handles for communication
pub fn spawn(config: WorkspaceRuntimeConfig) -> Result<RuntimeController> {
    let core_config = effective_core_config_for_runtime(&config)?;

    let llm_client = LlmClient::from_core_config_with_chatgpt_auth_storage_path(
        &core_config,
        config.chatgpt_auth_storage_path.clone(),
    )
    .context("Failed to create LLM client for runtime")?;
    let tools = crate::tools::ToolRegistry::with_config(Arc::new(core_config.clone()));

    spawn_with_llm_client_and_tools(config, llm_client, tools)
}

/// Spawn a new namespace-native runtime and return the renderer-host surface.
pub async fn spawn_with_namespace_surface(
    config: WorkspaceRuntimeConfig,
) -> Result<RuntimeNamespaceLaunch> {
    let core_config = effective_core_config_for_runtime(&config)?;

    let llm_client = LlmClient::from_core_config_with_chatgpt_auth_storage_path(
        &core_config,
        config.chatgpt_auth_storage_path.clone(),
    )
    .context("Failed to create LLM client for runtime")?;
    let tools = crate::tools::ToolRegistry::with_config(Arc::new(core_config));

    spawn_with_llm_client_and_tools_and_namespace_surface(config, llm_client, tools).await
}

/// Spawn a new agent runtime with an externally-provided LLM client.
///
/// This is useful for testing with a mock LLM provider.
pub fn spawn_with_llm_client(
    config: WorkspaceRuntimeConfig,
    llm_client: LlmClient,
) -> Result<RuntimeController> {
    let core_config = effective_core_config_for_runtime(&config)?;
    let tools = crate::tools::ToolRegistry::with_config(Arc::new(core_config));

    spawn_with_llm_client_and_tools(config, llm_client, tools)
}

/// Spawn a namespace-native runtime with an externally provided LLM client.
pub async fn spawn_with_llm_client_and_namespace_surface(
    config: WorkspaceRuntimeConfig,
    llm_client: LlmClient,
) -> Result<RuntimeNamespaceLaunch> {
    let core_config = effective_core_config_for_runtime(&config)?;
    let tools = crate::tools::ToolRegistry::with_config(Arc::new(core_config));
    spawn_with_llm_client_and_tools_and_namespace_surface(config, llm_client, tools).await
}

fn configure_runtime_tool_execution_binding(
    config: &WorkspaceRuntimeConfig,
    tools: &mut crate::tools::ToolRegistry,
) -> Result<()> {
    let resolved_agent_definition = crate::ResolvedAgentDefinition::from_runtime_config(config)?;
    let channel = runtime_install_channel(config);
    if let Some(default_cwd) = config.default_cwd_override.as_ref() {
        if let Some(ws_root) = resolved_agent_definition.workspace_root_dir.as_ref() {
            let scratch_dir = resolved_agent_definition
                .workspace_alan_dir
                .as_ref()
                .map(|alan_dir| crate::workspace_runtime_tmp_dir_from_alan_dir(alan_dir, channel))
                .unwrap_or_else(|| default_cwd.join(".alan").join("tmp"));
            tools.set_default_execution_binding(
                crate::tools::ToolExecutionBinding::with_workspace(
                    ws_root.clone(),
                    default_cwd.clone(),
                    scratch_dir,
                ),
            );
        } else {
            tools.set_default_cwd(default_cwd.clone());
        }
    } else if let Some(ws_root) = resolved_agent_definition.workspace_root_dir.as_ref() {
        let scratch_dir = resolved_agent_definition
            .workspace_alan_dir
            .as_ref()
            .map(|alan_dir| crate::workspace_runtime_tmp_dir_from_alan_dir(alan_dir, channel))
            .unwrap_or_else(|| ws_root.join(".alan").join("tmp"));
        tools.set_default_execution_binding(crate::tools::ToolExecutionBinding::with_workspace(
            ws_root.clone(),
            ws_root.clone(),
            scratch_dir,
        ));
    }

    Ok(())
}

pub fn effective_core_config_for_runtime(
    config: &WorkspaceRuntimeConfig,
) -> Result<crate::config::Config> {
    let resolved_agent_definition = crate::ResolvedAgentDefinition::from_runtime_config(config)?;
    let mut agent_config = config.agent_config.clone();
    if !resolved_agent_definition.config_overlay_paths.is_empty() {
        agent_config = agent_config
            .with_agent_root_overlays(&resolved_agent_definition.config_overlay_paths)?;
    }
    let mut core_config = agent_config.core_config.clone();
    let home_paths = config
        .agent_home_paths
        .clone()
        .or_else(crate::AlanHomePaths::detect);
    let has_connections_store = home_paths
        .as_ref()
        .is_some_and(|paths| paths.global_connections_path.exists());
    if core_config.connection_profile.is_some() || has_connections_store {
        core_config.resolve_connection_profile(home_paths.as_ref())?;
    }
    let channel = runtime_install_channel(config);
    if let Some(alan_dir) = resolved_agent_definition.workspace_alan_dir.as_ref() {
        core_config.memory.workspace_dir = Some(
            crate::workspace_memory_dir_for_channel_from_alan_dir(alan_dir, channel),
        );
    }
    crate::resolve_runtime_request_controls(
        &core_config,
        crate::provider_capabilities_for_config(&core_config),
        agent_config.runtime_config.request_control_intent,
    )?;

    Ok(core_config)
}

async fn build_root_namespace_environment(
    llm_client: LlmClient,
    tools: crate::tools::ToolRegistry,
    mount_grant_applicator_factory: Option<Arc<dyn super::MountGrantApplicatorFactory>>,
) -> Result<NamespaceRuntimeEnvironment> {
    let tool_names = tools
        .list_tools()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();

    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(RuntimeLlmProvider::new(llm_client)));
    let routefs = Arc::new(alan_routefs::RouteFs::new());
    let srvfs = Arc::new(alan_kernel::SrvFs::new());

    let procfs = alan_kernel::ProcFs::new();
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(procfs.clone())));
    let agent_root_tree = InProcessTransport::new(agent_root.clone());

    let mut process_namespace = alan_kernel::Namespace::new();
    process_namespace.mount(
        "/agent",
        agent_root_tree.clone(),
        alan_kernel::Access::ReadWrite,
    );
    mount_llmfs_standard_handles(&mut process_namespace, srvfs.clone(), llmfs).await?;
    let route_tree =
        mount_routefs_standard_handles(&mut process_namespace, srvfs.clone(), routefs).await?;
    for tool_name in &tool_names {
        let tool = tools
            .get(tool_name)
            .with_context(|| format!("materialize Tool package metadata for {tool_name}"))?;
        let manifest = super::ToolPackageManifest::from_tool(
            tool.as_ref(),
            tools.execution_timeout_secs(tool_name).unwrap_or(30),
        )?;
        let manifest_bytes = serde_json::to_vec(&manifest)
            .with_context(|| format!("serialize Tool manifest for {tool_name}"))?;
        let manifest_fs = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            manifest_bytes,
        ));
        process_namespace.mount(
            &format!("/bin/{tool_name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadOnly,
        );
        process_namespace.mount(
            &format!("/lib/exec/{tool_name}"),
            InProcessTransport::new(manifest_fs),
            alan_kernel::Access::ReadOnly,
        );
    }

    let live_namespace = alan_kernel::LiveNamespace::new(process_namespace);
    let root_pid = spawn_root_agent_process(&procfs, live_namespace.clone()).await?;
    agent_root
        .bind_process(root_pid.clone(), agentfs.clone())
        .await;
    agent_root.set_root_process(root_pid.clone()).await;

    let root_pid_value = alan_kernel::Pid(
        root_pid
            .parse::<u64>()
            .with_context(|| format!("parse root agent pid '{root_pid}'"))?,
    );
    let tool_runner = crate::tools::ToolProcessRunner::from_registry(&tools);
    let procfs_with_runner = procfs.clone().with_runner(Arc::new(tool_runner.clone()));
    procfs
        .bind_live_namespace(root_pid_value, live_namespace.clone())
        .await;
    let process_procfs = procfs_with_runner.for_live_spawner(
        Some(root_pid_value),
        live_namespace.clone(),
        alan_kernel::Credentials::user("root-agent"),
    );
    live_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(process_procfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::from_live_namespace(
        live_namespace.clone(),
    )));
    let namespace_environment =
        super::NamespaceRuntimeEnvironment::new(root, format!("/agent/{root_pid}"), "default")
            .with_process_context(procfs_with_runner, agent_root, root_pid_value, tool_runner)
            .with_shared_services(InProcessTransport::new(srvfs), route_tree);
    let namespace_environment = if let Some(factory) = mount_grant_applicator_factory {
        namespace_environment.with_mount_grant_applicator_factory(factory, live_namespace)
    } else {
        namespace_environment
    };
    Ok(namespace_environment)
}

async fn mount_llmfs_standard_handles(
    namespace: &mut alan_kernel::Namespace,
    srvfs: Arc<alan_kernel::SrvFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
) -> Result<()> {
    srvfs
        .post(
            LLM_SRV_HANDLE,
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        )
        .await;
    namespace.mount(
        SRV_MOUNT,
        InProcessTransport::new(srvfs.clone()),
        alan_kernel::Access::ReadOnly,
    );
    let (llm_tree, llm_access) = srvfs
        .lookup(LLM_SRV_HANDLE)
        .await
        .context("lookup llmfs handle after posting /srv/llm")?;
    namespace.mount(LLM_MOUNT, llm_tree, llm_access);
    Ok(())
}

async fn mount_routefs_standard_handles(
    namespace: &mut alan_kernel::Namespace,
    srvfs: Arc<alan_kernel::SrvFs>,
    routefs: Arc<alan_routefs::RouteFs>,
) -> Result<InProcessTransport> {
    srvfs
        .post(
            alan_routefs::SRV_HANDLE,
            InProcessTransport::new(routefs),
            alan_kernel::Access::ReadWrite,
        )
        .await;
    namespace.mount(
        SRV_MOUNT,
        InProcessTransport::new(srvfs.clone()),
        alan_kernel::Access::ReadOnly,
    );
    let (route_tree, route_access) = srvfs
        .lookup(alan_routefs::SRV_HANDLE)
        .await
        .context("lookup routefs handle after posting /srv/route")?;
    namespace.mount(alan_routefs::MOUNT_PATH, route_tree.clone(), route_access);
    Ok(route_tree)
}

async fn spawn_root_agent_process(
    procfs: &alan_kernel::ProcFs,
    process_namespace: alan_kernel::LiveNamespace,
) -> Result<String> {
    let spawner_procfs = procfs.for_live_spawner(
        None,
        process_namespace.clone(),
        alan_kernel::Credentials::user("service-manager"),
    );
    let clone_fid = Fid(NEXT_RUNTIME_NAMESPACE_FID.fetch_add(1, Ordering::Relaxed));
    spawner_procfs
        .walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .context("walk root agent /proc/clone")?;
    spawner_procfs
        .open(clone_fid, OpenMode::ReadWrite)
        .await
        .context("open root agent /proc/clone")?;
    let pid = String::from_utf8(
        spawner_procfs
            .read(clone_fid, 0, 64)
            .await
            .context("read root agent pid from /proc/clone")?,
    )
    .context("root agent pid is utf8")?;
    let exec = alan_kernel::ExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: Vec::new(),
        namespace: Some(alan_kernel::ExecNamespaceManifest::from_namespace(
            &process_namespace.snapshot(),
        )),
    };
    let exec_bytes = serde_json::to_vec(&exec).context("serialize root agent exec spec")?;
    spawner_procfs
        .write(clone_fid, 0, &exec_bytes)
        .await
        .context("write root agent exec spec")?;
    spawner_procfs
        .clunk(clone_fid)
        .await
        .context("commit root agent /proc/clone")?;
    Ok(pid)
}

/// Spawn a new agent runtime with an externally-provided LLM client and tools.
///
/// Hosts should use this when they need to inject concrete tool implementations
/// while keeping the runtime crate generic.
pub fn spawn_with_llm_client_and_tools(
    config: WorkspaceRuntimeConfig,
    llm_client: LlmClient,
    mut tools: crate::tools::ToolRegistry,
) -> Result<RuntimeController> {
    configure_runtime_tool_execution_binding(&config, &mut tools)?;

    let generation_capabilities = llm_client.capabilities();
    let host_capabilities = runtime_host_capabilities(&config, &tools);
    let mount_grant_applicator_factory = config.mount_grant_applicator_factory.clone();
    spawn_with_prepared_runtime_environment(
        config,
        RuntimeEnvironmentBootstrap::NamespaceRoot {
            llm_client,
            tools,
            mount_grant_applicator_factory,
        },
        host_capabilities,
        generation_capabilities,
    )
}

pub async fn spawn_with_llm_client_and_tools_and_namespace_surface(
    config: WorkspaceRuntimeConfig,
    llm_client: LlmClient,
    mut tools: crate::tools::ToolRegistry,
) -> Result<RuntimeNamespaceLaunch> {
    configure_runtime_tool_execution_binding(&config, &mut tools)?;

    let generation_capabilities = llm_client.capabilities();
    let host_capabilities = runtime_host_capabilities(&config, &tools);
    let environment = build_root_namespace_environment(
        llm_client,
        tools,
        config.mount_grant_applicator_factory.clone(),
    )
    .await?;
    let surface = RuntimeNamespaceSurface::new(
        environment.root_transport(),
        environment.agent_path().to_string(),
    );
    let controller = spawn_with_prepared_runtime_environment(
        config,
        RuntimeEnvironmentBootstrap::Ready(environment),
        host_capabilities,
        generation_capabilities,
    )?;
    Ok(RuntimeNamespaceLaunch {
        controller,
        surface,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn spawn_with_namespace_environment(
    config: WorkspaceRuntimeConfig,
    namespace: super::NamespaceRuntimeEnvironment,
    host_capabilities: crate::skills::SkillHostCapabilities,
    generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    spawn_with_prepared_runtime_environment(
        config,
        RuntimeEnvironmentBootstrap::Ready(namespace),
        host_capabilities,
        generation_capabilities,
    )
}

fn spawn_with_prepared_runtime_environment(
    config: WorkspaceRuntimeConfig,
    environment: RuntimeEnvironmentBootstrap,
    host_capabilities: crate::skills::SkillHostCapabilities,
    _generation_capabilities: crate::llm::ProviderCapabilities,
) -> Result<RuntimeController> {
    let (sub_tx, mut sub_rx) = mpsc::channel::<Submission>(32);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (ready_tx, ready_rx) =
        oneshot::channel::<std::result::Result<RuntimeStartupMetadata, String>>();

    let resolved_agent_definition = crate::ResolvedAgentDefinition::from_runtime_config(&config)?;
    let channel = runtime_install_channel(&config);

    let mut agent_config = config.agent_config.clone();
    if !resolved_agent_definition.config_overlay_paths.is_empty() {
        agent_config = agent_config
            .with_agent_root_overlays(&resolved_agent_definition.config_overlay_paths)?;
    }
    let mut core_config = agent_config.core_config.clone();
    let home_paths = config
        .agent_home_paths
        .clone()
        .or_else(crate::AlanHomePaths::detect);
    let has_connections_store = home_paths
        .as_ref()
        .is_some_and(|paths| paths.global_connections_path.exists());
    if core_config.connection_profile.is_some() || has_connections_store {
        core_config.resolve_connection_profile(home_paths.as_ref())?;
    }
    if let Some(alan_dir) = resolved_agent_definition.workspace_alan_dir.as_ref() {
        core_config.memory.workspace_dir = Some(
            crate::workspace_memory_dir_for_channel_from_alan_dir(alan_dir, channel),
        );
    }

    let mut runtime_config = agent_config.runtime_config.clone();
    runtime_config.chatgpt_auth_storage_path = config.chatgpt_auth_storage_path.clone();
    runtime_config.policy_engine =
        crate::policy::PolicyEngine::load_for_governance_with_default_policy_path(
            resolved_agent_definition.workspace_alan_dir.as_deref(),
            resolved_agent_definition.default_policy_path.as_deref(),
            &runtime_config.governance,
        );
    let prompt_cache_persona_dirs = resolved_agent_definition.persona_dirs.clone();
    if let Some(persona_dir) = resolved_agent_definition.writable_persona_dir.as_deref()
        && let Err(err) = crate::prompts::ensure_workspace_bootstrap_files_at(persona_dir)
    {
        warn!(
            path = %persona_dir.display(),
            error = %err,
            "Failed to initialize workspace persona files; continuing without bootstrap writes"
        );
    }
    if core_config.memory.enabled
        && let Some(memory_dir) = core_config.memory.workspace_dir.as_deref()
        && let Err(err) = crate::prompts::ensure_workspace_memory_layout_at(memory_dir)
    {
        warn!(
            path = %memory_dir.display(),
            error = %err,
            "Failed to initialize workspace memory layout; continuing without bootstrap writes"
        );
    }
    let rollouts_dir = resolved_agent_definition
        .workspace_alan_dir
        .as_ref()
        .map(|dir| crate::workspace_rollouts_dir_for_channel_from_alan_dir(dir, channel));
    let rollout_cwd = config
        .default_cwd_override
        .clone()
        .or_else(|| resolved_agent_definition.workspace_root_dir.clone());
    let runtime_workspace_root_dir = resolved_agent_definition.workspace_root_dir.clone();
    let recovery_rollout_path = config.recovery_rollout_path.clone();
    let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
    let mut prompt_cache =
        super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            resolved_agent_definition.capability_view.clone(),
            resolved_agent_definition.skill_overrides.clone(),
            prompt_cache_persona_dirs.clone(),
            host_capabilities,
        );
    prompt_cache.set_workspace_memory_dir(
        core_config
            .memory
            .enabled
            .then(|| core_config.memory.workspace_dir.clone())
            .flatten(),
    );

    // Spawn the main runtime task
    let task_handle = tokio::spawn(async move {
        let environment = match environment.into_environment().await {
            Ok(environment) => environment,
            Err(err) => {
                let _ = ready_tx.send(Err(format!("{:#}", err)));
                return;
            }
        };
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
            rollout_cwd.as_deref(),
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
            workspace_id: config.workspace_id.clone(),
            workspace_root_dir: runtime_workspace_root_dir,
            machine,
            current_submission_id: None,
            environment,
            core_config,
            runtime_config,
            workspace_persona_dirs: prompt_cache_persona_dirs.clone(),
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
                let namespace_input = state.namespace_environment().clone();
                let namespace_control = state.namespace_environment().clone();
                let poll_pending_namespace_response = state.turn_state.has_pending_interaction();
                tokio::select! {
                    submission = sub_rx.recv() => submission.map(QueuedRuntimeItem::Submission),
                    namespace_submission = read_namespace_input_submission(namespace_input, InputMode::FollowUp) => {
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
                    let namespace_input = state.namespace_environment().clone();
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
                            namespace_submission = read_namespace_input_submission(namespace_input.clone(), InputMode::FollowUp) => {
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
                    let namespace_input = state.namespace_environment().clone();
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
                            namespace_submission = read_namespace_input_submission(namespace_input.clone(), InputMode::FollowUp) => {
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

fn runtime_install_channel(config: &WorkspaceRuntimeConfig) -> crate::InstallChannel {
    config
        .agent_home_paths
        .as_ref()
        .map(|paths| paths.channel)
        .unwrap_or_else(crate::InstallChannel::detect_current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{agent_loop::DeferredRuntimeAction, memory_promotion};
    use alan_agent_protocol::{ContentPart, Op};
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
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let memory_dir = workspace_alan_dir.join("runtime/stable/memory");

        let mut machine = AgentMachine::new();
        machine.add_user_message("My name is Morris.");

        let mut turn_state = TurnState::default();
        turn_state.begin_turn(0);

        let mut core_config = crate::Config::default();
        core_config.memory.enabled = true;
        core_config.memory.workspace_dir = Some(memory_dir);
        let runtime_config = RuntimeConfig::from(&core_config);

        let state = RuntimeLoopState {
            workspace_id: "workspace-queue-test".to_string(),
            workspace_root_dir: None,
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_test(),
            core_config,
            runtime_config,
            workspace_persona_dirs: Vec::new(),
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
    async fn llmfs_standard_handle_posts_under_srv_and_mounts_tree_under_mnt_llm() {
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(MockLlmProvider::new()));
        let srvfs = Arc::new(alan_kernel::SrvFs::new());
        let mut namespace = alan_kernel::Namespace::new();

        mount_llmfs_standard_handles(&mut namespace, srvfs, llmfs)
            .await
            .unwrap();

        let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root);

        assert_eq!(shell.ls("/srv").await.unwrap(), vec!["llm".to_string()]);
        assert_eq!(shell.cat("/srv/llm").await.unwrap(), b"llm");
        assert_eq!(
            shell.ls("/srv/llm").await,
            Err(alan_ap::ErrorCode::NotDirectory),
            "/srv/llm is the rendezvous handle, not the llmfs state tree"
        );

        let llm_root = shell.ls("/mnt/llm").await.unwrap();
        assert!(llm_root.iter().any(|entry| entry == "connections"));
        assert!(llm_root.iter().any(|entry| entry == "providers"));
        let connections = shell.ls("/mnt/llm/connections").await.unwrap();
        assert_eq!(connections, vec!["default".to_string()]);
    }

    #[tokio::test]
    async fn root_namespace_environment_posts_routefs_and_mounts_route_tree() {
        let environment = build_root_namespace_environment(
            LlmClient::new(MockLlmProvider::new()),
            crate::tools::ToolRegistry::new(),
            None,
        )
        .await
        .unwrap();
        let shell = alan_shell::Shell::new(environment.root_transport());

        let srv_entries = shell.ls("/srv").await.unwrap();
        assert!(srv_entries.iter().any(|entry| entry == "llm"));
        assert!(srv_entries.iter().any(|entry| entry == "route"));
        assert_eq!(shell.cat("/srv/route").await.unwrap(), b"route");
        assert_eq!(
            shell.ls("/srv/route").await,
            Err(alan_ap::ErrorCode::NotDirectory),
            "/srv/route is the rendezvous handle, not the routefs state tree"
        );

        let route_root = shell.ls("/mnt/route").await.unwrap();
        for entry in ["send", "rules", "ports", "log"] {
            assert!(
                route_root.iter().any(|mounted| mounted == entry),
                "{route_root:?}"
            );
        }

        let message = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "type": "status",
            "content": "ready"
        }))
        .unwrap();
        shell.write("/mnt/route/send", &message).await.unwrap();
        let dead_letter =
            String::from_utf8(shell.cat("/mnt/route/ports/dead-letter").await.unwrap()).unwrap();
        assert!(
            dead_letter.contains(r#""rule":"dead-letter""#),
            "{dead_letter}"
        );
        assert!(dead_letter.contains(r#""type":"status""#), "{dead_letter}");
    }

    #[tokio::test]
    async fn root_namespace_mounts_complete_tool_packages() {
        let mut tools = crate::tools::ToolRegistry::new();
        tools.register(PackageTestTool {
            name: "example",
            description: "Example Tool",
        });
        let environment =
            build_root_namespace_environment(LlmClient::new(MockLlmProvider::new()), tools, None)
                .await
                .unwrap();
        let shell = alan_shell::Shell::new(environment.root_transport());

        assert!(
            shell
                .ls("/bin")
                .await
                .unwrap()
                .contains(&"example".to_string())
        );
        let manifest: crate::runtime::ToolPackageManifest =
            serde_json::from_slice(&shell.cat("/lib/exec/example/manifest").await.unwrap())
                .unwrap();
        manifest.validate_for_name("example").unwrap();
        let discovered = environment.discover_tool_packages().await.unwrap();
        assert_eq!(discovered, vec![manifest]);
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

    #[tokio::test]
    async fn invalid_tool_package_fails_before_agent_launch() {
        let mut tools = crate::tools::ToolRegistry::new();
        tools.register(PackageTestTool {
            name: "bad/name",
            description: "Invalid Tool",
        });

        let error = match build_root_namespace_environment(
            LlmClient::new(MockLlmProvider::new()),
            tools,
            None,
        )
        .await
        {
            Ok(_) => panic!("invalid Tool package must fail before launch"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("does not match mounted package"));
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

    #[tokio::test]
    async fn test_spawn_with_llm_client_and_tools_runs_turn_after_namespace_bootstrap() {
        let llm_client = LlmClient::new(
            MockLlmProvider::new()
                .with_response(mock_generation_response("hello from namespace bootstrap")),
        );
        let launch = spawn_with_llm_client_and_tools_and_namespace_surface(
            WorkspaceRuntimeConfig::default(),
            llm_client,
            crate::tools::ToolRegistry::new(),
        )
        .await
        .unwrap();
        let shell = alan_shell::Shell::new(launch.surface.root_transport());
        let mut ui_events = shell
            .tail(&format!(
                "{}/machine/ui/events",
                launch.surface.agent_path()
            ))
            .await
            .unwrap();
        let mut controller = launch.controller;
        controller.wait_until_ready().await.unwrap();

        controller
            .handle
            .submission_tx
            .send(Submission::new(Op::Turn {
                parts: vec![ContentPart::text("hello")],
                context: None,
            }))
            .await
            .unwrap();

        let observed = wait_for_ui_turn_completion(&mut ui_events, Duration::from_secs(5)).await;
        assert!(observed.iter().any(|event| matches!(
            event,
            alan_agent_protocol::UiEvent::Activity { snapshot }
                if snapshot.state == alan_agent_protocol::UiActivityState::Running
        )));
        let text = String::from_utf8(
            shell
                .cat(&format!("{}/io/output", launch.surface.agent_path()))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(text, "hello from namespace bootstrap");
        controller.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn namespace_surface_launch_exposes_live_agent_files_for_renderer_hosts() {
        let llm_client = LlmClient::new(
            MockLlmProvider::new().with_response(mock_generation_response("hello from surface")),
        );
        let mut launch = spawn_with_llm_client_and_tools_and_namespace_surface(
            WorkspaceRuntimeConfig::default(),
            llm_client,
            crate::tools::ToolRegistry::new(),
        )
        .await
        .unwrap();
        launch.controller.wait_until_ready().await.unwrap();

        let shell = alan_shell::Shell::new(launch.surface.root_transport());
        let agent_path = launch.surface.agent_path().to_string();
        assert!(agent_path.starts_with("/agent/"));
        let message = "hello from renderer host";
        shell
            .write(&format!("{agent_path}/io/input"), message.as_bytes())
            .await
            .expect("write agent input");
        let status = String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap();
        assert_eq!(status, "running\n");
        let input =
            String::from_utf8(shell.cat(&format!("{agent_path}/io/input")).await.unwrap()).unwrap();
        assert_eq!(input, message);
        launch.controller.shutdown().await.unwrap();
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
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let memory_dir = workspace_alan_dir.join("runtime/stable/memory");
        crate::prompts::ensure_workspace_memory_layout_at(&memory_dir).unwrap();

        let mut core_config = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core_config.memory.enabled = true;
        core_config.memory.workspace_dir = Some(memory_dir.clone());
        core_config.streaming_mode = crate::config::StreamingMode::Off;

        let mut agent_config = crate::AgentConfig::from(core_config);
        agent_config.runtime_config.streaming_mode = crate::config::StreamingMode::Off;

        let config = WorkspaceRuntimeConfig {
            agent_config,
            workspace_root_dir: Some(workspace_root),
            workspace_alan_dir: Some(workspace_alan_dir),
            ..WorkspaceRuntimeConfig::default()
        };
        let call_count = Arc::new(Mutex::new(0));
        let llm_client = LlmClient::new(ShutdownDrainMemoryPromotionProvider {
            call_count: Arc::clone(&call_count),
            deferred_delay: Duration::from_millis(100),
        });

        let launch = spawn_with_llm_client_and_namespace_surface(config, llm_client)
            .await
            .unwrap();
        let shell = alan_shell::Shell::new(launch.surface.root_transport());
        let mut output = shell
            .tail(&format!("{}/io/output", launch.surface.agent_path()))
            .await
            .unwrap();
        let mut controller = launch.controller;
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
    async fn test_spawn_with_namespace_environment_reaches_ready_without_legacy_capabilities() {
        let core_config = crate::Config::default();
        let generation_capabilities = crate::provider_capabilities_for_config(&core_config);
        let config = WorkspaceRuntimeConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..WorkspaceRuntimeConfig::default()
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
        assert!(ready.rollout_id.is_some());
        controller.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_namespace_io_input_frame_drives_runtime_turn_without_api_submission() {
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
        let config = WorkspaceRuntimeConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..WorkspaceRuntimeConfig::default()
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
        assert_eq!(output, "hello from namespace runtime");

        controller.shutdown().await.unwrap();
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
            workspace_id: "outer-idle-namespace-response-test".to_string(),
            workspace_root_dir: None,
            machine: crate::AgentMachine::new(),
            current_submission_id: None,
            environment: namespace_environment,
            core_config: crate::Config::default(),
            runtime_config: RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
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
        let config = WorkspaceRuntimeConfig {
            agent_config: crate::AgentConfig::from(core_config),
            ..WorkspaceRuntimeConfig::default()
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
        let config = WorkspaceRuntimeConfig::default();
        assert!(config.workspace_id.starts_with("workspace-"));
        assert!(config.workspace_root_dir.is_none());
        assert!(config.workspace_alan_dir.is_none());
    }

    #[test]
    fn test_runtime_host_capabilities_enable_delegated_support_for_top_level_runtime() {
        let config = WorkspaceRuntimeConfig::default();
        let tools = crate::tools::ToolRegistry::new();

        let capabilities = runtime_host_capabilities(&config, &tools);

        assert!(capabilities.supports_delegated_skill_invocation());
        assert!(capabilities.tools.contains("invoke_delegated_skill"));
    }

    #[test]
    fn test_runtime_host_capabilities_keep_delegated_support_off_for_child_launch_roots() {
        let config = WorkspaceRuntimeConfig {
            launch_root_dir: Some(PathBuf::from("/tmp/child-agent")),
            ..WorkspaceRuntimeConfig::default()
        };
        let tools = crate::tools::ToolRegistry::new();

        let capabilities = runtime_host_capabilities(&config, &tools);

        assert!(!capabilities.supports_delegated_skill_invocation());
        assert!(!capabilities.tools.contains("invoke_delegated_skill"));
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
            &WorkspaceRuntimeConfig::default(),
            &crate::tools::ToolRegistry::new(),
            [temp.path()],
        );

        assert!(capabilities.supports_required_tool("demo"));
    }

    #[test]
    fn test_agent_runtime_config_from_core_config() {
        let core_config = crate::config::Config::default();
        let runtime_config = WorkspaceRuntimeConfig::from(core_config.clone());

        assert!(runtime_config.workspace_id.starts_with("workspace-"));
        assert_eq!(runtime_config.workspace_root_dir, None);
        assert_eq!(runtime_config.workspace_alan_dir, None);
    }

    #[test]
    fn test_agent_runtime_config_clone() {
        let config = WorkspaceRuntimeConfig::default();
        let cloned = config.clone();
        assert_eq!(config.workspace_id, cloned.workspace_id);
    }

    #[test]
    fn test_agent_runtime_config_debug() {
        let config = WorkspaceRuntimeConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("WorkspaceRuntimeConfig"));
        assert!(debug_str.contains("workspace_id"));
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
    fn test_apply_persisted_state_some_zero_values() {
        // Regression test: ensure Some(0) values are correctly restored
        // and not treated as "not set" (which would use defaults instead)
        use crate::WorkspaceConfigState;

        let base_config = WorkspaceRuntimeConfig::default();
        let mut restored_config = base_config.clone();

        // Create persisted state with explicit 0 values
        let persisted = WorkspaceConfigState {
            max_tool_loops: Some(0),    // 0 = unlimited
            tool_repeat_limit: Some(0), // 0 = disable protection
            llm_timeout_secs: Some(0),  // 0 = no timeout
            tool_timeout_secs: Some(0), // 0 = no timeout
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        restored_config.apply_persisted_state(&persisted);

        // Verify Some(0) values were restored (not skipped)
        assert_eq!(
            restored_config.agent_config.runtime_config.max_tool_loops, 0,
            "max_tool_loops Some(0) should be restored"
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .tool_repeat_limit,
            0,
            "tool_repeat_limit Some(0) should be restored"
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .llm_request_timeout_secs,
            0,
            "llm_timeout_secs Some(0) should be restored"
        );
        assert_eq!(
            restored_config.agent_config.core_config.tool_timeout_secs, 0,
            "tool_timeout_secs Some(0) should be restored"
        );
    }

    #[test]
    fn test_agent_config_with_agent_root_overlays_updates_unmodified_runtime_fields() {
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
        let merged = base.with_agent_root_overlays(&[overlay_path]).unwrap();

        assert_eq!(merged.core_config.tool_repeat_limit, 9);
        assert!(merged.core_config.prompt_snapshot_enabled);
        assert_eq!(merged.runtime_config.tool_repeat_limit, 9);
        assert!(merged.runtime_config.prompt_snapshot_enabled);
    }

    #[test]
    fn test_agent_config_with_agent_root_overlays_updates_unmodified_reasoning_effort() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        write_agent_overlay(
            &overlay_path,
            r#"
model_reasoning_effort = "high"
"#,
        );

        let base = AgentConfig::from(crate::Config::default());
        let merged = base.with_agent_root_overlays(&[overlay_path]).unwrap();

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
    fn test_agent_config_with_agent_root_overlays_preserves_runtime_overrides() {
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

        let merged = base.with_agent_root_overlays(&[overlay_path]).unwrap();

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
    #[allow(clippy::field_reassign_with_default)]
    fn test_effective_core_config_for_runtime_preserves_explicit_agent_overrides_after_overlay() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let overlay_path = workspace_alan_dir.join("agents/default/agent.toml");
        std::fs::create_dir_all(overlay_path.parent().unwrap()).unwrap();
        std::fs::write(
            &overlay_path,
            r#"
	model_reasoning_effort = "high"
	"#,
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig {
            core_config_source: crate::ConfigSourceKind::GlobalAgentHome,
            workspace_root_dir: Some(workspace_root),
            workspace_alan_dir: Some(workspace_alan_dir.clone()),
            agent_home_paths: Some(crate::AlanHomePaths::from_home_dir(
                &temp.path().join("home"),
            )),
            ..WorkspaceRuntimeConfig::default()
        };
        config.agent_config.set_model_override("override-model");
        config
            .agent_config
            .set_model_reasoning_effort_override(Some(alan_agent_protocol::ReasoningEffort::Low));

        let core_config = effective_core_config_for_runtime(&config).unwrap();

        assert_eq!(core_config.openai_responses_model, "override-model");
        assert_eq!(
            core_config.model_reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
        assert_eq!(
            core_config.memory.workspace_dir,
            Some(crate::workspace_runtime_memory_dir_from_alan_dir(
                &workspace_alan_dir,
                crate::InstallChannel::Stable
            ))
        );
    }

    #[test]
    fn test_agent_config_with_agent_root_overlays_preserves_marked_same_value_runtime_overrides() {
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

        let merged = base.with_agent_root_overlays(&[overlay_path]).unwrap();

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

    #[test]
    fn test_apply_persisted_state_none_uses_base() {
        // Test that None values fall back to base config defaults
        use crate::WorkspaceConfigState;

        let base_config = WorkspaceRuntimeConfig::default();
        let mut restored_config = base_config.clone();

        // Create persisted state with None values
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        restored_config.apply_persisted_state(&persisted);

        // Verify None values use base config defaults
        assert_eq!(
            restored_config.agent_config.runtime_config.max_tool_loops,
            base_config.agent_config.runtime_config.max_tool_loops
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .tool_repeat_limit,
            base_config.agent_config.runtime_config.tool_repeat_limit
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .llm_request_timeout_secs,
            base_config
                .agent_config
                .runtime_config
                .llm_request_timeout_secs
        );
        assert_eq!(
            restored_config.agent_config.core_config.tool_timeout_secs,
            base_config.agent_config.core_config.tool_timeout_secs
        );
    }

    #[test]
    fn test_apply_persisted_state_non_zero_values() {
        // Test that non-zero values are correctly restored
        use crate::WorkspaceConfigState;

        let base_config = WorkspaceRuntimeConfig::default();
        let mut restored_config = base_config.clone();

        // Create persisted state with specific non-zero values
        let persisted = WorkspaceConfigState {
            max_tool_loops: Some(10),
            tool_repeat_limit: Some(8),
            llm_timeout_secs: Some(300),
            tool_timeout_secs: Some(60),
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        restored_config.apply_persisted_state(&persisted);

        // Verify values were restored
        assert_eq!(
            restored_config.agent_config.runtime_config.max_tool_loops,
            10
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .tool_repeat_limit,
            8
        );
        assert_eq!(
            restored_config
                .agent_config
                .runtime_config
                .llm_request_timeout_secs,
            300
        );
        assert_eq!(
            restored_config.agent_config.core_config.tool_timeout_secs,
            60
        );
    }

    #[test]
    fn test_apply_persisted_state_temperature_and_max_tokens() {
        use crate::WorkspaceConfigState;

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: None,
            llm_model: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            context_window_tokens: Some(32_768),
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: Some(0.7),
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(config.agent_config.runtime_config.temperature, 0.7);
        assert_eq!(config.agent_config.runtime_config.max_tokens, 4096);
        assert_eq!(
            config.agent_config.runtime_config.context_window_tokens,
            32_768
        );
        assert_eq!(
            config
                .agent_config
                .runtime_config
                .compaction_hard_trigger_ratio,
            0.7
        );
        assert!(
            (config
                .agent_config
                .runtime_config
                .compaction_soft_trigger_ratio
                - 0.63)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn test_apply_persisted_state_derives_soft_threshold_when_persisted_pair_is_invalid() {
        use crate::WorkspaceConfigState;

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: Some(0.75),
            compaction_hard_trigger_ratio: Some(0.7),
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(
            config
                .agent_config
                .runtime_config
                .compaction_hard_trigger_ratio,
            0.7
        );
        assert!(
            (config
                .agent_config
                .runtime_config
                .compaction_soft_trigger_ratio
                - 0.63)
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn test_apply_persisted_state_gemini_provider() {
        use crate::config::LlmProvider;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::GoogleGeminiGenerateContent),
            llm_model: Some("gemini-2.0-pro".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert!(matches!(
            config.agent_config.core_config.llm_provider,
            LlmProvider::GoogleGeminiGenerateContent
        ));
        assert_eq!(
            config
                .agent_config
                .core_config
                .google_gemini_generate_content_model,
            "gemini-2.0-pro"
        );
    }

    #[test]
    fn test_apply_persisted_state_openai_provider() {
        use crate::config::LlmProvider;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::OpenAiResponses),
            llm_model: Some("gpt-5.4".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert!(matches!(
            config.agent_config.core_config.llm_provider,
            LlmProvider::OpenAiResponses
        ));
        assert_eq!(
            config.agent_config.core_config.openai_responses_model,
            "gpt-5.4"
        );
    }

    #[test]
    fn test_apply_persisted_state_openai_chat_completions_compatible_provider() {
        use crate::config::LlmProvider;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::OpenAiChatCompletionsCompatible),
            llm_model: Some("qwen3.5-plus-2026-02-15".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert!(matches!(
            config.agent_config.core_config.llm_provider,
            LlmProvider::OpenAiChatCompletionsCompatible
        ));
        assert_eq!(
            config
                .agent_config
                .core_config
                .openai_chat_completions_compatible_model,
            "qwen3.5-plus-2026-02-15"
        );
    }

    #[test]
    fn test_apply_persisted_state_openai_chat_completions_provider() {
        use crate::config::LlmProvider;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::OpenAiChatCompletions),
            llm_model: Some("gpt-5.4".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert!(matches!(
            config.agent_config.core_config.llm_provider,
            LlmProvider::OpenAiChatCompletions
        ));
        assert_eq!(
            config
                .agent_config
                .core_config
                .openai_chat_completions_model,
            "gpt-5.4"
        );
    }

    #[test]
    fn test_apply_persisted_state_anthropic_provider() {
        use crate::config::LlmProvider;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::AnthropicMessages),
            llm_model: Some("claude-3-5-sonnet".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert!(matches!(
            config.agent_config.core_config.llm_provider,
            LlmProvider::AnthropicMessages
        ));
        assert_eq!(
            config.agent_config.core_config.anthropic_messages_model,
            "claude-3-5-sonnet"
        );
        assert_eq!(
            config.agent_config.runtime_config.context_window_tokens,
            200_000
        );
    }

    #[test]
    fn test_apply_persisted_state_refreshes_legacy_context_window_fallback() {
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::GoogleGeminiGenerateContent),
            llm_model: Some("gemini-2.5-pro".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(
            config.agent_config.runtime_config.context_window_tokens,
            1_048_576
        );
    }

    #[test]
    fn test_apply_persisted_state_keeps_explicit_context_window_override() {
        use crate::config::Config;
        use crate::{PersistedLlmProvider, WorkspaceConfigState};

        let mut config = WorkspaceRuntimeConfig::from(Config {
            llm_provider: crate::config::LlmProvider::OpenAiResponses,
            openai_responses_model: "gpt-5.4".to_string(),
            model_reasoning_effort: None,
            context_window_tokens: Some(42_000),
            ..Config::default()
        });
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: Some(PersistedLlmProvider::GoogleGeminiGenerateContent),
            llm_model: Some("gemini-2.5-pro".to_string()),
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: None,
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(
            config.agent_config.runtime_config.context_window_tokens,
            42_000
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_agent_runtime_config_set_workspace_paths() {
        let temp = TempDir::new().unwrap();
        let config = WorkspaceRuntimeConfig {
            workspace_root_dir: Some(temp.path().to_path_buf()),
            workspace_alan_dir: Some(temp.path().join(".alan")),
            ..Default::default()
        };

        assert_eq!(config.workspace_root_dir, Some(temp.path().to_path_buf()));
        assert_eq!(config.workspace_alan_dir, Some(temp.path().join(".alan")));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_workspace_runtime_config_set_workspace_id() {
        let mut config = WorkspaceRuntimeConfig::default();
        config.workspace_id = "custom-workspace-123".to_string();

        assert_eq!(config.workspace_id, "custom-workspace-123");
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_effective_core_config_for_runtime_applies_workspace_agent_overlays() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let overlay_path = workspace_alan_dir.join("agents/default/agent.toml");
        std::fs::create_dir_all(overlay_path.parent().unwrap()).unwrap();
        std::fs::write(
            &overlay_path,
            r#"
	tool_repeat_limit = 9
	model_reasoning_effort = "high"
	"#,
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::default();
        config.core_config_source = crate::ConfigSourceKind::GlobalAgentHome;
        config.workspace_root_dir = Some(workspace_root);
        config.workspace_alan_dir = Some(workspace_alan_dir.clone());
        config.agent_home_paths = Some(crate::AlanHomePaths::from_home_dir(
            &temp.path().join("home"),
        ));
        config.agent_config.core_config.llm_provider = crate::config::LlmProvider::OpenAiResponses;
        config.agent_config.core_config.openai_responses_api_key = Some("sk-openai-test".into());
        config.agent_config.core_config.openai_responses_model = "gpt-5.4".into();

        let core_config = effective_core_config_for_runtime(&config).unwrap();

        assert!(matches!(
            core_config.llm_provider,
            crate::config::LlmProvider::OpenAiResponses
        ));
        assert_eq!(core_config.tool_repeat_limit, 9);
        assert_eq!(
            core_config.model_reasoning_effort,
            Some(alan_agent_protocol::ReasoningEffort::High)
        );
        assert_eq!(
            core_config.memory.workspace_dir,
            Some(
                workspace_alan_dir
                    .join("runtime")
                    .join("stable")
                    .join("memory")
            )
        );
    }

    #[test]
    fn test_effective_core_config_for_runtime_dev_memory_is_channel_scoped() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        std::fs::create_dir_all(workspace_alan_dir.join("memory")).unwrap();

        let config = WorkspaceRuntimeConfig {
            workspace_root_dir: Some(workspace_root),
            workspace_alan_dir: Some(workspace_alan_dir.clone()),
            agent_home_paths: Some(crate::AlanHomePaths::from_home_dir_for_channel(
                &temp.path().join("home"),
                crate::InstallChannel::Dev,
            )),
            ..WorkspaceRuntimeConfig::default()
        };

        let core_config = effective_core_config_for_runtime(&config).unwrap();

        assert_eq!(
            core_config.memory.workspace_dir,
            Some(
                workspace_alan_dir
                    .join("runtime")
                    .join("dev")
                    .join("memory")
            )
        );
    }

    #[test]
    fn test_apply_persisted_state_tool_policy_settings() {
        use crate::WorkspaceConfigState;

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: Some(alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: Some(".alan/agents/default/policy.yaml".to_string()),
            }),
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(
            config.agent_config.runtime_config.governance,
            alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: Some(".alan/agents/default/policy.yaml".to_string()),
            }
        );
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
    fn test_agent_runtime_config_with_workspace_paths() {
        let temp = TempDir::new().unwrap();
        let config = WorkspaceRuntimeConfig {
            workspace_root_dir: Some(temp.path().to_path_buf()),
            workspace_alan_dir: Some(temp.path().join(".alan")),
            ..Default::default()
        };

        assert_eq!(config.workspace_root_dir, Some(temp.path().to_path_buf()));
        assert_eq!(config.workspace_alan_dir, Some(temp.path().join(".alan")));
    }

    #[test]
    fn test_agent_runtime_config_recovery_rollout_path() {
        let temp = TempDir::new().unwrap();
        let rollout_path = temp.path().join("rollout.jsonl");

        let config = WorkspaceRuntimeConfig {
            recovery_rollout_path: Some(rollout_path.clone()),
            ..Default::default()
        };

        assert_eq!(config.recovery_rollout_path, Some(rollout_path));
    }

    #[tokio::test]
    async fn test_initialize_agent_machine_from_rollout_preserves_current_process_cwd() {
        let temp = TempDir::new().unwrap();
        let recovered_cwd = temp.path().join("workspace/src");
        let recovered_rollouts = temp.path().join("recovered-rollouts");
        tokio::fs::create_dir_all(&recovered_cwd).await.unwrap();
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
            Some(recovered_cwd.as_path()),
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
            Some(recovered_cwd.to_string_lossy().as_ref())
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

    #[test]
    fn test_apply_persisted_state_governance_profile() {
        use crate::WorkspaceConfigState;

        let mut config = WorkspaceRuntimeConfig::default();
        let persisted = WorkspaceConfigState {
            max_tool_loops: None,
            tool_repeat_limit: None,
            llm_timeout_secs: None,
            tool_timeout_secs: None,
            llm_provider: None,
            llm_model: None,
            temperature: None,
            max_tokens: None,
            context_window_tokens: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            streaming_mode: None,
            partial_stream_recovery_mode: None,
            governance: Some(alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            }),
        };

        config.apply_persisted_state(&persisted);

        assert_eq!(
            config.agent_config.runtime_config.governance.profile,
            alan_agent_protocol::GovernanceProfile::Autonomous
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_continues_when_workspace_persona_bootstrap_is_unwritable() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let alan_dir = workspace_root.join(".alan");
        let persona_dir = alan_dir.join("agents/default/persona");

        std::fs::create_dir_all(&persona_dir).unwrap();
        std::fs::write(persona_dir.join("SOUL.md"), "existing persona").unwrap();

        let mut permissions = std::fs::metadata(&persona_dir).unwrap().permissions();
        permissions.set_mode(0o555);
        std::fs::set_permissions(&persona_dir, permissions).unwrap();

        let config = WorkspaceRuntimeConfig {
            workspace_root_dir: Some(workspace_root),
            workspace_alan_dir: Some(alan_dir),
            ..WorkspaceRuntimeConfig::default()
        };

        let llm_client = LlmClient::new(MockLlmProvider::new());
        let mut controller = spawn_with_llm_client(config, llm_client).unwrap();
        let ready = controller.wait_until_ready().await;

        let mut cleanup_permissions = std::fs::metadata(&persona_dir).unwrap().permissions();
        cleanup_permissions.set_mode(0o755);
        std::fs::set_permissions(&persona_dir, cleanup_permissions).unwrap();

        assert!(ready.is_ok());
        controller.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_spawn_initializes_bootstrap_for_memory_dir_persona_fallback() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        let memory_dir = workspace_root.join(".alan/memory");
        let persona_dir = workspace_root.join(".alan/agents/default/persona");
        std::fs::create_dir_all(&memory_dir).unwrap();

        let config = WorkspaceRuntimeConfig {
            agent_config: crate::AgentConfig {
                core_config: crate::Config {
                    memory: crate::config::MemoryConfig {
                        workspace_dir: Some(memory_dir),
                        strict_workspace: false,
                        ..crate::config::MemoryConfig::default()
                    },
                    ..crate::Config::default()
                },
                ..crate::AgentConfig::default()
            },
            ..WorkspaceRuntimeConfig::default()
        };

        let llm_client = LlmClient::new(MockLlmProvider::new());
        let mut controller = spawn_with_llm_client(config, llm_client).unwrap();
        let ready = controller.wait_until_ready().await;

        assert!(ready.is_ok());
        assert!(persona_dir.join("SOUL.md").exists());
        controller.shutdown().await.unwrap();
    }
}
