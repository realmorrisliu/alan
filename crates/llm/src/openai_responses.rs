//! OpenAI Responses API client.
//!
//! This module provides the provider surface for the OpenAI Responses API.

use anyhow::Result;
use async_trait::async_trait;
use std::time::Duration;

use crate::openai_chat_completions::{
    OpenAiChatCompletionsClient, OpenAiResponsesCompactRequest, OpenAiResponsesCompactResponse,
    OpenAiResponsesInputTokensRequest, OpenAiResponsesInputTokensResponse, OpenAiResponsesRequest,
    OpenAiResponsesResponse, convert_openai_responses_output,
};
use crate::{GenerationRequest, GenerationResponse, LlmProvider, StreamChunk};

#[cfg(test)]
use crate::openai_chat_completions::{
    OpenAiResponsesInputItem, OpenAiResponsesOutputTokensDetails, OpenAiResponsesUsage,
};

pub use crate::openai_chat_completions::{
    OpenAiResponsesCompactRequest as OpenAiResponsesCompactApiRequest,
    OpenAiResponsesCompactResponse as OpenAiResponsesCompactApiResponse,
    OpenAiResponsesFunctionCallItem as OpenAiResponsesFunctionCallEvent,
    OpenAiResponsesFunctionCallOutputItem as OpenAiResponsesFunctionCallOutputEvent,
    OpenAiResponsesInputItem as OpenAiResponsesRequestInputItem,
    OpenAiResponsesInputMessage as OpenAiResponsesRequestInputMessage,
    OpenAiResponsesInputTokensRequest as OpenAiResponsesInputTokensApiRequest,
    OpenAiResponsesInputTokensResponse as OpenAiResponsesInputTokensApiResponse,
    OpenAiResponsesOutputTokensDetails as OpenAiResponsesUsageOutputTokensDetails,
    OpenAiResponsesReasoning as OpenAiResponsesReasoningConfig,
    OpenAiResponsesReasoningInputItem as OpenAiResponsesReasoningRequestInputItem,
    OpenAiResponsesRequest as OpenAiResponsesApiRequest,
    OpenAiResponsesResponse as OpenAiResponsesApiResponse,
    OpenAiResponsesUsage as OpenAiResponsesApiUsage,
};

/// Client for the OpenAI Responses API.
pub struct OpenAiResponsesClient {
    inner: OpenAiChatCompletionsClient,
}

impl OpenAiResponsesClient {
    const BACKGROUND_POLL_INTERVAL: Duration = Duration::from_secs(2);

    pub fn with_params(api_key: &str, base_url: &str, model: &str) -> Self {
        Self {
            inner: OpenAiChatCompletionsClient::official_with_params(api_key, base_url, model),
        }
    }

    pub async fn openai_responses(
        &self,
        request: OpenAiResponsesRequest,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.openai_responses(request).await
    }

    pub async fn stream_openai_responses(
        &self,
        request: OpenAiResponsesRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        self.inner.stream_openai_responses(request, tx).await
    }

    pub async fn retrieve_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.retrieve_openai_response(response_id).await
    }

    pub async fn compact_openai_response(
        &self,
        request: OpenAiResponsesCompactRequest,
    ) -> Result<OpenAiResponsesCompactResponse> {
        self.inner.compact_openai_response(request).await
    }

    pub async fn count_openai_response_input_tokens(
        &self,
        request: OpenAiResponsesInputTokensRequest,
    ) -> Result<OpenAiResponsesInputTokensResponse> {
        self.inner.count_openai_response_input_tokens(request).await
    }

    pub async fn retrieve_openai_response_stream(
        &self,
        response_id: &str,
        starting_after: Option<u64>,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<()> {
        self.inner
            .retrieve_openai_response_stream(response_id, starting_after, tx)
            .await
    }

    pub async fn cancel_openai_response(
        &self,
        response_id: &str,
    ) -> Result<OpenAiResponsesResponse> {
        self.inner.cancel_openai_response(response_id).await
    }

    fn build_openai_responses_request(
        &self,
        request: GenerationRequest,
        stream: bool,
    ) -> Result<OpenAiResponsesRequest> {
        self.inner.build_openai_responses_request(request, stream)
    }

    #[cfg(test)]
    fn build_openai_responses_input_tokens_request(
        &self,
        request: GenerationRequest,
    ) -> Result<OpenAiResponsesInputTokensRequest> {
        self.inner
            .build_openai_responses_input_tokens_request(request)
    }

    async fn wait_for_background_response(
        &self,
        mut response: OpenAiResponsesResponse,
    ) -> Result<OpenAiResponsesResponse> {
        while matches!(response.status.as_deref(), Some("queued" | "in_progress")) {
            let response_id = response.id.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI Responses background response is missing an id while status is {:?}",
                    response.status
                )
            })?;
            tokio::time::sleep(Self::BACKGROUND_POLL_INTERVAL).await;
            response = self.retrieve_openai_response(&response_id).await?;
        }
        Ok(response)
    }
}

#[async_trait]
impl LlmProvider for OpenAiResponsesClient {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        let response_request = self.build_openai_responses_request(request, false)?;
        let background_requested = response_request.background == Some(true);
        let mut response = self.openai_responses(response_request).await?;
        if background_requested {
            response = self.wait_for_background_response(response).await?;
        }
        Ok(convert_openai_responses_output(response))
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> anyhow::Result<String> {
        let request = match system {
            Some(system) => GenerationRequest::new()
                .with_system_prompt(system)
                .with_user_message(user),
            None => GenerationRequest::new().with_user_message(user),
        };
        let response = self.generate(request).await?;
        Ok(response.content)
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let response_request = self.build_openai_responses_request(request, true)?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let client = Self {
            inner: self.inner.clone_with_same_config(),
        };
        tokio::spawn(async move {
            let _ = client.stream_openai_responses(response_request, tx).await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "openai_responses"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageRole;
    use serde_json::json;

    #[test]
    fn test_openai_responses_client_with_params() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        assert_eq!(client.provider_name(), "openai_responses");
        drop(client);
    }

    #[test]
    fn test_convert_messages_for_openai_responses_projects_tool_history() {
        let messages = vec![
            crate::Message {
                role: MessageRole::Assistant,
                content: "Let me inspect that.".to_string(),
                content_parts: Vec::new(),
                thinking: None,
                thinking_signature: Some("enc_sig".to_string()),
                redacted_thinking: None,
                tool_calls: Some(vec![crate::ToolCall {
                    id: Some("call_1".to_string()),
                    name: "lookup".to_string(),
                    arguments: json!({"query": "alan"}),
                }]),
                tool_call_id: None,
            },
            crate::Message {
                role: MessageRole::Tool,
                content: "{\"ok\":true}".to_string(),
                content_parts: Vec::new(),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
            },
        ];

        let converted =
            crate::openai_chat_completions::convert_messages_for_openai_responses(messages)
                .unwrap();
        assert_eq!(converted.len(), 4);

        match &converted[0] {
            OpenAiResponsesInputItem::Reasoning(reasoning) => {
                assert_eq!(reasoning.kind, "reasoning");
                assert_eq!(reasoning.encrypted_content, "enc_sig");
            }
            _ => panic!("expected reasoning item"),
        }

        match &converted[1] {
            OpenAiResponsesInputItem::Message(message) => {
                assert_eq!(message.role, "assistant");
                assert_eq!(message.content, json!("Let me inspect that."));
            }
            _ => panic!("expected assistant message"),
        }

        match &converted[2] {
            OpenAiResponsesInputItem::FunctionCall(tool_call) => {
                assert_eq!(tool_call.call_id, "call_1");
                assert_eq!(tool_call.name, "lookup");
                assert_eq!(tool_call.arguments, "{\"query\":\"alan\"}");
            }
            _ => panic!("expected function call"),
        }

        match &converted[3] {
            OpenAiResponsesInputItem::FunctionCallOutput(tool_output) => {
                assert_eq!(tool_output.call_id, "call_1");
                assert_eq!(tool_output.output, "{\"ok\":true}");
            }
            _ => panic!("expected function call output"),
        }
    }

    #[test]
    fn test_build_openai_responses_request_moves_system_prompt_to_instructions() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_system_prompt("System prompt")
                    .with_user_message("hello")
                    .with_extra_param("previous_response_id", json!("resp_prev"))
                    .with_extra_param("store", json!(false))
                    .with_extra_param("include", json!(["metadata.foo"])),
                false,
            )
            .unwrap();

        assert_eq!(request.instructions.as_deref(), Some("System prompt"));
        assert_eq!(request.previous_response_id.as_deref(), Some("resp_prev"));
        assert_eq!(request.store, Some(false));
        assert_eq!(
            request.include.as_deref(),
            Some(&["metadata.foo".to_string()][..])
        );
        assert_eq!(request.input.len(), 1);
        match &request.input[0] {
            OpenAiResponsesInputItem::Message(message) => {
                assert_eq!(message.role, "user");
                assert_eq!(message.content, json!("hello"));
            }
            _ => panic!("expected user message"),
        }
    }

    #[test]
    fn test_build_openai_responses_request_rejects_retired_input_override() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let error = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_extra_param("responses_input_items", json!([])),
                false,
            )
            .expect_err("retired override should fail before dispatch");

        assert!(error.to_string().contains("responses_input_items"));
        assert!(error.to_string().contains("Message::content_parts"));
    }

    #[test]
    fn test_build_openai_responses_request_background_defaults_store_true() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_extra_param("background", json!(true)),
                false,
            )
            .unwrap();

        assert_eq!(request.background, Some(true));
        assert_eq!(request.store, Some(true));
    }

    #[test]
    fn test_build_openai_responses_request_adds_reasoning_include_for_tool_loops() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_tool(crate::ToolDefinition {
                        name: "lookup".to_string(),
                        description: "Lookup a record".to_string(),
                        parameters: json!({
                            "type": "object",
                            "properties": { "query": { "type": "string" } },
                            "required": ["query"]
                        }),
                    }),
                false,
            )
            .unwrap();

        assert_eq!(
            request.include.as_deref(),
            Some(&["reasoning.encrypted_content".to_string()][..])
        );
    }

    #[test]
    fn test_build_openai_responses_request_preserves_context_management() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_context_management_compact_threshold(8192),
                false,
            )
            .unwrap();

        assert_eq!(
            request.extra_params.get("context_management"),
            Some(&json!({ "compact_threshold": 8192 }))
        );
    }

    #[test]
    fn test_build_openai_responses_input_tokens_request_uses_responses_shape() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_input_tokens_request(
                GenerationRequest::new()
                    .with_system_prompt("System prompt")
                    .with_user_message("hello")
                    .with_tool(crate::ToolDefinition {
                        name: "lookup".to_string(),
                        description: "Lookup a record".to_string(),
                        parameters: json!({
                            "type": "object",
                            "properties": { "query": { "type": "string" } },
                            "required": ["query"]
                        }),
                    })
                    .with_context_management_compact_threshold(8192)
                    .with_extra_param("store", json!(true)),
            )
            .unwrap();

        assert_eq!(request.instructions.as_deref(), Some("System prompt"));
        assert_eq!(request.input.len(), 1);
        assert!(request.tools.is_some());
        assert_eq!(request.tool_choice.as_deref(), Some("auto"));
        let tool = &request.tools.as_ref().expect("tools")[0];
        assert_eq!(tool.kind, "function");
        assert_eq!(tool.name, "lookup");
        assert_eq!(tool.description, "Lookup a record");
        assert_eq!(
            tool.parameters,
            json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            })
        );
        assert!(!request.extra_params.contains_key("store"));
        assert!(!request.extra_params.contains_key("context_management"));
    }

    #[test]
    fn test_build_openai_responses_request_uses_responses_native_tool_shape() {
        let client =
            OpenAiResponsesClient::with_params("test-key", "https://api.openai.com/v1", "gpt-5.4");
        let request = client
            .build_openai_responses_request(
                GenerationRequest::new()
                    .with_user_message("hello")
                    .with_tool(crate::ToolDefinition {
                        name: "lookup".to_string(),
                        description: "Lookup a record".to_string(),
                        parameters: json!({
                            "type": "object",
                            "properties": { "query": { "type": "string" } },
                            "required": ["query"]
                        }),
                    }),
                false,
            )
            .unwrap();

        let tool = &request.tools.as_ref().expect("tools")[0];
        assert_eq!(tool.kind, "function");
        assert_eq!(tool.name, "lookup");
        assert_eq!(tool.description, "Lookup a record");
        assert_eq!(
            tool.parameters,
            json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"]
            })
        );
    }

    #[test]
    fn test_openai_responses_input_tokens_response_deserializes_openapi_shape() {
        let response: OpenAiResponsesInputTokensResponse = serde_json::from_value(json!({
            "object": "response.input_tokens",
            "input_tokens": 11
        }))
        .expect("deserialize input token count response");

        assert_eq!(response.object.as_deref(), Some("response.input_tokens"));
        assert_eq!(response.input_tokens, 11);
    }

    #[test]
    fn test_openai_responses_compact_response_deserializes_openapi_shape() {
        let payload = json!({
            "id": "resp_001",
            "object": "response.compaction",
            "created_at": 1764967971i64,
            "output": [
                {
                    "id": "msg_000",
                    "type": "message",
                    "status": "completed",
                    "content": [{"type": "input_text", "text": "hello"}],
                    "role": "user"
                },
                {
                    "id": "cmp_001",
                    "type": "compaction",
                    "encrypted_content": "opaque"
                }
            ],
            "usage": {
                "input_tokens": 139,
                "output_tokens": 438,
                "total_tokens": 577,
                "output_tokens_details": {"reasoning_tokens": 64}
            }
        });

        let response: OpenAiResponsesCompactResponse = serde_json::from_value(payload).unwrap();
        assert_eq!(response.id.as_deref(), Some("resp_001"));
        assert_eq!(response.object.as_deref(), Some("response.compaction"));
        assert_eq!(response.created_at, Some(1764967971));
        assert_eq!(response.output.len(), 2);
        assert_eq!(response.output[1]["type"], json!("compaction"));
        assert_eq!(response.output[1]["encrypted_content"], json!("opaque"));
        assert_eq!(response.usage.unwrap().total_tokens, 577);
    }

    #[test]
    fn test_convert_openai_responses_output_extracts_final_tool_arguments() {
        let response = OpenAiResponsesResponse {
            id: Some("resp_123".to_string()),
            status: Some("completed".to_string()),
            background: Some(false),
            output: vec![
                json!({
                    "type": "reasoning",
                    "summary": [{"text": "Inspecting tool input"}],
                    "encrypted_content": "enc_reasoning"
                }),
                json!({
                    "type": "message",
                    "content": [
                        {"type": "output_text", "text": "I'll look that up."}
                    ]
                }),
                json!({
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "lookup",
                    "arguments": "{\"query\":\"alan\"}"
                }),
            ],
            usage: Some(OpenAiResponsesUsage {
                input_tokens: 11,
                input_tokens_details: Some(
                    crate::openai_chat_completions::OpenAiResponsesInputTokensDetails {
                        cached_tokens: Some(4),
                    },
                ),
                output_tokens: 22,
                total_tokens: 33,
                output_tokens_details: Some(OpenAiResponsesOutputTokensDetails {
                    reasoning_tokens: Some(7),
                }),
            }),
        };

        let converted = convert_openai_responses_output(response);
        assert_eq!(converted.content, "I'll look that up.");
        assert_eq!(converted.thinking.as_deref(), Some("Inspecting tool input"));
        assert_eq!(
            converted.thinking_signature.as_deref(),
            Some("enc_reasoning")
        );
        assert_eq!(converted.provider_response_id.as_deref(), Some("resp_123"));
        assert_eq!(
            converted.provider_response_status.as_deref(),
            Some("completed")
        );
        assert_eq!(converted.finish_reason.as_deref(), Some("tool_calls"));
        assert_eq!(converted.tool_calls.len(), 1);
        assert_eq!(converted.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(converted.tool_calls[0].name, "lookup");
        assert_eq!(converted.tool_calls[0].arguments, json!({"query": "alan"}));
        assert_eq!(
            converted.usage.map(|usage| usage.reasoning_tokens),
            Some(Some(7))
        );
        assert_eq!(
            converted.usage.map(|usage| usage.cached_prompt_tokens),
            Some(Some(4))
        );
    }
}
