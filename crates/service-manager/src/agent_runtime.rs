//! Agent Runtime Service implementation of the `/bin/alan-agent` Process image.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use alan_agent_engine::{
    AGENT_DEFINITION_FD, AgentExecutablePause, AgentExecutableRequest, AgentExecutableResult,
    AgentExecutableStatus, AgentProcessConfig, ContentPart, MEMORY_STORE_FD, Op, RuntimeController,
    SpawnHandle, SpawnTarget, Submission, UiActivitySnapshot, UiActivityState, UiNoticeKind,
    UiNoticeSnapshot, YieldKind,
    skills::SkillHostCapabilities,
    spawn_with_namespace_environment,
    tools::{ToolExecutionAuthority, ToolProcessRunner},
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{
    Access, ExecNamespaceManifest, ExecSpec, LiveNamespace, Pid, ProcessInvocation, ProcessOutcome,
};
use alan_llm::ProviderCapabilities;
use anyhow::{Context, Result, ensure};

use crate::{
    BootUnit, ConnectionService, HostMountService, ProcessLaunchContext,
    process_runner::SystemProcessRunner,
    runtime::{namespace_with_package_references, validate_package_reference_mounts},
};

const AGENT_EXECUTABLE: &str = "/bin/alan-agent";
static NEXT_AGENT_FID: AtomicU64 = AtomicU64::new(90_000);

#[derive(Clone)]
pub(crate) struct RootAgentTemplate {
    process: AgentProcessConfig,
    launch_context: ProcessLaunchContext,
    host_capabilities: SkillHostCapabilities,
    generation_capabilities: ProviderCapabilities,
    llm_connection: String,
}

impl RootAgentTemplate {
    pub(crate) fn new(
        process: AgentProcessConfig,
        launch_context: ProcessLaunchContext,
        host_capabilities: SkillHostCapabilities,
        generation_capabilities: ProviderCapabilities,
        llm_connection: String,
    ) -> Self {
        Self {
            process,
            launch_context,
            host_capabilities,
            generation_capabilities,
            llm_connection,
        }
    }
}

pub(crate) struct RootAgentProcess {
    pid: Pid,
    namespace: InProcessTransport,
    procfs: alan_kernel::ProcFs,
    ready: Option<tokio::sync::oneshot::Receiver<std::result::Result<(), String>>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl RootAgentProcess {
    pub(crate) fn pid(&self) -> Pid {
        self.pid
    }

    pub(crate) fn namespace(&self) -> InProcessTransport {
        self.namespace.clone()
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.procfs
            .try_observe_process_lifecycle(self.pid)
            .is_none_or(|(status, _)| status == alan_kernel::Status::Exited)
    }

    pub(crate) async fn wait_until_ready(&mut self) -> Result<()> {
        let Some(ready) = self.ready.take() else {
            return Ok(());
        };
        match ready.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(anyhow::anyhow!(message)),
            Err(_) => Err(anyhow::anyhow!(
                "Agent Runtime Service stopped before Root Agent readiness"
            )),
        }
    }
}

pub(crate) struct AgentRuntimeService {
    procfs: alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    tool_runner: ToolProcessRunner,
    pending_roots: Mutex<HashMap<u64, PendingRootLaunch>>,
    process_templates: Mutex<HashMap<u64, RootAgentTemplate>>,
}

pub(crate) struct AgentRuntimeFileServers {
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
}

impl AgentRuntimeFileServers {
    pub(crate) fn new(
        agent_root: Arc<alan_agentfs::AgentRootFs>,
        llmfs: Arc<alan_llmfs::LlmFs>,
    ) -> Self {
        Self { agent_root, llmfs }
    }

    pub(crate) fn from_refs(
        agent_root: &Arc<alan_agentfs::AgentRootFs>,
        llmfs: &Arc<alan_llmfs::LlmFs>,
    ) -> Self {
        Self::new(agent_root.clone(), llmfs.clone())
    }
}

struct PendingRootLaunch {
    template: RootAgentTemplate,
    namespace: LiveNamespace,
    ready: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
    stop: tokio::sync::oneshot::Receiver<()>,
}

struct AgentLaunch {
    template: RootAgentTemplate,
    namespace: LiveNamespace,
    request: Option<AgentExecutableRequest>,
    ready: Option<tokio::sync::oneshot::Sender<std::result::Result<(), String>>>,
    stop: Option<tokio::sync::oneshot::Receiver<()>>,
    root: bool,
}

impl AgentRuntimeService {
    pub(crate) fn new(
        procfs: alan_kernel::ProcFs,
        file_servers: AgentRuntimeFileServers,
        host_mount: Arc<HostMountService>,
        connection: Arc<ConnectionService>,
        tool_runner: ToolProcessRunner,
    ) -> Arc<Self> {
        Arc::new(Self {
            procfs,
            agent_root: file_servers.agent_root,
            llmfs: file_servers.llmfs,
            host_mount,
            connection,
            tool_runner,
            pending_roots: Mutex::new(HashMap::new()),
            process_templates: Mutex::new(HashMap::new()),
        })
    }

    fn process_runner(self: &Arc<Self>) -> Arc<SystemProcessRunner> {
        Arc::new(SystemProcessRunner::new(
            Some(Arc::downgrade(self)),
            Some(self.tool_runner.clone()),
        ))
    }

    pub(crate) async fn launch_root(
        self: &Arc<Self>,
        parent_pid: Pid,
        system_namespace: &LiveNamespace,
        unit: &BootUnit,
        template: &RootAgentTemplate,
    ) -> Result<RootAgentProcess> {
        ensure!(
            unit.executable == AGENT_EXECUTABLE,
            "Root Agent Boot Unit must execute {AGENT_EXECUTABLE}"
        );
        let source = namespace_with_package_references(
            system_namespace.snapshot(),
            &template.launch_context,
        )?;
        let namespace = project_boot_unit_namespace(&source.snapshot(), unit)?;
        let live_namespace = LiveNamespace::new(namespace);
        let procfs = self.procfs.clone().with_runner(self.process_runner());
        let spawner = procfs.for_live_spawner(
            Some(parent_pid),
            live_namespace.clone(),
            template.launch_context.credentials.clone(),
        );
        let fid = next_agent_fid();
        spawner
            .walk(Fid::ROOT, fid, &["clone".to_string()])
            .await
            .context("walk Root Agent /proc/clone")?;
        spawner
            .open(fid, OpenMode::ReadWrite)
            .await
            .context("open Root Agent /proc/clone")?;
        let pid = read_clone_pid(&spawner, fid).await?;
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        self.pending_roots
            .lock()
            .expect("pending roots mutex poisoned")
            .insert(
                pid.0,
                PendingRootLaunch {
                    template: template.clone(),
                    namespace: live_namespace.clone(),
                    ready: ready_tx,
                    stop: stop_rx,
                },
            );
        let descriptors = unit
            .descriptors
            .iter()
            .map(|descriptor| (descriptor.number, descriptor.path.clone()))
            .collect();
        let exec = ExecSpec {
            executable: AGENT_EXECUTABLE.to_string(),
            args: Vec::new(),
            namespace: Some(ExecNamespaceManifest::from_namespace(
                &live_namespace.snapshot(),
            )),
            descriptors,
        };
        if let Err(error) = commit_clone(&spawner, fid, &exec).await {
            self.pending_roots
                .lock()
                .expect("pending roots mutex poisoned")
                .remove(&pid.0);
            return Err(error);
        }
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::from_live_namespace(
            live_namespace,
        )));
        Ok(RootAgentProcess {
            pid,
            namespace: root,
            procfs: self.procfs.clone(),
            ready: Some(ready_rx),
            stop: Some(stop_tx),
        })
    }

    pub(crate) async fn detach_root(&self, mut root: RootAgentProcess, exit_code: i32) {
        root.stop.take();
        self.procfs.record_exit(root.pid, exit_code).await;
        self.release_process(root.pid).await;
    }

    pub(crate) async fn shutdown_root(&self, mut root: RootAgentProcess) -> Result<()> {
        if let Some(stop) = root.stop.take() {
            let _ = stop.send(());
        }
        wait_for_process_exit(&self.procfs, root.pid, Duration::from_secs(12)).await?;
        Ok(())
    }

    pub(crate) async fn run_agent_process(
        self: &Arc<Self>,
        invocation: ProcessInvocation,
    ) -> ProcessOutcome {
        if invocation.exec.executable != AGENT_EXECUTABLE {
            return ProcessOutcome::exited(127, b"alan-agent: executable mismatch\n");
        }
        let _cleanup = ProcessCleanup::new(Arc::downgrade(self), invocation.pid);
        let mut launch = match self.prepare_launch(&invocation) {
            Ok(launch) => launch,
            Err(error) => return process_error(error),
        };
        let result = self.run_prepared_agent(&invocation, &mut launch).await;
        if let Err(error) = &result
            && let Some(ready) = launch.ready.take()
        {
            let _ = ready.send(Err(format!("{error:#}")));
        }
        match result {
            Ok(outcome) => outcome,
            Err(error) => process_error(error),
        }
    }

    fn prepare_launch(&self, invocation: &ProcessInvocation) -> Result<AgentLaunch> {
        if invocation.exec.args.is_empty() {
            let pending = self
                .pending_roots
                .lock()
                .expect("pending roots mutex poisoned")
                .remove(&invocation.pid.0)
                .context("Root Agent Process has no pending launch template")?;
            return Ok(AgentLaunch {
                template: pending.template,
                namespace: pending.namespace,
                request: None,
                ready: Some(pending.ready),
                stop: Some(pending.stop),
                root: true,
            });
        }
        ensure!(
            invocation.exec.args.len() == 1,
            "alan-agent accepts one serialized SpawnSpec request"
        );
        let request: AgentExecutableRequest = serde_json::from_str(&invocation.exec.args[0])
            .context("parse /bin/alan-agent SpawnSpec request")?;
        let parent_pid = invocation
            .parent
            .context("child Agent Process has no parent Process")?;
        let parent = self
            .process_templates
            .lock()
            .expect("process templates mutex poisoned")
            .get(&parent_pid.0)
            .cloned()
            .context("parent Agent Process has no runtime template")?;
        let template = child_template(&parent, invocation, &request)?;
        Ok(AgentLaunch {
            template,
            namespace: LiveNamespace::new(invocation.namespace.clone()),
            request: Some(request),
            ready: None,
            stop: None,
            root: false,
        })
    }

    async fn run_prepared_agent(
        self: &Arc<Self>,
        invocation: &ProcessInvocation,
        launch: &mut AgentLaunch,
    ) -> Result<ProcessOutcome> {
        let pid = invocation.pid;
        let credentials = invocation.credentials.clone();
        launch.namespace.replace_mount(
            "/mnt/llm",
            InProcessTransport::new(Arc::new(
                self.llmfs.connection_view(&launch.template.llm_connection),
            )),
            Access::ReadWrite,
        );
        if launch.root {
            self.host_mount
                .register_process(pid, launch.namespace.clone());
        } else {
            let parent = invocation
                .parent
                .context("child Agent Process has no parent")?;
            self.host_mount.register_child_process(
                parent,
                pid,
                launch.namespace.clone(),
                &launch
                    .request
                    .as_ref()
                    .expect("child request exists")
                    .spawn
                    .host_mounts,
            )?;
        }
        validate_process_cwd(&launch.namespace, &launch.template.launch_context.cwd)?;
        self.tool_runner
            .register_process_authority(pid.0, self.host_mount.clone());
        launch.namespace.replace_mount(
            "/mnt/host-mount",
            InProcessTransport::new(self.host_mount.file_server_for_process(pid.0)),
            Access::ReadWrite,
        );
        if self.connection.has_profile(&launch.template.llm_connection) {
            self.connection
                .select(pid.0, &launch.template.llm_connection)?;
        }

        let agent = Arc::new(alan_agentfs::AgentFs::new());
        self.agent_root.bind_process(pid.0.to_string(), agent).await;
        if launch.root {
            self.agent_root.set_root_process(pid.0.to_string()).await;
        }

        let runtime_procfs = self.procfs.clone().with_runner(self.process_runner());
        runtime_procfs
            .bind_live_namespace(pid, launch.namespace.clone())
            .await;
        launch.namespace.replace_mount(
            "/proc",
            InProcessTransport::new(Arc::new(runtime_procfs.for_live_spawner(
                Some(pid),
                launch.namespace.clone(),
                credentials.clone(),
            ))),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::from_live_namespace(
            launch.namespace.clone(),
        )));
        launch.template.launch_context = launch
            .template
            .launch_context
            .rebound_live(launch.namespace.clone(), credentials);
        launch.template.process.namespace_cwd = PathBuf::from(&launch.template.launch_context.cwd);
        self.register_tool_execution_binding(
            pid,
            &launch.template.launch_context,
            launch
                .template
                .process
                .store_bindings
                .as_ref()
                .map(|stores| stores.tmp.clone()),
        )?;
        let environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
            root.clone(),
            format!("/agent/{}", pid.0),
            launch.template.llm_connection.clone(),
        )
        .with_namespace_cwd(&launch.template.launch_context.cwd)
        .with_tool_process_context(pid.0, self.tool_runner.clone());
        let mut controller = spawn_with_namespace_environment(
            launch.template.process.clone(),
            environment,
            launch.template.host_capabilities.clone(),
            launch.template.generation_capabilities,
        )?;
        controller
            .wait_until_ready()
            .await
            .context("Agent Machine failed to start")?;
        self.process_templates
            .lock()
            .expect("process templates mutex poisoned")
            .insert(pid.0, launch.template.clone());
        if let Some(ready) = launch.ready.take() {
            let _ = ready.send(Ok(()));
        }

        let outcome = if let Some(request) = launch.request.as_ref() {
            controller
                .handle
                .submission_tx
                .send(Submission::new(Op::Turn {
                    parts: vec![ContentPart::text(request.initial_task.clone())],
                    context: None,
                }))
                .await
                .context("submit initial child Agent Process turn")?;
            let result = wait_for_child_terminal(&root, pid, &controller).await?;
            let exit_code = match result.status {
                AgentExecutableStatus::Completed => 0,
                AgentExecutableStatus::Paused | AgentExecutableStatus::Failed => 1,
            };
            ProcessOutcome::exited(
                exit_code,
                result
                    .to_process_output_record()
                    .context("serialize Agent Executable result")?,
            )
        } else {
            let exit_code = wait_for_root_stop(
                launch
                    .stop
                    .take()
                    .context("Root Agent stop channel is absent")?,
                &controller,
            )
            .await;
            ProcessOutcome::exited(exit_code, Vec::new())
        };
        let shutdown = controller.shutdown().await;
        if outcome.exit_code == 0 {
            shutdown?;
        }
        Ok(outcome)
    }

    pub(crate) async fn release_process(&self, pid: Pid) {
        self.process_templates
            .lock()
            .expect("process templates mutex poisoned")
            .remove(&pid.0);
        self.pending_roots
            .lock()
            .expect("pending roots mutex poisoned")
            .remove(&pid.0);
        self.agent_root.unbind_process(&pid.0.to_string()).await;
        self.host_mount.unregister_process(pid);
        self.connection.release_process(pid.0);
        self.tool_runner.unregister_process(pid.0);
    }

    fn register_tool_execution_binding(
        &self,
        pid: Pid,
        launch_context: &ProcessLaunchContext,
        scratch_dir: Option<PathBuf>,
    ) -> Result<()> {
        let Some(scratch_dir) = scratch_dir else {
            return Ok(());
        };
        let binding = alan_agent_engine::tools::ToolExecutionBinding::awaiting_host_projection(
            Path::new(&launch_context.cwd).to_path_buf(),
            scratch_dir,
        );
        let binding = self.host_mount.reconcile(pid.0, binding)?;
        self.tool_runner.register_process_binding(pid.0, binding);
        Ok(())
    }
}

fn child_template(
    parent: &RootAgentTemplate,
    invocation: &ProcessInvocation,
    request: &AgentExecutableRequest,
) -> Result<RootAgentTemplate> {
    let spec = &request.spawn;
    spec.validate_agent_process_launch()?;
    ensure!(
        invocation
            .exec
            .descriptors
            .keys()
            .all(|descriptor| matches!(*descriptor, AGENT_DEFINITION_FD | MEMORY_STORE_FD)),
        "child Agent Process contains an unsupported descriptor"
    );
    let definition_path = invocation
        .exec
        .descriptors
        .get(&AGENT_DEFINITION_FD)
        .context("child Agent Process has no Agent Definition descriptor")?;
    let definition = match &spec.target {
        SpawnTarget::DefinitionDescriptor { descriptor } => parent
            .launch_context
            .descriptor(descriptor)
            .cloned()
            .with_context(|| format!("parent Process has no `{descriptor}` descriptor"))?,
        target @ SpawnTarget::PackageChildAgent { .. } => {
            let export = parent
                .process
                .agent_definition
                .capability_view
                .refresh()
                .resolve_child_agent_export(target)
                .cloned()
                .with_context(|| {
                    format!("Unknown package child Agent Executable target: {target:?}")
                })?;
            let file_tree = export
                .file_tree
                .context("package child Agent Executable has no immutable descriptor")?;
            alan_agent_engine::ProcessDescriptor::with_file_tree(
                export.root_dir.to_string_lossy(),
                file_tree,
            )?
        }
    };
    ensure!(
        definition.path == *definition_path,
        "child Agent Definition descriptor does not match SpawnSpec target"
    );

    let mut launch_context = parent.launch_context.child();
    launch_context.namespace = invocation.namespace.clone();
    launch_context.credentials = invocation.credentials.clone();
    launch_context.cwd = spec
        .launch
        .cwd
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    launch_context.descriptors.clear();
    launch_context.descriptors.insert(
        alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR.to_string(),
        definition,
    );
    let memory_path = invocation.exec.descriptors.get(&MEMORY_STORE_FD);
    let parent_memory = parent
        .launch_context
        .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR);
    validate_child_memory_mount(
        &invocation.namespace,
        spec.has_handle(SpawnHandle::Memory),
        parent_memory.map(|memory| memory.path.as_str()),
    )?;
    if spec.has_handle(SpawnHandle::Memory) {
        let memory_path =
            memory_path.context("child Agent Process has no Memory Store descriptor")?;
        let memory = parent_memory
            .cloned()
            .context("parent Process has no Memory Store descriptor")?;
        ensure!(
            memory.path == *memory_path,
            "child Memory Store descriptor does not match the parent descriptor"
        );
        launch_context.descriptors.insert(
            alan_agent_engine::MEMORY_STORE_DESCRIPTOR.to_string(),
            memory,
        );
    } else {
        ensure!(
            memory_path.is_none(),
            "child Agent Process passed a Memory Store descriptor without the Memory handle"
        );
    }
    launch_context.package_references.retain(|reference| {
        !invocation
            .namespace
            .union_at(&reference.namespace_path)
            .is_empty()
    });
    validate_package_reference_mounts(&launch_context)?;

    let mut process = parent.process.child_for_spawn(spec);
    process.agent_definition = alan_agent_engine::ResolvedAgentDefinition::from_process_inputs(
        launch_context.descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR),
        &launch_context.package_references,
        &process.agent_config.core_config.resolved_skill_overrides(),
        alan_agent_engine::ConfigSourceKind::Default,
    )?;
    process.namespace_cwd = PathBuf::from(&launch_context.cwd);
    process.memory_store_bound = launch_context
        .descriptor(alan_agent_engine::MEMORY_STORE_DESCRIPTOR)
        .is_some();
    let effective = alan_agent_engine::runtime::effective_core_config_for_runtime(&process)?;
    let llm_connection = resolve_child_connection(
        &parent.llm_connection,
        effective.connection_profile.as_deref(),
    )?;
    let tools = invocation
        .namespace
        .describe()
        .into_iter()
        .filter_map(|(path, _)| path.strip_prefix("/bin/").map(str::to_string))
        .filter(|name| name != "alan-agent" && name != "q")
        .collect::<Vec<_>>();
    Ok(RootAgentTemplate {
        process,
        launch_context,
        host_capabilities: alan_agent_engine::skills::build_skill_host_capabilities(tools, true),
        generation_capabilities: alan_agent_engine::provider_capabilities_for_config(&effective),
        llm_connection,
    })
}

fn resolve_child_connection(passed: &str, requested: Option<&str>) -> Result<String> {
    let requested = requested.unwrap_or(passed);
    ensure!(
        requested == passed,
        "Connection '{requested}' was not passed to the child Agent Process by the parent Process; available Connection is '{passed}'."
    );
    Ok(requested.to_string())
}

fn validate_child_memory_mount(
    namespace: &alan_kernel::Namespace,
    delegated: bool,
    memory_path: Option<&str>,
) -> Result<()> {
    if !delegated && let Some(memory_path) = memory_path {
        ensure!(
            namespace.union_at(memory_path).is_empty(),
            "child Agent Process retained a Memory Store mount without the Memory handle"
        );
    }
    Ok(())
}

fn project_boot_unit_namespace(
    base: &alan_kernel::Namespace,
    unit: &BootUnit,
) -> Result<alan_kernel::Namespace> {
    let namespace = base
        .project_mounts(unit.mounts.iter().map(|mount| {
            (
                mount.path.as_str(),
                mount.source.as_str(),
                match mount.access {
                    crate::MountAccess::Read => Access::ReadOnly,
                    crate::MountAccess::Write => Access::ReadWrite,
                },
            )
        }))
        .map_err(|_| {
            anyhow::anyhow!(
                "Boot Unit `{}` requests an unavailable mount projection",
                unit.name
            )
        })?;
    for descriptor in &unit.descriptors {
        ensure!(
            namespace.resolve(&descriptor.path).is_ok(),
            "Boot Unit `{}` descriptor {} is outside its namespace",
            unit.name,
            descriptor.number
        );
    }
    Ok(namespace)
}

fn validate_process_cwd(namespace: &LiveNamespace, cwd: &str) -> Result<()> {
    ensure!(
        cwd == "/" || namespace.snapshot().resolve(cwd).is_ok(),
        "Agent Process cwd '{cwd}' is outside its namespace"
    );
    Ok(())
}

async fn read_clone_pid(server: &impl FileServer, fid: Fid) -> Result<Pid> {
    let pid = String::from_utf8(server.read(fid, 0, 64).await?)?
        .parse::<u64>()
        .context("Agent Process clone PID is invalid")?;
    Ok(Pid(pid))
}

async fn commit_clone(server: &impl FileServer, fid: Fid, exec: &ExecSpec) -> Result<()> {
    server
        .write(fid, 0, &serde_json::to_vec(exec)?)
        .await
        .context("write Agent Process exec spec")?;
    server
        .clunk(fid)
        .await
        .context("commit Agent Process through /proc/clone")
}

fn next_agent_fid() -> Fid {
    Fid(NEXT_AGENT_FID.fetch_add(1, Ordering::Relaxed))
}

async fn wait_for_root_stop(
    mut stop: tokio::sync::oneshot::Receiver<()>,
    controller: &RuntimeController,
) -> i32 {
    loop {
        tokio::select! {
            _ = &mut stop => return 0,
            _ = tokio::time::sleep(Duration::from_millis(25)) => {
                if controller.is_finished() {
                    return 1;
                }
            }
        }
    }
}

async fn wait_for_child_terminal(
    root: &InProcessTransport,
    pid: Pid,
    controller: &RuntimeController,
) -> Result<AgentExecutableResult> {
    let shell = alan_shell::Shell::new(root.clone());
    let activity_path = format!("/agent/{}/machine/ui/activity", pid.0);
    let events_path = format!("/agent/{}/machine/ui/events", pid.0);
    let notice_path = format!("/agent/{}/machine/ui/notice", pid.0);
    loop {
        if controller.is_finished() {
            anyhow::bail!("child Agent Machine stopped before publishing a terminal result");
        }
        let events_started = shell.stat(&events_path).await.map(|stat| stat.length > 0)?;
        if events_started {
            let activity: UiActivitySnapshot =
                serde_json::from_slice(&shell.cat(&activity_path).await?)?;
            if matches!(
                activity.state,
                UiActivityState::Idle | UiActivityState::Paused
            ) {
                let notice: UiNoticeSnapshot =
                    serde_json::from_slice(&shell.cat(&notice_path).await?)?;
                let output_text =
                    String::from_utf8(shell.cat(&format!("/agent/{}/io/output", pid.0)).await?)
                        .context("child Agent output is utf8")?;
                let warnings = (notice.kind == UiNoticeKind::Warning)
                    .then(|| notice.message.clone())
                    .into_iter()
                    .collect();
                if notice.kind == UiNoticeKind::Error {
                    return Ok(AgentExecutableResult::failed_with_output(
                        output_text,
                        warnings,
                        notice.message,
                    ));
                }
                if activity.state == UiActivityState::Paused {
                    let Some(pause) = read_child_pause(&shell, pid).await? else {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        continue;
                    };
                    return Ok(AgentExecutableResult::paused(output_text, warnings, pause));
                }
                return Ok(AgentExecutableResult::completed(output_text, warnings));
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn read_child_pause(
    shell: &alan_shell::Shell,
    pid: Pid,
) -> Result<Option<AgentExecutablePause>> {
    let requests_path = format!("/agent/{}/requests", pid.0);
    for request_id in shell.ls(&requests_path).await? {
        if matches!(request_id.as_str(), "clone" | "events") {
            continue;
        }
        let request_path = format!("{requests_path}/{request_id}");
        if shell.cat(&format!("{request_path}/status")).await? != b"pending" {
            continue;
        }
        let kind = String::from_utf8(shell.cat(&format!("{request_path}/kind")).await?)
            .context("child Agent request kind is utf8")?;
        let kind = match kind.as_str() {
            "confirmation" => YieldKind::Confirmation,
            "structured_input" => YieldKind::StructuredInput,
            other => YieldKind::Custom(other.to_string()),
        };
        return Ok(Some(AgentExecutablePause { request_id, kind }));
    }
    Ok(None)
}

async fn wait_for_process_exit(
    procfs: &alan_kernel::ProcFs,
    pid: Pid,
    timeout: Duration,
) -> Result<()> {
    tokio::time::timeout(timeout, async {
        loop {
            if procfs
                .try_observe_process_lifecycle(pid)
                .is_none_or(|(status, _)| status == alan_kernel::Status::Exited)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .context("timed out waiting for Agent Process exit")?;
    Ok(())
}

fn process_error(error: anyhow::Error) -> ProcessOutcome {
    let result = AgentExecutableResult::failed(format!("alan-agent: {error:#}"));
    ProcessOutcome::exited(
        1,
        result
            .to_process_output_record()
            .unwrap_or_else(|_| format!("alan-agent: {error:#}\n").into_bytes()),
    )
}

struct ProcessCleanup {
    service: Weak<AgentRuntimeService>,
    pid: Pid,
}

impl ProcessCleanup {
    fn new(service: Weak<AgentRuntimeService>, pid: Pid) -> Self {
        Self { service, pid }
    }
}

impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        let service = self.service.clone();
        let pid = self.pid;
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Some(service) = service.upgrade() {
                    if let Err(error) =
                        wait_for_process_exit(&service.procfs, pid, Duration::from_secs(12)).await
                    {
                        tracing::warn!(pid = pid.0, %error, "Agent Process cleanup deferred");
                        return;
                    }
                    service.release_process(pid).await;
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, LiveNamespace, Namespace};

    use super::{resolve_child_connection, validate_child_memory_mount, validate_process_cwd};

    #[test]
    fn process_cwd_must_be_reachable_after_service_projection() {
        let namespace = LiveNamespace::new(Namespace::new());
        assert!(validate_process_cwd(&namespace, "/").is_ok());
        assert!(validate_process_cwd(&namespace, "/mnt/review").is_err());

        namespace.mount(
            "/mnt/review",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        );
        assert!(validate_process_cwd(&namespace, "/mnt/review/src").is_ok());
    }

    #[test]
    fn child_connection_must_be_passed_by_the_parent() {
        assert_eq!(
            resolve_child_connection("parent-profile", None).unwrap(),
            "parent-profile"
        );
        assert_eq!(
            resolve_child_connection("parent-profile", Some("parent-profile")).unwrap(),
            "parent-profile"
        );
        let error = resolve_child_connection("parent-profile", Some("other-profile")).unwrap_err();
        assert!(error.to_string().contains("other-profile"));
        assert!(error.to_string().contains("parent-profile"));
    }

    #[test]
    fn child_memory_mount_requires_the_memory_handle() {
        let mut namespace = Namespace::new();
        namespace.mount(
            "/memory",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadWrite,
        );

        assert!(validate_child_memory_mount(&namespace, true, Some("/memory")).is_ok());
        let error = validate_child_memory_mount(&namespace, false, Some("/memory")).unwrap_err();
        assert!(error.to_string().contains("without the Memory handle"));
    }
}
