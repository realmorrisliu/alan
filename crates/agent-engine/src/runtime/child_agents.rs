use super::agent_loop::RuntimeLoopState;
use super::child_runs::{
    ChildRunRecord, ChildRunRegistry, ChildRunStatus, ChildRunTerminationMode,
    ChildRunTerminationRequest,
};
use super::delegation_capabilities::{
    DelegatedSpawnRejected, evaluate_delegated_namespace, namespace_summary_from_bindings,
};
use super::engine::{
    AgentConfig, AgentProcessConfig, RuntimeController, RuntimeStartupMetadata,
    effective_core_config_for_runtime, runtime_host_capabilities_for_tools,
    spawn_with_namespace_environment,
};
#[cfg(test)]
use crate::llm::LlmClient;
use crate::tape::{ContentPart, Message};
use alan_agent_protocol::{
    DelegatedCapabilityDecision, DelegatedCapabilityRecovery, GovernanceConfig, Op, SpawnHandle,
    SpawnSpec, SpawnTarget, Submission, YieldKind,
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec};
#[cfg(test)]
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
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
const MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL: Duration = Duration::from_millis(250);
static NEXT_CHILD_NAMESPACE_FID: AtomicU64 = AtomicU64::new(80_000);

#[cfg(test)]
struct ChildLlmProvider {
    client: LlmClient,
}

#[cfg(test)]
impl ChildLlmProvider {
    fn new(client: LlmClient) -> Self {
        Self { client }
    }
}

#[cfg(test)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChildFileObservation {
    process_status: Option<alan_kernel::Status>,
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
    child_run_id: String,
    child_run_registry: ChildRunRegistry,
    timeout: Option<Duration>,
    process_registry: alan_kernel::ProcFs,
    process_environment: super::NamespaceRuntimeEnvironment,
    process_pid: String,
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
    #[cfg(test)]
    {
        return spawn_child_runtime_inner(parent, spec, None, cancel).await;
    }
    #[cfg(not(test))]
    {
        spawn_child_runtime_inner(parent, spec, cancel).await
    }
}

#[cfg(test)]
async fn spawn_child_runtime_with_client_factory<F>(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    llm_client_factory: F,
) -> Result<ChildRuntimeController>
where
    F: FnOnce(&crate::Config) -> Result<LlmClient> + Send,
{
    spawn_child_runtime_inner(parent, spec, Some(Box::new(llm_client_factory)), None).await
}

#[cfg(test)]
type TestChildLlmClientFactory<'a> =
    Box<dyn FnOnce(&crate::Config) -> Result<LlmClient> + Send + 'a>;

async fn spawn_child_runtime_inner(
    parent: &RuntimeLoopState,
    mut spec: SpawnSpec,
    #[cfg(test)] llm_client_factory: Option<TestChildLlmClientFactory<'_>>,
    cancel: Option<&CancellationToken>,
) -> Result<ChildRuntimeController> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        bail!(CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE);
    }

    let child_cwd = validate_child_launch_contract(&spec)?;
    let launch_root_dir = resolve_launch_root_dir(parent, &spec.target)?;
    let child_agent_config = build_child_agent_config(parent, &spec);
    let parent_launch_context = parent
        .namespace_environment()
        .launch_context()
        .cloned()
        .unwrap_or_else(crate::ProcessLaunchContext::root);
    let launch_context = build_child_launch_context(
        &parent_launch_context,
        &spec,
        child_cwd,
        launch_root_dir.as_ref(),
    )?;

    let mut child_config = AgentProcessConfig {
        agent_config: child_agent_config.clone(),
        // Child launches should still resolve their target/root overlays. Using the
        // default source keeps launch-root agent.toml in play instead of treating the
        // parent's effective config as a terminal env override.
        core_config_source: crate::ConfigSourceKind::Default,
        launch_context,
        store_bindings: parent.runtime_config.store_bindings.clone(),
        memory_store_backing: spec
            .has_handle(SpawnHandle::Memory)
            .then(|| parent.runtime_config.memory_store_backing.clone())
            .flatten(),
        recovery_rollout_path: None,
        mount_grant_applicator_factory: parent
            .namespace_environment()
            .mount_grant_applicator_factory(),
    };
    let resolved_child_definition = crate::ResolvedAgentDefinition::from_launch_context(
        &child_config.launch_context,
        &child_config
            .agent_config
            .core_config
            .resolved_skill_overrides(),
        child_config.core_config_source,
    )
    .context("Failed to resolve child-agent definition")?;
    let mut resolved_child_agent_config = child_agent_config.clone();
    if let Some(content) = resolved_child_definition.config_content.as_deref() {
        let source = launch_root_dir
            .as_ref()
            .map(|root| root.root_dir.join("agent.toml"))
            .unwrap_or_else(|| PathBuf::from("/agent-definition/agent.toml"));
        resolved_child_agent_config = resolved_child_agent_config
            .with_definition_overlay_content(content, &source)
            .context("Failed to resolve effective child-agent config")?;
    } else if let Some(config_path) = resolved_child_definition.config_path.as_ref() {
        resolved_child_agent_config = resolved_child_agent_config
            .with_definition_overlays(std::slice::from_ref(config_path))
            .context("Failed to resolve effective child-agent config")?;
    }
    if spec.has_handle(SpawnHandle::Memory) {
        resolved_child_agent_config.core_config.memory.store_dir =
            parent.core_config.memory.store_dir.clone();
    } else {
        resolved_child_agent_config.core_config.memory.store_dir = None;
    }
    child_config.agent_config = resolved_child_agent_config;
    child_config.core_config_source = crate::ConfigSourceKind::EnvOverride;
    let effective_child_core_config = effective_core_config_for_runtime(&child_config)
        .context("Failed to resolve effective child-agent runtime config")?;
    let child_namespace_plan = build_child_namespace_assembly_plan(
        parent,
        &spec,
        &effective_child_core_config,
        child_config.launch_context.clone(),
    )
    .await
    .context("Failed to assemble child-agent namespace plan")?;
    let child_connection = child_namespace_plan.llm_connection_name()?;
    ensure_child_connection_is_passed(parent, &child_connection)?;
    let delegation_capability_decision =
        evaluate_delegated_launch_capabilities(parent, &mut spec, &child_namespace_plan).await?;
    #[cfg(test)]
    let test_llm = if let Some(factory) = llm_client_factory {
        let client = factory(&effective_child_core_config)
            .context("Failed to create test child-agent LLM client")?;
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            &child_namespace_plan.llm_connection_name()?,
            Box::new(ChildLlmProvider::new(client)),
        );
        Some(InProcessTransport::new(llmfs))
    } else {
        None
    };
    let parent_process_context = parent.namespace_environment().process_context();
    let launch_procfs = parent_process_context
        .as_ref()
        .map(|context| context.launch_procfs.clone())
        .unwrap_or_default();
    let tool_runner = parent_process_context
        .as_ref()
        .map(|context| context.tool_runner.clone())
        .unwrap_or_else(|| {
            crate::tools::ToolProcessRunner::empty(Arc::new(effective_child_core_config.clone()))
        });
    let runtime_procfs = launch_procfs
        .clone()
        .with_runner(Arc::new(tool_runner.clone()));
    let child_tool_binding = child_namespace_plan.runtime_execution_binding(
        child_config
            .store_bindings
            .as_ref()
            .map(|stores| stores.tmp.clone()),
    )?;
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let shared_llm = parent
        .namespace_environment()
        .shared_services()
        .context("parent namespace missing callable Connection service for child-agent launch")?
        .llm;
    #[cfg(test)]
    let shared_llm = test_llm.unwrap_or(shared_llm);
    let mut handles = child_namespace_launch_handles_from_parent(parent, agentfs, shared_llm)
        .context("Failed to assemble child-agent shared namespace handles")?;
    for manifest in &child_namespace_plan.tool_packages {
        let name = &manifest.name;
        let manifest_fs = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            serde_json::to_vec(&manifest)?,
        ));
        handles = handles.with_tool_package(
            format!("/bin/{name}"),
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            format!("/lib/exec/{name}"),
            InProcessTransport::new(manifest_fs),
        );
    }
    let namespace_launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &child_namespace_plan,
        handles,
        parent_process_context,
        tool_runner,
        child_tool_binding,
        child_config.mount_grant_applicator_factory.clone(),
        "/bin/alan-agent",
    )
    .await
    .context("Failed to spawn child-agent process namespace")?;
    let child_process_pid = namespace_launch.pid.clone();
    let process_context = namespace_launch
        .environment
        .process_context()
        .expect("child namespace launch installs process context");
    let child_agent_root = process_context.agent_root.clone();
    let child_process_environment = child_observation_environment(
        &runtime_procfs,
        child_agent_root.clone(),
        &child_process_pid,
        &child_namespace_plan,
    )
    .await?;
    let generation_capabilities =
        crate::provider_capabilities_for_config(&effective_child_core_config);
    let host_capabilities = runtime_host_capabilities_for_tools(
        child_namespace_plan
            .tool_packages
            .iter()
            .map(|manifest| manifest.name.clone()),
    );
    let runtime = match spawn_with_namespace_environment(
        child_config,
        namespace_launch.environment,
        host_capabilities,
        generation_capabilities,
    )
    .context("Failed to spawn child-agent namespace runtime")
    {
        Ok(runtime) => runtime,
        Err(err) => {
            record_child_launch_failure_process(
                &launch_procfs,
                &child_agent_root,
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
                &child_agent_root,
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
        Some(startup_metadata.agent_path.clone()),
        Some(format!("{:?}", spec.target)),
    );
    if let Some(decision) = delegation_capability_decision {
        child_run_record = child_run_record.with_delegation_capability_decision(decision);
    }
    child_run_registry.register(child_run_record);
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
                &child_agent_root,
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
        child_run_id,
        child_run_registry,
        timeout: spec.launch.timeout_secs.map(Duration::from_secs),
        process_registry: launch_procfs,
        process_environment: child_process_environment,
        process_pid: child_process_pid,
    })
}

fn ensure_child_connection_is_passed(parent: &RuntimeLoopState, requested: &str) -> Result<()> {
    let passed = parent.namespace_environment().llm_connection();
    if requested != passed {
        bail!(
            "Child-agent Connection '{requested}' was not passed by the parent Process; available Connection is '{passed}'."
        );
    }
    Ok(())
}

fn build_child_launch_context(
    parent: &crate::ProcessLaunchContext,
    spec: &SpawnSpec,
    child_cwd: Option<String>,
    launch_root_dir: Option<&ResolvedLaunchRoot>,
) -> Result<crate::ProcessLaunchContext> {
    let memory_descriptor = parent.descriptor(crate::MEMORY_STORE_DESCRIPTOR).cloned();
    let parent_definition_path = parent
        .descriptor(crate::AGENT_DEFINITION_DESCRIPTOR)
        .map(|descriptor| descriptor.path.clone());
    let mut launch_context = parent.child();
    launch_context.descriptors.clear();

    if !spec.has_handle(SpawnHandle::HostMounts) {
        let inherited_mounts = std::mem::take(&mut launch_context.host_mounts);
        if let Some(cwd) = child_cwd.as_deref()
            && inherited_mounts
                .iter()
                .any(|grant| grant.resolve_host_path(cwd).is_some())
        {
            bail!("Child-agent launch cwd '{cwd}' requires the explicit host_mounts handle.");
        }
        if child_cwd.is_none()
            && inherited_mounts
                .iter()
                .any(|grant| grant.resolve_host_path(&launch_context.cwd).is_some())
        {
            launch_context.cwd = "/".to_string();
        }
        for grant in inherited_mounts {
            if parent_definition_path.as_deref() == Some(&grant.namespace_path) {
                launch_context.host_mounts.push(grant);
                continue;
            }
            launch_context.namespace.unmount(&grant.namespace_path);
        }
    }

    if let Some(cwd) = child_cwd {
        launch_context.cwd = cwd;
    }
    if spec.has_handle(SpawnHandle::Memory) {
        if let Some(descriptor) = memory_descriptor {
            launch_context
                .descriptors
                .insert(crate::MEMORY_STORE_DESCRIPTOR.to_string(), descriptor);
        }
    } else {
        launch_context.namespace.unmount("/memory");
    }

    if let Some(ResolvedLaunchRoot {
        root_dir,
        file_tree: Some(file_tree),
    }) = launch_root_dir
    {
        let descriptor_path = root_dir
            .to_str()
            .context("package child-agent descriptor path is not UTF-8")?;
        launch_context.descriptors.insert(
            crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            crate::ProcessDescriptor::with_file_tree(descriptor_path, file_tree.clone())?,
        );
    } else if let Some(ResolvedLaunchRoot { root_dir, .. }) = launch_root_dir {
        let source_path = parent
            .namespace_path(root_dir)
            .filter(|path| !parent.namespace.union_at(path).is_empty());
        if source_path.as_deref() != Some("/agent-definition")
            && let Some(source_path) = source_path
        {
            launch_context.namespace.unmount("/agent-definition");
            launch_context
                .namespace
                .bind("/agent-definition", &source_path);
        }
        launch_context.host_mounts.retain(|grant| {
            grant.namespace_path != "/agent-definition"
                && parent_definition_path.as_deref() != Some(&grant.namespace_path)
        });
        launch_context = launch_context
            .with_host_mount(crate::HostMountGrant::new(
                "/agent-definition",
                root_dir,
                alan_kernel::Access::ReadOnly,
            )?)
            .with_descriptor(
                crate::AGENT_DEFINITION_DESCRIPTOR,
                crate::ProcessDescriptor::new("/agent-definition")?,
            );
    }
    Ok(launch_context)
}

async fn evaluate_delegated_launch_capabilities(
    parent: &RuntimeLoopState,
    spec: &mut SpawnSpec,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<Option<DelegatedCapabilityDecision>> {
    let Some(context) = spec.delegated.as_ref() else {
        return Ok(None);
    };
    let requirements = context.requirements.clone();
    let child_namespace = namespace_summary_from_child_plan(plan);
    let parent_namespace = namespace_summary_from_parent(parent).await?;
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
    let mut described = plan.launch_context.namespace.describe();
    described.extend([
        (plan.agent_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.llm_mount.clone(), alan_kernel::Access::ReadWrite),
        (plan.srv_mount.clone(), alan_kernel::Access::ReadOnly),
        (plan.route_mount.clone(), alan_kernel::Access::ReadWrite),
    ]);
    namespace_summary_from_bindings(
        described.iter().map(|(path, _)| path.clone()).collect(),
        described
            .iter()
            .filter(|(_, access)| *access == alan_kernel::Access::ReadWrite)
            .map(|(path, _)| path.clone())
            .collect(),
        plan.bin_tool_mounts.clone(),
        plan.cwd.clone(),
        Some(plan.llm_connection_name.clone()),
    )
}

async fn namespace_summary_from_parent(
    parent: &RuntimeLoopState,
) -> Result<alan_agent_protocol::DelegatedNamespaceSummary> {
    let mut described = parent
        .namespace_environment()
        .launch_context()
        .map(|context| context.namespace.describe())
        .unwrap_or_default();
    described.extend([
        ("/agent".to_string(), alan_kernel::Access::ReadWrite),
        ("/mnt/llm".to_string(), alan_kernel::Access::ReadWrite),
        ("/srv".to_string(), alan_kernel::Access::ReadOnly),
        (
            alan_routefs::MOUNT_PATH.to_string(),
            alan_kernel::Access::ReadWrite,
        ),
    ]);
    Ok(namespace_summary_from_bindings(
        described.iter().map(|(path, _)| path.clone()).collect(),
        described
            .iter()
            .filter(|(_, access)| *access == alan_kernel::Access::ReadWrite)
            .map(|(path, _)| path.clone())
            .collect(),
        parent
            .static_tool_names()
            .await?
            .into_iter()
            .map(|tool| format!("/bin/{tool}"))
            .collect(),
        parent
            .namespace_environment()
            .launch_context()
            .map(|context| PathBuf::from(&context.cwd)),
        Some(
            parent
                .core_config
                .connection_profile
                .clone()
                .unwrap_or_else(|| "default".to_string()),
        ),
    ))
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

fn validate_child_launch_contract(spec: &SpawnSpec) -> Result<Option<String>> {
    if spec.has_handle(SpawnHandle::Artifacts) || spec.launch.output_dir.is_some() {
        bail!(
            "Child-agent launches do not support artifact routing yet; omit SpawnHandle::Artifacts and launch.output_dir."
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

    let cwd = spec
        .launch
        .cwd
        .as_deref()
        .map(|cwd| {
            let cwd = cwd.to_str().with_context(|| {
                format!(
                    "Child-agent launch cwd '{}' must be valid Unicode.",
                    cwd.display()
                )
            })?;
            crate::process_launch::normalize_namespace_path(cwd)
                .with_context(|| format!("Invalid child-agent launch cwd '{}'.", cwd))
        })
        .transpose()?;

    Ok(cwd)
}

fn resolve_launch_root_dir(
    parent: &RuntimeLoopState,
    target: &SpawnTarget,
) -> Result<Option<ResolvedLaunchRoot>> {
    match target {
        SpawnTarget::DefinitionDescriptor { descriptor } => {
            let descriptor = parent
                .namespace_environment()
                .launch_context()
                .and_then(|context| context.descriptor(descriptor))
                .with_context(|| format!("parent Process has no `{descriptor}` descriptor"))?;
            let root = if descriptor.file_tree.is_some() {
                PathBuf::from(&descriptor.path)
            } else {
                parent
                    .namespace_environment()
                    .launch_context()
                    .and_then(|context| context.host_path(&descriptor.path))
                    .with_context(|| {
                        format!(
                            "Agent Definition descriptor {} has no explicit Host Mount backing",
                            descriptor.path
                        )
                    })?
            };
            Ok(Some(ResolvedLaunchRoot {
                root_dir: root,
                file_tree: descriptor.file_tree.clone(),
            }))
        }
        SpawnTarget::PackageChildAgent { .. } => {
            let export = parent
                .prompt_cache
                .capability_view()
                .map(crate::skills::ResolvedCapabilityView::refresh)
                .and_then(|view| view.resolve_child_agent_export(target).cloned())
                .ok_or_else(|| anyhow::anyhow!("Unknown package child-agent target: {target:?}"))?;
            Ok(Some(ResolvedLaunchRoot {
                root_dir: export.root_dir,
                file_tree: export.file_tree,
            }))
        }
    }
}

struct ResolvedLaunchRoot {
    root_dir: PathBuf,
    file_tree: Option<crate::ProcessFileTree>,
}

#[allow(dead_code)]
impl ChildRuntimeController {
    async fn observe_files(&self) -> Result<Option<ChildFileObservation>> {
        let environment = &self.process_environment;
        let process_registry = &self.process_registry;
        let pid = self.process_pid.as_str();
        let timeout = Duration::from_secs(1);
        let pid = alan_kernel::Pid(pid.parse().context("parse observed child pid")?);
        let (process_status, process_exit_code) = process_registry
            .try_observe_process_lifecycle(pid)
            .unwrap_or((alan_kernel::Status::Running, None));
        let process = if process_status == alan_kernel::Status::Exited {
            process_registry.observe_process_files(pid).await
        } else {
            None
        };
        let activity = tokio::time::timeout(timeout, environment.read_ui_activity_snapshot())
            .await
            .context("observe child activity timed out")??;
        let output_text = tokio::time::timeout(timeout, environment.read_assistant_output())
            .await
            .context("observe child output timed out")??;
        let ui_events_offset = tokio::time::timeout(timeout, environment.ui_events_offset())
            .await
            .context("observe child UI events offset timed out")??;
        let notice = tokio::time::timeout(timeout, environment.read_ui_notice_snapshot())
            .await
            .context("observe child notice timed out")??;
        let request_ids = tokio::time::timeout(timeout, environment.request_ids())
            .await
            .context("observe child requests timed out")??;
        let pending_request_id =
            tokio::time::timeout(timeout, environment.pending_request_id(&request_ids))
                .await
                .context("observe child pending request timed out")??;
        let request_events_offset =
            tokio::time::timeout(timeout, environment.request_events_offset())
                .await
                .context("observe child request stream offset timed out")??;
        let action_ids = tokio::time::timeout(timeout, environment.action_ids())
            .await
            .context("observe child actions timed out")??;
        let action_events_offset =
            tokio::time::timeout(timeout, environment.action_events_offset())
                .await
                .context("observe child action stream offset timed out")??;
        Ok(Some(ChildFileObservation {
            process_status: Some(process_status),
            process_exit_code,
            output_text,
            process_output_offset: process
                .as_ref()
                .map(|snapshot| snapshot.output_offset)
                .unwrap_or(0),
            process_io_events_offset: process
                .as_ref()
                .map(|snapshot| snapshot.io_events_offset)
                .unwrap_or(0),
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
        }))
    }

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
        let file_poll_interval = self
            .timeout
            .map(|timeout| (timeout / 4).min(MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL))
            .unwrap_or(MAX_CHILD_FILE_OBSERVATION_POLL_INTERVAL)
            .max(Duration::from_millis(10));
        let mut last_file_observation = None;

        loop {
            if let Some(observation) = self.observe_files().await? {
                if last_file_observation.as_ref() != Some(&observation) {
                    latest_liveness_at = Instant::now();
                    self.child_run_registry.observe_progress(
                        &self.child_run_id,
                        "agentfs",
                        Some(format!(
                            "process={:?} exit={:?} activity={:?} output={} output_offset={} io_offset={} requests={} request_offset={} actions={} action_offset={} ui_offset={}",
                            observation.process_status,
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
                    if last_file_observation.as_ref().is_none_or(
                        |previous: &ChildFileObservation| previous.notice != observation.notice,
                    ) && observation.notice.kind == alan_agent_protocol::UiNoticeKind::Warning
                        && !observation.notice.message.is_empty()
                    {
                        push_bounded_child_warning(
                            &mut warnings,
                            observation.notice.message.clone(),
                        );
                    }
                }
                output_text.clone_from(&observation.output_text);
                if observation.process_status == Some(alan_kernel::Status::Exited) {
                    let exit_code = observation.process_exit_code.unwrap_or(1);
                    if exit_code == 130 {
                        return Ok(ChildRuntimeWaitOutcome::Observed(
                            self.externally_stopped_observed_event(
                                &observation.output_text,
                                &warnings,
                            ),
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
                                .then(|| format!("Child Process exited with code {exit_code}")),
                            None,
                        ),
                    ));
                }
                if observation.activity.state == alan_agent_protocol::UiActivityState::Paused
                    && let Some(request_id) = observation.pending_request_id.as_ref()
                {
                    let kind = self
                        .process_environment
                        .read_request_kind(request_id)
                        .await?;
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
            }

            if let Some(request) = self
                .child_run_registry
                .termination_request(&self.child_run_id)
            {
                match request.mode {
                    ChildRunTerminationMode::Graceful => self.terminate_runtime().await,
                    ChildRunTerminationMode::Forceful => self.abort_runtime().await,
                }
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    self.terminated_observed_event(request),
                ));
            }

            if let Some(cap) = wall_clock_cap
                && started_at.elapsed() >= cap
            {
                self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut)
                    .await;
                return Ok(ChildRuntimeWaitOutcome::Observed(
                    self.timed_out_observed_event("Child-agent wall-clock cap exceeded"),
                ));
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
                            self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut).await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(
                                self.timed_out_observed_event("Child-agent turn idle timed out"),
                            ));
                        }
                        _ = tokio::time::sleep(file_poll_interval) => {
                            continue;
                        }
                    }
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(idle_remaining) => {
                            self.abort_runtime_for_status(&ChildRuntimeStatus::TimedOut).await;
                            return Ok(ChildRuntimeWaitOutcome::Observed(
                                self.timed_out_observed_event("Child-agent turn idle timed out"),
                            ));
                        }
                        _ = tokio::time::sleep(file_poll_interval) => {
                            continue;
                        }
                    }
                }
            } else if let Some(cancel) = cancel {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        self.terminate_runtime().await;
                        return Ok(ChildRuntimeWaitOutcome::Cancelled);
                    }
                    _ = tokio::time::sleep(file_poll_interval) => {
                        continue;
                    }
                }
            } else {
                tokio::time::sleep(file_poll_interval).await;
            }
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
        let Ok(pid) = self.process_pid.parse::<u64>() else {
            return;
        };
        self.process_registry
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
        let Ok(pid) = self.process_pid.parse::<u64>() else {
            return;
        };
        self.process_registry
            .record_exit(
                alan_kernel::Pid(pid),
                child_runtime_process_exit_code(status),
            )
            .await;
        self.reconcile_exited_process().await;
    }

    async fn terminate_process_and_reconcile(&self) {
        let environment = &self.process_environment;
        let pid = self.process_pid.as_str();
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
        let environment = &self.process_environment;
        let pid = self.process_pid.as_str();
        if let Ok(Some(exit_code)) = environment.read_process_exit_code(pid).await {
            self.child_run_registry
                .reconcile_process_exit(&self.child_run_id, exit_code);
        }
    }

    async fn external_process_stop_observed(&self) -> bool {
        let environment = &self.process_environment;
        let pid = self.process_pid.as_str();
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
    agent_root: &alan_agentfs::AgentRootFs,
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
    agent_root.unbind_process(&pid.to_string()).await;
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
        child_agent_config.core_config.memory.store_dir = None;
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

#[derive(Debug, Clone)]
struct ChildNamespaceAssemblyPlan {
    agent_mount: String,
    llm_mount: String,
    llm_connection_name: String,
    srv_mount: String,
    route_mount: String,
    bin_tool_mounts: Vec<String>,
    tool_packages: Vec<super::ToolPackageManifest>,
    cwd: Option<PathBuf>,
    launch_context: crate::ProcessLaunchContext,
}

impl ChildNamespaceAssemblyPlan {
    fn runtime_execution_binding(
        &self,
        scratch: Option<PathBuf>,
    ) -> Result<Option<crate::tools::ToolExecutionBinding>> {
        if self.launch_context.host_mounts.is_empty() {
            return Ok(None);
        }
        let scratch = scratch.context(
            "child Agent Process with Host Mounts requires Agent Runtime Service store bindings",
        )?;
        self.execution_binding(scratch)
    }

    fn execution_binding(
        &self,
        scratch: PathBuf,
    ) -> Result<Option<crate::tools::ToolExecutionBinding>> {
        if self.launch_context.host_mounts.is_empty() {
            return Ok(None);
        }
        let launch_context = self.launch_context.clone();
        Ok(Some(
            crate::tools::ToolExecutionBinding::from_launch_context(&launch_context, scratch)?,
        ))
    }
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
            descriptors: self
                .launch_context
                .descriptors
                .iter()
                .zip(3_u32..)
                .map(|((_, descriptor), number)| (number, descriptor.path.clone()))
                .collect(),
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
        mounts.extend(self.bin_tool_names().map(|name| {
            ExecNamespaceMount::new(format!("/lib/exec/{name}"), ExecNamespaceAccess::ReadOnly)
        }));
        mounts.extend(self.launch_context.host_mounts.iter().map(|grant| {
            ExecNamespaceMount::new(
                grant.namespace_path.clone(),
                match grant.access {
                    alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                    alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                },
            )
        }));
        mounts.extend(
            self.launch_context
                .descriptors
                .values()
                .filter_map(|descriptor| {
                    self.launch_context
                        .namespace
                        .resolve(&descriptor.path)
                        .ok()
                        .map(|resolved| {
                            let access = match resolved.access {
                                alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                                alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                            };
                            ExecNamespaceMount::new(descriptor.path.clone(), access)
                        })
                }),
        );
        mounts.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.access.cmp(&right.access))
        });
        mounts.dedup();
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
    tool_manifests: Vec<(String, InProcessTransport)>,
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
            tool_manifests: Vec::new(),
        }
    }

    fn with_tool_package(
        mut self,
        bin_path: impl Into<String>,
        bin_tree: InProcessTransport,
        manifest_path: impl Into<String>,
        manifest_tree: InProcessTransport,
    ) -> Self {
        self.bin_tools.push((bin_path.into(), bin_tree));
        self.tool_manifests
            .push((manifest_path.into(), manifest_tree));
        self
    }
}

fn child_namespace_launch_handles_from_parent(
    parent: &RuntimeLoopState,
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: InProcessTransport,
) -> Result<ChildNamespaceLaunchHandles> {
    let shared_services = parent
        .namespace_environment()
        .shared_services()
        .context("parent namespace missing shared service handles for child-agent launch")?;
    Ok(ChildNamespaceLaunchHandles::new(
        agent_tree,
        llm_connection,
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
#[allow(clippy::too_many_arguments)]
async fn spawn_child_namespace_runtime_environment(
    launch_procfs: &alan_kernel::ProcFs,
    runtime_procfs: &alan_kernel::ProcFs,
    plan: &ChildNamespaceAssemblyPlan,
    handles: ChildNamespaceLaunchHandles,
    parent_process_context: Option<super::agent_loop::NamespaceProcessContext>,
    tool_runner: crate::tools::ToolProcessRunner,
    tool_binding: Option<crate::tools::ToolExecutionBinding>,
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
    if let Some(binding) = tool_binding {
        tool_runner.register_process_binding(child_pid, binding);
    }
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
    .with_launch_context(plan.launch_context.clone())
    .with_process_context(launch_procfs.clone(), agent_root, child_pid, tool_runner)
    .with_shared_services(
        handles.srv.clone(),
        handles.route.clone(),
        handles.llm_connection.clone(),
    );
    let mut environment = if let Some(factory) = mount_grant_applicator_factory {
        environment.with_mount_grant_applicator_factory(factory, live_namespace)
    } else {
        environment
    };
    if let Some(grant) = plan
        .launch_context
        .host_mounts
        .iter()
        .find(|grant| grant.namespace_path == "/agent-definition")
    {
        let applied = environment.apply_approved_mount_grant(&super::ApprovedMountGrant::new(
            grant.namespace_path.clone(),
            grant.host_path.clone(),
            match grant.access {
                alan_kernel::Access::ReadOnly => super::ApprovedMountGrantAccess::ReadOnly,
                alan_kernel::Access::ReadWrite => super::ApprovedMountGrantAccess::ReadWrite,
            },
            "Agent Definition launch reference",
        ));
        if environment.mount_grant_applicator_factory().is_some() {
            anyhow::ensure!(
                applied.namespace_applied,
                "failed to project child Agent Definition: {}",
                applied
                    .namespace_error
                    .unwrap_or_else(|| "unknown projection error".to_string())
            );
        }
    }

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
    let expected_manifests = plan
        .bin_tool_names()
        .map(|name| format!("/lib/exec/{name}"))
        .collect::<BTreeSet<_>>();
    let actual_manifests = handles
        .tool_manifests
        .iter()
        .map(|(mount, _)| mount.clone())
        .collect::<BTreeSet<_>>();
    if expected == actual && expected_manifests == actual_manifests {
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
    let mut namespace = plan.launch_context.namespace.child();
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
    for (mount, tree) in &handles.tool_manifests {
        namespace.mount(mount, tree.clone(), alan_kernel::Access::ReadOnly);
    }
    namespace
}

async fn child_observation_environment(
    procfs: &alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    pid: &str,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<super::NamespaceRuntimeEnvironment> {
    let agent_path = format!("/agent/{pid}");
    let agent_tree = agent_root
        .process_tree(pid)
        .await
        .with_context(|| format!("attach observer to child AgentFS {agent_path}"))?;
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        &agent_path,
        InProcessTransport::new(agent_tree),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        alan_kernel::Access::ReadWrite,
    );
    Ok(super::NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace))),
        agent_path,
        plan.llm_connection_name.clone(),
    )
    .with_launch_context(plan.launch_context.clone()))
}

#[cfg_attr(not(test), allow(dead_code))]
fn next_child_namespace_fid() -> Fid {
    Fid(NEXT_CHILD_NAMESPACE_FID.fetch_add(1, Ordering::Relaxed))
}

async fn build_child_namespace_assembly_plan(
    parent: &RuntimeLoopState,
    spec: &SpawnSpec,
    child_core_config: &crate::Config,
    launch_context: crate::ProcessLaunchContext,
) -> Result<ChildNamespaceAssemblyPlan> {
    let cwd = spec
        .launch
        .cwd
        .clone()
        .or_else(|| Some(PathBuf::from(&launch_context.cwd)));
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
        tool_packages: Vec::new(),
        cwd,
        launch_context,
    };

    let packages = parent
        .namespace_environment()
        .discover_tool_packages()
        .await?;
    plan.tool_packages = if let Some(profile) = spec.runtime_overrides.tool_profile.as_ref() {
        let available = packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<BTreeSet<_>>();
        let missing = profile
            .allowed_tools
            .iter()
            .filter(|name| !available.contains(name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!(
                "Child-agent launch requested unavailable tools: {}",
                missing.join(", ")
            );
        }
        packages
            .into_iter()
            .filter(|package| profile.allowed_tools.contains(&package.name))
            .collect()
    } else {
        packages
    };
    plan.bin_tool_mounts = plan
        .tool_packages
        .iter()
        .map(|package| format!("/bin/{}", package.name))
        .collect();
    Ok(plan)
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
        MountGrantApplicatorFactory, NamespaceRuntimeEnvironment, RuntimeConfig,
    };
    use crate::skills::SkillHostCapabilities;
    use crate::tools::Tool;
    use crate::tools::ToolRegistry;
    use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
    use alan_kernel::{
        Access as KernelAccess, Credentials as KernelCredentials, Namespace as KernelNamespace,
        ProcFs as KernelProcFs,
    };
    use alan_llm::LlmProvider;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

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

    fn namespace_environment_for_parent_test_with_route(
        routefs: Arc<alan_routefs::RouteFs>,
    ) -> NamespaceRuntimeEnvironment {
        namespace_environment_for_parent_test_with_services(
            routefs,
            Arc::new(alan_llmfs::LlmFs::new()),
        )
    }

    fn namespace_environment_for_parent_test_with_services(
        routefs: Arc<alan_routefs::RouteFs>,
        llmfs: Arc<alan_llmfs::LlmFs>,
    ) -> NamespaceRuntimeEnvironment {
        namespace_environment_for_parent_test_with_connection(routefs, llmfs, "default")
    }

    fn namespace_environment_for_parent_test_with_connection(
        routefs: Arc<alan_routefs::RouteFs>,
        llmfs: Arc<alan_llmfs::LlmFs>,
        connection: &str,
    ) -> NamespaceRuntimeEnvironment {
        let mut mounts = KernelNamespace::new();
        for name in ["alpha", "beta"] {
            let manifest =
                crate::runtime::ToolPackageManifest::from_tool(&NamedTestTool::new(name), 30)
                    .unwrap();
            mounts.mount(
                &format!("/bin/{name}"),
                memfs_transport(),
                KernelAccess::ReadOnly,
            );
            mounts.mount(
                &format!("/lib/exec/{name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(&manifest).unwrap(),
                ))),
                KernelAccess::ReadOnly,
            );
        }
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(mounts)));
        crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", connection)
            .with_shared_services(
                memfs_transport(),
                InProcessTransport::new(routefs),
                InProcessTransport::new(llmfs),
            )
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
        applied: Arc<Mutex<Vec<ApprovedMountGrant>>>,
    }

    impl RecordingMountGrantApplicatorFactory {
        fn created_count(&self) -> usize {
            *self
                .created
                .lock()
                .expect("created count lock should not be poisoned")
        }

        fn applied_grants(&self) -> Vec<ApprovedMountGrant> {
            self.applied
                .lock()
                .expect("applied grants lock should not be poisoned")
                .clone()
        }
    }

    impl MountGrantApplicatorFactory for RecordingMountGrantApplicatorFactory {
        fn create(
            &self,
            _pid: alan_kernel::Pid,
            live_namespace: alan_kernel::LiveNamespace,
            _inherited_mount_paths: &[String],
        ) -> Arc<dyn MountGrantApplicator> {
            *self
                .created
                .lock()
                .expect("created count lock should not be poisoned") += 1;
            Arc::new(RecordingMountGrantApplicator {
                live_namespace,
                applied: self.applied.clone(),
            })
        }
    }

    #[derive(Debug)]
    struct RecordingMountGrantApplicator {
        live_namespace: alan_kernel::LiveNamespace,
        applied: Arc<Mutex<Vec<ApprovedMountGrant>>>,
    }

    impl MountGrantApplicator for RecordingMountGrantApplicator {
        fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> anyhow::Result<KernelNamespace> {
            self.applied
                .lock()
                .expect("applied grants lock should not be poisoned")
                .push(grant.clone());
            let access = match grant.access {
                ApprovedMountGrantAccess::ReadOnly => KernelAccess::ReadOnly,
                ApprovedMountGrantAccess::ReadWrite => KernelAccess::ReadWrite,
            };
            self.live_namespace
                .mount(&grant.namespace_path, memfs_transport(), access);
            Ok(self.live_namespace.snapshot())
        }
    }

    struct MarkerTool {
        name: String,
        marker: String,
    }

    impl MarkerTool {
        fn new(name: &str, marker: &str) -> Self {
            Self {
                name: name.to_string(),
                marker: marker.to_string(),
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
        requests: RecordedRequests,
        response: GenerationResponse,
        capability_view: crate::skills::ResolvedCapabilityView,
    ) -> RuntimeLoopState {
        let source_root = temp.path().join("source");
        let definition_root = temp.path().join("definition");
        let store_root = temp.path().join("system-store");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::create_dir_all(definition_root.join("persona")).unwrap();
        std::fs::create_dir_all(definition_root.join("skills")).unwrap();
        std::fs::write(
            definition_root.join("agent.toml"),
            "tool_repeat_limit = 4\n",
        )
        .unwrap();

        let store_bindings = crate::AgentRuntimeStoreBindings {
            rollouts: store_root.join("rollouts"),
            checkpoints: store_root.join("checkpoints"),
            cache: store_root.join("cache"),
            tmp: store_root.join("tmp"),
            metadata: store_root.join("metadata"),
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
        let memory_store = store_root.join("memory");
        std::fs::create_dir_all(&memory_store).unwrap();

        let mut launch_namespace = KernelNamespace::new();
        launch_namespace.mount("/mnt/source", memfs_transport(), KernelAccess::ReadWrite);
        launch_namespace.mount(
            "/agent-definition",
            memfs_transport(),
            KernelAccess::ReadOnly,
        );
        launch_namespace.mount("/memory", memfs_transport(), KernelAccess::ReadWrite);
        let launch_context = crate::ProcessLaunchContext::new(
            launch_namespace,
            KernelCredentials::user("parent-agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", &source_root, KernelAccess::ReadWrite)
                .unwrap(),
        )
        .with_host_mount(
            crate::HostMountGrant::new(
                "/agent-definition",
                &definition_root,
                KernelAccess::ReadOnly,
            )
            .unwrap(),
        )
        .with_descriptor(
            crate::AGENT_DEFINITION_DESCRIPTOR,
            crate::ProcessDescriptor::new("/agent-definition").unwrap(),
        )
        .with_descriptor(
            crate::MEMORY_STORE_DESCRIPTOR,
            crate::ProcessDescriptor::new("/memory").unwrap(),
        );

        let mut core_config = crate::Config::default();
        core_config.memory.store_dir = Some(memory_store.clone());
        core_config.openai_responses_model = "gpt-5.4".to_string();
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

        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(RecordingProvider::new(requests, response)),
        );

        RuntimeLoopState {
            machine,
            current_submission_id: None,
            environment: namespace_environment_for_parent_test_with_services(
                Arc::new(alan_routefs::RouteFs::new()),
                llmfs,
            )
            .with_launch_context(launch_context),
            core_config,
            runtime_config: RuntimeConfig {
                store_bindings: Some(store_bindings),
                memory_store_backing: Some(memory_store),
                ..RuntimeConfig::default()
            },
            definition_persona_dirs: Vec::new(),
            prompt_cache:
                super::super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
                    capability_view,
                    Vec::new(),
                    SkillHostCapabilities::with_tools(["alpha", "beta"]),
                ),
            turn_state,
        }
    }

    fn parent_test_tools(config: &crate::Config) -> ToolRegistry {
        let mut tools = ToolRegistry::with_config(Arc::new(config.clone()));
        tools.register(NamedTestTool::new("alpha"));
        tools.register(NamedTestTool::new("beta"));
        tools
    }

    fn inherited_launch_context(parent: &RuntimeLoopState) -> crate::ProcessLaunchContext {
        parent
            .namespace_environment()
            .launch_context()
            .expect("test parent has a Process Launch Context")
            .child()
    }

    fn launch_spec(_definition_root: PathBuf) -> SpawnSpec {
        SpawnSpec {
            target: SpawnTarget::DefinitionDescriptor {
                descriptor: crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            },
            launch: alan_agent_protocol::SpawnLaunchInputs {
                task: "Review the repository changes".to_string(),
                cwd: None,
                timeout_secs: Some(30),
                output_dir: None,
            },
            handles: vec![SpawnHandle::HostMounts],
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        }
    }

    fn capability_plan(host_mount: Option<PathBuf>, tools: &[&str]) -> ChildNamespaceAssemblyPlan {
        let mut launch_context = crate::ProcessLaunchContext::root();
        let cwd = host_mount.as_ref().map(|_| PathBuf::from("/mnt/source"));
        if let Some(host_mount) = host_mount {
            launch_context.namespace.mount(
                "/mnt/source",
                memfs_transport(),
                KernelAccess::ReadWrite,
            );
            launch_context.host_mounts.push(
                crate::HostMountGrant::new("/mnt/source", host_mount, KernelAccess::ReadWrite)
                    .unwrap(),
            );
            launch_context.cwd = "/mnt/source".to_string();
        }
        ChildNamespaceAssemblyPlan {
            agent_mount: "/agent".to_string(),
            llm_mount: "/mnt/llm".to_string(),
            llm_connection_name: "default".to_string(),
            srv_mount: "/srv".to_string(),
            route_mount: alan_routefs::MOUNT_PATH.to_string(),
            bin_tool_mounts: tools.iter().map(|tool| format!("/bin/{tool}")).collect(),
            tool_packages: Vec::new(),
            cwd,
            launch_context,
        }
    }

    #[test]
    fn inherited_mount_without_host_backed_cwd_uses_authorized_native_cwd() {
        let source = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        let mut plan = capability_plan(Some(source.path().to_path_buf()), &["read_file"]);
        plan.cwd = None;
        plan.launch_context.cwd = "/".to_string();

        let binding = plan
            .runtime_execution_binding(Some(scratch.path().to_path_buf()))
            .unwrap()
            .expect("an inherited Host Mount should create a child Tool binding");

        assert_eq!(binding.cwd, dunce::canonicalize(source.path()).unwrap());
        assert_eq!(binding.namespace_cwd, PathBuf::from("/mnt/source"));
        assert_eq!(binding.host_mounts.len(), 1);
        assert_eq!(binding.host_mounts[0].namespace_path, "/mnt/source");
        let sandbox = binding.sandbox_spec.unwrap();
        assert!(
            !sandbox
                .readable_roots
                .iter()
                .any(|root| root == &dunce::canonicalize(scratch.path()).unwrap())
        );
    }

    #[test]
    fn package_projection_does_not_create_host_tool_authority() {
        let mut plan = capability_plan(None, &["read_file"]);
        plan.launch_context.package_references.push(
            crate::ProcessPackageReference::new(
                "example",
                "a".repeat(64),
                crate::ProcessPackageKind::Installed,
                "/lib/pkg/example",
                Vec::new(),
                memfs_transport(),
            )
            .unwrap(),
        );

        assert!(plan.runtime_execution_binding(None).unwrap().is_none());
    }

    #[tokio::test]
    async fn delegated_spawn_boundary_passes_satisfied_task_unchanged() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let host_mount = PathBuf::from("/tmp/repo");
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.launch.task = "Inspect local files".to_string();
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                    path: Some(PathBuf::from("/mnt/source")),
                },
                alan_agent_protocol::DelegatedCapabilityRequirement::LlmConnection,
            ],
        });

        let decision = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(Some(host_mount), &["read_file"]),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            decision.recovery,
            alan_agent_protocol::DelegatedCapabilityRecovery::Satisfied
        );
        assert_eq!(spec.launch.task, "Inspect local files");
    }

    #[tokio::test]
    async fn delegated_spawn_boundary_rewrites_narrowed_task_explicitly() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let host_mount = PathBuf::from("/tmp/repo");
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.launch.task = "Review GitHub issue against local code".to_string();
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                    path: Some(PathBuf::from("/mnt/source")),
                },
                alan_agent_protocol::DelegatedCapabilityRequirement::Github,
            ],
        });

        let decision = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(Some(host_mount), &["read_file"]),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            decision.recovery,
            alan_agent_protocol::DelegatedCapabilityRecovery::Narrowed
        );
        assert!(spec.launch.task.contains("NARROWED DELEGATION SCOPE"));
        assert!(spec.launch.task.contains("Withheld capabilities: github"));
    }

    #[tokio::test]
    async fn delegated_spawn_boundary_declines_unsatisfied_mount() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("unused"),
        );
        let mut spec = launch_spec(temp.path().join("agent"));
        spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
            requirements: vec![
                alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                    path: Some(PathBuf::from("/mnt/private")),
                },
            ],
        });

        let error = evaluate_delegated_launch_capabilities(
            &parent,
            &mut spec,
            &capability_plan(None, &["read_file"]),
        )
        .await
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
        let package_store = temp.path().join("package-store");
        let package_root = package_store.join("repo-review");
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
                path: package_store,
                scope: crate::skills::SkillScope::Descriptor,
            },
        ])
    }

    fn memfs_transport() -> InProcessTransport {
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new()))
    }

    fn host_launch_root(path: impl Into<PathBuf>) -> ResolvedLaunchRoot {
        ResolvedLaunchRoot {
            root_dir: path.into(),
            file_tree: None,
        }
    }

    fn package_skill_descriptor(id: &str) -> crate::ProcessFileTree {
        crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
            "SKILL.md".to_string(),
            format!("---\nname: {id}\ndescription: test\n---\n").into_bytes(),
        )]))
        .unwrap()
    }

    fn namespace_from_child_plan(plan: &ChildNamespaceAssemblyPlan) -> KernelNamespace {
        let mut namespace = plan.launch_context.namespace.child();
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
        for name in plan.bin_tool_names() {
            namespace.mount(
                &format!("/lib/exec/{name}"),
                memfs_transport(),
                KernelAccess::ReadOnly,
            );
        }
        namespace
    }

    async fn read_proc_path(fs: &KernelProcFs, names: Vec<String>, fid: Fid) -> String {
        fs.walk(Fid::ROOT, fid, &names).await.unwrap();
        fs.open(fid, OpenMode::Read).await.unwrap();
        String::from_utf8(fs.read(fid, 0, 4096).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn spawn_child_runtime_inherits_namespace_tools_but_not_optional_handles() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("definition");
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
        assert!(request.tools.iter().any(|tool| tool.name == "alpha"));
        assert!(request.tools.iter().any(|tool| tool.name == "beta"));
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
    async fn spawn_child_runtime_reuses_the_passed_callable_connection() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Shared Connection completed the child.");
        let parent = make_parent_state(&temp, requests.clone(), response);

        let child = spawn_child_runtime(&parent, launch_spec(temp.path().join("definition")))
            .await
            .unwrap();
        let result = child.join().await.unwrap();

        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "Shared Connection completed the child.");
        assert_eq!(requests.0.lock().unwrap().len(), 1);
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
        let root_dir = temp.path().join("definition");
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
        let root_dir = temp.path().join("definition");
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
        let root_dir = temp.path().join("definition");
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
    async fn spawn_child_runtime_filters_namespace_tools_with_override() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Only one tool should be visible.");
        let parent = make_parent_state_with_capability_view(
            &temp,
            requests.clone(),
            response.clone(),
            crate::skills::ResolvedCapabilityView::default(),
        );
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
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
    async fn spawn_child_runtime_respects_empty_namespace_tool_override() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("No tools should be visible.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
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

    #[tokio::test]
    async fn child_namespace_plan_mounts_only_allowed_tools() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });

        let launch_context = parent
            .namespace_environment()
            .launch_context()
            .unwrap()
            .child();
        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            launch_context,
        )
        .await
        .unwrap();

        assert_eq!(plan.llm_mount, "/mnt/llm");
        assert_eq!(plan.llm_connection_name().unwrap(), "default");
        assert_eq!(plan.srv_mount, "/srv");
        assert_eq!(plan.route_mount, "/mnt/route");
        assert_eq!(plan.cwd, Some(PathBuf::from("/mnt/source")));
        assert_eq!(plan.launch_context.cwd, "/mnt/source");
        assert_eq!(plan.bin_tool_mounts, vec!["/bin/alpha"]);
    }

    #[tokio::test]
    async fn child_clone_exec_spec_declares_agent_and_llm_mounts_for_pid() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");
        let mut child_core_config = parent.core_config.clone();
        child_core_config.connection_profile = Some("child-main".to_string());
        let spec = launch_spec(root_dir);

        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &child_core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        let exec = plan.clone_exec_spec_for_pid("42", "/bin/alan-agent", ["--boot"]);

        assert_eq!(
            serde_json::to_value(&exec).unwrap(),
            json!({
                "executable": "/bin/alan-agent",
                "args": ["--boot"],
                "descriptors": {
                    "3": "/agent-definition",
                    "4": "/memory"
                },
                "namespace": {
                    "mounts": [
                        {"path": "/agent", "access": "rw"},
                        {"path": "/agent-definition", "access": "ro"},
                        {"path": "/bin/alpha", "access": "ro"},
                        {"path": "/bin/beta", "access": "ro"},
                        {"path": "/lib/exec/alpha", "access": "ro"},
                        {"path": "/lib/exec/beta", "access": "ro"},
                        {"path": "/memory", "access": "rw"},
                        {"path": "/mnt/llm", "access": "rw"},
                        {"path": "/mnt/route", "access": "rw"},
                        {"path": "/mnt/source", "access": "rw"},
                        {"path": "/srv", "access": "ro"}
                    ]
                }
            })
        );
        let decoded: ExecSpec = serde_json::from_value(serde_json::to_value(&exec).unwrap())
            .expect("child clone document uses the kernel ExecSpec contract");
        assert_eq!(decoded, exec);
    }

    #[tokio::test]
    async fn child_clone_exec_spec_declares_only_allowed_bin_mounts() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });

        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        let manifest = plan.namespace_manifest_for_pid("99");

        assert_eq!(
            serde_json::to_value(&manifest).unwrap(),
            json!({
                "mounts": [
                    {"path": "/agent", "access": "rw"},
                    {"path": "/agent-definition", "access": "ro"},
                    {"path": "/bin/alpha", "access": "ro"},
                    {"path": "/lib/exec/alpha", "access": "ro"},
                    {"path": "/memory", "access": "rw"},
                    {"path": "/mnt/llm", "access": "rw"},
                    {"path": "/mnt/route", "access": "rw"},
                    {"path": "/mnt/source", "access": "rw"},
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
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
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
    async fn child_namespace_launch_and_supervisor_reattachment_use_proc_pid_files() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let tool_runner =
            crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(tool_runner.clone()));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_tool_package(
            "/bin/alpha",
            memfs_transport(),
            "/lib/exec/alpha",
            memfs_transport(),
        );

        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            tool_runner.clone(),
            plan.execution_binding(temp.path().join("scratch")).unwrap(),
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
        .with_tool_package(
            "/bin/alpha",
            memfs_transport(),
            "/lib/exec/alpha",
            memfs_transport(),
        );
        let nested = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            child_handles,
            launch.environment.process_context(),
            tool_runner.clone(),
            plan.execution_binding(temp.path().join("scratch")).unwrap(),
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
            &nested.environment.process_context().unwrap().agent_root,
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
        launch
            .environment
            .write_assistant_output("AgentFS child result")
            .await
            .unwrap();
        crate::runtime::ui_surfaces::turn_completed(&launch.environment, false)
            .await
            .unwrap();
        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: launch_procfs,
            process_environment: launch.environment,
            process_pid: process_pid.clone(),
        };

        let result = controller.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "AgentFS child result");
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
        let root_dir = temp.path().join("definition");
        let mut spec = launch_spec(root_dir);
        spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
            allowed_tools: vec!["alpha".to_string()],
        });
        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let tool_runner =
            crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(tool_runner.clone()));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_tool_package(
            "/bin/alpha",
            memfs_transport(),
            "/lib/exec/alpha",
            memfs_transport(),
        );
        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            tool_runner,
            plan.execution_binding(temp.path().join("scratch")).unwrap(),
            None,
            "/bin/alan-agent",
        )
        .await
        .unwrap();
        let process_pid = launch.pid.clone();
        let process_environment = launch.environment.clone();
        let controller = ChildRuntimeController {
            runtime: None,
            startup_metadata: test_startup_metadata("child-machine", None, false),
            child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
            child_run_registry: ChildRunRegistry::default(),
            timeout: None,
            process_registry: launch_procfs,
            process_environment: launch.environment,
            process_pid: process_pid.clone(),
        };

        assert_eq!(process_environment.ui_events_offset().await.unwrap(), 0);
        process_environment
            .write_process_control_for_pid(&process_pid, "cancel")
            .await
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
        let root_dir = temp.path().join("definition");
        let spec = launch_spec(root_dir);
        let mut plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        plan.launch_context
            .host_mounts
            .retain(|grant| grant.namespace_path != "/agent-definition");
        let package_child_definition = temp.path().join("package-child-definition");
        plan.launch_context.host_mounts.push(
            crate::HostMountGrant::new(
                "/agent-definition",
                package_child_definition.clone(),
                KernelAccess::ReadOnly,
            )
            .unwrap(),
        );
        let launch_procfs = KernelProcFs::new();
        let tool_runner =
            crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
        let runtime_procfs = launch_procfs
            .clone()
            .with_runner(Arc::new(tool_runner.clone()));
        let handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            memfs_transport(),
            memfs_transport(),
            memfs_transport(),
        )
        .with_tool_package(
            "/bin/alpha",
            memfs_transport(),
            "/lib/exec/alpha",
            memfs_transport(),
        )
        .with_tool_package(
            "/bin/beta",
            memfs_transport(),
            "/lib/exec/beta",
            memfs_transport(),
        );
        let factory = Arc::new(RecordingMountGrantApplicatorFactory::default());

        let mut launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            tool_runner,
            plan.execution_binding(temp.path().join("scratch")).unwrap(),
            Some(factory.clone()),
            "/bin/alan-agent",
        )
        .await
        .unwrap();

        assert_eq!(factory.created_count(), 1);
        assert_eq!(
            factory.applied_grants(),
            [ApprovedMountGrant::new(
                "/agent-definition",
                package_child_definition,
                ApprovedMountGrantAccess::ReadOnly,
                "Agent Definition launch reference",
            )]
        );
        assert!(
            launch
                .environment
                .mount_grant_applicator_factory()
                .is_some()
        );
        let definition_namespace = read_proc_path(
            &launch_procfs,
            vec![launch.pid.clone(), "namespace".to_string()],
            Fid(95),
        )
        .await;
        assert!(
            definition_namespace
                .lines()
                .any(|line| line == "/agent-definition ro"),
            "child Process must receive its target Agent Definition: {definition_namespace:?}"
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
        let launch_context = inherited_launch_context(&parent);
        parent.environment = namespace_environment_for_parent_test_with_route(routefs.clone())
            .with_launch_context(launch_context);

        let root_dir = temp.path().join("definition");
        let spec = launch_spec(root_dir);
        let plan = build_child_namespace_assembly_plan(
            &parent,
            &spec,
            &parent.core_config,
            inherited_launch_context(&parent),
        )
        .await
        .unwrap();
        let launch_procfs = KernelProcFs::new();
        let runtime_procfs = launch_procfs.clone().with_runner(Arc::new(
            crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config)),
        ));
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
            InProcessTransport::new(llmfs),
        )
        .unwrap()
        .with_tool_package(
            "/bin/alpha",
            memfs_transport(),
            "/lib/exec/alpha",
            memfs_transport(),
        )
        .with_tool_package(
            "/bin/beta",
            memfs_transport(),
            "/lib/exec/beta",
            memfs_transport(),
        );

        let launch = spawn_child_namespace_runtime_environment(
            &launch_procfs,
            &runtime_procfs,
            &plan,
            handles,
            None,
            crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config)),
            plan.execution_binding(temp.path().join("scratch")).unwrap(),
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
        child_tools.register(MarkerTool::new("alpha", "mounted-only"));
        let runner = crate::tools::ToolProcessRunner::from_registry(&child_tools);
        let invocation = alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(1),
            parent: Some(alan_kernel::Pid(0)),
            credentials: alan_kernel::Credentials::user("child-agent"),
            namespace: alan_kernel::Namespace::new(),
            exec: alan_kernel::ExecSpec {
                executable: "/bin/alpha".to_string(),
                args: vec!["{}".to_string()],
                namespace: None,
                descriptors: Default::default(),
            },
        };

        let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;

        assert_eq!(outcome.exit_code, 127);
        assert_eq!(outcome.output, b"executable is not mounted\n");
    }

    #[tokio::test]
    async fn spawn_child_runtime_conversation_snapshot_excludes_tool_outputs_without_handle() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Snapshot captured.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("definition");
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
        let root_dir = temp.path().join("definition");
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
    async fn spawn_child_runtime_preserves_explicit_connection_profile() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child used the explicit profile.");
        let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
        let profile_id = "explicit-main";
        parent.core_config.connection_profile = Some(profile_id.to_string());
        let launch_context = parent
            .namespace_environment()
            .launch_context()
            .unwrap()
            .clone();
        parent.environment = namespace_environment_for_parent_test_with_connection(
            Arc::new(alan_routefs::RouteFs::new()),
            Arc::new(alan_llmfs::LlmFs::new()),
            profile_id,
        )
        .with_launch_context(launch_context);
        let root_dir = temp.path().join("definition");
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
        assert_eq!(
            seen_config
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|config| config.connection_profile.as_deref()),
            Some(profile_id)
        );
    }

    #[tokio::test]
    async fn spawn_child_runtime_rejects_unpassed_definition_connection_reference() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child used its definition profile.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let profile_id = "child-main";
        let root_dir = temp.path().join("definition");
        std::fs::write(
            root_dir.join("agent.toml"),
            format!("connection_profile = \"{profile_id}\"\n"),
        )
        .unwrap();
        let error =
            match spawn_child_runtime_with_client_factory(&parent, launch_spec(root_dir), |_| {
                unreachable!("an unpassed Connection must fail before provider setup")
            })
            .await
            {
                Ok(_) => panic!("unpassed child Connection should be rejected"),
                Err(error) => error,
            };

        assert!(
            error
                .to_string()
                .contains("was not passed by the parent Process")
        );
    }

    #[tokio::test]
    async fn spawn_child_runtime_applies_reasoning_effort_override_after_overlay() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let root_dir = temp.path().join("definition");
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
    fn child_agent_config_requires_memory_handle_for_memory_dir() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("Child finished cleanly.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");

        let mut approval_spec = launch_spec(root_dir.clone());
        approval_spec.handles = vec![SpawnHandle::ApprovalScope];
        let approval_config = build_child_agent_config(&parent, &approval_spec);
        assert_eq!(approval_config.core_config.memory.store_dir, None);

        let mut override_spec = launch_spec(root_dir);
        override_spec.runtime_overrides.policy_path = Some("policy.yaml".to_string());
        let override_config = build_child_agent_config(&parent, &override_spec);
        assert_eq!(override_config.core_config.memory.store_dir, None);
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

    #[test]
    fn child_launch_contract_rejects_relative_namespace_cwd() {
        let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
        spec.launch.cwd = Some(PathBuf::from("docs"));

        let err = validate_child_launch_contract(&spec).unwrap_err();
        assert!(
            format!("{err:#}").contains("absolute"),
            "expected absolute-path validation error, got {err:#}"
        );
    }

    #[test]
    fn child_launch_contract_rejects_non_normal_namespace_cwd() {
        for cwd in ["/mnt/source/../other", "/mnt/./source"] {
            let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
            spec.launch.cwd = Some(PathBuf::from(cwd));

            let err = validate_child_launch_contract(&spec).unwrap_err();
            assert!(
                err.to_string().contains("Invalid child-agent launch cwd"),
                "expected normal-path validation error for {cwd}, got {err:#}"
            );
        }
    }

    #[test]
    fn child_launch_context_does_not_inherit_parent_host_mounts_or_descriptors_by_default() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("done"),
        );
        let parent_context = parent.namespace_environment().launch_context().unwrap();
        let definition = temp.path().join("child-definition");
        let mut spec = launch_spec(definition.clone());
        spec.handles.clear();

        let definition = host_launch_root(definition);
        let child =
            build_child_launch_context(parent_context, &spec, None, Some(&definition)).unwrap();

        assert_eq!(child.cwd, "/");
        assert!(child.namespace.resolve("/mnt/source").is_err());
        assert!(
            child
                .host_mounts
                .iter()
                .all(|grant| grant.namespace_path != "/mnt/source")
        );
        assert_eq!(
            child
                .descriptors
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec![crate::AGENT_DEFINITION_DESCRIPTOR]
        );
    }

    #[test]
    fn child_launch_context_keeps_package_projection_with_inherited_reference() {
        let package = TempDir::new().unwrap();
        let skill_root = package.path().join("skills/inherited");
        std::fs::create_dir_all(&skill_root).unwrap();
        std::fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: inherited\ndescription: Inherited package Skill.\n---\n",
        )
        .unwrap();

        let mut namespace = KernelNamespace::new();
        namespace.mount(
            "/lib/pkg/parent-pack",
            memfs_transport(),
            KernelAccess::ReadOnly,
        );
        let package_reference = crate::ProcessPackageReference::new(
            "parent-pack",
            "a".repeat(64),
            crate::ProcessPackageKind::Installed,
            "/lib/pkg/parent-pack",
            vec![
                crate::ProcessPackageSkillReference::new(
                    "inherited",
                    "skills/inherited",
                    Vec::new(),
                    package_skill_descriptor("inherited"),
                )
                .unwrap(),
            ],
            memfs_transport(),
        )
        .unwrap();
        let parent = crate::ProcessLaunchContext::new(
            namespace,
            KernelCredentials::user("parent-agent"),
            "/lib/pkg/parent-pack",
        )
        .unwrap()
        .with_package_reference(package_reference);
        let mut spec = launch_spec(package.path().join("unused-definition"));
        spec.handles.clear();

        let child = build_child_launch_context(&parent, &spec, None, None).unwrap();

        assert_eq!(child.cwd, "/lib/pkg/parent-pack");
        assert_eq!(child.package_references.len(), 1);
        assert_eq!(child.package_references[0].package_id, "parent-pack");
        assert!(child.namespace.resolve("/lib/pkg/parent-pack").is_ok());
        assert!(child.host_mounts.is_empty());
        let resolved = crate::ResolvedAgentDefinition::from_launch_context(
            &child,
            &[],
            crate::ConfigSourceKind::Default,
        )
        .unwrap();
        assert_eq!(resolved.capability_view.packages.len(), 1);
        assert_eq!(
            resolved.capability_view.packages[0].id,
            "installed:parent-pack:inherited"
        );
    }

    #[test]
    fn package_child_definition_is_passed_by_descriptor_without_host_mount() {
        let parent = crate::ProcessLaunchContext::root();
        let definition = ResolvedLaunchRoot {
            root_dir: PathBuf::from("/lib/pkg/review/skills/review/agents/critic"),
            file_tree: Some(
                crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
                    "agent.toml".to_string(),
                    b"tool_repeat_limit = 3\n".to_vec(),
                )]))
                .unwrap(),
            ),
        };
        let mut spec = launch_spec(definition.root_dir.clone());
        spec.handles.clear();

        let child = build_child_launch_context(&parent, &spec, None, Some(&definition)).unwrap();

        assert!(child.host_mounts.is_empty());
        let descriptor = child
            .descriptor(crate::AGENT_DEFINITION_DESCRIPTOR)
            .unwrap();
        assert_eq!(descriptor.path, definition.root_dir.to_string_lossy());
        assert!(
            descriptor
                .file_tree
                .as_ref()
                .is_some_and(|tree| tree.contains_file("agent.toml"))
        );
    }

    #[test]
    fn child_launch_context_binds_a_noncanonical_parent_definition_path() {
        let definition = TempDir::new().unwrap();
        let mut namespace = KernelNamespace::new();
        namespace.mount(
            "/lib/agents/root",
            memfs_transport(),
            KernelAccess::ReadOnly,
        );
        let parent = crate::ProcessLaunchContext::new(
            namespace,
            KernelCredentials::user("parent-agent"),
            "/",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new(
                "/lib/agents/root",
                definition.path(),
                KernelAccess::ReadOnly,
            )
            .unwrap(),
        )
        .with_descriptor(
            crate::AGENT_DEFINITION_DESCRIPTOR,
            crate::ProcessDescriptor::new("/lib/agents/root").unwrap(),
        );
        let spec = launch_spec(definition.path().to_path_buf());

        let definition_root = host_launch_root(definition.path());
        let child =
            build_child_launch_context(&parent, &spec, None, Some(&definition_root)).unwrap();

        assert!(child.namespace.resolve("/agent-definition").is_ok());
        assert_eq!(
            child
                .descriptor(crate::AGENT_DEFINITION_DESCRIPTOR)
                .unwrap()
                .path,
            "/agent-definition"
        );
    }

    #[test]
    fn child_launch_context_keeps_bootstrap_definition_until_descendant_target_is_projected() {
        let root = TempDir::new().unwrap();
        let parent_definition = root.path().join("parent-definition");
        let package_root = root.path().join("package");
        let child_definition = package_root.join("agents/reviewer");
        std::fs::create_dir_all(&parent_definition).unwrap();
        std::fs::create_dir_all(&child_definition).unwrap();

        let mut namespace = KernelNamespace::new();
        namespace.mount(
            "/agent-definition",
            memfs_transport(),
            KernelAccess::ReadOnly,
        );
        namespace.mount("/mnt/package", memfs_transport(), KernelAccess::ReadOnly);
        let parent = crate::ProcessLaunchContext::new(
            namespace,
            KernelCredentials::user("parent-agent"),
            "/",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new(
                "/agent-definition",
                parent_definition,
                KernelAccess::ReadOnly,
            )
            .unwrap(),
        )
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/package", package_root, KernelAccess::ReadOnly)
                .unwrap(),
        )
        .with_descriptor(
            crate::AGENT_DEFINITION_DESCRIPTOR,
            crate::ProcessDescriptor::new("/agent-definition").unwrap(),
        );

        let child_definition_root = host_launch_root(&child_definition);
        let child = build_child_launch_context(
            &parent,
            &launch_spec(child_definition.clone()),
            None,
            Some(&child_definition_root),
        )
        .unwrap();

        assert!(child.namespace.resolve("/agent-definition").is_ok());
        assert_eq!(
            child
                .host_mounts
                .iter()
                .find(|grant| grant.namespace_path == "/agent-definition")
                .unwrap()
                .host_path,
            child_definition
        );
    }

    #[test]
    fn child_launch_context_passes_parent_host_mounts_only_with_explicit_handle() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("done"),
        );
        let parent_context = parent.namespace_environment().launch_context().unwrap();
        let definition = temp.path().join("child-definition");
        let spec = launch_spec(definition.clone());

        let definition = host_launch_root(definition);
        let child =
            build_child_launch_context(parent_context, &spec, None, Some(&definition)).unwrap();

        assert_eq!(child.cwd, "/mnt/source");
        assert!(child.namespace.resolve("/mnt/source").is_ok());
        assert!(
            child
                .host_mounts
                .iter()
                .any(|grant| grant.namespace_path == "/mnt/source")
        );
    }

    #[test]
    fn child_launch_rejects_cwd_inside_an_unpassed_host_mount() {
        let temp = TempDir::new().unwrap();
        let parent = make_parent_state(
            &temp,
            RecordedRequests::default(),
            completed_response("done"),
        );
        let parent_context = parent.namespace_environment().launch_context().unwrap();
        let definition = temp.path().join("child-definition");
        let mut spec = launch_spec(definition.clone());
        spec.handles.clear();

        let definition = host_launch_root(definition);
        let error = build_child_launch_context(
            parent_context,
            &spec,
            Some("/mnt/source".to_string()),
            Some(&definition),
        )
        .unwrap_err();
        assert!(error.to_string().contains("explicit host_mounts handle"));
    }

    #[test]
    fn child_launch_contract_normalizes_repeated_namespace_separators() {
        let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
        spec.launch.cwd = Some(PathBuf::from("/mnt//source///docs"));

        assert_eq!(
            validate_child_launch_contract(&spec).unwrap().as_deref(),
            Some("/mnt/source/docs")
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
    async fn child_runtime_join_keeps_running_while_activity_file_is_fresh() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("finished after file heartbeat");
        let parent = make_parent_state(&temp, requests.clone(), response.clone());
        let spec = launch_spec(temp.path().join("definition"));
        let mut child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
            Ok(LlmClient::new(
                RecordingProvider::new(requests.clone(), response.clone())
                    .with_delay(Duration::from_millis(250)),
            ))
        })
        .await
        .unwrap();
        child.timeout = Some(Duration::from_millis(200));
        let environment = child.process_environment.clone();
        tokio::spawn(async move {
            for _ in 0..5 {
                crate::runtime::ui_surfaces::heartbeat(&environment)
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(35)).await;
            }
        });

        let result = child.join().await.unwrap();
        assert_eq!(result.status, ChildRuntimeStatus::Completed);
        assert_eq!(result.output_text, "finished after file heartbeat");
    }

    #[tokio::test]
    async fn spawn_child_runtime_cancellable_aborts_pre_cancelled_launch() {
        let temp = TempDir::new().unwrap();
        let requests = RecordedRequests::default();
        let response = completed_response("This should never run.");
        let parent = make_parent_state(&temp, requests, response);
        let root_dir = temp.path().join("definition");
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
        let root_dir = temp.path().join("definition");
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
        let process_environment = child.process_environment.clone();
        let process_pid = child.process_pid.clone();

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
                cwd: Some(PathBuf::from("/mnt/source")),
                timeout_secs: Some(30),
                ..alan_agent_protocol::SpawnLaunchInputs::default()
            },
            handles: vec![SpawnHandle::HostMounts],
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
        let package_store = temp.path().join("package-store");
        let package_root = package_store.join("repo-review");
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
                path: package_store,
                scope: crate::skills::SkillScope::Descriptor,
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
                cwd: Some(PathBuf::from("/mnt/source")),
                timeout_secs: Some(30),
                ..alan_agent_protocol::SpawnLaunchInputs::default()
            },
            handles: vec![SpawnHandle::HostMounts],
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
