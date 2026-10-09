use super::*;
use alan_service_manager::HostMountAccess;

#[tokio::test]
async fn same_agent_clients_keep_results_and_ordered_cwd_after_targeted_cancel() {
    let runtime = tempfile::tempdir().unwrap();
    let first_project = tempfile::tempdir().unwrap();
    let second_project = tempfile::tempdir().unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path()).unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    let provider = MockLlmProvider::new();
    let probe = provider.clone();
    let process = AgentProcessConfig {
        store_bindings: Some(AgentRuntimeStoreBindings {
            rollouts: runtime.path().join("rollouts"),
            checkpoints: runtime.path().join("checkpoints"),
            cache: runtime.path().join("cache"),
            tmp: runtime.path().join("tmp"),
            metadata: runtime.path().join("metadata"),
        }),
        ..AgentProcessConfig::default()
    };
    let host = AlanOsHost::boot(
        HostBootConfig::ephemeral(process, LlmClient::new(provider), tools),
        paths.clone(),
    )
    .await
    .unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let attachment = LocalAttachment::new(paths.clone());
    let a = Shell::new(attachment.connect().await.unwrap().root);
    let b = Shell::new(attachment.connect().await.unwrap().root);
    let control = HostCommandPlane::new(paths.clone());
    let boot = paths.read_status().unwrap().boot_id;
    let first = control
        .mount_project(
            uuid::Uuid::new_v4(),
            boot,
            first_project.path().to_owned(),
            HostMountAccess::ReadWrite,
        )
        .await
        .unwrap();
    assert_eq!(
        command(&a, &format!("cd {}", first.grant.namespace_path)).await["exit_code"],
        0
    );

    // Identical text must execute once per ID and remain independently readable.
    let body = "printf x >> count; wc -c < count";
    let (a_id, b_id) = tokio::join!(submit_command(&a, body), submit_command(&b, body));
    assert_ne!(a_id, b_id);
    let (a_result, b_result) = tokio::join!(command_result(&a, &a_id), command_result(&b, &b_id));
    assert_eq!(a_result["call_id"], a_id);
    assert_eq!(b_result["call_id"], b_id);
    assert_eq!(a_result["exit_code"], 0, "{a_result}");
    assert_eq!(b_result["exit_code"], 0, "{b_result}");
    assert_ne!(a_result["process"], b_result["process"]);
    let mut counts = [
        a_result["output"]["stdout"].as_str().unwrap().trim(),
        b_result["output"]["stdout"].as_str().unwrap().trim(),
    ];
    counts.sort();
    assert_eq!(counts, ["1", "2"]);
    assert_eq!(
        std::fs::read(first_project.path().join("count")).unwrap(),
        b"xx"
    );
    assert_eq!(command_result(&b, &a_id).await, a_result);
    assert_eq!(command_result(&a, &b_id).await, b_result);
    let failure = submit_command(&b, "printf failure >&2; exit 7").await;
    let failed = command_result(&a, &failure).await;
    assert_eq!(failed["call_id"], failure);
    assert_eq!(failed["exit_code"], 7);
    assert_eq!(failed["output"]["stderr"], "failure");
    assert_eq!(command_result(&b, &failure).await, failed);
    assert_eq!(command_result(&a, &a_id).await, a_result);

    let running = submit_command(&a, "printf started > started; sleep 30").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !first_project.path().join("started").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let second = control
        .mount_project(
            uuid::Uuid::new_v4(),
            boot,
            second_project.path().to_owned(),
            HostMountAccess::ReadWrite,
        )
        .await
        .unwrap();
    let cd = submit_command(&b, &format!("cd {}", second.grant.namespace_path)).await;
    wait_pending_input(&a, &cd, false).await;
    let after_cd = submit_command(&a, "printf y >> count").await;
    b.write(
        "/agent/root/machine/ctl",
        format!("queue-v1 interrupt {running}").as_bytes(),
    )
    .await
    .unwrap();
    assert_ne!(command_result(&b, &running).await["exit_code"], 0);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let activity: Value =
                serde_json::from_slice(&a.cat("/agent/root/machine/ui/activity").await.unwrap())
                    .unwrap();
            let queue: Value =
                serde_json::from_slice(&a.cat("/agent/root/machine/ui/queue").await.unwrap())
                    .unwrap();
            if activity["state"] == "paused"
                && queue["known"] == true
                && queue["paused"] == true
                && queue["pending_submission_ids"] == json!([cd, after_cd])
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    a.write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    assert_eq!(command_result(&a, &cd).await["exit_code"], 0);
    let after = command_result(&b, &after_cd).await;
    let read_isolated = matches!(
        alan_agent_engine::tools::active_backend_name(),
        "seatbelt" | "linux_reified_namespace"
    );
    if read_isolated {
        assert_eq!(after["exit_code"], 0, "{after}");
        assert_eq!(
            std::fs::read(second_project.path().join("count")).unwrap(),
            b"y"
        );
    } else {
        assert_ne!(after["exit_code"], 0, "{after}");
        assert_eq!(
            after["output"]["error"],
            "This backend cannot isolate inactive Host Mount reads"
        );
        assert!(!second_project.path().join("count").exists());
        // Retire the inactive grant so later single-grant cancellation is executable.
        control.revoke_host_mount(first.grant.id).await.unwrap();
    }
    assert_eq!(
        std::fs::read(first_project.path().join("count")).unwrap(),
        b"xx"
    );

    let running = submit_command(
        &a,
        "printf saved > cancelled; sleep 30; printf leaked > late",
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !second_project.path().join("cancelled").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let queued = submit_command(&b, "printf forbidden > discarded").await;
    wait_pending_input(&a, &queued, false).await;
    b.write(
        "/agent/root/machine/ctl",
        format!("queue-v1 interrupt {running}").as_bytes(),
    )
    .await
    .unwrap();
    let cancelled = command_result(&a, &running).await;
    assert_ne!(cancelled["exit_code"], 0, "{cancelled}");
    assert_eq!(
        b.cat(&format!(
            "{}/status",
            cancelled["process"].as_str().unwrap()
        ))
        .await
        .unwrap(),
        b"exited\n"
    );
    wait_pending_input(&b, &queued, true).await;
    a.write("/agent/root/machine/ctl", b"queue-v1 discard")
        .await
        .unwrap();
    let discarded = command_result(&b, &queued).await;
    assert_eq!(discarded["exit_code"], 1);
    assert_eq!(discarded["process"], "");
    let events = String::from_utf8(a.cat("/agent/root/machine/ui/events").await.unwrap()).unwrap();
    for id in [&running, &queued] {
        assert!(
            events.lines().any(|line| {
                let event: Value = serde_json::from_str(line).unwrap();
                event["type"] == "input_completed"
                    && event["status"] == "cancelled"
                    && event["submission_ids"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|candidate| candidate == id))
            }),
            "missing correlated cancellation for {id}: {events}"
        );
    }
    assert_eq!(
        std::fs::read(second_project.path().join("cancelled")).unwrap(),
        b"saved"
    );
    assert!(!second_project.path().join("late").exists());
    assert!(!second_project.path().join("discarded").exists());
    assert!(probe.recorded_requests().is_empty());
    stop.cancel();
    server.await.unwrap().unwrap();
}
