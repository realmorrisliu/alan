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

    for index in 0..11 {
        let id = environment
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

    let ids = environment.request_ids().await.unwrap();
    assert_eq!(
        environment.pending_request_id(&ids).await.unwrap(),
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

    let compact = environment
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("compact command should produce a submission");
    assert!(matches!(compact.op, Op::CompactWithOptions { focus: None }));

    let rollback = environment
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("rollback command should produce a submission");
    assert!(matches!(rollback.op, Op::Rollback { turns: 1 }));

    assert!(
        environment
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
    let interrupt = environment
        .read_next_machine_control_submission()
        .await
        .unwrap()
        .expect("interrupt command should produce a submission");
    assert!(matches!(interrupt.op, Op::Interrupt));
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

    shell
        .write("/agent/1/io/input", b"continue from files")
        .await
        .unwrap();
    let submission = environment
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
    let message = "x".repeat(70 * 1024);

    shell
        .write("/agent/1/io/input", message.as_bytes())
        .await
        .unwrap();
    let submission = environment
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
