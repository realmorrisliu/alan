//! Namespace-native environment available at the accepted-submission transition boundary.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

mod agent_files;
mod child_launch;
mod client;
mod generation;
mod host_mount_requests;
mod process_files;
mod tool_execution;

use std::sync::{Arc, atomic::AtomicU64};

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

/// Namespace-backed environment for an Agent Process.
#[derive(Clone)]
pub struct NamespaceRuntimeEnvironment {
    root: InProcessTransport,
    agent_path: String,
    llm_connection: String,
    tool_process_context: Option<NamespaceToolProcessContext>,
    input_offset: Arc<AtomicU64>,
    control_offset: Arc<AtomicU64>,
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

/// Narrow aP handle for logical Host Mount Service requests and status.
#[derive(Clone)]
pub(crate) struct NamespaceHostMountRequests {
    root: InProcessTransport,
}

pub(crate) use host_mount_requests::{HostMountTerminalResult, HostMountTerminalStatus};

/// Narrow handle for child Agent Process launch capabilities.
#[derive(Clone)]
pub(crate) struct NamespaceChildLaunch {
    llm_connection: String,
    launch_context: Option<crate::ProcessLaunchContext>,
    child_process_assembler: Option<Arc<dyn super::super::ChildAgentProcessAssembler>>,
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

    pub(crate) fn host_mount_requests(&self) -> NamespaceHostMountRequests {
        NamespaceHostMountRequests {
            root: self.root.clone(),
        }
    }

    pub(crate) fn child_launch(&self) -> NamespaceChildLaunch {
        NamespaceChildLaunch {
            llm_connection: self.llm_connection.clone(),
            launch_context: self.launch_context.clone(),
            child_process_assembler: self.child_process_assembler.clone(),
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

    /// Bind transition-local Tool execution to its already-created Process.
    pub fn with_tool_process_context(
        mut self,
        pid: alan_kernel::Pid,
        tool_runner: crate::tools::ToolProcessRunner,
    ) -> Self {
        self.tool_process_context = Some(NamespaceToolProcessContext { pid, tool_runner });
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

    pub fn root_transport(&self) -> InProcessTransport {
        self.root.clone()
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
