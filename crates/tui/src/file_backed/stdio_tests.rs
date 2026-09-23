use super::*;
use std::sync::Arc;

#[test]
fn line_drain_keeps_partial_records_until_newline() {
    let mut pending = b"{\"role\":\"user\"}\n{\"role\"".to_vec();

    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role":"user"}"#.to_vec()]
    );
    assert_eq!(pending, b"{\"role\"");

    pending.extend_from_slice(b": \"assistant\"}\n");
    assert_eq!(
        drain_lines(&mut pending),
        vec![br#"{"role": "assistant"}"#.to_vec()]
    );
    assert!(pending.is_empty());
}

#[test]
fn snapshot_restores_the_latest_matching_turn_after_root_process_change() {
    let snapshot = stdio_task_snapshot_from_history(
        "same task",
        br#"{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"old answer"}
{"version":1,"kind":"message","role":"user","content":"same task"}
{"version":1,"kind":"message","role":"assistant","content":"intermediate response"}
{"version":1,"kind":"message","role":"assistant","content":"current answer"}
"#,
        br#"{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(snapshot.task_seen);
    assert_eq!(snapshot.assistant_answer.as_deref(), Some("current answer"));
    assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
    assert!(snapshot.task_error.is_none());
}

#[test]
fn one_shot_result_waits_for_seen_task_and_idle_activity() {
    let mut task = StdioTaskSnapshot {
        task_seen: false,
        assistant_answer: Some("answer".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: None,
    };

    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.task_seen = true;
    task.activity_state = Some(UiActivityState::Running);
    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.activity_state = Some(UiActivityState::Idle);
    assert_eq!(
        finish_stdio_task_if_ready(&mut task).unwrap().as_deref(),
        Some("answer")
    );

    let mut failed_task = StdioTaskSnapshot {
        task_seen: true,
        assistant_answer: None,
        activity_state: Some(UiActivityState::Idle),
        task_error: Some("provider failed".to_string()),
    };
    let result = finish_stdio_task_if_ready(&mut failed_task);
    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider failed"
    );
}

#[tokio::test]
async fn one_shot_recovers_answer_when_root_agent_pid_changes() {
    use alan_agentfs::{AgentFs, AgentRootFs};
    use alan_ap::ProcessEventSource;
    use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, ProcFs};

    const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";
    const EXEC_SPEC: &str =
        r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;

    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn alan_ap::FileServer> = proc.clone();
    let proc_events: Arc<dyn ProcessEventSource> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        proc_server,
        proc_events,
    ));
    let mut namespace = Namespace::new();
    namespace.mount("/proc", InProcessTransport::new(proc), Access::ReadWrite);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let live_namespace = LiveNamespace::new(namespace);
    let root_transport = InProcessTransport::new(Arc::new(MountFs::from_live_namespace(
        live_namespace.clone(),
    )));
    let shell = alan_shell::Shell::new(root_transport);

    let old_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(old_pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(old_pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{old_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    let (mut tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (mut ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let controller_shell = shell.clone();
    let controller_agent_root = agent_root.clone();
    let controller_namespace = live_namespace.clone();
    let controller = tokio::spawn(async move {
        assert!(!input_tail.read(4096).await.unwrap().is_empty());
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let new_pid = controller_shell.spawn(EXEC_SPEC).await.unwrap();
        controller_agent_root
            .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
            .await;
        controller_agent_root
            .set_root_process(new_pid.clone())
            .await;
        controller_namespace.replace_mount(
            PID_MOUNT,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                "pid",
                format!("{new_pid}\n").into_bytes(),
            ))),
            Access::ReadOnly,
        );
        controller_shell
            .write(
                "/agent/root/machine/tape",
                b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"rebind me\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"one answer\"}\n",
            )
            .await
            .unwrap();
        controller_shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();
        input_tail.close().await.unwrap();
    });

    let mut root_agent_pid = Some(old_pid.parse::<u64>().unwrap());
    let answer = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        wait_for_stdio_answer(
            &shell,
            "/agent/root",
            "rebind me",
            &mut tape_tail,
            &mut ui_tail,
            &mut root_agent_pid,
        ),
    )
    .await
    .expect("one-shot PID rebind timed out")
    .unwrap();
    controller.await.unwrap();

    assert_eq!(answer, "one answer");
    assert_eq!(
        root_agent_pid,
        Some(current_root_agent_pid(&shell).await.unwrap().unwrap())
    );
    tape_tail.close().await.unwrap();
    ui_tail.close().await.unwrap();
}
