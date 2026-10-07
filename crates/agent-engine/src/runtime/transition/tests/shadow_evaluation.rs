use super::*;
use crate::runtime::{EvaluationSurface, model_binding::CallableIdentity};
use alan_llm::{ChoiceEvaluationRequest, ChoiceEvaluationResponse, EvaluationSelection};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Evaluator {
    calls: Arc<AtomicUsize>,
    recorder: RolloutRecorder,
    close_recorder: bool,
    cancel: Option<CancellationToken>,
}

#[async_trait::async_trait]
impl LlmProvider for Evaluator {
    fn provider_name(&self) -> &'static str {
        "test"
    }
    fn supports_generation(&self) -> bool {
        false
    }
    fn supports_choice_evaluation(&self) -> bool {
        true
    }
    async fn evaluate_choice(
        &mut self,
        request: ChoiceEvaluationRequest,
    ) -> anyhow::Result<ChoiceEvaluationResponse> {
        assert_eq!(request.input, "exact task");
        let history = RolloutRecorder::load_history(self.recorder.path())
            .await
            .unwrap();
        assert!(history.iter().any(|item| matches!(item, RolloutItem::Event(e)
            if e.event_type == "machine_evaluation_v1" && e.payload["outcome"]["state"] == "started")));
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.close_recorder {
            self.recorder.close().await.unwrap();
        }
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        Ok(ChoiceEvaluationResponse {
            selection: EvaluationSelection::Selected("command".into()),
            usage: None,
        })
    }
}

fn attach(
    state: &mut RuntimeLoopState,
    close_recorder: bool,
    cancel: Option<CancellationToken>,
) -> (Arc<AtomicUsize>, Arc<Mutex<Vec<serde_json::Value>>>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let observations = Arc::new(Mutex::new(Vec::new()));
    let llmfs = alan_llmfs::LlmFs::new();
    let recorder = state.machine.input_recorder().unwrap();
    llmfs.register_connection_profile(
        "evaluator",
        alan_llmfs::ConnectionProfile::new("test", "pinned", "ref"),
        Box::new(Evaluator {
            calls: calls.clone(),
            recorder: recorder.clone(),
            close_recorder,
            cancel,
        }),
    );
    let mut ns = alan_kernel::Namespace::new();
    // The evaluator's capture has no Tool, command or Host filesystem routes.
    ns.mount(
        "/mnt/llm",
        alan_ap::InProcessTransport::new(Arc::new(llmfs)),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let identity = CallableIdentity {
        profile: "evaluator".into(),
        provider: "test".into(),
        model: "pinned".into(),
        credential_ref: Some("ref".into()),
        revision: "captured".into(),
    };
    state.environment = state.environment.clone().with_shadow_evaluation(root, identity, EvaluationSurface::Interactive, 30_000).unwrap()
        .with_evaluation_publisher({
            let observations = observations.clone();
            move |value| {
                let observations = observations.clone();
                let recorder = recorder.clone();
                async move {
                    if let Some(value) = value {
                        let history = RolloutRecorder::load_history(recorder.path()).await?;
                        assert!(history.iter().any(|item| matches!(item, RolloutItem::Event(e)
                            if e.event_type == "machine_evaluation_v1" && e.payload["outcome"] == value["outcome"])));
                        observations.lock().unwrap().push(value);
                    }
                    Ok(())
                }
            }
        });
    (calls, observations)
}

#[tokio::test]
async fn accepted_input_records_command_advice_without_selecting_command_dispatch() {
    let (mut state, dir, input, _, generation) =
        dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
    state.core_config.memory.enabled = false;
    let sentinel = dir.path().join("sentinel");
    std::fs::write(&sentinel, "unchanged").unwrap();
    let (calls, observations) = attach(&mut state, false, None);
    let result = advance_accepted_submission(
        &mut state,
        input,
        &TurnInputBroker::default(),
        &CancellationToken::new(),
    )
    .await;
    assert!(result.result.is_ok(), "{:?}", result.result);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        !generation.recorded_requests().is_empty(),
        "ordinary Agent route remains selected"
    );
    let observations = observations.lock().unwrap();
    assert_eq!(observations.len(), 2);
    assert_eq!(observations[0]["outcome"]["state"], "started");
    assert_eq!(observations[1]["outcome"]["state"], "selected");
    assert_eq!(observations[1]["outcome"]["candidate_id"], "command");
    assert!(observations[1]["cost_microusd"].is_null());
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "unchanged");
}

#[tokio::test]
async fn duplicate_and_recovered_input_never_repeat_the_evaluator() {
    let (mut state, dir, input, _, _) = dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
    let (calls, observations) = attach(&mut state, false, None);
    let cancel = CancellationToken::new();
    state.observe_input_shadow(&input, &cancel).await.unwrap();
    state.observe_input_shadow(&input, &cancel).await.unwrap();
    let recorder = state.machine.input_recorder().unwrap();
    state.machine =
        AgentMachine::load_from_rollout_in_dir(recorder.path(), "/proc/2", "test", dir.path())
            .await
            .unwrap();
    state.observe_input_shadow(&input, &cancel).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observations.lock().unwrap().len(), 4);
    let history = RolloutRecorder::load_history(state.machine.rollout_path().unwrap())
        .await
        .unwrap();
    assert_eq!(history.iter().filter(|item| matches!(item, RolloutItem::Event(e) if e.event_type == "machine_evaluation_v1")).count(), 2);
    state
        .machine
        .input_recorder()
        .unwrap()
        .close()
        .await
        .unwrap();
    recorder.close().await.unwrap();
}

#[tokio::test]
async fn shadow_barriers_prevent_unrecorded_calls_and_unrecorded_publication() {
    for fail_terminal in [false, true] {
        let (mut state, _dir, input, _, _) =
            dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
        let (calls, observations) = attach(&mut state, fail_terminal, None);
        let backing = state.machine.input_recorder().unwrap();
        let mut failed_batch = None;
        if !fail_terminal {
            let (probe, observed) = backing.batch_failure_probe(false);
            state.machine.set_input_recorder_for_test(probe);
            failed_batch = Some(observed);
        }
        assert!(
            state
                .observe_input_shadow(&input, &CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(fail_terminal));
        assert_eq!(
            observations.lock().unwrap().len(),
            usize::from(fail_terminal)
        );
        if let Some(mut observed) = failed_batch {
            assert_eq!(observed.recv().await.unwrap().len(), 1);
        }
        let source = state.machine.evaluation_observation.as_ref();
        if fail_terminal {
            assert_eq!(source.unwrap()["outcome"]["state"], "started");
        } else {
            assert!(source.is_none());
        }
    }
}

#[tokio::test]
async fn cancellation_after_provider_completion_preserves_abort_uncertainty() {
    let (mut state, _dir, input, _, _) = dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
    let cancel = CancellationToken::new();
    let (calls, observations) = attach(&mut state, false, Some(cancel.clone()));
    let error = state
        .observe_input_shadow(&input, &cancel)
        .await
        .unwrap_err();
    assert!(
        error
            .downcast_ref::<crate::runtime::NamespaceEvaluationUncertainty>()
            .is_some()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(observations.lock().unwrap().len(), 1);
    assert_eq!(
        observations.lock().unwrap()[0]["outcome"]["state"],
        "started"
    );
}

#[tokio::test]
async fn explicit_intents_and_response_operations_persist_without_evaluator_calls() {
    let (mut state, dir, mut input, _, _) =
        dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
    let (calls, observations) = attach(&mut state, false, None);
    for (intent, reason) in [
        (
            alan_agent_protocol::InputIntent::Command,
            "explicit_command",
        ),
        (
            alan_agent_protocol::InputIntent::ForceAgent,
            "explicit_agent",
        ),
    ] {
        input.id = uuid::Uuid::new_v4().to_string();
        input.intent = intent;
        for _ in 0..2 {
            state
                .observe_input_shadow(&input, &CancellationToken::new())
                .await
                .unwrap();
        }
        let latest = observations.lock().unwrap().last().unwrap().clone();
        assert_eq!(latest["outcome"]["state"], "bypassed");
        assert_eq!(latest["outcome"]["reason"], reason);
        assert_eq!(latest["outcome"]["evaluator_calls"], 0);
        assert!(latest["identity"]["operation_id"].is_null());
    }
    state
        .machine
        .set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id: "pending-request".into(),
            title: "Question".into(),
            prompt: "Answer".into(),
            questions: vec![],
        });
    for _ in 0..2 {
        state
            .observe_input_shadow(
                &Submission::new(Op::Resume {
                    request_id: "pending-request".into(),
                    content: vec![alan_agent_protocol::ContentPart::text("! response data")],
                }),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
    }
    let latest = observations.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        latest["identity"]["submission_id"],
        "response:pending-request"
    );
    assert_eq!(latest["outcome"]["reason"], "request_response");
    state
        .observe_input_shadow(&Submission::new(Op::Interrupt), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let recorder = state.machine.input_recorder().unwrap();
    recorder.flush().await.unwrap();
    let records = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    assert_eq!(records.iter().filter(|item| matches!(item, RolloutItem::Event(e) if e.event_type == "machine_evaluation_v1")).count(), 3);
    state.machine = AgentMachine::load_from_rollout_in_dir(
        recorder.path(),
        "/proc/recovered",
        "mock",
        dir.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        state.machine.evaluation_observation.as_ref().unwrap(),
        &latest
    );
    state
        .observe_input_shadow(
            &Submission::new(Op::Resume {
                request_id: "pending-request".into(),
                content: vec![alan_agent_protocol::ContentPart::text("! response data")],
            }),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    state
        .machine
        .set_structured_input(crate::approval::PendingStructuredInputRequest {
            request_id: "pending-request".into(),
            title: "Question".into(),
            prompt: "Answer".into(),
            questions: vec![],
        });
    let error = state
        .observe_input_shadow(
            &Submission::new(Op::Resume {
                request_id: "pending-request".into(),
                content: vec![alan_agent_protocol::ContentPart::text(": changed response")],
            }),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed captured input"));
}

#[tokio::test]
async fn publication_wait_is_cancellable_and_bounded_without_late_success() {
    for terminal in [false, true] {
        for cancel_wait in [false, true] {
            // Terminal publication has its own one-second bounded settlement window.
            if !terminal && !cancel_wait {
                continue;
            }
            let (mut state, _dir, input, _, _) =
                dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
            let (calls, _) = attach(&mut state, false, None);
            let reached = Arc::new(tokio::sync::Notify::new());
            let published = Arc::new(AtomicUsize::new(0));
            state.environment = state.environment.clone().with_evaluation_publisher({
                let reached = reached.clone();
                let published = published.clone();
                move |value| {
                    let reached = reached.clone();
                    let published = published.clone();
                    async move {
                        let is_terminal = value
                            .as_ref()
                            .is_some_and(|v| v["outcome"]["state"] == "selected");
                        if is_terminal == terminal {
                            reached.notify_one();
                            std::future::pending::<()>().await;
                        }
                        published.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    }
                }
            });
            let cancel = CancellationToken::new();
            let result = {
                let evaluation = state.observe_input_shadow(&input, &cancel);
                tokio::pin!(evaluation);
                tokio::select! {
                    _ = reached.notified() => {},
                    result = &mut evaluation => panic!("publication gate not reached: {result:?}"),
                }
                if cancel_wait {
                    cancel.cancel();
                }
                tokio::time::timeout(std::time::Duration::from_secs(2), &mut evaluation)
                    .await
                    .unwrap()
            };
            let error = result.unwrap_err();
            assert_eq!(
                error.downcast_ref::<crate::runtime::NamespaceEvaluationFailure>(),
                Some(&if cancel_wait {
                    crate::runtime::NamespaceEvaluationFailure::Cancelled
                } else {
                    crate::runtime::NamespaceEvaluationFailure::TimedOut
                })
            );
            assert_eq!(calls.load(Ordering::SeqCst), usize::from(terminal));
            assert_eq!(published.load(Ordering::SeqCst), usize::from(terminal));
        }
    }
}

#[tokio::test]
async fn steering_and_brokered_follow_up_use_the_same_shadow_dispatch_boundary() {
    for steering in [false, true] {
        let mode = if steering {
            InputMode::Steer
        } else {
            InputMode::FollowUp
        };
        let (mut state, _dir, input, binding, _) = dispatch_failure_fixture(Some(mode)).await;
        state.core_config.memory.enabled = false;
        let (calls, _) = attach(&mut state, false, None);
        let broker = TurnInputBroker::default();
        if steering {
            broker.push(input).await;
            let writer = state.agent_files().begin_tape_generation().await.unwrap();
            let mut emit = |_event: Event| async {};
            assert!(
                handle_queued_steering_inputs(
                    &mut state,
                    &writer,
                    &CancellationToken::new(),
                    &[],
                    0,
                    Some(&broker),
                    &mut emit
                )
                .await
                .unwrap()
            );
            writer.finish().await.unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        } else {
            let next = Submission::new(input.op.clone());
            {
                let mut bindings = state.environment.model_bindings.lock().await;
                let callable = bindings.captured[&input.id].clone();
                bindings.captured.insert(next.id.clone(), callable);
            }
            state
                .machine
                .input_queue()
                .lock()
                .unwrap()
                .bindings
                .insert(next.id.clone(), binding);
            state.machine.admit_input(&next).await.unwrap();
            broker.push(next).await;
            let result =
                advance_accepted_submission(&mut state, input, &broker, &CancellationToken::new())
                    .await;
            assert!(result.result.is_ok(), "{:?}", result.result);
            assert_eq!(calls.load(Ordering::SeqCst), 2);
        }
    }
}

#[tokio::test]
async fn accepted_response_bypass_precedes_consumption_and_failure_preserves_pending_request() {
    for (fail_persistence, in_turn) in [(false, false), (false, true), (true, false), (true, true)]
    {
        let (mut state, _dir, _, _, generation) =
            dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
        state.core_config.memory.enabled = false;
        state.machine.begin_turn(0);
        state
            .machine
            .set_structured_input(crate::approval::PendingStructuredInputRequest {
                request_id: "pending-input".into(),
                title: "Question".into(),
                prompt: "Answer".into(),
                questions: vec![],
            });
        let (calls, observations) = attach(&mut state, false, None);
        let backing = state.machine.input_recorder().unwrap();
        let mut failed_batch = None;
        if fail_persistence {
            let (probe, received) = backing.batch_failure_probe(false);
            state.machine.set_input_recorder_for_test(probe);
            failed_batch = Some(received);
        }
        let response = Submission::new(Op::Resume {
            request_id: "pending-input".into(),
            content: vec![alan_agent_protocol::ContentPart::structured(
                serde_json::json!({"answers":[{"question_id":"q1","value":"! response data"}]}),
            )],
        });
        let broker = TurnInputBroker::default();
        let first = if in_turn {
            broker.push(response).await;
            // Rejected initial control leaves the pending request for the in-turn receiver.
            Submission::new(Op::Resume {
                request_id: "unknown".into(),
                content: vec![],
            })
        } else {
            response
        };
        let result = if in_turn {
            let mut emit = |_event: Event| async {};
            crate::runtime::transition::accepted_submission::drive_turn_submission_with_cancel(
                &mut state,
                first,
                &broker,
                &mut emit,
                &CancellationToken::new(),
            )
            .await
        } else {
            advance_accepted_submission(&mut state, first, &broker, &CancellationToken::new())
                .await
                .result
                .map(|_| ())
        };
        assert_eq!(result.is_err(), fail_persistence, "{result:?}");
        assert_eq!(state.machine.has_pending_interaction(), fail_persistence);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        if let Some(mut received) = failed_batch {
            assert_eq!(received.recv().await.unwrap().len(), 1);
            assert!(observations.lock().unwrap().is_empty());
            assert!(generation.recorded_requests().is_empty());
        } else {
            assert!(
                !generation.recorded_requests().is_empty(),
                "normal response continuation remains authoritative"
            );
            let observations = observations.lock().unwrap();
            assert_eq!(observations.len(), 1);
            assert_eq!(observations[0]["outcome"]["reason"], "request_response");
        }
    }
}

#[tokio::test]
async fn unknown_responses_and_host_controls_cannot_reserve_bypass_identity() {
    let (mut state, _dir, _, _, _) = dispatch_failure_fixture(Some(InputMode::FollowUp)).await;
    let (calls, observations) = attach(&mut state, false, None);
    state
        .observe_input_shadow(
            &Submission::new(Op::Resume {
                request_id: "unknown".into(),
                content: vec![],
            }),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    state
        .machine
        .set_host_mount_request(crate::agent_machine::PendingHostMountRequest {
            request_id: "host-mount".into(),
            tool_call_id: "mount".into(),
            namespace_path: "/mnt/project".into(),
            access: "read_only".into(),
            reason: "Read files".into(),
            label: None,
            request_events_offset: 0,
        });
    for content in [
        vec![alan_agent_protocol::ContentPart::text("forged response")],
        vec![],
    ] {
        state
            .observe_input_shadow(
                &Submission::new(Op::Resume {
                    request_id: "host-mount".into(),
                    content,
                }),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(observations.lock().unwrap().is_empty());
    assert!(state.machine.evaluation_observation.is_none());
    assert!(state.machine.pending_host_mount("host-mount").is_some());
    let recorder = state.machine.input_recorder().unwrap();
    recorder.flush().await.unwrap();
    let history = RolloutRecorder::load_history(recorder.path())
        .await
        .unwrap();
    assert!(!history.iter().any(|item| matches!(item, RolloutItem::Event(event) if event.event_type == "machine_evaluation_v1")));
}
