//! Real file-control selection while reliable recovered work stays paused.
use super::*;
use crate::tools::{
    ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding, ToolProcessRunner,
    ToolRegistry,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) struct SelectionAdapter(pub(super) PathBuf);
impl ToolExecutionAdapter for SelectionAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.0.clone()
    }
    fn cwd(&self) -> anyhow::Result<PathBuf> {
        Ok("/tmp".into())
    }
    fn resolve_path(&self, cwd: &Path, path: &Path) -> anyhow::Result<PathBuf> {
        Ok(cwd.join(path))
    }
    fn resolve_directory(&self, _: &Path, path: &Path) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(path == Path::new("/mnt/new"), "directory unavailable");
        Ok(path.into())
    }
    fn visible_path(&self, path: &Path) -> PathBuf {
        path.into()
    }
    fn project_text(&self, text: &str) -> String {
        text.into()
    }
    fn sandbox(&self) -> anyhow::Result<crate::tools::Sandbox> {
        Ok(crate::tools::Sandbox::new("/tmp".into()))
    }
}
#[derive(Debug)]
pub(super) struct SelectionAuthority;
impl ToolExecutionAuthority for SelectionAuthority {
    fn reconcile(
        &self,
        _: u64,
        mut binding: ToolExecutionBinding,
    ) -> anyhow::Result<ToolExecutionBinding> {
        anyhow::ensure!(
            binding.namespace_cwd == Path::new("/mnt/new"),
            "old grant revoked"
        );
        binding.cwd_grant_id = Some("replacement".into());
        Ok(binding.with_adapter(Arc::new(SelectionAdapter("/mnt/new".into()))))
    }
}

#[tokio::test]
async fn file_directory_selection_preserves_recovered_paused_work_until_continue() {
    directory_selection_preserves_recovered_paused_work_until_continue(false).await;
}

#[tokio::test]
async fn api_directory_selection_preserves_recovered_paused_work_until_continue() {
    directory_selection_preserves_recovered_paused_work_until_continue(true).await;
}

async fn directory_selection_preserves_recovered_paused_work_until_continue(api: bool) {
    let temp = TempDir::new().unwrap();
    let stores = crate::AgentRuntimeStoreBindings {
        rollouts: temp.path().join("rollouts"),
        checkpoints: temp.path().join("checkpoints"),
        cache: temp.path().join("cache"),
        tmp: temp.path().join("tmp"),
        metadata: temp.path().join("metadata"),
    };
    let source =
        AgentMachine::new_with_recorder_in_dir("/agent/old", "test-model", &stores.rollouts)
            .await
            .unwrap();
    let path = source.rollout_path().unwrap().clone();
    let mut inputs: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|text| {
            Submission::new(Op::Input {
                parts: vec![ContentPart::text(text)],
                mode: InputMode::FollowUp,
            })
        })
        .collect();
    for input in &inputs {
        crate::agent_machine::input_queue::admit_input(
            &source.input_queue(),
            source.input_recorder().as_ref(),
            input,
        )
        .await
        .unwrap();
    }
    source.input_recorder().unwrap().close().await.unwrap();
    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
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
        .with_tool_process_context(2, runner.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            store_bindings: Some(stores),
            recovery_rollout_path: Some(path),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    assert_eq!(
        runner.process_binding(2).unwrap().namespace_cwd,
        PathBuf::from("/mnt/old")
    );
    let id = uuid::Uuid::new_v4().to_string();
    if api {
        // Queue both records before startup yields to the event loop, so the
        // selector must pass admit_api_before_dispatch, not recv's idle path.
        let ordinary = Submission::new(Op::Input {
            parts: vec![ContentPart::text("third")],
            mode: InputMode::FollowUp,
        });
        runtime
            .handle
            .submission_tx
            .try_send(ordinary.clone())
            .unwrap();
        inputs.push(ordinary);
        runtime
            .handle
            .submission_tx
            .try_send(Submission {
                id: id.clone(),
                intent: alan_agent_protocol::InputIntent::Agent,
                op: Op::SelectProjectDirectory {
                    path: "/mnt/new".into(),
                },
            })
            .unwrap();
    }
    let metadata = runtime.wait_until_ready().await.unwrap();
    if !api {
        assert!(
            shell
                .write(
                    "/agent/2/machine/ctl",
                    b"project-cwd-v1 {}\nqueue-v1 continue"
                )
                .await
                .is_err(),
            "one write cannot inject a second control"
        );
        let invalid_id = uuid::Uuid::new_v4().to_string();
        let invalid = format!(
            "project-cwd-v1 {}",
            serde_json::json!({"id":invalid_id,"path":"/mnt/../project"})
        );
        shell
            .write("/agent/2/machine/ctl", invalid.as_bytes())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            runner.process_binding(2).unwrap().namespace_cwd,
            PathBuf::from("/mnt/old")
        );
        let paused: alan_agent_protocol::UiActivitySnapshot =
            serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap())
                .unwrap();
        assert_eq!(paused.state, alan_agent_protocol::UiActivityState::Paused);
        assert!(mock.recorded_requests().is_empty());
        let history =
            crate::rollout::RolloutRecorder::load_history(metadata.rollout_path.as_ref().unwrap())
                .await
                .unwrap();
        let admitted: Vec<_> = history
            .iter()
            .filter_map(|item| match item {
                crate::rollout::RolloutItem::Event(event)
                    if event.event_type == "machine_input_admitted_v1" =>
                {
                    Some(event.payload["id"].as_str().unwrap().to_owned())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            admitted,
            inputs.iter().map(|s| s.id.clone()).collect::<Vec<_>>()
        );
        assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(event) if event.event_type == "machine_input_dispatched_v1" || event.event_type == "machine_input_removed_v1")));
        let receipt = shell.cat("/agent/2/actions/a0/result").await;
        assert!(
            receipt.is_ok(),
            "valid-id semantic rejection must publish a correlated failed Action: {receipt:?}"
        );
        let rejected_result: serde_json::Value = serde_json::from_slice(&receipt.unwrap()).unwrap();
        assert_eq!(rejected_result["call_id"], invalid_id);
        assert_eq!(rejected_result["exit_code"], 1);
        assert_eq!(rejected_result["outcome"]["success"], false);
        assert_eq!(
            rejected_result["outcome"]["error"],
            "invalid absolute namespace directory"
        );
        assert_eq!(shell.cat("/agent/2/actions/a0/name").await.unwrap(), b"cd");
        assert_eq!(
            shell.cat("/agent/2/actions/a0/status").await.unwrap(),
            b"failed"
        );
        let rejected_id = uuid::Uuid::new_v4().to_string();
        let rejected = format!(
            "project-cwd-v1 {}",
            serde_json::json!({"id":rejected_id,"path":"/mnt/missing"})
        );
        shell
            .write("/agent/2/machine/ctl", rejected.as_bytes())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            runner.process_binding(2).unwrap().namespace_cwd,
            PathBuf::from("/mnt/old")
        );
        let rejected_result: serde_json::Value =
            serde_json::from_slice(&shell.cat("/agent/2/actions/a1/result").await.unwrap())
                .unwrap();
        assert_eq!(rejected_result["call_id"], rejected_id);
        assert_eq!(rejected_result["exit_code"], 1);
        assert_eq!(rejected_result["outcome"]["success"], false);
        assert!(mock.recorded_requests().is_empty());
        let control = format!(
            "project-cwd-v1 {}",
            serde_json::json!({"id":id,"path":"/mnt/new"})
        );
        shell
            .write("/agent/2/machine/ctl", control.as_bytes())
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let selected = runner.process_binding(2).unwrap().namespace_cwd;
    let queue: alan_agent_protocol::UiQueueSnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/queue").await.unwrap()).unwrap();
    assert!(queue.known && queue.paused);
    assert_eq!(
        queue.pending_submission_ids,
        inputs
            .iter()
            .map(|input| input.id.clone())
            .collect::<Vec<_>>()
    );
    assert!(queue.active_submission_ids.is_empty());
    let paused: alan_agent_protocol::UiActivitySnapshot =
        serde_json::from_slice(&shell.cat("/agent/2/machine/ui/activity").await.unwrap()).unwrap();
    assert_eq!(paused.state, alan_agent_protocol::UiActivityState::Paused);
    assert!(mock.recorded_requests().is_empty());
    assert_eq!(
        selected,
        PathBuf::from("/mnt/new"),
        "selector must select replacement authority without continuing recovered work"
    );
    let receipt_path = if api {
        "/agent/2/actions/a0/result"
    } else {
        "/agent/2/actions/a2/result"
    };
    let result: serde_json::Value =
        serde_json::from_slice(&shell.cat(receipt_path).await.unwrap()).unwrap();
    assert_eq!(result["call_id"], id);
    assert_eq!(result["outcome"]["cwd"], "/mnt/new");
    assert_eq!(result["result_preview"], "cwd: /mnt/new");
    assert!(result.get("presentation").is_none());
    let history = crate::rollout::RolloutRecorder::load_history(&metadata.rollout_path.unwrap())
        .await
        .unwrap();
    let admitted: Vec<_> = history
        .iter()
        .filter_map(|item| match item {
            crate::rollout::RolloutItem::Event(event)
                if event.event_type == "machine_input_admitted_v1" =>
            {
                Some(event.payload["id"].as_str().unwrap().to_owned())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        admitted,
        inputs.iter().map(|s| s.id.clone()).collect::<Vec<_>>()
    );
    assert!(!history.iter().any(|item| matches!(item, crate::rollout::RolloutItem::Event(event) if event.event_type == "machine_input_dispatched_v1")));
    shell
        .write("/agent/2/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().len() < inputs.len() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    runtime.shutdown().await.unwrap();
}

#[tokio::test]
async fn file_directory_selection_rejects_running_work_without_delayed_selection() {
    let mock = MockLlmProvider::new();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        "default",
        Box::new(GatedFirstGeneration {
            mock: mock.clone(),
            started: started.clone(),
            release: release.clone(),
        }),
    );
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        "/agent/2",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
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
        ToolExecutionBinding::awaiting_host_projection("/mnt/old".into(), "/tmp".into())
            .with_adapter(Arc::new(SelectionAdapter("/mnt/old".into()))),
    );
    runner.register_process_authority(2, Arc::new(SelectionAuthority));
    let environment = NamespaceRuntimeEnvironment::new(root, "/agent/2", "default")
        .with_namespace_cwd("/mnt/old")
        .with_tool_process_context(2, runner.clone());
    let mut core =
        crate::Config::for_openai_chat_completions_compatible("sk-test", None, Some("test-model"));
    core.memory.enabled = false;
    core.streaming_mode = crate::config::StreamingMode::Off;
    let capabilities = crate::provider_capabilities_for_config(&core);
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core),
            ..Default::default()
        },
        environment,
        crate::skills::SkillHostCapabilities::default(),
        capabilities,
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();
    runtime
        .handle
        .submission_tx
        .send(Submission::new(Op::Turn {
            parts: vec![ContentPart::text("hold")],
            context: None,
        }))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), started.notified())
        .await
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let control = format!(
        "project-cwd-v1 {}",
        serde_json::json!({"id":id,"path":"/mnt/new"})
    );
    shell
        .write("/agent/2/machine/ctl", control.as_bytes())
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(bytes) = shell.cat("/agent/2/actions/a0/result").await {
                break serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result["call_id"], id);
    assert_eq!(result["exit_code"], 1);
    assert!(
        result["outcome"]["error"]
            .as_str()
            .unwrap()
            .contains("settled")
    );
    assert_eq!(
        runner.process_binding(2).unwrap().namespace_cwd,
        PathBuf::from("/mnt/old")
    );
    release.notify_one();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        runner.process_binding(2).unwrap().namespace_cwd,
        PathBuf::from("/mnt/old"),
        "rejected selection must not execute after generation settles"
    );
    assert_eq!(mock.recorded_requests().len(), 1);
    runtime.shutdown().await.unwrap();
}
