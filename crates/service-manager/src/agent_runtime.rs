//! Agent Runtime Service implementation of the `/bin/alan-agent` Process image.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use alan_agent_engine::{
    AGENT_DEFINITION_FD, AgentExecutableRequest, AgentExecutableStatus, ContentPart,
    MEMORY_STORE_FD, Op, RuntimeHandle, SpawnHandle, SpawnMountAccess, SpawnTarget, Submission,
    spawn_with_namespace_environment,
    tools::{ToolExecutionAuthority, ToolProcessRunner},
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{
    Access, ExecNamespaceManifest, ExecSpec, LiveNamespace, Pid, ProcessInvocation, ProcessOutcome,
};
use anyhow::{Context, Result, ensure};

use crate::{
    BootUnit, ConnectionService, HostMountService, ProcessLaunchContext,
    process_runner::SystemProcessRunner,
    runtime::{
        namespace_with_package_references, service_activation::ServiceActivationDeps,
        validate_package_reference_mounts,
    },
};

mod child_wait;
mod helpers;
#[path = "agent_runtime/history_discovery.rs"]
mod history_discovery;
mod pause_answer;
mod pause_record;
mod process_cleanup;
mod resume_content;
#[path = "agent_runtime/root_recovery.rs"]
mod root_recovery;

use helpers::{
    commit_clone, project_boot_unit_namespace, read_clone_pid, resolve_child_connection,
    validate_child_memory_mount, validate_process_cwd,
};
pub(crate) mod sessions;
mod template;

use child_wait::{
    process_error, wait_for_child_terminal, wait_for_process_exit, wait_for_root_stop,
};
use process_cleanup::ProcessCleanup;
use resume_content::resume_content;
pub(crate) use template::{RootAgentProcess, RootAgentTemplate};

const AGENT_EXECUTABLE: &str = "/bin/alan-agent";
static NEXT_AGENT_FID: AtomicU64 = AtomicU64::new(90_000);

pub(crate) struct AgentRuntimeService {
    procfs: alan_kernel::ProcFs,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    tool_runner: ToolProcessRunner,
    pending_roots: Mutex<HashMap<u64, PendingRootLaunch>>,
    session_stops: Mutex<sessions::SessionStopMap>,
    process_templates: Mutex<HashMap<u64, RootAgentTemplate>>,
    root_runtime_handles: Mutex<HashMap<u64, RuntimeHandle>>,
    current_root_rollout: Mutex<root_recovery::RootRolloutState>,
    last_root_template: Mutex<Option<RootAgentTemplate>>,
    active_root_pid: Mutex<Option<u64>>,
    next_root_rollout: Mutex<Option<history_discovery::PendingRootRolloutSwitch>>,
    history_bound: tokio::sync::OnceCell<()>,
    pub(crate) service_activation: std::sync::OnceLock<ServiceActivationDeps>,
    pub(crate) workspace_isolation_port:
        std::sync::OnceLock<Arc<dyn alan_agent_engine::WorkspaceIsolationPort>>,
}

struct PendingRootLaunch {
    template: RootAgentTemplate,
    namespace: LiveNamespace,
    ready: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
    stop: tokio::sync::oneshot::Receiver<()>,
    is_primary_root: bool,
    parent_for_host_mounts: Option<Pid>,
}

struct AgentLaunch {
    template: RootAgentTemplate,
    namespace: LiveNamespace,
    request: Option<AgentExecutableRequest>,
    ready: Option<tokio::sync::oneshot::Sender<std::result::Result<(), String>>>,
    stop: Option<tokio::sync::oneshot::Receiver<()>>,
    is_primary_root: bool,
    parent_for_host_mounts: Option<Pid>,
}

impl AgentRuntimeService {
    pub(crate) fn new(
        procfs: alan_kernel::ProcFs,
        agent_root: Arc<alan_agentfs::AgentRootFs>,
        host_mount: Arc<HostMountService>,
        connection: Arc<ConnectionService>,
        tool_runner: ToolProcessRunner,
    ) -> Arc<Self> {
        Arc::new(Self {
            procfs,
            agent_root,
            host_mount,
            connection,
            tool_runner,
            pending_roots: Mutex::new(HashMap::new()),
            session_stops: Mutex::new(HashMap::new()),
            process_templates: Mutex::new(HashMap::new()),
            root_runtime_handles: Mutex::new(HashMap::new()),
            current_root_rollout: Mutex::new(root_recovery::RootRolloutState::NotStarted),
            last_root_template: Mutex::new(None),
            active_root_pid: Mutex::new(None),
            next_root_rollout: Mutex::new(None),
            history_bound: tokio::sync::OnceCell::new(),
            service_activation: std::sync::OnceLock::new(),
            workspace_isolation_port: std::sync::OnceLock::new(),
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
        self.ensure_history_service_bound().await;
        let mut template = template.clone();
        template.task_budget_binding = Default::default();
        self.prepare_root_template_recovery(&mut template).await?;
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
                    is_primary_root: true,
                    parent_for_host_mounts: None,
                },
            );
        let descriptors = unit
            .descriptors
            .iter()
            .map(|descriptor| (descriptor.number, descriptor.path.clone()))
            .collect();
        let (namespace_snapshot, namespace_generation) = live_namespace.snapshot_with_generation();
        let exec = ExecSpec {
            executable: AGENT_EXECUTABLE.to_string(),
            args: Vec::new(),
            namespace: ExecNamespaceManifest::from_snapshot(
                &namespace_snapshot,
                namespace_generation,
            ),
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
            ready: Some(ready_rx),
            stop: Some(stop_tx),
        })
    }

    pub(crate) async fn detach_root(
        &self,
        mut root: RootAgentProcess,
        exit_code: i32,
    ) -> Result<()> {
        root.stop.take();
        self.procfs.record_exit(root.pid, exit_code).await;
        self.release_process(root.pid).await
    }

    pub(crate) async fn shutdown_root(&self, mut root: RootAgentProcess) -> Result<()> {
        if let Some(stop) = root.stop.take() {
            let _ = stop.send(());
        }
        wait_for_process_exit(&self.procfs, root.pid, Duration::from_secs(12)).await?;
        self.release_process(root.pid).await
    }

    pub(crate) async fn run_agent_process(
        self: &Arc<Self>,
        invocation: ProcessInvocation,
    ) -> ProcessOutcome {
        if invocation.exec.executable != AGENT_EXECUTABLE {
            return ProcessOutcome::exited(127, b"alan-agent: executable mismatch\n");
        }
        let mut cleanup = ProcessCleanup::new(Arc::downgrade(self), invocation.pid);
        let mut launch = match self.prepare_launch(&invocation) {
            Ok(launch) => launch,
            Err(error) => return process_error(error),
        };
        cleanup.bind_budget(launch.template.task_budget_binding.clone());
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
                is_primary_root: pending.is_primary_root,
                parent_for_host_mounts: pending.parent_for_host_mounts,
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
            is_primary_root: false,
            parent_for_host_mounts: Some(parent_pid),
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
                self.connection
                    .capture_connection(&launch.template.llm_connection)
                    .await?,
            )),
            Access::ReadWrite,
        );
        if let Some(parent) = launch.parent_for_host_mounts {
            let requested_mounts = launch
                .request
                .as_ref()
                .map(|request| request.spawn.host_mounts.as_slice())
                .unwrap_or(&[]);
            self.host_mount.register_child_process(
                parent,
                pid,
                launch.namespace.clone(),
                requested_mounts,
            )?;
        } else {
            self.host_mount
                .register_process(pid, launch.namespace.clone());
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
        self.agent_root
            .bind_process(pid.0.to_string(), agent.clone())
            .await;
        if launch.is_primary_root {
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
        let evaluator = if let Some(selection) = &launch.template.input_shadow {
            Some(
                self.connection
                    .capture_evaluation(&selection.profile, &launch.namespace)
                    .await?,
            )
        } else {
            None
        };
        let effective_core = alan_agent_engine::runtime::effective_core_config_for_runtime(
            &launch.template.process,
        )?;
        let mut environment = alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
            root.clone(),
            format!("/agent/{}", pid.0),
            launch.template.llm_connection.clone(),
        )
        .with_task_budget_binding(launch.template.task_budget_binding.clone())
        .with_evaluation_publisher({
            let agent = agent.clone();
            move |observation| {
                let agent = agent.clone();
                async move {
                    agent
                        .publish_evaluation_observation(observation)
                        .await
                        .map_err(Into::into)
                }
            }
        })
        .with_work_publisher({
            let agent = agent.clone();
            move |work| {
                let agent = agent.clone();
                async move {
                    if let Some(pending) = work.as_ref().and_then(|w| w.get("pending_request")) {
                        agent
                            .restore_pending_request(
                                pending["id"].as_str().context("work request lacks id")?,
                                "structured_input",
                                pending["prompt"]
                                    .as_str()
                                    .context("work request lacks prompt")?,
                                &serde_json::to_string(&pending["options"])?,
                            )
                            .await?;
                    }
                    agent.publish_work(work).await.map_err(Into::into)
                }
            }
        })
        .with_connection_authority(Arc::new(
            crate::connection::process_binding::ProcessConnection {
                service: self.connection.clone(),
                profile: launch.template.llm_connection.clone(),
                namespace: launch.namespace.clone(),
                evaluator: evaluator.clone(),
                guardian_profile: effective_core.guardian_profile,
                smart_evaluator_profile: effective_core.smart_evaluator_profile,
                allow_profile_switch: launch.is_primary_root
                    && launch.template.allow_profile_switch,
                selected_profile: std::sync::Mutex::new(None),
            },
        ))
        .with_namespace_cwd(&launch.template.launch_context.cwd)
        .with_tool_process_context(pid.0, self.tool_runner.clone());
        if launch.is_primary_root
            && let Some(deps) = self.service_activation.get()
        {
            environment = environment.with_service_activation_authority(deps.for_root(
                launch.namespace.clone(),
                launch.template.process.trust_posture,
            ));
        }
        if let Some(port) = self.workspace_isolation_port.get() {
            environment = environment.with_workspace_isolation_port(port.clone());
        }
        if let Some(selection) = &launch.template.input_shadow {
            let captured = evaluator.context("selected evaluator was not captured")?;
            environment = environment.with_shadow_evaluation(
                captured.root,
                captured.identity,
                selection.surface.clone(),
                30_000,
            )?;
        }
        let mut controller = spawn_with_namespace_environment(
            launch.template.process.clone(),
            environment,
            launch.template.host_capabilities.clone(),
            launch.template.generation_capabilities,
        )?;
        if launch.is_primary_root {
            *self
                .active_root_pid
                .lock()
                .expect("active root pid mutex poisoned") = Some(pid.0);
        }
        if launch.request.is_none() {
            self.root_runtime_handles
                .lock()
                .expect("Root runtime handles mutex poisoned")
                .insert(pid.0, controller.handle.clone());
        }
        let startup = controller
            .wait_until_ready()
            .await
            .context("Agent Machine failed to start")?;
        if launch.is_primary_root {
            if !launch.template.ephemeral_rollout {
                if !launch.template.no_resume_update {
                    root_recovery::publish(
                        &launch.template.process,
                        startup.rollout_path.as_deref(),
                    )?;
                }
                if let (Some(stores), Some(rollout_path)) = (
                    launch.template.process.store_bindings.as_ref(),
                    startup.rollout_path.as_deref().or(launch
                        .template
                        .process
                        .recovery_rollout_path
                        .as_deref()),
                ) {
                    let scope = self
                        .resolve_active_project_scope(
                            Some(pid.0),
                            &launch.template.process.namespace_cwd,
                        )
                        .await;
                    let _ = history_discovery::record_rollout_project_index(
                        stores,
                        rollout_path,
                        &scope,
                    );
                }
            }
            let rollout = startup
                .rollout_path
                .clone()
                .or_else(|| launch.template.process.recovery_rollout_path.clone());
            *self
                .current_root_rollout
                .lock()
                .expect("current Root rollout mutex poisoned") = rollout
                .map(root_recovery::RootRolloutState::Durable)
                .unwrap_or(root_recovery::RootRolloutState::NoDurableRollout);
        }
        let runtime_handle = controller.handle.clone();
        agent
            .set_retention_recorder(move |id, cause| {
                let handle = runtime_handle.clone();
                async move {
                    handle
                        .record_action_retention(&format!("/agent/{}", pid.0), &id, &cause)
                        .await
                        .map_err(|_| alan_ap::ErrorCode::Io)
                }
            })
            .await;
        self.process_templates
            .lock()
            .expect("process templates mutex poisoned")
            .insert(pid.0, launch.template.clone());
        if let Some(ready) = launch.ready.take() {
            let _ = ready.send(Ok(()));
        }

        let outcome = if let Some(request) = launch.request.as_ref() {
            let mut op = Op::Turn {
                parts: vec![ContentPart::text(request.initial_task.clone())],
                context: None,
            };
            if let Some(request_id) = request.resume.clone() {
                op = Op::Resume {
                    request_id,
                    content: resume_content(request),
                };
            }
            controller
                .handle
                .submission_tx
                .send(Submission::new(op))
                .await
                .context("submit initial child Agent Process turn")?;
            let result = wait_for_child_terminal(
                &root,
                pid,
                &controller,
                request.spawn.launch.timeout_secs.map(Duration::from_secs),
            )
            .await?;
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

    pub(crate) async fn release_process(&self, pid: Pid) -> Result<()> {
        let runtime_handle = self
            .root_runtime_handles
            .lock()
            .expect("Root runtime handles mutex poisoned")
            .get(&pid.0)
            .cloned();
        let recorder_result = if let Some(runtime_handle) = runtime_handle {
            let result = runtime_handle
                .finish_after_exit()
                .await
                .context("finish Root rollout recorder before releasing Process")
                .map_err(|error| {
                    tracing::warn!(pid = pid.0, %error, "Failed to finish Root rollout recorder");
                    error
                });
            if result.is_ok() {
                self.root_runtime_handles
                    .lock()
                    .expect("Root runtime handles mutex poisoned")
                    .remove(&pid.0);
            }
            result
        } else {
            Ok(())
        };
        self.process_templates
            .lock()
            .expect("process templates mutex poisoned")
            .remove(&pid.0);
        self.pending_roots
            .lock()
            .expect("pending roots mutex poisoned")
            .remove(&pid.0);
        self.session_stops
            .lock()
            .expect("session stops mutex poisoned")
            .remove(&pid.0);
        self.agent_root.unbind_process(&pid.0.to_string()).await;
        self.host_mount.unregister_process(pid);
        self.connection.release_process(pid.0);
        self.tool_runner.unregister_process(pid.0);
        recorder_result
    }

    fn register_tool_execution_binding(
        &self,
        pid: Pid,
        launch_context: &ProcessLaunchContext,
        scratch_dir: Option<PathBuf>,
    ) -> Result<()> {
        let scratch_dir = scratch_dir.unwrap_or_default();
        let binding = alan_agent_engine::tools::ToolExecutionBinding::awaiting_host_projection(
            Path::new(&launch_context.cwd).to_path_buf(),
            scratch_dir,
        )
        .with_optional_secret_resolver(launch_context.tool_secret_resolver.clone());
        let binding = self.host_mount.reconcile(pid.0, binding)?;
        self.tool_runner.register_process_binding(pid.0, binding);
        if !launch_context.packaged_tools.is_empty() {
            self.tool_runner
                .register_process_tools(pid.0, launch_context.packaged_tools.clone())?;
        }
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
        !spec.runtime_overrides.read_only
            || spec
                .host_mounts
                .iter()
                .all(|mount| mount.access == SpawnMountAccess::ReadOnly),
        "read-only child Agent Process cannot request read_write Host Mounts"
    );
    ensure!(
        invocation
            .exec
            .descriptors
            .keys()
            .all(|descriptor| matches!(*descriptor, AGENT_DEFINITION_FD | MEMORY_STORE_FD)),
        "child Agent Process contains an unsupported descriptor"
    );
    let definition_path = invocation.exec.descriptors.get(&AGENT_DEFINITION_FD);
    let definition = match &spec.target {
        SpawnTarget::DefinitionDescriptor { descriptor } => {
            let definition_path = definition_path
                .context("child Agent Process has no Agent Definition descriptor")?;
            let definition = parent
                .launch_context
                .descriptor(descriptor)
                .cloned()
                .with_context(|| format!("parent Process has no `{descriptor}` descriptor"))?;
            ensure!(
                definition.path == *definition_path,
                "child Agent Definition descriptor does not match SpawnSpec target"
            );
            Some(definition)
        }
        target @ SpawnTarget::PackageChildAgent { .. } => {
            let definition_path = definition_path
                .context("child Agent Process has no Agent Definition descriptor")?;
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
            let definition = alan_agent_engine::ProcessDescriptor::with_file_tree(
                export.root_dir.to_string_lossy(),
                file_tree,
            )?;
            ensure!(
                definition.path == *definition_path,
                "child Agent Definition descriptor does not match SpawnSpec target"
            );
            Some(definition)
        }
        SpawnTarget::AdHocSubagent { .. } => match definition_path {
            Some(definition_path) => {
                let definition = parent
                    .launch_context
                    .descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR)
                    .cloned()
                    .context("parent Process has no Agent Definition descriptor")?;
                ensure!(
                    definition.path == *definition_path,
                    "child Agent Definition descriptor does not match SpawnSpec target"
                );
                Some(definition)
            }
            None => None,
        },
    };

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
    if let Some(definition) = definition {
        launch_context.descriptors.insert(
            alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            definition,
        );
    }
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

    let mut process = parent.process.child_for_spawn(request);
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
    let task_budget_binding = match request.async_workstream_lease.as_deref() {
        Some(lease_id) => {
            ensure!(
                parent
                    .process
                    .agent_config
                    .runtime_config
                    .ad_hoc_subagent_spawn_enabled,
                "only Root Agent Process may launch async workstreams"
            );
            parent
                .task_budget_binding
                .child_for_async_workstream(lease_id)?
        }
        None => parent.task_budget_binding.child()?,
    };
    Ok(RootAgentTemplate {
        task_budget_binding,
        process,
        resume_persisted_rollout: false,
        resume_rollout_id: None,
        ephemeral_rollout: false,
        no_resume_update: false,
        input_shadow: None,
        launch_context,
        host_capabilities: alan_agent_engine::skills::build_skill_host_capabilities(
            tools,
            !matches!(spec.target, SpawnTarget::AdHocSubagent { .. })
                && !spec.runtime_overrides.read_only,
        ),
        generation_capabilities: alan_agent_engine::provider_capabilities_for_config(&effective),
        llm_connection,
        allow_profile_switch: false,
    })
}

fn next_agent_fid() -> Fid {
    Fid(NEXT_AGENT_FID.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests;
