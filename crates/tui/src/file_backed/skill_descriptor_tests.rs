use super::model_transport_tests::catch_poll;
use super::skill_review_tests::no_stale_tab;
use super::skill_tests::{skill_event, snapshot};
use super::*;
use alan_agent_protocol::InputIntent;
use std::sync::Arc;

#[tokio::test]
async fn skill_consumer_descriptor_boundaries_and_popup_isolation() {
    let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
    let fault = Arc::new(stdio_tests::FaultingFileServer::new(root));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(fault.clone()),
        alan_kernel::Access::ReadWrite,
    );
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
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let mut watchers = AgentWatchers::start(tails, "/agent/root", tx);
    let outcome = catch_poll(async {
        skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
        app.input_intent = InputIntent::ForceAgent;
        app.composer.set_text_with_cursor("draft $", 7);
        app.refresh_completion();
        let before =
            [40, 60, 73, 80, 120].map(|width| completion_layout_tests::render(&app, width, 10).1);
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
        for (width, cursor) in [40, 60, 73, 80, 120].into_iter().zip(before) {
            let (rows, after) = completion_layout_tests::render(&app, width, 10);
            assert_eq!(after, cursor);
            assert!(
                rows.iter()
                    .skip(after.1 as usize + 1)
                    .any(|r| r.contains("next-only"))
            );
            assert!(
                rows.iter()
                    .take(after.1 as usize + 1)
                    .all(|r| !r.contains("next-only"))
            );
        }
        assert_eq!(app.composer.text(), "draft $");
        assert_eq!(app.composer.cursor(), 7);
        assert_eq!(app.input_intent, InputIntent::ForceAgent);
        let mut wrong = snapshot(&pid, 10, &["wrong-version"]);
        wrong.version += 1;
        let invalid = [
            serde_json::to_vec(&snapshot(&pid, 10, &["bad_id"])).unwrap(),
            serde_json::to_vec(&wrong).unwrap(),
            serde_json::to_vec(&snapshot("999999", 10, &["foreign"])).unwrap(),
            b"{broken".to_vec(),
            serde_json::to_vec(&snapshot(&pid, 10, &[]).unknown()).unwrap(),
            serde_json::to_vec(&snapshot(&pid, 11, &[])).unwrap(),
        ];
        for bytes in invalid {
            fault.descriptor_bytes(
                "/machine/ui/skills",
                serde_json::to_vec(&snapshot(&pid, 9, &["stale-choice"])).unwrap(),
            );
            app.skills = skills::SkillProjection::default();
            app.apply_skills(&owner, skills::read_skills(&shell, &owner).await);
            app.refresh_completion();
            assert!(app.completion.is_some());
            fault.descriptor_bytes("/machine/ui/skills", bytes);
            skills::dispatch_skill_event(
                &shell,
                &mut app,
                FileBackedEvent::SkillsChanged {
                    owner: owner.clone(),
                },
            )
            .await;
            no_stale_tab(&mut app);
        }
        let ids: Vec<String> = (0..2000).map(|i| format!("canonical-{i}")).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let mut bytes = serde_json::to_vec(&snapshot(&pid, 30, &refs)).unwrap();
        bytes.resize(1048576, b' ');
        fault.descriptor_bytes("/machine/ui/skills", bytes.clone());
        skills::dispatch_skill_event(
            &shell,
            &mut app,
            FileBackedEvent::SkillsChanged {
                owner: owner.clone(),
            },
        )
        .await;
        assert_eq!(
            app.completion_sources
                .skills
                .iter()
                .map(|c| &c.value)
                .collect::<Vec<_>>(),
            ids.iter().collect::<Vec<_>>()
        );
        bytes.push(b' ');
        fault.descriptor_bytes("/machine/ui/skills", bytes);
        skills::dispatch_skill_event(
            &shell,
            &mut app,
            FileBackedEvent::SkillsChanged {
                owner: owner.clone(),
            },
        )
        .await;
        no_stale_tab(&mut app);
        for text in ["/pro", "draft @"] {
            app.input_intent = InputIntent::Agent;
            app.composer.set_text(text);
            app.set_file_candidates(vec![CompletionCandidate::new("file-ref", None)]);
            app.refresh_completion();
            let before = format!("{:?}", app.completion);
            assert!(app.completion.is_some());
            skills::dispatch_skill_event(
                &shell,
                &mut app,
                FileBackedEvent::SkillsUnavailable {
                    owner: owner.clone(),
                },
            )
            .await;
            assert_eq!(format!("{:?}", app.completion), before);
            assert_eq!(app.composer.text(), text);
        }
        app.model
            .apply(&owner, Some(model_tests::snapshot(&pid, 1)));
        fault.descriptor_bytes(
            "/machine/ui/skills",
            serde_json::to_vec(&snapshot(&pid, 31, &["model-menu-skill"])).unwrap(),
        );
        skills::dispatch_skill_event(
            &shell,
            &mut app,
            FileBackedEvent::SkillsChanged {
                owner: owner.clone(),
            },
        )
        .await;
        assert!(!app.completion_sources.skills.is_empty());
        assert!(app.handle_command("/model").is_none());
        assert!(app.model_chooser.active);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let menu = app.completion.clone().expect("actual model completion");
        assert_eq!(
            menu.matches,
            vec![
                CompletionCandidate::new("B", Some("selected-next only".into())),
                CompletionCandidate::new("C", Some("selected-next only".into())),
            ]
        );
        assert_eq!(menu.selected, 1);
        assert_eq!(app.model_chooser.index, menu.selected);
        let index = app.model_chooser.index;
        let draft = app.composer.text().to_owned();
        let cursor = app.composer.cursor();
        let intent = app.input_intent;
        fault.descriptor_bytes("/machine/ui/skills", b"{broken".to_vec());
        skills::dispatch_skill_event(&shell, &mut app, FileBackedEvent::SkillsChanged { owner })
            .await;
        assert!(app.completion_sources.skills.is_empty());
        assert!(app.model_chooser.active);
        assert_eq!(app.completion.as_ref(), Some(&menu));
        assert_eq!(app.model_chooser.index, index);
        assert_eq!(app.composer.text(), draft);
        assert_eq!(app.composer.cursor(), cursor);
        assert_eq!(app.input_intent, intent);
    })
    .await;
    watchers.stop().await;
    assert!(watchers.recovery.is_none());
    assert!(!fault.open_paths().iter().any(|p| p.ends_with("/events")
        || p.ends_with("/machine/tape")
        || p.ends_with("/machine/ui/skills")));
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}
