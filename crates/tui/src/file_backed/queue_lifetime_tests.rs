use super::*;
use alan_agent_protocol::{InputIntent, UiInputStatus, UiQueueSnapshot};

fn receipt(app: &mut FileBackedApp, owner: &str, id: &str) -> PendingRootAgentTurn {
    app.queue.apply(owner, None);
    app.track_local_input(
        id,
        owner.into(),
        "first line\nsecond line\nthird line".into(),
        InputIntent::Agent,
    );
    app.acknowledge_local_input(id, owner);
    PendingRootAgentTurn {
        input: "first line\nsecond line\nthird line".into(),
        submission_id: id.into(),
        submitted_process: Some(owner.rsplit('/').next().unwrap().parse().unwrap()),
        submitted_at_ms: 0,
    }
}

fn tape(id: &str) -> TapeRecordV1 {
    serde_json::from_value(serde_json::json!({"version":1,"kind":"message","role":"user","content":"first line\nsecond line\nthird line","submission_id":id})).unwrap()
}

fn complete(app: &mut FileBackedApp, turn: PendingRootAgentTurn, status: UiInputStatus) {
    interrupt::observe_root_agent_completion(
        &mut VecDeque::from([turn.clone()]),
        &UiEvent::InputCompleted {
            submission_ids: vec![turn.submission_id],
            status,
            error: None,
        },
        app,
    );
}

#[test]
fn lifetime_completed_source_and_full_drain() {
    let mut app = FileBackedApp::new("/agent/root".into());
    let turn = receipt(&mut app, "/agent/42", "q");
    complete(&mut app, turn, UiInputStatus::Completed);
    assert_eq!(
        app.local_inputs["q"].body.capacity(),
        0,
        "HistoryCell owns terminal source storage"
    );
    assert!(
        app.transcript[0]
            .input_source()
            .unwrap()
            .0
            .contains("third line")
    );
    app.prune_rendered_prefix(RenderOpts::new(80, false), 100);
    assert!(app.local_inputs["q"].committed);
    for _ in 0..3 {
        app.apply_tape_record(tape("q"));
    }
    assert!(
        app.transcript.is_empty(),
        "late repeated Tape must not replay committed source"
    );
}

#[test]
fn lifetime_partial_terminal_late_tape() {
    let mut app = FileBackedApp::new("/agent/root".into());
    let turn = receipt(&mut app, "/agent/42", "q");
    app.prune_rendered_prefix(RenderOpts::new(80, false), 1);
    let before = app.transcript[0].render_styled_lines(RenderOpts::new(80, false));
    complete(&mut app, turn, UiInputStatus::Completed);
    assert_eq!(app.local_inputs["q"].body.capacity(), 0);
    app.apply_tape_record(tape("q"));
    assert_eq!(
        app.transcript[0].render_styled_lines(RenderOpts::new(80, false)),
        before
    );
    app.apply_tape_record(tape("q"));
    assert_eq!(app.transcript.len(), 1);
}

#[tokio::test]
async fn lifetime_real_clear_late_tape_and_reattach() {
    for mode in 0..3 {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let mut app = FileBackedApp::new("/agent/root".into());
        let turn = receipt(&mut app, &owner, "q");
        if mode == 1 {
            app.prune_rendered_prefix(RenderOpts::new(80, false), 1);
        }
        if mode == 2 {
            complete(&mut app, turn, UiInputStatus::Completed);
        }
        app.handle_command("/clear");
        assert_eq!(app.local_inputs["q"].cell, None);
        assert!(app.local_inputs["q"].committed);
        for _ in 0..2 {
            app.apply_tape_record(tape("q"));
            assert!(app.transcript.is_empty());
        }
        shell.write(&format!("{owner}/machine/tape"), format!("{}\n", serde_json::json!({"version":1,"kind":"message","role":"user","content":"first line\nsecond line\nthird line","submission_id":"q"})).as_bytes()).await.unwrap();
        for _ in 0..2 {
            let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
                .await
                .unwrap();
            tails.close().await;
            assert!(
                app.transcript
                    .iter()
                    .all(|cell| cell.input_source().is_none()),
                "clear must survive hydration"
            );
        }
        app.apply_tape_record(tape("fresh"));
        assert_eq!(
            app.transcript
                .iter()
                .filter(|cell| cell.input_source().is_some())
                .count(),
            1
        );
    }
}

#[test]
fn lifetime_failed_cancelled_and_nonterminal_storage() {
    for status in [UiInputStatus::Failed, UiInputStatus::Cancelled] {
        let mut app = FileBackedApp::new("/agent/root".into());
        let turn = receipt(&mut app, "/agent/42", "q");
        complete(&mut app, turn, status);
        assert!(!app.local_inputs.contains_key("q"));
        assert!(
            app.transcript
                .iter()
                .all(|cell| cell.input_source().is_none())
        );
        assert!(!app.transcript.is_empty());
    }
    let mut app = FileBackedApp::new("/agent/root".into());
    let turn = receipt(&mut app, "/agent/42", "q");
    let mut pending = VecDeque::from([turn]);
    app.prune_rendered_prefix(RenderOpts::new(80, false), 1);
    assert!(!app.local_inputs["q"].body.is_empty());
    let before = app.transcript[0].clone();
    app.queue.apply("/agent/43", None);
    interrupt::settle_unknown_replaced_input(&mut pending, Some(43), &mut app);
    assert!(pending.is_empty());
    assert!(
        app.rendered_history_lines(200)
            .iter()
            .any(|line| line.contains("outcome is unknown"))
    );
    assert!(!app.local_inputs["q"].terminal);
    assert!(!app.local_inputs["q"].body.is_empty());
    assert_eq!(app.transcript[0], before);
}

#[test]
fn lifetime_active_admission_is_not_execution() {
    let mut app = FileBackedApp::new("/agent/root".into());
    receipt(&mut app, "/agent/42", "q");
    app.queue.apply(
        "/agent/42",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec!["q".into()],
            ..Default::default()
        }),
    );
    app.dispatch(FileBackedEvent::Ui(UiEvent::Activity {
        snapshot: UiActivitySnapshot::paused(Some(10)),
    }));
    app.refresh_local_input_hint("q", "/agent/42");
    assert_eq!(app.activity.state, UiActivityState::Paused);
    assert_eq!(app.activity_label(), Some("waiting for input"));
    assert!(!app.notice.as_ref().unwrap().contains("running"));
    assert!(app.notice.as_ref().unwrap().contains("active"));
    model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    for state in [UiActivityState::Idle, UiActivityState::Paused] {
        app.activity.state = state;
        for width in 16..=80 {
            let line = app.context_line(width);
            let text = line.to_string();
            assert!(line.width() <= width, "{width}: {text}");
            assert!(
                text.split_whitespace()
                    .any(|word| matches!(word, "active" | "act")),
                "{width}: {text}"
            );
            assert!(!text.contains("working"), "{width}: {text}");
        }
    }
}

#[test]
fn lifetime_queue_admission_and_execution_remain_visible_in_narrow_headers() {
    let mut app = FileBackedApp::new("/agent/root".into());
    model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    for (queue, cues) in [
        (
            UiQueueSnapshot::default(),
            &["queue unknown", "q ?", "q?"][..],
        ),
        (
            UiQueueSnapshot {
                known: true,
                revision: 1,
                uncertain_submission_ids: vec!["uncertain".into()],
                ..Default::default()
            },
            &["uncertain", "unc"],
        ),
        (
            UiQueueSnapshot {
                known: true,
                revision: 1,
                deferred: true,
                ..Default::default()
            },
            &["deferred", "def"],
        ),
        (
            UiQueueSnapshot {
                known: true,
                revision: 1,
                paused: true,
                ..Default::default()
            },
            &["paused", "hold"],
        ),
        (
            UiQueueSnapshot {
                known: true,
                revision: 1,
                active_submission_ids: vec!["active".into()],
                ..Default::default()
            },
            &["active", "act"],
        ),
        (
            UiQueueSnapshot {
                known: true,
                revision: 1,
                pending_submission_ids: vec!["pending".into()],
                ..Default::default()
            },
            &["queued", "q 1", "q+"],
        ),
    ] {
        assert!(queue.is_valid());
        app.queue.apply("/agent/root", Some(queue));
        for (state, failed, states) in [
            (UiActivityState::Running, false, &["working", "run"][..]),
            (UiActivityState::Idle, false, &["ready"]),
            (UiActivityState::Paused, false, &["paused"]),
            (UiActivityState::Idle, true, &["failed", "fail"]),
        ] {
            app.activity.state = state;
            app.last_input_failed = failed;
            for width in 16..=80 {
                let line = app.context_line(width);
                let text = line.to_string();
                assert!(line.width() <= width, "{width}: {text}");
                assert!(
                    states
                        .iter()
                        .any(|s| text.split_whitespace().any(|word| word == *s)),
                    "{width}: {text}"
                );
                assert!(cues.iter().any(|cue| text.contains(cue)), "{width}: {text}");
                if state != UiActivityState::Running {
                    assert!(
                        !text
                            .split_whitespace()
                            .any(|word| matches!(word, "working" | "run")),
                        "{width}: {text}"
                    );
                }
            }
        }
    }
}
