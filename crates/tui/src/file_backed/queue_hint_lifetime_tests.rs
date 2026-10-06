use super::*;
use alan_agent_protocol::{InputIntent, UiInputStatus, UiQueueSnapshot};

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
            assert!(!app.notice.as_ref().unwrap().contains("/continue"));
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
