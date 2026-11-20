//! Guided provider authentication CLI (`alan login [provider]`) and shared OAuth helpers.

use super::connection::{
    ConnectionStores, chatgpt_auth_manager, connection_profile, display_identifier,
    ensure_credential_metadata, load_connections, prompt_secret_line, request_native,
    respond_native, run_connection_login, save_connections, secret_store, suggested_profile_id,
    validated_profile_id,
};
use super::connection_setup::{
    atomic_upsert_profile, load_connections_readonly, prioritize_gcp_projects,
    probe_local_magpie_quick, resolve_valid_vertex_token, vertex_auth_error_guidance,
};
use alan_agent_engine::LlmProvider;
use alan_auth::google::{
    GcpProject, GoogleAuthError, GoogleAuthManager, ResolvedGoogleToken, gcloud_default_project,
    gcloud_is_available, list_active_gcp_projects, resolve_adc_credentials_path,
    resolve_vertex_access_token, run_gcloud_adc_login,
};
use alan_auth::magpie::{DEFAULT_MAGPIE_BASE_URL, probe_magpie, probe_magpie_with_options};
use alan_auth::openrouter::{
    DEFAULT_OPENROUTER_KEY_EXCHANGE_URL, OpenRouterOAuthConfig, PendingOpenRouterBrowserLogin,
    exchange_openrouter_code, extract_authorization_code,
};
use alan_os_host::HostStorePaths;
use alan_service_manager::{
    ConnectionProfile, ConnectionsFile, normalize_profile_settings, validate_profile_settings,
};
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LoginProviderTarget {
    Chatgpt,
    OpenRouter,
    Vertex,
    Magpie,
    OpenAi,
}

impl LoginProviderTarget {
    fn from_str(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "chatgpt" | "codex" => Ok(Self::Chatgpt),
            "openrouter" => Ok(Self::OpenRouter),
            "vertex" | "vertex_gemini" | "gemini" | "google_gemini_generate_content" => {
                Ok(Self::Vertex)
            }
            "magpie" => Ok(Self::Magpie),
            "openai" | "openai_responses" => Ok(Self::OpenAi),
            other => anyhow::bail!(
                "Unknown login provider `{other}`. Supported providers: chatgpt, openrouter, vertex, magpie, openai"
            ),
        }
    }

    fn as_llm_provider(self) -> LlmProvider {
        match self {
            Self::Chatgpt => LlmProvider::Chatgpt,
            Self::OpenRouter => LlmProvider::OpenRouter,
            Self::Vertex => LlmProvider::GoogleGeminiGenerateContent,
            Self::Magpie => LlmProvider::Magpie,
            Self::OpenAi => LlmProvider::OpenAiResponses,
        }
    }
}

/// Execute top-level `alan login [provider]`.
pub async fn run_login(
    provider: Option<String>,
    no_browser: bool,
    device: bool,
    profile_override: Option<String>,
) -> Result<()> {
    let target = match provider {
        Some(raw) => LoginProviderTarget::from_str(&raw)?,
        None => prompt_provider_menu().await?,
    };

    anyhow::ensure!(
        !device || target == LoginProviderTarget::Chatgpt,
        "`--device` flow is supported only for `chatgpt`"
    );

    let open_browser = !no_browser;
    let profile_id = suggested_profile_id(target.as_llm_provider(), profile_override)?;

    match target {
        LoginProviderTarget::Chatgpt => {
            login_chatgpt_command(&profile_id, device, open_browser).await
        }
        LoginProviderTarget::OpenRouter => {
            login_openrouter_command(&profile_id, open_browser).await
        }
        LoginProviderTarget::Vertex => login_vertex_command(&profile_id, open_browser).await,
        LoginProviderTarget::Magpie => login_magpie_command(&profile_id).await,
        LoginProviderTarget::OpenAi => login_openai_command(&profile_id).await,
    }
}

async fn prompt_provider_menu() -> Result<LoginProviderTarget> {
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Provider argument required in non-interactive mode: `alan login <chatgpt|openrouter|vertex|magpie|openai>`"
    );
    let (loaded, magpie_probe) = tokio::join!(load_connections(), probe_local_magpie_quick());
    let (_, connections) = loaded?;
    let has_provider = |provider: LlmProvider| {
        connections
            .profiles
            .values()
            .any(|profile| profile.provider == provider)
    };
    let status = |provider: LlmProvider| {
        if has_provider(provider) {
            " (configured)"
        } else {
            ""
        }
    };
    let magpie_hint = magpie_probe.map_or_else(
        || DEFAULT_MAGPIE_BASE_URL.to_string(),
        |probe| format!("{}, {} models ready", probe.base_url, probe.models.len()),
    );
    println!("Select an LLM provider to connect:");
    println!(
        "  1) chatgpt    - ChatGPT Plus / Pro browser or device OAuth{}",
        status(LlmProvider::Chatgpt)
    );
    println!(
        "  2) openrouter - OpenRouter browser OAuth PKCE{}",
        status(LlmProvider::OpenRouter)
    );
    println!(
        "  3) vertex     - Google Cloud Vertex AI (OAuth / ADC + project picker){}",
        status(LlmProvider::GoogleGeminiGenerateContent)
    );
    println!(
        "  4) magpie     - Local/custom Magpie router ({magpie_hint}){}",
        status(LlmProvider::Magpie)
    );
    println!(
        "  5) openai     - OpenAI Responses API key{}",
        status(LlmProvider::OpenAiResponses)
    );
    print!("Choice [1-5]: ");
    io::stdout().flush()?;
    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let trimmed = choice.trim();
    match trimmed {
        "1" => Ok(LoginProviderTarget::Chatgpt),
        "2" => Ok(LoginProviderTarget::OpenRouter),
        "3" => Ok(LoginProviderTarget::Vertex),
        "4" => Ok(LoginProviderTarget::Magpie),
        "5" => Ok(LoginProviderTarget::OpenAi),
        other => LoginProviderTarget::from_str(other),
    }
}

async fn login_chatgpt_command(profile_id: &str, device: bool, open_browser: bool) -> Result<()> {
    let (mut stores, mut connections) = load_connections().await?;
    ensure_profile_exists(
        &mut stores,
        &mut connections,
        profile_id,
        LlmProvider::Chatgpt,
        None,
        BTreeMap::new(),
        true,
    )
    .await?;
    drop(stores);
    run_connection_login(profile_id, device, open_browser).await?;
    let (_, connections) = load_connections().await?;
    let model = effective_profile_model(&connections, profile_id);
    println!(
        "Connected {} (provider=chatgpt, model={}) and set as default profile.",
        display_identifier(profile_id),
        model
    );
    Ok(())
}

async fn login_openrouter_command(profile_id: &str, open_browser: bool) -> Result<()> {
    let (mut stores, mut connections) = load_connections().await?;
    let cred_id = validated_profile_id(profile_id)?;
    ensure_profile_exists(
        &mut stores,
        &mut connections,
        profile_id,
        LlmProvider::OpenRouter,
        Some(&cred_id),
        BTreeMap::new(),
        true,
    )
    .await?;
    login_openrouter_into_profile(&mut stores, &mut connections, profile_id, open_browser).await?;
    let model = effective_profile_model(&connections, profile_id);
    println!(
        "Connected {} (provider=openrouter, model={}) and set as default profile.",
        display_identifier(profile_id),
        model
    );
    Ok(())
}

async fn login_vertex_command(profile_id: &str, open_browser: bool) -> Result<()> {
    let host_store = HostStorePaths::detect()?;
    let existing_project = load_connections_readonly()
        .await
        .ok()
        .and_then(|connections| {
            connections
                .profiles
                .get(profile_id)
                .and_then(|profile| profile.settings.get("project_id").cloned())
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let (_, selected_project) = authenticate_vertex_and_select_project(
        &host_store.managed_auth,
        existing_project.as_deref(),
        open_browser,
    )
    .await?;

    let project_id = selected_project.context(
        "Vertex AI ADC requires a GCP `project_id`. Configure `gcloud config set project <id>` or re-run `alan login vertex`.",
    )?;
    let overrides = BTreeMap::from([("project_id".to_string(), project_id)]);
    let model = atomic_upsert_profile(
        profile_id,
        LlmProvider::GoogleGeminiGenerateContent,
        None,
        None,
        overrides,
        true,
    )
    .await?
    .unwrap_or_else(|| "<default>".to_string());
    println!(
        "Connected {} (provider=google_gemini_generate_content, model={}) and set as default profile.",
        display_identifier(profile_id),
        model
    );
    Ok(())
}

async fn login_magpie_command(profile_id: &str) -> Result<()> {
    let mut base_url = DEFAULT_MAGPIE_BASE_URL.to_string();
    let mut api_key: Option<String> = None;

    println!("Probing local Magpie router at {base_url}...");
    let mut probe = match probe_magpie(Some(&base_url)).await {
        Ok(res) => res,
        Err(err) => {
            if !io::stdin().is_terminal() {
                return Err(err.into());
            }
            println!("Local probe failed: {err}");
            base_url = prompt_line(&format!("Magpie base URL [{DEFAULT_MAGPIE_BASE_URL}]: "))?
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| DEFAULT_MAGPIE_BASE_URL.to_string());
            probe_magpie(Some(&base_url)).await?
        }
    };

    if probe.auth_required {
        let entered = prompt_secret_line(profile_id)?;
        probe = probe_magpie_with_options(
            Some(&probe.base_url),
            Some(&entered),
            Duration::from_secs(3),
        )
        .await?;
        anyhow::ensure!(
            !probe.auth_required,
            "Magpie router rejected the supplied API key"
        );
        api_key = Some(entered);
    }

    let chosen_model = select_magpie_model(&probe.models)?;
    let (mut stores, mut connections) = load_connections().await?;
    let mut overrides = BTreeMap::from([("base_url".to_string(), probe.base_url.clone())]);
    if let Some(model) = &chosen_model {
        overrides.insert("model".to_string(), model.clone());
    }
    let cred_id = if api_key.is_some() {
        Some(validated_profile_id(profile_id)?)
    } else {
        None
    };
    ensure_profile_exists(
        &mut stores,
        &mut connections,
        profile_id,
        LlmProvider::Magpie,
        cred_id.as_deref(),
        overrides,
        true,
    )
    .await?;

    if let (Some(key), Some(cred)) = (api_key, cred_id) {
        let request_id = request_native(&stores, profile_id, "secret_entry").await?;
        secret_store(&stores)?.save(&cred, &key)?;
        respond_native(
            &stores,
            &request_id,
            Some(format!("host-credential-store:{cred}")),
            "ready",
        )
        .await?;
    }

    let model = effective_profile_model(&connections, profile_id);
    println!(
        "Connected {} (provider=magpie, url={}, model={}, discovered={}) and set as default profile.",
        display_identifier(profile_id),
        probe.base_url,
        model,
        probe.models.len()
    );
    Ok(())
}

async fn login_openai_command(profile_id: &str) -> Result<()> {
    let env_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let api_key = match env_key {
        Some(key) if !io::stdin().is_terminal() => key,
        Some(key) => {
            if prompt_yes_no(
                "Detected OPENAI_API_KEY in environment. Import it? [Y/n]: ",
                true,
            )? {
                key
            } else {
                prompt_secret_line(profile_id)?
            }
        }
        None => prompt_secret_line(profile_id)?,
    };

    let (mut stores, mut connections) = load_connections().await?;
    let cred_id = validated_profile_id(profile_id)?;
    ensure_profile_exists(
        &mut stores,
        &mut connections,
        profile_id,
        LlmProvider::OpenAiResponses,
        Some(&cred_id),
        BTreeMap::new(),
        true,
    )
    .await?;
    let request_id = request_native(&stores, profile_id, "secret_entry").await?;
    secret_store(&stores)?.save(&cred_id, &api_key)?;
    respond_native(
        &stores,
        &request_id,
        Some(format!("host-credential-store:{cred_id}")),
        "ready",
    )
    .await?;

    let model = effective_profile_model(&connections, profile_id);
    println!(
        "Connected {} (provider=openai_responses, model={}) and set as default profile.",
        display_identifier(profile_id),
        model
    );
    Ok(())
}

/// Shared OpenRouter PKCE browser OAuth flow used by `alan login openrouter` and
/// `alan connection login <openrouter-profile>`.
pub(super) async fn login_openrouter_into_profile(
    stores: &mut ConnectionStores,
    connections: &mut ConnectionsFile,
    profile_id: &str,
    open_browser: bool,
) -> Result<()> {
    let credential_id =
        ensure_profile_credential_id(stores, connections, profile_id, LlmProvider::OpenRouter)
            .await?;
    let request_id = request_native(stores, profile_id, "browser_login").await?;

    let pending = PendingOpenRouterBrowserLogin::start(OpenRouterOAuthConfig::default()).await?;
    let authorize_url = pending.authorize_url().to_string();
    let callback_url = pending.callback_url().to_string();
    let verifier = pending.code_verifier().to_string();

    if open_browser {
        if let Err(err) = pending.open_browser() {
            eprintln!("Could not launch browser automatically ({err}).");
        }
        println!("Open this URL in your browser:\n{authorize_url}");
    } else {
        println!("Open this URL in your browser:\n{authorize_url}");
        println!(
            "Waiting on {callback_url}, or paste the redirected callback URL / authorization code and press Enter:"
        );
    }

    let success = if open_browser {
        pending.wait_and_exchange(Duration::from_secs(300)).await?
    } else if !io::stdin().is_terminal() {
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let code = extract_authorization_code(&line)?;
        exchange_openrouter_code(DEFAULT_OPENROUTER_KEY_EXCHANGE_URL, &code, &verifier).await?
    } else {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            let _ = io::stdin().read_line(&mut line);
            let _ = tx.send(line);
        });
        tokio::select! {
            res = pending.wait_and_exchange(Duration::from_secs(300)) => res?,
            pasted = &mut rx => {
                let raw = pasted.context("stdin paste channel closed")?;
                let code = extract_authorization_code(&raw)?;
                exchange_openrouter_code(DEFAULT_OPENROUTER_KEY_EXCHANGE_URL, &code, &verifier).await?
            }
        }
    };

    secret_store(stores)?.save(&credential_id, &success.api_key)?;
    respond_native(
        stores,
        &request_id,
        Some(format!("host-credential-store:{credential_id}")),
        "ready",
    )
    .await?;
    Ok(())
}

/// Shared Google Cloud Vertex AI authentication + GCP project picker used by `alan login vertex`
/// and `alan connection login <vertex-profile>`.
pub(super) async fn login_vertex_into_profile(
    stores: &mut ConnectionStores,
    connections: &mut ConnectionsFile,
    profile_id: &str,
    open_browser: bool,
) -> Result<()> {
    let existing_project = connection_profile(connections, profile_id)?
        .settings
        .get("project_id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let request_id = request_native(stores, profile_id, "browser_login").await?;
    let (_, selected_project) = authenticate_vertex_and_select_project(
        &stores.managed_auth,
        existing_project.as_deref(),
        open_browser,
    )
    .await?;

    if let Some(selected_project) = selected_project
        && let Some(profile) = connections.profiles.get_mut(profile_id)
    {
        profile
            .settings
            .insert("project_id".to_string(), selected_project);
        profile.updated_at = Utc::now();
        save_connections(stores, connections).await?;
    }

    respond_native(
        stores,
        &request_id,
        Some(format!("host-managed-auth:{profile_id}")),
        "ready",
    )
    .await?;
    Ok(())
}

async fn authenticate_vertex_and_select_project(
    managed_auth: &Path,
    existing_project: Option<&str>,
    open_browser: bool,
) -> Result<(ResolvedGoogleToken, Option<String>)> {
    let resolved = match resolve_valid_vertex_token(managed_auth).await {
        Some(token) => token,
        None => {
            let manager = GoogleAuthManager::with_storage_path(managed_auth.to_path_buf())?;
            match manager.start_browser_login().await {
                Ok(pending) => {
                    let authorize_url = pending.authorize_url().to_string();
                    if open_browser && let Err(err) = pending.open_browser() {
                        eprintln!("Could not launch browser automatically ({err}).");
                    }
                    println!("Open this URL in your browser:\n{authorize_url}");
                    if open_browser || !io::stdin().is_terminal() {
                        pending.wait_and_exchange(Duration::from_secs(300)).await?;
                    } else {
                        println!(
                            "Waiting for callback, or paste the redirected URL / code and press Enter:"
                        );
                        let mut pasted = String::new();
                        io::stdin().read_line(&mut pasted)?;
                        let trimmed = pasted.trim();
                        if trimmed.is_empty() {
                            pending.wait_and_exchange(Duration::from_secs(300)).await?;
                        } else {
                            pending.exchange_pasted_input(trimmed).await?;
                        }
                    }
                    resolve_vertex_access_token(Some(managed_auth)).await?
                }
                Err(GoogleAuthError::OAuthClientNotConfigured) => {
                    anyhow::ensure!(gcloud_is_available(), "{}", vertex_auth_error_guidance());
                    if let Some(adc_path) = resolve_adc_credentials_path() {
                        println!(
                            "Existing Google Application Default Credentials at {} could not be refreshed; running `gcloud auth application-default login`...",
                            adc_path.display()
                        );
                    } else {
                        println!(
                            "No Google Desktop OAuth client configured (ALAN_GOOGLE_OAUTH_CLIENT_ID); running `gcloud auth application-default login`..."
                        );
                    }
                    run_gcloud_adc_login().await?;
                    resolve_vertex_access_token(Some(managed_auth)).await?
                }
                Err(err) => return Err(err.into()),
            }
        }
    };

    println!("Authenticated via {}.", resolved.source.label());

    let (projects_result, gcloud_default) = tokio::join!(
        list_active_gcp_projects(&resolved.access_token),
        gcloud_default_project(),
    );
    let projects = match projects_result {
        Ok(list) => list,
        Err(err) => {
            tracing::warn!(%err, "failed to list active GCP projects");
            Vec::new()
        }
    };
    let preferred_project = existing_project.or(gcloud_default.as_deref());
    let projects = prioritize_gcp_projects(projects, preferred_project);
    let selected_project = choose_gcp_project(projects, preferred_project)?;
    Ok((resolved, selected_project))
}

async fn ensure_profile_exists(
    stores: &mut ConnectionStores,
    connections: &mut ConnectionsFile,
    profile_id: &str,
    provider: LlmProvider,
    credential_id: Option<&str>,
    overrides: BTreeMap<String, String>,
    set_default: bool,
) -> Result<()> {
    let mut changed = false;
    if let Some(existing) = connections.profiles.get_mut(profile_id) {
        anyhow::ensure!(
            existing.provider == provider,
            "Profile `{profile_id}` already exists for provider `{}`, expected `{}`",
            existing.provider.as_str(),
            provider.as_str()
        );
        if !overrides.is_empty() {
            let mut merged = existing.settings.clone();
            merged.extend(overrides);
            let normalized = normalize_profile_settings(provider, &merged);
            validate_profile_settings(provider, &normalized)?;
            existing.settings = normalized;
            existing.updated_at = Utc::now();
            changed = true;
        }
    } else {
        if let Some(cred_id) = credential_id {
            ensure_credential_metadata(connections, cred_id, provider, None)?;
        }
        let settings = normalize_profile_settings(provider, &overrides);
        validate_profile_settings(provider, &settings)?;
        let now = Utc::now();
        connections.profiles.insert(
            profile_id.to_string(),
            ConnectionProfile {
                provider,
                label: None,
                credential_id: credential_id.map(ToOwned::to_owned),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings,
            },
        );
        changed = true;
    }

    if set_default && connections.default_profile.as_deref() != Some(profile_id) {
        connections.default_profile = Some(profile_id.to_string());
        changed = true;
    }
    if changed {
        save_connections(stores, connections).await?;
    }
    let _ = chatgpt_auth_manager(stores);
    Ok(())
}

async fn ensure_profile_credential_id(
    stores: &mut ConnectionStores,
    connections: &mut ConnectionsFile,
    profile_id: &str,
    provider: LlmProvider,
) -> Result<String> {
    let profile = connection_profile(connections, profile_id)?;
    let profile_label = profile.label.clone();
    if let Some(existing) = &profile.credential_id {
        return Ok(existing.clone());
    }
    let cred_id = validated_profile_id(profile_id)?;
    ensure_credential_metadata(connections, &cred_id, provider, profile_label.as_deref())?;
    if let Some(profile) = connections.profiles.get_mut(profile_id) {
        profile.credential_id = Some(cred_id.clone());
        profile.updated_at = Utc::now();
    }
    save_connections(stores, connections).await?;
    Ok(cred_id)
}

fn effective_profile_model(connections: &ConnectionsFile, profile_id: &str) -> String {
    connections
        .profiles
        .get(profile_id)
        .and_then(|profile| {
            normalize_profile_settings(profile.provider, &profile.settings)
                .get("model")
                .cloned()
        })
        .unwrap_or_else(|| "<default>".to_string())
}

fn choose_gcp_project(
    projects: Vec<GcpProject>,
    existing_project: Option<&str>,
) -> Result<Option<String>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(existing_project
            .map(ToOwned::to_owned)
            .or_else(|| projects.first().map(|p| p.project_id.clone())));
    }
    if projects.is_empty() {
        let prompt = match existing_project {
            Some(current) => format!("GCP project_id [{current}]: "),
            None => "GCP project_id (leave empty to configure later): ".to_string(),
        };
        return Ok(prompt_line(&prompt)?
            .filter(|v| !v.is_empty())
            .or_else(|| existing_project.map(ToOwned::to_owned)));
    }

    println!("Active Google Cloud projects:");
    for (idx, project) in projects.iter().enumerate() {
        let num = idx + 1;
        if project.name.is_empty() || project.name == project.project_id {
            println!("  {num}) {}", project.project_id);
        } else {
            println!("  {num}) {} ({})", project.project_id, project.name);
        }
    }
    let prompt = match existing_project {
        Some(current) => format!("Select project number or enter project_id [{current}]: "),
        None => "Select project number or enter project_id [1]: ".to_string(),
    };
    let answer = prompt_line(&prompt)?;
    match answer.as_deref().map(str::trim) {
        None | Some("") => Ok(existing_project
            .map(ToOwned::to_owned)
            .or_else(|| projects.first().map(|p| p.project_id.clone()))),
        Some(text) => {
            if let Ok(index) = text.parse::<usize>()
                && index >= 1
                && index <= projects.len()
            {
                return Ok(Some(projects[index - 1].project_id.clone()));
            }
            Ok(Some(text.to_string()))
        }
    }
}

fn select_magpie_model(models: &[String]) -> Result<Option<String>> {
    if models.is_empty() {
        return Ok(None);
    }
    if models.len() == 1 || !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(Some(models[0].clone()));
    }
    println!("Discovered Magpie models:");
    for (idx, model) in models.iter().enumerate() {
        println!("  {}) {model}", idx + 1);
    }
    let answer = prompt_line("Select default model number or slug [1]: ")?;
    match answer.as_deref().map(str::trim) {
        None | Some("") => Ok(Some(models[0].clone())),
        Some(text) => {
            if let Ok(index) = text.parse::<usize>()
                && index >= 1
                && index <= models.len()
            {
                return Ok(Some(models[index - 1].clone()));
            }
            Ok(Some(text.to_string()))
        }
    }
}

fn prompt_line(prompt: &str) -> Result<Option<String>> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    let n = io::stdin().read_line(&mut line)?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let Some(answer) = prompt_line(prompt)? else {
        return Ok(default_yes);
    };
    if answer.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(answer.to_ascii_lowercase().as_str(), "y" | "yes"))
}
