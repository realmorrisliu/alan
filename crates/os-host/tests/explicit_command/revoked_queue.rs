use super::*;
use alan_service_manager::HostMountAccess;

#[tokio::test]
async fn paused_native_command_cannot_reuse_revoked_cwd_authority() {
    let runtime = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let readonly = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("nested")).unwrap();
    std::fs::write(readonly.path().join("manual"), "approved read").unwrap();
    std::fs::write(outside.path().join("secret"), "private").unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join("escape")).unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(runtime.path(), "test").unwrap();
    let mut tools = ToolRegistry::new();
    tools.register(alan_tools::BashTool::new());
    tools.register(alan_tools::ReadFileTool::new());
    tools.register(alan_tools::WriteFileTool::new());
    tools.register(alan_tools::EditFileTool::new());
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
        HostBootConfig::ephemeral("test", process, LlmClient::new(provider), tools),
        paths.clone(),
    )
    .await
    .unwrap();
    let stop = CancellationToken::new();
    let stopped = stop.clone();
    let server = tokio::spawn(async move { host.serve_until(stopped.cancelled_owned()).await });
    let shell = Shell::new(
        LocalAttachment::new(paths.clone())
            .connect()
            .await
            .unwrap()
            .root,
    );
    let control = HostCommandPlane::new(paths.clone());
    let mounted = control
        .mount_project(
            uuid::Uuid::new_v4(),
            paths.read_status().unwrap().boot_id,
            project.path().to_owned(),
            HostMountAccess::ReadWrite,
        )
        .await
        .unwrap();
    let selected = command(&shell, &format!("cd {}", mounted.grant.namespace_path)).await;
    assert_eq!(selected["exit_code"], 0, "{selected}");
    let boot = paths.read_status().unwrap().boot_id;
    let second = control
        .mount_project(
            uuid::Uuid::new_v4(),
            boot,
            other.path().to_owned(),
            HostMountAccess::ReadWrite,
        )
        .await
        .unwrap();
    let ro = control
        .mount_project(
            uuid::Uuid::new_v4(),
            boot,
            readonly.path().to_owned(),
            HostMountAccess::ReadOnly,
        )
        .await
        .unwrap();
    let mut generated = response();
    let read_isolated = matches!(
        alan_agent_engine::tools::active_backend_name(),
        "seatbelt" | "linux_reified_namespace"
    );
    eprintln!(
        "cross-grant qualification backend: {}",
        alan_agent_engine::tools::active_backend_name()
    );
    for (id, name, args) in [
        (
            "generated-shell",
            "bash",
            json!({"command":"cd nested && printf generated > agent-shell"}),
        ),
        (
            "write-other",
            "write_file",
            json!({"path":format!("{}/note", second.grant.namespace_path),"content":"shared"}),
        ),
        (
            "read-ro",
            "read_file",
            json!({"path":format!("{}/manual", ro.grant.namespace_path)}),
        ),
        (
            "write-ro",
            "write_file",
            json!({"path":format!("{}/manual", ro.grant.namespace_path),"content":"forbidden"}),
        ),
        (
            "read-escape",
            "read_file",
            json!({"path":format!("{}/escape/secret", mounted.grant.namespace_path)}),
        ),
        (
            "write-escape",
            "write_file",
            json!({"path":format!("{}/escape/secret", mounted.grant.namespace_path),"content":"forbidden"}),
        ),
    ] {
        generated.tool_calls.push(ToolCall {
            id: Some(id.into()),
            name: name.into(),
            arguments: args,
        });
    }
    probe.clone().with_responses(vec![generated, response()]);
    let edits = submit_input(
        &shell,
        "agent",
        "edit another authorized project and check read-only and symlink boundaries",
    )
    .await;
    wait_input(&shell, &edits).await;
    let generated_shell = command_result(&shell, "generated-shell").await;
    if read_isolated {
        assert_eq!(generated_shell["exit_code"], 0, "{generated_shell}");
        // The Agent's script-local cd must not change the shared Process cwd.
        assert_eq!(
            command(&shell, "cat nested/agent-shell").await["output"]["stdout"],
            "generated"
        );
    } else {
        assert_ne!(generated_shell["exit_code"], 0, "{generated_shell}");
        assert_eq!(
            generated_shell["output"]["error"],
            "This backend cannot isolate inactive Host Mount reads"
        );
        assert!(!project.path().join("nested/agent-shell").exists());
    }
    assert_eq!(command_result(&shell, "write-other").await["exit_code"], 0);
    assert_eq!(
        command_result(&shell, "read-ro").await["output"]["content"],
        "approved read"
    );
    for id in ["write-ro", "read-escape", "write-escape"] {
        let failed = command_result(&shell, id).await;
        assert_ne!(failed["exit_code"], 0, "{failed}");
    }
    assert_eq!(std::fs::read(other.path().join("note")).unwrap(), b"shared");
    assert_eq!(
        std::fs::read(readonly.path().join("manual")).unwrap(),
        b"approved read"
    );
    assert_eq!(
        std::fs::read(outside.path().join("secret")).unwrap(),
        b"private"
    );
    let native_other = command(
        &shell,
        &format!("cat '{}'", other.path().join("note").display()),
    )
    .await;
    assert_ne!(native_other["exit_code"], 0, "{native_other}");
    assert_eq!(
        command(&shell, &format!("cd {}", second.grant.namespace_path)).await["exit_code"],
        0
    );
    let native_read = command(&shell, "cat note").await;
    let expected = if read_isolated {
        assert_eq!(native_read["output"]["stdout"], "shared", "{native_read}");
        assert_eq!(
            command(&shell, "printf native > note").await["exit_code"],
            0
        );
        "native"
    } else {
        assert_ne!(native_read["exit_code"], 0, "{native_read}");
        assert_eq!(
            native_read["output"]["error"],
            "This backend cannot isolate inactive Host Mount reads"
        );
        "shared"
    };
    let mut read = response();
    read.tool_calls.push(ToolCall {
        id: Some("read-other".into()),
        name: "read_file".into(),
        arguments: json!({"path":"note"}),
    });
    probe.clone().with_responses(vec![read, response()]);
    let read = submit_input(&shell, "agent", "read the native edit").await;
    wait_input(&shell, &read).await;
    assert_eq!(
        command_result(&shell, "read-other").await["output"]["content"],
        expected
    );
    assert_eq!(
        command(&shell, &format!("cd {}", ro.grant.namespace_path)).await["exit_code"],
        0
    );
    let native_read = command(&shell, "cat manual").await;
    if read_isolated {
        assert_eq!(
            native_read["output"]["stdout"], "approved read",
            "{native_read}"
        );
    } else {
        assert_ne!(native_read["exit_code"], 0, "{native_read}");
        assert_eq!(
            native_read["output"]["error"],
            "This backend cannot isolate inactive Host Mount reads"
        );
    }
    let ro_write = command(&shell, "printf forbidden > manual").await;
    assert_ne!(ro_write["exit_code"], 0, "{ro_write}");
    assert_eq!(
        std::fs::read(readonly.path().join("manual")).unwrap(),
        b"approved read"
    );
    assert_eq!(
        command(&shell, &format!("cd {}", mounted.grant.namespace_path)).await["exit_code"],
        0
    );
    control.revoke_host_mount(ro.grant.id).await.unwrap();
    let mut read = response();
    read.tool_calls.push(ToolCall {
        id: Some("read-revoked".into()),
        name: "read_file".into(),
        arguments: json!({"path":format!("{}/manual", ro.grant.namespace_path)}),
    });
    probe.clone().with_responses(vec![read, response()]);
    let revoked = submit_input(&shell, "agent", "read the revoked grant").await;
    wait_input(&shell, &revoked).await;
    let failed = command_result(&shell, "read-revoked").await;
    assert_ne!(failed["exit_code"], 0, "{failed}");
    assert_eq!(
        std::fs::read(readonly.path().join("manual")).unwrap(),
        b"approved read"
    );

    // The remaining action uses one grant, including on safely degraded backends.
    control.revoke_host_mount(second.grant.id).await.unwrap();
    let generation_count = probe.recorded_requests().len();
    let running = submit_command(&shell, "printf saved > completed; sleep 30").await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !project.path().join("completed").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native action must start before interruption");
    let queued = submit_command(&shell, "printf forbidden >> queued-effect").await;
    wait_pending_input(&shell, &queued, false).await;
    control.revoke_host_mount(mounted.grant.id).await.unwrap();
    shell
        .write("/agent/root/machine/ctl", b"interrupt")
        .await
        .unwrap();
    assert_ne!(command_result(&shell, &running).await["exit_code"], 0);
    wait_pending_input(&shell, &queued, true).await;
    shell
        .write("/agent/root/machine/ctl", b"queue-v1 continue")
        .await
        .unwrap();
    let rejected = command_result(&shell, &queued).await;
    assert_ne!(rejected["exit_code"], 0, "{rejected}");
    let process = rejected["process"].as_str().unwrap();
    assert_eq!(
        shell.cat(&format!("{process}/status")).await.unwrap(),
        b"exited\n"
    );
    assert!(
        rejected["result_preview"]
            .as_str()
            .is_some_and(|text| text.contains("Process cwd grant was revoked or replaced")),
        "{rejected}"
    );
    assert!(!project.path().join("queued-effect").exists());
    assert_eq!(
        std::fs::read(project.path().join("completed")).unwrap(),
        b"saved"
    );
    assert_eq!(probe.recorded_requests().len(), generation_count);
    stop.cancel();
    server.await.unwrap().unwrap();
}
