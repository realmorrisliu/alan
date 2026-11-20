use super::{ConnectionStores, chatgpt_auth_manager, require_non_empty};
use alan_agent_engine::{Config, LlmProvider};
use anyhow::{Context, Result};

/// Verify connectivity and real credential validity for a configured connection profile.
pub(super) async fn verify_connection_connectivity(
    stores: &ConnectionStores,
    profile_id: &str,
    provider: LlmProvider,
    config: &Config,
) -> Result<()> {
    if provider == LlmProvider::TypesafeEvaluation {
        return Ok(());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|err| anyhow::anyhow!("build HTTP client for connection test: {err}"))?;

    fn require_base<'a>(profile_id: &str, raw: &'a str) -> Result<&'a str> {
        let trimmed = raw.trim();
        anyhow::ensure!(
            !trimmed.is_empty(),
            "profile `{profile_id}` is missing a base URL"
        );
        Ok(trimmed)
    }

    let request = match provider {
        LlmProvider::Chatgpt => {
            let auth = chatgpt_auth_manager(stores)?
                .request_auth_for_account(config.chatgpt_account_id.as_deref())
                .await
                .map_err(|err| {
                    anyhow::anyhow!("ChatGPT profile `{profile_id}` authentication failed: {err}")
                })?;
            let base = require_base(profile_id, &config.chatgpt_base_url)?;
            let mut url = reqwest::Url::parse(&format!("{}/models", base.trim_end_matches('/')))
                .with_context(|| format!("invalid ChatGPT base URL for `{profile_id}`"))?;
            url.query_pairs_mut()
                .append_pair("client_version", "0.159.0");
            client
                .get(url)
                .bearer_auth(&auth.access_token)
                .header("chatgpt-account-id", &auth.account_id)
        }
        LlmProvider::Magpie
        | LlmProvider::OpenAiResponses
        | LlmProvider::OpenAiChatCompletions
        | LlmProvider::OpenAiChatCompletionsCompatible
        | LlmProvider::OpenRouter => {
            let (raw_base, key) = match provider {
                LlmProvider::Magpie => (
                    &config.magpie_base_url,
                    require_non_empty(&config.magpie_api_key),
                ),
                LlmProvider::OpenAiResponses => (
                    &config.openai_responses_base_url,
                    require_non_empty(&config.openai_responses_api_key),
                ),
                LlmProvider::OpenAiChatCompletions => (
                    &config.openai_chat_completions_base_url,
                    require_non_empty(&config.openai_chat_completions_api_key),
                ),
                LlmProvider::OpenAiChatCompletionsCompatible => (
                    &config.openai_chat_completions_compatible_base_url,
                    require_non_empty(&config.openai_chat_completions_compatible_api_key),
                ),
                LlmProvider::OpenRouter => (
                    &config.openrouter_base_url,
                    require_non_empty(&config.openrouter_api_key),
                ),
                _ => unreachable!(),
            };
            let base = require_base(profile_id, raw_base)?;
            let url = format!("{}/models", base.trim_end_matches('/'));
            let mut req = client.get(&url);
            if let Some(key) = key {
                req = req.bearer_auth(key);
            } else if !matches!(
                provider,
                LlmProvider::Magpie | LlmProvider::OpenAiChatCompletionsCompatible
            ) {
                anyhow::bail!("profile `{profile_id}` has no API key configured");
            }
            if provider == LlmProvider::OpenRouter {
                if let Some(referer) = require_non_empty(&config.openrouter_http_referer) {
                    req = req.header("HTTP-Referer", referer);
                }
                if let Some(title) = require_non_empty(&config.openrouter_x_title) {
                    req = req.header("X-Title", title);
                }
            }
            req
        }
        LlmProvider::AnthropicMessages => {
            let base = require_base(profile_id, &config.anthropic_messages_base_url)?;
            let trimmed = base.trim_end_matches('/');
            let url = if trimmed.ends_with("/v1") {
                format!("{trimmed}/models")
            } else {
                format!("{trimmed}/v1/models")
            };
            let key = require_non_empty(&config.anthropic_messages_api_key).ok_or_else(|| {
                anyhow::anyhow!("profile `{profile_id}` has no API key configured")
            })?;
            let mut req = client
                .get(&url)
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
            if let Some(ua) = require_non_empty(&config.anthropic_messages_user_agent) {
                req = req.header("User-Agent", ua);
            }
            req
        }
        LlmProvider::GoogleGeminiGenerateContent => {
            if let Some(key) = require_non_empty(&config.google_gemini_generate_content_api_key) {
                let url = "https://aiplatform.googleapis.com/v1/publishers/google/models";
                client.get(url).header("x-goog-api-key", key)
            } else {
                let resolved = alan_auth::google::resolve_vertex_access_token(Some(
                    &stores.managed_auth,
                ))
                .await
                .map_err(|err| {
                    anyhow::anyhow!(
                        "Google Vertex credentials unavailable for profile `{profile_id}`: {err}"
                    )
                })?;
                if let Some(project_id) =
                    require_non_empty(&config.google_gemini_generate_content_project_id)
                {
                    let location = config.google_gemini_generate_content_location.trim();
                    let location = if location.is_empty() {
                        "global"
                    } else {
                        location
                    };
                    let host = if location == "global" {
                        "aiplatform.googleapis.com".to_string()
                    } else {
                        format!("{location}-aiplatform.googleapis.com")
                    };
                    let url = format!(
                        "https://{host}/v1/projects/{project_id}/locations/{location}/publishers/google/models"
                    );
                    client.get(&url).bearer_auth(&resolved.access_token)
                } else {
                    client
                        .get(alan_auth::google::GOOGLE_PROJECTS_URL)
                        .bearer_auth(&resolved.access_token)
                }
            }
        }
        LlmProvider::TypesafeEvaluation => unreachable!(),
    };

    let response = request.send().await.map_err(|err| {
        anyhow::anyhow!("connection test failed for profile `{profile_id}`: {err}")
    })?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        anyhow::bail!("authentication failed for profile `{profile_id}` (HTTP {status})");
    }
    if !status.is_success() {
        anyhow::bail!("connection test failed for profile `{profile_id}` (HTTP {status})");
    }
    Ok(())
}
