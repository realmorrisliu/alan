//! A recovered budget wait has a real retained model even without an active input.
use super::*;
use crate::runtime::model_binding::InputBinding;

async fn pending_budget(shell: &alan_shell::Shell, pid: u32) -> anyhow::Result<String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            for id in shell.ls(&format!("/agent/{pid}/requests")).await? {
                if id == "clone" || id == "events" {
                    continue;
                }
                let path = format!("/agent/{pid}/requests/{id}");
                if shell.cat(&format!("{path}/kind")).await? == b"task_budget_extension"
                    && shell.cat(&format!("{path}/status")).await? == b"pending"
                {
                    return Ok::<_, anyhow::Error>(id);
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await?
}

#[tokio::test]
async fn recovered_budget_wait_projects_original_model_and_clears_after_rejection() {
    recovered_budget_wait_projection(false).await;
}

#[tokio::test]
async fn recovered_budget_wait_projects_original_model_and_clears_after_extension() {
    recovered_budget_wait_projection(true).await;
}

async fn recovered_budget_wait_projection(approve: bool) {
    let temp = TempDir::new().unwrap();
    let mut response = reviewer_response("allow");
    response.content.clear();
    response.tool_calls = vec![alan_llm::ToolCall {
        id: Some("retained-plan".into()),
        name: "update_plan".into(),
        arguments: serde_json::json!({"plan":[{"step":"inspect","status":"pending"}]}),
    }];
    let a = MockLlmProvider::new().with_responses(vec![
        response,
        GenerationResponse {
            content: "Plan prepared".into(),
            tool_calls: vec![],
            ..reviewer_response("allow")
        },
    ]);
    let b = MockLlmProvider::new();
    let llmfs = alan_llmfs::LlmFs::new();
    llmfs.register_connection("A", Box::new(a.clone()));
    llmfs.register_connection("B", Box::new(b.clone()));
    let mut namespace = alan_kernel::Namespace::new();
    for pid in [1, 2] {
        namespace.mount(
            &format!("/agent/{pid}"),
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
    }
    namespace.mount(
        "/mnt/llm",
        InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = crate::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    core.task_budget.model_calls = std::num::NonZeroU64::new(1).unwrap();
    let make = |model: &str, effort| {
        let mut config = core.clone();
        config.model_reasoning_effort = Some(effort);
        CapturedCallable {
            identity: CallableIdentity {
                profile: "original-profile".into(),
                provider: "openai_responses".into(),
                model: model.into(),
                credential_ref: Some("PRIVATE_REF".into()),
                revision: "PRIVATE_REV".into(),
            },
            root: root.clone(),
            connection: model.into(),
            config,
        }
    };
    let authority = Arc::new(GatedSelection {
        a: make("A", alan_agent_protocol::ReasoningEffort::Low),
        b: make("B", alan_agent_protocol::ReasoningEffort::High),
        started: Default::default(),
        release: Default::default(),
    });
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let launch = |pid, recovery| {
        let environment =
            NamespaceRuntimeEnvironment::new(root.clone(), format!("/agent/{pid}"), "A")
                .with_connection_authority(authority.clone());
        spawn_with_namespace_environment(
            AgentProcessConfig {
                agent_config: crate::AgentConfig::from(core.clone()),
                store_bindings: Some(stores.clone()),
                recovery_rollout_path: recovery,
                ..Default::default()
            },
            environment,
            crate::skills::SkillHostCapabilities::default(),
            crate::provider_capabilities_for_config(&core),
        )
        .unwrap()
    };
    let mut original = launch(1, None);
    let source = original
        .wait_until_ready()
        .await
        .unwrap()
        .rollout_path
        .unwrap();
    let input = Submission::new(Op::Input {
        parts: vec![ContentPart::text("prepare a plan")],
        mode: InputMode::FollowUp,
    });
    original
        .handle
        .submission_tx
        .send(input.clone())
        .await
        .unwrap();
    let setup: anyhow::Result<()> = async {
        pending_budget(&shell, 1).await?;
        authority.release.notify_one();
        let select = Submission::new(Op::SelectModel { model: "B".into() });
        original.handle.submission_tx.send(select.clone()).await?;
        completed(&shell, &select.id).await?;
        anyhow::ensure!(a.recorded_requests().len() == 1 && b.recorded_requests().is_empty());
        Ok(())
    }
    .await;
    original.shutdown().await.unwrap();
    setup.unwrap();
    let mut recovered = launch(2, Some(source));
    recovered.wait_until_ready().await.unwrap();
    let result: anyhow::Result<()> = async {
        let models: alan_agent_protocol::UiModelSnapshot =
            serde_json::from_slice(&shell.cat("/agent/2/machine/ui/models").await?)?;
        anyhow::ensure!(
            models
                .active
                .as_ref()
                .is_some_and(|binding| binding.model == "A"),
            "recovered budget wait lost original model: {models:?}"
        );
        anyhow::ensure!(
            models.active.as_ref().unwrap().reasoning.effort
                == Some(alan_agent_protocol::ReasoningEffort::Low)
        );
        anyhow::ensure!(models.selected_next.as_ref().unwrap().model == "B");
        anyhow::ensure!(models.admitted.is_empty());
        let queue: alan_agent_protocol::UiQueueSnapshot =
            serde_json::from_slice(&shell.cat("/agent/2/machine/ui/queue").await?)?;
        anyhow::ensure!(queue.active_submission_ids.is_empty());
        let raw = serde_json::to_string(&models)?;
        for private in ["credential_ref", "revision", "PRIVATE_REF", "PRIVATE_REV"] {
            anyhow::ensure!(!raw.contains(private));
        }
        let activity: alan_agent_protocol::UiActivitySnapshot =
            serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await?)?;
        anyhow::ensure!(activity.state != alan_agent_protocol::UiActivityState::Running);
        anyhow::ensure!(a.recorded_requests().len() == 1 && b.recorded_requests().is_empty());
        let request = pending_budget(&shell, 2).await?;
        recovered
            .handle
            .submission_tx
            .send(Submission::new(Op::Resume {
                request_id: request,
                content: vec![ContentPart::structured(
                    serde_json::json!({"choice":if approve {"approve"} else {"reject"}}),
                )],
            }))
            .await?;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let models: alan_agent_protocol::UiModelSnapshot =
                    serde_json::from_slice(&shell.cat("/agent/2/machine/ui/models").await?)?;
                if models.active.is_none() {
                    anyhow::ensure!(models.selected_next.as_ref().unwrap().model == "B");
                    let raw = serde_json::to_string(&models)?;
                    for private in ["credential_ref", "revision", "PRIVATE_REF", "PRIVATE_REV"] {
                        anyhow::ensure!(!raw.contains(private));
                    }
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        anyhow::ensure!(
            a.recorded_requests().len() == if approve { 2 } else { 1 }
                && b.recorded_requests().is_empty()
        );
        anyhow::ensure!(
            a.recorded_requests()
                .iter()
                .all(|request| request.reasoning.effort
                    == Some(alan_agent_protocol::ReasoningEffort::Low))
        );
        Ok(())
    }
    .await;
    recovered.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn budget_wait_model_observation_never_infers_an_unvalidated_binding() {
    let provider = MockLlmProvider::new();
    let mut state = create_test_state_with_machine_tools_and_provider(
        AgentMachine::new(),
        crate::tools::ToolRegistry::new(),
        provider.clone(),
    )
    .await;
    state.core_config.task_budget.model_calls = std::num::NonZeroU64::new(1).unwrap();
    let callable = CapturedCallable {
        identity: CallableIdentity {
            profile: "test".into(),
            provider: "openai_responses".into(),
            model: "A".into(),
            credential_ref: None,
            revision: "private".into(),
        },
        root: state.environment.root_transport(),
        connection: "test".into(),
        config: state.core_config.clone(),
    };
    let binding = InputBinding {
        callable_binding: callable.identity.clone(),
        request_controls: Default::default(),
    };
    *state.environment.active_binding.write().unwrap() = Some((binding.clone(), callable.clone()));
    let mut next = callable;
    next.identity.model = "B".into();
    state.environment.model_bindings.lock().await.confirmed = Some(next);
    state.machine.accept_submission("view-task");
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .reserve(crate::agent_machine::task_budget::Admission::Model)
        .await
        .unwrap();
    crate::runtime::task_budget::pause(
        &mut state,
        &crate::agent_machine::task_budget::Exhausted {
            dimension: crate::agent_machine::task_budget::Dimension::ModelCalls,
        },
        None,
        &mut |_| async {},
    )
    .await
    .unwrap();
    // Match recovered waiting work: no input is currently executing.
    state
        .machine
        .input_queue()
        .lock()
        .unwrap()
        .active_submission_ids
        .clear();
    let wait = state.machine.task_budget_wait.clone().unwrap();
    let mut wrong = binding.clone();
    wrong.callable_binding.model = "B".into();
    let mut queues = RuntimeSubmissionQueues::new(state.machine.input_queue());
    queues.environment = Some(state.environment.clone());
    queues.model_process_path = "/proc/1".into();
    let shell = alan_shell::Shell::new(state.environment.root_transport());
    for value in [
        Some(serde_json::to_value(&binding).unwrap()),
        None,
        Some(serde_json::Value::Null),
        Some(serde_json::json!({"malformed":true})),
        Some(serde_json::to_value(wrong).unwrap()),
    ] {
        state.machine.task_budget_wait = Some(wait.clone());
        queues.observe_budget_wait_model(&state);
        let mut pending = wait.clone();
        pending
            .checkpoint
            .details
            .as_object_mut()
            .unwrap()
            .remove("generation_binding");
        if let Some(value) = &value {
            pending.checkpoint.details["generation_binding"] = value.clone();
        }
        state.machine.task_budget_wait = Some(pending);
        queues.observe_budget_wait_model(&state);
        queues.publish_models().await.unwrap();
        let models = snapshot(&shell).await.unwrap();
        assert_eq!(
            models.active.as_ref().map(|binding| binding.model.as_str()),
            (value == Some(serde_json::to_value(&binding).unwrap())).then_some("A")
        );
        assert_eq!(models.selected_next.as_ref().unwrap().model, "B");
    }
    state.machine.task_budget_wait = None;
    queues.observe_budget_wait_model(&state);
    queues.publish_models().await.unwrap();
    assert!(snapshot(&shell).await.unwrap().active.is_none());
    assert!(provider.recorded_requests().is_empty());
}
