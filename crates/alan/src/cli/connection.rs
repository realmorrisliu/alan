use alan_agent_engine::{Config, InstallChannel, LlmProvider};
use alan_auth::{
    BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthManager, DeviceCodeLoginOptions,
};
use alan_os_host::{
    HostStorePaths, LegacyConnectionPaths, SecretStore, SystemStorePaths, apply_profile_to_config,
    migrate_legacy_connections,
};
use alan_service_manager::{
    ConnectionCredential, ConnectionProfile, ConnectionsFile, CredentialKind,
    default_credential_backend, normalize_profile_settings, sanitize_identifier,
    validate_profile_settings,
};
use alan_shell::Shell;
use anyhow::{Context, Result};
use chrono::Utc;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

/// Existing Connection Service request selected by a native renderer adapter.
/// The value is an opaque request id, never credential material.
pub const NATIVE_CONNECTION_REQUEST_ENV: &str = "ALAN_NATIVE_CONNECTION_REQUEST_ID";

struct ConnectionStores {
    credentials_dir: PathBuf,
    managed_auth: PathBuf,
    shell: Shell,
}

async fn connection_stores() -> Result<ConnectionStores> {
    let channel = InstallChannel::detect_current();
    let system = SystemStorePaths::detect(channel.descriptor().id)?;
    let host = HostStorePaths::detect(channel.descriptor().id)?;
    if let Some(legacy) = LegacyConnectionPaths::detect(channel)? {
        migrate_legacy_connections(&legacy, &system, &host)?;
    }
    Ok(ConnectionStores {
        credentials_dir: host.credentials,
        managed_auth: host.managed_auth,
        shell: Shell::new(super::host::attach_or_start_host(channel).await?.root),
    })
}

async fn load_connections() -> Result<(ConnectionStores, ConnectionsFile)> {
    let stores = connection_stores().await?;
    let bytes = stores
        .shell
        .cat("/mnt/connections/metadata")
        .await
        .map_err(|error| anyhow::anyhow!("read Connection Service metadata: {error}"))?;
    let connections =
        serde_json::from_slice(&bytes).context("decode Connection Service metadata")?;
    Ok((stores, connections))
}

async fn save_connections(stores: &ConnectionStores, connections: &ConnectionsFile) -> Result<()> {
    let command = serde_json::json!({
        "op": "replace_metadata",
        "connections": connections,
    });
    stores
        .shell
        .write(
            "/mnt/connections/ctl",
            &serde_json::to_vec(&command).context("encode Connection Service command")?,
        )
        .await
        .map_err(|error| anyhow::anyhow!("update Connection Service metadata: {error}"))
}

async fn request_native(
    stores: &ConnectionStores,
    profile_id: &str,
    action: &str,
) -> Result<String> {
    if let Some(request_id) = std::env::var_os(NATIVE_CONNECTION_REQUEST_ENV) {
        let request_id = request_id
            .into_string()
            .map_err(|_| anyhow::anyhow!("native Connection request id is not UTF-8"))?;
        let requests: serde_json::Value = serde_json::from_slice(
            &stores
                .shell
                .cat("/mnt/connections/native-requests")
                .await
                .map_err(|error| anyhow::anyhow!("read native Connection requests: {error}"))?,
        )?;
        let request = requests
            .get(&request_id)
            .context("native Connection request is no longer pending")?;
        anyhow::ensure!(
            request
                .get("profile_id")
                .and_then(serde_json::Value::as_str)
                == Some(profile_id)
                && request.get("action").and_then(serde_json::Value::as_str) == Some(action),
            "native Connection request does not match the requested profile/action"
        );
        return Ok(request_id);
    }
    let request_id = format!("cli-{}", uuid::Uuid::new_v4().simple());
    let command = serde_json::json!({
        "op": "request_native",
        "request": {
            "id": request_id,
            "profile_id": profile_id,
            "action": action,
        }
    });
    stores
        .shell
        .write(
            "/mnt/connections/ctl",
            &serde_json::to_vec(&command).context("encode native Connection request")?,
        )
        .await
        .map_err(|error| anyhow::anyhow!("create native Connection request: {error}"))?;
    Ok(request_id)
}

async fn respond_native(
    stores: &ConnectionStores,
    request_id: &str,
    opaque_credential_ref: Option<String>,
    status: &str,
) -> Result<()> {
    stores
        .shell
        .write(
            "/mnt/connections/native-responses",
            &serde_json::to_vec(&serde_json::json!({
                "request_id": request_id,
                "opaque_credential_ref": opaque_credential_ref,
                "status": status,
            }))
            .context("encode native Connection response")?,
        )
        .await
        .map_err(|error| anyhow::anyhow!("answer native Connection request: {error}"))
}

fn secret_store(stores: &ConnectionStores) -> Result<SecretStore> {
    SecretStore::from_directory(&stores.credentials_dir)
}

fn chatgpt_auth_manager(stores: &ConnectionStores) -> Result<ChatgptAuthManager> {
    Ok(ChatgptAuthManager::new(
        ChatgptAuthConfig::with_storage_path(stores.managed_auth.clone()),
    )?)
}

fn display_identifier(value: &str) -> String {
    sanitize_identifier(value).unwrap_or_else(|| "<redacted>".to_string())
}

fn validated_profile_id(profile_id: &str) -> Result<String> {
    sanitize_identifier(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid profile id `{profile_id}`"))
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

fn ensure_credential_metadata(
    connections: &mut ConnectionsFile,
    credential_id: &str,
    provider: LlmProvider,
    profile_label: Option<&str>,
) -> Result<()> {
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    anyhow::ensure!(
        descriptor.credential_kind != CredentialKind::AmbientCloudAuth,
        "Provider `{}` does not use an explicit credential reference",
        provider.as_str()
    );
    if let Some(existing) = connections.credentials.get(credential_id) {
        anyhow::ensure!(
            existing.provider_family == provider,
            "Credential `{credential_id}` is already bound to provider `{}`",
            existing.provider_family.as_str()
        );
        anyhow::ensure!(
            existing.kind == descriptor.credential_kind,
            "Credential `{credential_id}` uses kind `{}` but provider `{}` requires `{}`",
            existing.kind.as_str(),
            provider.as_str(),
            descriptor.credential_kind.as_str()
        );
        return Ok(());
    }
    let label = profile_label
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(descriptor.display_name);
    connections.credentials.insert(
        credential_id.to_string(),
        ConnectionCredential {
            kind: descriptor.credential_kind,
            provider_family: provider,
            label: format!("{label} credential"),
            backend: default_credential_backend(descriptor.credential_kind).to_string(),
        },
    );
    Ok(())
}

fn parse_provider_id(raw: &str) -> Result<LlmProvider> {
    let provider = match raw.trim().to_ascii_lowercase().as_str() {
        "chatgpt" => LlmProvider::Chatgpt,
        "openai_responses" | "openai" => LlmProvider::OpenAiResponses,
        "openai_chat_completions" => LlmProvider::OpenAiChatCompletions,
        "openai_chat_completions_compatible" | "compatible" => {
            LlmProvider::OpenAiChatCompletionsCompatible
        }
        "openrouter" => LlmProvider::OpenRouter,
        "google_gemini_generate_content" | "gemini" => LlmProvider::GoogleGeminiGenerateContent,
        "anthropic_messages" | "anthropic" => LlmProvider::AnthropicMessages,
        other => anyhow::bail!("Unknown provider `{other}`"),
    };
    Ok(provider)
}

fn parse_setting_pairs(pairs: &[String]) -> Result<BTreeMap<String, String>> {
    pairs
        .iter()
        .map(|pair| {
            let (key, value) = pair
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Setting must use key=value: `{pair}`"))?;
            let key = key.trim();
            anyhow::ensure!(!key.is_empty(), "Setting key must not be empty");
            Ok((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn suggested_profile_id(provider: LlmProvider, requested: Option<String>) -> Result<String> {
    if let Some(requested) = requested {
        return validated_profile_id(&requested);
    }
    Ok(match provider {
        LlmProvider::Chatgpt => "chatgpt-main",
        LlmProvider::OpenAiResponses => "openai-main",
        LlmProvider::OpenAiChatCompletions => "openai-chat-main",
        LlmProvider::OpenAiChatCompletionsCompatible => "compatible-main",
        LlmProvider::OpenRouter => "openrouter-main",
        LlmProvider::GoogleGeminiGenerateContent => "gemini",
        LlmProvider::AnthropicMessages => "anthropic-main",
    }
    .to_string())
}

fn prompt_secret_line(profile_id: &str) -> Result<String> {
    print!("Secret for {}: ", display_identifier(profile_id));
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim().to_string();
    anyhow::ensure!(!value.is_empty(), "Secret must not be empty");
    Ok(value)
}

fn print_profile(profile_id: &str, profile: &ConnectionProfile, is_default: bool) {
    println!("profile_id: {}", display_identifier(profile_id));
    println!("provider: {}", profile.provider.as_str());
    println!(
        "credential: {}",
        if profile.credential_id.is_some() {
            "configured"
        } else {
            "unset"
        }
    );
    println!("default: {is_default}");
    if profile.settings.is_empty() {
        println!("settings: none");
    } else {
        println!(
            "settings_keys: {}",
            profile
                .settings
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

pub async fn run_connection_list() -> Result<()> {
    let (_, connections) = load_connections().await?;
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
            if connections.default_profile.as_deref() == Some(profile_id) {
                " | default"
            } else {
                ""
            }
        );
    }
    Ok(())
}

pub async fn run_connection_show(profile_id: &str) -> Result<()> {
    let (_, connections) = load_connections().await?;
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
    make_default: bool,
) -> Result<()> {
    let provider = parse_provider_id(provider_raw)?;
    let profile_id = suggested_profile_id(provider, profile_id)?;
    let settings = normalize_profile_settings(provider, &parse_setting_pairs(setting_pairs)?);
    validate_profile_settings(provider, &settings)?;
    let (stores, mut connections) = load_connections().await?;
    anyhow::ensure!(
        !connections.profiles.contains_key(&profile_id),
        "Connection profile `{profile_id}` already exists"
    );
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    let credential_id = if descriptor.credential_kind == CredentialKind::AmbientCloudAuth {
        None
    } else {
        let id = credential_id.unwrap_or_else(|| profile_id.clone());
        Some(validated_profile_id(&id)?)
    };
    if let Some(credential_id) = credential_id.as_ref() {
        ensure_credential_metadata(&mut connections, credential_id, provider, label.as_deref())?;
    }
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
    if make_default || connections.default_profile.is_none() {
        connections.default_profile = Some(profile_id.clone());
    }
    save_connections(&stores, &connections).await?;
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
    let credential_id = credential_id
        .map(|credential_id| validated_profile_id(&credential_id))
        .transpose()?;
    let (stores, mut connections) = load_connections().await?;
    let (provider, credential_label) = {
        let profile = connections
            .profiles
            .get(&profile_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
        (
            profile.provider,
            label.clone().or_else(|| profile.label.clone()),
        )
    };
    if let Some(credential_id) = credential_id.as_deref() {
        ensure_credential_metadata(
            &mut connections,
            credential_id,
            provider,
            credential_label.as_deref(),
        )?;
    }
    let profile = connections
        .profiles
        .get_mut(&profile_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))?;
    if let Some(label) = label {
        profile.label = Some(label);
    }
    if let Some(credential_id) = credential_id {
        profile.credential_id = Some(credential_id);
    }
    if !setting_pairs.is_empty() {
        let settings =
            normalize_profile_settings(profile.provider, &parse_setting_pairs(setting_pairs)?);
        validate_profile_settings(profile.provider, &settings)?;
        profile.settings = settings;
    }
    profile.updated_at = Utc::now();
    save_connections(&stores, &connections).await?;
    println!(
        "Updated connection profile {}",
        display_identifier(&profile_id)
    );
    Ok(())
}

pub async fn run_connection_set_secret(profile_id: &str, value: Option<String>) -> Result<()> {
    let secret = value
        .map(Ok)
        .unwrap_or_else(|| prompt_secret_line(profile_id))?;
    let (stores, connections) = load_connections().await?;
    let profile = connection_profile(&connections, profile_id)?;
    anyhow::ensure!(
        ConnectionsFile::profile_descriptor(profile.provider).credential_kind
            == CredentialKind::SecretString,
        "Provider `{}` does not support secret entry",
        profile.provider.as_str()
    );
    let credential_id = profile
        .credential_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Profile `{profile_id}` has no credential"))?;
    let request_id = request_native(&stores, profile_id, "secret_entry").await?;
    secret_store(&stores)?.save(credential_id, &secret)?;
    respond_native(
        &stores,
        &request_id,
        Some(format!("host-credential-store:{credential_id}")),
        "ready",
    )
    .await?;
    println!("Stored secret for {}.", display_identifier(profile_id));
    Ok(())
}

pub async fn run_connection_login(
    profile_id: &str,
    use_device_code: bool,
    open_browser: bool,
) -> Result<()> {
    let (stores, connections) = load_connections().await?;
    let profile = connection_profile(&connections, profile_id)?;
    anyhow::ensure!(
        profile.provider == LlmProvider::Chatgpt,
        "managed login is supported only for chatgpt"
    );
    let request_id = request_native(
        &stores,
        profile_id,
        if use_device_code {
            "device_login"
        } else {
            "browser_login"
        },
    )
    .await?;
    let manager = chatgpt_auth_manager(&stores)?;
    if use_device_code {
        let prompt = manager.start_device_code().await?;
        println!(
            "Open this URL in your browser:\n{}",
            prompt.verification_url
        );
        println!("Enter this one-time code:\n{}", prompt.user_code);
        manager
            .complete_device_code(&prompt, DeviceCodeLoginOptions::default())
            .await?;
    } else {
        manager
            .login_with_browser(BrowserLoginOptions {
                open_browser,
                forced_workspace_id: None,
                timeout: Duration::from_secs(300),
                redirect_uri: None,
                login_id: None,
            })
            .await?;
    }
    respond_native(
        &stores,
        &request_id,
        Some(format!("host-managed-auth:{profile_id}")),
        "ready",
    )
    .await?;
    println!("Logged in to {}.", display_identifier(profile_id));
    Ok(())
}

pub async fn run_connection_logout(profile_id: &str) -> Result<()> {
    let (stores, connections) = load_connections().await?;
    let profile = connection_profile(&connections, profile_id)?;
    let request_id = request_native(&stores, profile_id, "logout").await?;
    let removed = if profile.provider == LlmProvider::Chatgpt {
        chatgpt_auth_manager(&stores)?.logout().await?
    } else {
        let credential_id = profile
            .credential_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Profile `{profile_id}` has no credential"))?;
        secret_store(&stores)?.delete(credential_id)?
    };
    respond_native(&stores, &request_id, None, "logged_out").await?;
    println!(
        "{}",
        if removed {
            format!(
                "Removed credentials for {}.",
                display_identifier(profile_id)
            )
        } else {
            format!(
                "No credentials were present for {}.",
                display_identifier(profile_id)
            )
        }
    );
    Ok(())
}

pub async fn run_connection_current() -> Result<()> {
    let (_, connections) = load_connections().await?;
    println!(
        "effective_profile: {}",
        connections
            .default_profile
            .as_deref()
            .map(display_identifier)
            .unwrap_or_else(|| "none".to_string())
    );
    println!("source: default_profile");
    Ok(())
}

pub async fn run_connection_default_set(profile_id: &str) -> Result<()> {
    let profile_id = validated_profile_id(profile_id)?;
    let (stores, mut connections) = load_connections().await?;
    anyhow::ensure!(
        connections.profiles.contains_key(&profile_id),
        "Unknown connection profile `{profile_id}`"
    );
    connections.default_profile = Some(profile_id.clone());
    save_connections(&stores, &connections).await?;
    println!(
        "Default profile set to {}.",
        display_identifier(&profile_id)
    );
    Ok(())
}

pub async fn run_connection_default_clear() -> Result<()> {
    let (stores, mut connections) = load_connections().await?;
    connections.default_profile = None;
    save_connections(&stores, &connections).await?;
    println!("Cleared default profile.");
    Ok(())
}

pub async fn run_connection_test(profile_id: Option<String>) -> Result<()> {
    let (stores, connections) = load_connections().await?;
    let profile_id = profile_id
        .or_else(|| connections.default_profile.clone())
        .ok_or_else(|| anyhow::anyhow!("no profile selected"))?;
    let profile = connection_profile(&connections, &profile_id)?;
    let mut config = Config::default();
    let resolved = apply_profile_to_config(
        &connections,
        Some(&profile_id),
        &secret_store(&stores)?,
        &mut config,
    )?;
    if resolved.provider == LlmProvider::Chatgpt
        && chatgpt_auth_manager(&stores)?.status().await?.is_none()
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
    let (stores, mut connections) = load_connections().await?;
    let removed = connections.profiles.remove(&profile_id).is_some();
    if connections.default_profile.as_deref() == Some(&profile_id) {
        connections.default_profile = None;
    }
    if removed {
        save_connections(&stores, &connections).await?;
    }
    println!(
        "Connection profile {} {}.",
        display_identifier(&profile_id),
        if removed {
            "removed"
        } else {
            "was not present"
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_and_default_profile_ids_are_normalized() {
        assert_eq!(
            parse_provider_id("openrouter").unwrap(),
            LlmProvider::OpenRouter
        );
        assert_eq!(
            suggested_profile_id(LlmProvider::OpenRouter, None).unwrap(),
            "openrouter-main"
        );
    }

    #[test]
    fn connection_metadata_uses_the_service_file_surface() {
        let source = include_str!("connection.rs");
        assert!(source.contains("/mnt/connections/metadata"));
        assert!(source.contains("/mnt/connections/ctl"));
        assert!(source.contains("request_native"));
        assert!(source.contains("/mnt/connections/native-responses"));
        assert!(source.contains("host-credential-store:"));
        assert!(source.contains("host-managed-auth:"));
        assert!(!source.contains(&["ConnectionsFile::load", "_from_path"].concat()));
        assert!(!source.contains(&["connections.save", "_to_path"].concat()));
    }
}
