use super::*;
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::ProcessEventSource;
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, ProcFs};
use std::sync::Arc;

const PID_MOUNT: &str = "/mnt/service-manager/units/root-agent";
const EXEC_SPEC: &str =
    r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#;

async fn live_root_agent() -> (alan_shell::Shell, Arc<AgentRootFs>, LiveNamespace, String) {
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

    let pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;
    live_namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );

    (shell, agent_root, live_namespace, pid)
}

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
        br#"{"type":"activity","snapshot":{"version":1,"state":"running"}}
{"type":"error","message":"previous provider failure","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
{"type":"activity","snapshot":{"version":1,"state":"running"}}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(snapshot.task_started);
    assert_eq!(snapshot.assistant_answer.as_deref(), Some("current answer"));
    assert_eq!(snapshot.activity_state, Some(UiActivityState::Idle));
    assert!(snapshot.task_error.is_none());
}

#[test]
fn one_shot_reports_failure_before_the_user_record_reaches_tape() {
    let mut snapshot = stdio_task_snapshot_from_history(
        "task with no tape record",
        b"",
        br#"{"type":"activity","snapshot":{"version":1,"state":"running"}}
{"type":"error","message":"provider unavailable","recoverable":true}
{"type":"activity","snapshot":{"version":1,"state":"idle"}}
"#,
    )
    .unwrap();

    assert!(
        snapshot.task_started,
        "Running establishes this submitted task"
    );
    assert_eq!(
        finish_stdio_task_if_ready(&mut snapshot)
            .unwrap_err()
            .to_string(),
        "Agent task failed: provider unavailable"
    );
}

#[test]
fn one_shot_result_waits_for_seen_task_and_idle_activity() {
    let mut task = StdioTaskSnapshot {
        task_started: false,
        assistant_answer: Some("answer".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: None,
    };

    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.task_started = true;
    task.activity_state = Some(UiActivityState::Running);
    assert_eq!(finish_stdio_task_if_ready(&mut task).unwrap(), None);
    task.activity_state = Some(UiActivityState::Idle);
    assert_eq!(
        finish_stdio_task_if_ready(&mut task).unwrap().as_deref(),
        Some("answer")
    );

    let mut failed_task = StdioTaskSnapshot {
        task_started: true,
        assistant_answer: Some("intermediate response".to_string()),
        activity_state: Some(UiActivityState::Idle),
        task_error: Some("provider failed".to_string()),
    };
    let result = finish_stdio_task_if_ready(&mut failed_task);
    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider failed"
    );
}

#[tokio::test(start_paused = true)]
async fn one_shot_keeps_waiting_past_five_minutes_and_recovers_after_root_agent_pid_changes() {
    let (shell, agent_root, live_namespace, old_pid) = live_root_agent().await;

    let (mut tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (mut ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let (input_seen_tx, input_seen_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let controller_shell = shell.clone();
    let controller_agent_root = agent_root.clone();
    let controller_namespace = live_namespace.clone();
    let controller = tokio::spawn(async move {
        assert!(!input_tail.read(4096).await.unwrap().is_empty());
        input_seen_tx.send(()).unwrap();
        release_rx.await.unwrap();
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
    let answer = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            "rebind me",
            &mut tape_tail,
            &mut ui_tail,
            &mut root_agent_pid,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot completed before its AgentFS result: {result:?}"),
            result = input_seen_rx => result.unwrap(),
        }

        tokio::time::advance(std::time::Duration::from_secs(301)).await;
        let still_waiting =
            tokio::time::timeout(std::time::Duration::from_millis(1), &mut wait_for_answer).await;
        assert!(
            still_waiting.is_err(),
            "a valid one-shot task must not time out after five minutes"
        );

        release_tx.send(()).unwrap();
        wait_for_answer.await.unwrap()
    };
    controller.await.unwrap();

    assert_eq!(answer, "one answer");
    assert_eq!(
        root_agent_pid,
        Some(current_root_agent_pid(&shell).await.unwrap().unwrap())
    );
    tape_tail.close().await.unwrap();
    ui_tail.close().await.unwrap();
}

#[tokio::test]
async fn one_shot_returns_runtime_failure_without_a_tape_user_record() {
    let (shell, _agent_root, _live_namespace, pid) = live_root_agent().await;
    let (mut tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (mut ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut root_agent_pid = Some(pid.parse::<u64>().unwrap());

    let result = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            "fail before tape persistence",
            &mut tape_tail,
            &mut ui_tail,
            &mut root_agent_pid,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }

        shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\"}}\n{\"type\":\"error\",\"message\":\"provider unavailable\",\"recoverable\":true}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();

        wait_for_answer.await
    };

    assert_eq!(
        result.unwrap_err().to_string(),
        "Agent task failed: provider unavailable"
    );
    let tape = shell.cat("/agent/root/machine/tape").await.unwrap();
    assert!(
        tape.is_empty(),
        "this failure path must not depend on tape data"
    );
    let process_status =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap();
    assert_eq!(process_status.trim(), "running");

    tape_tail.close().await.unwrap();
    ui_tail.close().await.unwrap();
    input_tail.close().await.unwrap();
}

#[tokio::test]
async fn one_shot_cancellation_interrupts_before_running_is_observed() {
    let (shell, _agent_root, _live_namespace, pid) = live_root_agent().await;
    let (mut tape_tail, _) = tail_with_history(&shell, "/agent/root/machine/tape")
        .await
        .unwrap();
    let (mut ui_tail, _) = tail_with_history(&shell, "/agent/root/machine/ui/events")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let mut root_agent_pid = Some(pid.parse::<u64>().unwrap());
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

    let result = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            "cancel this turn",
            &mut tape_tail,
            &mut ui_tail,
            &mut root_agent_pid,
            async move { cancel_rx.await.map_err(anyhow::Error::from) },
        );
        tokio::pin!(wait_for_answer);

        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }
        cancel_tx.send(()).unwrap();

        tokio::time::timeout(std::time::Duration::from_millis(10), &mut wait_for_answer)
            .await
            .expect_err("cancellation waits until the task is accepted");
        let events = shell.cat("/agent/root/events").await.unwrap();
        assert!(
            !String::from_utf8_lossy(&events).contains("ctl:interrupt"),
            "do not let an idle Runtime consume the interrupt before input"
        );

        shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\"}}\n",
            )
            .await
            .unwrap();
        wait_for_answer.await
    };

    assert_eq!(result.unwrap_err().to_string(), "Agent task interrupted");
    let events = String::from_utf8(shell.cat("/agent/root/events").await.unwrap()).unwrap();
    assert!(
        events.contains("ctl:interrupt"),
        "one-shot cancellation must target the Agent Machine: {events:?}"
    );
    let process_status =
        String::from_utf8(shell.cat(&format!("/proc/{pid}/status")).await.unwrap()).unwrap();
    assert_eq!(process_status.trim(), "running");

    tape_tail.close().await.unwrap();
    ui_tail.close().await.unwrap();
    input_tail.close().await.unwrap();
}
