use std::sync::Arc;
use std::time::Duration;

use alan_agent_protocol::{Event, ToolCapability};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{
    Access, Credentials, ExecNamespaceManifest, ExecSpec, MountFs, Namespace, Pid, ProcFs,
    ProcessInvocation, ProcessOutcome, ProcessRunner,
};
use alan_llm::MockLlmProvider;
use alan_llmfs::LlmFs;
use alan_shell::Shell;
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;

use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use crate::config::Config;
use crate::runtime::{
    NamespaceRuntimeEnvironment, RuntimeConfig, ToolPackageManifest,
    loop_guard::ToolLoopGuard,
    prompt_cache::PromptAssemblyCache,
    tool_batch::{ToolBatchOrchestratorOutcome, ToolOrchestratorInputs},
    tool_packages::ToolExecutionHints,
    transition::{RuntimeLoopState, orchestrate_tool_batch_internal},
};

struct BarrierReadRunner {
    barrier: Arc<Barrier>,
}

#[async_trait::async_trait]
impl ProcessRunner for BarrierReadRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        let arg = invocation
            .exec
            .args
            .first()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .unwrap_or_default();
        let label = arg
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if invocation.exec.executable == "/bin/read_probe" {
            // Both read_probe calls must enter concurrently to pass the barrier.
            tokio::time::timeout(Duration::from_secs(2), self.barrier.wait())
                .await
                .expect("read-only tools must execute concurrently");
            // Intentionally finish the first call slower than the second to verify
            // deterministic ordering on actions/ and tape.
            if label == "first" {
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        }
        let body = serde_json::json!({ "success": true, "label": label }).to_string();
        ProcessOutcome::exited(0, format!("{body}\n").into_bytes())
    }
}

async fn parallel_batch_state(runner: Arc<dyn ProcessRunner>) -> (RuntimeLoopState, Shell) {
    let procfs = ProcFs::new();
    let agentfs = Arc::new(AgentFs::new());
    let llmfs = Arc::new(LlmFs::new());
    llmfs.register_connection("default", Box::new(MockLlmProvider::new()));

    let execution = ToolExecutionHints {
        arguments: "json_first_arg".to_string(),
        result: "stdout_json".to_string(),
    };
    let read_manifest = serde_json::to_vec(&ToolPackageManifest {
        version: 1,
        name: "read_probe".to_string(),
        description: "Pure read probe".to_string(),
        parameters: serde_json::json!({"type": "object"}),
        capability: ToolCapability::Read,
        capability_is_argument_dependent: false,
        timeout_secs: 5,
        execution: execution.clone(),
    })
    .unwrap();
    let write_manifest = serde_json::to_vec(&ToolPackageManifest {
        version: 1,
        name: "write_probe".to_string(),
        description: "Write probe".to_string(),
        parameters: serde_json::json!({"type": "object"}),
        capability: ToolCapability::Write,
        capability_is_argument_dependent: false,
        timeout_secs: 5,
        execution,
    })
    .unwrap();

    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs.clone()),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/llm",
        InProcessTransport::new(llmfs.clone()),
        Access::ReadWrite,
    );
    ns.mount(
        "/bin",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_files([
            ("read_probe".to_string(), Vec::new()),
            ("write_probe".to_string(), Vec::new()),
        ]))),
        Access::ReadOnly,
    );
    ns.mount(
        "/lib/exec/read_probe",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            read_manifest,
        ))),
        Access::ReadOnly,
    );
    ns.mount(
        "/lib/exec/write_probe",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "manifest",
            write_manifest,
        ))),
        Access::ReadOnly,
    );

    // Seed /proc/1 so child spawns have a valid parent.
    let credentials = Credentials::user("root-agent");
    let bootstrap = procfs.for_spawner(None, ns.clone(), credentials.clone());
    let fid = alan_ap::Fid(9999);
    alan_ap::FileServer::walk(&bootstrap, alan_ap::Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    alan_ap::FileServer::open(&bootstrap, fid, alan_ap::OpenMode::ReadWrite)
        .await
        .unwrap();
    let exec = ExecSpec {
        executable: "/bin/agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(&ns),
        descriptors: Default::default(),
    };
    alan_ap::FileServer::write(&bootstrap, fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    alan_ap::FileServer::clunk(&bootstrap, fid).await.unwrap();

    let spawner_procfs = Arc::new(procfs.with_runner(runner).for_spawner(
        Some(Pid(1)),
        ns.clone(),
        credentials,
    ));
    ns.mount(
        "/proc",
        InProcessTransport::new(spawner_procfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = Shell::new(root.clone());
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let core_config = Config::default();
    let runtime_config = RuntimeConfig::default();
    let state = RuntimeLoopState {
        machine: AgentMachine::new(),
        environment: env,
        core_config,
        runtime_config,
        prompt_cache: PromptAssemblyCache::new(Vec::new()),
    };
    (state, shell)
}

#[tokio::test]
async fn pure_read_only_batch_runs_concurrently_with_deterministic_tape_and_actions_order() {
    let barrier = Arc::new(Barrier::new(2));
    let (mut state, shell) = parallel_batch_state(Arc::new(BarrierReadRunner { barrier })).await;
    let cancel = CancellationToken::new();
    let writer = state.agent_files().begin_tape_generation().await.unwrap();
    let mut loop_guard = ToolLoopGuard::new(None, 0);
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let tool_calls = vec![
        NormalizedToolCall {
            id: "call_0".to_string(),
            name: "read_probe".to_string(),
            arguments: serde_json::json!({ "label": "first" }),
        },
        NormalizedToolCall {
            id: "call_1".to_string(),
            name: "read_probe".to_string(),
            arguments: serde_json::json!({ "label": "second" }),
        },
        NormalizedToolCall {
            id: "call_2".to_string(),
            name: "write_probe".to_string(),
            arguments: serde_json::json!({ "label": "third" }),
        },
    ];

    let outcome = orchestrate_tool_batch_internal(
        &mut loop_guard,
        &mut state,
        &tool_calls,
        ToolOrchestratorInputs {
            explicit_command: false,
            cancel: &cancel,
            steering_broker: None,
        },
        (None, None),
        &writer,
        &mut emit,
    )
    .await
    .unwrap();
    writer.finish().await.unwrap();

    assert!(matches!(
        outcome,
        ToolBatchOrchestratorOutcome::ContinueTurnLoop { .. }
    ));

    // Completed events must be strictly ordered call_0 -> call_1 -> call_2 even though
    // call_1 finished 40ms before call_0.
    let completed_ids: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            Event::ToolCallCompleted { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(completed_ids, vec!["call_0", "call_1", "call_2"]);

    // Tape tool messages must follow the exact batch order.
    let tape_ids: Vec<String> = state
        .machine
        .messages()
        .iter()
        .flat_map(|msg| msg.tool_responses().iter().map(|resp| resp.id.clone()))
        .collect();
    assert_eq!(tape_ids, vec!["call_0", "call_1", "call_2"]);

    // AgentFS actions/a0, a1, a2 must also be allocated in deterministic batch order.
    for (action_id, expected_call_id) in [("a0", "call_0"), ("a1", "call_1"), ("a2", "call_2")] {
        let result: serde_json::Value = serde_json::from_slice(
            &shell
                .cat(&format!("/agent/1/actions/{action_id}/result"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(result["call_id"], expected_call_id, "{action_id}: {result}");
    }
}
