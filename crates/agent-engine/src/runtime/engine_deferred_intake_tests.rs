//! Fresh API and namespace intake during a real bounded turn-end promotion.
use super::*;

#[tokio::test]
async fn deferred_duplicate_api_intake_keeps_original_capture_once() {
    duplicate_during_promotion(false).await;
}

#[tokio::test]
async fn deferred_duplicate_namespace_intake_keeps_original_capture_once() {
    duplicate_during_promotion(true).await;
}

async fn duplicate_during_promotion(namespace: bool) {
    let temp = TempDir::new().unwrap();
    let memory = temp.path().join("memory");
    crate::prompts::ensure_memory_store_layout_at(&memory).unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let a = MockLlmProvider::new().with_responses(vec![
        mock_generation_response("Noted."),
        mock_generation_response("{\"writes\":[]}"),
        mock_generation_response("duplicate input executed once"),
        mock_generation_response("{\"writes\":[]}"),
        mock_generation_response("{\"writes\":[]}"),
    ]);
    let b = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection(
        "A",
        Box::new(GatedPromotion {
            mock: a.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    registry.register_connection("B", Box::new(b.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(registry)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = crate::Config::default();
    core.memory.enabled = true;
    core.memory.store_dir = Some(memory.clone());
    core.streaming_mode = crate::config::StreamingMode::Off;
    core.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Medium);
    let make = |model: &str| CapturedCallable {
        identity: CallableIdentity {
            profile: "managed".into(),
            provider: "chatgpt".into(),
            model: model.into(),
            credential_ref: Some("exact-ref".into()),
            revision: "exact-rev".into(),
        },
        root: root.clone(),
        connection: model.into(),
        config: core.clone(),
    };
    let original = make("A");
    let authority = Arc::new(GatedSelection {
        a: original.clone(),
        b: make("B"),
        started: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    });
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "A")
        .with_connection_authority(authority.clone());
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            memory_store_bound: true,
            memory_store_backing: Some(memory),
            store_bindings: Some(stores),
            ..Default::default()
        },
        env.clone(),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let path = runtime
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let initial = Submission::new(Op::Turn {
        parts: vec![ContentPart::text("My name is Morris.")],
        context: None,
    });
    let duplicate = Submission::new(Op::Input {
        parts: vec![ContentPart::text("exact deferred duplicate payload")],
        mode: InputMode::FollowUp,
    });
    let selection = Submission::new(Op::SelectModel { model: "B".into() });
    let controls = crate::resolve_runtime_request_controls(
        &core,
        crate::provider_capabilities_for_config(&core),
        crate::RequestControlIntent::from_config(&core),
    )
    .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let result: anyhow::Result<()> = async {
            runtime.handle.submission_tx.send(initial.clone()).await?;
            tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
            completed(&shell, &initial.id).await?;
            anyhow::ensure!(a.recorded_requests().len() == 2, "real promotion is active");
            if namespace {
                let frame = format!(
                    "alan-input-v1\n{}",
                    serde_json::json!({
                        "version":1, "submission_id":duplicate.id, "intent":"agent",
                        "mode":"follow_up", "body":"exact deferred duplicate payload"
                    })
                );
                shell.write("/agent/1/io/input", frame.as_bytes()).await?;
                shell.write("/agent/1/io/input", frame.as_bytes()).await?;
                let frame = format!("select-model {} B", selection.id);
                shell
                    .write("/agent/1/machine/ctl", frame.as_bytes())
                    .await?;
            } else {
                runtime.handle.submission_tx.try_send(duplicate.clone())?;
                runtime.handle.submission_tx.try_send(duplicate.clone())?;
                runtime.handle.submission_tx.try_send(selection.clone())?;
            }
            // The serialized observer is blocked in selection after both deliveries.
            // Main dispatch cannot run until that control finishes, even after cancellation.
            tokio::time::timeout(Duration::from_secs(5), authority.started.notified()).await?;
            let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
            let admissions: Vec<_> = history
                .iter()
                .filter_map(|item| match item {
                    crate::rollout::RolloutItem::Event(e)
                        if e.event_type == "machine_input_admitted_v1"
                            && e.payload["id"] == duplicate.id =>
                    {
                        Some(&e.payload)
                    }
                    _ => None,
                })
                .collect();
            anyhow::ensure!(
                admissions.len() == 1,
                "deferred fresh intake must capture/admit once before enqueue, got {}",
                admissions.len()
            );
            anyhow::ensure!(admissions[0]["op"] == serde_json::to_value(&duplicate.op)?);
            anyhow::ensure!(
                admissions[0]["callable_binding"] == serde_json::to_value(&original.identity)?
            );
            anyhow::ensure!(admissions[0]["request_controls"] == serde_json::to_value(&controls)?);
            let receipt: alan_agent_protocol::UiQueueSnapshot =
                serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await?)?;
            anyhow::ensure!(receipt.pending_submission_ids == vec![duplicate.id.clone()]);
            // Release provider startup before unblocking selection/main dispatch.
            release.notify_one();
            authority.release.notify_one();
            completed(&shell, &selection.id).await?;
            anyhow::ensure!(
                env.model_bindings
                    .lock()
                    .await
                    .captured
                    .get(&duplicate.id)
                    .is_none_or(|captured| captured.identity == original.identity),
                "remaining capture must never be overwritten by later selection"
            );
            completed(&shell, &duplicate.id).await?;
            // Include requeued promotion settlement rather than racing shutdown.
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let snapshot: alan_agent_protocol::UiModelSnapshot =
                        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/models").await?)?;
                    let queue: alan_agent_protocol::UiQueueSnapshot =
                        serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await?)?;
                    if snapshot.active.is_none()
                        && !queue.deferred
                        && queue.pending_submission_ids.is_empty()
                    {
                        return Ok::<_, anyhow::Error>(());
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await??;
            Ok(())
        }
        .await;
        result
    })
    .await
    .map_err(|error| anyhow::anyhow!("bounded real deferred duplicate intake scenario: {error}"))
    .and_then(|result| result);
    // Cleanup also runs after scenario timeout/error; never mask the original
    // assertion failure with a secondary shutdown failure.
    release.notify_one();
    authority.release.notify_one();
    let cleanup = tokio::time::timeout(Duration::from_secs(10), runtime.shutdown())
        .await
        .map_err(|error| anyhow::anyhow!("bounded fixture shutdown timed out: {error}"))
        .and_then(|result| result);
    if let Err(error) = result {
        panic!("deferred intake scenario failed: {error:#}; separate cleanup outcome: {cleanup:?}");
    }
    cleanup.expect("fixture cleanup failed after successful scenario");
    assert!(b.recorded_requests().is_empty(), "never recapture later B");
    let requests = a.recorded_requests();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.max_tokens == Some(768))
            .count(),
        3,
        "one cancelled promotion, one new turn promotion, and one retained promotion retry"
    );
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    let settlements: Vec<_> = events
        .lines()
        .filter_map(|line| {
            match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                alan_agent_protocol::UiEvent::InputCompleted {
                    submission_ids,
                    status,
                    ..
                } if submission_ids.contains(&duplicate.id) => Some(status),
                _ => None,
            }
        })
        .collect();
    assert_eq!(
        settlements,
        vec![alan_agent_protocol::UiInputStatus::Completed],
        "one settlement and no false Failed for duplicate delivery"
    );
    let actual: Vec<_> = a
        .recorded_requests()
        .into_iter()
        .filter(|r| {
            r.messages
                .iter()
                .any(|m| m.content == "exact deferred duplicate payload")
                && r.max_tokens != Some(768)
        })
        .collect();
    assert_eq!(actual.len(), 1, "single actual input generation effect");
    assert_eq!(actual[0].reasoning, controls.reasoning);
    assert_eq!(
        actual[0]
            .messages
            .iter()
            .filter(|m| m.content == "exact deferred duplicate payload")
            .count(),
        1
    );
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|item| matches!(item,
        crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1"
            && e.payload["submission_id"] == duplicate.id))
            .count(),
        1
    );
    let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
    assert_eq!(
        tape.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|r| r["role"] == "user"
                && r["submission_id"] == duplicate.id
                && r["content"] == "exact deferred duplicate payload")
            .count(),
        1
    );
}
