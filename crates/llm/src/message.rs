use serde::{Deserialize, Serialize};

use crate::ToolCall;

const RETIRED_MESSAGE_OVERRIDE_KEYS: [&str; 3] = [
    "responses_input_items",
    "chat_completions_messages",
    "anthropic_messages",
];

/// A provider-neutral message in a generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    /// Text fallback for providers that do not support typed input parts.
    pub content: String,
    /// Typed input preserved for adapters that support multimodal or document input.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_parts: Vec<MessageContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redacted_thinking: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// A provider-neutral typed part of message input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContentPart {
    Text {
        text: String,
    },
    Attachment {
        hash: String,
        mime_type: String,
        #[serde(default)]
        metadata: serde_json::Value,
    },
    Structured {
        data: serde_json::Value,
    },
}

/// Role of the message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    Context,
}

impl Message {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    /// Create an assistant message with tool calls.
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            tool_calls: Some(tool_calls),
            ..Self::new(MessageRole::Assistant, content)
        }
    }

    /// Create a tool response message.
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(tool_call_id.into()),
            ..Self::new(MessageRole::Tool, content)
        }
    }

    /// Create reference-context input for a generation request.
    pub fn context(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Context, content)
    }

    fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            content_parts: Vec::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

pub(crate) fn reject_retired_message_overrides(
    request: &crate::GenerationRequest,
) -> anyhow::Result<()> {
    if let Some(key) = RETIRED_MESSAGE_OVERRIDE_KEYS
        .iter()
        .find(|key| request.extra_params.contains_key(**key))
    {
        anyhow::bail!(
            "`{key}` extra_param is retired; use provider-neutral `Message::content_parts`"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_provider_message_overrides_are_rejected() {
        for key in RETIRED_MESSAGE_OVERRIDE_KEYS {
            let request = crate::GenerationRequest::new()
                .with_user_message("hello")
                .with_extra_param(key, serde_json::json!([]));
            let error = reject_retired_message_overrides(&request).unwrap_err();
            assert!(error.to_string().contains(key));
        }
    }
}
