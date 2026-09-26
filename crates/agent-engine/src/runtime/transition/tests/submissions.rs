// Tests for direct submission handling
#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the submission fields that define this scenario"
)]
async fn test_handle_submission_cancel() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("existing history");
    machine.activate_task();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        ))
        .await,
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Interrupt);

    let cancel = CancellationToken::new();
    let result = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel).await;

    assert!(result.is_ok(), "interrupt should succeed: {result:?}");
    assert_eq!(events.len(), 1);
    assert_eq!(state.machine.messages().len(), 1);
    assert_eq!(
        state.machine.messages()[0].text_content(),
        "existing history"
    );
    assert!(!state.machine.has_active_task());
    match &events[0] {
        Event::TurnCompleted { summary } => {
            assert_eq!(summary.as_deref(), Some("Task cancelled by user"));
        }
        _ => panic!("Expected TurnCompleted event"),
    }
}

#[tokio::test]
async fn accepted_transition_returns_compact_outcome_and_clears_submission_identity() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        ))
        .await,
    );
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();

    let outcome = advance_accepted_submission(
        &mut state,
        Submission::new(alan_agent_protocol::Op::Interrupt),
        &broker,
        &cancel,
    )
    .await;

    assert_eq!(
        outcome.result.expect("interrupt transition should succeed"),
        TransitionCompletion::Completed
    );
    assert!(!outcome.requeue_inband_submissions);
    assert!(outcome.deferred_actions.is_empty());
    assert_eq!(state.machine.current_submission_id(), None);
}

#[tokio::test]
#[allow(
    clippy::field_reassign_with_default,
    reason = "the test highlights only the submission fields that define this scenario"
)]
async fn test_handle_submission_rollback() {
    let config = Config::default();
    let mut machine = AgentMachine::new();
    machine.add_user_message("u1");
    machine.add_assistant_message("a1", None);
    machine.add_user_message("u2");
    machine.add_assistant_message("a2", None);
    machine.activate_task();
    let runtime_config = super::RuntimeConfig::default();

    let mut state = RuntimeLoopState {
        machine,
        environment: namespace_environment_with_provider(DelayedMockProvider::new(
            tokio::time::Duration::from_millis(0),
            "",
        )),
        core_config: config,
        runtime_config,
        prompt_cache: crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new()),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let submission = Submission::new(alan_agent_protocol::Op::Rollback { turns: 1 });

    let cancel = CancellationToken::new();
    let result = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel).await;

    assert!(result.is_ok());
    assert_eq!(state.machine.messages().len(), 2);
    assert_eq!(events.len(), 3);
    assert!(events.iter().any(|event| matches!(
        event,
        Event::MachineRolledBack {
            turns: 1,
            removed_messages: 2,
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TextDelta { chunk, is_final }
            if *is_final && chunk.contains("Rolled back 1 turn(s), removed 2 message(s).")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Warning { message }
            if message == crate::ROLLBACK_NON_DURABLE_WARNING
    )));
}

#[tokio::test]
async fn command_steering_requires_ordered_admission() {
    use alan_agent_protocol::{ContentPart, InputIntent};
    use crate::runtime::turn_input::is_turn_inband_submission;
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "must not generate",
        ))
        .await,
    );
    let external_writer = state.agent_files().begin_tape_generation().await.unwrap();
    let mut emit = |_event| async {};
    let cancel = CancellationToken::new();
    let mut cases = [
        (InputMode::Steer, "pwd", "ordered queue admission"),
        (InputMode::NextTurn, "pwd", "ordered queue admission"),
        (InputMode::FollowUp, "", "missing command"),
    ].into_iter().map(|(mode, body, error)| (
        Op::Input { parts: vec![ContentPart::text(body)], mode }, error,
    )).collect::<Vec<_>>();
    cases.push((Op::Turn { parts: vec![ContentPart::text("pwd")], context: None }, "input operation"));
    for (op, error_text) in cases {
        let submission = Submission {
            id: uuid::Uuid::new_v4().to_string(), intent: InputIntent::Command, op,
        };
        let id = submission.id.clone();
        assert!(!is_turn_inband_submission(&submission));
        let error = handle_submission_with_cancel(&mut state, submission, &mut emit, &cancel)
            .await.unwrap_err();
        assert!(error.to_string().contains(error_text));
        assert!(state.machine.messages().is_empty());
        let shell = Shell::new(state.environment.root_transport());
        let actions = state.agent_files().action_ids().await.unwrap();
        let base = format!("{}/actions/{}", state.environment.agent_path(), actions.last().unwrap());
        let result: serde_json::Value = serde_json::from_slice(&shell.cat(&format!("{base}/result")).await.unwrap()).unwrap();
        assert_eq!(result["call_id"], id);
        assert_eq!(result["exit_code"], 1);
        assert_eq!(shell.cat(&format!("{base}/approval")).await.unwrap(), b"not_required");
        assert!(shell.cat(&format!("{base}/process")).await.unwrap().is_empty());
    }
    external_writer.finish().await.unwrap();
}

#[tokio::test]
async fn identical_inputs_publish_distinct_submission_ids_on_tape() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "answer",
        )).await,
    );
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    for id in ["client-one", "client-two"] {
        let mut submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("identical question")],
            mode: InputMode::FollowUp,
        });
        submission.id = id.into();
        advance_accepted_submission(&mut state, submission, &broker, &cancel)
            .await.result.unwrap();
    }
    let shell = Shell::new(state.environment.root_transport());
    let tape = shell.cat(&format!("{}/machine/tape", state.environment.agent_path())).await.unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&tape).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    for id in ["client-one", "client-two"] {
        let matching: Vec<_> = records.iter().filter(|record| record["submission_id"] == id).collect();
        assert_eq!(matching.len(), 2, "one user and one assistant record per input: {records:?}");
        assert_eq!(matching[0]["role"], "user");
        assert_eq!(matching[0]["content"], "identical question");
        assert_eq!(matching[1]["role"], "assistant");
        assert_eq!(matching[1]["content"], "answer");
    }
    let events = shell.cat(&format!("{}/machine/ui/events", state.environment.agent_path())).await.unwrap();
    let ids: Vec<_> = std::str::from_utf8(&events).unwrap().lines()
        .map(|line| serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap())
        .filter_map(|event| match event {
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. } => Some(submission_ids),
            _ => None,
        }).collect();
    assert_eq!(ids, vec![vec!["client-one"], vec!["client-two"]]);
}

#[tokio::test]
async fn input_completion_distinguishes_success_failure_and_cancellation() {
    use alan_agent_protocol::{UiEvent, UiInputStatus};
    for (mode, cancelled, expected) in [
        (InputMode::FollowUp, false, UiInputStatus::Completed),
        (InputMode::FollowUp, true, UiInputStatus::Cancelled),
        (InputMode::Steer, false, UiInputStatus::Failed),
    ] {
        let mut state = runtime_state_with_environment(
            namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::ZERO, "answer",
            )).await,
        );
        let submission = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("same question")], mode,
        });
        let id = submission.id.clone();
        let cancel = CancellationToken::new();
        if cancelled { cancel.cancel(); }
        let _ = advance_accepted_submission(&mut state, submission, &TurnInputBroker::default(), &cancel).await;
        let shell = Shell::new(state.environment.root_transport());
        let bytes = shell.cat(&format!("{}/machine/ui/events", state.environment.agent_path())).await.unwrap();
        let completions: Vec<_> = std::str::from_utf8(&bytes).unwrap().lines()
            .map(|line| serde_json::from_str::<UiEvent>(line).unwrap())
            .filter_map(|event| match event {
                UiEvent::InputCompleted { submission_ids, status, .. } => Some((submission_ids, status)),
                _ => None,
            }).collect();
        assert_eq!(completions, vec![(vec![id], expected)]);
        assert!(state.machine.current_submission_id().is_none());
    }
}

#[tokio::test]
async fn next_turn_inputs_keep_their_ids_in_the_shared_answer() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO, "combined answer",
        )).await,
    );
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    for id in ["queued-one", "queued-two"] {
        let mut input = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text(id)], mode: InputMode::NextTurn,
        });
        input.id = id.into();
        advance_accepted_submission(&mut state, input, &broker, &cancel).await.result.unwrap();
    }
    let mut input = Submission::new(Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("start")], context: None,
    });
    input.id = "trigger".into();
    advance_accepted_submission(&mut state, input, &broker, &cancel).await.result.unwrap();
    let shell = Shell::new(state.environment.root_transport());
    let tape = shell.cat("/agent/1/machine/tape").await.unwrap();
    let records: Vec<serde_json::Value> = std::str::from_utf8(&tape).unwrap().lines()
        .map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(records.len(), 2);
    for record in records {
        assert_eq!(record["submission_id"], "trigger");
        assert_eq!(record["related_submission_ids"], json!(["queued-one", "queued-two"]));
    }
}

#[tokio::test(start_paused = true)]
async fn late_steering_settles_separately_after_completion_or_cancellation() {
    for cancelled in [false, true] {
        let mut state = runtime_state_with_environment(
            namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::from_secs(1),
                "finished answer",
            ))
            .await,
        );
        state.core_config.memory.enabled = false;
        let shell = Shell::new(state.environment.root_transport());
        let broker = TurnInputBroker::default();
        let cancel = CancellationToken::new();
        let origin = Submission::new(Op::Turn {
            parts: vec![alan_agent_protocol::ContentPart::text("original task")],
            context: None,
        });
        let origin_id = origin.id.clone();
        let steering = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("late steering")],
            mode: InputMode::Steer,
        });
        let steering_id = steering.id.clone();
        let advance = advance_accepted_submission(&mut state, origin, &broker, &cancel);
        tokio::pin!(advance);
        assert!(
            tokio::time::timeout(tokio::time::Duration::from_millis(1), &mut advance)
                .await
                .is_err()
        );
        assert!(broker.push(steering).await);
        if cancelled {
            cancel.cancel();
        }
        advance.await.result.unwrap();
        let events = shell.cat("/agent/1/machine/ui/events").await.unwrap();
        let completed: Vec<_> = String::from_utf8(events)
            .unwrap()
            .lines()
            .filter_map(|line| {
                match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
                    alan_agent_protocol::UiEvent::InputCompleted {
                        submission_ids,
                        status,
                        ..
                    } => Some((submission_ids, status)),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(
            completed,
            vec![
                (
                    vec![steering_id.clone()],
                    if cancelled {
                        alan_agent_protocol::UiInputStatus::Cancelled
                    } else {
                        alan_agent_protocol::UiInputStatus::Failed
                    }
                ),
                (
                    vec![origin_id.clone()],
                    if cancelled {
                        alan_agent_protocol::UiInputStatus::Cancelled
                    } else {
                        alan_agent_protocol::UiInputStatus::Completed
                    }
                ),
            ]
        );
        let tape = String::from_utf8(shell.cat("/agent/1/machine/tape").await.unwrap()).unwrap();
        assert!(!tape.contains(&steering_id));
        assert!(
            cancelled
                || tape.lines().any(|line| {
                    let record: serde_json::Value = serde_json::from_str(line).unwrap();
                    record["role"] == "assistant"
                        && record["submission_id"] == origin_id
                        && record["content"] == "finished answer"
                })
        );
    }
}

#[tokio::test]
async fn steering_admitted_before_first_poll_is_settled() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "finished answer",
        ))
        .await,
    );
    state.core_config.memory.enabled = false;
    let shell = Shell::new(state.environment.root_transport());
    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    let origin = Submission::new(Op::Turn {
        parts: vec![alan_agent_protocol::ContentPart::text("original task")],
        context: None,
    });
    let steering = Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("early steering")],
        mode: InputMode::Steer,
    });
    let steering_id = steering.id.clone();
    let advance = advance_accepted_submission(&mut state, origin, &broker, &cancel);
    assert!(broker.push(steering).await);
    advance.await.result.unwrap();

    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    let settlements = events
        .lines()
        .filter_map(|line| match serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap() {
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, .. } => {
                Some(submission_ids)
            }
            _ => None,
        })
        .flatten()
        .filter(|id| id == &steering_id)
        .count();
    assert_eq!(settlements, 1, "every admitted input must settle exactly once");
    assert!(broker.try_recv().await.is_none());
}

#[tokio::test]
async fn cancellation_before_first_poll_settles_without_starting_work() {
    for mode in [InputMode::FollowUp, InputMode::NextTurn] {
        let mut state = runtime_state_with_environment(
            namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::ZERO,
                "must not generate",
            ))
            .await,
        );
        let shell = Shell::new(state.environment.root_transport());
        let queue = state.machine.input_queue();
        let broker = TurnInputBroker::from_queue(queue.clone());
        let cancel = CancellationToken::new();
        let input = Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("must not run")],
            mode,
        });
        let id = input.id.clone();
        let advance = advance_accepted_submission(&mut state, input, &broker, &cancel);
        assert_eq!(queue.lock().unwrap().active_submission_ids, std::slice::from_ref(&id));
        cancel.cancel();
        advance.await.result.unwrap();
        assert!(state.machine.messages().is_empty());
        assert_eq!(state.machine.queued_next_turn_input_count(), 0);
        assert!(queue.lock().unwrap().active_submission_ids.is_empty());
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
            alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. } if submission_ids == [id.clone()])));
    }
}

#[tokio::test]
async fn interrupting_suspended_input_settles_its_original_identity() {
    let mut state = runtime_state_with_environment(
        namespace_environment_with_live_process(DelayedMockProvider::new(
            tokio::time::Duration::ZERO,
            "must not generate",
        ))
        .await,
    );
    let shell = Shell::new(state.environment.root_transport());
    state.machine.accept_submission("origin");
    state
        .machine
        .set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id: "request".into(),
            title: "question".into(),
            prompt: "answer".into(),
            questions: Vec::new(),
        });
    advance_accepted_submission(
        &mut state,
        Submission::new(Op::Interrupt),
        &TurnInputBroker::default(),
        &CancellationToken::new(),
    )
    .await
    .result
    .unwrap();
    assert!(!state.machine.has_pending_interaction());
    let events = String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
    assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
        alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. } if submission_ids == ["origin"])));
}

#[tokio::test]
async fn resumed_tool_finalization_observes_cancellation_before_idle() {
    for cancelled in [false, true] {
        let mut state = runtime_state_with_environment(
            namespace_environment_with_live_process(DelayedMockProvider::new(
                tokio::time::Duration::ZERO,
                "unused",
            ))
            .await,
        );
        state.core_config.memory.enabled = false;
        state.machine.accept_submission("resumed-input");
        state.machine.set_turn_activity(TurnActivityState::Running);
        let cancel = CancellationToken::new();
        if cancelled {
            cancel.cancel();
        }
        finalize_replayed_tool_end_turn_best_effort(
            &mut state, &cancel, true, "test-resume", "test-resume",
        )
        .await;
        assert_eq!(state.machine.submission_was_cancelled(), cancelled);
        assert_eq!(state.machine.current_submission_id(), Some("resumed-input"));
        assert!(state.machine.input_queue().lock().unwrap().active_submission_ids.is_empty());
        assert!(!state.machine.is_turn_active());
    }
}
