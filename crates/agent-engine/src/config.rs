//! Configuration management.

use crate::models::{self, ModelCatalogProvider, ModelInfo};
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
    pub store_dir: Option<PathBuf>,
    pub strict_store: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            store_dir: None,
            strict_store: true,
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

/// Source used to load the effective Agent Process configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSourceKind {
    EnvOverride,
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

fn default_compaction_hard_trigger_ratio() -> f32 {
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
            compaction_soft_trigger_ratio: None,
            compaction_hard_trigger_ratio: None,
            prompt_snapshot_enabled: false,
            prompt_snapshot_max_chars: default_prompt_snapshot_max_chars(),
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

    /// Load agent-facing configuration only from `ALAN_CONFIG_PATH`.
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::load_with_metadata()?.into_config())
    }

    /// Load agent-facing configuration together with source metadata.
    pub fn load_with_metadata() -> anyhow::Result<LoadedConfig> {
        Self::load_from_override(Self::env_override_config_path())
    }

    /// Load configuration from file (TOML format)
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read configuration file {}", path.display()))?;
        let mut config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse configuration file {}", path.display()))?;
        config.skill_overrides = config.resolved_skill_overrides();
        config.validate_compaction_thresholds(path.display().to_string())?;
        Ok(config)
    }

    /// Get the config file path.
    /// The only direct runtime configuration source is `ALAN_CONFIG_PATH`.
    pub fn config_file_path() -> Option<std::path::PathBuf> {
        Self::env_override_config_path()
    }

    fn env_override_config_path() -> Option<std::path::PathBuf> {
        std::env::var("ALAN_CONFIG_PATH")
            .ok()
            .map(std::path::PathBuf::from)
    }

    fn load_from_override(
        override_path: Option<std::path::PathBuf>,
    ) -> anyhow::Result<LoadedConfig> {
        if let Some(config_path) = override_path {
            anyhow::ensure!(
                config_path.is_absolute(),
                "ALAN_CONFIG_PATH must be an absolute path: {}",
                config_path.display()
            );
            let config = Self::from_file(&config_path)?;
            tracing::info!(path = %config_path.display(), "Loaded configuration from file");
            return Ok(LoadedConfig {
                config,
                path: Some(config_path),
                source: ConfigSourceKind::EnvOverride,
            });
        }

        Ok(LoadedConfig {
            config: Self::default(),
            path: None,
            source: ConfigSourceKind::Default,
        })
    }

    pub fn with_definition_overlays(
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
        Ok(config)
    }

    pub fn with_definition_overlay_content(
        &self,
        content: &str,
        source: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let model_catalog = self.model_catalog.clone();
        let mut merged = toml::Value::try_from(self.clone())
            .context("failed to serialize base configuration for overlay merge")?;
        let mut overlay: toml::Value = toml::from_str(content)
            .with_context(|| format!("failed to parse configuration file {}", source.display()))?;
        strip_skill_overrides_from_overlay(&mut overlay);
        merge_toml_overlay(&mut merged, overlay);
        let mut config: Self = merged
            .try_into()
            .context("failed to deserialize merged agent-root configuration")?;
        if config.connection_profile == self.connection_profile {
            config.copy_internal_provider_config_from(self);
        } else if config.connection_profile.is_none() {
            config.connection_profile = self.connection_profile.clone();
            config.copy_internal_provider_config_from(self);
        }
        config.skill_overrides = merge_skill_override_overlay_from_content(
            &self.resolved_skill_overrides(),
            content,
            source,
        )?;
        config.model_catalog = model_catalog;
        config.validate_compaction_thresholds("merged agent-root configuration".to_string())?;
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
            .unwrap_or_else(default_compaction_hard_trigger_ratio)
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

pub(crate) fn merge_skill_override_overlay_from_content(
    base_overrides: &[SkillOverride],
    content: &str,
    source: &std::path::Path,
) -> anyhow::Result<Vec<SkillOverride>> {
    let overlay_overrides = parse_skill_overrides(content, source)?;
    Ok(merge_skill_overrides(base_overrides, &overlay_overrides))
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
#[path = "config_tests.rs"]
mod tests;
