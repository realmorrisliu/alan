use super::*;
use alan_agent_protocol::{PlanItem, PlanItemStatus};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn plan(explanation: &str, content: &str, status: PlanItemStatus) -> UiPlanSnapshot {
    UiPlanSnapshot::new(
        Some(explanation.into()),
        vec![PlanItem {
            id: "step-1".into(),
            content: content.into(),
            status,
        }],
    )
}

#[test]
fn compact_plan_records_keep_distinct_snapshots_and_cleared_plan() {
    let mut app = FileBackedApp::new("/agent/1".into());
    let first = plan(
        "old explanation",
        "read server> ready",
        PlanItemStatus::InProgress,
    );
    let second = plan(
        "new explanation",
        "verify result",
        PlanItemStatus::Completed,
    );
    for snapshot in [&first, &first, &second, &first] {
        app.apply_ui_plan_snapshot(snapshot.clone());
    }
    assert_eq!(app.transcript.len(), 3);
    for width in [48, 80, 120] {
        let rows = app.styled_history_lines(width);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].to_string().contains("Plan 1 · 0/1 completed"));
        assert!(rows[1].to_string().contains("Plan 2 · 1/1 completed"));
        assert!(rows[2].to_string().contains("Plan 3"));
    }
    app.apply_ui_plan_snapshot(UiPlanSnapshot::empty());
    assert_eq!(app.transcript.len(), 4);
    assert!(
        app.styled_history_lines(80)[3]
            .to_string()
            .contains("0/0 completed")
    );
}

#[tokio::test]
async fn retained_plan_navigation_after_drain_keeps_old_snapshot_and_draft() {
    let (shell, _, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let first = plan(
        "OLD_EXPLANATION",
        "server> ready\noriginal second line",
        PlanItemStatus::InProgress,
    );
    let second = plan("NEW_EXPLANATION", "new step", PlanItemStatus::Completed);
    let mut app = FileBackedApp::new(owner.clone());
    for snapshot in [&first, &first, &second] {
        shell
            .write(
                &format!("{owner}/machine/ui/plan"),
                serde_json::to_string(snapshot).unwrap().as_bytes(),
            )
            .await
            .unwrap();
        shell
            .write(
                &format!("{owner}/machine/ui/events"),
                format!(
                    "{}\n",
                    serde_json::to_string(&UiEvent::Plan {
                        snapshot: snapshot.clone()
                    })
                    .unwrap()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        app.apply_ui_plan_snapshot(snapshot.clone());
    }
    assert_eq!(read(&shell, &owner).await.unwrap().len(), 2);
    assert_eq!(app.drain_committed_scrollback(48, 1).len(), 2);
    assert!(app.transcript.is_empty());
    app.composer.set_text("界 draft 🦀");
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let saved = (
        app.composer.text().to_string(),
        app.composer.cursor(),
        app.input_intent,
    );
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    super::super::action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(app.modal.plans.len(), 2);
    assert_eq!(app.modal.selected, 1);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let text = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("OLD_EXPLANATION")
            && text.contains("server> ready")
            && text.contains("original second line"),
        "{text}"
    );
    assert!(!text.contains("NEW_EXPLANATION"), "{text}");
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 10);
    app.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 0);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(
        (
            app.composer.text().to_string(),
            app.composer.cursor(),
            app.input_intent
        ),
        saved
    );
    assert!(app.transcript.is_empty());
}

#[test]
fn plan_detail_fences_late_responses_and_reports_unavailability() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let generation = app.modal.generation;
    app.apply_plan_details(
        "/agent/1".into(),
        generation,
        vec![PlanEntry {
            owner: "/agent/old".into(),
            revision: 0,
            observed_only: false,
            snapshot: Err("retained Process files are gone".into()),
        }],
    );
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("unavailable"))
    );
    let expected = app.modal.rows.clone();
    for (owner, token) in [("/agent/other", generation), ("/agent/1", generation + 1)] {
        app.apply_plan_details(owner.into(), token, vec![]);
        assert_eq!(app.modal.rows, expected);
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    app.apply_plan_details(
        "/agent/1".into(),
        app.modal.generation,
        vec![PlanEntry {
            owner: "/agent/1".into(),
            revision: 1,
            observed_only: false,
            snapshot: Ok(plan("late", "stale", PlanItemStatus::Pending)),
        }],
    );
    assert!(app.modal.rows.is_empty());
}

#[test]
fn new_request_takes_input_from_details_and_fences_old_result() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.composer.set_text("saved draft");
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let generation = app.modal.generation;
    let request = super::super::file_surface::RequestSnapshot {
        id: "approval".into(),
        kind: "confirmation".into(),
        prompt: "Approve operation".into(),
        options: "[\"approve\",\"reject\"]".into(),
        status: "pending".into(),
    };
    app.set_pending_yield(
        super::super::file_surface::request_snapshot_to_pending_yield(request).unwrap(),
    );
    assert!(!app.modal.active);
    assert_eq!(app.composer.text(), "saved draft");
    app.apply_plan_details("/agent/1".into(), generation, vec![]);
    assert!(app.modal.plans.is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(!app.modal.active, "approval owns Ctrl+O");
}

#[tokio::test]
async fn long_ui_history_keeps_small_plans_and_distinguishes_display_bound() {
    let (shell, _, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let path = format!("{owner}/machine/ui/events");
    let event = UiEvent::Notice {
        snapshot: alan_agent_protocol::UiNoticeSnapshot {
            version: 1,
            kind: alan_agent_protocol::UiNoticeKind::Warning,
            message: "界".repeat(11000),
        },
    };
    for _ in 0..10 {
        shell
            .write(
                &path,
                format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
            )
            .await
            .unwrap();
    }
    let snapshot = plan(
        "older exact explanation",
        "old exact step",
        PlanItemStatus::Pending,
    );
    let event = UiEvent::Plan {
        snapshot: snapshot.clone(),
    };
    shell
        .write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
        )
        .await
        .unwrap();
    let entries = read(&shell, &owner).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].snapshot.as_ref().unwrap(), &snapshot);
    let event = UiEvent::Plan {
        snapshot: plan("large", &"x".repeat(270000), PlanItemStatus::InProgress),
    };
    shell
        .write(
            &path,
            format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
        )
        .await
        .unwrap();
    let entries = read(&shell, &owner).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].snapshot.as_ref().unwrap(), &snapshot);
    let error = entries[1].snapshot.as_ref().err().unwrap();
    assert!(
        error.contains("display bound") && error.contains("original remains"),
        "{error}"
    );
}

#[tokio::test]
async fn incomplete_current_event_preserves_earlier_plan_and_explicit_gap() {
    let (shell, _, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let event = UiEvent::Plan {
        snapshot: plan("old", "exact old step", PlanItemStatus::Pending),
    };
    let path = format!("{owner}/machine/ui/events");
    shell
        .write(
            &path,
            format!("{}\n{{\"type\":", serde_json::to_string(&event).unwrap()).as_bytes(),
        )
        .await
        .unwrap();
    let entries = read(&shell, &owner).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        rows(&entries[0])
            .iter()
            .any(|line| line.to_string().contains("exact old step"))
    );
    assert!(
        entries[1]
            .snapshot
            .as_ref()
            .err()
            .unwrap()
            .contains("incomplete")
    );
}

#[tokio::test]
async fn snapshot_without_old_events_never_substitutes_new_stream_plan_one() {
    let (shell, _, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let old = plan(
        "original observed snapshot",
        "old step",
        PlanItemStatus::Pending,
    );
    let new = plan(
        "new retained snapshot",
        "new step",
        PlanItemStatus::InProgress,
    );
    let mut app = FileBackedApp::new(owner.clone());
    app.apply_ui_plan_snapshot(old.clone());
    shell
        .write(
            &format!("{owner}/machine/ui/events"),
            format!(
                "{}\n",
                serde_json::to_string(&UiEvent::Plan {
                    snapshot: new.clone()
                })
                .unwrap()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    super::super::action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(app.modal.plans.len(), 2);
    assert!(app.modal.plans[0].observed_only);
    assert_eq!(app.modal.plans[0].snapshot.as_ref().unwrap(), &old);
    assert_eq!(app.modal.plans[1].snapshot.as_ref().unwrap(), &new);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal
            .rows
            .iter()
            .any(|line| line.to_string().contains("original observed snapshot"))
    );
}

#[tokio::test]
async fn bounded_retained_plan_history_merges_captured_tail_once_in_order() {
    let (shell, _, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let snapshots = (1..=4)
        .map(|revision| {
            plan(
                &format!("exact snapshot {revision}"),
                &"x".repeat(100_000),
                PlanItemStatus::Pending,
            )
        })
        .collect::<Vec<_>>();
    let mut app = FileBackedApp::new(owner.clone());
    for snapshot in &snapshots {
        shell
            .write(
                &format!("{owner}/machine/ui/events"),
                format!(
                    "{}\n",
                    serde_json::to_string(&UiEvent::Plan {
                        snapshot: snapshot.clone()
                    })
                    .unwrap()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        app.apply_ui_plan_snapshot(snapshot.clone());
    }
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    super::super::action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(app.modal.plans.len(), 5);
    for (entry, snapshot) in app.modal.plans[..2].iter().zip(&snapshots[..2]) {
        assert_eq!(entry.snapshot.as_ref().unwrap(), snapshot);
        assert!(!entry.observed_only);
    }
    assert!(
        app.modal.plans[2]
            .snapshot
            .as_ref()
            .unwrap_err()
            .contains("display bound")
    );
    for (entry, snapshot) in app.modal.plans[3..].iter().zip(&snapshots[2..]) {
        assert_eq!(entry.snapshot.as_ref().unwrap(), snapshot);
        assert!(entry.observed_only);
    }
    assert_eq!(app.modal.selected, 4);
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("display bound"))
    );
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("exact snapshot 2"))
    );
}

#[test]
fn plan_merge_requires_exact_identity_and_keeps_unmatched_captured_prefix() {
    let captured = PlanEntry {
        owner: "/agent/7".into(),
        revision: 1,
        snapshot: Ok(plan("original", "step", PlanItemStatus::Pending)),
        observed_only: true,
    };
    for field in ["same", "owner", "revision", "snapshot"] {
        let mut retained = captured.clone();
        retained.observed_only = false;
        match field {
            "owner" => retained.owner = "/agent/8".into(),
            "revision" => retained.revision = 2,
            "snapshot" => {
                retained.snapshot = Ok(plan("different", "step", PlanItemStatus::Pending))
            }
            _ => {}
        }
        let entries = merge_plans(vec![retained], vec![captured.clone()]);
        assert_eq!(
            entries.len(),
            if field == "same" { 1 } else { 2 },
            "{field}"
        );
        assert_eq!(entries[0].observed_only, field != "same", "{field}");
    }
    let mut common = captured.clone();
    common.revision = 2;
    let mut retained = common.clone();
    retained.observed_only = false;
    let entries = merge_plans(vec![retained], vec![captured.clone(), common]);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].snapshot, captured.snapshot);
    assert!(entries[0].observed_only);
    assert_eq!(entries[1].revision, 2);
    assert!(!entries[1].observed_only);
}

#[tokio::test]
async fn captured_plan_does_not_hide_retained_history_read_failure() {
    let (shell, root, _, pid) = crate::file_backed::stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let original = plan("captured original", "exact step", PlanItemStatus::Pending);
    let mut app = FileBackedApp::new(owner.clone());
    app.apply_ui_plan_snapshot(original.clone());
    assert!(root.unbind_process(&pid).await);
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    app.handle_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE));
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    super::super::action_detail_io::start_pending(&shell, &mut app, &tx);
    app.dispatch(
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap(),
    );
    assert_eq!(app.modal.plans.len(), 2);
    assert_eq!(app.modal.plans[0].snapshot.as_ref().unwrap(), &original);
    assert!(app.modal.plans[0].observed_only);
    assert!(
        app.modal.plans[1]
            .snapshot
            .as_ref()
            .unwrap_err()
            .contains("Plan history unavailable")
    );
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert!(
        app.modal
            .rows
            .iter()
            .any(|row| row.to_string().contains("captured original"))
    );
}
