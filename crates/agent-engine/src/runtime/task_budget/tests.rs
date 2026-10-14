use super::*;
use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use crate::runtime::submission_handlers::RuntimeOpAction;
use crate::runtime::transition::{NamespaceRuntimeEnvironment, handle_runtime_op};
use alan_agent_protocol::{ContentPart, Op};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use std::{num::NonZeroU64, sync::Arc};

fn state(machine: AgentMachine) -> RuntimeLoopState {
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let mut config = crate::Config::default();
    config.task_budget.model_calls = NonZeroU64::new(1).unwrap();
    RuntimeLoopState {
        machine,
        environment: NamespaceRuntimeEnvironment::new(root, "/agent/1", "default"),
        core_config: config,
        runtime_config: crate::runtime::RuntimeConfig::default(),
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    }
}

async fn waiting(machine: AgentMachine, tools: bool) -> (RuntimeLoopState, String) {
    let mut state = state(machine);
    state.machine.accept_submission("budget-task");
    begin(&mut state).await.unwrap();
    state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .reserve(crate::agent_machine::task_budget::Admission::Model)
        .await
        .unwrap();
    let continuation = tools.then(|| PendingToolReplayBatch {
        tool_calls: vec![NormalizedToolCall {
            id: "call-1".into(),
            name: "bash".into(),
            arguments: serde_json::json!({"command":"echo should-remain-governed"}),
        }],
        resume_with_generation: true,
    });
    let mut events = Vec::new();
    pause(
        &mut state,
        &Exhausted {
            dimension: Dimension::ModelCalls,
        },
        continuation,
        &mut |event| {
            events.push(event);
            async {}
        },
    )
    .await
    .unwrap();
    let [Event::Yield { request_id, .. }] = events.as_slice() else {
        panic!("expected correlated wait")
    };
    (state, request_id.clone())
}

#[tokio::test]
async fn budget_extension_replays_through_ordinary_policy_without_tool_or_effect_approval() {
    let (mut state, id) = waiting(AgentMachine::new(), true).await;
    let pending = state.machine.pending_confirmation().unwrap();
    let action = handle_runtime_op(
        &mut state,
        Op::Resume {
            request_id: id.clone(),
            content: vec![ContentPart::structured(
                serde_json::json!({"choice":"approve"}),
            )],
        },
        &mut |_| async {},
    )
    .await
    .unwrap();
    let RuntimeOpAction::ReplayApprovedToolBatch {
        approved_unknown_effect_call_id,
        approved_tool_escalation_call_id,
        tool_calls,
        ..
    } = action
    else {
        panic!("expected preserved batch")
    };
    assert_eq!(tool_calls[0].id, "call-1");
    assert!(approved_unknown_effect_call_id.is_none());
    assert!(approved_tool_escalation_call_id.is_none());
    assert!(state.machine.task_budget_wait.is_none());
    let budget = state.machine.task_budget.as_ref().unwrap().snapshot().await;
    assert_eq!(budget.model_calls, 1);
    assert_eq!(budget.limits.model_calls.get(), 2);
    assert!(resolve(&mut state.machine, &pending, true).await.is_err());
    assert!(matches!(
        handle_runtime_op(
            &mut state,
            Op::Resume {
                request_id: id,
                content: vec![]
            },
            &mut |_| async {}
        )
        .await
        .unwrap(),
        RuntimeOpAction::NoTurn
    ));
    assert_eq!(
        state
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .limits
            .model_calls
            .get(),
        2
    );
}

#[tokio::test]
async fn empty_or_negative_budget_response_does_not_resume_generation() {
    for content in [
        vec![],
        vec![ContentPart::structured(
            serde_json::json!({"choice":"reject"}),
        )],
    ] {
        let (mut state, id) = waiting(AgentMachine::new(), true).await;
        let action = handle_runtime_op(
            &mut state,
            Op::Resume {
                request_id: id,
                content,
            },
            &mut |_| async {},
        )
        .await
        .unwrap();
        assert!(matches!(action, RuntimeOpAction::NoTurn));
        assert_eq!(
            state
                .machine
                .task_budget
                .as_ref()
                .unwrap()
                .snapshot()
                .await
                .phase,
            Phase::Completed
        );
        assert!(state.machine.task_budget_wait.is_none());
        assert!(state.machine.tool_payload_by_call_id("call-1").is_some());
    }
}

#[tokio::test]
async fn budget_decline_settles_empty_explicit_continuation_without_repeating_completed_effects() {
    for content in [
        vec![],
        vec![ContentPart::structured(
            serde_json::json!({"choice":"reject"}),
        )],
    ] {
        let mut state = state(AgentMachine::new());
        begin(&mut state).await.unwrap();
        let result = serde_json::json!({"status":"completed", "output":"already done"});
        state
            .machine
            .add_tool_message("completed", "bash", result.clone());
        pause(
            &mut state,
            &Exhausted {
                dimension: Dimension::Time,
            },
            Some(PendingToolReplayBatch {
                tool_calls: vec![],
                resume_with_generation: false,
            }),
            &mut |_| async {},
        )
        .await
        .unwrap();
        let id = state
            .machine
            .pending_request_ids()
            .into_iter()
            .next()
            .unwrap();
        let action = handle_runtime_op(
            &mut state,
            Op::Resume {
                request_id: id,
                content,
            },
            &mut |_| async {},
        )
        .await
        .unwrap();
        assert!(matches!(action, RuntimeOpAction::NoTurn));
        assert!(!state.machine.has_pending_interaction());
        assert!(state.machine.task_budget_wait.is_none());
        assert_eq!(
            state
                .machine
                .task_budget
                .as_ref()
                .unwrap()
                .snapshot()
                .await
                .phase,
            Phase::Completed
        );
        assert_eq!(
            state.machine.tool_payload_by_call_id("completed"),
            Some(result)
        );
    }
}

#[tokio::test]
async fn budget_deadline_after_explicit_result_does_not_request_an_unneeded_extension() {
    let mut state = state(AgentMachine::new());
    begin(&mut state).await.unwrap();
    let call = NormalizedToolCall {
        id: "finished".into(),
        name: "bash".into(),
        arguments: serde_json::json!({"command":"true"}),
    };
    state.machine.add_tool_message(
        &call.id,
        &call.name,
        serde_json::json!({"status":"failed", "reason":"deadline"}),
    );
    assert!(
        !pause_tool_batch(
            &mut state,
            &Exhausted {
                dimension: Dimension::Time
            },
            &[call],
            false,
            &mut |_| async {}
        )
        .await
        .unwrap()
    );
    assert!(!state.machine.has_pending_interaction());
    assert!(state.machine.task_budget_wait.is_none());
}

#[tokio::test]
async fn recovery_republishes_budget_wait_once_with_the_same_ordered_continuation() {
    let directory = tempfile::TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "test", directory.path())
        .await
        .unwrap();
    let (source, _) = waiting(machine, true).await;
    let original = source.machine.task_budget_wait.as_ref().unwrap().clone();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        source.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        directory.path(),
    )
    .await
    .unwrap();
    let mut recovered = state(recovered);
    assert_eq!(recovered.machine.task_budget_wait.as_ref(), Some(&original));
    let id = publish_wait(&mut recovered).await.unwrap().unwrap();
    assert!(publish_wait(&mut recovered).await.unwrap().is_none());
    assert!(recovered.machine.pending_yield(&id).is_some());
    assert_eq!(
        recovered
            .machine
            .pending_tool_replay_batch(&original.checkpoint.checkpoint_id)
            .unwrap()
            .tool_calls[0]
            .id,
        "call-1"
    );
    assert_eq!(
        recovered
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .model_calls,
        1
    );
}

#[tokio::test]
async fn unowned_or_stale_budget_confirmation_cannot_extend_allowance() {
    let (mut state, id) = waiting(AgentMachine::new(), false).await;
    let mut forged = state.machine.pending_confirmation().unwrap();
    forged.details["additional"]["model_calls"] = serde_json::json!(1000);
    assert!(resolve(&mut state.machine, &forged, true).await.is_err());
    state
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .extend(&Extension {
            tool_calls: 1,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        handle_runtime_op(
            &mut state,
            Op::Resume {
                request_id: id.clone(),
                content: vec![ContentPart::structured(
                    serde_json::json!({"choice":"approve"})
                )]
            },
            &mut |_| async {}
        )
        .await
        .is_err()
    );
    assert!(state.machine.pending_yield(&id).is_some());
    assert!(state.machine.task_budget_wait.is_some());
    assert_eq!(
        state
            .machine
            .task_budget
            .as_ref()
            .unwrap()
            .snapshot()
            .await
            .limits
            .model_calls
            .get(),
        1
    );
}

#[tokio::test]
async fn recovery_after_extension_keeps_continuation_and_never_adds_original_allowance_twice() {
    let directory = tempfile::TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "test", directory.path())
        .await
        .unwrap();
    let (mut source, id) = waiting(machine, true).await;
    let original_checkpoint = source.machine.pending_confirmation().unwrap().checkpoint_id;
    handle_runtime_op(
        &mut source,
        Op::Resume {
            request_id: id,
            content: vec![ContentPart::structured(
                serde_json::json!({"choice":"approve"}),
            )],
        },
        &mut |_| async {},
    )
    .await
    .unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        source.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        directory.path(),
    )
    .await
    .unwrap();
    let mut recovered = state(recovered);
    let id = publish_wait(&mut recovered).await.unwrap().unwrap();
    let wait = recovered.machine.task_budget_wait.as_ref().unwrap();
    assert_ne!(wait.checkpoint.checkpoint_id, original_checkpoint);
    assert_eq!(wait.checkpoint.details["additional"]["model_calls"], 0);
    assert!(
        wait.checkpoint.details["additional"]["execution_seconds"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        wait.continuation.as_ref().unwrap().tool_calls[0].id,
        "call-1"
    );
    handle_runtime_op(
        &mut recovered,
        Op::Resume {
            request_id: id,
            content: vec![ContentPart::structured(
                serde_json::json!({"choice":"approve"}),
            )],
        },
        &mut |_| async {},
    )
    .await
    .unwrap();
    let snapshot = recovered
        .machine
        .task_budget
        .as_ref()
        .unwrap()
        .snapshot()
        .await;
    assert_eq!(
        (snapshot.model_calls, snapshot.limits.model_calls.get()),
        (1, 2)
    );
}

#[tokio::test]
async fn cancelling_a_budget_wait_does_not_restore_it_or_block_new_work() {
    let directory = tempfile::TempDir::new().unwrap();
    let machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "test", directory.path())
        .await
        .unwrap();
    let (mut source, _) = waiting(machine, true).await;
    crate::runtime::transition::handle_submission_with_cancel(
        &mut source,
        alan_agent_protocol::Submission {
            id: "cancel-budget".into(),
            intent: alan_agent_protocol::InputIntent::ForceAgent,
            op: Op::Interrupt,
        },
        &mut |_| async {},
        &tokio_util::sync::CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(source.machine.task_budget_wait.is_none());
    assert!(!source.machine.has_pending_interaction());
    let recovered = AgentMachine::load_from_rollout_in_dir(
        source.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        directory.path(),
    )
    .await
    .unwrap();
    assert!(recovered.task_budget_wait.is_none());
    assert!(recovered.task_budget.is_none());
    let mut recovered = state(recovered);
    begin(&mut recovered).await.unwrap();
    assert_eq!(
        recovered
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
