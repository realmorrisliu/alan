use super::*;
use alan_agent_protocol::{InputIntent, UiQueueSnapshot};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn enter() -> FileBackedEvent {
    FileBackedEvent::Terminal(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    )))
}
async fn fixture() -> (
    alan_shell::Shell,
    FileBackedApp,
    VecDeque<PendingRootAgentTurn>,
) {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("exact draft\nwith spaces  ");
    app.track_local_input(
        "mine",
        format!("/agent/{pid}"),
        app.composer.text().into(),
        InputIntent::Agent,
    );
    let pending = VecDeque::from([PendingRootAgentTurn {
        input: app.composer.text().into(),
        submission_id: "mine".into(),
        submitted_process: Some(pid.parse().unwrap()),
        submitted_at_ms: 0,
    }]);
    let snapshot = UiQueueSnapshot {
        known: true,
        revision: 1,
        paused: true,
        pending_submission_ids: vec!["mine".into()],
        ..Default::default()
    };
    shell
        .write(
            &format!("/agent/{pid}/machine/ui/queue"),
            &serde_json::to_vec(&snapshot).unwrap(),
        )
        .await
        .unwrap();
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    (shell, app, pending)
}
#[tokio::test]
async fn boundary_acknowledged_paused_dispatch_opens_and_confirms_project() {
    let (shell, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    app.composer.set_text("/project");
    assert!(dispatch_with_pending_submissions(&mut app, enter(), &pending).is_none());
    assert!(
        app.project_selection.is_some(),
        "actual Enter gate must allow acknowledged paused Q"
    );
    app.composer.set_text("/tmp/fixture");
    assert!(matches!(
        dispatch_with_pending_submissions(&mut app, enter(), &pending),
        Some(FileBackedAction::Project(ProjectControl::Mount { .. }))
    ));
    let events = shell
        .cat(&format!("{}/events", app.queue.owner))
        .await
        .unwrap();
    assert!(!String::from_utf8(events).unwrap().contains("continue"));
}
#[tokio::test]
async fn boundary_receipt_visible_once_and_repeated_receipt_preserves_new_draft() {
    let (_, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(
        app.transcript
            .iter()
            .filter(|cell| matches!(cell, HistoryCell::User(text) if text == &pending[0].input))
            .count(),
        1,
        "paused acknowledged input must be visible before Tape"
    );
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|notice| notice.contains("queued"))
    );
    app.composer.set_text(&pending[0].input);
    queue::confirm_local_receipts(&mut app, &pending);
    let mut pending = pending;
    interrupt::observe_root_agent_completion(
        &mut pending,
        &UiEvent::InputCompleted {
            submission_ids: vec!["mine".into()],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        },
        &mut app,
    );
    assert_eq!(
        app.composer.text(),
        "exact draft\nwith spaces  ",
        "one receipt must not consume a later identical draft"
    );
}
#[tokio::test]
async fn boundary_delayed_receipt_preserves_recalled_opposite_route() {
    let (_, mut app, pending) = fixture().await;
    app.composer.set_text(&pending[0].input);
    app.input_intent = InputIntent::Command;
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(app.composer.text(), pending[0].input);
    assert_eq!(app.input_intent, InputIntent::Command);
}
#[tokio::test]
async fn boundary_hydrated_narrow_status_uses_typed_state_color() {
    let (_, mut app, _) = fixture().await;
    for (state, color) in [
        (UiActivityState::Idle, Color::Green),
        (UiActivityState::Running, Color::Cyan),
        (UiActivityState::Paused, Color::Yellow),
    ] {
        app.activity.state = state;
        assert_eq!(app.context_line(24).style.fg, Some(color));
    }
}
#[tokio::test]
async fn boundary_receipt_is_not_stream_boundary_and_tape_places_dispatch() {
    for intent in [
        InputIntent::Agent,
        InputIntent::Command,
        InputIntent::ForceAgent,
    ] {
        let (_, mut app, pending) = fixture().await;
        app.input_intent = intent;
        app.track_local_input(
            "mine",
            app.queue.owner.clone(),
            pending[0].input.clone(),
            intent,
        );
        app.apply_tape_record(
            serde_json::from_str(
                r#"{"version":1,"kind":"message","role":"user","content":"A","submission_id":"a"}"#,
            )
            .unwrap(),
        );
        app.push_output("A first".into());
        queue::confirm_local_receipts(&mut app, &pending);
        app.push_output(" suffix".into());
        assert_eq!(
            app.transcript
                .iter()
                .filter_map(HistoryCell::assistant_source)
                .collect::<Vec<_>>(),
            vec!["A first suffix"],
            "receipt must not divide A"
        );
        app.apply_tape_record(serde_json::from_value(serde_json::json!({"version":1,"kind":"message","role":"user","content":pending[0].input,"submission_id":"mine","input_intent":intent})).unwrap());
        assert!(
            matches!(
                app.transcript.last(),
                Some(HistoryCell::User(_) | HistoryCell::Command(_))
            ),
            "dispatch Tape must place Q after A"
        );
    }
}

#[tokio::test]
async fn boundary_full_hydration_preserves_pending_receipt() {
    let (shell, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    assert!(
        app.rendered_history_lines(80)
            .iter()
            .any(|line| line.contains("exact draft")),
        "actual full hydration must retain Q receipt absent from Tape"
    );
}

#[tokio::test]
async fn boundary_terminal_and_tape_retire_full_payload() {
    let (_, mut app, mut pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    app.apply_tape_record(serde_json::from_value(serde_json::json!({"version":1,"kind":"message","role":"user","content":pending[0].input,"submission_id":"mine"})).unwrap());
    interrupt::observe_root_agent_completion(
        &mut pending,
        &UiEvent::InputCompleted {
            submission_ids: vec!["mine".into()],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        },
        &mut app,
    );
    assert!(
        app.local_inputs["mine"].body.capacity() == 0,
        "terminal + exact Tape must retire local payload while retaining ID suppression"
    );
}

#[tokio::test]
async fn boundary_physical_drain_then_two_exact_same_body_ids_emit_once_each() {
    let (_, mut app, mut pending) = fixture().await;
    let body = pending[0].input.clone();
    let mut second = pending[0].clone();
    second.submission_id = "second".into();
    app.track_local_input(
        "second",
        app.queue.owner.clone(),
        body.clone(),
        InputIntent::Agent,
    );
    pending.push_back(second);
    app.queue
        .snapshot
        .as_mut()
        .unwrap()
        .pending_submission_ids
        .push("second".into());
    queue::confirm_local_receipts(&mut app, &pending);
    let emitted = app.drain_committed_scrollback(80, 1);
    assert_eq!(
        emitted
            .iter()
            .filter(|line| line.to_string().contains("exact draft"))
            .count(),
        2
    );
    assert!(app.rendered_history_lines(80).is_empty());
    for id in ["mine", "second"] {
        app.apply_tape_record(serde_json::from_value(serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":id})).unwrap());
        queue::confirm_local_receipts(&mut app, &pending);
    }
    assert!(
        app.rendered_history_lines(80).is_empty(),
        "exact dispatch Tape must not re-emit physically drained receipts"
    );
    app.push_output("answer after dispatch".into());
    assert_eq!(
        app.transcript
            .iter()
            .filter_map(HistoryCell::assistant_source)
            .collect::<Vec<_>>(),
        vec!["answer after dispatch"]
    );
}

#[tokio::test]
async fn boundary_hydration_uses_authoritative_order_not_lexical_ids() {
    let (shell, mut app, _) = fixture().await;
    app.local_inputs.clear();
    for (id, body) in [
        ("z-first", "first admitted"),
        ("a-second", "second admitted"),
    ] {
        app.track_local_input(id, app.queue.owner.clone(), body.into(), InputIntent::Agent);
        app.acknowledge_local_input(id, &app.queue.owner.clone());
    }
    let mut q = app.queue.snapshot.clone().unwrap();
    q.pending_submission_ids = vec!["z-first".into(), "a-second".into()];
    shell
        .write(
            &format!("{}/machine/ui/queue", app.queue.owner),
            &serde_json::to_vec(&q).unwrap(),
        )
        .await
        .unwrap();
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    assert_eq!(
        app.transcript,
        vec![
            HistoryCell::User("first admitted".into()),
            HistoryCell::User("second admitted".into())
        ]
    );
}

#[tokio::test]
async fn boundary_hydration_does_not_restore_other_owner() {
    let (shell, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    app.local_inputs.get_mut("mine").unwrap().owner = "/agent/old".into();
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    assert!(
        app.rendered_history_lines(80).is_empty(),
        "old Root receipt cannot enter new Root hydration"
    );
}

#[tokio::test]
async fn boundary_cancelled_and_failed_without_tape_do_not_resurrect() {
    for status in [
        alan_agent_protocol::UiInputStatus::Cancelled,
        alan_agent_protocol::UiInputStatus::Failed,
    ] {
        let (shell, mut app, mut pending) = fixture().await;
        queue::confirm_local_receipts(&mut app, &pending);
        interrupt::observe_root_agent_completion(
            &mut pending,
            &UiEvent::InputCompleted {
                submission_ids: vec!["mine".into()],
                status,
                error: None,
            },
            &mut app,
        );
        let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
            .await
            .unwrap();
        tails.close().await;
        assert!(
            !app.rendered_history_lines(80)
                .iter()
                .any(|line| line.contains("exact draft")),
            "settled nondispatched receipt must not resurrect"
        );
    }
}

#[tokio::test]
async fn boundary_paused_receipt_has_exact_id_and_explicit_controls() {
    let (_, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    let notice = app.notice.as_deref().unwrap();
    assert!(
        notice.contains("mine")
            && notice.contains("paused")
            && notice.contains("/continue")
            && notice.contains("/discard"),
        "paused exact-ID receipt needs explicit controls: {notice}"
    );
}

#[tokio::test]
async fn boundary_delayed_receipt_persists_original_intent_and_reloads() {
    for intent in [
        InputIntent::Command,
        InputIntent::ForceAgent,
        InputIntent::Agent,
    ] {
        let (_, mut app, pending) = fixture().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history");
        app.composer = crate::composer::Composer::from_history_path(path.clone());
        app.composer.set_text(&pending[0].input);
        app.track_local_input(
            "mine",
            app.queue.owner.clone(),
            pending[0].input.clone(),
            intent,
        );
        app.input_intent = if intent == InputIntent::Command {
            InputIntent::Agent
        } else {
            InputIntent::Command
        };
        let revision = app.composer.draft_revision();
        queue::confirm_local_receipts(&mut app, &pending);
        assert_eq!(app.composer.text(), pending[0].input);
        assert!(app.composer.draft_revision() > revision);
        queue::confirm_local_receipts(&mut app, &pending);
        assert_eq!(crate::composer::load_history(&path, 20).len(), 1);
        let mut reloaded = crate::composer::Composer::from_history_path(path);
        reloaded.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(reloaded.text(), pending[0].input);
        assert_eq!(reloaded.recalled_intent(), Some(intent));
    }
}

#[tokio::test]
async fn remaining_partial_receipt_tape_never_replays_prefix() {
    let (_, mut app, pending) = fixture().await;
    let body = "唯一-prefix\n中間 content\nlast uncommitted";
    app.track_local_input(
        "mine",
        app.queue.owner.clone(),
        body.into(),
        InputIntent::Agent,
    );
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(24, false), 1), 1);
    assert!(
        !app.rendered_history_lines(24)
            .join("\n")
            .contains("唯一-prefix")
    );
    let record = serde_json::json!({"version":1,"kind":"message","role":"user","content":body,"submission_id":"mine"});
    app.apply_tape_record(serde_json::from_value(record.clone()).unwrap());
    assert!(
        !app.rendered_history_lines(24)
            .join("\n")
            .contains("唯一-prefix"),
        "exact Tape must preserve receipt source cutoff"
    );
    app.apply_tape_record(serde_json::from_value(record).unwrap());
    assert!(
        app.rendered_history_lines(24)
            .join("\n")
            .contains("last uncommitted")
    );
}

#[tokio::test]
async fn remaining_partial_receipt_hydration_resize_never_replays_prefix() {
    let (shell, mut app, pending) = fixture().await;
    let body = "唯一-prefix\n中間 content\nlast uncommitted";
    app.track_local_input(
        "mine",
        app.queue.owner.clone(),
        body.into(),
        InputIntent::Agent,
    );
    queue::confirm_local_receipts(&mut app, &pending);
    assert_eq!(app.prune_rendered_prefix(RenderOpts::new(24, false), 1), 1);
    for width in [18, 80, 24] {
        let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
            .await
            .unwrap();
        tails.close().await;
        let text = app.rendered_history_lines(width).join("\n");
        assert!(
            !text.contains("唯一-prefix"),
            "hydration must preserve committed source"
        );
        assert!(text.contains("uncommitted"));
    }
}

#[tokio::test]
async fn remaining_same_owner_final_reattach_preserves_receipt_association() {
    let (shell, mut app, pending) = fixture().await;
    app.transcript
        .push(HistoryCell::Assistant("active A".into()));
    queue::confirm_local_receipts(&mut app, &pending);
    let (tails, _) = reattach_to_current_agent(&shell, "/agent/root", &mut app, &[])
        .await
        .unwrap();
    tails.close().await;
    app.apply_tape_record(serde_json::from_value(serde_json::json!({"version":1,"kind":"message","role":"user","content":pending[0].input,"submission_id":"mine"})).unwrap());
    assert!(
        app.transcript
            .iter()
            .any(|cell| cell.assistant_source() == Some("active A")),
        "final merge must not point receipt at active assistant"
    );
    assert_eq!(
        app.transcript
            .iter()
            .filter(|cell| matches!(cell, HistoryCell::User(text) if text == &pending[0].input))
            .count(),
        1
    );
}

#[tokio::test]
async fn remaining_typed_queue_refresh_updates_once_only_receipt_hint() {
    let (shell, mut app, pending) = fixture().await;
    queue::confirm_local_receipts(&mut app, &pending);
    app.composer.set_text("new draft");
    let revision = app.composer.draft_revision();
    for (n, state) in ["queued", "active", "uncertain", "unconfirmed"]
        .into_iter()
        .enumerate()
    {
        let q = UiQueueSnapshot {
            known: state != "unconfirmed",
            revision: n as u64 + 2,
            pending_submission_ids: if state == "queued" {
                vec!["mine".into()]
            } else {
                vec![]
            },
            active_submission_ids: if state == "active" {
                vec!["mine".into()]
            } else {
                vec![]
            },
            uncertain_submission_ids: if state == "uncertain" {
                vec!["mine".into()]
            } else {
                vec![]
            },
            ..Default::default()
        };
        shell
            .write(
                &format!("{}/machine/ui/queue", app.queue.owner),
                &serde_json::to_vec(&q).unwrap(),
            )
            .await
            .unwrap();
        let owner = app.queue.owner.clone();
        app.queue
            .apply(&owner, queue::read_queue(&shell, &owner).await);
        queue::confirm_local_receipts(&mut app, &pending);
        let hint = app.notice.as_deref().unwrap();
        assert!(
            hint.contains("mine") && hint.contains(state) && !hint.contains("queued or active"),
            "typed current hint must refresh to {state}: {hint}"
        );
        assert_eq!(app.composer.text(), "new draft");
        assert_eq!(app.composer.draft_revision(), revision);
    }
}

#[tokio::test]
async fn boundary_nonzero_unknown_is_not_initial_authorization() {
    let (_, mut app, _) = fixture().await;
    app.queue = queue::QueueProjection::default();
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            revision: 5,
            ..Default::default()
        }),
    );
    assert!(!app.project_boundary_available(false));
}
