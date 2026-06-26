use crate::daemon::auth_control::AuthControlState;
use crate::daemon::connection_control::{
    ConnectionControlState, ConnectionCurrentState, ConnectionPinScope, ConnectionPinState,
};
use alan_agent_engine::{
    AlanHomePaths, ConnectionProfile, ConnectionsFile, CredentialKind, LlmProvider,
    sanitize_identifier,
};
use alan_auth::ChatgptAuthConfig;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

fn display_identifier(value: &str) -> String {
    sanitize_identifier(value).unwrap_or_else(|| "<redacted>".to_string())
}

fn connection_control() -> Result<Arc<ConnectionControlState>> {
    let home_paths = AlanHomePaths::detect().context("Cannot determine alan home directory")?;
    let auth_manager = alan_auth::ChatgptAuthManager::new(ChatgptAuthConfig::with_storage_path(
        home_paths.global_auth_path.clone(),
    ))?;
    let auth_control = Arc::new(AuthControlState::new(auth_manager, false));
    Ok(ConnectionControlState::new(home_paths, auth_control))
}

fn load_connections() -> Result<(AlanHomePaths, ConnectionsFile)> {
    let home_paths = AlanHomePaths::detect().context("Cannot determine alan home directory")?;
    let (connections, _) = ConnectionsFile::load_global()?;
    Ok((home_paths, connections))
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
    let control = connection_control()?;
    let workspace_dir = detect_workspace_dir_from_cwd();
    let current = control.current_selection(workspace_dir.as_deref())?;
    if let Some(profile_id) = current.effective_profile {
        return Ok(profile_id);
    }
    anyhow::bail!(
        "no profile selected; pass <profile-id>, set a default with `alan connection default set <profile-id>`, or pin one with `alan connection pin <profile-id>`"
    )
}

fn open_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg(url).status();
    #[cfg(target_os = "linux")]
    let status = Command::new("xdg-open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("cmd").args(["/C", "start", "", url]).status();

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let status: std::io::Result<std::process::ExitStatus> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "automatic browser open is not supported on this platform",
    ));

    let status = status.context("failed to start browser opener")?;
    if !status.success() {
        anyhow::bail!("browser opener exited with status {status}");
    }
    Ok(())
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
    let control = connection_control()?;
    let provider = parse_provider_id(provider_raw)?;
    let profile_id = suggested_profile_id(provider, profile_id)?;
    let settings = parse_setting_pairs(setting_pairs)?;
    let profile = control
        .create_profile(
            &profile_id,
            label,
            provider,
            credential_id,
            settings,
            activate,
        )
        .await?;
    let _ = profile;
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
    let control = connection_control()?;
    let settings = if setting_pairs.is_empty() {
        None
    } else {
        Some(parse_setting_pairs(setting_pairs)?)
    };
    let profile = control
        .update_profile(profile_id, label, credential_id, settings)
        .await?;
    let _ = profile;
    println!(
        "Updated connection profile {}",
        display_identifier(profile_id)
    );
    Ok(())
}

pub async fn run_connection_set_secret(profile_id: &str, value: Option<String>) -> Result<()> {
    let control = connection_control()?;
    let secret = match value {
        Some(value) => value,
        None => prompt_secret_line(profile_id)?,
    };
    let _ = control.set_secret(profile_id, &secret).await?;
    println!("Stored secret for {}.", display_identifier(profile_id));
    Ok(())
}

pub async fn run_connection_login(
    profile_id: &str,
    use_device_code: bool,
    open_browser: bool,
) -> Result<()> {
    let control = connection_control()?;
    let (_, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    if profile.provider != LlmProvider::Chatgpt {
        anyhow::bail!(
            "profile `{profile_id}` uses `{}`; managed login is supported only for chatgpt",
            profile.provider.as_str()
        );
    }

    if use_device_code {
        let start = control.start_device_login(profile_id, None).await?;
        println!("Open this URL in your browser:\n{}", start.verification_url);
        println!();
        println!("Enter this one-time code:\n{}", start.user_code);
        println!();
        let login = control
            .complete_device_login(profile_id, &start.login_id)
            .await?;
        println!("Logged in to {}.", display_identifier(profile_id));
        let _ = login;
        return Ok(());
    }

    let start = control
        .start_browser_login(profile_id, None, Duration::from_secs(300))
        .await?;
    println!(
        "Browser login started for {}.",
        display_identifier(profile_id)
    );
    println!("auth_url: {}", start.auth_url);
    if open_browser {
        open_url(&start.auth_url)?;
    }
    println!(
        "Complete the browser flow, then rerun `alan connection show {}` if needed.",
        display_identifier(profile_id)
    );
    Ok(())
}

pub async fn run_connection_logout(profile_id: &str) -> Result<()> {
    let control = connection_control()?;
    let (_, connections) = load_connections()?;
    let profile = connection_profile(&connections, profile_id)?;
    match profile.provider {
        LlmProvider::Chatgpt => {
            let result = control.logout(profile_id).await?;
            println!(
                "{}",
                if result.removed {
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
    let control = connection_control()?;
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let current =
        control.current_selection(workspace_dir.as_deref().or(fallback_workspace.as_deref()))?;
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_default_set(
    profile_id: &str,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let control = connection_control()?;
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let current = control
        .set_default_profile(
            profile_id,
            workspace_dir.as_deref().or(fallback_workspace.as_deref()),
        )
        .await?;
    println!("Default profile set to {}.", display_identifier(profile_id));
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_default_clear(workspace: Option<PathBuf>) -> Result<()> {
    let control = connection_control()?;
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let current = control
        .clear_default_profile(workspace_dir.as_deref().or(fallback_workspace.as_deref()))
        .await?;
    println!("Cleared default profile.");
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_pin(
    profile_id: &str,
    scope: ConnectionPinScope,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let control = connection_control()?;
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() && scope == ConnectionPinScope::Workspace {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let effective_workspace = workspace_dir.as_deref().or(fallback_workspace.as_deref());
    let current = control
        .pin_profile(profile_id, scope, effective_workspace)
        .await?;
    println!(
        "Pinned profile {} at {} scope.",
        display_identifier(profile_id),
        scope.as_str()
    );
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_unpin(
    scope: ConnectionPinScope,
    workspace: Option<PathBuf>,
) -> Result<()> {
    let control = connection_control()?;
    let workspace_dir = workspace.as_deref().and_then(detect_workspace_dir);
    let fallback_workspace = if workspace_dir.is_none() && scope == ConnectionPinScope::Workspace {
        detect_workspace_dir_from_cwd()
    } else {
        None
    };
    let effective_workspace = workspace_dir.as_deref().or(fallback_workspace.as_deref());
    let current = control.unpin_profile(scope, effective_workspace).await?;
    println!("Cleared {} pin.", scope.as_str());
    print_current_state(&current);
    Ok(())
}

pub async fn run_connection_test(profile_id: Option<String>) -> Result<()> {
    let control = connection_control()?;
    let profile_id = profile_or_current(profile_id).await?;
    let (_, connections) = load_connections()?;
    let profile = connection_profile(&connections, &profile_id)?;
    let _ = control.test_connection(&profile_id).await?;
    println!("profile_id: {}", display_identifier(&profile_id));
    println!("provider: {}", profile.provider.as_str());
    println!("status: success");
    Ok(())
}

pub async fn run_connection_remove(profile_id: &str) -> Result<()> {
    let control = connection_control()?;
    let removed = control.delete_profile(profile_id).await?;
    if removed {
        println!(
            "Removed connection profile {}.",
            display_identifier(profile_id)
        );
    } else {
        println!(
            "Connection profile {} was not present.",
            display_identifier(profile_id)
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
