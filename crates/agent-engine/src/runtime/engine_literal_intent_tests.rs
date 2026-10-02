//! Literal model input and deterministic Command routing through real AgentFS admission.
use super::super::directory_selection::{SelectionAdapter, SelectionAuthority};
use super::*;
use crate::tools::{ToolExecutionBinding, ToolProcessRunner, ToolRegistry};
use alan_agent_protocol::{InputIntent, UiEvent, UiInputStatus};

#[tokio::test]
async fn literal_force_agent_request_and_typed_command_remain_distinct() {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let mock = MockLlmProvider::new().with_response(mock_generation_response("DONE"));
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let runner = ToolProcessRunner::from_registry(&ToolRegistry::new());
    runner.register_process_binding(
        1,
        ToolExecutionBinding::awaiting_host_projection("/mnt/old".into(), temp.path().into())
            .with_adapter(Arc::new(SelectionAdapter("/mnt/old".into()))),
    );
    runner.register_process_authority(1, Arc::new(SelectionAuthority));
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_namespace_cwd("/mnt/old")
        .with_tool_process_context(1, runner.clone());
    let files = environment.agent_files();
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let metadata = runtime.wait_until_ready().await.unwrap();
    let mut tail = shell.tail("/agent/1/machine/ui/events").await.unwrap();
    let bodies = [
        " !HISTORY_AGENT_f1b62026 literal text; reply only DONE without tools.\n  ",
        " :literal !text; reply only DONE without tools.\n  ",
    ];
    let mut ids = Vec::new();
    for body in bodies {
        let id = uuid::Uuid::new_v4().to_string();
        ids.push(id.clone());
        let frame = serde_json::json!({"version":1,"submission_id":id,"intent":"force_agent","mode":"follow_up","body":body});
        shell
            .write(
                "/agent/1/io/input",
                format!("alan-input-v1\n{frame}").as_bytes(),
            )
            .await
            .unwrap();
        let events = wait_for_ui_turn_completion(&mut tail, Duration::from_secs(5)).await;
        assert!(events.iter().any(|event| matches!(event, UiEvent::InputCompleted { submission_ids, status: UiInputStatus::Completed, .. } if submission_ids == &vec![id.clone()])));
    }
    let before_command = mock.recorded_requests().len();
    let command_id = uuid::Uuid::new_v4().to_string();
    let frame = serde_json::json!({"version":1,"submission_id":command_id,"intent":"command","mode":"follow_up","body":"cd /mnt/new"});
    shell
        .write(
            "/agent/1/io/input",
            format!("alan-input-v1\n{frame}").as_bytes(),
        )
        .await
        .unwrap();
    let events = wait_for_ui_turn_completion(&mut tail, Duration::from_secs(5)).await;
    runtime.shutdown().await.unwrap();
    assert!(events.iter().any(|event| matches!(event, UiEvent::InputCompleted { submission_ids, status: UiInputStatus::Completed, .. } if submission_ids == &vec![command_id.clone()])));
    assert_eq!(
        runner.process_binding(1).unwrap().namespace_cwd,
        std::path::PathBuf::from("/mnt/new")
    );
    let requests = mock.recorded_requests();
    assert_eq!(before_command, 2);
    assert_eq!(
        requests.len(),
        before_command,
        "typed Command must not generate"
    );
    for (request, body) in requests.iter().zip(bodies) {
        let user = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == alan_llm::MessageRole::User)
            .unwrap();
        assert_eq!(user.content, body);
        // Text-only messages use the canonical `content` field; no rich parts are synthesized.
        assert!(user.content_parts.is_empty());
        let system = request.system_prompt.as_deref().unwrap();
        assert!(!system.contains("When a user message begins with `!`"));
        assert!(system.contains("User message content is literal text"));
        assert!(system.contains("Runtime already handles explicit Command intent"));
    }
    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert!(tape.contains("DONE"));
    let action_ids = files.action_ids().await.unwrap();
    assert_eq!(
        action_ids.len(),
        1,
        "only typed cd creates an Action; mock DONE calls no Tools"
    );
    let result: serde_json::Value = serde_json::from_slice(
        &shell
            .cat(&format!("/agent/1/actions/{}/result", action_ids[0]))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(result["call_id"], command_id);
    assert_eq!(result["outcome"]["cwd"], "/mnt/new");
    let history = crate::rollout::RolloutRecorder::load_history(&metadata.rollout_path.unwrap())
        .await
        .unwrap();
    for id in ids {
        assert!(history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == id && e.payload["intent"] == serde_json::to_value(InputIntent::ForceAgent).unwrap())));
    }
}
