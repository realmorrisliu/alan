use std::time::Duration;

use alan_agent_engine::{AgentProcessConfig, LlmClient, ToolRegistry};
use alan_ap::{ErrorCode, Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{ExitReason, Pid, ProcessWaitRecord};
use alan_llm::MockLlmProvider;
use alan_service_manager::{
    BootManifest, ProcessLaunchContext, ServiceManager, ServiceManagerConfig, SessionLaunchOptions,
    SupervisionStrategy, SystemStatus, UnitStatus,
};
use alan_shell::Shell;

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

async fn read_trimmed(shell: &Shell, path: &str) -> String {
    String::from_utf8(shell.cat(path).await.unwrap())
        .unwrap()
        .trim()
        .to_string()
}

async fn wait_for_unit_ready_with_generation(
    manager: &ServiceManager,
    name: &str,
    expected_generation: u64,
    old_pid: Option<u64>,
) -> u64 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let unit = manager.state().lock().await.unit(name).unwrap();
            if unit.status == UnitStatus::Ready
                && unit.generation >= expected_generation
                && let Some(pid) = unit.pid
                && Some(pid) != old_pid
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!("timed out waiting for unit `{name}` to reach Ready at gen >= {expected_generation}")
    })
}

async fn read_wait_record(shell: &Shell, pid: u64) -> ProcessWaitRecord {
    let raw = shell
        .cat(&format!("/proc/{pid}/wait"))
        .await
        .unwrap_or_else(|err| panic!("failed to read /proc/{pid}/wait: {err:?}"));
    let line = std::str::from_utf8(&raw).unwrap().lines().next().unwrap();
    serde_json::from_str(line).unwrap()
}

#[tokio::test]
async fn reverse_topological_shutdown_stops_dependents_before_prerequisites() {
    let manifest = BootManifest::system().unwrap();
    let manager = ServiceManager::boot(ephemeral_config()).await.unwrap();
    let state = manager.state();

    // Capture initial PIDs and a client namespace over /proc before shutdown.
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = Shell::new(InProcessTransport::new(namespace));

    let mut unit_pids = Vec::new();
    for unit in manifest.ordered() {
        let pid = state.lock().await.unit(&unit.name).unwrap().pid.unwrap();
        unit_pids.push((unit.name.clone(), pid));
    }

    manager.shutdown().await.unwrap();

    // Every supervised unit process must have exited cleanly via /proc/<pid>/wait.
    for (name, pid) in &unit_pids {
        let record = tokio::time::timeout(Duration::from_secs(2), read_wait_record(&shell, *pid))
            .await
            .unwrap_or_else(|_| panic!("unit `{name}` (pid {pid}) did not emit wait record"));
        assert_eq!(record.pid, *pid);
        assert_eq!(record.status, "exited");
        assert_eq!(record.exit_code, 0);
        assert_eq!(record.reason, ExitReason::Normal);
    }

    // Verify strict reverse-topological order against BootManifest.after edges.
    let shutdown_order = state.lock().await.shutdown_order().to_vec();
    assert_eq!(shutdown_order.len(), unit_pids.len());
    for unit in manifest.ordered() {
        let unit_pos = shutdown_order
            .iter()
            .position(|name| name == &unit.name)
            .unwrap_or_else(|| panic!("unit `{}` missing from shutdown_order", unit.name));
        for dep in &unit.after {
            let dep_pos = shutdown_order
                .iter()
                .position(|name| name == dep)
                .unwrap_or_else(|| panic!("dependency `{dep}` missing from shutdown_order"));
            assert!(
                unit_pos < dep_pos,
                "downstream dependent `{}` (pos {unit_pos}) must stop before prerequisite `{dep}` (pos {dep_pos}); order={shutdown_order:?}",
                unit.name
            );
        }
    }
}

#[tokio::test]
async fn one_for_one_leaf_self_heals_without_dropping_live_root_or_local_entry_sessions() {
    let manager = ServiceManager::boot(ephemeral_config()).await.unwrap();

    // Establish both a live local-entry client namespace and an active interactive session.
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let transport = InProcessTransport::new(namespace);
    let shell = Shell::new(transport.clone());
    let session = manager
        .open_session(SessionLaunchOptions::default())
        .await
        .unwrap();

    let old_package_pid = manager
        .state()
        .lock()
        .await
        .unit("package")
        .unwrap()
        .pid
        .unwrap();
    let root_pid_before = manager.root_pid();
    let local_entry_pid_before = manager
        .state()
        .lock()
        .await
        .unit("local-entry")
        .unwrap()
        .pid
        .unwrap();
    let agent_runtime_pid_before = manager
        .state()
        .lock()
        .await
        .unit("agent-runtime")
        .unwrap()
        .pid
        .unwrap();

    // Walk a pre-restart fid against /mnt/package/status to verify stale-generation rejection.
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

    // Crash the leaf `package` service; default strategy is OneForOne.
    manager.terminate_unit("package", 1).await.unwrap();

    let new_package_pid =
        wait_for_unit_ready_with_generation(&manager, "package", 2, Some(old_package_pid)).await;
    assert_ne!(new_package_pid, old_package_pid);

    // Downstream units and live sessions must NOT have been restarted or dropped.
    assert_eq!(manager.root_pid(), root_pid_before);
    for (untouched, expected_pid) in [
        ("root-agent", root_pid_before.0),
        ("local-entry", local_entry_pid_before),
        ("agent-runtime", agent_runtime_pid_before),
    ] {
        let snap = manager.state().lock().await.unit(untouched).unwrap();
        assert_eq!(snap.status, UnitStatus::Ready);
        assert_eq!(snap.pid, Some(expected_pid));
        assert_eq!(snap.generation, 1);
        assert_eq!(snap.restarts, 0);
    }

    // Active interactive session and local-entry shell remain healthy.
    assert_eq!(
        read_trimmed(&shell, &format!("/proc/{}/status", session.pid.0)).await,
        "running"
    );
    assert!(shell.ls(&session.agent_path).await.is_ok());
    assert!(shell.ls("/agent/root").await.is_ok());

    // Pre-restart fid walked against /mnt/package/status must fail cleanly with ErrorCode::Io.
    assert_eq!(
        transport.call(Request::Stat { fid: Fid(201) }).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport
            .call(Request::Open {
                fid: Fid(201),
                mode: OpenMode::Read,
            })
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport.call(Request::Clunk { fid: Fid(201) }).await,
        Err(ErrorCode::Io)
    );

    // Fresh walk over the same namespace binds transparently to generation 2.
    assert!(shell.cat("/mnt/package/status").await.is_ok());

    // Telemetry projections under /mnt/service-manager/units/package/ reflect the recovery.
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/package/status").await,
        "ready"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/package/pid").await,
        new_package_pid.to_string()
    );
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

    session.close().await.unwrap();
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn opt_in_rest_for_one_restarts_downstream_dependents_in_topological_order() {
    let manifest = BootManifest::system()
        .unwrap()
        .with_unit_supervision("agent-runtime", SupervisionStrategy::RestForOne)
        .unwrap();
    let config = ephemeral_config().with_boot_manifest(manifest);
    let manager = ServiceManager::boot(config).await.unwrap();

    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = Shell::new(InProcessTransport::new(namespace));

    let old_ar_pid = manager
        .state()
        .lock()
        .await
        .unit("agent-runtime")
        .unwrap()
        .pid
        .unwrap();
    let old_le_pid = manager
        .state()
        .lock()
        .await
        .unit("local-entry")
        .unwrap()
        .pid
        .unwrap();
    let old_root_pid = manager.root_pid().0;
    let mut upstream_pids = Vec::new();
    for name in ["connection", "package", "route", "host-mount"] {
        let pid = manager
            .state()
            .lock()
            .await
            .unit(name)
            .unwrap()
            .pid
            .unwrap();
        upstream_pids.push((name, pid));
    }

    // Crash `agent-runtime`, which is opted into RestForOne.
    manager.terminate_unit("agent-runtime", 1).await.unwrap();

    // Wait for `agent-runtime` and its downstream dependents (`local-entry`, `root-agent`) to recover.
    let new_ar_pid =
        wait_for_unit_ready_with_generation(&manager, "agent-runtime", 2, Some(old_ar_pid)).await;
    let new_le_pid =
        wait_for_unit_ready_with_generation(&manager, "local-entry", 2, Some(old_le_pid)).await;
    let new_root_pid =
        wait_for_unit_ready_with_generation(&manager, "root-agent", 2, Some(old_root_pid)).await;

    assert_ne!(new_ar_pid, old_ar_pid);
    assert_ne!(new_le_pid, old_le_pid);
    assert_ne!(new_root_pid, old_root_pid);
    assert_eq!(manager.root_pid(), Pid(new_root_pid));
    assert_eq!(manager.state().lock().await.status(), SystemStatus::Ready);

    // Downstream dependents must reflect cascade restart telemetry without counting as failures.
    for dep in ["local-entry", "root-agent"] {
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{dep}/generation")
            )
            .await,
            "2"
        );
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{dep}/restarts")
            )
            .await,
            "1"
        );
        assert_eq!(
            read_trimmed(
                &shell,
                &format!("/mnt/service-manager/units/{dep}/last_exit_reason")
            )
            .await,
            "rest-for-one cascade (agent-runtime)"
        );
    }
    assert_eq!(
        read_trimmed(
            &shell,
            "/mnt/service-manager/units/agent-runtime/last_exit_reason"
        )
        .await,
        "failed (exit 1)"
    );

    // Upstream and unrelated units must remain untouched.
    for (name, pid) in upstream_pids {
        let snap = manager.state().lock().await.unit(name).unwrap();
        assert_eq!(snap.status, UnitStatus::Ready);
        assert_eq!(snap.pid, Some(pid));
        assert_eq!(snap.generation, 1);
        assert_eq!(snap.restarts, 0);
        assert_eq!(snap.last_exit_reason, None);
    }

    // Rebound local-entry can hand off a fresh client namespace cleanly after the cascade.
    let (_, _, fresh_ns) = manager.local_entry().create_and_handoff().await.unwrap();
    let fresh_shell = Shell::new(InProcessTransport::new(fresh_ns));
    assert!(fresh_shell.ls("/agent/root").await.is_ok());

    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn opt_in_rest_for_one_cascades_transitively_across_multi_hop_dependents() {
    // Opt `package` into RestForOne: its transitive downstream dependents are
    // `agent-runtime` -> `local-entry` -> `root-agent`, while `connection`, `route`,
    // and `host-mount` are independent.
    let manifest = BootManifest::system()
        .unwrap()
        .with_unit_supervision("package", SupervisionStrategy::RestForOne)
        .unwrap();
    let manager = ServiceManager::boot(ephemeral_config().with_boot_manifest(manifest))
        .await
        .unwrap();

    let mut old_pids = std::collections::BTreeMap::new();
    for name in [
        "package",
        "agent-runtime",
        "local-entry",
        "root-agent",
        "connection",
        "route",
        "host-mount",
    ] {
        let pid = manager
            .state()
            .lock()
            .await
            .unit(name)
            .unwrap()
            .pid
            .unwrap();
        old_pids.insert(name, pid);
    }

    manager.terminate_unit("package", 1).await.unwrap();

    for cascaded in ["package", "agent-runtime", "local-entry", "root-agent"] {
        let new_pid =
            wait_for_unit_ready_with_generation(&manager, cascaded, 2, Some(old_pids[cascaded]))
                .await;
        assert_ne!(new_pid, old_pids[cascaded]);
    }
    for independent in ["connection", "route", "host-mount"] {
        let snap = manager.state().lock().await.unit(independent).unwrap();
        assert_eq!(snap.pid, Some(old_pids[independent]));
        assert_eq!(snap.generation, 1);
        assert_eq!(snap.restarts, 0);
    }

    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn unit_telemetry_projects_generation_restarts_backoff_until_and_cancelled_reason() {
    // Use a custom manifest with a larger backoff window on `route` so we can deterministically
    // observe `backoff_until` while the unit is in `backoff`, plus test `/proc/<pid>/ctl` cancel.
    let mut docs: Vec<String> = BootManifest::documents()
        .into_iter()
        .map(|(_, doc)| doc.to_string())
        .collect();
    for doc in &mut docs {
        if doc.contains("name = \"route\"") {
            *doc = doc.replace("initial_backoff_ms = 50", "initial_backoff_ms = 200");
        }
    }
    let leaked: Vec<&'static str> = docs
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &'static str)
        .collect();
    let manifest = BootManifest::parse(leaked).unwrap();

    let manager = ServiceManager::boot(ephemeral_config().with_boot_manifest(manifest))
        .await
        .unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = Shell::new(InProcessTransport::new(namespace));

    // Directory listing preserves existing files and exposes the four new telemetry files.
    let entries = shell.ls("/mnt/service-manager/units/route").await.unwrap();
    for expected in [
        "status",
        "pid",
        "attempts",
        "error",
        "generation",
        "restarts",
        "backoff_until",
        "last_exit_reason",
    ] {
        assert!(
            entries.iter().any(|entry| entry == expected),
            "missing `{expected}` in unit directory listing: {entries:?}"
        );
    }

    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/generation").await,
        "1"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/restarts").await,
        "0"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/backoff_until").await,
        ""
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/last_exit_reason").await,
        ""
    );

    // Cancel `route` via `/proc/<pid>/ctl` so `/proc/<pid>/wait` reports ExitReason::Cancelled.
    let old_pid = manager
        .state()
        .lock()
        .await
        .unit("route")
        .unwrap()
        .pid
        .unwrap();
    shell
        .write(&format!("/proc/{old_pid}/ctl"), b"cancel")
        .await
        .unwrap();

    // Observe the unit in `backoff` with a valid RFC3339 `backoff_until` and `cancelled (exit 130)`.
    let backoff_until = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let status = read_trimmed(&shell, "/mnt/service-manager/units/route/status").await;
            if status == "backoff" {
                return read_trimmed(&shell, "/mnt/service-manager/units/route/backoff_until")
                    .await;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("route did not enter backoff state");
    assert!(
        chrono::DateTime::parse_from_rfc3339(&backoff_until).is_ok(),
        "backoff_until should be valid RFC3339, got `{backoff_until}`"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/last_exit_reason").await,
        "cancelled (exit 130)"
    );

    // Wait for backoff to expire and unit to recover cleanly.
    let new_pid = wait_for_unit_ready_with_generation(&manager, "route", 2, Some(old_pid)).await;
    assert_ne!(new_pid, old_pid);
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/status").await,
        "ready"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/generation").await,
        "2"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/restarts").await,
        "1"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/backoff_until").await,
        ""
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/last_exit_reason").await,
        "cancelled (exit 130)"
    );

    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn ctl_retry_wakes_event_driven_supervisor_without_polling() {
    // Configure `route` with restart_limit = 0 so a single crash transitions it to Failed,
    // then issue `retry route` on `/mnt/service-manager/ctl` and confirm immediate recovery.
    let mut docs: Vec<String> = BootManifest::documents()
        .into_iter()
        .map(|(_, doc)| doc.to_string())
        .collect();
    for doc in &mut docs {
        if doc.contains("name = \"route\"") {
            *doc = doc.replace("restart_limit = 5", "restart_limit = 0");
        }
    }
    let leaked: Vec<&'static str> = docs
        .into_iter()
        .map(|s| Box::leak(s.into_boxed_str()) as &'static str)
        .collect();
    let manifest = BootManifest::parse(leaked).unwrap();

    let manager = ServiceManager::boot(ephemeral_config().with_boot_manifest(manifest))
        .await
        .unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = Shell::new(InProcessTransport::new(namespace));

    let old_pid = manager
        .state()
        .lock()
        .await
        .unit("route")
        .unwrap()
        .pid
        .unwrap();
    manager.terminate_unit("route", 1).await.unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if read_trimmed(&shell, "/mnt/service-manager/units/route/status").await == "failed" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("route did not enter failed state");
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/degraded").await,
        "true"
    );

    shell
        .write("/mnt/service-manager/ctl", b"retry route")
        .await
        .unwrap();

    let new_pid = wait_for_unit_ready_with_generation(&manager, "route", 2, Some(old_pid)).await;
    assert_ne!(new_pid, old_pid);
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/degraded").await,
        "false"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/generation").await,
        "2"
    );
    assert_eq!(
        read_trimmed(&shell, "/mnt/service-manager/units/route/restarts").await,
        "1"
    );

    manager.shutdown().await.unwrap();
}
