//! Namespace-native environment available at the accepted-submission transition boundary.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

mod agent_files;
mod client;
mod generation;

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
};

use alan_ap::InProcessTransport;
use anyhow::{Context, Result, bail};
use tokio_util::sync::CancellationToken;

use self::client::NamespaceClient;
use crate::evidence::redact_durable_evidence_text;

/// Configuration for one namespace-native Agent Process turn driver.
#[derive(Debug, Clone)]
pub struct NamespaceTurnRuntimeConfig {
    /// Absolute AgentFS path for the process, e.g. `/agent/1`.
    pub agent_path: String,
    /// Connection name under `/mnt/llm/connections`.
    pub llm_connection: String,
    /// Optional system prompt serialized into the llmfs request document.
    pub system_prompt: Option<String>,
}

impl NamespaceTurnRuntimeConfig {
    pub fn new(agent_path: impl Into<String>, llm_connection: impl Into<String>) -> Self {
        Self {
            agent_path: agent_path.into(),
            llm_connection: llm_connection.into(),
            system_prompt: None,
        }
    }

    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceTurnOutput {
    /// User input frame consumed from `io/input`.
    pub input: String,
    /// Assistant text accumulated from the llmfs `events` stream.
    pub response: String,
    /// Generation directory allocated by `/mnt/llm/connections/<conn>/clone`.
    pub generation_id: String,
}

/// A yield/request record written by the engine under `requests/<id>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceRequestRecord {
    pub kind: String,
    pub prompt: String,
    pub options: Option<String>,
}

impl NamespaceRequestRecord {
    pub fn new(kind: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            prompt: prompt.into(),
            options: None,
        }
    }

    pub fn with_options(mut self, options: impl Into<String>) -> Self {
        self.options = Some(options.into());
        self
    }
}

/// A tool/action record written by the engine under `actions/<id>/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceActionRecord {
    pub name: String,
    pub status: String,
    pub output: Option<String>,
    pub result: Option<String>,
    pub approval: Option<String>,
    pub process: Option<String>,
}

impl NamespaceActionRecord {
    pub fn new(name: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            output: None,
            result: None,
            approval: None,
            process: None,
        }
    }

    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    pub fn with_approval(mut self, approval: impl Into<String>) -> Self {
        self.approval = Some(approval.into());
        self
    }

    pub fn with_process(mut self, process: impl Into<String>) -> Self {
        self.process = Some(process.into());
        self
    }
}

/// Result of one namespace-native tool action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceToolActionOutput {
    pub action_id: String,
    pub pid: String,
    pub output: String,
    pub exit_code: i32,
}

/// Access mode for an approved host mount grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovedMountGrantAccess {
    ReadOnly,
    ReadWrite,
}

impl ApprovedMountGrantAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
        }
    }
}

/// A host mount grant that has already passed the approval boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedMountGrant {
    pub namespace_path: String,
    pub host_path: PathBuf,
    pub access: ApprovedMountGrantAccess,
    pub reason: String,
}

impl ApprovedMountGrant {
    pub fn new(
        namespace_path: impl Into<String>,
        host_path: PathBuf,
        access: ApprovedMountGrantAccess,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            namespace_path: namespace_path.into(),
            host_path,
            access,
            reason: reason.into(),
        }
    }
}

/// Host-provided hook for applying approved mount grants to a live namespace.
///
/// The engine owns the approval flow, but the host composition root owns
/// host-backed file-server construction. Implementations must keep hostfs
/// dependencies out of `alan-agent-engine`. A successful call returns the full
/// post-application namespace snapshot so later child launches inherit the same
/// view without exposing Host file-server construction to the engine.
pub trait MountGrantApplicator: std::fmt::Debug + Send + Sync {
    fn apply_mount_grant(&self, grant: &ApprovedMountGrant) -> Result<alan_kernel::Namespace>;
}

/// Host-provided factory that can build a mount grant applicator once the engine
/// has created the live namespace handle for a runtime.
pub trait MountGrantApplicatorFactory: std::fmt::Debug + Send + Sync {
    fn create(
        &self,
        pid: alan_kernel::Pid,
        live_namespace: alan_kernel::LiveNamespace,
        inherited_mount_paths: &[String],
    ) -> Arc<dyn MountGrantApplicator>;

    fn tool_execution_authority(&self) -> Option<Arc<dyn crate::tools::ToolExecutionAuthority>> {
        None
    }
}

/// Result of attempting live namespace projection for an approved grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceMountApplication {
    pub namespace_applied: bool,
    pub namespace_error: Option<String>,
}

impl NamespaceMountApplication {
    pub fn applied() -> Self {
        Self {
            namespace_applied: true,
            namespace_error: None,
        }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            namespace_applied: false,
            namespace_error: Some(reason.into()),
        }
    }

    pub fn failed(error: anyhow::Error) -> Self {
        Self {
            namespace_applied: false,
            namespace_error: Some(error.to_string()),
        }
    }
}

/// Namespace-backed environment for an Agent Process.
#[derive(Clone)]
pub struct NamespaceRuntimeEnvironment {
    root: InProcessTransport,
    agent_path: String,
    llm_connection: String,
    tool_process_context: Option<NamespaceToolProcessContext>,
    input_offset: Arc<AtomicU64>,
    control_offset: Arc<AtomicU64>,
    mount_grant_applicator: Option<Arc<dyn MountGrantApplicator>>,
    child_run_registry: super::super::child_runs::ChildRunRegistry,
    child_process_assembler: Option<Arc<dyn super::super::ChildAgentProcessAssembler>>,
    launch_context: Option<crate::ProcessLaunchContext>,
}

#[derive(Clone)]
struct NamespaceToolProcessContext {
    pub(crate) pid: alan_kernel::Pid,
    pub(crate) tool_runner: crate::tools::ToolProcessRunner,
}

impl std::fmt::Debug for NamespaceRuntimeEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamespaceRuntimeEnvironment")
            .field("agent_path", &self.agent_path)
            .field("llm_connection", &self.llm_connection)
            .field(
                "mount_grant_applicator",
                &self.mount_grant_applicator.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl NamespaceRuntimeEnvironment {
    pub fn new(
        root: InProcessTransport,
        agent_path: impl Into<String>,
        llm_connection: impl Into<String>,
    ) -> Self {
        Self {
            root,
            agent_path: agent_path.into(),
            llm_connection: llm_connection.into(),
            tool_process_context: None,
            input_offset: Arc::new(AtomicU64::new(0)),
            control_offset: Arc::new(AtomicU64::new(0)),
            mount_grant_applicator: None,
            child_run_registry: super::super::child_runs::ChildRunRegistry::default(),
            child_process_assembler: None,
            launch_context: None,
        }
    }

    /// Bind the explicit Process Launch Context used for child execution.
    pub fn with_launch_context(mut self, launch_context: crate::ProcessLaunchContext) -> Self {
        self.launch_context = Some(launch_context);
        self
    }

    pub(crate) fn launch_context(&self) -> Option<&crate::ProcessLaunchContext> {
        self.launch_context.as_ref()
    }

    /// Bind transition-local Tool execution to its already-created Process.
    pub fn with_tool_process_context(
        mut self,
        pid: alan_kernel::Pid,
        tool_runner: crate::tools::ToolProcessRunner,
    ) -> Self {
        self.tool_process_context = Some(NamespaceToolProcessContext { pid, tool_runner });
        self
    }

    pub(crate) fn tool_execution_binding(&self) -> Option<crate::tools::ToolExecutionBinding> {
        let context = self.tool_process_context.as_ref()?;
        context.tool_runner.process_binding(context.pid)
    }

    pub(crate) fn resolve_tool_capability(
        &self,
        package: &super::super::ToolPackageManifest,
        arguments: &serde_json::Value,
    ) -> alan_agent_protocol::ToolCapability {
        if !package.capability_is_argument_dependent {
            return package.capability;
        }
        self.tool_process_context
            .as_ref()
            .and_then(|context| {
                context
                    .tool_runner
                    .capability_for_tool(&package.name, arguments)
            })
            .unwrap_or(alan_agent_protocol::ToolCapability::Unknown)
    }

    #[cfg(test)]
    pub(crate) fn set_tool_execution_binding(
        &self,
        binding: crate::tools::ToolExecutionBinding,
    ) -> bool {
        self.tool_process_context.as_ref().is_some_and(|context| {
            context
                .tool_runner
                .register_process_binding(context.pid, binding);
            true
        })
    }

    pub(crate) fn persist_approved_host_mount(&mut self, grant: crate::HostMountGrant) -> bool {
        let Some(context) = self.launch_context.as_mut() else {
            return false;
        };
        if let Some(index) = context
            .host_mounts
            .iter()
            .position(|existing| existing.namespace_path == grant.namespace_path)
        {
            let changed = context.host_mounts[index] != grant;
            context.host_mounts[index] = grant;
            return changed;
        }
        context.host_mounts.push(grant);
        true
    }

    pub(crate) fn sync_tool_execution_binding(&self, scratch_dir: PathBuf) -> bool {
        let Some(launch_context) = self.launch_context.as_ref() else {
            return false;
        };
        let Ok(binding) =
            crate::tools::ToolExecutionBinding::from_launch_context(launch_context, scratch_dir)
        else {
            return false;
        };
        let Some(process_context) = self.tool_process_context.as_ref() else {
            return false;
        };
        let changed = process_context
            .tool_runner
            .process_binding(process_context.pid)
            != Some(binding.clone());
        process_context
            .tool_runner
            .register_process_binding(process_context.pid, binding);
        changed
    }

    #[cfg(test)]
    pub(crate) fn tool_sandbox_writable_roots(&self) -> Vec<std::path::PathBuf> {
        self.tool_execution_binding()
            .and_then(|binding| binding.sandbox_spec)
            .map(|spec| spec.writable_roots)
            .unwrap_or_default()
    }

    pub fn with_mount_grant_applicator(
        mut self,
        applicator: Arc<dyn MountGrantApplicator>,
    ) -> Self {
        self.mount_grant_applicator = Some(applicator);
        self
    }

    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }

    /// Authoritative `/proc/<pid>` path corresponding to this AgentFS view.
    pub fn process_path(&self) -> Result<String> {
        Ok(format!("/proc/{}", agent_pid_from_path(&self.agent_path)?))
    }

    /// Process-local projection registry for delegated child Agent Processes.
    pub(crate) fn child_run_registry(&self) -> &super::super::child_runs::ChildRunRegistry {
        &self.child_run_registry
    }

    /// Bind the Process-scoped Agent Runtime Service capability used for child assembly.
    pub fn with_child_process_assembler(
        mut self,
        assembler: Arc<dyn super::super::ChildAgentProcessAssembler>,
    ) -> Self {
        self.child_process_assembler = Some(assembler);
        self
    }

    pub(crate) fn child_process_assembler(
        &self,
    ) -> Option<Arc<dyn super::super::ChildAgentProcessAssembler>> {
        self.child_process_assembler.clone()
    }

    pub fn llm_connection(&self) -> &str {
        &self.llm_connection
    }

    /// Discover model-callable Tools from complete packages visible in this namespace.
    pub(crate) async fn discover_tool_packages(
        &self,
    ) -> Result<Vec<super::super::ToolPackageManifest>> {
        let client = self.client();
        let mut packages = Vec::new();
        for name in client
            .try_read_directory_names("/bin")
            .await?
            .unwrap_or_default()
        {
            if name.is_empty() || name.contains('/') {
                continue;
            }
            let path = format!("/lib/exec/{name}/manifest");
            let Some(raw) = client.try_read_file(&path).await? else {
                continue;
            };
            let manifest: super::super::ToolPackageManifest = serde_json::from_slice(&raw)
                .with_context(|| format!("parse Tool manifest at {path}"))?;
            manifest.validate_for_name(&name)?;
            packages.push(manifest);
        }
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(packages)
    }

    pub fn root_transport(&self) -> InProcessTransport {
        self.root.clone()
    }

    pub fn apply_approved_mount_grant(
        &mut self,
        grant: &ApprovedMountGrant,
    ) -> NamespaceMountApplication {
        let Some(applicator) = self.mount_grant_applicator.clone() else {
            return NamespaceMountApplication::unavailable(
                "live namespace mount applicator unavailable",
            );
        };
        match applicator.apply_mount_grant(grant) {
            Ok(namespace) => {
                if let Some(context) = self.launch_context.as_mut() {
                    context.namespace = namespace;
                }
                NamespaceMountApplication::applied()
            }
            Err(error) => NamespaceMountApplication::failed(error),
        }
    }

    pub async fn write_process_control(&self, command: &str) -> Result<()> {
        let pid = agent_pid_from_path(&self.agent_path)?;
        self.write_process_control_for_pid(pid, command).await
    }

    pub async fn write_process_control_for_pid(&self, pid: &str, command: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        let ctl_path = format!("/proc/{pid}/ctl");
        client
            .write_document(&ctl_path, command.as_bytes())
            .await
            .with_context(|| format!("write process control command to {ctl_path}"))
    }

    /// Read terminal process state from authoritative `/proc`.
    pub(crate) async fn read_process_exit_code(&self, pid: &str) -> Result<Option<i32>> {
        let client = NamespaceClient::new(self.root.clone());
        let status_path = format!("/proc/{pid}/status");
        let status = String::from_utf8(
            client
                .read_file(&status_path)
                .await
                .with_context(|| format!("read process status from {status_path}"))?,
        )
        .context("process status is utf8")?;
        if status.trim() != "exited" {
            return Ok(None);
        }
        let exit_path = format!("/proc/{pid}/exit");
        let exit = String::from_utf8(
            client
                .read_file(&exit_path)
                .await
                .with_context(|| format!("read process exit from {exit_path}"))?,
        )
        .context("process exit is utf8")?;
        let code = exit
            .trim()
            .parse::<i32>()
            .with_context(|| format!("parse process exit code from {exit_path}"))?;
        Ok(Some(code))
    }

    pub(crate) async fn read_process_io_offsets(&self, pid: &str) -> Result<(u64, u64)> {
        let client = NamespaceClient::new(self.root.clone());
        let output_path = format!("/proc/{pid}/io/output");
        let events_path = format!("/proc/{pid}/io/events");
        let output = client
            .stat_path(&output_path)
            .await
            .with_context(|| format!("stat process output at {output_path}"))?
            .length;
        let events = client
            .stat_path(&events_path)
            .await
            .with_context(|| format!("stat process IO events at {events_path}"))?
            .length;
        Ok((output, events))
    }

    pub async fn spawn_process<I, S>(&self, executable: &str, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let client = NamespaceClient::new(self.root.clone());
        let args: Vec<String> = args.into_iter().map(Into::into).collect();
        let exec_spec = serde_json::json!({
            "executable": executable,
            "args": args,
        });
        let exec_spec = serde_json::to_vec(&exec_spec).context("serialize exec spec")?;
        client
            .clone_with_document("/proc/clone", &exec_spec)
            .await
            .with_context(|| format!("spawn {executable} through /proc/clone"))
    }

    pub async fn run_tool_action<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let cancel = CancellationToken::new();
        self.run_tool_action_with_cancel_and_timeout(tool_name, executable, args, &cancel, 30)
            .await
    }

    pub async fn run_tool_action_with_cancel<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.run_tool_action_with_cancel_and_timeout(tool_name, executable, args, cancel, 30)
            .await
    }

    pub async fn run_tool_action_with_cancel_and_timeout<I, S>(
        &self,
        tool_name: &str,
        executable: &str,
        args: I,
        cancel: &CancellationToken,
        timeout_secs: usize,
    ) -> Result<NamespaceToolActionOutput>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if cancel.is_cancelled() {
            bail!("tool process cancelled before spawn");
        }
        let pid = self.spawn_process(executable, args).await?;
        let result = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = self.write_process_control_for_pid(&pid, "cancel").await;
                bail!("tool process {pid} cancelled");
            }
            result = self.read_process_result(&pid, timeout_secs) => {
                match result {
                    Ok(result) => result,
                    Err(err) => {
                        let _ = self.write_process_control_for_pid(&pid, "cancel").await;
                        return Err(err).with_context(|| {
                            format!("read tool process {pid} result")
                        });
                    }
                }
            }
        };
        let action_exit_code = logical_tool_action_exit_code(&result);
        let action_status = if action_exit_code == 0 {
            "completed"
        } else {
            "failed"
        };
        let mut result_doc = serde_json::json!({
            "exit_code": action_exit_code,
        });
        if action_exit_code != result.exit_code
            && let Some(object) = result_doc.as_object_mut()
        {
            object.insert(
                "process_exit_code".to_string(),
                serde_json::json!(result.exit_code),
            );
        }
        let durable_output = redact_durable_evidence_text(&result.output);
        let action_id = self
            .write_action(
                NamespaceActionRecord::new(tool_name, action_status)
                    .with_output(durable_output.text)
                    .with_result(result_doc.to_string())
                    .with_approval("not_required")
                    .with_process(format!("/proc/{pid}")),
            )
            .await?;
        Ok(NamespaceToolActionOutput {
            action_id,
            pid,
            output: result.output,
            exit_code: action_exit_code,
        })
    }

    async fn read_process_result(
        &self,
        pid: &str,
        timeout_secs: usize,
    ) -> Result<NamespaceProcessResult> {
        if timeout_secs == 0 {
            return self.read_process_result_until_exit(pid).await;
        }
        tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs as u64),
            self.read_process_result_until_exit(pid),
        )
        .await
        .with_context(|| format!("timed out waiting {timeout_secs}s for process {pid} to exit"))?
    }

    async fn read_process_result_until_exit(&self, pid: &str) -> Result<NamespaceProcessResult> {
        let client = NamespaceClient::new(self.root.clone());
        let status_path = format!("/proc/{pid}/status");
        let exit_path = format!("/proc/{pid}/exit");
        let output_path = format!("/proc/{pid}/io/output");
        loop {
            let status = String::from_utf8(
                client
                    .read_file(&status_path)
                    .await
                    .with_context(|| format!("read {status_path}"))?,
            )
            .context("process status is not utf8")?;
            if status.trim() == "exited" {
                let exit_code = String::from_utf8(
                    client
                        .read_file(&exit_path)
                        .await
                        .with_context(|| format!("read {exit_path}"))?,
                )
                .context("process exit code is not utf8")?
                .trim()
                .parse::<i32>()
                .context("process exit code is not an integer")?;
                let output = if client
                    .stat_path(&output_path)
                    .await
                    .with_context(|| format!("stat {output_path}"))?
                    .length
                    == 0
                {
                    String::new()
                } else {
                    String::from_utf8(
                        client
                            .read_file(&output_path)
                            .await
                            .with_context(|| format!("read {output_path}"))?,
                    )
                    .context("process output is not utf8")?
                };
                return Ok(NamespaceProcessResult { output, exit_code });
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

struct NamespaceProcessResult {
    output: String,
    exit_code: i32,
}

fn logical_tool_action_exit_code(result: &NamespaceProcessResult) -> i32 {
    let trimmed = result.output.trim();
    if trimmed.is_empty() {
        return result.exit_code;
    }

    let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return result.exit_code;
    };
    let payload_exit_code = payload
        .get("exit_code")
        .and_then(serde_json::Value::as_i64)
        .and_then(|code| i32::try_from(code).ok());
    let payload_success = payload.get("success").and_then(serde_json::Value::as_bool);

    if matches!(payload_success, Some(false)) {
        return payload_exit_code
            .filter(|code| *code != 0)
            .unwrap_or(if result.exit_code != 0 {
                result.exit_code
            } else {
                1
            });
    }

    if let Some(exit_code) = payload_exit_code
        && exit_code != 0
    {
        return exit_code;
    }

    result.exit_code
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceLlmCapabilities {
    pub provider: String,
    pub capabilities: alan_llm::ProviderCapabilities,
}

/// A minimal namespace-native runtime for one agent process.
pub struct NamespaceTurnRuntime {
    environment: NamespaceRuntimeEnvironment,
    config: NamespaceTurnRuntimeConfig,
}

impl NamespaceTurnRuntime {
    pub fn new(root: InProcessTransport, config: NamespaceTurnRuntimeConfig) -> Self {
        let environment = NamespaceRuntimeEnvironment::new(
            root,
            config.agent_path.clone(),
            config.llm_connection.clone(),
        );
        Self {
            environment,
            config,
        }
    }

    /// Read the current root-hash checkpoint for this runtime's `machine/tape`.
    pub async fn current_tape_checkpoint(&self) -> Result<String> {
        self.environment.current_tape_checkpoint().await
    }
}

impl NamespaceRuntimeEnvironment {
    fn client(&self) -> NamespaceClient {
        NamespaceClient::new(self.root.clone())
    }
}

fn agent_pid_from_path(agent_path: &str) -> Result<&str> {
    let components = agent_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    match components.as_slice() {
        ["agent", pid] if *pid != "root" => Ok(*pid),
        ["agent", "root"] => {
            bail!("process control requires a concrete /agent/<pid> path, got /agent/root")
        }
        _ => bail!("invalid agent path for process control: {agent_path}"),
    }
}

#[cfg(test)]
mod tests;
