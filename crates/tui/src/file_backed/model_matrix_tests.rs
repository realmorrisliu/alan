use super::model_transport_tests::catch_poll;
use super::*;
use std::sync::Arc;

async fn receive_model(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
    receipt: bool,
) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let event = rx.recv().await.unwrap();
            match event {
                event @ (FileBackedEvent::ModelChanged { .. }
                | FileBackedEvent::ModelUnavailable { .. })
                    if !receipt =>
                {
                    model::dispatch_model_event(shell, app, event).await;
                    break;
                }
                event @ (FileBackedEvent::ModelReceipt { .. }
                | FileBackedEvent::ModelReceiptsUnavailable { .. })
                    if receipt =>
                {
                    app.dispatch(event);
                    break;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}
async fn publish_receipt(
    shell: &alan_shell::Shell,
    owner: &str,
    id: &str,
    status: alan_agent_protocol::UiInputStatus,
) {
    let receipt = UiEvent::InputCompleted {
        submission_ids: vec![id.into()],
        status,
        error: None,
    };
    shell
        .write(
            &format!("{owner}/machine/ui/events"),
            format!("{}\n", serde_json::to_string(&receipt).unwrap()).as_bytes(),
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn final_public_projection_receipt_matrix() {
    use alan_agent_protocol::UiInputStatus;
    for case in ["projection-first", "failed", "success-unreadable"] {
        let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
        let owner = format!("/agent/{pid}");
        let mut initial = model_tests::snapshot(&pid, 1);
        initial.selected_next = initial.active.clone();
        shell
            .write(
                &format!("{owner}/machine/ui/models"),
                &serde_json::to_vec(&initial).unwrap(),
            )
            .await
            .unwrap();
        let mut app = FileBackedApp::new("/agent/root".into());
        let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
        let outcome = catch_poll(async {
            app.composer.set_text("exact retained\n  draft");
            app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
            app.handle_command("/model");
            app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Key(
                KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            )));
            let Some(FileBackedAction::SelectModel { owner, id, op }) =
                app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Key(
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                )))
            else {
                panic!("no public chooser action")
            };
            model::write_selection(&shell, &mut app, &owner, &id, op).await;
            assert!(app.model_chooser.pending.is_some());
            assert!(
                String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap())
                    .unwrap()
                    .contains(&format!("ctl:select-model {id} B"))
            );
            if case == "projection-first" {
                shell
                    .write(
                        &format!("{owner}/machine/ui/models"),
                        &serde_json::to_vec(&model_tests::snapshot(&pid, 2)).unwrap(),
                    )
                    .await
                    .unwrap();
                receive_model(&shell, &mut app, &mut rx, false).await;
                assert!(app.context_line(200).to_string().contains("next B"));
                assert_eq!(app.model_chooser.pending, Some((owner.clone(), id.clone())));
                publish_receipt(&shell, &owner, "unrelated", UiInputStatus::Completed).await;
                receive_model(&shell, &mut app, &mut rx, true).await;
                assert_eq!(app.model_chooser.pending, Some((owner.clone(), id.clone())));
            } else if case == "success-unreadable" {
                shell
                    .write(&format!("{owner}/machine/ui/models"), b"malformed")
                    .await
                    .unwrap();
                receive_model(&shell, &mut app, &mut rx, false).await;
                assert!(app.model.known().is_none());
                assert!(app.model_chooser.pending.is_some());
            }
            publish_receipt(
                &shell,
                &owner,
                &id,
                if case == "failed" {
                    UiInputStatus::Failed
                } else {
                    UiInputStatus::Completed
                },
            )
            .await;
            receive_model(&shell, &mut app, &mut rx, true).await;
            assert!(app.model_chooser.pending.is_none());
            assert!(app.model_chooser.uncertain.is_none());
            assert_eq!(app.composer.text(), "exact retained\n  draft");
            assert_eq!(
                app.input_intent,
                alan_agent_protocol::InputIntent::ForceAgent
            );
            match case {
                "failed" => {
                    assert_eq!(app.model.known(), Some(&initial));
                    assert!(app.notice.as_deref().unwrap().contains("rejected"));
                    assert!(app.context_line(200).to_string().contains("active A"));
                }
                "success-unreadable" => {
                    assert!(app.model.known().is_none());
                    assert!(!app.context_line(200).to_string().contains("next B"));
                    assert!(!app.model.status().contains("provider / B"));
                }
                _ => assert!(app.context_line(200).to_string().contains("next B")),
            }
        })
        .await;
        watchers.stop().await;
        assert!(watchers.recovery.is_none());
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}
#[tokio::test]
async fn final_owning_receipt_read_error_is_visible_uncertainty() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(stdio_tests::FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let owner = format!("/agent/{pid}");
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&model_tests::snapshot(&pid, 1)).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = catch_poll(async {
        app.handle_command("/model");
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        let Some(FileBackedAction::SelectModel { owner, id, op }) =
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        else {
            panic!()
        };
        model::write_selection(&shell, &mut app, &owner, &id, op).await;
        fault.close(pid.parse().unwrap());
        receive_model(&shell, &mut app, &mut rx, true).await;
        assert!(app.model_chooser.pending.is_none());
        assert_eq!(app.model_chooser.uncertain, Some((owner, id)));
        app.notice = None;
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
async fn final_success_receipt_then_models_io_failure() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(stdio_tests::FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let owner = format!("/agent/{pid}");
    let mut initial = model_tests::snapshot(&pid, 1);
    initial.selected_next = initial.active.clone();
    shell
        .write(
            &format!("{owner}/machine/ui/models"),
            &serde_json::to_vec(&initial).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = catch_poll(async {
        app.composer
            .set_text_with_cursor("exact retained\n  draft", 6);
        app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
        app.handle_command("/model");
        app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Key(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        )));
        let Some(FileBackedAction::SelectModel { owner, id, op }) =
            app.dispatch(FileBackedEvent::Terminal(crossterm::event::Event::Key(
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            )))
        else {
            panic!("no public chooser action")
        };
        model::write_selection(&shell, &mut app, &owner, &id, op).await;
        assert_eq!(app.model_chooser.pending, Some((owner.clone(), id.clone())));
        publish_receipt(
            &shell,
            &owner,
            &id,
            alan_agent_protocol::UiInputStatus::Completed,
        )
        .await;
        receive_model(&shell, &mut app, &mut rx, true).await;
        assert!(app.model_chooser.pending.is_none());
        assert!(app.model_chooser.uncertain.is_none());
        assert!(app.notice.as_deref().unwrap().contains("completed"));
        assert_eq!(app.model.known(), Some(&initial));
        let controls_before = shell.cat(&format!("{owner}/events")).await.unwrap();
        assert_eq!(
            String::from_utf8(controls_before.clone())
                .unwrap()
                .matches(&format!("ctl:select-model {id} B"))
                .count(),
            1
        );
        app.handle_command("/model");
        assert!(app.model_chooser.active);
        let (reached, resume) = fault.pause_read_after_matching_reads("/machine/ui/models", 1);
        shell
            .write(
                &format!("{owner}/machine/ui/models"),
                &serde_json::to_vec(&model_tests::snapshot(&pid, 2)).unwrap(),
            )
            .await
            .unwrap();
        let inject = async {
            reached.await.unwrap();
            fault.fail_reads_with_suffix("/machine/ui/models");
            resume.send(()).unwrap();
        };
        tokio::join!(receive_model(&shell, &mut app, &mut rx, false), inject);
        assert_eq!(
            fault.failed_read_count(),
            1,
            "actual models descriptor read must fail with Io"
        );
        assert_eq!(app.model.owner, owner);
        assert!(app.model.known().is_none());
        assert!(app.model.status().contains("unknown or unavailable"));
        assert!(!app.model.status().contains("provider / B"));
        assert!(!app.context_line(200).to_string().contains("next B"));
        assert!(!app.model_chooser.active);
        assert!(app.completion.is_none());
        app.handle_command("/model");
        assert!(!app.model_chooser.active);
        assert!(app.model_chooser.pending.is_none());
        assert!(app.model_chooser.uncertain.is_none());
        assert_eq!(app.composer.text(), "exact retained\n  draft");
        assert_eq!(app.composer.cursor(), 6);
        assert_eq!(
            app.input_intent,
            alan_agent_protocol::InputIntent::ForceAgent
        );
        let controls_after =
            String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap()).unwrap();
        let controls_before = String::from_utf8(controls_before).unwrap();
        assert_eq!(
            controls_after
                .lines()
                .filter(|line| line.starts_with("ctl:"))
                .collect::<Vec<_>>(),
            controls_before
                .lines()
                .filter(|line| line.starts_with("ctl:"))
                .collect::<Vec<_>>(),
            "no extra control or retry; publication notifications are not controls"
        );
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
#[tokio::test]
async fn final_old_owner_same_id_watch_cannot_settle_new_owner() {
    let (shell, root, namespace, old_pid) = stdio_tests::live_root_agent().await;
    let old_owner = format!("/agent/{old_pid}");
    let mut old_app = FileBackedApp::new(old_owner.clone());
    let tails = hydrate_and_open_tails(&shell, &old_owner, &mut old_app)
        .await
        .unwrap();
    let (old_tx, mut old_rx) = tokio::sync::mpsc::channel(128);
    let mut old_watchers = AgentWatchers::start(tails, &old_owner, old_tx);
    let outcome = catch_poll(async {
        let pid = super::model_transport_tests::replace(&shell, &root, &namespace).await;
        let owner = format!("/agent/{pid}");
        shell
            .write(
                &format!("{owner}/machine/ui/models"),
                &serde_json::to_vec(&model_tests::snapshot(&pid, 1)).unwrap(),
            )
            .await
            .unwrap();
        let mut app = FileBackedApp::new("/agent/root".into());
        let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
        let outcome = catch_poll(async {
            app.handle_command("/model");
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            let Some(FileBackedAction::SelectModel { owner, id, op }) =
                app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            else {
                panic!()
            };
            model::write_selection(&shell, &mut app, &owner, &id, op).await;
            publish_receipt(
                &shell,
                &old_owner,
                &id,
                alan_agent_protocol::UiInputStatus::Completed,
            )
            .await;
            receive_model(&shell, &mut app, &mut old_rx, true).await;
            assert_eq!(app.model_chooser.pending, Some((owner.clone(), id.clone())));
            publish_receipt(
                &shell,
                &owner,
                "unrelated",
                alan_agent_protocol::UiInputStatus::Completed,
            )
            .await;
            receive_model(&shell, &mut app, &mut rx, true).await;
            assert_eq!(app.model_chooser.pending, Some((owner.clone(), id.clone())));
            publish_receipt(
                &shell,
                &owner,
                &id,
                alan_agent_protocol::UiInputStatus::Completed,
            )
            .await;
            receive_model(&shell, &mut app, &mut rx, true).await;
            assert!(app.model_chooser.pending.is_none());
        })
        .await;
        watchers.stop().await;
        assert!(watchers.recovery.is_none());
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    })
    .await;
    old_watchers.stop().await;
    assert!(old_watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
