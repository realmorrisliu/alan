use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alan_agent_protocol::Event;
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use serde_json::json;
use tempfile::TempDir;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use crate::runtime::child_runs::ChildRunRegistry;
use crate::runtime::delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus};
use crate::runtime::delegated_skill_tool::{DelegatedChildRuntimeInputs, DelegatedSkillRuntime};
use crate::runtime::project_startup::ProjectWorkstreamStoreRef;
use crate::runtime::project_workstreams::{
    WorkstreamPersistence, WorkstreamState, bind_project_dir, load_workstream_record,
};
use crate::runtime::prompt_cache::PromptAssemblyCache;
use crate::runtime::transition::{
    NamespaceAgentFiles, NamespaceHostMountRequests, NamespaceProcessFiles,
};
use crate::skills::SkillHostCapabilities;
use crate::tools::{ToolExecutionAdapter, ToolExecutionBinding, ToolProcessRunner, ToolRegistry};

use super::super::subagent_tool::{
    PreparedChildKind, SubagentSpawnRequest, handle_spawn_subagent_with_async_launch,
    parse_subagent_spawn_request, spawn_subagent_tool_definition,
};
use super::{
    AsyncChildLaunchFn, AsyncWorkstreamRegistration, AsyncWorkstreamRegistry,
    AsyncWorkstreamStatus, LaunchedAsyncChild,
};

#[derive(Debug)]
struct FakeAdapter;

impl ToolExecutionAdapter for FakeAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        PathBuf::from("/mnt/project")
    }
    fn cwd(&self) -> anyhow::Result<PathBuf> {
        Ok(PathBuf::from("/tmp"))
    }
    fn resolve_path(&self, _ns: &Path, p: &Path) -> anyhow::Result<PathBuf> {
        Ok(p.to_path_buf())
    }
    fn visible_path(&self, host_path: &Path) -> PathBuf {
        host_path.to_path_buf()
    }
    fn project_text(&self, text: &str) -> String {
        text.to_string()
    }
    fn sandbox(&self) -> anyhow::Result<crate::tools::Sandbox> {
        anyhow::bail!("unused")
    }
}

struct TestHarness {
    machine: AgentMachine,
    prompt_cache: PromptAssemblyCache,
    agent_files: NamespaceAgentFiles,
    host_mount_requests: NamespaceHostMountRequests,
    inputs: DelegatedChildRuntimeInputs,
    async_workstreams: AsyncWorkstreamRegistry,
}

fn synthetic_child_files(pid: &str) -> (NamespaceAgentFiles, NamespaceProcessFiles) {
    let ns = Namespace::new();
    let transport = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let env = crate::runtime::NamespaceRuntimeEnvironment::new(
        transport,
        format!("/agent/{pid}"),
        "default",
    );
    (env.agent_files(), env.process_files())
}

fn build_harness(store: Option<ProjectWorkstreamStoreRef>) -> TestHarness {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/host-mount",
        InProcessTransport::new(crate::runtime::test_host_mount::TestHostMountFs::new()),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));

    let registry = ToolRegistry::new();
    let runner = ToolProcessRunner::from_registry(&registry);
    runner.register_process_binding(
        1,
        ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/project"),
            std::env::temp_dir(),
        )
        .with_adapter(Arc::new(FakeAdapter)),
    );

    let env = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_namespace_cwd("/mnt/project")
        .with_tool_process_context(1, runner);
    let agent_files = env.agent_files();
    let host_mount_requests = env.host_mount_requests();
    let async_workstreams = env.async_workstreams().clone();

    let inputs = DelegatedChildRuntimeInputs::new(
        env.child_launch(),
        env.tool_execution(),
        env.child_run_registry().clone(),
        "/proc/1".to_string(),
    )
    .with_workstream_store(store)
    .with_async_workstreams(async_workstreams.clone());

    let mut prompt_cache = PromptAssemblyCache::new(Vec::new());
    prompt_cache.set_host_capabilities(
        SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    TestHarness {
        machine: AgentMachine::new(),
        prompt_cache,
        agent_files,
        host_mount_requests,
        inputs,
        async_workstreams,
    }
}

fn runtime_from_harness(h: &mut TestHarness) -> DelegatedSkillRuntime<'_> {
    DelegatedSkillRuntime::new(
        &mut h.machine,
        &mut h.prompt_cache,
        h.agent_files.clone(),
        h.host_mount_requests.clone(),
        h.inputs.clone(),
    )
}

fn last_tool_payload(machine: &AgentMachine, call_id: &str) -> serde_json::Value {
    let raw = machine
        .prompt_view()
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == call_id)
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected tool response");
    serde_json::from_str(&raw).expect("tool response json")
}

fn synthetic_registration(
    registry: &AsyncWorkstreamRegistry,
    parent_agent_files: NamespaceAgentFiles,
    pid: &str,
    run_id: &str,
    stream_name: Option<&str>,
    gate: Arc<Notify>,
) -> AsyncWorkstreamRegistration {
    let (child_agent_files, child_process_files) = synthetic_child_files(pid);
    let cancel = CancellationToken::new();
    let cancel_for_fut = cancel.clone();
    let process_path = format!("/proc/{pid}");
    let run_id_string = run_id.to_string();
    let pid_string = pid.to_string();
    let kind = PreparedChildKind::AdHocSubagent {
        request: SubagentSpawnRequest {
            role: "researcher".to_string(),
            task: "Inspect crate".to_string(),
            model: None,
            read_only: false,
            cwd: None,
            host_mounts: Vec::new(),
            timeout_secs: None,
            stream_name: stream_name.map(str::to_string),
            persistence: Some(WorkstreamPersistence::Persistent),
            scope: Some("read-only scan".to_string()),
            background: true,
            write_mode: None,
            long_running: false,
            read_memory: false,
        },
        persisted_arguments: json!({"role": "researcher"}),
        workstream: None,
        write_isolation: None,
    };
    let _ = registry;
    AsyncWorkstreamRegistration {
        stream_name: stream_name.map(str::to_string),
        role: "researcher".to_string(),
        scope: Some("read-only scan".to_string()),
        persistence: WorkstreamPersistence::Persistent,
        action_id: None,
        kind,
        label: "researcher".to_string(),
        tracked: None,
        rollouts_dir: None,
        parent_agent_files,
        child_run_registry: ChildRunRegistry::default(),
        child_cancel: cancel,
        write_guard: None,
        launched: LaunchedAsyncChild {
            child_pid: pid_string.clone(),
            child_run_id: run_id_string.clone(),
            process_path: process_path.clone(),
            agent_path: format!("/agent/{pid_string}"),
            child_agent_files,
            child_process_files,
            completion: Box::pin(async move {
                tokio::select! {
                    _ = gate.notified() => Ok(ChildRuntimeResult {
                        status: ChildRuntimeStatus::Completed,
                        process_path,
                        child_run_id: Some(run_id_string),
                        rollout_path: None,
                        output_text: "complete".to_string(),
                        turn_summary: Some("complete".to_string()),
                        structured_output: None,
                        warnings: Vec::new(),
                        error_message: None,
                        pause: None,
                        child_run: None,
                    }),
                    _ = cancel_for_fut.cancelled() => Ok(ChildRuntimeResult::cancelled_before_launch()),
                }
            }),
        },
    }
}

#[test]
fn spawn_subagent_schema_and_parser_include_background_flag() {
    let def = spawn_subagent_tool_definition();
    assert_eq!(
        def.parameters["properties"]["background"]["type"],
        "boolean"
    );

    let sync_req = parse_subagent_spawn_request(&json!({
        "role": "researcher",
        "task": "Read module"
    }))
    .unwrap();
    assert!(!sync_req.background);

    let async_req = parse_subagent_spawn_request(&json!({
        "role": "researcher",
        "task": "Read module",
        "stream_name": "docs-scan",
        "persistence": "persistent",
        "background": true
    }))
    .unwrap();
    assert!(async_req.background);
    assert_eq!(async_req.stream_name.as_deref(), Some("docs-scan"));
    assert_eq!(
        async_req.persistence,
        Some(WorkstreamPersistence::Persistent)
    );
}

#[tokio::test]
async fn registry_tracks_active_rejects_duplicate_stream_and_cancels() {
    let harness = build_harness(None);
    let registry = harness.async_workstreams.clone();
    let gate = Arc::new(Notify::new());

    let info = registry
        .register_and_spawn(synthetic_registration(
            &registry,
            harness.agent_files.clone(),
            "2",
            "run-2",
            Some("docs-check"),
            gate.clone(),
        ))
        .unwrap();
    assert_eq!(info.state, AsyncWorkstreamStatus::Running);
    assert!(registry.is_stream_active("docs-check"));
    assert_eq!(registry.active_count(), 1);

    let duplicate = registry.register_and_spawn(synthetic_registration(
        &registry,
        harness.agent_files.clone(),
        "3",
        "run-3",
        Some("docs-check"),
        gate.clone(),
    ));
    assert!(duplicate.is_err());

    assert!(registry.cancel_stream("docs-check"));
    tokio::time::timeout(Duration::from_secs(2), async {
        while registry.active_count() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled stream did not terminate");

    let listed = registry.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].state, AsyncWorkstreamStatus::Cancelled);
}

#[tokio::test]
async fn registry_quiesce_for_reset_keeps_open_while_shutdown_closes_permanently() {
    let harness = build_harness(None);
    let registry = harness.async_workstreams.clone();

    let gate_1 = Arc::new(Notify::new());
    registry
        .register_and_spawn(synthetic_registration(
            &registry,
            harness.agent_files.clone(),
            "4",
            "run-4",
            Some("stream-a"),
            gate_1,
        ))
        .unwrap();

    registry
        .quiesce_for_reset(Duration::from_secs(2))
        .await
        .unwrap();
    assert!(!registry.is_closed());
    assert_eq!(registry.active_count(), 0);

    let gate_2 = Arc::new(Notify::new());
    registry
        .register_and_spawn(synthetic_registration(
            &registry,
            harness.agent_files.clone(),
            "5",
            "run-5",
            Some("stream-b"),
            gate_2,
        ))
        .unwrap();
    assert_eq!(registry.active_count(), 1);

    registry
        .quiesce_and_flush_all(Duration::from_secs(2))
        .await
        .unwrap();
    assert!(registry.is_closed());
    assert_eq!(registry.active_count(), 0);

    let after_close = registry.register_and_spawn(synthetic_registration(
        &registry,
        harness.agent_files,
        "6",
        "run-6",
        Some("stream-c"),
        Arc::new(Notify::new()),
    ));
    assert!(after_close.is_err());
}

#[tokio::test]
async fn async_persistent_workstream_returns_immediately_and_transitions_on_completion() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root: projects_root.clone(),
        project_key: binding.project_key.clone(),
        grant_ref: None,
    };

    let mut harness = build_harness(Some(store));
    let gate = Arc::new(Notify::new());
    let gate_for_spawn = gate.clone();
    let rollout_ckpt = temp.path().join("rollout-auth-async.jsonl");
    let rollout_clone = rollout_ckpt.clone();

    let launch_fn: AsyncChildLaunchFn = Arc::new(
        move |_runtime, _spec, cancel, _resume, _tracker| {
            let gate = gate_for_spawn.clone();
            let rollout = rollout_clone.clone();
            let (child_agent_files, child_process_files) = synthetic_child_files("2");
            Box::pin(async move {
                Ok(LaunchedAsyncChild {
                    child_pid: "2".to_string(),
                    child_run_id: "run-auth-async".to_string(),
                    process_path: "/proc/2".to_string(),
                    agent_path: "/agent/2".to_string(),
                    child_agent_files,
                    child_process_files,
                    completion: Box::pin(async move {
                        tokio::select! {
                            _ = gate.notified() => Ok(ChildRuntimeResult {
                                status: ChildRuntimeStatus::Completed,
                                process_path: "/proc/2".to_string(),
                                child_run_id: Some("run-auth-async".to_string()),
                                rollout_path: Some(rollout),
                                output_text: "auth scan complete".to_string(),
                                turn_summary: Some("auth scan complete".to_string()),
                                structured_output: None,
                                warnings: Vec::new(),
                                error_message: None,
                                pause: None,
                                child_run: None,
                            }),
                            _ = cancel.cancelled() => Ok(ChildRuntimeResult::cancelled_before_launch()),
                        }
                    }),
                })
            })
        },
    );

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let call = NormalizedToolCall {
        id: "call_async_1".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "researcher",
            "task": "Analyze auth module",
            "stream_name": "auth",
            "persistence": "persistent",
            "scope": "Audit auth token flow",
            "background": true
        }),
    };
    {
        let runtime = runtime_from_harness(&mut harness);
        handle_spawn_subagent_with_async_launch(
            runtime,
            true,
            &call,
            &call.arguments,
            &cancel,
            &cancel,
            &mut emit,
            launch_fn.clone(),
        )
        .await
        .unwrap();
    }

    let payload = last_tool_payload(&harness.machine, "call_async_1");
    assert_eq!(payload["status"], "running");
    assert_eq!(payload["background"], true);
    assert_eq!(payload["child_pid"], "2");
    assert_eq!(payload["child_process_path"], "/proc/2");
    assert_eq!(payload["child_agent_path"], "/agent/2");
    assert_eq!(payload["stream_name"], "auth");
    assert_eq!(payload["persistence"], "persistent");
    assert_eq!(payload["workstream_state"], "running");

    assert!(harness.async_workstreams.is_stream_active("auth"));
    let running_record = load_workstream_record(&projects_root, &binding.project_key, "auth")
        .unwrap()
        .unwrap();
    assert_eq!(running_record.state, WorkstreamState::Running);

    // Concurrent second dispatch on the same active stream must be rejected without touching child.
    let dup_call = NormalizedToolCall {
        id: "call_async_dup".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "researcher",
            "task": "Duplicate",
            "stream_name": "auth",
            "persistence": "persistent",
            "background": true
        }),
    };
    {
        let runtime = runtime_from_harness(&mut harness);
        handle_spawn_subagent_with_async_launch(
            runtime,
            true,
            &dup_call,
            &dup_call.arguments,
            &cancel,
            &cancel,
            &mut emit,
            launch_fn,
        )
        .await
        .unwrap();
    }
    let dup_payload = last_tool_payload(&harness.machine, "call_async_dup");
    assert_eq!(dup_payload["error_kind"], "workstream_running");
    assert_eq!(
        dup_payload["error"],
        "Workstream 'auth' is already running."
    );

    // Release background worker and wait for ledger transition to Idle + checkpoint ref.
    gate.notify_one();
    tokio::time::timeout(Duration::from_secs(2), async {
        while harness.async_workstreams.active_count() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("background workstream did not settle");

    let idle_record = load_workstream_record(&projects_root, &binding.project_key, "auth")
        .unwrap()
        .unwrap();
    assert_eq!(idle_record.state, WorkstreamState::Idle);
    assert_eq!(
        idle_record.last_checkpoint_ref.as_deref(),
        Some(rollout_ckpt.to_string_lossy().as_ref())
    );
}

#[tokio::test]
async fn async_one_off_archives_and_shutdown_quiesce_flushes_running_persistent_stream() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root: projects_root.clone(),
        project_key: binding.project_key.clone(),
        grant_ref: None,
    };

    let mut harness = build_harness(Some(store));
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let oneoff_launch: AsyncChildLaunchFn =
        Arc::new(move |_runtime, _spec, _cancel, _resume, _tracker| {
            let (child_agent_files, child_process_files) = synthetic_child_files("10");
            Box::pin(async move {
                Ok(LaunchedAsyncChild {
                    child_pid: "10".to_string(),
                    child_run_id: "run-oneoff-async".to_string(),
                    process_path: "/proc/10".to_string(),
                    agent_path: "/agent/10".to_string(),
                    child_agent_files,
                    child_process_files,
                    completion: Box::pin(async move {
                        Ok(ChildRuntimeResult {
                            status: ChildRuntimeStatus::Completed,
                            process_path: "/proc/10".to_string(),
                            child_run_id: Some("run-oneoff-async".to_string()),
                            rollout_path: None,
                            output_text: "oneoff done".to_string(),
                            turn_summary: None,
                            structured_output: None,
                            warnings: Vec::new(),
                            error_message: None,
                            pause: None,
                            child_run: None,
                        })
                    }),
                })
            })
        });

    let call_oneoff = NormalizedToolCall {
        id: "call_oneoff".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "checker",
            "task": "Quick lint",
            "stream_name": "quick-lint",
            "persistence": "one_off",
            "background": true
        }),
    };
    {
        let runtime = runtime_from_harness(&mut harness);
        handle_spawn_subagent_with_async_launch(
            runtime,
            true,
            &call_oneoff,
            &call_oneoff.arguments,
            &cancel,
            &cancel,
            &mut emit,
            oneoff_launch,
        )
        .await
        .unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while harness.async_workstreams.active_count() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("one_off async workstream did not archive");
    let oneoff_record = load_workstream_record(&projects_root, &binding.project_key, "quick-lint")
        .unwrap()
        .unwrap();
    assert_eq!(oneoff_record.state, WorkstreamState::Archived);

    // Dispatch a persistent async stream blocked in flight, then shutdown via quiesce_and_flush_all.
    let persistent_ckpt = temp.path().join("persistent-flushed.jsonl");
    let ckpt_clone = persistent_ckpt.clone();
    let gate = Arc::new(Notify::new());
    let gate_clone = gate.clone();
    let persistent_launch: AsyncChildLaunchFn = Arc::new(
        move |_runtime, _spec, cancel, _resume, _tracker| {
            let ckpt = ckpt_clone.clone();
            let gate = gate_clone.clone();
            let (child_agent_files, child_process_files) = synthetic_child_files("11");
            Box::pin(async move {
                Ok(LaunchedAsyncChild {
                    child_pid: "11".to_string(),
                    child_run_id: "run-long-async".to_string(),
                    process_path: "/proc/11".to_string(),
                    agent_path: "/agent/11".to_string(),
                    child_agent_files,
                    child_process_files,
                    completion: Box::pin(async move {
                        tokio::select! {
                            _ = gate.notified() => Ok(ChildRuntimeResult::cancelled_before_launch()),
                            _ = cancel.cancelled() => Ok(ChildRuntimeResult {
                                status: ChildRuntimeStatus::Cancelled,
                                process_path: "/proc/11".to_string(),
                                child_run_id: Some("run-long-async".to_string()),
                                rollout_path: Some(ckpt),
                                output_text: String::new(),
                                turn_summary: Some("flushed on exit".to_string()),
                                structured_output: None,
                                warnings: Vec::new(),
                                error_message: None,
                                pause: None,
                                child_run: None,
                            }),
                        }
                    }),
                })
            })
        },
    );

    let call_long = NormalizedToolCall {
        id: "call_long".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "builder",
            "task": "Long work",
            "stream_name": "long-run",
            "persistence": "persistent",
            "background": true
        }),
    };
    {
        let runtime = runtime_from_harness(&mut harness);
        handle_spawn_subagent_with_async_launch(
            runtime,
            true,
            &call_long,
            &call_long.arguments,
            &cancel,
            &cancel,
            &mut emit,
            persistent_launch,
        )
        .await
        .unwrap();
    }

    harness
        .async_workstreams
        .quiesce_and_flush_all(Duration::from_secs(2))
        .await
        .unwrap();

    let flushed_record = load_workstream_record(&projects_root, &binding.project_key, "long-run")
        .unwrap()
        .unwrap();
    assert_eq!(flushed_record.state, WorkstreamState::Idle);
    assert_eq!(
        flushed_record.last_checkpoint_ref.as_deref(),
        Some(persistent_ckpt.to_string_lossy().as_ref())
    );
}
