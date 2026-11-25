use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use alan_agent_protocol::Event;
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use serde_json::json;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use crate::agent_machine::pending_resume::ResumeTarget;
use crate::agent_machine::task_budget::{Admission, TaskBudget, TaskBudgetLimits};
use crate::agent_machine::{AgentMachine, NormalizedToolCall};
use crate::runtime::delegated_child_run::{ChildRuntimeResult, ChildRuntimeStatus};
use crate::runtime::delegated_skill_tool::{DelegatedChildRuntimeInputs, DelegatedSkillRuntime};
use crate::runtime::project_startup::ProjectWorkstreamStoreRef;
use crate::runtime::project_workstreams::{
    WorkstreamPersistence, WorkstreamState, bind_project_dir, load_project_ledger,
    load_workstream_record,
};
use crate::runtime::prompt_cache::PromptAssemblyCache;
use crate::runtime::transition::{NamespaceAgentFiles, NamespaceHostMountRequests};
use crate::skills::SkillHostCapabilities;
use crate::tools::{
    ToolExecutionAdapter, ToolExecutionAuthority, ToolExecutionBinding, ToolProcessRunner,
    ToolRegistry,
};

use super::super::{
    SubagentSpawnFn, handle_spawn_subagent_with_spawn, parse_subagent_spawn_request,
    spawn_subagent_tool_definition,
};

#[derive(Debug)]
struct FakeAuthority {
    active_roots: Arc<Mutex<Vec<PathBuf>>>,
}

impl ToolExecutionAuthority for FakeAuthority {
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
        self.active_roots.lock().unwrap().clone()
    }
}

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
}

fn build_harness(
    store: Option<ProjectWorkstreamStoreRef>,
    grant_files: Option<Vec<(String, Vec<u8>)>>,
    authority_roots: Option<Arc<Mutex<Vec<PathBuf>>>>,
) -> TestHarness {
    let agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    if let Some(grants) = grant_files {
        for (grant_id, bytes) in grants {
            let grant_fs =
                alan_ap::reference::MemFs::with_read_only_files([("record".to_string(), bytes)]);
            ns.mount(
                &format!("/mnt/host-mount/grants/{grant_id}"),
                InProcessTransport::new(Arc::new(grant_fs)),
                Access::ReadOnly,
            );
        }
    } else {
        ns.mount(
            "/mnt/host-mount",
            InProcessTransport::new(crate::runtime::test_host_mount::TestHostMountFs::new()),
            Access::ReadWrite,
        );
    }
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));

    let registry = ToolRegistry::new();
    let runner = ToolProcessRunner::from_registry(&registry);
    if let Some(roots) = authority_roots {
        runner.register_process_authority(
            1,
            Arc::new(FakeAuthority {
                active_roots: roots,
            }),
        );
        runner.register_process_binding(
            1,
            ToolExecutionBinding::awaiting_host_projection(
                PathBuf::from("/mnt/project"),
                std::env::temp_dir(),
            )
            .with_adapter(Arc::new(FakeAdapter)),
        );
    }

    let env = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_namespace_cwd("/mnt/project")
        .with_tool_process_context(1, runner);
    let agent_files = env.agent_files();
    let host_mount_requests = env.host_mount_requests();
    let child_launch = env.child_launch();
    let tool_execution = env.tool_execution();
    let child_run_registry = env.child_run_registry().clone();

    let inputs = DelegatedChildRuntimeInputs::new(
        child_launch,
        tool_execution,
        child_run_registry,
        "/proc/1".to_string(),
    )
    .with_workstream_store(store);

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

#[test]
fn spawn_subagent_schema_and_parser_support_workstream_fields() {
    let def = spawn_subagent_tool_definition();
    assert_eq!(
        def.parameters["properties"]["stream_name"]["type"],
        "string"
    );
    assert_eq!(
        def.parameters["properties"]["persistence"]["enum"],
        json!(["persistent", "one_off"])
    );
    assert_eq!(def.parameters["properties"]["scope"]["type"], "string");

    let default_req = parse_subagent_spawn_request(&json!({
        "role": "researcher",
        "task": "Inspect crates"
    }))
    .unwrap();
    assert_eq!(default_req.stream_name, None);
    assert_eq!(default_req.persistence, None);
    assert_eq!(
        default_req.effective_persistence(),
        WorkstreamPersistence::OneOff
    );

    let persistent_req = parse_subagent_spawn_request(&json!({
        "role": "builder",
        "task": "Implement feature",
        "stream_name": "feature-auth",
        "persistence": "persistent",
        "scope": "Auth implementation"
    }))
    .unwrap();
    assert_eq!(persistent_req.stream_name.as_deref(), Some("feature-auth"));
    assert_eq!(
        persistent_req.persistence,
        Some(WorkstreamPersistence::Persistent)
    );
    assert_eq!(
        persistent_req.effective_persistence(),
        WorkstreamPersistence::Persistent
    );
    assert_eq!(persistent_req.scope.as_deref(), Some("Auth implementation"));

    assert!(
        parse_subagent_spawn_request(&json!({
            "role": "builder",
            "task": "x",
            "stream_name": "../escape"
        }))
        .is_none()
    );
    assert!(
        parse_subagent_spawn_request(&json!({
            "role": "builder",
            "task": "x",
            "persistence": "forever"
        }))
        .is_none()
    );
}

#[tokio::test]
async fn persistent_workstream_saves_checkpoint_and_resumes_with_revalidation() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, Some("grant-repo".to_string())).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root: projects_root.clone(),
        project_key: binding.project_key.clone(),
        grant_ref: binding.grant_ref.clone(),
    };

    let grant_files = vec![(
        "grant-repo".to_string(),
        serde_json::to_vec(&json!({
            "id": "grant-repo",
            "label": "repo",
            "namespace_path": "/mnt/project",
            "access": "read_write",
            "provenance": "test",
            "active": true
        }))
        .unwrap(),
    )];
    let authority_roots = Arc::new(Mutex::new(vec![PathBuf::from("/mnt/project")]));
    let mut harness = build_harness(Some(store), Some(grant_files), Some(authority_roots));

    let ckpt_1 = temp.path().join("rollout-1.jsonl");
    let ckpt_2 = temp.path().join("rollout-2.jsonl");
    std::fs::write(&ckpt_1, "{}\n").unwrap();
    std::fs::write(&ckpt_2, "{}\n").unwrap();

    let observed_resumes: Arc<Mutex<Vec<Option<ResumeTarget>>>> = Arc::new(Mutex::new(Vec::new()));
    let ckpt_1_clone = ckpt_1.clone();
    let ckpt_2_clone = ckpt_2.clone();
    let call_num = Arc::new(Mutex::new(0usize));
    let spawn_fn: SubagentSpawnFn = {
        let observed_resumes = Arc::clone(&observed_resumes);
        let call_num = Arc::clone(&call_num);
        Arc::new(move |_runtime, _spec, _cancel, resume, _tracker| {
            observed_resumes.lock().unwrap().push(resume);
            let mut n = call_num.lock().unwrap();
            *n += 1;
            let rollout_path = if *n == 1 {
                ckpt_1_clone.clone()
            } else {
                ckpt_2_clone.clone()
            };
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    process_path: "/proc/10".to_string(),
                    child_run_id: Some("run-1".to_string()),
                    rollout_path: Some(rollout_path),
                    output_text: "done".to_string(),
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
    let mut emit = |_event: Event| async {};

    let call_1 = NormalizedToolCall {
        id: "call_ws_1".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Start auth module",
            "stream_name": "auth",
            "persistence": "persistent",
            "scope": "Authentication module",
            "host_mounts": [{"grant": "grant-repo", "target": "/mnt/project", "access": "read_write"}]
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &call_1,
        &call_1.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_fn.clone(),
    )
    .await
    .unwrap();

    let payload_1 = last_tool_payload(&harness.machine, "call_ws_1");
    assert_eq!(payload_1["stream_name"], "auth");
    assert_eq!(payload_1["persistence"], "persistent");
    assert_eq!(payload_1["workstream_state"], "idle");
    assert_eq!(
        payload_1["last_checkpoint_ref"],
        ckpt_1.display().to_string()
    );

    let record_1 = load_workstream_record(&projects_root, &binding.project_key, "auth")
        .unwrap()
        .unwrap();
    assert_eq!(record_1.state, WorkstreamState::Idle);
    assert_eq!(
        record_1.last_checkpoint_ref.as_deref(),
        Some(ckpt_1.to_string_lossy().as_ref())
    );

    // Second dispatch resumes the idle persistent stream from ckpt_1
    let call_2 = NormalizedToolCall {
        id: "call_ws_2".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Continue auth tests",
            "stream_name": "auth",
            "persistence": "persistent",
            "host_mounts": [{"grant": "grant-repo", "target": "/mnt/project", "access": "read_write"}]
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &call_2,
        &call_2.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_fn,
    )
    .await
    .unwrap();

    let resumes = observed_resumes.lock().unwrap();
    assert!(resumes[0].is_none());
    let resumed_target = resumes[1].as_ref().expect("second run must resume ckpt_1");
    assert!(resumed_target.request_id.is_empty());
    assert_eq!(resumed_target.child_pid, None);
    assert_eq!(
        resumed_target.rollout_path.as_deref(),
        Some(ckpt_1.to_string_lossy().as_ref())
    );

    let record_2 = load_workstream_record(&projects_root, &binding.project_key, "auth")
        .unwrap()
        .unwrap();
    assert_eq!(record_2.state, WorkstreamState::Idle);
    assert_eq!(
        record_2.last_checkpoint_ref.as_deref(),
        Some(ckpt_2.to_string_lossy().as_ref())
    );
}

#[tokio::test]
async fn one_off_workstream_auto_collapses_to_archived_while_retaining_rollout_evidence() {
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

    let mut harness = build_harness(Some(store), None, None);
    let rollout_path = temp.path().join("oneoff-rollout.jsonl");
    let rollout_clone = rollout_path.clone();
    let spawn_fn: SubagentSpawnFn = Arc::new(move |_runtime, _spec, _cancel, _resume, _tracker| {
        let rollout_path = rollout_clone.clone();
        Box::pin(async move {
            Ok(ChildRuntimeResult {
                status: ChildRuntimeStatus::Completed,
                process_path: "/proc/11".to_string(),
                child_run_id: Some("run-oneoff".to_string()),
                rollout_path: Some(rollout_path),
                output_text: "lint ok".to_string(),
                turn_summary: None,
                structured_output: None,
                warnings: Vec::new(),
                error_message: None,
                pause: None,
                child_run: None,
            })
        })
    });

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let call = NormalizedToolCall {
        id: "call_oneoff".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "checker",
            "task": "Run lint check",
            "stream_name": "lint-pass"
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &call,
        &call.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_fn,
    )
    .await
    .unwrap();

    let payload = last_tool_payload(&harness.machine, "call_oneoff");
    assert_eq!(payload["stream_name"], "lint-pass");
    assert_eq!(payload["persistence"], "one_off");
    assert_eq!(payload["workstream_state"], "archived");
    assert_eq!(payload["rollout_ref"], rollout_path.display().to_string());

    let ledger = load_project_ledger(&projects_root, &binding.project_key).unwrap();
    assert!(ledger.active_streams().is_empty());
    assert_eq!(ledger.counts().archived, 1);
    let archived = load_workstream_record(&projects_root, &binding.project_key, "lint-pass")
        .unwrap()
        .unwrap();
    assert_eq!(archived.state, WorkstreamState::Archived);
    assert_eq!(
        archived.rollout_ref.as_deref(),
        Some(rollout_path.to_string_lossy().as_ref())
    );
}

#[tokio::test]
async fn revoked_grant_and_exhausted_budget_refuse_persistent_resume_prelaunch() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, Some("grant-repo".to_string())).unwrap();
    let store = ProjectWorkstreamStoreRef {
        projects_root: projects_root.clone(),
        project_key: binding.project_key.clone(),
        grant_ref: binding.grant_ref.clone(),
    };

    // Seed an idle persistent workstream with a checkpoint
    let mut seed = crate::runtime::project_workstreams::WorkstreamRecord::new(
        "auth",
        "Auth work",
        WorkstreamPersistence::Persistent,
    );
    seed.state = WorkstreamState::Idle;
    seed.last_checkpoint_ref = Some("/tmp/auth-ckpt.jsonl".to_string());
    crate::runtime::project_workstreams::upsert_workstream(
        &projects_root,
        &binding.project_key,
        seed,
    )
    .unwrap();

    // Case 1: /mnt/host-mount/grants marks grant active=false
    let revoked_files = vec![(
        "grant-repo".to_string(),
        serde_json::to_vec(&json!({
            "id": "grant-repo",
            "label": "repo",
            "namespace_path": "/mnt/project",
            "access": "read_write",
            "provenance": "test",
            "active": false
        }))
        .unwrap(),
    )];
    let mut harness_revoked_fs = build_harness(Some(store.clone()), Some(revoked_files), None);
    let spawned = Arc::new(Mutex::new(false));
    let spawned_flag = Arc::clone(&spawned);
    let spawn_never: SubagentSpawnFn = Arc::new(move |_, _, _, _, _| {
        *spawned_flag.lock().unwrap() = true;
        Box::pin(async { unreachable!("must not spawn on prelaunch rejection") })
    });

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let call_revoked = NormalizedToolCall {
        id: "call_revoked_fs".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Resume auth",
            "stream_name": "auth",
            "persistence": "persistent",
            "host_mounts": [{"grant": "grant-repo", "target": "/mnt/project", "access": "read_only"}]
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness_revoked_fs),
        true,
        &call_revoked,
        &call_revoked.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_never.clone(),
    )
    .await
    .unwrap();
    assert!(!*spawned.lock().unwrap());
    let payload_revoked = last_tool_payload(&harness_revoked_fs.machine, "call_revoked_fs");
    assert_eq!(payload_revoked["error_kind"], "grant_revoked");
    assert_eq!(
        load_workstream_record(&projects_root, &binding.project_key, "auth")
            .unwrap()
            .unwrap()
            .state,
        WorkstreamState::Idle
    );

    // Case 2: ToolExecutionAuthority reports no active roots (revoked)
    let active_roots = Arc::new(Mutex::new(Vec::new()));
    let mut harness_revoked_auth = build_harness(Some(store.clone()), None, Some(active_roots));
    let call_revoked_auth = NormalizedToolCall {
        id: "call_revoked_auth".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Resume auth",
            "stream_name": "auth",
            "persistence": "persistent",
            "host_mounts": [{"grant": "grant-repo", "target": "/mnt/project", "access": "read_only"}]
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness_revoked_auth),
        true,
        &call_revoked_auth,
        &call_revoked_auth.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_never.clone(),
    )
    .await
    .unwrap();
    assert!(!*spawned.lock().unwrap());
    assert_eq!(
        last_tool_payload(&harness_revoked_auth.machine, "call_revoked_auth")["error_kind"],
        "grant_revoked"
    );

    // Case 3: Exhausted live task_budget prevents resume
    let mut harness_budget = build_harness(Some(store), None, None);
    let limits = TaskBudgetLimits {
        tool_calls: std::num::NonZeroU64::new(1).unwrap(),
        ..Default::default()
    };
    let budget = TaskBudget::start("task-1".to_string(), limits, None)
        .await
        .unwrap();
    budget.reserve(Admission::Tool).await.unwrap();
    harness_budget.machine.task_budget = Some(budget);

    let call_budget = NormalizedToolCall {
        id: "call_budget".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Resume auth",
            "stream_name": "auth",
            "persistence": "persistent"
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness_budget),
        true,
        &call_budget,
        &call_budget.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_never,
    )
    .await
    .unwrap();
    assert!(!*spawned.lock().unwrap());
    assert_eq!(
        last_tool_payload(&harness_budget.machine, "call_budget")["error_kind"],
        "task_budget_exhausted"
    );
    assert_eq!(
        load_workstream_record(&projects_root, &binding.project_key, "auth")
            .unwrap()
            .unwrap()
            .state,
        WorkstreamState::Idle
    );
}

#[tokio::test]
async fn ephemeral_session_never_mutates_project_ledger_and_rejects_persistent() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();

    // Ephemeral runtime passes store = None
    let mut harness = build_harness(None, None, None);
    let spawn_fn: SubagentSpawnFn = Arc::new(|_, _, _, _, _| {
        Box::pin(async {
            Ok(ChildRuntimeResult {
                status: ChildRuntimeStatus::Completed,
                process_path: "/proc/12".to_string(),
                child_run_id: None,
                rollout_path: None,
                output_text: "ok".to_string(),
                turn_summary: None,
                structured_output: None,
                warnings: Vec::new(),
                error_message: None,
                pause: None,
                child_run: None,
            })
        })
    });

    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let oneoff_call = NormalizedToolCall {
        id: "call_eph_oneoff".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "researcher",
            "task": "Quick lookup",
            "stream_name": "ephemeral-check"
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &oneoff_call,
        &oneoff_call.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_fn.clone(),
    )
    .await
    .unwrap();
    assert!(
        !crate::runtime::project_workstreams::ledger_path(&projects_root, &binding.project_key)
            .unwrap()
            .exists(),
        "ephemeral session must never create or mutate ledger.json"
    );

    let persistent_call = NormalizedToolCall {
        id: "call_eph_persistent".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Persist work",
            "stream_name": "auth",
            "persistence": "persistent"
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &persistent_call,
        &persistent_call.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_fn,
    )
    .await
    .unwrap();
    assert_eq!(
        last_tool_payload(&harness.machine, "call_eph_persistent")["error_kind"],
        "project_not_bound"
    );
}

#[tokio::test]
async fn running_workstream_rejects_concurrent_resume() {
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

    let running = crate::runtime::project_workstreams::WorkstreamRecord::new(
        "auth",
        "Auth work",
        WorkstreamPersistence::Persistent,
    );
    crate::runtime::project_workstreams::upsert_workstream(
        &projects_root,
        &binding.project_key,
        running,
    )
    .unwrap();

    let mut harness = build_harness(Some(store), None, None);
    let spawn_never: SubagentSpawnFn =
        Arc::new(|_, _, _, _, _| Box::pin(async { unreachable!("must not spawn when running") }));
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let call = NormalizedToolCall {
        id: "call_busy".to_string(),
        name: "spawn_subagent".to_string(),
        arguments: json!({
            "role": "implementer",
            "task": "Overlap auth",
            "stream_name": "auth",
            "persistence": "persistent"
        }),
    };
    handle_spawn_subagent_with_spawn(
        runtime_from_harness(&mut harness),
        true,
        &call,
        &call.arguments,
        &cancel,
        &cancel,
        &mut emit,
        spawn_never,
    )
    .await
    .unwrap();
    assert_eq!(
        last_tool_payload(&harness.machine, "call_busy")["error_kind"],
        "workstream_running"
    );
}
