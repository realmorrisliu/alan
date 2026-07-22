use super::*;

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.target,
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        }
    );
    assert_eq!(spec.handles, vec![SpawnHandle::ApprovalScope]);
    assert!(spec.host_mounts.is_empty());
    assert_eq!(spec.launch.cwd, None);
    assert_eq!(
        spec.launch.timeout_secs,
        Some(DEFAULT_DELEGATED_TIMEOUT_SECS)
    );
    let requirements = &spec
        .delegated
        .as_ref()
        .expect("delegated launch should carry classified requirements")
        .requirements;
    assert!(!requirements.iter().any(|requirement| matches!(
        requirement,
        alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
            path: Some(path)
        } if path == std::path::Path::new("/mnt/source")
    )));
    assert!(
        requirements.contains(&alan_agent_protocol::DelegatedCapabilityRequirement::LlmConnection)
    );

    let prompt_view = state.machine.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("\"task\":\"Review the current diff and summarize risks.\""));
    assert!(tool_result.contains("\"status\":\"completed\""));
    assert!(tool_result.contains("Delegated review completed."));
    assert!(tool_result.contains("child_run"));
    assert!(tool_result.contains("child-machine"));
}

#[tokio::test]
async fn test_delegated_capability_rejection_is_recorded_on_parent_tape() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let tool_call = NormalizedToolCall {
        id: "call_capability_mismatch".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review GitHub issue 42"
        }),
    };
    let decision = alan_agent_protocol::DelegatedCapabilityDecision {
        requirements: vec![alan_agent_protocol::DelegatedCapabilityRequirement::Github],
        namespace: alan_agent_protocol::DelegatedNamespaceSummary::default(),
        unsatisfied: vec![alan_agent_protocol::DelegatedCapabilityRequirement::Github],
        recovery: alan_agent_protocol::DelegatedCapabilityRecovery::ParentPath,
        narrowed_task: None,
    };
    let expected_decision = decision.clone();
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel| {
            Box::pin(async move { Err(anyhow::Error::new(DelegatedSpawnRejected { decision })) })
        },
    )
    .await
    .unwrap();

    let tool_result = tool_result_text_for_call(&state, "call_capability_mismatch");
    let record: DelegatedSkillInvocationRecord = serde_json::from_str(&tool_result).unwrap();
    assert_eq!(
        record.result.error_kind.as_deref(),
        Some("delegated_capability_mismatch")
    );
    assert_eq!(record.result.capability_decision, Some(expected_decision));
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_from_catalog_without_activation() {
    let temp = TempDir::new().unwrap();
    let package_store = temp.path().join("package-store");
    let skill_dir = package_store.join("repo-review");
    std::fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Repo Review
description: Review repository changes
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("agents/reviewer/agent.toml"),
        "openai_responses_model = \"gpt-5.4\"\n",
    )
    .unwrap();

    let mut state = create_test_transition_state();
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_package_store(&package_store),
            Vec::new(),
            SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok());
    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.target,
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        }
    );
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_rejects_when_runtime_support_is_disabled()
 {
    let temp = TempDir::new().unwrap();
    let package_store = temp.path().join("package-store");
    let skill_dir = package_store.join("repo-review");
    std::fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Repo Review
description: Review repository changes
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("agents/reviewer/agent.toml"),
        "openai_responses_model = \"gpt-5.4\"\n",
    )
    .unwrap();

    let mut state = create_test_transition_state();
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_package_store(&package_store),
            Vec::new(),
            SkillHostCapabilities::default().with_runtime_defaults(),
        );

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            panic!("unsupported runtimes must not spawn delegated runtimes");
            #[allow(
                unreachable_code,
                reason = "the typed future after panic satisfies the injected spawn closure signature"
            )]
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: String::new(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let prompt_view = state.machine.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("delegated_invocation_unavailable"));
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_records_successful_tool_call() {
    let temp = TempDir::new().unwrap();
    let mut state = create_test_transition_state();
    state.machine = AgentMachine::new_with_recorder_in_dir("/proc/test", "gpt-5-mini", temp.path())
        .await
        .unwrap();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let rollout_path = state.machine.rollout_path().unwrap().clone();
    let mut tool_call = None;
    for _ in 0..20 {
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        tool_call = items.into_iter().find_map(|item| match item {
            RolloutItem::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        });
        if tool_call.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let tool_call = tool_call.expect("expected delegated tool-call rollout record");
    assert_eq!(tool_call.name, "invoke_delegated_skill");
    assert!(tool_call.success);
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_records_normalized_namespace_cwd() {
    let temp = TempDir::new().unwrap();
    let mut state = create_test_transition_state();
    state.machine = AgentMachine::new_with_recorder_in_dir("/proc/test", "gpt-5-mini", temp.path())
        .await
        .unwrap();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Read docs and explain full steward mode.",
            "cwd": "docs"
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let rollout_path = state.machine.rollout_path().unwrap().clone();
    let mut recorded_tool_call = None;
    for _ in 0..20 {
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        recorded_tool_call = items.into_iter().find_map(|item| match item {
            RolloutItem::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        });
        if recorded_tool_call.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let recorded_tool_call = recorded_tool_call.expect("expected delegated tool-call record");
    assert!(recorded_tool_call.arguments.get("workspace_root").is_none());
    assert_eq!(
        recorded_tool_call.arguments["cwd"],
        json!("/mnt/source/docs")
    );
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_bounds_preview_and_payload() {
    let mut state = create_test_transition_state();
    let long_skill_id = format!("repo-review-{}", "x".repeat(150));
    let long_target = format!("reviewer-{}", "y".repeat(150));
    let long_task = "Review the current diff and summarize risks. ".repeat(80);
    activate_test_delegated_skill(&mut state, &long_skill_id, &long_target);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": long_skill_id,
            "target": long_target,
            "task": long_task
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("delegated-result ".repeat(40)),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let preview = events
        .iter()
        .find_map(|event| match event {
            Event::ToolCallCompleted {
                id,
                result_preview: Some(preview),
                ..
            } if id == "call_1" => Some(preview.as_str()),
            _ => None,
        })
        .expect("expected delegated skill preview");
    assert!(preview.chars().count() <= 163);
    assert!(preview.ends_with("..."));

    let prompt_view = state.machine.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected delegated skill tool result");
    let payload: serde_json::Value = serde_json::from_str(&tool_result).unwrap();
    assert!(payload["skill_id"].as_str().unwrap().chars().count() <= MAX_DELEGATED_SKILL_ID_CHARS);
    assert!(payload["target"].as_str().unwrap().chars().count() <= MAX_DELEGATED_TARGET_CHARS);
    assert!(payload["task"].as_str().unwrap().chars().count() <= MAX_DELEGATED_TASK_CHARS);
    assert!(
        payload["result"]["summary"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= MAX_DELEGATED_RESULT_SUMMARY_CHARS
    );
}

#[tokio::test]
async fn test_pre_cancelled_delegated_invocation_does_not_project_or_spawn_child_runtime() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state.machine.set_turn_activity(TurnActivityState::Running);
    let tool_call = NormalizedToolCall {
        id: "call_pre_cancelled".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff."
        }),
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut emit = |_event: Event| async {};

    let outcome = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_runtime, _spec, _cancel| {
            Box::pin(async { panic!("pre-cancelled launch must not reach the spawn boundary") })
        },
    )
    .await
    .unwrap();

    assert_eq!(outcome, VirtualToolOutcome::EndTurn);
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_honors_interrupt() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state.machine.set_turn_activity(TurnActivityState::Running);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            let cancel_for_task = cancel_for_task.clone();
            Box::pin(async move {
                cancel_for_task.cancelled().await;
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Cancelled,
                    process_path: "child-machine".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    );
    tokio::task::yield_now().await;
    cancel.cancel();

    let result = result.await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnCompleted { summary: Some(summary) } if summary == "Task cancelled by user"
    )));

    let prompt_view = state.machine.prompt_view();
    assert!(!prompt_view.messages.iter().any(|message| matches!(
        message,
        crate::tape::Message::Tool { responses }
            if responses.iter().any(|response| response.id == "call_1")
    )));
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invoke_delegated_skill_honors_interrupt_during_startup() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state.machine.set_turn_activity(TurnActivityState::Running);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            let cancel_for_task = cancel_for_task.clone();
            Box::pin(async move {
                cancel_for_task.cancelled().await;
                Err(anyhow::anyhow!("Child-agent launch cancelled"))
            })
        },
    );
    tokio::task::yield_now().await;
    cancel.cancel();

    let result = result.await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnCompleted { summary: Some(summary) } if summary == "Task cancelled by user"
    )));

    let prompt_view = state.machine.prompt_view();
    assert!(!prompt_view.messages.iter().any(|message| matches!(
        message,
        crate::tape::Message::Tool { responses }
            if responses.iter().any(|response| response.id == "call_1")
    )));
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_invalid_delegated_skill_request() {
    let mut state = create_test_transition_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Error {
                message,
                recoverable: true
            } if message.contains("delegated skill invocation")
        )
    }));
}

#[tokio::test]
async fn test_dispatch_virtual_tool_call_rejects_target_mismatch() {
    let mut state = create_test_transition_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "grader",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill_with_spawn(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            panic!("target mismatch should not attempt child launch");
            #[allow(
                unreachable_code,
                reason = "the typed future after panic satisfies the injected spawn closure signature"
            )]
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: String::new(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let prompt_view = state.machine.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("\"status\":\"failed\""));
    assert!(tool_result.contains("delegate_target_mismatch"));
    assert!(tool_result.contains("\"resolved_target\":\"reviewer\""));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { id, .. } if id == "call_1"
    )));
}
