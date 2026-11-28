//! In-process end-to-end regression coverage for event-driven process supervision.
//!
//! Every test here boots a real in-process runtime and drives only the public aP
//! file surfaces (`/mnt/service-manager/...`, `/proc/<pid>/wait`,
//! `/proc/<pid>/ctl`, `/proc/clone`). The suite deliberately avoids AF_UNIX
//! sockets, pseudo-terminals, and `sandbox-exec`, so the same coverage runs both
//! inside Seatbelt confinement and on a plain host.
//!
//! The three focused regressions are:
//! 1. A live [`EmbeddedAlan`] agent workflow exposes the per-unit telemetry
//!    projection (`generation`, `restarts`, `backoff_until`, `last_exit_reason`).
//! 2. A live [`ServiceManager`] drives unit exit detection from the blocking
//!    `/proc/<pid>/wait` stream (not polling), restarts the leaf behind its
//!    generation-tracked [`SupervisedHandle`], and rejects stale fids.
//! 3. `/proc` commit + cascade closes a `link_parent` child's wait stream while a
//!    detached sibling keeps running.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_ap::{ErrorCode, Fid, FileServer, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{
    CANCELLED_EXIT_CODE, Credentials, ExecNamespaceManifest, ExecSpec, ExitReason, Namespace, Pid,
    ProcFs, ProcessInvocation, ProcessOutcome, ProcessRunner, ProcessWaitRecord, Status,
};
use alan_llm::{GenerationResponse, MockLlmProvider};
use alan_os_host::{EmbeddedAlan, EmbeddedConfig, LlmSource};
use alan_service_manager::{
    ProcessLaunchContext, ServiceManager, ServiceManagerConfig, UnitStatus,
};
use alan_shell::{Shell, Tail};

fn response(content: &str) -> GenerationResponse {
    GenerationResponse {
        content: content.to_string(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: Vec::new(),
        usage: None,
        finish_reason: Some("stop".to_string()),
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    }
}

fn ephemeral_config() -> ServiceManagerConfig {
    let mut process = AgentProcessConfig::default();
    process.agent_config.core_config.memory.enabled = false;
    ServiceManagerConfig::ephemeral(
        process,
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    )
}

fn embedded_config(reply: &str) -> EmbeddedConfig {
    EmbeddedConfig::ephemeral(LlmSource::Client(LlmClient::new(
        MockLlmProvider::new().with_response(response(reply)),
    )))
}

async fn read_trimmed(shell: &Shell, path: &str) -> String {
    let raw = shell
        .cat(path)
        .await
        .unwrap_or_else(|error| panic!("failed to read {path}: {error:?}"));
    String::from_utf8(raw)
        .unwrap_or_else(|error| panic!("{path} is not UTF-8: {error}"))
        .trim()
        .to_string()
}

fn parse_wait_record(bytes: &[u8]) -> ProcessWaitRecord {
    let text = std::str::from_utf8(bytes).expect("wait record is UTF-8");
    let line = text.lines().next().expect("wait record has a line");
    serde_json::from_str(line).expect("wait record is terminal JSON")
}

async fn tail_until_contains(tail: &mut Tail, expected: &str) -> String {
    let mut collected = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while !collected.contains(expected) {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for `{expected}`, collected `{collected}`"
        );
        let chunk = tokio::time::timeout(remaining, tail.read(4096))
            .await
            .expect("timed out waiting for output chunk")
            .expect("output tail read failed");
        if chunk.is_empty() {
            break;
        }
        collected.push_str(&String::from_utf8_lossy(&chunk));
    }
    collected
}

async fn wait_for_ready_generation(
    manager: &ServiceManager,
    name: &str,
    expected_generation: u64,
    stale_pid: Option<u64>,
) -> u64 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let unit = manager.state().lock().await.unit(name).unwrap();
            if unit.status == UnitStatus::Ready
                && unit.generation >= expected_generation
                && let Some(pid) = unit.pid
                && Some(pid) != stale_pid
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("timed out waiting for `{name}` Ready at generation >= {expected_generation}")
    })
}

#[tokio::test]
async fn embedded_agent_workflow_exposes_unit_telemetry_projection() {
    let app = EmbeddedAlan::boot(embedded_config("embedded telemetry turn complete"))
        .await
        .unwrap();
    let shell = app.shell();

    // Every supervised system unit projects the four telemetry files.
    let units = shell.ls("/mnt/service-manager/units").await.unwrap();
    for unit in [
        "root-agent",
        "agent-runtime",
        "package",
        "route",
        "local-entry",
    ] {
        assert!(
            units.iter().any(|entry| entry == unit),
            "missing `{unit}` in unit listing: {units:?}"
        );
    }

    for unit in ["root-agent", "package", "route"] {
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{unit}/generation")
            )
            .await,
            "1",
            "initial generation for `{unit}`"
        );
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{unit}/restarts")
            )
            .await,
            "0",
            "initial restarts for `{unit}`"
        );
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{unit}/backoff_until")
            )
            .await,
            "",
            "initial backoff_until for `{unit}`"
        );
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{unit}/last_exit_reason")
            )
            .await,
            "",
            "initial last_exit_reason for `{unit}`"
        );
    }

    // A real agent turn runs on the same live instance without disturbing telemetry.
    let client = app.root_agent();
    let mut output = client.tail_output().await.unwrap();
    client.submit_text("report telemetry").await.unwrap();
    let text = tail_until_contains(&mut output, "embedded telemetry turn complete").await;
    assert!(
        text.contains("embedded telemetry turn complete"),
        "unexpected agent output: {text}"
    );

    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/root-agent/generation").await,
        "1"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/root-agent/restarts").await,
        "0"
    );
    assert_eq!(
        read_trimmed(
            &shell,
            "/mnt/service-manager/units/root-agent/last_exit_reason"
        )
        .await,
        ""
    );

    output.close().await.unwrap();
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn live_service_manager_restart_recovery_is_event_driven_not_polled() {
    let manager = ServiceManager::boot(ephemeral_config()).await.unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let transport = InProcessTransport::new(namespace.clone());
    let shell = Shell::new(transport.clone());

    // Baseline telemetry projection.
    for (path, expected) in [
        ("/mnt/service-manager/units/package/generation", "1"),
        ("/mnt/service-manager/units/package/restarts", "0"),
        ("/mnt/service-manager/units/package/backoff_until", ""),
        ("/mnt/service-manager/units/package/last_exit_reason", ""),
    ] {
        assert_eq!(read_trimmed(&shell, path).await, expected, "{path}");
    }

    let stale_pid = manager
        .state()
        .lock()
        .await
        .unit("package")
        .unwrap()
        .pid
        .unwrap();

    // A pre-restart fid against the leaf service must later fail cleanly.
    assert!(matches!(
        transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(201),
                names: vec!["mnt".into(), "package".into(), "status".into()],
            })
            .await,
        Ok(Response::Walk { .. })
    ));

    // Subscribe to the blocking terminal-exit stream before the crash.
    let mut wait_tail = shell
        .tail(&format!("/proc/{stale_pid}/wait"))
        .await
        .unwrap();
    let wait_task = tokio::spawn(async move { wait_tail.read(4096).await });

    // While the unit is live the stream must stay parked, not spin or resolve.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !wait_task.is_finished(),
        "/proc/{stale_pid}/wait must block until the unit actually exits"
    );

    manager.terminate_unit("package", 1).await.unwrap();

    let bytes = tokio::time::timeout(Duration::from_secs(5), wait_task)
        .await
        .expect("blocking /proc/<pid>/wait must fire on unit exit")
        .expect("wait reader task panicked")
        .expect("wait stream read failed");
    let record = parse_wait_record(&bytes);
    assert_eq!(
        record.pid, stale_pid,
        "terminal record names the crashed pid"
    );
    assert_eq!(record.status, "exited");
    assert_eq!(record.exit_code, 1);
    assert_eq!(record.reason, ExitReason::Failed);

    // A second read from offset 0 after exit is race-free and returns the same record.
    let post_exit = shell.cat(&format!("/proc/{stale_pid}/wait")).await.unwrap();
    assert_eq!(parse_wait_record(&post_exit), record);

    // Event-driven supervisor restarts the leaf behind its generation-tracked handle.
    let new_pid = wait_for_ready_generation(&manager, "package", 2, Some(stale_pid)).await;
    assert_ne!(new_pid, stale_pid);
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/package/generation").await,
        "2"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/package/restarts").await,
        "1"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/package/backoff_until").await,
        ""
    );
    assert_eq!(
        read_trimmed(
            &shell,
            "/mnt/service-manager/units/package/last_exit_reason"
        )
        .await,
        "failed (exit 1)"
    );
    assert_eq!(
        read_trimmed(&shell, &format!("/proc/{new_pid}/status")).await,
        "running"
    );

    // Pre-restart fids are rejected; fresh walks bind to the rebound generation.
    assert_eq!(
        transport.call(Request::Stat { fid: Fid(201) }).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport.call(Request::Clunk { fid: Fid(201) }).await,
        Err(ErrorCode::Io)
    );
    assert!(shell.cat("/mnt/package/status").await.is_ok());

    manager.shutdown().await.unwrap();
}

/// A `ProcessRunner` that starts a process and keeps it alive until the kernel
/// records its exit, so the `link_parent` cascade has live processes to observe.
struct HoldingRunner {
    invocations: Mutex<Vec<(Pid, Option<Pid>, bool)>>,
    started: tokio::sync::Notify,
}

impl HoldingRunner {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            invocations: Mutex::new(Vec::new()),
            started: tokio::sync::Notify::new(),
        })
    }

    fn link_parent_for(&self, pid: Pid) -> Option<(Option<Pid>, bool)> {
        self.invocations
            .lock()
            .expect("invocation log poisoned")
            .iter()
            .find(|(recorded, _, _)| *recorded == pid)
            .map(|(_, parent, link_parent)| (*parent, *link_parent))
    }

    /// Wait until the kernel has dispatched `count` committed processes to the runner.
    async fn wait_for_count(&self, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .invocations
                    .lock()
                    .expect("invocation log poisoned")
                    .len()
                    >= count
                {
                    return;
                }
                self.started.notified().await;
            }
        })
        .await
        .expect("timed out waiting for committed processes to reach the runner");
    }
}

#[async_trait::async_trait]
impl ProcessRunner for HoldingRunner {
    async fn run(&self, invocation: ProcessInvocation) -> ProcessOutcome {
        self.invocations
            .lock()
            .expect("invocation log poisoned")
            .push((
                invocation.pid,
                invocation.parent,
                invocation.exec.link_parent,
            ));
        self.started.notify_waiters();
        std::future::pending::<()>().await;
        ProcessOutcome::exited(0, Vec::new())
    }
}

async fn commit_process(procfs: &ProcFs, namespace: &Namespace, link_parent: bool) -> Pid {
    let fid = Fid(1);
    procfs
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .expect("walk /proc/clone");
    procfs
        .open(fid, OpenMode::ReadWrite)
        .await
        .expect("open /proc/clone");
    let allocated: u64 = String::from_utf8(procfs.read(fid, 0, 64).await.expect("clone pid"))
        .expect("clone pid is UTF-8")
        .trim()
        .parse()
        .expect("clone pid is numeric");
    let exec = ExecSpec {
        executable: "/bin/supervised-holder".to_string(),
        args: Vec::new(),
        namespace: ExecNamespaceManifest::from_namespace(namespace),
        descriptors: BTreeMap::new(),
        link_parent,
    };
    procfs
        .write(fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .expect("write exec spec");
    procfs.clunk(fid).await.expect("commit /proc/clone");
    Pid(allocated)
}

async fn cancel_process(procfs: &ProcFs, pid: Pid) {
    let fid = Fid(2);
    procfs
        .walk(Fid::ROOT, fid, &[pid.0.to_string(), "ctl".to_string()])
        .await
        .expect("walk /proc/<pid>/ctl");
    procfs
        .open(fid, OpenMode::Write)
        .await
        .expect("open /proc/<pid>/ctl");
    procfs.write(fid, 0, b"cancel").await.expect("write cancel");
    procfs.clunk(fid).await.expect("clunk /proc/<pid>/ctl");
}

#[tokio::test]
async fn link_parent_cascade_closes_child_wait_and_spares_detached_sibling() {
    let runner = HoldingRunner::new();
    let procfs = ProcFs::new().with_runner(runner.clone());
    let namespace = Namespace::new();

    // Parent commits at the namespace root; it is not itself parent-linked.
    let parent = commit_process(
        &procfs.for_spawner(None, namespace.clone(), Credentials::system()),
        &namespace,
        false,
    )
    .await;

    // A parent-linked child and a detached sibling share the same parent.
    let linked = commit_process(
        &procfs.for_spawner(Some(parent), namespace.clone(), Credentials::system()),
        &namespace,
        true,
    )
    .await;
    let detached = commit_process(
        &procfs.for_spawner(Some(parent), namespace.clone(), Credentials::system()),
        &namespace,
        false,
    )
    .await;
    runner.wait_for_count(3).await;
    assert_eq!(
        runner.link_parent_for(parent),
        Some((None, false)),
        "parent process must not be parent-linked"
    );
    assert_eq!(
        runner.link_parent_for(linked),
        Some((Some(parent), true)),
        "parent-owned child must set link_parent"
    );
    assert_eq!(
        runner.link_parent_for(detached),
        Some((Some(parent), false)),
        "detached sibling must keep link_parent false"
    );

    // Subscribe to the linked child's terminal-exit stream before cancelling the parent.
    let wait_view = procfs.for_spawner(None, namespace.clone(), Credentials::system());
    wait_view
        .walk(
            Fid::ROOT,
            Fid(3),
            &[linked.0.to_string(), "wait".to_string()],
        )
        .await
        .expect("walk linked child /proc/<pid>/wait");
    wait_view
        .open(Fid(3), OpenMode::Read)
        .await
        .expect("open linked child /proc/<pid>/wait");
    let wait_task = tokio::spawn(async move { wait_view.read(Fid(3), 0, 4096).await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !wait_task.is_finished(),
        "linked child wait stream must block while the child runs"
    );

    // Cancelling the parent must deterministically cascade to the linked child.
    let cancel_view = procfs.for_spawner(None, namespace.clone(), Credentials::system());
    cancel_process(&cancel_view, parent).await;

    let bytes = tokio::time::timeout(Duration::from_secs(5), wait_task)
        .await
        .expect("linked child /proc/<pid>/wait must fire on parent cancel")
        .expect("wait reader task panicked")
        .expect("wait stream read failed");
    let record = parse_wait_record(&bytes);
    assert_eq!(record.pid, linked.0);
    assert_eq!(record.status, "exited");
    assert_eq!(record.exit_code, CANCELLED_EXIT_CODE);
    assert_eq!(record.reason, ExitReason::Cancelled);

    // A post-exit read from offset 0 is immediate and identical.
    let post_view = procfs.for_spawner(None, namespace.clone(), Credentials::system());
    post_view
        .walk(
            Fid::ROOT,
            Fid(4),
            &[linked.0.to_string(), "wait".to_string()],
        )
        .await
        .unwrap();
    post_view.open(Fid(4), OpenMode::Read).await.unwrap();
    let direct = post_view.read(Fid(4), 0, 4096).await.unwrap();
    assert_eq!(parse_wait_record(&direct), record);

    // A write open on the read-only wait stream is refused at dial time.
    let read_only_view = procfs.for_spawner(None, namespace.clone(), Credentials::system());
    read_only_view
        .walk(
            Fid::ROOT,
            Fid(5),
            &[linked.0.to_string(), "wait".to_string()],
        )
        .await
        .unwrap();
    assert!(matches!(
        read_only_view.open(Fid(5), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    ));

    // The detached sibling survives the parent cascade untouched.
    assert_eq!(
        procfs.try_observe_process_lifecycle(detached),
        Some((Status::Running, None)),
        "detached link_parent:false child must survive parent cancellation"
    );

    // Clean up the surviving detached child so no runner task outlives the test.
    let cleanup_view = procfs.for_spawner(None, namespace.clone(), Credentials::system());
    cancel_process(&cleanup_view, detached).await;
    assert_eq!(
        procfs.try_observe_process_lifecycle(detached),
        Some((Status::Exited, Some(CANCELLED_EXIT_CODE)))
    );
}
