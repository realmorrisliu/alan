//! Correlated removal record acknowledgement boundaries owned by rollout.
use super::*;

#[tokio::test]
async fn targeted_queue_cancellation_reconciles_failed_ack_with_durable_recovery() {
    for written in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        let path = machine.rollout_path().unwrap().clone();
        let mut queues = RuntimeSubmissionQueues::new(machine.input_queue());
        queues.recorder = machine.input_recorder();
        for id in ["keep", "cancel-me"] {
            let input = Submission {
                id: id.into(),
                intent: Default::default(),
                op: Op::Turn {
                    parts: vec![],
                    context: None,
                },
            };
            queues.admit_input(&input).await.unwrap();
            queues.push_outer_submission(input);
        }
        let (probe, mut observed) = queues
            .recorder
            .as_ref()
            .unwrap()
            .batch_failure_probe(written);
        queues.recorder = Some(probe.clone());
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/agent/1",
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
        let shell = alan_shell::Shell::new(root.clone());
        let files = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default").agent_files();
        assert!(
            queues
                .handle_control(
                    &Submission::new(Op::InterruptSubmission {
                        submission_id: "cancel-me".into(),
                    }),
                    &files,
                    None
                )
                .await
        );
        let batch = observed.recv().await.unwrap();
        assert_eq!(batch.len(), 1);
        let local = queues
            .outer_queue
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            local,
            if written {
                vec!["keep"]
            } else {
                vec!["keep", "cancel-me"]
            }
        );
        assert!(
            matches!(&batch[0], crate::rollout::RolloutItem::Event(event)
            if event.event_type == "machine_inputs_removed_v1"
                && event.payload["submission_ids"] == serde_json::json!(["cancel-me"]))
        );
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
                .await
                .unwrap();
        let recovered_ids = recovered
            .input_queue()
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter_map(|item| match item {
                QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            recovered_ids, local,
            "local disposition must match actual durable evidence"
        );
        let events =
            String::from_utf8(shell.cat("/agent/1/machine/ui/events").await.unwrap()).unwrap();
        if written {
            assert!(events.lines().any(|line| matches!(serde_json::from_str::<alan_agent_protocol::UiEvent>(line).unwrap(),
                alan_agent_protocol::UiEvent::InputCompleted { submission_ids, status: alan_agent_protocol::UiInputStatus::Cancelled, .. }
                if submission_ids == ["cancel-me"])));
            assert!(!events.contains("Queue cancellation rejected"));
        } else {
            assert!(events.contains("uncertain"));
            assert!(events.contains("cancel-me"));
            assert!(!events.contains("Queued input cancelled without execution"));
        }
        assert!(observed.try_recv().is_err());
        probe.close().await.unwrap();
    }
}

#[tokio::test]
async fn removal_evidence_uses_full_canonical_history_validation() {
    for suffix in [
        "\n{invalid}\n".as_bytes(),
        b"\n\xff\n",
        b"\n{\"type\":\"unknown_current_record\",\"data\":{}}\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        let ids = vec!["one".to_owned(), "two".to_owned()];
        crate::agent_machine::input_queue::persist_input_removals(Some(&recorder), &ids)
            .await
            .unwrap();
        recorder.close().await.unwrap();
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .append(true)
            .open(recorder.path())
            .await
            .unwrap();
        file.write_all(suffix).await.unwrap();
        file.flush().await.unwrap();
        assert!(
            crate::rollout::RolloutRecorder::load_history(recorder.path())
                .await
                .is_err()
        );
        assert!(
            crate::agent_machine::input_queue::persist_input_removals(Some(&recorder), &ids)
                .await
                .is_err(),
            "an earlier matching removal cannot bypass invalid later history"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let recorder = crate::rollout::RolloutRecorder::new_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let ids = vec!["one".to_owned(), "two".to_owned()];
    crate::agent_machine::input_queue::persist_input_removals(Some(&recorder), &ids)
        .await
        .unwrap();
    recorder.close().await.unwrap();
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(recorder.path())
        .await
        .unwrap();
    file.write_all(b"{\"type\":").await.unwrap();
    file.flush().await.unwrap();
    assert!(
        crate::agent_machine::input_queue::persist_input_removals(Some(&recorder), &ids)
            .await
            .is_ok(),
        "canonical torn EOF tolerance still permits complete earlier evidence"
    );
}

#[tokio::test]
async fn removal_ack_failure_is_uncertain_and_recovery_never_removes_a_prefix() {
    for written in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        let path = machine.rollout_path().unwrap().clone();
        let ids: Vec<_> = ["first", "second", "third"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        for id in &ids {
            machine
                .admit_input(&Submission {
                    id: id.clone(),
                    intent: Default::default(),
                    op: Op::Turn {
                        parts: vec![],
                        context: None,
                    },
                })
                .await
                .unwrap();
        }
        let (probe, mut observed) = machine
            .input_recorder()
            .unwrap()
            .batch_failure_probe(written);
        let result =
            crate::agent_machine::input_queue::persist_input_removals(Some(&probe), &ids).await;
        assert_eq!(
            result.is_ok(),
            written,
            "complete evidence must reconcile failed flush"
        );
        if let Err(error) = result {
            assert!(error.to_string().contains("uncertain"));
            for id in &ids {
                assert!(error.to_string().contains(id));
            }
        }
        let batch = observed.recv().await.unwrap();
        assert_eq!(batch.len(), 1);
        assert!(observed.try_recv().is_err());
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
                .await
                .unwrap();
        let pending: Vec<_> = {
            let queue = recovered.input_queue();
            let queue = queue.lock().unwrap();
            queue
                .pending
                .iter()
                .filter_map(|item| match item {
                    QueuedRuntimeItem::Submission(input) => Some(input.id.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            pending,
            if written { vec![] } else { ids.clone() },
            "failed ack is all-or-none recovery evidence, never a per-ID prefix"
        );
        if written {
            crate::agent_machine::input_queue::persist_input_removals(Some(&probe), &ids)
                .await
                .unwrap();
            assert!(
                observed.try_recv().is_err(),
                "retry reconciles evidence without rewriting"
            );
        }
        probe.close().await.unwrap();
    }
}
