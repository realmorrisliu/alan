use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Notify;

#[derive(Debug, Default)]
struct HeldGeneration {
    entered: Notify,
    release: Notify,
    calls: AtomicUsize,
    settled: AtomicBool,
}

#[derive(Debug)]
struct Provider {
    gate: Arc<HeldGeneration>,
    inner: MockLlmProvider,
    hold_call: usize,
}

struct HeldCall(Arc<HeldGeneration>);

impl Drop for HeldCall {
    fn drop(&mut self) {
        self.0.settled.store(true, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl LlmProvider for Provider {
    async fn generate_stream(
        &mut self,
        request: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        if self.gate.calls.fetch_add(1, Ordering::SeqCst) == self.hold_call {
            let _held = HeldCall(self.gate.clone());
            self.gate.entered.notify_one();
            self.gate.release.notified().await;
        }
        self.inner.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

async fn spawn_child(shell: &alan_shell::Shell, root: Pid) -> String {
    let descriptors: Value = serde_json::from_slice(
        &shell
            .cat(&format!("/proc/{}/descriptors", root.0))
            .await
            .unwrap(),
    )
    .unwrap();
    let descriptor = alan_agent_engine::AGENT_DEFINITION_FD.to_string();
    let namespace_path = format!("/proc/{}/namespace", root.0);
    let namespace = String::from_utf8(shell.cat(&namespace_path).await.unwrap()).unwrap();
    let mounts: Vec<_> = namespace
        .lines()
        .filter_map(|line| {
            let (path, access) = line.rsplit_once(' ').unwrap();
            (!path.starts_with("/memory")).then(|| json!({"path":path,"access":access}))
        })
        .collect();
    let generation = shell.stat(&namespace_path).await.unwrap().qid.version;
    let request = json!({
        "initial_task":"a child must not obtain a fresh parent allowance",
        "spawn":{
            "target":{"kind":"definition_descriptor","descriptor":"agent-definition"},
            "launch":{"task":"check the parent budget"}, "handles":[]
        }
    });
    let exec = json!({
        "executable":"/bin/alan-agent", "args":[request.to_string()],
        "descriptors":{(descriptor.clone()):descriptors[&descriptor]},
        "namespace":{"generation":generation,"mounts":mounts}
    })
    .to_string();
    shell
        .spawn(&exec)
        .await
        .unwrap_or_else(|error| panic!("{error:?}: {exec}"))
        .trim()
        .to_string()
}

async fn child_output(shell: &alan_shell::Shell, pid: &str) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if shell.cat(&format!("/proc/{pid}/status")).await.unwrap() == b"exited\n" {
                return shell.cat(&format!("/proc/{pid}/io/output")).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child must stop before another provider request")
}

#[tokio::test]
async fn agent_executable_cannot_start_without_or_escape_its_parent_task_budget() {
    let gate = Arc::new(HeldGeneration::default());
    let mut process = AgentProcessConfig::default();
    process.agent_config.core_config.memory.enabled = false;
    process.agent_config.core_config.task_budget.model_calls =
        std::num::NonZeroU64::new(1).unwrap();
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        process,
        ProcessLaunchContext::root().with_descriptor(
            alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
            alan_agent_engine::ProcessDescriptor::with_file_tree(
                "/lib/agents/root",
                alan_agent_engine::ProcessFileTree::new(std::collections::BTreeMap::from([(
                    "agent.toml".into(),
                    Vec::new(),
                )]))
                .unwrap(),
            )
            .unwrap(),
        ),
        LlmClient::new(Provider {
            gate: gate.clone(),
            inner: MockLlmProvider::new(),
            hold_call: 0,
        }),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let shell = alan_shell::Shell::new(manager.root_namespace.clone());
    let pid = spawn_child(&shell, manager.root_pid()).await;
    let output = String::from_utf8(child_output(&shell, &pid).await).unwrap();
    assert!(
        output.contains("parent Agent Process has no admitted task budget"),
        "{output}"
    );
    assert_eq!(gate.calls.load(Ordering::SeqCst), 0);

    shell
        .write(
            "/agent/root/io/input",
            b"hold the final admitted parent model call",
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
    let pid = spawn_child(&shell, manager.root_pid()).await;
    let output = child_output(&shell, &pid).await;
    gate.release.notify_one();
    let result = alan_agent_engine::AgentExecutableResult::from_process_output(&output).unwrap();
    assert_eq!(
        result.status,
        alan_agent_engine::AgentExecutableStatus::Failed
    );
    assert!(result.error_message.unwrap().contains("Parent task budget"));
    assert!(
        result.pause.is_none(),
        "child-local allowance cannot extend its parent"
    );
    assert_eq!(gate.calls.load(Ordering::SeqCst), 1);
    manager.shutdown().await.unwrap();
}

pub(super) fn admitted_parent_fixture() -> (MockLlmProvider, ToolRegistry, Arc<Notify>, Arc<Notify>)
{
    struct HoldTool(Arc<Notify>, Arc<Notify>);
    impl alan_agent_engine::tools::Tool for HoldTool {
        fn name(&self) -> &str {
            "hold_parent_task"
        }
        fn description(&self) -> &str {
            "Keep the parent task active during child lifecycle acceptance"
        }
        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }
        fn execute(
            &self,
            _: Value,
            _: &alan_agent_engine::tools::ToolContext,
        ) -> alan_agent_engine::tools::ToolResult {
            let entered = self.0.clone();
            let release = self.1.clone();
            Box::pin(async move {
                entered.notify_one();
                release.notified().await;
                Ok(json!({"done":true}))
            })
        }
    }
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut tools = ToolRegistry::new();
    tools.register(HoldTool(entered.clone(), release.clone()));
    let provider = MockLlmProvider::new().with_responses(vec![alan_llm::GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: vec![],
        tool_calls: vec![alan_llm::ToolCall {
            id: Some("hold-parent".into()),
            name: "hold_parent_task".into(),
            arguments: json!({}),
        }],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: vec![],
    }]);
    (provider, tools, entered, release)
}

#[tokio::test]
async fn parent_process_abort_stops_inflight_child_without_a_fresh_allowance() {
    for via_ctl in [false, true] {
        let gate = Arc::new(HeldGeneration::default());
        let (provider, tools, entered, release) = admitted_parent_fixture();
        let temp = tempfile::tempdir().unwrap();
        let stores = alan_agent_engine::AgentRuntimeStoreBindings {
            rollouts: temp.path().join("rollouts"),
            metadata: temp.path().join("metadata"),
            checkpoints: temp.path().join("checkpoints"),
            cache: temp.path().join("cache"),
            tmp: temp.path().join("tmp"),
        };
        std::fs::create_dir_all(&stores.tmp).unwrap();
        let mut process = AgentProcessConfig {
            store_bindings: Some(stores),
            ..Default::default()
        };
        process.agent_config.core_config.memory.enabled = false;
        let launch = ProcessLaunchContext::root().with_descriptor(
            alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
            alan_agent_engine::ProcessDescriptor::with_file_tree(
                "/lib/agents/root",
                alan_agent_engine::ProcessFileTree::new(BTreeMap::new()).unwrap(),
            )
            .unwrap(),
        );
        let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
            process,
            launch,
            LlmClient::new(Provider {
                gate: gate.clone(),
                inner: provider,
                hold_call: 1,
            }),
            tools,
        ))
        .await
        .unwrap();
        let parent = manager.root_pid();
        let shell = alan_shell::Shell::new(manager.root_namespace.clone());
        shell
            .write("/agent/root/io/input", b"hold parent task")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        let child = spawn_child(&shell, parent).await;
        tokio::time::timeout(Duration::from_secs(5), gate.entered.notified())
            .await
            .unwrap();
        if via_ctl {
            shell
                .write(&format!("/proc/{}/ctl", parent.0), b"cancel")
                .await
                .unwrap();
        } else {
            manager.terminate_unit("root-agent", 137).await.unwrap();
        }
        let stopped =
            tokio::time::timeout(Duration::from_secs(2), child_output(&shell, &child)).await;
        let settled = gate.settled.load(Ordering::SeqCst);
        let calls = gate.calls.load(Ordering::SeqCst);
        gate.release.notify_one();
        release.notify_one();
        let new_parent = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let pid = manager.root_pid();
                if pid != Pid(0) && pid != parent {
                    break pid;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            gate.calls.load(Ordering::SeqCst),
            2,
            "Root replacement must not replay old work"
        );
        assert_eq!(
            manager.procfs.try_observe_process_lifecycle(new_parent),
            Some((Status::Running, None))
        );
        manager.shutdown().await.unwrap();
        let output = stopped.expect("an aborted parent must stop the child's admitted model work");
        assert!(settled, "child Process completion must await model cleanup");
        assert_eq!(
            calls, 2,
            "no replacement task or model retry may be admitted"
        );
        let result =
            alan_agent_engine::AgentExecutableResult::from_process_output(&output).unwrap();
        assert_eq!(
            result.status,
            alan_agent_engine::AgentExecutableStatus::Failed
        );
        assert!(
            result.pause.is_none(),
            "a dead ancestor cannot receive child-local extension"
        );
    }
}
