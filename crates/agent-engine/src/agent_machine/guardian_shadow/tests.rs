use super::*;
use crate::agent_machine::AgentMachine;

fn started() -> Observation {
    Observation {
        identity: Identity {
            task_id: "task".into(),
            tool_call_id: "call".into(),
            action_digest: "a".repeat(64),
            request_digest: "b".repeat(64),
            policy_digest: "c".repeat(64),
            baseline_request_digest: "d".repeat(64),
            baseline_binding: None,
            reviewer: CallableIdentity {
                profile: "reviewer".into(),
                provider: "openai_responses".into(),
                model: "review-model".into(),
                credential_ref: Some("key-ref".into()),
                revision: "revision".into(),
            },
        },
        baseline: Assessment::Allow,
        stage: Stage::Started,
    }
}

#[tokio::test]
async fn acknowledged_shadow_start_and_finish_survive_real_recovery_without_replay() {
    for finished in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "main", dir.path())
            .await
            .unwrap();
        let start = started();
        assert!(machine.begin_guardian_shadow(start.clone()).await.unwrap());
        if finished {
            let mut end = start.clone();
            end.stage = Stage::Finished {
                candidate: Assessment::Deny {
                    rationale: "shadow only".into(),
                },
                elapsed_millis: 12,
                final_response_usage: None,
                cost_microusd: None,
            };
            machine.persist_guardian_shadow(end).await.unwrap();
        }
        let path = machine.rollout_path().unwrap().clone();
        machine.flush_recorder().await.unwrap();
        let mut recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/proc/2", "main", dir.path())
                .await
                .unwrap();
        assert!(!recovered.begin_guardian_shadow(start).await.unwrap());
        assert_eq!(recovered.guardian_shadow.observations.len(), 1);
        assert_eq!(
            matches!(
                recovered
                    .guardian_shadow
                    .observations
                    .values()
                    .next()
                    .unwrap()
                    .stage,
                Stage::Finished { .. }
            ),
            finished
        );
    }
}

#[tokio::test]
async fn shadow_requires_durability_and_failed_acknowledgement_blocks_further_admission() {
    let mut volatile = AgentMachine::new();
    assert!(volatile.begin_guardian_shadow(started()).await.is_err());
    let dir = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/proc/1", "main", dir.path())
        .await
        .unwrap();
    machine.recorder = Some(
        machine
            .recorder
            .as_ref()
            .unwrap()
            .flush_failure_probe()
            .await,
    );
    assert!(machine.begin_guardian_shadow(started()).await.is_err());
    let mut next = started();
    next.identity.tool_call_id = "another-call".into();
    assert!(
        machine
            .begin_guardian_shadow(next)
            .await
            .unwrap_err()
            .to_string()
            .contains("uncertain")
    );
    assert!(machine.guardian_shadow.observations.is_empty());
}

#[test]
fn shadow_recovery_rejects_changed_identity_missing_start_and_replaced_outcome() {
    let start = started();
    let mut finish = start.clone();
    finish.stage = Stage::Finished {
        candidate: Assessment::Allow,
        elapsed_millis: 1,
        final_response_usage: None,
        cost_microusd: None,
    };
    assert!(validate_transition(None, &finish).is_err());
    assert!(validate_transition(Some(&start), &finish).is_ok());
    let mut changed = finish.clone();
    changed.identity.policy_digest = "e".repeat(64);
    assert!(validate_transition(Some(&start), &changed).is_err());
    changed = finish.clone();
    changed.stage = Stage::Finished {
        candidate: Assessment::Deny {
            rationale: "replacement".into(),
        },
        elapsed_millis: 1,
        final_response_usage: None,
        cost_microusd: None,
    };
    assert!(validate_transition(Some(&finish), &changed).is_err());
    assert!(validate_transition(Some(&finish), &start).is_err());
}
