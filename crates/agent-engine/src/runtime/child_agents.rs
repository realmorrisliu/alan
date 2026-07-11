use super::agent_loop::RuntimeLoopState;
use super::child_runs::{
    ChildRunRecord, ChildRunRegistry, ChildRunStatus, ChildRunTerminationMode,
    ChildRunTerminationRequest,
};
use super::delegation_capabilities::{
    DelegatedSpawnRejected, evaluate_delegated_namespace, namespace_summary_from_bindings,
};
use super::engine::{
    AgentConfig, RuntimeController, RuntimeEventEnvelope, RuntimeLivenessEnvelope,
    RuntimeStartupMetadata, WorkspaceRuntimeConfig, spawn_with_namespace_environment,
};
use crate::llm::LlmClient;
use crate::tape::{ContentPart, Message};
use crate::tools::ToolRegistry;
use alan_agent_protocol::{
    DelegatedCapabilityDecision, DelegatedCapabilityRecovery, GovernanceConfig, Op, SpawnHandle,
    SpawnSpec, SpawnTarget, Submission, YieldKind,
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec};
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio_util::sync::CancellationToken;

const CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE: &str = "Child-agent launch cancelled";
const MAX_CHILD_CONVERSATION_MESSAGES: usize = 8;
const MAX_CHILD_CONVERSATION_CHARS: usize = 4_000;
const MAX_CHILD_PLAN_ITEMS: usize = 16;
const MAX_CHILD_PLAN_ITEM_CHARS: usize = 240;
const MAX_CHILD_TOOL_RESULTS: usize = 6;
const MAX_CHILD_TOOL_RESULT_CHARS: usize = 1_200;
const MAX_OBSERVED_CHILD_WARNINGS: usize = 32;
const MAX_OBSERVED_CHILD_WARNING_CHARS: usize = 512;
static NEXT_CHILD_NAMESPACE_FID: AtomicU64 = AtomicU64::new(80_000);

struct ChildLlmProvider {
    client: LlmClient,
}

impl ChildLlmProvider {
    fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ChildLlmProvider {
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

struct ChildToolProcessRunner {
    tools: ToolRegistry,
}

impl ChildToolProcessRunner {
    fn new(tools: ToolRegistry) -> Self {
        Self { tools }
    }
}

#[async_trait::async_trait]
impl alan_kernel::ProcessRunner for ChildToolProcessRunner {
    async fn run(&self, invocation: alan_kernel::ProcessInvocation) -> alan_kernel::ProcessOutcome {
        if invocation
            .namespace
            .resolve(&invocation.exec.executable)
            .is_err()
        {
            return alan_kernel::ProcessOutcome::exited(127, b"executable is not mounted\n");
        }
        let tool_name = invocation
            .exec
            .executable
            .rsplit('/')
            .next()
            .unwrap_or(invocation.exec.executable.as_str());
        let arguments = invocation
            .exec
            .args
            .first()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or(serde_json::Value::Null);

        match self.tools.execute(tool_name, arguments).await {
            Ok(output) => {
                let mut bytes =
                    serde_json::to_vec(&output).unwrap_or_else(|_| b"{\"success\":true}".to_vec());
                bytes.push(b'\n');
                alan_kernel::ProcessOutcome::exited(0, bytes)
            }
            Err(err) => {
                let mut bytes = serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error": format!("{err:#}"),
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec());
                bytes.push(b'\n');
                alan_kernel::ProcessOutcome::exited(1, bytes)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChildRuntimeStatus {
    Completed,
    Paused,
    Cancelled,
    TimedOut,
    Terminated,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildRuntimePause {
    pub request_id: String,
    pub kind: YieldKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChildRuntimeResult {
    pub status: ChildRuntimeStatus,
    pub process_path: String,
    pub child_run_id: Option<String>,
    pub rollout_path: Option<PathBuf>,
    pub output_text: String,
    pub turn_summary: Option<String>,
    pub structured_output: Option<serde_json::Value>,
    pub warnings: Vec<String>,
    pub error_message: Option<String>,
    pub pause: Option<ChildRuntimePause>,
    pub child_run: Option<ChildRunRecord>,
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

enum ChildEventObservation {
    Terminal(ObservedChildTerminalEvent),
    Progress,
    Ignored,
}

enum ChildLivenessObservation {
    Progress,
    Ignored,
    Closed,
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

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ChildRuntimeController {
    runtime: Option<RuntimeController>,
    startup_metadata: RuntimeStartupMetadata,
    event_rx: tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>,
    liveness_rx: tokio::sync::broadcast::Receiver<RuntimeLivenessEnvelope>,
    submission_id: String,
    child_run_id: String,
    child_run_registry: ChildRunRegistry,
    timeout: Option<Duration>,
    process_registry: Option<alan_kernel::ProcFs>,
    process_environment: Option<super::NamespaceRuntimeEnvironment>,
    process_pid: Option<String>,
}

#[allow(dead_code)]
pub(crate) async fn spawn_child_runtime(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
) -> Result<ChildRuntimeController> {
    spawn_child_runtime_with_optional_cancel(parent, spec, None).await
}

#[allow(dead_code)]
pub(crate) async fn spawn_child_runtime_cancellable(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<ChildRuntimeController> {
    spawn_child_runtime_with_optional_cancel(parent, spec, Some(cancel)).await
}

async fn spawn_child_runtime_with_optional_cancel(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: Option<&CancellationToken>,
) -> Result<ChildRuntimeController> {
    let chatgpt_auth_storage_path = parent.runtime_config.chatgpt_auth_storage_path.clone();
    spawn_child_runtime_with_client_factory_and_cancel(
        parent,
        spec,
        move |core_config| {
            LlmClient::from_core_config_with_chatgpt_auth_storage_path(
                core_config,
                chatgpt_auth_storage_path.clone(),
            )
        },
        cancel,
    )
    .await
}

#[cfg(test)]
async fn spawn_child_runtime_with_client_factory<F>(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    llm_client_factory: F,
) -> Result<ChildRuntimeController>
where
    F: FnOnce(&crate::Config) -> Result<LlmClient>,
{
    spawn_child_runtime_with_client_factory_and_cancel(
        parent,
        spec,
        |core_config| llm_client_factory(core_config),
        None,
    )
    .await
}

async fn spawn_child_runtime_with_client_factory_and_cancel<F>(
    parent: &RuntimeLoopState,
    mut spec: SpawnSpec,
    llm_client_factory: F,
    cancel: Option<&CancellationToken>,
) -> Result<ChildRuntimeController>
where
    F: FnOnce(&crate::Config) -> Result<LlmClient>,
{
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
    }

    validate_child_launch_contract(&spec)?;
    let launch_root_dir = resolve_launch_root_dir(parent, &spec.target)?;
    let child_agent_config = build_child_agent_config(parent, &spec);
    let workspace_root_dir = resolve_child_workspace_root(parent, &spec);
    let workspace_alan_dir = resolve_child_workspace_alan_dir(
        &spec,
        workspace_root_dir.as_deref(),
        parent.core_config.memory.workspace_dir.as_deref(),
    );
    let child_workspace_id = format!("{}:child:{}", parent.workspace_id, uuid::Uuid::new_v4());
    let default_cwd_override = spec
        .launch
        .cwd
        .clone()
        .or_else(|| workspace_root_dir.clone());

    let mut child_config = WorkspaceRuntimeConfig {
        agent_config: child_agent_config.clone(),
        // Child launches should still resolve their target/root overlays. Using the
        // default source keeps launch-root agent.toml in play instead of treating the
        // parent's effective config as a terminal env override.
        core_config_source: crate::ConfigSourceKind::Default,
        agent_name: None,
        workspace_id: child_workspace_id,
        workspace_root_dir,
        workspace_alan_dir,
        recovery_rollout_path: None,
        launch_root_dir,
        default_cwd_override,
        agent_home_paths: parent_agent_home_paths(parent),
        chatgpt_auth_storage_path: parent.runtime_config.chatgpt_auth_storage_path.clone(),
        mount_grant_applicator_factory: parent
            .namespace_environment()
            .mount_grant_applicator_factory(),
    };
    let resolved_child_definition =
        crate::ResolvedAgentDefinition::from_runtime_config(&child_config)
            .context("Failed to resolve child-agent definition")?;
    let mut resolved_child_agent_config = child_agent_config.clone();
    if !resolved_child_definition.config_overlay_paths.is_empty() {
        resolved_child_agent_config = resolved_child_agent_config
            .with_agent_root_overlays(&resolved_child_definition.config_overlay_paths)
            .context("Failed to resolve effective child-agent config")?;
    }
    if spec.has_handle(SpawnHandle::Memory) {
        if let Some(alan_dir) = resolved_child_definition.workspace_alan_dir.as_ref() {
            let channel = parent_runtime_channel(parent);
            resolved_child_agent_config.core_config.memory.workspace_dir = Some(
                crate::workspace_memory_dir_for_channel_from_alan_dir(alan_dir, channel),
            );
        }
    } else {
        resolved_child_agent_config.core_config.memory.workspace_dir = None;
    }
    let effective_child_core_config = resolved_child_agent_config.core_config.clone();
    child_config.agent_config = resolved_child_agent_config;
    child_config.core_config_source = crate::ConfigSourceKind::EnvOverride;
    let child_namespace_plan =
        build_child_namespace_assembly_plan(parent, &spec, &effective_child_core_config)
            .context("Failed to assemble child-agent namespace plan")?;
    let delegation_capability_decision =
        evaluate_delegated_launch_capabilities(parent, &mut spec, &child_namespace_plan)?;
    let child_tools = build_child_tool_registry_from_namespace_plan(
        parent,
        &spec,
        &effective_child_core_config,
        &child_namespace_plan,
    )
    .context("Failed to build child-agent tool registry")?;
    let llm_client = llm_client_factory(&effective_child_core_config)
        .context("Failed to create child-agent LLM client")?;
    let parent_process_context = parent.namespace_environment().process_context();
    let launch_procfs = parent_process_context
        .as_ref()
        .map(|context| context.procfs.clone())
        .unwrap_or_default();
    let runtime_procfs = launch_procfs
        .clone()
        .with_runner(Arc::new(ChildToolProcessRunner::new(child_tools.clone())));
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        &child_namespace_plan.llm_connection_name()?,
        Box::new(ChildLlmProvider::new(llm_client)),
    );
    let mut handles = child_namespace_launch_handles_from_parent(parent, agentfs, llmfs)
        .context("Failed to assemble child-agent shared namespace handles")?;
    for mount in &child_namespace_plan.bin_tool_mounts {
        handles = handles.with_bin_tool(
            mount.clone(),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        );
    }
    let namespace_launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &child_namespace_plan,
        handles,
        parent_process_context,
        child_config.mount_grant_applicator_factory.clone(),
        "/bin/alan-agent",
    )
    .await
    .context("Failed to spawn child-agent process namespace")?;
    let child_process_environment = namespace_launch.environment.clone();
    let child_process_pid = namespace_launch.pid.clone();
    let generation_capabilities =
        crate::provider_capabilities_for_config(&effective_child_core_config);
    let runtime = match spawn_with_namespace_environment(
        child_config,
        namespace_launch.environment,
        child_tools,
        generation_capabilities,
    )
    .context("Failed to spawn child-agent namespace runtime")
    {
        Ok(runtime) => runtime,
        Err(err) => {
            record_child_launch_failure_process(
                &launch_procfs,
                &child_process_environment,
                &child_process_pid,
                &err,
            )
            .await;
            return Err(err);
        }
    };
    let (runtime, startup_metadata) = match wait_for_child_runtime_startup(runtime, cancel).await {
        Ok(ready) => ready,
        Err(err) => {
            record_child_launch_failure_process(
                &launch_procfs,
                &child_process_environment,
                &child_process_pid,
                &err,
            )
            .await;
            return Err(err);
        }
    };
    let child_run_registry = parent.child_run_registry().clone();
    let child_run_id = uuid::Uuid::new_v4().to_string();
    let mut child_run_record = ChildRunRecord::new(
        child_run_id.clone(),
        parent.process_path().to_string(),
        startup_metadata.process_path.clone(),
        resolved_child_definition
            .workspace_root_dir
            .as_ref()
            .map(|path| path.display().to_string()),
        Some(startup_metadata.agent_path.clone()),
        Some(format!("{:?}", spec.target)),
    );
    if let Some(decision) = delegation_capability_decision {
        child_run_record = child_run_record.with_delegation_capability_decision(decision);
    }
    child_run_registry.register(child_run_record);
    let event_rx = runtime.handle.event_sender.subscribe();
    let liveness_rx = runtime.handle.liveness_sender.subscribe();
    let submission = Submission::new(Op::Turn {
        parts: vec![ContentPart::text(build_child_task_text(parent, &spec))],
        context: None,
    });
    let runtime = match send_initial_child_submission(runtime, submission.clone(), cancel).await {
        Ok(runtime) => runtime,
        Err(err) => {
            let status = child_run_status_for_launch_error(&err);
            record_child_launch_failure_process(
                &launch_procfs,
                &child_process_environment,
                &child_process_pid,
                &err,
            )
            .await;
            child_run_registry.mark_terminal(&child_run_id, status, Some(format!("{err:#}")));
            return Err(err);
        }
    };
    child_run_registry.mark_running(&child_run_id);

    Ok(ChildRuntimeController {
        runtime: Some(runtime),
        startup_metadata,
        event_rx,
        liveness_rx,
        submission_id: submission.id,
        child_run_id,
        child_run_registry,
        timeout: spec.launch.timeout_secs.map(Duration::from_secs),
        process_registry: Some(launch_procfs),
        process_environment: Some(child_process_environment),
        process_pid: Some(child_process_pid),
    })
}

fn evaluate_delegated_launch_capabilities(
    parent: &RuntimeLoopState,
    spec: &mut SpawnSpec,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<Option<DelegatedCapabilityDecision>> {
    let Some(context) = spec.delegated.as_ref() else {
        return Ok(None);
    };
    let requirements = context.requirements.clone();
    let child_namespace = namespace_summary_from_child_plan(plan);
    let parent_namespace = namespace_summary_from_parent(parent);
    let decision = evaluate_delegated_namespace(
        &spec.launch.task,
        &requirements,
        child_namespace,
        &parent_namespace,
    );

    match decision.recovery {
        DelegatedCapabilityRecovery::Satisfied => Ok(Some(decision)),
        DelegatedCapabilityRecovery::Narrowed => {
            if let Some(narrowed_task) = decision.narrowed_task.clone() {
                spec.launch.task = narrowed_task;
            }
            Ok(Some(decision))
        }
        DelegatedCapabilityRecovery::ParentPath
        | DelegatedCapabilityRecovery::AskUser
        | DelegatedCapabilityRecovery::Limitation => {
            Err(DelegatedSpawnRejected { decision }.into())
        }
    }
}

fn namespace_summary_from_child_plan(
    plan: &ChildNamespaceAssemblyPlan,
) -> alan_agent_protocol::DelegatedNamespaceSummary {
    namespace_summary_from_bindings(
        vec![
            plan.agent_mount.clone(),
            plan.llm_mount.clone(),
            plan.srv_mount.clone(),
            plan.route_mount.clone(),
        ],
        plan.bin_tool_mounts.clone(),
        plan.workspace_root.clone(),
        Some(plan.llm_connection_name.clone()),
    )
}

fn namespace_summary_from_parent(
    parent: &RuntimeLoopState,
) -> alan_agent_protocol::DelegatedNamespaceSummary {
    namespace_summary_from_bindings(
        vec![
            "/agent".to_string(),
            "/mnt/llm".to_string(),
            "/srv".to_string(),
            alan_routefs::MOUNT_PATH.to_string(),
        ],
        parent
            .static_tool_names()
            .into_iter()
            .map(|tool| format!("/bin/{tool}"))
            .collect(),
        bound_workspace_root(parent),
        Some(
            parent
                .core_config
                .connection_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ),
    )
}

async fn wait_for_child_runtime_startup(
    mut runtime: RuntimeController,
    cancel: Option<&CancellationToken>,
) -> Result<(RuntimeController, RuntimeStartupMetadata)> {
    let startup_metadata = if let Some(cancel) = cancel {
        if cancel.is_cancelled() {
            runtime.abort().await;
            bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
        }
        tokio::select! {
            _ = cancel.cancelled() => {
                runtime.abort().await;
                bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
            }
            ready = runtime.wait_until_ready() => {
                ready.context("Child-agent runtime failed to start")?
            }
        }
    } else {
        runtime
            .wait_until_ready()
            .await
            .context("Child-agent runtime failed to start")?
    };

    Ok((runtime, startup_metadata))
}

async fn send_initial_child_submission(
    runtime: RuntimeController,
    submission: Submission,
    cancel: Option<&CancellationToken>,
) -> Result<RuntimeController> {
    if let Some(cancel) = cancel {
        if cancel.is_cancelled() {
            runtime.abort().await;
            bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
        }
        tokio::select! {
            _ = cancel.cancelled() => {
                runtime.abort().await;
                bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
            }
            result = runtime.handle.submission_tx.send(submission) => {
                result.context("Failed to submit initial child-agent turn")?
            }
        }
    } else {
        runtime
            .handle
            .submission_tx
            .send(submission)
            .await
            .context("Failed to submit initial child-agent turn")?;
    }

    Ok(runtime)
}

fn validate_child_launch_contract(spec: &SpawnSpec) -> Result<()> {
    if spec.has_handle(SpawnHandle::Artifacts) || spec.launch.output_dir.is_some() {
        bail!(
            "Child-agent launches do not support artifact routing yet; omit SpawnHandle::Artifacts and launch.output_dir."
        );
    }

    if let Some(workspace_root) = spec.launch.workspace_root.as_deref()
        && !workspace_root.is_absolute()
    {
        bail!(
            "Child-agent launch workspace_root '{}' must be absolute.",
            workspace_root.display()
        );
    }

    if let Some(cwd) = spec.launch.cwd.as_deref()
        && !cwd.is_absolute()
    {
        bail!(
            "Child-agent launch cwd '{}' must be absolute.",
            cwd.display()
        );
    }

    if let (Some(workspace_root), Some(cwd)) = (
        spec.launch.workspace_root.as_deref(),
        spec.launch.cwd.as_deref(),
    ) {
        let normalized_workspace_root = lexically_normalize_path(workspace_root);
        let normalized_cwd = lexically_normalize_path(cwd);
        if !normalized_cwd.starts_with(&normalized_workspace_root) {
            bail!(
                "Child-agent launch cwd '{}' must stay within workspace_root '{}'.",
                normalized_cwd.display(),
                normalized_workspace_root.display()
            );
        }
    }

    Ok(())
}

fn lexically_normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn resolve_launch_root_dir(
    parent: &RuntimeLoopState,
    target: &SpawnTarget,
) -> Result<Option<PathBuf>> {
    match target {
        SpawnTarget::ResolvedAgentRoot { root_dir } => Ok(Some(root_dir.clone())),
        SpawnTarget::PackageChildAgent { .. } => parent
            .prompt_cache
            .capability_view()
            .map(crate::skills::ResolvedCapabilityView::refresh)
            .and_then(|view| view.resolve_child_agent_target(target))
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("Unknown package child-agent target: {target:?}")),
    }
}

#[allow(dead_code)]
impl ChildRuntimeController {
    pub(crate) fn startup_metadata(&self) -> &RuntimeStartupMetadata {
        &self.startup_metadata
    }

    pub(crate) async fn join(mut self) -> Result<ChildRuntimeResult> {
        let observed = match self
            .wait_for_terminal_event_with_optional_cancel(None)
            .await?
        {
            ChildRuntimeWaitOutcome::Observed(observed) => observed,
            ChildRuntimeWaitOutcome::Cancelled => {
                return Ok(self.cancelled_result());
            }
        };

        self.finish_after_observed_terminal_event(observed).await
    }

    pub(crate) async fn join_until_cancelled(
        mut self,
        cancel: &CancellationToken,
    ) -> Result<ChildRuntimeResult> {
        match self
            .wait_for_terminal_event_with_optional_cancel(Some(cancel))
            .await?
        {
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
        let rollout_fallback_text = if observed.output_text.trim().is_empty() {
            read_latest_assistant_text_from_rollout(self.startup_metadata.rollout_path.as_deref())
                .await
        } else {
            None
        };
        let output_text = if observed.output_text.trim().is_empty() {
            rollout_fallback_text.unwrap_or(observed.output_text)
        } else {
            observed.output_text
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

    pub(crate) async fn cancel(mut self) -> Result<ChildRuntimeResult> {
        let result = self.cancelled_result();
        self.terminate_runtime().await;
        Ok(result)
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

    async fn wait_for_terminal_event_with_optional_cancel(
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
        let mut liveness_closed = false;
        let mut check_process_stop = false;

        loop {
            if let Some(observed) = self.observe_buffered_child_events(
                &mut output_text,
                &mut warnings,
                &mut latest_liveness_at,
            ) {
                if self.external_process_stop_observed().await {
                    self.abort_runtime().await;
                    return Ok(ChildRuntimeWaitOutcome::Observed(
                        self.externally_stopped_observed_event(
                            &observed.output_text,
                            &observed.warnings,
                        ),
                    ));
                }
                return Ok(ChildRuntimeWaitOutcome::Observed(observed));
            }

            if let Some(request) = self
                .child_run_registry
                .termination_request(&self.child_run_id)
            {
                if let Some(observed) = self.observe_buffered_child_events(
                    &mut output_text,
                    &mut warnings,
                    &mut latest_liveness_at,
                ) {
                    return Ok(ChildRuntimeWaitOutcome::Observed(observed));
                }
                match request.mode {
                    ChildRunTerminationMode::Graceful => self.terminate_runtime().await,
                    ChildRunTerminationMode::Forceful => self.abort_runtime().await,
                }
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    self.terminated_observed_event(request),
                ));
            }

            if check_process_stop && self.external_process_stop_observed().await {
                self.abort_runtime().await;
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    self.externally_stopped_observed_event(&output_text, &warnings),
                ));
            }
            check_process_stop = false;

            if let Some(cap) = wall_clock_cap
                && started_at.elapsed() >= cap
            {
                self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut)
                    .await;
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    self.timed_out_observed_event("Child-agent wall-clock cap exceeded"),
                ));
            }

            let recv = if let Some(timeout) = self.timeout {
                let deadline = latest_liveness_at + timeout;
                let idle_remaining = deadline.saturating_duration_since(Instant::now());
                if let Some(cancel) = cancel {
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            self.terminate_runtime().await;
                            return Ok(ChildRuntimeWaitOutcome::Cancelled);
                        }
                        _ = tokio::time::sleep(idle_remaining) => {
                            self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut).await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(
                                self.timed_out_observed_event("Child-agent turn idle timed out"),
                            ));
                        }
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {
                            check_process_stop = true;
                            continue;
                        }
                        liveness = self.liveness_rx.recv(), if !liveness_closed => {
                            self.apply_liveness_observation(
                                liveness,
                                &mut latest_liveness_at,
                                &mut liveness_closed,
                            );
                            continue;
                        }
                        recv = self.event_rx.recv() => recv,
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(idle_remaining) => {
                            self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut).await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(
                                self.timed_out_observed_event("Child-agent turn idle timed out"),
                            ));
                        }
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {
                            check_process_stop = true;
                            continue;
                        }
                        liveness = self.liveness_rx.recv(), if !liveness_closed => {
                            self.apply_liveness_observation(
                                liveness,
                                &mut latest_liveness_at,
                                &mut liveness_closed,
                            );
                            continue;
                        }
                        recv = self.event_rx.recv() => recv,
                    }
                }
            } else if let Some(cancel) = cancel {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        self.terminate_runtime().await;
                        return Ok(ChildRuntimeWaitOutcome::Cancelled);
                    }
                    liveness = self.liveness_rx.recv(), if !liveness_closed => {
                        self.apply_liveness_observation(
                            liveness,
                            &mut latest_liveness_at,
                            &mut liveness_closed,
                        );
                        continue;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {
                        check_process_stop = true;
                        continue;
                    }
                    recv = self.event_rx.recv() => recv,
                }
            } else {
                tokio::select! {
                    liveness = self.liveness_rx.recv(), if !liveness_closed => {
                        self.apply_liveness_observation(
                            liveness,
                            &mut latest_liveness_at,
                            &mut liveness_closed,
                        );
                        continue;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {
                        check_process_stop = true;
                        continue;
                    }
                    recv = self.event_rx.recv() => recv,
                }
            };

            match self.observe_child_event(recv, &mut output_text, &mut warnings) {
                ChildEventObservation::Terminal(observed) => {
                    if self.external_process_stop_observed().await {
                        self.abort_runtime().await;
                        return Ok(ChildRuntimeWaitOutcome::Observed(
                            self.externally_stopped_observed_event(
                                &observed.output_text,
                                &observed.warnings,
                            ),
                        ));
                    }
                    return Ok(ChildRuntimeWaitOutcome::Observed(observed));
                }
                ChildEventObservation::Progress => {
                    latest_liveness_at = Instant::now();
                    continue;
                }
                ChildEventObservation::Ignored => continue,
            }
        }
    }

    fn observe_buffered_child_events(
        &mut self,
        output_text: &mut String,
        warnings: &mut Vec<String>,
        latest_liveness_at: &mut Instant,
    ) -> Option<ObservedChildTerminalEvent> {
        loop {
            let recv = match self.event_rx.try_recv() {
                Ok(envelope) => Ok(envelope),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Lagged(skipped)) => Err(RecvError::Lagged(skipped)),
                Err(TryRecvError::Closed) => Err(RecvError::Closed),
            };
            match self.observe_child_event(recv, output_text, warnings) {
                ChildEventObservation::Terminal(observed) => return Some(observed),
                ChildEventObservation::Progress => {
                    *latest_liveness_at = Instant::now();
                }
                ChildEventObservation::Ignored => {}
            }
        }
    }

    fn apply_liveness_observation(
        &self,
        recv: std::result::Result<RuntimeLivenessEnvelope, RecvError>,
        latest_liveness_at: &mut Instant,
        liveness_closed: &mut bool,
    ) {
        match self.observe_liveness_event(recv) {
            ChildLivenessObservation::Progress => {
                *latest_liveness_at = Instant::now();
            }
            ChildLivenessObservation::Closed => {
                *liveness_closed = true;
            }
            ChildLivenessObservation::Ignored => {}
        }
    }

    fn observe_liveness_event(
        &self,
        recv: std::result::Result<RuntimeLivenessEnvelope, RecvError>,
    ) -> ChildLivenessObservation {
        match recv {
            Ok(envelope) => {
                if envelope.submission_id.as_deref() != Some(self.submission_id.as_str()) {
                    return ChildLivenessObservation::Ignored;
                }
                self.child_run_registry
                    .observe_heartbeat(&self.child_run_id, envelope.status.clone());
                self.child_run_registry.observe_progress(
                    &self.child_run_id,
                    "runtime_heartbeat",
                    envelope.status,
                );
                ChildLivenessObservation::Progress
            }
            Err(RecvError::Lagged(_)) => {
                let status = Some("active_submission".to_string());
                self.child_run_registry
                    .observe_heartbeat(&self.child_run_id, status.clone());
                self.child_run_registry.observe_progress(
                    &self.child_run_id,
                    "runtime_heartbeat",
                    status,
                );
                ChildLivenessObservation::Progress
            }
            Err(RecvError::Closed) => ChildLivenessObservation::Closed,
        }
    }

    fn observe_child_event(
        &self,
        recv: std::result::Result<RuntimeEventEnvelope, RecvError>,
        output_text: &mut String,
        warnings: &mut Vec<String>,
    ) -> ChildEventObservation {
        match recv {
            Ok(envelope) => {
                if envelope.submission_id.as_deref() != Some(self.submission_id.as_str()) {
                    return ChildEventObservation::Ignored;
                }

                match envelope.event {
                    alan_agent_protocol::Event::TextDelta { chunk, .. } => {
                        if !chunk.is_empty() {
                            output_text.push_str(&chunk);
                            self.child_run_registry.observe_progress(
                                &self.child_run_id,
                                "text_delta",
                                Some("child emitted text".to_string()),
                            );
                        }
                        ChildEventObservation::Progress
                    }
                    alan_agent_protocol::Event::Warning { message } => {
                        self.child_run_registry
                            .observe_warning(&self.child_run_id, message.clone());
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "warning",
                            Some(message.clone()),
                        );
                        push_bounded_child_warning(warnings, message);
                        ChildEventObservation::Progress
                    }
                    alan_agent_protocol::Event::TurnCompleted { summary } => {
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "turn_completed",
                            summary.clone(),
                        );
                        let structured_output = parse_child_structured_output(output_text.as_str());
                        ChildEventObservation::Terminal(ObservedChildTerminalEvent {
                            output_text: output_text.clone(),
                            turn_summary: summary,
                            structured_output,
                            warnings: warnings.clone(),
                            error_message: None,
                            pause: None,
                            status: ChildRuntimeStatus::Completed,
                        })
                    }
                    alan_agent_protocol::Event::Yield {
                        request_id, kind, ..
                    } => {
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "yield",
                            Some(format!("child yielded for {}", yield_kind_label(&kind))),
                        );
                        let structured_output = parse_child_structured_output(output_text.as_str());
                        ChildEventObservation::Terminal(ObservedChildTerminalEvent {
                            output_text: output_text.clone(),
                            turn_summary: None,
                            structured_output,
                            warnings: warnings.clone(),
                            error_message: None,
                            pause: Some(ChildRuntimePause { request_id, kind }),
                            status: ChildRuntimeStatus::Paused,
                        })
                    }
                    alan_agent_protocol::Event::Error {
                        message,
                        recoverable,
                    } if !recoverable => {
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "error",
                            Some(message.clone()),
                        );
                        let structured_output = parse_child_structured_output(output_text.as_str());
                        ChildEventObservation::Terminal(ObservedChildTerminalEvent {
                            output_text: output_text.clone(),
                            turn_summary: None,
                            structured_output,
                            warnings: warnings.clone(),
                            error_message: Some(message),
                            pause: None,
                            status: ChildRuntimeStatus::Failed,
                        })
                    }
                    alan_agent_protocol::Event::Error { message, .. } => {
                        self.child_run_registry
                            .observe_warning(&self.child_run_id, message.clone());
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "recoverable_error",
                            Some(message.clone()),
                        );
                        push_bounded_child_warning(warnings, message);
                        ChildEventObservation::Progress
                    }
                    alan_agent_protocol::Event::ToolCallStarted { name, .. } => {
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "tool_call_started",
                            Some(format!("tool {name} started")),
                        );
                        ChildEventObservation::Progress
                    }
                    alan_agent_protocol::Event::ToolCallCompleted { name, success, .. } => {
                        let tool = name.unwrap_or_else(|| "<unknown>".to_string());
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "tool_call_completed",
                            Some(format!("tool {tool} completed success={success:?}")),
                        );
                        ChildEventObservation::Progress
                    }
                    alan_agent_protocol::Event::PlanUpdated { explanation, .. } => {
                        self.child_run_registry.observe_progress(
                            &self.child_run_id,
                            "plan_updated",
                            explanation,
                        );
                        ChildEventObservation::Progress
                    }
                    _ => ChildEventObservation::Progress,
                }
            }
            Err(RecvError::Lagged(skipped)) => {
                let message = format!(
                    "Child-agent runtime event stream lagged by {skipped} event(s) before a terminal event could be observed"
                );
                push_bounded_child_warning(warnings, message.clone());
                ChildEventObservation::Terminal(ObservedChildTerminalEvent {
                    output_text: output_text.clone(),
                    turn_summary: None,
                    structured_output: parse_child_structured_output(output_text.as_str()),
                    warnings: warnings.clone(),
                    error_message: Some(message),
                    pause: None,
                    status: ChildRuntimeStatus::Failed,
                })
            }
            Err(RecvError::Closed) => ChildEventObservation::Terminal(ObservedChildTerminalEvent {
                output_text: output_text.clone(),
                turn_summary: None,
                structured_output: parse_child_structured_output(output_text.as_str()),
                warnings: warnings.clone(),
                error_message: Some(
                    "Child-agent runtime stopped before producing a terminal event".to_string(),
                ),
                pause: None,
                status: ChildRuntimeStatus::Failed,
            }),
        }
    }

    async fn terminate_runtime(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let _ = runtime.shutdown().await;
        }
        self.terminate_process_and_reconcile().await;
    }

    async fn finish_runtime_and_process(&mut self, status: &ChildRuntimeStatus) {
        if let Some(runtime) = self.runtime.take() {
            let _ = runtime.shutdown().await;
        }
        let (Some(process_registry), Some(pid)) =
            (self.process_registry.as_ref(), self.process_pid.as_deref())
        else {
            return;
        };
        let Ok(pid) = pid.parse::<u64>() else {
            return;
        };
        process_registry
            .record_exit(
                alan_kernel::Pid(pid),
                child_runtime_process_exit_code(status),
            )
            .await;
        self.reconcile_exited_process().await;
    }

    async fn abort_runtime(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.abort().await;
        }
        self.terminate_process_and_reconcile().await;
    }

    async fn abort_runtime_for_status(&mut self, status: &ChildRuntimeStatus) {
        if let Some(runtime) = self.runtime.take() {
            runtime.abort().await;
        }
        let (Some(process_registry), Some(pid)) =
            (self.process_registry.as_ref(), self.process_pid.as_deref())
        else {
            return;
        };
        let Ok(pid) = pid.parse::<u64>() else {
            return;
        };
        process_registry
            .record_exit(
                alan_kernel::Pid(pid),
                child_runtime_process_exit_code(status),
            )
            .await;
        self.reconcile_exited_process().await;
    }

    async fn terminate_process_and_reconcile(&self) {
        let (Some(environment), Some(pid)) = (
            self.process_environment.as_ref(),
            self.process_pid.as_deref(),
        ) else {
            return;
        };
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
            return;
        }
        let _ = environment
            .write_process_control_for_pid(pid, "cancel")
            .await;
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    async fn reconcile_exited_process(&self) {
        let (Some(environment), Some(pid)) = (
            self.process_environment.as_ref(),
            self.process_pid.as_deref(),
        ) else {
            return;
        };
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    async fn external_process_stop_observed(&self) -> bool {
        let (Some(environment), Some(pid)) = (
            self.process_environment.as_ref(),
            self.process_pid.as_deref(),
        ) else {
            return false;
        };
        matches!(environment.read_process_exit_code(pid).await, Ok(Some(130)))
    }

    fn timed_out_observed_event(&self, message: &str) -> ObservedChildTerminalEvent {
        ObservedChildTerminalEvent {
            output_text: String::new(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: Some(message.to_string()),
            pause: None,
            status: ChildRuntimeStatus::TimedOut,
        }
    }

    fn terminated_observed_event(
        &self,
        request: ChildRunTerminationRequest,
    ) -> ObservedChildTerminalEvent {
        ObservedChildTerminalEvent {
            output_text: String::new(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: Some(format!(
                "Child-agent terminated by {} with {:?} mode: {}",
                request.actor, request.mode, request.reason
            )),
            pause: None,
            status: ChildRuntimeStatus::Terminated,
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
                "Child-agent terminated through external /proc/<pid>/ctl process control"
                    .to_string(),
            ),
            pause: None,
            status: ChildRuntimeStatus::Terminated,
        }
    }
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

fn child_run_status_for_launch_error(error: &anyhow::Error) -> ChildRunStatus {
    if error.chain().any(|cause| {
        cause
            .to_string()
            .contains(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE)
    }) {
        ChildRunStatus::Cancelled
    } else {
        ChildRunStatus::Failed
    }
}

async fn record_child_launch_failure_process(
    procfs: &alan_kernel::ProcFs,
    environment: &super::NamespaceRuntimeEnvironment,
    pid: &str,
    error: &anyhow::Error,
) {
    let Ok(pid) = pid.parse::<u64>() else {
        return;
    };
    let exit_code = match child_run_status_for_launch_error(error) {
        ChildRunStatus::Cancelled => 130,
        _ => 1,
    };
    procfs.record_exit(alan_kernel::Pid(pid), exit_code).await;
    if let Some(context) = environment.process_context() {
        context.agent_root.unbind_process(&pid.to_string()).await;
    }
}

fn yield_kind_label(kind: &YieldKind) -> String {
    match kind {
        YieldKind::Confirmation => "confirmation".to_string(),
        YieldKind::StructuredInput => "structured_input".to_string(),
        YieldKind::Custom(value) => value.clone(),
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

fn build_child_agent_config(parent: &RuntimeLoopState, spec: &SpawnSpec) -> AgentConfig {
    let mut child_agent_config = AgentConfig::from(parent.core_config.clone());
    child_agent_config.runtime_config = parent.runtime_config.clone();

    if !spec.has_handle(SpawnHandle::Memory) {
        child_agent_config.core_config.memory.workspace_dir = None;
    }

    if spec.has_handle(SpawnHandle::ApprovalScope) {
        child_agent_config.runtime_config.governance = parent.runtime_config.governance.clone();
    } else {
        child_agent_config.runtime_config.governance = GovernanceConfig::default();
    }

    if let Some(model) = spec.runtime_overrides.model.as_deref() {
        child_agent_config.set_model_override(model);
    }
    if let Some(effort) = spec.runtime_overrides.model_reasoning_effort {
        child_agent_config.set_model_reasoning_effort_override(Some(effort));
    }
    if let Some(policy_path) = spec.runtime_overrides.policy_path.clone() {
        child_agent_config.runtime_config.governance.policy_path = Some(policy_path);
    }

    child_agent_config
}

#[cfg(test)]
fn build_child_tool_registry(
    parent: &RuntimeLoopState,
    spec: &SpawnSpec,
    child_core_config: &crate::Config,
) -> Result<ToolRegistry> {
    let namespace_plan = build_child_namespace_assembly_plan(parent, spec, child_core_config)?;
    build_child_tool_registry_from_namespace_plan(parent, spec, child_core_config, &namespace_plan)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildNamespaceAssemblyPlan {
    agent_mount: String,
    llm_mount: String,
    llm_connection_name: String,
    srv_mount: String,
    route_mount: String,
    bin_tool_mounts: Vec<String>,
    workspace_root: Option<PathBuf>,
    cwd: Option<PathBuf>,
}

impl ChildNamespaceAssemblyPlan {
    fn bin_tool_names(&self) -> impl Iterator<Item = &str> {
        self.bin_tool_mounts
            .iter()
            .filter_map(|mount| mount.strip_prefix("/bin/"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn clone_exec_spec_for_pid<I, S>(
        &self,
        child_pid: &str,
        executable: impl Into<String>,
        args: I,
    ) -> ExecSpec
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        ExecSpec {
            executable: executable.into(),
            args: args.into_iter().map(Into::into).collect(),
            namespace: Some(self.namespace_manifest_for_pid(child_pid)),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn namespace_manifest_for_pid(&self, _child_pid: &str) -> ExecNamespaceManifest {
        let mut mounts = vec![
            ExecNamespaceMount::new(self.agent_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.llm_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.route_mount.clone(), ExecNamespaceAccess::ReadWrite),
            ExecNamespaceMount::new(self.srv_mount.clone(), ExecNamespaceAccess::ReadOnly),
        ];
        mounts.extend(
            self.bin_tool_mounts
                .iter()
                .cloned()
                .map(|path| ExecNamespaceMount::new(path, ExecNamespaceAccess::ReadOnly)),
        );
        mounts.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.access.cmp(&right.access))
        });
        ExecNamespaceManifest { mounts }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn llm_connection_name(&self) -> Result<String> {
        if self.llm_connection_name.is_empty() || self.llm_connection_name.contains('/') {
            bail!(
                "child namespace plan has invalid llm connection name '{}'",
                self.llm_connection_name
            );
        }
        Ok(self.llm_connection_name.clone())
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone)]
struct ChildNamespaceLaunchHandles {
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: InProcessTransport,
    srv: InProcessTransport,
    route: InProcessTransport,
    bin_tools: Vec<(String, InProcessTransport)>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ChildNamespaceLaunchHandles {
    fn new(
        agent_tree: Arc<alan_agentfs::AgentFs>,
        llm_connection: InProcessTransport,
        srv: InProcessTransport,
        route: InProcessTransport,
    ) -> Self {
        Self {
            agent_tree,
            llm_connection,
            srv,
            route,
            bin_tools: Vec::new(),
        }
    }

    fn with_bin_tool(mut self, mount_path: impl Into<String>, tree: InProcessTransport) -> Self {
        self.bin_tools.push((mount_path.into(), tree));
        self
    }
}

fn child_namespace_launch_handles_from_parent(
    parent: &RuntimeLoopState,
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: Arc<alan_llmfs::LlmFs>,
) -> Result<ChildNamespaceLaunchHandles> {
    let shared_services = parent
        .namespace_environment()
        .shared_services()
        .context("parent namespace missing shared service handles for child-agent launch")?;
    Ok(ChildNamespaceLaunchHandles::new(
        agent_tree,
        InProcessTransport::new(llm_connection),
        shared_services.srv,
        shared_services.route,
    ))
}

#[cfg_attr(not(test), allow(dead_code))]
struct ChildNamespaceRuntimeLaunch {
    pid: String,
    exec: ExecSpec,
    environment: super::NamespaceRuntimeEnvironment,
}

#[cfg_attr(not(test), allow(dead_code))]
async fn spawn_child_namespace_runtime_environment(
    launch_procfs: &alan_kernel::ProcFs,
    runtime_procfs: &alan_kernel::ProcFs,
    plan: &ChildNamespaceAssemblyPlan,
    handles: ChildNamespaceLaunchHandles,
    parent_process_context: Option<super::agent_loop::NamespaceProcessContext>,
    mount_grant_applicator_factory: Option<Arc<dyn super::MountGrantApplicatorFactory>>,
    executable: &str,
) -> Result<ChildNamespaceRuntimeLaunch> {
    validate_child_namespace_launch_handles(plan, &handles)?;

    let (agent_root, parent_pid) = match parent_process_context {
        Some(context) => (context.agent_root, Some(context.pid)),
        None => (
            Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(
                launch_procfs.clone(),
            ))),
            None,
        ),
    };
    let agent_root_tree = InProcessTransport::new(agent_root.clone());
    let spawner_namespace =
        child_spawner_namespace_from_launch_handles(plan, agent_root_tree.clone(), &handles);
    let spawner_procfs = launch_procfs.for_spawner(
        parent_pid,
        spawner_namespace,
        alan_kernel::Credentials::user("root-agent"),
    );
    let clone_fid = next_child_namespace_fid();
    spawner_procfs
        .walk(Fid::ROOT, clone_fid, &["clone".to_string()])
        .await
        .context("walk child /proc/clone")?;
    spawner_procfs
        .open(clone_fid, OpenMode::ReadWrite)
        .await
        .context("open child /proc/clone")?;
    let pid = String::from_utf8(
        spawner_procfs
            .read(clone_fid, 0, 64)
            .await
            .context("read child /proc/clone pid")?,
    )
    .context("child /proc/clone pid is utf8")?;
    let exec = plan.clone_exec_spec_for_pid(&pid, executable, std::iter::empty::<String>());
    let exec_bytes = serde_json::to_vec(&exec).context("serialize child exec spec")?;
    spawner_procfs
        .write(clone_fid, 0, &exec_bytes)
        .await
        .context("write child exec spec to /proc/clone")?;
    spawner_procfs
        .clunk(clone_fid)
        .await
        .context("commit child /proc/clone")?;
    agent_root
        .bind_process(pid.clone(), handles.agent_tree.clone())
        .await;

    let child_pid = alan_kernel::Pid(
        pid.parse::<u64>()
            .with_context(|| format!("parse child pid '{pid}'"))?,
    );
    let child_namespace =
        child_runtime_namespace_from_launch_handles(plan, agent_root_tree, &handles);
    let live_namespace = alan_kernel::LiveNamespace::new(child_namespace);
    runtime_procfs
        .bind_live_namespace(child_pid, live_namespace.clone())
        .await;
    let child_procfs = runtime_procfs.for_live_spawner(
        Some(child_pid),
        live_namespace.clone(),
        alan_kernel::Credentials::user("child-agent"),
    );
    live_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(child_procfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::from_live_namespace(
        live_namespace.clone(),
    )));
    let environment = super::NamespaceRuntimeEnvironment::new(
        root,
        format!("/agent/{pid}"),
        plan.llm_connection_name()?,
    )
    .with_process_context(launch_procfs.clone(), agent_root, child_pid)
    .with_shared_services(handles.srv.clone(), handles.route.clone());
    let environment = if let Some(factory) = mount_grant_applicator_factory {
        environment.with_mount_grant_applicator_factory(factory, live_namespace)
    } else {
        environment
    };

    Ok(ChildNamespaceRuntimeLaunch {
        pid,
        exec,
        environment,
    })
}

#[cfg_attr(not(test), allow(dead_code))]
fn validate_child_namespace_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    handles: &ChildNamespaceLaunchHandles,
) -> Result<()> {
    let expected: BTreeSet<&str> = plan.bin_tool_mounts.iter().map(String::as_str).collect();
    let actual: BTreeSet<&str> = handles
        .bin_tools
        .iter()
        .map(|(mount, _)| mount.as_str())
        .collect();
    if expected == actual {
        return Ok(());
    }

    let missing = expected
        .difference(&actual)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let unexpected = actual
        .difference(&expected)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "child namespace launch handles do not match plan: missing [{}], unexpected [{}]",
        missing,
        unexpected
    );
}

#[cfg_attr(not(test), allow(dead_code))]
fn child_spawner_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg_attr(not(test), allow(dead_code))]
fn child_runtime_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg_attr(not(test), allow(dead_code))]
fn child_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        &plan.agent_mount,
        agent_root_tree,
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        &plan.llm_mount,
        handles.llm_connection.clone(),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        &plan.srv_mount,
        handles.srv.clone(),
        alan_kernel::Access::ReadOnly,
    );
    namespace.mount(
        &plan.route_mount,
        handles.route.clone(),
        alan_kernel::Access::ReadWrite,
    );
    for (mount, tree) in &handles.bin_tools {
        namespace.mount(mount, tree.clone(), alan_kernel::Access::ReadOnly);
    }
    namespace
}

#[cfg_attr(not(test), allow(dead_code))]
fn next_child_namespace_fid() -> Fid {
    Fid(NEXT_CHILD_NAMESPACE_FID.fetch_add(1, Ordering::Relaxed))
}

fn build_child_namespace_assembly_plan(
    parent: &RuntimeLoopState,
    spec: &SpawnSpec,
    child_core_config: &crate::Config,
) -> Result<ChildNamespaceAssemblyPlan> {
    let workspace_root = resolve_child_workspace_root(parent, spec);
    let cwd = spec
        .launch
        .cwd
        .clone()
        .or_else(|| workspace_root.clone())
        .or_else(|| parent.default_tool_cwd());
    let llm_connection = child_core_config
        .connection_profile
        .as_deref()
        .unwrap_or("default");
    let mut plan = ChildNamespaceAssemblyPlan {
        agent_mount: "/agent".to_string(),
        llm_mount: "/mnt/llm".to_string(),
        llm_connection_name: llm_connection.to_string(),
        srv_mount: "/srv".to_string(),
        route_mount: alan_routefs::MOUNT_PATH.to_string(),
        bin_tool_mounts: Vec::new(),
        workspace_root: workspace_root.clone(),
        cwd,
    };

    if !spec.has_handle(SpawnHandle::Workspace) {
        return Ok(plan);
    }

    let selected_tool_names = selected_child_tool_names(parent, spec);
    let normalized_requested_workspace_root =
        workspace_root.as_deref().map(lexically_normalize_path);
    let normalized_parent_workspace_root =
        bound_workspace_root(parent).map(|root| lexically_normalize_path(&root));

    plan.bin_tool_mounts = selected_tool_names
        .into_iter()
        .filter(|tool_name| {
            child_tool_is_mountable(
                parent,
                tool_name,
                normalized_requested_workspace_root.as_ref(),
                normalized_parent_workspace_root.as_ref(),
            )
        })
        .map(|tool_name| format!("/bin/{tool_name}"))
        .collect();
    Ok(plan)
}

fn selected_child_tool_names(parent: &RuntimeLoopState, spec: &SpawnSpec) -> Vec<String> {
    spec.runtime_overrides
        .tool_profile
        .as_ref()
        .map(|tool_profile| tool_profile.allowed_tools.clone())
        .unwrap_or_else(|| parent.static_tool_names())
}

fn child_tool_is_mountable(
    parent: &RuntimeLoopState,
    tool_name: &str,
    normalized_requested_workspace_root: Option<&PathBuf>,
    normalized_parent_workspace_root: Option<&PathBuf>,
) -> bool {
    child_tool_can_share_existing_binding(
        parent,
        tool_name,
        normalized_requested_workspace_root,
        normalized_parent_workspace_root,
    ) || parent.tool_catalog().has_tool_factory(tool_name)
}

fn child_tool_can_share_existing_binding(
    parent: &RuntimeLoopState,
    tool_name: &str,
    normalized_requested_workspace_root: Option<&PathBuf>,
    normalized_parent_workspace_root: Option<&PathBuf>,
) -> bool {
    let Some(tool) = parent.tool_catalog().get(tool_name) else {
        return false;
    };
    tool.locality() == crate::tools::ToolLocality::Global
        || (normalized_parent_workspace_root.is_some()
            && normalized_requested_workspace_root == normalized_parent_workspace_root)
}

fn build_child_tool_registry_from_namespace_plan(
    parent: &RuntimeLoopState,
    spec: &SpawnSpec,
    child_core_config: &crate::Config,
    namespace_plan: &ChildNamespaceAssemblyPlan,
) -> Result<ToolRegistry> {
    let child_config = Arc::new(child_core_config.clone());
    if !spec.has_handle(SpawnHandle::Workspace) {
        return Ok(ToolRegistry::with_config(child_config));
    }

    let mut tools = if let Some(workspace_root) = spec.launch.workspace_root.as_deref() {
        let mut rebound = ToolRegistry::with_config(Arc::clone(&child_config));
        let normalized_requested_workspace_root = lexically_normalize_path(workspace_root);
        let normalized_parent_workspace_root =
            bound_workspace_root(parent).map(|root| lexically_normalize_path(&root));

        for tool_name in namespace_plan.bin_tool_names() {
            if child_tool_can_share_existing_binding(
                parent,
                tool_name,
                Some(&normalized_requested_workspace_root),
                normalized_parent_workspace_root.as_ref(),
            ) && let Some(tool) = parent.tool_catalog().get(tool_name)
            {
                rebound.register_shared(tool);
                continue;
            }

            if let Some(materialized_tool) = parent.tool_catalog().materialize(tool_name) {
                rebound.register_boxed(materialized_tool);
            }
        }
        validate_child_tool_profile_allowlist(
            &rebound,
            spec.runtime_overrides.tool_profile.as_ref(),
            spec.launch.workspace_root.as_deref(),
        )?;
        rebound
    } else if let Some(tool_profile) = spec.runtime_overrides.tool_profile.as_ref() {
        let filtered = parent
            .tool_catalog()
            .catalog_filtered_clone_with_config(namespace_plan.bin_tool_names(), child_config);
        validate_child_tool_profile_allowlist(&filtered, Some(tool_profile), None)?;
        filtered
    } else {
        parent.tool_catalog().clone_with_config(child_config)
    };

    if let Some(cwd) = namespace_plan.cwd.clone() {
        if let Some(workspace_root) = namespace_plan.workspace_root.clone() {
            tools.set_default_workspace_binding(workspace_root, cwd);
        } else {
            tools.set_default_cwd(cwd);
        }
    }
    Ok(tools)
}

fn validate_child_tool_profile_allowlist(
    tools: &ToolRegistry,
    tool_profile: Option<&alan_agent_protocol::SpawnToolProfileOverride>,
    workspace_root: Option<&Path>,
) -> Result<()> {
    let Some(tool_profile) = tool_profile else {
        return Ok(());
    };

    let missing_tools = tools.validate_required_tools(&tool_profile.allowed_tools)?;
    if missing_tools.is_empty() {
        return Ok(());
    }

    if let Some(workspace_root) = workspace_root {
        bail!(
            "Child-agent launch requested tools that cannot be bound for workspace '{}': {}",
            workspace_root.display(),
            missing_tools.join(", ")
        );
    }

    bail!(
        "Child-agent launch requested unavailable tools: {}",
        missing_tools.join(", ")
    );
}

fn resolve_child_workspace_root(parent: &RuntimeLoopState, spec: &SpawnSpec) -> Option<PathBuf> {
    spec.launch.workspace_root.clone().or_else(|| {
        if spec.has_handle(SpawnHandle::Workspace) {
            bound_workspace_root(parent)
        } else {
            None
        }
    })
}

fn resolve_child_workspace_alan_dir(
    spec: &SpawnSpec,
    workspace_root_dir: Option<&Path>,
    memory_dir: Option<&Path>,
) -> Option<PathBuf> {
    if !spec.has_handle(SpawnHandle::Memory) && !preserves_workspace_policy_context(spec) {
        return None;
    }

    workspace_root_dir
        .map(|root| root.join(".alan"))
        .or_else(|| infer_workspace_alan_dir_from_memory_dir(memory_dir))
}

fn preserves_workspace_policy_context(spec: &SpawnSpec) -> bool {
    spec.has_handle(SpawnHandle::ApprovalScope) || spec.runtime_overrides.policy_path.is_some()
}

fn infer_workspace_alan_dir_from_memory_dir(memory_dir: Option<&Path>) -> Option<PathBuf> {
    let memory_dir = memory_dir?;
    if memory_dir.file_name()? != "memory" {
        return None;
    }
    let alan_dir = memory_dir.parent()?;
    if alan_dir.file_name()? == ".alan" {
        return Some(alan_dir.to_path_buf());
    }
    if alan_dir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "runtime")
    {
        let workspace_alan_dir = alan_dir.parent()?.parent()?;
        return (workspace_alan_dir.file_name()? == ".alan")
            .then(|| workspace_alan_dir.to_path_buf());
    }
    None
}

pub(super) fn infer_workspace_root_from_memory_dir(memory_dir: Option<&Path>) -> Option<PathBuf> {
    let alan_dir = infer_workspace_alan_dir_from_memory_dir(memory_dir);
    let alan_dir = alan_dir.as_deref()?;
    (alan_dir.file_name()? == ".alan").then(|| alan_dir.parent().map(Path::to_path_buf))?
}

fn parent_runtime_channel(parent: &RuntimeLoopState) -> crate::InstallChannel {
    parent_runtime_channel_from_memory(parent).unwrap_or_else(crate::InstallChannel::detect_current)
}

fn parent_agent_home_paths(parent: &RuntimeLoopState) -> Option<crate::AlanHomePaths> {
    let channel = parent_runtime_channel_from_memory(parent)?;
    let current_home_paths = crate::AlanHomePaths::detect()?;
    Some(crate::AlanHomePaths::from_home_dir_for_channel(
        &current_home_paths.home_dir,
        channel,
    ))
}

fn parent_runtime_channel_from_memory(parent: &RuntimeLoopState) -> Option<crate::InstallChannel> {
    let memory_dir = parent.core_config.memory.workspace_dir.as_deref()?;
    if let Some(channel_dir) = memory_dir.parent()
        && channel_dir
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "runtime")
        && let Some(channel_id) = channel_dir.file_name().and_then(|name| name.to_str())
        && let Some(channel) = crate::InstallChannel::from_id(channel_id)
    {
        return Some(channel);
    }
    None
}

pub(super) fn bound_workspace_root(state: &RuntimeLoopState) -> Option<PathBuf> {
    state.workspace_root_dir.clone().or_else(|| {
        infer_workspace_root_from_memory_dir(state.core_config.memory.workspace_dir.as_deref())
    })
}

fn build_child_task_text(parent: &RuntimeLoopState, spec: &SpawnSpec) -> String {
    let mut sections = vec![spec.launch.task.trim().to_string()];

    if let Some(metadata) = render_launch_metadata(spec) {
        sections.push(metadata);
    }
    if spec.has_handle(SpawnHandle::ConversationSnapshot)
        && let Some(snapshot) = render_conversation_snapshot(parent)
    {
        sections.push(snapshot);
    }
    if spec.has_handle(SpawnHandle::Plan)
        && let Some(snapshot) = render_plan_snapshot(parent)
    {
        sections.push(snapshot);
    }
    if spec.has_handle(SpawnHandle::ToolResults)
        && let Some(snapshot) = render_tool_results_snapshot(parent)
    {
        sections.push(snapshot);
    }

    sections
        .into_iter()
        .filter(|section| !section.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_launch_metadata(spec: &SpawnSpec) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(cwd) = spec.launch.cwd.as_ref() {
        lines.push(format!("cwd: {}", cwd.display()));
    }
    if let Some(workspace_root) = spec.launch.workspace_root.as_ref() {
        lines.push(format!("workspace_root: {}", workspace_root.display()));
    }
    if let Some(output_dir) = spec.launch.output_dir.as_ref() {
        lines.push(format!("output_dir: {}", output_dir.display()));
    }

    (!lines.is_empty()).then(|| format!("Execution Context\n{}", lines.join("\n")))
}

fn render_conversation_snapshot(parent: &RuntimeLoopState) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(summary) = parent.machine.tape.summary() {
        lines.push("Summary:".to_string());
        lines.push(truncate_chars(summary.trim(), MAX_CHILD_CONVERSATION_CHARS));
    }

    let recent_messages = parent
        .machine
        .tape
        .messages()
        .iter()
        .rev()
        .filter(|message| matches!(message, Message::User { .. } | Message::Assistant { .. }))
        .take(MAX_CHILD_CONVERSATION_MESSAGES)
        .cloned()
        .collect::<Vec<_>>();

    if !recent_messages.is_empty() {
        lines.push("Recent Messages:".to_string());
        for message in recent_messages.into_iter().rev() {
            let role = match &message {
                Message::User { .. } => "user",
                Message::Assistant { .. } => "assistant",
                Message::Tool { .. } => unreachable!("tool messages are filtered out above"),
                Message::System { .. } => "system",
                Message::Context { .. } => "context",
            };
            let text = match &message {
                Message::Assistant { .. } => message.non_thinking_text_content(),
                _ => message.text_content(),
            };
            if !text.trim().is_empty() {
                lines.push(format!(
                    "- {role}: {}",
                    truncate_chars(text.trim(), MAX_CHILD_CONVERSATION_CHARS / 2)
                ));
            }
        }
    }

    (!lines.is_empty()).then(|| format!("Parent Conversation Snapshot\n{}", lines.join("\n")))
}

fn render_plan_snapshot(parent: &RuntimeLoopState) -> Option<String> {
    let plan_snapshot = parent.turn_state.plan_snapshot()?;
    let mut lines = Vec::new();
    if let Some(explanation) = plan_snapshot.explanation.as_deref()
        && !explanation.trim().is_empty()
    {
        lines.push(format!(
            "Explanation: {}",
            truncate_chars(explanation.trim(), MAX_CHILD_PLAN_ITEM_CHARS)
        ));
    }
    for item in plan_snapshot.items.iter().take(MAX_CHILD_PLAN_ITEMS) {
        lines.push(format!(
            "- [{}] {}",
            match item.status {
                alan_agent_protocol::PlanItemStatus::Pending => "pending",
                alan_agent_protocol::PlanItemStatus::InProgress => "in_progress",
                alan_agent_protocol::PlanItemStatus::Completed => "completed",
            },
            truncate_chars(item.content.trim(), MAX_CHILD_PLAN_ITEM_CHARS)
        ));
    }

    (!lines.is_empty()).then(|| format!("Parent Plan Snapshot\n{}", lines.join("\n")))
}

fn render_tool_results_snapshot(parent: &RuntimeLoopState) -> Option<String> {
    let mut lines = Vec::new();
    for message in parent
        .machine
        .tape
        .messages()
        .iter()
        .rev()
        .filter(|message| matches!(message, Message::Tool { .. }))
        .take(MAX_CHILD_TOOL_RESULTS)
    {
        for response in message.tool_responses() {
            let content =
                truncate_chars(response.text_content().trim(), MAX_CHILD_TOOL_RESULT_CHARS);
            if !content.is_empty() {
                lines.push(format!("- {}: {}", response.id, content));
            }
        }
    }
    lines.reverse();
    (!lines.is_empty()).then(|| format!("Parent Tool Results\n{}", lines.join("\n")))
}

fn truncate_chars(text: &str, limit: usize) -> String {
    let truncated: String = text.chars().take(limit).collect();
    if truncated.chars().count() == text.chars().count() {
        truncated
    } else {
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{GenerationRequest, GenerationResponse, StreamChunk, TokenUsage};
    use crate::runtime::{
        ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
        MountGrantApplicatorFactory, RuntimeConfig, RuntimeEnvironment,
    };
    use crate::skills::SkillHostCapabilities;
    use crate::tools::Tool;
    use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
    use alan_kernel::{
        Access as KernelAccess, Credentials as KernelCredentials, Namespace as KernelNamespace,
        ProcFs as KernelProcFs,
    };
    use alan_llm::LlmProvider;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn test_liveness_rx() -> tokio::sync::broadcast::Receiver<RuntimeLivenessEnvelope> {
        tokio::sync::broadcast::channel(8).0.subscribe()
    }

    fn test_startup_metadata(
        process_path: impl Into<String>,
        rollout_path: Option<PathBuf>,
        durable: bool,
    ) -> RuntimeStartupMetadata {
        RuntimeStartupMetadata {
            process_path: process_path.into(),
            agent_path: "/agent/test".to_string(),
            rollout_id: None,
            rollout_path,
            durability: super::super::engine::AgentMachineDurabilityState {
                durable,
                required: false,
            },
            execution_backend: crate::tools::active_backend_name().to_string(),
            request_controls: crate::ResolvedRequestControls::default(),
            warnings: Vec::new(),
        }
    }

    fn namespace_environment_for_parent_test() -> RuntimeEnvironment {
        namespace_environment_for_parent_test_with_route(Arc::new(alan_routefs::RouteFs::new()))
    }

    fn namespace_environment_for_parent_test_with_route(
        routefs: Arc<alan_routefs::RouteFs>,
    ) -> RuntimeEnvironment {
        let root =
            InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(KernelNamespace::new())));
        let namespace =
            crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
                .with_shared_services(memfs_transport(), InProcessTransport::new(routefs));
        RuntimeEnvironment::namespace(namespace)
    }

    #[derive(Clone, Default)]
    struct RecordedRequests(Arc<Mutex<Vec<GenerationRequest>>>);

    #[derive(Clone)]
    struct RecordingProvider {
        requests: RecordedRequests,
        response: GenerationResponse,
        delay: Option<Duration>,
    }

    impl RecordingProvider {
        fn new(requests: RecordedRequests, response: GenerationResponse) -> Self {
            Self {
                requests,
                response,
                delay: None,
            }
        }

        fn with_delay(mut self, delay: Duration) -> Self {
            self.delay = Some(delay);
            self
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for RecordingProvider {
        async fn generate(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            self.requests.0.lock().unwrap().push(request);
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            Ok(self.response.clone())
        }

        async fn chat(&mut self, _system: Option<&str>, user: &str) -> anyhow::Result<String> {
            Ok(format!("chat: {user}"))
        }

        async fn generate_stream(
            &mut self,
            request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.requests.0.lock().unwrap().push(request);
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let _ = tx
                .send(StreamChunk {
                    text: Some(self.response.content.clone()),
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: Some(TokenUsage {
                        prompt_tokens: 1,
                        cached_prompt_tokens: None,
                        completion_tokens: 1,
                        total_tokens: 2,
                        reasoning_tokens: None,
                    }),
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some("stop".to_string()),
                })
                .await;
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "openai_responses"
        }
    }

    struct NamedTestTool {
        name: String,
    }

    impl NamedTestTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    impl Tool for NamedTestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "test tool"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(
            &self,
            _arguments: serde_json::Value,
            _ctx: &crate::tools::ToolContext,
        ) -> crate::tools::ToolResult {
            Box::pin(async { Ok(json!({"ok": true})) })
        }
    }

    #[derive(Debug, Default)]
    struct RecordingMountGrantApplicatorFactory {
        created: Arc<Mutex<usize>>,
    }

    impl RecordingMountGrantApplicatorFactory {
        fn created_count(&self) -> usize {
            *self
                .created
                .lock()
                .expect("created count lock should not be poisoned")
        }
    }

    impl MountGrantApplicatorFactory for RecordingMountGrantApplicatorFactory {
        fn create(
            &self,
            live_namespace: alan_kernel::LiveNamespace,
        ) -> Arc<dyn MountGrantApplicator> {
            *self
                .created
                .lock()
                .expect("created count lock should not be poisoned") += 1;
            Arc::new(RecordingMountGrantApplicator { live_namespace })
        }
    }

    #[derive(Debug)]
    struct RecordingMountGrantApplicator {
        live_namespace: alan_kernel::LiveNamespace,
    }

    impl MountGrantApplicator for RecordingMountGrantApplicator {
        fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> anyhow::Result<()> {
            let access = match grant.access {
                ApprovedMountGrantAccess::ReadOnly => KernelAccess::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => KernelAccess::ReadWrite,
            };
            self.live_namespace
                .mount(&grant.namespace_path, memfs_transport(), access);
            Ok(())
        }
    }

    struct WorkspaceBoundTestTool {
        name: String,
        workspace_root: PathBuf,
    }

    impl WorkspaceBoundTestTool {
        fn new(name: &str, workspace_root: PathBuf) -> Self {
            Self {
                name: name.to_string(),
                workspace_root,
            }
        }
    }

    impl Tool for WorkspaceBoundTestTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "workspace-bound test tool"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": {
                        "type": "string"
                    }
                }
            })
        }

        fn execute(
            &self,
            arguments: serde_json::Value,
            ctx: &crate::tools::ToolContext,
        ) -> crate::tools::ToolResult {
            let workspace_root = self.workspace_root.clone();
            let path = ctx.resolve_path(arguments["path"].as_str().unwrap_or(""));
            Box::pin(async move {
                if !path.starts_with(&workspace_root) {
                    anyhow::bail!(
                        "outside workspace: '{}' not within '{}'",
                        path.display(),
                        workspace_root.display()
                    );
                }

                let content = tokio::fs::read_to_string(&path).await?;
                Ok(json!({
                    "path": path.to_string_lossy(),
                    "content": content
                }))
            })
        }

        fn locality(&self) -> crate::tools::ToolLocality {
            crate::tools::ToolLocality::WorkspaceLocal
        }
    }

    struct MarkerTool {
        name: String,
        marker: String,
        locality: crate::tools::ToolLocality,
    }

    impl MarkerTool {
        fn new(name: &str, marker: &str, locality: crate::tools::ToolLocality) -> Self {
            Self {
                name: name.to_string(),
                marker: marker.to_string(),
                locality,
            }
        }
    }

    impl Tool for MarkerTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "marker test tool"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        fn execute(
            &self,
            _arguments: serde_json::Value,
            _ctx: &crate::tools::ToolContext,
        ) -> crate::tools::ToolResult {
            let marker = self.marker.clone();
            Box::pin(async move { Ok(json!({ "marker": marker })) })
        }

        fn locality(&self) -> crate::tools::ToolLocality {
            self.locality
        }
    }

    fn make_parent_state(
        temp: &TempDir,
        requests: RecordedRequests,
        response: GenerationResponse,
    ) -> RuntimeLoopState {
        make_parent_state_with_capability_view(
            temp,
            requests,
            response,
            crate::skills::ResolvedCapabilityView::default(),
        )
    }

    fn make_parent_state_with_capability_view(
        temp: &TempDir,
        _requests: RecordedRequests,
        _response: GenerationResponse,
        capability_view: crate::skills::ResolvedCapabilityView,
    ) -> RuntimeLoopState {
        let workspace_root = temp.path().join("repo");
        let workspace_alan_dir = workspace_root.join(".alan");
        let launch_root = workspace_root.join(".alan/agents/grader");
        std::fs::create_dir_all(launch_root.join("persona")).unwrap();
        std::fs::create_dir_all(crate::workspace_runtime_rollouts_dir_from_alan_dir(
            &workspace_alan_dir,
            crate::InstallChannel::Stable,
        ))
        .unwrap();
        std::fs::create_dir_all(launch_root.join("skills")).unwrap();
        std::fs::write(launch_root.join("agent.toml"), "tool_repeat_limit = 4\n").unwrap();

        let mut core_config = crate::Config::default();
        core_config.memory.workspace_dir = Some(crate::workspace_runtime_memory_dir_from_alan_dir(
            &workspace_alan_dir,
            crate::InstallChannel::Stable,
        ));
        core_config.openai_responses_model = "gpt-5.4".to_string();
        let mut tools = ToolRegistry::with_config(Arc::new(core_config.clone()));
        tools.set_default_cwd(workspace_root.clone());
        tools.register(NamedTestTool::new("alpha"));
        tools.register(NamedTestTool::new("beta"));

        let mut machine = crate::AgentMachine::new();
        machine.add_user_message("Parent user asks for review");
        machine.add_assistant_message("Parent assistant explains the approach", None);
        machine.add_tool_message("tool_call_1", "alpha", json!({"summary": "tool output"}));

        let mut turn_state = super::super::TurnState::default();
        turn_state.set_plan_snapshot(
            Some("Finish the delegated check".to_string()),
            vec![alan_agent_protocol::PlanItem {
                id: "plan-1".to_string(),
                content: "Inspect the changed files".to_string(),
                status: alan_agent_protocol::PlanItemStatus::InProgress,
            }],
        );

        RuntimeLoopState {
            workspace_id: "parent-workspace".to_string(),
            workspace_root_dir: Some(workspace_root),
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_parent_test(),
            tool_catalog: tools,
            core_config,
            runtime_config: RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            prompt_cache:
                super::super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
                    capability_view,
                    Vec::new(),
                    SkillHostCapabilities::with_tools(["alpha", "beta"]),
                ),
            turn_state,
        }
    }

    fn launch_spec(root_dir: PathBuf) -> SpawnSpec {
        SpawnSpec {
            target: SpawnTarget::ResolvedAgentRoot { root_dir },
            launch: alan_agent_protocol::SpawnLaunchInputs {
                task: "Review the repository changes".to_string(),
                cwd: None,
                workspace_root: None,
                timeout_secs: Some(30),
                output_dir: None,
            },
            handles: Vec::new(),
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        }
    }

    fn capability_plan(
        workspace_root: Option<PathBuf>,
        tools: &[&str],
    ) -> ChildNamespaceAssemblyPlan {
        ChildNamespaceAssemblyPlan {
            agent_mount: "/agent".to_string(),
            llm_mount: "/mnt/llm".to_string(),
            llm_connection_name: "default".to_string(),
            srv_mount: "/srv".to_string(),
            route_mount: alan_routefs::MOUNT_PATH.to_string(),
            bin_tool_mounts: tools.iter().map(|tool| format!("/bin/{tool}")).collect(),
            cwd: workspace_root.clone(),
            workspace_root,
        }
    }

    #[test]
    fn delegated_spawn_boundary_passes_satisfied_task_unchanged() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let workspace_root = PathBuf::from("/tmp/repo");
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.launch.task = "Inspect local files".to_string();
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::WorkspaceRead {
                    path: Some(workspace_root.clone()),
                },
                alan_agent_protocol::DelegatedCapabilityRequirement::LlmConnection,
            ],
        });

        let decision = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(Some(workspace_root), &["read_file"]),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            decision.recovery,
            alan_agent_protocol::DelegatedCapabilityRecovery::Satisfied
        );
        assert_eq!(spec.launch.task, "Inspect local files");
    }

    #[test]
    fn delegated_spawn_boundary_rewrites_narrowed_task_explicitly() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let workspace_root = PathBuf::from("/tmp/repo");
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.launch.task = "Review GitHub issue against local code".to_string();
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::WorkspaceRead {
                    path: Some(workspace_root.clone()),
                },
                alan_agent_protocol::DelegatedCapabilityRequirement::Github,
            ],
        });

        let decision = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(Some(workspace_root), &["read_file"]),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            decision.recovery,
            alan_agent_protocol::DelegatedCapabilityRecovery::Narrowed
        );
        assert!(spec.launch.task.contains("NARROWED DELEGATION SCOPE"));
        assert!(spec.launch.task.contains("Withheld capabilities: github"));
    }

    #[test]
    fn delegated_spawn_boundary_declines_unsatisfied_workspace() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::WorkspaceRead {
                    path: Some(PathBuf::from("/outside/repo")),
                },
            ],
        });

        let error = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(None, &["read_file"]),
        )
        .unwrap_err();
        let rejection = error.downcast_ref::<DelegatedSpawnRejected>().unwrap();

        assert_eq!(
            rejection.decision.recovery,
            alan_agent_protocol::DelegatedCapabilityRecovery::AskUser
        );
        assert_eq!(rejection.decision.unsatisfied.len(), 1);
    }

    fn completed_response(text: &str) -> GenerationResponse {
        GenerationResponse {
            content: text.to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 8,
                cached_prompt_tokens: None,
                completion_tokens: 4,
                total_tokens: 12,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }
    }

    fn capability_view_with_package_child_agent(
        temp: &TempDir,
    ) -> crate::skills::ResolvedCapabilityView {
        let workspace_root = temp.path().join("repo");
        let package_root = workspace_root.join(".alan/agents/default/skills/repo-review");
        std::fs::create_dir_all(package_root.join("agents/reviewer")).unwrap();
        std::fs::write(
            package_root.join("SKILL.md"),
            r#"---
name: Repo Review
description: Review repository changes
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            package_root.join("agents/reviewer/agent.toml"),
            "tool_repeat_limit = 4\n",
        )
        .unwrap();
        crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
            crate::skills::ScopedPackageDir {
                path: workspace_root.join(".alan/agents/default/skills"),
                scope: crate::skills::SkillScope::Repo,
            },
        ])
    }

    fn memfs_transport() -> InProcessTransport {
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new()))
    }

    fn namespace_from_child_plan(plan: &ChildNamespaceAssemblyPlan) -> KernelNamespace {
        let mut namespace = KernelNamespace::new();
        namespace.mount(
            &plan.agent_mount,
            memfs_transport(),
            KernelAccess::ReadWrite,
        );
        namespace.mount(&plan.llm_mount, memfs_transport(), KernelAccess::ReadWrite);
        namespace.mount(&plan.srv_mount, memfs_transport(), KernelAccess::ReadOnly);
        namespace.mount(
            &plan.route_mount,
            memfs_transport(),
            KernelAccess::ReadWrite,
        );
        for mount in &plan.bin_tool_mounts {
            namespace.mount(mount, memfs_transport(), KernelAccess::ReadOnly);
        }
        namespace
    }

    async fn read_proc_path(fs: &KernelProcFs, names: Vec<String>, fid: Fid) -> String {
        fs.walk(Fid::ROOT, fid, &names).await.unwrap();
        fs.open(fid, OpenMode::Read).await.unwrap();
        String::from_utf8(fs.read(fid, 0, 4096).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn spawn_child_runtime_defaults_to_exec_like_non_inheritance() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let spec = launch_spec(root_dir);

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "Child finished cleanly.");
        let recorded = requests.0.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        let request = &recorded[0];
        assert!(request.tools.iter().all(|tool| tool.name != "alpha"));
        assert!(request.tools.iter().all(|tool| tool.name != "beta"));
        let user_text = request
            .messages
            .iter()
            .map(|message| message.content.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(user_text.contains("Review the repository changes"));
        assert!(!user_text.contains("Parent Conversation Snapshot"));
        assert!(!user_text.contains("Parent Plan Snapshot"));
        assert!(!user_text.contains("Parent Tool Results"));
    }

    #[tokio::test]
    async fn spawn_child_runtime_preserves_parent_dev_channel_for_rollouts() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let workspace_alan_dir = parent.workspace_root_dir.as_ref().unwrap().join(".alan");
        parent.core_config.memory.workspace_dir =
            Some(crate::workspace_runtime_memory_dir_from_alan_dir(
                &workspace_alan_dir,
                crate::InstallChannel::Dev,
            ));
        let root_dir = workspace_alan_dir.join("agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Memory];

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        let rollout_path = result.rollout_path.expect("child rollout path");
        let dev_rollouts_dir = crate::workspace_runtime_rollouts_dir_from_alan_dir(
            &workspace_alan_dir,
            crate::InstallChannel::Dev,
        );
        let stable_rollouts_dir = crate::workspace_runtime_rollouts_dir_from_alan_dir(
            &workspace_alan_dir,
            crate::InstallChannel::Stable,
        );
        assert!(rollout_path.starts_with(dev_rollouts_dir));
        assert!(!rollout_path.starts_with(stable_rollouts_dir));
    }

    #[tokio::test]
    async fn spawn_child_runtime_binds_requested_parent_handles() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Bound handles processed.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![
            SpawnHandle::ConversationSnapshot,
            SpawnHandle::Plan,
            SpawnHandle::ToolResults,
        ];

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let recorded = requests.0.lock().unwrap();
        let user_text = recorded
            .iter()
            .flat_map(|request| {
                request
                    .messages
                    .iter()
                    .map(|message| message.content.clone())
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(user_text.contains("Parent Conversation Snapshot"));
        assert!(user_text.contains("Parent Plan Snapshot"));
        assert!(user_text.contains("Parent Tool Results"));
        assert!(user_text.contains("Inspect the changed files"));
        assert!(user_text.contains("tool output"));
    }

    #[tokio::test]
    async fn spawn_child_runtime_rejects_artifact_handle_without_runtime_binding() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Artifacts are not supported.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Artifacts];

        let err = match spawn_child_runtime_with_client_factory(&parent, spec, |_| unreachable!())
            .await
        {
            Ok(_) => panic!("artifact handle should be rejected until artifact routing exists"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("Child-agent launches do not support artifact routing yet")
        );
    }

    #[tokio::test]
    async fn spawn_child_runtime_rejects_output_dir_without_runtime_binding() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Artifacts are not supported.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.launch.output_dir = Some(temp.path().join("repo/out"));

        let err = match spawn_child_runtime_with_client_factory(&parent, spec, |_| unreachable!())
            .await
        {
            Ok(_) => panic!("output_dir should be rejected until artifact routing exists"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("Child-agent launches do not support artifact routing yet")
        );
    }

    #[tokio::test]
    async fn spawn_child_runtime_filters_workspace_tools_with_override() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Only one tool should be visible.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let recorded = requests.0.lock().unwrap();
        let tool_names = recorded[0]
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        assert!(tool_names.contains(&"alpha"));
        assert!(!tool_names.contains(&"beta"));
    }

    #[tokio::test]
    async fn spawn_child_runtime_respects_empty_workspace_tool_override() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("No tools should be visible.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: Vec::new(),
        });

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let recorded = requests.0.lock().unwrap();
        let tool_names = recorded[0]
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        assert!(!tool_names.contains(&"alpha"));
        assert!(!tool_names.contains(&"beta"));
    }

    #[test]
    fn child_namespace_plan_without_workspace_handle_mounts_no_bin_tools() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut child_core_config = parent.core_config.clone();
        child_core_config.connection_profile = Some("child-main".to_string());
        let spec = launch_spec(root_dir);

        let plan = build_child_namespace_assembly_plan(&parent, &spec, &child_core_config).unwrap();

        assert_eq!(plan.agent_mount, "/agent");
        assert_eq!(plan.llm_mount, "/mnt/llm");
        assert_eq!(plan.llm_connection_name().unwrap(), "child-main");
        assert_eq!(plan.srv_mount, "/srv");
        assert_eq!(plan.route_mount, "/mnt/route");
        assert!(plan.bin_tool_mounts.is_empty());
        assert_eq!(plan.workspace_root, None);
    }

    #[test]
    fn child_namespace_plan_mounts_only_allowed_workspace_tools() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });

        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();

        assert_eq!(plan.llm_mount, "/mnt/llm");
        assert_eq!(plan.llm_connection_name().unwrap(), "default");
        assert_eq!(plan.srv_mount, "/srv");
        assert_eq!(plan.route_mount, "/mnt/route");
        assert_eq!(plan.workspace_root, Some(temp.path().join("repo")));
        assert_eq!(plan.cwd, Some(temp.path().join("repo")));
        assert_eq!(plan.bin_tool_mounts, vec!["/bin/alpha"]);
    }

    #[test]
    fn child_clone_exec_spec_declares_agent_and_llm_mounts_for_pid() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut child_core_config = parent.core_config.clone();
        child_core_config.connection_profile = Some("child-main".to_string());
        let spec = launch_spec(root_dir);

        let plan = build_child_namespace_assembly_plan(&parent, &spec, &child_core_config).unwrap();
        let exec = plan.clone_exec_spec_for_pid("42", "/bin/alan-agent", ["--boot"]);

        assert_eq!(
            serde_json::to_value(&exec).unwrap(),
            json!({
                "executable": "/bin/alan-agent",
                "args": ["--boot"],
                "namespace": {
                    "mounts": [
                        {"path": "/agent", "access": "rw"},
                        {"path": "/mnt/llm", "access": "rw"},
                        {"path": "/mnt/route", "access": "rw"},
                        {"path": "/srv", "access": "ro"}
                    ]
                }
            })
        );
        let decoded: ExecSpec = serde_json::from_value(serde_json::to_value(&exec).unwrap())
            .expect("child clone document uses the kernel ExecSpec contract");
        assert_eq!(decoded, exec);
    }

    #[test]
    fn child_clone_exec_spec_declares_only_allowed_bin_mounts() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });

        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let manifest = plan.namespace_manifest_for_pid("99");

        assert_eq!(
            serde_json::to_value(&manifest).unwrap(),
            json!({
                "mounts": [
                    {"path": "/agent", "access": "rw"},
                    {"path": "/bin/alpha", "access": "ro"},
                    {"path": "/mnt/llm", "access": "rw"},
                    {"path": "/mnt/route", "access": "rw"},
                    {"path": "/srv", "access": "ro"}
                ]
            })
        );
    }

    #[tokio::test]
    async fn child_clone_exec_spec_commits_through_proc_clone_with_planned_namespace() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let procfs = KernelProcFs::new();
        let spawner = procfs.for_spawner(
            None,
            namespace_from_child_plan(&plan),
            KernelCredentials::user("alan"),
        );

        spawner
            .walk(Fid::ROOT, Fid(90), &["clone".to_string()])
            .await
            .unwrap();
        spawner.open(Fid(90), OpenMode::ReadWrite).await.unwrap();
        let pid = String::from_utf8(spawner.read(Fid(90), 0, 64).await.unwrap()).unwrap();
        let exec = plan.clone_exec_spec_for_pid(&pid, "/bin/alan-agent", ["--boot"]);
        let exec_bytes = serde_json::to_vec(&exec).unwrap();
        spawner.write(Fid(90), 0, &exec_bytes).await.unwrap();
        spawner.clunk(Fid(90)).await.unwrap();

        let namespace =
            read_proc_path(&procfs, vec![pid.clone(), "namespace".to_string()], Fid(91)).await;
        assert!(
            namespace.lines().any(|line| line == "/agent rw"),
            "agent overlay is mounted at /agent: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/mnt/llm rw"),
            "llm connection is mounted: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/bin/alpha ro"),
            "allowed tool executable is mounted read-only: {namespace:?}"
        );
        assert!(
            !namespace.lines().any(|line| line.contains("<child-pid>")),
            "placeholder is expanded before the process becomes public: {namespace:?}"
        );
    }

    #[tokio::test]
    async fn child_namespace_launch_helper_returns_runtime_environment_for_proc_pid() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let child_tools = build_child_tool_registry_from_namespace_plan(
            &parent,
            &spec,
            &parent.core_config,
            &plan,
        )
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(ChildToolProcessRunner::new(child_tools)));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_bin_tool("/bin/alpha", memfs_transport());

        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            None,
            "/bin/alan-agent",
        )
        .await
        .unwrap();

        assert_eq!(launch.pid, "1");
        assert_eq!(launch.environment.agent_path(), "/agent/1");
        assert_eq!(launch.environment.llm_connection(), "default");
        assert_eq!(
            launch.exec,
            plan.clone_exec_spec_for_pid("1", "/bin/alan-agent", std::iter::empty::<String>())
        );

        assert_eq!(
            read_proc_path(
                &launch_procfs,
                vec![launch.pid.clone(), "status".to_string()],
                Fid(90),
            )
            .await,
            "running\n",
            "child agent process should stay running after launch"
        );
        let namespace = read_proc_path(
            &launch_procfs,
            vec![launch.pid.clone(), "namespace".to_string()],
            Fid(92),
        )
        .await;
        assert!(
            namespace.lines().any(|line| line == "/agent rw"),
            "agent overlay is mounted: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/mnt/llm rw"),
            "llm connection is present: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/mnt/route rw"),
            "routefs tree is present: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/srv ro"),
            "service handle registry is present: {namespace:?}"
        );
        assert!(
            namespace.lines().any(|line| line == "/bin/alpha ro"),
            "allowed tool mount is present: {namespace:?}"
        );

        let child_handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_bin_tool("/bin/alpha", memfs_transport());
        let nested = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            child_handles,
            launch.environment.process_context(),
            None,
            "/bin/alan-agent",
        )
        .await
        .unwrap();
        assert_eq!(nested.pid, "2");
        assert_eq!(
            read_proc_path(
                &launch_procfs,
                vec![nested.pid.clone(), "parent".to_string()],
                Fid(94),
            )
            .await,
            "1"
        );
        let parent_shell = alan_shell::Shell::new(launch.environment.root_transport());
        assert_eq!(
            parent_shell.ls("/agent/1/children").await.unwrap(),
            vec![nested.pid.clone()],
            "delegated Agent Process must be inspectable from the parent AgentFS view"
        );
        record_child_launch_failure_process(
            &launch_procfs,
            &nested.environment,
            &nested.pid,
            &anyhow::anyhow!("simulated child runtime startup failure"),
        )
        .await;
        assert_eq!(
            nested
                .environment
                .read_process_exit_code(&nested.pid)
                .await
                .unwrap(),
            Some(1)
        );
        assert!(
            parent_shell
                .ls("/agent/1/children")
                .await
                .unwrap()
                .is_empty(),
            "failed child launch must leave no running child entry"
        );

        let tool = launch
            .environment
            .run_tool_action("alpha", "/bin/alpha", ["{}"])
            .await
            .unwrap();
        assert_eq!(tool.pid, "3");
        assert_eq!(tool.action_id, "a0");
        assert_eq!(tool.output.trim(), r#"{"ok":true}"#);
        let tool_namespace = read_proc_path(
            &launch_procfs,
            vec![tool.pid.clone(), "namespace".to_string()],
            Fid(93),
        )
        .await;
        assert!(
            tool_namespace.lines().any(|line| line == "/agent rw"),
            "child-spawned processes inherit the agent overlay: {tool_namespace:?}"
        );
        assert!(
            tool_namespace.lines().any(|line| line == "/mnt/llm rw"),
            "child-spawned processes inherit the llm connection: {tool_namespace:?}"
        );
        assert!(
            tool_namespace.lines().any(|line| line == "/mnt/route rw"),
            "child-spawned processes inherit routefs: {tool_namespace:?}"
        );
        assert!(
            tool_namespace.lines().any(|line| line == "/srv ro"),
            "child-spawned processes inherit /srv handles: {tool_namespace:?}"
        );
        assert!(
            tool_namespace.lines().any(|line| line == "/bin/alpha ro"),
            "child-spawned processes inherit mounted tools: {tool_namespace:?}"
        );

        let process_reader = launch.environment.clone();
        let process_pid = launch.pid.clone();
        let (tx, event_rx) = tokio::sync::broadcast::channel(4);
        let submission_id = "completed-child-process".to_string();
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted { summary: None },
        });
        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: Some(launch_procfs),
            process_environment: Some(launch.environment),
            process_pid: Some(process_pid.clone()),
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(
            process_reader
                .read_process_exit_code(&process_pid)
                .await
                .unwrap(),
            Some(0),
            "normal completion must not be rewritten as ctl cancellation (130)"
        );
    }

    #[tokio::test]
    async fn external_proc_ctl_stops_child_runtime_controller() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child should be stopped externally.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let child_tools = build_child_tool_registry_from_namespace_plan(
            &parent,
            &spec,
            &parent.core_config,
            &plan,
        )
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(ChildToolProcessRunner::new(child_tools)));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_bin_tool("/bin/alpha", memfs_transport());
        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            None,
            "/bin/alan-agent",
        )
        .await
        .unwrap();
        let process_pid = launch.pid.clone();
        let process_environment = launch.environment.clone();
        let (event_tx, event_rx) = tokio::sync::broadcast::channel(4);
        let submission_id = "externally-stopped-child".to_string();
        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx,
            liveness_rx: test_liveness_rx(),
            submission_id: submission_id.clone(),
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: Some(launch_procfs),
            process_environment: Some(launch.environment),
            process_pid: Some(process_pid.clone()),
        };

        process_environment
            .write_process_control_for_pid(&process_pid, "cancel")
            .await
            .unwrap();
        event_tx
            .send(RuntimeEventEnvelope {
                submission_id: Some(submission_id),
                event: alan_agent_protocol::Event::TurnCompleted { summary: None },
            })
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), controller.join())
            .await
            .expect("controller must observe external proc cancellation")
            .unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Terminated);
        assert!(
            result
                .error_message
                .as_deref()
                .is_some_and(|message| message.contains("/proc/<pid>/ctl"))
        );
        assert_eq!(
            process_environment
                .read_process_exit_code(&process_pid)
                .await
                .unwrap(),
            Some(130)
        );
    }

    #[tokio::test]
    async fn child_namespace_launch_attaches_mount_grant_applicator_factory() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let child_tools = build_child_tool_registry_from_namespace_plan(
            &parent,
            &spec,
            &parent.core_config,
            &plan,
        )
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(ChildToolProcessRunner::new(child_tools)));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_bin_tool("/bin/alpha", memfs_transport())
        .with_bin_tool("/bin/beta", memfs_transport());
        let factory = Arc::new(RecordingMountGrantApplicatorFactory::default());

        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            Some(factory.clone()),
            "/bin/alan-agent",
        )
        .await
        .unwrap();

        assert_eq!(factory.created_count(), 1);
        assert!(
            launch
                .environment
                .mount_grant_applicator_factory()
                .is_some()
        );
        let applied = launch
            .environment
            .apply_approved_mount_grant(&ApprovedMountGrant::new(
                "/mnt/project",
                PathBuf::from("/unused/by/test/applicator"),
                ApprovedMountGrantAccess::ReadWrite,
                "Need project files",
            ));
        assert!(applied.namespace_applied);
        assert_eq!(applied.namespace_error, None);

        let namespace = read_proc_path(
            &launch_procfs,
            vec![launch.pid.clone(), "namespace".to_string()],
            Fid(94),
        )
        .await;
        assert!(
            namespace.lines().any(|line| line == "/mnt/project rw"),
            "child process namespace should reflect applicator live mounts: {namespace:?}"
        );
    }

    #[tokio::test]
    async fn child_namespace_launch_handles_share_parent_routefs() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let routefs = Arc::new(alan_routefs::RouteFs::new());
        routefs
            .install_rule(
                "10-results",
                alan_routefs::RuleSpec::for_type("result", "review"),
            )
            .await
            .unwrap();
        let mut parent = make_parent_state(&temp, requests, response);
        parent.environment = namespace_environment_for_parent_test_with_route(routefs.clone());

        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::Workspace];
        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();
        let child_tools = build_child_tool_registry_from_namespace_plan(
            &parent,
            &spec,
            &parent.core_config,
            &plan,
        )
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(ChildToolProcessRunner::new(child_tools)));
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            &plan.llm_connection_name().unwrap(),
            Box::new(ChildLlmProvider::new(LlmClient::new(
                RecordingProvider::new(RecordedRequests::default(), completed_response("unused")),
            ))),
        );
        let handles = child_namespace_launch_handles_from_parent(
            &parent,
            Arc::new(alan_agentfs::AgentFs::new()),
            llmfs,
        )
        .unwrap()
        .with_bin_tool("/bin/alpha", memfs_transport())
        .with_bin_tool("/bin/beta", memfs_transport());

        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            None,
            "/bin/alan-agent",
        )
        .await
        .unwrap();

        let child_shell = alan_shell::Shell::new(launch.environment.root_transport());
        let message = serde_json::to_vec(&json!({
            "version": 1,
            "type": "result",
            "content": "child result"
        }))
        .unwrap();
        child_shell
            .write("/mnt/route/send", &message)
            .await
            .unwrap();

        let parent_route_shell = alan_shell::Shell::new(InProcessTransport::new(routefs));
        let routed =
            String::from_utf8(parent_route_shell.cat("/ports/review").await.unwrap()).unwrap();
        assert!(routed.contains(r#""type":"result""#), "{routed}");
        assert!(routed.contains(r#""content":"child result""#), "{routed}");
    }

    #[tokio::test]
    async fn child_tool_runner_rejects_unmounted_tool_executables() {
        let mut child_tools = ToolRegistry::new();
        child_tools.register(MarkerTool::new(
            "alpha",
            "mounted-only",
            crate::tools::ToolLocality::Global,
        ));
        let runner = ChildToolProcessRunner::new(child_tools);
        let invocation = alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(1),
            parent: Some(alan_kernel::Pid(0)),
            credentials: alan_kernel::Credentials::user("child-agent"),
            namespace: alan_kernel::Namespace::new(),
            exec: alan_kernel::ExecSpec {
                executable: "/bin/alpha".to_string(),
                args: vec!["{}".to_string()],
                namespace: None,
            },
        };

        let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;

        assert_eq!(outcome.exit_code, 127);
        assert_eq!(outcome.output, b"executable is not mounted\n");
    }

    #[test]
    fn child_namespace_plan_omits_unbindable_workspace_local_tool_for_other_workspace() {
        let temp = TempDir::new().unwrap();
        let parent_root = temp.path().join("repo");
        let child_root = temp.path().join("other-repo");
        std::fs::create_dir_all(&child_root).unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_cwd(parent_root);
        parent_tools.register(WorkspaceBoundTestTool::new(
            "workspace_read",
            temp.path().join("repo"),
        ));
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(child_root.clone());
        spec.launch.cwd = Some(child_root.clone());

        let plan =
            build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config).unwrap();

        assert_eq!(plan.workspace_root, Some(child_root.clone()));
        assert_eq!(plan.cwd, Some(child_root));
        assert!(plan.bin_tool_mounts.is_empty());
    }

    #[tokio::test]
    async fn build_child_tool_registry_skips_workspace_local_tools_without_catalog_factory() {
        let temp = TempDir::new().unwrap();
        let parent_root = temp.path().join("repo");
        let child_root = temp.path().join("other-repo");
        std::fs::create_dir_all(&child_root).unwrap();
        std::fs::write(child_root.join("target.txt"), "child workspace contents\n").unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_cwd(parent_root.clone());
        parent_tools.register(WorkspaceBoundTestTool::new("workspace_read", parent_root));
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(child_root.clone());
        spec.launch.cwd = Some(child_root.clone());

        let child_tools = build_child_tool_registry(&parent, &spec, &parent.core_config).unwrap();
        assert!(child_tools.get("workspace_read").is_none());
    }

    #[tokio::test]
    async fn build_child_tool_registry_rejects_missing_requested_workspace_tool_without_factory() {
        let temp = TempDir::new().unwrap();
        let parent_root = temp.path().join("repo");
        let child_root = temp.path().join("other-repo");
        std::fs::create_dir_all(&child_root).unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_cwd(parent_root.clone());
        parent_tools.register(WorkspaceBoundTestTool::new("workspace_read", parent_root));
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(child_root.clone());
        spec.launch.cwd = Some(child_root);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["workspace_read".to_string()],
        });

        let err = match build_child_tool_registry(&parent, &spec, &parent.core_config) {
            Ok(_) => panic!("expected missing requested workspace tool to fail"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("requested tools that cannot be bound for workspace")
        );
        assert!(err.to_string().contains("workspace_read"));
    }

    #[tokio::test]
    async fn build_child_tool_registry_materializes_workspace_tools_from_parent_factories() {
        let temp = TempDir::new().unwrap();
        let parent_root = temp.path().join("repo");
        let child_root = temp.path().join("other-repo");
        std::fs::create_dir_all(&child_root).unwrap();
        std::fs::write(child_root.join("target.txt"), "child workspace contents\n").unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_cwd(parent_root);
        let child_root_for_factory = child_root.clone();
        parent_tools.register_tool_factory("workspace_read", move || {
            Box::new(WorkspaceBoundTestTool::new(
                "workspace_read",
                child_root_for_factory.clone(),
            ))
        });
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(child_root.clone());
        spec.launch.cwd = Some(child_root.clone());
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["workspace_read".to_string()],
        });

        let child_tools = build_child_tool_registry(&parent, &spec, &parent.core_config).unwrap();
        let result = child_tools
            .execute("workspace_read", json!({ "path": "target.txt" }))
            .await
            .unwrap();

        assert_eq!(result["content"], json!("child workspace contents\n"));
        assert_eq!(
            result["path"],
            json!(child_root.join("target.txt").to_string_lossy().to_string())
        );
    }

    #[tokio::test]
    async fn build_child_tool_registry_preserves_global_override_before_factory() {
        let temp = TempDir::new().unwrap();
        let parent_root = temp.path().join("repo");
        let child_root = temp.path().join("other-repo");
        std::fs::create_dir_all(&child_root).unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_cwd(parent_root);
        parent_tools.register(MarkerTool::new(
            "override_tool",
            "override",
            crate::tools::ToolLocality::Global,
        ));
        parent_tools.register_tool_factory("override_tool", || {
            Box::new(MarkerTool::new(
                "override_tool",
                "factory",
                crate::tools::ToolLocality::Global,
            ))
        });
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(child_root.clone());
        spec.launch.cwd = Some(child_root);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["override_tool".to_string()],
        });

        let child_tools = build_child_tool_registry(&parent, &spec, &parent.core_config).unwrap();
        let result = child_tools
            .execute("override_tool", json!({}))
            .await
            .unwrap();

        assert_eq!(result["marker"], json!("override"));
    }

    #[tokio::test]
    async fn build_child_tool_registry_preserves_same_workspace_local_override_before_factory() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(&workspace_root).unwrap();

        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let mut parent_tools = ToolRegistry::new();
        parent_tools.set_default_workspace_root(workspace_root.clone());
        parent_tools.register(MarkerTool::new(
            "workspace_override",
            "override",
            crate::tools::ToolLocality::WorkspaceLocal,
        ));
        parent_tools.register_tool_factory("workspace_override", || {
            Box::new(MarkerTool::new(
                "workspace_override",
                "factory",
                crate::tools::ToolLocality::WorkspaceLocal,
            ))
        });
        *parent.tool_catalog_mut_for_test() = parent_tools;

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.launch.workspace_root = Some(workspace_root.clone());
        spec.launch.cwd = Some(workspace_root);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["workspace_override".to_string()],
        });

        let child_tools = build_child_tool_registry(&parent, &spec, &parent.core_config).unwrap();
        let result = child_tools
            .execute("workspace_override", json!({}))
            .await
            .unwrap();

        assert_eq!(result["marker"], json!("override"));
    }

    #[tokio::test]
    async fn build_child_tool_registry_preserves_inherited_sandbox_grants() {
        let temp = TempDir::new().unwrap();
        let approved = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let workspace_root = parent.workspace_root_dir.clone().unwrap();
        {
            let parent_tools = parent.tool_catalog_mut_for_test();
            parent_tools.set_default_workspace_root(workspace_root.clone());
            assert!(parent_tools.add_default_sandbox_writable_root(approved.path().to_path_buf()));
        }

        let mut spec = launch_spec(workspace_root.join(".alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];

        let child_tools = build_child_tool_registry(&parent, &spec, &parent.core_config).unwrap();
        let roots = child_tools.default_sandbox_writable_roots();

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0], workspace_root);
        assert_eq!(roots[1], dunce::canonicalize(approved.path()).unwrap());
    }

    #[tokio::test]
    async fn build_child_tool_registry_rejects_unavailable_requested_tool_profile_entries() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);

        let mut spec = launch_spec(temp.path().join("repo/.alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string(), "missing".to_string()],
        });

        let err = match build_child_tool_registry(&parent, &spec, &parent.core_config) {
            Ok(_) => panic!("expected unavailable requested tool profile entry to fail"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("requested unavailable tools"));
        assert!(err.to_string().contains("missing"));
    }

    #[tokio::test]
    async fn spawn_child_runtime_conversation_snapshot_excludes_tool_outputs_without_handle() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Snapshot captured.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.handles = vec![SpawnHandle::ConversationSnapshot];

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let recorded = requests.0.lock().unwrap();
        let user_text = recorded
            .iter()
            .flat_map(|request| {
                request
                    .messages
                    .iter()
                    .map(|message| message.content.clone())
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(user_text.contains("Parent Conversation Snapshot"));
        assert!(!user_text.contains("tool output"));
    }

    #[tokio::test]
    async fn spawn_child_runtime_uses_effective_launch_root_config_for_llm_setup() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        std::fs::write(
            root_dir.join("agent.toml"),
            r#"
tool_repeat_limit = 9
"#,
        )
        .unwrap();
        let seen_config = Arc::new(Mutex::new(None::<crate::Config>));
        let seen_config_for_factory = seen_config.clone();

        let child =
            spawn_child_runtime_with_client_factory(&parent, launch_spec(root_dir), |config| {
                *seen_config_for_factory.lock().unwrap() = Some(config.clone());
                Ok(LlmClient::new(RecordingProvider::new(
                    requests.clone(),
                    response.clone(),
                )))
            })
            .await
            .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let seen_config = seen_config.lock().unwrap().clone().unwrap();
        assert_eq!(seen_config.effective_model(), "gpt-5.4");
        assert_eq!(seen_config.tool_repeat_limit, 9);
    }

    #[tokio::test]
    async fn spawn_child_runtime_applies_reasoning_effort_override_after_overlay() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        std::fs::write(
            root_dir.join("agent.toml"),
            r#"
model_reasoning_effort = "high"
"#,
        )
        .unwrap();
        let seen_config = Arc::new(Mutex::new(None::<crate::Config>));
        let seen_config_for_factory = seen_config.clone();
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.model_reasoning_effort =
            Some(alan_agent_protocol::ReasoningEffort::Low);

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |config| {
            *seen_config_for_factory.lock().unwrap() = Some(config.clone());
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        let seen_config = seen_config.lock().unwrap().clone().unwrap();
        assert_eq!(
            crate::resolve_runtime_request_controls(
                &seen_config,
                crate::provider_capabilities_for_config(&seen_config),
                crate::RequestControlIntent::default(),
            )
            .unwrap()
            .reasoning_effort(),
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );

        let recorded = requests.0.lock().unwrap();
        assert_eq!(
            recorded[0].reasoning.effort,
            Some(alan_agent_protocol::ReasoningEffort::Low)
        );
    }

    #[test]
    fn child_workspace_alan_dir_requires_memory_or_policy_context() {
        let workspace_root = PathBuf::from("/tmp/repo");
        let memory_dir = PathBuf::from("/tmp/repo/.alan/memory");
        let runtime_memory_dir = PathBuf::from("/tmp/repo/.alan/runtime/stable/memory");
        let mut spec = launch_spec(workspace_root.join(".alan/agents/grader"));

        assert_eq!(
            resolve_child_workspace_alan_dir(
                &spec,
                Some(workspace_root.as_path()),
                Some(memory_dir.as_path()),
            ),
            None
        );

        spec.handles.push(SpawnHandle::ApprovalScope);
        assert_eq!(
            resolve_child_workspace_alan_dir(
                &spec,
                Some(workspace_root.as_path()),
                Some(runtime_memory_dir.as_path()),
            ),
            Some(workspace_root.join(".alan"))
        );

        spec.handles.clear();
        spec.runtime_overrides.policy_path = Some(".alan/agents/default/policy.yaml".to_string());
        assert_eq!(
            resolve_child_workspace_alan_dir(
                &spec,
                Some(workspace_root.as_path()),
                Some(memory_dir.as_path()),
            ),
            Some(workspace_root.join(".alan"))
        );
    }

    #[test]
    fn child_agent_config_requires_memory_handle_for_memory_dir() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");

        let mut approval_spec = launch_spec(root_dir.clone());
        approval_spec.handles = vec![SpawnHandle::ApprovalScope];
        let approval_config = build_child_agent_config(&parent, &approval_spec);
        assert_eq!(approval_config.core_config.memory.workspace_dir, None);

        let mut override_spec = launch_spec(root_dir);
        override_spec.runtime_overrides.policy_path =
            Some(".alan/agents/default/policy.yaml".to_string());
        let override_config = build_child_agent_config(&parent, &override_spec);
        assert_eq!(override_config.core_config.memory.workspace_dir, None);
    }

    #[test]
    fn push_bounded_child_warning_keeps_recent_truncated_warnings() {
        let mut warnings = Vec::new();

        for index in 0..(MAX_OBSERVED_CHILD_WARNINGS + 2) {
            push_bounded_child_warning(
                &mut warnings,
                format!(
                    "warning-{index:03}-{}",
                    "x".repeat(MAX_OBSERVED_CHILD_WARNING_CHARS)
                ),
            );
        }

        assert_eq!(warnings.len(), MAX_OBSERVED_CHILD_WARNINGS);
        assert!(warnings[0].starts_with("warning-002-"));
        assert!(
            warnings
                .iter()
                .all(|warning| warning.chars().count() <= MAX_OBSERVED_CHILD_WARNING_CHARS)
        );
        assert!(warnings.last().unwrap().ends_with("..."));
    }

    #[tokio::test]
    async fn spawn_child_runtime_does_not_bind_memory_dir_for_policy_context_only_launches() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("repo");
        std::fs::create_dir_all(workspace_root.join(".alan/agents/default")).unwrap();
        std::fs::write(
            workspace_root.join(".alan/agents/default/policy.yaml"),
            "version: 1\nrules: []\n",
        )
        .unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
        parent.runtime_config.governance.policy_path =
            Some(".alan/agents/default/policy.yaml".to_string());
        let root_dir = workspace_root.join(".alan/agents/grader");
        std::fs::write(
            root_dir.join("agent.toml"),
            format!(
                "[memory]\nworkspace_dir = \"{}\"\n",
                workspace_root.join(".alan/overlay-memory").display()
            ),
        )
        .unwrap();
        let seen_configs = Arc::new(Mutex::new(Vec::<crate::Config>::new()));
        let seen_configs_for_factory = seen_configs.clone();

        let mut approval_spec = launch_spec(root_dir.clone());
        approval_spec.handles = vec![SpawnHandle::ApprovalScope];
        let child = spawn_child_runtime_with_client_factory(&parent, approval_spec, |config| {
            seen_configs_for_factory
                .lock()
                .unwrap()
                .push(config.clone());
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);

        let mut override_spec = launch_spec(root_dir);
        override_spec.runtime_overrides.policy_path =
            Some(".alan/agents/default/policy.yaml".to_string());
        let child = spawn_child_runtime_with_client_factory(&parent, override_spec, |config| {
            seen_configs_for_factory
                .lock()
                .unwrap()
                .push(config.clone());
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);

        let seen_configs = seen_configs.lock().unwrap();
        assert_eq!(seen_configs.len(), 2);
        assert_eq!(seen_configs[0].memory.workspace_dir, None);
        assert_eq!(seen_configs[1].memory.workspace_dir, None);
    }

    #[test]
    fn child_workspace_root_uses_parent_workspace_instead_of_nested_tool_cwd() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let workspace_root = temp.path().join("repo");
        let nested_cwd = workspace_root.join("nested/src");
        std::fs::create_dir_all(&nested_cwd).unwrap();
        parent
            .tool_catalog_mut_for_test()
            .set_default_cwd(nested_cwd);

        let mut spec = launch_spec(workspace_root.join(".alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];

        assert_eq!(
            resolve_child_workspace_root(&parent, &spec),
            Some(workspace_root)
        );
    }

    #[test]
    fn child_workspace_root_uses_bound_parent_workspace_with_custom_memory_dir() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let mut parent = make_parent_state(&temp, requests, response);
        let workspace_root = temp.path().join("repo");
        parent.core_config.memory.workspace_dir = Some(temp.path().join("custom-memory"));

        let mut spec = launch_spec(workspace_root.join(".alan/agents/grader"));
        spec.handles = vec![SpawnHandle::Workspace];

        assert_eq!(
            resolve_child_workspace_root(&parent, &spec),
            Some(workspace_root)
        );
    }

    #[test]
    fn child_launch_contract_rejects_cwd_outside_workspace_root() {
        let workspace_root = PathBuf::from("/tmp/repo");
        let mut spec = launch_spec(workspace_root.join(".alan/agents/grader"));
        spec.launch.workspace_root = Some(workspace_root);
        spec.launch.cwd = Some(PathBuf::from("/tmp/other-workspace/docs"));

        let err = validate_child_launch_contract(&spec).unwrap_err();
        assert!(
            err.to_string().contains("cwd"),
            "expected cwd validation error, got {err:#}"
        );
    }

    #[test]
    fn child_launch_contract_rejects_relative_launch_paths() {
        let mut spec = launch_spec(PathBuf::from("/tmp/repo/.alan/agents/grader"));
        spec.launch.workspace_root = Some(PathBuf::from("repo"));

        let err = validate_child_launch_contract(&spec).unwrap_err();
        assert!(
            err.to_string().contains("absolute"),
            "expected absolute-path validation error, got {err:#}"
        );

        spec.launch.workspace_root = Some(PathBuf::from("/tmp/repo"));
        spec.launch.cwd = Some(PathBuf::from("docs"));

        let err = validate_child_launch_contract(&spec).unwrap_err();
        assert!(
            err.to_string().contains("absolute"),
            "expected absolute-path validation error, got {err:#}"
        );
    }

    #[tokio::test]
    async fn child_runtime_join_captures_non_empty_final_text_delta() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-123".to_string();
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TextDelta {
                chunk: "final child output".to_string(),
                is_final: true,
            },
        });
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted { summary: None },
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "final child output");
        assert!(result.structured_output.is_none());
    }

    #[tokio::test]
    async fn child_runtime_join_extracts_structured_output_from_json_body() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-json".to_string();
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TextDelta {
                chunk: "{\"status\":\"completed\",\"summary\":\"done\"}".to_string(),
                is_final: true,
            },
        });
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted { summary: None },
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(
            result
                .structured_output
                .as_ref()
                .and_then(|v| v.get("summary")),
            Some(&serde_json::json!("done"))
        );
    }

    #[tokio::test]
    async fn child_runtime_join_backfills_output_from_rollout_without_text_deltas() {
        let rollout = tempfile::NamedTempFile::new().unwrap();
        let answer = "{\"status\":\"completed\",\"summary\":\"done\"}";
        let items = [
            crate::rollout::RolloutItem::AgentMachineMeta(crate::rollout::AgentMachineMeta {
                rollout_id: "rollout-child".to_string(),
                process_path: "/proc/42".to_string(),
                started_at: "2026-04-22T13:08:19Z".to_string(),
                cwd: "/tmp".to_string(),
                model: "gpt-5.4".to_string(),
                reasoning_effort: None,
            }),
            crate::rollout::RolloutItem::Message(crate::rollout::MessageRecord {
                role: "assistant".to_string(),
                content: Some(answer.to_string()),
                tool_name: None,
                message: Some(Message::assistant(answer)),
                timestamp: "2026-04-22T13:08:20Z".to_string(),
            }),
        ];
        let content = items
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            + "\n";
        std::fs::write(rollout.path(), content).unwrap();

        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-rollout".to_string();
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted {
                summary: Some("Task completed".to_string()),
            },
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata(
                "child-machine",
                Some(rollout.path().to_path_buf()),
                true,
            ),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(
            result.output_text,
            "{\"status\":\"completed\",\"summary\":\"done\"}"
        );
        assert_eq!(
            result
                .structured_output
                .as_ref()
                .and_then(|value| value.get("summary")),
            Some(&serde_json::json!("done"))
        );
    }

    #[test]
    fn parse_child_structured_output_reads_last_json_fence() {
        let text = "Notes before\n```json\n{\"status\":\"completed\",\"summary\":\"first\"}\n```\nMore notes\n```json\n{\"status\":\"completed\",\"summary\":\"second\"}\n```";

        let parsed = parse_child_structured_output(text).unwrap();
        assert_eq!(parsed["summary"], serde_json::json!("second"));
    }

    #[test]
    fn extract_latest_assistant_text_from_rollout_reads_nested_text_parts() {
        let contents = concat!(
            "{\"type\":\"message\",\"role\":\"assistant\",\"content\":null,\"message\":{\"parts\":[{\"type\":\"text\",\"text\":\"first\"}]}}\n",
            "{\"type\":\"message\",\"role\":\"assistant\",\"content\":null,\"message\":{\"parts\":[{\"type\":\"text\",\"text\":\"second\"},{\"type\":\"tool_request\",\"id\":\"ignored\"}]}}\n"
        );

        let extracted = extract_latest_assistant_text_from_rollout(contents).unwrap();
        assert_eq!(extracted, "second");
    }

    #[tokio::test]
    async fn child_runtime_join_fails_when_event_stream_lags() {
        let (tx, rx) = tokio::sync::broadcast::channel(1);
        let submission_id = "sub-456".to_string();
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TextDelta {
                chunk: "partial child output".to_string(),
                is_final: false,
            },
        });
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted {
                summary: Some("done".to_string()),
            },
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Failed);
        assert_eq!(
            result.error_message.as_deref(),
            Some(
                "Child-agent runtime event stream lagged by 1 event(s) before a terminal event could be observed"
            )
        );
        assert_eq!(
            result.warnings,
            vec![
                "Child-agent runtime event stream lagged by 1 event(s) before a terminal event could be observed"
                    .to_string()
            ]
        );
    }

    #[tokio::test]
    async fn child_runtime_join_until_cancelled_handles_none_timeout_without_panicking() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-789".to_string();
        let submission_id_for_task = submission_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = tx.send(RuntimeEventEnvelope {
                submission_id: Some(submission_id_for_task),
                event: alan_agent_protocol::Event::TurnCompleted {
                    summary: Some("done".to_string()),
                },
            });
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };
        let cancel = CancellationToken::new();

        let result = controller.join_until_cancelled(&cancel).await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.turn_summary.as_deref(), Some("done"));
    }

    #[tokio::test]
    async fn child_runtime_join_prefers_buffered_terminal_event_over_termination_request() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-terminal-before-termination".to_string();
        let child_run_id = format!("test-child-run-{}", uuid::Uuid::new_v4());
        let child_run_registry = ChildRunRegistry::default();
        child_run_registry.register(ChildRunRecord::new(
            child_run_id.clone(),
            "parent-machine".to_string(),
            "child-machine".to_string(),
            None,
            None,
            None,
        ));
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TextDelta {
                chunk: "finished".to_string(),
                is_final: true,
            },
        });
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::TurnCompleted {
                summary: Some("done".to_string()),
            },
        });
        child_run_registry
            .request_termination(
                "parent-machine",
                &child_run_id,
                "operator",
                ChildRunTerminationMode::Forceful,
                "late stop",
            )
            .unwrap();

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: child_run_id.clone(),
            child_run_registry: child_run_registry.clone(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "finished");
        assert_eq!(
            child_run_registry.get(&child_run_id).unwrap().status,
            ChildRunStatus::Completed
        );
    }

    #[tokio::test]
    async fn child_runtime_join_marks_paused_child_run_terminal_after_shutdown() {
        let (tx, rx) = tokio::sync::broadcast::channel(8);
        let submission_id = "sub-yield".to_string();
        let child_run_id = format!("test-child-run-{}", uuid::Uuid::new_v4());
        let child_run_registry = ChildRunRegistry::default();
        child_run_registry.register(ChildRunRecord::new(
            child_run_id.clone(),
            "parent-machine".to_string(),
            "child-machine".to_string(),
            None,
            None,
            None,
        ));
        let _ = tx.send(RuntimeEventEnvelope {
            submission_id: Some(submission_id.clone()),
            event: alan_agent_protocol::Event::Yield {
                request_id: "yield-1".to_string(),
                kind: YieldKind::Confirmation,
                payload: serde_json::json!({}),
            },
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx: test_liveness_rx(),
            submission_id,
            child_run_id: child_run_id.clone(),
            child_run_registry: child_run_registry.clone(),
            timeout: None,
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Paused);
        let child_run = child_run_registry.get(&child_run_id).unwrap();
        assert_eq!(child_run.status, ChildRunStatus::Failed);
        assert!(child_run.status.is_terminal());
    }

    #[tokio::test]
    async fn cancel_child_runtime_returns_cancelled_status() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("This should not finish before cancellation.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let spec = launch_spec(root_dir);

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(
                RecordingProvider::new(requests.clone(), response.clone())
                    .with_delay(Duration::from_secs(5)),
            ))
        })
        .await
        .unwrap();
        let result = child.cancel().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Cancelled);
    }

    #[tokio::test]
    async fn child_runtime_join_until_cancelled_returns_cancelled_status() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("This should not finish before cancellation.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let spec = launch_spec(root_dir);

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(
                RecordingProvider::new(requests.clone(), response.clone())
                    .with_delay(Duration::from_secs(5)),
            ))
        })
        .await
        .unwrap();

        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel_for_task.cancel();
        });

        let result = child.join_until_cancelled(&cancel).await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Cancelled);
    }

    #[tokio::test]
    async fn child_runtime_join_keeps_running_while_heartbeat_is_fresh() {
        let (tx, rx) = tokio::sync::broadcast::channel(16);
        let (liveness_tx, liveness_rx) = tokio::sync::broadcast::channel(16);
        let submission_id = "sub-heartbeat".to_string();
        let submission_id_for_task = submission_id.clone();
        let liveness_submission_id_for_task = submission_id.clone();
        tokio::spawn(async move {
            let _ = liveness_tx.send(RuntimeLivenessEnvelope {
                submission_id: Some(liveness_submission_id_for_task.clone()),
                status: Some("still running".to_string()),
            });
            for _ in 0..4 {
                tokio::time::sleep(Duration::from_millis(35)).await;
                let _ = liveness_tx.send(RuntimeLivenessEnvelope {
                    submission_id: Some(liveness_submission_id_for_task.clone()),
                    status: Some("still running".to_string()),
                });
            }
            let _ = tx.send(RuntimeEventEnvelope {
                submission_id: Some(submission_id_for_task.clone()),
                event: alan_agent_protocol::Event::TextDelta {
                    chunk: "finished after heartbeat".to_string(),
                    is_final: true,
                },
            });
            let _ = tx.send(RuntimeEventEnvelope {
                submission_id: Some(submission_id_for_task),
                event: alan_agent_protocol::Event::TurnCompleted { summary: None },
            });
        });

        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            event_rx: rx,
            liveness_rx,
            submission_id,
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: Some(Duration::from_millis(80)),
            process_registry: None,
            process_environment: None,
            process_pid: None,
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "finished after heartbeat");
    }

    #[tokio::test]
    async fn spawn_child_runtime_cancellable_aborts_pre_cancelled_launch() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("This should never run.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let spec = launch_spec(root_dir);
        let cancel = CancellationToken::new();
        cancel.cancel();

        let err = match spawn_child_runtime_cancellable(&parent, spec, &cancel).await {
            Ok(_) => {
                panic!("pre-cancelled launch should abort before returning a child controller")
            }
            Err(err) => err,
        };

        assert!(err.to_string().contains("Child-agent launch cancelled"));
    }

    #[test]
    fn child_run_status_for_launch_error_maps_cancelled_launches_to_cancelled() {
        let cancelled = anyhow::anyhow!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
        let failed = anyhow::anyhow!("Failed to submit initial child-agent turn");

        assert_eq!(
            child_run_status_for_launch_error(&cancelled),
            ChildRunStatus::Cancelled
        );
        assert_eq!(
            child_run_status_for_launch_error(&failed),
            ChildRunStatus::Failed
        );
    }

    #[tokio::test]
    async fn child_runtime_join_returns_promptly_after_timeout() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("This should not finish before timeout.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("repo/.alan/agents/grader");
        let mut spec = launch_spec(root_dir);
        spec.launch.timeout_secs = Some(1);

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(
                RecordingProvider::new(requests.clone(), response.clone())
                    .with_delay(Duration::from_secs(30)),
            ))
        })
        .await
        .unwrap();
        let process_environment = child.process_environment.clone().unwrap();
        let process_pid = child.process_pid.clone().unwrap();

        let started_at = std::time::Instant::now();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::TimedOut);
        assert_eq!(
            process_environment
                .read_process_exit_code(&process_pid)
                .await
                .unwrap(),
            Some(124)
        );
        assert!(
            started_at.elapsed() < Duration::from_secs(8),
            "timed-out child join should abort promptly instead of waiting for graceful shutdown"
        );
    }

    #[tokio::test]
    async fn spawn_child_runtime_resolves_package_child_agent_target() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Package child target resolved.");
        let capability_view = capability_view_with_package_child_agent(&temp);
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            capability_view,
        );
        let spec = SpawnSpec {
            target: SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            },
            launch: alan_agent_protocol::SpawnLaunchInputs {
                task: "Review the repository changes".to_string(),
                workspace_root: Some(temp.path().join("repo")),
                timeout_secs: Some(30),
                ..alan_agent_protocol::SpawnLaunchInputs::default()
            },
            handles: vec![SpawnHandle::Workspace],
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        };

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "Package child target resolved.");
    }

    #[tokio::test]
    async fn spawn_child_runtime_resolves_package_child_agent_target_from_refreshed_view() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Package child target resolved after refresh.");
        let workspace_root = temp.path().join("repo");
        let package_root = workspace_root.join(".alan/agents/default/skills/repo-review");
        std::fs::create_dir_all(&package_root).unwrap();
        std::fs::write(
            package_root.join("SKILL.md"),
            r#"---
name: Repo Review
description: Review repository changes
---

Body
"#,
        )
        .unwrap();

        let capability_view = crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
            crate::skills::ScopedPackageDir {
                path: workspace_root.join(".alan/agents/default/skills"),
                scope: crate::skills::SkillScope::Repo,
            },
        ]);
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            capability_view,
        );

        std::fs::create_dir_all(package_root.join("agents/reviewer")).unwrap();
        std::fs::write(
            package_root.join("agents/reviewer/agent.toml"),
            "tool_repeat_limit = 4\n",
        )
        .unwrap();

        let spec = SpawnSpec {
            target: SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            },
            launch: alan_agent_protocol::SpawnLaunchInputs {
                task: "Review the repository changes".to_string(),
                workspace_root: Some(workspace_root),
                timeout_secs: Some(30),
                ..alan_agent_protocol::SpawnLaunchInputs::default()
            },
            handles: vec![SpawnHandle::Workspace],
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        };

        let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(RecordingProvider::new(
                requests.clone(),
                response.clone(),
            )))
        })
        .await
        .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(
            result.output_text,
            "Package child target resolved after refresh."
        );
    }
}
