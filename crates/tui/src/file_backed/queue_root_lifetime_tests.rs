use super::*;
use alan_agent_protocol::{InputIntent, UiQueueSnapshot};
use alan_agentfs::AgentFs;
use std::sync::Arc;

#[tokio::test]
async fn lifetime_partial_receipt_survives_public_root_watch_refresh() {
    for copies in [1, 2] {
        public_root_watch_refresh(copies).await;
    }
}

async fn public_root_watch_refresh(copies: usize) {
    let (shell, root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    let old_owner = format!("/agent/{old_pid}");
    let body = "old Q first line\n  retained second line\nretained third line";
    let old_queue = UiQueueSnapshot {
        known: true,
        revision: 9,
        paused: true,
        pending_submission_ids: vec!["q".into()],
        ..Default::default()
    };
    shell
        .write(
            &format!("{old_owner}/machine/ui/queue"),
            &serde_json::to_vec(&old_queue).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    app.track_local_input("q", old_owner.clone(), body.into(), InputIntent::Agent);
    let mut pending = VecDeque::from([PendingRootAgentTurn {
        input: body.into(),
        submission_id: "q".into(),
        submitted_process: Some(old_pid.parse().unwrap()),
        submitted_at_ms: 0,
    }]);
    queue::confirm_local_receipts(&mut app, &pending);
    assert!(app.local_inputs["q"].acknowledged);
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(80, false), 1), 1);
    let cut = app.local_inputs["q"].source_cut;
    assert_ne!(cut, (0, 0));
    let anchor = app.local_inputs["q"].cell.unwrap();
    let source = app.transcript[anchor].clone();
    let suffix = source.render_styled_lines(RenderOpts::new(80, false));
    assert!(!suffix.is_empty());
    assert!(
        app.rendered_history_lines(80)
            .iter()
            .any(|line| line.contains("retained third line"))
    );
    assert!(!app.local_inputs["q"].terminal);
    assert!(!app.local_inputs["q"].committed);
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
    let mut exercise = Box::pin(async {
        assert_eq!(watchers.root_agent_pid, Some(old_pid.parse().unwrap()));

        // Exact-ID evidence belongs to the detached owner. Recovery tails observe
        // it during refresh; it must not resurrect the already committed prefix.
        let q_record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"q"});
        shell
            .write(
                &format!("{old_owner}/machine/tape"),
                format!("{q_record}\n").repeat(copies).as_bytes(),
            )
            .await
            .unwrap();
        let new_pid = shell.spawn(stdio_tests::EXEC_SPEC).await.unwrap();
        root.bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
            .await;
        root.set_root_process(new_pid.clone()).await;
        namespace.replace_mount(
            stdio_tests::PID_MOUNT,
            alan_ap::InProcessTransport::new(Arc::new(
                alan_ap::reference::MemFs::with_read_only_file(
                    "pid",
                    format!("{new_pid}\n").into_bytes(),
                ),
            )),
            alan_kernel::Access::ReadOnly,
        );
        let new_owner = format!("/agent/{new_pid}");
        assert_eq!(pending.len(), 1);
        assert!(
            watchers
                .refresh_root_agent_attachment(
                    &shell,
                    "/agent/root",
                    &mut app,
                    &mut rx,
                    &mut pending,
                    &tx
                )
                .await
        );
        // Same follow-up as the renderer's RootAgentPidRefresh branch.
        settle_unknown_replaced_input(&mut pending, watchers.root_agent_pid, &mut app);
        assert!(pending.is_empty());
        assert_eq!(watchers.root_agent_pid, Some(new_pid.parse().unwrap()));
        assert!(!watchers.pid_refresh_failed);
        assert_eq!(app.queue.owner, new_owner);
        assert!(
            app.rendered_history_lines(200)
                .iter()
                .any(|line| line.contains("outcome is unknown"))
        );
        assert_eq!(app.local_inputs["q"].owner, old_owner);
        assert_eq!(app.local_inputs["q"].body, body);
        assert!(!app.local_inputs["q"].terminal);
        assert!(!app.local_inputs["q"].committed);
        assert_eq!(app.local_inputs["q"].source_cut, cut);
        assert_eq!(app.local_inputs["q"].cell, Some(anchor));
        assert_eq!(app.transcript[anchor], source);
        assert_eq!(
            app.transcript[anchor].render_styled_lines(RenderOpts::new(80, false)),
            suffix
        );
        assert!(
            shell
                .cat(&format!("{new_owner}/machine/tape"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            queue::read_queue(&shell, &new_owner)
                .await
                .is_none_or(|q| !q.pending_submission_ids.contains(&"q".into())
                    && !q.active_submission_ids.contains(&"q".into()))
        );

        for _ in 0..2 {
            assert!(
                !watchers
                    .refresh_root_agent_attachment(
                        &shell,
                        "/agent/root",
                        &mut app,
                        &mut rx,
                        &mut pending,
                        &tx
                    )
                    .await
            );
            let (tails, settled) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
                .await
                .unwrap();
            assert_eq!(tails.root_agent_pid, Some(new_pid.parse().unwrap()));
            tails.close().await;
            assert!(settled.is_empty());
            assert_eq!(app.local_inputs["q"].cell, Some(anchor));
            assert_eq!(app.local_inputs["q"].source_cut, cut);
            assert_eq!(app.local_inputs["q"].body, body);
            assert!(!app.local_inputs["q"].terminal);
            assert_eq!(app.transcript[anchor], source);
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|cell| cell.input_source().is_some_and(|(text, _, _)| text == body))
                    .count(),
                1
            );
            assert!(
                shell
                    .cat(&format!("{new_owner}/machine/tape"))
                    .await
                    .unwrap()
                    .is_empty()
            );
        }

        // A fresh distinct ID still follows real queue publication/watch dispatch
        // and exact-ID Tape observation. This fixture projects admission; it is not
        // a backend execution or native Supervisor test.
        app.track_local_input(
            "fresh",
            new_owner.clone(),
            "fresh task".into(),
            InputIntent::Agent,
        );
        pending.push_back(PendingRootAgentTurn {
            input: "fresh task".into(),
            submission_id: "fresh".into(),
            submitted_process: Some(new_pid.parse().unwrap()),
            submitted_at_ms: 0,
        });
        let fresh_queue = UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec!["fresh".into()],
            ..Default::default()
        };
        shell
            .write(
                &format!("{new_owner}/machine/ui/queue"),
                &serde_json::to_vec(&fresh_queue).unwrap(),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !app.local_inputs["fresh"].acknowledged {
                let event = rx.recv().await.unwrap();
                match event {
                    event @ (FileBackedEvent::QueueChanged { .. }
                    | FileBackedEvent::QueueUnavailable { .. }) => {
                        queue::dispatch_queue_event(&shell, &mut app, &pending, event).await
                    }
                    event => {
                        app.dispatch(event);
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(app.local_inputs["fresh"].owner, new_owner);
        assert!(
            app.queue
                .snapshot
                .as_ref()
                .unwrap()
                .active_submission_ids
                .contains(&"fresh".into())
        );
        assert!(
            !app.queue
                .snapshot
                .as_ref()
                .unwrap()
                .active_submission_ids
                .contains(&"q".into())
        );
        let fresh_record = serde_json::json!({"version":1,"kind":"message","role":"user","content":"fresh task","submission_id":"fresh"});
        shell
            .write(
                &format!("{new_owner}/machine/tape"),
                format!("{fresh_record}\n{fresh_record}\n").as_bytes(),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !app.local_inputs["fresh"].tape_seen {
                let event = rx.recv().await.unwrap();
                app.dispatch(event);
            }
        })
        .await
        .unwrap();
        for _ in 0..2 {
            let (tails, settled) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
                .await
                .unwrap();
            tails.close().await;
            assert!(settled.is_empty());
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|cell| cell
                        .input_source()
                        .is_some_and(|(text, _, _)| text == "fresh task"))
                    .count(),
                1
            );
            assert_eq!(app.local_inputs["q"].owner, old_owner);
            assert_eq!(app.local_inputs["q"].body, body);
            assert_eq!(app.local_inputs["q"].source_cut, cut);
            assert_eq!(app.local_inputs["q"].cell, Some(anchor));
            assert!(!app.local_inputs["q"].terminal);
            assert_eq!(app.transcript[anchor], source);
        }
        let tape = String::from_utf8(
            shell
                .cat(&format!("{new_owner}/machine/tape"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(!tape.contains("old Q"));
        assert!(!tape.contains("\"submission_id\":\"q\""));
    });
    // Catch each poll's assertion outcome, then use the ordinary teardown
    // before surfacing it. No extra watcher lifetime manager is needed.
    let outcome = std::future::poll_fn(|cx| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::future::Future::poll(exercise.as_mut(), cx)
        })) {
            Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
            Ok(std::task::Poll::Ready(())) => std::task::Poll::Ready(Ok(())),
            Err(panic) => std::task::Poll::Ready(Err(panic)),
        }
    })
    .await;
    drop(exercise);
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn submitted_equal_body_receipts_share_projected_coordinates() {
    for copies in [1, 2] {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let body = "same first line\n  retained second line\nretained third line";
        let mut app = FileBackedApp::new("/agent/root".into());
        let queue = UiQueueSnapshot {
            known: true,
            revision: 1,
            pending_submission_ids: vec!["fresh".into()],
            ..Default::default()
        };
        app.queue.apply(&owner, Some(queue.clone()));
        let mut tape = String::new();
        for (id, answer, count) in [("q", "answer Q", copies), ("fresh", "answer fresh", 1)] {
            app.track_local_input(id, owner.clone(), body.into(), InputIntent::Agent);
            app.acknowledge_local_input(id, &owner);
            let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":id});
            for _ in 0..count {
                tape.push_str(&format!("{record}\n"));
                app.apply_tape_record(serde_json::from_value(record.clone()).unwrap());
            }
            let index = app.local_inputs[id].cell.unwrap();
            app.transcript[index].trim_rendered_prefix(RenderOpts::new(80, false), 1);
            let cut = app.transcript[index].input_source().unwrap().2;
            assert_ne!(cut, (0, 0));
            app.local_inputs.get_mut(id).unwrap().source_cut = cut;
            let record = serde_json::json!({"version":1,"kind":"message","role":"assistant","content":answer,"submission_id":id});
            tape.push_str(&format!("{record}\n"));
            app.apply_tape_record(serde_json::from_value(record).unwrap());
        }
        let expected = app.transcript.clone();
        assert_eq!(expected.len(), 4);
        shell
            .write(&format!("{owner}/machine/tape"), tape.as_bytes())
            .await
            .unwrap();
        shell
            .write(
                &format!("{owner}/machine/ui/queue"),
                &serde_json::to_vec(&queue).unwrap(),
            )
            .await
            .unwrap();
        let tasks = [PendingRootAgentTurn {
            input: body.into(),
            submission_id: "fresh".into(),
            submitted_process: Some(pid.parse().unwrap()),
            submitted_at_ms: 0,
        }];
        for _ in 0..3 {
            let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &tasks)
                .await
                .unwrap();
            let raw = tails.tape_history.clone();
            tails.close().await;
            assert_eq!(raw, tape.as_bytes());
            assert_eq!(
                &app.transcript[..app.transcript.len().min(4)],
                &expected,
                "copies={copies}: source/answer chronology lost"
            );
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|cell| cell.input_source().is_some())
                    .count(),
                2
            );
            for (id, index) in [("q", 0), ("fresh", 2)] {
                let receipt = &app.local_inputs[id];
                assert_eq!(receipt.owner, owner);
                assert_eq!(receipt.cell, Some(index));
                assert_eq!(receipt.body, body);
                assert_eq!(
                    receipt.source_cut,
                    expected[index].input_source().unwrap().2
                );
                assert!(receipt.tape_seen);
                assert!(!receipt.terminal);
                assert!(!receipt.committed);
                assert_eq!(app.transcript[index], expected[index]);
            }
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|cell| cell.assistant_source() == Some("answer Q"))
                    .count(),
                1
            );
            assert_eq!(
                app.transcript
                    .iter()
                    .filter(|cell| cell.assistant_source() == Some("answer fresh"))
                    .count(),
                1
            );
            let before = app.transcript.clone();
            for id in ["q", "fresh"] {
                let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":id});
                app.apply_tape_record(serde_json::from_value(record).unwrap());
            }
            assert_eq!(app.transcript, before, "late exact-ID Tape replayed source");
            assert_eq!(app.tape_consumed_offset, tape.len());
            assert_eq!(
                shell.cat(&format!("{owner}/machine/tape")).await.unwrap(),
                tape.as_bytes()
            );
            assert_eq!(queue::read_queue(&shell, &owner).await, Some(queue.clone()));
        }
    }
}
