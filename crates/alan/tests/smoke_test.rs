//! Smoke Tests — Mock runtime integration tests for coding agent verification loop.
//!
//! These tests use MockLlmProvider to exercise the runtime without real LLM calls.
//! Run with `cargo test -p alan --test smoke_test -- --nocapture` to see full event logs.

use alan_agent_engine::runtime::spawn_with_llm_client_and_tools;
use alan_agent_engine::{
    AlanHomePaths, LlmClient, RuntimeEventEnvelope, WorkspaceRuntimeConfig, spawn_with_llm_client,
};
use alan_agent_protocol::{ContentPart, Event, Op, Submission};
use alan_llm::{GenerationResponse, MessageRole, MockLlmProvider, TokenUsage, ToolCall};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

/// Collect events from a runtime until TurnCompleted or timeout.
async fn collect_events_until_turn_complete(
    mut rx: tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>,
    timeout: Duration,
) -> Vec<Event> {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(envelope) => {
                        let event = envelope.event.clone();
                        events.push(event.clone());
                        if matches!(event, Event::TurnCompleted { .. }) {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("[smoke] WARNING: lagged {n} events");
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                eprintln!("[smoke] TIMEOUT waiting for TurnCompleted after {:?}", timeout);
                break;
            }
        }
    }

    events
}

fn print_event_summary(test_name: &str, events: &[Event]) {
    eprintln!("\n=== {test_name}: collected {} events ===", events.len());
    for (i, event) in events.iter().enumerate() {
        let tag = match event {
            Event::TurnStarted { .. } => "TurnStarted",
            Event::TurnCompleted { .. } => "TurnCompleted",
            Event::ThinkingDelta { chunk, is_final } => {
                eprintln!(
                    "  [{i}] ThinkingDelta(is_final={is_final}): {:?}",
                    &chunk[..chunk.len().min(80)]
                );
                continue;
            }
            Event::TextDelta { chunk, is_final } => {
                eprintln!("  [{i}] TextDelta(is_final={is_final}): {chunk:?}");
                continue;
            }
            Event::ToolCallStarted { name, .. } => {
                eprintln!("  [{i}] ToolCallStarted: {name}");
                continue;
            }
            Event::ToolCallCompleted { id, .. } => {
                eprintln!("  [{i}] ToolCallCompleted: {id}");
                continue;
            }
            Event::Error { message, .. } => {
                eprintln!("  [{i}] Error: {message}");
                continue;
            }
            other => {
                eprintln!("  [{i}] {:?}", std::mem::discriminant(other));
                continue;
            }
        };
        eprintln!("  [{i}] {tag}");
    }
    eprintln!("=== end {test_name} ===\n");
}

#[tokio::test]
async fn smoke_text_response() {
    // MockLlmProvider returns a fixed text response
    let mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: "Hello from mock LLM!".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 5,
            total_tokens: 15,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });

    let llm_client = LlmClient::new(mock);
    let config = WorkspaceRuntimeConfig::default();

    let mut controller =
        spawn_with_llm_client(config, llm_client).expect("spawn_with_llm_client should succeed");

    controller
        .wait_until_ready()
        .await
        .expect("runtime should become ready");

    // Subscribe to events before submitting
    let rx = controller.handle.event_sender.subscribe();

    // Submit a Turn op
    let submission = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("Say hello")],
        context: None,
    });
    controller
        .handle
        .submission_tx
        .send(submission)
        .await
        .expect("send submission");

    // Collect events
    let events = collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    print_event_summary("smoke_text_response", &events);

    // Basic sanity checks
    let has_turn_started = events
        .iter()
        .any(|e| matches!(e, Event::TurnStarted { .. }));
    let has_turn_completed = events
        .iter()
        .any(|e| matches!(e, Event::TurnCompleted { .. }));
    let has_content = events.iter().any(|e| match e {
        Event::TextDelta { chunk, .. } => !chunk.is_empty(),
        _ => false,
    });

    assert!(has_turn_started, "Expected TurnStarted event");
    assert!(has_turn_completed, "Expected TurnCompleted event");
    assert!(has_content, "Expected at least one content event");

    // Verify TurnStarted comes before TurnCompleted
    let start_idx = events
        .iter()
        .position(|e| matches!(e, Event::TurnStarted { .. }));
    let end_idx = events
        .iter()
        .position(|e| matches!(e, Event::TurnCompleted { .. }));
    if let (Some(s), Some(e)) = (start_idx, end_idx) {
        assert!(s < e, "TurnStarted should come before TurnCompleted");
    }

    controller.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_tool_call_flow() {
    let temp = tempfile::tempdir().expect("tempdir");
    let file_path = temp.path().join("test.txt");
    std::fs::write(&file_path, "hello from smoke test").expect("write test file");
    let file_path_str = file_path.to_string_lossy().to_string();

    // First response: a tool call. Second response: text after tool result.
    let mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![ToolCall {
                id: Some("call_001".to_string()),
                name: "read_file".to_string(),
                arguments: serde_json::json!({"path": file_path_str}),
            }],
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "I read the file for you.".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                cached_prompt_tokens: None,
                completion_tokens: 10,
                total_tokens: 30,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let llm_client = LlmClient::new(mock);
    let mut config = WorkspaceRuntimeConfig::default();
    // Skip tool approval prompts in tests
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let tools = alan_tools::create_tool_registry_with_core_tools(temp.path().to_path_buf());

    let mut controller = spawn_with_llm_client_and_tools(config, llm_client, tools)
        .expect("spawn_with_llm_client_and_tools should succeed");

    controller
        .wait_until_ready()
        .await
        .expect("runtime should become ready");

    let rx = controller.handle.event_sender.subscribe();

    let submission = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("Read /tmp/test.txt for me")],
        context: None,
    });
    controller
        .handle
        .submission_tx
        .send(submission)
        .await
        .expect("send submission");

    let events = collect_events_until_turn_complete(rx, Duration::from_secs(15)).await;
    print_event_summary("smoke_tool_call_flow", &events);

    // Check for tool-related events
    let has_tool_started = events
        .iter()
        .any(|e| matches!(e, Event::ToolCallStarted { .. }));
    let has_tool_completed = events
        .iter()
        .any(|e| matches!(e, Event::ToolCallCompleted { .. }));

    // Tool events may or may not appear depending on tool registry config,
    // but we should at least get turn lifecycle events
    let has_turn_started = events
        .iter()
        .any(|e| matches!(e, Event::TurnStarted { .. }));
    let has_turn_completed = events
        .iter()
        .any(|e| matches!(e, Event::TurnCompleted { .. }));

    assert!(has_turn_started, "Expected TurnStarted event");
    assert!(has_turn_completed, "Expected TurnCompleted event");

    eprintln!(
        "[smoke_tool_call_flow] tool_started={has_tool_started}, tool_completed={has_tool_completed}"
    );

    controller.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_cross_workspace_reading_surfaces_workspace_delegation_path() {
    let temp_home = TempDir::new().expect("temp home");
    let temp_current = TempDir::new().expect("current workspace");
    let temp_other = TempDir::new().expect("other workspace");

    let workspace_root = temp_current.path().join("home-workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    let other_workspace_root = temp_other.path().join("other-workspace");
    fs::create_dir_all(&workspace_alan_dir).expect("create current workspace .alan");
    fs::create_dir_all(other_workspace_root.join("docs/spec"))
        .expect("create other workspace docs");
    fs::write(
        other_workspace_root.join("docs/spec/alan_coding_steward_contract.md"),
        "# Steward Contract\nfull steward mode routes repo-local work to a child runtime.\n",
    )
    .expect("write other workspace contract doc");

    let mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![ToolCall {
                id: Some("call_cross_workspace_bash".to_string()),
                name: "bash".to_string(),
                arguments: serde_json::json!({
                    "command": format!(
                        "cd {} && rg -n \"full steward mode\" docs/spec/alan_coding_steward_contract.md",
                        other_workspace_root.display()
                    ),
                    "timeout": 60
                }),
            }],
            usage: Some(TokenUsage {
                prompt_tokens: 24,
                cached_prompt_tokens: None,
                completion_tokens: 8,
                total_tokens: 32,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "That target is in another workspace. I should delegate with workspace-inspect."
                .to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 32,
                cached_prompt_tokens: None,
                completion_tokens: 10,
                total_tokens: 42,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let llm_client = LlmClient::new(mock.clone());
    let mut config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let tools = alan_tools::create_tool_registry_with_core_tools(workspace_root.clone());

    let mut controller = spawn_with_llm_client_and_tools(config, llm_client, tools)
        .expect("spawn_with_llm_client_and_tools should succeed");
    controller
        .wait_until_ready()
        .await
        .expect("runtime should become ready");

    let rx = controller.handle.event_sender.subscribe();
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "去另一个本地 workspace 看 alan 的 steward 设计文档，告诉我 full steward mode 是什么意思。",
            )],
            context: None,
        }))
        .await
        .expect("send submission");

    let events = collect_events_until_turn_complete(rx, Duration::from_secs(15)).await;
    print_event_summary(
        "smoke_cross_workspace_reading_surfaces_workspace_delegation_path",
        &events,
    );

    let tool_completed = events.iter().find_map(|event| match event {
        Event::ToolCallCompleted {
            success,
            result_preview,
            audit,
            ..
        } => Some((success, result_preview, audit)),
        _ => None,
    });
    let Some((success, result_preview, audit)) = tool_completed else {
        panic!("expected ToolCallCompleted event");
    };
    assert_eq!(*success, Some(false));
    assert!(
        result_preview
            .as_deref()
            .unwrap_or_default()
            .contains("requires_workspace_delegation"),
        "expected workspace delegation preview, got {:?}",
        result_preview
    );
    assert!(
        audit
            .as_ref()
            .is_some_and(|audit| audit.policy_source == "workspace_routing_preflight")
    );
    assert!(
        events.iter().all(|event| {
            !matches!(
                event,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )
        }),
        "cross-workspace local reads should not trigger confirmation escalation"
    );

    let recorded_requests = mock.recorded_requests();
    let system_prompt = recorded_requests
        .iter()
        .filter_map(|request| request.system_prompt.as_deref())
        .find(|prompt| prompt.contains("skill_id: workspace-inspect"))
        .expect("expected system prompt with workspace-inspect guidance");
    assert!(system_prompt.contains("skill_id: workspace-inspect"));
    assert!(system_prompt.contains("execution: delegate(target=workspace-reader)"));
    assert!(system_prompt.contains("invoke_delegated_skill"));
    assert!(system_prompt.contains("workspace_root"));
    assert!(system_prompt.contains("optional `cwd`"));

    controller.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_colloquial_cross_repo_request_still_exposes_workspace_delegation_guidance() {
    let temp_home = TempDir::new().expect("temp home");
    let temp_current = TempDir::new().expect("current workspace");
    let temp_other = TempDir::new().expect("other repo");

    let workspace_root = temp_current.path().join("home-workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    let other_workspace_root = temp_other.path().join("Developer/steward-notes");
    fs::create_dir_all(&workspace_alan_dir).expect("create current workspace .alan");
    fs::create_dir_all(other_workspace_root.join("docs/spec")).expect("create other repo docs");
    fs::write(
        other_workspace_root.join("docs/spec/alan_coding_steward_contract.md"),
        "# Steward Contract\nfull steward mode routes repo-local work to a child runtime.\n",
    )
    .expect("write other repo contract doc");

    let user_request = "你去 steward-notes 那个 repo 看一下 docs/spec/alan_coding_steward_contract.md，告诉我 full steward mode 讲了什么。";
    let mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![ToolCall {
                id: Some("call_colloquial_cross_repo_bash".to_string()),
                name: "bash".to_string(),
                arguments: serde_json::json!({
                    "command": format!(
                        "cd {} && rg -n \"full steward mode\" docs/spec/alan_coding_steward_contract.md",
                        other_workspace_root.display()
                    ),
                    "timeout": 60
                }),
            }],
            usage: Some(TokenUsage {
                prompt_tokens: 28,
                cached_prompt_tokens: None,
                completion_tokens: 9,
                total_tokens: 37,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "The request is still repo-bound, so I need workspace delegation guidance."
                .to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 32,
                cached_prompt_tokens: None,
                completion_tokens: 10,
                total_tokens: 42,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let llm_client = LlmClient::new(mock.clone());
    let mut config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let tools = alan_tools::create_tool_registry_with_core_tools(workspace_root.clone());

    let mut controller = spawn_with_llm_client_and_tools(config, llm_client, tools)
        .expect("spawn_with_llm_client_and_tools should succeed");
    controller
        .wait_until_ready()
        .await
        .expect("runtime should become ready");

    let rx = controller.handle.event_sender.subscribe();
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(user_request)],
            context: None,
        }))
        .await
        .expect("send submission");

    let events = collect_events_until_turn_complete(rx, Duration::from_secs(15)).await;
    print_event_summary(
        "smoke_colloquial_cross_repo_request_still_exposes_workspace_delegation_guidance",
        &events,
    );

    let tool_completed = events.iter().find_map(|event| match event {
        Event::ToolCallCompleted {
            success,
            result_preview,
            audit,
            ..
        } => Some((success, result_preview, audit)),
        _ => None,
    });
    let Some((success, result_preview, audit)) = tool_completed else {
        panic!("expected ToolCallCompleted event");
    };
    assert_eq!(*success, Some(false));
    assert!(
        result_preview
            .as_deref()
            .unwrap_or_default()
            .contains("requires_workspace_delegation"),
        "expected workspace delegation preview, got {:?}",
        result_preview
    );
    assert!(
        audit
            .as_ref()
            .is_some_and(|audit| audit.policy_source == "workspace_routing_preflight")
    );
    assert!(
        events.iter().all(|event| {
            !matches!(
                event,
                Event::Yield {
                    kind: alan_agent_protocol::YieldKind::Confirmation,
                    ..
                }
            )
        }),
        "colloquial cross-repo requests should not trigger confirmation escalation"
    );

    let recorded_requests = mock.recorded_requests();
    let first_request = recorded_requests
        .first()
        .expect("expected recorded LLM request");
    assert!(
        first_request
            .messages
            .iter()
            .any(|message| message.role == MessageRole::User && message.content == user_request),
        "expected recorded request to include the original colloquial user prompt"
    );

    let system_prompt = recorded_requests
        .iter()
        .filter_map(|request| request.system_prompt.as_deref())
        .find(|prompt| prompt.contains("skill_id: workspace-inspect"))
        .expect("expected system prompt with workspace-inspect guidance");
    assert!(system_prompt.contains("skill_id: workspace-inspect"));
    assert!(system_prompt.contains("execution: delegate(target=workspace-reader)"));
    assert!(system_prompt.contains("invoke_delegated_skill"));
    assert!(system_prompt.contains("workspace_root"));
    assert!(system_prompt.contains("optional `cwd`"));

    controller.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_multiple_turns() {
    let mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: "First response".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 5,
                cached_prompt_tokens: None,
                completion_tokens: 3,
                total_tokens: 8,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "Second response".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 15,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let llm_client = LlmClient::new(mock);
    let config = WorkspaceRuntimeConfig::default();

    let mut controller =
        spawn_with_llm_client(config, llm_client).expect("spawn_with_llm_client should succeed");

    controller
        .wait_until_ready()
        .await
        .expect("runtime should become ready");

    // Turn 1
    let rx1 = controller.handle.event_sender.subscribe();
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("First question")],
            context: None,
        }))
        .await
        .expect("send");
    let events1 = collect_events_until_turn_complete(rx1, Duration::from_secs(10)).await;
    print_event_summary("smoke_multiple_turns (turn 1)", &events1);

    // Turn 2
    let rx2 = controller.handle.event_sender.subscribe();
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("Second question")],
            context: None,
        }))
        .await
        .expect("send");
    let events2 = collect_events_until_turn_complete(rx2, Duration::from_secs(10)).await;
    print_event_summary("smoke_multiple_turns (turn 2)", &events2);

    // Both turns should have complete lifecycle
    for (name, events) in [("turn1", &events1), ("turn2", &events2)] {
        let started = events
            .iter()
            .any(|e| matches!(e, Event::TurnStarted { .. }));
        let completed = events
            .iter()
            .any(|e| matches!(e, Event::TurnCompleted { .. }));
        assert!(started, "{name}: Expected TurnStarted");
        assert!(completed, "{name}: Expected TurnCompleted");
    }

    controller.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn smoke_cross_session_persona_memory_is_reinjected() {
    let temp_home = TempDir::new().expect("temp home");
    let temp_workspace = TempDir::new().expect("temp workspace");
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    fs::create_dir_all(&workspace_alan_dir).expect("create workspace .alan");

    let persona_user_path = workspace_alan_dir.join("agents/default/persona/USER.md");
    let workspace_duplicate_user_path = workspace_root.join("USER.md");
    let marker = "ALAN_SMOKE_PERSONA_MEMORY_MARKER";

    let first_mock = MockLlmProvider::new().with_responses(vec![
        GenerationResponse {
            content: String::new(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: vec![ToolCall {
                id: Some("call_store_memory".to_string()),
                name: "write_file".to_string(),
                arguments: serde_json::json!({
                    "path": persona_user_path.to_string_lossy(),
                    "content": format!("# USER\n- Favorite smoke marker: {marker}\n"),
                }),
            }],
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 25,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
        GenerationResponse {
            content: "Stored marker.".to_string(),
            thinking: None,
            thinking_signature: None,
            redacted_thinking: Vec::new(),
            tool_calls: Vec::new(),
            usage: Some(TokenUsage {
                prompt_tokens: 20,
                cached_prompt_tokens: None,
                completion_tokens: 5,
                total_tokens: 25,
                reasoning_tokens: None,
            }),
            finish_reason: None,
            provider_response_id: None,
            provider_response_status: None,
            warnings: Vec::new(),
        },
    ]);

    let first_llm_client = LlmClient::new(first_mock);
    let mut first_config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir.clone()),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };
    first_config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let first_tools = alan_tools::create_tool_registry_with_core_tools(workspace_root.clone());

    let mut first_controller =
        spawn_with_llm_client_and_tools(first_config, first_llm_client, first_tools)
            .expect("spawn first runtime");
    first_controller
        .wait_until_ready()
        .await
        .expect("first runtime ready");

    let rx = first_controller.handle.event_sender.subscribe();
    first_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "Remember a stable smoke marker across sessions.",
            )],
            context: None,
        }))
        .await
        .expect("send first submission");

    let first_events = collect_events_until_turn_complete(rx, Duration::from_secs(15)).await;
    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. })),
        "first turn should complete"
    );
    first_controller
        .shutdown()
        .await
        .expect("shutdown first runtime");

    let persisted_user = fs::read_to_string(&persona_user_path).expect("read persisted USER.md");
    assert!(
        persisted_user.contains(marker),
        "expected persona USER.md to contain stored marker"
    );
    assert!(
        !workspace_duplicate_user_path.exists(),
        "workspace root USER.md duplicate should not be created"
    );

    let second_mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: "Loaded marker.".to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 3,
            total_tokens: 13,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let second_llm_client = LlmClient::new(second_mock.clone());
    let second_config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir.clone()),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };

    let mut second_controller =
        spawn_with_llm_client(second_config, second_llm_client).expect("spawn second runtime");
    second_controller
        .wait_until_ready()
        .await
        .expect("second runtime ready");

    let rx = second_controller.handle.event_sender.subscribe();
    second_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("What stable memory do you have?")],
            context: None,
        }))
        .await
        .expect("send second submission");

    let second_events = collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. })),
        "second turn should complete"
    );
    second_controller
        .shutdown()
        .await
        .expect("shutdown second runtime");

    let recorded_requests = second_mock.recorded_requests();
    let system_prompt = recorded_requests
        .iter()
        .filter_map(|request| request.system_prompt.as_deref())
        .find(|prompt| prompt.contains(marker))
        .or_else(|| {
            recorded_requests
                .iter()
                .filter_map(|request| request.system_prompt.as_deref())
                .find(|prompt| prompt.contains("Write updates to:"))
        })
        .expect("expected recorded system prompt containing persisted persona memory");
    assert!(
        system_prompt.contains(marker),
        "expected persisted persona marker to be reinjected into the next session prompt"
    );
    assert!(
        system_prompt.contains(&format!(
            "Write updates to: {}",
            persona_user_path.display()
        )),
        "expected system prompt to show the exact writable USER.md target"
    );
}

#[tokio::test]
async fn smoke_cross_session_runtime_memory_recall_bundle_is_reinjected() {
    let temp_home = TempDir::new().expect("temp home");
    let temp_workspace = TempDir::new().expect("temp workspace");
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    let memory_dir = alan_agent_engine::workspace_runtime_memory_dir_from_alan_dir(
        &workspace_alan_dir,
        alan_agent_engine::InstallChannel::Stable,
    );
    fs::create_dir_all(&workspace_alan_dir).expect("create workspace .alan");
    alan_agent_engine::prompts::ensure_workspace_memory_layout_at(&memory_dir)
        .expect("initialize workspace memory layout");

    let marker = "ALAN_SMOKE_RUNTIME_MEMORY_RECALL";
    fs::write(
        memory_dir.join("USER.md"),
        format!("# User Memory\n- Favorite runtime recall marker: {marker}\n"),
    )
    .expect("write memory USER.md");

    let second_mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: marker.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 3,
            total_tokens: 13,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let second_llm_client = LlmClient::new(second_mock.clone());
    let second_config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir.clone()),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };

    let mut second_controller =
        spawn_with_llm_client(second_config, second_llm_client).expect("spawn second runtime");
    second_controller
        .wait_until_ready()
        .await
        .expect("second runtime ready");

    let rx = second_controller.handle.event_sender.subscribe();
    second_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "What is my favorite runtime recall marker?",
            )],
            context: None,
        }))
        .await
        .expect("send second submission");

    let second_events = collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. })),
        "second turn should complete"
    );
    second_controller
        .shutdown()
        .await
        .expect("shutdown second runtime");

    let recorded_requests = second_mock.recorded_requests();
    let system_prompt = recorded_requests
        .iter()
        .filter_map(|request| request.system_prompt.as_deref())
        .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
        .expect("expected recorded runtime recall prompt");
    assert!(
        system_prompt.contains("## Runtime Recall Bundle"),
        "expected runtime recall bundle to be appended for identity recall questions"
    );
    assert!(
        system_prompt.contains(".alan/runtime/stable/memory/USER.md"),
        "expected runtime recall bundle to cite USER.md"
    );
    assert!(
        system_prompt.contains(marker),
        "expected runtime recall bundle to include the stored marker"
    );
}

#[tokio::test]
async fn smoke_cross_session_handoff_continuity_is_recalled() {
    let temp_home = TempDir::new().expect("temp home");
    let temp_workspace = TempDir::new().expect("temp workspace");
    let workspace_root = temp_workspace.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    fs::create_dir_all(&workspace_alan_dir).expect("create workspace .alan");
    let continuity_marker = "continuity:ALAN_SMOKE_HANDOFF_MARKER";

    let first_mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: continuity_marker.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 3,
            total_tokens: 13,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let first_llm_client = LlmClient::new(first_mock);
    let first_config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir.clone()),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };

    let mut first_controller =
        spawn_with_llm_client(first_config, first_llm_client).expect("spawn first runtime");
    first_controller
        .wait_until_ready()
        .await
        .expect("first runtime ready");

    let rx = first_controller.handle.event_sender.subscribe();
    first_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(format!(
                "Reply with exactly {continuity_marker} so it becomes the latest handoff marker."
            ))],
            context: None,
        }))
        .await
        .expect("send first submission");

    let first_events = collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    assert!(
        first_events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. })),
        "first turn should complete"
    );
    first_controller
        .shutdown()
        .await
        .expect("shutdown first runtime");

    let latest_handoff = fs::read_to_string(
        alan_agent_engine::workspace_runtime_memory_dir_from_alan_dir(
            &workspace_alan_dir,
            alan_agent_engine::InstallChannel::Stable,
        )
        .join("handoffs/LATEST.md"),
    )
    .unwrap();
    assert!(
        latest_handoff.contains(continuity_marker),
        "expected latest handoff to preserve the continuity marker"
    );

    let second_mock = MockLlmProvider::new().with_response(GenerationResponse {
        content: continuity_marker.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: Some(TokenUsage {
            prompt_tokens: 10,
            cached_prompt_tokens: None,
            completion_tokens: 3,
            total_tokens: 13,
            reasoning_tokens: None,
        }),
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    });
    let second_llm_client = LlmClient::new(second_mock.clone());
    let second_config = WorkspaceRuntimeConfig {
        workspace_root_dir: Some(workspace_root.clone()),
        workspace_alan_dir: Some(workspace_alan_dir.clone()),
        agent_home_paths: Some(AlanHomePaths::from_home_dir(temp_home.path())),
        ..WorkspaceRuntimeConfig::default()
    };

    let mut second_controller =
        spawn_with_llm_client(second_config, second_llm_client).expect("spawn second runtime");
    second_controller
        .wait_until_ready()
        .await
        .expect("second runtime ready");

    let rx = second_controller.handle.event_sender.subscribe();
    second_controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(
                "What were we doing in the previous session? Reply with exactly the saved continuity marker.",
            )],
            context: None,
        }))
        .await
        .expect("send second submission");

    let second_events = collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, Event::TurnCompleted { .. })),
        "second turn should complete"
    );
    second_controller
        .shutdown()
        .await
        .expect("shutdown second runtime");

    let recorded_requests = second_mock.recorded_requests();
    let system_prompt = recorded_requests
        .iter()
        .filter_map(|request| request.system_prompt.as_deref())
        .find(|prompt| prompt.contains("## Runtime Recall Bundle"))
        .expect("expected recorded runtime recall prompt");
    assert!(system_prompt.contains("## Runtime Recall Bundle"));
    assert!(system_prompt.contains(".alan/runtime/stable/memory/handoffs/LATEST.md"));
    assert!(system_prompt.contains(continuity_marker));
}
