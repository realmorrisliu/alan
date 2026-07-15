//! Namespace-native environment owned by the agent loop.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

mod client;

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_agent_protocol::{
    ContentPart, Event, InputMode, Op, Submission, UiActivitySnapshot, UiEvent, UiNoticeSnapshot,
    UiPlanSnapshot, UiThinkingSnapshot,
};
use alan_ap::{Fid, InProcessTransport, OpenMode};
use alan_llm::{GenerationRequest, GenerationResponse};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use self::client::{InputFrame, NamespaceClient};
use crate::evidence::{
    EvidenceResolutionError, EvidenceResolutionErrorCode, NamespaceEvidenceReference,
    is_retention_expired_record, redact_durable_evidence_text,
};

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

    pub async fn read_llm_connection_capabilities(&self) -> Result<NamespaceLlmCapabilities> {
        let path = format!("/mnt/llm/connections/{}/capabilities", self.llm_connection);
        let client = self.client();
        let raw = client
            .read_file(&path)
            .await
            .with_context(|| format!("read llm connection capabilities from {path}"))?;
        let doc: LlmCapabilitiesDoc =
            serde_json::from_slice(&raw).context("parse llm connection capabilities")?;
        if doc.version != 1 {
            bail!("unsupported llm capabilities version {}", doc.version);
        }
        if doc.connection != self.llm_connection {
            bail!(
                "llm capabilities connection mismatch: expected {}, got {}",
                self.llm_connection,
                doc.connection
            );
        }
        Ok(NamespaceLlmCapabilities {
            provider: doc.provider,
            capabilities: doc.capabilities,
        })
    }

    pub async fn read_next_input(&self) -> Result<String> {
        let input_path = format!("{}/io/input", self.agent_path);
        let client = self.client();
        let offset = self.input_offset.load(Ordering::Relaxed);
        let raw = client
            .read_stream_from(&input_path, offset)
            .await
            .with_context(|| format!("read input from {input_path}"))?;
        let frame = InputFrame::parse_one(&raw).context("parse agent io/input frame")?;
        self.input_offset
            .fetch_add(frame.bytes_consumed as u64, Ordering::Relaxed);
        Ok(frame.message)
    }

    pub async fn read_next_input_submission(&self, mode: InputMode) -> Result<Submission> {
        let message = self.read_next_input().await?;
        Ok(Submission::new(Op::Input {
            parts: vec![ContentPart::text(message)],
            mode,
        }))
    }

    pub async fn read_next_machine_control_submission(&self) -> Result<Option<Submission>> {
        let events_path = format!("{}/events", self.agent_path);
        let client = self.client();
        let offset = self.control_offset.load(Ordering::Relaxed);
        let stat = client
            .stat_path(&events_path)
            .await
            .with_context(|| format!("stat agent events from {events_path}"))?;
        if stat.length <= offset {
            return Ok(None);
        }

        let raw = client
            .read_file_range(&events_path, offset, stat.length - offset)
            .await
            .with_context(|| format!("read agent events from {events_path}"))?;
        let mut consumed = 0_u64;

        for line in raw.split_inclusive(|byte| *byte == b'\n') {
            if !line.ends_with(b"\n") {
                break;
            }
            consumed += line.len() as u64;
            let record = String::from_utf8(line[..line.len() - 1].to_vec())
                .context("agent events record is not utf8")?;
            if let Some(command) = record.strip_prefix("ctl:") {
                self.control_offset
                    .store(offset + consumed, Ordering::Relaxed);
                if let Some(submission) = machine_control_submission(command) {
                    return Ok(Some(submission));
                }
            }
        }

        self.control_offset
            .store(offset + consumed, Ordering::Relaxed);
        Ok(None)
    }

    pub async fn resume_submission_from_answered_request(
        &self,
        request_id: &str,
    ) -> Result<Option<Submission>> {
        let Some(response) = self.read_answered_request_response(request_id).await? else {
            return Ok(None);
        };
        Ok(Some(Submission::new(Op::Resume {
            request_id: request_id.to_string(),
            content: vec![request_response_content_part(response)],
        })))
    }

    pub async fn read_answered_request_response(&self, request_id: &str) -> Result<Option<String>> {
        validate_agent_file_id(request_id, "request id")?;
        let client = self.client();
        let request_path = format!("{}/requests/{request_id}", self.agent_path);
        let status_path = format!("{request_path}/status");
        let Some(status) = client
            .try_read_file(&status_path)
            .await
            .with_context(|| format!("read request status from {status_path}"))?
        else {
            return Ok(None);
        };
        let status = String::from_utf8(status).context("request status is not utf8")?;
        if status.trim() != "answered" {
            return Ok(None);
        }
        let response_path = format!("{request_path}/response");
        let Some(response) = client
            .try_read_file(&response_path)
            .await
            .with_context(|| format!("read request response from {response_path}"))?
        else {
            return Ok(None);
        };
        let response = String::from_utf8(response).context("request response is not utf8")?;
        Ok(Some(response))
    }

    pub async fn generate(&self, request: &GenerationRequest) -> Result<GenerationResponse> {
        let request_doc = LlmRequestDoc::from_generation_request(request)?;
        let request_bytes = serde_json::to_vec(&request_doc).context("serialize llmfs request")?;
        let client = NamespaceClient::new(self.root.clone());
        let generation_id = start_generation(&client, &self.llm_connection, &request_bytes).await?;
        let response = read_generation_response(&client, &self.llm_connection, &generation_id)
            .await
            .with_context(|| format!("read llmfs generation {generation_id}"))?;
        Ok(response)
    }

    pub async fn generate_controlled(
        &self,
        request: &GenerationRequest,
        timeout_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<GenerationResponse> {
        let request_doc = LlmRequestDoc::from_generation_request(request)?;
        let request_bytes = serde_json::to_vec(&request_doc).context("serialize llmfs request")?;
        let client = NamespaceClient::new(self.root.clone());
        let generation_id = start_generation_controlled(
            &client,
            &self.llm_connection,
            &request_bytes,
            timeout_secs,
            cancel,
        )
        .await?;
        let read_response = read_generation_response(&client, &self.llm_connection, &generation_id);
        let response = run_generation_read_with_controls(
            read_response,
            &client,
            &self.llm_connection,
            &generation_id,
            timeout_secs,
            cancel,
        )
        .await
        .with_context(|| format!("read llmfs generation {generation_id}"))?;
        Ok(response)
    }

    pub async fn generate_with_text_events<E, F>(
        &self,
        request: &GenerationRequest,
        emit: &mut E,
    ) -> Result<(GenerationResponse, bool)>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        let request_doc = LlmRequestDoc::from_generation_request(request)?;
        let request_bytes = serde_json::to_vec(&request_doc).context("serialize llmfs request")?;
        let client = NamespaceClient::new(self.root.clone());
        let generation_id = start_generation(&client, &self.llm_connection, &request_bytes).await?;
        let response = read_generation_response_with_text_events(
            &client,
            &self.llm_connection,
            &generation_id,
            emit,
        )
        .await
        .with_context(|| format!("read llmfs generation {generation_id}"))?;
        Ok(response)
    }

    pub async fn generate_with_text_events_controlled<E, F>(
        &self,
        request: &GenerationRequest,
        emit: &mut E,
        timeout_secs: u64,
        cancel: &CancellationToken,
    ) -> Result<(GenerationResponse, bool)>
    where
        E: FnMut(Event) -> F,
        F: std::future::Future<Output = ()>,
    {
        let request_doc = LlmRequestDoc::from_generation_request(request)?;
        let request_bytes = serde_json::to_vec(&request_doc).context("serialize llmfs request")?;
        let client = NamespaceClient::new(self.root.clone());
        let generation_id = start_generation_controlled(
            &client,
            &self.llm_connection,
            &request_bytes,
            timeout_secs,
            cancel,
        )
        .await?;
        let read_response = read_generation_response_with_text_events(
            &client,
            &self.llm_connection,
            &generation_id,
            emit,
        );
        let response = run_generation_read_with_controls(
            read_response,
            &client,
            &self.llm_connection,
            &generation_id,
            timeout_secs,
            cancel,
        )
        .await
        .with_context(|| format!("read llmfs generation {generation_id}"))?;
        Ok(response)
    }

    pub async fn write_assistant_state(&self, response: &str) -> Result<()> {
        self.write_assistant_output(response).await?;
        self.write_turn_tape_state(None, response).await
    }

    pub async fn write_assistant_output(&self, response: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_agent_output(&client, &self.agent_path, response).await
    }

    pub async fn write_user_state(&self, input: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_tape_records(&client, &self.agent_path, [("user", input)]).await
    }

    pub async fn write_turn_tape_state(&self, input: Option<&str>, response: &str) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        let mut records = Vec::new();
        if let Some(input) = input.filter(|value| !value.trim().is_empty()) {
            records.push(("user", input));
        }
        records.push(("assistant", response));
        write_tape_records(&client, &self.agent_path, records).await
    }

    pub async fn begin_tape_generation(&self) -> Result<NamespaceTapeWriter> {
        let client = NamespaceClient::new(self.root.clone());
        NamespaceTapeWriter::open(client, &self.agent_path).await
    }

    pub async fn current_tape_checkpoint(&self) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        read_current_tape_checkpoint(&client, &self.agent_path).await
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

    pub async fn write_request(&self, record: NamespaceRequestRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_request_record(&client, &self.agent_path, record).await
    }

    pub async fn write_action(&self, record: NamespaceActionRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_action_record(&client, &self.agent_path, record).await
    }

    pub(crate) async fn read_ui_activity_snapshot(
        &self,
    ) -> Result<alan_agent_protocol::UiActivitySnapshot> {
        serde_json::from_slice(
            &self
                .client()
                .read_file(&ui_activity_path(&self.agent_path))
                .await?,
        )
        .context("parse agent activity snapshot")
    }

    pub(crate) async fn read_assistant_output(&self) -> Result<String> {
        let path = format!("{}/io/output", self.agent_path);
        String::from_utf8(self.client().read_file(&path).await?)
            .context("agent assistant output is utf8")
    }

    pub(crate) async fn read_ui_notice_snapshot(
        &self,
    ) -> Result<alan_agent_protocol::UiNoticeSnapshot> {
        serde_json::from_slice(
            &self
                .client()
                .read_file(&ui_notice_path(&self.agent_path))
                .await?,
        )
        .context("parse agent notice snapshot")
    }

    pub(crate) async fn ui_events_offset(&self) -> Result<u64> {
        Ok(self
            .client()
            .stat_path(&ui_events_path(&self.agent_path))
            .await?
            .length)
    }

    pub(crate) async fn request_ids(&self) -> Result<Vec<String>> {
        self.child_tree_ids("requests").await
    }

    pub(crate) async fn pending_request_id(&self, ids: &[String]) -> Result<Option<String>> {
        for id in ids {
            let path = format!("{}/requests/{id}/status", self.agent_path);
            let status = String::from_utf8(self.client().read_file(&path).await?)
                .context("request status is utf8")?;
            if status.trim() == "pending" {
                return Ok(Some(id.clone()));
            }
        }
        Ok(None)
    }

    pub(crate) async fn action_ids(&self) -> Result<Vec<String>> {
        self.child_tree_ids("actions").await
    }

    pub(crate) async fn read_request_kind(&self, id: &str) -> Result<String> {
        let path = format!("{}/requests/{id}/kind", self.agent_path);
        String::from_utf8(self.client().read_file(&path).await?).context("request kind is utf8")
    }

    pub(crate) async fn request_events_offset(&self) -> Result<u64> {
        self.child_tree_events_offset("requests").await
    }

    pub(crate) async fn action_events_offset(&self) -> Result<u64> {
        self.child_tree_events_offset("actions").await
    }

    async fn child_tree_events_offset(&self, tree: &str) -> Result<u64> {
        Ok(self
            .client()
            .stat_path(&format!("{}/{tree}/events", self.agent_path))
            .await?
            .length)
    }

    async fn child_tree_ids(&self, tree: &str) -> Result<Vec<String>> {
        let mut ids = self
            .client()
            .try_read_directory_names(&format!("{}/{tree}", self.agent_path))
            .await?
            .unwrap_or_default();
        ids.retain(|name| !matches!(name.as_str(), "clone" | "events" | "help"));
        ids.sort();
        Ok(ids)
    }

    /// Build a bounded reference only when the path currently resolves in this
    /// Agent Process namespace.
    pub(crate) async fn evidence_reference(
        &self,
        path: impl Into<String>,
    ) -> Option<NamespaceEvidenceReference> {
        let path = path.into();
        let client = NamespaceClient::new(self.root.clone());
        let stat = client.stat_path(&path).await.ok()?;
        Some(NamespaceEvidenceReference {
            path,
            offset: Some(0),
            length: Some(stat.length),
        })
    }

    /// Resolve evidence through the same namespace walk used for ordinary
    /// files, preserving preview and child metadata in structured failures.
    pub(crate) async fn resolve_evidence_reference(
        &self,
        reference: &NamespaceEvidenceReference,
        preview: Option<String>,
        child_run: Option<serde_json::Value>,
    ) -> std::result::Result<Vec<u8>, EvidenceResolutionError> {
        let client = NamespaceClient::new(self.root.clone());
        let full_bytes =
            client
                .read_file(&reference.path)
                .await
                .map_err(|_| EvidenceResolutionError {
                    code: EvidenceResolutionErrorCode::Missing,
                    reference: reference.clone(),
                    message: "evidence reference is not reachable in this namespace".to_string(),
                    preview: preview.clone(),
                    child_run: child_run.clone(),
                })?;

        if is_retention_expired_record(&full_bytes) {
            return Err(EvidenceResolutionError {
                code: EvidenceResolutionErrorCode::RetentionExpired,
                reference: reference.clone(),
                message: "evidence expired under the storing file server retention policy"
                    .to_string(),
                preview,
                child_run,
            });
        }
        let range = match (reference.offset, reference.length) {
            (Some(offset), Some(length)) => usize::try_from(offset)
                .ok()
                .zip(usize::try_from(length).ok())
                .and_then(|(start, length)| start.checked_add(length).map(|end| (start, end))),
            (Some(offset), None) => usize::try_from(offset)
                .ok()
                .map(|start| (start, full_bytes.len())),
            (None, Some(length)) => usize::try_from(length).ok().map(|end| (0, end)),
            (None, None) => return Ok(full_bytes),
        };
        range
            .filter(|(start, end)| *start <= *end && *end <= full_bytes.len())
            .map(|(start, end)| full_bytes[start..end].to_vec())
            .ok_or_else(|| EvidenceResolutionError {
                code: EvidenceResolutionErrorCode::Missing,
                reference: reference.clone(),
                message: "evidence reference range is not available".to_string(),
                preview,
                child_run,
            })
    }

    pub(crate) async fn write_ui_activity_snapshot(
        &self,
        snapshot: &UiActivitySnapshot,
    ) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_activity_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_plan_snapshot(&self, snapshot: &UiPlanSnapshot) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_plan_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_thinking_snapshot(
        &self,
        snapshot: &UiThinkingSnapshot,
    ) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_thinking_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn write_ui_notice_snapshot(&self, snapshot: &UiNoticeSnapshot) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        write_json_document(&client, &ui_notice_path(&self.agent_path), snapshot).await
    }

    pub(crate) async fn append_ui_event(&self, event: &UiEvent) -> Result<()> {
        let client = NamespaceClient::new(self.root.clone());
        append_json_line(&client, &ui_events_path(&self.agent_path), event).await
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

#[derive(Deserialize)]
struct LlmCapabilitiesDoc {
    version: u16,
    connection: String,
    provider: String,
    capabilities: alan_llm::ProviderCapabilities,
}

/// Canonical v1 record appended to `machine/tape`.
///
/// This is deliberately small and self-contained so ADR-0027 D1 can later hash
/// each record without depending on file offsets or mutable tape state.
#[derive(Serialize)]
struct TapeRecordV1<'a> {
    version: u16,
    kind: &'static str,
    role: &'a str,
    content: &'a str,
}

/// A held GENERATING lease for `machine/tape`.
pub struct NamespaceTapeWriter {
    client: NamespaceClient,
    fid: Fid,
    closed: bool,
}

impl NamespaceTapeWriter {
    async fn open(client: NamespaceClient, agent_path: &str) -> Result<Self> {
        let tape_path = format!("{agent_path}/machine/tape");
        let fid = client.walk_to(&tape_path).await?;
        client
            .open(fid, OpenMode::Write)
            .await
            .with_context(|| format!("open tape writer for {tape_path}"))?;
        Ok(Self {
            client,
            fid,
            closed: false,
        })
    }

    pub async fn append_record(&mut self, role: &str, content: &str) -> Result<()> {
        let bytes = tape_record_bytes(role, content)?;
        self.client
            .write_at(self.fid, 0, &bytes)
            .await
            .context("append tape record")?;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        self.closed = true;
        self.client.clunk(self.fid).await
    }
}

impl Drop for NamespaceTapeWriter {
    fn drop(&mut self) {
        if !self.closed {
            tracing::warn!("namespace tape writer dropped without clunking machine/tape lease");
        }
    }
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

    /// Run one turn from the next committed `io/input` message.
    pub async fn run_next_turn(&mut self) -> Result<NamespaceTurnOutput> {
        let client = NamespaceClient::new(self.environment.root.clone());
        let message = self.environment.read_next_input().await?;

        let request = GenerationRequest::new().with_user_message(message.clone());
        let request = if let Some(system_prompt) = self.config.system_prompt.clone() {
            request.with_system_prompt(system_prompt)
        } else {
            request
        };
        let request_doc = LlmRequestDoc::from_generation_request(&request)?;
        let request_bytes = serde_json::to_vec(&request_doc).context("serialize llmfs request")?;
        let generation_id =
            start_generation(&client, &self.config.llm_connection, &request_bytes).await?;
        let generation_response =
            read_generation_response(&client, &self.config.llm_connection, &generation_id).await?;
        let response = generation_response.content;

        write_agent_output(&client, &self.config.agent_path, &response).await?;
        write_tape_records(
            &client,
            &self.config.agent_path,
            [("user", message.as_str()), ("assistant", response.as_str())],
        )
        .await?;

        Ok(NamespaceTurnOutput {
            input: message,
            response,
            generation_id,
        })
    }
}

impl NamespaceRuntimeEnvironment {
    fn client(&self) -> NamespaceClient {
        NamespaceClient::new(self.root.clone())
    }
}

#[derive(serde::Serialize)]
struct LlmRequestDoc<'a> {
    version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: &'a [alan_llm::Message],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    tools: &'a [alan_llm::ToolDefinition],
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "alan_llm::ReasoningControls::is_empty")]
    reasoning: alan_llm::ReasoningControls,
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty")]
    extra_params: &'a std::collections::HashMap<String, serde_json::Value>,
}

impl<'a> LlmRequestDoc<'a> {
    fn from_generation_request(request: &'a alan_llm::GenerationRequest) -> Result<Self> {
        if request.messages.is_empty() {
            bail!("namespace llmfs generation requires at least one message");
        }
        Ok(Self {
            version: 2,
            system: request.system_prompt.as_deref(),
            messages: &request.messages,
            tools: &request.tools,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            reasoning: request.reasoning,
            extra_params: &request.extra_params,
        })
    }
}

#[derive(Deserialize)]
struct LlmEvent {
    #[serde(default)]
    version: Option<u16>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    thinking_signature: Option<String>,
    #[serde(default)]
    redacted_thinking: Option<String>,
    #[serde(default)]
    finish_reason: Option<String>,
    #[serde(default)]
    provider_response_id: Option<String>,
    #[serde(default)]
    provider_response_status: Option<String>,
    #[serde(default)]
    sequence_number: Option<u64>,
    #[serde(default)]
    usage: Option<LlmEventTokenUsage>,
    #[serde(default)]
    tool_call: Option<LlmEventToolCallDelta>,
    #[serde(default)]
    done: Option<bool>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    rejected: Option<bool>,
    #[serde(default)]
    aborted: Option<bool>,
}

#[derive(Deserialize)]
struct LlmEventTokenUsage {
    prompt_tokens: i32,
    #[serde(default)]
    cached_prompt_tokens: Option<i32>,
    completion_tokens: i32,
    total_tokens: i32,
    #[serde(default)]
    reasoning_tokens: Option<i32>,
}

impl From<LlmEventTokenUsage> for alan_llm::TokenUsage {
    fn from(value: LlmEventTokenUsage) -> Self {
        Self {
            prompt_tokens: value.prompt_tokens,
            cached_prompt_tokens: value.cached_prompt_tokens,
            completion_tokens: value.completion_tokens,
            total_tokens: value.total_tokens,
            reasoning_tokens: value.reasoning_tokens,
        }
    }
}

#[derive(Deserialize)]
struct LlmEventToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments_delta: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments_delta: String,
    final_arguments: Option<String>,
}

impl PartialToolCall {
    fn apply_delta(&mut self, delta: LlmEventToolCallDelta) {
        if let Some(id) = delta.id {
            self.id = Some(id);
        }
        if let Some(name) = delta.name {
            self.name = Some(name);
        }
        if let Some(arguments_delta) = delta.arguments_delta {
            self.arguments_delta.push_str(&arguments_delta);
        }
        if let Some(arguments) = delta.arguments {
            self.final_arguments = Some(arguments);
        }
    }
}

fn assemble_llmfs_tool_calls(
    tool_call_buffers: BTreeMap<usize, PartialToolCall>,
) -> (Vec<alan_llm::ToolCall>, Vec<String>) {
    let mut tool_calls = Vec::new();
    let mut warnings = Vec::new();
    for (_index, call) in tool_call_buffers {
        let Some(name) = call.name.filter(|value| !value.trim().is_empty()) else {
            warnings.push("Dropped malformed llmfs tool call without a name.".to_string());
            continue;
        };
        let arguments_raw = call
            .final_arguments
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                if call.arguments_delta.trim().is_empty() {
                    "{}".to_string()
                } else {
                    call.arguments_delta
                }
            });
        match serde_json::from_str::<serde_json::Value>(&arguments_raw) {
            Ok(arguments) => tool_calls.push(alan_llm::ToolCall {
                id: call.id,
                name,
                arguments,
            }),
            Err(err) => warnings.push(format!(
                "Dropped malformed llmfs tool call `{name}` arguments: {err}."
            )),
        }
    }
    (tool_calls, warnings)
}

async fn start_generation(
    client: &NamespaceClient,
    llm_connection: &str,
    request: &[u8],
) -> Result<String> {
    let clone_path = format!("/mnt/llm/connections/{llm_connection}/clone");
    let generation_id = client
        .clone_via_open(&clone_path)
        .await
        .context("llmfs clone returned generation id")?;

    let data_path = format!("/mnt/llm/connections/{llm_connection}/{generation_id}/data");
    client.write_document(&data_path, request).await?;
    Ok(generation_id)
}

async fn start_generation_controlled(
    client: &NamespaceClient,
    llm_connection: &str,
    request: &[u8],
    timeout_secs: u64,
    cancel: &CancellationToken,
) -> Result<String> {
    let clone_path = format!("/mnt/llm/connections/{llm_connection}/clone");
    let generation_id = client
        .clone_via_open(&clone_path)
        .await
        .context("llmfs clone returned generation id")?;

    let data_path = format!("/mnt/llm/connections/{llm_connection}/{generation_id}/data");
    let commit = client.write_document(&data_path, request);
    let result = run_generation_step_with_controls(
        commit,
        client,
        llm_connection,
        &generation_id,
        timeout_secs,
        cancel,
    )
    .await;
    match result {
        Ok(()) => Ok(generation_id),
        Err(err) => Err(err),
    }
}

async fn abort_generation(
    client: &NamespaceClient,
    llm_connection: &str,
    generation_id: &str,
) -> Result<()> {
    let ctl_path = format!("/mnt/llm/connections/{llm_connection}/{generation_id}/ctl");
    client.write_document(&ctl_path, b"abort").await
}

async fn run_generation_step_with_controls<T, Fut>(
    operation: Fut,
    client: &NamespaceClient,
    llm_connection: &str,
    generation_id: &str,
    timeout_secs: u64,
    cancel: &CancellationToken,
) -> Result<T>
where
    Fut: std::future::Future<Output = Result<T>>,
{
    if timeout_secs == 0 {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = abort_generation(client, llm_connection, generation_id).await;
                Err(anyhow::anyhow!("LLM request cancelled"))
            }
            result = operation => result,
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = abort_generation(client, llm_connection, generation_id).await;
                Err(anyhow::anyhow!("LLM request cancelled"))
            }
            result = tokio::time::timeout(
                tokio::time::Duration::from_secs(timeout_secs),
                operation,
            ) => match result {
                Ok(result) => result,
                Err(_) => {
                    let _ = abort_generation(client, llm_connection, generation_id).await;
                    Err(anyhow::anyhow!("LLM request timed out"))
                }
            },
        }
    }
}

async fn run_generation_read_with_controls<T, Fut>(
    operation: Fut,
    client: &NamespaceClient,
    llm_connection: &str,
    generation_id: &str,
    timeout_secs: u64,
    cancel: &CancellationToken,
) -> Result<T>
where
    Fut: std::future::Future<Output = Result<T>>,
{
    run_generation_step_with_controls(
        operation,
        client,
        llm_connection,
        generation_id,
        timeout_secs,
        cancel,
    )
    .await
}

async fn read_generation_response(
    client: &NamespaceClient,
    llm_connection: &str,
    generation_id: &str,
) -> Result<GenerationResponse> {
    let mut ignore = |_event: Event| async {};
    read_generation_response_with_text_events(client, llm_connection, generation_id, &mut ignore)
        .await
        .map(|(response, _)| response)
}

async fn read_generation_response_with_text_events<E, F>(
    client: &NamespaceClient,
    llm_connection: &str,
    generation_id: &str,
    emit: &mut E,
) -> Result<(GenerationResponse, bool)>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let events_path = format!("/mnt/llm/connections/{llm_connection}/{generation_id}/events");
    let fid = client
        .open_path_guarded(&events_path, OpenMode::Read)
        .await?;
    let mut offset = 0_u64;
    let mut response = String::new();
    let mut thinking = String::new();
    let mut thinking_signature: Option<String> = None;
    let mut redacted_thinking = Vec::new();
    let mut usage = None;
    let mut finish_reason = None;
    let mut provider_response_id = None;
    let mut provider_response_status = None;
    let mut tool_call_buffers: BTreeMap<usize, PartialToolCall> = BTreeMap::new();
    let mut pending = Vec::new();
    let mut emitted_text = false;
    loop {
        let chunk = client.read_at(fid.fid(), offset, 4096).await?;
        if chunk.is_empty() {
            tokio::task::yield_now().await;
            continue;
        }
        offset += chunk.len() as u64;
        pending.extend_from_slice(&chunk);
        while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=pos).collect();
            let line = std::str::from_utf8(&line[..line.len() - 1])
                .context("llmfs event line is not utf8")?;
            if line.is_empty() {
                continue;
            }
            let event: LlmEvent =
                serde_json::from_str(line).with_context(|| format!("parse event {line:?}"))?;
            if let Some(version) = event.version
                && version != 1
            {
                bail!("unsupported llmfs event version {version}");
            }
            if let Some(text) = event.text {
                response.push_str(&text);
                if !text.is_empty() {
                    emitted_text = true;
                    emit(Event::TextDelta {
                        chunk: text,
                        is_final: false,
                    })
                    .await;
                }
            }
            if let Some(delta) = event.thinking
                && !delta.is_empty()
            {
                thinking.push_str(&delta);
            }
            if let Some(signature) = event.thinking_signature
                && !signature.is_empty()
            {
                match &mut thinking_signature {
                    Some(existing) => existing.push_str(&signature),
                    None => thinking_signature = Some(signature),
                }
            }
            if let Some(redacted) = event.redacted_thinking
                && !redacted.is_empty()
            {
                redacted_thinking.push(redacted);
            }
            if let Some(usage_update) = event.usage {
                usage = Some(usage_update.into());
            }
            if let Some(reason) = event.finish_reason
                && !reason.is_empty()
            {
                finish_reason = Some(reason);
            }
            if let Some(response_id) = event.provider_response_id
                && !response_id.is_empty()
            {
                provider_response_id = Some(response_id);
            }
            if let Some(status) = event.provider_response_status
                && !status.is_empty()
            {
                provider_response_status = Some(status);
            }
            if let Some(tool_delta) = event.tool_call {
                tool_call_buffers
                    .entry(tool_delta.index)
                    .or_default()
                    .apply_delta(tool_delta);
            }
            let _ = event.sequence_number;
            if event.done == Some(true) {
                let (tool_calls, warnings) = assemble_llmfs_tool_calls(tool_call_buffers);
                fid.close().await?;
                return Ok((
                    GenerationResponse {
                        content: response,
                        thinking: if thinking.is_empty() {
                            None
                        } else {
                            Some(thinking)
                        },
                        thinking_signature,
                        redacted_thinking,
                        tool_calls,
                        usage,
                        finish_reason: Some(finish_reason.unwrap_or_else(|| "stop".to_string())),
                        provider_response_id: provider_response_id
                            .or_else(|| Some(generation_id.to_string())),
                        provider_response_status: provider_response_status
                            .or_else(|| Some("completed".to_string())),
                        warnings,
                    },
                    emitted_text,
                ));
            }
            if let Some(error) = event.error {
                fid.close().await?;
                bail!("llmfs generation failed: {error}");
            }
            if event.rejected == Some(true) {
                fid.close().await?;
                bail!("llmfs generation request was rejected");
            }
            if event.aborted == Some(true) {
                fid.close().await?;
                bail!("llmfs generation request was aborted");
            }
        }
    }
}

async fn write_agent_output(
    client: &NamespaceClient,
    agent_path: &str,
    response: &str,
) -> Result<()> {
    let output_path = format!("{agent_path}/io/output");
    client
        .write_document(&output_path, response.as_bytes())
        .await
        .with_context(|| format!("write assistant output to {output_path}"))
}

async fn write_tape_records<'a>(
    client: &NamespaceClient,
    agent_path: &str,
    records: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<()> {
    let mut writer = NamespaceTapeWriter::open(client.clone(), agent_path).await?;
    for (role, content) in records {
        writer.append_record(role, content).await?;
    }
    writer.finish().await
}

async fn read_current_tape_checkpoint(
    client: &NamespaceClient,
    agent_path: &str,
) -> Result<String> {
    let checkpoint_path = format!("{agent_path}/machine/checkpoints/current");
    let bytes = client
        .read_file(&checkpoint_path)
        .await
        .with_context(|| format!("read current tape checkpoint from {checkpoint_path}"))?;
    let checkpoint = String::from_utf8(bytes).context("current tape checkpoint is not utf8")?;
    Ok(checkpoint.trim().to_string())
}

fn tape_record_bytes(role: &str, content: &str) -> Result<Vec<u8>> {
    let record = TapeRecordV1 {
        version: 1,
        kind: "message",
        role,
        content,
    };
    let mut bytes = serde_json::to_vec(&record).context("serialize tape record")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_agent_file_id(id: &str, label: &str) -> Result<()> {
    if id.is_empty() || id.contains('/') || id == "." || id == ".." {
        bail!("invalid {label}: {id:?}");
    }
    Ok(())
}

fn request_response_content_part(response: String) -> ContentPart {
    match serde_json::from_str::<serde_json::Value>(&response) {
        Ok(value) => ContentPart::structured(value),
        Err(_) => ContentPart::text(response),
    }
}

fn machine_control_submission(command: &str) -> Option<Submission> {
    match command.trim() {
        "compact" => Some(Submission::new(Op::CompactWithOptions { focus: None })),
        "rollback" => Some(Submission::new(Op::Rollback { turns: 1 })),
        // Turn interrupt is agent-runtime control (stop the current turn,
        // keep the agent alive), not kernel process lifecycle:
        // `/proc/<pid>/ctl` interrupt terminates the process, which is the
        // wrong semantics for a renderer host's Esc. File clients interrupt
        // through machine/ctl.
        "interrupt" => Some(Submission::new(Op::Interrupt)),
        _ => None,
    }
}

async fn write_request_record(
    client: &NamespaceClient,
    agent_path: &str,
    record: NamespaceRequestRecord,
) -> Result<String> {
    let clone_path = format!("{agent_path}/requests/clone");
    let id = client
        .clone_via_open(&clone_path)
        .await
        .with_context(|| format!("create request through {clone_path}"))?;
    let request_path = format!("{agent_path}/requests/{id}");
    client
        .write_document(&format!("{request_path}/kind"), record.kind.as_bytes())
        .await?;
    client
        .write_document(&format!("{request_path}/prompt"), record.prompt.as_bytes())
        .await?;
    if let Some(options) = record.options {
        client
            .write_document(&format!("{request_path}/options"), options.as_bytes())
            .await?;
    }
    Ok(id)
}

async fn write_action_record(
    client: &NamespaceClient,
    agent_path: &str,
    record: NamespaceActionRecord,
) -> Result<String> {
    let clone_path = format!("{agent_path}/actions/clone");
    let id = client
        .clone_via_open(&clone_path)
        .await
        .with_context(|| format!("create action through {clone_path}"))?;
    let action_path = format!("{agent_path}/actions/{id}");
    client
        .write_document(&format!("{action_path}/name"), record.name.as_bytes())
        .await?;
    client
        .write_document(&format!("{action_path}/status"), record.status.as_bytes())
        .await?;
    if let Some(output) = record.output {
        client
            .write_document(&format!("{action_path}/output"), output.as_bytes())
            .await?;
    }
    if let Some(result) = record.result {
        client
            .write_document(&format!("{action_path}/result"), result.as_bytes())
            .await?;
    }
    if let Some(approval) = record.approval {
        client
            .write_document(&format!("{action_path}/approval"), approval.as_bytes())
            .await?;
    }
    if let Some(process) = record.process {
        client
            .write_document(&format!("{action_path}/process"), process.as_bytes())
            .await?;
    }
    Ok(id)
}

fn ui_activity_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/activity")
}

fn ui_plan_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/plan")
}

fn ui_thinking_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/thinking")
}

fn ui_notice_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/notice")
}

fn ui_events_path(agent_path: &str) -> String {
    format!("{agent_path}/machine/ui/events")
}

async fn write_json_document<T: Serialize>(
    client: &NamespaceClient,
    path: &str,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value).context("serialize ui snapshot")?;
    client.write_document(path, &bytes).await
}

async fn append_json_line<T: Serialize>(
    client: &NamespaceClient,
    path: &str,
    value: &T,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(value).context("serialize ui event")?;
    bytes.push(b'\n');
    client.write_document(path, &bytes).await
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
