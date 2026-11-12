// Real Process execution: count the irreversible effect before withholding the result.
struct BlockedCountedEffect {
    count: Arc<AtomicUsize>,
    started: Arc<tokio::sync::Notify>,
}

impl Tool for BlockedCountedEffect {
    fn name(&self) -> &str {
        "blocked_effect"
    }
    fn description(&self) -> &str {
        "Count once then withhold result"
    }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object"})
    }
    fn capability(&self, _: &Value) -> ToolCapability {
        ToolCapability::Write
    }
    fn execute(&self, _: Value, _: &ToolContext) -> ToolResult {
        let count = self.count.clone();
        let started = self.started.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            started.notify_one();
            std::future::pending().await
        })
    }
}

async fn run_blocked_effect(
    state: &mut RuntimeLoopState,
    id: &str,
    cancel: &CancellationToken,
) -> crate::runtime::tool_execution::ToolExecutionOutcome {
    use crate::runtime::tool_execution::{
        ToolExecutionRequest, ToolExecutionRuntime, execute_allowed_tool_call,
    };
    let call = NormalizedToolCall {
        id: id.into(),
        name: "blocked_effect".into(),
        arguments: json!({}),
    };
    let runtime = ToolExecutionRuntime::new(
        &mut state.machine,
        state.environment.agent_files(),
        state.environment.host_mount_requests(),
        state.environment.tool_execution(),
        "/proc/1".into(),
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        execute_allowed_tool_call(
            runtime,
            ToolExecutionRequest {
                task_cancel: cancel,
                tool_call: &call,
                tool_arguments: &call.arguments,
                tool_timeout_secs: 1,
                tool_capability: ToolCapability::Write,
                tool_audit: None,
                approval: "not_required",
                allow_approved_unknown_effect_execution: false,
                cancel,
            },
            &mut |_| async {},
        ),
    )
    .await
    .expect("bounded execution")
    .unwrap()
}

async fn counted_partial_effect_remains_unknown(cancel_after_effect: bool) {
    let temp = tempfile::TempDir::new().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", temp.path())
        .await
        .unwrap();
    machine.add_user_message("execute exactly once");
    let count = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(tokio::sync::Notify::new());
    let mut tools = ToolRegistry::new();
    tools.register(BlockedCountedEffect {
        count: count.clone(),
        started: started.clone(),
    });
    let mut state = create_test_state_with_machine_and_tools(machine, tools).await;
    let cancel = CancellationToken::new();
    let trigger = tokio::spawn({
        let cancel = cancel.clone();
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(2), started.notified())
                .await
                .expect("real effect must execute");
            if cancel_after_effect {
                cancel.cancel();
            }
        }
    });
    run_blocked_effect(&mut state, "first", &cancel).await;
    trigger.await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let identity = build_effect_identity(
        &state.machine,
        "blocked_effect",
        &json!({}),
        EffectCategory::Process,
    );
    let record = state
        .machine
        .effect_by_idempotency_key(&identity.idempotency_key)
        .unwrap();
    assert_eq!(record.status, crate::rollout::EffectStatus::Unknown);
    assert_eq!(
        record.reason.as_deref(),
        Some(if cancel_after_effect {
            "cancelled with unknown effects"
        } else {
            "timeout with unknown effects"
        })
    );
    assert_eq!(
        record.result_payload.as_ref().unwrap()["process"],
        "/proc/2"
    );
    let fresh_cancel = CancellationToken::new();
    assert_eq!(
        run_blocked_effect(&mut state, "second", &fresh_cancel).await,
        crate::runtime::tool_execution::ToolExecutionOutcome::PauseTurn
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    state.machine.flush_recorder().await.unwrap();
    let rollout = state.machine.rollout_path().unwrap().clone();
    let recovered =
        AgentMachine::load_from_rollout_in_dir(&rollout, "/proc/3", "mock", temp.path())
            .await
            .unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(BlockedCountedEffect {
        count: count.clone(),
        started: Arc::new(tokio::sync::Notify::new()),
    });
    let mut recovered = create_test_state_with_machine_tools_provider_and_agent_path(
        recovered,
        tools,
        SimpleMockProvider,
        "/agent/3",
    )
    .await;
    assert_eq!(
        run_blocked_effect(&mut recovered, "recovered", &fresh_cancel).await,
        crate::runtime::tool_execution::ToolExecutionOutcome::PauseTurn
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn budget_deadline_cleans_up_real_tool_and_budget_approval_never_authorizes_unknown_effects() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", temp.path()).await.unwrap();
    machine.add_user_message("execute exactly once");
    let count = Arc::new(AtomicUsize::new(0));
    let mut tools = ToolRegistry::new();
    tools.register(BlockedCountedEffect {count:count.clone(),started:Arc::new(tokio::sync::Notify::new())});
    let mut state = create_test_state_with_machine_and_tools(machine, tools).await;
    state.core_config.task_budget.execution_seconds = std::num::NonZeroU64::new(1).unwrap();
    crate::runtime::task_budget::begin(&mut state).await.unwrap();
    let call = NormalizedToolCall {id:"budget-deadline".into(),name:"blocked_effect".into(),arguments:json!({})};
    let cancel = CancellationToken::new();
    let writer = state.agent_files().begin_tape_generation().await.unwrap();
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), orchestrate_tool_batch_internal(
        &mut ToolLoopGuard::new(None, 3), &mut state, std::slice::from_ref(&call),
        ToolOrchestratorInputs {explicit_command:false,cancel:&cancel,steering_broker:None},
        (None,None), &writer, &mut |_| async {},
    )).await.expect("budget must await bounded Process cleanup").unwrap();
    writer.finish().await.unwrap();
    assert_eq!(outcome, ToolBatchOrchestratorOutcome::PauseTurn);
    assert_eq!(count.load(Ordering::SeqCst),1);
    assert!(!cancel.is_cancelled());
    assert!(!state.machine.submission_was_cancelled());
    let identity = build_effect_identity(&state.machine,"blocked_effect",&json!({}),EffectCategory::Process);
    assert_eq!(state.machine.effect_by_idempotency_key(&identity.idempotency_key).unwrap().status,crate::rollout::EffectStatus::Unknown);
    let id = state.machine.pending_request_ids().into_iter().next().unwrap();
    let action = handle_runtime_op(&mut state, Op::Resume {
        request_id:id, content:vec![alan_agent_protocol::ContentPart::structured(json!({"choice":"approve"}))],
    },&mut |_| async {}).await.unwrap();
    assert!(matches!(action, RuntimeOpAction::ReplayApprovedToolBatch {
        approved_unknown_effect_call_id:None, approved_tool_escalation_call_id:None, ..
    }));
    assert_eq!(run_blocked_effect(&mut state,"retry-after-budget",&cancel).await,crate::runtime::tool_execution::ToolExecutionOutcome::PauseTurn);
    assert_eq!(count.load(Ordering::SeqCst),1);
    assert_ne!(state.machine.pending_confirmation().unwrap().checkpoint_type,crate::approval::TASK_BUDGET_CHECKPOINT_TYPE);
}

#[tokio::test]
async fn counted_partial_effect_timeout_requires_reconciliation_after_recovery() {
    counted_partial_effect_remains_unknown(false).await;
}

#[tokio::test]
async fn counted_partial_effect_cancellation_requires_reconciliation_after_recovery() {
    counted_partial_effect_remains_unknown(true).await;
}
