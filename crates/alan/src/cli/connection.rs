use alan_agent_engine::{Config, LlmProvider};
use alan_auth::{
    BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthManager, DeviceCodeLoginOptions,
};

mod test_connectivity;
use alan_os_host::{HostStorePaths, SecretStore, SystemStorePaths, apply_profile_to_config};
use alan_service_manager::{
    ConnectionCredential, ConnectionProfile, ConnectionService, ConnectionsFile, CredentialKind,
    PackageService, default_credential_backend, first_party_provider_catalog,
    normalize_profile_settings, provider_packages, sanitize_identifier, validate_profile_settings,
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
use test_connectivity::verify_connection_connectivity;

/// Existing Connection Service request selected by a native renderer adapter.
/// The value is an opaque request id, never credential material.
pub const NATIVE_CONNECTION_REQUEST_ENV: &str = "ALAN_NATIVE_CONNECTION_REQUEST_ID";

pub(super) struct ConnectionStores {
    _installation_access: alan_os_host::installation::InstallationAccess,
    pub(super) credentials_dir: PathBuf,
    pub(super) managed_auth: PathBuf,
    shell: Shell,
    pub(super) expected_metadata: String,
}

pub(super) async fn load_connections() -> Result<(ConnectionStores, ConnectionsFile)> {
    alan_os_host::installation::validate_current_invocation()?;
    let installation_access = alan_os_host::installation::InstallationPaths::detect()?.access()?;

    let system = SystemStorePaths::detect()?;
    let host = HostStorePaths::detect()?;
    let shell = if std::env::var_os(NATIVE_CONNECTION_REQUEST_ENV).is_some() {
        let paths = super::host::explicit_instance_paths()?;
        Shell::new(
            alan_os_host::LocalAttachment::new(paths)
                .connect()
                .await?
                .root,
        )
    } else {
        let service = ConnectionService::open(&system.connection_bindings()?)?;
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/mnt/connections",
            alan_ap::InProcessTransport::new(service.file_server()),
            alan_kernel::Access::ReadWrite,
        );
        Shell::new(alan_ap::InProcessTransport::new(std::sync::Arc::new(
            alan_kernel::MountFs::new(namespace),
        )))
    };
    let bytes = shell
        .cat("/mnt/connections/metadata")
        .await
        .map_err(|error| anyhow::anyhow!("read Connection Service metadata: {error}"))?;
    let connections: ConnectionsFile =
        serde_json::from_slice(&bytes).context("decode Connection Service metadata")?;
    let stores = ConnectionStores {
        _installation_access: installation_access,
        credentials_dir: host.credentials,
        managed_auth: host.managed_auth,
        shell,
        expected_metadata: connections.fingerprint()?,
    };
    Ok((stores, connections))
}

pub(super) async fn save_connections(
    stores: &mut ConnectionStores,
    connections: &ConnectionsFile,
) -> Result<()> {
    let command = serde_json::json!({
        "op": "replace_metadata",
        "expected": &stores.expected_metadata,
        "connections": connections,
    });
    stores
        .shell
        .write(
            "/mnt/connections/ctl",
            &serde_json::to_vec(&command).context("encode Connection Service command")?,
        )
        .await
        .map_err(|error| anyhow::anyhow!("update Connection Service metadata: {error}"))?;
    stores.expected_metadata = connections.fingerprint()?;
    Ok(())
}

pub(super) async fn request_native(
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

pub(super) async fn respond_native(
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

pub(super) fn secret_store(stores: &ConnectionStores) -> Result<SecretStore> {
    SecretStore::from_directory(&stores.credentials_dir)
}

pub(super) fn chatgpt_auth_manager(stores: &ConnectionStores) -> Result<ChatgptAuthManager> {
    Ok(ChatgptAuthManager::new(
        ChatgptAuthConfig::with_storage_path(stores.managed_auth.clone()),
    )?)
}

pub(super) fn require_non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
}

pub(super) fn display_identifier(value: &str) -> String {
    sanitize_identifier(value).unwrap_or_else(|| "<redacted>".to_string())
}

pub(super) fn validated_profile_id(profile_id: &str) -> Result<String> {
    sanitize_identifier(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Invalid profile id `{profile_id}`"))
}

pub(super) fn connection_profile<'a>(
    connections: &'a ConnectionsFile,
    profile_id: &str,
) -> Result<&'a ConnectionProfile> {
    connections
        .profiles
        .get(profile_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown connection profile `{profile_id}`"))
}

pub(super) fn ensure_credential_metadata(
    connections: &mut ConnectionsFile,
    credential_id: &str,
    provider: LlmProvider,
    profile_label: Option<&str>,
) -> Result<()> {
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    anyhow::ensure!(
        descriptor.credential_kind != CredentialKind::AmbientCloudAuth
            || descriptor.supports_secret_entry,
        "Provider `{}` does not use an explicit credential reference",
        provider.as_str()
    );
    let expected_kind = if descriptor.credential_kind == CredentialKind::AmbientCloudAuth
        && descriptor.supports_secret_entry
    {
        CredentialKind::SecretString
    } else {
        descriptor.credential_kind
    };
    if let Some(existing) = connections.credentials.get(credential_id) {
        anyhow::ensure!(
            existing.provider_family == provider,
            "Credential `{credential_id}` is already bound to provider `{}`",
            existing.provider_family.as_str()
        );
        anyhow::ensure!(
            existing.kind == expected_kind,
            "Credential `{credential_id}` uses kind `{}` but provider `{}` requires `{}`",
            existing.kind.as_str(),
            provider.as_str(),
            expected_kind.as_str()
        );
        return Ok(());
    }
    let label = profile_label
        .filter(|label| !label.trim().is_empty())
        .unwrap_or(descriptor.display_name);
    connections.credentials.insert(
        credential_id.to_string(),
        ConnectionCredential {
            kind: expected_kind,
            provider_family: provider,
            label: format!("{label} credential"),
            backend: default_credential_backend(expected_kind).to_string(),
        },
    );
    Ok(())
}

#[cfg(test)]
fn parse_provider_id(raw: &str) -> Result<LlmProvider> {
    let provider = match raw.trim().to_ascii_lowercase().as_str() {
        "magpie" => LlmProvider::Magpie,
        "chatgpt" => LlmProvider::Chatgpt,
        "openai_responses" | "openai" => LlmProvider::OpenAiResponses,
        "openai_chat_completions" => LlmProvider::OpenAiChatCompletions,
        "openai_chat_completions_compatible" | "compatible" => {
            LlmProvider::OpenAiChatCompletionsCompatible
        }
        "openrouter" => LlmProvider::OpenRouter,
        "google_gemini_generate_content" | "gemini" => LlmProvider::GoogleGeminiGenerateContent,
        "anthropic_messages" | "anthropic" => LlmProvider::AnthropicMessages,
        "typesafe" => LlmProvider::TypesafeEvaluation,
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

pub(super) fn suggested_profile_id(
    provider: LlmProvider,
    requested: Option<String>,
) -> Result<String> {
    if let Some(requested) = requested {
        return validated_profile_id(&requested);
    }
    Ok(match provider {
        LlmProvider::Magpie => "magpie-main",
        LlmProvider::Chatgpt => "chatgpt-main",
        LlmProvider::OpenAiResponses => "openai-main",
        LlmProvider::OpenAiChatCompletions => "openai-chat-main",
        LlmProvider::OpenAiChatCompletionsCompatible => "compatible-main",
        LlmProvider::OpenRouter => "openrouter-main",
        LlmProvider::GoogleGeminiGenerateContent => "vertex-main",
        LlmProvider::AnthropicMessages => "anthropic-main",
        LlmProvider::TypesafeEvaluation => "typesafe-main",
    }
    .to_string())
}

pub(super) fn prompt_secret_line(profile_id: &str) -> Result<String> {
    print!("Secret for {}: ", display_identifier(profile_id));
    io::stdout().flush()?;
    let echo_guard = alan_tui::terminal::StdinEchoGuard::disable_if_tty()
        .map_err(|err| anyhow::anyhow!("disable terminal echo for secret entry: {err}"))?;
    let mut value = String::new();
    let read_res = io::stdin().read_line(&mut value);
    let was_tty = echo_guard.is_some();
    drop(echo_guard);
    if was_tty {
        println!();
    }
    read_res?;
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
    } else {
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
    }
    let builtins = first_party_provider_catalog()
        .iter()
        .map(|d| d.provider_id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!("first_party_providers: {builtins}");
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
    let packages = SystemStorePaths::detect()
        .ok()
        .and_then(|system| system.packages().ok())
        .and_then(|root| PackageService::open(root).ok());
    let (provider, installed_pkg) =
        provider_packages::resolve_provider_selection(provider_raw, packages.as_deref())?;
    let profile_id = suggested_profile_id(provider, profile_id)?;
    let parsed_overrides = parse_setting_pairs(setting_pairs)?;
    let settings = provider_packages::normalize_settings_with_provider_package(
        provider,
        installed_pkg.as_ref().map(|(_, export)| export),
        &parsed_overrides,
    )?;
    let source = installed_pkg
        .as_ref()
        .map(|(pkg_id, _)| provider_packages::provider_package_source(pkg_id))
        .unwrap_or_else(|| "managed".to_string());
    let (mut stores, mut connections) = load_connections().await?;
    anyhow::ensure!(
        !connections.profiles.contains_key(&profile_id),
        "Connection profile `{profile_id}` already exists"
    );
    let descriptor = ConnectionsFile::profile_descriptor(provider);
    let credential_id = if descriptor.credential_kind == CredentialKind::AmbientCloudAuth
        || descriptor.credential_optional
    {
        credential_id
            .map(|id| validated_profile_id(&id))
            .transpose()?
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
            source,
            settings,
        },
    );
    anyhow::ensure!(
        !make_default || provider.supports_generation(),
        "evaluation-only profile cannot be the generation default"
    );
    if make_default || (connections.default_profile.is_none() && provider.supports_generation()) {
        connections.default_profile = Some(profile_id.clone());
    }
    save_connections(&mut stores, &connections).await?;
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
    let (mut stores, mut connections) = load_connections().await?;
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
        let mut merged = profile.settings.clone();
        merged.extend(parse_setting_pairs(setting_pairs)?);
        let settings = normalize_profile_settings(profile.provider, &merged);
        validate_profile_settings(profile.provider, &settings)?;
        profile.settings = settings;
    }
    profile.updated_at = Utc::now();
    save_connections(&mut stores, &connections).await?;
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
    let (mut stores, mut connections) = load_connections().await?;
    let (provider, profile_label, existing_credential_id) = {
        let profile = connection_profile(&connections, profile_id)?;
        anyhow::ensure!(
            ConnectionsFile::profile_descriptor(profile.provider).supports_secret_entry,
            "Provider `{}` does not support secret entry",
            profile.provider.as_str()
        );
        (
            profile.provider,
            profile.label.clone(),
            profile.credential_id.clone(),
        )
    };
    let credential_id = match existing_credential_id {
        Some(id) => id,
        None => {
            let id = validated_profile_id(profile_id)?;
            ensure_credential_metadata(&mut connections, &id, provider, profile_label.as_deref())?;
            if let Some(profile) = connections.profiles.get_mut(profile_id) {
                profile.credential_id = Some(id.clone());
                profile.updated_at = Utc::now();
            }
            save_connections(&mut stores, &connections).await?;
            id
        }
    };
    let request_id = request_native(&stores, profile_id, "secret_entry").await?;
    secret_store(&stores)?.save(&credential_id, &secret)?;
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
    let (mut stores, mut connections) = load_connections().await?;
    let provider = connection_profile(&connections, profile_id)?.provider;
    anyhow::ensure!(
        !use_device_code || provider == LlmProvider::Chatgpt,
        "device code login is supported only for chatgpt"
    );
    match provider {
        LlmProvider::Chatgpt => {
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
        }
        LlmProvider::OpenRouter => {
            super::login::login_openrouter_into_profile(
                &mut stores,
                &mut connections,
                profile_id,
                open_browser,
            )
            .await?;
        }
        LlmProvider::GoogleGeminiGenerateContent => {
            super::login::login_vertex_into_profile(
                &mut stores,
                &mut connections,
                profile_id,
                open_browser,
            )
            .await?;
        }
        other => anyhow::bail!(
            "login is supported for chatgpt, openrouter, and vertex profiles (found `{}`)",
            other.as_str()
        ),
    }
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
    let (mut stores, mut connections) = load_connections().await?;
    anyhow::ensure!(
        connections.profiles.contains_key(&profile_id),
        "Unknown connection profile `{profile_id}`"
    );
    connections.default_profile = Some(profile_id.clone());
    save_connections(&mut stores, &connections).await?;
    println!(
        "Default profile set to {}.",
        display_identifier(&profile_id)
    );
    Ok(())
}

pub async fn run_connection_default_clear() -> Result<()> {
    let (mut stores, mut connections) = load_connections().await?;
    connections.default_profile = None;
    save_connections(&mut stores, &connections).await?;
    println!("Cleared default profile.");
    Ok(())
}

pub async fn run_connection_test(profile_id: Option<String>) -> Result<()> {
    let (stores, connections) = load_connections().await?;
    let profile_id = profile_id
        .or_else(|| connections.default_profile.clone())
        .ok_or_else(|| anyhow::anyhow!("no profile selected"))?;
    let profile = connection_profile(&connections, &profile_id)?;
    let packages = SystemStorePaths::detect()
        .ok()
        .and_then(|system| system.packages().ok())
        .and_then(|root| PackageService::open(root).ok());
    provider_packages::validate_provider_package_for_profile(packages.as_deref(), profile)?;
    let mut config = Config::default();
    let resolved = apply_profile_to_config(
        &connections,
        Some(&profile_id),
        &secret_store(&stores)?,
        &mut config,
    )?;
    let _provider_config = config.to_provider_config()?;
    verify_connection_connectivity(&stores, &profile_id, resolved.provider, &config).await?;
    println!("profile_id: {}", display_identifier(&profile_id));
    println!("provider: {}", profile.provider.as_str());
    println!("status: success");
    Ok(())
}

pub async fn run_connection_remove(profile_id: &str) -> Result<()> {
    let profile_id = validated_profile_id(profile_id)?;
    let (mut stores, mut connections) = load_connections().await?;
    let removed = connections.profiles.remove(&profile_id).is_some();
    if connections.default_profile.as_deref() == Some(&profile_id) {
        connections.default_profile = None;
    }
    if removed {
        save_connections(&mut stores, &connections).await?;
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
        assert_eq!(parse_provider_id("magpie").unwrap(), LlmProvider::Magpie);
        assert_eq!(
            suggested_profile_id(LlmProvider::Magpie, None).unwrap(),
            "magpie-main"
        );
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
    fn first_party_catalog_has_only_four_official_providers_and_demotes_anthropic() {
        let builtins: Vec<LlmProvider> = first_party_provider_catalog()
            .iter()
            .map(|d| d.provider_id)
            .collect();
        assert_eq!(
            builtins,
            vec![
                LlmProvider::Magpie,
                LlmProvider::OpenAiResponses,
                LlmProvider::Chatgpt,
                LlmProvider::GoogleGeminiGenerateContent,
                LlmProvider::OpenRouter,
            ]
        );
        assert!(!ConnectionsFile::profile_descriptor(LlmProvider::AnthropicMessages).first_party);
        assert!(ConnectionsFile::profile_descriptor(LlmProvider::Magpie).credential_optional);
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

    #[tokio::test]
    async fn extension_provider_requires_installed_package_before_selection() {
        let packages = PackageService::ephemeral().unwrap();
        assert!(
            provider_packages::resolve_provider_selection("anthropic_messages", Some(&packages))
                .is_err()
        );
        assert!(
            provider_packages::resolve_provider_selection(
                "pkg:anthropic-provider",
                Some(&packages)
            )
            .is_err()
        );

        let pkg_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../service-manager/packages/anthropic-provider");
        let installed = alan_service_manager::run_q_command_with_package_service(
            &packages,
            Some(&pkg_dir),
            &[
                "install".to_string(),
                "--name".to_string(),
                "anthropic-provider".to_string(),
                alan_service_manager::CLI_IMPORT_MOUNT_PATH.to_string(),
            ],
        )
        .await
        .unwrap();
        assert_eq!(installed.exit_code, 0);

        let (provider, pkg) = provider_packages::resolve_provider_selection(
            "pkg:anthropic-provider",
            Some(&packages),
        )
        .unwrap();
        assert_eq!(provider, LlmProvider::AnthropicMessages);
        assert_eq!(pkg.unwrap().0, "anthropic-provider");

        let (by_proto, _) =
            provider_packages::resolve_provider_selection("anthropic_messages", Some(&packages))
                .unwrap();
        assert_eq!(by_proto, LlmProvider::AnthropicMessages);
    }
}
