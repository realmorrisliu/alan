use super::directory_selection::{SelectionAdapter, SelectionAuthority};
use super::*;
use crate::policy::TrustPosture;
use crate::tools::{ToolExecutionBinding, ToolProcessRunner, ToolRegistry};
use alan_agent_protocol::{
    Submission, UiActivitySnapshot, UiActivityState, UiNoticeKind, UiNoticeSnapshot,
};
use std::time::Duration;

fn build_stores(root: &std::path::Path) -> crate::AgentRuntimeStoreBindings {
    crate::AgentRuntimeStoreBindings {
        rollouts: root.join("rollouts"),
        checkpoints: root.join("checkpoints"),
        cache: root.join("cache"),
        tmp: root.join("tmp"),
        metadata: root.join("metadata"),
    }
}

async fn submit_text(handle: &crate::runtime::RuntimeHandle, text: &str) {
    handle
        .submission_tx
        .send(Submission::new(Op::Input {
            parts: vec![ContentPart::text(text)],
            mode: InputMode::FollowUp,
        }))
        .await
        .unwrap();
}

async fn wait_for_notice_containing(
    shell: &alan_shell::Shell,
    agent_path: &str,
    substring: &str,
) -> UiNoticeSnapshot {
    let notice_path = format!("{agent_path}/machine/ui/notice");
    let events_path = format!("{agent_path}/machine/ui/events");
    let mut last_raw = String::new();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = shell.cat(&notice_path).await {
                last_raw = String::from_utf8_lossy(&bytes).into_owned();
                if let Ok(snapshot) = serde_json::from_slice::<UiNoticeSnapshot>(&bytes)
                    && snapshot.message.contains(substring)
                {
                    return snapshot;
                }
            }
            if let Ok(event_bytes) = shell.cat(&events_path).await {
                for line in String::from_utf8_lossy(&event_bytes).lines() {
                    if let Ok(alan_agent_protocol::UiEvent::Notice { snapshot }) =
                        serde_json::from_str::<alan_agent_protocol::UiEvent>(line)
                        && snapshot.message.contains(substring)
                    {
                        return snapshot;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await;
    match result {
        Ok(snapshot) => snapshot,
        Err(_) => {
            let events = shell
                .cat(&events_path)
                .await
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            panic!(
                "timed out waiting for notice containing `{substring}`; last notice={last_raw}; events={events}"
            )
        }
    }
}

async fn wait_for_idle(shell: &alan_shell::Shell, agent_path: &str) {
    let activity_path = format!("{agent_path}/machine/ui/activity");
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = shell.cat(&activity_path).await
                && let Ok(snapshot) = serde_json::from_slice::<UiActivitySnapshot>(&bytes)
                && snapshot.state == UiActivityState::Idle
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .expect("timed out waiting for idle activity");
}

#[tokio::test]
async fn reset_session_via_machine_ctl_unpauses_clears_tape_and_rotates_rollout() {
    let temp = TempDir::new().unwrap();
    let stores = build_stores(temp.path());
    std::fs::create_dir_all(&stores.rollouts).unwrap();

    // Seed a rollout with pending queued work so runtime boots PAUSED with stale queued items.
    let source =
        AgentMachine::new_with_recorder_in_dir("/agent/old", "test-model", &stores.rollouts)
            .await
            .unwrap();
    let initial_rollout_path = source.rollout_path().unwrap().clone();
    let stale = Submission::new(Op::Input {
        parts: vec![ContentPart::text("stale queued turn")],
        mode: InputMode::FollowUp,
    });
    crate::agent_machine::input_queue::admit_input(
        &source.input_queue(),
        source.input_recorder().as_ref(),
        &stale,
    )
    .await
    .unwrap();
    source.input_recorder().unwrap().close().await.unwrap();

    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let runner = ToolProcessRunner::from_registry(&ToolRegistry::new());
    runner.register_process_binding(
        2,
        ToolExecutionBinding::awaiting_host_projection("/mnt/old".into(), temp.path().into())
            .with_adapter(Arc::new(SelectionAdapter("/mnt/old".into()))),
    );
    runner.register_process_authority(2, Arc::new(SelectionAuthority));
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default")
        .with_namespace_cwd("/mnt/old")
        .with_tool_process_context(2, runner);

    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores.clone()),
            recovery_rollout_path: Some(initial_rollout_path.clone()),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();

    let paused: UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
    assert_eq!(paused.state, UiActivityState::Paused);
    assert!(mock.recorded_requests().is_empty());

    shell
        .write("/agent/2/machine/ctl", b"reset-session")
        .await
        .unwrap();

    let notice = wait_for_notice_containing(&shell, "/agent/2", "Started a fresh session.").await;
    assert_eq!(notice.kind, UiNoticeKind::Warning);
    wait_for_idle(&shell, "/agent/2").await;

    // A new turn must run immediately without /continue and without stale history.
    submit_text(&runtime.handle, "fresh turn").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().is_empty() {
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .unwrap();

    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 1);
    let user_texts: Vec<&str> = requests[0]
        .messages
        .iter()
        .filter(|msg| msg.role == alan_llm::MessageRole::User)
        .map(|msg| msg.content.as_str())
        .collect();
    assert_eq!(user_texts, vec!["fresh turn"]);

    let rollout_files = std::fs::read_dir(&stores.rollouts)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect::<Vec<_>>();
    assert!(
        rollout_files.len() >= 2 && rollout_files.iter().any(|p| p != &initial_rollout_path),
        "reset-session must create a distinct rollout file: {rollout_files:?}"
    );

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn rollback_n_via_machine_ctl_removes_requested_turns() {
    let temp = TempDir::new().unwrap();
    let stores = build_stores(temp.path());
    std::fs::create_dir_all(&stores.rollouts).unwrap();

    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let agentfs = Arc::new(alan_agentfs::AgentFs::new());
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(agentfs),
        alan_kernel::Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs),
        alan_kernel::Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let runner = ToolProcessRunner::from_registry(&ToolRegistry::new());
    runner.register_process_binding(
        2,
        ToolExecutionBinding::awaiting_host_projection("/mnt/old".into(), temp.path().into())
            .with_adapter(Arc::new(SelectionAdapter("/mnt/old".into()))),
    );
    runner.register_process_authority(2, Arc::new(SelectionAuthority));
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default")
        .with_namespace_cwd("/mnt/old")
        .with_tool_process_context(2, runner);

    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();

    for (idx, prompt) in ["turn one", "turn two", "turn three"].iter().enumerate() {
        submit_text(&runtime.handle, prompt).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while mock.recorded_requests().len() <= idx {
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        })
        .await
        .unwrap();
        wait_for_idle(&shell, "/agent/2").await;
    }

    shell
        .write("/agent/2/machine/ctl", b"rollback 2")
        .await
        .unwrap();
    let notice = wait_for_notice_containing(&shell, "/agent/2", "rolled back 2 turns").await;
    assert_eq!(notice.kind, UiNoticeKind::Rollback);

    submit_text(&runtime.handle, "turn after rollback").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().len() < 4 {
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    })
    .await
    .unwrap();

    let last = mock.recorded_requests().pop().unwrap();
    let user_texts: Vec<&str> = last
        .messages
        .iter()
        .filter(|msg| msg.role == alan_llm::MessageRole::User)
        .map(|msg| msg.content.as_str())
        .collect();
    assert_eq!(user_texts, vec!["turn one", "turn after rollback"]);

    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn trust_posture_toggle_switches_only_between_governed_and_confined_trusted() {
    for (pid, initial_posture, verb, expected_posture, expected_notice) in [
        (
            2,
            TrustPosture::Governed,
            b"trust-posture confined-trusted".as_slice(),
            TrustPosture::ConfinedTrusted,
            "ConfinedTrusted",
        ),
        (
            2,
            TrustPosture::ConfinedTrusted,
            b"trust-posture governed".as_slice(),
            TrustPosture::Governed,
            "Governed",
        ),
        (
            3,
            TrustPosture::HostAccountTrusted,
            b"trust-posture governed".as_slice(),
            TrustPosture::HostAccountTrusted,
            "cannot be modified",
        ),
    ] {
        let temp = TempDir::new().unwrap();
        let stores = build_stores(temp.path());
        std::fs::create_dir_all(&stores.rollouts).unwrap();

        let mock = MockLlmProvider::new();
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(mock));
        let agentfs = Arc::new(alan_agentfs::AgentFs::new());
        let agent_path = format!("/agent/{pid}");
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            &agent_path,
            InProcessTransport::new(agentfs),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let runner = ToolProcessRunner::from_registry(&ToolRegistry::new());
        runner.register_process_binding(
            pid,
            ToolExecutionBinding::awaiting_host_projection("/mnt/old".into(), temp.path().into())
                .with_adapter(Arc::new(SelectionAdapter("/mnt/old".into()))),
        );
        runner.register_process_authority(pid, Arc::new(SelectionAuthority));
        let environment = NamespaceRuntimeEnvironment::new(root, &agent_path, "default")
            .with_namespace_cwd("/mnt/old")
            .with_tool_process_context(pid, runner.clone())
            .with_trust_posture(initial_posture);

        let mut core = crate::Config::for_openai_chat_completions_compatible(
            "sk-test",
            None,
            Some("test-model"),
        );
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let capabilities = crate::provider_capabilities_for_config(&core);
        let mut runtime = spawn_with_namespace_environment(
            AgentProcessConfig {
                agent_config: crate::AgentConfig::from(core),
                store_bindings: Some(stores),
                trust_posture: initial_posture,
                ..Default::default()
            },
            environment,
            crate::skills::SkillHostCapabilities::default(),
            capabilities,
        )
        .unwrap();
        runtime.wait_until_ready().await.unwrap();

        assert_eq!(runner.process_trust_posture(pid), Some(initial_posture));
        shell
            .write(&format!("{agent_path}/machine/ctl"), verb)
            .await
            .unwrap();
        let notice = wait_for_notice_containing(&shell, &agent_path, expected_notice).await;
        assert_eq!(notice.kind, UiNoticeKind::Warning);
        assert_eq!(runner.process_trust_posture(pid), Some(expected_posture));

        runtime.shutdown().await.unwrap();
    }
}
