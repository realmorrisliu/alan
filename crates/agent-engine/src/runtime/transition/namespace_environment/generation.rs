use std::collections::BTreeMap;

use alan_agent_protocol::Event;
use alan_ap::OpenMode;
use alan_llm::{GenerationRequest, GenerationResponse};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::agent_files::{write_agent_output, write_tape_records};
use super::client::NamespaceClient;
use super::{
    NamespaceLlmCapabilities, NamespaceRuntimeEnvironment, NamespaceTurnOutput,
    NamespaceTurnRuntime,
};

#[derive(Deserialize)]
struct LlmCapabilitiesDoc {
    version: u16,
    connection: String,
    provider: String,
    capabilities: alan_llm::ProviderCapabilities,
}

impl NamespaceRuntimeEnvironment {
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
}

impl NamespaceTurnRuntime {
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

pub(super) async fn read_generation_response_with_text_events<E, F>(
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
