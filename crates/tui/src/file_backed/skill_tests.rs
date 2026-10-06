use super::*;
use std::time::Duration;

pub(super) async fn skill_event(
    rx: &mut tokio::sync::mpsc::Receiver<FileBackedEvent>,
) -> FileBackedEvent {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = rx.recv().await.expect("watcher stopped");
            if matches!(
                event,
                FileBackedEvent::SkillsChanged { .. } | FileBackedEvent::SkillsUnavailable { .. }
            ) {
                return event;
            }
        }
    })
    .await
    .expect("skill event timeout")
}

#[tokio::test]
async fn skill_consumer_watch_adverse_snapshots_and_root_isolation() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let owner = format!("/agent/{pid}");
    let path = format!("{owner}/machine/ui/skills");
    shell
        .write(
            &path,
            &serde_json::to_vec(&snapshot(&pid, 1, &["first"])).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text_with_cursor("keep $", 6);
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
    let outcome = catch_poll(async {
        // Drain historical notification before exercising live publications.
        skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
        app.refresh_completion();
        shell
            .write(
                &path,
                &serde_json::to_vec(&snapshot(&pid, 2, &["next-only"])).unwrap(),
            )
            .await
            .unwrap();
        skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
        assert_eq!(
            app.completion.as_ref().unwrap().matches[0].value,
            "next-only"
        );
        let before = app.composer.text().to_string();
        let cursor = app.composer.cursor();
        app.apply_skills(&owner, Some(snapshot(&pid, 1, &["stale"])));
        assert_eq!(app.completion_sources.skills[0].value, "next-only");
        for invalid in [
            None,
            Some(snapshot("999999", 3, &["foreign"])),
            Some(snapshot(&pid, 3, &["bad_id"])),
        ] {
            app.apply_skills(&owner, invalid);
            assert!(app.completion_sources.skills.is_empty());
            assert!(app.completion.is_none());
            app.apply_skills(&owner, Some(snapshot(&pid, 2, &["same-version"])));
            assert!(app.completion_sources.skills.is_empty());
        }
        // Known-empty and explicit unknown both clear; only a newer publication revives.
        app.apply_skills(&owner, Some(snapshot(&pid, 3, &[])));
        app.apply_skills(&owner, Some(snapshot(&pid, 4, &["revived"])));
        app.apply_skills(&owner, Some(snapshot(&pid, 4, &[]).unknown()));
        assert!(app.completion_sources.skills.is_empty());
        shell.write(&path, b"{broken").await.unwrap();
        skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
        assert!(app.completion_sources.skills.is_empty());
        app.apply_skills(&owner, Some(snapshot(&pid, 5, &["old-owner"])));
        let next = model_transport_tests::replace(&shell, &root, &namespace).await;
        shell
            .write(
                &format!("/agent/{next}/machine/ui/skills"),
                &serde_json::to_vec(&snapshot(&next, 1, &["new-owner"])).unwrap(),
            )
            .await
            .unwrap();
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
        assert_eq!(app.skills.owner, format!("/agent/{next}"));
        assert_eq!(app.completion_sources.skills[0].value, "new-owner");
        skills::dispatch_skill_event(
            &shell,
            &mut app,
            FileBackedEvent::SkillsChanged {
                owner: owner.clone(),
            },
        )
        .await;
        skills::dispatch_skill_event(
            &shell,
            &mut app,
            FileBackedEvent::SkillsUnavailable {
                owner: owner.clone(),
            },
        )
        .await;
        assert_eq!(app.completion_sources.skills[0].value, "new-owner");
        assert_eq!(app.composer.text(), before);
        assert_eq!(app.composer.cursor(), cursor);
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
use super::model_transport_tests::catch_poll;
use alan_agent_protocol::UiSkillSnapshot;

pub(super) fn snapshot(pid: &str, revision: u64, ids: &[&str]) -> UiSkillSnapshot {
    UiSkillSnapshot {
        version: alan_agent_protocol::UI_SURFACE_VERSION,
        publication_version: revision,
        process_path: format!("/proc/{pid}"),
        known: true,
        mentionable_skill_ids: ids.iter().map(|id| (*id).into()).collect(),
    }
}

#[tokio::test]
async fn skill_consumer_unreadable_clears_and_teardown_clunks() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = std::sync::Arc::new(stdio_tests::FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let owner = format!("/agent/{pid}");
    shell
        .write(
            &format!("{owner}/machine/ui/skills"),
            &serde_json::to_vec(&snapshot(&pid, 1, &["visible"])).unwrap(),
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
        assert_eq!(app.completion_sources.skills[0].value, "visible");
        app.composer.set_text_with_cursor("draft $", 7);
        app.input_intent = alan_agent_protocol::InputIntent::ForceAgent;
        app.refresh_completion();
        fault.fail_reads_with_suffix("/machine/ui/skills");
        skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
        assert!(fault.failed_read_count() > 0);
        super::skill_review_tests::no_stale_tab(&mut app);
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    assert!(
        !fault
            .open_paths()
            .iter()
            .any(|p| p.ends_with("/events") || p.ends_with("/machine/tape")),
        "watch descriptors leaked"
    );
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn skill_consumer_hydrates_explicit_ids_before_ready() {
    let (shell, _, _, pid) = stdio_tests::live_root_agent().await;
    shell
        .write(
            &format!("/agent/{pid}/machine/ui/skills"),
            &serde_json::to_vec(&snapshot(&pid, 1, &["explicit-only", "other-skill"])).unwrap(),
        )
        .await
        .unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.set_skill_candidates(vec![CompletionCandidate::new("host-fake", None)]);
    app.composer.set_text_with_cursor("draft $", 7);
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = catch_poll(async {
        assert_eq!(
            app.completion_sources
                .skills
                .iter()
                .map(|c| c.value.as_str())
                .collect::<Vec<_>>(),
            vec!["explicit-only", "other-skill"]
        );
        app.refresh_completion();
        assert_eq!(
            app.completion.as_ref().unwrap().matches[0].value,
            "explicit-only"
        );
        assert_eq!(app.composer.text(), "draft $");
        assert_eq!(app.composer.cursor(), 7);
    })
    .await;
    watchers.stop().await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
