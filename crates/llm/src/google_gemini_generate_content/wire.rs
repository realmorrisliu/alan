//! Vertex AI `GenerateContent` wire types.

use serde::{Deserialize, Serialize};

/// Request body for the Google Gemini GenerateContent API.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleGeminiGenerateContentRequest {
    /// Conversation contents
    pub contents: Vec<Content>,
    /// System instruction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<Content>,
    /// Tools for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Function-calling behaviour controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfig>,
    /// Generation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

/// Content represents a message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Role: "user" or "model" ("function" is accepted for legacy history)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Parts of the content (may be missing in some responses)
    #[serde(default)]
    pub parts: Vec<Part>,
}

/// Part of content: text, thought, media, function call, or function response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// Text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Whether this part carries a thought summary instead of answer text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    /// Opaque reasoning signature that must be replayed in the same part position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    /// Inline base64 media.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<InlineData>,
    /// URI-referenced media (for example `gs://` objects).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    /// Function call from model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    /// Function response to model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
}

/// Inline media bytes encoded as base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineData {
    pub mime_type: String,
    /// Base64-encoded bytes.
    pub data: String,
}

/// Media referenced by URI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub mime_type: String,
    pub file_uri: String,
}

/// Function call requested by the model
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCall {
    /// Provider call id, when the model supplies one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Function name (absent on continuation chunks of streamed arguments).
    #[serde(default)]
    pub name: String,
    /// Arguments as JSON object
    #[serde(default)]
    pub args: serde_json::Value,
    /// Incremental arguments when `streamFunctionCallArguments` is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_args: Option<Vec<PartialArg>>,
    /// Whether more chunks for this call follow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub will_continue: Option<bool>,
}

/// One streamed function-call argument fragment addressed by JSON path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialArg {
    #[serde(default)]
    pub json_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub null_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub will_continue: Option<bool>,
}

/// Response to a function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResponse {
    /// Provider call id this response answers, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Function name
    pub name: String,
    /// Response data
    pub response: serde_json::Value,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    /// Function declarations
    pub function_declarations: Vec<FunctionDeclaration>,
}

/// Function declaration schema
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDeclaration {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Full JSON Schema, distinct from Vertex's restricted OpenAPI Schema field.
    #[serde(rename = "parametersJsonSchema")]
    pub parameters: serde_json::Value,
}

/// Tool configuration.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_calling_config: Option<FunctionCallingConfig>,
}

/// Function-calling configuration.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionCallingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Stream function-call arguments as `partialArgs` (Gemini 3 on Vertex AI).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_function_call_arguments: Option<bool>,
}

/// Generation configuration
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    /// Temperature (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Max output tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    /// Top-P sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-K sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    /// Gemini thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    /// Output MIME type (for example `application/json`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    /// Full JSON Schema constraining structured generation output, distinct from
    /// Vertex's restricted OpenAPI `responseSchema` field.
    #[serde(rename = "responseJsonSchema", skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
}

/// Gemini thinking controls.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    /// Return thought summaries as `thought: true` parts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
}

/// Response from the Google Gemini GenerateContent API.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoogleGeminiGenerateContentResponse {
    /// Generated candidates
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    /// Usage metadata
    pub usage_metadata: Option<UsageMetadata>,
    /// Model version
    pub model_version: Option<String>,
    /// Provider response identifier.
    #[serde(default)]
    pub response_id: Option<String>,
    /// Prompt feedback (e.g., when blocked by safety filters)
    pub prompt_feedback: Option<PromptFeedback>,
}

/// A generated candidate response
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Content of the response
    pub content: Option<Content>,
    /// Why generation stopped
    pub finish_reason: Option<String>,
    /// Index of this candidate
    pub index: Option<i32>,
    /// Safety ratings for the generated content
    #[serde(default)]
    pub safety_ratings: Vec<SafetyRating>,
}

/// Prompt feedback when content is blocked
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptFeedback {
    /// Why the prompt was blocked
    pub block_reason: Option<String>,
    /// Safety ratings for the prompt
    #[serde(default)]
    pub safety_ratings: Vec<SafetyRating>,
}

/// Safety rating for content
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    /// Safety category
    pub category: String,
    /// Probability of harm
    pub probability: String,
}

/// Token usage metadata
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetadata {
    pub prompt_token_count: Option<i32>,
    pub candidates_token_count: Option<i32>,
    pub total_token_count: Option<i32>,
    /// Prompt tokens served from the context cache.
    #[serde(default)]
    pub cached_content_token_count: Option<i32>,
    /// Tokens spent on thinking (not included in `candidates_token_count`).
    #[serde(default)]
    pub thoughts_token_count: Option<i32>,
    /// Prompt tokens consumed by tool-use results.
    #[serde(default)]
    pub tool_use_prompt_token_count: Option<i32>,
}

impl UsageMetadata {
    /// Project Gemini usage into the provider-neutral shape.
    ///
    /// `completion_tokens` includes thinking tokens, matching the other adapters
    /// where reasoning tokens are a subset of completion tokens.
    pub fn to_token_usage(&self) -> crate::TokenUsage {
        let candidates = self.candidates_token_count.unwrap_or(0);
        let thoughts = self.thoughts_token_count.unwrap_or(0);
        let prompt =
            self.prompt_token_count.unwrap_or(0) + self.tool_use_prompt_token_count.unwrap_or(0);
        let completion = candidates + thoughts;
        crate::TokenUsage {
            prompt_tokens: prompt,
            cached_prompt_tokens: self.cached_content_token_count,
            completion_tokens: completion,
            total_tokens: self.total_token_count.unwrap_or(prompt + completion),
            reasoning_tokens: self.thoughts_token_count,
            ..Default::default()
        }
    }
}

impl Part {
    /// Create a text part
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// Create a function call part
    pub fn function_call(name: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            function_call: Some(FunctionCall {
                id: None,
                name: name.into(),
                args,
                partial_args: None,
                will_continue: None,
            }),
            ..Self::default()
        }
    }

    /// Create a function response part
    pub fn function_response(name: impl Into<String>, response: serde_json::Value) -> Self {
        Self {
            function_response: Some(FunctionResponse {
                id: None,
                name: name.into(),
                response,
            }),
            ..Self::default()
        }
    }

    /// Create an inline-media part.
    pub fn inline_data(mime_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            inline_data: Some(InlineData {
                mime_type: mime_type.into(),
                data: data.into(),
            }),
            ..Self::default()
        }
    }

    /// Create a URI-referenced media part.
    pub fn file_data(mime_type: impl Into<String>, file_uri: impl Into<String>) -> Self {
        Self {
            file_data: Some(FileData {
                mime_type: mime_type.into(),
                file_uri: file_uri.into(),
            }),
            ..Self::default()
        }
    }

    /// Whether this part is a thought summary.
    pub fn is_thought(&self) -> bool {
        self.thought == Some(true)
    }
}

impl Content {
    /// Create user content
    pub fn user(parts: Vec<Part>) -> Self {
        Self {
            role: Some("user".to_string()),
            parts,
        }
    }

    /// Create model content
    pub fn model(parts: Vec<Part>) -> Self {
        Self {
            role: Some("model".to_string()),
            parts,
        }
    }

    /// Create function response content (legacy `function` role).
    pub fn function(parts: Vec<Part>) -> Self {
        Self {
            role: Some("function".to_string()),
            parts,
        }
    }
}
