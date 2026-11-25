use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_engine::{
    AgentProcessConfig, AgentRuntimeStoreBindings, LlmClient, ProjectStartupContext, ToolRegistry,
    WorkstreamState, bind_project_dir, load_workstream_record,
};
use alan_agent_protocol::{UiActivityState, UiEvent};
use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, MessageRole, StreamChunk, TokenUsage,
    ToolCall, ToolCallDelta,
};
use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use anyhow::{Context, Result};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::{Notify, mpsc};

#[derive(Debug, Default)]
struct AsyncHarnessShared {
    entered_a: Notify,
    entered_b: Notify,
    gate_a: Notify,
    gate_b: Notify,
    root_tool_outputs: Mutex<Vec<String>>,
    root_turn2_system_prompt: Mutex<Option<String>>,
    seen_requests: Mutex<Vec<String>>,
}

#[derive(Debug, Clone)]
struct AsyncWorkstreamMockProvider {
    shared: Arc<AsyncHarnessShared>,
}

fn spawn_text_stream(text: String) -> mpsc::Receiver<StreamChunk> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        let _ = tx
            .send(StreamChunk {
                text: Some(text),
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: None,
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: false,
                finish_reason: None,
            })
            .await;
        let _ = tx
            .send(StreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    cached_prompt_tokens: None,
                    completion_tokens: 5,
                    total_tokens: 15,
                    reasoning_tokens: None,
                    cache_creation_tokens: None,
                    cost_usd: None,
                }),
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("stop".to_string()),
            })
            .await;
    });
    rx
}

fn spawn_tool_calls_stream(calls: Vec<ToolCall>) -> mpsc::Receiver<StreamChunk> {
    let (tx, rx) = mpsc::channel(8);
    tokio::spawn(async move {
        for (index, tool_call) in calls.into_iter().enumerate() {
            let arguments =
                serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".into());
            let _ = tx
                .send(StreamChunk {
                    text: None,
                    thinking: None,
                    thinking_signature: None,
                    redacted_thinking: None,
                    usage: None,
                    provider_response_id: None,
                    provider_response_status: None,
                    sequence_number: None,
                    tool_call_delta: Some(ToolCallDelta {
                        index,
                        id: tool_call.id,
                        name: Some(tool_call.name),
                        arguments_delta: Some(arguments.clone()),
                        arguments: Some(arguments),
                    }),
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
        }
        let _ = tx
            .send(StreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    cached_prompt_tokens: None,
                    completion_tokens: 5,
                    total_tokens: 15,
                    reasoning_tokens: None,
                    cache_creation_tokens: None,
                    cost_usd: None,
                }),
                provider_response_id: None,
                provider_response_status: None,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some("tool_calls".to_string()),
            })
            .await;
    });
    rx
}

#[async_trait::async_trait]
impl LlmProvider for AsyncWorkstreamMockProvider {
    async fn generate(&mut self, _request: GenerationRequest) -> Result<GenerationResponse> {
        Ok(GenerationResponse {
            content: "sync mock".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: None,
            finish_reason: Some("stop".to_string()),
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        })
    }

    async fn chat(&mut self, _system: Option<&str>, user: &str) -> Result<String> {
        Ok(format!("mock chat: {user}"))
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>> {
        self.shared
            .seen_requests
            .lock()
            .expect("seen lock")
            .push(serde_json::to_string(&request.messages).unwrap_or_default());
        let has_user = |needle: &str| {
            request.messages.iter().any(|m| {
                m.role == MessageRole::User
                    && (m.content.contains(needle)
                        || m.content_parts
                            .iter()
                            .any(|p| format!("{p:?}").contains(needle)))
            })
        };

        if !has_user("ROOT_START_RESEARCH") && has_user("TASK_A_CHILD") {
            let shared = self.shared.clone();
            let (tx, rx) = mpsc::channel(8);
            tokio::spawn(async move {
                shared.entered_a.notify_one();
                tokio::select! {
                    _ = shared.gate_a.notified() => {
                        let _ = tx.send(StreamChunk {
                            text: Some("CHILD_A_DONE".to_string()),
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: false,
                            finish_reason: None,
                        }).await;
                        let _ = tx.send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: Some("stop".to_string()),
                        }).await;
                    }
                    _ = tx.closed() => {}
                }
            });
            return Ok(rx);
        }

        if !has_user("ROOT_START_RESEARCH") && has_user("TASK_B_CHILD") {
            let shared = self.shared.clone();
            let (tx, rx) = mpsc::channel(8);
            tokio::spawn(async move {
                shared.entered_b.notify_one();
                tokio::select! {
                    _ = shared.gate_b.notified() => {
                        let _ = tx.send(StreamChunk {
                            text: Some("CHILD_B_DONE".to_string()),
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: false,
                            finish_reason: None,
                        }).await;
                        let _ = tx.send(StreamChunk {
                            text: None,
                            thinking: None,
                            thinking_signature: None,
                            redacted_thinking: None,
                            usage: None,
                            provider_response_id: None,
                            provider_response_status: None,
                            sequence_number: None,
                            tool_call_delta: None,
                            is_finished: true,
                            finish_reason: Some("stop".to_string()),
                        }).await;
                    }
                    _ = tx.closed() => {}
                }
            });
            return Ok(rx);
        }

        if has_user("ROOT_INTERACTIVE_PING") {
            *self
                .shared
                .root_turn2_system_prompt
                .lock()
                .expect("system prompt lock") = request.system_prompt;
            return Ok(spawn_text_stream("ROOT_INTERACTIVE_OK".to_string()));
        }

        let tool_outputs: Vec<String> = request
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .map(|m| m.content.clone())
            .collect();
        if !tool_outputs.is_empty() {
            *self
                .shared
                .root_tool_outputs
                .lock()
                .expect("tool outputs lock") = tool_outputs;
            return Ok(spawn_text_stream("ROOT_DISPATCHED_AND_IDLE".to_string()));
        }

        Ok(spawn_tool_calls_stream(vec![
            ToolCall {
                id: Some("call_a".to_string()),
                name: "spawn_subagent".to_string(),
                arguments: json!({
                    "role": "researcher",
                    "task": "TASK_A_CHILD audit auth flow",
                    "read_only": true,
                    "stream_name": "research-a",
                    "persistence": "persistent",
                    "scope": "Audit auth flow",
                    "background": true
                }),
            },
            ToolCall {
                id: Some("call_b".to_string()),
                name: "spawn_subagent".to_string(),
                arguments: json!({
                    "role": "researcher",
                    "task": "TASK_B_CHILD inspect lint rules",
                    "read_only": true,
                    "stream_name": "research-b",
                    "persistence": "one_off",
                    "scope": "Inspect lint rules",
                    "background": true
                }),
            },
        ]))
    }

    fn provider_name(&self) -> &'static str {
        "async-workstream-mock"
    }
}

async fn wait_for_idle_turn(tail: &mut alan_shell::Tail) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
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
    .context("turn did not reach idle")?
}

#[tokio::test]
async fn root_dispatches_concurrent_async_workstreams_stays_interactive_and_flushes_on_exit()
-> Result<()> {
    let temp = TempDir::new().context("create temp dir")?;
    let system_store = temp.path().join("system-store");
    let projects_root = system_store.join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo)?;
    let binding = bind_project_dir(&projects_root, &repo, None)?;
    let startup_ctx = ProjectStartupContext::resolve_and_load_read_only(&projects_root, &repo)?;

    let store_bindings = AgentRuntimeStoreBindings {
        rollouts: system_store.join("rollouts"),
        checkpoints: system_store.join("checkpoints"),
        cache: system_store.join("cache"),
        tmp: system_store.join("tmp"),
        metadata: system_store.join("metadata"),
    };
    for dir in [
        &store_bindings.rollouts,
        &store_bindings.checkpoints,
        &store_bindings.cache,
        &store_bindings.tmp,
        &store_bindings.metadata,
    ] {
        std::fs::create_dir_all(dir)?;
    }

    let process_config = AgentProcessConfig {
        store_bindings: Some(store_bindings),
        ..Default::default()
    };

    let shared = Arc::new(AsyncHarnessShared::default());
    let provider = AsyncWorkstreamMockProvider {
        shared: shared.clone(),
    };
    let client = LlmClient::new(provider);
    let mut tools = ToolRegistry::new();
    alan_tools::register_builtin_tool_catalog(&mut tools);
    for tool in alan_tools::create_core_tools() {
        tools.register_boxed(tool);
    }

    let runtime_dir = TempDir::new().context("create runtime endpoint dir")?;
    let endpoint = HostEndpointPaths::from_runtime_dir(runtime_dir.path())?;
    let boot_config = HostBootConfig::ephemeral(process_config, client, tools)
        .with_default_root_agent_definition()?
        .with_project_startup_context(startup_ctx);
    let host = AlanOsHost::boot(boot_config, endpoint.clone()).await?;
    let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
    let host_task = tokio::spawn(host.serve_until(async move {
        let _ = shutdown_rx.await;
    }));

    let attachment = LocalAttachment::new(endpoint).connect().await?;
    let shell = alan_shell::Shell::new(attachment.root);
    let mut events = shell.tail("/agent/root/machine/ui/events").await?;

    // Turn 1: Root dispatches research-a (persistent) & research-b (one_off) with background=true
    // and returns immediately while both children stay blocked inside their LLM generation.
    shell
        .write("/agent/root/io/input", b"ROOT_START_RESEARCH")
        .await?;
    wait_for_idle_turn(&mut events).await?;
    let output_turn1 = String::from_utf8(shell.cat("/agent/root/io/output").await?)?;
    assert!(
        output_turn1.contains("ROOT_DISPATCHED_AND_IDLE"),
        "expected Root to finish dispatch turn while children run: {output_turn1}"
    );

    // Wait until both background child Agent Processes are live inside their generation streams.
    let tool_outputs_snapshot = shared
        .root_tool_outputs
        .lock()
        .expect("tool outputs lock")
        .clone();
    if let Err(err) = tokio::time::timeout(Duration::from_secs(10), async {
        shared.entered_a.notified().await;
        shared.entered_b.notified().await;
    })
    .await
    {
        let seen = shared.seen_requests.lock().expect("seen lock").clone();
        let proc9 = shell
            .cat("/proc/9/status")
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        let action0 = shell
            .cat("/agent/root/actions/a0/result")
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        let action1 = shell
            .cat("/agent/root/actions/a1/result")
            .await
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        let rec_a = load_workstream_record(&projects_root, &binding.project_key, "research-a").ok();
        anyhow::bail!(
            "both async child workstreams did not enter generation ({err}); seen={seen:?}; proc9={proc9:?}; action0={action0:?}; action1={action1:?}; rec_a={rec_a:?}; tool_outputs={tool_outputs_snapshot:?}"
        );
    }

    let tool_outputs = shared
        .root_tool_outputs
        .lock()
        .expect("tool outputs lock")
        .clone();
    assert_eq!(tool_outputs.len(), 2);
    let mut parsed_pids = Vec::new();
    for raw in &tool_outputs {
        let val: serde_json::Value = serde_json::from_str(raw)?;
        assert_eq!(val["status"], "running");
        assert_eq!(val["background"], true);
        let child_pid = val["child_pid"].as_str().expect("child_pid").to_string();
        let child_agent_path = val["child_agent_path"]
            .as_str()
            .expect("child_agent_path")
            .to_string();
        assert_eq!(child_agent_path, format!("/agent/{child_pid}"));
        // Verify /proc/<pid> and /agent/<pid> exist in the live foreground namespace.
        let proc_dir = shell.ls(&format!("/proc/{child_pid}")).await?;
        assert!(!proc_dir.is_empty());
        let agent_dir = shell.ls(&child_agent_path).await?;
        assert!(!agent_dir.is_empty());
        parsed_pids.push(child_pid);
    }

    // Verify Root remains fully interactive and handles Turn 2 while both children are still blocked.
    shell
        .write("/agent/root/io/input", b"ROOT_INTERACTIVE_PING")
        .await?;
    wait_for_idle_turn(&mut events).await?;
    let output_turn2 = String::from_utf8(shell.cat("/agent/root/io/output").await?)?;
    assert!(
        output_turn2.contains("ROOT_INTERACTIVE_OK"),
        "Root failed to stay interactive while async children run: {output_turn2}"
    );
    let turn2_sys = shared
        .root_turn2_system_prompt
        .lock()
        .expect("system prompt lock")
        .clone()
        .unwrap_or_default();
    assert!(
        turn2_sys.contains("<active_async_workstreams")
            && turn2_sys.contains("research-a")
            && turn2_sys.contains("research-b"),
        "expected Root turn 2 prompt to reflect live async workstreams:\n{turn2_sys}"
    );

    // Let one_off stream `research-b` finish cleanly while `research-a` stays running.
    shared.gate_b.notify_one();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(rec) =
                load_workstream_record(&projects_root, &binding.project_key, "research-b")?
                && rec.state == WorkstreamState::Archived
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .context("one_off background workstream research-b did not archive")??;

    // Trigger actual foreground shutdown while persistent `research-a` is still active.
    // The foreground shutdown sequence must interrupt `research-a`, flush its checkpoint,
    // persist Idle + last_checkpoint_ref in ledger.json, and stop cleanly without hanging.
    let _ = shutdown.send(());
    tokio::time::timeout(Duration::from_secs(15), host_task)
        .await
        .context("foreground host did not exit cleanly after shutdown")???;

    let rec_a = load_workstream_record(&projects_root, &binding.project_key, "research-a")?
        .expect("research-a ledger record must exist");
    assert_eq!(
        rec_a.state,
        WorkstreamState::Idle,
        "persistent workstream must flush to Idle on foreground exit"
    );
    let ckpt_a = rec_a
        .last_checkpoint_ref
        .as_deref()
        .expect("persistent workstream must persist last_checkpoint_ref on exit");
    assert!(
        PathBuf::from(ckpt_a).exists(),
        "flushed checkpoint file does not exist on disk: {ckpt_a}"
    );

    let rec_b = load_workstream_record(&projects_root, &binding.project_key, "research-b")?
        .expect("research-b ledger record must exist");
    assert_eq!(rec_b.state, WorkstreamState::Archived);

    Ok(())
}
