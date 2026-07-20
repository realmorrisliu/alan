use alan_agent_engine::runtime::effective_core_config_for_runtime;
use alan_agent_engine::{
    AgentProcessConfig, AgentRuntimeStoreBindings, Config, LlmClient, ToolRegistry,
};
use alan_agent_protocol::{UiActivityState, UiEvent};
use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use anyhow::{Context, Result, ensure};
use std::{env, path::PathBuf, time::Duration};
use tempfile::TempDir;

const LIVE_ENABLE_ENV: &str = "ALAN_LIVE_PROVIDER_TESTS";
const CHATGPT_AUTH_STORAGE_PATH_ENV: &str = "ALAN_LIVE_CHATGPT_AUTH_STORAGE_PATH";
const CHATGPT_BASE_URL_ENV: &str = "ALAN_LIVE_CHATGPT_BASE_URL";
const CHATGPT_MODEL_ENV: &str = "ALAN_LIVE_CHATGPT_MODEL";
const CHATGPT_ACCOUNT_ID_ENV: &str = "ALAN_LIVE_CHATGPT_ACCOUNT_ID";

fn non_empty_env(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn wait_for_idle(tail: &mut alan_shell::Tail) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut pending = String::new();
        let mut saw_running = false;
        loop {
            pending
                .push_str(&String::from_utf8(tail.read(4096).await?).context("decode UI stream")?);
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].to_string();
                pending.drain(..=newline);
                let event: UiEvent = serde_json::from_str(&line).context("decode UI event")?;
                if let UiEvent::Activity { snapshot } = event {
                    saw_running |= snapshot.state == UiActivityState::Running;
                    if saw_running && snapshot.state == UiActivityState::Idle {
                        return Ok::<_, anyhow::Error>(());
                    }
                }
            }
        }
    })
    .await
    .context("live turn did not reach idle")?
}

#[tokio::test]
#[ignore = "live runtime smoke; requires ALAN_LIVE_PROVIDER_TESTS=1 and managed ChatGPT auth"]
async fn live_chatgpt_runtime_smoke_uses_agentfs() -> Result<()> {
    if !env::var(LIVE_ENABLE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
    {
        return Ok(());
    }
    let Some(auth_storage_path) = non_empty_env(CHATGPT_AUTH_STORAGE_PATH_ENV) else {
        return Ok(());
    };

    let system_store = TempDir::new().context("create temporary System Store")?;
    let store_bindings = AgentRuntimeStoreBindings {
        rollouts: system_store.path().join("rollouts"),
        checkpoints: system_store.path().join("checkpoints"),
        cache: system_store.path().join("cache"),
        tmp: system_store.path().join("tmp"),
        metadata: system_store.path().join("metadata"),
    };
    for path in [
        &store_bindings.rollouts,
        &store_bindings.checkpoints,
        &store_bindings.cache,
        &store_bindings.tmp,
        &store_bindings.metadata,
    ] {
        std::fs::create_dir_all(path)?;
    }

    let model = non_empty_env(CHATGPT_MODEL_ENV).unwrap_or_else(|| "gpt-5.3-codex".to_string());
    let mut core_config =
        Config::for_chatgpt(non_empty_env(CHATGPT_BASE_URL_ENV).as_deref(), Some(&model));
    core_config.chatgpt_account_id = non_empty_env(CHATGPT_ACCOUNT_ID_ENV);

    let mut config = AgentProcessConfig::from(core_config);
    config.store_bindings = Some(store_bindings);
    let effective = effective_core_config_for_runtime(&config)?;
    let client = LlmClient::from_core_config_with_chatgpt_auth_storage_path(
        &effective,
        Some(PathBuf::from(auth_storage_path)),
    )?;
    let mut tools = ToolRegistry::new();
    alan_tools::register_builtin_tool_catalog(&mut tools);
    for tool in alan_tools::create_core_tools() {
        tools.register_boxed(tool);
    }
    let runtime = TempDir::new().context("create Host runtime directory")?;
    let endpoint = HostEndpointPaths::from_runtime_dir(runtime.path(), "test")?;
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral("test", config, client, tools),
        endpoint.clone(),
    )
    .await?;
    let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
    let host_task = tokio::spawn(host.serve_until(async move {
        let _ = shutdown_rx.await;
    }));
    let attachment = LocalAttachment::new(endpoint).connect().await?;
    let shell = alan_shell::Shell::new(attachment.root);
    let mut events = shell.tail("/agent/root/machine/ui/events").await?;

    let token = "ALAN_RUNTIME_LIVE_CHATGPT_OK";
    shell
        .write(
            "/agent/root/io/input",
            format!("Reply with exactly: {token}. Do not use tools, markdown, or punctuation.")
                .as_bytes(),
        )
        .await?;
    wait_for_idle(&mut events).await?;
    let output = String::from_utf8(shell.cat("/agent/root/io/output").await?)?;
    let _ = shutdown.send(());
    host_task.await??;
    ensure!(
        output.contains(token),
        "live output did not contain {token}: {output:?}"
    );
    Ok(())
}
