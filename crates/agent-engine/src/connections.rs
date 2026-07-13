use crate::config::{Config, LlmProvider};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

const CONNECTIONS_VERSION: u32 = 1;
const CHATGPT_AUTH_BACKEND: &str = "host_managed_auth";
const SECRET_STORE_BACKEND: &str = "host_credential_store";
const AMBIENT_BACKEND: &str = "ambient";
const SECRET_STORE_FILE_NAME: &str = "secrets.toml";

/// Host-selected backing for Connection Service metadata and Host-owned secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStoreBindings {
    pub metadata_path: PathBuf,
    pub credentials_dir: PathBuf,
}

impl ConnectionStoreBindings {
    pub fn new(metadata_path: PathBuf, credentials_dir: PathBuf) -> anyhow::Result<Self> {
        validate_safe_absolute_path("connection metadata path", &metadata_path)?;
        validate_safe_absolute_path("Host credential directory", &credentials_dir)?;
        Ok(Self {
            metadata_path,
            credentials_dir,
        })
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SecretStoreFile {
    #[serde(default)]
    secrets: BTreeMap<String, String>,
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
    pub credential_kind: CredentialKind,
    pub supports_browser_login: bool,
    pub supports_device_login: bool,
    pub supports_secret_entry: bool,
    pub supports_logout: bool,
    pub supports_test: bool,
    pub required_settings: &'static [&'static str],
    pub optional_settings: &'static [&'static str],
    pub default_settings: &'static [(&'static str, &'static str)],
}

#[derive(Clone)]
pub struct SecretStore {
    credentials_dir: PathBuf,
    resolved_secrets: BTreeMap<String, String>,
}

impl std::fmt::Debug for SecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretStore")
            .field("credentials_dir", &self.credentials_dir)
            .field("resolved_secret_count", &self.resolved_secrets.len())
            .finish()
    }
}

impl SecretStore {
    pub fn from_directory(credentials_dir: &Path) -> anyhow::Result<Self> {
        validate_safe_absolute_path("Host credential directory", credentials_dir)?;
        Ok(Self {
            credentials_dir: credentials_dir.to_path_buf(),
            resolved_secrets: BTreeMap::new(),
        })
    }

    pub fn with_resolved_secret(
        credentials_dir: &Path,
        credential_id: &str,
        secret: String,
    ) -> anyhow::Result<Self> {
        let mut store = Self::from_directory(credentials_dir)?;
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        store
            .resolved_secrets
            .insert(credential_id.to_string(), secret);
        Ok(store)
    }

    #[cfg(test)]
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            credentials_dir: root_dir,
            resolved_secrets: BTreeMap::new(),
        }
    }

    pub fn load(&self, credential_id: &str) -> anyhow::Result<Option<String>> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        if let Some(secret) = self.resolved_secrets.get(credential_id) {
            return Ok(Some(secret.clone()));
        }
        let secrets = self.read_secret_file()?;
        Ok(secrets.secrets.get(credential_id).cloned())
    }

    pub fn save(&self, credential_id: &str, secret: &str) -> anyhow::Result<()> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        let mut secrets = self.read_secret_file()?;
        secrets
            .secrets
            .insert(credential_id.to_string(), secret.trim().to_string());
        self.write_secret_file(&secrets)
    }

    pub fn delete(&self, credential_id: &str) -> anyhow::Result<bool> {
        let credential_id = validated_identifier_component("credential id", credential_id)?;
        let mut secrets = self.read_secret_file()?;
        let removed = secrets.secrets.remove(credential_id).is_some();
        if removed {
            self.write_secret_file(&secrets)?;
        }
        Ok(removed)
    }

    fn secret_file_path(&self) -> anyhow::Result<PathBuf> {
        let path = self.credentials_dir.join(SECRET_STORE_FILE_NAME);
        validate_safe_absolute_path("secret store path", &path)?;
        Ok(path)
    }

    fn read_secret_file(&self) -> anyhow::Result<SecretStoreFile> {
        let path = self.secret_file_path()?;
        match std::fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content)
                .with_context(|| format!("failed to parse secret store {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SecretStoreFile::default())
            }
            Err(error) => Err(error)
                .with_context(|| format!("failed to read secret store {}", path.display())),
        }
    }

    fn write_secret_file(&self, secret_file: &SecretStoreFile) -> anyhow::Result<()> {
        let path = self.secret_file_path()?;
        if secret_file.secrets.is_empty() {
            match std::fs::remove_file(&path) {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to remove secret store {}", path.display())
                    });
                }
            }
        }
        let rendered = toml::to_string_pretty(secret_file)
            .context("failed to encode secret store while saving")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create credentials directory {}",
                    parent.display()
                )
            })?;
        }
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;

            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .with_context(|| format!("failed to open secret store {}", path.display()))?;
            file.write_all(rendered.as_bytes())
                .with_context(|| format!("failed to write secret store {}", path.display()))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&path, rendered)
                .with_context(|| format!("failed to write secret store {}", path.display()))?;
        }
        Ok(())
    }
}

impl ConnectionsFile {
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
        validate_safe_absolute_path("connection metadata path", path)?;
        if self.version != CONNECTIONS_VERSION {
            anyhow::bail!("unsupported connections file version {}", self.version);
        }
        let rendered = toml::to_string_pretty(self)
            .context("failed to encode connections.toml while saving")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create connection metadata directory {}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(path, rendered)
            .with_context(|| format!("failed to write connections file {}", path.display()))
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
            if credential.kind != descriptor.credential_kind {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` uses credential kind `{}` but provider `{}` requires `{}`",
                    credential.kind.as_str(),
                    profile.provider.as_str(),
                    descriptor.credential_kind.as_str(),
                );
            }
            credential.kind
        } else {
            if descriptor.credential_kind != CredentialKind::AmbientCloudAuth {
                anyhow::bail!(
                    "Profile `{selected_profile_id}` requires a credential for provider `{}`",
                    profile.provider.as_str()
                );
            }
            CredentialKind::AmbientCloudAuth
        };

        Ok(ResolvedConnectionProfile {
            profile_id: selected_profile_id,
            provider: profile.provider,
            credential_id: profile.credential_id.clone(),
            credential_kind,
            settings: normalized,
        })
    }

    pub fn apply_profile_to_config(
        &self,
        profile_id: Option<&str>,
        secret_store: &SecretStore,
        config: &mut Config,
    ) -> anyhow::Result<ResolvedConnectionProfile> {
        let resolved = self.resolve_profile(profile_id)?;
        apply_resolved_profile_to_config(&resolved, secret_store, config)?;
        Ok(resolved)
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

pub fn provider_catalog() -> &'static [ProviderDescriptor] {
    const CHATGPT_REQUIRED: &[&str] = &["base_url", "model"];
    const CHATGPT_OPTIONAL: &[&str] = &["account_id"];
    const CHATGPT_DEFAULTS: &[(&str, &str)] = &[
        ("base_url", "https://chatgpt.com/backend-api/codex"),
        ("model", "gpt-5.3-codex"),
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
        ("model", "claude-3-5-sonnet-latest"),
        ("client_name", ""),
        ("user_agent", ""),
    ];

    const GEMINI_REQUIRED: &[&str] = &["project_id", "location", "model"];
    const GEMINI_DEFAULTS: &[(&str, &str)] = &[
        ("project_id", ""),
        ("location", "us-central1"),
        ("model", "gemini-2.0-flash"),
    ];

    static CATALOG: std::sync::OnceLock<Vec<ProviderDescriptor>> = std::sync::OnceLock::new();
    CATALOG
        .get_or_init(|| {
            vec![
                ProviderDescriptor {
                    provider_id: LlmProvider::Chatgpt,
                    display_name: "ChatGPT / Codex",
                    credential_kind: CredentialKind::ManagedOauth,
                    supports_browser_login: true,
                    supports_device_login: true,
                    supports_secret_entry: false,
                    supports_logout: true,
                    supports_test: true,
                    required_settings: CHATGPT_REQUIRED,
                    optional_settings: CHATGPT_OPTIONAL,
                    default_settings: CHATGPT_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::OpenAiResponses,
                    display_name: "OpenAI Responses API",
                    credential_kind: CredentialKind::SecretString,
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
                    provider_id: LlmProvider::OpenAiChatCompletions,
                    display_name: "OpenAI Chat Completions API",
                    credential_kind: CredentialKind::SecretString,
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
                    credential_kind: CredentialKind::SecretString,
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
                    provider_id: LlmProvider::OpenRouter,
                    display_name: "OpenRouter",
                    credential_kind: CredentialKind::SecretString,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: OPENROUTER_REQUIRED,
                    optional_settings: OPENROUTER_OPTIONAL,
                    default_settings: OPENROUTER_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::AnthropicMessages,
                    display_name: "Anthropic Messages API",
                    credential_kind: CredentialKind::SecretString,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: true,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: ANTHROPIC_REQUIRED,
                    optional_settings: ANTHROPIC_OPTIONAL,
                    default_settings: ANTHROPIC_DEFAULTS,
                },
                ProviderDescriptor {
                    provider_id: LlmProvider::GoogleGeminiGenerateContent,
                    display_name: "Google Gemini GenerateContent API",
                    credential_kind: CredentialKind::AmbientCloudAuth,
                    supports_browser_login: false,
                    supports_device_login: false,
                    supports_secret_entry: false,
                    supports_logout: false,
                    supports_test: true,
                    required_settings: GEMINI_REQUIRED,
                    optional_settings: &[],
                    default_settings: GEMINI_DEFAULTS,
                },
            ]
        })
        .as_slice()
}

pub fn default_profile_source() -> String {
    "managed".to_string()
}

pub fn default_profile_timestamp() -> DateTime<Utc> {
    Utc::now()
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

fn validated_identifier_component<'a>(label: &str, value: &'a str) -> anyhow::Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        anyhow::bail!("invalid {label} `{value}`");
    }
    Ok(trimmed)
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

fn apply_resolved_profile_to_config(
    resolved: &ResolvedConnectionProfile,
    secret_store: &SecretStore,
    config: &mut Config,
) -> anyhow::Result<()> {
    apply_resolved_profile_metadata_to_config(resolved, config);
    if matches!(
        resolved.provider,
        LlmProvider::Chatgpt | LlmProvider::GoogleGeminiGenerateContent
    ) {
        return Ok(());
    }
    let Some(credential_id) = resolved.credential_id.as_deref() else {
        anyhow::bail!("Profile `{}` is missing a credential", resolved.profile_id);
    };
    let api_key = secret_store.load(credential_id)?.ok_or_else(|| {
        anyhow::anyhow!(
            "Credential `{credential_id}` for profile `{}` is missing a secret",
            resolved.profile_id
        )
    })?;
    match resolved.provider {
        LlmProvider::OpenAiResponses => config.openai_responses_api_key = Some(api_key),
        LlmProvider::OpenAiChatCompletions => {
            config.openai_chat_completions_api_key = Some(api_key)
        }
        LlmProvider::OpenAiChatCompletionsCompatible => {
            config.openai_chat_completions_compatible_api_key = Some(api_key)
        }
        LlmProvider::OpenRouter => config.openrouter_api_key = Some(api_key),
        LlmProvider::AnthropicMessages => config.anthropic_messages_api_key = Some(api_key),
        LlmProvider::Chatgpt | LlmProvider::GoogleGeminiGenerateContent => unreachable!(),
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
            config.google_gemini_generate_content_project_id =
                Some(resolved.settings["project_id"].clone());
            config.google_gemini_generate_content_location = resolved.settings["location"].clone();
            config.google_gemini_generate_content_model = resolved.settings["model"].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn normalize_profile_settings_applies_defaults() {
        let settings = BTreeMap::from([("model".to_string(), "gpt-5".to_string())]);
        let normalized = normalize_profile_settings(LlmProvider::OpenAiResponses, &settings);
        assert_eq!(
            normalized.get("base_url").map(String::as_str),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(normalized.get("model").map(String::as_str), Some("gpt-5"));
    }

    #[test]
    fn openrouter_descriptor_exposes_default_model_and_metadata_settings() {
        let descriptor = ConnectionsFile::profile_descriptor(LlmProvider::OpenRouter);
        assert_eq!(descriptor.provider_id, LlmProvider::OpenRouter);
        assert_eq!(descriptor.credential_kind, CredentialKind::SecretString);
        assert!(descriptor.supports_secret_entry);
        assert!(descriptor.required_settings.contains(&"model"));
        assert!(descriptor.optional_settings.contains(&"base_url"));
        assert!(descriptor.optional_settings.contains(&"http_referer"));
        assert!(descriptor.optional_settings.contains(&"x_title"));
        assert!(descriptor.optional_settings.contains(&"app_categories"));

        let normalized = normalize_profile_settings(LlmProvider::OpenRouter, &BTreeMap::new());
        assert_eq!(
            normalized.get("base_url").map(String::as_str),
            Some("https://openrouter.ai/api/v1")
        );
        assert_eq!(
            normalized.get("model").map(String::as_str),
            Some("moonshotai/kimi-k2.6")
        );
        validate_profile_settings(LlmProvider::OpenRouter, &normalized).unwrap();
    }

    #[test]
    fn openrouter_settings_are_not_accepted_by_generic_compatible_provider() {
        let settings = BTreeMap::from([
            (
                "base_url".to_string(),
                "https://proxy.example/v1".to_string(),
            ),
            ("model".to_string(), "qwen3.5-plus".to_string()),
            ("http_referer".to_string(), "https://alan.local".to_string()),
        ]);

        let result =
            validate_profile_settings(LlmProvider::OpenAiChatCompletionsCompatible, &settings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http_referer"));
    }

    #[test]
    fn secret_store_round_trips_secret() {
        let temp = TempDir::new().unwrap();
        let store = SecretStore::from_directory(temp.path()).unwrap();
        store.save("kimi", "sk-test").unwrap();
        assert_eq!(store.load("kimi").unwrap().as_deref(), Some("sk-test"));
        assert!(store.delete("kimi").unwrap());
        assert_eq!(store.load("kimi").unwrap(), None);
    }

    #[test]
    fn secret_store_accepts_one_host_resolved_secret_without_platform_logic() {
        let temp = TempDir::new().unwrap();
        let store =
            SecretStore::with_resolved_secret(temp.path(), "native", "host-secret".to_string())
                .unwrap();

        assert_eq!(
            store.load("native").unwrap().as_deref(),
            Some("host-secret")
        );
        assert_eq!(store.load("missing").unwrap(), None);
        assert!(!format!("{store:?}").contains("host-secret"));
    }

    #[test]
    fn secret_store_uses_only_explicit_credentials_dir() {
        let temp = TempDir::new().unwrap();
        let credentials = temp.path().join("host-store/dev/credentials");
        let store = SecretStore::from_directory(&credentials).unwrap();

        store.save("dev-profile", "sk-dev").unwrap();

        assert!(
            std::fs::read_to_string(credentials.join("secrets.toml"))
                .unwrap()
                .contains("sk-dev")
        );
        assert!(
            !temp
                .path()
                .join("host-store/stable/credentials/secrets.toml")
                .exists()
        );
    }

    #[test]
    fn dev_connection_store_does_not_fall_back_to_stable_store() {
        let temp = TempDir::new().unwrap();
        let stable_path = temp.path().join("system-store/stable/connections.toml");
        let dev_path = temp.path().join("system-store/dev/connections.toml");
        let stable_connections = ConnectionsFile {
            default_profile: Some("stable-main".to_string()),
            profiles: BTreeMap::from([(
                "stable-main".to_string(),
                ConnectionProfile {
                    provider: LlmProvider::OpenAiResponses,
                    label: Some("Stable".to_string()),
                    credential_id: Some("stable-main".to_string()),
                    created_at: default_profile_timestamp(),
                    updated_at: default_profile_timestamp(),
                    source: default_profile_source(),
                    settings: BTreeMap::new(),
                },
            )]),
            ..ConnectionsFile::default()
        };
        stable_connections
            .save_to_path(&stable_path)
            .expect("save stable connections");

        let (dev_connections, loaded_path) =
            ConnectionsFile::load_from_path(&dev_path).expect("load dev connections");

        assert_eq!(loaded_path.as_deref(), Some(dev_path.as_path()));
        assert_eq!(dev_connections.default_profile, None);
        assert!(dev_connections.profiles.is_empty());
    }

    #[test]
    fn resolve_profile_uses_default_profile() {
        let file = ConnectionsFile {
            default_profile: Some("chatgpt-main".to_string()),
            credentials: BTreeMap::from([(
                "chatgpt".to_string(),
                ConnectionCredential {
                    kind: CredentialKind::ManagedOauth,
                    provider_family: LlmProvider::Chatgpt,
                    label: "ChatGPT login".to_string(),
                    backend: CHATGPT_AUTH_BACKEND.to_string(),
                },
            )]),
            profiles: BTreeMap::from([(
                "chatgpt-main".to_string(),
                ConnectionProfile {
                    provider: LlmProvider::Chatgpt,
                    label: Some("ChatGPT".to_string()),
                    credential_id: Some("chatgpt".to_string()),
                    created_at: default_profile_timestamp(),
                    updated_at: default_profile_timestamp(),
                    source: default_profile_source(),
                    settings: BTreeMap::new(),
                },
            )]),
            ..ConnectionsFile::default()
        };

        let resolved = file.resolve_profile(None).unwrap();
        assert_eq!(resolved.profile_id, "chatgpt-main");
        assert_eq!(
            resolved.settings.get("model").map(String::as_str),
            Some("gpt-5.3-codex")
        );
    }

    #[test]
    fn apply_openrouter_profile_to_config_loads_secret_and_metadata() {
        let temp = TempDir::new().unwrap();
        let store = SecretStore::from_directory(temp.path()).unwrap();
        store.save("openrouter-main", "sk-or").unwrap();

        let file = ConnectionsFile {
            credentials: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionCredential {
                    kind: CredentialKind::SecretString,
                    provider_family: LlmProvider::OpenRouter,
                    label: "OpenRouter credential".to_string(),
                    backend: SECRET_STORE_BACKEND.to_string(),
                },
            )]),
            profiles: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionProfile {
                    provider: LlmProvider::OpenRouter,
                    label: Some("OpenRouter".to_string()),
                    credential_id: Some("openrouter-main".to_string()),
                    created_at: default_profile_timestamp(),
                    updated_at: default_profile_timestamp(),
                    source: default_profile_source(),
                    settings: BTreeMap::from([
                        ("http_referer".to_string(), "https://alan.local".to_string()),
                        ("x_title".to_string(), "alan".to_string()),
                        (
                            "app_categories".to_string(),
                            "cli-agent,devtool".to_string(),
                        ),
                    ]),
                },
            )]),
            ..ConnectionsFile::default()
        };

        let mut config = Config::default();
        let resolved = file
            .apply_profile_to_config(Some("openrouter-main"), &store, &mut config)
            .unwrap();

        assert_eq!(resolved.provider, LlmProvider::OpenRouter);
        assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
        assert_eq!(config.openrouter_api_key.as_deref(), Some("sk-or"));
        assert_eq!(config.openrouter_base_url, "https://openrouter.ai/api/v1");
        assert_eq!(config.openrouter_model, "moonshotai/kimi-k2.6");
        assert_eq!(
            config.openrouter_http_referer.as_deref(),
            Some("https://alan.local")
        );
        assert_eq!(config.openrouter_x_title.as_deref(), Some("alan"));
        assert_eq!(
            config.openrouter_app_categories,
            vec!["cli-agent", "devtool"]
        );
    }

    #[test]
    fn service_owned_profile_metadata_does_not_load_secret_material() {
        let file = ConnectionsFile {
            credentials: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionCredential {
                    kind: CredentialKind::SecretString,
                    provider_family: LlmProvider::OpenRouter,
                    label: "OpenRouter credential".to_string(),
                    backend: SECRET_STORE_BACKEND.to_string(),
                },
            )]),
            profiles: BTreeMap::from([(
                "openrouter-main".to_string(),
                ConnectionProfile {
                    provider: LlmProvider::OpenRouter,
                    label: None,
                    credential_id: Some("openrouter-main".to_string()),
                    created_at: default_profile_timestamp(),
                    updated_at: default_profile_timestamp(),
                    source: default_profile_source(),
                    settings: BTreeMap::new(),
                },
            )]),
            ..ConnectionsFile::default()
        };
        let mut config = Config::default();

        file.apply_profile_metadata_to_config(Some("openrouter-main"), &mut config)
            .unwrap();

        assert_eq!(config.llm_provider, LlmProvider::OpenRouter);
        assert_eq!(
            config.connection_profile.as_deref(),
            Some("openrouter-main")
        );
        assert_eq!(config.openrouter_api_key, None);
    }
}
