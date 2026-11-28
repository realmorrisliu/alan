use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alan_agent_protocol::{
    ProcessNamespaceAccess, ProcessNamespaceMount, SpawnHandle, SpawnHostMount, SpawnLaunchInputs,
    SpawnMountAccess, SpawnRuntimeOverrides, SpawnSpec, SpawnTarget,
};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{
    Access, Credentials, ExecNamespaceManifest, ExecSpec, MountFs, Namespace, Pid, ProcFs,
    ProcessInvocation, ProcessOutcome, ProcessRunner, Status,
};
use anyhow::Result;
use tokio::sync::Notify;

use super::*;

fn mount(path: &str, access: ProcessNamespaceAccess) -> ProcessNamespaceMount {
    ProcessNamespaceMount::new(path, access)
}

fn parent_mounts() -> Vec<ProcessNamespaceMount> {
    use ProcessNamespaceAccess::{ReadOnly, ReadWrite};
    vec![
        mount("/proc", ReadWrite),
        mount("/agent", ReadWrite),
        mount("/srv", ReadOnly),
        mount("/bin/alan-agent", ReadOnly),
        mount("/mnt/llm", ReadWrite),
        mount("/mnt/route", ReadWrite),
        mount("/mnt/host-mount", ReadWrite),
        mount("/man", ReadOnly),
        mount("/lib/agents/root", ReadOnly),
        mount("/lib/pkg/example", ReadOnly),
        mount("/memory", ReadWrite),
        mount("/mnt/source", ReadWrite),
    ]
}

fn descriptors() -> BTreeMap<u32, String> {
    BTreeMap::from([
        (
            alan_agent_protocol::AGENT_DEFINITION_DESCRIPTOR,
            "/lib/agents/root".to_string(),
        ),
        (
            alan_agent_protocol::MEMORY_STORE_DESCRIPTOR,
            "/memory".to_string(),
        ),
    ])
}

fn spec() -> SpawnSpec {
    SpawnSpec {
        target: SpawnTarget::DefinitionDescriptor {
            descriptor: crate::AGENT_DEFINITION_DESCRIPTOR.to_string(),
        },
        launch: SpawnLaunchInputs {
            task: "inspect".to_string(),
            ..SpawnLaunchInputs::default()
        },
        handles: Vec::new(),
        host_mounts: Vec::new(),
        runtime_overrides: SpawnRuntimeOverrides::default(),
        delegated: None,
    }
}

fn plan(spec: &SpawnSpec) -> Result<ChildNamespacePlan> {
    let plan = build_child_namespace_plan(
        spec,
        &parent_mounts(),
        Some("/lib/agents/root"),
        &descriptors(),
        &[],
        "default",
    )?;
    evaluate_delegated_launch_capabilities(
        &mut spec.clone(),
        &plan,
        &parent_mounts(),
        Path::new("/mnt/source"),
    )?;
    Ok(plan)
}

#[test]
fn missing_project_mount_retains_capability_mismatch_before_cwd_rejection() {
    let mut spec = spec();
    spec.launch.cwd = Some("/mnt/source".into());
    spec.delegated = Some(alan_agent_protocol::DelegatedSpawnContext {
        requirements: crate::runtime::delegation_capabilities::classify_delegated_task_requirements(
            "Read fixture.txt",
            spec.launch.cwd.as_deref(),
        ),
    });
    let error = plan(&spec).unwrap_err();
    let rejection = error
        .downcast_ref::<crate::runtime::delegation_capabilities::DelegatedSpawnRejected>()
        .unwrap();
    assert_eq!(
        rejection.decision.recovery,
        alan_agent_protocol::DelegatedCapabilityRecovery::ParentPath
    );
    assert_eq!(
        rejection.decision.unsatisfied,
        vec![
            alan_agent_protocol::DelegatedCapabilityRequirement::MountRead {
                path: Some("/mnt/source".into()),
            }
        ]
    );
}

#[test]
fn child_namespace_defaults_to_root_without_ambient_host_mounts() {
    let plan = plan(&spec()).unwrap();

    assert_eq!(plan.cwd, Path::new("/"));
    assert!(
        plan.process_mounts
            .iter()
            .any(|mount| mount.path == "/bin/alan-agent")
    );
    assert!(
        plan.process_mounts
            .iter()
            .any(|mount| mount.path == "/lib/pkg/example")
    );
    for ambient in ["/bin", "/lib", "/memory", "/mnt/source"] {
        assert!(
            !plan
                .process_mounts
                .iter()
                .any(|mount| mount.path == ambient),
            "child unexpectedly inherited {ambient}"
        );
    }
}

#[test]
fn child_namespace_projects_only_explicit_host_mount_with_narrower_access() {
    let mut spec = spec();
    spec.launch.cwd = Some(PathBuf::from("/mnt/review"));
    spec.host_mounts.push(SpawnHostMount {
        grant: "grant-source".to_string(),
        target: PathBuf::from("/mnt/review"),
        access: SpawnMountAccess::ReadOnly,
    });

    let plan = plan(&spec).unwrap();

    assert_eq!(plan.cwd, Path::new("/mnt/review"));
    assert!(plan.effective_mounts.iter().any(|mount| {
        mount.path == "/mnt/review" && mount.access == ProcessNamespaceAccess::ReadOnly
    }));
    assert!(
        !plan
            .process_mounts
            .iter()
            .any(|mount| mount.path == "/mnt/source" || mount.path == "/mnt/review")
    );
}

#[test]
fn child_namespace_rejects_cwd_outside_explicit_capabilities() {
    let mut spec = spec();
    spec.launch.cwd = Some(PathBuf::from("/mnt/source"));

    let error = plan(&spec).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("outside its delegated namespace")
    );
}

#[test]
fn child_namespace_passes_memory_only_by_descriptor_handle() {
    let mut spec = spec();
    assert!(
        !plan(&spec)
            .unwrap()
            .effective_mounts
            .iter()
            .any(|mount| mount.path == "/memory")
    );

    spec.handles.push(SpawnHandle::Memory);
    let with_mem = plan(&spec).unwrap();
    assert!(with_mem.effective_mounts.iter().any(|mount| {
        mount.path == "/memory" && mount.access == ProcessNamespaceAccess::ReadOnly
    }));
}

#[test]
fn child_launch_requires_absolute_normalized_cwd() {
    for cwd in ["docs", "/mnt/source/../secret", "/mnt//source"] {
        let mut spec = spec();
        spec.launch.cwd = Some(PathBuf::from(cwd));

        assert!(
            validate_child_launch_contract(&spec).is_err(),
            "accepted invalid cwd {cwd}"
        );
    }
}

#[test]
fn terminal_child_can_finish_before_live_agentfs_is_observed() {
    let completed = alan_agent_protocol::AgentExecutableResult::completed("done", Vec::new());
    assert!(child_reached_observable_terminal(0, &completed));
    assert!(!child_reached_observable_terminal(1, &completed));

    let paused = alan_agent_protocol::AgentExecutableResult::paused(
        "partial",
        Vec::new(),
        alan_agent_protocol::AgentExecutablePause {
            request_id: "request-1".to_string(),
            kind: alan_agent_protocol::YieldKind::Confirmation,
            payload: None,
        },
    );
    assert!(child_reached_observable_terminal(1, &paused));

    let failed = alan_agent_protocol::AgentExecutableResult::failed("failed");
    assert!(child_reached_observable_terminal(1, &failed));
    assert!(!child_reached_observable_terminal(0, &failed));
}

#[test]
fn subagent_plan_enforces_read_only_and_equal_or_narrower_parent_mounts() {
    let mut mounts = parent_mounts();
    mounts.retain(|m| m.path != "/mnt/source");
    mounts.push(mount("/mnt/source", ProcessNamespaceAccess::ReadOnly));

    let mut rw_subagent = spec();
    rw_subagent.target = SpawnTarget::AdHocSubagent {
        role: "verifier".to_string(),
    };
    rw_subagent.host_mounts.push(SpawnHostMount {
        grant: "grant-ro".to_string(),
        target: PathBuf::from("/mnt/source"),
        access: SpawnMountAccess::ReadWrite,
    });
    assert!(
        build_child_namespace_plan(&rw_subagent, &mounts, None, &descriptors(), &[], "default")
            .is_err(),
        "read_only parent mount must not widen to read_write child mount"
    );

    let mut ro_subagent = rw_subagent;
    ro_subagent.host_mounts[0].access = SpawnMountAccess::ReadOnly;
    ro_subagent.runtime_overrides.read_only = true;
    ro_subagent.launch.cwd = Some(PathBuf::from("/mnt/source"));
    let plan =
        build_child_namespace_plan(&ro_subagent, &mounts, None, &descriptors(), &[], "default")
            .unwrap();
    assert!(plan.validate_cwd_scope().is_ok());
}

#[test]
fn workstream_checkpoint_recovery_uses_recovery_rollout_without_inherited_resume() {
    let ctx = ChildTaskContext::new(None, None, None);
    let ckpt_target = crate::agent_machine::pending_resume::ResumeTarget {
        request_id: String::new(),
        child_pid: None,
        rollout_path: Some("/tmp/ws-ckpt.jsonl".to_string()),
    };
    let ws_req = build_child_executable_request(&ctx, spec(), Some(&ckpt_target), None);
    assert_eq!(ws_req.resume, None);
    assert!(!ws_req.inherited_resume);
    assert_eq!(
        ws_req.recovery_rollout.as_deref(),
        Some("/tmp/ws-ckpt.jsonl")
    );

    let inherited_target = crate::agent_machine::pending_resume::ResumeTarget {
        request_id: "req-1".to_string(),
        child_pid: Some("9".to_string()),
        rollout_path: Some("/tmp/inherited.jsonl".to_string()),
    };
    let inherited_req = build_child_executable_request(&ctx, spec(), Some(&inherited_target), None);
    assert_eq!(inherited_req.resume.as_deref(), Some("req-1"));
    assert!(inherited_req.inherited_resume);
    assert_eq!(
        inherited_req.recovery_rollout.as_deref(),
        Some("/tmp/inherited.jsonl")
    );
}

#[test]
fn child_namespace_mounts_agent_read_only_until_runtime_child_view() {
    let plan = plan(&spec()).unwrap();
    let process_agent = plan
        .process_mounts
        .iter()
        .find(|mount| mount.path == "/agent")
        .expect("child plan must include /agent mount");
    assert_eq!(
        process_agent.access,
        ProcessNamespaceAccess::ReadOnly,
        "child plan must not inherit parent read_write /agent mount"
    );
    let summary = plan.namespace_summary();
    assert!(
        !summary.writable_mounts.iter().any(|path| path == "/agent"),
        "child summary must not expose /agent as writable: {:?}",
        summary.writable_mounts
    );
}

/// Captures committed `ExecSpec`s and keeps spawned processes alive until aborted
/// by kernel cancellation or drop.
struct RecordingHoldingRunner {
    recorded: std::sync::Mutex<Vec<(Pid, ExecSpec)>>,
    started: Notify,
}

impl RecordingHoldingRunner {
    fn new() -> Self {
        Self {
            recorded: std::sync::Mutex::new(Vec::new()),
            started: Notify::new(),
        }
    }

    fn exec_for(&self, pid: Pid) -> Option<ExecSpec> {
        self.recorded
            .lock()
            .expect("recorded mutex poisoned")
            .iter()
            .find(|(recorded_pid, _)| *recorded_pid == pid)
            .map(|(_, exec)| exec.clone())
    }
}

#[async_trait::async_trait]
impl ProcessRunner for RecordingHoldingRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        self.recorded
            .lock()
            .expect("recorded mutex poisoned")
            .push((invocation.pid, invocation.exec.clone()));
        self.started.notify_waiters();
        std::future::pending::<()>().await;
        ProcessOutcome::exited(0, Vec::new())
    }
}

#[tokio::test]
async fn parent_owned_child_launch_sets_link_parent_and_cascades_while_detached_survives() {
    let runner = Arc::new(RecordingHoldingRunner::new());
    let procfs = ProcFs::new().with_runner(runner.clone());

    let mut parent_ns = Namespace::new();
    for path in [
        "/srv",
        "/bin/alan-agent",
        "/mnt/llm",
        "/mnt/route",
        "/mnt/host-mount",
        "/man",
        "/lib/agents/root",
        "/bin/detached-tool",
    ] {
        parent_ns.mount(
            path,
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadWrite,
        );
    }
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new(Arc::new(procfs.clone())));
    parent_ns.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    parent_ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );

    // Commit parent process (pid 1) via bootstrap /proc/clone with AGENT_DEFINITION_FD.
    let bootstrap = procfs.for_spawner(None, parent_ns.clone(), Credentials::user("root-agent"));
    let parent_exec = ExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(&parent_ns),
        descriptors: BTreeMap::from([(
            alan_agent_protocol::AGENT_DEFINITION_DESCRIPTOR,
            "/lib/agents/root".to_string(),
        )]),
        link_parent: false,
    };
    let fid = alan_ap::Fid(777);
    alan_ap::FileServer::walk(&bootstrap, alan_ap::Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    alan_ap::FileServer::open(&bootstrap, fid, alan_ap::OpenMode::ReadWrite)
        .await
        .unwrap();
    alan_ap::FileServer::write(
        &bootstrap,
        fid,
        0,
        &serde_json::to_vec(&parent_exec).unwrap(),
    )
    .await
    .unwrap();
    alan_ap::FileServer::clunk(&bootstrap, fid).await.unwrap();

    // Bind parent's /proc view so clones record parent = Pid(1).
    let parent_procfs = procfs.for_spawner(
        Some(Pid(1)),
        parent_ns.clone(),
        Credentials::user("root-agent"),
    );
    parent_ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(parent_procfs)),
        Access::ReadWrite,
    );
    let root_transport = InProcessTransport::new(Arc::new(MountFs::new(parent_ns)));
    let env =
        crate::runtime::NamespaceRuntimeEnvironment::new(root_transport, "/agent/1", "default");
    let parent_runtime = ChildLaunchRuntime::new(
        env.child_launch(),
        env.tool_execution(),
        env.child_run_registry().clone(),
        "/proc/1".to_string(),
        None,
        ChildTaskContext::new(None, None, None),
    );
    let parent_descriptors = env
        .process_files()
        .read_process_descriptors("1")
        .await
        .unwrap();

    // 1. Parent-owned child agent launch through ChildProcessLaunch sets link_parent: true.
    let (linked_child_pid, _, _) = ChildProcessLaunch {
        parent: &parent_runtime,
        process_files: env.child_launch().process_files(),
        parent_pid: "1",
        parent_descriptors: &parent_descriptors,
        requested_spec: &spec(),
        cancel: None,
        resume: None,
    }
    .spawn()
    .await
    .unwrap();
    let linked_pid = Pid(linked_child_pid.parse().unwrap());
    agent_root
        .bind_process(linked_child_pid.clone(), Arc::new(AgentFs::new()))
        .await;

    // 2. Independent/detached spawn via spawn_process keeps link_parent: false.
    let mut detached_retained = None;
    let detached_pid_str = env
        .process_files()
        .spawn_process(
            "/bin/detached-tool",
            Vec::<String>::new(),
            &mut detached_retained,
        )
        .await
        .unwrap();
    let detached_pid = Pid(detached_pid_str.parse().unwrap());

    // Wait until runner recorded both invocations.
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if runner.exec_for(linked_pid).is_some() && runner.exec_for(detached_pid).is_some() {
                break;
            }
            runner.started.notified().await;
        }
    })
    .await
    .expect("runner should start both child processes");

    assert!(
        runner.exec_for(linked_pid).unwrap().link_parent,
        "parent-owned child agent launch must set link_parent: true"
    );
    assert!(
        !runner.exec_for(detached_pid).unwrap().link_parent,
        "independent/detached spawn must keep link_parent: false"
    );

    // Subscribe to the linked child's /proc/<pid>/wait stream before cancelling the parent.
    let wait_task = {
        let process_files = env.process_files();
        let pid = linked_child_pid.clone();
        tokio::spawn(async move { process_files.wait_for_process_exit(&pid).await })
    };

    // Cancel the parent process through /proc/1/ctl.
    env.process_files()
        .write_process_control_for_pid("1", "cancel")
        .await
        .unwrap();

    // Linked child must cascade-cancel deterministically and unblock /proc/<child>/wait.
    let wait_event = tokio::time::timeout(Duration::from_secs(2), wait_task)
        .await
        .expect("linked child /proc/<pid>/wait must fire on parent cancel")
        .unwrap()
        .unwrap();
    assert_eq!(wait_event.pid, linked_pid.0);
    assert_eq!(wait_event.status, "exited");
    assert_eq!(wait_event.exit_code, alan_kernel::CANCELLED_EXIT_CODE);
    assert_eq!(wait_event.reason, "cancelled");

    // Detached child (link_parent: false) must remain running independently.
    assert_eq!(
        procfs.try_observe_process_lifecycle(detached_pid),
        Some((Status::Running, None)),
        "detached link_parent: false child must survive parent cancellation"
    );
    assert_eq!(
        env.process_files()
            .read_process_exit_code(&detached_pid_str)
            .await
            .unwrap(),
        None
    );

    // Clean up detached child so no background task leaks.
    let _ = env
        .process_files()
        .write_process_control_for_pid(&detached_pid_str, "cancel")
        .await;
}
