use super::*;
use alan_kernel::ExecSpec;

#[tokio::test]
async fn spawn_child_runtime_inherits_namespace_tools_but_not_optional_handles() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        crate::skills::ResolvedCapabilityView::default(),
    );
    let root_dir = temp.path().join("definition");
    let spec = launch_spec(root_dir);

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "Child finished cleanly.");
    let recorded = requests.0.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    let request = &recorded[0];
    assert!(request.tools.iter().any(|tool| tool.name == "alpha"));
    assert!(request.tools.iter().any(|tool| tool.name == "beta"));
    let user_text = request
        .messages
        .iter()
        .map(|message| message.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(user_text.contains("Review the repository changes"));
    assert!(!user_text.contains("Parent Conversation Snapshot"));
    assert!(!user_text.contains("Parent Plan Snapshot"));
    assert!(!user_text.contains("Parent Tool Results"));
}

#[tokio::test]
async fn spawn_child_runtime_reuses_the_passed_callable_connection() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Shared Connection completed the child.");
    let parent = make_parent_state(&temp, requests.clone(), response);

    let child = spawn_child_runtime(&parent, launch_spec(temp.path().join("definition")))
        .await
        .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "Shared Connection completed the child.");
    assert_eq!(requests.0.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn spawn_child_runtime_binds_requested_parent_handles() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Bound handles processed.");
    let parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        crate::skills::ResolvedCapabilityView::default(),
    );
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.handles = vec![
        SpawnHandle::ConversationSnapshot,
        SpawnHandle::Plan,
        SpawnHandle::ToolResults,
    ];

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    let recorded = requests.0.lock().unwrap();
    let user_text = recorded
        .iter()
        .flat_map(|request| {
            request
                .messages
                .iter()
                .map(|message| message.content.clone())
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(user_text.contains("Parent Conversation Snapshot"));
    assert!(user_text.contains("Parent Plan Snapshot"));
    assert!(user_text.contains("Parent Tool Results"));
    assert!(user_text.contains("Inspect the changed files"));
    assert!(user_text.contains("tool output"));
}

#[tokio::test]
async fn spawn_child_runtime_rejects_artifact_handle_without_runtime_binding() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Artifacts are not supported.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.handles = vec![SpawnHandle::Artifacts];

    let err = match spawn_child_runtime_with_client_factory(&parent, spec, |_| unreachable!()).await
    {
        Ok(_) => panic!("artifact handle should be rejected until artifact routing exists"),
        Err(err) => err,
    };

    assert!(
        err.to_string()
            .contains("Child Agent Process launches do not support artifact routing yet")
    );
}

#[tokio::test]
async fn spawn_child_runtime_rejects_output_dir_without_runtime_binding() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Artifacts are not supported.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.launch.output_dir = Some(temp.path().join("repo/out"));

    let err = match spawn_child_runtime_with_client_factory(&parent, spec, |_| unreachable!()).await
    {
        Ok(_) => panic!("output_dir should be rejected until artifact routing exists"),
        Err(err) => err,
    };

    assert!(
        err.to_string()
            .contains("Child Agent Process launches do not support artifact routing yet")
    );
}

#[tokio::test]
async fn spawn_child_runtime_filters_namespace_tools_with_override() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Only one tool should be visible.");
    let parent = make_parent_state_with_capability_view(
        &temp,
        requests.clone(),
        response.clone(),
        crate::skills::ResolvedCapabilityView::default(),
    );
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    let recorded = requests.0.lock().unwrap();
    let tool_names = recorded[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"alpha"));
    assert!(!tool_names.contains(&"beta"));
}

#[tokio::test]
async fn spawn_child_runtime_respects_empty_namespace_tool_override() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("No tools should be visible.");
    let parent = make_parent_state(&temp, requests.clone(), response.clone());
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: Vec::new(),
    });

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |_| {
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    let recorded = requests.0.lock().unwrap();
    let tool_names = recorded[0]
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    assert!(!tool_names.contains(&"alpha"));
    assert!(!tool_names.contains(&"beta"));
}

#[tokio::test]
async fn child_namespace_plan_mounts_only_allowed_tools() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });

    let launch_context = parent.child_launch().launch_context().unwrap().child();
    let plan =
        build_child_namespace_assembly_plan(&parent, &spec, &parent.core_config, launch_context)
            .await
            .unwrap();

    assert_eq!(plan.llm_mount, "/mnt/llm");
    assert_eq!(plan.llm_connection_name().unwrap(), "default");
    assert_eq!(plan.srv_mount, "/srv");
    assert_eq!(plan.route_mount, "/mnt/route");
    assert_eq!(plan.cwd, Some(PathBuf::from("/mnt/source")));
    assert_eq!(plan.launch_context.cwd, "/mnt/source");
    assert_eq!(plan.bin_tool_mounts, vec!["/bin/alpha"]);
}

#[tokio::test]
async fn child_clone_exec_spec_declares_agent_and_llm_mounts_for_pid() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut child_core_config = parent.core_config.clone();
    child_core_config.connection_profile = Some("child-main".to_string());
    let spec = launch_spec(root_dir);

    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &child_core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let exec = plan.clone_exec_spec_for_pid("42", "/bin/alan-agent", ["--boot"]);

    assert_eq!(
        serde_json::to_value(&exec).unwrap(),
        json!({
            "executable": "/bin/alan-agent",
            "args": ["--boot"],
            "descriptors": {
                "3": "/agent-definition",
                "4": "/memory"
            },
            "namespace": {
                "mounts": [
                    {"path": "/agent", "access": "rw"},
                    {"path": "/agent-definition", "access": "ro"},
                    {"path": "/bin/alpha", "access": "ro"},
                    {"path": "/bin/beta", "access": "ro"},
                    {"path": "/lib/exec/alpha", "access": "ro"},
                    {"path": "/lib/exec/beta", "access": "ro"},
                    {"path": "/memory", "access": "rw"},
                    {"path": "/mnt/llm", "access": "rw"},
                    {"path": "/mnt/route", "access": "rw"},
                    {"path": "/mnt/source", "access": "rw"},
                    {"path": "/srv", "access": "ro"}
                ]
            }
        })
    );
    let decoded: ExecSpec = serde_json::from_value(serde_json::to_value(&exec).unwrap())
        .expect("child clone document uses the kernel ExecSpec contract");
    assert_eq!(decoded, exec);
}

#[tokio::test]
async fn child_clone_exec_spec_declares_only_allowed_bin_mounts() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });

    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let manifest = plan.namespace_manifest_for_pid("99");

    assert_eq!(
        serde_json::to_value(&manifest).unwrap(),
        json!({
            "mounts": [
                {"path": "/agent", "access": "rw"},
                {"path": "/agent-definition", "access": "ro"},
                {"path": "/bin/alpha", "access": "ro"},
                {"path": "/lib/exec/alpha", "access": "ro"},
                {"path": "/memory", "access": "rw"},
                {"path": "/mnt/llm", "access": "rw"},
                {"path": "/mnt/route", "access": "rw"},
                {"path": "/mnt/source", "access": "rw"},
                {"path": "/srv", "access": "ro"}
            ]
        })
    );
}

#[tokio::test]
async fn child_clone_exec_spec_commits_through_proc_clone_with_planned_namespace() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });
    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let procfs = KernelProcFs::new();
    let spawner = procfs.for_spawner(
        None,
        namespace_from_child_plan(&plan),
        KernelCredentials::user("alan"),
    );

    spawner
        .walk(Fid::ROOT, Fid(90), &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(Fid(90), OpenMode::ReadWrite).await.unwrap();
    let pid = String::from_utf8(spawner.read(Fid(90), 0, 64).await.unwrap()).unwrap();
    let exec = plan.clone_exec_spec_for_pid(&pid, "/bin/alan-agent", ["--boot"]);
    let exec_bytes = serde_json::to_vec(&exec).unwrap();
    spawner.write(Fid(90), 0, &exec_bytes).await.unwrap();
    spawner.clunk(Fid(90)).await.unwrap();

    let namespace =
        read_proc_path(&procfs, vec![pid.clone(), "namespace".to_string()], Fid(91)).await;
    assert!(
        namespace.lines().any(|line| line == "/agent rw"),
        "agent overlay is mounted at /agent: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/mnt/llm rw"),
        "llm connection is mounted: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/bin/alpha ro"),
        "allowed tool executable is mounted read-only: {namespace:?}"
    );
    assert!(
        !namespace.lines().any(|line| line.contains("<child-pid>")),
        "placeholder is expanded before the process becomes public: {namespace:?}"
    );
}

#[tokio::test]
async fn child_namespace_launch_and_supervisor_reattachment_use_proc_pid_files() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });
    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let launch_procfs = KernelProcFs::new();
    let tool_runner =
        crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
    let runtime_procfs = launch_procfs
        .clone()
        .with_runner(Arc::new(tool_runner.clone()));
    let handles = ChildNamespaceLaunchHandles::new(
        Arc::new(alan_agentfs::AgentFs::new()),
        memfs_transport(),
        memfs_transport(),
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/alpha",
        memfs_transport(),
        "/lib/exec/alpha",
        memfs_transport(),
    );

    let launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &plan,
        handles,
        None,
        tool_runner.clone(),
        plan.execution_binding(temp.path().join("scratch")).unwrap(),
        None,
        "/bin/alan-agent",
    )
    .await
    .unwrap();

    assert_eq!(launch.pid, "1");
    assert_eq!(launch.environment.agent_path(), "/agent/1");
    assert_eq!(
        launch.environment.child_launch().connection_name(),
        "default"
    );
    assert_eq!(
        launch.exec,
        plan.clone_exec_spec_for_pid("1", "/bin/alan-agent", std::iter::empty::<String>())
    );

    assert_eq!(
        read_proc_path(
            &launch_procfs,
            vec![launch.pid.clone(), "status".to_string()],
            Fid(90),
        )
        .await,
        "running\n",
        "child agent process should stay running after launch"
    );
    let namespace = read_proc_path(
        &launch_procfs,
        vec![launch.pid.clone(), "namespace".to_string()],
        Fid(92),
    )
    .await;
    assert!(
        namespace.lines().any(|line| line == "/agent rw"),
        "agent overlay is mounted: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/mnt/llm rw"),
        "llm connection is present: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/mnt/route rw"),
        "routefs tree is present: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/srv ro"),
        "service handle registry is present: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/bin/alpha ro"),
        "allowed tool mount is present: {namespace:?}"
    );

    let child_handles = ChildNamespaceLaunchHandles::new(
        Arc::new(alan_agentfs::AgentFs::new()),
        memfs_transport(),
        memfs_transport(),
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/alpha",
        memfs_transport(),
        "/lib/exec/alpha",
        memfs_transport(),
    );
    let nested = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &plan,
        child_handles,
        Some(TestParentProcessContext {
            agent_root: launch.agent_root.clone(),
            pid: alan_kernel::Pid(launch.pid.parse().unwrap()),
        }),
        tool_runner.clone(),
        plan.execution_binding(temp.path().join("scratch")).unwrap(),
        None,
        "/bin/alan-agent",
    )
    .await
    .unwrap();
    assert_eq!(nested.pid, "2");
    assert_eq!(
        read_proc_path(
            &launch_procfs,
            vec![nested.pid.clone(), "parent".to_string()],
            Fid(94),
        )
        .await,
        "1"
    );
    let parent_shell = alan_shell::Shell::new(launch.environment.root_transport());
    assert_eq!(
        parent_shell.ls("/agent/1/children").await.unwrap(),
        vec![nested.pid.clone()],
        "delegated Agent Process must be inspectable from the parent AgentFS view"
    );
    record_child_launch_failure_process(
        &nested.lifecycle,
        &anyhow::anyhow!("simulated child runtime startup failure"),
    )
    .await;
    assert_eq!(
        nested
            .environment
            .process_files()
            .read_process_exit_code(&nested.pid)
            .await
            .unwrap(),
        Some(1)
    );
    assert!(
        parent_shell
            .ls("/agent/1/children")
            .await
            .unwrap()
            .is_empty(),
        "failed child launch must leave no running child entry"
    );

    let tool = launch
        .environment
        .tool_execution()
        .run_action("alpha", "/bin/alpha", ["{}"])
        .await
        .unwrap();
    assert_eq!(tool.pid, "3");
    assert_eq!(tool.action_id, "a0");
    assert_eq!(tool.output.trim(), r#"{"ok":true}"#);
    let tool_namespace = read_proc_path(
        &launch_procfs,
        vec![tool.pid.clone(), "namespace".to_string()],
        Fid(93),
    )
    .await;
    assert!(
        tool_namespace.lines().any(|line| line == "/agent rw"),
        "child-spawned processes inherit the agent overlay: {tool_namespace:?}"
    );
    assert!(
        tool_namespace.lines().any(|line| line == "/mnt/llm rw"),
        "child-spawned processes inherit the llm connection: {tool_namespace:?}"
    );
    assert!(
        tool_namespace.lines().any(|line| line == "/mnt/route rw"),
        "child-spawned processes inherit routefs: {tool_namespace:?}"
    );
    assert!(
        tool_namespace.lines().any(|line| line == "/srv ro"),
        "child-spawned processes inherit /srv handles: {tool_namespace:?}"
    );
    assert!(
        tool_namespace.lines().any(|line| line == "/bin/alpha ro"),
        "child-spawned processes inherit mounted tools: {tool_namespace:?}"
    );

    let process_reader = launch.environment.process_files();
    let process_pid = launch.pid.clone();
    let agent_files = launch.environment.agent_files();
    agent_files
        .write_assistant_output("AgentFS child result")
        .await
        .unwrap();
    crate::runtime::ui_surfaces::turn_completed(&agent_files, false)
        .await
        .unwrap();
    let controller = DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        runtime: None,
        startup_metadata: test_startup_metadata("child-machine", None, false),
        child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
        child_run_registry: ChildRunRegistry::default(),
        timeout: None,
        process_lifecycle: launch.lifecycle,
        agent_files,
        process_files: launch.environment.process_files(),
        process_pid: process_pid.clone(),
    });

    let result = controller.join().await.unwrap();
    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(result.output_text, "AgentFS child result");
    assert_eq!(
        process_reader
            .read_process_exit_code(&process_pid)
            .await
            .unwrap(),
        Some(0),
        "normal completion must not be rewritten as ctl cancellation (130)"
    );
}

#[tokio::test]
async fn external_proc_ctl_stops_child_runtime_controller() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child should be stopped externally.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.tool_profile = Some(alan_agent_protocol::SpawnToolProfileOverride {
        allowed_tools: vec!["alpha".to_string()],
    });
    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let launch_procfs = KernelProcFs::new();
    let tool_runner =
        crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
    let runtime_procfs = launch_procfs
        .clone()
        .with_runner(Arc::new(tool_runner.clone()));
    let handles = ChildNamespaceLaunchHandles::new(
        Arc::new(alan_agentfs::AgentFs::new()),
        memfs_transport(),
        memfs_transport(),
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/alpha",
        memfs_transport(),
        "/lib/exec/alpha",
        memfs_transport(),
    );
    let launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &plan,
        handles,
        None,
        tool_runner,
        plan.execution_binding(temp.path().join("scratch")).unwrap(),
        None,
        "/bin/alan-agent",
    )
    .await
    .unwrap();
    let process_pid = launch.pid.clone();
    let process_files = launch.environment.process_files();
    let agent_files = launch.environment.agent_files();
    let controller = DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        runtime: None,
        startup_metadata: test_startup_metadata("child-machine", None, false),
        child_run_id: format!("test-child-run-{}", uuid::Uuid::new_v4()),
        child_run_registry: ChildRunRegistry::default(),
        timeout: None,
        process_lifecycle: launch.lifecycle,
        agent_files: agent_files.clone(),
        process_files: launch.environment.process_files(),
        process_pid: process_pid.clone(),
    });

    assert_eq!(agent_files.ui_events_offset().await.unwrap(), 0);
    process_files
        .write_process_control_for_pid(&process_pid, "cancel")
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), controller.join())
        .await
        .expect("controller must observe external proc cancellation")
        .unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Terminated);
    assert!(
        result
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("/proc/<pid>/ctl"))
    );
    assert_eq!(
        process_files
            .read_process_exit_code(&process_pid)
            .await
            .unwrap(),
        Some(130)
    );
}

#[tokio::test]
async fn child_namespace_launch_attaches_mount_grant_applicator_factory() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");
    let spec = launch_spec(root_dir);
    let mut plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    plan.launch_context
        .host_mounts
        .retain(|grant| grant.namespace_path != "/agent-definition");
    let package_child_definition = temp.path().join("package-child-definition");
    plan.launch_context.host_mounts.push(
        crate::HostMountGrant::new(
            "/agent-definition",
            package_child_definition.clone(),
            KernelAccess::ReadOnly,
        )
        .unwrap(),
    );
    let launch_procfs = KernelProcFs::new();
    let tool_runner =
        crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config));
    let runtime_procfs = launch_procfs
        .clone()
        .with_runner(Arc::new(tool_runner.clone()));
    let handles = ChildNamespaceLaunchHandles::new(
        Arc::new(alan_agentfs::AgentFs::new()),
        memfs_transport(),
        memfs_transport(),
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/alpha",
        memfs_transport(),
        "/lib/exec/alpha",
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/beta",
        memfs_transport(),
        "/lib/exec/beta",
        memfs_transport(),
    );
    let factory = Arc::new(RecordingMountGrantApplicatorFactory::default());

    let mut launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &plan,
        handles,
        None,
        tool_runner,
        plan.execution_binding(temp.path().join("scratch")).unwrap(),
        Some(factory.clone()),
        "/bin/alan-agent",
    )
    .await
    .unwrap();

    assert_eq!(factory.created_count(), 1);
    assert_eq!(
        factory.applied_grants(),
        [ApprovedMountGrant::new(
            "/agent-definition",
            package_child_definition,
            ApprovedMountGrantAccess::ReadOnly,
            "Agent Definition launch reference",
        )]
    );
    let definition_namespace = read_proc_path(
        &launch_procfs,
        vec![launch.pid.clone(), "namespace".to_string()],
        Fid(95),
    )
    .await;
    assert!(
        definition_namespace
            .lines()
            .any(|line| line == "/agent-definition ro"),
        "child Agent Process must receive its target definition: {definition_namespace:?}"
    );
    let applied = launch
        .environment
        .apply_approved_mount_grant(&ApprovedMountGrant::new(
            "/mnt/project",
            PathBuf::from("/unused/by/test/applicator"),
            ApprovedMountGrantAccess::ReadWrite,
            "Need project files",
        ));
    assert!(applied.namespace_applied);
    assert_eq!(applied.namespace_error, None);

    let namespace = read_proc_path(
        &launch_procfs,
        vec![launch.pid.clone(), "namespace".to_string()],
        Fid(94),
    )
    .await;
    assert!(
        namespace.lines().any(|line| line == "/mnt/project rw"),
        "child Agent Process namespace should reflect applicator live mounts: {namespace:?}"
    );
}

#[tokio::test]
async fn child_namespace_launch_handles_share_parent_routefs() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let routefs = Arc::new(alan_routefs::RouteFs::new());
    routefs
        .install_rule(
            "10-results",
            alan_routefs::RuleSpec::for_type("result", "review"),
        )
        .await
        .unwrap();
    let mut parent = make_parent_state(&temp, requests, response);
    let launch_context = inherited_launch_context(&parent);
    parent.environment = namespace_environment_for_parent_test_with_route(routefs.clone())
        .with_launch_context(launch_context);

    let root_dir = temp.path().join("definition");
    let spec = launch_spec(root_dir);
    let plan = build_child_namespace_assembly_plan(
        &parent,
        &spec,
        &parent.core_config,
        inherited_launch_context(&parent),
    )
    .await
    .unwrap();
    let launch_procfs = KernelProcFs::new();
    let runtime_procfs = launch_procfs.clone().with_runner(Arc::new(
        crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config)),
    ));
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection(
        &plan.llm_connection_name().unwrap(),
        Box::new(ChildLlmProvider::new(LlmClient::new(
            RecordingProvider::new(RecordedRequests::default(), completed_response("unused")),
        ))),
    );
    let handles = ChildNamespaceLaunchHandles::new(
        Arc::new(alan_agentfs::AgentFs::new()),
        InProcessTransport::new(llmfs),
        memfs_transport(),
        InProcessTransport::new(routefs.clone()),
    )
    .with_tool_package(
        "/bin/alpha",
        memfs_transport(),
        "/lib/exec/alpha",
        memfs_transport(),
    )
    .with_tool_package(
        "/bin/beta",
        memfs_transport(),
        "/lib/exec/beta",
        memfs_transport(),
    );

    let launch = spawn_child_namespace_runtime_environment(
        &launch_procfs,
        &runtime_procfs,
        &plan,
        handles,
        None,
        crate::tools::ToolProcessRunner::from_registry(&parent_test_tools(&parent.core_config)),
        plan.execution_binding(temp.path().join("scratch")).unwrap(),
        None,
        "/bin/alan-agent",
    )
    .await
    .unwrap();

    let child_shell = alan_shell::Shell::new(launch.environment.root_transport());
    let message = serde_json::to_vec(&json!({
        "version": 1,
        "type": "result",
        "content": "child result"
    }))
    .unwrap();
    child_shell
        .write("/mnt/route/send", &message)
        .await
        .unwrap();

    let parent_route_shell = alan_shell::Shell::new(InProcessTransport::new(routefs));
    let routed = String::from_utf8(parent_route_shell.cat("/ports/review").await.unwrap()).unwrap();
    assert!(routed.contains(r#""type":"result""#), "{routed}");
    assert!(routed.contains(r#""content":"child result""#), "{routed}");
}

#[tokio::test]
async fn child_tool_runner_rejects_unmounted_tool_executables() {
    let mut child_tools = ToolRegistry::new();
    child_tools.register(MarkerTool::new("alpha", "mounted-only"));
    let runner = crate::tools::ToolProcessRunner::from_registry(&child_tools);
    let invocation = alan_kernel::ProcessInvocation {
        pid: alan_kernel::Pid(1),
        parent: Some(alan_kernel::Pid(0)),
        credentials: alan_kernel::Credentials::user("child-agent"),
        namespace: alan_kernel::Namespace::new(),
        exec: alan_kernel::ExecSpec {
            executable: "/bin/alpha".to_string(),
            args: vec!["{}".to_string()],
            namespace: None,
            descriptors: Default::default(),
        },
    };

    let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;

    assert_eq!(outcome.exit_code, 127);
    assert_eq!(outcome.output, b"executable is not mounted\n");
}
