//! Isolated qualification host: real Shell clients and evaluator, no Host tools.
//! Run via the harness; this is not a shipped CLI mode or a generation benchmark.
use std::{io::Read, path::PathBuf, sync::Arc, time::Duration};

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry, runtime::EvaluationSurface};
use alan_ap::InProcessTransport;
use alan_llm::{GenerationResponse, MockLlmProvider, ToolCall, TypesafeEvaluationClient};
use alan_service_manager::{
    ConnectionStoreBindings, ConnectionsFile, InputShadowSelection, LlmClientFactory,
    ProcessLaunchContext, ServiceManager, ServiceManagerConfig,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// Keep credentials out of Debug output, including on failed fixture startup.
struct Factory {
    generation: MockLlmProvider,
}
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
            Some("main") => Ok(LlmClient::new(self.generation.clone())),
            _ => anyhow::bail!("unknown qualification profile"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 3 || (args.len() == 4 && args[3] == "--pending"),
        "usage: shadow_client_fixture interactive|redirected REPORT [--pending]"
    );
    let surface = match args[1].as_str() {
        "interactive" => EvaluationSurface::Interactive,
        "redirected" => EvaluationSurface::Redirected,
        _ => anyhow::bail!("invalid surface"),
    };
    let pending = args.len() == 4;
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
    let generation = if pending {
        MockLlmProvider::new().with_responses(vec![GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: vec![],
            usage: None,
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: vec![],
            tool_calls: vec![ToolCall {
                id: Some("qualification-response".into()),
                name: "request_user_input".into(),
                arguments: json!({
                    "title":"QUALIFICATION_RESPONSE", "prompt":"Enter the literal response",
                    "questions":[{"id":"response","label":"QUALIFICATION_RESPONSE",
                        "prompt":"Enter the literal response","kind":"text","required":true}]
                }),
            }],
        }])
    } else {
        MockLlmProvider::new()
    };
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(generation.clone()),
        ToolRegistry::new(),
    );
    config.process.agent_config.core_config.memory.enabled = false;
    config.connection_store = Some(ConnectionStoreBindings::new(metadata)?);
    config.llm_factory = Arc::new(Factory { generation });
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
        let observer_shell = shell.clone();
        let observer_report = report.clone();
        let observer = tokio::spawn(async move {
            let shell = observer_shell;
            let report = observer_report;
            let mut response_request = None;
            let observed = tokio::time::timeout(Duration::from_secs(45), async {
                loop {
                    let projection: Value = serde_json::from_slice(
                        &shell.cat("/agent/root/machine/evaluation").await?,
                    )?;
                    let observation = &projection["observation"];
                    if let Some(id) = observation["identity"]["submission_id"].as_str() {
                        if pending
                            && response_request.is_none()
                            && !id.starts_with("response:")
                            && let Some(mut request) = pending_request(&shell).await?
                        {
                            request["prelude_submission_id"] = json!(id);
                            std::fs::write(
                                report.with_extension("pending.json"),
                                serde_json::to_vec(&request)?,
                            )?;
                            response_request = Some(request);
                        }
                        if pending && !id.starts_with("response:") {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            continue;
                        }
                        let completion_id = response_request
                            .as_ref()
                            .and_then(|r| r["prelude_submission_id"].as_str())
                            .unwrap_or(id);
                        let events = shell.cat("/agent/root/machine/ui/events").await?;
                        for line in events.split(|b| *b == b'\n').filter(|s| !s.is_empty()) {
                            let event: Value = serde_json::from_slice(line)?;
                            if event["type"] == "input_completed"
                                && event["submission_ids"]
                                    .as_array()
                                    .is_some_and(|ids| ids.iter().any(|v| v == completion_id))
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
                                if let Some(request) = &mut response_request {
                                    let request_id =
                                        request["request_id"].as_str().unwrap().to_owned();
                                    ensure!(
                                        id == format!("response:{request_id}"),
                                        "response identity changed"
                                    );
                                    request["body"] = json!(String::from_utf8(
                                        shell
                                            .cat(&format!(
                                                "/agent/root/requests/{request_id}/response"
                                            ))
                                            .await?
                                    )?);
                                    request["status"] = json!(String::from_utf8(
                                        shell
                                            .cat(&format!(
                                                "/agent/root/requests/{request_id}/status"
                                            ))
                                            .await?
                                    )?);
                                }
                                save_report(
                                    &report,
                                    observation,
                                    &event,
                                    response_request.as_ref(),
                                )?;
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
                save_report(
                    &report,
                    &projection["observation"],
                    &Value::Null,
                    response_request.as_ref(),
                )?;
                return Ok(());
            }
            observed?
        });
        let redirected = surface == EvaluationSurface::Redirected;
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
        if pending && redirected {
            observer.abort();
            let _ = observer.await;
            let error = client_result.expect_err("redirected pending task unexpectedly completed");
            ensure!(
                error.to_string().contains("needs interactive input"),
                "unexpected redirected failure"
            );
            let request = pending_request(&shell)
                .await?
                .context("Machine did not create a pending request")?;
            std::fs::write(
                &report,
                serde_json::to_vec_pretty(&json!({
                    "version":1,"unsupported":true,"client_error":"needs interactive input",
                    "request":request,"fixture_source_sha256":source_hash()
                }))?,
            )?;
            return Ok(());
        }
        let observed = observer.await?;
        client_result?;
        observed
    }
    .await;
    let shutdown = manager.shutdown().await;
    result?;
    shutdown
}

fn source_hash() -> String {
    Sha256::digest(include_bytes!("shadow_client_fixture.rs"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn save_report(
    report: &std::path::Path,
    observation: &Value,
    completion: &Value,
    response: Option<&Value>,
) -> Result<()> {
    let document = json!({"version":1,"observation":observation,"completion":completion,
        "generation":"fixed_mock","host_tools":[],"host_project_mounted":false,
        "fixture_source_sha256":source_hash(),"response_request":response});
    let staged = report.with_extension("partial");
    std::fs::write(&staged, serde_json::to_vec_pretty(&document)?)?;
    std::fs::rename(staged, report)?;
    Ok(())
}

async fn pending_request(shell: &alan_shell::Shell) -> Result<Option<Value>> {
    for id in shell.ls("/agent/root/requests").await? {
        let path = format!("/agent/root/requests/{id}");
        if shell.cat(&format!("{path}/kind")).await.ok().as_deref() == Some(b"structured_input")
            && shell.cat(&format!("{path}/status")).await.ok().as_deref() == Some(b"pending")
        {
            return Ok(Some(
                json!({"request_id":id,"kind":"structured_input","status":"pending"}),
            ));
        }
    }
    Ok(None)
}
