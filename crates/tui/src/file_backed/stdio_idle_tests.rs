use super::*;

#[tokio::test]
async fn one_shot_rebases_tails_after_the_previous_turn_reaches_idle() {
    let (shell, _agent_root, _live_namespace, pid) = stdio_tests::live_root_agent().await;
    let previous = open_stdio_tail_attachment(&shell, "/agent/root")
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
