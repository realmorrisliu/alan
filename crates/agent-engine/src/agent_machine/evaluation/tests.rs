use super::*;
use crate::agent_machine::AgentMachine;
use crate::rollout::{RolloutItem, RolloutRecorder};
use alan_agent_protocol::{ContentPart, InputMode, Op, Submission};

fn started() -> Observation {
    Observation {
        identity: Identity {
            source_rollout_id: "source-rollout".into(),
            submission_id: "original-input".into(),
            input_sha256: "a".repeat(64),
            surface: Surface::Interactive,
            operation_id: Some("e0".into()),
            callable: CallableIdentity {
                profile: "evaluator".into(),
                provider: "typesafe".into(),
                model: "jev-1.13.0".into(),
                credential_ref: None,
                revision: "original-revision".into(),
            },
            schema: "choice.v1".into(),
            deadline_ms: 1000,
            candidates: vec![Candidate {
                id: "command".into(),
                description: "Already literal command input".into(),
            }],
        },
        outcome: Outcome::Started,
        elapsed_ms: None,
        usage: None,
    }
}

fn event(observation: &Observation) -> EventRecord {
    EventRecord {
        event_type: EVENT_TYPE.into(),
        payload: serde_json::to_value(observation).unwrap(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

#[tokio::test]
async fn rollout_recovery_preserves_advice_identity_without_consuming_pending_input() {
    let directory = tempfile::tempdir().unwrap();
    for finished in [false, true] {
        let source = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", directory.path())
            .await
            .unwrap();
        let mut start = started();
        start.identity.source_rollout_id = source.rollout_id().unwrap().into();
        let input = Submission {
            id: start.identity.submission_id.clone(),
            intent: alan_agent_protocol::InputIntent::Agent,
            op: Op::Input {
                parts: vec![ContentPart::text("pwd")],
                mode: InputMode::FollowUp,
            },
        };
        use sha2::Digest;
        start.identity.input_sha256 = hex::encode(sha2::Sha256::digest(b"pwd"));
        source.admit_input(&input).await.unwrap();
        let mut records = vec![RolloutItem::Event(event(&start))];
        let mut terminal = start.clone();
        terminal.outcome = Outcome::Selected {
            candidate_id: "command".into(),
        };
        terminal.elapsed_ms = Some(90);
        if finished {
            records.push(RolloutItem::Event(event(&terminal)));
        }
        source
            .recorder()
            .unwrap()
            .persist_batch(records)
            .await
            .unwrap();
        let mut path = source.rollout_path().unwrap().clone();
        for pid in ["/proc/2", "/proc/3"] {
            let recovered =
                AgentMachine::load_from_rollout_in_dir(&path, pid, "mock", directory.path())
                    .await
                    .unwrap();
            let snapshot = recovered.evaluation_observation.as_ref().unwrap();
            assert_eq!(
                snapshot["identity"],
                serde_json::to_value(&start.identity).unwrap()
            );
            assert_eq!(
                snapshot["outcome"]["state"],
                if finished { "selected" } else { "interrupted" }
            );
            assert!(snapshot["cost_microusd"].is_null());
            if !finished {
                assert!(snapshot["elapsed_ms"].is_null());
            }
            {
                let queue = recovered.input_queue();
                let queue = queue.lock().unwrap();
                assert!(queue.paused);
                assert_eq!(queue.pending.len(), 1);
                assert!(queue.active_submission_ids.is_empty());
                assert!(!queue.settled_ids.contains(&input.id));
            }
            // Recovery copies source evidence without inventing another call or terminal.
            path = recovered.rollout_path().unwrap().clone();
            recovered.recorder().unwrap().flush().await.unwrap();
            let history = RolloutRecorder::load_history(&path).await.unwrap();
            assert_eq!(
                history
                    .iter()
                    .filter(
                        |item| matches!(item, RolloutItem::Event(e) if e.event_type == EVENT_TYPE)
                    )
                    .count(),
                if finished { 2 } else { 1 }
            );
            recovered.recorder().unwrap().close().await.unwrap();
        }
        source.recorder().unwrap().close().await.unwrap();
    }
}

#[test]
fn ambiguous_or_expanded_evidence_cannot_become_recovered_advice() {
    let start = started();
    let mut done = start.clone();
    done.outcome = Outcome::Selected {
        candidate_id: "command".into(),
    };
    done.elapsed_ms = Some(90);
    assert!(recover(&[event(&done)]).is_err());
    assert!(recover(&[event(&start), event(&done), event(&done)]).is_ok());
    let mut mutated = done.clone();
    mutated.identity.callable.model = "different-model".into();
    assert!(recover(&[event(&start), event(&mutated)]).is_err());
    mutated = done.clone();
    mutated.outcome = Outcome::Selected {
        candidate_id: "outside-candidates".into(),
    };
    assert!(recover(&[event(&start), event(&mutated)]).is_err());
    mutated.outcome = Outcome::Cancelled;
    assert!(recover(&[event(&start), event(&mutated), event(&done)]).is_err());
    assert!(recover(&[event(&start), event(&done), event(&start)]).is_err());
    let mut malformed = event(&start);
    malformed.payload["identity"]["deadline_ms"] = serde_json::json!(0);
    assert!(recover(&[malformed]).is_err());
    assert!(recover(&[]).unwrap().is_none());
}

#[test]
fn repeated_old_terminal_record_does_not_hide_a_newer_observation() {
    let first = started();
    let mut done = first.clone();
    done.outcome = Outcome::NoMatch;
    done.elapsed_ms = Some(10);
    let mut second = first.clone();
    second.identity.submission_id = "second-input".into();
    second.identity.operation_id = Some("e1".into());
    let events = [event(&first), event(&done), event(&second), event(&done)];
    let recovered = recover(&events).unwrap().unwrap();
    assert_eq!(recovered["identity"]["submission_id"], "second-input");
    assert_eq!(recovered["outcome"]["state"], "interrupted");
    let mut second_done = second.clone();
    second_done.outcome = Outcome::Cancelled;
    second_done.elapsed_ms = Some(20);
    let events = [
        event(&first),
        event(&done),
        event(&second),
        event(&second_done),
        event(&done),
    ];
    let recovered = recover(&events).unwrap().unwrap();
    assert_eq!(recovered["identity"]["submission_id"], "second-input");
    assert_eq!(recovered["outcome"]["state"], "cancelled");
}

#[tokio::test]
async fn writer_acknowledges_once_and_rejects_changed_or_recovered_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", directory.path())
        .await
        .unwrap();
    let mut start = started();
    start.identity.source_rollout_id = machine.rollout_id().unwrap().into();
    let payload = serde_json::to_value(&start).unwrap();
    assert!(
        machine
            .persist_evaluation_observation(payload.clone())
            .await
            .unwrap()
    );
    assert_eq!(
        machine.evaluation_observation.as_ref().unwrap()["outcome"]["state"],
        "started"
    );
    assert!(
        !machine
            .persist_evaluation_observation(payload.clone())
            .await
            .unwrap()
    );
    let mut terminal = start.clone();
    terminal.outcome = Outcome::Selected {
        candidate_id: "command".into(),
    };
    terminal.elapsed_ms = Some(30);
    assert!(
        machine
            .persist_evaluation_observation(serde_json::to_value(&terminal).unwrap())
            .await
            .unwrap()
    );
    let acknowledged = machine.evaluation_observation.clone();
    assert!(
        !machine
            .persist_evaluation_observation(payload)
            .await
            .unwrap()
    );
    assert_eq!(machine.evaluation_observation, acknowledged);
    terminal.outcome = Outcome::Cancelled;
    assert!(
        machine
            .persist_evaluation_observation(serde_json::to_value(&terminal).unwrap())
            .await
            .is_err()
    );
    let history = RolloutRecorder::load_history(machine.rollout_path().unwrap())
        .await
        .unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|item| matches!(item, RolloutItem::Event(e) if e.event_type == EVENT_TYPE))
            .count(),
        2
    );
    let mut recovered = AgentMachine::load_from_rollout_in_dir(
        machine.rollout_path().unwrap(),
        "/proc/2",
        "mock",
        directory.path(),
    )
    .await
    .unwrap();
    assert!(
        !recovered
            .persist_evaluation_observation(serde_json::to_value(&start).unwrap())
            .await
            .unwrap()
    );
    start.identity.source_rollout_id = recovered.rollout_id().unwrap().into();
    start.identity.operation_id = Some("another-operation".into());
    assert!(
        recovered
            .persist_evaluation_observation(serde_json::to_value(&start).unwrap())
            .await
            .is_err()
    );
    assert_eq!(recovered.evaluation_observation, acknowledged);
    recovered.recorder().unwrap().close().await.unwrap();
    machine.recorder().unwrap().close().await.unwrap();
}

#[tokio::test]
async fn writer_requires_storage_and_never_publishes_unacknowledged_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new();
    assert!(
        machine
            .persist_evaluation_observation(serde_json::to_value(started()).unwrap())
            .await
            .is_err()
    );
    assert!(machine.evaluation_observation.is_none());
    for terminal in [false, true] {
        let mut machine =
            AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", directory.path())
                .await
                .unwrap();
        let mut observation = started();
        observation.identity.source_rollout_id = machine.rollout_id().unwrap().into();
        if terminal {
            machine
                .persist_evaluation_observation(serde_json::to_value(&observation).unwrap())
                .await
                .unwrap();
            observation.outcome = Outcome::NoMatch;
            observation.elapsed_ms = Some(2);
        }
        let previous = machine.evaluation_observation.clone();
        let backing = machine.recorder().unwrap().clone();
        let (probe, mut observed) = backing.batch_failure_probe(false);
        machine.recorder = Some(probe);
        assert!(
            machine
                .persist_evaluation_observation(serde_json::to_value(&observation).unwrap())
                .await
                .is_err()
        );
        assert_eq!(observed.recv().await.unwrap().len(), 1);
        assert_eq!(machine.evaluation_observation, previous);
        let recovered = AgentMachine::load_from_rollout_in_dir(
            machine.rollout_path().unwrap(),
            "/proc/2",
            "mock",
            directory.path(),
        )
        .await
        .unwrap();
        if terminal {
            assert_eq!(
                recovered.evaluation_observation.as_ref().unwrap()["outcome"]["state"],
                "interrupted"
            );
        } else {
            assert!(recovered.evaluation_observation.is_none());
        }
        recovered.recorder().unwrap().close().await.unwrap();
        machine.recorder().unwrap().close().await.unwrap();
        backing.close().await.unwrap();
    }
}

#[tokio::test]
async fn reconciliation_restores_evidence_but_cannot_continue_an_interrupted_attempt() {
    let directory = tempfile::tempdir().unwrap();
    for terminal in [false, true] {
        let mut machine =
            AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", directory.path())
                .await
                .unwrap();
        let mut start = started();
        start.identity.source_rollout_id = machine.rollout_id().unwrap().into();
        machine
            .persist_evaluation_observation(serde_json::to_value(&start).unwrap())
            .await
            .unwrap();
        let mut done = start.clone();
        done.outcome = Outcome::NoMatch;
        done.elapsed_ms = Some(12);
        if terminal {
            // Model a writer that persisted the terminal before its caller received
            // acknowledgement and updated the in-memory projection.
            machine
                .recorder()
                .unwrap()
                .persist_batch(vec![RolloutItem::Event(event(&done))])
                .await
                .unwrap();
        } else {
            // A dropped start acknowledgement never established live ownership.
            machine.evaluation_observation = None;
        }
        assert!(
            !machine
                .persist_evaluation_observation(serde_json::to_value(&start).unwrap())
                .await
                .unwrap()
        );
        let expected = if terminal { "no_match" } else { "interrupted" };
        assert_eq!(
            machine.evaluation_observation.as_ref().unwrap()["outcome"]["state"],
            expected
        );
        let mut recovered = AgentMachine::load_from_rollout_in_dir(
            machine.rollout_path().unwrap(),
            "/proc/2",
            "mock",
            directory.path(),
        )
        .await
        .unwrap();
        if !terminal {
            assert!(
                machine
                    .persist_evaluation_observation(serde_json::to_value(&done).unwrap())
                    .await
                    .is_err()
            );
            assert!(
                recovered
                    .persist_evaluation_observation(serde_json::to_value(&done).unwrap())
                    .await
                    .is_err()
            );
        }
        assert_eq!(
            recovered.evaluation_observation.as_ref().unwrap()["outcome"]["state"],
            expected
        );
        recovered.recorder().unwrap().close().await.unwrap();
        machine.recorder().unwrap().close().await.unwrap();
    }
}

#[tokio::test]
async fn bypass_is_one_terminal_record_and_cannot_claim_a_model_call() {
    let directory = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "mock", directory.path())
        .await
        .unwrap();
    let mut observation = started();
    observation.identity.source_rollout_id = machine.rollout_id().unwrap().into();
    observation.identity.operation_id = None;
    observation.outcome = Outcome::Bypassed {
        reason: BypassReason::ExplicitCommand,
        evaluator_calls: 0,
    };
    observation.elapsed_ms = Some(1);
    assert!(
        machine
            .persist_evaluation_observation(serde_json::to_value(&observation).unwrap())
            .await
            .unwrap()
    );
    assert!(
        !machine
            .persist_evaluation_observation(serde_json::to_value(&observation).unwrap())
            .await
            .unwrap()
    );
    let recovered = AgentMachine::load_from_rollout_in_dir(
        machine.rollout_path().unwrap(),
        "/proc/2",
        "mock",
        directory.path(),
    )
    .await
    .unwrap();
    assert_eq!(
        recovered.evaluation_observation,
        machine.evaluation_observation
    );
    for field in ["calls", "operation", "usage"] {
        let mut invalid = observation.clone();
        match field {
            "calls" => {
                invalid.outcome = Outcome::Bypassed {
                    reason: BypassReason::ExplicitCommand,
                    evaluator_calls: 1,
                }
            }
            "operation" => invalid.identity.operation_id = Some("not-allocated".into()),
            _ => {
                invalid.usage = Some(Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                })
            }
        }
        assert!(recover(&[event(&invalid)]).is_err());
    }
    let mut evaluated = observation.clone();
    evaluated.outcome = Outcome::Started;
    evaluated.elapsed_ms = None;
    evaluated.identity.operation_id = Some("operation".into());
    assert!(recover(&[event(&observation), event(&evaluated)]).is_err());
    assert!(recover(&[event(&evaluated), event(&observation)]).is_err());
}
