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

struct SteerTapeFaultFs {
    inner: alan_agentfs::AgentFs,
    tape_path: u64,
    failures: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl alan_ap::FileServer for SteerTapeFaultFs {
    async fn walk(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        names: &[String],
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }
    async fn open(
        &self,
        fid: alan_ap::Fid,
        mode: alan_ap::OpenMode,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.open(fid, mode).await
    }
    async fn read(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        count: u32,
    ) -> Result<Vec<u8>, alan_ap::ErrorCode> {
        self.inner.read(fid, offset, count).await
    }
    async fn write(
        &self,
        fid: alan_ap::Fid,
        offset: u64,
        data: &[u8],
    ) -> Result<u32, alan_ap::ErrorCode> {
        if self.inner.stat(fid).await?.qid.path == self.tape_path
            && serde_json::from_slice::<serde_json::Value>(data).is_ok_and(|record| {
                record["role"] == "user" && record["submission_id"] == "tape-fault-steer"
            })
        {
            self.failures
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(alan_ap::ErrorCode::Io);
        }
        self.inner.write(fid, offset, data).await
    }
    async fn stat(&self, fid: alan_ap::Fid) -> Result<alan_ap::Stat, alan_ap::ErrorCode> {
        self.inner.stat(fid).await
    }
    async fn create(
        &self,
        fid: alan_ap::Fid,
        newfid: alan_ap::Fid,
        name: &str,
        kind: alan_ap::FileKind,
    ) -> Result<alan_ap::Qid, alan_ap::ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }
    async fn remove(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.remove(fid).await
    }
    async fn clunk(&self, fid: alan_ap::Fid) -> Result<(), alan_ap::ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn steering_tape_failure_settles_dispatched_input_with_original_turn() {
    use alan_ap::FileServer;
    let temp = TempDir::new().unwrap();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let mut tools = ToolRegistry::new();
    tools.register(GateTool {
        started: started.clone(),
        release: release.clone(),
    });
    let mut first = mock_generation_response("");
    first.tool_calls.push(alan_llm::ToolCall {
        id: Some("tape-fault-gate".into()),
        name: "steer_gate".into(),
        arguments: serde_json::json!({}),
    });
    let provider = MockLlmProvider::new().with_responses(vec![
        first,
        mock_generation_response("must not generate after failed Tape append"),
    ]);
    let state = create_test_state_with_machine_tools_and_provider(
        AgentMachine::new(),
        tools,
        provider.clone(),
    )
    .await;
    let inner = alan_agentfs::AgentFs::new();
    let tape_path = inner
        .walk(
            alan_ap::Fid::ROOT,
            alan_ap::Fid(9),
            &["machine".into(), "tape".into()],
        )
        .await
        .unwrap()
        .path;
    inner.clunk(alan_ap::Fid(9)).await.unwrap();
    let fault = Arc::new(SteerTapeFaultFs {
        inner,
        tape_path,
        failures: Default::default(),
    });
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/",
        state.environment.root_transport(),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let env = NamespaceRuntimeEnvironment::new(
        InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns))),
        "/agent/1",
        "default",
    )
    .with_namespace_cwd("/mnt/source");
    let shell = alan_shell::Shell::new(env.root_transport());
    let mut core = state.core_config;
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            store_bindings: Some(stores(&temp)),
            ..Default::default()
        },
        env,
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
        parts: vec![ContentPart::text("start gated turn")],
        mode: InputMode::FollowUp,
    });
    let steer = Submission {
        id: "tape-fault-steer".into(),
        intent: Default::default(),
        op: Op::Input {
            parts: vec![ContentPart::text("never replay this steering")],
            mode: InputMode::Steer,
        },
    };
    let result: anyhow::Result<()> = async {
        runtime.handle.submission_tx.send(active.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), started.notified()).await?;
        runtime.handle.submission_tx.send(steer.clone()).await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let history = crate::rollout::RolloutRecorder::load_history(&path).await?;
                if history.iter().any(|item| matches!(item,
                    crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_admitted_v1" && e.payload["id"] == steer.id)) {
                    return anyhow::Ok(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await??;
        // The observer processes this control after the prior intake has also
        // pushed Steer into its broker. Durable admission alone precedes that push.
        let barrier = Submission::new(Op::SelectModel { model: "unavailable-barrier".into() });
        runtime.handle.submission_tx.send(barrier.clone()).await?;
        anyhow::ensure!(settlement(&shell, &barrier.id).await?.0 == alan_agent_protocol::UiInputStatus::Failed,
            "no-authority selection barrier must not change the callable");
        release.notify_one();
        anyhow::ensure!(settlement(&shell, &active.id).await?.0 == alan_agent_protocol::UiInputStatus::Failed,
            "original input must fail at Tape boundary");
        anyhow::ensure!(fault.failures.load(std::sync::atomic::Ordering::SeqCst) == 1,
            "actual steering Tape write fault must execute exactly once");
        anyhow::ensure!(settlement(&shell, &steer.id).await.context("missing Failed receipt for durably dispatched steering after actual Tape fault")?.0 == alan_agent_protocol::UiInputStatus::Failed,
            "durably dispatched steering must share original failed settlement");
        let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await?)?;
        anyhow::ensure!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line),
            Ok(alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, .. })
                if submission_ids.contains(&active.id) && submission_ids.contains(&steer.id))), "same failed turn owns both IDs");
        Ok(())
    }.await;
    release.notify_one();
    runtime.shutdown().await.unwrap();
    result.unwrap();
    assert_eq!(
        provider.recorded_requests().len(),
        1,
        "no continuation request after Tape failure"
    );
    let history = crate::rollout::RolloutRecorder::load_history(&path)
        .await
        .unwrap();
    assert_eq!(history.iter().filter(|item| matches!(item,
        crate::rollout::RolloutItem::Event(e) if e.event_type == "machine_input_dispatched_v1" && e.payload["submission_id"] == steer.id)).count(), 1);
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/proc/2", "test", temp.path())
        .await
        .unwrap();
    assert!(
        recovered.input_queue().lock().unwrap().pending.is_empty(),
        "dispatched IDs must not replay on recovery"
    );
}
