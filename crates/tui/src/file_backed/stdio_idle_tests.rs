use super::*;

fn publish_root_agent_pid(namespace: &alan_kernel::LiveNamespace, pid: impl std::fmt::Display) {
    namespace.replace_mount(
        stdio_tests::PID_MOUNT,
        InProcessTransport::new(std::sync::Arc::new(
            alan_ap::reference::MemFs::with_read_only_file("pid", format!("{pid}\n").into_bytes()),
        )),
        alan_kernel::Access::ReadOnly,
    );
}

#[tokio::test]
async fn one_shot_start_waits_for_root_agent_pid_during_restart() {
    let (shell, _agent_root, namespace, pid) = stdio_tests::live_root_agent().await;
    publish_root_agent_pid(&namespace, 0);

    let attach_shell = shell.clone();
    let mut attaching = tokio::spawn(async move {
        open_stdio_tail_attachment_when_idle(&attach_shell, "/agent/root").await
    });
    tokio::select! {
        _ = &mut attaching => panic!("one-shot startup returned before a replacement PID was published"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
    }

    publish_root_agent_pid(&namespace, &pid);
    let attachment = tokio::time::timeout(std::time::Duration::from_secs(2), &mut attaching)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(attachment.root_agent_pid, pid.parse::<u64>().unwrap());
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_start_retries_a_stale_published_root_agent_pid() {
    let (shell, agent_root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    assert!(agent_root.unbind_process(&old_pid).await);

    let attach_shell = shell.clone();
    let mut attaching = tokio::spawn(async move {
        open_stdio_tail_attachment_when_idle(&attach_shell, "/agent/root").await
    });
    tokio::select! {
        _ = &mut attaching => panic!("one-shot startup failed on the detached but published PID"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
    }

    publish_root_agent_pid(&namespace, 0);
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(
            new_pid.clone(),
            std::sync::Arc::new(alan_agentfs::AgentFs::new()),
        )
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    publish_root_agent_pid(&namespace, &new_pid);
    let attachment = tokio::time::timeout(std::time::Duration::from_secs(2), &mut attaching)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(attachment.root_agent_pid, new_pid.parse::<u64>().unwrap());
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_tail_open_retries_a_stale_published_root_agent_pid() {
    let (shell, agent_root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    assert!(agent_root.unbind_process(&old_pid).await);

    let attach_shell = shell.clone();
    let mut attaching = tokio::spawn(async move {
        tail::open_stdio_tail_attachment(&attach_shell, "/agent/root").await
    });
    tokio::select! {
        _ = &mut attaching => panic!("one-shot tail open failed on the detached but published PID"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {}
    }

    publish_root_agent_pid(&namespace, 0);
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;

    let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(
            new_pid.clone(),
            std::sync::Arc::new(alan_agentfs::AgentFs::new()),
        )
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    publish_root_agent_pid(&namespace, &new_pid);
    let attachment = tokio::time::timeout(std::time::Duration::from_secs(2), &mut attaching)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(attachment.root_agent_pid, new_pid.parse::<u64>().unwrap());
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_start_retries_the_complete_attach_when_root_agent_changes_between_phases() {
    let (shell, agent_root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    let pid_fs = std::sync::Arc::new(stdio_tests::FaultingFileServer::new(std::sync::Arc::new(
        alan_ap::reference::MemFs::with_read_only_file("pid", format!("{old_pid}\n").into_bytes()),
    )));
    let (read_reached, resume_read) = pid_fs.pause_read_after_matching_reads("pid", 3);
    namespace.replace_mount(
        stdio_tests::PID_MOUNT,
        InProcessTransport::new(pid_fs),
        alan_kernel::Access::ReadOnly,
    );

    let attach_shell = shell.clone();
    let mut attaching = tokio::spawn(async move {
        open_stdio_tail_attachment_when_idle(&attach_shell, "/agent/root").await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), read_reached)
        .await
        .unwrap()
        .unwrap();

    assert!(agent_root.unbind_process(&old_pid).await);
    publish_root_agent_pid(&namespace, 0);
    let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(
            new_pid.clone(),
            std::sync::Arc::new(alan_agentfs::AgentFs::new()),
        )
        .await;
    agent_root.set_root_process(new_pid.clone()).await;
    publish_root_agent_pid(&namespace, &new_pid);
    resume_read.send(()).unwrap();

    let attachment = tokio::time::timeout(std::time::Duration::from_secs(2), &mut attaching)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(attachment.root_agent_pid, new_pid.parse::<u64>().unwrap());
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
}

#[tokio::test]
async fn one_shot_rebases_tails_after_the_previous_turn_reaches_idle() {
    let (shell, _agent_root, _live_namespace, pid) = stdio_tests::live_root_agent().await;
    let previous = tail::open_stdio_tail_attachment(&shell, "/agent/root")
        .await
        .unwrap();
    let agent_path = format!("/agent/{pid}");
    shell
        .write(
            &format!("{agent_path}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"repeat me\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"previous answer\"}\n",
        )
        .await
        .unwrap();
    shell
        .write(
            &format!("{agent_path}/machine/ui/events"),
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":1}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();

    let mut attachment = open_stdio_tail_attachment_when_idle(&shell, "/agent/root")
        .await
        .unwrap();
    close_stdio_tails(previous.tape_tail, previous.ui_tail)
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&attachment.tape_history).contains("previous answer"));
    assert!(String::from_utf8_lossy(&attachment.ui_history).contains("\"state\":\"idle\""));
    assert_eq!(
        attachment.tape_tail.offset(),
        attachment.tape_history.len() as u64
    );
    assert_eq!(
        attachment.ui_tail.offset(),
        attachment.ui_history.len() as u64
    );

    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let answer = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            StdioTaskWaitContext::new("repeat me", attachment.tape_history.clone()),
            &mut attachment,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot returned before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }

        shell
            .write(
                &format!("{agent_path}/machine/tape"),
                b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"repeat me\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"preamble\"}\n",
            )
            .await
            .unwrap();
        shell
            .write(
                &format!("{agent_path}/machine/ui/events"),
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_millis(10), &mut wait_for_answer)
            .await
            .expect_err(
                "a delayed prior Idle event must not complete the new task with its preamble",
            );

        shell
            .write(
                &format!("{agent_path}/machine/tape"),
                b"{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"new answer\"}\n",
            )
            .await
            .unwrap();
        shell
            .write(
                &format!("{agent_path}/machine/ui/events"),
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":18446744073709551615}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();
        wait_for_answer.await.unwrap()
    };

    assert_eq!(answer, "new answer");
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
    input_tail.close().await.unwrap();
}

#[tokio::test]
async fn one_shot_recovers_when_final_tape_read_races_root_agent_restart() {
    let (shell, agent_root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    let tail_closer = std::sync::Arc::new(stdio_tests::FaultingFileServer::new(agent_root.clone()));
    namespace.replace_mount(
        "/agent",
        alan_ap::InProcessTransport::new(tail_closer.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let mut attachment = open_stdio_tail_attachment_when_idle(&shell, "/agent/root")
        .await
        .unwrap();
    let mut input_tail = shell.tail("/agent/root/io/input").await.unwrap();
    let (answer, new_pid) = {
        let wait_for_answer = wait_for_stdio_answer(
            &shell,
            "/agent/root",
            StdioTaskWaitContext::new("restart after idle", attachment.tape_history.clone()),
            &mut attachment,
            std::future::pending::<anyhow::Result<()>>(),
        );
        tokio::pin!(wait_for_answer);
        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot completed before input was observed: {result:?}"),
            input = input_tail.read(4096) => assert!(!input.unwrap().is_empty()),
        }

        shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"restart after idle\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"old answer\"}\n",
        )
        .await
        .unwrap();
        shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":18446744073709551615}}\n",
        )
        .await
        .unwrap();
        let (walked, resume) = tail_closer.pause_next_walk_with_suffix("/machine/tape");
        shell
            .write(
                "/agent/root/machine/ui/events",
                b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
            )
            .await
            .unwrap();

        tokio::select! {
            result = &mut wait_for_answer => panic!("one-shot failed before the final tape read completed: {result:?}"),
            result = tokio::time::timeout(std::time::Duration::from_secs(2), walked) => {
                result.unwrap().unwrap();
            }
        }
        tail_closer.close(old_pid.parse().unwrap());
        assert!(agent_root.unbind_process(&old_pid).await);
        let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
        agent_root
            .bind_process(
                new_pid.clone(),
                std::sync::Arc::new(alan_agentfs::AgentFs::new()),
            )
            .await;
        agent_root.set_root_process(new_pid.clone()).await;
        namespace.replace_mount(
            stdio_tests::PID_MOUNT,
            alan_ap::InProcessTransport::new(std::sync::Arc::new(
                alan_ap::reference::MemFs::with_read_only_file(
                    "pid",
                    format!("{new_pid}\n").into_bytes(),
                ),
            )),
            alan_kernel::Access::ReadOnly,
        );
        shell
        .write(
            "/agent/root/machine/tape",
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"restart after idle\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"recovered answer\"}\n",
        )
        .await
        .unwrap();
        shell
        .write(
            "/agent/root/machine/ui/events",
            b"{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"running\",\"started_at_ms\":18446744073709551615}}\n{\"type\":\"activity\",\"snapshot\":{\"version\":1,\"state\":\"idle\"}}\n",
        )
        .await
        .unwrap();
        resume.send(()).unwrap();

        (wait_for_answer.await.unwrap(), new_pid)
    };
    assert_eq!(answer, "recovered answer");
    assert_eq!(attachment.root_agent_pid, new_pid.parse::<u64>().unwrap());
    close_stdio_tails(attachment.tape_tail, attachment.ui_tail)
        .await
        .unwrap();
    input_tail.close().await.unwrap();
}
