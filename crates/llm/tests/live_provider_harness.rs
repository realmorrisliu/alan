use alan_llm::factory::{self, ProviderConfig};
use alan_llm::{GenerationRequest, LlmProvider, ReasoningEffort, StreamChunk, ToolDefinition};
use anyhow::{Context, Result, ensure};
use std::env;
use std::path::PathBuf;
use tokio::sync::mpsc::Receiver;
use tokio::time::{Duration, Instant, timeout};

const LIVE_ENABLE_ENV: &str = "ALAN_LIVE_PROVIDER_TESTS";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const STREAM_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
struct LiveProviderHarness {
    label: &'static str,
    config: ProviderConfig,
}

#[derive(Debug)]
struct CollectedStream {
    text: String,
    saw_final_chunk: bool,
    final_finish_reason: Option<String>,
    saw_provider_response_id: bool,
}

fn live_enabled() -> bool {
    env::var(LIVE_ENABLE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn provider_harness_or_skip(provider: &'static str) -> Option<LiveProviderHarness> {
    if !live_enabled() {
        eprintln!("[live-provider-harness] skipping {provider}: set {LIVE_ENABLE_ENV}=1 to enable");
        return None;
    }

    match provider {
        "openai_responses" => {
            let Some(api_key) = non_empty_env("ALAN_LIVE_OPENAI_RESPONSES_API_KEY") else {
                eprintln!(
                    "[live-provider-harness] skipping openai_responses: ALAN_LIVE_OPENAI_RESPONSES_API_KEY is unset"
                );
                return None;
            };
            let model = non_empty_env("ALAN_LIVE_OPENAI_RESPONSES_MODEL")
                .unwrap_or_else(|| "gpt-5.4".to_string());
            let mut config = ProviderConfig::openai_responses(api_key, model);
            if let Some(base_url) = non_empty_env("ALAN_LIVE_OPENAI_RESPONSES_BASE_URL") {
                config = config.with_base_url(base_url);
            }
            Some(LiveProviderHarness {
                label: "openai_responses",
                config,
            })
        }
        "chatgpt" => {
            let Some(auth_storage_path) = non_empty_env("ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH")
            else {
                eprintln!(
                    "[live-provider-harness] skipping chatgpt: ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH is unset"
                );
                return None;
            };
            let model =
                non_empty_env("ALAN_LIVE_CHATGPT_MODEL").unwrap_or_else(|| "gpt-5.4".to_string());
            let mut config = ProviderConfig::chatgpt(model)
                .with_chatgpt_auth_storage_path(PathBuf::from(auth_storage_path));
            if let Some(base_url) = non_empty_env("ALAN_LIVE_CHATGPT_BASE_URL") {
                config = config.with_base_url(base_url);
            }
            if let Some(account_id) = non_empty_env("ALAN_LIVE_CHATGPT_ACCOUNT_ID") {
                config = config.with_chatgpt_account_id(account_id);
            }
            Some(LiveProviderHarness {
                label: "chatgpt",
                config,
            })
        }
        "openai_chat_completions" => {
            let Some(api_key) = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_API_KEY") else {
                eprintln!(
                    "[live-provider-harness] skipping openai_chat_completions: ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_API_KEY is unset"
                );
                return None;
            };
            let model = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_MODEL")
                .unwrap_or_else(|| "gpt-5.4".to_string());
            let mut config = ProviderConfig::openai_chat_completions(api_key, model);
            if let Some(base_url) = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPLETIONS_BASE_URL") {
                config = config.with_base_url(base_url);
            }
            Some(LiveProviderHarness {
                label: "openai_chat_completions",
                config,
            })
        }
        "openai_chat_completions_compatible" => {
            let Some(api_key) = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_API_KEY") else {
                eprintln!(
                    "[live-provider-harness] skipping openai_chat_completions_compatible: ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_API_KEY is unset"
                );
                return None;
            };
            let Some(base_url) = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_BASE_URL") else {
                eprintln!(
                    "[live-provider-harness] skipping openai_chat_completions_compatible: ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_BASE_URL is unset"
                );
                return None;
            };
            let model = non_empty_env("ALAN_LIVE_OPENAI_CHAT_COMPATIBLE_MODEL")
                .unwrap_or_else(|| "qwen3.5-plus".to_string());
            let config = ProviderConfig::openai_chat_completions_compatible(api_key, model)
                .with_base_url(base_url);
            Some(LiveProviderHarness {
                label: "openai_chat_completions_compatible",
                config,
            })
        }
        "openrouter" => {
            let Some(api_key) = non_empty_env("ALAN_LIVE_OPENROUTER_API_KEY") else {
                eprintln!(
                    "[live-provider-harness] skipping openrouter: ALAN_LIVE_OPENROUTER_API_KEY is unset"
                );
                return None;
            };
            let model = non_empty_env("ALAN_LIVE_OPENROUTER_MODEL")
                .unwrap_or_else(|| "moonshotai/kimi-k2.6".to_string());
            let mut config = ProviderConfig::openrouter(api_key, model);
            if let Some(base_url) = non_empty_env("ALAN_LIVE_OPENROUTER_BASE_URL") {
                config = config.with_base_url(base_url);
            }
            if let Some(http_referer) = non_empty_env("ALAN_LIVE_OPENROUTER_HTTP_REFERER") {
                config = config.with_http_referer(http_referer);
            }
            if let Some(x_title) = non_empty_env("ALAN_LIVE_OPENROUTER_X_TITLE") {
                config = config.with_x_title(x_title);
            }
            if let Some(app_categories) = non_empty_env("ALAN_LIVE_OPENROUTER_APP_CATEGORIES") {
                config = config.with_app_categories(
                    app_categories
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty()),
                );
            }
            Some(LiveProviderHarness {
                label: "openrouter",
                config,
            })
        }
        "anthropic_messages" => {
            let Some(api_key) = non_empty_env("ALAN_LIVE_ANTHROPIC_MESSAGES_API_KEY") else {
                eprintln!(
                    "[live-provider-harness] skipping anthropic_messages: ALAN_LIVE_ANTHROPIC_MESSAGES_API_KEY is unset"
                );
                return None;
            };
            let model = non_empty_env("ALAN_LIVE_ANTHROPIC_MESSAGES_MODEL")
                .unwrap_or_else(|| "claude-3-5-sonnet-latest".to_string());
            let mut config = ProviderConfig::anthropic_messages(api_key, model);
            if let Some(base_url) = non_empty_env("ALAN_LIVE_ANTHROPIC_MESSAGES_BASE_URL") {
                config = config.with_base_url(base_url);
            }
            if let Some(client_name) = non_empty_env("ALAN_LIVE_ANTHROPIC_MESSAGES_CLIENT_NAME") {
                config = config.with_client_name(client_name);
            }
            if let Some(user_agent) = non_empty_env("ALAN_LIVE_ANTHROPIC_MESSAGES_USER_AGENT") {
                config = config.with_user_agent(user_agent);
            }
            Some(LiveProviderHarness {
                label: "anthropic_messages",
                config,
            })
        }
        other => panic!("unsupported live harness provider: {other}"),
    }
}

fn env_truthy(name: &str) -> Option<bool> {
    non_empty_env(name).map(|value| {
        matches!(
            value.as_str(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        )
    })
}

fn exact_reply_request(token: &str) -> GenerationRequest {
    GenerationRequest::new()
        .with_system_prompt(
            "You are a protocol conformance harness. Reply with the exact token requested by the user. Do not add markdown, code fences, punctuation, or explanations.",
        )
        .with_user_message(format!("Reply with exactly: {token}"))
        // Mirror the runtime's default temperature so live-provider coverage
        // catches managed surfaces that reject generic unified request fields.
        .with_temperature(0.3)
        .with_max_tokens(64)
}

async fn create_provider(config: ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    factory::create_provider(config)
}

async fn run_basic_generation(harness: &LiveProviderHarness, token: &str) -> Result<String> {
    let capabilities = harness.config.provider_type.capabilities();
    let mut provider = create_provider(harness.config.clone())
        .await
        .with_context(|| format!("failed to construct {}", harness.label))?;
    let response = timeout(
        REQUEST_TIMEOUT,
        provider.generate(exact_reply_request(token)),
    )
    .await
    .with_context(|| format!("{} basic generation timed out", harness.label))?
    .with_context(|| format!("{} basic generation failed", harness.label))?;

    ensure!(
        response.content.contains(token),
        "{} basic generation did not contain expected token `{token}`: {:?}",
        harness.label,
        response.content
    );
    ensure!(
        response.finish_reason.is_some(),
        "{} basic generation missing finish_reason",
        harness.label
    );
    if capabilities.supports_provider_response_id {
        ensure!(
            response
                .provider_response_id
                .as_deref()
                .is_some_and(|value| !value.is_empty()),
            "{} basic generation missing provider_response_id",
            harness.label
        );
    }
    if capabilities.supports_provider_response_status {
        ensure!(
            response
                .provider_response_status
                .as_deref()
                .is_some_and(|value| !value.is_empty()),
            "{} basic generation missing provider_response_status",
            harness.label
        );
    }

    Ok(response
        .provider_response_id
        .unwrap_or_else(|| String::from("<none>")))
}

async fn collect_stream(mut rx: Receiver<StreamChunk>) -> Result<CollectedStream> {
    let deadline = Instant::now() + STREAM_TIMEOUT;
    let mut text = String::new();
    let mut saw_final_chunk = false;
    let mut final_finish_reason = None;
    let mut saw_provider_response_id = false;

    loop {
        let now = Instant::now();
        ensure!(now < deadline, "stream collection timed out");
        let remaining = deadline.saturating_duration_since(now);
        let chunk = timeout(remaining, rx.recv())
            .await
            .context("stream recv timeout")?;

        let Some(chunk) = chunk else {
            break;
        };

        if let Some(delta) = chunk.text.as_deref() {
            text.push_str(delta);
        }
        if chunk
            .provider_response_id
            .as_deref()
            .is_some_and(|value| !value.is_empty())
        {
            saw_provider_response_id = true;
        }
        if chunk.is_finished {
            saw_final_chunk = true;
            final_finish_reason = chunk.finish_reason;
            break;
        }
    }

    Ok(CollectedStream {
        text,
        saw_final_chunk,
        final_finish_reason,
        saw_provider_response_id,
    })
}

async fn run_stream_generation(harness: &LiveProviderHarness, token: &str) -> Result<()> {
    let capabilities = harness.config.provider_type.capabilities();
    let mut provider = create_provider(harness.config.clone())
        .await
        .with_context(|| format!("failed to construct {}", harness.label))?;
    let rx = timeout(
        REQUEST_TIMEOUT,
        provider.generate_stream(exact_reply_request(token)),
    )
    .await
    .with_context(|| format!("{} stream initialization timed out", harness.label))?
    .with_context(|| format!("{} stream initialization failed", harness.label))?;
    let collected = collect_stream(rx)
        .await
        .with_context(|| format!("{} stream collection failed", harness.label))?;

    ensure!(
        collected.text.contains(token),
        "{} stream output did not contain expected token `{token}`: {:?}",
        harness.label,
        collected.text
    );
    ensure!(
        collected.saw_final_chunk,
        "{} stream ended without a final chunk",
        harness.label
    );
    ensure!(
        collected.final_finish_reason.is_some(),
        "{} final stream chunk missing finish_reason",
        harness.label
    );
    if capabilities.supports_provider_response_id {
        ensure!(
            collected.saw_provider_response_id,
            "{} stream never surfaced provider_response_id",
            harness.label
        );
    }

    Ok(())
}

async fn run_responses_continuation(harness: &LiveProviderHarness) -> Result<()> {
    let capabilities = harness.config.provider_type.capabilities();
    ensure!(
        capabilities.supports_server_managed_continuation,
        "{} does not declare server-managed continuation support",
        harness.label
    );

    let initial_token = format!("ALAN_LIVE_{}_INITIAL_OK", harness.label.to_uppercase());
    let followup_token = format!("ALAN_LIVE_{}_FOLLOWUP_OK", harness.label.to_uppercase());

    let mut provider = create_provider(harness.config.clone())
        .await
        .with_context(|| format!("failed to construct {}", harness.label))?;

    let initial = timeout(
        REQUEST_TIMEOUT,
        provider.generate(exact_reply_request(&initial_token)),
    )
    .await
    .with_context(|| format!("{} continuation bootstrap timed out", harness.label))?
    .with_context(|| format!("{} continuation bootstrap failed", harness.label))?;
    ensure!(
        initial.content.contains(&initial_token),
        "{} continuation bootstrap did not contain `{initial_token}`: {:?}",
        harness.label,
        initial.content
    );
    let response_id = initial
        .provider_response_id
        .clone()
        .filter(|value| !value.is_empty())
        .context("continuation bootstrap response is missing provider_response_id")?;

    let followup_request =
        exact_reply_request(&followup_token).with_previous_response_id(response_id);
    let followup = timeout(REQUEST_TIMEOUT, provider.generate(followup_request))
        .await
        .with_context(|| format!("{} continuation follow-up timed out", harness.label))?
        .with_context(|| format!("{} continuation follow-up failed", harness.label))?;
    ensure!(
        followup.content.contains(&followup_token),
        "{} continuation follow-up did not contain `{followup_token}`: {:?}",
        harness.label,
        followup.content
    );
    ensure!(
        followup.finish_reason.is_some(),
        "{} continuation follow-up missing finish_reason",
        harness.label
    );

    Ok(())
}

fn openrouter_model_indicates_reasoning(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("kimi-k2.6") || model.contains("kimi-k2-thinking")
}

fn openrouter_model_indicates_tool_calls(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("kimi-k2.6") || model.contains("kimi-k2")
}

async fn run_openrouter_reasoning_if_supported(harness: &LiveProviderHarness) -> Result<()> {
    let should_run = env_truthy("ALAN_LIVE_OPENROUTER_REASONING")
        .unwrap_or_else(|| openrouter_model_indicates_reasoning(&harness.config.model));
    if !should_run {
        eprintln!(
            "[live-provider-harness] skipping openrouter reasoning: configured model support not asserted"
        );
        return Ok(());
    }

    let token = "ALAN_LIVE_OPENROUTER_REASONING_OK";
    let mut provider = create_provider(harness.config.clone())
        .await
        .context("failed to construct openrouter provider for reasoning check")?;
    let response = timeout(
        REQUEST_TIMEOUT,
        provider.generate(
            exact_reply_request(token)
                .with_reasoning_effort(ReasoningEffort::Minimal)
                .with_extra_param("include_reasoning", serde_json::json!(true)),
        ),
    )
    .await
    .context("openrouter reasoning generation timed out")?
    .context("openrouter reasoning generation failed")?;

    ensure!(
        response.content.contains(token),
        "openrouter reasoning generation did not contain expected token `{token}`: {:?}",
        response.content
    );
    ensure!(
        response
            .thinking
            .as_deref()
            .is_some_and(|value| !value.is_empty()),
        "openrouter reasoning generation did not surface reasoning text"
    );

    Ok(())
}

async fn run_openrouter_tool_call_if_supported(harness: &LiveProviderHarness) -> Result<()> {
    let should_run = env_truthy("ALAN_LIVE_OPENROUTER_TOOL_CALLS")
        .unwrap_or_else(|| openrouter_model_indicates_tool_calls(&harness.config.model));
    if !should_run {
        eprintln!(
            "[live-provider-harness] skipping openrouter tool calls: configured model support not asserted"
        );
        return Ok(());
    }

    let mut provider = create_provider(harness.config.clone())
        .await
        .context("failed to construct openrouter provider for tool-call check")?;
    let tool = ToolDefinition::new("record_status", "Record a required status token.")
        .with_parameters(serde_json::json!({
            "type": "object",
            "properties": {
                "token": {
                    "type": "string",
                    "description": "The exact status token."
                }
            },
            "required": ["token"],
            "additionalProperties": false
        }));
    let request = GenerationRequest::new()
        .with_system_prompt(
            "You are a tool-call conformance harness. When the user asks you to record a token, call the provided tool exactly once and do not answer in text.",
        )
        .with_user_message("Record the exact token ALAN_LIVE_OPENROUTER_TOOL_OK.")
        .with_tool(tool)
        .with_temperature(0.0)
        .with_max_tokens(128);
    let response = timeout(REQUEST_TIMEOUT, provider.generate(request))
        .await
        .context("openrouter tool-call generation timed out")?
        .context("openrouter tool-call generation failed")?;

    let tool_call = response
        .tool_calls
        .iter()
        .find(|tool_call| tool_call.name == "record_status")
        .context("openrouter did not emit the expected record_status tool call")?;
    ensure!(
        tool_call
            .arguments
            .get("token")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.contains("ALAN_LIVE_OPENROUTER_TOOL_OK")),
        "openrouter tool call did not include the expected token: {:?}",
        tool_call.arguments
    );

    Ok(())
}

async fn run_live_contract(harness: LiveProviderHarness) -> Result<()> {
    let basic_token = format!("ALAN_LIVE_{}_BASIC_OK", harness.label.to_uppercase());
    let stream_token = format!("ALAN_LIVE_{}_STREAM_OK", harness.label.to_uppercase());

    run_basic_generation(&harness, &basic_token).await?;
    run_stream_generation(&harness, &stream_token).await?;
    if harness
        .config
        .provider_type
        .capabilities()
        .supports_server_managed_continuation
    {
        run_responses_continuation(&harness).await?;
    }

    Ok(())
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and provider credentials"]
async fn live_openai_responses_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("openai_responses") else {
        return Ok(());
    };
    run_live_contract(harness).await
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and managed ChatGPT auth"]
async fn live_chatgpt_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("chatgpt") else {
        return Ok(());
    };
    run_live_contract(harness).await
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and provider credentials"]
async fn live_openai_chat_completions_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("openai_chat_completions") else {
        return Ok(());
    };
    run_live_contract(harness).await
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and compatible endpoint credentials"]
async fn live_openai_chat_completions_compatible_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("openai_chat_completions_compatible") else {
        return Ok(());
    };
    run_live_contract(harness).await
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and OpenRouter credentials"]
async fn live_openrouter_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("openrouter") else {
        return Ok(());
    };
    run_live_contract(harness.clone()).await?;
    run_openrouter_reasoning_if_supported(&harness).await?;
    run_openrouter_tool_call_if_supported(&harness).await
}

#[tokio::test]
#[ignore = "live network test; requires ALAN_LIVE_PROVIDER_TESTS=1 and provider credentials"]
async fn live_anthropic_messages_contract() -> Result<()> {
    let Some(harness) = provider_harness_or_skip("anthropic_messages") else {
        return Ok(());
    };
    run_live_contract(harness).await
}
