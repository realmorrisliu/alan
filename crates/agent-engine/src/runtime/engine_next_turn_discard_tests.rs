use super::*;
use futures::FutureExt;

async fn queue_control(f: &mut Fixture, op: Op) -> bool {
    tokio::time::timeout(
        Duration::from_secs(5),
        f.queues
            .handle_control(&Submission::new(op), &f.state.agent_files(), None),
    )
    .await
    .expect("queue control must settle")
}

async fn cancelled_exactly(f: &Fixture, ids: &[String]) {
    let events = String::from_utf8(
        alan_shell::Shell::new(f.state.environment.root_transport())
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap(),
    )
    .unwrap();
    let receipts: Vec<_> = events
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
        receipts,
        ids.iter()
            .map(|id| (
                vec![id.clone()],
                alan_agent_protocol::UiInputStatus::Cancelled
            ))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn discard_handled_next_turn_idle_interrupt_pauses() {
    let mut f = Fixture::new().await;
    let n = next();
    let binding = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.retained(&n, &binding).await;
    assert!(!queue_control(&mut f, Op::Interrupt).await);
    assert!(f.queues.is_paused(), "NextTurn-only idle queue must pause");
    f.run(Submission::new(Op::Interrupt)).await.unwrap();
    assert!(
        f.queues.is_paused(),
        "idle interrupt transition must preserve pause"
    );
    queue_control(&mut f, Op::DiscardQueue).await;
    cancelled_exactly(&f, std::slice::from_ref(&n.id)).await;
    assert!(f.state.machine.drain_next_turn_inputs().is_empty());
    f.no_dispatch(std::slice::from_ref(&n.id)).await;
}

#[tokio::test]
async fn discard_handled_next_turn_mixed_exact_batch_and_faults() {
    for fault in [None, Some(false), Some(true)] {
        let mut f = Fixture::new().await;
        let original_recorder = f.queues.recorder.clone().unwrap();
        let phase = tokio::time::timeout(Duration::from_secs(15),
            std::panic::AssertUnwindSafe(async {
        let n = next();
        let binding = f.admit(&n).await;
        let capture = f.state.environment.model_bindings.lock().await.captured[&n.id].clone();
        f.run(n.clone()).await.unwrap();
        f.retained(&n, &binding).await;
        let handled_only = next();
        let handled_binding = f.admit(&handled_only).await;
        let handled_capture = f.state.environment.model_bindings.lock().await.captured[&handled_only.id].clone();
        f.run(handled_only.clone()).await.unwrap();
        f.retained(&handled_only, &handled_binding).await;
        let p = turn();
        let pending_binding = f.admit(&p).await;
        let pending_capture = f.state.environment.model_bindings.lock().await.captured[&p.id].clone();
        f.queues.push_outer_submission(p.clone());
        f.queues.push_outer_submission(n.clone());
        {
            let mut q = f.queues.outer_queue.lock().unwrap();
            q.queued_next_turn_inputs
                .push_back((None, vec![ContentPart::text("anonymous legacy payload")]));
            assert!(q.pending_binding_rejections.is_empty());
            assert!(!q.pending.iter().any(|item| matches!(item,
                QueuedRuntimeItem::Submission(s) if s.id == handled_only.id)),
                "distinct admitted NextTurn identity must exist exclusively in Machine container");
            q.paused = true;
        }
        let ids = vec![p.id.clone(), n.id.clone(), handled_only.id.clone()];
        let mut observed = if let Some(written) = fault {
            let (probe, observed) = f
                .queues
                .recorder
                .as_ref()
                .unwrap()
                .batch_failure_probe(written);
            f.queues.recorder = Some(probe);
            Some(observed)
        } else {
            None
        };
        queue_control(&mut f, Op::DiscardQueue).await;
        if let Some(observed) = &mut observed {
            batch(
                &tokio::time::timeout(Duration::from_secs(5), observed.recv())
                    .await
                    .expect("removal probe must settle")
                    .unwrap(),
                "machine_inputs_removed_v1",
                &ids,
            );
            assert!(observed.try_recv().is_err(), "one durable batch only");
        }
        if fault == Some(false) {
            assert!(f.queues.is_paused());
            f.retained(&n, &binding).await;
            f.retained(&handled_only, &handled_binding).await;
            f.recovery_retains(&p, &pending_binding).await;
            {
                let q = f.queues.outer_queue.lock().unwrap();
                assert_eq!(q.pending.len(), 2);
                assert_eq!(q.queued_next_turn_inputs.len(), 3);
                assert!(q.queued_next_turn_inputs.back().unwrap().0.is_none());
                assert!(q.pending_binding_rejections.is_empty());
                assert_eq!(q.bindings[&p.id], pending_binding);
                assert!(ids.iter().all(|id| q.queue_uncertain_ids.contains(id)));
            }
            let captures = f.state.environment.model_bindings.lock().await;
            let retained = &captures.captured[&n.id];
            assert_eq!(retained.identity, capture.identity);
            assert_eq!(retained.connection, capture.connection);
            assert_eq!(
                serde_json::to_value(&retained.config).unwrap(),
                serde_json::to_value(&capture.config).unwrap()
            );
            let retained = &captures.captured[&handled_only.id];
            assert_eq!(retained.identity, handled_capture.identity);
            assert_eq!(retained.connection, handled_capture.connection);
            assert_eq!(serde_json::to_value(&retained.config).unwrap(),
                serde_json::to_value(&handled_capture.config).unwrap());
            let retained = &captures.captured[&p.id];
            assert_eq!(retained.identity, pending_capture.identity);
            assert_eq!(retained.connection, pending_capture.connection);
            assert_eq!(serde_json::to_value(&retained.config).unwrap(),
                serde_json::to_value(&pending_capture.config).unwrap());
            drop(captures);
            f.no_receipt(&ids).await;
            queue_control(&mut f, Op::ContinueQueue).await;
            assert!(
                f.queues.is_paused(),
                "actual unresolved removal uncertainty must block continue"
            );
            assert!(f.queues.pop_outer().is_none(), "Continue must not dispatch queued work");
            f.retained(&n, &binding).await;
            f.retained(&handled_only, &handled_binding).await;
            f.no_receipt(&ids).await;
            f.no_dispatch(&ids).await;
            tokio::time::timeout(Duration::from_secs(5), f.queues.recorder.as_ref().unwrap().close())
                .await.expect("fault recorder cleanup bounded").unwrap();
            f.queues.recorder = Some(original_recorder.clone());
            queue_control(&mut f, Op::DiscardQueue).await;
            let history = f.history().await;
            let removals: Vec<_> = history.into_iter().filter(|item| matches!(item,
                RolloutItem::Event(e) if e.event_type == "machine_inputs_removed_v1")).collect();
            batch(&removals, "machine_inputs_removed_v1", &ids);
        }
        {
            assert!(!f.queues.is_paused());
            {
                let q = f.queues.outer_queue.lock().unwrap();
                assert!(q.pending.is_empty());
                assert!(
                    q.queued_next_turn_inputs.is_empty(),
                    "anonymous content must also be discarded"
                );
                assert!(q.pending_binding_rejections.is_empty());
                assert!(q.queue_uncertain_ids.is_empty(), "successful same-ID retry clears owned uncertainty");
                assert!(ids.iter().all(|id| !q.bindings.contains_key(id)));
            }
            assert!(ids.iter().all(|id| {
                !f.state
                    .environment
                    .model_bindings
                    .try_lock()
                    .unwrap()
                    .captured
                    .contains_key(id)
            }));
            cancelled_exactly(&f, &ids).await;
            let events = String::from_utf8(
                alan_shell::Shell::new(f.state.environment.root_transport())
                    .cat("/agent/1/machine/ui/events")
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert!(
                events.contains("Discarded 4 queued inputs without execution"),
                "count unique exact identities plus actual anonymous payloads"
            );
            let recovered = AgentMachine::load_from_rollout_in_dir(
                f.state.machine.rollout_path().unwrap(),
                "/proc/2",
                "test",
                f.temp.path(),
            )
            .await
            .unwrap();
            assert!(recovered.input_queue().lock().unwrap().pending.is_empty());
            assert!(recovered.input_queue().lock().unwrap().bindings.is_empty());
            f.no_dispatch(&ids).await;
            // Restore the ordinary writer only after asserting the failed
            // acknowledgement and exact durable disposition above.
            if fault == Some(true) {
                tokio::time::timeout(Duration::from_secs(5), f.queues.recorder.as_ref().unwrap().close())
                    .await.expect("fault recorder cleanup bounded").unwrap();
            }
            f.queues.recorder = Some(original_recorder.clone());
            let t = turn();
            f.admit(&t).await;
            tokio::time::timeout(Duration::from_secs(5), f.run(t))
                .await
                .expect("later explicit Turn must settle")
                .unwrap();
            let requests = f.provider.recorded_requests();
            assert_eq!(requests.len(), 1);
            let request = serde_json::to_string(&requests[0].messages).unwrap();
            assert!(!request.contains("exact queued payload"));
            assert!(!request.contains("anonymous legacy payload"));
            assert!(request.contains("exact trigger"));
        }
        }).catch_unwind()).await;
        // Cleanup is separately bounded and cannot replace the original assertion panic.
        let cleanup = tokio::time::timeout(Duration::from_secs(5), async {
            f.queues.recorder.as_ref().unwrap().close().await?;
            original_recorder.close().await
        })
        .await;
        match phase {
            Ok(Err(panic)) => std::panic::resume_unwind(panic),
            Err(error) => {
                panic!("mixed discard assertion phase timed out: {error}; cleanup={cleanup:?}")
            }
            Ok(Ok(())) => cleanup.expect("recorder cleanup must settle").unwrap(),
        }
    }
}

#[tokio::test]
async fn discard_handled_next_turn_continue_preserves_payload() {
    let mut f = Fixture::new().await;
    let n = next();
    let binding = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.queues.pause();
    queue_control(&mut f, Op::ContinueQueue).await;
    assert!(!f.queues.is_paused());
    f.retained(&n, &binding).await;
    f.no_receipt(std::slice::from_ref(&n.id)).await;
    assert_eq!(f.state.machine.drain_next_turn_inputs().len(), 1);
}

#[tokio::test]
async fn discard_handled_next_turn_targeted_interrupt_clears_rejection() {
    let mut f = Fixture::new().await;
    let n = next();
    let binding = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    f.retained(&n, &binding).await;
    f.queues
        .outer_queue
        .lock()
        .unwrap()
        .pending_binding_rejections
        .insert(n.id.clone());
    queue_control(
        &mut f,
        Op::InterruptSubmission {
            submission_id: n.id.clone(),
        },
    )
    .await;
    assert!(f.queues.is_paused());
    assert!(
        f.queues
            .outer_queue
            .lock()
            .unwrap()
            .pending_binding_rejections
            .is_empty()
    );
    cancelled_exactly(&f, std::slice::from_ref(&n.id)).await;
    assert!(f.state.machine.drain_next_turn_inputs().is_empty());
    queue_control(&mut f, Op::ContinueQueue).await;
    assert!(!f.queues.is_paused());
}

#[tokio::test]
async fn discard_handled_next_turn_recovered_handler_excludes_exact_input() {
    let mut f = Fixture::new().await;
    let n = next();
    let binding = f.admit(&n).await;
    f.run(n.clone()).await.unwrap();
    let recovered = AgentMachine::load_from_rollout_in_dir(
        f.state.machine.rollout_path().unwrap(),
        "/proc/2",
        "test",
        f.temp.path(),
    )
    .await
    .unwrap();
    f.state.machine = recovered;
    f.queues = RuntimeSubmissionQueues::new(f.state.machine.input_queue());
    f.queues.environment = Some(f.state.environment.clone());
    f.queues.recorder = f.state.machine.input_recorder();
    let item = f
        .queues
        .outer_queue
        .lock()
        .unwrap()
        .pending
        .pop_front()
        .unwrap();
    let QueuedRuntimeItem::Submission(input) = item else {
        panic!("recovered input required")
    };
    assert_eq!(input.id, n.id);
    f.run(input).await.unwrap();
    f.retained(&n, &binding).await;
    queue_control(&mut f, Op::DiscardQueue).await;
    cancelled_exactly(&f, std::slice::from_ref(&n.id)).await;
    assert!(f.state.machine.drain_next_turn_inputs().is_empty());
    let recovered_again = AgentMachine::load_from_rollout_in_dir(
        f.state.machine.rollout_path().unwrap(),
        "/proc/3",
        "test",
        f.temp.path(),
    )
    .await
    .unwrap();
    assert!(
        recovered_again
            .input_queue()
            .lock()
            .unwrap()
            .pending
            .is_empty()
    );
    f.no_dispatch(std::slice::from_ref(&n.id)).await;
}
