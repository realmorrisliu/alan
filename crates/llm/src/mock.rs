use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::{
    GenerationRequest, GenerationResponse, LlmProvider, StreamChunk, TokenUsage, ToolCallDelta,
};

/// A mock LLM provider for testing
///
/// # Example
///
/// ```rust,ignore
/// use alan_llm::mock::MockLlmProvider;
///
/// let mut mock = MockLlmProvider::new()
///     .with_response(GenerationResponse::new("Hello!"));
///
/// let response = mock.generate(GenerationRequest::new()).await.unwrap();
/// assert_eq!(response.content, "Hello!");
/// ```
#[derive(Debug, Clone)]
pub struct MockLlmProvider {
    responses: Arc<std::sync::Mutex<Vec<GenerationResponse>>>,
    recorded_requests: Arc<std::sync::Mutex<Vec<GenerationRequest>>>,
    default_response: GenerationResponse,
}

impl MockLlmProvider {
    /// Create a new mock provider with a default response
    pub fn new() -> Self {
        Self {
            responses: Arc::new(std::sync::Mutex::new(Vec::new())),
            recorded_requests: Arc::new(std::sync::Mutex::new(Vec::new())),
            default_response: GenerationResponse {
                content: "Mock response".to_string(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: Vec::new(),
                tool_calls: Vec::new(),
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    cached_prompt_tokens: None,
                    completion_tokens: 5,
                    total_tokens: 15,
                    reasoning_tokens: None,
                }),
                finish_reason: None,
                provider_response_id: None,
                provider_response_status: None,
                warnings: Vec::new(),
            },
        }
    }

    /// Add a pre-programmed response
    pub fn with_response(mut self, response: GenerationResponse) -> Self {
        self.default_response = response;
        self
    }

    /// Add multiple responses (will be returned in order)
    pub fn with_responses(self, responses: Vec<GenerationResponse>) -> Self {
        if let Ok(mut guard) = self.responses.lock() {
            *guard = responses;
        }
        self
    }

    /// Get recorded requests for verification
    pub fn recorded_requests(&self) -> Vec<GenerationRequest> {
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Clear recorded requests
    pub fn clear_recorded(&self) {
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockLlmProvider {
    async fn generate(&mut self, request: GenerationRequest) -> Result<GenerationResponse> {
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request);

        let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        if responses.is_empty() {
            Ok(self.default_response.clone())
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn chat(&mut self, _system: Option<&str>, user: &str) -> Result<String> {
        Ok(format!("Mock response to: {}", user))
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.recorded_requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(request);

        let mut responses = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        let response = if responses.is_empty() {
            self.default_response.clone()
        } else {
            responses.remove(0)
        };

        let (tx, rx) = mpsc::channel(10);

        let content = response.content.clone();
        let tool_calls = response.tool_calls.clone();
        let usage = response.usage;
        let provider_response_id = response.provider_response_id.clone();
        let provider_response_status = response.provider_response_status;
        tokio::spawn(async move {
            if !content.is_empty() {
                let _ = tx
                    .send(StreamChunk {
                        text: Some(content),
                        thinking: None,
                        thinking_signature: None,
                        redacted_thinking: None,
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

            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage,
                    provider_response_id,
                    provider_response_status,
                    sequence_number: None,
                    tool_call_delta: None,
                    is_finished: true,
                    finish_reason: Some(if tool_calls.is_empty() {
                        "stop".to_string()
                    } else {
                        "tool_calls".to_string()
                    }),
                })
                .await;
        });

        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}
