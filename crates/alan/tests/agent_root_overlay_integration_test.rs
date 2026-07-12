use alan_agent_engine::runtime::spawn_with_llm_client_and_tools_and_namespace_surface;
use alan_agent_engine::{AlanHomePaths, LlmClient, WorkspaceRuntimeConfig};
use alan_agent_protocol::{ContentPart, Op, Submission, UiActivityState, UiEvent};
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

async fn wait_for_turn(tail: &mut alan_shell::Tail) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut pending = String::new();
        let mut saw_running = false;
        loop {
            pending.push_str(&String::from_utf8(tail.read(4096).await.unwrap()).unwrap());
            while let Some(newline) = pending.find('\n') {
                let line = pending[..newline].to_string();
                pending.drain(..=newline);
                let event: UiEvent = serde_json::from_str(&line).unwrap();
                if let UiEvent::Activity { snapshot } = event {
                    saw_running |= snapshot.state == UiActivityState::Running;
                    if saw_running && snapshot.state == UiActivityState::Idle {
                        return;
                    }
                }
            }
        }
    })
    .await
    .expect("turn UI stream did not reach idle");
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
        alan_agent_engine::workspace_runtime_rollouts_dir_from_alan_dir(
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
    recovery_rollout_path: Option<PathBuf>,
) -> WorkspaceRuntimeConfig {
    let mut config = WorkspaceRuntimeConfig {
        workspace_id: "workspace-overlay-integration".to_string(),
        workspace_root_dir: Some(workspace_root.to_path_buf()),
        workspace_alan_dir: Some(workspace_alan_dir.to_path_buf()),
        recovery_rollout_path,
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
) -> Vec<GenerationRequest> {
    let (provider, recorded_requests) = RecordingProvider::new(responses);
    let llm_client = LlmClient::new(provider);
    let tools = alan_tools::create_tool_registry_with_core_tools(
        config.workspace_root_dir.clone().unwrap(),
    );

    let launch = spawn_with_llm_client_and_tools_and_namespace_surface(config, llm_client, tools)
        .await
        .unwrap();
    let shell = alan_shell::Shell::new(launch.surface.root_transport());
    let mut ui_events = shell
        .tail(&format!(
            "{}/machine/ui/events",
            launch.surface.agent_path()
        ))
        .await
        .unwrap();
    let mut controller = launch.controller;
    controller.wait_until_ready().await.unwrap();

    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(prompt)],
            context: None,
        }))
        .await
        .unwrap();

    wait_for_turn(&mut ui_events).await;
    controller.shutdown().await.unwrap();

    recorded_requests.lock().unwrap().clone()
}

fn only_rollout_path(rollouts_dir: &Path) -> PathBuf {
    let mut stack = vec![rollouts_dir.to_path_buf()];
    let mut matches = Vec::new();
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
            if filename.ends_with(".jsonl") {
                matches.push(path);
            }
        }
    }
    assert_eq!(matches.len(), 1, "expected exactly one rollout record");
    matches.pop().unwrap()
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

    let requests = run_turn(
        runtime_config_for(home_paths, &workspace_root, &workspace_alan_dir, None),
        vec![
            tool_call_response(&read_target),
            text_response("done after policy"),
        ],
        "please use $overlay-skill and inspect the file",
    )
    .await;

    assert_overlay_requests(&requests);
}

#[tokio::test]
async fn named_agent_overlay_survives_rollout_recovery_in_new_processes() {
    let temp = TempDir::new().unwrap();
    let (home_paths, workspace_root, workspace_alan_dir, _) = prepare_overlay_chain(&temp);
    let rollouts_dir = alan_agent_engine::workspace_runtime_rollouts_dir_from_alan_dir(
        &workspace_alan_dir,
        alan_agent_engine::InstallChannel::Stable,
    );

    let first_requests = run_turn(
        runtime_config_for(
            home_paths.clone(),
            &workspace_root,
            &workspace_alan_dir,
            None,
        ),
        vec![text_response("first turn")],
        "please use $overlay-skill on the first turn",
    )
    .await;
    assert_overlay_requests(&first_requests);

    let rollout_path = only_rollout_path(&rollouts_dir);

    let recovered_requests = run_turn(
        runtime_config_for(
            home_paths.clone(),
            &workspace_root,
            &workspace_alan_dir,
            Some(rollout_path.clone()),
        ),
        vec![text_response("recovered turn")],
        "please use $overlay-skill after recovery",
    )
    .await;
    let recovered_overlay_requests = assert_overlay_requests(&recovered_requests);
    assert_request_messages_include_history(
        recovered_overlay_requests
            .last()
            .copied()
            .expect("expected recovered overlay request"),
        &[
            (
                MessageRole::User,
                "please use $overlay-skill on the first turn",
            ),
            (MessageRole::Assistant, "first turn"),
            (
                MessageRole::User,
                "please use $overlay-skill after recovery",
            ),
        ],
    );

    let second_recovery_requests = run_turn(
        runtime_config_for(
            home_paths,
            &workspace_root,
            &workspace_alan_dir,
            Some(rollout_path),
        ),
        vec![text_response("another recovered turn")],
        "please use $overlay-skill from the same execution record",
    )
    .await;
    let recovered_overlay_requests = assert_overlay_requests(&second_recovery_requests);
    assert_request_messages_include_history(
        recovered_overlay_requests
            .last()
            .copied()
            .expect("expected second recovered overlay request"),
        &[
            (
                MessageRole::User,
                "please use $overlay-skill on the first turn",
            ),
            (MessageRole::Assistant, "first turn"),
            (
                MessageRole::User,
                "please use $overlay-skill from the same execution record",
            ),
        ],
    );
}
