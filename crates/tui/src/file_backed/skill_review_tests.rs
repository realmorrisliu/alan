use super::model_transport_tests::catch_poll;
use super::skill_tests::{skill_event, snapshot};
use super::*;
use alan_agent_protocol::InputIntent;
use std::sync::Arc;

fn identity(namespace: &alan_kernel::LiveNamespace, bytes: Vec<u8>) {
    namespace.replace_mount(
        stdio_tests::PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid", bytes,
        ))),
        alan_kernel::Access::ReadOnly,
    );
}

pub(super) fn no_stale_tab(app: &mut FileBackedApp) {
    let draft = app.composer.text().to_string();
    let cursor = app.composer.cursor();
    assert!(app.completion_sources.skills.is_empty());
    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert_eq!(app.composer.text(), draft);
    assert_eq!(app.composer.cursor(), cursor);
    assert_eq!(app.input_intent, InputIntent::ForceAgent);
}

#[tokio::test]
async fn skill_consumer_root_loss_rejects_old_watch_and_recovers_same_root() {
    for lost in [b"".to_vec(), b"not-a-pid\n".to_vec()] {
        let (shell, _, namespace, pid) = stdio_tests::live_root_agent().await;
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
        let mut watchers = AgentWatchers::start(tails, "/agent/root", tx.clone());
        let mut observation_jobs = tokio::task::JoinSet::new();
        let skill_reads =
            observation_io::start(shell.clone(), false, tx.clone(), &mut observation_jobs);
        let outcome = catch_poll(async {
            skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
            app.composer.set_text_with_cursor("draft $", 7);
            app.input_intent = InputIntent::ForceAgent;
            app.refresh_completion();
            assert!(app.completion.is_some());
            identity(&namespace, lost);
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
            no_stale_tab(&mut app);
            shell
                .write(
                    &path,
                    &serde_json::to_vec(&snapshot(&pid, 2, &["stale-restored"])).unwrap(),
                )
                .await
                .unwrap();
            skills::dispatch_skill_event(&shell, &mut app, skill_event(&mut rx).await).await;
            no_stale_tab(&mut app);
            identity(&namespace, format!("{pid}\n").into_bytes());
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
            assert_eq!(app.skills.owner, owner);
            observation_io::request(&skill_reads, owner.clone());
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    match rx.recv().await.unwrap() {
                        FileBackedEvent::SkillsChanged { owner: observed }
                            if observed == app.skills.owner =>
                        {
                            observation_io::request(&skill_reads, observed);
                        }
                        FileBackedEvent::ObservationRead {
                            revision,
                            owner,
                            snapshot,
                        } => {
                            observation_io::apply(
                                &mut app,
                                &skill_reads,
                                revision,
                                &owner,
                                snapshot,
                            );
                            if !app.completion_sources.skills.is_empty() {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            })
            .await
            .expect("repinned Skill descriptor must hydrate through actual observation worker");
            assert_eq!(app.completion_sources.skills[0].value, "stale-restored");
            assert_eq!(app.composer.text(), "draft $");
            assert_eq!(app.composer.cursor(), 7);
            assert_eq!(app.input_intent, InputIntent::ForceAgent);
        })
        .await;
        watchers.stop().await;
        drop(skill_reads);
        if outcome.is_err() {
            observation_jobs.abort_all();
        }
        while let Some(joined) = observation_jobs.join_next().await {
            if outcome.is_ok() {
                joined.unwrap();
            }
        }
        assert!(watchers.recovery.is_none());
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
    }
}
