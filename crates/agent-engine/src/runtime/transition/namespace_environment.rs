//! Namespace-native environment available at the accepted-submission transition boundary.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

mod agent_files;
pub(crate) use agent_files::PendingActionPublication;
pub(crate) use agent_files::valid_project_directory;
mod tape_files;
pub(crate) use tape_files::NamespaceTapeWriter;
mod child_launch;
mod client;
mod decision;
mod evaluation;
mod event_streams;
mod evidence_read;
pub(crate) use decision::NamespaceDecisionConnection;
pub use decision::{NamespaceDecision, NamespaceDecisionFailure, NamespaceDecisionUncertainty};
pub(crate) use event_streams::{ChildEventWake, ChildEventWatchers};
pub(crate) use evidence_read::read_action_evidence;
mod generation;
pub use evaluation::{
    NamespaceEvaluation, NamespaceEvaluationFailure, NamespaceEvaluationUncertainty,
};
mod host_mount_requests;
mod operation_cleanup;
mod process_files;
pub(crate) use operation_cleanup::{NamespaceOperationCleanup, NamespaceOperationUncertainty};
mod tool_execution;
pub(crate) use tool_execution::PreparedToolAction;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamespaceToolObservationFailure {
    Timeout,
    ResultUnavailable,
    Cancelled,
}

impl NamespaceToolObservationFailure {
    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::Timeout => "timeout with unknown effects",
            Self::ResultUnavailable => "result unavailable with unknown effects",
            Self::Cancelled => "cancelled with unknown effects",
        }
    }
}

/// A bounded observation failure after a Tool Process was spawned.
#[derive(Debug, thiserror::Error)]
#[error("Tool Process {pid}: {}", category.reason())]
pub(crate) struct NamespaceToolProcessError {
    pub(crate) pid: String,
    pub(crate) category: NamespaceToolObservationFailure,
    pub(crate) action_id: Option<String>,
}

/// Correlation and human-approval evidence for one Tool Action.
#[derive(Clone, Copy)]
pub(crate) struct NamespaceToolActionEvidence<'a> {
    pub(crate) call_id: &'a str,
    pub(crate) approval: &'a str,
    pub(crate) arguments: &'a serde_json::Value,
    pub(crate) submission_id: Option<&'a str>,
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    pub(crate) task_budget_binding: crate::TaskBudgetBinding,
    pub(crate) operation_cleanup: NamespaceOperationCleanup,
    pub(crate) shadow_evaluation: Option<Arc<super::super::shadow_evaluation::ShadowEvaluation>>,
    pub(crate) work_publisher: Option<super::super::shadow_evaluation::EvaluationPublisher>,
    pub(crate) evaluation_publisher: Option<super::super::shadow_evaluation::EvaluationPublisher>,
    pub(crate) service_activation:
        Option<Arc<dyn super::super::service_activation::ServiceActivationAuthority>>,
    root: InProcessTransport,
    agent_path: String,
    llm_connection: String,
    namespace_cwd: PathBuf,
    tool_process_context: Option<NamespaceToolProcessContext>,
    input_offset: Arc<AtomicU64>,
    control_offset: Arc<AtomicU64>,
    posture_authority_revision: Arc<AtomicU64>,
    smart_auto_extensions_enabled: Arc<std::sync::atomic::AtomicBool>,
    submission_response_schemas:
        Arc<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
    action_recorder: Option<crate::rollout::RolloutRecorder>,
    child_run_registry: super::super::child_runs::ChildRunRegistry,
    async_workstreams: super::super::async_workstreams::AsyncWorkstreamRegistry,
    write_isolation_queue: super::super::write_isolation::SharedWriteQueueRegistry,
    workspace_isolation: Option<Arc<dyn super::super::write_isolation::WorkspaceIsolationPort>>,
    pub(crate) model_bindings:
        Arc<tokio::sync::Mutex<super::super::model_binding::ProcessBindings>>,
    // Active turn snapshot is independent of async catalog capture/selection. Never held across await.
    pub(crate) active_binding: Arc<
        std::sync::RwLock<
            Option<(
                super::super::model_binding::InputBinding,
                super::super::model_binding::CapturedCallable,
            )>,
        >,
    >,
}

/// Narrow file-native handle for one mounted LLM Connection.
#[derive(Clone)]
pub(crate) struct NamespaceGeneration {
    root: InProcessTransport,
    llm_connection: String,
    budget: Option<crate::agent_machine::task_budget::TaskBudget>,
    operation_cleanup: NamespaceOperationCleanup,
}

/// Narrow handle for files owned by one AgentFS process layout.
#[derive(Clone)]
pub(crate) struct NamespaceAgentFiles {
    root: InProcessTransport,
    agent_path: String,
    input_offset: Arc<AtomicU64>,
    control_offset: Arc<AtomicU64>,
    submission_response_schemas:
        Arc<std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>>,
    action_recorder: Option<crate::rollout::RolloutRecorder>,
}

impl NamespaceAgentFiles {
    pub(super) fn agent_path(&self) -> &str {
        &self.agent_path
    }

    pub(in crate::runtime) fn root_transport(&self) -> &InProcessTransport {
        &self.root
    }
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
    namespace_cwd: PathBuf,
    process_files: NamespaceProcessFiles,
}

/// Narrow handle for Tool package discovery, policy capability, and execution.
#[derive(Clone)]
pub(crate) struct NamespaceToolExecution {
    operation_cleanup: NamespaceOperationCleanup,
    root: InProcessTransport,
    process_files: NamespaceProcessFiles,
    agent_files: NamespaceAgentFiles,
    tool_process_context: Option<NamespaceToolProcessContext>,
}

/// Narrow handle for Tool authorization authority snapshots and dedicated Guardian/Smart capture.
#[derive(Clone)]
pub(crate) struct NamespaceToolAuthorization {
    model_bindings: Arc<tokio::sync::Mutex<super::super::model_binding::ProcessBindings>>,
    operation_cleanup: NamespaceOperationCleanup,
    tool_process_context: Option<NamespaceToolProcessContext>,
    posture_authority_revision: Arc<AtomicU64>,
}

impl NamespaceToolAuthorization {
    pub(crate) fn authority_revision(&self) -> Option<u64> {
        let context = self.tool_process_context.as_ref()?;
        context.tool_runner.authority_revision(context.pid)
    }

    pub(crate) fn posture_authority_revision(&self) -> u64 {
        self.posture_authority_revision
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(crate) fn git_push_delegations(&self) -> Vec<crate::policy::GitPushDelegationScope> {
        let Some(context) = &self.tool_process_context else {
            return Vec::new();
        };
        context.tool_runner.git_push_delegations(context.pid)
    }

    pub(crate) async fn capture_guardian_generation(
        &self,
        profile: &str,
    ) -> Result<NamespaceGeneration> {
        let authority = self
            .model_bindings
            .lock()
            .await
            .authority
            .clone()
            .ok_or_else(|| anyhow::anyhow!("guardian Connection authority unavailable"))?;
        let captured = authority.capture_guardian(profile).await?;
        Ok(NamespaceGeneration {
            root: captured.root,
            llm_connection: captured.connection,
            budget: None,
            operation_cleanup: self.operation_cleanup.clone(),
        })
    }
}

#[derive(Clone)]
struct NamespaceToolProcessContext {
    pub(crate) pid: u64,
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
    /// Per-input captures follow the Machine's acknowledged binding disposition.
    /// Never infer settlement from UI publication or from admitted-ID history.
    pub(crate) async fn reconcile_input_captures(
        &self,
        queue: &Arc<std::sync::Mutex<crate::agent_machine::input_queue::MachineInputQueue>>,
    ) {
        let mut bindings = self.model_bindings.lock().await;
        let queue = queue.lock().expect("input queue poisoned");
        bindings
            .captured
            .retain(|id, _| queue.bindings.contains_key(id));
    }
    pub fn new(
        root: InProcessTransport,
        agent_path: impl Into<String>,
        llm_connection: impl Into<String>,
    ) -> Self {
        Self {
            task_budget_binding: Default::default(),
            operation_cleanup: Default::default(),
            shadow_evaluation: None,
            evaluation_publisher: None,
            work_publisher: None,
            service_activation: None,
            root,
            agent_path: agent_path.into(),
            llm_connection: llm_connection.into(),
            namespace_cwd: PathBuf::from("/"),
            model_bindings: Default::default(),
            active_binding: Default::default(),
            tool_process_context: None,
            input_offset: Arc::new(AtomicU64::new(0)),
            control_offset: Arc::new(AtomicU64::new(0)),
            posture_authority_revision: Arc::new(AtomicU64::new(0)),
            smart_auto_extensions_enabled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            submission_response_schemas: Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            action_recorder: None,
            child_run_registry: super::super::child_runs::ChildRunRegistry::default(),
            async_workstreams: super::super::async_workstreams::AsyncWorkstreamRegistry::new(),
            write_isolation_queue: super::super::write_isolation::SharedWriteQueueRegistry::new(),
            workspace_isolation: None,
        }
    }

    /// Attach a [`crate::runtime::write_isolation::WorkspaceIsolationPort`] for `worktree` / `cow_workspace` tiered write isolation.
    pub fn with_workspace_isolation_port(
        mut self,
        port: Arc<dyn super::super::write_isolation::WorkspaceIsolationPort>,
    ) -> Self {
        self.workspace_isolation = Some(port);
        self
    }

    /// Bind trusted Process ancestry supplied by Agent Runtime Service.
    pub fn with_task_budget_binding(mut self, binding: crate::TaskBudgetBinding) -> Self {
        self.task_budget_binding = binding;
        self
    }

    /// Bind the Alan OS working directory selected by Agent Runtime Service.
    pub fn with_namespace_cwd(mut self, namespace_cwd: impl Into<PathBuf>) -> Self {
        self.namespace_cwd = namespace_cwd.into();
        self
    }

    pub(crate) fn change_process_directory(&mut self, path: &std::path::Path) -> Result<PathBuf> {
        let namespace_cwd = self.tool_execution().change_process_directory(path)?;
        self.namespace_cwd = namespace_cwd.clone();
        Ok(namespace_cwd)
    }

    /// Attach the Connection-owned authority scoped to this Process profile.
    pub fn with_connection_authority(
        self,
        authority: Arc<dyn super::super::model_binding::ConnectionAuthority>,
    ) -> Self {
        self.model_bindings
            .try_lock()
            .expect("new environment")
            .authority = Some(authority);
        self
    }

    /// Attach the Root-only host service-activation authority.
    pub fn with_service_activation_authority(
        mut self,
        authority: Arc<dyn super::super::service_activation::ServiceActivationAuthority>,
    ) -> Self {
        self.service_activation = Some(authority);
        self
    }

    pub(crate) fn service_activation(
        &self,
    ) -> Option<&Arc<dyn super::super::service_activation::ServiceActivationAuthority>> {
        self.service_activation.as_ref()
    }

    pub(crate) fn generation(&self) -> NamespaceGeneration {
        if let Some((_, callable)) = &*self.active_binding.read().expect("active binding snapshot")
        {
            return NamespaceGeneration {
                root: callable.root.clone(),
                llm_connection: callable.connection.clone(),
                budget: None,
                operation_cleanup: self.operation_cleanup.clone(),
            };
        }
        NamespaceGeneration {
            root: self.root.clone(),
            llm_connection: self.llm_connection.clone(),
            budget: None,
            operation_cleanup: self.operation_cleanup.clone(),
        }
    }

    pub(crate) fn with_action_recorder(
        mut self,
        recorder: Option<crate::rollout::RolloutRecorder>,
    ) -> Self {
        self.action_recorder = recorder;
        self
    }

    pub(crate) fn take_submission_response_schema(
        &self,
        submission_id: &str,
    ) -> Option<serde_json::Value> {
        self.submission_response_schemas
            .lock()
            .expect("submission response schema mutex poisoned")
            .remove(submission_id)
    }

    pub(crate) fn agent_files(&self) -> NamespaceAgentFiles {
        NamespaceAgentFiles {
            root: self.root.clone(),
            agent_path: self.agent_path.clone(),
            input_offset: Arc::clone(&self.input_offset),
            control_offset: Arc::clone(&self.control_offset),
            submission_response_schemas: Arc::clone(&self.submission_response_schemas),
            action_recorder: self.action_recorder.clone(),
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
            namespace_cwd: self.namespace_cwd.clone(),
            process_files: self.process_files(),
        }
    }

    pub(crate) fn tool_execution(&self) -> NamespaceToolExecution {
        NamespaceToolExecution {
            operation_cleanup: self.operation_cleanup.clone(),
            root: self.root.clone(),
            process_files: self.process_files(),
            agent_files: self.agent_files(),
            tool_process_context: self.tool_process_context.clone(),
        }
    }

    pub(crate) fn tool_authorization(&self) -> NamespaceToolAuthorization {
        NamespaceToolAuthorization {
            model_bindings: Arc::clone(&self.model_bindings),
            operation_cleanup: self.operation_cleanup.clone(),
            tool_process_context: self.tool_process_context.clone(),
            posture_authority_revision: Arc::clone(&self.posture_authority_revision),
        }
    }

    /// Update the invocation's active trust posture on the attached Tool Process runner.
    pub(crate) fn set_trust_posture(&self, trust_posture: crate::policy::TrustPosture) {
        self.smart_auto_extensions_enabled.store(
            trust_posture == crate::policy::TrustPosture::ConfinedSmart,
            std::sync::atomic::Ordering::Release,
        );
        if let Some(context) = &self.tool_process_context {
            context
                .tool_runner
                .register_process_trust_posture(context.pid, trust_posture);
        }
    }

    pub(crate) fn smart_auto_extensions_enabled(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.smart_auto_extensions_enabled)
    }

    /// Bump the monotonic posture authority revision when downgrading trust posture
    /// so subsequent authorization checks observe a fresh authority epoch.
    pub(crate) fn bump_posture_authority_revision(&self) -> u64 {
        self.posture_authority_revision
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            .saturating_add(1)
    }

    /// Bind the invocation's explicit trust posture on the attached Tool Process runner.
    pub(crate) fn with_trust_posture(self, trust_posture: crate::policy::TrustPosture) -> Self {
        self.set_trust_posture(trust_posture);
        self
    }

    /// Read bounded non-authoritative `AGENTS.md`/`CLAUDE.md` knowledge from explicitly
    /// mounted workspace roots in this Process namespace.
    pub(crate) async fn read_project_knowledge_section(&self) -> Option<String> {
        let ctx = self.tool_process_context.as_ref()?;
        let mut roots = ctx.tool_runner.project_knowledge_roots(ctx.pid);
        if roots.is_empty() {
            // If an authoritative HostMountService returned empty, grants are inactive/revoked.
            if ctx.tool_runner.authority_revision(ctx.pid).is_some() {
                return None;
            }
            let binding = self.tool_execution().execution_binding()?;
            if !binding.has_adapter() {
                return None;
            }
            let cwd = binding.namespace_cwd;
            if cwd.starts_with("/mnt/") && !cwd.starts_with("/mnt/llm") {
                roots.push(cwd);
            }
        }
        if roots.is_empty() {
            return None;
        }
        let client = client::NamespaceClient::new(self.root.clone());
        let mut seen = std::collections::BTreeSet::new();
        let mut docs = Vec::new();
        for root in roots {
            if !seen.insert(root.clone()) {
                continue;
            }
            for filename in crate::prompts::project_knowledge::PROJECT_KNOWLEDGE_FILENAMES {
                let ns_path = root.join(filename);
                let ns_str = ns_path.to_string_lossy();
                let Ok(Some(raw)) = client
                    .try_read_file_bounded(
                        &ns_str,
                        crate::prompts::project_knowledge::PROJECT_KNOWLEDGE_MAX_FILE_BYTES,
                    )
                    .await
                else {
                    continue;
                };
                if let Some((content, truncated)) =
                    crate::prompts::project_knowledge::bound_project_knowledge_bytes(&raw)
                {
                    docs.push(
                        crate::prompts::project_knowledge::ProjectKnowledgeDocument {
                            filename: (*filename).to_string(),
                            namespace_path: ns_str.into_owned(),
                            content,
                            truncated,
                        },
                    );
                }
            }
        }
        crate::prompts::project_knowledge::render_project_knowledge_section(&docs)
    }

    /// Bind transition-local Tool execution to its already-created Process.
    pub fn with_tool_process_context(
        mut self,
        pid: u64,
        tool_runner: crate::tools::ToolProcessRunner,
    ) -> Self {
        self.tool_process_context = Some(NamespaceToolProcessContext { pid, tool_runner });
        self
    }

    pub(crate) fn llm_connection_name(&self) -> &str {
        &self.llm_connection
    }

    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }

    /// Process-local projection registry for delegated child Agent Processes.
    pub(crate) fn child_run_registry(&self) -> &super::super::child_runs::ChildRunRegistry {
        &self.child_run_registry
    }

    /// Process-local registry for in-instance asynchronously running child workstreams.
    pub fn async_workstreams(&self) -> &super::super::async_workstreams::AsyncWorkstreamRegistry {
        &self.async_workstreams
    }

    /// Per-grant FIFO queue registry serializing `shared_queue` writers.
    pub fn write_isolation_queue(
        &self,
    ) -> &super::super::write_isolation::SharedWriteQueueRegistry {
        &self.write_isolation_queue
    }

    /// Optional Host-agnostic CoW / `git worktree` isolation port.
    pub fn workspace_isolation(
        &self,
    ) -> Option<&Arc<dyn super::super::write_isolation::WorkspaceIsolationPort>> {
        self.workspace_isolation.as_ref()
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
