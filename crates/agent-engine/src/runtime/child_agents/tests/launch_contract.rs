use super::*;
use alan_kernel::ExecNamespaceAccess;

#[tokio::test]
async fn spawn_child_runtime_conversation_snapshot_excludes_tool_outputs_without_handle() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Snapshot captured.");
    let parent = make_parent_state(&temp, requests.clone(), response.clone());
    let root_dir = temp.path().join("definition");
    let mut spec = launch_spec(root_dir);
    spec.handles = vec![SpawnHandle::ConversationSnapshot];

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
    assert!(!user_text.contains("tool output"));
}

#[tokio::test]
async fn spawn_child_runtime_uses_effective_launch_root_config_for_llm_setup() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
    let root_dir = temp.path().join("definition");
    install_parent_definition(&mut parent, b"tool_repeat_limit = 9\n".to_vec());
    let seen_config = Arc::new(Mutex::new(None::<crate::Config>));
    let seen_config_for_factory = seen_config.clone();

    let child = spawn_child_runtime_with_client_factory(&parent, launch_spec(root_dir), |config| {
        *seen_config_for_factory.lock().unwrap() = Some(config.clone());
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    let seen_config = seen_config.lock().unwrap().clone().unwrap();
    assert_eq!(seen_config.effective_model(), "gpt-5.4");
    assert_eq!(seen_config.tool_repeat_limit, 9);
}

#[tokio::test]
async fn spawn_child_runtime_preserves_explicit_connection_profile() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child used the explicit profile.");
    let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
    let profile_id = "explicit-main";
    parent.core_config.connection_profile = Some(profile_id.to_string());
    let launch_context = parent.child_launch().launch_context().unwrap().clone();
    parent.environment = namespace_environment_for_parent_test_with_connection(
        Arc::new(alan_routefs::RouteFs::new()),
        Arc::new(alan_llmfs::LlmFs::new()),
        profile_id,
    )
    .with_launch_context(launch_context);
    let root_dir = temp.path().join("definition");
    let seen_config = Arc::new(Mutex::new(None::<crate::Config>));
    let seen_config_for_factory = seen_config.clone();

    let child = spawn_child_runtime_with_client_factory(&parent, launch_spec(root_dir), |config| {
        *seen_config_for_factory.lock().unwrap() = Some(config.clone());
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    assert_eq!(
        seen_config
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|config| config.connection_profile.as_deref()),
        Some(profile_id)
    );
}

#[tokio::test]
async fn spawn_child_runtime_rejects_unpassed_definition_connection_reference() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child used its definition profile.");
    let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
    let profile_id = "child-main";
    let root_dir = temp.path().join("definition");
    install_parent_definition(
        &mut parent,
        format!("connection_profile = \"{profile_id}\"\n").into_bytes(),
    );
    let error =
        match spawn_child_runtime_with_client_factory(&parent, launch_spec(root_dir), |_| {
            unreachable!("an unpassed Connection must fail before provider setup")
        })
        .await
        {
            Ok(_) => panic!("unpassed child Connection should be rejected"),
            Err(error) => error,
        };

    assert!(
        error
            .to_string()
            .contains("was not passed to the child Agent Process by the parent Process")
    );
}

#[tokio::test]
async fn spawn_child_runtime_applies_reasoning_effort_override_after_overlay() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let mut parent = make_parent_state(&temp, requests.clone(), response.clone());
    let root_dir = temp.path().join("definition");
    install_parent_definition(&mut parent, b"model_reasoning_effort = \"high\"\n".to_vec());
    let seen_config = Arc::new(Mutex::new(None::<crate::Config>));
    let seen_config_for_factory = seen_config.clone();
    let mut spec = launch_spec(root_dir);
    spec.runtime_overrides.model_reasoning_effort = Some(alan_agent_protocol::ReasoningEffort::Low);

    let child = spawn_child_runtime_with_client_factory(&parent, spec, |config| {
        *seen_config_for_factory.lock().unwrap() = Some(config.clone());
        Ok(LlmClient::new(RecordingProvider::new(
            requests.clone(),
            response.clone(),
        )))
    })
    .await
    .unwrap();
    let result = child.join().await.unwrap();

    assert_eq!(result.status, ChildRuntimeStatus::Completed);
    let seen_config = seen_config.lock().unwrap().clone().unwrap();
    assert_eq!(
        crate::resolve_runtime_request_controls(
            &seen_config,
            crate::provider_capabilities_for_config(&seen_config),
            crate::RequestControlIntent::default(),
        )
        .unwrap()
        .reasoning_effort(),
        Some(alan_agent_protocol::ReasoningEffort::Low)
    );

    let recorded = requests.0.lock().unwrap();
    assert_eq!(
        recorded[0].reasoning.effort,
        Some(alan_agent_protocol::ReasoningEffort::Low)
    );
}

#[test]
fn child_agent_config_requires_memory_handle_for_memory_dir() {
    let temp = TempDir::new().unwrap();
    let requests = RecordedRequests::default();
    let response = completed_response("Child finished cleanly.");
    let parent = make_parent_state(&temp, requests, response);
    let root_dir = temp.path().join("definition");

    let mut approval_spec = launch_spec(root_dir.clone());
    approval_spec.handles = vec![SpawnHandle::ApprovalScope];
    let approval_config = build_child_agent_config(&parent, &approval_spec);
    assert_eq!(approval_config.core_config.memory.store_dir, None);

    let mut override_spec = launch_spec(root_dir);
    override_spec.runtime_overrides.policy_path = Some("policy.yaml".to_string());
    let override_config = build_child_agent_config(&parent, &override_spec);
    assert_eq!(override_config.core_config.memory.store_dir, None);
}

#[test]
fn child_launch_contract_rejects_relative_namespace_cwd() {
    let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
    spec.launch.cwd = Some(PathBuf::from("docs"));

    let err = validate_child_launch_contract(&spec).unwrap_err();
    assert!(
        format!("{err:#}").contains("absolute"),
        "expected absolute-path validation error, got {err:#}"
    );
}

#[test]
fn child_launch_contract_rejects_non_normal_namespace_cwd() {
    for cwd in ["/mnt/source/../other", "/mnt/./source"] {
        let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
        spec.launch.cwd = Some(PathBuf::from(cwd));

        let err = validate_child_launch_contract(&spec).unwrap_err();
        assert!(
            err.to_string()
                .contains("Invalid child Agent Process launch cwd"),
            "expected normal-path validation error for {cwd}, got {err:#}"
        );
    }
}

#[test]
fn child_launch_context_defaults_to_no_descriptors_or_inherited_cwd() {
    let temp = TempDir::new().unwrap();
    let parent = make_parent_state(
        &temp,
        RecordedRequests::default(),
        completed_response("done"),
    );
    let parent_context = parent.child_launch().launch_context().cloned().unwrap();
    let definition = temp.path().join("child-definition");
    let mut spec = launch_spec(definition.clone());
    spec.handles.clear();

    let definition = host_launch_root(definition);
    let child =
        build_child_launch_context(&parent_context, &spec, None, Some(&definition)).unwrap();

    assert_eq!(child.cwd, "/");
    assert_eq!(
        child
            .descriptors
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![crate::AGENT_DEFINITION_DESCRIPTOR]
    );
}

#[test]
fn child_launch_context_keeps_package_projection_with_inherited_reference() {
    let package = TempDir::new().unwrap();
    let skill_root = package.path().join("skills/inherited");
    std::fs::create_dir_all(&skill_root).unwrap();
    std::fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: inherited\ndescription: Inherited package Skill.\n---\n",
    )
    .unwrap();

    let mut namespace = KernelNamespace::new();
    namespace.mount(
        "/lib/pkg/parent-pack",
        memfs_transport(),
        KernelAccess::ReadOnly,
    );
    let package_reference = crate::ProcessPackageReference::new(
        "parent-pack",
        "a".repeat(64),
        crate::ProcessPackageKind::Installed,
        "/lib/pkg/parent-pack",
        vec![
            crate::ProcessPackageSkillReference::new(
                "inherited",
                "skills/inherited",
                Vec::new(),
                package_skill_descriptor("inherited"),
            )
            .unwrap(),
        ],
        memfs_transport(),
    )
    .unwrap();
    let parent = crate::ProcessLaunchContext::new(
        namespace,
        KernelCredentials::user("parent-agent"),
        "/lib/pkg/parent-pack",
    )
    .unwrap()
    .with_package_reference(package_reference);
    let mut spec = launch_spec(package.path().join("unused-definition"));
    spec.handles.clear();

    let child = build_child_launch_context(&parent, &spec, None, None).unwrap();

    assert_eq!(child.cwd, "/");
    assert_eq!(child.package_references.len(), 1);
    assert_eq!(child.package_references[0].package_id, "parent-pack");
    assert!(child.namespace.resolve("/lib/pkg/parent-pack").is_ok());
    let resolved = crate::ResolvedAgentDefinition::from_launch_context(
        &child,
        &[],
        crate::ConfigSourceKind::Default,
    )
    .unwrap();
    assert_eq!(resolved.capability_view.packages.len(), 1);
    assert_eq!(
        resolved.capability_view.packages[0].id,
        "installed:parent-pack:inherited"
    );
}

#[tokio::test]
async fn package_child_definition_is_passed_by_descriptor_and_package_mount() {
    let mut namespace = KernelNamespace::new();
    namespace.mount("/lib/pkg/review", memfs_transport(), KernelAccess::ReadOnly);
    namespace.mount(
        "/lib/agents/root",
        memfs_transport(),
        KernelAccess::ReadOnly,
    );
    let parent =
        crate::ProcessLaunchContext::new(namespace, KernelCredentials::user("parent-agent"), "/")
            .unwrap()
            .with_descriptor(
                crate::AGENT_DEFINITION_DESCRIPTOR,
                crate::ProcessDescriptor::with_file_tree(
                    "/lib/agents/root",
                    crate::ProcessFileTree::default(),
                )
                .unwrap(),
            )
            .with_package_reference(
                crate::ProcessPackageReference::new(
                    "review",
                    "a".repeat(64),
                    crate::ProcessPackageKind::Installed,
                    "/lib/pkg/review",
                    Vec::new(),
                    memfs_transport(),
                )
                .unwrap(),
            );
    let definition = ResolvedLaunchRoot {
        root_dir: PathBuf::from("/lib/pkg/review/skills/review/agents/critic"),
        file_tree: Some(
            crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
                "agent.toml".to_string(),
                b"tool_repeat_limit = 3\n".to_vec(),
            )]))
            .unwrap(),
        ),
    };
    let mut spec = launch_spec(definition.root_dir.clone());
    spec.handles.clear();

    let child = build_child_launch_context(&parent, &spec, None, Some(&definition)).unwrap();

    let descriptor = child
        .descriptor(crate::AGENT_DEFINITION_DESCRIPTOR)
        .unwrap();
    assert_eq!(descriptor.path, definition.root_dir.to_string_lossy());
    assert!(
        descriptor
            .file_tree
            .as_ref()
            .is_some_and(|tree| tree.contains_file("agent.toml"))
    );

    let mut plan = capability_plan(None, &[]);
    plan.launch_context = child;
    let manifest = plan.namespace_manifest_for_pid("99");
    assert!(manifest.mounts.iter().any(|mount| {
        mount.path == "/lib/pkg/review" && mount.access == ExecNamespaceAccess::ReadOnly
    }));
    assert!(
        !manifest
            .mounts
            .iter()
            .any(|mount| { mount.path == "/lib/pkg/review/skills/review/agents/critic" })
    );

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
    spawner
        .write(Fid(90), 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    spawner.clunk(Fid(90)).await.unwrap();
}

#[test]
fn child_launch_contract_normalizes_repeated_namespace_separators() {
    let mut spec = launch_spec(PathBuf::from("/tmp/definition"));
    spec.launch.cwd = Some(PathBuf::from("/mnt//source///docs"));

    assert_eq!(
        validate_child_launch_contract(&spec).unwrap().as_deref(),
        Some("/mnt/source/docs")
    );
}
