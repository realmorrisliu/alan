use super::*;

#[tokio::test]
async fn owner_completion_waits_for_descendant_cleanup_before_agentfs_idle() {
    let (_dir, mut state, reads, calls, generation) =
        fixture(Some("hostfs"), None, usize::MAX).await;
    let input = control("Which crate defines HostDirFs?");
    state.machine.accept_submission(input.id.clone());
    crate::runtime::task_budget::begin(&mut state)
        .await
        .unwrap();
    let parent = state.machine.task_budget.clone().unwrap();
    let (cleanup, release, child) = super::super::completion_budget::held_descendant(parent).await;
    let files = state.agent_files();
    let transition = submit(&mut state, input);
    let inspect = async {
        cleanup.notified().await;
        let activity = files.read_ui_activity_snapshot().await.unwrap();
        release.notify_one();
        activity.state
    };
    let (result, during_cleanup) =
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(transition, inspect)
        })
        .await
        .unwrap();
    result.unwrap();
    assert!(child.await.unwrap().is_err());
    assert_ne!(during_cleanup, alan_agent_protocol::UiActivityState::Idle);
    assert_eq!(
        files.read_ui_activity_snapshot().await.unwrap().state,
        alan_agent_protocol::UiActivityState::Idle
    );
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn owner_budget_extension_resumes_the_program_without_repeating_source_reads() {
    let (_dir, mut state, reads, calls, generation) =
        fixture(Some("hostfs"), None, usize::MAX).await;
    state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(1).unwrap();
    let input = control("Which crate defines HostDirFs?");
    state.machine.accept_submission(input.id.clone());
    handle_submission_with_cancel(
        &mut state,
        input.clone(),
        &mut |_| async {},
        &CancellationToken::new(),
    )
    .await
    .expect("exhaustion pauses the owner program, rather than failing it");
    for expected_reads in [1, 2] {
        assert_eq!(reads.load(Ordering::SeqCst), expected_reads);
        assert!(state.machine.task_budget_wait.is_some());
        let request_id = state
            .machine
            .pending_request_ids()
            .into_iter()
            .next()
            .unwrap();
        handle_submission_with_cancel(
            &mut state,
            Submission::new(Op::Resume {
                request_id,
                content: vec![alan_agent_protocol::ContentPart::structured(
                    json!({"choice":"approve"}),
                )],
            }),
            &mut |_| async {},
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    }
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(generation.recorded_requests().is_empty());
    assert!(matches!(
        &state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Completed { owner } if owner == "hostfs"
    ));
    assert!(!state.machine.has_pending_interaction());
    assert_eq!(state.machine.owner_work.as_ref().unwrap().work_id, input.id);
    assert_eq!(
        state
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .tool_calls,
        3
    );
}

async fn submit(state: &mut RuntimeLoopState, input: Submission) -> Result<()> {
    handle_submission_with_cancel(state, input, &mut |_| async {}, &CancellationToken::new()).await
}

fn budget_reply(state: &RuntimeLoopState, choice: &str) -> Submission {
    Submission::new(Op::Resume {
        request_id: state
            .machine
            .pending_request_ids()
            .into_iter()
            .next()
            .unwrap(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            json!({"choice":choice}),
        )],
    })
}

#[tokio::test]
async fn owner_budget_recovery_preserves_capture_evaluation_and_fallback_progress() {
    for (literal, tools, models, fallback) in [
        (true, 1, 64, false),
        (false, 2, 64, false),
        (false, 64, 1, true),
        (false, 64, 1, false),
    ] {
        let selection = if fallback { None } else { Some("hostfs") };
        let quote = fallback.then_some(1000);
        let (dir, mut state, reads, calls, generation) =
            fixture(selection, quote, usize::MAX).await;
        state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(tools).unwrap();
        state.core_config.task_budget.model_calls = std::num::NonZeroU64::new(models).unwrap();
        let input = control(if literal {
            "Which crate defines HostDirFs?"
        } else {
            "Who revokes old fids?"
        });
        state.machine.accept_submission(input.id.clone());
        submit(&mut state, input.clone()).await.unwrap();
        let original = state.machine.owner_work.clone().unwrap();
        assert_eq!(
            original.outcome,
            crate::agent_machine::owner_work::Outcome::BudgetPaused
        );
        let old_request = state.machine.pending_request_ids()[0].clone();
        state.machine.flush_recorder().await.unwrap();
        let path = state.machine.input_recorder().unwrap().path().to_path_buf();
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/proc/9", "mock-model", dir.path())
                .await
                .unwrap();
        assert_eq!(recovered.owner_work.as_ref(), Some(&original));
        let evaluator = evaluator_callable(&recovered, calls.clone(), selection);
        let mut registry = ToolRegistry::new();
        registry.register(SourceTool {
            reads: reads.clone(),
            revoke_after: usize::MAX,
        });
        let mut resumed = tool_batch::create_test_state_with_machine_tools_provider_and_agent_path(
            recovered,
            registry,
            generation.clone(),
            "/agent/9",
        )
        .await;
        resumed.core_config = state.core_config.clone();
        let (binding, mut callable) = state
            .environment
            .active_binding
            .read()
            .unwrap()
            .clone()
            .unwrap();
        callable.root = resumed.environment.root_transport();
        *resumed.environment.active_binding.write().unwrap() = Some((binding, callable));
        resumed.environment = resumed
            .environment
            .with_connection_authority(Arc::new(Authority {
                captured: evaluator,
                quote,
                close_before_wait: None,
            }));
        crate::runtime::task_budget::publish_wait(&mut resumed)
            .await
            .unwrap();
        assert_eq!(resumed.environment.agent_path(), "/agent/9");
        assert_ne!(
            resumed.machine.rollout_id(),
            Some(original.source_rollout_id.as_str())
        );
        for _ in 0..5 {
            if resumed.machine.task_budget_wait.is_none() {
                break;
            }
            let reply = budget_reply(&resumed, "approve");
            submit(&mut resumed, reply).await.unwrap();
        }
        assert!(!resumed.machine.has_pending_interaction());
        let work = resumed.machine.owner_work.as_ref().unwrap();
        assert!(
            matches!(&work.outcome, crate::agent_machine::owner_work::Outcome::Completed { owner } if owner=="hostfs")
        );
        assert_eq!(work.work_id, input.id);
        assert_eq!(work.source_rollout_id, original.source_rollout_id);
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(!literal));
        assert_eq!(generation.recorded_requests().len(), usize::from(fallback));
        let spent = resumed
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await;
        assert_eq!(spent.tool_calls, 3);
        assert_eq!(spent.model_calls, u64::from(!literal) + u64::from(fallback));
        assert!(spent.foreground_complete);
        let before = (reads.load(Ordering::SeqCst), calls.load(Ordering::SeqCst));
        let _ = submit(
            &mut resumed,
            Submission::new(Op::Resume {
                request_id: old_request,
                content: vec![alan_agent_protocol::ContentPart::text("approve")],
            }),
        )
        .await;
        assert_eq!(
            (reads.load(Ordering::SeqCst), calls.load(Ordering::SeqCst)),
            before
        );
    }
}

#[tokio::test]
async fn owner_budget_decline_and_interrupt_retire_the_unexecuted_read_without_generation() {
    for choice in [Some("reject"), Some(""), None] {
        let (_dir, mut state, reads, calls, generation) =
            fixture(Some("hostfs"), None, usize::MAX).await;
        state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(1).unwrap();
        let input = control("Which crate defines HostDirFs?");
        state.machine.accept_submission(input.id.clone());
        submit(&mut state, input).await.unwrap();
        let pending = state
            .machine
            .owner_work
            .as_ref()
            .unwrap()
            .pending_read
            .clone()
            .unwrap();
        let reply = match choice {
            Some("") => Submission::new(Op::Resume {
                request_id: state.machine.pending_request_ids()[0].clone(),
                content: vec![],
            }),
            Some(choice) => budget_reply(&state, choice),
            None => Submission::new(Op::Interrupt),
        };
        submit(&mut state, reply).await.unwrap();
        assert_eq!(
            state.machine.owner_work.as_ref().unwrap().outcome,
            crate::agent_machine::owner_work::Outcome::Cancelled
        );
        assert!(
            state
                .machine
                .owner_work
                .as_ref()
                .unwrap()
                .pending_read
                .is_none()
        );
        assert!(state.machine.tool_payload_by_call_id(&pending.id).is_some());
        assert!(!state.machine.has_pending_interaction());
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(generation.recorded_requests().is_empty());
    }
}

#[tokio::test]
async fn owned_human_selection_survives_a_budget_pause_without_another_model_or_prompt() {
    let (_dir, mut state, reads, calls, generation) = fixture(None, None, usize::MAX).await;
    state.core_config.task_budget.model_calls = std::num::NonZeroU64::new(1).unwrap();
    let input = control("Who revokes old fids?");
    state.machine.accept_submission(input.id.clone());
    submit(&mut state, input).await.unwrap();
    let owner_request = state.machine.pending_request_ids()[0].clone();
    submit(
        &mut state,
        Submission::new(Op::Resume {
            request_id: owner_request.clone(),
            content: vec![alan_agent_protocol::ContentPart::text("hostfs")],
        }),
    )
    .await
    .unwrap();
    assert!(state.machine.task_budget_wait.is_some());
    assert_eq!(
        state
            .machine
            .owner_work
            .as_ref()
            .unwrap()
            .selected_owner
            .as_deref(),
        Some("hostfs")
    );
    assert_eq!(reads.load(Ordering::SeqCst), 2);
    assert!(
        submit(
            &mut state,
            Submission::new(Op::Resume {
                request_id: owner_request,
                content: vec![alan_agent_protocol::ContentPart::text("kernel")]
            })
        )
        .await
        .is_err()
    );
    let reply = budget_reply(&state, "approve");
    submit(&mut state, reply).await.unwrap();
    assert!(
        matches!(&state.machine.owner_work.as_ref().unwrap().outcome,
        crate::agent_machine::owner_work::Outcome::Completed { owner } if owner=="hostfs")
    );
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
    assert!(!state.machine.has_pending_interaction());
}

#[tokio::test]
async fn owner_budget_resume_rejects_a_changed_or_revoked_evaluator_before_model_admission() {
    for revoked in [false, true] {
        let (_dir, mut state, reads, calls, generation) =
            fixture(Some("hostfs"), None, usize::MAX).await;
        state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(2).unwrap();
        let input = control("Who revokes old fids?");
        state.machine.accept_submission(input.id.clone());
        submit(&mut state, input).await.unwrap();
        assert!(state.machine.task_budget_wait.is_some());
        let original = state
            .machine
            .owner_work
            .as_ref()
            .unwrap()
            .evaluator_binding
            .clone();
        assert!(original.is_some());
        let mut captured = evaluator_callable(&state.machine, calls.clone(), Some("hostfs"));
        if revoked {
            captured.identity.profile = "outside-grant".into();
        } else {
            captured.identity.revision = "changed-revision".into();
        }
        state.environment = state
            .environment
            .with_connection_authority(Arc::new(Authority {
                captured,
                quote: None,
                close_before_wait: None,
            }));
        let reply = budget_reply(&state, "approve");
        assert!(submit(&mut state, reply).await.is_err());
        assert_eq!(
            state.machine.owner_work.as_ref().unwrap().evaluator_binding,
            original
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(generation.recorded_requests().is_empty());
        assert_eq!(
            state
                .machine
                .task_budget
                .as_ref()
                .unwrap()
                .snapshot()
                .await
                .model_calls,
            0
        );
    }
}

struct SuspendedSourceTool {
    reads: Arc<AtomicUsize>,
    suspended: Arc<std::sync::atomic::AtomicBool>,
    fault: &'static str,
}
impl Tool for SuspendedSourceTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read sources with independently revocable first-source authority"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({"type":"object"})
    }
    fn capability(&self, _: &serde_json::Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }
    fn execute(&self, arguments: serde_json::Value, _: &ToolContext) -> ToolResult {
        self.reads.fetch_add(1, Ordering::SeqCst);
        let first = arguments["path"] == "/mnt/source/hostfs.rs";
        let fault = if first && self.suspended.load(Ordering::SeqCst) {
            self.fault
        } else {
            "none"
        };
        Box::pin(async move {
            if fault == "revoked" {
                anyhow::bail!("first source authority revoked; other sources remain readable");
            }
            let content = if first && fault != "changed" {
                "pub struct HostDirFs {}"
            } else {
                "pub struct Other {}"
            };
            Ok(json!({"content":content,"start_line":1,"end_line":1,"type":"text"}))
        })
    }
}

#[tokio::test]
async fn recovered_owner_validation_rechecks_previously_validated_authority_and_bytes() {
    for fault in ["none", "changed", "revoked"] {
        let dir = TempDir::new().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock-model", dir.path())
            .await
            .unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let suspended = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let registry = || {
            let mut registry = ToolRegistry::new();
            registry.register(SuspendedSourceTool {
                reads: reads.clone(),
                suspended: suspended.clone(),
                fault,
            });
            registry
        };
        let generation = alan_llm::MockLlmProvider::new();
        let mut state = tool_batch::create_test_state_with_machine_tools_and_provider(
            machine,
            registry(),
            generation.clone(),
        )
        .await;
        let mut input = control("Which crate defines HostDirFs?");
        if let Op::Input { parts, .. } = &mut input.op
            && let alan_agent_protocol::ContentPart::Structured { data } = &mut parts[0]
        {
            data["owner_work_v1"]["candidates"][0]["sources"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path":"/mnt/source/extra.rs","start_line":1,"end_line":1}));
        }
        state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(4).unwrap();
        state.machine.accept_submission(input.id.clone());
        crate::runtime::task_budget::begin(&mut state)
            .await
            .unwrap();
        state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(1).unwrap();
        submit(&mut state, input).await.unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 4);
        assert_eq!(
            state.machine.owner_work.as_ref().unwrap().validated_sources,
            1
        );
        let proposal = state
            .machine
            .task_budget_wait
            .as_ref()
            .unwrap()
            .checkpoint
            .details["additional"]["tool_calls"]
            .clone();
        state.machine.flush_recorder().await.unwrap();
        let path = state.machine.input_recorder().unwrap().path().to_path_buf();
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/proc/9", "mock-model", dir.path())
                .await
                .unwrap();
        let mut resumed = tool_batch::create_test_state_with_machine_tools_provider_and_agent_path(
            recovered,
            registry(),
            generation.clone(),
            "/agent/9",
        )
        .await;
        resumed.core_config = state.core_config.clone();
        suspended.store(true, Ordering::SeqCst);
        crate::runtime::task_budget::publish_wait(&mut resumed)
            .await
            .unwrap();
        let reply = budget_reply(&resumed, "approve");
        let result = submit(&mut resumed, reply).await;
        let work = resumed.machine.owner_work.as_ref().unwrap();
        if fault == "none" {
            result.unwrap();
            assert!(matches!(
                work.outcome,
                crate::agent_machine::owner_work::Outcome::Completed { .. }
            ));
            assert_eq!(reads.load(Ordering::SeqCst), 6);
        } else {
            assert!(
                result.is_err(),
                "stale first source must prevent completion: {fault}"
            );
            assert!(matches!(
                work.outcome,
                crate::agent_machine::owner_work::Outcome::Failed { .. }
            ));
            assert_eq!(reads.load(Ordering::SeqCst), 5);
        }
        assert_eq!(
            proposal,
            json!(2),
            "finite extension must cover the complete fresh validation pass"
        );
        assert!(generation.recorded_requests().is_empty());
        assert!(
            resumed
                .machine
                .task_budget
                .as_ref()
                .unwrap()
                .snapshot()
                .await
                .tool_calls
                >= 5
        );
    }
}

#[tokio::test]
async fn mismatched_owner_checkpoint_cannot_extend_budget_or_consume_the_request() {
    let (_dir, mut state, reads, _calls, generation) =
        fixture(Some("hostfs"), None, usize::MAX).await;
    state.core_config.task_budget.tool_calls = std::num::NonZeroU64::new(1).unwrap();
    let input = control("Which crate defines HostDirFs?");
    state.machine.accept_submission(input.id.clone());
    submit(&mut state, input).await.unwrap();
    let before = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    let request = state.machine.pending_request_ids()[0].clone();
    state.machine.owner_work.as_mut().unwrap().budget_resumes += 1;
    let reply = budget_reply(&state, "approve");
    assert!(submit(&mut state, reply).await.is_err());
    assert_eq!(
        state.machine.task_budget.as_ref().unwrap().snapshot().await,
        before
    );
    assert!(state.machine.pending_yield(&request).is_some());
    assert!(state.machine.task_budget_wait.is_some());
    assert_eq!(reads.load(Ordering::SeqCst), 1);
    assert!(generation.recorded_requests().is_empty());
}

#[tokio::test]
async fn deadline_settled_first_validation_result_is_not_current_authority_after_recovery() {
    for fault in ["revoked", "changed"] {
        let dir = TempDir::new().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock-model", dir.path())
            .await
            .unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let suspended = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let registry = || {
            let mut registry = ToolRegistry::new();
            registry.register(SuspendedSourceTool {
                reads: reads.clone(),
                suspended: suspended.clone(),
                fault,
            });
            registry
        };
        let generation = alan_llm::MockLlmProvider::new();
        let mut state = tool_batch::create_test_state_with_machine_tools_and_provider(
            machine,
            registry(),
            generation.clone(),
        )
        .await;
        state.core_config.task_budget.execution_seconds = std::num::NonZeroU64::new(1).unwrap();
        let input = control("Which crate defines HostDirFs?");
        state.machine.accept_submission(input.id.clone());
        let observed = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let completions = Arc::new(AtomicUsize::new(0));
        let mut emit = |event| {
            let hold = matches!(event, Event::ToolCallCompleted { .. })
                && completions.fetch_add(1, Ordering::SeqCst) == 2;
            let observed = observed.clone();
            let release = release.clone();
            async move {
                if hold {
                    observed.notify_one();
                    release.notified().await;
                }
            }
        };
        let cancel = CancellationToken::new();
        {
            let transition = handle_submission_with_cancel(&mut state, input, &mut emit, &cancel);
            tokio::pin!(transition);
            tokio::select! {
                _ = observed.notified() => {},
                result = &mut transition => panic!("validation completion gate not reached: {result:?}"),
            }
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(1200), &mut transition)
                    .await
                    .is_err()
            );
            release.notify_one();
            transition.await.unwrap();
        }
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        let work = state.machine.owner_work.as_ref().unwrap();
        assert_eq!(work.validated_sources, 0);
        let pending = work.pending_read.as_ref().unwrap();
        assert_eq!(
            state.machine.tool_payload_by_call_id(&pending.id).unwrap()["success"],
            true
        );
        assert!(state.machine.task_budget_wait.is_some());
        state.machine.flush_recorder().await.unwrap();
        let path = state.machine.input_recorder().unwrap().path().to_path_buf();
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/proc/9", "mock-model", dir.path())
                .await
                .unwrap();
        let mut resumed = tool_batch::create_test_state_with_machine_tools_provider_and_agent_path(
            recovered,
            registry(),
            generation.clone(),
            "/agent/9",
        )
        .await;
        resumed.core_config = state.core_config.clone();
        suspended.store(true, Ordering::SeqCst);
        crate::runtime::task_budget::publish_wait(&mut resumed)
            .await
            .unwrap();
        let reply = budget_reply(&resumed, "approve");
        assert!(
            submit(&mut resumed, reply).await.is_err(),
            "cached validation result must not survive changed authority: {fault}"
        );
        assert!(matches!(
            resumed.machine.owner_work.as_ref().unwrap().outcome,
            crate::agent_machine::owner_work::Outcome::Failed { .. }
        ));
        assert_eq!(reads.load(Ordering::SeqCst), 4);
        assert!(generation.recorded_requests().is_empty());
    }
}
