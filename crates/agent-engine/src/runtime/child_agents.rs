use super::agent_loop::RuntimeLoopState;
#[cfg(test)]
use super::child_runs::ChildRunRegistry;
use super::child_runs::{ChildRunRecord, ChildRunStatus};
#[cfg(test)]
use super::delegated_child_run::ChildRuntimeStatus;
use super::delegated_child_run::{DelegatedChildRunSupervision, DelegatedChildRunSupervisor};
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
    SpawnSpec, SpawnTarget, Submission,
};
#[cfg(test)]
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
#[cfg(test)]
use alan_kernel::{ExecNamespaceAccess, ExecNamespaceManifest, ExecNamespaceMount, ExecSpec};
#[cfg(test)]
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE: &str = "Child Agent Process launch cancelled";
const MAX_CHILD_CONVERSATION_MESSAGES: usize = 8;
const MAX_CHILD_CONVERSATION_CHARS: usize = 4_000;
const MAX_CHILD_PLAN_ITEMS: usize = 16;
const MAX_CHILD_PLAN_ITEM_CHARS: usize = 240;
const MAX_CHILD_TOOL_RESULTS: usize = 6;
const MAX_CHILD_TOOL_RESULT_CHARS: usize = 1_200;
const ROUTE_MOUNT_PATH: &str = "/mnt/route";
#[cfg(test)]
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

#[cfg(test)]
pub(crate) async fn spawn_child_runtime(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
) -> Result<DelegatedChildRunSupervisor> {
    spawn_child_runtime_with_optional_cancel(parent, spec, None).await
}

#[allow(
    dead_code,
    reason = "cancellable adapter remains available to focused child-runtime tests"
)]
pub(crate) async fn spawn_child_runtime_cancellable(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<DelegatedChildRunSupervisor> {
    spawn_child_runtime_with_optional_cancel(parent, spec, Some(cancel)).await
}

async fn spawn_child_runtime_with_optional_cancel(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: Option<&CancellationToken>,
) -> Result<DelegatedChildRunSupervisor> {
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
) -> Result<DelegatedChildRunSupervisor>
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
) -> Result<DelegatedChildRunSupervisor> {
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
    };
    let resolved_child_definition = crate::ResolvedAgentDefinition::from_launch_context(
        &child_config.launch_context,
        &child_config
            .agent_config
            .core_config
            .resolved_skill_overrides(),
        child_config.core_config_source,
    )
    .context("Failed to resolve child Agent Process definition")?;
    let mut resolved_child_agent_config = resolved_child_definition
        .apply_to_agent_config(&child_agent_config)
        .context("Failed to resolve effective child Agent Process config")?;
    if spec.has_handle(SpawnHandle::Memory) {
        resolved_child_agent_config.core_config.memory.store_dir =
            parent.core_config.memory.store_dir.clone();
    } else {
        resolved_child_agent_config.core_config.memory.store_dir = None;
    }
    child_config.agent_config = resolved_child_agent_config;
    child_config.core_config_source = crate::ConfigSourceKind::EnvOverride;
    let effective_child_core_config = effective_core_config_for_runtime(&child_config)
        .context("Failed to resolve effective child Agent Process runtime config")?;
    let child_namespace_plan = build_child_namespace_assembly_plan(
        parent,
        &spec,
        &effective_child_core_config,
        child_config.launch_context.clone(),
    )
    .await
    .context("Failed to assemble child Agent Process namespace plan")?;
    let child_connection = child_namespace_plan.llm_connection_name()?;
    ensure_child_connection_is_passed(parent, &child_connection)?;
    let delegation_capability_decision =
        evaluate_delegated_launch_capabilities(parent, &mut spec, &child_namespace_plan).await?;
    #[cfg(test)]
    let test_llm = if let Some(factory) = llm_client_factory {
        let client = factory(&effective_child_core_config)
            .context("Failed to create test child Agent Process LLM client")?;
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection(
            &child_namespace_plan.llm_connection_name()?,
            Box::new(ChildLlmProvider::new(client)),
        );
        Some(InProcessTransport::new(llmfs))
    } else {
        None
    };
    let assembler = parent
        .namespace_environment()
        .child_process_assembler()
        .context("parent Agent Process has no Agent Runtime Service child assembly capability")?;
    let assembly = assembler
        .assemble(super::ChildAgentProcessAssemblyRequest {
            plan: child_namespace_plan.clone(),
            scratch_dir: child_config
                .store_bindings
                .as_ref()
                .map(|stores| stores.tmp.clone()),
            executable: "/bin/alan-agent".to_string(),
            #[cfg(test)]
            llm_override: test_llm,
        })
        .await
        .context("Failed to spawn child Agent Process namespace")?;
    let child_process_pid = assembly.pid.clone();
    let child_process_environment = assembly.observation_environment;
    let process_lifecycle = assembly.lifecycle;
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
        assembly.environment,
        host_capabilities,
        generation_capabilities,
    )
    .context("Failed to spawn child Agent Process runtime")
    {
        Ok(runtime) => runtime,
        Err(err) => {
            record_child_launch_failure_process(&process_lifecycle, &err).await;
            return Err(err);
        }
    };
    let (runtime, startup_metadata) = match wait_for_child_runtime_startup(runtime, cancel).await {
        Ok(ready) => ready,
        Err(err) => {
            record_child_launch_failure_process(&process_lifecycle, &err).await;
            return Err(err);
        }
    };
    let child_run_registry = parent.child_run_registry().clone();
    let child_run_id = uuid::Uuid::new_v4().to_string();
    let mut child_run_record = ChildRunRecord::new(
        child_run_id.clone(),
        parent.process_path(),
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
            record_child_launch_failure_process(&process_lifecycle, &err).await;
            child_run_registry.mark_terminal(&child_run_id, status, Some(format!("{err:#}")));
            return Err(err);
        }
    };
    child_run_registry.mark_running(&child_run_id);

    Ok(DelegatedChildRunSupervisor::new(
        DelegatedChildRunSupervision {
            runtime: Some(runtime),
            startup_metadata,
            child_run_id,
            child_run_registry,
            timeout: spec.launch.timeout_secs.map(Duration::from_secs),
            process_lifecycle,
            process_environment: child_process_environment,
            process_pid: child_process_pid,
        },
    ))
}

fn ensure_child_connection_is_passed(parent: &RuntimeLoopState, requested: &str) -> Result<()> {
    let passed = parent.namespace_environment().llm_connection();
    if requested != passed {
        bail!(
            "Connection '{requested}' was not passed to the child Agent Process by the parent Process; available Connection is '{passed}'."
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
            bail!(
                "Child Agent Process launch cwd '{cwd}' requires the explicit host_mounts handle."
            );
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
            .context("package child Agent Executable descriptor path is not UTF-8")?;
        if !spec.has_handle(SpawnHandle::HostMounts)
            && let Some(parent_definition_path) = parent_definition_path.as_deref()
        {
            launch_context
                .host_mounts
                .retain(|grant| grant.namespace_path != parent_definition_path);
            launch_context.namespace.unmount(parent_definition_path);
        }
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
        (ROUTE_MOUNT_PATH.to_string(), alan_kernel::Access::ReadWrite),
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
                ready.context("Child Agent Process runtime failed to start")?
            }
        }
    } else {
        runtime
            .wait_until_ready()
            .await
            .context("Child Agent Process runtime failed to start")?
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
                result.context("Failed to submit initial child Agent Process turn")?
            }
        }
    } else {
        runtime
            .handle
            .submission_tx
            .send(submission)
            .await
            .context("Failed to submit initial child Agent Process turn")?;
    }

    Ok(runtime)
}

fn validate_child_launch_contract(spec: &SpawnSpec) -> Result<Option<String>> {
    if spec.has_handle(SpawnHandle::Artifacts) || spec.launch.output_dir.is_some() {
        bail!(
            "Child Agent Process launches do not support artifact routing yet; omit SpawnHandle::Artifacts and launch.output_dir."
        );
    }

    if let Some(cwd) = spec.launch.cwd.as_deref()
        && !cwd.is_absolute()
    {
        bail!(
            "Child Agent Process launch cwd '{}' must be absolute.",
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
                    "Child Agent Process launch cwd '{}' must be valid Unicode.",
                    cwd.display()
                )
            })?;
            crate::process_launch::normalize_namespace_path(cwd)
                .with_context(|| format!("Invalid child Agent Process launch cwd '{}'.", cwd))
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
                .ok_or_else(|| {
                    anyhow::anyhow!("Unknown package child Agent Executable target: {target:?}")
                })?;
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
    lifecycle: &Arc<dyn super::AgentProcessLifecycle>,
    error: &anyhow::Error,
) {
    let exit_code = match child_run_status_for_launch_error(error) {
        ChildRunStatus::Cancelled => 130,
        _ => 1,
    };
    lifecycle.finish(exit_code).await;
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

type ChildNamespaceAssemblyPlan = super::ChildAgentProcessAssemblyPlan;

#[cfg(test)]
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
                .package_references
                .iter()
                .filter_map(|reference| {
                    self.launch_context
                        .namespace
                        .resolve(&reference.namespace_path)
                        .ok()
                        .map(|resolved| {
                            let access = match resolved.access {
                                alan_kernel::Access::ReadOnly => ExecNamespaceAccess::ReadOnly,
                                alan_kernel::Access::ReadWrite => ExecNamespaceAccess::ReadWrite,
                            };
                            ExecNamespaceMount::new(reference.namespace_path.clone(), access)
                        })
                }),
        );
        mounts.extend(
            self.launch_context
                .descriptors
                .values()
                .filter(|descriptor| {
                    !self
                        .launch_context
                        .package_references
                        .iter()
                        .any(|reference| {
                            Path::new(&descriptor.path).starts_with(&reference.namespace_path)
                        })
                })
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
}

#[derive(Clone)]
#[cfg(test)]
struct ChildNamespaceLaunchHandles {
    agent_tree: Arc<alan_agentfs::AgentFs>,
    llm_connection: InProcessTransport,
    srv: InProcessTransport,
    route: InProcessTransport,
    bin_tools: Vec<(String, InProcessTransport)>,
    tool_manifests: Vec<(String, InProcessTransport)>,
}

#[cfg(test)]
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

#[cfg(test)]
struct ChildNamespaceRuntimeLaunch {
    pid: String,
    exec: ExecSpec,
    environment: super::NamespaceRuntimeEnvironment,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    lifecycle: Arc<dyn super::AgentProcessLifecycle>,
}

#[cfg(test)]
#[derive(Clone)]
struct TestParentProcessContext {
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    pid: alan_kernel::Pid,
}

#[allow(
    clippy::too_many_arguments,
    reason = "arguments expose each namespace resource explicitly at the transitional assembly seam"
)]
#[cfg(test)]
async fn spawn_child_namespace_runtime_environment(
    launch_procfs: &alan_kernel::ProcFs,
    runtime_procfs: &alan_kernel::ProcFs,
    plan: &ChildNamespaceAssemblyPlan,
    handles: ChildNamespaceLaunchHandles,
    parent_process_context: Option<TestParentProcessContext>,
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
    let mut environment = super::NamespaceRuntimeEnvironment::new(
        root,
        format!("/agent/{pid}"),
        plan.llm_connection_name()?,
    )
    .with_launch_context(plan.launch_context.clone())
    .with_tool_process_context(child_pid, tool_runner.clone());
    let has_mount_grant_applicator = mount_grant_applicator_factory.is_some();
    if let Some(factory) = mount_grant_applicator_factory {
        let applicator = factory.create(child_pid, live_namespace, &[]);
        if let Some(authority) = factory.tool_execution_authority() {
            tool_runner.register_process_authority(child_pid, authority);
        }
        environment = environment.with_mount_grant_applicator(applicator);
    }
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
        if has_mount_grant_applicator {
            anyhow::ensure!(
                applied.namespace_applied,
                "failed to project child Agent Process definition: {}",
                applied
                    .namespace_error
                    .unwrap_or_else(|| "unknown projection error".to_string())
            );
        }
    }

    let lifecycle: Arc<dyn super::AgentProcessLifecycle> = Arc::new(TestAgentProcessLifecycle {
        procfs: launch_procfs.clone(),
        agent_root: agent_root.clone(),
        pid: child_pid,
    });
    Ok(ChildNamespaceRuntimeLaunch {
        pid,
        exec,
        environment,
        agent_root,
        lifecycle,
    })
}

#[cfg(test)]
struct TestAgentProcessLifecycle {
    procfs: alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    pid: alan_kernel::Pid,
}

#[cfg(test)]
impl std::fmt::Debug for TestAgentProcessLifecycle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestAgentProcessLifecycle")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl super::AgentProcessLifecycle for TestAgentProcessLifecycle {
    async fn finish(&self, exit_code: i32) {
        self.procfs.record_exit(self.pid, exit_code).await;
        self.agent_root
            .unbind_process(&self.pid.0.to_string())
            .await;
    }
}

#[cfg(test)]
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

#[cfg(test)]
fn child_spawner_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg(test)]
fn child_runtime_namespace_from_launch_handles(
    plan: &ChildNamespaceAssemblyPlan,
    agent_root_tree: InProcessTransport,
    handles: &ChildNamespaceLaunchHandles,
) -> alan_kernel::Namespace {
    child_namespace_from_launch_handles(plan, agent_root_tree, handles)
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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
        route_mount: ROUTE_MOUNT_PATH.to_string(),
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
                "Child Agent Process launch requested unavailable tools: {}",
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
mod tests;
