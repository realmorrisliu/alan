use alan_agent_engine::runtime::spawn_with_llm_client_and_tools;
use alan_agent_engine::{
    AlanHomePaths, LlmClient, RuntimeEventEnvelope, WorkspaceRuntimeConfig, session_storage_key,
};
use alan_agent_protocol::{ContentPart, Event, Op, Submission};
use alan_llm::{
    GenerationRequest, GenerationResponse, LlmProvider, MessageRole, StreamChunk, ToolCall,
    ToolCallDelta,
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::mpsc;

const AGENT_NAME: &str = "coder";
const MODEL: &str = "gpt-5.4";
const SKILL_ID: &str = "overlay-skill";
const SKILL_NAME: &str = "Overlay Skill";

#[derive(Clone)]
struct RecordingProvider {
    responses: Arc<Mutex<VecDeque<GenerationResponse>>>,
    recorded_requests: Arc<Mutex<Vec<GenerationRequest>>>,
}

impl RecordingProvider {
    fn new(responses: Vec<GenerationResponse>) -> (Self, Arc<Mutex<Vec<GenerationRequest>>>) {
        let recorded_requests = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                responses: Arc::new(Mutex::new(responses.into())),
                recorded_requests: Arc::clone(&recorded_requests),
            },
            recorded_requests,
        )
    }
}

#[async_trait::async_trait]
impl LlmProvider for RecordingProvider {
    async fn generate(&mut self, request: GenerationRequest) -> anyhow::Result<GenerationResponse> {
        self.recorded_requests.lock().unwrap().push(request);

        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("recording provider response queue exhausted"))
    }

    async fn chat(&mut self, _system: Option<&str>, _user: &str) -> anyhow::Result<String> {
        Err(anyhow::anyhow!(
            "recording provider does not implement chat"
        ))
    }

    async fn generate_stream(
        &mut self,
        request: GenerationRequest,
    ) -> anyhow::Result<mpsc::Receiver<StreamChunk>> {
        Ok(response_stream(self.generate(request).await?))
    }

    fn provider_name(&self) -> &'static str {
        "openai_responses"
    }
}

async fn collect_events_until_turn_complete(
    mut rx: tokio::sync::broadcast::Receiver<RuntimeEventEnvelope>,
    timeout: Duration,
) -> (Vec<Event>, bool) {
    let mut events = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut turn_completed = false;

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(envelope) => {
                        let event = envelope.event.clone();
                        events.push(event.clone());
                        if matches!(event, Event::TurnCompleted { .. }) {
                            turn_completed = true;
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                }
            }
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }

    (events, turn_completed)
}

fn tool_call_response(path: &Path) -> GenerationResponse {
    GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![ToolCall {
            id: Some("call_read_overlay".to_string()),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": path.to_string_lossy().to_string()
            }),
        }],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn text_response(content: &str) -> GenerationResponse {
    GenerationResponse {
        content: content.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn response_stream(response: GenerationResponse) -> mpsc::Receiver<StreamChunk> {
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        if !response.content.is_empty() {
            let _ = tx
                .send(StreamChunk {
                    text: Some(response.content),
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
        }

        let tool_calls = response.tool_calls;
        for (index, tool_call) in tool_calls.iter().enumerate() {
            let arguments =
                serde_json::to_string(&tool_call.arguments).unwrap_or_else(|_| "{}".to_string());
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
                        id: tool_call.id.clone(),
                        name: Some(tool_call.name.clone()),
                        arguments_delta: Some(arguments.clone()),
                        arguments: Some(arguments),
                    }),
                    is_finished: false,
                    finish_reason: None,
                })
                .await;
        }

        let finish_reason = response.finish_reason.unwrap_or_else(|| {
            if tool_calls.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        });
        let _ = tx
            .send(StreamChunk {
                text: None,
                thinking: None,
                thinking_signature: None,
                redacted_thinking: None,
                usage: response.usage,
                provider_response_id: response.provider_response_id,
                provider_response_status: response.provider_response_status,
                sequence_number: None,
                tool_call_delta: None,
                is_finished: true,
                finish_reason: Some(finish_reason),
            })
            .await;
    });
    rx
}

fn write_skill(root: &Path, body: &str) {
    let skill_dir = root.join("skills").join(SKILL_ID);
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            r#"---
name: {SKILL_NAME}
description: Overlay verification skill
---

{body}
"#
        ),
    )
    .unwrap();
}

fn write_agent_root(
    root: &Path,
    model_reasoning_effort: &str,
    soul_text: &str,
    skill_body: &str,
    policy_yaml: Option<&str>,
) {
    std::fs::create_dir_all(root.join("persona")).unwrap();
    std::fs::write(
        root.join("agent.toml"),
        format!("model_reasoning_effort = \"{model_reasoning_effort}\"\n"),
    )
    .unwrap();
    std::fs::write(root.join("persona/SOUL.md"), soul_text).unwrap();
    write_skill(root, skill_body);
    if let Some(policy_yaml) = policy_yaml {
        std::fs::write(root.join("policy.yaml"), policy_yaml).unwrap();
    }
}

fn prepare_overlay_chain(temp: &TempDir) -> (AlanHomePaths, PathBuf, PathBuf, PathBuf) {
    let home_dir = temp.path().join("home");
    let workspace_root = temp.path().join("workspace");
    let workspace_alan_dir = workspace_root.join(".alan");
    let home_paths = AlanHomePaths::from_home_dir(&home_dir);

    std::fs::create_dir_all(
        alan_agent_engine::workspace_runtime_sessions_dir_from_alan_dir(
            &workspace_alan_dir,
            alan_agent_engine::InstallChannel::Stable,
        ),
    )
    .unwrap();
    std::fs::create_dir_all(
        alan_agent_engine::workspace_runtime_memory_dir_from_alan_dir(
            &workspace_alan_dir,
            alan_agent_engine::InstallChannel::Stable,
        ),
    )
    .unwrap();

    write_agent_root(
        &home_paths.global_agent_root_dir,
        "minimal",
        "global default soul",
        "global default skill body",
        None,
    );
    write_agent_root(
        &workspace_root.join(".alan/agents/default"),
        "low",
        "workspace default soul",
        "workspace default skill body",
        None,
    );
    write_agent_root(
        &home_paths.global_named_agents_dir.join(AGENT_NAME),
        "medium",
        "global named soul",
        "global named skill body",
        None,
    );
    write_agent_root(
        &workspace_root.join(".alan/agents").join(AGENT_NAME),
        "high",
        "workspace named soul",
        "workspace named skill body",
        Some(
            r#"
default_action: allow
rules:
  - tool: read_file
    action: deny
    reason: workspace named policy deny
"#,
        ),
    );

    let read_target = workspace_root.join("policy-check.txt");
    std::fs::write(&read_target, "secret").unwrap();

    (home_paths, workspace_root, workspace_alan_dir, read_target)
}

fn runtime_config_for(
    home_paths: AlanHomePaths,
    workspace_root: &Path,
    workspace_alan_dir: &Path,
    session_id: &str,
    resume_rollout_path: Option<PathBuf>,
) -> WorkspaceRuntimeConfig {
    let mut config = WorkspaceRuntimeConfig {
        session_id: Some(session_id.to_string()),
        workspace_id: session_storage_key(session_id),
        workspace_root_dir: Some(workspace_root.to_path_buf()),
        workspace_alan_dir: Some(workspace_alan_dir.to_path_buf()),
        resume_rollout_path,
        agent_home_paths: Some(home_paths),
        ..WorkspaceRuntimeConfig::default()
    };
    config.agent_name = Some(AGENT_NAME.to_string());
    config.agent_config.core_config.openai_responses_api_key = Some("sk-test".to_string());
    config.agent_config.core_config.openai_responses_model = MODEL.to_string();
    config.agent_config.runtime_config.streaming_mode = alan_agent_engine::StreamingMode::Off;
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    config
}

async fn run_turn(
    config: WorkspaceRuntimeConfig,
    responses: Vec<GenerationResponse>,
    prompt: &str,
) -> (Vec<Event>, Vec<GenerationRequest>) {
    let (provider, recorded_requests) = RecordingProvider::new(responses);
    let llm_client = LlmClient::new(provider);
    let tools = alan_tools::create_tool_registry_with_core_tools(
        config.workspace_root_dir.clone().unwrap(),
    );

    let mut controller = spawn_with_llm_client_and_tools(config, llm_client, tools).unwrap();
    controller.wait_until_ready().await.unwrap();

    let rx = controller.handle.event_sender.subscribe();
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(prompt)],
            context: None,
        }))
        .await
        .unwrap();

    let (events, turn_completed) =
        collect_events_until_turn_complete(rx, Duration::from_secs(10)).await;
    controller.shutdown().await.unwrap();

    assert!(
        turn_completed,
        "timed out waiting for TurnCompleted; observed events: {events:?}"
    );
    let exhausted_provider_errors: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            Event::Error { message, .. }
                if message.contains("recording provider response queue exhausted") =>
            {
                Some(message.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(
        exhausted_provider_errors.is_empty(),
        "unexpected extra LLM requests exhausted scripted responses: {exhausted_provider_errors:?}"
    );

    let requests = recorded_requests.lock().unwrap().clone();
    (events, requests)
}

fn latest_rollout_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
    let mut stack = vec![sessions_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if filename.contains(session_id) && filename.ends_with(".jsonl") {
                return path;
            }
        }
    }
    panic!("rollout path not found for requested integration-test session");
}

fn assert_overlay_request(request: &GenerationRequest) {
    let system_prompt = request.system_prompt.as_deref().unwrap_or("");
    assert!(system_prompt.contains("workspace named soul"));
    assert!(!system_prompt.contains("global named soul"));
    assert!(!system_prompt.contains("workspace default soul"));
    assert!(system_prompt.contains("workspace named skill body"));
    assert!(!system_prompt.contains("global named skill body"));
    assert!(!system_prompt.contains("workspace default skill body"));
    assert_eq!(
        request.reasoning.effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}

fn is_memory_promotion_request(request: &GenerationRequest) -> bool {
    request.system_prompt.as_deref() == Some(alan_agent_engine::prompts::MEMORY_PROMOTION_PROMPT)
}

fn overlay_requests(requests: &[GenerationRequest]) -> Vec<&GenerationRequest> {
    let unexpected_requests: Vec<&GenerationRequest> = requests
        .iter()
        .filter(|request| !is_memory_promotion_request(request))
        .filter(|request| {
            !request
                .system_prompt
                .as_deref()
                .is_some_and(|prompt| prompt.contains("workspace named soul"))
        })
        .collect();
    assert!(
        unexpected_requests.is_empty(),
        "unexpected internal requests recorded during overlay test: {unexpected_requests:?}"
    );

    requests
        .iter()
        .filter(|request| {
            request
                .system_prompt
                .as_deref()
                .is_some_and(|prompt| prompt.contains("workspace named soul"))
        })
        .collect()
}

fn assert_overlay_requests(requests: &[GenerationRequest]) -> Vec<&GenerationRequest> {
    let overlay_requests = overlay_requests(requests);
    assert!(
        !overlay_requests.is_empty(),
        "expected at least one recorded overlay LLM request"
    );
    overlay_requests
        .iter()
        .copied()
        .for_each(assert_overlay_request);
    overlay_requests
}

fn assert_request_messages_include_history(
    request: &GenerationRequest,
    expected_messages: &[(MessageRole, &str)],
) {
    let mut cursor = 0usize;
    for message in &request.messages {
        if let Some((expected_role, expected_content)) = expected_messages.get(cursor)
            && message.role == *expected_role
            && message.content.contains(expected_content)
        {
            cursor += 1;
        }
    }

    let actual_messages: Vec<(MessageRole, &str)> = request
        .messages
        .iter()
        .map(|message| (message.role, message.content.as_str()))
        .collect();
    assert_eq!(
        cursor,
        expected_messages.len(),
        "expected request history subsequence {:?}, actual messages were {:?}",
        expected_messages,
        actual_messages
    );
}

#[tokio::test]
async fn named_agent_overlay_applies_highest_precedence_across_runtime_surfaces() {
    let temp = TempDir::new().unwrap();
    let (home_paths, workspace_root, workspace_alan_dir, read_target) =
        prepare_overlay_chain(&temp);

    let (events, requests) = run_turn(
        runtime_config_for(
            home_paths,
            &workspace_root,
            &workspace_alan_dir,
            "sess-overlay-surfaces",
            None,
        ),
        vec![
            tool_call_response(&read_target),
            text_response("done after policy"),
        ],
        "please use $overlay-skill and inspect the file",
    )
    .await;

    assert_overlay_requests(&requests);

    let policy_audit = events.iter().find_map(|event| match event {
        Event::ToolCallCompleted {
            id,
            audit: Some(audit),
            ..
        } if id == "call_read_overlay" => Some(audit),
        _ => None,
    });

    let policy_audit = policy_audit.expect("expected denied read_file audit");
    assert_eq!(policy_audit.action, "deny");
    assert_eq!(policy_audit.policy_source, "workspace_policy_file");
    assert_eq!(
        policy_audit.reason.as_deref(),
        Some("workspace named policy deny")
    );
}

#[tokio::test]
async fn named_agent_overlay_survives_resume_and_fork_runtime_restarts() {
    let temp = TempDir::new().unwrap();
    let (home_paths, workspace_root, workspace_alan_dir, _) = prepare_overlay_chain(&temp);
    let sessions_dir = alan_agent_engine::workspace_runtime_sessions_dir_from_alan_dir(
        &workspace_alan_dir,
        alan_agent_engine::InstallChannel::Stable,
    );

    let session_id = "sess-overlay-base";
    let (_, first_requests) = run_turn(
        runtime_config_for(
            home_paths.clone(),
            &workspace_root,
            &workspace_alan_dir,
            session_id,
            None,
        ),
        vec![text_response("first turn")],
        "please use $overlay-skill on the first turn",
    )
    .await;
    assert_overlay_requests(&first_requests);

    let rollout_path = latest_rollout_path(&sessions_dir, session_id);

    let (_, resumed_requests) = run_turn(
        runtime_config_for(
            home_paths.clone(),
            &workspace_root,
            &workspace_alan_dir,
            session_id,
            Some(rollout_path.clone()),
        ),
        vec![text_response("resumed turn")],
        "please use $overlay-skill after resume",
    )
    .await;
    let resumed_overlay_requests = assert_overlay_requests(&resumed_requests);
    assert_request_messages_include_history(
        resumed_overlay_requests
            .last()
            .copied()
            .expect("expected resumed overlay request"),
        &[
            (
                MessageRole::User,
                "please use $overlay-skill on the first turn",
            ),
            (MessageRole::Assistant, "first turn"),
            (MessageRole::User, "please use $overlay-skill after resume"),
        ],
    );

    let (_, forked_requests) = run_turn(
        runtime_config_for(
            home_paths,
            &workspace_root,
            &workspace_alan_dir,
            "sess-overlay-fork",
            Some(rollout_path),
        ),
        vec![text_response("forked turn")],
        "please use $overlay-skill after fork",
    )
    .await;
    let forked_overlay_requests = assert_overlay_requests(&forked_requests);
    assert_request_messages_include_history(
        forked_overlay_requests
            .last()
            .copied()
            .expect("expected forked overlay request"),
        &[
            (
                MessageRole::User,
                "please use $overlay-skill on the first turn",
            ),
            (MessageRole::Assistant, "first turn"),
            (MessageRole::User, "please use $overlay-skill after fork"),
        ],
    );
}
