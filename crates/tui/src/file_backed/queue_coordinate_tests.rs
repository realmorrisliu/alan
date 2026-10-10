use super::*;
use alan_agent_protocol::InputIntent;

#[tokio::test]
async fn hydrated_preview_boundary_survives_public_reattach() {
    for thinking in [false, true] {
        for mode in 0..3 {
            let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
            let owner = format!("/agent/{pid}");
            let mut app = FileBackedApp::new("/agent/root".into());
            app.transcript = vec![
                HistoryCell::User("old".into()),
                HistoryCell::Assistant("old answer".into()),
            ];
            let tape = if mode == 1 {
                "{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"A\",\"submission_id\":\"a\"}\n"
            } else {
                ""
            };
            shell
                .write(&format!("{owner}/machine/tape"), tape.as_bytes())
                .await
                .unwrap();
            let event = if thinking {
                serde_json::json!({"type":"thinking","snapshot":{"version":1,"state":"complete","text":"new preview","duration_secs":1}})
            } else {
                serde_json::json!({"type":"plan","snapshot":{"version":1,"items":[{"id":"p","content":"new preview","status":"in_progress"}]}})
            };
            // Serialize the actual protocol event, not a guessed event envelope.
            let event = if thinking {
                UiEvent::Thinking {
                    snapshot: serde_json::from_value(event["snapshot"].clone()).unwrap(),
                }
            } else {
                UiEvent::Plan {
                    snapshot: serde_json::from_value(event["snapshot"].clone()).unwrap(),
                }
            };
            shell
                .write(
                    &format!("{owner}/machine/ui/events"),
                    format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
                )
                .await
                .unwrap();
            let task = PendingRootAgentTurn {
                input: "A".into(),
                submission_id: "a".into(),
                submitted_process: Some(pid.parse().unwrap()),
                submitted_at_ms: 0,
            };
            for _ in 0..3 {
                let tasks = if mode == 0 {
                    &[][..]
                } else {
                    std::slice::from_ref(&task)
                };
                let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, tasks)
                    .await
                    .unwrap();
                tails.close().await;
                assert_eq!(app.transcript[0], HistoryCell::User("old".into()));
                assert_eq!(
                    app.transcript[1],
                    HistoryCell::Assistant("old answer".into())
                );
                if mode == 0 {
                    assert_eq!(
                        app.pending_remote_turn_start,
                        Some(2),
                        "hydrated position must follow retained prefix"
                    );
                } else {
                    assert_eq!(
                        app.pending_remote_turn_start, None,
                        "bounded/discarded preview must not carry a position"
                    );
                }
            }
            let late = serde_json::json!({"version":1,"kind":"message","role":"user","content":"new","submission_id":"new"});
            app.apply_tape_record(serde_json::from_value(late).unwrap());
            assert_eq!(app.transcript[0], HistoryCell::User("old".into()));
            assert_eq!(
                app.transcript[1],
                HistoryCell::Assistant("old answer".into())
            );
            if mode == 0 {
                assert_eq!(app.transcript[2].input_source().unwrap().0, "new");
                assert!(matches!(
                    app.transcript[3],
                    HistoryCell::Plan { .. } | HistoryCell::Thinking { .. }
                ));
                assert_eq!(app.transcript.len(), 4);
            }
        }
    }
}

#[tokio::test]
async fn retained_restore_helpers_preserve_positions() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(&owner, None);
    app.track_local_input("q", owner.clone(), "Q".into(), InputIntent::Agent);
    app.acknowledge_local_input("q", &owner);
    app.action_cells.insert("action".into(), 0);
    app.pending_remote_turn_start = Some(1);
    for suffix in [
        "",
        "{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"answer\"}\n",
    ] {
        shell
            .write(&format!("{owner}/machine/tape"), suffix.as_bytes())
            .await
            .unwrap();
        let tail = shell.tail(&format!("{owner}/machine/tape")).await.unwrap();
        for _ in 0..2 {
            assert!(previous_input::restore_tape_history(&mut app, &tail, 0).await);
            assert_eq!(app.pending_remote_turn_start, Some(1));
            assert_eq!(app.local_inputs["q"].cell, Some(0));
            assert_eq!(app.action_cells["action"], 0);
        }
        tail.close().await.unwrap();
    }
    for _ in 0..2 {
        previous_input::restore_answer(&mut app, "Q", "answer".into());
        assert_eq!(app.pending_remote_turn_start, Some(1));
        assert_eq!(app.local_inputs["q"].cell, Some(0));
        assert_eq!(app.action_cells["action"], 0);
    }
}
