use super::*;
use alan_agent_protocol::{InputIntent, UiInputStatus, UiQueueSnapshot};

#[test]
fn initial_submission_notice_follows_admission_and_direct_terminal_receipts() {
    for paused in [false, true] {
        for queue_receipt in [false, true] {
            for status in [
                UiInputStatus::Completed,
                UiInputStatus::Failed,
                UiInputStatus::Cancelled,
            ] {
                let mut app = FileBackedApp::new("/agent/8".into());
                app.queue.apply("/agent/8", None);
                app.composer.set_text("body");
                app.track_local_input("q", "/agent/8".into(), "body".into(), InputIntent::Agent);
                app.show_submission_sent("q");
                let mut pending = VecDeque::from([PendingRootAgentTurn {
                    input: "body".into(),
                    submission_id: "q".into(),
                    submitted_process: Some(8),
                    submitted_at_ms: 0,
                }]);
                if queue_receipt {
                    app.queue.apply(
                        "/agent/8",
                        Some(UiQueueSnapshot {
                            known: true,
                            revision: 1,
                            paused,
                            pending_submission_ids: vec!["q".into()],
                            ..Default::default()
                        }),
                    );
                    queue::confirm_local_receipts(&mut app, &pending);
                    assert!(app.local_inputs["q"].acknowledged);
                    assert!(app.notice.as_deref().unwrap().contains(if paused {
                        "paused"
                    } else {
                        "queued"
                    }));
                    assert!(
                        !app.notice
                            .as_deref()
                            .unwrap()
                            .contains("admission unconfirmed")
                    );
                }
                app.composer.set_text("new draft 中文😀");
                interrupt::observe_root_agent_completion(
                    &mut pending,
                    &UiEvent::InputCompleted {
                        submission_ids: vec!["q".into()],
                        status,
                        error: Some("terminal diagnostic".into()),
                    },
                    &mut app,
                );
                assert!(pending.is_empty());
                assert!(
                    app.notice.is_none(),
                    "paused={paused}, queue_receipt={queue_receipt}, status={status:?}"
                );
                assert_eq!(app.composer.text(), "new draft 中文😀");
            }
        }
    }
}

#[test]
fn older_terminal_receipt_preserves_newer_initial_submission_notice() {
    let mut app = FileBackedApp::new("/agent/8".into());
    app.queue.apply("/agent/8", None);
    let mut pending = VecDeque::new();
    for id in ["old", "new"] {
        app.track_local_input(id, "/agent/8".into(), id.into(), InputIntent::Agent);
        app.show_submission_sent(id);
        pending.push_back(PendingRootAgentTurn {
            input: id.into(),
            submission_id: id.into(),
            submitted_process: Some(8),
            submitted_at_ms: 0,
        });
    }
    interrupt::observe_root_agent_completion(
        &mut pending,
        &UiEvent::InputCompleted {
            submission_ids: vec!["old".into()],
            status: UiInputStatus::Completed,
            error: None,
        },
        &mut app,
    );
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].submission_id, "new");
    assert_eq!(
        app.notice.as_ref().unwrap().submission.as_deref(),
        Some("new")
    );
    assert!(!app.local_inputs["new"].terminal);
}

#[tokio::test]
async fn lifetime_queue_events_clear_stale_paused_notice() {
    for read_failure in [false, true] {
        for terminal in [false, true] {
            let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
            let owner = format!("/agent/{pid}");
            let mut app = FileBackedApp::new("/agent/root".into());
            app.queue.apply(
                &owner,
                Some(UiQueueSnapshot {
                    known: true,
                    revision: 1,
                    paused: true,
                    pending_submission_ids: vec!["q".into()],
                    ..Default::default()
                }),
            );
            app.track_local_input("q", owner.clone(), "body".into(), InputIntent::Agent);
            app.acknowledge_local_input("q", &owner);
            assert!(app.notice.as_ref().unwrap().contains("/continue"));
            if terminal {
                let turn = PendingRootAgentTurn {
                    input: "body".into(),
                    submission_id: "q".into(),
                    submitted_process: Some(pid.parse().unwrap()),
                    submitted_at_ms: 0,
                };
                interrupt::observe_root_agent_completion(
                    &mut VecDeque::from([turn]),
                    &UiEvent::InputCompleted {
                        submission_ids: vec!["q".into()],
                        status: UiInputStatus::Completed,
                        error: None,
                    },
                    &mut app,
                );
            }
            if read_failure {
                shell
                    .write(&format!("{owner}/machine/ui/queue"), b"invalid")
                    .await
                    .unwrap();
            }
            let event = if read_failure {
                FileBackedEvent::QueueChanged {
                    owner: owner.clone(),
                }
            } else {
                FileBackedEvent::QueueUnavailable {
                    owner: owner.clone(),
                }
            };
            queue::dispatch_queue_event(&shell, &mut app, &VecDeque::new(), event).await;
            assert!(!app.notice.as_deref().unwrap_or("").contains("/continue"));
            assert_eq!(app.notice.is_none(), terminal);
            assert!(app.context_line(80).to_string().contains("unknown"));
            assert!(app.queue.snapshot.is_none());
            app.queue.apply("/agent/99999", None);
            app.notice = Some("new Root notice".into());
            queue::dispatch_queue_event(
                &shell,
                &mut app,
                &VecDeque::new(),
                FileBackedEvent::QueueUnavailable { owner },
            )
            .await;
            assert_eq!(app.notice.as_deref(), Some("new Root notice"));
        }
    }
}
