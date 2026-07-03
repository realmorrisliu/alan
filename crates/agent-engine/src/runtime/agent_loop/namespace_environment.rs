//! Namespace-native environment owned by the agent loop.
//!
//! This module contains the file-operation environment used by the engine when
//! a turn is driven by a single aP namespace handle: input is read from
//! `/agent/<pid>/io/input`, generation is performed through `/mnt/llm`, tools are
//! spawned through `/proc/clone`, and state is written back to `/agent/<pid>`.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use alan_agent_protocol::{ContentPart, Event, InputMode, Op, Submission};
use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, OpenMode, Request, Response, Stat};
use alan_llm::{GenerationRequest, GenerationResponse};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

static NEXT_FID: AtomicU64 = AtomicU64::new(10_000);

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
    input_offset: Arc<AtomicU64>,
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
            input_offset: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn agent_path(&self) -> &str {
        &self.agent_path
    }

    pub fn llm_connection(&self) -> &str {
        &self.llm_connection
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

    pub async fn write_request(&self, record: NamespaceRequestRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_request_record(&client, &self.agent_path, record).await
    }

    pub async fn write_action(&self, record: NamespaceActionRecord) -> Result<String> {
        let client = NamespaceClient::new(self.root.clone());
        write_action_record(&client, &self.agent_path, record).await
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
        let action_status = if result.exit_code == 0 {
            "completed"
        } else {
            "failed"
        };
        let result_doc = serde_json::json!({
            "exit_code": result.exit_code,
        })
        .to_string();
        let action_id = self
            .write_action(
                NamespaceActionRecord::new(tool_name, action_status)
                    .with_output(result.output.clone())
                    .with_result(result_doc)
                    .with_approval("not_required")
                    .with_process(format!("/proc/{pid}")),
            )
            .await?;
        Ok(NamespaceToolActionOutput {
            action_id,
            pid,
            output: result.output,
            exit_code: result.exit_code,
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
            version: 1,
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

struct InputFrame {
    message: String,
    bytes_consumed: usize,
}

impl InputFrame {
    fn total_len(raw: &[u8]) -> Result<Option<usize>> {
        let Some(nl) = raw.iter().position(|&b| b == b'\n') else {
            return Ok(None);
        };
        let len: usize = std::str::from_utf8(&raw[..nl])
            .context("input frame length is not utf8")?
            .parse()
            .context("input frame length is not a number")?;
        let start = nl + 1;
        let end = start
            .checked_add(len)
            .context("input frame length overflowed")?;
        Ok(Some(end))
    }

    fn parse_one(raw: &[u8]) -> Result<Self> {
        let end = Self::total_len(raw)?.context("input frame is missing length header")?;
        if raw.len() < end {
            bail!("input frame is truncated");
        }
        let start = raw
            .iter()
            .position(|&b| b == b'\n')
            .expect("total_len requires a length header")
            + 1;
        let message = String::from_utf8(raw[start..end].to_vec())
            .context("input frame payload is not utf8")?;
        Ok(Self {
            message,
            bytes_consumed: end,
        })
    }
}

#[derive(Clone)]
struct NamespaceClient {
    fs: InProcessTransport,
}

struct NamespaceFidGuard {
    client: NamespaceClient,
    fid: Option<Fid>,
}

impl NamespaceFidGuard {
    fn new(client: NamespaceClient, fid: Fid) -> Self {
        Self {
            client,
            fid: Some(fid),
        }
    }

    fn fid(&self) -> Fid {
        self.fid.expect("namespace fid guard is closed")
    }

    async fn close(mut self) -> Result<()> {
        let Some(fid) = self.fid.take() else {
            return Ok(());
        };
        self.client.clunk(fid).await
    }
}

impl Drop for NamespaceFidGuard {
    fn drop(&mut self) {
        let Some(fid) = self.fid.take() else {
            return;
        };
        let client = self.client.clone();
        drop(tokio::spawn(async move {
            let _ = client.clunk(fid).await;
        }));
    }
}

impl NamespaceClient {
    fn new(fs: InProcessTransport) -> Self {
        Self { fs }
    }

    async fn walk_to(&self, path: &str) -> Result<Fid> {
        let fid = Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await?
        {
            Response::Walk { .. } => Ok(fid),
            _ => bail!("unexpected walk response for {path}"),
        }
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<FileKind> {
        match self.fs.call(Request::Open { fid, mode }).await? {
            Response::Open { qid } => Ok(qid.kind),
            _ => bail!("unexpected open response"),
        }
    }

    async fn open_guarded_fid(&self, fid: Fid, mode: OpenMode) -> Result<NamespaceFidGuard> {
        match self.open(fid, mode).await {
            Ok(_) => Ok(NamespaceFidGuard::new(self.clone(), fid)),
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    async fn open_path_guarded(&self, path: &str, mode: OpenMode) -> Result<NamespaceFidGuard> {
        let fid = self.walk_to(path).await?;
        self.open_guarded_fid(fid, mode).await
    }

    async fn read_at(&self, fid: Fid, offset: u64, count: u32) -> Result<Vec<u8>> {
        match self.fs.call(Request::Read { fid, offset, count }).await? {
            Response::Read { data } => Ok(data),
            _ => bail!("unexpected read response"),
        }
    }

    async fn stat(&self, fid: Fid) -> Result<Stat> {
        match self.fs.call(Request::Stat { fid }).await? {
            Response::Stat { stat } => Ok(stat),
            _ => bail!("unexpected stat response"),
        }
    }

    async fn read_all_opened(&self, fid: Fid) -> Result<Vec<u8>> {
        let mut offset = 0_u64;
        let mut data = Vec::new();
        loop {
            let chunk = self.read_at(fid, offset, 64 * 1024).await?;
            if chunk.is_empty() {
                break;
            }
            offset += chunk.len() as u64;
            let reached_short_read = chunk.len() < 64 * 1024;
            data.extend_from_slice(&chunk);
            if reached_short_read {
                break;
            }
        }
        Ok(data)
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let fid = self.open_path_guarded(path, OpenMode::Read).await?;
        let data = self.read_all_opened(fid.fid()).await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    async fn try_read_file(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let fid = Fid(NEXT_FID.fetch_add(1, Ordering::Relaxed));
        match self
            .fs
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: fid,
                names: split_path(path),
            })
            .await
        {
            Ok(Response::Walk { .. }) => {}
            Ok(_) => bail!("unexpected walk response for {path}"),
            Err(ErrorCode::NotFound) => return Ok(None),
            Err(err) => return Err(err).with_context(|| format!("walk to {path}")),
        }

        let fid = self.open_guarded_fid(fid, OpenMode::Read).await?;
        let data = self.read_all_opened(fid.fid()).await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(Some(data)),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    async fn stat_path(&self, path: &str) -> Result<Stat> {
        let fid = self.walk_to(path).await?;
        let stat = self.stat(fid).await;
        let clunk = self.clunk(fid).await;
        match (stat, clunk) {
            (Ok(stat), Ok(())) => Ok(stat),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    async fn write_document(&self, path: &str, data: &[u8]) -> Result<()> {
        let fid = self.walk_to(path).await?;
        match self.write_opened(fid, data).await {
            Ok(()) => self.clunk(fid).await,
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
        .with_context(|| format!("write {path}"))
    }

    async fn write_opened(&self, fid: Fid, data: &[u8]) -> Result<()> {
        self.open(fid, OpenMode::Write).await?;
        self.write_all_opened(fid, data).await
    }

    async fn write_all_opened(&self, fid: Fid, data: &[u8]) -> Result<()> {
        let mut offset = 0_u64;
        let mut remaining = data;
        if remaining.is_empty() {
            self.write_at(fid, 0, remaining).await?;
            return Ok(());
        }
        while !remaining.is_empty() {
            let written = self.write_at(fid, offset, remaining).await?;
            if written == 0 || written > remaining.len() {
                bail!("invalid write count from file server");
            }
            offset += written as u64;
            remaining = &remaining[written..];
        }
        Ok(())
    }

    async fn write_at(&self, fid: Fid, offset: u64, data: &[u8]) -> Result<usize> {
        match self
            .fs
            .call(Request::Write {
                fid,
                offset,
                data: data.to_vec(),
            })
            .await?
        {
            Response::Write { count } => Ok(count as usize),
            _ => bail!("unexpected write response"),
        }
    }

    async fn clone_via_open(&self, path: &str) -> Result<String> {
        let fid = self.walk_to(path).await?;
        match async {
            self.open(fid, OpenMode::ReadWrite).await?;
            let id = String::from_utf8(self.read_at(fid, 0, 128).await?)
                .with_context(|| format!("{path} returned non-utf8 id"))?;
            Ok(id)
        }
        .await
        {
            Ok(id) => {
                self.clunk(fid).await?;
                Ok(id)
            }
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    async fn clone_with_document(&self, path: &str, data: &[u8]) -> Result<String> {
        let fid = self.walk_to(path).await?;
        match async {
            self.open(fid, OpenMode::ReadWrite).await?;
            let id = String::from_utf8(self.read_at(fid, 0, 128).await?)
                .with_context(|| format!("{path} returned non-utf8 id"))?;
            self.write_all_opened(fid, data).await?;
            Ok(id)
        }
        .await
        {
            Ok(id) => {
                self.clunk(fid).await?;
                Ok(id)
            }
            Err(err) => {
                let _ = self.clunk(fid).await;
                Err(err)
            }
        }
    }

    async fn read_stream_from(&self, path: &str, offset: u64) -> Result<Vec<u8>> {
        let fid = self.open_path_guarded(path, OpenMode::Read).await?;
        let data = async {
            let mut data = self.read_at(fid.fid(), offset, 64 * 1024).await?;
            let total_len =
                InputFrame::total_len(&data)?.context("input frame is missing length header")?;
            while data.len() < total_len {
                let remaining = total_len - data.len();
                let count = remaining.min(64 * 1024) as u32;
                if count == 0 {
                    bail!("input frame is truncated");
                }
                let chunk = self
                    .read_at(fid.fid(), offset + data.len() as u64, count)
                    .await?;
                if chunk.is_empty() {
                    bail!("input frame is truncated");
                }
                data.extend_from_slice(&chunk);
            }
            Ok(data)
        }
        .await;
        let clunk = fid.close().await;
        match (data, clunk) {
            (Ok(data), Ok(())) => Ok(data),
            (Err(err), _) => Err(err),
            (_, Err(err)) => Err(err),
        }
    }

    async fn clunk(&self, fid: Fid) -> Result<()> {
        match self.fs.call(Request::Clunk { fid }).await? {
            Response::Clunk => Ok(()),
            _ => bail!("unexpected clunk response"),
        }
    }
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
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
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    use alan_agentfs::{AgentConformanceChecker, AgentFs, AgentRootFs};
    use alan_ap::{FileServer, InProcessTransport, Qid, Request};
    use alan_kernel::{
        Access, Credentials, MountFs, Namespace, ProcFs, ProcessInvocation, ProcessOutcome,
        ProcessRunner,
    };
    use alan_llm::{
        GenerationRequest, GenerationResponse, LlmProvider, MockLlmProvider, StreamChunk,
    };
    use alan_llmfs::LlmFs;
    use alan_shell::Shell;
    use tokio::sync::Notify;

    use super::*;

    struct EchoRunner;

    #[async_trait::async_trait]
    impl ProcessRunner for EchoRunner {
        async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
            let Ok(resolved) = invocation.namespace.resolve(&invocation.exec.executable) else {
                return ProcessOutcome::exited(127, b"executable is not mounted\n".to_vec());
            };
            let fid = Fid(60_000 + invocation.pid.0);
            let reachable = resolved
                .call(Request::Walk {
                    fid: Fid::ROOT,
                    newfid: fid,
                    names: resolved.rel.clone(),
                })
                .await
                .is_ok();
            let _ = resolved.call(Request::Clunk { fid }).await;
            if !reachable {
                return ProcessOutcome::exited(127, b"executable is not reachable\n".to_vec());
            }
            let mut output = invocation.exec.args.join(" ").into_bytes();
            output.push(b'\n');
            ProcessOutcome::exited(0, output)
        }
    }

    struct LargeOutputRunner;

    #[async_trait::async_trait]
    impl ProcessRunner for LargeOutputRunner {
        async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
            ProcessOutcome::exited(0, vec![b'x'; 70 * 1024])
        }
    }

    struct AbortObservedRunner {
        started: Arc<Notify>,
        dropped: Arc<Notify>,
    }

    struct AbortDropGuard {
        dropped: Arc<Notify>,
    }

    impl Drop for AbortDropGuard {
        fn drop(&mut self) {
            self.dropped.notify_one();
        }
    }

    #[async_trait::async_trait]
    impl ProcessRunner for AbortObservedRunner {
        async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
            let _guard = AbortDropGuard {
                dropped: Arc::clone(&self.dropped),
            };
            self.started.notify_one();
            std::future::pending::<ProcessOutcome>().await
        }
    }

    struct BlockingStreamProvider {
        started: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl LlmProvider for BlockingStreamProvider {
        async fn generate(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<GenerationResponse> {
            Err(anyhow::anyhow!("blocking provider uses streaming"))
        }

        async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
            Ok("blocking stream provider".to_string())
        }

        async fn generate_stream(
            &mut self,
            _request: GenerationRequest,
        ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
            self.started.notify_one();
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            tokio::spawn(async move {
                let _hold = tx;
                std::future::pending::<()>().await;
            });
            Ok(rx)
        }

        fn provider_name(&self) -> &'static str {
            "blocking_stream"
        }
    }

    fn input_frame(message: &str) -> Vec<u8> {
        format!("{}\n{message}", message.len()).into_bytes()
    }

    fn tool_test_environment(
        runner: Arc<dyn ProcessRunner>,
    ) -> (NamespaceRuntimeEnvironment, Shell) {
        let procfs = ProcFs::new().with_runner(runner);
        let agentfs = Arc::new(AgentFs::new());
        let binfs = Arc::new(alan_ap::reference::MemFs::new());

        let mut child_namespace = Namespace::new();
        child_namespace.mount(
            "/proc",
            InProcessTransport::new(Arc::new(procfs.clone())),
            Access::ReadWrite,
        );
        child_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs.clone()),
            Access::ReadWrite,
        );
        child_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);

        let spawner_procfs =
            Arc::new(procfs.for_spawner(None, child_namespace, Credentials::user("root-agent")));
        let mut root_namespace = Namespace::new();
        root_namespace.mount(
            "/proc",
            InProcessTransport::new(spawner_procfs),
            Access::ReadWrite,
        );
        root_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
        (
            NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default"),
            Shell::new(root),
        )
    }

    struct BlockingReadFs {
        read_started: Notify,
        clunked: Notify,
        clunk_count: AtomicUsize,
    }

    impl BlockingReadFs {
        fn new() -> Self {
            Self {
                read_started: Notify::new(),
                clunked: Notify::new(),
                clunk_count: AtomicUsize::new(0),
            }
        }

        fn qid(kind: FileKind) -> Qid {
            Qid {
                kind,
                version: 0,
                path: 1,
            }
        }
    }

    #[async_trait::async_trait]
    impl FileServer for BlockingReadFs {
        async fn walk(
            &self,
            _fid: Fid,
            _newfid: Fid,
            _names: &[String],
        ) -> std::result::Result<Qid, ErrorCode> {
            Ok(Self::qid(FileKind::Stream))
        }

        async fn open(&self, _fid: Fid, _mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
            Ok(Self::qid(FileKind::Stream))
        }

        async fn read(
            &self,
            _fid: Fid,
            _offset: u64,
            _count: u32,
        ) -> std::result::Result<Vec<u8>, ErrorCode> {
            self.read_started.notify_one();
            std::future::pending().await
        }

        async fn write(
            &self,
            _fid: Fid,
            _offset: u64,
            _data: &[u8],
        ) -> std::result::Result<u32, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn stat(&self, _fid: Fid) -> std::result::Result<Stat, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn create(
            &self,
            _fid: Fid,
            _newfid: Fid,
            _name: &str,
            _kind: FileKind,
        ) -> std::result::Result<Qid, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn remove(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn clunk(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
            self.clunk_count.fetch_add(1, AtomicOrdering::SeqCst);
            self.clunked.notify_one();
            Ok(())
        }
    }

    struct ScriptedReadFs {
        data: Vec<u8>,
        clunk_count: AtomicUsize,
    }

    impl ScriptedReadFs {
        fn new(data: impl Into<Vec<u8>>) -> Self {
            Self {
                data: data.into(),
                clunk_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl FileServer for ScriptedReadFs {
        async fn walk(
            &self,
            _fid: Fid,
            _newfid: Fid,
            _names: &[String],
        ) -> std::result::Result<Qid, ErrorCode> {
            Ok(BlockingReadFs::qid(FileKind::Stream))
        }

        async fn open(&self, _fid: Fid, _mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
            Ok(BlockingReadFs::qid(FileKind::Stream))
        }

        async fn read(
            &self,
            _fid: Fid,
            offset: u64,
            count: u32,
        ) -> std::result::Result<Vec<u8>, ErrorCode> {
            let start = offset as usize;
            if start >= self.data.len() {
                return Ok(Vec::new());
            }
            let end = (start + count as usize).min(self.data.len());
            Ok(self.data[start..end].to_vec())
        }

        async fn write(
            &self,
            _fid: Fid,
            _offset: u64,
            _data: &[u8],
        ) -> std::result::Result<u32, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn stat(&self, _fid: Fid) -> std::result::Result<Stat, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn create(
            &self,
            _fid: Fid,
            _newfid: Fid,
            _name: &str,
            _kind: FileKind,
        ) -> std::result::Result<Qid, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn remove(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
            Err(ErrorCode::Unsupported)
        }

        async fn clunk(&self, _fid: Fid) -> std::result::Result<(), ErrorCode> {
            self.clunk_count.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn read_stream_clunks_open_fid_when_cancelled() {
        let fs = Arc::new(BlockingReadFs::new());
        let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));
        let task = tokio::spawn({
            let client = client.clone();
            async move { client.read_stream_from("/agent/1/io/input", 0).await }
        });

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            fs.read_started.notified(),
        )
        .await
        .expect("read should start");
        task.abort();
        let _ = task.await;
        tokio::time::timeout(std::time::Duration::from_secs(1), fs.clunked.notified())
            .await
            .expect("cancelled read should clunk the fid");

        assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn llmfs_event_error_clunks_events_fid() {
        let fs = Arc::new(ScriptedReadFs::new(
            b"{\"version\":1,\"error\":\"temporary 503\"}\n".as_slice(),
        ));
        let client = NamespaceClient::new(InProcessTransport::new(fs.clone()));
        let mut ignore = |_event: Event| async {};

        let err =
            read_generation_response_with_text_events(&client, "default", "gen-1", &mut ignore)
                .await
                .unwrap_err();

        assert!(
            err.to_string().contains("llmfs generation failed"),
            "{err:#}"
        );
        assert_eq!(fs.clunk_count.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn controlled_generation_aborts_llmfs_on_cancel() {
        let started = Arc::new(Notify::new());
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(BlockingStreamProvider {
                started: Arc::clone(&started),
            }),
        );
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let cancel = CancellationToken::new();
        let task = tokio::spawn({
            let environment = environment.clone();
            let cancel = cancel.clone();
            async move {
                let request = GenerationRequest::new().with_user_message("hello");
                let mut ignore = |_event: Event| async {};
                environment
                    .generate_with_text_events_controlled(&request, &mut ignore, 30, &cancel)
                    .await
            }
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
            .await
            .expect("provider stream should start");
        cancel.cancel();
        let err = task.await.unwrap().unwrap_err();
        let err = format!("{err:#}");
        assert!(
            err.contains("cancelled") || err.contains("aborted"),
            "{err}"
        );

        let status = String::from_utf8(
            shell
                .cat("/mnt/llm/connections/default/g0/status")
                .await
                .unwrap(),
        )
        .unwrap();
        let status: serde_json::Value = serde_json::from_str(&status).unwrap();
        assert_eq!(status["status"], "aborted");
    }

    #[tokio::test]
    async fn m2_shell_talks_to_agent_through_files() {
        let procfs = Arc::new(ProcFs::new());
        let proc_server: Arc<dyn FileServer> = procfs.clone();
        let agentfs = Arc::new(AgentFs::new());
        let agent_root = Arc::new(AgentRootFs::new(proc_server));
        let llmfs = Arc::new(LlmFs::new());
        llmfs.register_connection(
            "default",
            Box::new(MockLlmProvider::new().with_response(GenerationResponse {
                content: "hello from llmfs".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: None,
                finish_reason: None,
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            })),
        );

        let mut ns = Namespace::new();
        ns.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
        ns.mount(
            "/agent",
            InProcessTransport::new(agent_root.clone()),
            Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());

        let pid = shell
            .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
            .await
            .unwrap();
        assert_eq!(pid, "1");
        agent_root.bind_process(pid.clone(), agentfs.clone()).await;
        agent_root.set_root_process(pid.clone()).await;

        shell
            .write("/agent/1/io/input", &input_frame("hello agent"))
            .await
            .unwrap();
        let mut output_tail = shell.tail("/agent/1/io/output").await.unwrap();

        let mut runtime = NamespaceTurnRuntime::new(
            root.clone(),
            NamespaceTurnRuntimeConfig::new("/agent/1", "default")
                .with_system_prompt("You are an M2 test agent."),
        );
        let turn = runtime.run_next_turn().await.unwrap();

        assert_eq!(turn.input, "hello agent");
        assert_eq!(turn.response, "hello from llmfs");
        assert!(!turn.generation_id.is_empty());

        let streamed = output_tail.read(64 * 1024).await.unwrap();
        output_tail.close().await.unwrap();
        assert_eq!(String::from_utf8(streamed).unwrap(), "hello from llmfs");

        let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
        assert!(tape.contains(r#""role":"user""#), "{tape}");
        assert!(tape.contains(r#""content":"hello agent""#), "{tape}");
        assert!(tape.contains(r#""role":"assistant""#), "{tape}");
        assert!(tape.contains(r#""content":"hello from llmfs""#), "{tape}");
        let checkpoint = runtime.current_tape_checkpoint().await.unwrap();
        let checkpoint_file = String::from_utf8(
            shell
                .cat("/agent/1/machine/checkpoints/current")
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(checkpoint, checkpoint_file.trim());
        assert!(checkpoint.starts_with("sha256:"), "{checkpoint}");

        AgentConformanceChecker::new(root)
            .check_agent_process("/agent/1")
            .await
            .assert_ok();
    }

    #[tokio::test]
    async fn engine_writes_requests_and_actions_as_agent_files() {
        let procfs = Arc::new(ProcFs::new());
        let proc_server: Arc<dyn FileServer> = procfs.clone();
        let agentfs = Arc::new(AgentFs::new());
        let agent_root = Arc::new(AgentRootFs::new(proc_server));
        let mut ns = Namespace::new();
        ns.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
        ns.mount(
            "/agent",
            InProcessTransport::new(agent_root.clone()),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let pid = shell
            .spawn(r#"{"executable":"/bin/agent","args":[]}"#)
            .await
            .unwrap();
        assert_eq!(pid, "1");
        agent_root.bind_process(pid.clone(), agentfs.clone()).await;
        agent_root.set_root_process(pid).await;
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");

        let request_id = environment
            .write_request(
                NamespaceRequestRecord::new("confirmation", "approve this action?")
                    .with_options(r#"{"choices":["approve","deny"]}"#),
            )
            .await
            .unwrap();
        assert_eq!(request_id, "r0");
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/requests/{request_id}/kind"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "confirmation"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/requests/{request_id}/prompt"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "approve this action?"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/requests/{request_id}/options"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            r#"{"choices":["approve","deny"]}"#
        );

        let action_id = environment
            .write_action(
                NamespaceActionRecord::new("read", "completed")
                    .with_output("file contents")
                    .with_result(r#"{"ok":true}"#)
                    .with_approval("not_required")
                    .with_process("/proc/42"),
            )
            .await
            .unwrap();
        assert_eq!(action_id, "a0");
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/name"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "read"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/status"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "completed"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/output"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "file contents"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/result"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            r#"{"ok":true}"#
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/approval"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "not_required"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/agent/1/actions/{action_id}/process"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "/proc/42"
        );

        AgentConformanceChecker::new(root)
            .check_agent_process("/agent/1")
            .await
            .assert_ok();
    }

    #[tokio::test]
    async fn answered_request_response_resumes_engine_pending_yield_from_files() {
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default");

        let request_id = environment
            .write_request(NamespaceRequestRecord::new(
                "structured_input",
                "Provide the missing detail",
            ))
            .await
            .unwrap();
        assert_eq!(request_id, "r0");
        assert!(
            environment
                .resume_submission_from_answered_request(&request_id)
                .await
                .unwrap()
                .is_none()
        );

        shell
            .write(
                &format!("/agent/1/requests/{request_id}/response"),
                br#"{"answers":[{"question_id":"q1","value":"answer from request file"}]}"#,
            )
            .await
            .unwrap();
        let submission = environment
            .resume_submission_from_answered_request(&request_id)
            .await
            .unwrap()
            .expect("answered request becomes a resume submission");
        match &submission.op {
            Op::Resume {
                request_id: resumed_id,
                content,
            } => {
                assert_eq!(resumed_id, "r0");
                assert_eq!(
                    content,
                    &vec![ContentPart::structured(serde_json::json!({
                        "answers": [{"question_id": "q1", "value": "answer from request file"}]
                    }))]
                );
            }
            other => panic!("expected Op::Resume, got {other:?}"),
        }

        let mut turn_state = super::super::super::turn_state::TurnState::default();
        turn_state.set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id,
            title: "Missing detail".to_string(),
            prompt: "Provide the missing detail".to_string(),
            questions: Vec::new(),
        });
        let mut state = super::super::RuntimeLoopState {
            workspace_id: "namespace-resume-test".to_string(),
            workspace_root_dir: None,
            session: crate::Session::new(),
            current_submission_id: None,
            environment: super::super::RuntimeEnvironment::namespace(environment),
            tool_catalog: crate::tools::ToolRegistry::new(),
            core_config: crate::Config::default(),
            runtime_config: super::super::super::RuntimeConfig::default(),
            workspace_persona_dirs: Vec::new(),
            prompt_cache: super::super::super::prompt_cache::PromptAssemblyCache::new(Vec::new()),
            turn_state,
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };

        let action = super::super::super::submission_handlers::handle_runtime_op_with_cancel(
            &mut state,
            submission.op,
            &mut emit,
            &cancel,
        )
        .await
        .unwrap();

        assert!(
            matches!(
                action,
                super::super::super::submission_handlers::RuntimeOpAction::RunTurn { .. }
            ),
            "resume should re-enter the turn path: {action:?}"
        );
        assert!(!state.turn_state.has_pending_interaction());
        assert!(events.is_empty());
        let messages = state.session.tape.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tool_responses()[0].id, "r0");
        assert_eq!(
            messages[0].tool_responses()[0].text_content(),
            r#"{"answers":[{"question_id":"q1","value":"answer from request file"}]}"#
        );
    }

    #[tokio::test]
    async fn input_frame_becomes_engine_input_submission() {
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        shell
            .write("/agent/1/io/input", b"continue from files")
            .await
            .unwrap();
        let submission = environment
            .read_next_input_submission(InputMode::FollowUp)
            .await
            .unwrap();

        match submission.op {
            Op::Input { parts, mode } => {
                assert_eq!(mode, InputMode::FollowUp);
                assert_eq!(parts, vec![ContentPart::text("continue from files")]);
            }
            other => panic!("expected Op::Input, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn input_frame_larger_than_initial_read_becomes_submission() {
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
        let message = "x".repeat(70 * 1024);

        shell
            .write("/agent/1/io/input", message.as_bytes())
            .await
            .unwrap();
        let submission = environment
            .read_next_input_submission(InputMode::FollowUp)
            .await
            .unwrap();

        match submission.op {
            Op::Input { parts, mode } => {
                assert_eq!(mode, InputMode::FollowUp);
                assert_eq!(parts, vec![ContentPart::text(message)]);
            }
            other => panic!("expected Op::Input, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn engine_spawns_process_with_spawner_assembled_namespace() {
        let procfs = ProcFs::new();
        let agentfs = Arc::new(AgentFs::new());
        let llmfs = Arc::new(LlmFs::new());
        let binfs = Arc::new(alan_ap::reference::MemFs::new());

        let mut child_namespace = Namespace::new();
        child_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        child_namespace.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            Access::ReadWrite,
        );
        child_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);

        let spawner_procfs =
            Arc::new(procfs.for_spawner(None, child_namespace, Credentials::user("root-agent")));
        let mut root_namespace = Namespace::new();
        root_namespace.mount(
            "/proc",
            InProcessTransport::new(spawner_procfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/root", "default");

        let pid = environment
            .spawn_process("/bin/agent", Vec::<String>::new())
            .await
            .unwrap();

        assert_eq!(pid, "1");
        assert_eq!(
            String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap(),
            "running\n"
        );
        assert_eq!(
            String::from_utf8(
                shell
                    .cat(&format!("/proc/{pid}/credentials"))
                    .await
                    .unwrap()
            )
            .unwrap(),
            "root-agent"
        );
        let namespace =
            String::from_utf8(shell.cat(&format!("/proc/{pid}/namespace")).await.unwrap()).unwrap();
        assert!(namespace.lines().any(|line| line == "/agent/1 rw"));
        assert!(namespace.lines().any(|line| line == "/mnt/llm rw"));
        assert!(namespace.lines().any(|line| line == "/bin ro"));
    }

    #[tokio::test]
    async fn engine_runs_tool_as_process_and_projects_action_files() {
        let procfs = ProcFs::new().with_runner(Arc::new(EchoRunner));
        let agentfs = Arc::new(AgentFs::new());
        let binfs = Arc::new(alan_ap::reference::MemFs::new());

        let mut child_namespace = Namespace::new();
        child_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs.clone()),
            Access::ReadWrite,
        );
        child_namespace.mount("/bin", InProcessTransport::new(binfs), Access::ReadOnly);

        let spawner_procfs =
            Arc::new(procfs.for_spawner(None, child_namespace, Credentials::user("root-agent")));
        let mut root_namespace = Namespace::new();
        root_namespace.mount(
            "/proc",
            InProcessTransport::new(spawner_procfs),
            Access::ReadWrite,
        );
        root_namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(root_namespace)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let action = environment
            .run_tool_action(
                "echo",
                "/bin/greeting",
                ["hello".to_string(), "from-process".to_string()],
            )
            .await
            .unwrap();

        assert_eq!(action.pid, "1");
        assert_eq!(action.action_id, "a0");
        assert_eq!(action.output, "hello from-process\n");
        assert_eq!(action.exit_code, 0);
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "exited\n"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/io/output").await.unwrap()).unwrap(),
            "hello from-process\n"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/name").await.unwrap()).unwrap(),
            "echo"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/status").await.unwrap()).unwrap(),
            "completed"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/output").await.unwrap()).unwrap(),
            "hello from-process\n"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/result").await.unwrap()).unwrap(),
            r#"{"exit_code":0}"#
        );
        assert_eq!(
            String::from_utf8(shell.cat("/agent/1/actions/a0/process").await.unwrap()).unwrap(),
            "/proc/1"
        );
    }

    #[tokio::test]
    async fn run_tool_action_cancels_spawned_process_on_cancel() {
        let started = Arc::new(Notify::new());
        let dropped = Arc::new(Notify::new());
        let (environment, shell) = tool_test_environment(Arc::new(AbortObservedRunner {
            started: Arc::clone(&started),
            dropped: Arc::clone(&dropped),
        }));
        let cancel = CancellationToken::new();
        let task = tokio::spawn({
            let environment = environment.clone();
            let cancel = cancel.clone();
            async move {
                environment
                    .run_tool_action_with_cancel(
                        "blocked",
                        "/bin/blocked",
                        Vec::<String>::new(),
                        &cancel,
                    )
                    .await
            }
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
            .await
            .expect("tool runner should start");
        cancel.cancel();
        let err = task.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("cancelled"), "{err:#}");
        tokio::time::timeout(std::time::Duration::from_secs(1), dropped.notified())
            .await
            .expect("tool runner future should be aborted");
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "exited\n"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/exit").await.unwrap()).unwrap(),
            "130"
        );
    }

    #[tokio::test]
    async fn run_tool_action_cancels_spawned_process_on_wait_timeout() {
        let started = Arc::new(Notify::new());
        let dropped = Arc::new(Notify::new());
        let (environment, shell) = tool_test_environment(Arc::new(AbortObservedRunner {
            started: Arc::clone(&started),
            dropped: Arc::clone(&dropped),
        }));
        let cancel = CancellationToken::new();
        let task = tokio::spawn({
            let environment = environment.clone();
            let cancel = cancel.clone();
            async move {
                environment
                    .run_tool_action_with_cancel_and_timeout(
                        "blocked",
                        "/bin/blocked",
                        Vec::<String>::new(),
                        &cancel,
                        1,
                    )
                    .await
            }
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), started.notified())
            .await
            .expect("tool runner should start");
        let err = tokio::time::timeout(std::time::Duration::from_secs(2), task)
            .await
            .expect("tool wait should use the configured timeout")
            .unwrap()
            .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("timed out waiting 1s"), "{err}");
        tokio::time::timeout(std::time::Duration::from_secs(1), dropped.notified())
            .await
            .expect("tool runner future should be aborted on wait timeout");
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/status").await.unwrap()).unwrap(),
            "exited\n"
        );
        assert_eq!(
            String::from_utf8(shell.cat("/proc/1/exit").await.unwrap()).unwrap(),
            "130"
        );
    }

    #[tokio::test]
    async fn run_tool_action_reads_output_larger_than_initial_read() {
        let (environment, _shell) = tool_test_environment(Arc::new(LargeOutputRunner));

        let action = environment
            .run_tool_action("large", "/bin/large", Vec::<String>::new())
            .await
            .unwrap();

        assert_eq!(action.output.len(), 70 * 1024);
        assert!(action.output.bytes().all(|byte| byte == b'x'));
        assert_eq!(action.exit_code, 0);
    }

    #[tokio::test]
    async fn engine_tape_writer_holds_generating_lease_and_allows_readers() {
        let agentfs = Arc::new(AgentFs::new());
        let mut ns = Namespace::new();
        ns.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
        let shell = Shell::new(root.clone());
        let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

        let mut writer = environment.begin_tape_generation().await.unwrap();

        let second_writer = environment.begin_tape_generation().await;
        assert!(
            second_writer.is_err(),
            "a second engine writer must not acquire machine/tape while GENERATING lease is held"
        );

        let mut tape_tail = shell.tail("/agent/1/machine/tape").await.unwrap();
        writer.append_record("user", "hello").await.unwrap();
        let streamed = String::from_utf8(tape_tail.read(64 * 1024).await.unwrap()).unwrap();
        assert!(streamed.contains(r#""role":"user""#), "{streamed}");
        assert!(streamed.contains(r#""content":"hello""#), "{streamed}");
        tape_tail.close().await.unwrap();

        writer.append_record("assistant", "hi").await.unwrap();
        writer.finish().await.unwrap();

        let mut next_writer = environment.begin_tape_generation().await.unwrap();
        next_writer
            .append_record("assistant", "after lease")
            .await
            .unwrap();
        next_writer.finish().await.unwrap();

        let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
        assert!(tape.contains(r#""content":"hi""#), "{tape}");
        assert!(tape.contains(r#""content":"after lease""#), "{tape}");
    }

    #[test]
    fn tape_record_shape_is_content_addressable_ready() {
        let record = tape_record_bytes("assistant", "stable text").unwrap();
        assert_eq!(
            String::from_utf8(record).unwrap(),
            r#"{"version":1,"kind":"message","role":"assistant","content":"stable text"}"#
                .to_string()
                + "\n",
            "tape records must stay canonical, self-contained newline-delimited units"
        );
    }
}
