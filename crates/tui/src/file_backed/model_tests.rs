use super::*;
use alan_agent_protocol::{
    ReasoningControls, ReasoningEffort, UiAdmittedModel, UiModelBinding, UiModelCatalog,
    UiModelChoice, UiModelControlSource, UiModelSnapshot, UiQueueSnapshot,
};
fn binding(model: &str) -> UiModelBinding {
    UiModelBinding {
        profile: "profile".into(),
        provider: "provider".into(),
        model: model.into(),
        reasoning: ReasoningControls {
            effort: Some(ReasoningEffort::High),
        },
        control_source: UiModelControlSource::AgentMachineOverride,
    }
}
pub(super) fn install_header_model(app: &mut FileBackedApp, model: &str) {
    let mut s = snapshot("1", 1);
    s.selected_next = Some(binding(model));
    s.active = None;
    app.model.apply("/agent/1", Some(s));
}
pub(super) fn snapshot(pid: &str, version: u64) -> UiModelSnapshot {
    UiModelSnapshot {
        known: true,
        process_path: format!("/proc/{pid}"),
        publication_version: version,
        selected_next: Some(binding("B")),
        active: Some(binding("A")),
        admitted: vec![
            UiAdmittedModel {
                submission_id: "old-q".into(),
                binding: Some(binding("A")),
            },
            UiAdmittedModel {
                submission_id: "legacy".into(),
                binding: None,
            },
        ],
        catalog: Some(UiModelCatalog {
            profile: "profile".into(),
            models: vec![
                UiModelChoice {
                    model: "B".into(),
                    supported_reasoning_efforts: vec![],
                    default_reasoning_effort: None,
                },
                UiModelChoice {
                    model: "C".into(),
                    supported_reasoning_efforts: vec![],
                    default_reasoning_effort: None,
                },
            ],
        }),
        ..Default::default()
    }
}
pub(super) fn ready() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            pending_submission_ids: vec!["old-q".into()],
            ..Default::default()
        }),
    );
    app.model.apply("/agent/1", Some(snapshot("1", 3)));
    app
}
#[test]
fn model_launch_config_is_not_authoritative() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.set_effective_model(Some("launch-secret-model".into()));
    let line = app.context_line(120).to_string();
    assert!(
        !line.contains("launch-secret-model"),
        "launch config leaked as model truth: {line}"
    );
}
#[test]
fn model_owner_version_unknown_and_readfailure_are_distinct() {
    let mut p = model::ModelProjection::default();
    p.apply("/agent/1", Some(snapshot("1", 4)));
    p.apply("/agent/1", Some(snapshot("1", 3)));
    assert_eq!(p.known().unwrap().publication_version, 4);
    p.apply("/agent/1", None);
    assert!(p.snapshot.is_none());
    p.apply("/agent/1", Some(snapshot("1", 3)));
    assert!(p.snapshot.is_none());
    let unknown = UiModelSnapshot {
        process_path: "/proc/1".into(),
        ..Default::default()
    };
    p.apply("/agent/1", Some(unknown));
    assert!(!p.snapshot.as_ref().unwrap().known);
    p.apply("/agent/2", Some(snapshot("2", 1)));
    assert_eq!(p.known().unwrap().publication_version, 1);
    p.apply("/agent/2", Some(snapshot("1", 20)));
    assert!(p.snapshot.is_none());
    let mut invalid = snapshot("2", 99);
    invalid.version = 999;
    p.apply("/agent/2", Some(invalid));
    assert!(p.snapshot.is_none());
}
#[test]
fn model_status_separates_active_next_admitted_and_legacy_unknown() {
    let mut app = ready();
    app.handle_command("/status");
    let status = app.notice.as_ref().unwrap();
    for text in [
        "selected-next: profile / provider / B",
        "active: profile / provider / A",
        "admitted old-q: profile / provider / A",
        "admitted legacy: unknown",
        "reasoning high",
        "AgentMachineOverride",
    ] {
        assert!(status.contains(text), "{status}");
    }
    let header = app.context_line(200).to_string();
    assert!(header.contains("active A") && header.contains("next B"));
    let mut missing = snapshot("1", 4);
    missing.selected_next = None;
    app.model.apply("/agent/1", Some(missing));
    assert!(app.model.status().contains("selected-next: unknown"));
}
#[test]
fn model_picker_preserves_intent_draft_paused_admission_and_exact_receipt() {
    let mut app = ready();
    app.composer.set_text("retained\n draft");
    app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
    assert!(app.handle_command("/model").is_none());
    assert!(app.model_chooser.active);
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Up,
        crossterm::event::KeyModifiers::NONE,
    ));
    let action = app
        .handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ))
        .unwrap();
    let FileBackedAction::SelectModel { owner, id, op } = action else {
        panic!("not typed model control");
    };
    assert!(matches!(op, alan_agent_protocol::Op::SelectModel { model } if model == "B"));
    assert_eq!(app.composer.text(), "retained\n draft");
    assert_eq!(
        app.input_intent,
        alan_agent_protocol::InputIntent::ForceAgent
    );
    assert!(app.queue.snapshot.as_ref().unwrap().paused);
    assert_eq!(
        app.model.known().unwrap().admitted[0]
            .binding
            .as_ref()
            .unwrap()
            .model,
        "A"
    );
    let receipt = alan_agent_protocol::UiEvent::InputCompleted {
        submission_ids: vec![id],
        status: alan_agent_protocol::UiInputStatus::Completed,
        error: None,
    };
    assert!(!app.observe_model_receipt("/agent/2", &receipt));
    let wrong = alan_agent_protocol::UiEvent::InputCompleted {
        submission_ids: vec!["other".into()],
        status: alan_agent_protocol::UiInputStatus::Completed,
        error: None,
    };
    assert!(!app.observe_model_receipt(&owner, &wrong));
    app.model.apply("/agent/1", Some(snapshot("1", 4)));
    assert!(
        app.model_chooser.pending.is_some(),
        "projection cannot acknowledge control"
    );
    assert!(app.observe_model_receipt(&owner, &receipt));
    assert!(!app.observe_model_receipt(&owner, &receipt));
    assert!(app.model_chooser.pending.is_none());
    app.handle_command("/model");
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    ));
    assert_eq!(app.composer.text(), "retained\n draft");
    assert_eq!(
        app.input_intent,
        alan_agent_protocol::InputIntent::ForceAgent
    );
}
#[tokio::test]
async fn model_file_backed_hydration_notification_and_failure() {
    let (shell, _root, _namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&snapshot(&pid, 2)).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    assert_eq!(app.model.owner, owner);
    assert_eq!(app.model.known().unwrap().publication_version, 2);
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = super::model_transport_tests::catch_poll(async {
        app.queue.owner = owner.clone();
        app.handle_command("/model");
        app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Up,
            crossterm::event::KeyModifiers::NONE,
        ));
        let Some(FileBackedAction::SelectModel {
            owner: selected_owner,
            id,
            op,
        }) = app.handle_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ))
        else {
            panic!("no selection");
        };
        assert_eq!(selected_owner, owner);
        model::write_selection(&shell, &mut app, &owner, &id, op).await;
        let events =
            String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap()).unwrap();
        assert!(
            events.contains(&format!("ctl:select-model {id} B")),
            "{events}"
        );
        assert!(app.model_chooser.pending.is_some());
        let receipt = alan_agent_protocol::UiEvent::InputCompleted {
            submission_ids: vec![id],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        };
        shell
            .write(
                &format!("{owner}/machine/ui/events"),
                format!("{}\n", serde_json::to_string(&receipt).unwrap()).as_bytes(),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if let FileBackedEvent::ModelReceipt { owner, event } = rx.recv().await.unwrap() {
                    assert!(app.observe_model_receipt(&owner, &event));
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(app.model_chooser.pending.is_none());
        shell
            .write(
                &format!("{owner}/machine/ui/models"),
                &serde_json::to_vec(&snapshot(&pid, 5)).unwrap(),
            )
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let event = rx.recv().await.unwrap();
                if matches!(&event, FileBackedEvent::ModelChanged { .. }) {
                    model::dispatch_model_event(&shell, &mut app, event).await;
                    if app
                        .model
                        .known()
                        .is_some_and(|s| s.publication_version == 5)
                    {
                        break;
                    }
                }
            }
        })
        .await
        .unwrap();
        shell
            .write(&format!("{owner}/machine/ui/models"), b"malformed")
            .await
            .unwrap();
        model::dispatch_model_event(
            &shell,
            &mut app,
            FileBackedEvent::ModelChanged {
                owner: owner.clone(),
            },
        )
        .await;
        assert!(app.model.snapshot.is_none());
        model::dispatch_model_event(
            &shell,
            &mut app,
            FileBackedEvent::ModelUnavailable { owner },
        )
        .await;
        assert!(app.model.snapshot.is_none());
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn identical_active_and_next_binding_shows_one_model_and_actual_effort() {
    let mut projection = model::ModelProjection::default();
    let mut value = snapshot("1", 1);
    value.selected_next = value.active.clone();
    projection.apply("/agent/1", Some(value));
    assert_eq!(projection.header(), "active A · high");
}
