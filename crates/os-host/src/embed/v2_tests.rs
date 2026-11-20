use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alan_agent_engine::tools::{Tool, ToolCapability, ToolContext, ToolRegistry, ToolResult};
use alan_agent_engine::{HookDecision, LlmClient, ToolPolicyHook, TrustPosture};
use alan_ap::{Fid, FileKind, OpenMode, Request};
use alan_llm::{
    GenerationRequest, GenerationResponse, MockLlmProvider, StreamChunk, TokenUsage, ToolCall,
};
use anyhow::Context;
use serde_json::json;
use tokio::sync::Notify;

use super::tests::{mock_response, next_embed_event, read_until_submission, stream_chunk};
use super::*;

struct AppNoteTool {
    invoked: Arc<AtomicBool>,
}

impl Tool for AppNoteTool {
    fn name(&self) -> &str {
        "app_note"
    }

    fn description(&self) -> &str {
        "Read or write notes inside /mnt/my-app or /mnt/ro-app via ToolContext::namespace"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "op": { "type": "string" },
                "dir": { "type": "string" },
                "file": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["op"]
        })
    }

    fn capability(&self, arguments: &serde_json::Value) -> ToolCapability {
        if arguments.get("op").and_then(serde_json::Value::as_str) == Some("read") {
            ToolCapability::Read
        } else {
            ToolCapability::Write
        }
    }

    fn execute(&self, arguments: serde_json::Value, ctx: &ToolContext) -> ToolResult {
        let invoked = Arc::clone(&self.invoked);
        let namespace = ctx.namespace().cloned();
        Box::pin(async move {
            invoked.store(true, Ordering::SeqCst);
            let ns = namespace.context("ToolContext::namespace() missing in custom Tool")?;
            let op = arguments["op"].as_str().unwrap_or("read");
            let dir = arguments["dir"].as_str().unwrap_or("my-app");
            let file = arguments["file"].as_str().unwrap_or("note.txt");
            if op == "read" {
                let shell = Shell::new(ns);
                let data = shell
                    .cat(&format!("/mnt/{dir}/{file}"))
                    .await
                    .map_err(|code| anyhow::anyhow!("read failed: {code}"))?;
                return Ok(json!({ "text": String::from_utf8_lossy(&data).to_string() }));
            }
            let body = arguments["content"].as_str().unwrap_or("");
            ns.call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["mnt".to_string(), dir.to_string()],
            })
            .await
            .map_err(|code| anyhow::anyhow!("walk /mnt/{dir}: {code}"))?;
            ns.call(Request::Create {
                fid: Fid(1),
                newfid: Fid(2),
                name: file.to_string(),
                kind: FileKind::File,
            })
            .await
            .map_err(|code| anyhow::anyhow!("create /mnt/{dir}/{file}: {code}"))?;
            ns.call(Request::Open {
                fid: Fid(2),
                mode: OpenMode::Write,
            })
            .await
            .map_err(|code| anyhow::anyhow!("open /mnt/{dir}/{file}: {code}"))?;
            ns.call(Request::Write {
                fid: Fid(2),
                offset: 0,
                data: body.as_bytes().to_vec(),
            })
            .await
            .map_err(|code| anyhow::anyhow!("write /mnt/{dir}/{file}: {code}"))?;
            let _ = ns.call(Request::Clunk { fid: Fid(2) }).await;
            let _ = ns.call(Request::Clunk { fid: Fid(1) }).await;
            Ok(json!({ "written": body }))
        })
    }
}

fn tool_call_response(
    call_id: &str,
    name: &str,
    arguments: serde_json::Value,
) -> GenerationResponse {
    GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![ToolCall {
            id: Some(call_id.to_string()),
            name: name.to_string(),
            arguments,
        }],
        usage: Some(TokenUsage {
            prompt_tokens: 15,
            cached_prompt_tokens: None,
            completion_tokens: 6,
            total_tokens: 21,
            reasoning_tokens: None,
            ..Default::default()
        }),
        finish_reason: Some("tool_calls".to_string()),
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn response_with_usage(text: &str) -> GenerationResponse {
    let mut r = mock_response(text);
    r.usage = Some(TokenUsage {
        prompt_tokens: 20,
        cached_prompt_tokens: Some(4),
        completion_tokens: 8,
        total_tokens: 28,
        reasoning_tokens: Some(3),
        ..Default::default()
    });
    r
}

#[tokio::test]
async fn writable_mnt_app_supports_shell_and_custom_tool_via_namespace_context() {
    let invoked = Arc::new(AtomicBool::new(false));
    let mut tools = ToolRegistry::new();
    tools.register(AppNoteTool {
        invoked: Arc::clone(&invoked),
    });

    let app_fs = Arc::new(alan_ap::reference::MemFs::writable());
    let ro_fs = Arc::new(alan_ap::reference::MemFs::with_read_only_file(
        "locked.txt",
        b"immutable".to_vec(),
    ));

    let provider = MockLlmProvider::new().with_responses(vec![
        tool_call_response(
            "call_write_app",
            "app_note",
            json!({ "op": "write", "dir": "my-app", "file": "note.txt", "content": "written-by-tool" }),
        ),
        response_with_usage("tool write complete"),
        tool_call_response(
            "call_read_app",
            "app_note",
            json!({ "op": "read", "dir": "my-app", "file": "note.txt" }),
        ),
        response_with_usage("tool read complete"),
        tool_call_response(
            "call_write_ro",
            "app_note",
            json!({ "op": "write", "dir": "ro-app", "file": "blocked.txt", "content": "nope" }),
        ),
        response_with_usage("ro rejected as expected"),
    ]);

    let process = AgentProcessConfig {
        trust_posture: TrustPosture::ConfinedTrusted,
        ..Default::default()
    };

    let app = EmbeddedAlan::boot(
        EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider.clone())))
            .with_process_config(process)
            .with_tools(tools)
            .with_mount("/mnt/my-app", app_fs, Access::ReadWrite)
            .with_mount("/mnt/ro-app", ro_fs, Access::ReadOnly),
    )
    .await
    .unwrap();

    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();

    // 1. Custom Tool creates & writes /mnt/my-app/note.txt, then shell reads and overwrites it.
    let s1 = client.submit_text("write note").await.unwrap();
    let e1 = read_until_submission(&mut stream, &s1).await;
    assert!(
        invoked.load(Ordering::SeqCst),
        "AppNoteTool not invoked; events={e1:?}, recorded={:?}",
        provider.recorded_requests()
    );
    assert_eq!(
        app.shell().cat("/mnt/my-app/note.txt").await.unwrap(),
        b"written-by-tool"
    );
    assert!(
        e1.iter().any(|ev| matches!(ev, EmbedEvent::ToolStarted { call_id, name, .. } if call_id == "call_write_app" && name == "app_note")),
        "missing ToolStarted: {e1:?}"
    );
    assert!(
        e1.iter().any(|ev| matches!(ev, EmbedEvent::ToolFinished { call_id, name, status, .. } if call_id == "call_write_app" && name == "app_note" && status == "completed")),
        "missing ToolFinished: {e1:?}"
    );
    assert!(
        e1.iter()
            .any(|ev| matches!(ev, EmbedEvent::Usage { submission_id: Some(id), .. } if id == &s1)),
        "missing Usage: {e1:?}"
    );

    // 2. Shell writes /mnt/my-app/note.txt and custom Tool reads updated bytes.
    app.shell()
        .write("/mnt/my-app/note.txt", b"written-by-shell")
        .await
        .unwrap();
    let s2 = client.submit_text("read note").await.unwrap();
    let _e2 = read_until_submission(&mut stream, &s2).await;
    let saw_shell_bytes = provider.recorded_requests().iter().any(|req| {
        req.messages
            .iter()
            .any(|m| m.content.contains("written-by-shell"))
    });
    assert!(
        saw_shell_bytes,
        "custom Tool did not read shell-written /mnt/my-app/note.txt"
    );

    // 3. Access::ReadOnly /mnt/ro-app still rejects writes even with ConfinedTrusted.
    let s3 = client.submit_text("write ro").await.unwrap();
    let e3 = read_until_submission(&mut stream, &s3).await;
    assert!(
        e3.iter().any(|ev| matches!(ev, EmbedEvent::ToolFinished { call_id, status, .. } if call_id == "call_write_ro" && status == "failed")),
        "expected ReadOnly mount write to fail: {e3:?}"
    );
    assert!(app.shell().cat("/mnt/ro-app/blocked.txt").await.is_err());

    app.shutdown().await.unwrap();

    // 4. Reserved /mnt/* mounts are rejected at boot.
    for reserved in [
        "/mnt/llm",
        "/mnt/mem",
        "/mnt/package",
        "/mnt/route",
        "/mnt/connections",
        "/mnt/host-mount",
        "/mnt/service-manager",
    ] {
        let p = MockLlmProvider::new();
        let err = EmbeddedAlan::boot(
            EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(p))).with_mount(
                reserved,
                Arc::new(alan_ap::reference::MemFs::writable()),
                Access::ReadWrite,
            ),
        )
        .await
        .err()
        .expect("reserved mount should fail");
        assert!(
            err.to_string().contains("reserved"),
            "unexpected error for {reserved}: {err}"
        );
    }
}

struct IncrementalStreamProvider {
    first_chunk_sent: Arc<Notify>,
    release_second_chunk: Arc<Notify>,
}

#[async_trait::async_trait]
impl alan_llm::LlmProvider for IncrementalStreamProvider {
    async fn generate(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<GenerationResponse> {
        Ok(mock_response("Hello world"))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Ok("Hello world".to_string())
    }

    async fn generate_stream(
        &mut self,
        _request: GenerationRequest,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let sent = Arc::clone(&self.first_chunk_sent);
        let release = Arc::clone(&self.release_second_chunk);
        tokio::spawn(async move {
            let mut think = stream_chunk(None, false);
            think.thinking = Some("pondering...".to_string());
            let _ = tx.send(think).await;
            // Exceed the 80-char live guardrail hold buffer so tokens flush immediately mid-stream.
            let _ = tx
                .send(stream_chunk(
                    Some("Hello from live incremental streaming token buffer — releasing first chunk cleanly!! "),
                    false,
                ))
                .await;
            sent.notify_one();
            release.notified().await;
            let _ = tx.send(stream_chunk(Some("world"), false)).await;
            let mut done = stream_chunk(None, true);
            done.usage = Some(TokenUsage {
                prompt_tokens: 11,
                cached_prompt_tokens: None,
                completion_tokens: 4,
                total_tokens: 15,
                reasoning_tokens: Some(2),
                ..Default::default()
            });
            let _ = tx.send(done).await;
        });
        Ok(rx)
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

#[tokio::test]
async fn live_streaming_emits_text_and_thinking_before_turn_completion() {
    let first_chunk_sent = Arc::new(Notify::new());
    let release_second_chunk = Arc::new(Notify::new());
    let provider = IncrementalStreamProvider {
        first_chunk_sent: Arc::clone(&first_chunk_sent),
        release_second_chunk: Arc::clone(&release_second_chunk),
    };
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider),
    )))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let submission_id = client.submit_text("stream incrementally").await.unwrap();

    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        first_chunk_sent.notified(),
    )
    .await
    .unwrap();

    let expected_prefix =
        "Hello from live incremental streaming token buffer — releasing first chunk cleanly!! ";
    let mut pre_release_text = String::new();
    let mut saw_thinking = false;
    let mut saw_thinking_final = false;
    while pre_release_text != expected_prefix {
        match next_embed_event(&mut stream).await {
            EmbedEvent::ThinkingDelta {
                chunk, is_final, ..
            } => {
                if chunk.contains("pondering") {
                    saw_thinking = true;
                }
                if is_final {
                    saw_thinking_final = true;
                }
            }
            EmbedEvent::TextDelta {
                chunk, is_final, ..
            } => {
                assert!(!is_final);
                pre_release_text.push_str(&chunk);
            }
            EmbedEvent::TurnFinished { .. } => {
                panic!("TurnFinished arrived before second chunk was released");
            }
            _ => {}
        }
    }
    assert!(saw_thinking, "expected live ThinkingDelta before release");

    release_second_chunk.notify_one();
    let rest = read_until_submission(&mut stream, &submission_id).await;
    let remaining_text: String = rest
        .iter()
        .filter_map(|e| match e {
            EmbedEvent::TextDelta {
                chunk,
                is_final: false,
                ..
            } => Some(chunk.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(remaining_text, "world", "{rest:?}");
    saw_thinking_final |= rest
        .iter()
        .any(|e| matches!(e, EmbedEvent::ThinkingDelta { is_final: true, .. }));
    assert!(saw_thinking_final, "expected ThinkingDelta(is_final: true)");
    assert!(rest.iter().any(|e| matches!(
        e,
        EmbedEvent::Usage { submission_id: Some(id), usage } if id == &submission_id && usage.input_tokens == 11 && usage.output_tokens == 4
    )));

    app.shutdown().await.unwrap();
}

struct RedLineHook {
    mode: Arc<std::sync::Mutex<&'static str>>,
}

impl ToolPolicyHook for RedLineHook {
    fn evaluate(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        capability: ToolCapability,
    ) -> HookDecision {
        assert_eq!(tool_name, "app_note");
        assert_eq!(capability, ToolCapability::Write);
        match *self.mode.lock().unwrap() {
            "deny"
                if args.get("dir").and_then(serde_json::Value::as_str)
                    == Some("human_judgment") =>
            {
                HookDecision::Deny("red line: human_judgment is immutable".to_string())
            }
            "escalate" => HookDecision::Escalate("confirm domain mutation".to_string()),
            _ => HookDecision::Continue,
        }
    }
}

#[tokio::test]
async fn tool_policy_hook_denies_even_when_trusted_and_escalates_rich_pending_request() {
    let invoked = Arc::new(AtomicBool::new(false));
    let mut tools = ToolRegistry::new();
    tools.register(AppNoteTool {
        invoked: Arc::clone(&invoked),
    });

    let mode = Arc::new(std::sync::Mutex::new("deny"));
    let hook = Arc::new(RedLineHook {
        mode: Arc::clone(&mode),
    });

    let app_fs = Arc::new(alan_ap::reference::MemFs::writable());
    let provider = MockLlmProvider::new().with_responses(vec![
        tool_call_response(
            "call_deny",
            "app_note",
            json!({ "op": "write", "dir": "human_judgment", "file": "note.txt", "content": "forbidden" }),
        ),
        response_with_usage("blocked safely"),
        tool_call_response(
            "call_escalate",
            "app_note",
            json!({ "op": "write", "dir": "my-app", "file": "approved.txt", "content": "confirmed" }),
        ),
        response_with_usage("approved and written"),
    ]);

    let process = AgentProcessConfig {
        trust_posture: TrustPosture::ConfinedTrusted,
        ..Default::default()
    };

    let app = EmbeddedAlan::boot(
        EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider.clone())))
            .with_process_config(process)
            .with_tools(tools)
            .with_tool_policy_hook(hook)
            .with_mount("/mnt/my-app", app_fs, Access::ReadWrite),
    )
    .await
    .unwrap();

    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();

    // 1. Deny even with ConfinedTrusted -> tool never executes and policy blocks safely.
    let s1 = client.submit_text("mutate forbidden field").await.unwrap();
    let _e1 = read_until_submission(&mut stream, &s1).await;
    assert!(!invoked.load(Ordering::SeqCst), "denied tool must not run");
    assert!(
        provider.recorded_requests().iter().any(|req| {
            req.messages
                .iter()
                .any(|m| m.content.contains("red line: human_judgment is immutable"))
        }),
        "expected ToolPolicyHook deny reason in tool feedback"
    );

    // 2. Escalate -> yields rich PendingRequest, approval executes tool cleanly.
    *mode.lock().unwrap() = "escalate";
    let s2 = client
        .submit_text("mutate with confirmation")
        .await
        .unwrap();
    let request_id = loop {
        match next_embed_event(&mut stream).await {
            EmbedEvent::PendingRequest {
                request_id,
                kind,
                prompt,
                options,
                default_option,
                capability,
                reason,
                ..
            } => {
                assert_eq!(kind, "tool_escalation_confirmation");
                assert!(
                    prompt.contains("confirm domain mutation"),
                    "prompt: {prompt}"
                );
                assert!(options.iter().any(|o| o == "approve"));
                assert!(default_option.is_some());
                assert_eq!(capability.as_deref(), Some("write"));
                assert_eq!(reason.as_deref(), Some("confirm domain mutation"));
                break request_id;
            }
            EmbedEvent::TurnFinished { .. } => panic!("turn finished without PendingRequest"),
            _ => {}
        }
    };
    client
        .respond_request(&request_id, "approve")
        .await
        .unwrap();
    let e2 = read_until_submission(&mut stream, &s2).await;
    assert!(invoked.load(Ordering::SeqCst), "approved tool should run");
    assert_eq!(
        app.shell().cat("/mnt/my-app/approved.txt").await.unwrap(),
        b"confirmed"
    );
    assert!(matches!(
        e2.last(),
        Some(EmbedEvent::TurnFinished {
            status: UiInputStatus::Completed,
            ..
        })
    ));

    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn structured_output_response_schema_validates_and_retries() {
    let schema = json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "score": { "type": "integer" }
        },
        "required": ["title", "score"],
        "additionalProperties": false
    });
    let provider = MockLlmProvider::new().with_responses(vec![
        mock_response("not valid json"),
        mock_response(r#"{"title":"Slowpage","score":9}"#),
    ]);
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(provider.clone()),
    )))
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let sub = client
        .submit_text_with_schema("draft title", schema.clone())
        .await
        .unwrap();
    let events = read_until_submission(&mut stream, &sub).await;
    let output: String = super::tests::collect_text(&events);
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid final JSON");
    assert_eq!(parsed["title"], "Slowpage");
    assert_eq!(parsed["score"], 9);

    let recorded = provider.recorded_requests();
    assert_eq!(recorded.len(), 2, "expected single schema recovery retry");
    assert_eq!(recorded[0].response_schema.as_ref(), Some(&schema));
    assert_eq!(recorded[1].response_schema.as_ref(), Some(&schema));

    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn system_prompt_replace_and_no_builtin_skills_eliminate_missing_noise() {
    let provider = MockLlmProvider::new().with_response(mock_response("clean prompt ok"));
    let app = EmbeddedAlan::boot(
        EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(provider.clone())))
            .with_system_prompt_mode(SystemPromptMode::Replace {
                prompt: "You are Project-M3 domain analyst.".to_string(),
            })
            .with_builtin_skills(false),
    )
    .await
    .unwrap();
    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let s1 = client.submit_text("check prompt").await.unwrap();
    let _ = read_until_submission(&mut stream, &s1).await;

    let recorded = provider.recorded_requests();
    let sys = recorded[0].system_prompt.as_deref().unwrap_or("");
    assert!(
        sys.starts_with("You are Project-M3 domain analyst."),
        "unexpected Replace prompt: {sys}"
    );
    assert!(!sys.contains("# Runtime Base Constraints"));
    assert!(!sys.contains("# alan System Prompt"));
    assert!(!sys.contains("[MISSING"));
    let pkg_dir = app.shell().ls("/lib/pkg").await.unwrap_or_default();
    assert!(
        pkg_dir.is_empty(),
        "builtin skills should be omitted: {pkg_dir:?}"
    );

    app.shutdown().await.unwrap();

    // Also confirm Append mode omits [MISSING FROM AGENT DEFINITION DESCRIPTOR] noise.
    let append_provider = MockLlmProvider::new().with_response(mock_response("append clean"));
    let app_append = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(append_provider.clone()),
    )))
    .await
    .unwrap();
    let c = app_append.root_agent();
    let mut st = c.event_stream().await.unwrap();
    let s2 = c.submit_text("hi").await.unwrap();
    let _ = read_until_submission(&mut st, &s2).await;
    let append_sys = append_provider.recorded_requests()[0]
        .system_prompt
        .as_deref()
        .unwrap_or("")
        .to_string();
    assert!(!append_sys.contains("MISSING FROM AGENT DEFINITION"));
    app_append.shutdown().await.unwrap();
}

#[tokio::test]
async fn multi_session_child_session_and_reset_reuse_shared_provider() {
    let shared = SharedLlmProvider::new(MockLlmProvider::new().with_responses(vec![
        mock_response("root-1"),
        mock_response("session-1"),
        mock_response("child-1"),
        mock_response("root-reset"),
    ]));
    let app = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::SharedProvider(shared)))
        .await
        .unwrap();

    let root = app.root_agent();
    let mut root_stream = root.event_stream().await.unwrap();
    let s_root = root.submit_text("root hi").await.unwrap();
    assert_eq!(
        super::tests::collect_text(&read_until_submission(&mut root_stream, &s_root).await),
        "root-1"
    );

    let session = app
        .open_session(SessionOptions::default().with_system_prompt_mode(
            SystemPromptMode::Replace {
                prompt: "Session 1".into(),
            },
        ))
        .await
        .unwrap();
    assert_ne!(session.pid(), app.root_pid());
    let mut s_stream = session.event_stream().await.unwrap();
    let s_id = session.submit_text("session hi").await.unwrap();
    assert_eq!(
        super::tests::collect_text(&read_until_submission(&mut s_stream, &s_id).await),
        "session-1"
    );

    let child = app
        .spawn_child_session(session.pid(), SessionOptions::default())
        .await
        .unwrap();
    assert_ne!(child.pid(), session.pid());
    let mut c_stream = child.event_stream().await.unwrap();
    let c_id = child.submit_text("child hi").await.unwrap();
    assert_eq!(
        super::tests::collect_text(&read_until_submission(&mut c_stream, &c_id).await),
        "child-1"
    );

    child.close().await.unwrap();
    app.close_session(session.pid()).await.unwrap();

    let old_root_pid = app.root_pid();
    let new_root_pid = app.reset_session(SessionOptions::default()).await.unwrap();
    assert_ne!(old_root_pid, new_root_pid);

    let mut fresh_stream = app.root_agent().event_stream().await.unwrap();
    let s_reset = app.root_agent().submit_text("after reset").await.unwrap();
    assert_eq!(
        super::tests::collect_text(&read_until_submission(&mut fresh_stream, &s_reset).await),
        "root-reset"
    );

    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn isolated_host_mount_shared_credentials_config_and_drop_work_cleanly() {
    let sys_dir = tempfile::tempdir().unwrap();
    let host_dir = tempfile::tempdir().unwrap();
    std::fs::write(host_dir.path().join("hello.txt"), b"from-host").unwrap();

    let invoked = Arc::new(AtomicBool::new(false));
    let mut tools = ToolRegistry::new();
    tools.register(AppNoteTool {
        invoked: Arc::clone(&invoked),
    });

    let process = AgentProcessConfig {
        trust_posture: TrustPosture::ConfinedTrusted,
        ..Default::default()
    };

    let provider = MockLlmProvider::new().with_responses(vec![
        tool_call_response(
            "call_read_host",
            "app_note",
            json!({ "op": "read", "dir": "project-request-1", "file": "hello.txt" }),
        ),
        response_with_usage("host read complete"),
    ]);

    let app = EmbeddedAlan::boot(
        EmbeddedConfig::isolated(
            sys_dir.path(),
            LlmSource::Client(LlmClient::new(provider.clone())),
        )
        .with_process_config(process)
        .with_tools(tools),
    )
    .await
    .unwrap();
    let grant = app
        .host_mount(host_dir.path(), HostMountAccess::ReadWrite)
        .unwrap();
    assert_eq!(grant.namespace_path, "/mnt/project-request-1");

    // Host Mount grants project strictly onto the authorized Root Agent Process, not local-entry.
    assert!(
        app.shell()
            .cat(&format!("{}/hello.txt", grant.namespace_path))
            .await
            .is_err()
    );

    let client = app.root_agent();
    let mut stream = client.event_stream().await.unwrap();
    let sub = client.submit_text("read mounted host file").await.unwrap();
    let _ = read_until_submission(&mut stream, &sub).await;
    assert!(invoked.load(Ordering::SeqCst));
    assert!(
        provider
            .recorded_requests()
            .iter()
            .any(|req| req.messages.iter().any(|m| m.content.contains("from-host"))),
        "Root Agent tool did not read HostMount file: {:?}",
        provider.recorded_requests()
    );

    // Verify Drop triggers cleanup without calling `.shutdown().await`.
    drop(app);
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Verify ephemeral without host mounts denies host_mount and SharedProductCredentials builds cleanly.
    let eph = EmbeddedAlan::boot(EmbeddedConfig::ephemeral(LlmSource::Client(
        LlmClient::new(MockLlmProvider::new().with_response(mock_response("ok"))),
    )))
    .await
    .unwrap();
    assert!(
        eph.host_mount(host_dir.path(), HostMountAccess::ReadOnly)
            .is_err()
    );
    drop(eph);

    let shared_cfg = EmbeddedConfig::ephemeral(LlmSource::SharedProductCredentials);
    let (sm_cfg, host_enabled) = shared_cfg.into_service_manager_config().unwrap();
    assert!(!host_enabled);
    assert!(sm_cfg.in_memory_stores);
    assert!(sm_cfg.connection_store.is_some());
}
