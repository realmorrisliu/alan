mod delegated_launch;
mod launch_context;
mod runtime_startup;
mod task_context;
#[cfg(test)]
mod test_namespace;

use super::child_runs::ChildRunRecord;
#[cfg(test)]
use super::child_runs::ChildRunRegistry;
#[cfg(test)]
use super::delegated_child_run::ChildRuntimeStatus;
use super::delegated_child_run::{DelegatedChildRunSupervision, DelegatedChildRunSupervisor};
use super::engine::{runtime_host_capabilities_for_tools, spawn_with_namespace_environment};
use super::launch_config::{AgentProcessConfig, effective_core_config_for_runtime};
use super::transition::RuntimeLoopState;
#[cfg(test)]
use crate::llm::LlmClient;
use crate::tape::ContentPart;
use alan_agent_protocol::{Op, SpawnHandle, SpawnSpec, Submission};
#[cfg(test)]
use alan_ap::InProcessTransport;
#[cfg(test)]
use alan_llm::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};
use anyhow::{Context, Result, bail};
use delegated_launch::evaluate_delegated_launch_capabilities;
#[cfg(test)]
use launch_context::ResolvedLaunchRoot;
use launch_context::{
    build_child_agent_config, build_child_launch_context, ensure_child_connection_is_passed,
    resolve_launch_root_dir, validate_child_launch_contract,
};
use runtime_startup::{
    CHILD_AGENT_LAUNCH_CANCELLED_MESSAGE, child_run_status_for_launch_error,
    record_child_launch_failure_process, send_initial_child_submission,
    wait_for_child_runtime_startup,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
use test_namespace::{
    ChildNamespaceLaunchHandles, TestParentProcessContext, child_observation_environment,
    spawn_child_namespace_runtime_environment,
};
use tokio_util::sync::CancellationToken;

const ROUTE_MOUNT_PATH: &str = "/mnt/route";
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
        parts: vec![ContentPart::text(task_context::build_child_task_text(
            parent, &spec,
        ))],
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
            agent_files: child_process_environment.agent_files(),
            process_files: child_process_environment.process_files(),
            process_pid: child_process_pid,
        },
    ))
}

type ChildNamespaceAssemblyPlan = super::ChildAgentProcessAssemblyPlan;

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

    let packages = parent.tool_execution().discover_packages().await?;
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

#[cfg(test)]
mod tests;
