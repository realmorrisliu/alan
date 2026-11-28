use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alan_agent_engine::{
    AGENT_DEFINITION_DESCRIPTOR, AgentProcessConfig, ProcessDescriptor, ProcessFileTree,
    SystemPromptMode, ToolPolicyHook,
};
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{LiveNamespace, Pid};
use anyhow::{Context, Result, ensure};

use super::{
    AGENT_EXECUTABLE, AgentRuntimeService, PendingRootLaunch, RootAgentProcess, RootAgentTemplate,
    commit_clone, next_agent_fid, project_boot_unit_namespace, read_clone_pid,
    wait_for_process_exit,
};
use crate::BootUnit;
use crate::runtime::namespace_with_package_references;

/// Per-session overrides when opening, spawning a child of, or resetting an interactive session.
#[derive(Clone, Default)]
pub struct SessionLaunchOptions {
    /// Replace [`AgentProcessConfig`] for this session. When omitted, inherits from Root.
    pub process: Option<AgentProcessConfig>,
    /// Override [`SystemPromptMode`] for this session.
    pub system_prompt_mode: Option<SystemPromptMode>,
    /// Override default `response_schema` for this session (`Some(None)` clears inherited schema).
    pub response_schema: Option<Option<serde_json::Value>>,
    /// Override host [`ToolPolicyHook`] for this session.
    pub tool_policy_hook: Option<Arc<dyn ToolPolicyHook>>,
    /// Override the explicit Agent Definition descriptor tree for this session.
    pub definition: Option<ProcessFileTree>,
    /// Override the Alan OS workspace directory (`/mnt/...` or `/`) for this session.
    pub cwd: Option<String>,
    /// Override the Connection Service profile name for this session.
    pub llm_connection: Option<String>,
    /// Override whether this session keeps rollouts ephemeral (`true` by default for opened sessions).
    pub ephemeral_rollout: Option<bool>,
}

impl SessionLaunchOptions {
    /// Override the full [`AgentProcessConfig`] for this session.
    pub fn with_process(mut self, process: AgentProcessConfig) -> Self {
        self.process = Some(process);
        self
    }

    /// Override [`SystemPromptMode`] (`Append` or `Replace`) for this session.
    pub fn with_system_prompt_mode(mut self, mode: SystemPromptMode) -> Self {
        self.system_prompt_mode = Some(mode);
        self
    }

    /// Set or clear default JSON Schema structured output for this session.
    pub fn with_response_schema(mut self, schema: Option<serde_json::Value>) -> Self {
        self.response_schema = Some(schema);
        self
    }

    /// Override the synchronous [`ToolPolicyHook`] for this session.
    pub fn with_tool_policy_hook(mut self, hook: Arc<dyn ToolPolicyHook>) -> Self {
        self.tool_policy_hook = Some(hook);
        self
    }

    /// Override the Agent Definition file tree for this session.
    pub fn with_definition(mut self, definition: ProcessFileTree) -> Self {
        self.definition = Some(definition);
        self
    }

    /// Override the initial namespace working directory for this session.
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Override the LLM connection profile ID for this session.
    pub fn with_llm_connection(mut self, profile_id: impl Into<String>) -> Self {
        self.llm_connection = Some(profile_id.into());
        self
    }

    /// Control whether this session writes to durable rollouts (`false`) or stays ephemeral (`true`).
    pub fn with_ephemeral_rollout(mut self, ephemeral: bool) -> Self {
        self.ephemeral_rollout = Some(ephemeral);
        self
    }
}

impl RootAgentTemplate {
    pub(crate) fn for_session(
        &self,
        options: &SessionLaunchOptions,
        reset_root: bool,
    ) -> Result<Self> {
        let mut template = self.clone();
        template.task_budget_binding = Default::default();
        template.resume_persisted_rollout = false;
        template.resume_rollout_id = None;
        template.ephemeral_rollout = if reset_root {
            options.ephemeral_rollout.unwrap_or(self.ephemeral_rollout)
        } else {
            options.ephemeral_rollout.unwrap_or(true)
        };

        if let Some(custom_process) = &options.process {
            let mut process = custom_process.clone();
            if process.store_bindings.is_none() {
                process.store_bindings = self.process.store_bindings.clone();
            }
            if process.memory_store_backing.is_none() {
                process.memory_store_backing = self.process.memory_store_backing.clone();
            }
            process.memory_store_bound |= self.process.memory_store_bound;
            process.recovery_rollout_path = None;
            template.process = process;
        } else {
            template.process.recovery_rollout_path = None;
        }

        if let Some(mode) = &options.system_prompt_mode {
            template.process.system_prompt_mode = mode.clone();
        }
        if let Some(schema) = &options.response_schema {
            template.process.response_schema = schema.clone();
        }
        if let Some(hook) = &options.tool_policy_hook {
            template.process.tool_policy_hook = Some(hook.clone());
        }
        if let Some(tree) = &options.definition {
            template.launch_context = template.launch_context.clone().with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::with_file_tree("/lib/agents/root", tree.clone())?,
            );
        }
        if let Some(cwd) = &options.cwd {
            let trimmed = cwd.trim();
            ensure!(trimmed.starts_with('/'), "session cwd must start with '/'");
            template.launch_context.cwd = trimmed.to_string();
            template.process.namespace_cwd = std::path::PathBuf::from(trimmed);
        }
        if let Some(connection) = &options.llm_connection {
            template.llm_connection = connection.clone();
        }

        Ok(template)
    }
}

impl AgentRuntimeService {
    pub(crate) async fn launch_interactive_session(
        self: &Arc<Self>,
        parent_pid: Pid,
        system_namespace: &LiveNamespace,
        unit: &BootUnit,
        template: &RootAgentTemplate,
        parent_for_host_mounts: Option<Pid>,
    ) -> Result<(Pid, InProcessTransport)> {
        ensure!(
            unit.executable == AGENT_EXECUTABLE,
            "Agent session Boot Unit must execute {AGENT_EXECUTABLE}"
        );
        self.ensure_history_service_bound().await;
        let mut template = template.clone();
        template.task_budget_binding = Default::default();
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
            .context("walk Agent session /proc/clone")?;
        spawner
            .open(fid, OpenMode::ReadWrite)
            .await
            .context("open Agent session /proc/clone")?;
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
                    is_primary_root: false,
                    parent_for_host_mounts,
                },
            );
        let descriptors = unit
            .descriptors
            .iter()
            .map(|descriptor| (descriptor.number, descriptor.path.clone()))
            .collect();
        let (namespace_snapshot, namespace_generation) = live_namespace.snapshot_with_generation();
        let exec = alan_kernel::ExecSpec {
            executable: AGENT_EXECUTABLE.to_string(),
            args: Vec::new(),
            namespace: alan_kernel::ExecNamespaceManifest::from_snapshot(
                &namespace_snapshot,
                namespace_generation,
            ),
            descriptors,
            link_parent: false,
        };
        if let Err(error) = commit_clone(&spawner, fid, &exec).await {
            self.pending_roots
                .lock()
                .expect("pending roots mutex poisoned")
                .remove(&pid.0);
            return Err(error);
        }
        let session_ns = InProcessTransport::new(Arc::new(
            alan_kernel::MountFs::from_live_namespace(live_namespace.clone()),
        ));
        let mut handle = RootAgentProcess {
            pid,
            namespace: session_ns,
            ready: Some(ready_rx),
            stop: Some(stop_tx),
        };
        if let Err(error) = handle.wait_until_ready().await {
            handle.stop.take();
            self.procfs.record_exit(pid, 1).await;
            let _ = self.release_process(pid).await;
            return Err(error);
        }
        if let Some(stop) = handle.stop.take() {
            self.session_stops
                .lock()
                .expect("session stops mutex poisoned")
                .insert(pid.0, stop);
        }
        let client_namespace = LiveNamespace::new(live_namespace.snapshot().child());
        client_namespace.replace_mount(
            "/agent",
            InProcessTransport::new(
                self.agent_root
                    .actor_view(alan_agentfs::AgentActorView::Client),
            ),
            alan_kernel::Access::ReadWrite,
        );
        let client_ns = InProcessTransport::new(Arc::new(
            alan_kernel::MountFs::from_live_namespace(client_namespace),
        ));
        Ok((pid, client_ns))
    }

    pub(crate) async fn close_session(&self, pid: Pid) -> Result<()> {
        if *self
            .active_root_pid
            .lock()
            .expect("active root pid mutex poisoned")
            == Some(pid.0)
        {
            anyhow::bail!("cannot close primary Root Agent via close_session; use reset_session");
        }
        let stop = self
            .session_stops
            .lock()
            .expect("session stops mutex poisoned")
            .remove(&pid.0)
            .with_context(|| format!("unknown or already-closed Agent session {}", pid.0))?;
        let _ = stop.send(());
        wait_for_process_exit(&self.procfs, pid, Duration::from_secs(12)).await?;
        self.release_process(pid).await
    }

    pub(crate) async fn shutdown_sessions(&self) -> Result<()> {
        let drained: Vec<(u64, tokio::sync::oneshot::Sender<()>)> = {
            let mut map = self
                .session_stops
                .lock()
                .expect("session stops mutex poisoned");
            std::mem::take(&mut *map).into_iter().collect()
        };
        // Send all shutdown signals first so sessions stop concurrently, then collect cleanup.
        let mut pids = Vec::with_capacity(drained.len());
        for (pid, stop) in drained {
            let _ = stop.send(());
            pids.push(Pid(pid));
        }
        for pid in pids {
            wait_for_process_exit(&self.procfs, pid, Duration::from_secs(12)).await?;
            self.release_process(pid).await?;
        }
        Ok(())
    }
}

pub(super) type SessionStopMap = HashMap<u64, tokio::sync::oneshot::Sender<()>>;
