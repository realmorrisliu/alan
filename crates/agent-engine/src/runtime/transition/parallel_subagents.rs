//! Concurrent execution of contiguous `spawn_subagent` and `invoke_delegated_skill` calls.
//!
//! Launches all prepared child Agent Processes concurrently through [`tokio::task::JoinSet`] while
//! publishing AgentFS `actions/<id>` records, `Event` emissions, and Machine tape results strictly
//! in the original tool-call batch order.

use std::future::Future;

use alan_agent_protocol::Event;
use anyhow::Result;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::{RuntimeLoopState, ToolLoopGuard, ToolOrchestratorInputs, delegated_skill_runtime};
use crate::agent_machine::NormalizedToolCall;
use crate::runtime::subagent_tool::{
    ExecutedChildSlot, SlotExecutionError, SubagentSpawnFn, commit_executed_slot,
    default_subagent_spawn_fn, execute_prepared_slot, finalize_action_record,
    finalize_uncommitted_slot, prepare_delegated_skill_slot, prepare_subagent_slot,
};
use crate::runtime::virtual_tool::VirtualToolOutcome;

/// Result of handling a contiguous concurrent prefix of subagent / delegated calls.
pub(super) enum ParallelSubagentBatchOutcome {
    Continue {
        next_idx: usize,
        refresh_context: bool,
    },
    EndTurn,
}

/// Unified outcome across both `parallel_read_tools` and `parallel_subagents` prefix dispatchers.
pub(super) enum ParallelBatchStep {
    Advanced {
        next_idx: usize,
        refresh_context: bool,
    },
    EndTurn,
}

/// Dispatch either a contiguous pure read-only tool run or a contiguous subagent/delegated run.
#[allow(
    clippy::too_many_arguments,
    reason = "Parallel prefix dispatcher mirrors orchestrate_tool_batch_internal arguments for read and subagent runs"
)]
pub(super) async fn try_run_parallel_prefix<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    start_idx: usize,
    approved_call_indices: (Option<usize>, Option<usize>),
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<Option<ParallelBatchStep>>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    if let Some(outcome) = super::parallel_read_tools::try_run_parallel_read_only_prefix(
        loop_guard,
        state,
        tool_calls,
        start_idx,
        approved_call_indices,
        inputs,
        emit,
    )
    .await?
    {
        return Ok(Some(match outcome {
            super::parallel_read_tools::ParallelReadBatchOutcome::Completed { count } => {
                ParallelBatchStep::Advanced {
                    next_idx: start_idx + count,
                    refresh_context: false,
                }
            }
            super::parallel_read_tools::ParallelReadBatchOutcome::EndTurn => {
                ParallelBatchStep::EndTurn
            }
        }));
    }

    Ok(
        try_run_parallel_subagent_prefix(loop_guard, state, tool_calls, start_idx, inputs, emit)
            .await?
            .map(|outcome| match outcome {
                ParallelSubagentBatchOutcome::Continue {
                    next_idx,
                    refresh_context,
                } => ParallelBatchStep::Advanced {
                    next_idx,
                    refresh_context,
                },
                ParallelSubagentBatchOutcome::EndTurn => ParallelBatchStep::EndTurn,
            }),
    )
}

fn is_parallel_subagent_tool_name(name: &str) -> bool {
    matches!(name, "spawn_subagent" | "invoke_delegated_skill")
}

/// Execute a contiguous run of 2+ `spawn_subagent` / `invoke_delegated_skill` calls concurrently.
pub(super) async fn try_run_parallel_subagent_prefix<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    start_idx: usize,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
) -> Result<Option<ParallelSubagentBatchOutcome>>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    try_run_parallel_subagent_prefix_with_spawn(
        loop_guard,
        state,
        tool_calls,
        start_idx,
        inputs,
        emit,
        default_subagent_spawn_fn(),
    )
    .await
}

pub(super) async fn try_run_parallel_subagent_prefix_with_spawn<E, F>(
    loop_guard: &mut ToolLoopGuard,
    state: &mut RuntimeLoopState,
    tool_calls: &[NormalizedToolCall],
    start_idx: usize,
    inputs: ToolOrchestratorInputs<'_>,
    emit: &mut E,
    spawn_fn: SubagentSpawnFn,
) -> Result<Option<ParallelSubagentBatchOutcome>>
where
    E: FnMut(Event) -> F,
    F: Future<Output = ()>,
{
    let slice = &tool_calls[start_idx..];
    if slice.len() < 2 || inputs.explicit_command || inputs.cancel.is_cancelled() {
        return Ok(None);
    }

    let prefix_len = slice
        .iter()
        .take_while(|call| {
            is_parallel_subagent_tool_name(&call.name)
                && state.machine.tool_payload_by_call_id(&call.id).is_none()
        })
        .count();
    if prefix_len < 2 {
        return Ok(None);
    }

    let mut admitted_calls = Vec::with_capacity(prefix_len);
    let mut guard_trip_message = None;
    for call in &slice[..prefix_len] {
        if let Some(msg) = loop_guard.before_tool_call(&call.name, &call.arguments) {
            guard_trip_message = Some(msg);
            break;
        }
        admitted_calls.push(call.clone());
    }

    if admitted_calls.is_empty() {
        let msg = guard_trip_message.expect("empty admitted list implies guard tripped");
        crate::runtime::ui_surfaces::error_notice(&state.agent_files(), &msg).await?;
        emit(Event::Error {
            message: msg.clone(),
            recoverable: true,
        })
        .await;
        emit(Event::TextDelta {
            chunk: msg,
            is_final: true,
        })
        .await;
        return Ok(Some(ParallelSubagentBatchOutcome::EndTurn));
    }

    let admitted_count = admitted_calls.len();
    let task_cancel = inputs.cancel;
    let subagent_enabled = state.runtime_config.ad_hoc_subagent_spawn_enabled;
    let agent_files = state.agent_files();

    // Emit ToolCallStarted and allocate AgentFS actions/<id> strictly in original batch order.
    let mut prepared = Vec::with_capacity(admitted_count);
    {
        let mut runtime = delegated_skill_runtime(state);
        for (slot_idx, call) in admitted_calls.iter().enumerate() {
            emit(Event::ToolCallStarted {
                title: None,
                id: call.id.clone(),
                name: call.name.clone(),
                audit: None,
            })
            .await;
            let slot = match call.name.as_str() {
                "spawn_subagent" => {
                    prepare_subagent_slot(
                        &mut runtime,
                        subagent_enabled,
                        slot_idx,
                        call,
                        &call.arguments,
                    )
                    .await?
                }
                "invoke_delegated_skill" => {
                    prepare_delegated_skill_slot(&mut runtime, slot_idx, call, &call.arguments)
                        .await?
                }
                other => unreachable!("unexpected parallel subagent tool name: {other}"),
            };
            prepared.push(slot);
        }
    }

    // Spawn and execute all slots concurrently; propagate either parent cancellation immediately.
    let batch_cancel = CancellationToken::new();
    let mut join_set = JoinSet::new();
    for slot in prepared {
        let slot_agent_files = agent_files.clone();
        let slot_cancel = batch_cancel.clone();
        let slot_spawn = spawn_fn.clone();
        join_set.spawn(async move {
            execute_prepared_slot(slot, slot_agent_files, slot_cancel, slot_spawn).await
        });
    }

    let mut completed_slots: Vec<Option<ExecutedChildSlot>> =
        (0..admitted_count).map(|_| None).collect();
    let mut completed_count = 0;
    while completed_count < admitted_count {
        tokio::select! {
            Some(joined) = join_set.join_next() => {
                let executed = joined?;
                let idx = executed.index;
                completed_slots[idx] = Some(executed);
                completed_count += 1;
            }
            _ = inputs.cancel.cancelled(), if !batch_cancel.is_cancelled() => {
                batch_cancel.cancel();
            }
            _ = task_cancel.cancelled(), if !batch_cancel.is_cancelled() => {
                batch_cancel.cancel();
            }
        }
    }

    // Commit actions, tool completion events, and tape messages strictly in original batch order.
    let mut refresh_context = false;
    let mut completed_iter = completed_slots.into_iter().flatten();
    let mut early_end_turn = false;
    {
        let mut runtime = delegated_skill_runtime(state);
        for slot in &mut completed_iter {
            match commit_executed_slot(&mut runtime, slot, inputs.cancel, task_cancel, emit).await?
            {
                VirtualToolOutcome::Continue {
                    refresh_context: item_refresh,
                } => refresh_context |= item_refresh,
                VirtualToolOutcome::EndTurn => {
                    early_end_turn = true;
                    break;
                }
                VirtualToolOutcome::PauseTurn | VirtualToolOutcome::NotVirtual => {}
            }
        }
    }
    if early_end_turn {
        let rollouts_dir = state
            .runtime_config
            .store_bindings
            .as_ref()
            .map(|stores| stores.rollouts.clone());
        let cancelled =
            crate::runtime::delegated_child_run::ChildRuntimeResult::cancelled_before_launch();
        for remaining in completed_iter {
            finalize_uncommitted_slot(&remaining, rollouts_dir.as_deref(), &cancelled).await;
            if let Ok(result) = &remaining.outcome {
                let _ = finalize_action_record(
                    &agent_files,
                    remaining.action_id.as_deref(),
                    &remaining.kind,
                    &remaining.label,
                    result,
                )
                .await;
            } else if !matches!(remaining.outcome, Err(SlotExecutionError::Prelaunch(_))) {
                let _ = finalize_action_record(
                    &agent_files,
                    remaining.action_id.as_deref(),
                    &remaining.kind,
                    &remaining.label,
                    &cancelled,
                )
                .await;
            }
        }
        return Ok(Some(ParallelSubagentBatchOutcome::EndTurn));
    }

    Ok(Some(ParallelSubagentBatchOutcome::Continue {
        next_idx: start_idx + admitted_count,
        refresh_context,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        agent_machine::AgentMachine,
        config::Config,
        runtime::{
            NamespaceRuntimeEnvironment, RuntimeConfig,
            delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus},
            prompt_cache::PromptAssemblyCache,
        },
        skills::{ResolvedCapabilityView, ScopedPackageDir, SkillHostCapabilities, SkillScope},
        tools::ToolRegistry,
    };
    use alan_agent_protocol::SpawnTarget;
    use alan_agentfs::AgentFs;
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace};
    use alan_shell::Shell;
    use serde_json::json;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::sync::Barrier;

    fn test_state_with_capabilities(delegated: bool) -> (RuntimeLoopState, InProcessTransport) {
        let agentfs = Arc::new(AgentFs::new());
        let mut namespace = Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(agentfs),
            Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
        let tools = ToolRegistry::new();
        let runner = crate::tools::ToolProcessRunner::from_registry(&tools);
        let environment = NamespaceRuntimeEnvironment::new(root.clone(), "/agent/1", "default")
            .with_tool_process_context(1, runner)
            .with_namespace_cwd("/");
        let mut prompt_cache = PromptAssemblyCache::new(Vec::new());
        if delegated {
            prompt_cache.set_host_capabilities(
                SkillHostCapabilities::default()
                    .with_runtime_defaults()
                    .with_delegated_skill_invocation(),
            );
        }
        let state = RuntimeLoopState {
            machine: AgentMachine::new(),
            environment,
            core_config: Config::default(),
            runtime_config: RuntimeConfig::default(),
            prompt_cache,
        };
        (state, root)
    }

    fn completed_mock_child(pid: usize, role: &str, text: &str) -> ChildRuntimeResult {
        ChildRuntimeResult {
            status: ChildRuntimeStatus::Completed,
            process_path: format!("/proc/{pid}"),
            child_run_id: Some(format!("run-{pid}")),
            rollout_path: None,
            output_text: text.to_string(),
            turn_summary: Some(format!("{role} done")),
            structured_output: None,
            warnings: Vec::new(),
            error_message: None,
            pause: None,
            child_run: None,
        }
    }

    #[tokio::test]
    async fn parallel_subagent_batch_executes_concurrently_and_commits_in_order() {
        let (mut state, root) = test_state_with_capabilities(true);
        let mut loop_guard = ToolLoopGuard::new(Some(4), 16);
        let barrier = Arc::new(Barrier::new(2));
        let next_pid = Arc::new(AtomicUsize::new(10));
        let events = Arc::new(Mutex::new(Vec::new()));

        let spawn_fn: SubagentSpawnFn = Arc::new({
            let barrier = barrier.clone();
            let next_pid = next_pid.clone();
            move |_runtime, spec, _cancel, _resume, tracker| {
                let barrier = barrier.clone();
                let next_pid = next_pid.clone();
                Box::pin(async move {
                    let pid = next_pid.fetch_add(1, Ordering::SeqCst);
                    let role = match &spec.target {
                        SpawnTarget::AdHocSubagent { role } => role.clone(),
                        other => format!("{other:?}"),
                    };
                    if let Some(tracker) = tracker {
                        tracker
                            .mark_started(
                                &pid.to_string(),
                                &format!("run-{pid}"),
                                &format!("/proc/{pid}"),
                            )
                            .await;
                    }
                    // Proves both spawned children overlap at the exact same time; sequential would timeout.
                    tokio::time::timeout(Duration::from_secs(2), barrier.wait())
                        .await
                        .expect("subagents must reach barrier concurrently");
                    // Reverse finish timing: researcher completes slower than reviewer.
                    if role == "researcher" {
                        tokio::time::sleep(Duration::from_millis(35)).await;
                    } else {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Ok(completed_mock_child(
                        pid,
                        &role,
                        &format!("{role} completed"),
                    ))
                })
            }
        });

        let tool_calls = vec![
            NormalizedToolCall {
                id: "call-0".to_string(),
                name: "spawn_subagent".to_string(),
                arguments: json!({"role": "researcher", "task": "Inspect files", "read_only": true}),
            },
            NormalizedToolCall {
                id: "call-1".to_string(),
                name: "spawn_subagent".to_string(),
                arguments: json!({"role": "reviewer", "task": "Check diffs", "read_only": true}),
            },
        ];

        let cancel = CancellationToken::new();
        let outcome = try_run_parallel_subagent_prefix_with_spawn(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            0,
            ToolOrchestratorInputs {
                explicit_command: false,
                cancel: &cancel,
                steering_broker: None,
            },
            &mut |event| {
                let events = events.clone();
                async move {
                    events.lock().unwrap().push(event);
                }
            },
            spawn_fn,
        )
        .await
        .unwrap();

        assert!(matches!(
            outcome,
            Some(ParallelSubagentBatchOutcome::Continue {
                next_idx: 2,
                refresh_context: true
            })
        ));

        let completed_ids = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                Event::ToolCallCompleted { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(completed_ids, vec!["call-0", "call-1"]);

        assert!(state.machine.tool_payload_by_call_id("call-0").is_some());
        assert!(state.machine.tool_payload_by_call_id("call-1").is_some());
        let recorded_tool_ids = state
            .machine
            .prompt_view()
            .messages
            .iter()
            .filter_map(|msg| match msg {
                crate::tape::Message::Tool { responses } => {
                    Some(responses.iter().map(|r| r.id.clone()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(recorded_tool_ids, vec!["call-0", "call-1"]);

        let shell = Shell::new(root);
        let action_entries = shell.ls("/agent/1/actions").await.unwrap();
        let action_ids = action_entries
            .into_iter()
            .filter(|name| !matches!(name.as_str(), "clone" | "events" | "help"))
            .collect::<Vec<_>>();
        assert_eq!(action_ids, vec!["a0", "a1"]);
        let name_1 =
            String::from_utf8(shell.cat("/agent/1/actions/a0/name").await.unwrap()).unwrap();
        let name_2 =
            String::from_utf8(shell.cat("/agent/1/actions/a1/name").await.unwrap()).unwrap();
        assert_eq!(name_1, "subagent:researcher");
        assert_eq!(name_2, "subagent:reviewer");
        let result_1 =
            String::from_utf8(shell.cat("/agent/1/actions/a0/result").await.unwrap()).unwrap();
        assert!(result_1.contains("researcher · pid "));
        assert!(result_1.contains("completed"));
    }

    #[tokio::test]
    async fn parallel_mixed_subagent_and_delegated_skill_runs_concurrently() {
        let temp = TempDir::new().unwrap();
        let pkg_store = temp.path().join("packages");
        let skill_dir = pkg_store.join("repo-review");
        let target_dir = skill_dir.join("agents/reviewer");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: Repo Review\ndescription: Review repository changes\n---\n\n# Instructions\nUse this skill when asked.\n",
        )
        .unwrap();
        std::fs::write(
            target_dir.join("agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: pkg_store,
            scope: SkillScope::Installed,
        }]);
        let (mut state, _) = test_state_with_capabilities(true);
        state.prompt_cache = PromptAssemblyCache::with_fixed_capability_view(
            view,
            Vec::new(),
            SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );

        let barrier = Arc::new(Barrier::new(2));
        let spawn_fn: SubagentSpawnFn = Arc::new({
            let barrier = barrier.clone();
            move |_runtime, spec, _cancel, _resume, _tracker| {
                let barrier = barrier.clone();
                Box::pin(async move {
                    tokio::time::timeout(Duration::from_secs(2), barrier.wait())
                        .await
                        .expect("mixed batch must reach barrier concurrently");
                    let label = match spec.target {
                        SpawnTarget::AdHocSubagent { role } => role,
                        SpawnTarget::PackageChildAgent { export_name, .. } => export_name,
                        other => format!("{other:?}"),
                    };
                    Ok(completed_mock_child(20, &label, "ok"))
                })
            }
        });

        let tool_calls = vec![
            NormalizedToolCall {
                id: "mixed-0".to_string(),
                name: "spawn_subagent".to_string(),
                arguments: json!({"role": "scout", "task": "Scan deps"}),
            },
            NormalizedToolCall {
                id: "mixed-1".to_string(),
                name: "invoke_delegated_skill".to_string(),
                arguments: json!({"skill_id": "repo-review", "target": "reviewer", "task": "Review changes"}),
            },
        ];

        let cancel = CancellationToken::new();
        let mut loop_guard = ToolLoopGuard::new(Some(4), 16);
        let outcome = try_run_parallel_subagent_prefix_with_spawn(
            &mut loop_guard,
            &mut state,
            &tool_calls,
            0,
            ToolOrchestratorInputs {
                explicit_command: false,
                cancel: &cancel,
                steering_broker: None,
            },
            &mut |_| async {},
            spawn_fn,
        )
        .await
        .unwrap();

        assert!(matches!(
            outcome,
            Some(ParallelSubagentBatchOutcome::Continue { next_idx: 2, .. })
        ));
        assert!(state.machine.tool_payload_by_call_id("mixed-0").is_some());
        assert!(state.machine.tool_payload_by_call_id("mixed-1").is_some());
    }

    #[tokio::test]
    async fn parallel_subagent_batch_propagates_cancellation() {
        let (mut state, _) = test_state_with_capabilities(true);
        state
            .machine
            .set_turn_activity(crate::agent_machine::TurnActivityState::Running);
        let mut loop_guard = ToolLoopGuard::new(Some(4), 16);
        let cancel = CancellationToken::new();
        let barrier = Arc::new(Barrier::new(2));

        let spawn_fn: SubagentSpawnFn = Arc::new({
            let barrier = barrier.clone();
            move |_runtime, _spec, child_cancel, _resume, _tracker| {
                let barrier = barrier.clone();
                Box::pin(async move {
                    barrier.wait().await;
                    child_cancel.cancelled().await;
                    Ok(ChildRuntimeResult::cancelled_before_launch())
                })
            }
        });

        let cancel_trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel_trigger.cancel();
        });

        let tool_calls = vec![
            NormalizedToolCall {
                id: "c-0".to_string(),
                name: "spawn_subagent".to_string(),
                arguments: json!({"role": "worker-a", "task": "Work A"}),
            },
            NormalizedToolCall {
                id: "c-1".to_string(),
                name: "spawn_subagent".to_string(),
                arguments: json!({"role": "worker-b", "task": "Work B"}),
            },
        ];

        let outcome = tokio::time::timeout(
            Duration::from_secs(2),
            try_run_parallel_subagent_prefix_with_spawn(
                &mut loop_guard,
                &mut state,
                &tool_calls,
                0,
                ToolOrchestratorInputs {
                    explicit_command: false,
                    cancel: &cancel,
                    steering_broker: None,
                },
                &mut |_| async {},
                spawn_fn,
            ),
        )
        .await
        .expect("cancelled batch must finish cleanly")
        .unwrap();

        assert!(matches!(
            outcome,
            Some(ParallelSubagentBatchOutcome::EndTurn)
        ));
    }
}
