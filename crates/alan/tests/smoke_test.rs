//! File-native runtime smoke tests through an explicit ephemeral Alan OS Host.

use alan_agent_engine::{
    AgentProcessConfig, AgentRuntimeStoreBindings, HostMountGrant, LlmClient, ProcessLaunchContext,
    ToolRegistry,
};
use alan_agent_protocol::{UiActivityState, UiEvent};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, Namespace};
use alan_llm::{GenerationResponse, MockLlmProvider, TokenUsage, ToolCall};
use alan_os_host::{AlanOsHost, HostBootConfig, HostEndpointPaths, LocalAttachment};
use std::time::Duration;

const AGENT_PATH: &str = "/agent/root";
const TEST_TIMEOUT: Duration = Duration::from_secs(20);
const ACTION_TIMEOUT: Duration = Duration::from_secs(60);

struct TestHost {
    root: InProcessTransport,
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
    _runtime: tempfile::TempDir,
}

impl TestHost {
    async fn boot(config: AgentProcessConfig, llm: LlmClient, tools: ToolRegistry) -> Self {
        let runtime = tempfile::tempdir().unwrap();
        let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
        let host = tokio::time::timeout(
            TEST_TIMEOUT,
            AlanOsHost::boot(
                HostBootConfig::ephemeral("test", config, llm, tools),
                paths.clone(),
            ),
        )
        .await
        .expect("ephemeral Alan OS Host boot timed out")
        .unwrap();
        let (shutdown, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            host.serve_until(async move {
                let _ = stopped.await;
            })
            .await
        });
        let attachment = tokio::time::timeout(TEST_TIMEOUT, LocalAttachment::new(paths).connect())
            .await
            .expect("ephemeral Alan OS Host attachment timed out")
            .unwrap();
        Self {
            root: attachment.root,
            shutdown,
            task,
            _runtime: runtime,
        }
    }

    async fn shutdown(self) {
        let _ = self.shutdown.send(());
        tokio::time::timeout(TEST_TIMEOUT, self.task)
            .await
            .expect("ephemeral Alan OS Host shutdown timed out")
            .unwrap()
            .unwrap();
    }
}

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

async fn read_tail_until(tail: &mut alan_shell::Tail, expected: &str) -> String {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut output = String::new();
        loop {
            output.push_str(&String::from_utf8(tail.read(4096).await.unwrap()).unwrap());
            if output.contains(expected) {
                return output;
            }
        }
    })
    .await
    .unwrap_or_else(|error| panic!("Agent output did not contain {expected:?}: {error}"))
}

async fn wait_for_idle(events: &mut alan_shell::Tail) {
    // Coverage instrumentation can make the first full-workspace Agent turn substantially
    // slower than the ordinary test build. Keep submission and shutdown deadlines tight, but
    // give this asynchronous end-to-end transition enough time under that gate.
    tokio::time::timeout(ACTION_TIMEOUT, async {
        let mut pending = String::new();
        let mut saw_running = false;
        loop {
            pending.push_str(&String::from_utf8(events.read(4096).await.unwrap()).unwrap());
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
    .expect("Agent activity did not return to idle")
}

async fn submit(shell: &alan_shell::Shell, text: &str) {
    tokio::time::timeout(
        TEST_TIMEOUT,
        shell.write(&format!("{AGENT_PATH}/io/input"), text.as_bytes()),
    )
    .await
    .expect("AgentFS input submission timed out")
    .unwrap();
}

async fn wait_for_action(shell: &alan_shell::Shell) -> String {
    tokio::time::timeout(TEST_TIMEOUT, async {
        loop {
            if let Some(action) = shell
                .ls(&format!("{AGENT_PATH}/actions"))
                .await
                .unwrap()
                .into_iter()
                .find(|name| name.starts_with('a'))
            {
                return action;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Agent action file did not appear")
}

#[tokio::test]
async fn alan_os_host_smoke_covers_multiple_turns_tools_and_agentfs() {
    let temp = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("test.txt"), "hello from smoke test").unwrap();
    let tool_response = GenerationResponse {
        tool_calls: vec![ToolCall {
            id: Some("call_001".to_string()),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "/mnt/source/test.txt"}),
        }],
        ..response("")
    };
    let mock = MockLlmProvider::new().with_responses(vec![
        response("First response"),
        tool_response,
        response("I read the file for you."),
    ]);
    let mut config = AgentProcessConfig::default();
    // This smoke owns the foreground Host -> AgentFS -> Tool Process path. Turn-end
    // Memory promotion is an independent deferred consumer of the LLM connection;
    // leaving it enabled would make a sequential mock race the next foreground turn.
    config.agent_config.core_config.memory.enabled = false;
    config.agent_config.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    let grant = HostMountGrant::new("/mnt/source", temp.path(), Access::ReadOnly).unwrap();
    let mut namespace = Namespace::new();
    alan_os_host::host_mounts::apply_host_mount_declarations(
        &mut namespace,
        std::slice::from_ref(&grant),
    )
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
    let host = TestHost::boot(config, LlmClient::new(mock), tools).await;
    let shell = alan_shell::Shell::new(host.root.clone());
    let mut output = shell
        .tail(&format!("{AGENT_PATH}/io/output"))
        .await
        .unwrap();
    let mut events = shell
        .tail(&format!("{AGENT_PATH}/machine/ui/events"))
        .await
        .unwrap();
    submit(&shell, "First question").await;
    assert_eq!(
        read_tail_until(&mut output, "First response").await,
        "First response"
    );
    wait_for_idle(&mut events).await;

    submit(&shell, "Read the test file").await;
    let second_output = read_tail_until(&mut output, "I read the file for you.").await;
    wait_for_idle(&mut events).await;
    assert_eq!(second_output, "I read the file for you.");
    let action = wait_for_action(&shell).await;
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("{AGENT_PATH}/actions/{action}/status"))
                .await
                .unwrap(),
        )
        .unwrap(),
        "completed"
    );
    let action_output = String::from_utf8(
        shell
            .cat(&format!("{AGENT_PATH}/actions/{action}/output"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(action_output.contains("/mnt/source/test.txt"));
    assert!(!action_output.contains(temp.path().to_string_lossy().as_ref()));
    events.close().await.unwrap();
    output.close().await.unwrap();
    host.shutdown().await;
}
