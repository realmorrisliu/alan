//! Namespace-native environment available at the accepted-submission transition boundary.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

mod agent_files;
mod client;
mod generation;
mod process_files;
mod tool_execution;

use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicU64},
};

use alan_ap::InProcessTransport;
use anyhow::Result;

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

/// Narrow file-native handle for one mounted LLM Connection.
#[derive(Clone)]
pub(crate) struct NamespaceGeneration {
    root: InProcessTransport,
    llm_connection: String,
}

/// Narrow handle for files owned by one AgentFS process layout.
#[derive(Clone)]
pub(crate) struct NamespaceAgentFiles {
    root: InProcessTransport,
    agent_path: String,
    input_offset: Arc<AtomicU64>,
    control_offset: Arc<AtomicU64>,
}

/// Narrow handle for lifecycle and stream files owned by the Process table.
#[derive(Clone)]
pub(crate) struct NamespaceProcessFiles {
    root: InProcessTransport,
    agent_path: String,
}

/// Narrow handle for Tool package discovery, policy capability, and execution.
#[derive(Clone)]
pub(crate) struct NamespaceToolExecution {
    root: InProcessTransport,
    process_files: NamespaceProcessFiles,
    agent_files: NamespaceAgentFiles,
    tool_process_context: Option<NamespaceToolProcessContext>,
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

    pub(crate) fn generation(&self) -> NamespaceGeneration {
        NamespaceGeneration {
            root: self.root.clone(),
            llm_connection: self.llm_connection.clone(),
        }
    }

    pub(crate) fn agent_files(&self) -> NamespaceAgentFiles {
        NamespaceAgentFiles {
            root: self.root.clone(),
            agent_path: self.agent_path.clone(),
            input_offset: Arc::clone(&self.input_offset),
            control_offset: Arc::clone(&self.control_offset),
        }
    }

    pub(crate) fn process_files(&self) -> NamespaceProcessFiles {
        NamespaceProcessFiles {
            root: self.root.clone(),
            agent_path: self.agent_path.clone(),
        }
    }

    pub(crate) fn tool_execution(&self) -> NamespaceToolExecution {
        NamespaceToolExecution {
            root: self.root.clone(),
            process_files: self.process_files(),
            agent_files: self.agent_files(),
            tool_process_context: self.tool_process_context.clone(),
        }
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
        self.environment
            .agent_files()
            .current_tape_checkpoint()
            .await
    }
}

#[cfg(test)]
mod tests;
