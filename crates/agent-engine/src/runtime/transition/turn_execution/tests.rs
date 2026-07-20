use super::*;
use crate::{
    agent_machine::{AgentMachine, TurnActivityState},
    config::Config,
    rollout::{RolloutItem, RolloutRecorder},
    runtime::{NamespaceRuntimeEnvironment, RuntimeConfig},
    skills::{ResolvedCapabilityView, ScopedPackageDir, SkillScope},
    tape::{ContentPart, Message, ToolRequest},
    tools::{Tool, ToolContext, ToolRegistry, ToolResult},
};
use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, ToolCall, ToolCallDelta,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

struct TestToolProcessRunner {
    tools: ToolRegistry,
}

impl TestToolProcessRunner {
    fn new(tools: ToolRegistry) -> Self {
        Self { tools }
    }
}

#[async_trait]
impl alan_kernel::ProcessRunner for TestToolProcessRunner {
    async fn run(&self, invocation: alan_kernel::ProcessInvocation) -> alan_kernel::ProcessOutcome {
        if invocation
            .namespace
            .resolve(&invocation.exec.executable)
            .is_err()
        {
            return alan_kernel::ProcessOutcome::exited(
                127,
                b"executable is not mounted\n".to_vec(),
            );
        }
        let tool_name = invocation
            .exec
            .executable
            .rsplit('/')
            .next()
            .unwrap_or(invocation.exec.executable.as_str());
        let arguments = invocation
            .exec
            .args
            .first()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or(serde_json::Value::Null);

        match self.tools.execute(tool_name, arguments).await {
            Ok(output) => {
                let mut bytes =
                    serde_json::to_vec(&output).unwrap_or_else(|_| b"{\"success\":true}".to_vec());
                bytes.push(b'\n');
                alan_kernel::ProcessOutcome::exited(0, bytes)
            }
            Err(err) => {
                let mut bytes = serde_json::to_vec(&serde_json::json!({
                    "success": false,
                    "error": format!("{err:#}"),
                }))
                .unwrap_or_else(|_| b"{\"success\":false}".to_vec());
                bytes.push(b'\n');
                alan_kernel::ProcessOutcome::exited(1, bytes)
            }
        }
    }
}

fn maybe_memory_promotion_response(request: &GenerationRequest) -> Option<GenerationResponse> {
    let system_prompt = request.system_prompt.as_deref()?;
    if system_prompt != crate::prompts::MEMORY_PROMOTION_PROMPT {
        return None;
    }

    let joined_user_text = request
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let content = if joined_user_text.contains("My name is Morris.") {
        serde_json::json!({
            "writes": [{
                "kind": "user_identity",
                "target": "USER.md",
                "confidence": "high",
                "disposition": "promote_now",
                "observation": "Name: Morris",
                "evidence": ["My name is Morris."],
                "promotion_rationale": "Direct user-stated stable identity detail."
            }]
        })
        .to_string()
    } else {
        serde_json::json!({ "writes": [] }).to_string()
    };

    Some(GenerationResponse {
        content,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        warnings: Vec::new(),
        provider_response_id: None,
        provider_response_status: None,
    })
}

fn response_stream(response: GenerationResponse) -> tokio::sync::mpsc::Receiver<StreamChunk> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        if !response.content.is_empty()
            || response
                .thinking
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            || response
                .thinking_signature
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            || !response.redacted_thinking.is_empty()
        {
            let mut redacted = response.redacted_thinking.into_iter();
            let _ = tx
                .send(StreamChunk {
                    text: (!response.content.is_empty()).then_some(response.content),
                    thinking: response.thinking,
                    thinking_signature: response.thinking_signature,
                    redacted_thinking: redacted.next(),
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
            for redacted in redacted {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: Some(redacted),
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: false,
                        finish_reason: None,
                    })
                    .await;
            }
        }

        let tool_calls = response.tool_calls;
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let arguments =
                serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: Some(ToolCallDelta {
                        index,
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        arguments_delta: Some(arguments.clone()),
                        arguments: Some(arguments),
                    }),
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
        }

        let finish_reason = response.finish_reason.unwrap_or_else(|| {
            if tool_calls.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        });
        let _ = tx
            .send(StreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: response.usage,
                provider_response_id: response.provider_response_id,
                provider_response_status: response.provider_response_status,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some(finish_reason),
            })
            .await;
    });
    rx
}

// Mock provider that returns content without tool calls
struct ContentMockProvider {
    content: String,
    thinking: Option<String>,
}

impl ContentMockProvider {
    fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            thinking: None,
        }
    }

    fn with_thinking(mut self, thinking: impl Into<String>) -> Self {
        self.thinking = Some(thinking.into());
        self
    }
}

#[async_trait]
impl LlmProvider for ContentMockProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response);
        }
        Ok(GenerationResponse {
            content: self.content.clone(),
            thinking: self.thinking.clone(),
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(self.content.clone())
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response_stream(response));
        }
        Ok(response_stream(GenerationResponse {
            content: self.content.clone(),
            thinking: self.thinking.clone(),
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![],
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }))
    }

    fn provider_name(&self) -> &'static str {
        "content_mock"
    }
}

struct BlockingStreamProvider {
    started: Arc<tokio::sync::Notify>,
}

#[async_trait]
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

struct PanicOnStreamProvider {
    content: String,
    generate_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl LlmProvider for PanicOnStreamProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        self.generate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(self.content.clone())
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.generate_calls.fetch_add(1, Ordering::SeqCst);
        Ok(response_stream(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }))
    }

    fn provider_name(&self) -> &'static str {
        "panic_on_stream"
    }
}

struct TransientStreamFailureProvider {
    generate_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl LlmProvider for TransientStreamFailureProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        Err(anyhow::anyhow!(
            "transient stream provider should use generate_stream"
        ))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok("transient stream mock".to_string())
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let call = self.generate_calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            return Err(anyhow::anyhow!("temporary 503 from stream"));
        }
        Ok(response_stream(GenerationResponse {
            content: "Recovered after retry.".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        }))
    }

    fn provider_name(&self) -> &'static str {
        "transient_stream_failure"
    }
}

struct PanicIfGeneratedProvider;

struct NamedRecordingStreamProvider {
    provider_name: &'static str,
    chunks: Vec<String>,
    requests: Arc<Mutex<Vec<GenerationRequest>>>,
}

impl NamedRecordingStreamProvider {
    fn content(&self) -> String {
        self.chunks.concat()
    }
}

#[async_trait]
impl LlmProvider for PanicIfGeneratedProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        panic!("namespace-backed turn must not call provider generate")
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        panic!("namespace-backed turn must not call provider chat")
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        panic!("namespace-backed turn must not call provider generate_stream")
    }

    fn provider_name(&self) -> &'static str {
        "content_mock"
    }
}

#[async_trait]
impl LlmProvider for NamedRecordingStreamProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.requests.lock().unwrap().push(request);
        Ok(GenerationResponse {
            content: self.content(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(self.content())
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.requests.lock().unwrap().push(request);
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let chunks = self.chunks.clone();
        tokio::spawn(async move {
            if chunks.is_empty() {
                let _ = tx
                    .send(StreamChunk {
                        text: None,
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: None,
                        tool_call_delta: None,
                        is_finished: true,
                        finish_reason: Some("stop".to_string()),
                    })
                    .await;
                return;
            }

            let chunk_count = chunks.len();
            for (index, chunk) in chunks.into_iter().enumerate() {
                let is_finished = index + 1 == chunk_count;
                let _ = tx
                    .send(StreamChunk {
                        text: Some(chunk),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
                        usage: None,
                        provider_response_id: None,
                        provider_response_status: None,
                        sequence_number: Some(index as u64),
                        tool_call_delta: None,
                        is_finished,
                        finish_reason: is_finished.then(|| "stop".to_string()),
                    })
                    .await;
            }
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        self.provider_name
    }
}

struct FailOnMemoryPromotionProvider {
    content: String,
}

#[async_trait]
impl LlmProvider for FailOnMemoryPromotionProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        if maybe_memory_promotion_response(&request).is_some() {
            panic!("turn execution should not synchronously call memory promotion");
        }

        Ok(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(self.content.clone())
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        if maybe_memory_promotion_response(&request).is_some() {
            panic!("turn execution should not synchronously call memory promotion");
        }

        Ok(response_stream(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }))
    }

    fn provider_name(&self) -> &'static str {
        "fail_on_memory_promotion"
    }
}

// Mock provider that returns tool calls
struct ToolCallMockProvider {
    tool_calls: Vec<ToolCall>,
    content: String,
}

impl ToolCallMockProvider {
    fn new(tool_calls: Vec<ToolCall>, content: impl Into<String>) -> Self {
        Self {
            tool_calls,
            content: content.into(),
        }
    }
}

#[async_trait]
impl LlmProvider for ToolCallMockProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response);
        }
        Ok(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: self.tool_calls.clone(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        })
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(format!("mock: {}", self.content))
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response_stream(response));
        }
        Ok(response_stream(GenerationResponse {
            content: self.content.clone(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: self.tool_calls.clone(),
            usage: None,
            finish_reason: None,
            warnings: Vec::new(),
            provider_response_id: None,
            provider_response_status: None,
        }))
    }

    fn provider_name(&self) -> &'static str {
        "tool_mock"
    }
}

struct CapturingResponsesProvider {
    requests: Arc<Mutex<Vec<GenerationRequest>>>,
    response: GenerationResponse,
    provider_name: &'static str,
}

#[async_trait]
impl LlmProvider for CapturingResponsesProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.requests.lock().unwrap().push(request);
        Ok(self.response.clone())
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok(self.response.content.clone())
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.requests.lock().unwrap().push(request);
        Ok(response_stream(self.response.clone()))
    }

    fn provider_name(&self) -> &'static str {
        self.provider_name
    }
}

struct SequenceMockProvider {
    responses: VecDeque<GenerationResponse>,
    generate_calls: Arc<AtomicUsize>,
}

impl SequenceMockProvider {
    fn new(responses: Vec<GenerationResponse>, generate_calls: Arc<AtomicUsize>) -> Self {
        Self {
            responses: responses.into(),
            generate_calls,
        }
    }
}

#[async_trait]
impl LlmProvider for SequenceMockProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.generate_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response);
        }
        self.responses
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("No more scripted responses"))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok("sequence mock".to_string())
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        self.generate_calls.fetch_add(1, Ordering::SeqCst);
        if let Some(response) = maybe_memory_promotion_response(&request) {
            return Ok(response_stream(response));
        }
        let response = self
            .responses
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("No more scripted responses"))?;
        Ok(response_stream(response))
    }

    fn provider_name(&self) -> &'static str {
        "sequence_mock"
    }
}

struct NetworkCapabilityTool;

impl Tool for NetworkCapabilityTool {
    fn name(&self) -> &str {
        "network_probe"
    }

    fn description(&self) -> &str {
        "Test tool classified as network capability."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
        Box::pin(async move { Ok(json!({"ok": true})) })
    }

    fn capability(&self, _arguments: &serde_json::Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Network
    }
}

struct ReadCapabilityTool;

impl Tool for ReadCapabilityTool {
    fn name(&self) -> &str {
        "local_probe"
    }

    fn description(&self) -> &str {
        "Test tool classified as read capability."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
        Box::pin(async move { Ok(json!({"ok": true})) })
    }

    fn capability(&self, _arguments: &serde_json::Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
}

struct LargeOutputTool {
    output: String,
}

impl LargeOutputTool {
    fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
        }
    }
}

impl Tool for LargeOutputTool {
    fn name(&self) -> &str {
        "emit_large_output"
    }

    fn description(&self) -> &str {
        "Emit a large text payload for compaction tests."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    fn execute(&self, _arguments: serde_json::Value, _ctx: &ToolContext) -> ToolResult {
        let payload = serde_json::to_value(ContentPart::text(self.output.clone())).unwrap();
        Box::pin(async move { Ok(payload) })
    }
}

fn create_test_state_with_provider<P: LlmProvider + 'static>(provider: P) -> RuntimeLoopState {
    create_test_state_with_provider_and_tools(provider, ToolRegistry::new())
}

fn create_test_state_with_provider_and_tools<P: LlmProvider + 'static>(
    provider: P,
    tools: ToolRegistry,
) -> RuntimeLoopState {
    create_test_state_with_provider_and_tools_and_shell(provider, tools).0
}

fn create_test_state_with_provider_and_tools_and_shell<P: LlmProvider + 'static>(
    provider: P,
    mut tools: ToolRegistry,
) -> (RuntimeLoopState, alan_shell::Shell) {
    let config = Config {
        openai_responses_model: "mock-model".to_string(),
        ..Default::default()
    };
    let machine = AgentMachine::new();
    let llmfs = std::sync::Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(provider));

    let mut process_namespace = alan_kernel::Namespace::new();
    process_namespace.mount(
        "/agent/1",
        alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    process_namespace.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    for tool_name in tools.list_tools() {
        process_namespace.mount(
            &format!("/bin/{tool_name}"),
            alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadOnly,
        );
        let tool = tools.get(tool_name).unwrap();
        let manifest = crate::runtime::ToolPackageManifest::from_tool(
            tool.as_ref(),
            tools.execution_timeout_secs(tool_name).unwrap_or(30),
        )
        .unwrap();
        process_namespace.mount(
            &format!("/lib/exec/{tool_name}"),
            alan_ap::InProcessTransport::new(std::sync::Arc::new(
                alan_ap::reference::MemFs::with_read_only_file(
                    "manifest",
                    serde_json::to_vec(&manifest).unwrap(),
                ),
            )),
            alan_kernel::Access::ReadOnly,
        );
    }
    let launch_context = crate::ProcessLaunchContext::new(
        process_namespace.clone(),
        alan_kernel::Credentials::user("test-agent"),
        "/mnt/source",
    )
    .unwrap()
    .with_host_mount(
        crate::HostMountGrant::new("/mnt/source", "/tmp", alan_kernel::Access::ReadWrite).unwrap(),
    );
    tools.set_default_execution_binding(
        crate::tools::ToolExecutionBinding::from_launch_context(
            &launch_context,
            std::path::PathBuf::from("/tmp/alan-turn-executor-test-scratch"),
        )
        .unwrap(),
    );
    let procfs = alan_kernel::ProcFs::new().with_runner(std::sync::Arc::new(
        TestToolProcessRunner::new(tools.clone()),
    ));
    let process_procfs = procfs.for_spawner(
        None,
        process_namespace.clone(),
        alan_kernel::Credentials::user("root-agent"),
    );
    process_namespace.mount(
        "/proc",
        alan_ap::InProcessTransport::new(std::sync::Arc::new(process_procfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_kernel::MountFs::new(
        process_namespace,
    )));
    // Keep turn-executor tests deterministic by defaulting to non-streaming unless a test
    // explicitly opts into streaming semantics.
    let runtime_config = RuntimeConfig {
        streaming_mode: crate::config::StreamingMode::Off,
        ..RuntimeConfig::default()
    };

    let state = RuntimeLoopState {
        machine,
        environment: NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default")
            .with_launch_context(launch_context),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };
    (state, alan_shell::Shell::new(root))
}

mod support;
use support::*;

mod namespace_generation;

mod host_mount_evidence;
mod turn_execution;

mod confirmation;
mod memory_surfaces;
mod recording_provider;
mod runtime_recall;

use recording_provider::RecordingToolCallProvider;

mod active_skill_context;
mod generation_failures;
mod prompt_and_tools;
