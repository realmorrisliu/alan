use super::*;

#[test]
fn test_runtime_host_capabilities_enable_delegated_support_for_top_level_runtime() {
    let config = AgentProcessConfig::default();
    let tools = crate::tools::ToolRegistry::new();

    let capabilities = runtime_host_capabilities(&config, &tools);

    assert!(capabilities.supports_delegated_skill_invocation());
    assert!(capabilities.tools.contains("invoke_delegated_skill"));
}

#[test]
fn test_runtime_host_capabilities_include_host_path_executables() {
    let temp = tempfile::TempDir::new().unwrap();
    let executable_path = {
        #[cfg(windows)]
        {
            temp.path().join("demo.cmd")
        }

        #[cfg(not(windows))]
        {
            temp.path().join("demo")
        }
    };
    std::fs::write(&executable_path, "echo demo\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable_path, permissions).unwrap();
    }

    let capabilities = runtime_host_capabilities_with_path_dirs(
        &AgentProcessConfig::default(),
        &crate::tools::ToolRegistry::new(),
        [temp.path()],
    );

    assert!(capabilities.supports_required_tool("demo"));
}

#[test]
fn test_agent_runtime_handle_clone() {
    let (sub_tx, _sub_rx) = mpsc::channel(10);

    let handle = RuntimeHandle {
        submission_tx: sub_tx,
        shutdown_tx: None,
    };

    let cloned = handle.clone();
    // Both handles should share the same channels
    drop(cloned);
    drop(handle);
}

#[test]
fn test_agent_runtime_handle_fields() {
    let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);

    let handle = RuntimeHandle {
        submission_tx: sub_tx,
        shutdown_tx: None,
    };

    // Verify handle can be created
    assert!(!handle.submission_tx.is_closed());
}

#[tokio::test]
async fn test_agent_runtime_handle_shutdown_without_channel() {
    let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);
    let handle = RuntimeHandle {
        submission_tx: sub_tx,
        shutdown_tx: None,
    };

    let result = handle.shutdown().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_runtime_handle_shutdown_with_channel() {
    let (sub_tx, _sub_rx) = mpsc::channel::<Submission>(10);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    let handle = RuntimeHandle {
        submission_tx: sub_tx,
        shutdown_tx: Some(shutdown_tx),
    };

    // Shutdown should send signal
    let result = handle.shutdown().await;
    assert!(result.is_ok());

    // Verify shutdown signal was sent
    let signal = shutdown_rx.recv().await;
    assert!(signal.is_some());
}

#[tokio::test]
async fn test_initialize_agent_machine_from_rollout_preserves_current_process_cwd() {
    let temp = TempDir::new().unwrap();
    let process_cwd = std::path::Path::new("/mnt/source/src");
    let recovered_rollouts = temp.path().join("recovered-rollouts");
    let mut source =
        AgentMachine::new_with_recorder_in_dir("/proc/41", "gemini-2.0-flash", temp.path())
            .await
            .unwrap();
    source.add_user_message("Hello");
    source.flush().await;
    let rollout_path = source.rollout_path().unwrap().clone();
    drop(source);

    let startup = initialize_agent_machine(
        AgentMachineLaunchContext {
            process_path: "/proc/42",
            agent_path: "/agent/42",
            model: "gemini-2.0-flash",
        },
        Some(&rollout_path),
        Some(&recovered_rollouts),
        true,
        Some(process_cwd),
        crate::ResolvedRequestControls {
            reasoning: alan_agent_protocol::ReasoningControls {
                effort: Some(alan_agent_protocol::ReasoningEffort::Medium),
            },
            source: crate::RequestControlSource::AgentMachineOverride,
            diagnostics: Vec::new(),
        },
    )
    .await
    .unwrap();

    assert_eq!(startup.metadata.process_path, "/proc/42");
    assert_eq!(startup.metadata.agent_path, "/agent/42");
    assert!(startup.metadata.rollout_id.is_some());

    let persisted_path = startup
        .metadata
        .rollout_path
        .clone()
        .expect("recovered machine should create a new rollout recorder");
    let persisted_items = crate::rollout::RolloutRecorder::load_history(&persisted_path)
        .await
        .unwrap();
    let persisted_meta = persisted_items.into_iter().find_map(|item| match item {
        crate::rollout::RolloutItem::AgentMachineMeta(meta) => Some(meta),
        _ => None,
    });

    assert_eq!(
        persisted_meta.as_ref().map(|meta| meta.cwd.as_str()),
        Some("/mnt/source/src")
    );
    assert_eq!(
        persisted_meta
            .as_ref()
            .map(|meta| meta.process_path.as_str()),
        Some("/proc/42")
    );
    assert_eq!(
        persisted_meta
            .as_ref()
            .and_then(|meta| meta.reasoning_effort),
        Some(alan_agent_protocol::ReasoningEffort::Medium)
    );

    drop(startup);
    let _ = tokio::fs::remove_file(persisted_path).await;
}
