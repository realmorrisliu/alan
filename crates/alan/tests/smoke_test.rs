//! File-native runtime smoke tests.

use alan_agent_engine::{
    AgentProcessConfig, AgentRuntimeStoreBindings, HostMountGrant, LlmClient, ProcessLaunchContext,
    RuntimeNamespaceSurface, spawn_with_llm_client_and_namespace_surface,
    spawn_with_llm_client_and_tools_and_namespace_surface,
};
use alan_agent_protocol::{ContentPart, Op, Submission, UiActivityState, UiEvent};
use alan_kernel::{Access, Credentials, Namespace};
use alan_llm::{GenerationResponse, MockLlmProvider, TokenUsage, ToolCall};
use std::time::Duration;

fn response(content: &str) -> GenerationResponse {
    GenerationResponse {
        content: content.to_string(),
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
    }
}

async fn wait_for_turn(tail: &mut alan_shell::Tail, surface: &RuntimeNamespaceSurface) -> String {
    tokio::time::timeout(Duration::from_secs(15), async {
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
                        let shell = alan_shell::Shell::new(surface.root_transport());
                        return String::from_utf8(
                            shell
                                .cat(&format!("{}/io/output", surface.agent_path()))
                                .await
                                .unwrap(),
                        )
                        .unwrap();
                    }
                }
            }
        }
    })
    .await
    .expect("turn UI stream did not reach idle")
}

async fn submit(controller: &alan_agent_engine::RuntimeController, text: &str) {
    controller
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text(text)],
            context: None,
        }))
        .await
        .unwrap();
}

#[tokio::test]
async fn smoke_text_response_is_observable_from_agentfs() {
    let launch = spawn_with_llm_client_and_namespace_surface(
        AgentProcessConfig::default(),
        LlmClient::new(MockLlmProvider::new().with_response(response("Hello from AgentFS!"))),
    )
    .await
    .unwrap();
    let shell = alan_shell::Shell::new(launch.surface.root_transport());
    let mut events = shell
        .tail(&format!(
            "{}/machine/ui/events",
            launch.surface.agent_path()
        ))
        .await
        .unwrap();
    let mut controller = launch.controller;
    controller.wait_until_ready().await.unwrap();

    submit(&controller, "Say hello").await;
    let output = wait_for_turn(&mut events, &launch.surface).await;
    assert_eq!(output, "Hello from AgentFS!");
    controller.shutdown().await.unwrap();
}

#[tokio::test]
async fn smoke_tool_result_is_observable_from_action_files() {
    let temp = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let file = temp.path().join("test.txt");
    std::fs::write(&file, "hello from smoke test").unwrap();
    let tool_response = GenerationResponse {
        tool_calls: vec![ToolCall {
            id: Some("call_001".to_string()),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "/mnt/source/test.txt"}),
        }],
        ..response("")
    };
    let mock = MockLlmProvider::new()
        .with_responses(vec![tool_response, response("I read the file for you.")]);
    let mut config = AgentProcessConfig::default();
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let grant = HostMountGrant::new("/mnt/source", temp.path(), Access::ReadOnly).unwrap();
    let mut namespace = Namespace::new();
    alan::host_mounts::apply_host_mount_declarations(&mut namespace, std::slice::from_ref(&grant))
        .unwrap();
    config.launch_context =
        ProcessLaunchContext::new(namespace, Credentials::user("smoke-agent"), "/mnt/source")
            .unwrap();
    config.launch_context.host_mounts = vec![grant];
    let store_bindings = AgentRuntimeStoreBindings {
        rollouts: store.path().join("rollouts"),
        checkpoints: store.path().join("checkpoints"),
        cache: store.path().join("cache"),
        tmp: store.path().join("tmp"),
        metadata: store.path().join("metadata"),
    };
    for path in [
        &store_bindings.rollouts,
        &store_bindings.checkpoints,
        &store_bindings.cache,
        &store_bindings.tmp,
        &store_bindings.metadata,
    ] {
        std::fs::create_dir_all(path).unwrap();
    }
    config.store_bindings = Some(store_bindings);
    let tools = alan_tools::create_tool_registry_with_core_tools(temp.path().to_path_buf());
    let launch =
        spawn_with_llm_client_and_tools_and_namespace_surface(config, LlmClient::new(mock), tools)
            .await
            .unwrap();
    let shell = alan_shell::Shell::new(launch.surface.root_transport());
    let mut events = shell
        .tail(&format!(
            "{}/machine/ui/events",
            launch.surface.agent_path()
        ))
        .await
        .unwrap();
    let mut controller = launch.controller;
    controller.wait_until_ready().await.unwrap();

    submit(&controller, "Read the test file").await;
    let output = wait_for_turn(&mut events, &launch.surface).await;
    assert!(output.contains("I read the file for you."));
    let actions = shell
        .ls(&format!("{}/actions", launch.surface.agent_path()))
        .await
        .unwrap();
    let action = actions.iter().find(|name| name.starts_with('a')).unwrap();
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!(
                    "{}/actions/{action}/status",
                    launch.surface.agent_path()
                ))
                .await
                .unwrap(),
        )
        .unwrap(),
        "completed"
    );
    let action_output = String::from_utf8(
        shell
            .cat(&format!(
                "{}/actions/{action}/output",
                launch.surface.agent_path()
            ))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(action_output.contains("/mnt/source/test.txt"));
    assert!(!action_output.contains(temp.path().to_string_lossy().as_ref()));
    controller.shutdown().await.unwrap();
}

#[tokio::test]
async fn smoke_multiple_turns_resume_ui_stream_by_offset() {
    let mock = MockLlmProvider::new().with_responses(vec![
        response("First response"),
        response("Second response"),
    ]);
    let launch = spawn_with_llm_client_and_namespace_surface(
        AgentProcessConfig::default(),
        LlmClient::new(mock),
    )
    .await
    .unwrap();
    let shell = alan_shell::Shell::new(launch.surface.root_transport());
    let mut events = shell
        .tail(&format!(
            "{}/machine/ui/events",
            launch.surface.agent_path()
        ))
        .await
        .unwrap();
    let mut controller = launch.controller;
    controller.wait_until_ready().await.unwrap();

    submit(&controller, "First question").await;
    assert_eq!(
        wait_for_turn(&mut events, &launch.surface).await,
        "First response"
    );
    submit(&controller, "Second question").await;
    assert_eq!(
        wait_for_turn(&mut events, &launch.surface).await,
        "First responseSecond response"
    );
    controller.shutdown().await.unwrap();
}
