use super::*;

pub(super) struct RecordingToolCallProvider {
    tool_calls: Vec<ToolCall>,
    content: String,
    seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
}

impl RecordingToolCallProvider {
    pub(super) fn new(
        tool_calls: Vec<ToolCall>,
        content: impl Into<String>,
        seen_system_prompts: Arc<std::sync::Mutex<Vec<String>>>,
    ) -> Self {
        Self {
            tool_calls,
            content: content.into(),
            seen_system_prompts,
        }
    }

    fn record_system_prompt(&self, request: &GenerationRequest) {
        if let Some(system_prompt) = request.system_prompt.as_ref() {
            self.seen_system_prompts
                .lock()
                .unwrap()
                .push(system_prompt.clone());
        }
    }
}

#[async_trait]
impl LlmProvider for RecordingToolCallProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.record_system_prompt(&request);
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
        self.record_system_prompt(&request);
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
        "recording_tool_call_mock"
    }
}
