//! Persisted direct-launch workspace configuration.

use serde::{Deserialize, Serialize};

/// LLM provider type for persistence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PersistedLlmProvider {
    #[serde(rename = "google_gemini_generate_content")]
    GoogleGeminiGenerateContent,
    #[serde(rename = "chatgpt")]
    Chatgpt,
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    #[serde(rename = "openai_chat_completions")]
    OpenAiChatCompletions,
    #[serde(rename = "openai_chat_completions_compatible")]
    OpenAiChatCompletionsCompatible,
    #[serde(rename = "openrouter")]
    OpenRouter,
    #[serde(rename = "anthropic_messages")]
    AnthropicMessages,
}

/// Configuration state for a workspace
///
/// These fields are persisted so that workspace behavior
/// remains consistent across restarts.
///
/// Note: Fields using `Option` type allow distinguishing between "not set" (None)
/// and "explicitly set to 0" (Some(0)), which is important for values like
/// `tool_repeat_limit` where 0 means "disable protection".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfigState {
    // Runtime behavior settings
    /// Maximum tool loops per turn (Some(0) = unlimited, None = use default)
    ///
    /// Note: Runtime semantics are 0 = unlimited, but we use Option for persistence
    /// to distinguish "explicitly set to 0" from "not set".
    pub max_tool_loops: Option<usize>,
    /// Tool repeat limit (Some(0) = disable protection, None = use default)
    pub tool_repeat_limit: Option<usize>,
    /// LLM request timeout in seconds (Some(0) = no timeout, None = use default)
    pub llm_timeout_secs: Option<usize>,
    /// Tool execution timeout in seconds (Some(0) = no ToolRegistry timeout, None = use default)
    ///
    /// Note: Setting this to 0 disables the ToolRegistry-level timeout wrapper
    /// and built-in Firecrawl HTTP timeouts. Custom tools may still enforce
    /// their own internal timeouts.
    pub tool_timeout_secs: Option<usize>,

    // LLM provider settings (persisted for consistency)
    /// LLM provider type
    pub llm_provider: Option<PersistedLlmProvider>,
    /// Model name (provider-specific)
    pub llm_model: Option<String>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Max tokens for generation
    pub max_tokens: Option<u32>,
    /// Context window budget for compaction heuristics.
    pub context_window_tokens: Option<u32>,
    /// Deprecated hard-threshold alias for automatic compaction.
    pub compaction_trigger_ratio: Option<f32>,
    /// Utilization ratio threshold for pre-flush soft pressure.
    pub compaction_soft_trigger_ratio: Option<f32>,
    /// Utilization ratio threshold for hard compaction pressure.
    pub compaction_hard_trigger_ratio: Option<f32>,
    /// Streaming strategy (`auto`/`on`/`off`)
    pub streaming_mode: Option<crate::config::StreamingMode>,
    /// Recovery behavior when streaming is interrupted after visible output.
    pub partial_stream_recovery_mode: Option<crate::config::PartialStreamRecoveryMode>,
    /// Governance configuration
    pub governance: Option<alan_agent_protocol::GovernanceConfig>,
}
