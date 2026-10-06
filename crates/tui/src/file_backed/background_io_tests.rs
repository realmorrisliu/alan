use super::*;
use alan_agent_protocol::InputIntent;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn held_host_operation_keeps_input_draw_and_quit_available_and_preserves_draft() {
    let (release, wait) = tokio::sync::oneshot::channel();
    let wait = Arc::new(Mutex::new(Some(wait)));
    let handler: ProjectControlHandler = Arc::new(move |_| {
        let wait = wait.lock().unwrap().take().unwrap();
        Box::pin(async move {
            wait.await.unwrap();
            Ok(ProjectControlResult::Mounted {
                receipt: ProjectMountReceipt {
                    grant_id: "grant".into(),
                    namespace_path: "/mnt/project".into(),
                    label: "project".into(),
                    access: ProjectAccess::ReadOnly,
                },
                completion_root: "/unused".into(),
            })
        })
    });
    let mut app = FileBackedApp::new("/agent/root".into());
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut jobs = tokio::task::JoinSet::new();
    project_io::start(
        &mut app,
        &handler,
        ProjectControl::Mount {
            operation_id: project_dispatch::project_operation_id(),
            host_path: "/unused".into(),
            access: ProjectAccess::ReadOnly,
        },
        "/agent/8".into(),
        &mut jobs,
        &tx,
    );
    tx.send(FileBackedEvent::Terminal(TerminalEvent::Paste(
        "保留 draft".into(),
    )))
    .await
    .unwrap();
    let event = receive_file_backed_event(&mut VecDeque::new(), &mut rx)
        .await
        .unwrap();
    let pending_turns = VecDeque::new();
    assert!(dispatch_with_pending_submissions(&mut app, event, &pending_turns).is_none());
    let check_pending_input = |app: &mut FileBackedApp, draft: &str| {
        let enter = || {
            FileBackedEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )))
        };
        assert!(dispatch_with_pending_submissions(app, enter(), &pending_turns).is_none());
        assert_eq!(app.composer.text(), draft);
        assert!(
            app.local_inputs.is_empty(),
            "blocked input must not enter history"
        );
        let mut help_app = app.clone();
        help_app.composer.set_text("/help");
        assert!(
            dispatch_with_pending_submissions(&mut help_app, enter(), &pending_turns).is_none()
        );
        assert!(help_app.notice.as_deref().unwrap().contains("/compact"));
        let mut quit_app = app.clone();
        quit_app.composer.set_text("/quit");
        assert!(matches!(
            dispatch_with_pending_submissions(&mut quit_app, enter(), &pending_turns),
            Some(FileBackedAction::Quit)
        ));
    };
    check_pending_input(&mut app, "保留 draft");
    assert_eq!(app.composer.text(), "保留 draft");
    assert!(!app.project_boundary_available(false));
    let backend = ratatui::backend::TestBackend::new(80, 20);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| layout::draw_at(frame, &app, 0))
        .unwrap();
    release.send(()).unwrap();
    let FileBackedEvent::ProjectHostCompleted {
        owner,
        command,
        result,
    } = rx.recv().await.unwrap()
    else {
        panic!("host completion");
    };
    assert!(
        project_io::finish(&mut app, owner, Some("/agent/8".into()), command, result).is_some()
    );
    assert_eq!(
        app.composer.text(),
        "保留 draft",
        "late authorization must not erase editing"
    );
    assert!(!app.project_host_pending);
    assert!(app.pending_project_control.is_some());
    for (suffix, expected, fenced) in [
        (" after mount", "保留 draft after mount", false),
        (" fenced", "保留 draft after mount fenced", true),
    ] {
        if fenced {
            app.fence_project_control();
            assert!(app.pending_project_control.as_ref().unwrap().fenced);
        }
        assert!(
            dispatch_with_pending_submissions(
                &mut app,
                FileBackedEvent::Terminal(TerminalEvent::Paste(suffix.into())),
                &pending_turns,
            )
            .is_none()
        );
        check_pending_input(&mut app, expected);
    }
    jobs.join_next().await.unwrap().unwrap();
}

#[test]
fn external_revoke_clears_only_the_confirmed_selected_grant() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply("/agent/8", Some(Default::default()));
    app.project = Some(ProjectMountReceipt {
        grant_id: "selected".into(),
        namespace_path: "/mnt/project".into(),
        label: "project".into(),
        access: ProjectAccess::ReadWrite,
    });
    app.set_file_candidates(vec![CompletionCandidate::new("secret.txt", None)]);
    project_io::observe_grant(&mut app, "other", Some(false));
    project_io::observe_grant(&mut app, "selected", None);
    assert!(app.project.is_some());
    project_io::observe_grant(&mut app, "selected", Some(false));
    assert!(app.project.is_none());
    assert!(app.completion_sources.files.is_empty());
    assert!(app.project_boundary_available(false));
}

#[tokio::test]
async fn held_model_observation_does_not_block_input_and_invalidated_result_is_ignored() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(stdio_tests::FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let owner = format!("/agent/{pid}");
    let (reached, release) = fault.pause_next_walk_with_suffix("/machine/ui/models");
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut jobs = tokio::task::JoinSet::new();
    let request = observation_io::start(shell, true, tx.clone(), &mut jobs);
    let mut app = FileBackedApp::new(owner.clone());
    app.model.apply(&owner, None);
    observation_io::request(&request, owner.clone());
    reached.await.unwrap();
    tx.send(FileBackedEvent::Terminal(TerminalEvent::Paste(
        "still editable".into(),
    )))
    .await
    .unwrap();
    let event = receive_file_backed_event(&mut VecDeque::new(), &mut rx)
        .await
        .unwrap();
    app.dispatch_with_pending_submission(event, false);
    assert_eq!(app.composer.text(), "still editable");
    observation_io::request(&request, String::new());
    release.send(()).unwrap();
    let FileBackedEvent::ObservationRead {
        revision,
        owner,
        snapshot,
    } = rx.recv().await.unwrap()
    else {
        panic!("observation result");
    };
    observation_io::apply(&mut app, &request, revision, &owner, snapshot);
    assert!(app.model.snapshot.is_none());
    drop(request);
    jobs.join_next().await.unwrap().unwrap();
}

#[tokio::test]
async fn project_cwd_write_fences_actual_root_replacement_without_retargeting_or_revoking() {
    for during_write in [false, true] {
        let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
        let fault = Arc::new(stdio_tests::FaultingFileServer::new(root.clone()));
        namespace.replace_mount(
            "/agent",
            InProcessTransport::new(fault.clone()),
            alan_kernel::Access::ReadWrite,
        );
        let owner = format!("/agent/{pid}");
        let receipt = ProjectMountReceipt {
            grant_id: "retained-grant".into(),
            namespace_path: "/mnt/candidate".into(),
            label: "candidate".into(),
            access: ProjectAccess::ReadWrite,
        };
        let (id, command) = project_dispatch::project_selector(&receipt.namespace_path);
        let mut app = FileBackedApp::new("/agent/root".into());
        app.stage_project_control(
            owner.clone(),
            id.clone(),
            Some((receipt, "/unused".into())),
            None,
        );
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut jobs = tokio::task::JoinSet::new();
        let outcome = model_transport_tests::catch_poll(async {
            let next = if during_write {
                let (reached, release) = fault.pause_next_write_with_suffix("/machine/ctl");
                project_io::write_cwd(
                    &mut app,
                    &shell,
                    owner.clone(),
                    id.clone(),
                    command.clone(),
                    &mut jobs,
                    &tx,
                );
                tokio::time::timeout(std::time::Duration::from_secs(3), reached)
                    .await
                    .unwrap()
                    .unwrap();
                let next = model_transport_tests::replace(&shell, &root, &namespace).await;
                release.send(()).unwrap();
                next
            } else {
                let next = model_transport_tests::replace(&shell, &root, &namespace).await;
                project_io::write_cwd(
                    &mut app,
                    &shell,
                    owner.clone(),
                    id.clone(),
                    command.clone(),
                    &mut jobs,
                    &tx,
                );
                next
            };
            let event = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let FileBackedEvent::ProjectCwdWritten {
                owner: written_owner,
                id: written_id,
                owner_current,
                result,
            } = event
            else {
                panic!("cwd result");
            };
            assert_eq!(written_owner, owner);
            assert_eq!(written_id, id);
            assert!(!owner_current, "Root changed at actual transport boundary");
            assert_eq!(
                result.is_ok(),
                during_write,
                "only the already-pinned old-owner write may complete"
            );
            let old_events =
                String::from_utf8(shell.cat(&format!("{owner}/events")).await.unwrap()).unwrap();
            assert_eq!(
                old_events.matches(&command).count(),
                usize::from(during_write)
            );
            let new_events =
                String::from_utf8(shell.cat(&format!("/agent/{next}/events")).await.unwrap())
                    .unwrap();
            assert!(
                !new_events.contains("project-cwd-v1"),
                "never retarget selection to replacement Root"
            );
            // The root loop fences this exact correlated late result; no terminal
            // Action confirmation exists that could authorize Host grant cleanup.
            let pending = app.pending_project_control.as_ref().unwrap();
            assert_eq!((&pending.owner, &pending.id), (&written_owner, &written_id));
            app.fence_project_control();
            assert!(app.pending_project_control.as_ref().unwrap().fenced);
            assert_eq!(
                app.retained_project_grant().as_deref(),
                Some("retained-grant")
            );
            assert!(app.project_cleanup.is_none());
            assert!(app.take_ready_project_revoke().is_none());
            assert!(app.project.is_none());
            assert_eq!(app.namespace_cwd, std::path::PathBuf::from("/"));
        })
        .await;
        if outcome.is_err() {
            jobs.abort_all();
        }
        while let Some(joined) = jobs.join_next().await {
            if outcome.is_ok() {
                joined.unwrap();
            }
        }
        assert!(
            !fault
                .open_paths()
                .iter()
                .any(|path| path.ends_with("/machine/ctl")),
            "cwd descriptor must be closed"
        );
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}

#[tokio::test]
async fn unknown_project_reply_retries_exact_operation_and_preserves_editing() {
    let handler: ProjectControlHandler = Arc::new(|_| {
        Box::pin(async {
            Ok(ProjectControlResult::MountUncertain {
                message: "reply lost".into(),
            })
        })
    });
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply("/agent/8", Some(Default::default()));
    let command = ProjectControl::Mount {
        operation_id: project_dispatch::project_operation_id(),
        host_path: "/fixture".into(),
        access: ProjectAccess::ReadOnly,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let mut jobs = tokio::task::JoinSet::new();
    project_io::start(
        &mut app,
        &handler,
        command.clone(),
        "/agent/8".into(),
        &mut jobs,
        &tx,
    );
    app.composer.set_text_with_cursor("界 draft 🦀", 4);
    app.input_intent = InputIntent::ForceAgent;
    let FileBackedEvent::ProjectHostCompleted {
        owner,
        command: delivered,
        result,
    } = rx.recv().await.unwrap()
    else {
        panic!("Host response")
    };
    project_io::finish(&mut app, owner, Some("/agent/8".into()), delivered, result);
    assert!(!app.project_host_pending);
    assert!(!app.project_boundary_available(false));
    let enter = || {
        FileBackedEvent::Terminal(TerminalEvent::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )))
    };
    let pending = VecDeque::new();
    assert!(dispatch_with_pending_submissions(&mut app, enter(), &pending).is_none());
    assert_eq!(app.composer.text(), "界 draft 🦀");
    assert_eq!(app.composer.cursor(), 4);
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
    for (text, quit) in [("/help", false), ("/quit", true)] {
        let mut clone = app.clone();
        clone.composer.set_text(text);
        clone.input_intent = InputIntent::Agent;
        let action = dispatch_with_pending_submissions(&mut clone, enter(), &pending);
        assert_eq!(matches!(action, Some(FileBackedAction::Quit)), quit);
    }
    // Actual slash dispatch must permit the retained retry despite the new-selection fence.
    let mut retry_app = app.clone();
    retry_app.composer.set_text("/project");
    retry_app.input_intent = InputIntent::Agent;
    let Some(FileBackedAction::Project(retry)) =
        dispatch_with_pending_submissions(&mut retry_app, enter(), &pending)
    else {
        panic!("retained retry")
    };
    assert_eq!(retry, command);
    project_io::start(&mut app, &handler, retry, "/agent/8".into(), &mut jobs, &tx);
    assert_eq!(app.composer.text(), "界 draft 🦀");
    assert_eq!(app.composer.cursor(), 4);
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
    let FileBackedEvent::ProjectHostCompleted {
        owner,
        command: delivered,
        result,
    } = rx.recv().await.unwrap()
    else {
        panic!("retry response")
    };
    project_io::finish(&mut app, owner, Some("/agent/8".into()), delivered, result);
    let other = ProjectControl::Mount {
        operation_id: project_dispatch::project_operation_id(),
        host_path: "/other".into(),
        access: ProjectAccess::ReadWrite,
    };
    project_io::finish(
        &mut app,
        "/agent/8".into(),
        Some("/agent/8".into()),
        other,
        Ok(ProjectControlResult::MountRejected {
            message: "stale".into(),
        }),
    );
    assert_eq!(app.uncertain_project_mount.as_ref().unwrap().1, command);
    project_io::start(
        &mut app,
        &handler,
        command.clone(),
        "/agent/9".into(),
        &mut jobs,
        &tx,
    );
    assert!(
        !app.project_host_pending,
        "different Root cannot retry old operation"
    );
    project_io::finish(
        &mut app,
        "/agent/8".into(),
        Some("/agent/8".into()),
        command,
        Ok(ProjectControlResult::MountRejected {
            message: "known rejection".into(),
        }),
    );
    assert!(app.uncertain_project_mount.is_none());
    assert!(app.project_boundary_available(false));
    while jobs.join_next().await.is_some() {}
}
