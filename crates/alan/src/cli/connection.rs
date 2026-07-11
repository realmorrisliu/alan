use crate::registry::normalize_workspace_root_path;
use alan_agent_engine::{
    AgentRootLayout, AlanHomePaths, Config, ConnectionCredential, ConnectionProfile,
    ConnectionsFile, CredentialKind, LlmProvider, SecretStore, default_credential_backend,
    normalize_profile_settings, sanitize_identifier, validate_profile_settings,
};
use alan_auth::{
    BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthManager, DeviceCodeLoginOptions,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ConnectionPinScope {
    #[default]
    Global,
    Workspace,
}

impl ConnectionPinScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionSelectionSource {
    None,
    DefaultProfile,
    GlobalPin,
    WorkspacePin,
}

impl ConnectionSelectionSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::DefaultProfile => "default_profile",
            Self::GlobalPin => "global_pin",
            Self::WorkspacePin => "workspace_pin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectionPinState {
    scope: ConnectionPinScope,
    config_path: PathBuf,
    profile_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectionCurrentState {
    workspace_dir: Option<PathBuf>,
    global_pin: Option<ConnectionPinState>,
    workspace_pin: Option<ConnectionPinState>,
    default_profile: Option<String>,
    effective_profile: Option<String>,
    effective_source: ConnectionSelectionSource,
}

fn display_identifier(value: &str) -> String {
    sanitize_identifier(value).unwrap_or_else(|| "<redacted>".to_string())
}

fn chatgpt_auth_manager(home_paths: &AlanHomePaths) -> Result<ChatgptAuthManager> {
    Ok(ChatgptAuthManager::new(
        ChatgptAuthConfig::with_storage_path(home_paths.global_auth_path.clone()),
    )?)
}

fn load_connections() -> Result<(AlanHomePaths, ConnectionsFile)> {
    let home_paths = AlanHomePaths::detect().context("Cannot determine alan home directory")?;
    let (connections, _) = ConnectionsFile::load_global()?;
    Ok((home_paths, connections))
}

fn save_connections(connections: &ConnectionsFile) -> Result<()> {
    connections.save_global()
}

fn validated_profile_id(profile_id: &str) -> Result<String> {
    sanitize_identifier(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid profile id `{profile_id}`"))
}

fn validated_workspace_root_path(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("failed to resolve workspace path {}", path.display()))?;
    if !canonical.is_dir() {
        anyhow::bail!("workspace path {} is not a directory", canonical.display());
    }
    Ok(normalize_workspace_root_path(&canonical))
}

fn global_agent_config_path() -> Result<PathBuf> {
    let paths = AlanHomePaths::detect().context("Cannot determine alan home directory")?;
    let path = paths.global_agent_config_path;
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        || !AgentRootLayout::new().is_default_agent_config_path_shape(&path)
    {
        anyhow::bail!("invalid global agent config path {}", path.display());
    }
    Ok(path)
}

fn read_global_agent_config_table() -> Result<toml::Table> {
    let path = global_agent_config_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) if content.trim().is_empty() => Ok(toml::Table::new()),
        Ok(content) => {
            let value: toml::Value = toml::from_str(&content)
                .with_context(|| format!("failed to parse agent config {}", path.display()))?;
            value.as_table().cloned().ok_or_else(|| {
                anyhow::anyhow!("agent config {} must be a TOML table", path.display())
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read agent config {}", path.display()))
        }
    }
}

fn read_global_connection_profile_setting() -> Result<Option<String>> {
    let path = global_agent_config_path()?;
    match read_global_agent_config_table()?.get("connection_profile") {
        Some(toml::Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
        Some(toml::Value::String(_)) | None => Ok(None),
        Some(_) => anyhow::bail!("connection_profile in {} must be a string", path.display()),
    }
}

fn write_global_connection_profile_setting(profile_id: Option<&str>) -> Result<()> {
    let path = global_agent_config_path()?;
    let mut table = read_global_agent_config_table()?;
    match profile_id {
        Some(profile_id) => {
            table.insert(
                "connection_profile".to_string(),
                toml::Value::String(profile_id.to_string()),
            );
        }
        None => {
            table.remove("connection_profile");
        }
    }
    if table.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to remove empty agent config {}", path.display())
                });
            }
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create agent config directory {}",
                parent.display()
            )
        })?;
    }
    let rendered = toml::to_string_pretty(&table)
        .context("failed to encode agent configuration while updating connection_profile")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("failed to open agent config {}", path.display()))?;
        file.write_all(rendered.as_bytes())
            .with_context(|| format!("failed to write agent config {}", path.display()))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, rendered)
        .with_context(|| format!("failed to write agent config {}", path.display()))?;
    Ok(())
}

fn workspace_pin_key(workspace_root: &Path) -> String {
    workspace_root.to_string_lossy().into_owned()
}

fn current_selection(workspace_dir: Option<&Path>) -> Result<ConnectionCurrentState> {
    let (home_paths, connections) = load_connections()?;
    let workspace_dir = workspace_dir
        .map(validated_workspace_root_path)
        .transpose()?;
    let global_pin =
        read_global_connection_profile_setting()?.map(|profile_id| ConnectionPinState {
            scope: ConnectionPinScope::Global,
            config_path: home_paths.global_agent_config_path.clone(),
            profile_id,
        });
    let workspace_pin = workspace_dir.as_ref().and_then(|workspace_root| {
        connections
            .workspace_pins
            .get(&workspace_pin_key(workspace_root))
            .cloned()
            .map(|profile_id| ConnectionPinState {
                scope: ConnectionPinScope::Workspace,
                config_path: home_paths.global_connections_path.clone(),
                profile_id,
            })
    });
    let (effective_profile, effective_source) =
        if let Some(pin) = workspace_pin.as_ref().or(global_pin.as_ref()) {
            let source = if pin.scope == ConnectionPinScope::Workspace {
                ConnectionSelectionSource::WorkspacePin
            } else {
                ConnectionSelectionSource::GlobalPin
            };
            (Some(pin.profile_id.clone()), source)
        } else if let Some(default_profile) = connections.default_profile.clone() {
            (
                Some(default_profile),
                ConnectionSelectionSource::DefaultProfile,
            )
        } else {
            (None, ConnectionSelectionSource::None)
        };
    Ok(ConnectionCurrentState {
        workspace_dir,
        global_pin,
        workspace_pin,
        default_profile: connections.default_profile,
        effective_profile,
        effective_source,
    })
}

fn connection_profile<'a>(
    connections: &'a ConnectionsFile,
    profile_id: &str,
) -> Result<&'a ConnectionProfile> {
    connections
        .profiles
        .get(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))
}

fn parse_provider_id(raw: &str) -> Result<LlmProvider> {
    match raw.trim() {
        "chatgpt" => Ok(LlmProvider::Chatgpt),
        "google_gemini_generate_content" => Ok(LlmProvider::GoogleGeminiGenerateContent),
        "openai_responses" => Ok(LlmProvider::OpenAiResponses),
        "openai_chat_completions" => Ok(LlmProvider::OpenAiChatCompletions),
        "openai_chat_completions_compatible" => Ok(LlmProvider::OpenAiChatCompletionsCompatible),
        "openrouter" => Ok(LlmProvider::OpenRouter),
        "anthropic_messages" => Ok(LlmProvider::AnthropicMessages),
        other => anyhow::bail!("unknown provider `{other}`"),
    }
}

fn parse_setting_pairs(pairs: &[String]) -> Result<BTreeMap<String, String>> {
    let mut settings = BTreeMap::new();
    for pair in pairs {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid setting `{pair}`; expected key=value"))?;
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            anyhow::bail!("invalid setting `{pair}`; key cannot be empty");
        }
        settings.insert(key.to_string(), value.to_string());
    }
    Ok(settings)
}

fn print_profile(profile_id: &str, profile: &ConnectionProfile, is_default: bool) {
    println!("profile_id: {}", display_identifier(profile_id));
    println!(
        "label: {}",
        if profile.label.is_some() {
            "<set>"
        } else {
            "<unset>"
        }
    );
    println!("provider: {}", profile.provider.as_str());
    println!(
        "credential: {}",
        if profile.credential_id.is_some() {
            "<configured>"
        } else {
            "<unset>"
        }
    );
    println!("default: {is_default}");
    println!("source: configured");
    if profile.settings.is_empty() {
        println!("settings: <none>");
    } else {
        let keys = profile
            .settings
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        println!("settings_keys: {keys}");
    }
}

fn print_pin_state(label: &str, pin: Option<&ConnectionPinState>) {
    match pin {
        Some(pin) => println!(
            "{label}: {} ({}) [{}]",
            display_identifier(&pin.profile_id),
            pin.scope.as_str(),
            pin.config_path.display()
        ),
        None => println!("{label}: <unset>"),
    }
}

fn print_current_state(current: &ConnectionCurrentState) {
    if let Some(workspace_dir) = current.workspace_dir.as_deref() {
        println!("workspace_dir: {}", workspace_dir.display());
    }
    print_pin_state("global_pin", current.global_pin.as_ref());
    print_pin_state("workspace_pin", current.workspace_pin.as_ref());
    match current.default_profile.as_deref() {
        Some(profile_id) => println!("default_profile: {}", display_identifier(profile_id)),
        None => println!("default_profile: <unset>"),
    }
    match current.effective_profile.as_deref() {
        Some(profile_id) => println!("effective_profile: {}", display_identifier(profile_id)),
        None => println!("effective_profile: <unset>"),
    }
    println!("effective_source: {}", current.effective_source.as_str());
}

fn detect_workspace_dir_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    detect_workspace_dir(&cwd)
}

fn detect_workspace_dir(path: &Path) -> Option<PathBuf> {
    let normalized = std::fs::canonicalize(path)
        .ok()
        .unwrap_or_else(|| path.to_path_buf());
    if normalized
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false)
        && normalized.is_dir()
    {
        return normalized.parent().map(Path::to_path_buf);
    }

    let alan_dir = normalized.join(".alan");
    if alan_dir.is_dir() {
        return Some(normalized);
    }

    None
}

fn prompt_secret_line(profile_id: &str) -> Result<String> {
    print!("Secret for {}: ", display_identifier(profile_id));
    io::stdout().flush()?;
    let mut secret = String::new();
    io::stdin().read_line(&mut secret)?;
    let trimmed = secret.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("secret cannot be empty");
    }
    Ok(trimmed)
}

fn suggested_profile_id(provider: LlmProvider, requested: Option<String>) -> Result<String> {
    if let Some(profile_id) = requested {
        return sanitize_identifier(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("invalid profile id `{profile_id}`"));
    }

    let default = match provider {
        LlmProvider::Chatgpt => "chatgpt-main",
        LlmProvider::OpenAiResponses => "openai-main",
        LlmProvider::OpenAiChatCompletions => "openai-chat",
        LlmProvider::OpenAiChatCompletionsCompatible => "compatible-main",
        LlmProvider::OpenRouter => "openrouter-main",
        LlmProvider::GoogleGeminiGenerateContent => "gemini",
        LlmProvider::AnthropicMessages => "anthropic-main",
    };
    Ok(default.to_string())
}

async fn profile_or_current(explicit_profile_id: Option<String>) -> Result<String> {
    if let Some(profile_id) = explicit_profile_id {
        return Ok(profile_id);
    }
    let workspace_dir = detect_workspace_dir_from_cwd();
    let current = current_selection(workspace_dir.as_deref())?;
    if let Some(profile_id) = current.effective_profile {
        return Ok(profile_id);
    }
    anyhow::bail!(
        "no profile selected; pass <profile-id>, set a default with `alan connection default set <profile-id>`, or pin one with `alan connection pin <profile-id>`"
    )
}

pub async fn run_connection_list() -> Result<()> {
    let (_, connections) = load_connections()?;
    if let Some(default_profile) = connections.default_profile.as_deref() {
        println!("default_profile: {}", display_identifier(default_profile));
    }
    if connections.profiles.is_empty() {
        println!("No connection profiles configured.");
        return Ok(());
    }
    for (profile_id, profile) in &connections.profiles {
        println!(
            "{} | provider={} | credential={}{}",
            display_identifier(profile_id),
            profile.provider.as_str(),
            if profile.credential_id.is_some() {
                "configured"
            } else {
                "unset"
            },
            if connections.default_profile.as_deref() == Some(profile_id.as_str()) {
                " | default"
            } else {
                ""
            }
        );
    }
    Ok(())
}

pub async fn run_connection_show(profile_id: &str) -> Result<()> {
    let (_, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    print_profile(
        profile_id,
        profile,
        connections.default_profile.as_deref() == Some(profile_id),
    );
    Ok(())
}

pub async fn run_connection_add(
    provider_raw: &str,
    profile_id: Option<String>,
    label: Option<String>,
    credential_id: Option<String>,
    setting_pairs: &[String],
    activate: bool,
) -> Result<()> {
    let provider = parse_provider_id(provider_raw)?;
    let profile_id = suggested_profile_id(provider, profile_id)?;
    let settings = parse_setting_pairs(setting_pairs)?;
    let (_, mut connections) = load_connections()?;
    if connections.profiles.contains_key(&profile_id) {
        anyhow::bail!("Connection profile `{profile_id}` already exists");
    }
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    let credential_id = if descriptor.credential_kind == CredentialKind::AmbientCloudAuth {
        None
    } else {
        let chosen = credential_id.unwrap_or_else(|| profile_id.clone());
        Some(
            sanitize_identifier(&chosen)
                .ok_or_else(|| anyhow::anyhow!("Invalid credential id `{chosen}`"))?,
        )
    };
    if let Some(credential_id) = credential_id.as_ref() {
        let credential_label = label
            .clone()
            .unwrap_or_else(|| descriptor.display_name.to_string());
        connections
            .credentials
            .entry(credential_id.clone())
            .or_insert_with(|| ConnectionCredential {
                kind: descriptor.credential_kind,
                provider_family: provider,
                label: format!("{credential_label} credential"),
                backend: default_credential_backend(descriptor.credential_kind).to_string(),
            });
    }
    let settings = normalize_profile_settings(provider, &settings);
    validate_profile_settings(provider, &settings)?;
    let now = Utc::now();
    connections.profiles.insert(
        profile_id.clone(),
        ConnectionProfile {
            provider,
            label,
            credential_id,
            created_at: now,
            updated_at: now,
            source: "managed".to_string(),
            settings,
        },
    );
    if activate || connections.default_profile.is_none() {
        connections.default_profile = Some(profile_id.clone());
    }
    save_connections(&connections)?;
    println!(
        "Created connection profile {}",
        display_identifier(&profile_id)
    );
    Ok(())
}

pub async fn run_connection_edit(
    profile_id: &str,
    label: Option<String>,
    credential_id: Option<String>,
    setting_pairs: &[String],
) -> Result<()> {
    let profile_id = validated_profile_id(profile_id)?;
    let settings = if setting_pairs.is_empty() {
        None
    } else {
        Some(parse_setting_pairs(setting_pairs)?)
    };
    let (_, mut connections) = load_connections()?;
    let profile = connections
        .profiles
        .get_mut(&profile_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
    if let Some(label) = label {
        profile.label = Some(label);
    }
    if let Some(credential_id) = credential_id {
        let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
        let credential_id = sanitize_identifier(&credential_id)
            .ok_or_else(|| anyhow::anyhow!("Invalid credential id `{credential_id}`"))?;
        let credential_label = profile
            .label
            .clone()
            .unwrap_or_else(|| descriptor.display_name.to_string());
        connections
            .credentials
            .entry(credential_id.clone())
            .or_insert_with(|| ConnectionCredential {
                kind: descriptor.credential_kind,
                provider_family: profile.provider,
                label: format!("{credential_label} credential"),
                backend: default_credential_backend(descriptor.credential_kind).to_string(),
            });
        profile.credential_id = Some(credential_id);
    }
    if let Some(settings) = settings {
        let settings = normalize_profile_settings(profile.provider, &settings);
        validate_profile_settings(profile.provider, &settings)?;
        profile.settings = settings;
    }
    profile.updated_at = Utc::now();
    save_connections(&connections)?;
    println!(
        "Updated connection profile {}",
        display_identifier(&profile_id)
    );
    Ok(())
}

pub async fn run_connection_set_secret(profile_id: &str, value: Option<String>) -> Result<()> {
    let secret = match value {
        Some(value) => value,
        None => prompt_secret_line(profile_id)?,
    };
    let (_, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    let descriptor = ConnectionsFile::profile_descriptor(profile.provider);
    if descriptor.credential_kind != CredentialKind::SecretString {
        anyhow::bail!(
            "Provider `{}` does not support secret entry",
            profile.provider.as_str()
        );
    }
    let credential_id = profile
        .credential_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Profile `{profile_id}` has no credential"))?;
    SecretStore::detect()?.save(credential_id, &secret)?;
    println!("Stored secret for {}.", display_identifier(profile_id));
    Ok(())
}

pub async fn run_connection_login(
    profile_id: &str,
    use_device_code: bool,
    open_browser: bool,
) -> Result<()> {
    let (home_paths, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    if profile.provider != LlmProvider::Chatgpt {
        anyhow::bail!(
            "profile `{profile_id}` uses `{}`; managed login is supported only for chatgpt",
            profile.provider.as_str()
        );
    }

    if use_device_code {
        let manager = chatgpt_auth_manager(&home_paths)?;
        let prompt = manager.start_device_code().await?;
        println!(
            "Open this URL in your browser:\n{}",
            prompt.verification_url
        );
        println!();
        println!("Enter this one-time code:\n{}", prompt.user_code);
        println!();
        let login = manager
            .complete_device_code(&prompt, DeviceCodeLoginOptions::default())
            .await?;
        println!("Logged in to {}.", display_identifier(profile_id));
        let _ = login;
        return Ok(());
    }

    let manager = chatgpt_auth_manager(&home_paths)?;
    let login = manager
        .login_with_browser(BrowserLoginOptions {
            open_browser,
            forced_workspace_id: None,
            timeout: Duration::from_secs(300),
            redirect_uri: None,
            login_id: None,
        })
        .await?;
    println!("Logged in to {}.", display_identifier(profile_id));
    let _ = login;
    Ok(())
}

pub async fn run_connection_logout(profile_id: &str) -> Result<()> {
    let (home_paths, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    match profile.provider {
        LlmProvider::Chatgpt => {
            let removed = chatgpt_auth_manager(&home_paths)?.logout().await?;
            println!(
                "{}",
                if removed {
                    format!(
                        "Removed managed credentials for {}.",
                        display_identifier(profile_id)
                    )
                } else {
                    format!(
                        "No managed credentials were present for {}.",
                        display_identifier(profile_id)
                    )
                }
            );
        }
        _ => {
            if ConnectionsFile::profile_descriptor(profile.provider).credential_kind
                != CredentialKind::SecretString
            {
                anyhow::bail!(
                    "provider `{}` does not support logout",
                    profile.provider.as_str()
                );
            }
            let store = alan_agent_engine::SecretStore::detect()?;
            let credential_id = profile
                .credential_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("profile `{profile_id}` has no credential"))?;
            let removed = store.delete(credential_id)?;
            println!(
                "{}",
                if removed {
                    format!("Removed secret for {}.", display_identifier(profile_id))
                } else {
                    format!(
                        "No secret was present for {}.",
                        display_identifier(profile_id)
                    )
                }
            );
        }
    }
    Ok(())
}

pub async fn run_connection_current(workspace: Option<PathBuf>) -> Result<()> {
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let current = current_selection(workspace_dir.as_deref().or(fallback_workspace.as_deref()))?;
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_default_set(
    profile_id: &str,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let profile_id = validated_profile_id(profile_id)?;
    let (_, mut connections) = load_connections()?;
    if !connections.profiles.contains_key(&profile_id) {
        anyhow::bail!("Unknown connection profile `{profile_id}`");
    }
    connections.default_profile = Some(profile_id.clone());
    save_connections(&connections)?;
    let current = current_selection(workspace_dir.as_deref().or(fallback_workspace.as_deref()))?;
    println!(
        "Default profile set to {}.",
        display_identifier(&profile_id)
    );
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_default_clear(workspace: Option<PathBuf>) -> Result<()> {
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let (_, mut connections) = load_connections()?;
    connections.default_profile = None;
    save_connections(&connections)?;
    let current = current_selection(workspace_dir.as_deref().or(fallback_workspace.as_deref()))?;
    println!("Cleared default profile.");
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_pin(
    profile_id: &str,
    scope: ConnectionPinScope,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() && scope == ConnectionPinScope::Workspace {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let effective_workspace = workspace_dir.as_deref().or(fallback_workspace.as_deref());
    let profile_id = validated_profile_id(profile_id)?;
    let (_, mut connections) = load_connections()?;
    if !connections.profiles.contains_key(&profile_id) {
        anyhow::bail!("Unknown connection profile `{profile_id}`");
    }
    match scope {
        ConnectionPinScope::Global => write_global_connection_profile_setting(Some(&profile_id))?,
        ConnectionPinScope::Workspace => {
            let workspace_root = effective_workspace
                .ok_or_else(|| anyhow::anyhow!("workspace is required for workspace scope"))
                .and_then(validated_workspace_root_path)?;
            connections
                .workspace_pins
                .insert(workspace_pin_key(&workspace_root), profile_id.clone());
            save_connections(&connections)?;
        }
    }
    let current = current_selection(effective_workspace)?;
    println!(
        "Pinned profile {} at {} scope.",
        display_identifier(&profile_id),
        scope.as_str()
    );
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_unpin(
    scope: ConnectionPinScope,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() && scope == ConnectionPinScope::Workspace {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let effective_workspace = workspace_dir.as_deref().or(fallback_workspace.as_deref());
    match scope {
        ConnectionPinScope::Global => write_global_connection_profile_setting(None)?,
        ConnectionPinScope::Workspace => {
            let workspace_root = effective_workspace
                .ok_or_else(|| anyhow::anyhow!("workspace is required for workspace scope"))
                .and_then(validated_workspace_root_path)?;
            let (_, mut connections) = load_connections()?;
            connections
                .workspace_pins
                .remove(&workspace_pin_key(&workspace_root));
            save_connections(&connections)?;
        }
    }
    let current = current_selection(effective_workspace)?;
    println!("Cleared {} pin.", scope.as_str());
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_test(profile_id: Option<String>) -> Result<()> {
    let profile_id = profile_or_current(profile_id).await?;
    let (home_paths, connections) = load_connections()?;
    let profile = connection_profile(&connections, &profile_id)?;
    let mut config = Config::default();
    let resolved = connections.apply_profile_to_config(
        Some(&profile_id),
        &SecretStore::detect()?,
        &mut config,
    )?;
    if resolved.provider == LlmProvider::Chatgpt
        && chatgpt_auth_manager(&home_paths)?.status().await?.is_none()
    {
        anyhow::bail!("ChatGPT profile `{profile_id}` is not logged in");
    }
    let _provider_config = config.to_provider_config()?;
    println!("profile_id: {}", display_identifier(&profile_id));
    println!("provider: {}", profile.provider.as_str());
    println!("status: success");
    Ok(())
}

pub async fn run_connection_remove(profile_id: &str) -> Result<()> {
    let profile_id = validated_profile_id(profile_id)?;
    let (_, mut connections) = load_connections()?;
    let removed = connections.profiles.remove(&profile_id).is_some();
    if connections.default_profile.as_deref() == Some(profile_id.as_str()) {
        connections.default_profile = None;
    }
    if removed {
        save_connections(&connections)?;
    }
    if removed {
        println!(
            "Removed connection profile {}.",
            display_identifier(&profile_id)
        );
    } else {
        println!(
            "Connection profile {} was not present.",
            display_identifier(&profile_id)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_provider_id_accepts_openrouter() {
        assert_eq!(
            parse_provider_id("openrouter").unwrap(),
            LlmProvider::OpenRouter
        );
    }

    #[test]
    fn suggested_profile_id_defaults_openrouter() {
        assert_eq!(
            suggested_profile_id(LlmProvider::OpenRouter, None).unwrap(),
            "openrouter-main"
        );
    }
}
