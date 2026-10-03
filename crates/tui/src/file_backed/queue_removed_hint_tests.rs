use super::*;
use alan_agent_protocol::{InputIntent, UiInputStatus, UiQueueSnapshot};

#[tokio::test]
async fn terminal_receipt_refreshes_hint_in_either_queue_event_order() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    for status in [
        UiInputStatus::Completed,
        UiInputStatus::Failed,
        UiInputStatus::Cancelled,
    ] {
        for queue_first in [true, false] {
            for unrelated_notice in [false, true] {
                for preacknowledged in [true, false] {
                    let mut app = FileBackedApp::new("/agent/root".into());
                    app.queue.apply(
                        &owner,
                        Some(UiQueueSnapshot {
                            known: true,
                            revision: 1,
                            active_submission_ids: vec!["q".into()],
                            ..Default::default()
                        }),
                    );
                    app.composer.set_text("body");
                    app.track_local_input("q", owner.clone(), "body".into(), InputIntent::Agent);
                    if preacknowledged {
                        app.acknowledge_local_input("q", &owner);
                    } else {
                        app.refresh_local_input_hint("q", &owner);
                    }
                    assert!(app.notice.as_ref().unwrap().contains("active"));
                    app.composer.set_text("new draft 🧭");
                    let mut pending = VecDeque::from([PendingRootAgentTurn {
                        input: "body".into(),
                        submission_id: "q".into(),
                        submitted_process: Some(pid.parse().unwrap()),
                        submitted_at_ms: 0,
                    }]);
                    let completion = UiEvent::InputCompleted {
                        submission_ids: vec!["q".into()],
                        status,
                        error: Some("terminal diagnostic".into()),
                    };
                    shell
                        .write(
                            &format!("{owner}/machine/ui/queue"),
                            &serde_json::to_vec(&UiQueueSnapshot {
                                known: true,
                                revision: 2,
                                ..Default::default()
                            })
                            .unwrap(),
                        )
                        .await
                        .unwrap();
                    if queue_first {
                        queue::dispatch_queue_event(
                            &shell,
                            &mut app,
                            &pending,
                            FileBackedEvent::QueueChanged {
                                owner: owner.clone(),
                            },
                        )
                        .await;
                        assert!(app.notice.as_ref().unwrap().contains("outcome unknown"));
                        assert!(!app.local_inputs["q"].terminal);
                    }
                    if unrelated_notice {
                        app.notice = Some("unrelated notice".into());
                    }
                    assert_eq!(pending[0].submission_id, "q");
                    assert_eq!(app.local_inputs["q"].owner, owner);
                    assert_eq!(app.queue.owner, owner);
                    assert!(!app.local_inputs["q"].terminal);
                    assert_eq!(app.local_inputs["q"].acknowledged, preacknowledged);
                    interrupt::observe_root_agent_completion(&mut pending, &completion, &mut app);
                    assert!(pending.is_empty());
                    if status == UiInputStatus::Completed {
                        assert!(app.local_inputs["q"].acknowledged);
                        assert!(app.local_inputs["q"].terminal);
                        assert_eq!(app.local_inputs["q"].owner, owner);
                        assert_eq!(
                            app.rendered_history_lines(80)
                                .iter()
                                .filter(|line| line.contains("body"))
                                .count(),
                            1
                        );
                    }
                    let history = app.transcript.clone();
                    assert_eq!(app.composer.text(), "new draft 🧭");
                    if unrelated_notice {
                        assert_eq!(app.notice.as_deref(), Some("unrelated notice"));
                    }
                    if !queue_first {
                        queue::dispatch_queue_event(
                            &shell,
                            &mut app,
                            &pending,
                            FileBackedEvent::QueueChanged {
                                owner: owner.clone(),
                            },
                        )
                        .await;
                    }
                    assert!(pending.is_empty());
                    assert_eq!(app.composer.text(), "new draft 🧭");
                    assert_eq!(
                        app.notice.as_deref(),
                        Some(if unrelated_notice {
                            "unrelated notice"
                        } else {
                            "queued 0"
                        }),
                        "status={status:?}, queue_first={queue_first}, preacknowledged={preacknowledged}"
                    );
                    queue::dispatch_queue_event(
                        &shell,
                        &mut app,
                        &pending,
                        FileBackedEvent::QueueUnavailable {
                            owner: owner.clone(),
                        },
                    )
                    .await;
                    assert_eq!(
                        app.notice.as_deref(),
                        Some(if unrelated_notice {
                            "unrelated notice"
                        } else {
                            "queue unknown"
                        })
                    );
                    assert_eq!(app.transcript, history);
                    assert_eq!(app.composer.text(), "new draft 🧭");
                    if status != UiInputStatus::Completed {
                        assert!(
                            app.rendered_history_lines(80)
                                .iter()
                                .any(|line| { line.contains("terminal diagnostic") })
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn terminal_acknowledgement_consumes_only_its_unchanged_draft() {
    for changed_draft in [false, true] {
        let mut app = FileBackedApp::new("/agent/8".into());
        app.queue.apply("/agent/8", None);
        app.composer.set_text("body");
        app.track_local_input("q", "/agent/8".into(), "body".into(), InputIntent::Agent);
        app.refresh_local_input_hint("q", "/agent/8");
        if changed_draft {
            app.composer.set_text("new draft 🧭");
        }
        app.notice = Some("unrelated notice".into());
        let mut pending = VecDeque::from([PendingRootAgentTurn {
            input: "body".into(),
            submission_id: "q".into(),
            submitted_process: Some(8),
            submitted_at_ms: 0,
        }]);
        let completion = UiEvent::InputCompleted {
            submission_ids: vec!["q".into()],
            status: UiInputStatus::Completed,
            error: None,
        };
        interrupt::observe_root_agent_completion(&mut pending, &completion, &mut app);
        assert!(pending.is_empty());
        assert!(app.local_inputs["q"].acknowledged);
        assert!(app.local_inputs["q"].terminal);
        assert_eq!(app.notice.as_deref(), Some("unrelated notice"));
        assert_eq!(
            app.composer.text(),
            if changed_draft { "new draft 🧭" } else { "" }
        );
        let history = app.transcript.clone();
        interrupt::observe_root_agent_completion(&mut pending, &completion, &mut app);
        assert_eq!(app.transcript, history);
        assert_eq!(
            app.rendered_history_lines(80)
                .iter()
                .filter(|line| line.contains("body"))
                .count(),
            1
        );
    }
}

#[test]
fn uncorrelated_terminal_receipt_preserves_live_hint() {
    for wrong_owner in [false, true] {
        let mut app = FileBackedApp::new("/agent/root".into());
        app.queue.apply(
            "/agent/8",
            Some(UiQueueSnapshot {
                known: true,
                revision: 1,
                paused: true,
                pending_submission_ids: vec!["q".into()],
                ..Default::default()
            }),
        );
        app.track_local_input("q", "/agent/8".into(), "body".into(), InputIntent::Agent);
        app.acknowledge_local_input("q", "/agent/8");
        assert!(app.notice.as_ref().unwrap().contains("/continue"));
        let notice = app.notice.clone();
        let mut pending = VecDeque::from([PendingRootAgentTurn {
            input: "other".into(),
            submission_id: "other".into(),
            submitted_process: Some(9),
            submitted_at_ms: 0,
        }]);
        if wrong_owner {
            app.track_local_input(
                "other",
                "/agent/9".into(),
                "other".into(),
                InputIntent::Agent,
            );
        }
        interrupt::observe_root_agent_completion(
            &mut pending,
            &UiEvent::InputCompleted {
                submission_ids: vec![if wrong_owner { "other" } else { "wrong-id" }.into()],
                status: UiInputStatus::Completed,
                error: None,
            },
            &mut app,
        );
        assert_eq!(app.notice, notice);
        assert!(!app.local_inputs["q"].terminal);
    }
}

#[tokio::test]
async fn lifetime_removed_receipt_hint_queue_dispatch() {
    for status in [UiInputStatus::Failed, UiInputStatus::Cancelled] {
        for malformed in [false, true] {
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
            let mut pending = VecDeque::from([PendingRootAgentTurn {
                input: "body".into(),
                submission_id: "q".into(),
                submitted_process: Some(pid.parse().unwrap()),
                submitted_at_ms: 0,
            }]);
            interrupt::observe_root_agent_completion(
                &mut pending,
                &UiEvent::InputCompleted {
                    submission_ids: vec!["q".into()],
                    status,
                    error: Some("diagnostic retained".into()),
                },
                &mut app,
            );
            assert!(pending.is_empty());
            assert!(!app.local_inputs.contains_key("q"));
            assert!(
                app.rendered_history_lines(80)
                    .iter()
                    .any(|line| line.contains("diagnostic retained"))
            );
            let history = app.transcript.clone();
            if malformed {
                shell
                    .write(&format!("{owner}/machine/ui/queue"), b"invalid")
                    .await
                    .unwrap();
            }
            let event = if malformed {
                FileBackedEvent::QueueChanged {
                    owner: owner.clone(),
                }
            } else {
                FileBackedEvent::QueueUnavailable {
                    owner: owner.clone(),
                }
            };
            queue::dispatch_queue_event(&shell, &mut app, &pending, event).await;
            assert_eq!(app.notice.as_deref(), Some("queue unknown"));
            assert_eq!(app.transcript, history);
            app.notice = Some("unrelated notice".into());
            queue::dispatch_queue_event(
                &shell,
                &mut app,
                &pending,
                FileBackedEvent::QueueUnavailable {
                    owner: owner.clone(),
                },
            )
            .await;
            assert_eq!(app.notice.as_deref(), Some("unrelated notice"));
            app.queue.apply("/agent/99999", None);
            app.notice = Some("new Root notice".into());
            queue::dispatch_queue_event(
                &shell,
                &mut app,
                &pending,
                FileBackedEvent::QueueUnavailable { owner },
            )
            .await;
            assert_eq!(app.notice.as_deref(), Some("new Root notice"));
        }
    }
}
