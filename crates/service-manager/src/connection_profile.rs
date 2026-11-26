use alan_agent_engine::{Config, LlmProvider};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const CONNECTIONS_VERSION: u32 = 1;
const CHATGPT_AUTH_BACKEND: &str = "host_managed_auth";
const SECRET_STORE_BACKEND: &str = "host_credential_store";
const AMBIENT_BACKEND: &str = "ambient";

/// Host-selected System Store backing for Connection Service metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStoreBindings {
    pub metadata_path: PathBuf,
}

impl ConnectionStoreBindings {
    pub fn new(metadata_path: PathBuf) -> anyhow::Result<Self> {
        validate_safe_absolute_path("connection metadata path", &metadata_path)?;
        Ok(Self { metadata_path })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    ManagedOauth,
    SecretString,
    AmbientCloudAuth,
}

impl CredentialKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ManagedOauth => "managed_oauth",
            Self::SecretString => "secret_string",
            Self::AmbientCloudAuth => "ambient_cloud_auth",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionCredential {
    pub kind: CredentialKind,
    pub provider_family: LlmProvider,
    pub label: String,
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionProfile {
    pub provider: LlmProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    #[serde(default = "default_profile_timestamp")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_profile_timestamp")]
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_profile_source")]
    pub source: String,
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionsFile {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub credentials: BTreeMap<String, ConnectionCredential>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ConnectionProfile>,
}

impl Default for ConnectionsFile {
    fn default() -> Self {
        Self {
            version: CONNECTIONS_VERSION,
            default_profile: None,
            credentials: BTreeMap::new(),
            profiles: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConnectionProfile {
    pub profile_id: String,
    pub provider: LlmProvider,
    pub credential_id: Option<String>,
    pub credential_kind: CredentialKind,
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDescriptor {
    pub provider_id: LlmProvider,
    pub display_name: &'static str,
    /// Whether this provider is part of Alan's official first-party built-in set.
    pub first_party: bool,
    pub credential_kind: CredentialKind,
    /// Whether profiles for this provider may omit a credential (e.g. local Magpie
    /// without auth, or Vertex AI using ambient ADC instead of Express Mode).
    pub credential_optional: bool,
    pub supports_browser_login: bool,
    pub supports_device_login: bool,
    pub supports_secret_entry: bool,
    pub supports_logout: bool,
    pub supports_test: bool,
    pub required_settings: &'static [&'static str],
    pub optional_settings: &'static [&'static str],
    pub default_settings: &'static [(&'static str, &'static str)],
}

fn lock_metadata(path: &Path) -> anyhow::Result<std::fs::File> {
    validate_safe_absolute_path("connection metadata path", path)?;
    std::fs::create_dir_all(
        path.parent()
            .context("connection metadata path has no parent")?,
    )?;
    let mut lock_path = path.as_os_str().to_owned();
    lock_path.push(".lock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let lock = options
        .open(PathBuf::from(lock_path))
        .context("open Connection Store lock")?;
    anyhow::ensure!(
        lock.metadata()?.is_file(),
        "Connection Store lock is not a regular file"
    );
    lock.lock().context("lock Connection Store metadata")?;
    Ok(lock)
}

impl ConnectionsFile {
    /// Stable content fingerprint for optimistic metadata replacement.
    pub fn fingerprint(&self) -> anyhow::Result<String> {
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub fn load_from_path(path: &Path) -> anyhow::Result<(Self, Option<PathBuf>)> {
        validate_safe_absolute_path("connection metadata path", path)?;
        match std::fs::read_to_string(path) {
            Ok(content) => {
                let parsed: Self = toml::from_str(&content).with_context(|| {
                    format!("failed to parse connections file {}", path.display())
                })?;
                if parsed.version != CONNECTIONS_VERSION {
                    anyhow::bail!(
                        "unsupported connections file version {} in {}",
                        parsed.version,
                        path.display()
                    );
                }
                Ok((parsed, Some(path.to_path_buf())))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok((Self::default(), Some(path.to_path_buf())))
            }
            Err(error) => Err(error)
                .with_context(|| format!("failed to read connections file {}", path.display())),
        }
    }

    pub fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let _lock = lock_metadata(path)?;
        self.save_locked(path)
    }

    /// Atomically reject a stale snapshot or publish its replacement under the store lock.
    pub fn save_if_unchanged(&self, path: &Path, expected: &Self) -> anyhow::Result<()> {
        let _lock = lock_metadata(path)?;
        anyhow::ensure!(
            Self::load_from_path(path)?.0 == *expected,
            "connection metadata changed; reload before retrying"
        );
        self.save_locked(path)
    }

    fn save_locked(&self, path: &Path) -> anyhow::Result<()> {
        validate_safe_absolute_path("connection metadata path", path)?;
        if self.version != CONNECTIONS_VERSION {
            anyhow::bail!("unsupported connections file version {}", self.version);
        }
        let rendered = toml::to_string_pretty(self)
            .context("failed to encode connections.toml while saving")?;
        let parent = path
            .parent()
            .context("connection metadata path has no parent")?;
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create connection metadata directory {}",
                parent.display()
            )
        })?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        staged.write_all(rendered.as_bytes())?;
        staged.as_file().sync_all()?;
        staged
            .persist(path)
            .with_context(|| format!("failed to replace connections file {}", path.display()))?;
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }

    pub fn profile_descriptor(provider: LlmProvider) -> &'static ProviderDescriptor {
        provider_catalog()
            .iter()
            .find(|descriptor| descriptor.provider_id == provider)
            .expect("provider descriptor missing")
    }

    pub fn resolve_profile(
        &self,
        profile_id: Option<&str>,
    ) -> anyhow::Result<ResolvedConnectionProfile> {
        let selected_profile_id = profile_id
            .map(|value| {
                sanitize_identifier(value)
                    .ok_or_else(|| anyhow::anyhow!("invalid profile id `{value}`"))
            })
            .transpose()?
            .or_else(|| self.default_profile.clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No connection profile selected. Set connection_profile in agent.toml or default_profile in connections.toml."
                )
            })?;
        let profile = self
            .profiles
            .get(&selected_profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{selected_profile_id}`"))?;
        let descriptor = Self::profile_descriptor(profile.provider);
        let normalized = normalize_profile_settings(profile.provider, &profile.settings);
        validate_profile_settings(profile.provider, &normalized)?;

        let credential_kind = if let Some(credential_id) = profile.credential_id.as_deref() {
            let credential = self.credentials.get(credential_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "Profile `{selected_profile_id}` references unknown credential `{credential_id}`"
                )
            })?;
            if credential.provider_family != profile.provider {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` uses provider `{}` but credential `{credential_id}` is bound to `{}`",
                    profile.provider.as_str(),
                    credential.provider_family.as_str(),
                );
            }
            let vertex_express_secret = profile.provider
                == LlmProvider::GoogleGeminiGenerateContent
                && credential.kind == CredentialKind::SecretString;
            if credential.kind != descriptor.credential_kind && !vertex_express_secret {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` uses credential kind `{}` but provider `{}` requires `{}`",
                    credential.kind.as_str(),
                    profile.provider.as_str(),
                    descriptor.credential_kind.as_str(),
                );
            }
            credential.kind
        } else {
            if !descriptor.credential_optional
                && descriptor.credential_kind != CredentialKind::AmbientCloudAuth
            {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` requires a credential for provider `{}`",
                    profile.provider.as_str()
                );
            }
            if profile.provider == LlmProvider::GoogleGeminiGenerateContent
                && normalized
                    .get("project_id")
                    .map(|value| value.trim())
                    .unwrap_or_default()
                    .is_empty()
            {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` uses Vertex AI ADC and requires setting `project_id` (or attach a Vertex Express API key credential)"
                );
            }
            descriptor.credential_kind
        };

        Ok(ResolvedConnectionProfile {
            profile_id: selected_profile_id,
            provider: profile.provider,
            credential_id: profile.credential_id.clone(),
            credential_kind,
            settings: normalized,
        })
    }

    pub fn apply_profile_metadata_to_config(
        &self,
        profile_id: Option<&str>,
        config: &mut Config,
    ) -> anyhow::Result<ResolvedConnectionProfile> {
        let resolved = self.resolve_profile(profile_id)?;
        apply_resolved_profile_metadata_to_config(&resolved, config);
        Ok(resolved)
    }
}

/// Official first-party built-in providers shown in connection wizards and defaults:
/// Magpie, OpenAI (Responses + ChatGPT/Codex), Google Cloud Vertex AI, and OpenRouter.
pub fn first_party_provider_catalog() -> &'static [ProviderDescriptor] {
    static FIRST_PARTY: std::sync::OnceLock<Vec<ProviderDescriptor>> = std::sync::OnceLock::new();
    FIRST_PARTY
        .get_or_init(|| {
            provider_catalog()
                .iter()
                .filter(|descriptor| descriptor.first_party)
                .cloned()
                .collect()
        })
        .as_slice()
}

pub fn provider_catalog() -> &'static [ProviderDescriptor] {
    const MAGPIE_REQUIRED: &[&str] = &["base_url", "model"];
    const MAGPIE_OPTIONAL: &[&str] = &["thinking_level_map"];
    const MAGPIE_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "http://127.0.0.1:3425/v1"),
        ("model", "default"),
    ];

    const CHATGPT_REQUIRED: &[&str] = &["base_url", "model"];
    const CHATGPT_OPTIONAL: &[&str] = &["account_id"];
    const CHATGPT_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://api.openai.com/v1"),
        ("model", "gpt-5.4"),
        ("account_id", ""),
    ];

    const OPENAI_REQUIRED: &[&str] = &["base_url", "model"];
    const OPENAI_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://api.openai.com/v1"),
        ("model", "gpt-5.4"),
    ];

    const OPENAI_COMPAT_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://api.openai.com/v1"),
        ("model", "qwen3.5-plus"),
    ];

    const OPENROUTER_REQUIRED: &[&str] = &["model"];
    const OPENROUTER_OPTIONAL: &[&str] = &["base_url", "http_referer", "x_title", "app_categories"];
    const OPENROUTER_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://openrouter.ai/api/v1"),
        ("model", "moonshotai/kimi-k2.6"),
    ];

    const ANTHROPIC_REQUIRED: &[&str] = &["base_url", "model"];
    const ANTHROPIC_OPTIONAL: &[&str] = &["client_name", "user_agent"];
    const ANTHROPIC_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://api.anthropic.com/v1"),
        ("model", "claude-sonnet-4-5"),
        ("client_name", ""),
        ("user_agent", ""),
    ];

    const GEMINI_REQUIRED: &[&str] = &["location", "model"];
    const GEMINI_OPTIONAL: &[&str] = &["project_id"];
    const GEMINI_DEFAULTS: &[(&str, &str)] = &[
        ("project_id", ""),
        ("location", "global"),
        ("model", "gemini-2.5-flash"),
    ];

    static CATALOG: std::sync::OnceLock<Vec<ProviderDescriptor>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            vec![
                ProviderDescriptor {
                    provider_id: LlmProvider::Magpie,
                    display_name: "Magpie",
                    first_party: true,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: true,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: MAGPIE_REQUIRED,
                    optional_settings: MAGPIE_OPTIONAL,
                    default_settings: MAGPIE_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::OpenAiResponses,
                    display_name: "OpenAI Responses API",
                    first_party: true,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: OPENAI_REQUIRED,
                    optional_settings: &[],
                    default_settings: OPENAI_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::Chatgpt,
                    display_name: "ChatGPT (Sign in with ChatGPT)",
                    first_party: true,
                    credential_kind: CredentialKind::ManagedOauth,
                    credential_optional: false,
                    supports_browser_login: true,
                    supports_device_login: false,
                    supports_secret_entry: false,
                    supports_logout: true,
                    supports_test: true,
                    required_settings: CHATGPT_REQUIRED,
                    optional_settings: CHATGPT_OPTIONAL,
                    default_settings: CHATGPT_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::GoogleGeminiGenerateContent,
                    display_name: "Google Cloud Vertex AI",
                    first_party: true,
                    credential_kind: CredentialKind::AmbientCloudAuth,
                    credential_optional: true,
                    supports_browser_login: true,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: true,
                    supports_test: true,
                    required_settings: GEMINI_REQUIRED,
                    optional_settings: GEMINI_OPTIONAL,
                    default_settings: GEMINI_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::OpenRouter,
                    display_name: "OpenRouter",
                    first_party: true,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: true,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: OPENROUTER_REQUIRED,
                    optional_settings: OPENROUTER_OPTIONAL,
                    default_settings: OPENROUTER_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::TypesafeEvaluation,
                    display_name: "TypeSafe Choice",
                    first_party: false,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: true,
                    supports_test: true,
                    required_settings: &["model"],
                    optional_settings: &[],
                    default_settings: &[("model", "jev-1.13.0")],
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::OpenAiChatCompletions,
                    display_name: "OpenAI Chat Completions API",
                    first_party: false,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: OPENAI_REQUIRED,
                    optional_settings: &[],
                    default_settings: OPENAI_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::OpenAiChatCompletionsCompatible,
                    display_name: "OpenAI Chat Completions API-compatible",
                    first_party: false,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: OPENAI_REQUIRED,
                    optional_settings: &[],
                    default_settings: OPENAI_COMPAT_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::AnthropicMessages,
                    display_name: "Anthropic Messages API (protocol)",
                    first_party: false,
                    credential_kind: CredentialKind::SecretString,
                    credential_optional: false,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: ANTHROPIC_REQUIRED,
                    optional_settings: ANTHROPIC_OPTIONAL,
                    default_settings: ANTHROPIC_DEFAULTS,
                },
            ]
        })
        .as_slice()
}

pub fn default_profile_source() -> String {
    "managed".to_string()
}

/// Stable sentinel for legacy metadata whose creation or update time is unknown.
pub fn default_profile_timestamp() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

pub fn normalize_profile_settings(
    provider: LlmProvider,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    let mut normalized = BTreeMap::new();
    for (key, value) in descriptor.default_settings {
        normalized.insert((*key).to_string(), (*value).to_string());
    }
    for (key, value) in settings {
        normalized.insert(key.clone(), value.clone());
    }
    normalized
}

pub fn validate_profile_settings(
    provider: LlmProvider,
    settings: &BTreeMap<String, String>,
) -> anyhow::Result<()> {
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    for key in descriptor.required_settings {
        let value = settings
            .get(*key)
            .map(|value| value.trim())
            .unwrap_or_default();
        if value.is_empty() {
            anyhow::bail!(
                "Provider `{}` requires setting `{}`",
                provider.as_str(),
                key
            );
        }
    }
    let allowed_keys: std::collections::BTreeSet<&str> = descriptor
        .required_settings
        .iter()
        .chain(descriptor.optional_settings.iter())
        .copied()
        .collect();
    for key in settings.keys() {
        if !allowed_keys.contains(key.as_str()) {
            anyhow::bail!(
                "Provider `{}` does not support setting `{}`",
                provider.as_str(),
                key
            );
        }
    }
    if provider == LlmProvider::TypesafeEvaluation {
        alan_llm::TypesafeEvaluationClient::validate_model(&settings["model"])?;
    }
    if provider == LlmProvider::Magpie
        && let Some(raw_map) = settings
            .get("thinking_level_map")
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        let parsed: serde_json::Value = serde_json::from_str(raw_map).with_context(
            || "Provider `magpie` setting `thinking_level_map` must be a JSON object",
        )?;
        anyhow::ensure!(
            alan_llm::magpie::parse_reasoning_effort_map(&parsed).is_some(),
            "Provider `magpie` setting `thinking_level_map` must map reasoning efforts to strings or null"
        );
    }
    Ok(())
}

pub fn default_credential_backend(kind: CredentialKind) -> &'static str {
    match kind {
        CredentialKind::ManagedOauth => CHATGPT_AUTH_BACKEND,
        CredentialKind::SecretString => SECRET_STORE_BACKEND,
        CredentialKind::AmbientCloudAuth => AMBIENT_BACKEND,
    }
}

pub fn sanitize_identifier(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut sanitized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            sanitized.push(ch);
        } else {
            return None;
        }
    }
    Some(sanitized)
}

fn validate_safe_absolute_path(label: &str, path: &Path) -> anyhow::Result<()> {
    if !path.is_absolute() {
        anyhow::bail!("{label} must be absolute: {}", path.display());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        anyhow::bail!(
            "{label} must not contain relative components: {}",
            path.display()
        );
    }
    Ok(())
}

fn apply_resolved_profile_metadata_to_config(
    resolved: &ResolvedConnectionProfile,
    config: &mut Config,
) {
    config.reset_internal_provider_config();
    config.connection_profile = Some(resolved.profile_id.clone());
    match resolved.provider {
        LlmProvider::Magpie => {
            config.llm_provider = LlmProvider::Magpie;
            config.magpie_base_url = resolved.settings["base_url"].clone();
            config.magpie_model = resolved.settings["model"].clone();
            config.magpie_thinking_level_map = resolved
                .settings
                .get("thinking_level_map")
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|value| alan_llm::magpie::parse_reasoning_effort_map(&value));
        }
        LlmProvider::TypesafeEvaluation => {
            config.llm_provider = LlmProvider::TypesafeEvaluation;
            config.typesafe_model = resolved.settings.get("model").cloned().unwrap_or_default();
        }

        LlmProvider::Chatgpt => {
            config.llm_provider = LlmProvider::Chatgpt;
            config.chatgpt_base_url = resolved.settings["base_url"].clone();
            config.chatgpt_model = resolved.settings["model"].clone();
            let account_id = resolved.settings["account_id"].trim().to_string();
            config.chatgpt_account_id = if account_id.is_empty() {
                None
            } else {
                Some(account_id)
            };
        }
        LlmProvider::OpenAiResponses => {
            config.llm_provider = LlmProvider::OpenAiResponses;
            config.openai_responses_base_url = resolved.settings["base_url"].clone();
            config.openai_responses_model = resolved.settings["model"].clone();
        }
        LlmProvider::OpenAiChatCompletions => {
            config.llm_provider = LlmProvider::OpenAiChatCompletions;
            config.openai_chat_completions_base_url = resolved.settings["base_url"].clone();
            config.openai_chat_completions_model = resolved.settings["model"].clone();
        }
        LlmProvider::OpenAiChatCompletionsCompatible => {
            config.llm_provider = LlmProvider::OpenAiChatCompletionsCompatible;
            config.openai_chat_completions_compatible_base_url =
                resolved.settings["base_url"].clone();
            config.openai_chat_completions_compatible_model = resolved.settings["model"].clone();
        }
        LlmProvider::OpenRouter => {
            config.llm_provider = LlmProvider::OpenRouter;
            config.openrouter_base_url = resolved
                .settings
                .get("base_url")
                .cloned()
                .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string());
            config.openrouter_model = resolved.settings["model"].clone();
            config.openrouter_http_referer = resolved
                .settings
                .get("http_referer")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            config.openrouter_x_title = resolved
                .settings
                .get("x_title")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            config.openrouter_app_categories = resolved
                .settings
                .get("app_categories")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
        }
        LlmProvider::AnthropicMessages => {
            config.llm_provider = LlmProvider::AnthropicMessages;
            config.anthropic_messages_base_url = resolved.settings["base_url"].clone();
            config.anthropic_messages_model = resolved.settings["model"].clone();
            config.anthropic_messages_client_name = resolved
                .settings
                .get("client_name")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            config.anthropic_messages_user_agent = resolved
                .settings
                .get("user_agent")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
        }
        LlmProvider::GoogleGeminiGenerateContent => {
            config.llm_provider = LlmProvider::GoogleGeminiGenerateContent;
            config.google_gemini_generate_content_project_id = resolved
                .settings
                .get("project_id")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            config.google_gemini_generate_content_location = resolved.settings["location"].clone();
            config.google_gemini_generate_content_model = resolved.settings["model"].clone();
        }
    }
}

#[cfg(test)]
#[path = "connection_profile_tests.rs"]
mod tests;
