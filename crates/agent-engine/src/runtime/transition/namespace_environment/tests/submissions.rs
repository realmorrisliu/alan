use super::*;

#[tokio::test]
async fn pending_request_selection_ignores_lexicographic_id_order() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();

    for index in 0..11 {
        let id = agent_files
            .write_request(NamespaceRequestRecord::new("confirmation", "approve?"))
            .await
            .unwrap();
        assert_eq!(id, format!("r{index}"));
        if index < 10 {
            shell
                .write(&format!("/agent/1/requests/{id}/response"), b"approved")
                .await
                .unwrap();
        }
    }

    let ids = agent_files.request_ids().await.unwrap();
    assert_eq!(
        agent_files.pending_request_id(&ids).await.unwrap(),
        Some("r10".into())
    );
}

#[tokio::test]
async fn machine_ctl_records_become_control_submissions_in_order() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();

    shell
        .write("/agent/1/io/output", b"assistant output")
        .await
        .unwrap();
    shell
        .write("/agent/1/machine/ctl", b"compact")
        .await
        .unwrap();
    shell
        .write("/agent/1/machine/ctl", b"rollback")
        .await
        .unwrap();

    let compact = agent_files
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("compact command should produce a submission");
    assert!(matches!(compact.op, Op::CompactWithOptions { focus: None }));

    let rollback = agent_files
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("rollback command should produce a submission");
    assert!(matches!(rollback.op, Op::Rollback { turns: 1 }));

    assert!(
        agent_files
            .read_next_machine_control_submission()
            .await
            .unwrap()
            .is_none()
    );

    // Turn interrupt is a machine/ctl verb: a file client's Esc must
    // cancel the running turn without touching kernel process lifecycle.
    shell
        .write("/agent/1/machine/ctl", b"interrupt")
        .await
        .unwrap();
    let interrupt = agent_files
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("interrupt command should produce a submission");
    assert!(matches!(interrupt.op, Op::Interrupt));
    for (verb, discard) in [("queue-v1 continue", false), ("queue-v1 discard", true)] {
        shell
            .write("/agent/1/machine/ctl", verb.as_bytes())
            .await
            .unwrap();
        let control = agent_files
            .read_next_machine_control_submission()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(matches!(control.op, Op::DiscardQueue), discard);
        assert_eq!(matches!(control.op, Op::ContinueQueue), !discard);
    }
}

#[tokio::test]
async fn input_frame_becomes_engine_input_submission() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();

    shell
        .write("/agent/1/io/input", b"continue from files")
        .await
        .unwrap();
    let submission = agent_files
        .read_next_input_submission(InputMode::FollowUp)
        .await
        .unwrap();

    match submission.op {
        Op::Input { parts, mode } => {
            assert_eq!(mode, InputMode::FollowUp);
            assert_eq!(parts, vec![ContentPart::text("continue from files")]);
        }
        other => panic!("expected Op::Input, got {other:?}"),
    }
}

#[tokio::test]
async fn input_frame_larger_than_initial_read_becomes_submission() {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let agent_files = environment.agent_files();
    let message = "x".repeat(70 * 1024);

    shell
        .write("/agent/1/io/input", message.as_bytes())
        .await
        .unwrap();
    let submission = agent_files
        .read_next_input_submission(InputMode::FollowUp)
        .await
        .unwrap();

    match submission.op {
        Op::Input { parts, mode } => {
            assert_eq!(mode, InputMode::FollowUp);
            assert_eq!(parts, vec![ContentPart::text(message)]);
        }
        other => panic!("expected Op::Input, got {other:?}"),
    }
}

#[tokio::test]
async fn versioned_inputs_keep_client_identity_and_intent() {
    use alan_agent_protocol::{InputIntent, UserInputRecord};
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let first_client = Shell::new(root.clone());
    let second_client = Shell::new(root.clone());
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let files = environment.agent_files();
    let first = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, "first\nbody");
    let second = UserInputRecord::new(InputIntent::ForceAgent, InputMode::Steer, "!literal text");
    first_client
        .write("/agent/1/io/input", &first.encode_payload().unwrap())
        .await
        .unwrap();
    second_client
        .write("/agent/1/io/input", &second.encode_payload().unwrap())
        .await
        .unwrap();
    for record in [first, second] {
        let submission = files
            .read_next_input_submission(InputMode::NextTurn)
            .await
            .unwrap();
        assert_eq!(submission.id, record.submission_id);
        assert_eq!(submission.intent, record.intent);
        match submission.op {
            Op::Input { parts, mode } => {
                assert_eq!(mode, record.mode);
                assert_eq!(parts, vec![ContentPart::text(record.body)]);
            }
            other => panic!("unexpected operation: {other:?}"),
        }
    }
    let command = UserInputRecord::new(InputIntent::Command, InputMode::FollowUp, "pwd");
    first_client
        .write("/agent/1/io/input", &command.encode_payload().unwrap())
        .await
        .unwrap();
    let admitted = files
        .read_next_input_submission(InputMode::NextTurn)
        .await
        .unwrap();
    assert_eq!(admitted.intent, InputIntent::Command);
    assert_eq!(admitted.id, command.submission_id);
    let record = UserInputRecord::new(InputIntent::Agent, InputMode::FollowUp, "typed body");
    first_client
        .write("/agent/1/io/input", &record.encode_payload().unwrap())
        .await
        .unwrap();
    assert!(
        files
            .read_next_input()
            .await
            .unwrap_err()
            .to_string()
            .contains("submission-aware")
    );
    first_client
        .write("/agent/1/io/input", b"legacy body")
        .await
        .unwrap();
    assert_eq!(files.read_next_input().await.unwrap(), "legacy body");
}
