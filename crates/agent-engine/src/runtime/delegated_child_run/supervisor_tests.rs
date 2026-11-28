use std::sync::Arc;
use std::time::{Duration, Instant};

use alan_agent_protocol::{AgentExecutableResult, UiActivitySnapshot, YieldKind};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{
    Access, Credentials, ExecNamespaceManifest, ExecSpec, MountFs, Namespace, Pid, ProcFs,
    ProcessInvocation, ProcessOutcome, ProcessRunner,
};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::helpers::{agent_result_matches_exit_code, invalid_agent_result_observation};
use super::*;
use crate::runtime::child_runs::{ChildRunRecord, ChildRunRegistry};
use crate::runtime::transition::{NamespaceRequestRecord, NamespaceRuntimeEnvironment};

struct SupervisorFixture {
    procfs: ProcFs,
    env: NamespaceRuntimeEnvironment,
    child_pid: String,
    child_run_id: String,
    registry: ChildRunRegistry,
}

async fn setup_fixture(
    timeout: Option<Duration>,
) -> (SupervisorFixture, DelegatedChildRunSupervisor) {
    let procfs = ProcFs::new();
    let child_agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        InProcessTransport::new(child_agentfs),
        Access::ReadWrite,
    );

    let bootstrap = procfs.for_spawner(None, ns.clone(), Credentials::user("child-agent"));
    let exec = ExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(&ns),
        descriptors: Default::default(),
        link_parent: false,
    };
    let fid = alan_ap::Fid(900);
    alan_ap::FileServer::walk(&bootstrap, alan_ap::Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    alan_ap::FileServer::open(&bootstrap, fid, alan_ap::OpenMode::ReadWrite)
        .await
        .unwrap();
    alan_ap::FileServer::write(&bootstrap, fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    alan_ap::FileServer::clunk(&bootstrap, fid).await.unwrap();

    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let registry = ChildRunRegistry::default();
    let child_run_id = "run-event-driven".to_string();
    registry.register(ChildRunRecord::new(
        child_run_id.clone(),
        "/proc/0".to_string(),
        "/proc/1".to_string(),
        Some("/agent/1".to_string()),
        Some("test".to_string()),
    ));
    registry.mark_running(&child_run_id);

    let supervisor = DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        startup: ChildProcessStartup {
            process_path: "/proc/1".to_string(),
            rollout_path: None,
            warnings: Vec::new(),
        },
        child_run_id: child_run_id.clone(),
        child_run_registry: registry.clone(),
        timeout,
        agent_files: env.agent_files(),
        process_files: env.process_files(),
        process_pid: "1".to_string(),
        resume_target: None,
    });

    (
        SupervisorFixture {
            procfs,
            env,
            child_pid: "1".to_string(),
            child_run_id,
            registry,
        },
        supervisor,
    )
}

#[test]
fn process_exit_requires_a_matching_agent_executable_result() {
    let completed = AgentExecutableResult::completed("done", Vec::new());
    assert!(agent_result_matches_exit_code(Some(0), &completed));
    assert!(!agent_result_matches_exit_code(Some(1), &completed));

    let failed = AgentExecutableResult::failed("failed");
    assert!(agent_result_matches_exit_code(Some(1), &failed));
    assert!(!agent_result_matches_exit_code(Some(0), &failed));

    let invalid = invalid_agent_result_observation(0, "raw".to_string(), Vec::new());
    assert_eq!(invalid.status, ChildRuntimeStatus::Failed);
    assert_eq!(invalid.output_text, "raw");
    assert!(
        invalid
            .error_message
            .unwrap()
            .contains("without a valid terminal result")
    );
}

#[tokio::test]
async fn supervisor_wakes_from_agent_events_on_pause_without_sleep_polling() {
    let (fixture, supervisor) = setup_fixture(Some(Duration::from_secs(10))).await;
    fixture
        .env
        .agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(1))
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let handle =
        tokio::spawn(async move { supervisor.join_until_cancelled(&cancel_for_task).await });

    // Let the supervisor park on /agent/1/events + /proc/1/wait.
    tokio::task::yield_now().await;

    let started = Instant::now();
    let req_id = fixture
        .env
        .agent_files()
        .write_request(NamespaceRequestRecord::new(
            "confirmation",
            "approve tool execution",
        ))
        .await
        .unwrap();
    fixture
        .env
        .agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::paused(Some(1)))
        .await
        .unwrap();

    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("supervisor must wake promptly on pause event")
        .unwrap()
        .unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Paused);
    let pause = result.pause.expect("pause metadata must be populated");
    assert_eq!(pause.request_id, req_id);
    assert_eq!(pause.kind, YieldKind::Confirmation);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "event-driven wakeup should not stall on process exit or polling"
    );
}

#[tokio::test]
async fn supervisor_wakes_from_proc_wait_on_process_exit() {
    let (fixture, supervisor) = setup_fixture(Some(Duration::from_secs(10))).await;
    fixture
        .env
        .agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(1))
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let handle =
        tokio::spawn(async move { supervisor.join_until_cancelled(&cancel_for_task).await });

    tokio::task::yield_now().await;

    // External cancellation via /proc/1/ctl triggers /proc/1/wait immediately.
    fixture
        .env
        .process_files()
        .write_process_control_for_pid(&fixture.child_pid, "cancel")
        .await
        .unwrap();

    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("supervisor must wake from /proc/<pid>/wait on exit")
        .unwrap()
        .unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Terminated);
    assert_eq!(
        fixture.registry.get(&fixture.child_run_id).unwrap().status,
        ChildRunStatus::Terminated
    );
}

#[tokio::test]
async fn supervisor_wakes_immediately_on_registry_termination_request() {
    let (fixture, supervisor) = setup_fixture(Some(Duration::from_secs(10))).await;
    fixture
        .env
        .agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(1))
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let handle =
        tokio::spawn(async move { supervisor.join_until_cancelled(&cancel_for_task).await });

    tokio::task::yield_now().await;

    fixture
        .registry
        .request_termination(
            "/proc/0",
            &fixture.child_run_id,
            "parent",
            ChildRunTerminationMode::Graceful,
            "operator requested stop",
        )
        .unwrap();

    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("supervisor must wake immediately on termination request")
        .unwrap()
        .unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Terminated);
    assert!(
        result
            .error_message
            .as_deref()
            .is_some_and(|msg| msg.contains("operator requested stop"))
    );
    assert_eq!(
        fixture
            .procfs
            .try_observe_process_lifecycle(Pid(1))
            .map(|(status, _)| status),
        Some(alan_kernel::Status::Exited)
    );
}

#[tokio::test]
async fn supervisor_wakes_on_proc_wait_completed_result_and_records_warnings() {
    struct CompletingRunner {
        release: Arc<Notify>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for CompletingRunner {
        async fn run(&self, _invocation: ProcessInvocation) -> ProcessOutcome {
            self.release.notified().await;
            let result = AgentExecutableResult::completed(
                "final answer",
                vec!["non-fatal child warning".to_string()],
            );
            ProcessOutcome::exited(0, result.to_process_output_record().unwrap())
        }
    }

    let release = Arc::new(Notify::new());
    let procfs = ProcFs::new().with_runner(Arc::new(CompletingRunner {
        release: release.clone(),
    }));
    let child_agentfs = Arc::new(AgentFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(procfs.clone())),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        InProcessTransport::new(child_agentfs),
        Access::ReadWrite,
    );
    let bootstrap = procfs.for_spawner(None, ns.clone(), Credentials::user("child-agent"));
    let exec = ExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(&ns),
        descriptors: Default::default(),
        link_parent: false,
    };
    let fid = alan_ap::Fid(901);
    alan_ap::FileServer::walk(&bootstrap, alan_ap::Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    alan_ap::FileServer::open(&bootstrap, fid, alan_ap::OpenMode::ReadWrite)
        .await
        .unwrap();
    alan_ap::FileServer::write(&bootstrap, fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    alan_ap::FileServer::clunk(&bootstrap, fid).await.unwrap();

    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let env = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    env.agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(1))
        .await
        .unwrap();
    let registry = ChildRunRegistry::default();
    let child_run_id = "run-completed-wait".to_string();
    registry.register(ChildRunRecord::new(
        child_run_id.clone(),
        "/proc/0".to_string(),
        "/proc/1".to_string(),
        Some("/agent/1".to_string()),
        Some("test".to_string()),
    ));
    registry.mark_running(&child_run_id);

    let supervisor = DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        startup: ChildProcessStartup {
            process_path: "/proc/1".to_string(),
            rollout_path: None,
            warnings: Vec::new(),
        },
        child_run_id: child_run_id.clone(),
        child_run_registry: registry.clone(),
        timeout: Some(Duration::from_secs(10)),
        agent_files: env.agent_files(),
        process_files: env.process_files(),
        process_pid: "1".to_string(),
        resume_target: None,
    });

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let handle =
        tokio::spawn(async move { supervisor.join_until_cancelled(&cancel_for_task).await });

    tokio::task::yield_now().await;
    release.notify_one();

    let result = tokio::time::timeout(Duration::from_secs(2), handle)
        .await
        .expect("supervisor must wake from /proc/<pid>/wait on completion")
        .unwrap()
        .unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "final answer");
    assert_eq!(result.warnings, vec!["non-fatal child warning".to_string()]);
    assert_eq!(
        registry.get(&child_run_id).unwrap().status,
        ChildRunStatus::Completed
    );
}

#[tokio::test]
async fn supervisor_enforces_idle_timeout_without_polling() {
    let (fixture, supervisor) = setup_fixture(Some(Duration::from_millis(60))).await;
    fixture
        .env
        .agent_files()
        .write_ui_activity_snapshot(&UiActivitySnapshot::running(1))
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        supervisor.join_until_cancelled(&cancel),
    )
    .await
    .expect("idle timeout must trigger promptly")
    .unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::TimedOut);
    assert_eq!(
        result.error_message.as_deref(),
        Some("Delegated Child Run idle timed out")
    );
}
