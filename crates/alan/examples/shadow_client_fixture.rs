//! Isolated qualification host: real Shell clients and evaluator, no Host tools.
//! Run via the harness; this is not a shipped CLI mode or a generation benchmark.
use std::{io::Read, path::PathBuf, sync::Arc, time::Duration};

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry, runtime::EvaluationSurface};
use alan_ap::InProcessTransport;
use alan_llm::{MockLlmProvider, TypesafeEvaluationClient};
use alan_service_manager::{
    ConnectionStoreBindings, ConnectionsFile, InputShadowSelection, LlmClientFactory,
    ProcessLaunchContext, ServiceManager, ServiceManagerConfig,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// Keep credentials out of Debug output, including on failed fixture startup.
struct Factory;
impl std::fmt::Debug for Factory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QualificationFactory")
    }
}
impl LlmClientFactory for Factory {
    fn create(
        &self,
        _: &alan_agent_engine::Config,
        selected: Option<&str>,
        _: &ConnectionsFile,
    ) -> Result<LlmClient> {
        match selected {
            Some("evaluation") => Ok(LlmClient::new(TypesafeEvaluationClient::new(
                std::env::var("TYPESAFE_API_KEY")?,
                "jev-1.13.0".into(),
            )?)),
            Some("main") => Ok(LlmClient::new(MockLlmProvider::new())),
            _ => anyhow::bail!("unknown qualification profile"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3,
        "usage: shadow_client_fixture interactive|redirected REPORT"
    );
    let surface = match args[1].as_str() {
        "interactive" => EvaluationSurface::Interactive,
        "redirected" => EvaluationSurface::Redirected,
        _ => anyhow::bail!("invalid surface"),
    };
    let report = PathBuf::from(&args[2]);
    ensure!(!report.exists(), "report must not already exist");
    let temp = tempfile::tempdir()?;
    let metadata = temp.path().join("connections.toml");
    let connections: ConnectionsFile = serde_json::from_value(json!({
        "version":1,"default_profile":"main",
        "profiles":{
            "main":{"provider":"openai_responses","credential_id":"main-key","created_at":"2026-10-07T00:00:00Z","updated_at":"2026-10-07T00:00:00Z","source":"managed","settings":{"model":"gpt-5.4"}},
            "evaluation":{"provider":"typesafe","credential_id":"eval-key","created_at":"2026-10-07T00:00:00Z","updated_at":"2026-10-07T00:00:00Z","source":"managed","settings":{"model":"jev-1.13.0"}}
        },
        "credentials":{
            "main-key":{"kind":"secret_string","provider_family":"openai_responses","label":"fixture","backend":"host_credential_store"},
            "eval-key":{"kind":"secret_string","provider_family":"typesafe","label":"fixture","backend":"host_credential_store"}
        }
    }))?;
    connections.save_to_path(&metadata)?;
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.process.agent_config.core_config.memory.enabled = false;
    config.connection_store = Some(ConnectionStoreBindings::new(metadata)?);
    config.llm_factory = Arc::new(Factory);
    config.input_shadow = Some(InputShadowSelection {
        profile: "evaluation".into(),
        surface: surface.clone(),
    });
    config.process.store_bindings = Some(alan_agent_engine::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("runtime-metadata"),
    });
    let manager = ServiceManager::boot(config).await?;
    let result = async {
        let (_, _, namespace) = manager.local_entry().create_and_handoff().await?;
        let root = InProcessTransport::new(namespace);
        let shell = alan_shell::Shell::new(root.clone());
        // Tool packages are mounted from the empty registry. Verify the executable
        // and Host project are absent before admitting any corpus input.
        ensure!(
            shell.cat("/lib/exec/bash/manifest").await.is_err(),
            "unexpected bash capability"
        );
        ensure!(
            shell.ls("/mnt/project").await.is_err(),
            "unexpected project mount"
        );
        let observer = tokio::spawn(async move {
            let observed = tokio::time::timeout(Duration::from_secs(45), async {
                loop {
                    let projection: Value = serde_json::from_slice(
                        &shell.cat("/agent/root/machine/evaluation").await?,
                    )?;
                    let observation = &projection["observation"];
                    if let Some(id) = observation["identity"]["submission_id"].as_str() {
                        let events = shell.cat("/agent/root/machine/ui/events").await?;
                        for line in events.split(|b| *b == b'\n').filter(|s| !s.is_empty()) {
                            let event: Value = serde_json::from_slice(line)?;
                            if event["type"] == "input_completed"
                                && event["submission_ids"]
                                    .as_array()
                                    .is_some_and(|ids| ids.iter().any(|v| v == id))
                            {
                                // Refresh after the completion receipt; a pre-receipt
                                // snapshot can race terminal publication. Retain even
                                // unsettled evidence for the collector to reject.
                                let settled: Value = serde_json::from_slice(
                                    &shell.cat("/agent/root/machine/evaluation").await?,
                                )?;
                                let observation = &settled["observation"];
                                ensure!(
                                    observation["identity"]["submission_id"] == id,
                                    "evaluation identity changed"
                                );
                                save_report(&report, observation, &event)?;
                                return Ok::<_, anyhow::Error>(());
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            if observed.is_err() {
                let projection: Value =
                    serde_json::from_slice(&shell.cat("/agent/root/machine/evaluation").await?)?;
                save_report(&report, &projection["observation"], &Value::Null)?;
                return Ok(());
            }
            observed?
        });
        let client_result = match surface {
            EvaluationSurface::Interactive => {
                alan_tui::run_file_backed(alan_tui::FileBackedRunConfig::new(root, "/agent/root"))
                    .await
            }
            EvaluationSurface::Redirected => {
                let mut input = String::new();
                std::io::stdin().read_to_string(&mut input)?;
                alan_tui::run_stdio_task(root, "/agent/root", &input, std::future::pending())
                    .await
                    .map(|_| ())
            }
        };
        let observed = observer.await?;
        client_result?;
        observed
    }
    .await;
    let shutdown = manager.shutdown().await;
    result?;
    shutdown
}

fn save_report(report: &std::path::Path, observation: &Value, completion: &Value) -> Result<()> {
    let source_hash: String = Sha256::digest(include_bytes!("shadow_client_fixture.rs"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let document = json!({"version":1,"observation":observation,"completion":completion,
        "generation":"fixed_mock","host_tools":[],"host_project_mounted":false,
        "fixture_source_sha256":source_hash});
    let staged = report.with_extension("partial");
    std::fs::write(&staged, serde_json::to_vec_pretty(&document)?)?;
    std::fs::rename(staged, report)?;
    Ok(())
}
