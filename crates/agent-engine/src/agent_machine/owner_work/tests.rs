use super::*;
use alan_agent_protocol::{OwnerCandidate, OwnerSourceRange};

fn snapshot() -> Snapshot {
    let request = OwnerWorkRequest {
        version: 1,
        question: "Who owns the source?".into(),
        evaluator_profile: "eval".into(),
        candidates: vec![OwnerCandidate {
            id: "hostfs".into(),
            sources: vec![OwnerSourceRange {
                path: "/mnt/source/lib.rs".into(),
                start_line: 1,
                end_line: 1,
            }],
        }],
    };
    Snapshot {
        version: 1,
        work_id: uuid::Uuid::new_v4().to_string(),
        source_rollout_id: "source-rollout".into(),
        request_sha256: digest(&serde_json::to_vec(&request).unwrap()),
        request,
        evidence: vec![],
        evaluator_calls: 0,
        generation_calls: 0,
        fallback: None,
        owned_request: None,
        outcome: Outcome::Started,
    }
}
fn event(snapshot: &Snapshot) -> EventRecord {
    EventRecord {
        event_type: EVENT_TYPE.into(),
        payload: serde_json::to_value(snapshot).unwrap(),
        timestamp: "test".into(),
    }
}
#[test]
fn recovery_preserves_waits_and_interrupts_active_work_without_replenishing_attempts() {
    let start = snapshot();
    let mut active = start.clone();
    active.evaluator_calls = 1;
    let interrupted = recover(&[event(&start), event(&active)]).unwrap().unwrap();
    assert_eq!(interrupted.outcome, Outcome::Interrupted);
    assert_eq!(interrupted.evaluator_calls, 1);
    let mut wait = active.clone();
    wait.owned_request = Some("r7".into());
    wait.outcome = Outcome::Waiting {
        request_id: "r7".into(),
        reason: "generation_budget_unavailable".into(),
    };
    assert_eq!(
        recover(&[
            event(&start), event(&active), event(&wait),
            EventRecord {
                event_type: WAIT_ACK_TYPE.into(),
                payload: serde_json::json!({"work_id":wait.work_id,"request_id":"r7","request_sha256":wait.request_sha256}),
                timestamp: "test".into(),
            },
        ]).unwrap(),
        Some(wait.clone())
    );
    let unacknowledged = recover(&[event(&start), event(&active), event(&wait)])
        .unwrap()
        .unwrap();
    assert_eq!(unacknowledged.outcome, Outcome::Interrupted);
    assert_eq!(unacknowledged.owned_request, Some("r7".into()));
    let wrong_ack = EventRecord {
        event_type: WAIT_ACK_TYPE.into(),
        payload: serde_json::json!({"work_id":wait.work_id,"request_id":"r8","request_sha256":wait.request_sha256}),
        timestamp: "test".into(),
    };
    assert!(recover(&[event(&start), event(&wait), wrong_ack.clone()]).is_err());
    assert!(recover(&[event(&start), wrong_ack, event(&wait)]).is_err());
    let mut invalid = wait.clone();
    invalid.evaluator_calls = 0;
    assert!(recover(&[event(&start), event(&wait), event(&invalid)]).is_err());
    invalid = wait.clone();
    invalid.outcome = Outcome::Started;
    assert!(recover(&[event(&start), event(&wait), event(&invalid)]).is_err());
    invalid = wait.clone();
    invalid.request.question = "changed".into();
    invalid.request_sha256 = digest(&serde_json::to_vec(&invalid.request).unwrap());
    assert!(recover(&[event(&start), event(&wait), event(&invalid)]).is_err());
}

#[tokio::test]
async fn written_wait_without_flush_ack_never_restores_an_answerable_request() {
    for write_before_error in [false, true] {
        let dir = tempfile::TempDir::new().unwrap();
        let mut machine =
            super::super::AgentMachine::new_with_recorder_in_dir("/proc/1", "test", dir.path())
                .await
                .unwrap();
        let mut start = snapshot();
        start.source_rollout_id = machine.rollout_id().unwrap().into();
        machine.persist_owner_work(start.clone()).await.unwrap();
        start.evaluator_calls = 1;
        machine.persist_owner_work(start.clone()).await.unwrap();
        let backing = machine.input_recorder().unwrap();
        let (probe, mut observed) = backing.batch_failure_probe(write_before_error);
        machine.recorder = Some(probe);
        let mut wait = start;
        wait.owned_request = Some("r7".into());
        wait.outcome = Outcome::Waiting {
            request_id: "r7".into(),
            reason: "budget_unavailable".into(),
        };
        assert!(machine.persist_owner_wait(wait).await.is_err());
        assert!(
            matches!(&observed.recv().await.unwrap()[0], RolloutItem::Event(e) if e.event_type == EVENT_TYPE)
        );
        let history = RolloutRecorder::load_history(backing.path()).await.unwrap();
        assert_eq!(history.iter().any(|item| matches!(item, RolloutItem::Event(e) if e.event_type==EVENT_TYPE && e.payload["outcome"]["state"]=="waiting")), write_before_error);
        assert!(
            !history
                .iter()
                .any(|item| matches!(item, RolloutItem::Event(e) if e.event_type==WAIT_ACK_TYPE))
        );
        let recovered = super::super::AgentMachine::load_from_rollout_in_dir(
            backing.path(),
            "/proc/2",
            "test",
            dir.path(),
        )
        .await
        .unwrap();
        let work = recovered.owner_work.as_ref().unwrap();
        assert_eq!(work.outcome, Outcome::Interrupted);
        assert_eq!(work.evaluator_calls, 1);
        assert_eq!(work.generation_calls, 0);
        assert!(!recovered.has_pending_interaction());
    }
}
#[test]
fn completed_work_requires_real_captured_ranges_and_rejects_terminal_conflicts() {
    let start = snapshot();
    let mut completed = start.clone();
    completed.outcome = Outcome::Completed {
        owner: "hostfs".into(),
    };
    assert!(completed.validate().is_err());
    completed.evidence = vec![Evidence {
        owner: "hostfs".into(),
        source: completed.request.candidates[0].sources[0].clone(),
        content: "pub struct HostDirFs {}".into(),
        sha256: digest(b"pub struct HostDirFs {}"),
    }];
    assert_eq!(
        recover(&[event(&start), event(&completed), event(&completed)]).unwrap(),
        Some(completed.clone())
    );
    let mut forged = completed.clone();
    forged.outcome = Outcome::Failed {
        reason: "rewrite".into(),
    };
    assert!(recover(&[event(&start), event(&completed), event(&forged)]).is_err());
    forged = completed.clone();
    forged.evidence[0].content = "changed".into();
    assert!(forged.validate().is_err());
    assert!(completed.projection().unwrap().get("request").is_none());
}
