use alan_agent_protocol::{ReasoningControls, ReasoningEffort};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::message::{Message, MessageRole};

/// Compatibility/support tier for a provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityTier {
    TierAFullFidelityStateful,
    TierBFullFidelityStateless,
    TierCBestEffortCompatible,
}

/// Where provider instructions should be projected on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstructionRole {
    ResponsesInstructions,
    Developer,
    System,
    AnthropicSystem,
}

/// Runtime-visible capability matrix for a provider family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub supports_streaming_text: bool,
    pub supports_streaming_tool_calls: bool,
    pub supports_provider_response_id: bool,
    pub supports_provider_response_status: bool,
    pub supports_reasoning_text: bool,
    pub supports_reasoning_signature: bool,
    pub supports_reasoning_effort_control: bool,
    pub supports_redacted_thinking: bool,
    pub supports_multimodal_input: bool,
    pub supports_document_input: bool,
    pub supports_cached_token_usage: bool,
    pub supports_server_managed_continuation: bool,
    pub supports_background_execution: bool,
    pub supports_retrieve_cancel: bool,
    pub supports_provider_compaction: bool,
    pub instruction_role: InstructionRole,
    pub compatibility_tier: CompatibilityTier,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Token usage information
#[derive(Debug, Clone, Copy)]
pub struct TokenUsage {
    pub prompt_tokens: i32,
    pub cached_prompt_tokens: Option<i32>,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub reasoning_tokens: Option<i32>,
}

/// Unified request for generation
#[derive(Debug, Clone)]
pub struct GenerationRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    /// Canonical cross-provider reasoning controls.
    pub reasoning: ReasoningControls,
    /// Provider-specific extra parameters
    pub extra_params: HashMap<String, serde_json::Value>,
}

/// Response from generation
#[derive(Debug, Clone)]
pub struct GenerationResponse {
    pub content: String,
    pub thinking: Option<String>,
    pub thinking_signature: Option<String>,
    pub redacted_thinking: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
    /// Provider-native response identifier (for example Responses API `response.id`).
    pub provider_response_id: Option<String>,
    /// Provider-native terminal or in-flight status (for example Responses API `status`).
    pub provider_response_status: Option<String>,
    /// Provider/runtime warnings collected while assembling this response.
    pub warnings: Vec<String>,
}

/// A chunk of streaming response
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Text content (incremental)
    pub text: Option<String>,
    /// Thinking content (incremental)
    pub thinking: Option<String>,
    /// Thinking signature content (incremental or final depending on provider)
    pub thinking_signature: Option<String>,
    /// Redacted thinking block data
    pub redacted_thinking: Option<String>,
    /// Token usage (typically emitted near stream completion)
    pub usage: Option<TokenUsage>,
    /// Provider-native response identifier surfaced during streaming completion events.
    pub provider_response_id: Option<String>,
    /// Provider-native status surfaced during streaming completion events.
    pub provider_response_status: Option<String>,
    /// Provider-native stream cursor, for example Responses API `sequence_number`.
    pub sequence_number: Option<u64>,
    /// Tool call delta (for OpenAI-style streaming tool calls)
    pub tool_call_delta: Option<ToolCallDelta>,
    /// Whether this is the final chunk
    pub is_finished: bool,
    /// Finish reason if complete
    pub finish_reason: Option<String>,
}

/// Tool call delta for streaming
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: Option<String>,
    /// Complete tool-call arguments when the provider yields a finalized item.
    pub arguments: Option<String>,
}

// ============================================================================
// Builder Pattern
// ============================================================================

impl GenerationRequest {
    /// Create a new empty generation request
    pub fn new() -> Self {
        Self {
            system_prompt: None,
            messages: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            reasoning: ReasoningControls::default(),
            extra_params: HashMap::new(),
        }
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add a user message
    pub fn with_user_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::User,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add an assistant message
    pub fn with_assistant_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role: MessageRole::Assistant,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add a message with a specific role
    pub fn with_message(mut self, role: MessageRole, content: impl Into<String>) -> Self {
        self.messages.push(Message {
            role,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self
    }

    /// Add a tool definition
    pub fn with_tool(mut self, tool: ToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: i32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set canonical named reasoning effort.
    pub fn with_reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning.effort = Some(effort);
        self
    }

    /// Set canonical reasoning controls.
    pub fn with_reasoning_controls(mut self, controls: ReasoningControls) -> Self {
        self.reasoning = controls;
        self
    }

    /// Add extra provider-specific parameter
    pub fn with_extra_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra_params.insert(key.into(), value);
        self
    }

    /// Chain this request from a previous Responses API response.
    pub fn with_previous_response_id(mut self, response_id: impl Into<String>) -> Self {
        self.extra_params.insert(
            "previous_response_id".to_string(),
            serde_json::Value::String(response_id.into()),
        );
        self
    }

    /// Control whether the Responses API stores server-side state for this request.
    pub fn with_store(mut self, store: bool) -> Self {
        self.extra_params
            .insert("store".to_string(), serde_json::Value::Bool(store));
        self
    }

    /// Request asynchronous background execution on Responses-compatible providers.
    pub fn with_background(mut self, background: bool) -> Self {
        self.extra_params.insert(
            "background".to_string(),
            serde_json::Value::Bool(background),
        );
        self
    }

    /// Request additional fields in Responses API output items.
    pub fn with_include<I, S>(mut self, include: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_params.insert(
            "include".to_string(),
            serde_json::Value::Array(
                include
                    .into_iter()
                    .map(|value| serde_json::Value::String(value.into()))
                    .collect(),
            ),
        );
        self
    }

    /// Set the raw Responses API `context_management` object.
    pub fn with_context_management(mut self, context_management: serde_json::Value) -> Self {
        self.extra_params
            .insert("context_management".to_string(), context_management);
        self
    }

    /// Enable server-side compaction with a `compact_threshold`.
    pub fn with_context_management_compact_threshold(mut self, compact_threshold: u64) -> Self {
        self.extra_params.insert(
            "context_management".to_string(),
            serde_json::json!({ "compact_threshold": compact_threshold }),
        );
        self
    }
}

impl Default for GenerationRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDefinition {
    /// Create a new tool definition
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: serde_json::json!({"type": "object"}),
        }
    }

    /// Set the parameters schema
    pub fn with_parameters(mut self, params: serde_json::Value) -> Self {
        self.parameters = params;
        self
    }

    /// Add a string parameter
    pub fn with_string_param(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let params = self.parameters.as_object_mut().unwrap();

        if !params.contains_key("properties") {
            params.insert("properties".to_string(), serde_json::json!({}));
        }

        if !params.contains_key("required") {
            params.insert("required".to_string(), serde_json::json!([]));
        }

        params["properties"][&name] = serde_json::json!({
            "type": "string",
            "description": description.into()
        });

        params["required"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!(name));

        self
    }
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: None,
            name: name.into(),
            arguments,
        }
    }

    /// Set the tool call ID
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
}
