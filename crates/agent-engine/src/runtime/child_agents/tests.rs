use super::*;
use crate::llm::{GenerationRequest, GenerationResponse, StreamChunk, TokenUsage};
use crate::runtime::controller::RuntimeStartupMetadata;
use crate::runtime::launch_config::AgentConfig;
use crate::runtime::transition::RuntimeLoopState;
use crate::runtime::{
    ApprovedMountGrant, ApprovedMountGrantAccess, MountGrantApplicator,
    MountGrantApplicatorFactory, NamespaceRuntimeEnvironment, RuntimeConfig,
};
use crate::skills::SkillHostCapabilities;
use crate::tools::Tool;
use crate::tools::ToolRegistry;
use alan_agent_protocol::SpawnTarget;
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{
    Access as KernelAccess, Credentials as KernelCredentials, Namespace as KernelNamespace,
    ProcFs as KernelProcFs,
};
use alan_llm::LlmProvider;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

fn child_launch_runtime(parent: &RuntimeLoopState, spec: &SpawnSpec) -> ChildLaunchRuntime {
    super::super::transition::child_launch_runtime(parent, spec)
}

async fn spawn_child_runtime(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
) -> Result<DelegatedChildRunSupervisor> {
    let runtime = child_launch_runtime(parent, &spec);
    super::spawn_child_runtime(runtime, spec).await
}

async fn spawn_child_runtime_cancellable(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    cancel: &CancellationToken,
) -> Result<DelegatedChildRunSupervisor> {
    let runtime = child_launch_runtime(parent, &spec);
    super::spawn_child_runtime_cancellable(runtime, spec, cancel).await
}

async fn spawn_child_runtime_with_client_factory<F>(
    parent: &RuntimeLoopState,
    spec: SpawnSpec,
    llm_client_factory: F,
) -> Result<DelegatedChildRunSupervisor>
where
    F: FnOnce(&crate::Config) -> Result<LlmClient> + Send,
{
    let runtime = child_launch_runtime(parent, &spec);
    super::spawn_child_runtime_with_client_factory(runtime, spec, llm_client_factory).await
}

async fn build_child_namespace_assembly_plan(
    parent: &RuntimeLoopState,
    spec: &SpawnSpec,
    child_core_config: &crate::Config,
    launch_context: crate::ProcessLaunchContext,
) -> Result<ChildNamespaceAssemblyPlan> {
    let runtime = child_launch_runtime(parent, spec);
    super::build_child_namespace_assembly_plan(&runtime, spec, child_core_config, launch_context)
        .await
}

async fn evaluate_delegated_launch_capabilities(
    parent: &RuntimeLoopState,
    spec: &mut SpawnSpec,
    plan: &ChildNamespaceAssemblyPlan,
) -> Result<Option<alan_agent_protocol::DelegatedCapabilityDecision>> {
    let runtime = child_launch_runtime(parent, spec);
    super::evaluate_delegated_launch_capabilities(&runtime, spec, plan).await
}

fn build_child_agent_config(parent: &RuntimeLoopState, spec: &SpawnSpec) -> AgentConfig {
    let runtime = child_launch_runtime(parent, spec);
    super::build_child_agent_config(&runtime, spec)
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
        durability: super::super::controller::AgentMachineDurabilityState {
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
    namespace_environment_for_parent_test_with_services(routefs, Arc::new(alan_llmfs::LlmFs::new()))
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
            crate::runtime::ToolPackageManifest::from_tool(&NamedTestTool::new(name), 30).unwrap();
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
    let procfs = KernelProcFs::new();
    let tool_runner = crate::tools::ToolRegistry::new().process_runner();
    let assembler = TestChildProcessAssembler {
        procfs,
        tool_runner,
        srv: memfs_transport(),
        route: InProcessTransport::new(routefs),
        llm: InProcessTransport::new(llmfs),
        parent: None,
    };
    crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", connection)
        .with_child_process_assembler(Arc::new(assembler))
}

#[derive(Clone)]
struct TestChildProcessAssembler {
    procfs: KernelProcFs,
    tool_runner: crate::tools::ToolProcessRunner,
    srv: InProcessTransport,
    route: InProcessTransport,
    llm: InProcessTransport,
    parent: Option<TestParentProcessContext>,
}

impl std::fmt::Debug for TestChildProcessAssembler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TestChildProcessAssembler")
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl super::super::ChildAgentProcessAssembler for TestChildProcessAssembler {
    async fn assemble(
        &self,
        request: super::super::ChildAgentProcessAssemblyRequest,
    ) -> Result<super::super::AssembledChildAgentProcess> {
        let super::super::ChildAgentProcessAssemblyRequest {
            plan,
            scratch_dir,
            executable,
            llm_override,
        } = request;
        let mut handles = ChildNamespaceLaunchHandles::new(
            Arc::new(alan_agentfs::AgentFs::new()),
            llm_override.unwrap_or_else(|| self.llm.clone()),
            self.srv.clone(),
            self.route.clone(),
        );
        for manifest in &plan.tool_packages {
            let name = &manifest.name;
            handles = handles.with_tool_package(
                format!("/bin/{name}"),
                memfs_transport(),
                format!("/lib/exec/{name}"),
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(manifest)?,
                ))),
            );
        }
        let runtime_procfs = self
            .procfs
            .clone()
            .with_runner(Arc::new(self.tool_runner.clone()));
        let binding = plan.runtime_execution_binding(scratch_dir)?;
        let mut launch = spawn_child_namespace_runtime_environment(
            &self.procfs,
            &runtime_procfs,
            &plan,
            handles,
            self.parent.clone(),
            self.tool_runner.clone(),
            binding,
            None,
            &executable,
        )
        .await?;
        let observation_environment = child_observation_environment(
            &runtime_procfs,
            launch.agent_root.clone(),
            &launch.pid,
            &plan,
        )
        .await?;
        let child_assembler = Self {
            procfs: self.procfs.clone(),
            tool_runner: self.tool_runner.clone(),
            srv: self.srv.clone(),
            route: self.route.clone(),
            llm: self.llm.clone(),
            parent: Some(TestParentProcessContext {
                agent_root: launch.agent_root.clone(),
                pid: alan_kernel::Pid(launch.pid.parse()?),
            }),
        };
        launch.environment = launch
            .environment
            .with_child_process_assembler(Arc::new(child_assembler));
        Ok(super::super::AssembledChildAgentProcess {
            pid: launch.pid,
            environment: launch.environment,
            observation_environment,
            lifecycle: launch.lifecycle,
        })
    }
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
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
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
        crate::HostMountGrant::new("/mnt/source", &source_root, KernelAccess::ReadWrite).unwrap(),
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
    let mut machine = crate::agent_machine::AgentMachine::new();
    machine.add_user_message("Parent user asks for review");
    machine.add_assistant_message("Parent assistant explains the approach", None);
    machine.add_tool_message("tool_call_1", "alpha", json!({"summary": "tool output"}));

    machine.set_plan_snapshot(
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
        prompt_cache: super::super::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view,
            Vec::new(),
            SkillHostCapabilities::with_tools(["alpha", "beta"]),
        ),
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
        .child_launch()
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
        launch_context
            .namespace
            .mount("/mnt/source", memfs_transport(), KernelAccess::ReadWrite);
        launch_context.host_mounts.push(
            crate::HostMountGrant::new("/mnt/source", host_mount, KernelAccess::ReadWrite).unwrap(),
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

mod capability_boundary;
mod launch_contract;
mod lifecycle;
mod namespace_runtime;
mod package_targets;
