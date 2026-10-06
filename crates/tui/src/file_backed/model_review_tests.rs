use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
fn key(app: &mut FileBackedApp, code: KeyCode) -> Option<FileBackedAction> {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE))
}
#[test]
fn remaining_running_selection_public_dispatch() {
    let mut app = super::model_tests::ready();
    app.activity = alan_agent_protocol::UiActivitySnapshot::running(0);
    app.activity.waiting_submission_ids.push("old-q".into());
    app.handle_command("/model");
    assert!(app.model_chooser.active);
    app.composer.set_text("retained draft");
    app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
    app.dispatch_with_pending_submission(
        FileBackedEvent::Terminal(crossterm::event::Event::Key(KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::NONE,
        ))),
        true,
    );
    assert!(matches!(
        app.dispatch_with_pending_submission(
            FileBackedEvent::Terminal(crossterm::event::Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE
            ))),
            true
        ),
        Some(FileBackedAction::SelectModel { .. })
    ));
    assert_eq!(app.composer.text(), "retained draft");
    assert_eq!(
        app.model.known().unwrap().active.as_ref().unwrap().model,
        "A"
    );
}
#[tokio::test]
async fn remaining_valid_large_document() {
    let (shell, _root, _namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let snapshot = super::model_tests::snapshot(&pid, 1);
    for length in [300000, 1048576] {
        let mut bytes = serde_json::to_vec(&snapshot).unwrap();
        bytes.resize(length, b' ');
        assert!(
            serde_json::from_slice::<alan_agent_protocol::UiModelSnapshot>(&bytes)
                .unwrap()
                .is_valid()
        );
        shell
            .write(&format!("{owner}/machine/ui/models"), &bytes)
            .await
            .unwrap();
        assert_eq!(
            model::read_model(&shell, &owner).await,
            Some(snapshot.clone())
        );
    }
}
#[test]
fn remaining_status_catalog_is_field_level() {
    let mut app = super::model_tests::ready();
    for (version, catalog, label) in [
        (4, None, "catalog: unknown or unavailable"),
        (
            5,
            Some(alan_agent_protocol::UiModelCatalog {
                profile: "profile".into(),
                models: vec![],
            }),
            "catalog: empty",
        ),
    ] {
        let mut snapshot = super::model_tests::snapshot("1", version);
        snapshot.catalog = catalog;
        app.model.apply("/agent/1", Some(snapshot));
        app.handle_command("/status");
        let status = app.notice.as_ref().unwrap();
        assert!(status.contains(label));
        assert!(status.contains("active: profile / provider / A"));
        assert!(status.contains("selected-next: profile / provider / B"));
    }
}
#[tokio::test]
async fn remaining_control_write_failure_is_uncertain_not_rejection() {
    let (shell, root, _namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let mut app = super::model_tests::ready();
    app.queue.owner = owner.clone();
    app.model
        .apply(&owner, Some(super::model_tests::snapshot(&pid, 1)));
    app.handle_command("/model");
    key(&mut app, KeyCode::Up);
    let Some(FileBackedAction::SelectModel { owner, id, op }) = key(&mut app, KeyCode::Enter)
    else {
        panic!()
    };
    assert!(root.unbind_process(&pid).await);
    model::write_selection(&shell, &mut app, &owner, &id, op).await;
    assert!(app.model_chooser.pending.is_none());
    assert_eq!(app.model_chooser.uncertain, Some((owner, id)));
    assert!(app.context_line(200).to_string().contains("uncertain"));
}
#[test]
fn remaining_running_slash_submit_routes_without_task_admission() {
    let mut app = super::model_tests::ready();
    app.activity = alan_agent_protocol::UiActivitySnapshot::running(0);
    app.composer.set_text("/model");
    assert!(
        app.dispatch_with_pending_submission(
            FileBackedEvent::Terminal(crossterm::event::Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE
            ))),
            true
        )
        .is_none()
    );
    assert!(app.model_chooser.active);
    assert!(app.transcript.is_empty());
}
#[test]
fn review_bare_enter_requires_explicit_selection() {
    let mut app = super::model_tests::ready();
    app.handle_command("/model");
    assert!(
        key(&mut app, KeyCode::Enter).is_none(),
        "bare Enter silently selected row zero"
    );
    assert!(app.model_chooser.pending.is_none());
}
#[test]
fn review_catalog_reorder_never_sends_invisible_model() {
    let mut app = super::model_tests::ready();
    app.handle_command("/model");
    key(&mut app, KeyCode::Down);
    let visible = app
        .completion
        .as_ref()
        .unwrap()
        .selected_candidate()
        .unwrap()
        .value
        .clone();
    let mut next = app.model.known().unwrap().clone();
    next.publication_version += 1;
    next.catalog.as_mut().unwrap().models.reverse();
    app.model.apply("/agent/1", Some(next));
    if let Some(FileBackedAction::SelectModel {
        op: alan_agent_protocol::Op::SelectModel { model },
        ..
    }) = key(&mut app, KeyCode::Enter)
    {
        assert_eq!(model, visible, "catalog changed under visible selection");
    }
}
#[test]
fn review_receipt_loss_owner_scope_and_late_settlement() {
    let mut app = super::model_tests::ready();
    app.handle_command("/model");
    key(&mut app, KeyCode::Down);
    let Some(FileBackedAction::SelectModel { owner, id, .. }) = key(&mut app, KeyCode::Enter)
    else {
        panic!()
    };
    app.lose_model_receipts("/agent/2");
    assert!(app.model_chooser.pending.is_some());
    app.lose_model_receipts(&owner);
    assert!(app.model_chooser.pending.is_none());
    assert_eq!(
        app.model_chooser.uncertain,
        Some((owner.clone(), id.clone()))
    );
    app.notice = None;
    assert!(app.context_line(200).to_string().contains("uncertain"));
    app.handle_command("/model");
    assert!(!app.model_chooser.active);
    let receipt = alan_agent_protocol::UiEvent::InputCompleted {
        submission_ids: vec![id],
        status: alan_agent_protocol::UiInputStatus::Completed,
        error: None,
    };
    assert!(!app.observe_model_receipt("/agent/2", &receipt));
    assert!(app.observe_model_receipt(&owner, &receipt));
    assert!(!app.observe_model_receipt(&owner, &receipt));
    assert!(app.model_chooser.uncertain.is_none());
}
#[test]
fn review_confirmation_revalidates_unknown_owner_state() {
    for reason in ["unknown", "owner"] {
        let mut app = super::model_tests::ready();
        app.composer.set_text("exact draft\n  ");
        app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
        app.handle_command("/model");
        key(&mut app, KeyCode::Down);
        match reason {
            "unknown" => app.model.apply("/agent/1", None),
            _ => app.model.owner = "/agent/2".into(),
        }
        assert!(key(&mut app, KeyCode::Enter).is_none(), "reason {reason}");
        assert!(app.model_chooser.pending.is_none());
        assert_eq!(app.composer.text(), "exact draft\n  ");
        assert_eq!(
            app.input_intent,
            alan_agent_protocol::InputIntent::ForceAgent
        );
    }
}
#[tokio::test]
async fn review_file_backed_malformed_receipt_preserves_uncertainty() {
    let (shell, _root, _namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = super::model_transport_tests::catch_poll(async {
        app.model_chooser.pending = Some((owner.clone(), "pending-model".into()));
        shell
            .write(&format!("{owner}/machine/ui/events"), b"invalid-json\n")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let event = rx.recv().await.unwrap();
                if matches!(event, FileBackedEvent::ModelReceiptsUnavailable { .. }) {
                    app.dispatch(event);
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            app.model_chooser.uncertain,
            Some((owner, "pending-model".into()))
        );
        assert!(app.context_line(200).to_string().contains("uncertain"));
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
#[tokio::test]
async fn review_model_read_rejects_oversized_document() {
    let (shell, _root, _namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    // AgentFS itself rejects publications beyond its 1 MiB contract.
    assert!(
        shell
            .write(&format!("{owner}/machine/ui/models"), &vec![b' '; 1048577])
            .await
            .is_err()
    );
    assert!(
        super::action_detail_io::reference::range_with_budget(
            &shell,
            &format!("{owner}/machine/ui/models"),
            0,
            1048577,
            1048576
        )
        .await
        .is_err()
    );
}
#[test]
fn review_root_reset_preserves_visible_uncertainty() {
    let mut app = super::model_tests::ready();
    app.model_chooser.pending = Some(("/agent/1".into(), "pending".into()));
    app.reset_for_root_process_change();
    let line = app.context_line(200).to_string();
    let notice = app.notice.as_deref().unwrap_or("");
    assert!(
        format!("{line} {notice}").contains("uncertain"),
        "reset erased control disposition"
    );
}
