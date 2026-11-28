use alan_ap::{ErrorCode, Fid, OpenMode, Request, Response};

use super::*;

#[tokio::test]
async fn all_srv_handles_use_supervised_handle_and_reject_stale_fids_after_restart() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();

    let expected_handles = [
        "service-manager",
        "route",
        "connection",
        "llm",
        "package",
        "agent-runtime",
        "host-mount",
        "local-entry",
    ];
    for name in expected_handles {
        let handle = manager
            .supervised_handles
            .get(name)
            .unwrap_or_else(|| panic!("missing SupervisedHandle for /srv/{name}"));
        assert!(handle.is_bound(), "/srv/{name} should be bound after boot");
        assert_eq!(
            handle.generation(),
            1,
            "/srv/{name} should start at generation 1"
        );
    }

    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let transport = InProcessTransport::new(namespace);

    // Walk a fid against a non-package service mount (/mnt/connections/status) before restart.
    assert!(matches!(
        transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(101),
                names: vec!["mnt".into(), "connections".into(), "status".into()],
            })
            .await,
        Ok(Response::Walk { .. })
    ));

    let old_pid = Pid(manager
        .state()
        .lock()
        .await
        .unit("connection")
        .unwrap()
        .pid
        .unwrap());
    manager.terminate_unit("connection", 1).await.unwrap();

    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let unit = manager.state().lock().await.unit("connection").unwrap();
            if unit.status == crate::UnitStatus::Ready && unit.pid != Some(old_pid.0) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Connection Service was not restarted");

    // Both handles published by `connection` incremented generation; unrelated handles stayed at 1.
    assert_eq!(manager.supervised_handles["connection"].generation(), 2);
    assert_eq!(manager.supervised_handles["llm"].generation(), 2);
    for untouched in [
        "service-manager",
        "route",
        "package",
        "agent-runtime",
        "host-mount",
        "local-entry",
    ] {
        assert_eq!(
            manager.supervised_handles[untouched].generation(),
            1,
            "untouched /srv/{untouched} generation should remain 1"
        );
    }

    // Pre-restart fid against /mnt/connections/status fails cleanly with ErrorCode::Io.
    assert_eq!(
        transport.call(Request::Stat { fid: Fid(101) }).await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport
            .call(Request::Open {
                fid: Fid(101),
                mode: OpenMode::Read,
            })
            .await,
        Err(ErrorCode::Io)
    );
    assert_eq!(
        transport.call(Request::Clunk { fid: Fid(101) }).await,
        Err(ErrorCode::Io)
    );

    // Fresh walk over the same existing namespace binds transparently to generation 2.
    let shell = alan_shell::Shell::new(transport);
    let status = String::from_utf8(shell.cat("/mnt/connections/status").await.unwrap()).unwrap();
    assert!(
        status.starts_with("profiles="),
        "unexpected status: {status}"
    );
    assert!(
        shell
            .ls("/mnt/llm/connections")
            .await
            .unwrap()
            .iter()
            .any(|name| name == "default")
    );

    let handles_snapshot = manager.supervised_handles.clone();
    manager.shutdown().await.unwrap();
    for (name, handle) in handles_snapshot {
        assert!(
            !handle.is_bound(),
            "/srv/{name} handle should be deactivated on shutdown"
        );
    }
}
