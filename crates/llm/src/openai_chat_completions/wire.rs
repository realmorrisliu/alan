//! OpenAI Chat Completions and Responses API wire models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct OpenAiChatCompletionsRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiChatCompletionsToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<OpenAiChatCompletionsStreamOptions>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiChatCompletionsStreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsMessage {
    pub role: String, // system, user, assistant, tool
    pub content: Option<String>,
    /// Provider-specific reasoning/thinking content (e.g. DeepSeek `reasoning_content`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    /// Provider-specific reasoning metadata payload (e.g. encrypted reasoning state).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiChatCompletionsToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsToolDefinition {
    pub r#type: String,
    pub function: OpenAiChatCompletionsFunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponsesToolDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAiChatCompletionsFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChatCompletionsFunctionCall {
    pub name: String,
    pub arguments: String, // JSON string, needs parsing
}

// Responses API types

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    pub input: Vec<OpenAiResponsesInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiResponsesReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesReasoning {
    pub effort: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OpenAiResponsesInputItem {
    Message(OpenAiResponsesInputMessage),
    Reasoning(OpenAiResponsesReasoningInputItem),
    FunctionCall(OpenAiResponsesFunctionCallItem),
    FunctionCallOutput(OpenAiResponsesFunctionCallOutputItem),
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesInputMessage {
    pub role: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesReasoningInputItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub encrypted_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesFunctionCallItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenAiResponsesFunctionCallOutputItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub call_id: String,
    pub output: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesResponse {
    pub id: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub background: Option<bool>,
    #[serde(default)]
    pub output: Vec<serde_json::Value>,
    pub usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesInputTokensRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub input: Vec<OpenAiResponsesInputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiResponsesReasoning>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesInputTokensResponse {
    pub object: Option<String>,
    pub input_tokens: i32,
}

#[derive(Debug, Serialize)]
pub struct OpenAiResponsesCompactRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<serde_json::Value>>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra_params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesCompactResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created_at: Option<i64>,
    #[serde(default)]
    pub output: Vec<serde_json::Value>,
    pub usage: Option<OpenAiResponsesUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesUsage {
    pub input_tokens: i32,
    pub input_tokens_details: Option<OpenAiResponsesInputTokensDetails>,
    pub output_tokens: i32,
    pub total_tokens: i32,
    pub output_tokens_details: Option<OpenAiResponsesOutputTokensDetails>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesInputTokensDetails {
    pub cached_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiResponsesOutputTokensDetails {
    pub reasoning_tokens: Option<i32>,
}

// Response types

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChatCompletionsChoice>,
    pub usage: Option<OpenAiChatCompletionsUsage>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChoice {
    pub index: i32,
    pub message: OpenAiChatCompletionsMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsUsage {
    pub prompt_tokens: i32,
    pub prompt_tokens_details: Option<OpenAiChatCompletionsPromptTokensDetails>,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub completion_tokens_details: Option<OpenAiChatCompletionsCompletionTokensDetails>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsPromptTokensDetails {
    pub cached_tokens: Option<i32>,
    pub audio_tokens: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsCompletionTokensDetails {
    pub reasoning_tokens: Option<i32>,
    pub audio_tokens: Option<i32>,
    pub accepted_prediction_tokens: Option<i32>,
    pub rejected_prediction_tokens: Option<i32>,
}

// Streaming response types

/// Stream chunk from OpenAI streaming API
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChatCompletionsChunkChoice>,
    pub usage: Option<OpenAiChatCompletionsUsage>,
}

/// A choice in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsChunkChoice {
    pub index: i32,
    pub delta: OpenAiChatCompletionsDeltaMessage,
    pub finish_reason: Option<String>,
}

/// Delta message in streaming response (incremental content)
#[derive(Debug, Deserialize, Default)]
pub struct OpenAiChatCompletionsDeltaMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub reasoning: Option<serde_json::Value>,
    #[serde(default)]
    pub tool_calls: Option<Vec<OpenAiChatCompletionsStreamToolCall>>,
}

/// Tool call in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsStreamToolCall {
    pub index: i32,
    pub id: Option<String>,
    pub r#type: Option<String>,
    pub function: Option<OpenAiChatCompletionsStreamFunctionCall>,
}

/// Function call in streaming response
#[derive(Debug, Deserialize)]
pub struct OpenAiChatCompletionsStreamFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

impl OpenAiChatCompletionsMessage {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            role: "assistant".to_string(),
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a tool message (response to a tool call)
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            reasoning_content: None,
            reasoning: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

impl OpenAiChatCompletionsToolDefinition {
    /// Create a tool definition from name, description, and parameters
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            r#type: "function".to_string(),
            function: OpenAiChatCompletionsFunctionDefinition {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
            },
        }
    }
}

impl OpenAiResponsesToolDefinition {
    /// Create a Responses-native tool definition from name, description, and parameters.
    pub fn new(name: &str, description: &str, parameters: serde_json::Value) -> Self {
        Self {
            kind: "function".to_string(),
            name: name.to_string(),
            description: description.to_string(),
            parameters,
        }
    }
}
