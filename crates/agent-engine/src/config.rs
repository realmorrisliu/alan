//! Configuration management.

use crate::connections::{ConnectionsFile, ResolvedConnectionProfile, SecretStore};
use crate::models::{self, ModelCatalogProvider, ModelInfo};
use crate::paths::AlanHomePaths;
use crate::skills::{SkillOverride, merge_skill_overrides};
use alan_agent_protocol::ReasoningEffort;
use anyhow::Context;
use serde::{Deserialize, Serialize, de};
use std::path::PathBuf;
use std::sync::Arc;

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
    pub workspace_dir: Option<PathBuf>,
    pub strict_workspace: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            workspace_dir: None,
            strict_workspace: true,
        }
    }
}

/// AgentMachine durability configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DurabilityConfig {
    /// Fail startup instead of silently degrading to in-memory mode.
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LlmProvider {
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

impl LlmProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GoogleGeminiGenerateContent => "google_gemini_generate_content",
            Self::Chatgpt => "chatgpt",
            Self::OpenAiResponses => "openai_responses",
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::OpenAiChatCompletionsCompatible => "openai_chat_completions_compatible",
            Self::OpenRouter => "openrouter",
            Self::AnthropicMessages => "anthropic_messages",
        }
    }
}

impl<'de> Deserialize<'de> for LlmProvider {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const SUPPORTED: &[&str] = &[
            "google_gemini_generate_content",
            "chatgpt",
            "openai_responses",
            "openai_chat_completions",
            "openai_chat_completions_compatible",
            "openrouter",
            "anthropic_messages",
        ];

        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "google_gemini_generate_content" => Ok(Self::GoogleGeminiGenerateContent),
            "chatgpt" => Ok(Self::Chatgpt),
            "openai_responses" => Ok(Self::OpenAiResponses),
            "openai_chat_completions" => Ok(Self::OpenAiChatCompletions),
            "openai_chat_completions_compatible" => Ok(Self::OpenAiChatCompletionsCompatible),
            "openrouter" => Ok(Self::OpenRouter),
            "anthropic_messages" => Ok(Self::AnthropicMessages),
            other => Err(de::Error::unknown_variant(other, SUPPORTED)),
        }
    }
}

/// Runtime streaming behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamingMode {
    /// Use provider-native streaming when possible.
    #[default]
    Auto,
    /// Force streaming path.
    On,
    /// Force non-streaming path.
    Off,
}

/// Behavior when a streaming response is interrupted after visible output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartialStreamRecoveryMode {
    /// Attempt one non-streaming continuation pass to recover from interruption.
    #[default]
    ContinueOnce,
    /// Keep partial output and do not attempt continuation.
    Off,
}

/// Source used to load the effective global agent configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSourceKind {
    EnvOverride,
    GlobalAgentHome,
    Default,
}

/// Loaded configuration plus resolution metadata.
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub path: Option<PathBuf>,
    pub source: ConfigSourceKind,
}

#[derive(Debug, Default, Deserialize)]
struct SkillOverrideOverlayFile {
    #[serde(default)]
    skill_overrides: Vec<SkillOverride>,
}

impl LoadedConfig {
    pub fn into_config(self) -> Config {
        self.config
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    // ========================================================================
    // Connection Profile Selection
    // ========================================================================
    /// Canonical operator-facing connection profile reference.
    #[serde(default)]
    pub connection_profile: Option<String>,

    // ========================================================================
    // Internal resolved provider configuration
    // ========================================================================
    /// Active LLM provider resolved from the selected connection profile.
    #[serde(skip, default = "default_llm_provider")]
    pub llm_provider: LlmProvider,

    // ========================================================================
    // Google Gemini GenerateContent API Configuration
    // ========================================================================
    /// GOOGLE_GEMINI_GENERATE_CONTENT_PROJECT_ID
    #[serde(skip, default)]
    pub google_gemini_generate_content_project_id: Option<String>,

    /// GOOGLE_GEMINI_GENERATE_CONTENT_LOCATION (default: us-central1)
    #[serde(skip, default = "default_google_gemini_generate_content_location")]
    pub google_gemini_generate_content_location: String,

    /// GOOGLE_GEMINI_GENERATE_CONTENT_MODEL (default: gemini-2.0-flash)
    #[serde(skip, default = "default_google_gemini_generate_content_model")]
    pub google_gemini_generate_content_model: String,

    // ========================================================================
    // OpenAI Responses API Configuration
    // ========================================================================
    /// OPENAI_RESPONSES_API_KEY
    #[serde(skip, default)]
    pub openai_responses_api_key: Option<String>,

    /// OPENAI_RESPONSES_BASE_URL (default: <https://api.openai.com/v1>)
    #[serde(skip, default = "default_openai_responses_base_url")]
    pub openai_responses_base_url: String,

    /// OPENAI_RESPONSES_MODEL (default: gpt-5.4)
    #[serde(skip, default = "default_openai_responses_model")]
    pub openai_responses_model: String,

    // ========================================================================
    // ChatGPT/Codex Managed Auth Configuration
    // ========================================================================
    /// CHATGPT_BASE_URL (default: <https://chatgpt.com/backend-api/codex>)
    #[serde(skip, default = "default_chatgpt_base_url")]
    pub chatgpt_base_url: String,

    /// CHATGPT_MODEL (default: gpt-5.3-codex)
    #[serde(skip, default = "default_chatgpt_model")]
    pub chatgpt_model: String,

    /// Optional ChatGPT account/workspace id enforced before request dispatch.
    #[serde(skip, default)]
    pub chatgpt_account_id: Option<String>,

    // ========================================================================
    // OpenAI Chat Completions API Configuration
    // ========================================================================
    /// OPENAI_CHAT_COMPLETIONS_API_KEY
    #[serde(skip, default)]
    pub openai_chat_completions_api_key: Option<String>,

    /// OPENAI_CHAT_COMPLETIONS_BASE_URL (default: <https://api.openai.com/v1>)
    #[serde(skip, default = "default_openai_chat_completions_base_url")]
    pub openai_chat_completions_base_url: String,

    /// OPENAI_CHAT_COMPLETIONS_MODEL (default: gpt-5.4)
    #[serde(skip, default = "default_openai_chat_completions_model")]
    pub openai_chat_completions_model: String,

    // ========================================================================
    // OpenAI Chat Completions API-compatible Configuration
    // ========================================================================
    /// OPENAI_CHAT_COMPLETIONS_COMPATIBLE_API_KEY
    #[serde(skip, default)]
    pub openai_chat_completions_compatible_api_key: Option<String>,

    /// OPENAI_CHAT_COMPLETIONS_COMPATIBLE_BASE_URL (default: <https://api.openai.com/v1>)
    #[serde(skip, default = "default_openai_chat_completions_compatible_base_url")]
    pub openai_chat_completions_compatible_base_url: String,

    /// OPENAI_CHAT_COMPLETIONS_COMPATIBLE_MODEL (default: qwen3.5-plus)
    #[serde(skip, default = "default_openai_chat_completions_compatible_model")]
    pub openai_chat_completions_compatible_model: String,

    // ========================================================================
    // OpenRouter Configuration
    // ========================================================================
    /// OPENROUTER_API_KEY
    #[serde(skip, default)]
    pub openrouter_api_key: Option<String>,

    /// OPENROUTER_BASE_URL (default: <https://openrouter.ai/api/v1>)
    #[serde(skip, default = "default_openrouter_base_url")]
    pub openrouter_base_url: String,

    /// OPENROUTER_MODEL
    #[serde(skip, default = "default_openrouter_model")]
    pub openrouter_model: String,

    /// OPENROUTER_HTTP_REFERER
    #[serde(skip, default)]
    pub openrouter_http_referer: Option<String>,

    /// OPENROUTER_X_TITLE
    #[serde(skip, default)]
    pub openrouter_x_title: Option<String>,

    /// OPENROUTER_APP_CATEGORIES
    #[serde(skip, default)]
    pub openrouter_app_categories: Vec<String>,

    // ========================================================================
    // Anthropic Messages API Configuration
    // ========================================================================
    /// ANTHROPIC_MESSAGES_API_KEY
    #[serde(skip, default)]
    pub anthropic_messages_api_key: Option<String>,

    /// ANTHROPIC_MESSAGES_BASE_URL (default: <https://api.anthropic.com/v1>)
    #[serde(skip, default = "default_anthropic_messages_base_url")]
    pub anthropic_messages_base_url: String,

    /// ANTHROPIC_MESSAGES_MODEL (default: claude-3-5-sonnet-latest)
    #[serde(skip, default = "default_anthropic_messages_model")]
    pub anthropic_messages_model: String,

    /// ANTHROPIC_MESSAGES_CLIENT_NAME - Client name for usage tracking (e.g., "marco")
    #[serde(skip, default)]
    pub anthropic_messages_client_name: Option<String>,

    /// ANTHROPIC_MESSAGES_USER_AGENT - Custom User-Agent header
    #[serde(skip, default)]
    pub anthropic_messages_user_agent: Option<String>,

    /// LLM request timeout in seconds
    #[serde(default = "default_llm_timeout_secs")]
    pub llm_request_timeout_secs: usize,

    /// Tool execution timeout in seconds
    #[serde(default = "default_tool_timeout_secs")]
    pub tool_timeout_secs: usize,

    /// Optional hard limit for tool-call loop iterations in a single turn.
    /// `None` means unlimited (default).
    #[serde(default)]
    pub max_tool_loops: Option<usize>,

    /// Consecutive identical tool-call guard.
    /// Set to 0 to disable this guard.
    #[serde(default = "default_tool_repeat_limit")]
    pub tool_repeat_limit: usize,

    /// Optional prompt context window budget used for compaction heuristics.
    /// When omitted, alan prefers curated model metadata and only falls back
    /// conservatively before provider validation runs.
    #[serde(default)]
    pub context_window_tokens: Option<u32>,

    /// Deprecated alias for the hard utilization ratio threshold.
    #[serde(default)]
    pub compaction_trigger_ratio: Option<f32>,

    /// Utilization ratio of the context window at which automatic compaction
    /// should first attempt a silent memory flush.
    #[serde(default)]
    pub compaction_soft_trigger_ratio: Option<f32>,

    /// Utilization ratio of the context window at which automatic compaction
    /// becomes mandatory.
    #[serde(default)]
    pub compaction_hard_trigger_ratio: Option<f32>,

    // ========================================================================
    // Prompt Logging
    // ========================================================================
    /// Enable prompt snapshot logging for observability
    #[serde(default)]
    pub prompt_snapshot_enabled: bool,

    /// Max characters to include in prompt snapshots
    #[serde(default = "default_prompt_snapshot_max_chars")]
    pub prompt_snapshot_max_chars: usize,

    // ========================================================================
    // Thinking / Reasoning Controls
    // ========================================================================
    #[serde(default, rename = "thinking_budget_tokens", skip_serializing)]
    pub(crate) deprecated_thinking_budget_tokens: Option<u32>,

    /// Named cross-provider model reasoning effort. None = use model/provider default.
    #[serde(default)]
    pub model_reasoning_effort: Option<ReasoningEffort>,

    /// Streaming strategy (`auto`/`on`/`off`).
    #[serde(default = "default_streaming_mode")]
    pub streaming_mode: StreamingMode,

    /// Recovery strategy when streaming is interrupted after visible output.
    #[serde(default = "default_partial_stream_recovery_mode")]
    pub partial_stream_recovery_mode: PartialStreamRecoveryMode,

    // ========================================================================
    // Memory Configuration
    // ========================================================================
    #[serde(default)]
    pub memory: MemoryConfig,

    // ========================================================================
    // Durability Configuration
    // ========================================================================
    #[serde(default)]
    pub durability: DurabilityConfig,

    /// Agent-root skill exposure override metadata.
    #[doc(hidden)]
    #[serde(default)]
    pub skill_overrides: Vec<SkillOverride>,

    /// Resolved model metadata catalog (bundled or overlay-merged).
    #[doc(hidden)]
    #[serde(skip)]
    pub model_catalog: Option<Arc<crate::ModelCatalog>>,
}

fn default_llm_provider() -> LlmProvider {
    LlmProvider::OpenAiResponses
}

fn default_openai_responses_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_openai_responses_model() -> String {
    models::default_model_slug(ModelCatalogProvider::OpenAiResponses).to_string()
}

fn default_chatgpt_base_url() -> String {
    "https://chatgpt.com/backend-api/codex".to_string()
}

fn default_chatgpt_model() -> String {
    "gpt-5.3-codex".to_string()
}

fn default_google_gemini_generate_content_location() -> String {
    "us-central1".to_string()
}

fn default_google_gemini_generate_content_model() -> String {
    "gemini-2.0-flash".to_string()
}

fn default_openai_chat_completions_compatible_base_url() -> String {
    default_openai_responses_base_url()
}

fn default_openai_chat_completions_base_url() -> String {
    default_openai_responses_base_url()
}

fn default_openai_chat_completions_model() -> String {
    models::default_model_slug(ModelCatalogProvider::OpenAiChatCompletions).to_string()
}

fn default_openai_chat_completions_compatible_model() -> String {
    models::default_model_slug(ModelCatalogProvider::OpenAiChatCompletionsCompatible).to_string()
}

fn default_openrouter_base_url() -> String {
    alan_llm::openrouter::OPENROUTER_BASE_URL.to_string()
}

fn default_openrouter_model() -> String {
    "moonshotai/kimi-k2.6".to_string()
}

fn default_anthropic_messages_base_url() -> String {
    "https://api.anthropic.com/v1".to_string()
}

fn default_anthropic_messages_model() -> String {
    "claude-3-5-sonnet-latest".to_string()
}

fn default_llm_timeout_secs() -> usize {
    180
}

fn default_tool_timeout_secs() -> usize {
    30
}

fn default_prompt_snapshot_max_chars() -> usize {
    8000
}

fn default_tool_repeat_limit() -> usize {
    4
}

fn default_compaction_trigger_ratio() -> f32 {
    0.8
}

fn default_streaming_mode() -> StreamingMode {
    StreamingMode::Auto
}

fn default_partial_stream_recovery_mode() -> PartialStreamRecoveryMode {
    PartialStreamRecoveryMode::ContinueOnce
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connection_profile: None,
            llm_provider: default_llm_provider(),
            google_gemini_generate_content_project_id: None,
            google_gemini_generate_content_location:
                default_google_gemini_generate_content_location(),
            google_gemini_generate_content_model: default_google_gemini_generate_content_model(),
            openai_responses_api_key: None,
            openai_responses_base_url: default_openai_responses_base_url(),
            openai_responses_model: default_openai_responses_model(),
            chatgpt_base_url: default_chatgpt_base_url(),
            chatgpt_model: default_chatgpt_model(),
            chatgpt_account_id: None,
            openai_chat_completions_api_key: None,
            openai_chat_completions_base_url: default_openai_chat_completions_base_url(),
            openai_chat_completions_model: default_openai_chat_completions_model(),
            openai_chat_completions_compatible_api_key: None,
            openai_chat_completions_compatible_base_url:
                default_openai_chat_completions_compatible_base_url(),
            openai_chat_completions_compatible_model:
                default_openai_chat_completions_compatible_model(),
            openrouter_api_key: None,
            openrouter_base_url: default_openrouter_base_url(),
            openrouter_model: default_openrouter_model(),
            openrouter_http_referer: None,
            openrouter_x_title: None,
            openrouter_app_categories: Vec::new(),
            anthropic_messages_api_key: None,
            anthropic_messages_base_url: default_anthropic_messages_base_url(),
            anthropic_messages_model: default_anthropic_messages_model(),
            anthropic_messages_client_name: None,
            anthropic_messages_user_agent: None,
            llm_request_timeout_secs: default_llm_timeout_secs(),
            tool_timeout_secs: default_tool_timeout_secs(),
            max_tool_loops: None,
            tool_repeat_limit: default_tool_repeat_limit(),
            context_window_tokens: None,
            compaction_trigger_ratio: None,
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            prompt_snapshot_enabled: false,
            prompt_snapshot_max_chars: default_prompt_snapshot_max_chars(),
            deprecated_thinking_budget_tokens: None,
            model_reasoning_effort: None,
            streaming_mode: default_streaming_mode(),
            partial_stream_recovery_mode: default_partial_stream_recovery_mode(),

            memory: MemoryConfig::default(),
            durability: DurabilityConfig::default(),
            skill_overrides: Vec::new(),
            model_catalog: None,
        }
    }
}

impl Config {
    pub fn reset_internal_provider_config(&mut self) {
        self.llm_provider = default_llm_provider();
        self.google_gemini_generate_content_project_id = None;
        self.google_gemini_generate_content_location =
            default_google_gemini_generate_content_location();
        self.google_gemini_generate_content_model = default_google_gemini_generate_content_model();
        self.openai_responses_api_key = None;
        self.openai_responses_base_url = default_openai_responses_base_url();
        self.openai_responses_model = default_openai_responses_model();
        self.chatgpt_base_url = default_chatgpt_base_url();
        self.chatgpt_model = default_chatgpt_model();
        self.chatgpt_account_id = None;
        self.openai_chat_completions_api_key = None;
        self.openai_chat_completions_base_url = default_openai_chat_completions_base_url();
        self.openai_chat_completions_model = default_openai_chat_completions_model();
        self.openai_chat_completions_compatible_api_key = None;
        self.openai_chat_completions_compatible_base_url =
            default_openai_chat_completions_compatible_base_url();
        self.openai_chat_completions_compatible_model =
            default_openai_chat_completions_compatible_model();
        self.openrouter_api_key = None;
        self.openrouter_base_url = default_openrouter_base_url();
        self.openrouter_model = default_openrouter_model();
        self.openrouter_http_referer = None;
        self.openrouter_x_title = None;
        self.openrouter_app_categories.clear();
        self.anthropic_messages_api_key = None;
        self.anthropic_messages_base_url = default_anthropic_messages_base_url();
        self.anthropic_messages_model = default_anthropic_messages_model();
        self.anthropic_messages_client_name = None;
        self.anthropic_messages_user_agent = None;
    }

    fn copy_internal_provider_config_from(&mut self, other: &Self) {
        self.llm_provider = other.llm_provider;
        self.google_gemini_generate_content_project_id =
            other.google_gemini_generate_content_project_id.clone();
        self.google_gemini_generate_content_location =
            other.google_gemini_generate_content_location.clone();
        self.google_gemini_generate_content_model =
            other.google_gemini_generate_content_model.clone();
        self.openai_responses_api_key = other.openai_responses_api_key.clone();
        self.openai_responses_base_url = other.openai_responses_base_url.clone();
        self.openai_responses_model = other.openai_responses_model.clone();
        self.chatgpt_base_url = other.chatgpt_base_url.clone();
        self.chatgpt_model = other.chatgpt_model.clone();
        self.chatgpt_account_id = other.chatgpt_account_id.clone();
        self.openai_chat_completions_api_key = other.openai_chat_completions_api_key.clone();
        self.openai_chat_completions_base_url = other.openai_chat_completions_base_url.clone();
        self.openai_chat_completions_model = other.openai_chat_completions_model.clone();
        self.openai_chat_completions_compatible_api_key =
            other.openai_chat_completions_compatible_api_key.clone();
        self.openai_chat_completions_compatible_base_url =
            other.openai_chat_completions_compatible_base_url.clone();
        self.openai_chat_completions_compatible_model =
            other.openai_chat_completions_compatible_model.clone();
        self.openrouter_api_key = other.openrouter_api_key.clone();
        self.openrouter_base_url = other.openrouter_base_url.clone();
        self.openrouter_model = other.openrouter_model.clone();
        self.openrouter_http_referer = other.openrouter_http_referer.clone();
        self.openrouter_x_title = other.openrouter_x_title.clone();
        self.openrouter_app_categories = other.openrouter_app_categories.clone();
        self.anthropic_messages_api_key = other.anthropic_messages_api_key.clone();
        self.anthropic_messages_base_url = other.anthropic_messages_base_url.clone();
        self.anthropic_messages_model = other.anthropic_messages_model.clone();
        self.anthropic_messages_client_name = other.anthropic_messages_client_name.clone();
        self.anthropic_messages_user_agent = other.anthropic_messages_user_agent.clone();
    }

    pub fn resolve_connection_profile(
        &mut self,
        home_paths: Option<&AlanHomePaths>,
    ) -> anyhow::Result<ResolvedConnectionProfile> {
        let home_paths = home_paths
            .cloned()
            .or_else(AlanHomePaths::detect)
            .ok_or_else(|| anyhow::anyhow!("Could not determine alan home directory"))?;
        let (connections, _) = ConnectionsFile::load_from_home_paths(&home_paths)?;
        let secret_store = SecretStore::from_home_paths(&home_paths)?;
        let selected_profile = self.connection_profile.clone();
        connections.apply_profile_to_config(selected_profile.as_deref(), &secret_store, self)
    }

    /// Load agent-facing configuration from `ALAN_CONFIG_PATH` or `~/.alan/agents/default/agent.toml`.
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::load_with_metadata()?.into_config())
    }

    /// Load agent-facing configuration together with source metadata.
    pub fn load_with_metadata() -> anyhow::Result<LoadedConfig> {
        Self::load_with_paths(
            Self::env_override_config_path(),
            Self::global_agent_config_file_path(),
        )
    }

    /// Load configuration from file (TOML format)
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read configuration file {}", path.display()))?;
        let mut config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse configuration file {}", path.display()))?;
        config.skill_overrides = config.resolved_skill_overrides();
        config.validate_compaction_thresholds(path.display().to_string())?;
        config.validate_reasoning_controls(path.display().to_string())?;
        Ok(config)
    }

    /// Get the config file path.
    /// Resolution order:
    /// 1. `ALAN_CONFIG_PATH` override
    /// 2. `~/.alan/agents/default/agent.toml`
    pub fn config_file_path() -> Option<std::path::PathBuf> {
        Self::resolve_config_file_path(
            Self::env_override_config_path(),
            Self::global_agent_config_file_path(),
        )
    }

    fn env_override_config_path() -> Option<std::path::PathBuf> {
        std::env::var("ALAN_CONFIG_PATH")
            .ok()
            .map(std::path::PathBuf::from)
    }

    fn global_agent_config_file_path() -> Option<std::path::PathBuf> {
        AlanHomePaths::detect().map(|paths| paths.global_agent_config_path)
    }

    #[cfg(test)]
    fn global_agent_config_file_path_from_home(
        home: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        Some(AlanHomePaths::from_home_dir(home).global_agent_config_path)
    }

    fn resolve_config_file_path(
        override_path: Option<std::path::PathBuf>,
        global_agent_path: Option<std::path::PathBuf>,
    ) -> Option<std::path::PathBuf> {
        if let Some(path) = override_path {
            return Some(path);
        }

        if let Some(path) = global_agent_path
            && path.exists()
        {
            return Some(path);
        }

        None
    }

    fn load_with_paths(
        override_path: Option<std::path::PathBuf>,
        global_agent_path: Option<std::path::PathBuf>,
    ) -> anyhow::Result<LoadedConfig> {
        if let Some(config_path) = override_path
            && config_path.exists()
        {
            let config = Self::from_file(&config_path)?;
            tracing::info!(path = %config_path.display(), "Loaded configuration from file");
            return Ok(LoadedConfig {
                config,
                path: Some(config_path),
                source: ConfigSourceKind::EnvOverride,
            });
        }

        if let Some(config_path) = global_agent_path
            && config_path.exists()
        {
            let config = Self::from_file(&config_path)?;
            tracing::info!(path = %config_path.display(), "Loaded configuration from file");
            return Ok(LoadedConfig {
                config,
                path: Some(config_path),
                source: ConfigSourceKind::GlobalAgentHome,
            });
        }

        Ok(LoadedConfig {
            config: Self::default(),
            path: None,
            source: ConfigSourceKind::Default,
        })
    }

    pub fn with_agent_root_overlays(
        &self,
        overlay_paths: &[std::path::PathBuf],
    ) -> anyhow::Result<Self> {
        let model_catalog = self.model_catalog.clone();
        let mut merged = toml::Value::try_from(self.clone())
            .context("failed to serialize base configuration for overlay merge")?;

        for path in overlay_paths {
            if !path.exists() {
                continue;
            }

            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read configuration file {}", path.display()))?;
            let overlay: toml::Value = toml::from_str(&content).with_context(|| {
                format!("failed to parse configuration file {}", path.display())
            })?;
            let mut overlay = overlay;
            strip_skill_overrides_from_overlay(&mut overlay);
            merge_toml_overlay(&mut merged, overlay);
        }

        let mut config: Self = merged
            .try_into()
            .context("failed to deserialize merged agent-root configuration")?;
        if config.connection_profile == self.connection_profile {
            config.copy_internal_provider_config_from(self);
        } else if config.connection_profile.is_none() {
            config.connection_profile = self.connection_profile.clone();
            config.copy_internal_provider_config_from(self);
        }
        config.skill_overrides = merge_skill_override_overlays_from_paths(
            &self.resolved_skill_overrides(),
            overlay_paths,
        )?;
        config.model_catalog = model_catalog;
        config.validate_compaction_thresholds("merged agent-root configuration".to_string())?;
        config.validate_reasoning_controls("merged agent-root configuration".to_string())?;
        Ok(config)
    }

    pub fn resolved_skill_overrides(&self) -> Vec<SkillOverride> {
        self.skill_overrides.clone()
    }

    pub fn for_google_gemini_generate_content(
        project_id: &str,
        location: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::GoogleGeminiGenerateContent,
            google_gemini_generate_content_project_id: Some(project_id.to_string()),
            google_gemini_generate_content_location: location
                .map(ToString::to_string)
                .unwrap_or_else(default_google_gemini_generate_content_location),
            google_gemini_generate_content_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_google_gemini_generate_content_model),
            ..Self::default()
        }
    }

    pub fn for_openai_responses(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: Some(api_key.to_string()),
            openai_responses_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_responses_base_url),
            openai_responses_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_responses_model),
            ..Self::default()
        }
    }

    pub fn for_chatgpt(base_url: Option<&str>, model: Option<&str>) -> Self {
        Self {
            llm_provider: LlmProvider::Chatgpt,
            chatgpt_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_chatgpt_base_url),
            chatgpt_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_chatgpt_model),
            chatgpt_account_id: None,
            ..Self::default()
        }
    }

    pub fn for_openai_chat_completions(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiChatCompletions,
            openai_chat_completions_api_key: Some(api_key.to_string()),
            openai_chat_completions_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_base_url),
            openai_chat_completions_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_model),
            ..Self::default()
        }
    }

    pub fn for_openai_chat_completions_compatible(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenAiChatCompletionsCompatible,
            openai_chat_completions_compatible_api_key: Some(api_key.to_string()),
            openai_chat_completions_compatible_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_compatible_base_url),
            openai_chat_completions_compatible_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openai_chat_completions_compatible_model),
            ..Self::default()
        }
    }

    pub fn for_openrouter(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
        http_referer: Option<&str>,
        x_title: Option<&str>,
        app_categories: Vec<String>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::OpenRouter,
            openrouter_api_key: Some(api_key.to_string()),
            openrouter_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_openrouter_base_url),
            openrouter_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_openrouter_model),
            openrouter_http_referer: http_referer.map(ToString::to_string),
            openrouter_x_title: x_title.map(ToString::to_string),
            openrouter_app_categories: app_categories,
            ..Self::default()
        }
    }

    pub fn for_anthropic_messages(
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self {
            llm_provider: LlmProvider::AnthropicMessages,
            anthropic_messages_api_key: Some(api_key.to_string()),
            anthropic_messages_base_url: base_url
                .map(ToString::to_string)
                .unwrap_or_else(default_anthropic_messages_base_url),
            anthropic_messages_model: model
                .map(ToString::to_string)
                .unwrap_or_else(default_anthropic_messages_model),
            ..Self::default()
        }
    }

    pub fn has_google_gemini_generate_content_config(&self) -> bool {
        self.google_gemini_generate_content_project_id.is_some()
    }

    pub fn has_openai_responses_config(&self) -> bool {
        self.openai_responses_api_key.is_some()
    }

    pub fn has_openai_chat_completions_config(&self) -> bool {
        self.openai_chat_completions_api_key.is_some()
    }

    pub fn has_openai_chat_completions_compatible_config(&self) -> bool {
        self.openai_chat_completions_compatible_api_key.is_some()
    }

    pub fn has_openrouter_config(&self) -> bool {
        self.openrouter_api_key.is_some() && !self.openrouter_model.trim().is_empty()
    }

    pub fn has_anthropic_messages_config(&self) -> bool {
        self.anthropic_messages_api_key.is_some()
    }

    pub fn has_llm_config(&self) -> bool {
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                self.has_google_gemini_generate_content_config()
            }
            LlmProvider::Chatgpt => true,
            LlmProvider::OpenAiResponses => self.has_openai_responses_config(),
            LlmProvider::OpenAiChatCompletions => self.has_openai_chat_completions_config(),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.has_openai_chat_completions_compatible_config()
            }
            LlmProvider::OpenRouter => self.has_openrouter_config(),
            LlmProvider::AnthropicMessages => self.has_anthropic_messages_config(),
        }
    }

    pub fn effective_model(&self) -> &str {
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => &self.google_gemini_generate_content_model,
            LlmProvider::Chatgpt => &self.chatgpt_model,
            LlmProvider::OpenAiResponses => self.resolved_openai_responses_model(),
            LlmProvider::OpenAiChatCompletions => self.resolved_openai_chat_completions_model(),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                &self.openai_chat_completions_compatible_model
            }
            LlmProvider::OpenRouter => &self.openrouter_model,
            LlmProvider::AnthropicMessages => &self.anthropic_messages_model,
        }
    }

    pub fn set_effective_model(&mut self, model: impl Into<String>) {
        let model = model.into();
        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                self.google_gemini_generate_content_model = model;
            }
            LlmProvider::Chatgpt => {
                self.chatgpt_model = model;
            }
            LlmProvider::OpenAiResponses => {
                self.openai_responses_model = model;
            }
            LlmProvider::OpenAiChatCompletions => {
                self.openai_chat_completions_model = model;
            }
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.openai_chat_completions_compatible_model = model;
            }
            LlmProvider::OpenRouter => {
                self.openrouter_model = model;
            }
            LlmProvider::AnthropicMessages => {
                self.anthropic_messages_model = model;
            }
        }
    }

    pub fn set_model_catalog(&mut self, model_catalog: Arc<crate::ModelCatalog>) {
        self.model_catalog = Some(model_catalog);
    }

    pub fn effective_model_info(&self) -> Option<&ModelInfo> {
        match self.llm_provider {
            LlmProvider::Chatgpt => None,
            LlmProvider::OpenAiResponses => self.resolved_model_catalog().find_model_info(
                ModelCatalogProvider::OpenAiResponses,
                self.resolved_openai_responses_model(),
            ),
            LlmProvider::OpenAiChatCompletions => self.resolved_model_catalog().find_model_info(
                ModelCatalogProvider::OpenAiChatCompletions,
                self.resolved_openai_chat_completions_model(),
            ),
            LlmProvider::OpenAiChatCompletionsCompatible => {
                self.resolved_model_catalog().find_model_info(
                    ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                    &self.openai_chat_completions_compatible_model,
                )
            }
            LlmProvider::OpenRouter
            | LlmProvider::GoogleGeminiGenerateContent
            | LlmProvider::AnthropicMessages => None,
        }
    }

    pub fn effective_context_window_tokens(&self) -> u32 {
        self.context_window_tokens
            .or_else(|| {
                self.effective_model_info()
                    .map(|model_info| model_info.context_window_tokens)
            })
            .unwrap_or_else(|| inferred_context_window_tokens(self.llm_provider))
    }

    pub fn effective_compaction_hard_trigger_ratio(&self) -> f32 {
        self.compaction_hard_trigger_ratio
            .or(self.compaction_trigger_ratio)
            .unwrap_or_else(default_compaction_trigger_ratio)
    }

    pub fn effective_compaction_soft_trigger_ratio(&self) -> f32 {
        self.compaction_soft_trigger_ratio
            .unwrap_or_else(|| self.effective_compaction_hard_trigger_ratio() * 0.9)
    }

    fn resolved_openai_responses_model(&self) -> &str {
        &self.openai_responses_model
    }

    fn resolved_openai_chat_completions_model(&self) -> &str {
        &self.openai_chat_completions_model
    }

    /// Convert to LLM provider configuration
    pub fn to_provider_config(&self) -> anyhow::Result<crate::llm::ProviderConfig> {
        use crate::llm::factory::ProviderConfig;

        match self.llm_provider {
            LlmProvider::GoogleGeminiGenerateContent => {
                let project_id = self
                    .google_gemini_generate_content_project_id
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Google Gemini GenerateContent API provider requires google_gemini_generate_content_project_id"
                        )
                    })?;
                Ok(ProviderConfig::google_gemini_generate_content(
                    project_id,
                    &self.google_gemini_generate_content_model,
                )
                .with_location(&self.google_gemini_generate_content_location))
            }
            LlmProvider::Chatgpt => {
                let mut provider_config = ProviderConfig::chatgpt(&self.chatgpt_model)
                    .with_base_url(&self.chatgpt_base_url);
                if let Some(account_id) = &self.chatgpt_account_id {
                    provider_config = provider_config.with_chatgpt_account_id(account_id);
                }
                Ok(provider_config)
            }
            LlmProvider::OpenAiResponses => {
                let api_key = self.openai_responses_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "OpenAI Responses API provider requires openai_responses_api_key"
                    )
                })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Responses API",
                    ModelCatalogProvider::OpenAiResponses,
                    self.resolved_openai_responses_model(),
                )?;
                Ok(ProviderConfig::openai_responses(
                    api_key,
                    self.resolved_openai_responses_model(),
                )
                .with_base_url(&self.openai_responses_base_url))
            }
            LlmProvider::OpenAiChatCompletions => {
                let api_key = self
                    .openai_chat_completions_api_key
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "OpenAI Chat Completions API provider requires openai_chat_completions_api_key"
                        )
                    })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Chat Completions API",
                    ModelCatalogProvider::OpenAiChatCompletions,
                    self.resolved_openai_chat_completions_model(),
                )?;
                Ok(ProviderConfig::openai_chat_completions(
                    api_key,
                    self.resolved_openai_chat_completions_model(),
                )
                .with_base_url(&self.openai_chat_completions_base_url))
            }
            LlmProvider::OpenAiChatCompletionsCompatible => {
                let api_key = self
                    .openai_chat_completions_compatible_api_key
                    .as_ref()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "OpenAI Chat Completions API-compatible provider requires openai_chat_completions_compatible_api_key"
                        )
                    })?;
                validate_supported_model(
                    self.resolved_model_catalog(),
                    "OpenAI Chat Completions API-compatible",
                    ModelCatalogProvider::OpenAiChatCompletionsCompatible,
                    &self.openai_chat_completions_compatible_model,
                )?;
                Ok(ProviderConfig::openai_chat_completions_compatible(
                    api_key,
                    &self.openai_chat_completions_compatible_model,
                )
                .with_base_url(&self.openai_chat_completions_compatible_base_url))
            }
            LlmProvider::OpenRouter => {
                let api_key = self.openrouter_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("OpenRouter provider requires openrouter_api_key")
                })?;
                if self.openrouter_model.trim().is_empty() {
                    anyhow::bail!("OpenRouter provider requires openrouter_model");
                }
                let mut config = ProviderConfig::openrouter(api_key, &self.openrouter_model)
                    .with_base_url(&self.openrouter_base_url);
                if let Some(http_referer) = &self.openrouter_http_referer {
                    config = config.with_http_referer(http_referer);
                }
                if let Some(x_title) = &self.openrouter_x_title {
                    config = config.with_x_title(x_title);
                }
                if !self.openrouter_app_categories.is_empty() {
                    config = config.with_app_categories(self.openrouter_app_categories.clone());
                }
                Ok(config)
            }
            LlmProvider::AnthropicMessages => {
                let api_key = self.anthropic_messages_api_key.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Anthropic Messages API provider requires anthropic_messages_api_key"
                    )
                })?;
                let mut config =
                    ProviderConfig::anthropic_messages(api_key, &self.anthropic_messages_model)
                        .with_base_url(&self.anthropic_messages_base_url);

                if let Some(client_name) = &self.anthropic_messages_client_name {
                    config = config.with_client_name(client_name);
                }
                if let Some(user_agent) = &self.anthropic_messages_user_agent {
                    config = config.with_user_agent(user_agent);
                }

                Ok(config)
            }
        }
    }

    fn resolved_model_catalog(&self) -> &crate::ModelCatalog {
        if let Some(model_catalog) = self.model_catalog.as_deref() {
            model_catalog
        } else {
            models::base_catalog()
        }
    }

    fn validate_compaction_thresholds(&self, source: String) -> anyhow::Result<()> {
        if self.compaction_trigger_ratio.is_some() && self.compaction_hard_trigger_ratio.is_some() {
            anyhow::bail!(
                "configuration file {} sets both deprecated `compaction_trigger_ratio` and `compaction_hard_trigger_ratio`; remove the deprecated field",
                source
            );
        }

        let hard = self.effective_compaction_hard_trigger_ratio();
        let soft = self.effective_compaction_soft_trigger_ratio();
        if !(hard > 0.0 && hard <= 1.0) {
            anyhow::bail!(
                "configuration file {} has invalid compaction hard threshold {}; expected 0 < hard <= 1",
                source,
                hard
            );
        }
        if !(soft > 0.0 && soft < hard) {
            anyhow::bail!(
                "configuration file {} has invalid compaction thresholds; expected 0 < soft < hard <= 1, got soft={} hard={}",
                source,
                soft,
                hard
            );
        }
        Ok(())
    }

    fn validate_reasoning_controls(&self, source: String) -> anyhow::Result<()> {
        if self.deprecated_thinking_budget_tokens.is_some() {
            anyhow::bail!(
                "configuration file {} uses removed `thinking_budget_tokens`; replace it with `model_reasoning_effort`",
                source
            );
        }
        Ok(())
    }
}

fn merge_toml_overlay(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                if let Some(existing) = base_table.get_mut(&key) {
                    merge_toml_overlay(existing, value);
                } else {
                    base_table.insert(key, value);
                }
            }
        }
        (base_slot, overlay_value) => {
            *base_slot = overlay_value;
        }
    }
}

fn strip_skill_overrides_from_overlay(overlay: &mut toml::Value) {
    if let Some(table) = overlay.as_table_mut() {
        table.remove("skill_overrides");
    }
}

pub(crate) fn merge_skill_override_overlays_from_paths(
    base_overrides: &[SkillOverride],
    overlay_paths: &[PathBuf],
) -> anyhow::Result<Vec<SkillOverride>> {
    let mut merged_overrides = base_overrides.to_vec();

    for path in overlay_paths {
        if !path.exists() {
            continue;
        }
        let overlay_overrides = read_skill_overrides(path)?;
        merged_overrides = merge_skill_overrides(&merged_overrides, &overlay_overrides);
    }

    Ok(merged_overrides)
}

pub(crate) fn read_skill_overrides(path: &std::path::Path) -> anyhow::Result<Vec<SkillOverride>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read configuration file {}", path.display()))?;
    parse_skill_overrides(&content, path)
}

fn parse_skill_overrides(
    content: &str,
    path: &std::path::Path,
) -> anyhow::Result<Vec<SkillOverride>> {
    let overlay: SkillOverrideOverlayFile = toml::from_str(content)
        .with_context(|| format!("failed to parse configuration file {}", path.display()))?;
    Ok(overlay.skill_overrides)
}

fn inferred_context_window_tokens(provider: LlmProvider) -> u32 {
    match provider {
        LlmProvider::GoogleGeminiGenerateContent => 1_048_576,
        LlmProvider::AnthropicMessages => 200_000,
        LlmProvider::Chatgpt => 400_000,
        LlmProvider::OpenAiResponses
        | LlmProvider::OpenAiChatCompletions
        | LlmProvider::OpenAiChatCompletionsCompatible
        | LlmProvider::OpenRouter => 32_768,
    }
}

fn validate_supported_model(
    catalog: &crate::ModelCatalog,
    provider_name: &str,
    provider: ModelCatalogProvider,
    model: &str,
) -> anyhow::Result<()> {
    if catalog.find_model_info(provider, model).is_some() {
        return Ok(());
    }

    let supported = catalog.supported_model_slugs(provider).join(", ");
    anyhow::bail!(
        "{provider_name} model `{model}` is not in alan's curated catalog. Supported models: {supported}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.llm_provider, LlmProvider::OpenAiResponses);
        assert_eq!(
            config.google_gemini_generate_content_location,
            "us-central1"
        );
        assert_eq!(
            config.google_gemini_generate_content_model,
            "gemini-2.0-flash"
        );
        assert_eq!(
            config.openai_responses_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(config.openai_responses_model, "gpt-5.4");
        assert_eq!(
            config.openai_chat_completions_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
        assert_eq!(
            config.openai_chat_completions_compatible_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(
            config.openai_chat_completions_compatible_model,
            "qwen3.5-plus"
        );
        assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.openrouter_model, "moonshotai/kimi-k2.6");
        assert!(config.openrouter_api_key.is_none());
        assert!(config.openrouter_app_categories.is_empty());
        assert_eq!(
            config.anthropic_messages_base_url,
            "https://api.anthropic.com/v1"
        );
        assert_eq!(config.anthropic_messages_model, "claude-3-5-sonnet-latest");
        assert_eq!(config.llm_request_timeout_secs, 180);
        assert_eq!(config.tool_timeout_secs, 30);
        assert_eq!(config.tool_repeat_limit, 4);
        assert_eq!(config.context_window_tokens, None);
        assert_eq!(config.compaction_trigger_ratio, None);
        assert_eq!(config.compaction_hard_trigger_ratio, None);
        assert_eq!(config.compaction_soft_trigger_ratio, None);
        assert!((config.effective_compaction_hard_trigger_ratio() - 0.8).abs() < f32::EPSILON);
        assert!((config.effective_compaction_soft_trigger_ratio() - 0.72).abs() < f32::EPSILON);
        assert_eq!(config.effective_context_window_tokens(), 1_050_000);
        assert_eq!(config.prompt_snapshot_max_chars, 8000);
        assert!(!config.prompt_snapshot_enabled);
        assert!(config.max_tool_loops.is_none());
        assert_eq!(config.streaming_mode, StreamingMode::Auto);
        assert_eq!(
            config.partial_stream_recovery_mode,
            PartialStreamRecoveryMode::ContinueOnce
        );
        assert!(config.skill_overrides.is_empty());
        // Memory config
        assert!(config.memory.enabled);
        assert!(config.memory.strict_workspace);
        assert!(config.memory.workspace_dir.is_none());
        assert!(!config.durability.required);
    }

    #[test]
    fn test_config_for_google_gemini_generate_content() {
        let config = Config::for_google_gemini_generate_content(
            "project",
            Some("europe-west1"),
            Some("gemini-2.5-pro"),
        );
        assert_eq!(
            config.llm_provider,
            LlmProvider::GoogleGeminiGenerateContent
        );
        assert_eq!(
            config.google_gemini_generate_content_project_id,
            Some("project".to_string())
        );
        assert_eq!(
            config.google_gemini_generate_content_location,
            "europe-west1"
        );
        assert_eq!(
            config.google_gemini_generate_content_model,
            "gemini-2.5-pro"
        );
        assert!(config.has_google_gemini_generate_content_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_google_gemini_generate_content_defaults() {
        let config = Config::for_google_gemini_generate_content("project", None, None);
        assert_eq!(
            config.google_gemini_generate_content_location,
            "us-central1"
        );
        assert_eq!(
            config.google_gemini_generate_content_model,
            "gemini-2.0-flash"
        );
    }

    #[test]
    fn test_config_for_openai_responses() {
        let config = Config::for_openai_responses(
            "sk-test",
            Some("https://api.openai.com/v1"),
            Some("gpt-5.4"),
        );
        assert_eq!(config.llm_provider, LlmProvider::OpenAiResponses);
        assert_eq!(config.openai_responses_api_key, Some("sk-test".to_string()));
        assert_eq!(config.openai_responses_model, "gpt-5.4");
        assert!(config.has_openai_responses_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_openai_responses_defaults() {
        let config = Config::for_openai_responses("sk-test", None, None);
        assert_eq!(
            config.openai_responses_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(config.openai_responses_model, "gpt-5.4");
    }

    #[test]
    fn test_config_for_chatgpt() {
        let config = Config::for_chatgpt(
            Some("https://chatgpt.com/backend-api/codex"),
            Some("gpt-5.3-codex"),
        );
        assert_eq!(config.llm_provider, LlmProvider::Chatgpt);
        assert_eq!(
            config.chatgpt_base_url,
            "https://chatgpt.com/backend-api/codex"
        );
        assert_eq!(config.chatgpt_model, "gpt-5.3-codex");
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_chatgpt_defaults() {
        let config = Config::for_chatgpt(None, None);
        assert_eq!(
            config.chatgpt_base_url,
            "https://chatgpt.com/backend-api/codex"
        );
        assert_eq!(config.chatgpt_model, "gpt-5.3-codex");
    }

    #[test]
    fn test_config_for_openai_chat_completions() {
        let config = Config::for_openai_chat_completions(
            "sk-test",
            Some("https://api.openai.com/v1"),
            Some("gpt-5.4"),
        );
        assert_eq!(config.llm_provider, LlmProvider::OpenAiChatCompletions);
        assert_eq!(
            config.openai_chat_completions_api_key,
            Some("sk-test".to_string())
        );
        assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
        assert!(config.has_openai_chat_completions_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_openai_chat_completions_defaults() {
        let config = Config::for_openai_chat_completions("sk-test", None, None);
        assert_eq!(
            config.openai_chat_completions_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(config.openai_chat_completions_model, "gpt-5.4");
    }

    #[test]
    fn test_config_for_openai_chat_completions_compatible() {
        let config = Config::for_openai_chat_completions_compatible(
            "sk-test",
            Some("https://api.openai.com/v1"),
            Some("qwen3.5-plus"),
        );
        assert_eq!(
            config.llm_provider,
            LlmProvider::OpenAiChatCompletionsCompatible
        );
        assert_eq!(
            config.openai_chat_completions_compatible_api_key,
            Some("sk-test".to_string())
        );
        assert_eq!(
            config.openai_chat_completions_compatible_model,
            "qwen3.5-plus"
        );
        assert!(config.has_openai_chat_completions_compatible_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_openai_chat_completions_compatible_defaults() {
        let config = Config::for_openai_chat_completions_compatible("sk-test", None, None);
        assert_eq!(
            config.openai_chat_completions_compatible_base_url,
            "https://api.openai.com/v1"
        );
        assert_eq!(
            config.openai_chat_completions_compatible_model,
            "qwen3.5-plus"
        );
    }

    #[test]
    fn test_config_for_openrouter() {
        let config = Config::for_openrouter(
            "sk-or",
            None,
            Some("anthropic/claude-sonnet-4"),
            Some("https://alan.local"),
            Some("alan"),
            vec!["cli-agent".to_string()],
        );
        assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
        assert_eq!(config.openrouter_api_key.as_deref(), Some("sk-or"));
        assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.openrouter_model, "anthropic/claude-sonnet-4");
        assert_eq!(
            config.openrouter_http_referer.as_deref(),
            Some("https://alan.local")
        );
        assert_eq!(config.openrouter_x_title.as_deref(), Some("alan"));
        assert_eq!(config.openrouter_app_categories, vec!["cli-agent"]);
        assert!(config.has_openrouter_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_anthropic_messages() {
        let config = Config::for_anthropic_messages(
            "ak-test",
            Some("https://api.anthropic.com/v1"),
            Some("claude-sonnet-4-5"),
        );
        assert_eq!(config.llm_provider, LlmProvider::AnthropicMessages);
        assert_eq!(
            config.anthropic_messages_api_key,
            Some("ak-test".to_string())
        );
        assert_eq!(config.anthropic_messages_model, "claude-sonnet-4-5");
        assert!(config.has_anthropic_messages_config());
        assert!(config.has_llm_config());
    }

    #[test]
    fn test_config_for_anthropic_messages_with_options() {
        let config = Config {
            llm_provider: LlmProvider::AnthropicMessages,
            anthropic_messages_api_key: Some("key".to_string()),
            anthropic_messages_base_url: "https://api.anthropic.com/v1".to_string(),
            anthropic_messages_model: "claude-3".to_string(),
            anthropic_messages_client_name: Some("test-client".to_string()),
            anthropic_messages_user_agent: Some("test-agent/1.0".to_string()),
            ..Config::default()
        };
        assert_eq!(
            config.anthropic_messages_client_name,
            Some("test-client".to_string())
        );
        assert_eq!(
            config.anthropic_messages_user_agent,
            Some("test-agent/1.0".to_string())
        );
    }

    #[test]
    fn test_config_for_anthropic_messages_defaults() {
        let config = Config::for_anthropic_messages("ak-test", None, None);
        assert_eq!(
            config.anthropic_messages_base_url,
            "https://api.anthropic.com/v1"
        );
        assert_eq!(config.anthropic_messages_model, "claude-3-5-sonnet-latest");
    }

    #[test]
    fn test_effective_model() {
        let gemini =
            Config::for_google_gemini_generate_content("project", None, Some("gemini-2.5-pro"));
        assert_eq!(gemini.effective_model(), "gemini-2.5-pro");

        let chatgpt = Config::for_chatgpt(None, Some("gpt-5.3-codex"));
        assert_eq!(chatgpt.effective_model(), "gpt-5.3-codex");

        let openai_responses = Config::for_openai_responses("k", None, Some("gpt-5.4"));
        assert_eq!(openai_responses.effective_model(), "gpt-5.4");

        let openai_chat_completions =
            Config::for_openai_chat_completions("k", None, Some("gpt-5.4"));
        assert_eq!(openai_chat_completions.effective_model(), "gpt-5.4");

        let openai_chat_completions_compatible =
            Config::for_openai_chat_completions_compatible("k", None, Some("qwen3.5-plus"));
        assert_eq!(
            openai_chat_completions_compatible.effective_model(),
            "qwen3.5-plus"
        );

        let anthropic = Config::for_anthropic_messages("k", None, Some("claude-3-5-sonnet"));
        assert_eq!(anthropic.effective_model(), "claude-3-5-sonnet");

        let openrouter = Config::for_openrouter(
            "sk-or",
            None,
            Some("openai/gpt-5.4"),
            None,
            None,
            Vec::new(),
        );
        assert_eq!(openrouter.effective_model(), "openai/gpt-5.4");
    }

    #[test]
    fn test_has_llm_config_without_api_key() {
        let mut config = Config {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: None,
            openai_chat_completions_api_key: None,
            openai_chat_completions_compatible_api_key: None,
            ..Config::default()
        };
        assert!(!config.has_openai_responses_config());
        assert!(!config.has_llm_config());

        config.llm_provider = LlmProvider::OpenAiChatCompletions;
        assert!(!config.has_openai_chat_completions_config());
        assert!(!config.has_llm_config());

        config.llm_provider = LlmProvider::OpenAiChatCompletionsCompatible;
        assert!(!config.has_openai_chat_completions_compatible_config());
        assert!(!config.has_llm_config());

        config.llm_provider = LlmProvider::OpenRouter;
        config.openrouter_api_key = Some("sk-or".to_string());
        config.openrouter_model.clear();
        assert!(!config.has_openrouter_config());
        assert!(!config.has_llm_config());

        config.llm_provider = LlmProvider::AnthropicMessages;
        config.anthropic_messages_api_key = None;
        assert!(!config.has_anthropic_messages_config());
        assert!(!config.has_llm_config());

        config.llm_provider = LlmProvider::GoogleGeminiGenerateContent;
        config.google_gemini_generate_content_project_id = None;
        assert!(!config.has_google_gemini_generate_content_config());
    }

    #[test]
    fn test_openai_provider_does_not_treat_compat_key_as_valid_config() {
        let config = Config {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: None,
            openai_chat_completions_compatible_api_key: Some("sk-legacy".to_string()),
            ..Config::default()
        };

        assert!(!config.has_openai_responses_config());
        assert!(!config.has_llm_config());
    }

    #[test]
    fn test_llm_provider_deserialization() {
        let gemini: LlmProvider =
            serde_json::from_str("\"google_gemini_generate_content\"").unwrap();
        assert_eq!(gemini, LlmProvider::GoogleGeminiGenerateContent);

        let chatgpt: LlmProvider = serde_json::from_str("\"chatgpt\"").unwrap();
        assert_eq!(chatgpt, LlmProvider::Chatgpt);

        let openai_responses: LlmProvider = serde_json::from_str("\"openai_responses\"").unwrap();
        assert_eq!(openai_responses, LlmProvider::OpenAiResponses);

        let openai_chat_completions: LlmProvider =
            serde_json::from_str("\"openai_chat_completions\"").unwrap();
        assert_eq!(openai_chat_completions, LlmProvider::OpenAiChatCompletions);

        let openai_chat_completions_compatible: LlmProvider =
            serde_json::from_str("\"openai_chat_completions_compatible\"").unwrap();
        assert_eq!(
            openai_chat_completions_compatible,
            LlmProvider::OpenAiChatCompletionsCompatible
        );

        let openrouter: LlmProvider = serde_json::from_str("\"openrouter\"").unwrap();
        assert_eq!(openrouter, LlmProvider::OpenRouter);

        let anthropic: LlmProvider = serde_json::from_str("\"anthropic_messages\"").unwrap();
        assert_eq!(anthropic, LlmProvider::AnthropicMessages);
    }

    #[test]
    fn test_config_from_file() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("test_config.toml");

        let toml_content = r#"
connection_profile = "openai-main"
llm_request_timeout_secs = 300
tool_timeout_secs = 60
streaming_mode = "off"
partial_stream_recovery_mode = "off"
"#;

        let mut file = std::fs::File::create(&config_path).unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.connection_profile.as_deref(), Some("openai-main"));
        assert_eq!(config.llm_request_timeout_secs, 300);
        assert_eq!(config.tool_timeout_secs, 60);
        assert_eq!(config.streaming_mode, StreamingMode::Off);
        assert_eq!(
            config.partial_stream_recovery_mode,
            PartialStreamRecoveryMode::Off
        );
    }

    #[test]
    fn test_config_from_file_accepts_model_reasoning_effort() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("test_config.toml");
        std::fs::write(
            &config_path,
            r#"
model_reasoning_effort = "high"
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.model_reasoning_effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn test_config_from_file_rejects_legacy_thinking_budget() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("test_config.toml");
        std::fs::write(
            &config_path,
            r#"
model_reasoning_effort = "medium"
thinking_budget_tokens = 2048
"#,
        )
        .unwrap();

        let err = Config::from_file(&config_path).unwrap_err();
        assert!(
            err.to_string()
                .contains("uses removed `thinking_budget_tokens`")
        );
        assert!(err.to_string().contains("model_reasoning_effort"));
    }

    #[test]
    fn test_request_control_resolver_uses_model_default_without_budget() {
        let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        let resolved = crate::resolve_runtime_request_controls(
            &config,
            crate::provider_capabilities_for_config(&config),
            crate::RequestControlIntent::default(),
        )
        .unwrap();
        assert_eq!(resolved.reasoning_effort(), Some(ReasoningEffort::Medium));
    }

    #[test]
    fn test_config_from_file_accepts_skill_overrides() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("test_config.toml");

        std::fs::write(
            &config_path,
            r#"
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false
"#,
        )
        .unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.skill_overrides.len(), 1);
        assert_eq!(
            config
                .skill_overrides
                .iter()
                .find(|entry| entry.skill_id == "plan")
                .unwrap()
                .allow_implicit_invocation,
            Some(false)
        );
    }

    #[test]
    fn test_config_from_file_rejects_legacy_skill_override_key() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("legacy-skill-override.toml");

        std::fs::write(
            &config_path,
            r#"
[[skill_overrides]]
skill_id = "plan"
allow_implicit_invocation = false
"#,
        )
        .unwrap();

        let err = Config::from_file(&config_path).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("failed to parse configuration file"));
        assert!(message.contains("skill_id"));
    }

    #[test]
    fn test_config_from_file_rejects_noncanonical_skill_override_id() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("noncanonical-skill-override.toml");

        std::fs::write(
            &config_path,
            r#"
[[skill_overrides]]
skill = "repo.review"
allow_implicit_invocation = false
"#,
        )
        .unwrap();

        let err = Config::from_file(&config_path).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("failed to parse configuration file"));
        assert!(message.contains("repo.review"));
        assert!(message.contains("repo-review"));
    }

    #[test]
    fn test_config_from_file_defaults_skill_overrides_when_omitted() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("test_config.toml");

        std::fs::write(&config_path, "connection_profile = \"openai-main\"\n").unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.connection_profile.as_deref(), Some("openai-main"));
        assert!(config.skill_overrides.is_empty());
    }

    #[test]
    fn test_with_agent_root_overlays_merges_skill_overrides_field_by_field() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        std::fs::write(
            &overlay_path,
            r#"
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false
"#,
        )
        .unwrap();

        let base = Config {
            skill_overrides: vec![SkillOverride {
                skill_id: "plan".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
            ..Config::default()
        };
        let config = base
            .with_agent_root_overlays(std::slice::from_ref(&overlay_path))
            .unwrap();
        assert_eq!(
            config
                .skill_overrides
                .iter()
                .find(|entry| entry.skill_id == "plan")
                .unwrap()
                .enabled,
            Some(false)
        );
        assert_eq!(
            config
                .skill_overrides
                .iter()
                .find(|entry| entry.skill_id == "plan")
                .unwrap()
                .allow_implicit_invocation,
            Some(false)
        );
    }

    #[test]
    fn test_with_agent_root_overlays_merges_skill_overrides_across_multiple_roots() {
        let temp = TempDir::new().unwrap();
        let first_overlay = temp.path().join("global-agent.toml");
        let second_overlay = temp.path().join("workspace-agent.toml");
        std::fs::write(
            &first_overlay,
            r#"
[[skill_overrides]]
skill = "release-checklist"
enabled = false
"#,
        )
        .unwrap();
        std::fs::write(
            &second_overlay,
            r#"
[[skill_overrides]]
skill = "deploy-checklist"
allow_implicit_invocation = false
"#,
        )
        .unwrap();

        let config = Config::default()
            .with_agent_root_overlays(&[first_overlay, second_overlay])
            .unwrap();

        assert_eq!(
            config
                .skill_overrides
                .iter()
                .find(|entry| entry.skill_id == "release-checklist")
                .unwrap()
                .enabled,
            Some(false)
        );
        assert_eq!(
            config
                .skill_overrides
                .iter()
                .find(|entry| entry.skill_id == "deploy-checklist")
                .unwrap()
                .allow_implicit_invocation,
            Some(false)
        );
    }

    #[test]
    fn test_config_from_file_not_found() {
        let result = Config::from_file(Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_config_from_file_invalid_toml() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("invalid.toml");

        std::fs::write(&config_path, "not valid toml {{").unwrap();

        let result = Config::from_file(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_launch_root_agent_configs_parse_as_agent_overlays() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let paths = [
            crate_root.join("skills/repo-coding/agents/repo-worker/agent.toml"),
            crate_root.join("skills/workspace-inspect/agents/workspace-reader/agent.toml"),
            crate_root.join("skills/skill-creator/agents/skill-creator/agent.toml"),
        ];

        for path in paths {
            Config::from_file(&path)
                .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
        }
    }

    #[test]
    fn test_config_file_path_prefers_alan_config_path_env() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let canonical_config = Config::global_agent_config_file_path_from_home(&home).unwrap();
        std::fs::create_dir_all(canonical_config.parent().unwrap()).unwrap();
        std::fs::write(
            &canonical_config,
            "llm_provider = \"google_gemini_generate_content\"\n",
        )
        .unwrap();

        let override_path = temp.path().join("override.toml");
        std::fs::write(
            &override_path,
            "llm_provider = \"google_gemini_generate_content\"\n",
        )
        .unwrap();

        let resolved =
            Config::resolve_config_file_path(Some(override_path.clone()), Some(canonical_config))
                .unwrap();
        assert_eq!(resolved, override_path);
    }

    #[test]
    fn test_config_file_path_uses_global_agent_home() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let canonical_config = Config::global_agent_config_file_path_from_home(&home).unwrap();
        std::fs::create_dir_all(canonical_config.parent().unwrap()).unwrap();
        std::fs::write(
            &canonical_config,
            "llm_provider = \"google_gemini_generate_content\"\n",
        )
        .unwrap();
        let resolved =
            Config::resolve_config_file_path(None, Some(canonical_config.clone())).unwrap();
        assert_eq!(resolved, canonical_config);
    }

    #[test]
    fn test_load_falls_back_to_global_agent_home_when_override_missing() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let canonical_config = Config::global_agent_config_file_path_from_home(&home).unwrap();
        std::fs::create_dir_all(canonical_config.parent().unwrap()).unwrap();
        std::fs::write(&canonical_config, "connection_profile = \"openai-main\"\n").unwrap();

        let missing_override = temp.path().join("missing-override.toml");
        let loaded =
            Config::load_with_paths(Some(missing_override), Some(canonical_config)).unwrap();
        assert_eq!(loaded.source, ConfigSourceKind::GlobalAgentHome);
        assert_eq!(
            loaded.config.connection_profile.as_deref(),
            Some("openai-main")
        );
    }

    #[test]
    fn test_load_uses_default_when_canonical_missing() {
        let temp = TempDir::new().unwrap();
        let missing_override = temp.path().join("missing-override.toml");
        let loaded = Config::load_with_paths(Some(missing_override), None).unwrap();
        assert_eq!(loaded.source, ConfigSourceKind::Default);
        assert!(loaded.path.is_none());
        assert_eq!(loaded.config.llm_provider, Config::default().llm_provider);
    }

    #[test]
    fn test_load_uses_existing_override_when_present() {
        let temp = TempDir::new().unwrap();
        let override_path = temp.path().join("override.toml");
        std::fs::write(&override_path, "connection_profile = \"gemini-main\"\n").unwrap();
        let loaded = Config::load_with_paths(Some(override_path.clone()), None).unwrap();
        assert_eq!(loaded.source, ConfigSourceKind::EnvOverride);
        assert_eq!(loaded.path, Some(override_path));
        assert_eq!(
            loaded.config.connection_profile.as_deref(),
            Some("gemini-main")
        );
    }

    #[test]
    fn test_load_with_paths_rejects_deprecated_provider_key_names() {
        let temp = TempDir::new().unwrap();
        let override_path = temp.path().join("legacy.toml");
        std::fs::write(
            &override_path,
            r#"
llm_provider = "openai_compatible"
openai_compat_api_key = "sk-test"
"#,
        )
        .unwrap();

        let err = Config::load_with_paths(Some(override_path.clone()), None).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("failed to parse configuration file"));
        assert!(message.contains(&override_path.display().to_string()));
    }

    #[test]
    fn test_config_from_file_rejects_deprecated_openai_key_names() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("legacy.toml");
        std::fs::write(
            &config_path,
            r#"
openai_api_key = "sk-test"
openai_model = "gpt-5"
"#,
        )
        .unwrap();

        let err = Config::from_file(&config_path).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("failed to parse configuration file"));
        assert!(message.contains(&config_path.display().to_string()));
    }

    #[test]
    fn test_config_from_file_full() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("full_config.toml");

        let toml_content = r#"
connection_profile = "anthropic-main"
llm_request_timeout_secs = 240
tool_timeout_secs = 45
max_tool_loops = 10
tool_repeat_limit = 5
prompt_snapshot_enabled = true
prompt_snapshot_max_chars = 10000
context_window_tokens = 65536
compaction_trigger_ratio = 0.75
streaming_mode = "on"
partial_stream_recovery_mode = "continue_once"

[memory]
enabled = false
strict_workspace = false

[durability]
required = true
"#;

        std::fs::write(&config_path, toml_content).unwrap();

        let config = Config::from_file(&config_path).unwrap();
        assert_eq!(config.connection_profile.as_deref(), Some("anthropic-main"));
        assert_eq!(config.llm_request_timeout_secs, 240);
        assert_eq!(config.tool_timeout_secs, 45);
        assert_eq!(config.max_tool_loops, Some(10));
        assert_eq!(config.tool_repeat_limit, 5);
        assert_eq!(config.context_window_tokens, Some(65_536));
        assert_eq!(config.compaction_trigger_ratio, Some(0.75));
        assert!((config.effective_compaction_hard_trigger_ratio() - 0.75).abs() < f32::EPSILON);
        assert!((config.effective_compaction_soft_trigger_ratio() - 0.675).abs() < f32::EPSILON);
        assert!(config.prompt_snapshot_enabled);
        assert_eq!(config.prompt_snapshot_max_chars, 10000);
        assert_eq!(config.streaming_mode, StreamingMode::On);
        assert_eq!(
            config.partial_stream_recovery_mode,
            PartialStreamRecoveryMode::ContinueOnce
        );
        // Memory
        assert!(!config.memory.enabled);
        assert!(!config.memory.strict_workspace);
        assert!(config.durability.required);
    }

    #[test]
    fn test_memory_config_default() {
        let memory = MemoryConfig::default();
        assert!(memory.enabled);
        assert!(memory.strict_workspace);
        assert!(memory.workspace_dir.is_none());
    }

    #[test]
    fn test_effective_compaction_thresholds_default_soft_from_hard() {
        let config = Config::default();

        assert!((config.effective_compaction_hard_trigger_ratio() - 0.8).abs() < f32::EPSILON);
        assert!((config.effective_compaction_soft_trigger_ratio() - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_from_file_rejects_duplicate_hard_threshold_fields() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");
        std::fs::write(
            &config_path,
            r#"
connection_profile = "openai-main"
compaction_trigger_ratio = 0.8
compaction_hard_trigger_ratio = 0.85
"#,
        )
        .unwrap();

        let err = Config::from_file(&config_path).unwrap_err();
        assert!(
            err.to_string().contains(
                "deprecated `compaction_trigger_ratio` and `compaction_hard_trigger_ratio`"
            )
        );
    }

    #[test]
    fn test_memory_config_deserialization() {
        let toml_content = r#"
enabled = false
strict_workspace = false
workspace_dir = "/custom/path"
"#;
        let memory: MemoryConfig = toml::from_str(toml_content).unwrap();
        assert!(!memory.enabled);
        assert!(!memory.strict_workspace);
        assert_eq!(memory.workspace_dir, Some(PathBuf::from("/custom/path")));
    }

    #[test]
    fn test_durability_config_deserialization() {
        let toml_content = r#"
[durability]
required = true
"#;
        let config: Config = toml::from_str(toml_content).unwrap();
        assert!(config.durability.required);
    }

    #[test]
    fn test_effective_context_window_tokens_uses_explicit_override() {
        let config = Config {
            context_window_tokens: Some(42_000),
            ..Config::default()
        };

        assert_eq!(config.effective_context_window_tokens(), 42_000);
    }

    #[test]
    fn test_effective_context_window_tokens_uses_provider_family_defaults() {
        let gemini =
            Config::for_google_gemini_generate_content("project", None, Some("gemini-2.5-pro"));
        assert_eq!(gemini.effective_context_window_tokens(), 1_048_576);

        let chatgpt = Config::for_chatgpt(None, Some("gpt-5.3-codex"));
        assert_eq!(chatgpt.effective_context_window_tokens(), 400_000);

        let anthropic =
            Config::for_anthropic_messages("key", None, Some("claude-3-5-sonnet-latest"));
        assert_eq!(anthropic.effective_context_window_tokens(), 200_000);

        let openai_responses = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        assert_eq!(
            openai_responses.effective_context_window_tokens(),
            1_050_000
        );

        let openai_chat_completions =
            Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.4"));
        assert_eq!(
            openai_chat_completions.effective_context_window_tokens(),
            1_050_000
        );

        let openai_chat_completions_pro =
            Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.2-pro"));
        assert_eq!(
            openai_chat_completions_pro.effective_context_window_tokens(),
            400_000
        );

        let openai_compat = Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("bailian/qwen3.5-plus-2026-02-15"),
        );
        assert_eq!(openai_compat.effective_context_window_tokens(), 1_000_000);

        let minimax =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("MiniMax-M2.5"));
        assert_eq!(minimax.effective_context_window_tokens(), 204_800);

        let glm = Config::for_openai_chat_completions_compatible("sk-test", None, Some("glm-5"));
        assert_eq!(glm.effective_context_window_tokens(), 200_000);

        let kimi =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("kimi-k2.5"));
        assert_eq!(kimi.effective_context_window_tokens(), 250_000);

        let deepseek = Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("deepseek-reasoner"),
        );
        assert_eq!(deepseek.effective_context_window_tokens(), 128_000);

        let unknown = Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("my-custom-model"),
        );
        assert_eq!(unknown.effective_context_window_tokens(), 32_768);
    }

    #[test]
    fn test_effective_context_window_tokens_uses_overlay_model_catalog() {
        let temp = TempDir::new().unwrap();
        let alan_dir = temp.path().join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();
        std::fs::write(
            alan_dir.join("models.toml"),
            r#"
[openai_chat_completions_compatible]
[[openai_chat_completions_compatible.models]]
slug = "custom-kimi"
family = "custom"
context_window_tokens = 654321
supports_reasoning = true
"#,
        )
        .unwrap();

        let catalog = crate::ModelCatalog::load_with_overlays(Some(temp.path())).unwrap();
        let mut config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("custom-kimi"));
        config.set_model_catalog(Arc::new(catalog));

        assert_eq!(config.effective_context_window_tokens(), 654_321);
        assert_eq!(config.effective_model_info().unwrap().slug, "custom-kimi");
    }

    #[test]
    fn test_with_agent_root_overlays_preserves_model_catalog() {
        let temp = TempDir::new().unwrap();
        let alan_dir = temp.path().join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();
        std::fs::write(
            alan_dir.join("models.toml"),
            r#"
[openai_chat_completions_compatible]
[[openai_chat_completions_compatible.models]]
slug = "custom-kimi"
family = "custom"
context_window_tokens = 654321
supports_reasoning = true
"#,
        )
        .unwrap();

        let overlay_path = temp.path().join("agent.toml");
        std::fs::write(&overlay_path, "model_reasoning_effort = \"high\"\n").unwrap();

        let catalog = crate::ModelCatalog::load_with_overlays(Some(temp.path())).unwrap();
        let mut config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("custom-kimi"));
        config.set_model_catalog(std::sync::Arc::new(catalog));

        let overlaid = config.with_agent_root_overlays(&[overlay_path]).unwrap();

        assert_eq!(overlaid.model_reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(overlaid.effective_model_info().unwrap().slug, "custom-kimi");
        assert_eq!(overlaid.effective_context_window_tokens(), 654_321);
    }

    #[test]
    fn test_with_agent_root_overlays_merges_model_reasoning_effort() {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        std::fs::write(&overlay_path, "model_reasoning_effort = \"high\"\n").unwrap();

        let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        let overlaid = config.with_agent_root_overlays(&[overlay_path]).unwrap();

        assert_eq!(
            crate::resolve_runtime_request_controls(
                &overlaid,
                crate::provider_capabilities_for_config(&overlaid),
                crate::RequestControlIntent::default(),
            )
            .unwrap()
            .reasoning_effort(),
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn test_with_agent_root_overlays_preserves_internal_provider_state_for_same_connection_profile()
    {
        let temp = TempDir::new().unwrap();
        let overlay_path = temp.path().join("agent.toml");
        std::fs::write(&overlay_path, "tool_repeat_limit = 9\n").unwrap();

        let config = Config {
            connection_profile: Some("openai-main".to_string()),
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: Some("sk-test".to_string()),
            openai_responses_model: "gpt-5.4".to_string(),
            ..Default::default()
        };

        let overlaid = config.with_agent_root_overlays(&[overlay_path]).unwrap();

        assert_eq!(overlaid.connection_profile.as_deref(), Some("openai-main"));
        assert_eq!(overlaid.llm_provider, LlmProvider::OpenAiResponses);
        assert_eq!(
            overlaid.openai_responses_api_key.as_deref(),
            Some("sk-test")
        );
        assert_eq!(overlaid.openai_responses_model, "gpt-5.4");
        assert_eq!(overlaid.tool_repeat_limit, 9);
    }

    #[test]
    fn test_to_provider_config_gemini() {
        let config = Config::for_google_gemini_generate_content(
            "my-project",
            None,
            Some("gemini-2.0-flash"),
        );
        let provider_config = config.to_provider_config().unwrap();
        // Verify it creates the right config type
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::GoogleGeminiGenerateContent
        );
        assert_eq!(provider_config.project_id, Some("my-project".to_string()));
        assert_eq!(provider_config.model, "gemini-2.0-flash");
    }

    #[test]
    fn test_to_provider_config_google_gemini_generate_content_missing_project() {
        let config = Config {
            llm_provider: LlmProvider::GoogleGeminiGenerateContent,
            google_gemini_generate_content_project_id: None,
            ..Config::default()
        };
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("google_gemini_generate_content_project_id")
        );
    }

    #[test]
    fn test_to_provider_config_openai_responses() {
        let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenAiResponses
        );
        assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
        assert_eq!(provider_config.model, "gpt-5.4");
    }

    #[test]
    fn test_to_provider_config_chatgpt() {
        let config = Config::for_chatgpt(
            Some("https://chatgpt.com/backend-api/codex"),
            Some("gpt-5.3-codex"),
        );
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::ChatgptResponses
        );
        assert_eq!(provider_config.api_key, None);
        assert_eq!(
            provider_config.base_url,
            Some("https://chatgpt.com/backend-api/codex".to_string())
        );
        assert_eq!(provider_config.model, "gpt-5.3-codex");
        assert_eq!(provider_config.expected_account_id, None);
    }

    #[test]
    fn test_to_provider_config_chatgpt_with_account_binding() {
        let mut config = Config::for_chatgpt(
            Some("https://chatgpt.com/backend-api/codex"),
            Some("gpt-5.3-codex"),
        );
        config.chatgpt_account_id = Some("acct_123".to_string());
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.expected_account_id.as_deref(),
            Some("acct_123")
        );
    }

    #[test]
    fn test_to_provider_config_openai_responses_missing_key() {
        let config = Config {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: None,
            openai_chat_completions_api_key: None,
            openai_chat_completions_compatible_api_key: None,
            ..Config::default()
        };
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("openai_responses_api_key")
        );
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions() {
        let config = Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.4"));
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenAiChatCompletions
        );
        assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
        assert_eq!(provider_config.model, "gpt-5.4");
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions_compatible() {
        let config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("qwen3.5-plus"));
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenAiChatCompletionsCompatible
        );
        assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
        assert_eq!(provider_config.model, "qwen3.5-plus");
    }

    #[test]
    fn test_to_provider_config_openrouter() {
        let config = Config::for_openrouter(
            "sk-or",
            Some("https://openrouter.example/api/v1"),
            Some("anthropic/claude-sonnet-4"),
            Some("https://alan.local"),
            Some("alan"),
            vec!["cli-agent".to_string()],
        );
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenRouter
        );
        assert_eq!(provider_config.api_key.as_deref(), Some("sk-or"));
        assert_eq!(
            provider_config.base_url.as_deref(),
            Some("https://openrouter.example/api/v1")
        );
        assert_eq!(provider_config.model, "anthropic/claude-sonnet-4");
        assert_eq!(
            provider_config.http_referer.as_deref(),
            Some("https://alan.local")
        );
        assert_eq!(provider_config.x_title.as_deref(), Some("alan"));
        assert_eq!(
            provider_config.app_categories,
            Some(vec!["cli-agent".to_string()])
        );
    }

    #[test]
    fn test_to_provider_config_openrouter_missing_model() {
        let config = Config {
            llm_provider: LlmProvider::OpenRouter,
            openrouter_api_key: Some("sk-or".to_string()),
            openrouter_model: String::new(),
            ..Config::default()
        };
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("openrouter_model"));
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions_compatible_accepts_snapshot_and_vendor_prefix()
     {
        let config = Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("bailian/qwen3.5-plus-2026-02-15"),
        );
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenAiChatCompletionsCompatible
        );
        assert_eq!(provider_config.model, "bailian/qwen3.5-plus-2026-02-15");
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions_compatible_rejects_non_snapshot_variant_suffix()
     {
        let config = Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("kimi-k2.5-thinking"),
        );
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("curated catalog"));
    }

    #[test]
    fn test_to_provider_config_openai_does_not_fall_back_to_compat_settings() {
        let config = Config {
            llm_provider: LlmProvider::OpenAiResponses,
            openai_responses_api_key: None,
            openai_chat_completions_compatible_api_key: Some("sk-legacy".to_string()),
            openai_chat_completions_compatible_base_url: "https://proxy.example/v1".to_string(),
            openai_chat_completions_compatible_model: "qwen3.5-plus".to_string(),
            ..Config::default()
        };

        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("openai_responses_api_key")
        );
    }

    #[test]
    fn test_to_provider_config_openai_rejects_unsupported_model() {
        let config = Config::for_openai_responses("sk-test", None, Some("gpt-4o"));
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("curated catalog"));
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions_compatible_rejects_outdated_model_family() {
        let config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("kimi-k2"));
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("curated catalog"));
    }

    #[test]
    fn test_to_provider_config_openai_chat_completions_compatible_accepts_workspace_overlay_model()
    {
        let temp = TempDir::new().unwrap();
        let alan_dir = temp.path().join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();
        std::fs::write(
            alan_dir.join("models.toml"),
            r#"
[openai_chat_completions_compatible]
[[openai_chat_completions_compatible.models]]
slug = "custom-kimi"
family = "custom"
context_window_tokens = 654321
supports_reasoning = true
"#,
        )
        .unwrap();

        let catalog = crate::ModelCatalog::load_with_overlays(Some(temp.path())).unwrap();
        let mut config =
            Config::for_openai_chat_completions_compatible("sk-test", None, Some("custom-kimi"));
        config.set_model_catalog(Arc::new(catalog));

        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::OpenAiChatCompletionsCompatible
        );
        assert_eq!(provider_config.model, "custom-kimi");
    }

    #[test]
    fn test_to_provider_config_anthropic_messages() {
        let config = Config::for_anthropic_messages("sk-test", None, Some("claude-3"));
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.provider_type,
            alan_llm::factory::ProviderType::AnthropicMessages
        );
        assert_eq!(provider_config.api_key, Some("sk-test".to_string()));
        assert_eq!(provider_config.model, "claude-3");
    }

    #[test]
    fn test_to_provider_config_anthropic_messages_with_options() {
        let config = Config {
            llm_provider: LlmProvider::AnthropicMessages,
            anthropic_messages_api_key: Some("key".to_string()),
            anthropic_messages_base_url: "https://custom.api.com".to_string(),
            anthropic_messages_model: "claude-3".to_string(),
            anthropic_messages_client_name: Some("test-client".to_string()),
            anthropic_messages_user_agent: Some("test-agent/1.0".to_string()),
            ..Config::default()
        };
        let provider_config = config.to_provider_config().unwrap();
        assert_eq!(
            provider_config.base_url,
            Some("https://custom.api.com".to_string())
        );
        assert_eq!(provider_config.client_name, Some("test-client".to_string()));
        assert_eq!(
            provider_config.user_agent,
            Some("test-agent/1.0".to_string())
        );
    }

    #[test]
    fn test_to_provider_config_anthropic_messages_missing_key() {
        let config = Config {
            llm_provider: LlmProvider::AnthropicMessages,
            anthropic_messages_api_key: None,
            ..Config::default()
        };
        let result = config.to_provider_config();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("anthropic_messages_api_key")
        );
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_llm_provider(), LlmProvider::OpenAiResponses);
        assert_eq!(
            default_google_gemini_generate_content_location(),
            "us-central1"
        );
        assert_eq!(
            default_google_gemini_generate_content_model(),
            "gemini-2.0-flash"
        );
        assert_eq!(
            default_openai_responses_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(default_openai_responses_model(), "gpt-5.4");
        assert_eq!(
            default_openai_chat_completions_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(default_openai_chat_completions_model(), "gpt-5.4");
        assert_eq!(
            default_openai_chat_completions_compatible_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            default_openai_chat_completions_compatible_model(),
            "qwen3.5-plus"
        );
        assert_eq!(
            default_openrouter_base_url(),
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(default_openrouter_model(), "moonshotai/kimi-k2.6");
        assert_eq!(
            default_anthropic_messages_base_url(),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(
            default_anthropic_messages_model(),
            "claude-3-5-sonnet-latest"
        );
        assert_eq!(default_llm_timeout_secs(), 180);
        assert_eq!(default_tool_timeout_secs(), 30);
        assert_eq!(default_prompt_snapshot_max_chars(), 8000);
        assert_eq!(default_tool_repeat_limit(), 4);
        assert_eq!(default_streaming_mode(), StreamingMode::Auto);
        assert_eq!(
            default_partial_stream_recovery_mode(),
            PartialStreamRecoveryMode::ContinueOnce
        );
    }
}
