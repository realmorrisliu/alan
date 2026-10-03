//! Real Tool-gated inband consumption, distinct from the retained late-arrival case.
use super::*;
use crate::runtime::transition::tests::tool_batch::create_test_state_with_machine_tools_and_provider;
use crate::tools::{Tool, ToolContext, ToolRegistry, ToolResult};

struct GateTool {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Tool for GateTool {
    fn name(&self) -> &str {
        "steer_gate"
    }
    fn description(&self) -> &str {
        "Gate the real Tool boundary before next generation"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object"})
    }
    fn capability(&self, _: &serde_json::Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
    fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
        let started = self.started.clone();
        let release = self.release.clone();
        Box::pin(async move {
            started.notify_one();
            release.notified().await;
            Ok(serde_json::json!({"ok":true}))
        })
    }
}

#[tokio::test]
async fn admission_recovery_live_compatible_steer_consumed_by_next_actual_a_request() {
    let temp = TempDir::new().unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let mut tools = ToolRegistry::new();
    tools.register(GateTool {
        started: started.clone(),
        release: release.clone(),
    });
    let state = create_test_state_with_machine_tools_and_provider(
        AgentMachine::new(),
        tools,
        MockLlmProvider::new(),
    )
    .await;
    let mut first = mock_generation_response("");
    first.tool_calls.push(alan_llm::ToolCall {
        id: Some("real-gated-call".into()),
        name: "steer_gate".into(),
        arguments: serde_json::json!({}),
    });
    let a = MockLlmProvider::new().with_responses(vec![
        first,
        mock_generation_response("A completed with steering"),
    ]);
    let b = MockLlmProvider::new().with_response(mock_generation_response("B next input"));
    let registry = alan_llmfs::LlmFs::new();
    registry.register_connection("A", Box::new(a.clone()));
    registry.register_connection("B", Box::new(b.clone()));
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let make_callable = |name: &str| {
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(Arc::new(registry.connection_snapshot(name))),
            alan_kernel::Access::ReadWrite,
        );
        CapturedCallable {
            identity: CallableIdentity {
                profile: "managed".into(),
                provider: "openai_responses".into(),
                model: name.into(),
                credential_ref: None,
                revision: name.into(),
            },
            root: InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
            connection: name.into(),
            config: core.clone(),
        }
    };
    let callable_a = make_callable("A");
    let callable_b = make_callable("B");
    let env = state
        .environment
        .with_connection_authority(Arc::new(LiveAuthority {
            a: callable_a.clone(),
            b: callable_b.clone(),
        }));
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores(&temp)),
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
    let active = Submission::new(Op::Input {
        parts: vec![ContentPart::text("active A")],
        mode: InputMode::FollowUp,
    });
    let steer = Submission {
        id: "exact-live-compatible-steer".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("unique inband steering instruction")],
            mode: InputMode::Steer,
        },
    };
    let select = Submission::new(Op::SelectModel { model: "B".into() });
    let next = Submission::new(Op::Input {
        parts: vec![ContentPart::text("next input on B")],
        mode: InputMode::FollowUp,
    });
    let result: anyhow::Result<()> = async {
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        anyhow::ensure!(
            a.recorded_requests().len() == 1 && b.recorded_requests().is_empty(),
            "first actual A request before Tool gate"
        );
        runtime.handle.submission_tx.send(steer.clone()).await?;
        runtime.handle.submission_tx.send(steer.clone()).await?;
        // Same FIFO receiver, serialized observer: completed selection proves the
        // previous admit_during_submission (including broker.push) has returned.
        runtime.handle.submission_tx.send(select.clone()).await?;
        anyhow::ensure!(
            settlement(&shell, &select.id).await?.0
                == alan_agent_protocol::UiInputStatus::Completed,
            "selection barrier failed"
        );
        anyhow::ensure!(
            env.model_bindings
                .lock()
                .await
                .confirmed
                .as_ref()
                .unwrap()
                .identity
                == callable_b.identity,
            "current selection must be B before release"
        );
        let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
        let admitted = history
            .iter()
            .find_map(|item| match item {
                crate::rollout::RolloutItem::Event(e)
                    if e.event_type == "machine_input_admitted_v1"
                        && e.payload["id"] == steer.id =>
                {
                    Some(&e.payload)
                }
                _ => None,
            })
            .context("Steer durable admission missing")?;
        anyhow::ensure!(
            admitted["callable_binding"] == serde_json::to_value(&callable_a.identity)?,
            "Steer captured A"
        );
        anyhow::ensure!(
            admitted["op"] == serde_json::to_value(&steer.op)?,
            "no FollowUp conversion"
        );
        anyhow::ensure!(
            history.iter().filter(|item| matches!(item,
                crate::rollout::RolloutItem::Event(e)
                    if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == steer.id
            )).count() == 1,
            "one durable admission for repeated unsettled Steer"
        );
        let receipt: alan_agent_protocol::UiQueueSnapshot =
            serde_json::from_slice(&shell.cat("/agent/1/machine/ui/queue").await?)?;
        anyhow::ensure!(
            receipt
                .pending_submission_ids
                .iter()
                .filter(|id| *id == &steer.id)
                .count()
                == 1,
            "one pending Steer receipt before Tool release"
        );
        anyhow::ensure!(
            env.model_bindings.lock().await.captured[&steer.id].identity == callable_a.identity,
            "original captured callable remains intact"
        );
        let expected_controls = crate::resolve_runtime_request_controls(
            &core,
            crate::provider_capabilities_for_config(&core),
            Default::default(),
        )?;
        anyhow::ensure!(
            admitted["request_controls"] == serde_json::to_value(expected_controls)?,
            "original full controls remain intact"
        );
        runtime.handle.submission_tx.send(next.clone()).await?;
        release.notify_one();
        anyhow::ensure!(
            settlement(&shell, &steer.id).await?.0 == alan_agent_protocol::UiInputStatus::Completed,
            "inband Steer failed"
        );
        anyhow::ensure!(
            settlement(&shell, &active.id).await?.0
                == alan_agent_protocol::UiInputStatus::Completed,
            "active A failed"
        );
        anyhow::ensure!(
            settlement(&shell, &next.id).await?.0 == alan_agent_protocol::UiInputStatus::Completed,
            "next B failed"
        );
        Ok(())
    }
    .await;
    release.notify_one();
    runtime.shutdown().await.unwrap();
    result.unwrap();
    let requests = a.recorded_requests();
    assert_eq!(
        requests.len(),
        2,
        "first Tool request then actual A continuation"
    );
    assert!(
        !requests[0]
            .messages
            .iter()
            .any(|m| m.content.contains("unique inband steering instruction"))
    );
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|m| m.content.contains("unique inband steering instruction"))
    );
    assert_eq!(
        requests[1]
            .messages
            .iter()
            .map(|m| m
                .content
                .matches("unique inband steering instruction")
                .count())
            .sum::<usize>(),
        1,
        "duplicate external Steer must enter actual generation exactly once"
    );
    assert_eq!(
        b.recorded_requests().len(),
        1,
        "only next standalone input uses current B"
    );
    let records: Vec<serde_json::Value> =
        String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(
        records
            .iter()
            .filter(|r| r["role"] == "user"
                && r["submission_id"] == steer.id
                && r["content"] == "unique inband steering instruction")
            .count(),
        1,
        "structured Tape association with exact ID"
    );
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    let dispatch_ids: Vec<_> = history
        .iter()
        .filter_map(|item| match item {
            crate::rollout::RolloutItem::Event(e)
                if e.event_type == "machine_input_dispatched_v1" =>
            {
                Some(e.payload["submission_id"].as_str().unwrap().to_owned())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        dispatch_ids,
        vec![active.id.clone(), steer.id.clone(), next.id.clone()],
        "one durable consumption per input, inband Steer between A and next B"
    );
    let events: Vec<alan_agent_protocol::UiEvent> =
        String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    let steer_settlements: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            alan_agent_protocol::UiEvent::InputCompleted {
                submission_ids,
                status,
                ..
            } if submission_ids.contains(&steer.id) => Some((submission_ids, status)),
            _ => None,
        })
        .collect();
    assert_eq!(steer_settlements.len(), 1);
    assert_eq!(
        steer_settlements[0].1,
        &alan_agent_protocol::UiInputStatus::Completed
    );
    assert_eq!(
        steer_settlements[0]
            .0
            .iter()
            .cloned()
            .collect::<std::collections::HashSet<_>>(),
        std::collections::HashSet::from([active.id, steer.id]),
        "Steer co-settles with active A, never a standalone Turn or next B"
    );
}

struct LiveAuthority {
    a: CapturedCallable,
    b: CapturedCallable,
}
#[async_trait]
impl ConnectionAuthority for LiveAuthority {
    async fn capture(&self, model: Option<&str>) -> anyhow::Result<CapturedCallable> {
        Ok(if model == Some("B") {
            self.b.clone()
        } else {
            self.a.clone()
        })
    }
    async fn restore(&self, identity: &CallableIdentity) -> anyhow::Result<CapturedCallable> {
        let callable = if identity == &self.a.identity {
            &self.a
        } else {
            &self.b
        };
        anyhow::ensure!(identity == &callable.identity, "exact restore unavailable");
        Ok(callable.clone())
    }
    async fn catalog(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
}
