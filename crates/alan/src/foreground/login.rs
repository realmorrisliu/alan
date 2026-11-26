//! Host-local interactive `/login` handler wired into the file-backed TUI.

use crate::cli::connection_setup::{
    atomic_upsert_profile_and_reload, load_connections_readonly, prioritize_gcp_projects,
    probe_local_magpie_quick, resolve_valid_vertex_token,
};
use alan_agent_engine::LlmProvider;
use alan_auth::google::{
    GoogleAuthError, GoogleAuthManager, gcloud_default_project, gcloud_is_available,
    list_active_gcp_projects, resolve_vertex_access_token, run_gcloud_adc_login_streaming,
};
use alan_auth::magpie::{DEFAULT_MAGPIE_BASE_URL, probe_magpie_with_options};
use alan_auth::openrouter::{OpenRouterOAuthConfig, PendingOpenRouterBrowserLogin};
use alan_auth::{
    BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthManager, browser::open_browser,
};
use alan_os_host::{HostEndpointPaths, HostStorePaths};
use alan_service_manager::sanitize_identifier;
use alan_tui::{
    LoginControl, LoginControlHandler, LoginInputField, LoginMethod, LoginProviderChoice,
    LoginResult, LoginSelectOption, LoginSubmission,
};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Build the TUI `LoginControlHandler` bound to this Host's stores and command plane.
pub(super) fn build_login_control_handler(paths: HostEndpointPaths) -> LoginControlHandler {
    Arc::new(move |command| {
        let paths = paths.clone();
        Box::pin(async move {
            match command {
                LoginControl::ListProviders => list_providers().await,
                LoginControl::Submit(submission) => match submit_login(&paths, submission).await {
                    Ok(result) => Ok(result),
                    Err(error) => Ok(LoginResult::Failed {
                        message: format!("{error:#}"),
                    }),
                },
            }
        })
    })
}

async fn list_providers() -> Result<LoginResult> {
    let (connections_res, magpie_probe) =
        tokio::join!(load_connections_readonly(), probe_local_magpie_quick());
    let connections = connections_res.unwrap_or_default();
    let has = |provider: LlmProvider| {
        connections
            .profiles
            .values()
            .any(|p| p.provider == provider)
    };
    let suffix = |provider: LlmProvider, base: String| {
        if has(provider) {
            format!("{base} [configured]")
        } else {
            base
        }
    };
    let magpie_detail = magpie_probe.map_or_else(
        || format!("Auto-probe {DEFAULT_MAGPIE_BASE_URL}"),
        |probe| format!("Local router ready ({} models)", probe.models.len()),
    );

    Ok(LoginResult::Providers(vec![
        LoginProviderChoice {
            id: "chatgpt".to_string(),
            label: "ChatGPT (Sign in with ChatGPT)".to_string(),
            detail: suffix(
                LlmProvider::Chatgpt,
                "Browser OAuth (use your ChatGPT plan & credits)".to_string(),
            ),
            default_profile: "chatgpt-main".to_string(),
            method: LoginMethod::BrowserOrAuto,
        },
        LoginProviderChoice {
            id: "openrouter".to_string(),
            label: "OpenRouter".to_string(),
            detail: suffix(LlmProvider::OpenRouter, "PKCE Browser OAuth".to_string()),
            default_profile: "openrouter-main".to_string(),
            method: LoginMethod::BrowserOrAuto,
        },
        LoginProviderChoice {
            id: "vertex".to_string(),
            label: "Google Cloud Vertex AI".to_string(),
            detail: suffix(
                LlmProvider::GoogleGeminiGenerateContent,
                "Google OAuth / ADC + project picker".to_string(),
            ),
            default_profile: "vertex-main".to_string(),
            method: LoginMethod::CloudProject,
        },
        LoginProviderChoice {
            id: "magpie".to_string(),
            label: "Magpie".to_string(),
            detail: suffix(LlmProvider::Magpie, magpie_detail),
            default_profile: "magpie-main".to_string(),
            method: LoginMethod::BrowserOrAuto,
        },
        LoginProviderChoice {
            id: "openai".to_string(),
            label: "OpenAI".to_string(),
            detail: suffix(
                LlmProvider::OpenAiResponses,
                "API key (OPENAI_API_KEY)".to_string(),
            ),
            default_profile: "openai-main".to_string(),
            method: LoginMethod::ApiKey {
                allow_base_url: true,
            },
        },
    ]))
}

async fn submit_login(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    match sub.provider_id.trim().to_ascii_lowercase().as_str() {
        "chatgpt" => submit_chatgpt(paths, sub).await,
        "openrouter" => submit_openrouter(paths, sub).await,
        "vertex" | "vertex_gemini" | "google_gemini_generate_content" | "gemini" => {
            submit_vertex(paths, sub).await
        }
        "magpie" => submit_magpie(paths, sub).await,
        "openai" | "openai_responses" => submit_openai(paths, sub).await,
        other => Ok(LoginResult::Failed {
            message: format!("Unknown provider `{other}`"),
        }),
    }
}

async fn submit_chatgpt(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    let host_store = HostStorePaths::detect()?;
    let manager = ChatgptAuthManager::new(ChatgptAuthConfig::with_storage_path(
        host_store.managed_auth,
    ))?;
    sub.progress.status("Starting Sign in with ChatGPT...");
    let server = manager.bind_browser_callback_server().await?;
    let pending = manager.begin_browser_login(BrowserLoginOptions::default())?;
    let opened = open_browser(&pending.auth_url).is_ok();
    sub.progress.browser_prompt(
        pending.auth_url.clone(),
        None,
        Some(if opened {
            "Complete Sign in with ChatGPT in your browser, then return here.".to_string()
        } else {
            "Open this URL in your browser to Sign in with ChatGPT.".to_string()
        }),
    );

    let mut receipt = manager
        .wait_for_browser_callback_on(&server, &pending)
        .await?;
    let result = manager
        .complete_browser_login(&pending, receipt.completion.clone())
        .await;
    let _ = manager
        .write_browser_login_result(&mut receipt.stream, result.as_ref())
        .await;
    let success = result?;

    let profile_id = normalized_profile_id(sub.profile_id.as_deref(), "chatgpt-main")?;
    let model = atomic_upsert_profile_and_reload(
        paths,
        &profile_id,
        LlmProvider::Chatgpt,
        None,
        None,
        BTreeMap::new(),
        true,
    )
    .await?;

    Ok(LoginResult::Connected {
        profile: profile_id,
        provider: LlmProvider::Chatgpt.as_str().to_string(),
        model,
        message: success.email.map(|email| format!("Signed in as {email}")),
    })
}

async fn submit_openrouter(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    sub.progress
        .status("Starting OpenRouter PKCE browser sign-in...");
    let pending = PendingOpenRouterBrowserLogin::start(OpenRouterOAuthConfig::default()).await?;
    let opened = pending.open_browser().is_ok();
    sub.progress.browser_prompt(
        pending.authorize_url().to_string(),
        None,
        Some(if opened {
            "Authorize Alan in your browser; waiting on local callback...".to_string()
        } else {
            "Open this URL in your browser to authorize Alan with OpenRouter.".to_string()
        }),
    );

    let success = pending.wait_and_exchange(Duration::from_secs(300)).await?;
    let profile_id = normalized_profile_id(sub.profile_id.as_deref(), "openrouter-main")?;
    let model = atomic_upsert_profile_and_reload(
        paths,
        &profile_id,
        LlmProvider::OpenRouter,
        Some(&profile_id),
        Some(&success.api_key),
        BTreeMap::new(),
        true,
    )
    .await?;

    Ok(LoginResult::Connected {
        profile: profile_id,
        provider: LlmProvider::OpenRouter.as_str().to_string(),
        model,
        message: Some("OpenRouter connected via PKCE OAuth".to_string()),
    })
}

async fn submit_vertex(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    let profile_id = normalized_profile_id(sub.profile_id.as_deref(), "vertex-main")?;

    if let Some(secret) = sub
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        sub.progress.status("Saving Vertex AI Express API key...");
        let model = atomic_upsert_profile_and_reload(
            paths,
            &profile_id,
            LlmProvider::GoogleGeminiGenerateContent,
            Some(&profile_id),
            Some(secret),
            BTreeMap::new(),
            true,
        )
        .await?;
        return Ok(LoginResult::Connected {
            profile: profile_id,
            provider: LlmProvider::GoogleGeminiGenerateContent
                .as_str()
                .to_string(),
            model,
            message: Some("Vertex AI Express connected via API key".to_string()),
        });
    }

    let host_store = HostStorePaths::detect()?;
    sub.progress
        .status("Checking Google Cloud Vertex AI credentials...");

    let resolved = match resolve_valid_vertex_token(&host_store.managed_auth).await {
        Some(token) => token,
        None => {
            let manager = GoogleAuthManager::with_storage_path(host_store.managed_auth.clone())?;
            match manager.start_browser_login().await {
                Ok(pending) => {
                    let opened = pending.open_browser().is_ok();
                    sub.progress.browser_prompt(
                        pending.authorize_url().to_string(),
                        None,
                        Some(if opened {
                            "Sign in with Google Cloud in your browser...".to_string()
                        } else {
                            "Open this URL in your browser to sign in to Google Cloud.".to_string()
                        }),
                    );
                    pending.wait_and_exchange(Duration::from_secs(300)).await?;
                    resolve_vertex_access_token(Some(&host_store.managed_auth)).await?
                }
                Err(GoogleAuthError::OAuthClientNotConfigured) => {
                    if gcloud_is_available() {
                        sub.progress.status(
                            "Launching Google Cloud ADC sign-in (`gcloud auth application-default login`)...",
                        );
                        let progress = sub.progress.clone();
                        run_gcloud_adc_login_streaming(move |url| {
                            let _ = progress.browser_prompt(
                                url,
                                None,
                                Some(
                                    "Complete Google Cloud sign-in in your browser, then return here."
                                        .to_string(),
                                ),
                            );
                        })
                        .await?;
                        resolve_vertex_access_token(Some(&host_store.managed_auth)).await?
                    } else {
                        return Ok(LoginResult::NeedsInput {
                            field: LoginInputField::Secret,
                            title: "Vertex AI Express API Key".to_string(),
                            prompt: "No `gcloud` CLI or Google OAuth client found. Enter a Vertex AI Express API key (or press Esc and run `gcloud auth application-default login`):".to_string(),
                            masked: true,
                            allow_empty: false,
                            default_value: None,
                        });
                    }
                }
                Err(err) => return Err(err.into()),
            }
        }
    };

    let selected_project = sub
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .or_else(|| {
            sub.selection
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
        .map(ToOwned::to_owned);

    let project_id = if let Some(project) = selected_project {
        project
    } else {
        sub.progress
            .status("Listing active Google Cloud projects...");
        let (projects_res, default_proj) = tokio::join!(
            list_active_gcp_projects(&resolved.access_token),
            gcloud_default_project(),
        );
        let projects =
            prioritize_gcp_projects(projects_res.unwrap_or_default(), default_proj.as_deref());
        if !projects.is_empty() {
            let options = projects
                .into_iter()
                .map(|p| {
                    let is_default = default_proj.as_deref() == Some(p.project_id.as_str());
                    LoginSelectOption {
                        id: p.project_id.clone(),
                        label: if p.name.is_empty() || p.name == p.project_id {
                            p.project_id
                        } else {
                            format!("{} ({})", p.name, p.project_id)
                        },
                        detail: is_default.then(|| "gcloud default".to_string()),
                    }
                })
                .collect();
            return Ok(LoginResult::NeedsSelection {
                title: "Select Google Cloud Project".to_string(),
                prompt: "Choose a GCP project for Vertex AI:".to_string(),
                options,
            });
        }
        return Ok(LoginResult::NeedsInput {
            field: LoginInputField::ProjectId,
            title: "Google Cloud Project ID".to_string(),
            prompt: "Enter your GCP project_id for Vertex AI:".to_string(),
            masked: false,
            allow_empty: false,
            default_value: default_proj,
        });
    };

    let overrides = BTreeMap::from([("project_id".to_string(), project_id.clone())]);
    let model = atomic_upsert_profile_and_reload(
        paths,
        &profile_id,
        LlmProvider::GoogleGeminiGenerateContent,
        None,
        None,
        overrides,
        true,
    )
    .await?;

    Ok(LoginResult::Connected {
        profile: profile_id,
        provider: LlmProvider::GoogleGeminiGenerateContent
            .as_str()
            .to_string(),
        model,
        message: Some(format!(
            "Vertex AI connected (project={project_id}, {})",
            resolved.source.label()
        )),
    })
}

async fn submit_magpie(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    let base_url = sub
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_MAGPIE_BASE_URL);
    sub.progress
        .status(format!("Probing Magpie router at {base_url}..."));

    let probe = match probe_magpie_with_options(
        Some(base_url),
        sub.secret.as_deref(),
        Duration::from_secs(2),
    )
    .await
    {
        Ok(res) => res,
        Err(err) if sub.base_url.is_none() => {
            return Ok(LoginResult::NeedsInput {
                field: LoginInputField::BaseUrl,
                title: "Magpie Router URL".to_string(),
                prompt: format!(
                    "No router answered on {DEFAULT_MAGPIE_BASE_URL} ({err}). Enter router base URL:"
                ),
                masked: false,
                allow_empty: false,
                default_value: Some(DEFAULT_MAGPIE_BASE_URL.to_string()),
            });
        }
        Err(err) => return Err(err.into()),
    };

    if probe.auth_required && sub.secret.is_none() {
        return Ok(LoginResult::NeedsInput {
            field: LoginInputField::Secret,
            title: "Magpie API Key".to_string(),
            prompt: format!("{} requires an API key:", probe.base_url),
            masked: true,
            allow_empty: false,
            default_value: None,
        });
    }

    if probe.models.len() > 1 && sub.selection.is_none() {
        let options = probe
            .models
            .iter()
            .map(|slug| LoginSelectOption {
                id: slug.clone(),
                label: slug.clone(),
                detail: None,
            })
            .collect();
        return Ok(LoginResult::NeedsSelection {
            title: "Select Magpie Model".to_string(),
            prompt: format!("Choose default model from {}:", probe.base_url),
            options,
        });
    }

    let profile_id = normalized_profile_id(sub.profile_id.as_deref(), "magpie-main")?;
    let mut overrides = BTreeMap::from([("base_url".to_string(), probe.base_url.clone())]);
    if let Some(model) = sub
        .selection
        .clone()
        .or_else(|| probe.models.first().cloned())
    {
        overrides.insert("model".to_string(), model);
    }

    let secret = sub
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let cred_id = secret.map(|_| profile_id.as_str());

    let model = atomic_upsert_profile_and_reload(
        paths,
        &profile_id,
        LlmProvider::Magpie,
        cred_id,
        secret,
        overrides,
        true,
    )
    .await?;

    Ok(LoginResult::Connected {
        profile: profile_id,
        provider: LlmProvider::Magpie.as_str().to_string(),
        model,
        message: Some(format!("Connected Magpie ({})", probe.base_url)),
    })
}

async fn submit_openai(paths: &HostEndpointPaths, sub: LoginSubmission) -> Result<LoginResult> {
    let env_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let api_key = if let Some(secret) = sub
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        secret.to_string()
    } else if sub.selection.as_deref() == Some("env") {
        env_key.context("OPENAI_API_KEY was unset before selection completed")?
    } else if env_key.is_some() && sub.selection.is_none() {
        return Ok(LoginResult::NeedsSelection {
            title: "OpenAI Credentials".to_string(),
            prompt: "OPENAI_API_KEY found in environment:".to_string(),
            options: vec![
                LoginSelectOption {
                    id: "env".to_string(),
                    label: "Use OPENAI_API_KEY from environment".to_string(),
                    detail: None,
                },
                LoginSelectOption {
                    id: "manual".to_string(),
                    label: "Enter an OpenAI API key manually".to_string(),
                    detail: None,
                },
            ],
        });
    } else {
        return Ok(LoginResult::NeedsInput {
            field: LoginInputField::Secret,
            title: "OpenAI API Key".to_string(),
            prompt: "Enter your OpenAI API key:".to_string(),
            masked: true,
            allow_empty: false,
            default_value: None,
        });
    };

    let profile_id = normalized_profile_id(sub.profile_id.as_deref(), "openai-main")?;

    let mut overrides = BTreeMap::new();
    if let Some(base_url) = sub
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        overrides.insert("base_url".to_string(), base_url.to_string());
    }

    let model = atomic_upsert_profile_and_reload(
        paths,
        &profile_id,
        LlmProvider::OpenAiResponses,
        Some(&profile_id),
        Some(&api_key),
        overrides,
        true,
    )
    .await?;

    Ok(LoginResult::Connected {
        profile: profile_id,
        provider: LlmProvider::OpenAiResponses.as_str().to_string(),
        model,
        message: Some("OpenAI connected".to_string()),
    })
}

fn normalized_profile_id(requested: Option<&str>, default_id: &str) -> Result<String> {
    let raw = requested
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(default_id);
    sanitize_identifier(raw).ok_or_else(|| anyhow::anyhow!("Invalid profile id `{raw}`"))
}
