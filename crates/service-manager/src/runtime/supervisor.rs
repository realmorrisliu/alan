//! Event-driven service supervision and service-handle lifecycle ownership.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use alan_ap::{FileServer, InProcessTransport};
use alan_kernel::{Credentials, ExitReason, LiveNamespace, Pid, Status};
use anyhow::{Context, Result, ensure};

use super::boot_setup::{
    SystemServiceHandles, open_proc_wait, publish_unit_handles, read_opened_wait_record,
    wait_unit_ready,
};
use crate::{
    BootManifest, BootUnit, ConnectionService, HostMountService, LocalEntryService, ManagerState,
    PackageService, RestartDecision, SupervisedHandle, SupervisionStrategy,
    agent_runtime::{AgentRuntimeService, RootAgentProcess, RootAgentTemplate},
    process_spawn::spawn_unit_process,
};

pub(super) const SERVICE_MANAGER_SRV_HANDLE: &str = "service-manager";

pub(super) fn create_supervised_handles(
    manifest: &BootManifest,
) -> BTreeMap<String, Arc<SupervisedHandle>> {
    let mut handles = BTreeMap::new();
    handles.insert(
        SERVICE_MANAGER_SRV_HANDLE.to_string(),
        SupervisedHandle::new(),
    );
    for handle in manifest
        .ordered()
        .flat_map(|unit| unit.published_handles.iter())
    {
        handles
            .entry(handle.clone())
            .or_insert_with(SupervisedHandle::new);
    }
    handles
}

#[derive(Clone, Copy)]
pub(super) struct ActiveUnit {
    pub(super) pid: Pid,
    pub(super) started_at: Instant,
}

struct UnitExitEvent {
    unit_name: String,
    pid: Pid,
    exit_code: i32,
    reason: ExitReason,
}

struct UnitWatcher {
    cancel: Option<tokio::sync::oneshot::Sender<()>>,
    _task: tokio::task::JoinHandle<()>,
}

impl Drop for UnitWatcher {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}

pub(super) struct SupervisorEnvironment {
    pub(super) root: RootAgentProcess,
    pub(super) root_template: RootAgentTemplate,
    pub(super) manifest: BootManifest,
    pub(super) state: Arc<tokio::sync::Mutex<ManagerState>>,
    pub(super) procfs: alan_kernel::ProcFs,
    pub(super) srvfs: Arc<alan_kernel::SrvFs>,
    pub(super) system_namespace: LiveNamespace,
    pub(super) agent_runtime: Arc<AgentRuntimeService>,
    pub(super) agent_root: Arc<alan_agentfs::AgentRootFs>,
    pub(super) llmfs: Arc<alan_llmfs::LlmFs>,
    pub(super) routefs: Arc<alan_routefs::RouteFs>,
    pub(super) host_mount: Arc<HostMountService>,
    pub(super) connection: Arc<ConnectionService>,
    pub(super) package: Arc<PackageService>,
    pub(super) supervised_handles: BTreeMap<String, Arc<SupervisedHandle>>,
    pub(super) active_units: BTreeMap<String, ActiveUnit>,
    pub(super) manager_pid: Pid,
    pub(super) local_entry: Arc<LocalEntryService>,
    pub(super) fileserver_manager: Arc<crate::FileserverManager>,
}

pub(super) struct SupervisorRuntime {
    manifest: BootManifest,
    state: Arc<tokio::sync::Mutex<ManagerState>>,
    procfs: alan_kernel::ProcFs,
    srvfs: Arc<alan_kernel::SrvFs>,
    system_namespace: LiveNamespace,
    manager_pid: Pid,
    active: BTreeMap<String, ActiveUnit>,
    pending: BTreeMap<String, Instant>,
    watchers: BTreeMap<String, UnitWatcher>,
    cascade_restarts: BTreeMap<String, Vec<String>>,
    exit_tx: tokio::sync::mpsc::UnboundedSender<UnitExitEvent>,
    exit_rx: Option<tokio::sync::mpsc::UnboundedReceiver<UnitExitEvent>>,
    agent_runtime: Arc<AgentRuntimeService>,
    agent_root: Arc<alan_agentfs::AgentRootFs>,
    llmfs: Arc<alan_llmfs::LlmFs>,
    routefs: Arc<alan_routefs::RouteFs>,
    host_mount: Arc<HostMountService>,
    connection: Arc<ConnectionService>,
    package: Arc<PackageService>,
    supervised_handles: BTreeMap<String, Arc<SupervisedHandle>>,
    local_entry: Arc<LocalEntryService>,
    root: Option<RootAgentProcess>,
    root_pid: Arc<AtomicU64>,
    root_template: RootAgentTemplate,
}

pub(super) enum SupervisorCommand {
    Open {
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
        reply: tokio::sync::oneshot::Sender<Result<(Pid, InProcessTransport)>>,
    },
    SpawnChild {
        parent_pid: Pid,
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
        reply: tokio::sync::oneshot::Sender<Result<(Pid, InProcessTransport)>>,
    },
    Close {
        pid: Pid,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    ResetRoot {
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
        reply: tokio::sync::oneshot::Sender<Result<Pid>>,
    },
}

async fn next_pending_sleep(pending: &BTreeMap<String, Instant>) {
    match pending.values().copied().min() {
        Some(deadline) => {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
        }
        None => std::future::pending::<()>().await,
    }
}

async fn watch_unit_exit(
    procfs: alan_kernel::ProcFs,
    unit_name: String,
    pid: Pid,
    exit_tx: tokio::sync::mpsc::UnboundedSender<UnitExitEvent>,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let (view, fid) = match open_proc_wait(&procfs, pid).await {
        Ok(opened) => opened,
        Err(error) => {
            tracing::warn!(
                unit = %unit_name,
                pid = pid.0,
                error = %error,
                "failed to open /proc/<pid>/wait for supervised unit"
            );
            return;
        }
    };
    tokio::select! {
        _ = &mut cancel_rx => {}
        result = read_opened_wait_record(&view, fid, pid) => {
            match result {
                Ok(record) => {
                    let _ = exit_tx.send(UnitExitEvent {
                        unit_name,
                        pid,
                        exit_code: record.exit_code,
                        reason: record.reason,
                    });
                }
                Err(error) => {
                    tracing::warn!(
                        unit = %unit_name,
                        pid = pid.0,
                        error = %error,
                        "supervised unit /proc/<pid>/wait stream failed"
                    );
                }
            }
        }
    }
    let _ = view.clunk(fid).await;
}

async fn run_supervisor(
    mut runtime: SupervisorRuntime,
    mut shutdown: tokio::sync::oneshot::Receiver<()>,
    mut commands: tokio::sync::mpsc::UnboundedReceiver<SupervisorCommand>,
    mut exits: tokio::sync::mpsc::UnboundedReceiver<UnitExitEvent>,
) -> Result<()> {
    let retry_notify = runtime.state.lock().await.retry_notifier();
    runtime.watch_all_active_units();
    runtime.drain_retry_requests().await;
    runtime.launch_due_units().await?;
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            cmd = commands.recv() => {
                if let Some(cmd) = cmd {
                    runtime.handle_command(cmd).await;
                }
            }
            Some(exit) = exits.recv() => {
                runtime.handle_unit_exit_event(exit).await?;
            }
            _ = retry_notify.notified() => {
                runtime.drain_retry_requests().await;
                runtime.launch_due_units().await?;
            }
            _ = next_pending_sleep(&runtime.pending) => {
                runtime.launch_due_units().await?;
            }
        }
    }
    runtime.stop().await
}

impl SupervisorRuntime {
    pub(super) fn from_assembled(
        assembled: SupervisorEnvironment,
        root_pid: Arc<AtomicU64>,
    ) -> Self {
        let (exit_tx, exit_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            manifest: assembled.manifest,
            state: assembled.state,
            procfs: assembled.procfs,
            srvfs: assembled.srvfs,
            system_namespace: assembled.system_namespace,
            manager_pid: assembled.manager_pid,
            active: assembled.active_units,
            pending: BTreeMap::new(),
            watchers: BTreeMap::new(),
            cascade_restarts: BTreeMap::new(),
            exit_tx,
            exit_rx: Some(exit_rx),
            agent_runtime: assembled.agent_runtime,
            agent_root: assembled.agent_root,
            llmfs: assembled.llmfs,
            routefs: assembled.routefs,
            host_mount: assembled.host_mount,
            connection: assembled.connection,
            package: assembled.package,
            supervised_handles: assembled.supervised_handles,
            local_entry: assembled.local_entry,
            root: Some(assembled.root),
            root_pid,
            root_template: assembled.root_template,
        }
    }

    pub(super) async fn settle_initial_root(&mut self) -> Result<()> {
        let initial_ready = self
            .root
            .as_mut()
            .context("Root Agent launch was not retained")?
            .wait_until_ready()
            .await;
        match initial_ready {
            Ok(_) => {
                self.root_template.resume_persisted_rollout = true;
                self.state
                    .lock()
                    .await
                    .mark_ready("root-agent")
                    .map_err(|error| anyhow::anyhow!("mark Root Agent ready: {error:?}"))
            }
            Err(error) => {
                let active = self
                    .active
                    .get("root-agent")
                    .copied()
                    .context("Root Agent launch was not tracked")?;
                self.procfs.record_exit(active.pid, 1).await;
                self.handle_exit("root-agent", active, 1, ExitReason::Failed)
                    .await?;
                self.state
                    .lock()
                    .await
                    .note_error("root-agent", format!("{error:#}"))
                    .map_err(|code| anyhow::anyhow!("record Root Agent boot error: {code:?}"))?;
                loop {
                    let Some(deadline) = self.pending.remove("root-agent") else {
                        anyhow::bail!(
                            "Root Agent exhausted boot restart budget: {}",
                            self.state
                                .lock()
                                .await
                                .unit("root-agent")
                                .and_then(|unit| unit.error)
                                .unwrap_or_else(|| format!("{error:#}"))
                        );
                    };
                    tokio::time::sleep(deadline.saturating_duration_since(Instant::now())).await;
                    match self.launch("root-agent").await {
                        Ok(()) => break,
                        Err(error) => self.handle_launch_failure("root-agent", error).await?,
                    }
                }
                Ok(())
            }
        }
    }

    pub(super) fn start(
        mut self,
    ) -> (
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::mpsc::UnboundedSender<SupervisorCommand>,
        tokio::task::JoinHandle<Result<()>>,
    ) {
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let (commands_tx, commands_rx) = tokio::sync::mpsc::unbounded_channel();
        let exits = self
            .exit_rx
            .take()
            .expect("SupervisorRuntime started more than once");
        let task = tokio::spawn(run_supervisor(self, shutdown_rx, commands_rx, exits));
        (shutdown, commands_tx, task)
    }

    fn watch_all_active_units(&mut self) {
        let units = self
            .active
            .iter()
            .map(|(name, active)| (name.clone(), active.pid))
            .collect::<Vec<_>>();
        for (name, pid) in units {
            if !self.watchers.contains_key(&name) {
                self.watch_unit(&name, pid);
            }
        }
    }

    fn watch_unit(&mut self, name: &str, pid: Pid) {
        self.watchers.remove(name);
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(watch_unit_exit(
            self.procfs.clone(),
            name.to_string(),
            pid,
            self.exit_tx.clone(),
            cancel_rx,
        ));
        self.watchers.insert(
            name.to_string(),
            UnitWatcher {
                cancel: Some(cancel_tx),
                _task: task,
            },
        );
    }

    async fn drain_retry_requests(&mut self) {
        for name in self.state.lock().await.take_retry_requests() {
            if !self.active.contains_key(&name) {
                self.pending.insert(name, Instant::now());
            }
        }
    }

    async fn handle_command(&mut self, cmd: SupervisorCommand) {
        match cmd {
            SupervisorCommand::Open { options, reply } => {
                let result = self.open_session(options).await;
                let _ = reply.send(result);
            }
            SupervisorCommand::SpawnChild {
                parent_pid,
                options,
                reply,
            } => {
                let result = self.spawn_child_session(parent_pid, options).await;
                let _ = reply.send(result);
            }
            SupervisorCommand::Close { pid, reply } => {
                let result = self.agent_runtime.close_session(pid).await;
                let _ = reply.send(result);
            }
            SupervisorCommand::ResetRoot { options, reply } => {
                let result = self.reset_root_session(options).await;
                let _ = reply.send(result);
            }
        }
    }

    async fn open_session(
        &self,
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
    ) -> Result<(Pid, InProcessTransport)> {
        let unit = self
            .manifest
            .get("root-agent")
            .cloned()
            .context("unknown Boot Unit `root-agent`")?;
        let template = self.root_template.for_session(&options, false)?;
        self.agent_runtime
            .launch_interactive_session(
                self.manager_pid,
                &self.system_namespace,
                &unit,
                &template,
                None,
            )
            .await
    }

    async fn spawn_child_session(
        &self,
        parent_pid: Pid,
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
    ) -> Result<(Pid, InProcessTransport)> {
        ensure!(
            matches!(
                self.procfs.try_observe_process_lifecycle(parent_pid),
                Some((Status::Running, _))
            ),
            "parent session Process {} is not active",
            parent_pid.0
        );
        let unit = self
            .manifest
            .get("root-agent")
            .cloned()
            .context("unknown Boot Unit `root-agent`")?;
        let template = self.root_template.for_session(&options, false)?;
        self.agent_runtime
            .launch_interactive_session(
                parent_pid,
                &self.system_namespace,
                &unit,
                &template,
                Some(parent_pid),
            )
            .await
    }

    async fn reset_root_session(
        &mut self,
        options: crate::agent_runtime::sessions::SessionLaunchOptions,
    ) -> Result<Pid> {
        self.watchers.remove("root-agent");
        self.active.remove("root-agent");
        self.pending.remove("root-agent");
        self.root_pid.store(0, Ordering::Release);
        if let Some(root) = self.root.take() {
            self.agent_runtime.shutdown_root(root).await?;
        }
        self.root_template = self.root_template.for_session(&options, true)?;
        let unit = self
            .manifest
            .get("root-agent")
            .cloned()
            .context("unknown Boot Unit `root-agent`")?;
        self.launch_root(&unit).await?;
        Ok(Pid(self.root_pid.load(Ordering::Acquire)))
    }

    async fn handle_unit_exit_event(&mut self, exit: UnitExitEvent) -> Result<()> {
        let Some(active) = self.active.get(&exit.unit_name).copied() else {
            return Ok(());
        };
        if active.pid != exit.pid {
            return Ok(());
        }
        self.handle_exit(&exit.unit_name, active, exit.exit_code, exit.reason)
            .await
    }

    async fn launch_due_units(&mut self) -> Result<()> {
        let now = Instant::now();
        let due = self
            .manifest
            .ordered()
            .filter(|unit| {
                self.pending
                    .get(&unit.name)
                    .is_some_and(|deadline| *deadline <= now)
            })
            .map(|unit| unit.name.clone())
            .collect::<Vec<_>>();
        for name in due {
            self.pending.remove(&name);
            match self.launch(&name).await {
                Ok(()) => {
                    if let Some(dependents) = self.cascade_restarts.remove(&name) {
                        self.restart_cascade_dependents(dependents).await?;
                    }
                }
                Err(error) => self.handle_launch_failure(&name, error).await?,
            }
        }
        Ok(())
    }

    async fn restart_cascade_dependents(&mut self, dependents: Vec<String>) -> Result<()> {
        for dep in dependents {
            if self.active.contains_key(&dep) {
                continue;
            }
            self.pending.remove(&dep);
            if let Err(error) = self.launch(&dep).await {
                self.handle_launch_failure(&dep, error).await?;
            }
        }
        Ok(())
    }

    async fn handle_exit(
        &mut self,
        name: &str,
        active: ActiveUnit,
        exit_code: i32,
        reason: ExitReason,
    ) -> Result<()> {
        self.watchers.remove(name);
        self.active.remove(name);
        self.invalidate_handles(name).await;
        if name == "root-agent" {
            let detach_result = if let Some(root) = self.root.take() {
                self.agent_runtime.detach_root(root, exit_code).await
            } else {
                Ok(())
            };
            self.root_pid.store(0, Ordering::Release);
            detach_result?;
        }
        let stable_for_ms =
            u64::try_from(active.started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let decision = self
            .state
            .lock()
            .await
            .record_exit_with_reason(name, exit_code, reason, stable_for_ms)
            .map_err(|error| anyhow::anyhow!("record `{name}` exit: {error:?}"))?;
        self.apply_restart_decision(name, decision).await
    }

    async fn handle_launch_failure(&mut self, name: &str, error: anyhow::Error) -> Result<()> {
        self.watchers.remove(name);
        let pid = self.state.lock().await.unit(name).and_then(|unit| unit.pid);
        if let Some(pid) = pid {
            self.procfs.record_exit(Pid(pid), 1).await;
            self.active.remove(name);
            if name == "root-agent" {
                self.root_pid.store(0, Ordering::Release);
                self.agent_runtime.release_process(Pid(pid)).await?;
            }
        }
        self.invalidate_handles(name).await;
        let mut state = self.state.lock().await;
        if pid.is_none() {
            state
                .start_failure(name, format!("{error:#}"))
                .map_err(|code| anyhow::anyhow!("track `{name}` launch failure: {code:?}"))?;
        }
        let decision = state
            .record_exit_with_reason(name, 1, ExitReason::Failed, 0)
            .map_err(|code| anyhow::anyhow!("record `{name}` launch failure: {code:?}"))?;
        state
            .note_error(name, format!("{error:#}"))
            .map_err(|code| anyhow::anyhow!("record `{name}` launch error: {code:?}"))?;
        drop(state);
        self.apply_restart_decision(name, decision).await
    }

    async fn apply_restart_decision(
        &mut self,
        name: &str,
        decision: RestartDecision,
    ) -> Result<()> {
        if let RestartDecision::RestartAfterMs(delay) = decision {
            if self.manifest.get(name).map(|unit| unit.supervision)
                == Some(SupervisionStrategy::RestForOne)
                && !self.cascade_restarts.contains_key(name)
            {
                self.prepare_rest_for_one_cascade(name).await?;
            }
            self.pending.insert(
                name.to_string(),
                Instant::now() + Duration::from_millis(delay),
            );
        }
        Ok(())
    }

    async fn prepare_rest_for_one_cascade(&mut self, trigger_unit: &str) -> Result<()> {
        let dependents = self.manifest.downstream_dependents(trigger_unit);
        if dependents.is_empty() {
            return Ok(());
        }
        for dep in dependents.iter().rev() {
            self.watchers.remove(dep);
            self.pending.remove(dep);
            let active = self.active.remove(dep);
            if dep == "root-agent" {
                self.root_pid.store(0, Ordering::Release);
                if let Some(root) = self.root.take() {
                    self.agent_runtime.shutdown_root(root).await?;
                } else if let Some(active) = active {
                    self.procfs.record_exit(active.pid, 0).await;
                }
            } else if let Some(active) = active {
                self.procfs.record_exit(active.pid, 0).await;
            }
            self.invalidate_handles(dep).await;
            self.state
                .lock()
                .await
                .record_cascade_stop(dep, trigger_unit)
                .map_err(|error| anyhow::anyhow!("record `{dep}` cascade stop: {error:?}"))?;
        }
        self.cascade_restarts
            .insert(trigger_unit.to_string(), dependents);
        Ok(())
    }

    async fn launch(&mut self, name: &str) -> Result<()> {
        let unit = self
            .manifest
            .get(name)
            .cloned()
            .with_context(|| format!("unknown Boot Unit `{name}`"))?;
        if name == "root-agent" {
            return self.launch_root(&unit).await;
        }

        let (pid, _) = spawn_unit_process(
            &self.procfs,
            self.manager_pid,
            &self.system_namespace,
            Credentials::system(),
            &unit,
            &[],
        )
        .await?;
        self.state
            .lock()
            .await
            .start_attempt(name, pid)
            .map_err(|error| anyhow::anyhow!("track `{name}` restart: {error:?}"))?;
        if name == "local-entry" {
            self.local_entry
                .set_service_pid(pid)
                .await
                .map_err(|error| anyhow::anyhow!("replace Local Entry Process: {error:?}"))?;
        }
        publish_unit_handles(
            &unit,
            &SystemServiceHandles {
                srvfs: &self.srvfs,
                agent_root: &self.agent_root,
                llmfs: &self.llmfs,
                routefs: &self.routefs,
                host_mount: &self.host_mount,
                connection: &self.connection,
                package: &self.package,
                supervised_handles: &self.supervised_handles,
                local_entry: Some(&self.local_entry),
            },
        )
        .await?;
        wait_unit_ready(&unit, pid, &self.procfs, &self.srvfs).await?;
        self.state
            .lock()
            .await
            .mark_ready(name)
            .map_err(|error| anyhow::anyhow!("mark `{name}` ready: {error:?}"))?;
        self.active.insert(
            name.to_string(),
            ActiveUnit {
                pid,
                started_at: Instant::now(),
            },
        );
        self.watch_unit(name, pid);
        Ok(())
    }

    async fn launch_root(&mut self, unit: &BootUnit) -> Result<()> {
        let mut root = self
            .agent_runtime
            .launch_root(
                self.manager_pid,
                &self.system_namespace,
                unit,
                &self.root_template,
            )
            .await?;
        let pid = root.pid();
        if let Err(error) = self.state.lock().await.start_attempt("root-agent", pid) {
            self.agent_runtime.detach_root(root, 1).await?;
            return Err(anyhow::anyhow!("track Root Agent restart: {error:?}"));
        }
        if let Err(error) = root.wait_until_ready().await {
            self.agent_runtime.detach_root(root, 1).await?;
            return Err(error).context("replacement Root Agent failed before readiness");
        }
        self.root_template.resume_persisted_rollout = true;
        self.state
            .lock()
            .await
            .mark_ready("root-agent")
            .map_err(|error| anyhow::anyhow!("mark Root Agent ready: {error:?}"))?;
        self.active.insert(
            unit.name.clone(),
            ActiveUnit {
                pid,
                started_at: Instant::now(),
            },
        );
        self.root_pid.store(pid.0, Ordering::Release);
        self.root = Some(root);
        self.watch_unit(&unit.name, pid);
        Ok(())
    }

    async fn invalidate_handles(&self, name: &str) {
        if let Some(unit) = self.manifest.get(name) {
            for handle in &unit.published_handles {
                if let Some(supervised) = self.supervised_handles.get(handle) {
                    supervised.deactivate();
                }
                self.srvfs.unpost(handle).await;
            }
        }
    }

    async fn stop_unit_for_shutdown(&mut self, name: &str) -> Result<()> {
        self.watchers.remove(name);
        let active = self.active.remove(name);
        let was_running = active.is_some() || (name == "root-agent" && self.root.is_some());
        if name == "root-agent" {
            self.root_pid.store(0, Ordering::Release);
            if let Some(root) = self.root.take() {
                self.agent_runtime.shutdown_root(root).await?;
            } else if let Some(active) = active {
                self.procfs.record_exit(active.pid, 0).await;
            }
        } else if let Some(active) = active {
            self.procfs.record_exit(active.pid, 0).await;
        }
        self.invalidate_handles(name).await;
        let _ = self.state.lock().await.record_stopped(name, was_running);
        Ok(())
    }

    async fn stop(mut self) -> Result<()> {
        self.watchers.clear();
        self.pending.clear();
        self.cascade_restarts.clear();

        let mut first_error = self.agent_runtime.shutdown_sessions().await.err();
        let stop_order = self
            .manifest
            .reverse_ordered()
            .map(|unit| unit.name.clone())
            .collect::<Vec<_>>();
        for name in stop_order {
            if let Err(err) = self.stop_unit_for_shutdown(&name).await
                && first_error.is_none()
            {
                first_error = Some(err);
            }
        }
        for (name, handle) in &self.supervised_handles {
            handle.deactivate();
            self.srvfs.unpost(name).await;
        }
        if let Some(err) = first_error {
            return Err(err);
        }
        Ok(())
    }
}
