use super::*;
use alan_llm::{GenerationResponse, ToolCall};
use serde_json::{Value, json};

#[tokio::test]
async fn agent_discovers_and_invokes_work_command_through_process_namespace() {
    let mut call = GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: vec![],
        tool_calls: vec![],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: vec![],
    };
    let mut answer = call.clone();
    answer.content = "Work status inspected".into();
    call.tool_calls.push(ToolCall {
        id: Some("inspect-work".into()),
        name: "agent_work".into(),
        arguments: json!({"action":"status","target":"root"}),
    });
    let provider = MockLlmProvider::new().with_responses(vec![call, answer]);
    let probe = provider.clone();
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(provider),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    shell
        .write("/agent/root/io/input", b"Inspect my work status")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let activity: Value = serde_json::from_slice(
                &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            if probe.recorded_requests().len() == 2 && activity["state"] == "idle" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Agent work Tool completion");
    let requests = probe.recorded_requests();
    let tool = requests[0]
        .tools
        .iter()
        .find(|tool| tool.name == "agent_work")
        .expect("manifest-discovered Tool offered without registry injection");
    assert!(tool.description.contains("not a completed answer"));
    let mut found = false;
    for action in shell.ls("/agent/root/actions").await.unwrap() {
        if !action.starts_with('a') {
            continue;
        }
        let base = format!("/agent/root/actions/{action}");
        let result: Value =
            serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap())
                .unwrap_or_default();
        if result["call_id"] != "inspect-work" {
            continue;
        }
        assert_eq!(result["exit_code"], 0);
        let output: Value =
            serde_json::from_slice(&shell.cat(&format!("{base}/output")).await.unwrap()).unwrap();
        assert_eq!(output["success"], true);
        assert_eq!(output["version"], 1);
        assert!(output["activity"].is_object());
        assert!(
            !shell
                .cat(&format!("{base}/process"))
                .await
                .unwrap()
                .is_empty()
        );
        found = true;
    }
    assert!(found, "correlated Process Action evidence");
    manager.shutdown().await.unwrap();
}
