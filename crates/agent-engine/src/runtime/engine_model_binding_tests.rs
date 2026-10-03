//! Real Runtime admission must capture callable identity before acknowledgement.
use super::*;
#[path = "engine_admission_recovery_tests.rs"]
mod admission_recovery;
#[path = "engine_model_contention_tests.rs"]
mod contention;
#[path = "engine_literal_intent_tests.rs"]
mod literal_intent;
#[path = "engine_model_retention_tests.rs"]
mod retention;

#[tokio::test]
async fn runtime_api_and_file_admission_capture_callable_before_dispatch() {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection_profile(
        "profile-a",
        alan_llmfs::ConnectionProfile::new("openai_responses", "gpt-5.4", "credential-a"),
        Box::new(GatedFirstGeneration {
            mock: mock.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(registry.connection_snapshot("profile-a"))),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("gpt-5.4"));
    core.llm_provider = crate::config::LlmProvider::OpenAiResponses;
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, "/agent/1", "profile-a"),
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    let metadata = runtime.wait_until_ready().await.unwrap();
    let path = metadata.rollout_path.unwrap();
    let status: serde_json::Value = serde_json::from_slice(
        &shell
            .cat("/agent/1/machine/ui/models")
            .await
            .expect("Runtime publishes safe model status before Ready"),
    )
    .unwrap();
    assert_eq!(status["catalog"]["models"].as_array().unwrap().len(), 1);
    assert_eq!(status["selected_next"]["model"], "gpt-5.4");
    assert!(status["active"].is_null());
    assert!(!status.to_string().contains("credential"));
    let active = Submission::new(Op::Input {
        parts: vec![ContentPart::text("hold active A")],
        mode: InputMode::FollowUp,
    });
    runtime.handle.submission_tx.send(active).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    let queued = Submission::new(Op::Input {
        parts: vec![ContentPart::text("queued API A")],
        mode: InputMode::FollowUp,
    });
    runtime
        .handle
        .submission_tx
        .send(queued.clone())
        .await
        .unwrap();
    let file_id = uuid::Uuid::new_v4().to_string();
    let frame = serde_json::json!({
        "version":1,"submission_id":file_id,"intent":"agent",
        "mode":"follow_up","body":"queued file A"
    });
    shell
        .write(
            "/agent/1/io/input",
            format!("alan-input-v1\n{frame}").as_bytes(),
        )
        .await
        .unwrap();
    let admitted = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let history = crate::rollout::RolloutRecorder::load_history(&path)
                .await
                .unwrap();
            let payloads: Vec<_> = history
                .into_iter()
                .filter_map(|item| match item {
                    crate::rollout::RolloutItem::Event(event)
                        if event.event_type == "machine_input_admitted_v1"
                            && (event.payload["id"] == queued.id
                                || event.payload["id"] == file_id) =>
                    {
                        Some(event.payload)
                    }
                    _ => None,
                })
                .collect();
            if payloads.len() == 2 {
                break payloads;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        mock.recorded_requests().len(),
        1,
        "queued inputs must not dispatch while A is active"
    );
    let projected: alan_agent_protocol::UiModelSnapshot =
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot: alan_agent_protocol::UiModelSnapshot =
                    serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await.unwrap())
                        .unwrap();
                if snapshot
                    .admitted
                    .iter()
                    .any(|entry| entry.submission_id == queued.id)
                    && snapshot
                        .admitted
                        .iter()
                        .any(|entry| entry.submission_id == file_id)
                {
                    break snapshot;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    assert!(projected.is_valid());
    assert_eq!(projected.active.as_ref().unwrap().model, "gpt-5.4");
    assert_eq!(projected.selected_next.as_ref().unwrap().model, "gpt-5.4");
    assert!(
        projected
            .admitted
            .iter()
            .all(|entry| entry.binding.as_ref().unwrap().model == "gpt-5.4")
    );
    assert!(
        !serde_json::to_string(&projected)
            .unwrap()
            .contains("credential")
    );
    // Stop the real Runtime before assertions so failure does not leave tasks alive.
    runtime.shutdown().await.unwrap();
    release.notify_one();
    for payload in admitted {
        assert!(
            payload
                .get("callable_binding")
                .is_some_and(|binding| !binding.is_null()),
            "acknowledged input {} lacks durable callable capture: {payload}",
            payload["id"]
        );
        assert!(
            payload
                .get("request_controls")
                .is_some_and(|controls| !controls.is_null()),
            "admission lacks resolved controls: {payload}"
        );
    }
}
