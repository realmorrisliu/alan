use super::*;
use std::sync::Arc;
use stdio_tests::{EXEC_SPEC, FaultingFileServer, PID_MOUNT};

pub(super) async fn catch_poll(
    future: impl std::future::Future<Output = ()>,
) -> std::thread::Result<()> {
    let mut exercise = Box::pin(future);
    std::future::poll_fn(|cx| {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| exercise.as_mut().poll(cx)))
        {
            Ok(std::task::Poll::Pending) => std::task::Poll::Pending,
            Ok(std::task::Poll::Ready(())) => std::task::Poll::Ready(Ok(())),
            Err(panic) => std::task::Poll::Ready(Err(panic)),
        }
    })
    .await
}

pub(super) async fn replace(
    shell: &alan_shell::Shell,
    root: &alan_agentfs::AgentRootFs,
    namespace: &alan_kernel::LiveNamespace,
) -> String {
    let pid = shell.spawn(EXEC_SPEC).await.unwrap();
    root.bind_process(pid.clone(), Arc::new(alan_agentfs::AgentFs::new()))
        .await;
    root.set_root_process(pid.clone()).await;
    namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        alan_kernel::Access::ReadOnly,
    );
    pid
}

#[tokio::test]
async fn final_replacement_during_control_completion_invalidates_old_owner() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(FaultingFileServer::new(root.clone()));
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
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
    let outcome = catch_poll(async {
        app.handle_command("/model");
        app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        let Some(FileBackedAction::SelectModel { owner, id, op }) =
            app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        else {
            panic!("no action")
        };
        let (reached, resume) = fault.pause_next_write_with_suffix("/machine/ctl");
        let replacement = async {
            reached.await.unwrap();
            let next = replace(&shell, &root, &namespace).await;
            resume.send(()).unwrap();
            next
        };
        let (_, next) = tokio::join!(
            model::write_selection(&shell, &mut app, &owner, &id, op),
            replacement
        );
        assert!(
            app.model_chooser.pending.is_none(),
            "old-owner write completed after Root replacement"
        );
        assert_eq!(
            app.model_chooser.uncertain,
            Some((owner.clone(), id.clone()))
        );
        watchers
            .refresh_root_agent_attachment(
                &shell,
                "/agent/root",
                &mut app,
                &mut rx,
                &mut VecDeque::new(),
                &tx,
            )
            .await;
        assert_eq!(app.model.owner, format!("/agent/{next}"));
        assert_eq!(
            app.model_chooser.uncertain,
            Some((owner.clone(), id.clone()))
        );
        let events =
            String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap()).unwrap();
        assert_eq!(
            events.matches(&format!("ctl:select-model {id} B")).count(),
            1
        );
        assert!(
            !String::from_utf8(shell.cat(&format!("/agent/{next}/events")).await.unwrap())
                .unwrap()
                .contains("select-model")
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
async fn root_loss_revokes_model_events_and_prewrite_until_descriptor_repin() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(FaultingFileServer::new(root));
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
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
    let outcome = catch_poll(async {
        app.composer.set_text_with_cursor("ForceAgent draft\n  ", 4);
        app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
        for (index, unavailable) in ["invalid-pid\n", ""].into_iter().enumerate() {
            // Stage through the real picker while Root is still pinned.
            app.handle_command("/model");
            app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            let Some(FileBackedAction::SelectModel {
                owner: selected_owner,
                id,
                op,
            }) = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            else {
                panic!("repinned model picker did not stage control");
            };
            namespace.replace_mount(
                PID_MOUNT,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "pid",
                    unavailable.as_bytes().to_vec(),
                ))),
                alan_kernel::Access::ReadOnly,
            );
            model::write_selection(&shell, &mut app, &selected_owner, &id, op).await;
            let events =
                String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap()).unwrap();
            assert!(
                !events.contains(&format!("ctl:select-model {id}")),
                "Root loss must prevent ctl write"
            );
            assert_eq!(
                app.model_chooser.uncertain,
                Some((owner.clone(), id.clone()))
            );
            watchers
                .refresh_root_agent_attachment(
                    &shell,
                    "/agent/root",
                    &mut app,
                    &mut rx,
                    &mut VecDeque::new(),
                    &tx,
                )
                .await;
            assert!(
                app.model.owner.is_empty(),
                "Root loss must revoke model event authority"
            );
            let version = index as u64 + 10;
            shell
                .write(
                    &format!("{owner}/machine/ui/models"),
                    &serde_json::to_vec(&model_tests::snapshot(&pid, version)).unwrap(),
                )
                .await
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    let event = rx.recv().await.unwrap();
                    if matches!(&event, FileBackedEvent::ModelChanged { .. }) {
                        model::dispatch_model_event(&shell, &mut app, event).await;
                        // All actual old-owner notifications must remain powerless.
                        assert!(app.model.known().is_none());
                        break;
                    }
                }
            })
            .await
            .unwrap();
            app.handle_command("/status");
            assert!(app.notice.as_deref().unwrap().contains("unknown"));
            assert!(app.handle_command("/model").is_none());
            assert!(!app.model_chooser.active);
            assert_eq!(app.composer.text(), "ForceAgent draft\n  ");
            assert_eq!(app.composer.cursor(), 4);
            assert_eq!(
                app.input_intent,
                alan_agent_protocol::InputIntent::ForceAgent
            );
            // Exact actual receipt can settle uncertainty but cannot repin projection.
            let receipt = alan_agent_protocol::UiEvent::InputCompleted {
                submission_ids: vec![id],
                status: alan_agent_protocol::UiInputStatus::Failed,
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
                    if let FileBackedEvent::ModelReceipt { owner, event } = rx.recv().await.unwrap()
                    {
                        assert!(app.observe_model_receipt(&owner, &event));
                        break;
                    }
                }
            })
            .await
            .unwrap();
            assert!(app.model.known().is_none());
            namespace.replace_mount(
                PID_MOUNT,
                InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
                    "pid",
                    format!("{pid}\n").into_bytes(),
                ))),
                alan_kernel::Access::ReadOnly,
            );
            watchers
                .refresh_root_agent_attachment(
                    &shell,
                    "/agent/root",
                    &mut app,
                    &mut rx,
                    &mut VecDeque::new(),
                    &tx,
                )
                .await;
            assert_eq!(app.model.owner, owner);
            assert_eq!(app.model.known().unwrap().publication_version, version);
            assert!(app.model_chooser.uncertain.is_none());
            assert_eq!(app.composer.text(), "ForceAgent draft\n  ");
            assert_eq!(app.composer.cursor(), 4);
        }
        app.handle_command("/model");
        assert!(app.model_chooser.active);
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    assert!(
        !fault
            .open_paths()
            .iter()
            .any(|p| p.ends_with("/machine/ui/events") || p.ends_with("/machine/tape"))
    );
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn final_injected_panic_clunks_before_propagation() {
    let (shell, root, namespace, _) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let mut app = FileBackedApp::new("/agent/root".into());
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = catch_poll(async {
        assert!(
            fault
                .open_paths()
                .iter()
                .any(|p| p.ends_with("/machine/ui/events"))
        );
        panic!("injected model fixture failure")
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    assert!(
        !fault
            .open_paths()
            .iter()
            .any(|p| p.ends_with("/machine/ui/events") || p.ends_with("/machine/tape")),
        "teardown did not clunk recovery/watch descriptors"
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| std::panic::resume_unwind(
            outcome.unwrap_err()
        )))
        .is_err()
    );
}
