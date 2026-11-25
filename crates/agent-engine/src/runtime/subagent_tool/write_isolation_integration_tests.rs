use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use alan_agent_protocol::{Event, SpawnHandle, SpawnMountAccess, SpawnTarget};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_memfs::MemFs;
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use crate::runtime::async_workstreams::{AsyncWorkstreamRegistry, default_async_child_launch_fn};
use crate::runtime::child_runs::ChildRunRegistry;
use crate::runtime::delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus};
use crate::runtime::delegated_skill_tool::{DelegatedChildRuntimeInputs, DelegatedSkillRuntime};
use crate::runtime::memory_proposals::try_read_ap_file;
use crate::runtime::memory_surfaces::project_summary::inspect_project_summary;
use crate::runtime::project_startup::ProjectWorkstreamStoreRef;
use crate::runtime::project_workstreams::bind_project_dir;
use crate::runtime::prompt_cache::PromptAssemblyCache;
use crate::runtime::transition::{NamespaceAgentFiles, NamespaceHostMountRequests};
use crate::runtime::write_isolation::{GitWorktreeIsolationPort, WorkspaceIsolationPort};
use crate::skills::SkillHostCapabilities;
use crate::tools::{
    ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding, ToolProcessRunner,
    ToolRegistry,
};

use super::{
    SubagentSpawnFn, commit_executed_slot, execute_prepared_slot_with_async,
    handle_spawn_subagent_with_spawn, prepare_subagent_slot,
};

#[derive(Debug)]
struct StaticAuthority(Arc<Mutex<Vec<PathBuf>>>);

impl ToolExecutionAuthority for StaticAuthority {
    fn reconcile(
        &self,
        _pid: u64,
        binding: ToolExecutionBinding,
    ) -> anyhow::Result<ToolExecutionBinding> {
        Ok(binding)
    }
    fn authority_revision(&self, _pid: u64) -> Option<u64> {
        Some(1)
    }
    fn project_knowledge_roots(&self, _pid: u64) -> Vec<PathBuf> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Debug)]
struct StaticAdapter(PathBuf);

impl ToolExecutionAdapter for StaticAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        PathBuf::from("/mnt/project")
    }
    fn cwd(&self) -> anyhow::Result<PathBuf> {
        Ok(self.0.clone())
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

struct FullHarness {
    machine: AgentMachine,
    prompt_cache: PromptAssemblyCache,
    agent_files: NamespaceAgentFiles,
    host_mount_requests: NamespaceHostMountRequests,
    inputs: DelegatedChildRuntimeInputs,
    async_workstreams: AsyncWorkstreamRegistry,
    child_run_registry: ChildRunRegistry,
    root_transport: InProcessTransport,
}

fn build_isolation_harness(
    repo_dir: &Path,
    store: ProjectWorkstreamStoreRef,
    memory_dir: PathBuf,
    isolation_port: Arc<dyn WorkspaceIsolationPort>,
) -> FullHarness {
    let agentfs = Arc::new(AgentFs::new());
    let memfs = Arc::new(MemFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/mem",
        InProcessTransport::new(memfs as Arc<dyn alan_ap::FileServer>),
        Access::ReadWrite,
    );
    ns.mount(
        "/mnt/host-mount",
        InProcessTransport::new(crate::runtime::test_host_mount::TestHostMountFs::new()),
        Access::ReadWrite,
    );
    let grant_record = serde_json::to_vec(&json!({
        "grant_id": "repo-rw",
        "active": true,
        "access": "read_write",
        "namespace_path": "/mnt/project",
    }))
    .unwrap();
    let grant_fs =
        alan_ap::reference::MemFs::with_read_only_files([("record".to_string(), grant_record)]);
    ns.mount(
        "/mnt/host-mount/grants/repo-rw",
        InProcessTransport::new(Arc::new(grant_fs)),
        Access::ReadOnly,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));

    let registry = ToolRegistry::new();
    let runner = ToolProcessRunner::from_registry(&registry);
    let roots = Arc::new(Mutex::new(vec![PathBuf::from("/mnt/project")]));
    runner.register_process_authority(1, Arc::new(StaticAuthority(roots)));
    runner.register_process_binding(
        1,
        ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/project"),
            repo_dir.to_path_buf(),
        )
        .with_adapter(Arc::new(StaticAdapter(repo_dir.to_path_buf()))),
    );

    let env = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_namespace_cwd("/mnt/project")
        .with_tool_process_context(1, runner)
        .with_workspace_isolation_port(isolation_port);

    let agent_files = env.agent_files();
    let host_mount_requests = env.host_mount_requests();
    let async_workstreams = env.async_workstreams().clone();
    let child_run_registry = env.child_run_registry().clone();
    let root_transport = env.root_transport();
    let inputs = DelegatedChildRuntimeInputs::new(
        env.child_launch(),
        env.tool_execution(),
        child_run_registry.clone(),
        "/proc/1".to_string(),
    )
    .with_workstream_store(Some(store))
    .with_memory_store_backing(Some(memory_dir))
    .with_workspace_isolation(env.workspace_isolation().cloned());

    let mut prompt_cache = PromptAssemblyCache::new(Vec::new());
    prompt_cache.set_host_capabilities(
        SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    FullHarness {
        machine: AgentMachine::new(),
        prompt_cache,
        agent_files,
        host_mount_requests,
        inputs,
        async_workstreams,
        child_run_registry,
        root_transport,
    }
}

fn runtime_of(h: &mut FullHarness) -> DelegatedSkillRuntime<'_> {
    DelegatedSkillRuntime::new(
        &mut h.machine,
        &mut h.prompt_cache,
        h.agent_files.clone(),
        h.host_mount_requests.clone(),
        h.inputs.clone(),
    )
}

fn last_payload(machine: &AgentMachine, call_id: &str) -> serde_json::Value {
    let raw = machine
        .prompt_view()
        .messages
        .iter()
        .find_map(|m| match m {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|r| r.id == call_id)
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("tool response");
    serde_json::from_str(&raw).unwrap()
}

#[tokio::test]
async fn parallel_read_only_runs_concurrently_while_shared_queue_serializes_in_fifo_order() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let projects_root = temp.path().join("projects");
    let binding = bind_project_dir(&projects_root, &repo_dir, Some("repo-rw".to_string())).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root,
        project_key: binding.project_key,
        grant_ref: Some("repo-rw".to_string()),
    };
    let port: Arc<dyn WorkspaceIsolationPort> = Arc::new(GitWorktreeIsolationPort::new(
        temp.path().join("wt"),
        temp.path().join("evidence"),
    ));
    let mut harness = build_isolation_harness(&repo_dir, store, temp.path().join("mem"), port);

    // 1. Two read_only slots overlap concurrently.
    let ro_calls = [
        NormalizedToolCall {
            id: "ro-1".to_string(),
            name: "spawn_subagent".to_string(),
            arguments: json!({"role": "reviewer", "task": "Read 1", "write_mode": "read_only"}),
        },
        NormalizedToolCall {
            id: "ro-2".to_string(),
            name: "spawn_subagent".to_string(),
            arguments: json!({"role": "reviewer", "task": "Read 2", "write_mode": "read_only"}),
        },
    ];
    let mut rt = runtime_of(&mut harness);
    let slot1 = prepare_subagent_slot(&mut rt, true, 0, &ro_calls[0], &ro_calls[0].arguments)
        .await
        .unwrap();
    let slot2 = prepare_subagent_slot(&mut rt, true, 1, &ro_calls[1], &ro_calls[1].arguments)
        .await
        .unwrap();

    let ro_active = Arc::new(AtomicUsize::new(0));
    let ro_peak = Arc::new(AtomicUsize::new(0));
    let ro_spawn: SubagentSpawnFn = {
        let ro_active = Arc::clone(&ro_active);
        let ro_peak = Arc::clone(&ro_peak);
        Arc::new(move |_runtime, spec, _resume, _cancel, _emit| {
            let ro_active = Arc::clone(&ro_active);
            let ro_peak = Arc::clone(&ro_peak);
            Box::pin(async move {
                assert!(spec.runtime_overrides.read_only);
                let cur = ro_active.fetch_add(1, Ordering::SeqCst) + 1;
                ro_peak.fetch_max(cur, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(40)).await;
                ro_active.fetch_sub(1, Ordering::SeqCst);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "/proc/90".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: "read ok".to_string(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        })
    };

    let cancel = CancellationToken::new();
    let (e1, e2) = tokio::join!(
        execute_prepared_slot_with_async(
            slot1,
            harness.agent_files.clone(),
            harness.async_workstreams.clone(),
            None,
            harness.child_run_registry.clone(),
            cancel.clone(),
            Arc::clone(&ro_spawn),
            default_async_child_launch_fn(),
        ),
        execute_prepared_slot_with_async(
            slot2,
            harness.agent_files.clone(),
            harness.async_workstreams.clone(),
            None,
            harness.child_run_registry.clone(),
            cancel.clone(),
            ro_spawn,
            default_async_child_launch_fn(),
        ),
    );
    assert_eq!(
        ro_peak.load(Ordering::SeqCst),
        2,
        "read_only slots must run concurrently"
    );

    let mut emit = |_e: Event| async {};
    let mut rt = runtime_of(&mut harness);
    commit_executed_slot(&mut rt, e1, &cancel, &cancel, &mut emit)
        .await
        .unwrap();
    commit_executed_slot(&mut rt, e2, &cancel, &cancel, &mut emit)
        .await
        .unwrap();
    assert_eq!(
        last_payload(&harness.machine, "ro-1")["write_mode"],
        "read_only"
    );

    // 2. Two shared_queue writers for the same grant serialize strictly in FIFO order.
    let sq_calls = [
        NormalizedToolCall {
            id: "sq-1".to_string(),
            name: "spawn_subagent".to_string(),
            arguments: json!({
                "role": "writer-1",
                "task": "Write 1",
                "write_mode": "shared_queue",
                "host_mounts": [{"grant": "repo-rw", "target": "/mnt/project", "access": "read_write"}]
            }),
        },
        NormalizedToolCall {
            id: "sq-2".to_string(),
            name: "spawn_subagent".to_string(),
            arguments: json!({
                "role": "writer-2",
                "task": "Write 2",
                "write_mode": "shared_queue",
                "host_mounts": [{"grant": "repo-rw", "target": "/mnt/project", "access": "read_write"}]
            }),
        },
    ];
    let mut rt = runtime_of(&mut harness);
    let sq1 = prepare_subagent_slot(&mut rt, true, 0, &sq_calls[0], &sq_calls[0].arguments)
        .await
        .unwrap();
    let sq2 = prepare_subagent_slot(&mut rt, true, 1, &sq_calls[1], &sq_calls[1].arguments)
        .await
        .unwrap();

    let sq_active = Arc::new(AtomicUsize::new(0));
    let sq_peak = Arc::new(AtomicUsize::new(0));
    let sq_order = Arc::new(Mutex::new(Vec::<String>::new()));
    let sq_spawn: SubagentSpawnFn = {
        let sq_active = Arc::clone(&sq_active);
        let sq_peak = Arc::clone(&sq_peak);
        let sq_order = Arc::clone(&sq_order);
        Arc::new(move |_runtime, spec, _resume, _cancel, _emit| {
            let sq_active = Arc::clone(&sq_active);
            let sq_peak = Arc::clone(&sq_peak);
            let sq_order = Arc::clone(&sq_order);
            Box::pin(async move {
                let role = match spec.target {
                    SpawnTarget::AdHocSubagent { role } => role,
                    _ => String::new(),
                };
                let cur = sq_active.fetch_add(1, Ordering::SeqCst) + 1;
                sq_peak.fetch_max(cur, Ordering::SeqCst);
                sq_order.lock().unwrap().push(role);
                tokio::time::sleep(Duration::from_millis(25)).await;
                sq_active.fetch_sub(1, Ordering::SeqCst);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "/proc/91".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: "write ok".to_string(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        })
    };
    let (se1, se2) = tokio::join!(
        execute_prepared_slot_with_async(
            sq1,
            harness.agent_files.clone(),
            harness.async_workstreams.clone(),
            None,
            harness.child_run_registry.clone(),
            cancel.clone(),
            Arc::clone(&sq_spawn),
            default_async_child_launch_fn(),
        ),
        execute_prepared_slot_with_async(
            sq2,
            harness.agent_files.clone(),
            harness.async_workstreams.clone(),
            None,
            harness.child_run_registry.clone(),
            cancel.clone(),
            sq_spawn,
            default_async_child_launch_fn(),
        ),
    );
    assert_eq!(
        sq_peak.load(Ordering::SeqCst),
        1,
        "shared_queue writers must never overlap"
    );
    assert_eq!(*sq_order.lock().unwrap(), vec!["writer-1", "writer-2"]);
    let mut rt = runtime_of(&mut harness);
    commit_executed_slot(&mut rt, se1, &cancel, &cancel, &mut emit)
        .await
        .unwrap();
    commit_executed_slot(&mut rt, se2, &cancel, &cancel, &mut emit)
        .await
        .unwrap();
}

#[tokio::test]
async fn worktree_isolation_narrows_mounts_emits_merge_handoff_and_commits_ro_child_memory_proposals()
 {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@alan.dev"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Alan Test"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();
    fs::write(repo_dir.join("README.md"), "# Alan\n").unwrap();
    Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    let projects_root = temp.path().join("projects");
    let binding = bind_project_dir(&projects_root, &repo_dir, Some("repo-rw".to_string())).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root,
        project_key: binding.project_key,
        grant_ref: Some("repo-rw".to_string()),
    };
    let memory_dir = temp.path().join("memory-store");
    let git_port = Arc::new(GitWorktreeIsolationPort::new(
        temp.path().join("wt"),
        temp.path().join("evidence"),
    ));
    git_port.register_grant("repo-rw", &repo_dir, "/mnt/project");
    let port: Arc<dyn WorkspaceIsolationPort> = git_port.clone();
    let mut harness = build_isolation_harness(&repo_dir, store, memory_dir.clone(), port);

    let child_spawn: SubagentSpawnFn = {
        let git_port = Arc::clone(&git_port);
        Arc::new(move |_runtime, spec, _resume, _cancel, _emit| {
            let git_port = Arc::clone(&git_port);
            Box::pin(async move {
                assert!(spec.has_handle(SpawnHandle::Memory));
                assert_eq!(spec.host_mounts.len(), 1);
                let isolated_ns = spec.host_mounts[0].target.clone();
                assert!(
                    isolated_ns.starts_with("/mnt/worktrees/"),
                    "expected isolated namespace target under /mnt/worktrees/, got {}",
                    isolated_ns.display()
                );
                assert_eq!(spec.host_mounts[0].access, SpawnMountAccess::ReadWrite);
                assert_eq!(spec.launch.cwd.as_deref(), Some(isolated_ns.as_path()));
                let isolated_host = git_port
                    .host_worktree_path(&spec.host_mounts[0].grant)
                    .expect("registered host worktree path");
                fs::write(isolated_host.join("README.md"), "# Alan\nworktree change\n").unwrap();

                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "/proc/92".to_string(),
                    child_run_id: Some("run-wt-92".to_string()),
                    rollout_path: None,
                    output_text: "isolated edits done".to_string(),
                    turn_summary: None,
                    structured_output: Some(json!({
                        "memory_proposals": [{
                            "kind": "convention",
                            "summary": "Isolate long-running stream edits inside GitWorktreeIsolationPort",
                            "provenance_rollout": "rollout://wt-92"
                        }]
                    })),
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        })
    };

    let call = NormalizedToolCall {
        id: "call_wt".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Edit README cleanly in isolated worktree",
            "stream_name": "wt-feature",
            "persistence": "persistent",
            "write_mode": "worktree",
            "long_running": true,
            "read_memory": true,
            "host_mounts": [{"grant": "repo-rw", "target": "/mnt/project", "access": "read_write"}]
        }),
    };
    let cancel = CancellationToken::new();
    let mut emit = |_e: Event| async {};
    handle_spawn_subagent_with_spawn(
        runtime_of(&mut harness),
        true,
        &call,
        &call.arguments,
        &cancel,
        &cancel,
        &mut emit,
        child_spawn,
    )
    .await
    .unwrap();

    assert_eq!(
        fs::read_to_string(repo_dir.join("README.md")).unwrap(),
        "# Alan\n"
    );

    let payload = last_payload(&harness.machine, "call_wt");
    assert_eq!(payload["write_mode"], "worktree", "payload={payload}");
    assert_eq!(
        payload["memory_proposals_commit"]["committed"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "payload={payload}"
    );
    let handoff = payload["merge_handoff"]
        .as_object()
        .expect("merge handoff present");
    assert_eq!(handoff["workstream_name"], "wt-feature");
    assert!(
        handoff["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f == "README.md")
    );
    assert!(
        handoff["patch_evidence_ref"]
            .as_str()
            .unwrap()
            .ends_with(".patch")
    );

    let mem_bytes = try_read_ap_file(&harness.root_transport, "/mnt/mem/project-memory.md")
        .await
        .unwrap()
        .expect("project-memory.md written via Root AP transport");
    let mem_text = String::from_utf8(mem_bytes).unwrap();
    assert!(mem_text.contains("GitWorktreeIsolationPort"));
    assert!(mem_text.contains("rollout://wt-92"));

    let summary_view = inspect_project_summary(&memory_dir, Some("/mnt/project")).unwrap();
    assert!(summary_view.exists);
}
