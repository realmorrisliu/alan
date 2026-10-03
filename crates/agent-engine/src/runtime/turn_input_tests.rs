use super::*;
use std::sync::Arc;

use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_shell::Shell;

#[test]
fn test_inband_and_resume_submission_classification() {
    assert!(is_turn_resume_submission(&Op::Resume {
        request_id: "latest".to_string(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            serde_json::json!({"choice": "approve"})
        )],
    }));
    assert!(!is_turn_inband_submission(&Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
        mode: InputMode::FollowUp,
    })));
    assert!(is_turn_inband_submission(&Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("steer")],
        mode: InputMode::Steer,
    })));
    assert!(!is_turn_inband_submission(&Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("next turn")],
        mode: InputMode::NextTurn,
    })));
    assert!(is_turn_inband_submission(&Submission::new(Op::Resume {
        request_id: "latest".to_string(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            serde_json::json!({"choice": "approve"})
        )],
    })));
    assert!(is_turn_resume_submission(&Op::Resume {
        request_id: "r1".to_string(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            serde_json::json!({"answers": []})
        )],
    }));
    assert!(is_turn_resume_submission(&Op::Resume {
        request_id: "c1".to_string(),
        content: vec![alan_agent_protocol::ContentPart::structured(
            serde_json::json!({"success": true})
        )],
    }));
}

#[tokio::test]
async fn machine_broker_handles_share_wakeups_and_capacity() {
    let machine = AgentMachine::new();
    let receiver = TurnInputBroker::from_queue(machine.input_queue());
    let sender = TurnInputBroker::from_queue(machine.input_queue());
    let cancel = CancellationToken::new();
    let waiting = receiver.recv(&cancel);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(1), &mut waiting)
            .await
            .is_err()
    );
    let input = Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("steering")],
        mode: InputMode::Steer,
    });
    assert!(sender.push(input.clone()).await);
    let received = tokio::time::timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.id, input.id);
    for _ in 0..MAX_BROKERED_INBAND_USER_INPUTS {
        assert!(sender.push(input.clone()).await);
    }
    assert!(!receiver.push(input).await);
    assert_eq!(
        receiver.drain().await.len(),
        MAX_BROKERED_INBAND_USER_INPUTS
    );
    assert!(sender.try_recv().await.is_none());
}

#[tokio::test]
async fn test_turn_input_broker_roundtrip_and_drain() {
    let broker = TurnInputBroker::default();
    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "sub-1".to_string(),
                op: Op::Resume {
                    request_id: "latest".to_string(),
                    content: vec![alan_agent_protocol::ContentPart::structured(
                        serde_json::json!({"choice": "approve"}),
                    )],
                },
            })
            .await
    );

    let cancel = CancellationToken::new();
    let got = broker.recv(&cancel).await.expect("queued submission");
    assert_eq!(got.id, "sub-1");

    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "sub-2".to_string(),
                op: Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text("follow up")],
                    mode: InputMode::Steer,
                },
            })
            .await
    );
    let got = broker.try_recv().await.expect("queued submission");
    assert_eq!(got.id, "sub-2");

    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "sub-3".to_string(),
                op: Op::Resume {
                    request_id: "r1".to_string(),
                    content: vec![alan_agent_protocol::ContentPart::structured(
                        serde_json::json!({"answers": []}),
                    )],
                },
            })
            .await
    );
    let drained = broker.drain().await;
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].id, "sub-3");
    cancel.cancel();
    assert!(broker.recv(&cancel).await.is_none());
}

#[tokio::test]
async fn test_turn_input_broker_caps_user_inputs_but_allows_resume_submissions() {
    let broker = TurnInputBroker::default();
    for idx in 0..MAX_BROKERED_INBAND_USER_INPUTS {
        assert!(
            broker
                .push(Submission {
                    intent: Default::default(),
                    id: format!("u-{idx}"),
                    op: Op::Input {
                        parts: vec![alan_agent_protocol::ContentPart::text(format!("msg {idx}"))],
                        mode: InputMode::Steer,
                    },
                })
                .await
        );
    }

    assert!(
        !broker
            .push(Submission {
                intent: Default::default(),
                id: "u-overflow".to_string(),
                op: Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text("overflow")],
                    mode: InputMode::Steer,
                },
            })
            .await
    );
    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "resume-1".to_string(),
                op: Op::Resume {
                    request_id: "latest".to_string(),
                    content: vec![alan_agent_protocol::ContentPart::structured(
                        serde_json::json!({"choice": "approve"}),
                    )],
                },
            })
            .await
    );
}

#[tokio::test]
async fn cancellation_drop_is_durable_or_retained_without_settlement() {
    for (fail, written) in [(false, false), (true, false), (false, true)] {
        let dir = tempfile::tempdir().unwrap();
        let mut machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
            .await
            .unwrap();
        let path = machine.rollout_path().unwrap().clone();
        let broker = TurnInputBroker::from_queue(machine.input_queue());
        machine.accept_submission("original");
        for (idx, mode) in [InputMode::Steer, InputMode::FollowUp]
            .into_iter()
            .enumerate()
        {
            let input = Submission::new(Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text("drop")],
                mode,
            });
            machine.admit_input(&input).await.unwrap();
            if idx == 0 {
                assert!(broker.push(input).await);
            } else {
                machine.push_buffered_inband_submission(input);
            }
        }
        let mut observed = if written {
            let (probe, observed) = machine.input_recorder().unwrap().batch_failure_probe(true);
            machine.set_input_recorder_for_test(probe);
            Some(observed)
        } else {
            None
        };
        if fail {
            machine.input_recorder().unwrap().close().await.unwrap();
        }
        let mut events = Vec::new();
        let mut emit = |event| {
            events.push(event);
            async {}
        };
        let result = emit_dropped_in_turn_submissions(&mut emit, &mut machine, &broker).await;
        assert_eq!(result.is_err(), fail);
        if let Some(observed) = observed.as_mut() {
            let batch = observed.recv().await.unwrap();
            assert_eq!(batch.len(), 1);
            assert!(observed.try_recv().is_err());
            machine.input_recorder().unwrap().close().await.unwrap();
        }
        assert_eq!(events.len(), 1);
        if fail {
            assert!(
                matches!(&events[0], Event::Error { message, .. } if message.contains("uncertain") && !message.contains("Dropped"))
            );
        }
        assert_eq!(
            machine.buffered_inband_user_input_count(),
            if fail { 2 } else { 0 }
        );
        if fail {
            assert_eq!(machine.current_submission_id(), Some("original"));
        }
        let recovered =
            AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
                .await
                .unwrap();
        assert_eq!(
            recovered.input_queue().lock().unwrap().pending.len(),
            if fail { 2 } else { 0 }
        );
    }
}

#[tokio::test]
async fn test_emit_dropped_in_turn_submissions_reports_count() {
    let broker = TurnInputBroker::default();
    let mut machine = AgentMachine::new();
    machine.accept_submission("original");
    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "u-2".into(),
                op: Op::Input {
                    parts: vec![alan_agent_protocol::ContentPart::text("brokered")],
                    mode: InputMode::Steer
                },
            })
            .await
    );
    machine.push_buffered_inband_submission(Submission {
        intent: Default::default(),
        id: "u-1".to_string(),
        op: Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("queued")],
            mode: InputMode::Steer,
        },
    });
    assert!(
        broker
            .push(Submission {
                intent: Default::default(),
                id: "c-1".to_string(),
                op: Op::Resume {
                    request_id: "latest".to_string(),
                    content: vec![alan_agent_protocol::ContentPart::structured(
                        serde_json::json!({"choice": "approve"}),
                    )],
                },
            })
            .await
    );

    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    emit_dropped_in_turn_submissions(&mut emit, &mut machine, &broker)
        .await
        .unwrap();

    machine.reset_turn();
    assert_eq!(machine.current_submission_id(), Some("u-2"));
    assert_eq!(machine.related_submission_ids(), ["original", "u-1"]);
    assert_eq!(machine.clear_buffered_inband_submissions(), 0);
    assert!(broker.try_recv().await.is_none());
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Error { message, recoverable }
            if *recoverable && message.contains("Dropped 3 in-turn buffered submissions")
    )));
}

#[tokio::test]
async fn pending_wait_overflow_writer_failure_retains_input_without_completion() {
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment =
        super::super::transition::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let dir = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    for _ in 0..MAX_BUFFERED_INBAND_USER_INPUTS {
        machine.push_buffered_inband_submission(Submission::new(Op::Input {
            parts: vec![],
            mode: InputMode::FollowUp,
        }));
    }
    let input = Submission::new(Op::Input {
        parts: vec![],
        mode: InputMode::Steer,
    });
    machine.admit_input(&input).await.unwrap();
    machine.input_recorder().unwrap().close().await.unwrap();
    let broker = TurnInputBroker::from_queue(machine.input_queue());
    assert!(broker.push(input.clone()).await);
    let mut events = vec![];
    let mut emit = |event| {
        events.push(event);
        async {}
    };
    let result = next_pending_interaction_submission(
        &mut machine,
        &environment.agent_files(),
        &environment.host_mount_requests(),
        &broker,
        &mut emit,
        &CancellationToken::new(),
    )
    .await;
    assert!(result.is_err());
    assert!(events.is_empty());
    assert_eq!(
        machine.buffered_inband_user_input_count(),
        MAX_BUFFERED_INBAND_USER_INPUTS + 1
    );
    assert!(
        shell
            .cat("/agent/1/machine/ui/events")
            .await
            .unwrap()
            .is_empty()
    );
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    assert_eq!(recovered.input_queue().lock().unwrap().pending.len(), 1);
}

#[tokio::test]
async fn namespace_answered_request_unblocks_pending_interaction_wait() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment =
        super::super::transition::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");

    let agent_files = environment.agent_files();
    let host_mount_requests = environment.host_mount_requests();
    let request_id = agent_files
        .write_request(super::super::transition::NamespaceRequestRecord::new(
            "structured_input",
            "Provide the missing value",
        ))
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut machine = AgentMachine::new_with_recorder_in_dir("/agent/1", "test", dir.path())
        .await
        .unwrap();
    let path = machine.rollout_path().unwrap().clone();
    machine.set_structured_input(crate::approval::PendingStructuredInputRequest {
        request_id: request_id.clone(),
        title: "Missing value".to_string(),
        prompt: "Provide the missing value".to_string(),
        questions: Vec::new(),
    });

    let broker = TurnInputBroker::default();
    for _ in 0..MAX_BUFFERED_INBAND_USER_INPUTS {
        machine.push_buffered_inband_submission(Submission::new(Op::Input {
            parts: vec![alan_agent_protocol::ContentPart::text("queued")],
            mode: InputMode::Steer,
        }));
    }
    let overflow = Submission::new(Op::Input {
        parts: vec![alan_agent_protocol::ContentPart::text("overflow")],
        mode: InputMode::Steer,
    });
    let overflow_id = overflow.id.clone();
    machine.admit_input(&overflow).await.unwrap();
    assert!(broker.push(overflow).await);
    let cancel = CancellationToken::new();
    let mut events = Vec::new();
    let mut emit = |event| {
        events.push(event);
        async {}
    };

    let response_path = format!("/agent/1/requests/{request_id}/response");
    let waiter = next_pending_interaction_submission(
        &mut machine,
        &agent_files,
        &host_mount_requests,
        &broker,
        &mut emit,
        &cancel,
    );
    let writer = async {
        tokio::time::sleep(Duration::from_millis(25)).await;
        shell
            .write(
                &response_path,
                br#"{"answers":[{"question_id":"q1","value":"from agentfs"}]}"#,
            )
            .await
            .unwrap();
    };
    let (submission, _) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(waiter, writer)
    })
    .await
    .expect("namespace response should unblock pending wait");
    let submission = submission
        .unwrap()
        .expect("answered request should become next submission");

    match submission.op {
        Op::Resume {
            request_id: resumed_id,
            content,
        } => {
            assert_eq!(resumed_id, request_id);
            assert_eq!(
                content,
                vec![alan_agent_protocol::ContentPart::structured(
                    serde_json::json!({
                        "answers": [{"question_id": "q1", "value": "from agentfs"}]
                    })
                )]
            );
        }
        other => panic!("expected Op::Resume from namespace response, got {other:?}"),
    }
    assert!(events.iter().any(
        |event| matches!(event, Event::Error { message, .. } if message.contains("Too many queued"))
    ));
    let ui = shell.cat("/agent/1/machine/ui/events").await.unwrap();
    let terminal: alan_agent_protocol::UiEvent = serde_json::from_slice(&ui).unwrap();
    assert!(
        matches!(terminal, alan_agent_protocol::UiEvent::InputCompleted {
            submission_ids, status: alan_agent_protocol::UiInputStatus::Failed, error: Some(_)
        } if submission_ids == [overflow_id])
    );
    let recovered = AgentMachine::load_from_rollout_in_dir(&path, "/agent/2", "test", dir.path())
        .await
        .unwrap();
    assert!(recovered.input_queue().lock().unwrap().pending.is_empty());
    assert_eq!(
        machine.buffered_inband_user_input_count(),
        MAX_BUFFERED_INBAND_USER_INPUTS
    );
}

#[tokio::test]
async fn host_mount_service_terminal_status_unblocks_recovered_logical_wait() {
    let agentfs = Arc::new(AgentFs::new());
    let host_mount = crate::runtime::test_host_mount::TestHostMountFs::new();
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/host-mount",
        InProcessTransport::new(host_mount.clone()),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let environment =
        super::super::transition::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();
    let host_mount_requests = environment.host_mount_requests();
    let request_document = serde_json::json!({
        "namespace_path": "/mnt/project",
        "access": "read_only",
        "reason": "Read project files"
    });
    let request_id = host_mount_requests
        .create(&serde_json::to_vec(&request_document).unwrap())
        .await
        .unwrap();
    let mut machine = AgentMachine::new();
    machine.set_host_mount_request(crate::agent_machine::PendingHostMountRequest {
        request_id: request_id.clone(),
        tool_call_id: "call-mount".to_string(),
        namespace_path: "/mnt/project".to_string(),
        access: "read_only".to_string(),
        reason: "Read project files".to_string(),
        label: None,
        request_events_offset: 0,
    });
    assert!(agent_files.request_ids().await.unwrap().is_empty());

    let broker = TurnInputBroker::default();
    let cancel = CancellationToken::new();
    let mut emit = |_event| async {};
    let waiter = next_pending_interaction_submission(
        &mut machine,
        &agent_files,
        &host_mount_requests,
        &broker,
        &mut emit,
        &cancel,
    );
    let settler = async {
        tokio::time::sleep(Duration::from_millis(25)).await;
        host_mount
            .settle(&request_id, "approved", Some("grant-opaque"), None)
            .await;
    };
    let (submission, _) = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(waiter, settler)
    })
    .await
    .expect("Host Mount terminal status should unblock pending wait");
    let submission = submission
        .unwrap()
        .expect("service status becomes a submission");
    assert_eq!(submission.id, format!("host-mount:{request_id}"));
    match submission.op {
        Op::Resume {
            request_id: resumed_id,
            content,
        } => {
            assert_eq!(resumed_id, request_id);
            assert!(content.is_empty());
        }
        other => panic!("expected Host Mount Op::Resume, got {other:?}"),
    }
}
