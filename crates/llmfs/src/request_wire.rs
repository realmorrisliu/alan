use alan_llm::{
    GenerationRequest, Message, MessageContentPart, MessageRole, ProviderCapabilities,
    ReasoningControls, ReasoningEffort, ToolCall, ToolDefinition,
};
use serde::Deserialize;

/// The provider-neutral request document written to a Generation's `data` file.
///
/// Version 2 adds typed message content so official provider adapters can preserve
/// multimodal and document input across the llmfs boundary.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRequestDocV2 {
    version: u16,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    messages: Vec<WireMessage>,
    #[serde(default)]
    tools: Vec<WireToolDefinition>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    max_tokens: Option<i32>,
    #[serde(default)]
    reasoning: WireReasoningControls,
    #[serde(default)]
    extra_params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireMessage {
    role: WireMessageRole,
    content: String,
    #[serde(default)]
    content_parts: Vec<WireMessageContentPart>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    thinking_signature: Option<String>,
    #[serde(default)]
    redacted_thinking: Option<Vec<String>>,
    #[serde(default)]
    tool_calls: Option<Vec<WireToolCall>>,
    #[serde(default)]
    tool_call_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum WireMessageContentPart {
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

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum WireMessageRole {
    System,
    User,
    Assistant,
    Tool,
    Context,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireToolCall {
    #[serde(default)]
    id: Option<String>,
    name: String,
    arguments: serde_json::Value,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireReasoningControls {
    #[serde(default)]
    effort: Option<ReasoningEffort>,
}

impl WireRequestDocV2 {
    pub(super) fn into_generation_request(
        self,
        capabilities: ProviderCapabilities,
    ) -> Result<GenerationRequest, ()> {
        if self.version != 2 || self.messages.is_empty() {
            return Err(());
        }
        let request = GenerationRequest {
            system_prompt: self.system,
            messages: self.messages.into_iter().map(Into::into).collect(),
            tools: self.tools.into_iter().map(Into::into).collect(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            reasoning: ReasoningControls {
                effort: self.reasoning.effort,
            },
            extra_params: self.extra_params.into_iter().collect(),
        };
        validate_rich_content(&request, capabilities)?;
        Ok(request)
    }
}

fn validate_rich_content(
    request: &GenerationRequest,
    capabilities: ProviderCapabilities,
) -> Result<(), ()> {
    for part in request
        .messages
        .iter()
        .flat_map(|message| &message.content_parts)
    {
        let MessageContentPart::Attachment { mime_type, .. } = part else {
            continue;
        };
        let supported = if mime_type.starts_with("image/") {
            capabilities.supports_multimodal_input
        } else {
            capabilities.supports_document_input
        };
        if !supported {
            return Err(());
        }
    }
    Ok(())
}

impl From<WireMessage> for Message {
    fn from(value: WireMessage) -> Self {
        Self {
            role: value.role.into(),
            content: value.content,
            content_parts: value.content_parts.into_iter().map(Into::into).collect(),
            thinking: value.thinking,
            thinking_signature: value.thinking_signature,
            redacted_thinking: value.redacted_thinking,
            tool_calls: value
                .tool_calls
                .map(|tool_calls| tool_calls.into_iter().map(Into::into).collect()),
            tool_call_id: value.tool_call_id,
        }
    }
}

impl From<WireMessageContentPart> for MessageContentPart {
    fn from(value: WireMessageContentPart) -> Self {
        match value {
            WireMessageContentPart::Text { text } => Self::Text { text },
            WireMessageContentPart::Attachment {
                hash,
                mime_type,
                metadata,
            } => Self::Attachment {
                hash,
                mime_type,
                metadata,
            },
            WireMessageContentPart::Structured { data } => Self::Structured { data },
        }
    }
}

impl From<WireMessageRole> for MessageRole {
    fn from(value: WireMessageRole) -> Self {
        match value {
            WireMessageRole::System => Self::System,
            WireMessageRole::User => Self::User,
            WireMessageRole::Assistant => Self::Assistant,
            WireMessageRole::Tool => Self::Tool,
            WireMessageRole::Context => Self::Context,
        }
    }
}

impl From<WireToolDefinition> for ToolDefinition {
    fn from(value: WireToolDefinition) -> Self {
        Self {
            name: value.name,
            description: value.description,
            parameters: value.parameters,
        }
    }
}

impl From<WireToolCall> for ToolCall {
    fn from(value: WireToolCall) -> Self {
        Self {
            id: value.id,
            name: value.name,
            arguments: value.arguments,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_llm::factory::ProviderType;
    use serde_json::json;

    #[test]
    fn v2_preserves_typed_attachment_content() {
        let document: WireRequestDocV2 = serde_json::from_value(json!({
            "version": 2,
            "messages": [{
                "role": "user",
                "content": "Describe [attachment: img_hash (image/png)]",
                "content_parts": [
                    {"type": "text", "text": "Describe this image"},
                    {
                        "type": "attachment",
                        "hash": "img_hash",
                        "mime_type": "image/png",
                        "metadata": {"image_url": "https://example.com/cat.png"}
                    }
                ]
            }]
        }))
        .unwrap();

        let request = document
            .into_generation_request(ProviderType::OpenAiResponses.capabilities())
            .unwrap();
        assert_eq!(
            request.messages[0].content_parts,
            vec![
                MessageContentPart::Text {
                    text: "Describe this image".to_string(),
                },
                MessageContentPart::Attachment {
                    hash: "img_hash".to_string(),
                    mime_type: "image/png".to_string(),
                    metadata: json!({"image_url": "https://example.com/cat.png"}),
                },
            ]
        );
    }

    #[test]
    fn retired_v1_is_rejected() {
        let document: WireRequestDocV2 = serde_json::from_value(json!({
            "version": 1,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .unwrap();

        assert!(
            document
                .into_generation_request(ProviderType::OpenAiResponses.capabilities())
                .is_err()
        );
    }

    #[test]
    fn unsupported_rich_content_is_rejected_before_dispatch() {
        let document: WireRequestDocV2 = serde_json::from_value(json!({
            "version": 2,
            "messages": [{
                "role": "user",
                "content": "[attachment: img_hash (image/png)]",
                "content_parts": [{
                    "type": "attachment",
                    "hash": "img_hash",
                    "mime_type": "image/png"
                }]
            }]
        }))
        .unwrap();

        assert!(
            document
                .into_generation_request(ProviderType::OpenRouter.capabilities())
                .is_err()
        );
    }
}
