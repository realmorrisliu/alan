use alan_agent_engine::runtime::{
    effective_core_config_for_runtime, spawn_with_llm_client_and_tools,
};
use alan_agent_engine::{
    AlanHomePaths, Config, LlmClient, RuntimeController, RuntimeEventEnvelope, ToolRegistry,
    WorkspaceRuntimeConfig,
};
use alan_agent_protocol::{ContentPart, Event, Op, Submission};
use anyhow::{Context, Result, ensure};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

const LIVE_ENABLE_ENV: &str = "ALAN_LIVE_PROVIDER_TESTS";
const CHATGPT_AUTH_STORAGE_PATH_ENV: &str = "ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH";
const CHATGPT_BASE_URL_ENV: &str = "ALAN_LIVE_CHATGPT_BASE_URL";
const CHATGPT_MODEL_ENV: &str = "ALAN_LIVE_CHATGPT_MODEL";
const CHATGPT_ACCOUNT_ID_ENV: &str = "ALAN_LIVE_CHATGPT_ACCOUNT_ID";
const TURN_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug)]
struct CollectedTurn {
    events: Vec<Event>,
    text: String,
    warnings: Vec<String>,
    errors: Vec<String>,
    saw_turn_completed: bool,
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

fn spawn_live_runtime(
    config: WorkspaceRuntimeConfig,
    tools: ToolRegistry,
) -> Result<RuntimeController> {
    let core_config = effective_core_config_for_runtime(&config)?;
    let llm_client = LlmClient::from_core_config_with_chatgpt_auth_storage_path(
        &core_config,
        config.chatgpt_auth_storage_path.clone(),
    )?;
    spawn_with_llm_client_and_tools(config, llm_client, tools)
}

async fn collect_events_until_terminal(
    mut rx: tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>,
    timeout: Duration,
) -> CollectedTurn {
    let mut events = Vec::new();
    let mut text = String::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut saw_turn_completed = false;
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(envelope) => {
                        let event = envelope.event.clone();
                        match &event {
                            Event::TextDelta { chunk, .. } => text.push_str(chunk),
                            Event::Warning { message } => warnings.push(message.clone()),
                            Event::Error { message, recoverable } => {
                                errors.push(format!("{message} (recoverable={recoverable})"));
                            }
                            Event::TurnCompleted { .. } => {
                                saw_turn_completed = true;
                            }
                            _ => {}
                        }
                        events.push(event);

                        if saw_turn_completed || !errors.is_empty() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("[live-runtime-smoke] WARNING: lagged {n} events");
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                eprintln!(
                    "[live-runtime-smoke] TIMEOUT waiting for terminal event after {:?}",
                    timeout
                );
                break;
            }
        }
    }

    CollectedTurn {
        events,
        text,
        warnings,
        errors,
        saw_turn_completed,
    }
}

fn print_event_summary(test_name: &str, turn: &CollectedTurn) {
    eprintln!(
        "\n=== {test_name}: collected {} events, warnings={}, errors={} ===",
        turn.events.len(),
        turn.warnings.len(),
        turn.errors.len()
    );
    for (idx, event) in turn.events.iter().enumerate() {
        match event {
            Event::TurnStarted { .. } => eprintln!("  [{idx}] TurnStarted"),
            Event::TurnCompleted { .. } => eprintln!("  [{idx}] TurnCompleted"),
            Event::TextDelta { chunk, is_final } => {
                eprintln!("  [{idx}] TextDelta(is_final={is_final}): {chunk:?}");
            }
            Event::ThinkingDelta { chunk, is_final } => {
                eprintln!(
                    "  [{idx}] ThinkingDelta(is_final={is_final}): {:?}",
                    &chunk[..chunk.len().min(80)]
                );
            }
            Event::Warning { message } => eprintln!("  [{idx}] Warning: {message}"),
            Event::Error {
                message,
                recoverable,
            } => {
                eprintln!("  [{idx}] Error(recoverable={recoverable}): {message}");
            }
            other => eprintln!("  [{idx}] {:?}", std::mem::discriminant(other)),
        }
    }
    eprintln!("=== end {test_name} ===\n");
}

#[tokio::test]
#[ignore = "live runtime smoke; requires ALAN_LIVE_PROVIDER_TESTS=1 and managed ChatGPT auth"]
async fn live_chatgpt_runtime_smoke() -> Result<()> {
    if !live_enabled() {
        eprintln!("[live-runtime-smoke] skipping chatgpt: set {LIVE_ENABLE_ENV}=1 to enable");
        return Ok(());
    }

    let Some(auth_storage_path) = non_empty_env(CHATGPT_AUTH_STORAGE_PATH_ENV) else {
        eprintln!(
            "[live-runtime-smoke] skipping chatgpt: {CHATGPT_AUTH_STORAGE_PATH_ENV} is unset"
        );
        return Ok(());
    };

    let temp_home = TempDir::new().context("create temp home")?;
    let temp_workspace = TempDir::new().context("create temp workspace root")?;
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    std::fs::create_dir_all(&workspace_alan_dir).context("create workspace .alan dir")?;

    let base_url = non_empty_env(CHATGPT_BASE_URL_ENV);
    let model = non_empty_env(CHATGPT_MODEL_ENV).unwrap_or_else(|| "gpt-5.3-codex".to_string());

    let mut core_config = Config::for_chatgpt(base_url.as_deref(), Some(&model));
    if let Some(account_id) = non_empty_env(CHATGPT_ACCOUNT_ID_ENV) {
        core_config.chatgpt_account_id = Some(account_id);
    }

    let mut runtime_config = WorkspaceRuntimeConfig::from(core_config);
    runtime_config.workspace_root_dir = Some(workspace_root.clone());
    runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
    runtime_config.default_cwd_override = Some(workspace_root);
    runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp_home.path()));
    runtime_config.chatgpt_auth_storage_path = Some(PathBuf::from(auth_storage_path));

    let mut tools = ToolRegistry::new();
    if let Some(cwd) = runtime_config.default_cwd_override.clone() {
        tools.set_default_cwd(cwd);
    }

    let mut controller = spawn_live_runtime(runtime_config, tools)
        .context("spawn runtime with managed ChatGPT provider")?;
    controller
        .wait_until_ready()
        .await
        .context("runtime should become ready")?;

    let rx = controller.handle.event_sender.subscribe();
    let token = "ALAN_RUNTIME_LIVE_CHATGPT_OK";
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(format!(
                "Reply with exactly: {token}. Do not use tools, markdown, or punctuation."
            ))],
            context: None,
        }))
        .await
        .context("submit live runtime turn")?;

    let turn = collect_events_until_terminal(rx, TURN_TIMEOUT).await;
    print_event_summary("live_chatgpt_runtime_smoke", &turn);

    controller.shutdown().await.context("shutdown runtime")?;

    ensure!(
        turn.errors.is_empty(),
        "live runtime emitted unexpected errors: {:?}",
        turn.errors
    );
    ensure!(
        turn.saw_turn_completed,
        "live runtime did not reach TurnCompleted; warnings={:?}, text={:?}",
        turn.warnings,
        turn.text
    );
    ensure!(
        turn.text.contains(token),
        "live runtime text did not contain expected token `{token}`: {:?}",
        turn.text
    );

    Ok(())
}

#[tokio::test]
#[ignore = "live runtime memory smoke; requires ALAN_LIVE_PROVIDER_TESTS=1 and managed ChatGPT auth"]
async fn live_chatgpt_runtime_cross_process_memory_smoke() -> Result<()> {
    if !live_enabled() {
        eprintln!(
            "[live-runtime-smoke] skipping chatgpt memory: set {LIVE_ENABLE_ENV}=1 to enable"
        );
        return Ok(());
    }

    let Some(auth_storage_path) = non_empty_env(CHATGPT_AUTH_STORAGE_PATH_ENV) else {
        eprintln!(
            "[live-runtime-smoke] skipping chatgpt memory: {CHATGPT_AUTH_STORAGE_PATH_ENV} is unset"
        );
        return Ok(());
    };

    let temp_home = TempDir::new().context("create temp home")?;
    let temp_workspace = TempDir::new().context("create temp workspace root")?;
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    std::fs::create_dir_all(&workspace_alan_dir).context("create workspace .alan dir")?;

    let base_url = non_empty_env(CHATGPT_BASE_URL_ENV);
    let model = non_empty_env(CHATGPT_MODEL_ENV).unwrap_or_else(|| "gpt-5.3-codex".to_string());
    let persona_user_path = workspace_alan_dir.join("agents/default/persona/USER.md");
    let wrong_workspace_user_path = workspace_root.join("USER.md");
    let marker = "ALAN_LIVE_RUNTIME_MEMORY_MARKER";

    let mut core_config = Config::for_chatgpt(base_url.as_deref(), Some(&model));
    if let Some(account_id) = non_empty_env(CHATGPT_ACCOUNT_ID_ENV) {
        core_config.chatgpt_account_id = Some(account_id);
    }

    let mut first_runtime_config = WorkspaceRuntimeConfig::from(core_config.clone());
    first_runtime_config.workspace_root_dir = Some(workspace_root.clone());
    first_runtime_config.workspace_alan_dir = Some(workspace_alan_dir.clone());
    first_runtime_config.default_cwd_override = Some(workspace_root.clone());
    first_runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp_home.path()));
    first_runtime_config.chatgpt_auth_storage_path = Some(PathBuf::from(auth_storage_path.clone()));
    first_runtime_config.agent_config.runtime_config.governance =
        alan_agent_protocol::GovernanceConfig {
            profile: alan_agent_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        };

    let first_tools = alan_tools::create_tool_registry_with_core_tools(workspace_root.clone());
    let mut first_controller = spawn_live_runtime(first_runtime_config, first_tools)
        .context("spawn first live runtime with tools")?;
    first_controller
        .wait_until_ready()
        .await
        .context("first live runtime should become ready")?;

    let rx = first_controller.handle.event_sender.subscribe();
    first_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(format!(
                "Persist one stable preference across future Agent Processes. Use tools to update the exact `USER.md` write target from the workspace persona context so it contains the line `Favorite live runtime marker: {marker}`. Do not create `./USER.md` or any sibling copy. After the tool call succeeds, reply with exactly: stored:{marker}"
            ))],
            context: None,
        }))
        .await
        .context("submit live memory storage turn")?;

    let first_turn = collect_events_until_terminal(rx, TURN_TIMEOUT).await;
    print_event_summary(
        "live_chatgpt_runtime_cross_process_memory_smoke (store)",
        &first_turn,
    );
    first_controller
        .shutdown()
        .await
        .context("shutdown first live runtime")?;

    ensure!(
        first_turn.errors.is_empty(),
        "live memory store turn emitted unexpected errors: {:?}",
        first_turn.errors
    );
    ensure!(
        first_turn.saw_turn_completed,
        "live memory store turn did not reach TurnCompleted; warnings={:?}, text={:?}",
        first_turn.warnings,
        first_turn.text
    );
    ensure!(
        first_turn.text.contains(&format!("stored:{marker}")),
        "live memory store confirmation did not contain expected marker: {:?}",
        first_turn.text
    );

    let persona_user = std::fs::read_to_string(&persona_user_path).with_context(|| {
        format!(
            "read persisted persona file {}",
            persona_user_path.display()
        )
    })?;
    ensure!(
        persona_user.contains(marker),
        "persisted USER.md did not contain expected marker: {:?}",
        persona_user
    );
    ensure!(
        !wrong_workspace_user_path.exists(),
        "unexpected workspace-root USER.md duplicate was created at {}",
        wrong_workspace_user_path.display()
    );

    let mut second_runtime_config = WorkspaceRuntimeConfig::from(core_config);
    second_runtime_config.workspace_root_dir = Some(workspace_root.clone());
    second_runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
    second_runtime_config.default_cwd_override = Some(workspace_root);
    second_runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp_home.path()));
    second_runtime_config.chatgpt_auth_storage_path = Some(PathBuf::from(auth_storage_path));

    let mut second_tools = ToolRegistry::new();
    if let Some(cwd) = second_runtime_config.default_cwd_override.clone() {
        second_tools.set_default_cwd(cwd);
    }

    let mut second_controller = spawn_live_runtime(second_runtime_config, second_tools)
        .context("spawn second live runtime")?;
    second_controller
        .wait_until_ready()
        .await
        .context("second live runtime should become ready")?;

    let rx = second_controller.handle.event_sender.subscribe();
    second_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "What is my favorite live runtime marker? Reply with exactly the saved marker and nothing else.",
            )],
            context: None,
        }))
        .await
        .context("submit live memory recall turn")?;

    let second_turn = collect_events_until_terminal(rx, TURN_TIMEOUT).await;
    print_event_summary(
        "live_chatgpt_runtime_cross_process_memory_smoke (recall)",
        &second_turn,
    );
    second_controller
        .shutdown()
        .await
        .context("shutdown second live runtime")?;

    ensure!(
        second_turn.errors.is_empty(),
        "live memory recall turn emitted unexpected errors: {:?}",
        second_turn.errors
    );
    ensure!(
        second_turn.saw_turn_completed,
        "live memory recall turn did not reach TurnCompleted; warnings={:?}, text={:?}",
        second_turn.warnings,
        second_turn.text
    );
    ensure!(
        second_turn.text.contains(marker),
        "live memory recall text did not contain expected marker `{marker}`: {:?}",
        second_turn.text
    );

    Ok(())
}

#[tokio::test]
#[ignore = "live runtime continuity smoke; requires ALAN_LIVE_PROVIDER_TESTS=1 and managed ChatGPT auth"]
async fn live_chatgpt_runtime_cross_process_continuity_smoke() -> Result<()> {
    if !live_enabled() {
        eprintln!(
            "[live-runtime-smoke] skipping chatgpt continuity: set {LIVE_ENABLE_ENV}=1 to enable"
        );
        return Ok(());
    }

    let Some(auth_storage_path) = non_empty_env(CHATGPT_AUTH_STORAGE_PATH_ENV) else {
        eprintln!(
            "[live-runtime-smoke] skipping chatgpt continuity: {CHATGPT_AUTH_STORAGE_PATH_ENV} is unset"
        );
        return Ok(());
    };

    let temp_home = TempDir::new().context("create temp home")?;
    let temp_workspace = TempDir::new().context("create temp workspace root")?;
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    std::fs::create_dir_all(&workspace_alan_dir).context("create workspace .alan dir")?;

    let base_url = non_empty_env(CHATGPT_BASE_URL_ENV);
    let model = non_empty_env(CHATGPT_MODEL_ENV).unwrap_or_else(|| "gpt-5.3-codex".to_string());
    let continuity_marker = "continuity:ALAN_LIVE_RUNTIME_HANDOFF_MARKER";

    let mut core_config = Config::for_chatgpt(base_url.as_deref(), Some(&model));
    if let Some(account_id) = non_empty_env(CHATGPT_ACCOUNT_ID_ENV) {
        core_config.chatgpt_account_id = Some(account_id);
    }

    let mut first_runtime_config = WorkspaceRuntimeConfig::from(core_config.clone());
    first_runtime_config.workspace_root_dir = Some(workspace_root.clone());
    first_runtime_config.workspace_alan_dir = Some(workspace_alan_dir.clone());
    first_runtime_config.default_cwd_override = Some(workspace_root.clone());
    first_runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp_home.path()));
    first_runtime_config.chatgpt_auth_storage_path = Some(PathBuf::from(auth_storage_path.clone()));

    let mut first_tools = ToolRegistry::new();
    if let Some(cwd) = first_runtime_config.default_cwd_override.clone() {
        first_tools.set_default_cwd(cwd);
    }

    let mut first_controller = spawn_live_runtime(first_runtime_config, first_tools)
        .context("spawn first continuity runtime")?;
    first_controller
        .wait_until_ready()
        .await
        .context("first continuity runtime should become ready")?;

    let rx = first_controller.handle.event_sender.subscribe();
    first_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(format!(
                "Reply with exactly {continuity_marker}. Do not use tools, markdown, or punctuation."
            ))],
            context: None,
        }))
        .await
        .context("submit live continuity seed turn")?;

    let first_turn = collect_events_until_terminal(rx, TURN_TIMEOUT).await;
    print_event_summary(
        "live_chatgpt_runtime_cross_process_continuity_smoke (seed)",
        &first_turn,
    );
    first_controller
        .shutdown()
        .await
        .context("shutdown first continuity runtime")?;

    ensure!(
        first_turn.errors.is_empty(),
        "live continuity seed turn emitted unexpected errors: {:?}",
        first_turn.errors
    );
    ensure!(
        first_turn.saw_turn_completed,
        "live continuity seed turn did not reach TurnCompleted; warnings={:?}, text={:?}",
        first_turn.warnings,
        first_turn.text
    );
    ensure!(
        first_turn.text.contains(continuity_marker),
        "live continuity seed turn did not contain marker `{continuity_marker}`: {:?}",
        first_turn.text
    );

    let handoff_path = alan_agent_engine::workspace_runtime_memory_dir_from_alan_dir(
        &workspace_alan_dir,
        alan_agent_engine::InstallChannel::Stable,
    )
    .join("handoffs/LATEST.md");
    let latest_handoff = std::fs::read_to_string(&handoff_path)
        .with_context(|| format!("read {}", handoff_path.display()))?;
    ensure!(
        latest_handoff.contains(continuity_marker),
        "latest handoff did not preserve continuity marker: {:?}",
        latest_handoff
    );

    let mut second_runtime_config = WorkspaceRuntimeConfig::from(core_config);
    second_runtime_config.workspace_root_dir = Some(workspace_root.clone());
    second_runtime_config.workspace_alan_dir = Some(workspace_alan_dir.clone());
    second_runtime_config.default_cwd_override = Some(workspace_root);
    second_runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp_home.path()));
    second_runtime_config.chatgpt_auth_storage_path = Some(PathBuf::from(auth_storage_path));

    let mut second_tools = ToolRegistry::new();
    if let Some(cwd) = second_runtime_config.default_cwd_override.clone() {
        second_tools.set_default_cwd(cwd);
    }

    let mut second_controller = spawn_live_runtime(second_runtime_config, second_tools)
        .context("spawn second continuity runtime")?;
    second_controller
        .wait_until_ready()
        .await
        .context("second continuity runtime should become ready")?;

    let rx = second_controller.handle.event_sender.subscribe();
    second_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "What was the last continuity marker from the previous Agent Process? Reply with exactly the saved marker and nothing else.",
            )],
            context: None,
        }))
        .await
        .context("submit live continuity recall turn")?;

    let second_turn = collect_events_until_terminal(rx, TURN_TIMEOUT).await;
    print_event_summary(
        "live_chatgpt_runtime_cross_process_continuity_smoke (recall)",
        &second_turn,
    );
    second_controller
        .shutdown()
        .await
        .context("shutdown second continuity runtime")?;

    ensure!(
        second_turn.errors.is_empty(),
        "live continuity recall turn emitted unexpected errors: {:?}",
        second_turn.errors
    );
    ensure!(
        second_turn.saw_turn_completed,
        "live continuity recall turn did not reach TurnCompleted; warnings={:?}, text={:?}",
        second_turn.warnings,
        second_turn.text
    );
    ensure!(
        second_turn.text.contains(continuity_marker),
        "live continuity recall text did not contain expected marker `{continuity_marker}`: {:?}",
        second_turn.text
    );

    Ok(())
}
