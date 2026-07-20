use super::*;
use alan_agent_engine::LlmProvider as ConnectionProvider;
use alan_kernel::Status;
use alan_llm::{LlmProvider, MockLlmProvider};

use crate::{BootUnit, ConnectionCredential, ConnectionProfile, CredentialKind};

#[derive(Debug)]
struct MissingSecretFactory;

#[derive(Debug, Default)]
struct SlowMockLlmProvider(MockLlmProvider);

#[async_trait::async_trait]
impl LlmProvider for SlowMockLlmProvider {
    async fn generate(
        &mut self,
        request: alan_llm::GenerationRequest,
    ) -> Result<alan_llm::GenerationResponse> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.0.generate(request).await
    }

    async fn chat(&mut self, system: Option<&str>, user: &str) -> Result<String> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.0.chat(system, user).await
    }

    async fn generate_stream(
        &mut self,
        request: alan_llm::GenerationRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<alan_llm::StreamChunk>> {
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.0.generate_stream(request).await
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

impl LlmClientFactory for MissingSecretFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        _selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        anyhow::bail!("selected profile is missing a secret")
    }
}

#[derive(Debug, Default)]
struct RecordingFactory {
    selected_profiles: std::sync::Mutex<Vec<Option<String>>>,
}

impl LlmClientFactory for RecordingFactory {
    fn create(
        &self,
        _base_config: &alan_agent_engine::Config,
        selected_profile: Option<&str>,
        _connections: &ConnectionsFile,
    ) -> Result<LlmClient> {
        self.selected_profiles
            .lock()
            .map_err(|_| anyhow::anyhow!("recording factory lock poisoned"))?
            .push(selected_profile.map(str::to_string));
        Ok(LlmClient::new(MockLlmProvider::new()))
    }
}

#[tokio::test]
async fn boot_rejects_ambient_package_namespace_mounts() {
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.launch_context.namespace.mount(
        "/lib/pkg/ambient",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );

    let error = ServiceManager::boot(config).await.err().unwrap();

    assert!(
        error
            .to_string()
            .contains("namespace mounts overlapping /lib/pkg are not accepted")
    );
}

#[tokio::test]
async fn boot_rejects_root_namespace_mount_covering_package_namespace() {
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.launch_context.namespace.mount(
        "/",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );

    let error = ServiceManager::boot(config).await.err().unwrap();

    assert!(
        error
            .to_string()
            .contains("namespace mounts overlapping /lib/pkg are not accepted")
    );
}

#[tokio::test]
async fn installed_distribution_is_visible_only_after_explicit_process_reference() {
    let service = PackageService::ephemeral("test").unwrap();
    let installed = service
        .execute(crate::PackageCommand::Install {
            request_id: "dogfood-install".to_string(),
            package_id: "dogfood-pack".to_string(),
            snapshot: crate::PackageSnapshot {
                source_name: "dogfood-pack".to_string(),
                entries: vec![
                    crate::PackageSnapshotEntry {
                        path: "research/SKILL.md".to_string(),
                        bytes: b"---\nname: Research\ndescription: Research Skill.\n---\n".to_vec(),
                        executable: false,
                    },
                    crate::PackageSnapshotEntry {
                        path: "shared/data.txt".to_string(),
                        bytes: b"shared".to_vec(),
                        executable: false,
                    },
                    crate::PackageSnapshotEntry {
                        path: "skills/web.md".to_string(),
                        bytes: b"Use WebSearch for this work.".to_vec(),
                        executable: false,
                    },
                ],
            },
        })
        .unwrap();
    assert!(installed.success, "{}", installed.message);
    service
        .execute(crate::PackageCommand::Install {
            request_id: "hidden-install".to_string(),
            package_id: "hidden-pack".to_string(),
            snapshot: crate::PackageSnapshot {
                source_name: "hidden-pack".to_string(),
                entries: vec![crate::PackageSnapshotEntry {
                    path: "hidden/SKILL.md".to_string(),
                    bytes: b"---\nname: Hidden\ndescription: Hidden Skill.\n---\n".to_vec(),
                    executable: false,
                }],
            },
        })
        .unwrap();

    let mut launch_context = ProcessLaunchContext::root();
    assert!(
        launch_context
            .namespace
            .resolve("/lib/pkg/dogfood-pack")
            .is_err()
    );
    project_package_reference(&service, &mut launch_context, "dogfood-pack").unwrap();
    assert!(
        launch_context
            .namespace
            .resolve("/lib/pkg/dogfood-pack/skills/research/SKILL.md")
            .is_ok()
    );
    assert!(
        launch_context
            .namespace
            .resolve("/lib/pkg/hidden-pack")
            .is_err()
    );
    let package_shell = alan_shell::Shell::new(InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(launch_context.namespace.clone()),
    )));
    assert_eq!(
        package_shell
            .cat("/lib/pkg/dogfood-pack/source/shared/data.txt")
            .await
            .unwrap(),
        b"shared"
    );
    assert_eq!(
        package_shell
            .write("/lib/pkg/dogfood-pack/skills/research/SKILL.md", b"mutate",)
            .await,
        Err(alan_ap::ErrorCode::NoAccess)
    );

    let definition = alan_agent_engine::ResolvedAgentDefinition::from_process_inputs(
        launch_context.descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR),
        &launch_context.package_references,
        &[],
        alan_agent_engine::ConfigSourceKind::Default,
    )
    .unwrap();
    let registry = alan_agent_engine::skills::SkillsRegistry::load_capability_view(
        &definition.capability_view,
        &[],
    )
    .unwrap();
    assert!(registry.has(&"research".to_string()));
    assert!(registry.has(&"web".to_string()));
    assert!(!registry.has(&"hidden".to_string()));
    let web = registry.get(&"web".to_string()).unwrap();
    assert!(
        web.compatibility
            .dependencies
            .iter()
            .any(|dependency| { dependency.identity_key() == "runtime_capability:web-search" })
    );
    let issues = alan_agent_engine::skills::skill_availability_issues(
        web,
        &alan_agent_engine::skills::SkillHostCapabilities::default(),
    );
    assert!(!issues.is_empty());

    let child = launch_context.child();
    assert_eq!(child.package_references.len(), 1);
    assert_eq!(child.package_references[0].package_id, "dogfood-pack");
    assert!(
        child
            .namespace
            .resolve("/lib/pkg/dogfood-pack/skills/web/SKILL.md")
            .is_ok()
    );
}

#[tokio::test]
async fn unavailable_default_connection_does_not_prevent_system_boot() {
    let temp = tempfile::tempdir().unwrap();
    let metadata = temp.path().join("connections.toml");
    let credential_id = "missing-secret".to_string();
    let profile_id = "default-profile".to_string();
    let now = chrono::Utc::now();
    let connections = ConnectionsFile {
        version: 1,
        default_profile: Some(profile_id.clone()),
        credentials: [(
            credential_id.clone(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: ConnectionProvider::OpenAiResponses,
                label: "Missing secret".to_string(),
                backend: "host_credential_store".to_string(),
            },
        )]
        .into_iter()
        .collect(),
        profiles: [(
            profile_id.clone(),
            ConnectionProfile {
                provider: ConnectionProvider::OpenAiResponses,
                label: None,
                credential_id: Some(credential_id),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: BTreeMap::new(),
            },
        )]
        .into_iter()
        .collect(),
    };
    connections.save_to_path(&metadata).unwrap();

    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.connection_store = Some(ConnectionStoreBindings::new(metadata).unwrap());
    config.llm_factory = Arc::new(MissingSecretFactory);

    let manager = ServiceManager::boot(config).await.unwrap();
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    let status = String::from_utf8(shell.cat("/mnt/connections/status").await.unwrap()).unwrap();
    assert!(status.contains("ready=0") && status.contains("unavailable=1"));
    assert_eq!(
        shell.cat("/mnt/connections/validation").await.unwrap(),
        br#"{"default-profile":"unavailable"}"#
    );
    assert_eq!(shell.cat(BOOT_STATE_PATH).await.unwrap(), b"ready\n");
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn file_tree_agent_definition_selects_connection_before_boot() {
    let temp = tempfile::tempdir().unwrap();
    let metadata = temp.path().join("connections.toml");
    let profile_id = "definition-profile".to_string();
    let credential_id = "definition-secret".to_string();
    let now = chrono::Utc::now();
    ConnectionsFile {
        version: 1,
        default_profile: None,
        credentials: [(
            credential_id.clone(),
            ConnectionCredential {
                kind: CredentialKind::SecretString,
                provider_family: ConnectionProvider::OpenAiResponses,
                label: "Definition secret".to_string(),
                backend: "host_credential_store".to_string(),
            },
        )]
        .into_iter()
        .collect(),
        profiles: [(
            profile_id.clone(),
            ConnectionProfile {
                provider: ConnectionProvider::OpenAiResponses,
                label: None,
                credential_id: Some(credential_id),
                created_at: now,
                updated_at: now,
                source: "managed".to_string(),
                settings: BTreeMap::new(),
            },
        )]
        .into_iter()
        .collect(),
    }
    .save_to_path(&metadata)
    .unwrap();
    let definition = alan_agent_engine::ProcessFileTree::new(BTreeMap::from([(
        "agent.toml".to_string(),
        format!("connection_profile = \"{profile_id}\"\n").into_bytes(),
    )]))
    .unwrap();
    let launch_context = ProcessLaunchContext::root().with_descriptor(
        alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
        alan_agent_engine::ProcessDescriptor::with_file_tree("/agent-definition", definition)
            .unwrap(),
    );
    let mut config = ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        launch_context,
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    );
    config.connection_store = Some(ConnectionStoreBindings::new(metadata).unwrap());
    let factory = Arc::new(RecordingFactory::default());
    config.llm_factory = factory.clone();

    let manager = ServiceManager::boot(config).await.unwrap();

    assert_eq!(
        factory.selected_profiles.lock().unwrap().as_slice(),
        &[Some(profile_id)]
    );
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn root_agent_is_replaced_without_pid_continuity() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    assert_eq!(manager.manager_pid(), Pid(1));
    let old_pid = manager.root_pid();

    manager.terminate_unit("root-agent", 0).await.unwrap();
    let new_pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let pid = manager.root_pid();
            if pid != Pid(0) && pid != old_pid {
                break pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Root Agent was not replaced");

    assert_eq!(
        manager.procfs.try_observe_process_lifecycle(old_pid),
        Some((Status::Exited, Some(0)))
    );
    assert_eq!(
        manager.procfs.try_observe_process_lifecycle(new_pid),
        Some((Status::Running, None))
    );
    let unit = manager.state().lock().await.unit("root-agent").unwrap();
    assert_eq!(unit.pid, Some(new_pid.0));
    assert_eq!(unit.attempts, 2);
    assert_eq!(unit.status, crate::UnitStatus::Ready);

    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    shell.ls("/agent/root").await.unwrap();
    assert!(shell.ls("/lib/pkg/alan-memory").await.is_err());
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/proc/{}/parent", new_pid.0))
                .await
                .unwrap()
        )
        .unwrap()
        .trim(),
        "1"
    );
    let services = shell.ls("/srv").await.unwrap();
    for required in [
        "service-manager",
        "agent-runtime",
        "connection",
        "package",
        "host-mount",
        "local-entry",
        "llm",
        "route",
    ] {
        assert!(services.iter().any(|service| service == required));
    }
    assert_eq!(
        shell.cat("/mnt/service-manager/status").await.unwrap(),
        b"ready\n"
    );
    let route_pid = manager
        .state()
        .lock()
        .await
        .unit("route")
        .unwrap()
        .pid
        .unwrap();
    let route_namespace = String::from_utf8(
        shell
            .cat(&format!("/proc/{route_pid}/namespace"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(route_namespace.lines().any(|line| line == "/proc rw"));
    assert!(route_namespace.lines().any(|line| line == "/srv rw"));
    assert!(
        !route_namespace
            .lines()
            .any(|line| line.starts_with("/agent "))
    );
    assert!(
        !route_namespace
            .lines()
            .any(|line| line.starts_with("/memory "))
    );
    assert_eq!(
        serde_json::from_slice::<BTreeMap<u32, String>>(
            &shell
                .cat(&format!("/proc/{}/descriptors", new_pid.0))
                .await
                .unwrap()
        )
        .unwrap(),
        [
            (3, "/lib/agents/root".to_string()),
            (4, "/memory".to_string()),
        ]
        .into_iter()
        .collect()
    );
    assert!(
        String::from_utf8(shell.cat("/mnt/connections/status").await.unwrap())
            .unwrap()
            .contains("channel=test")
    );
    assert_eq!(
        shell.ls("/mnt/host-mount").await.unwrap(),
        vec![
            "requests".to_string(),
            "grants".to_string(),
            "events".to_string(),
        ]
    );
    assert_eq!(
        shell.ls("/mnt/host-mount/requests").await.unwrap(),
        vec!["clone".to_string(), "events".to_string()]
    );
    for retired in ["request", "projection", "approval", "status"] {
        assert!(
            shell
                .stat(&format!("/mnt/host-mount/{retired}"))
                .await
                .is_err(),
            "retired flat Host Mount surface {retired} must be absent"
        );
    }
    assert!(
        shell
            .ls("/mnt/llm/connections")
            .await
            .unwrap()
            .iter()
            .any(|connection| connection == "default")
    );
    let packages = shell
        .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
        .await
        .unwrap();
    assert_eq!(packages.exit_code, 0);
    let packages = String::from_utf8(packages.output).unwrap();
    assert!(packages.contains("alan-memory"), "{packages}");
    assert!(packages.contains("alan-skill-creator"), "{packages}");
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn child_agent_executes_through_proc_clone_and_cleans_up_agentfs() {
    let definition = alan_agent_engine::ProcessFileTree::new(BTreeMap::new()).unwrap();
    let launch_context = ProcessLaunchContext::root()
        .with_descriptor(
            alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
            alan_agent_engine::ProcessDescriptor::with_file_tree("/lib/agents/root", definition)
                .unwrap(),
        )
        .with_descriptor(
            alan_agent_engine::MEMORY_STORE_DESCRIPTOR,
            alan_agent_engine::ProcessDescriptor::new("/memory").unwrap(),
        );
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        launch_context,
        LlmClient::new(SlowMockLlmProvider::default()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let root_pid = manager.root_pid();
    let shell = alan_shell::Shell::new(manager.root_namespace.clone());
    let namespace = String::from_utf8(
        shell
            .cat(&format!("/proc/{}/namespace", root_pid.0))
            .await
            .unwrap(),
    )
    .unwrap();
    let mounts = namespace
        .lines()
        .map(|line| {
            let (path, access) = line.rsplit_once(' ').unwrap();
            let access = match access {
                "ro" => alan_agent_protocol::ProcessNamespaceAccess::ReadOnly,
                "rw" => alan_agent_protocol::ProcessNamespaceAccess::ReadWrite,
                other => panic!("unexpected namespace access {other}"),
            };
            alan_agent_protocol::ProcessNamespaceMount::new(path, access)
        })
        .filter(|mount| mount.path != "/memory")
        .collect();
    let request = alan_agent_protocol::AgentExecutableRequest {
        spawn: alan_agent_protocol::SpawnSpec {
            target: alan_agent_protocol::SpawnTarget::DefinitionDescriptor {
                descriptor: alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR.to_string(),
            },
            launch: alan_agent_protocol::SpawnLaunchInputs {
                task: "respond once".to_string(),
                ..alan_agent_protocol::SpawnLaunchInputs::default()
            },
            handles: Vec::new(),
            host_mounts: Vec::new(),
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        },
        initial_task: "respond once".to_string(),
    };
    let exec = alan_agent_protocol::ProcessExecSpec {
        executable: "/bin/alan-agent".to_string(),
        args: vec![serde_json::to_string(&request).unwrap()],
        namespace: alan_agent_protocol::ProcessNamespaceManifest { mounts },
        descriptors: BTreeMap::from([(
            alan_agent_protocol::AGENT_DEFINITION_DESCRIPTOR,
            "/lib/agents/root".to_string(),
        )]),
    };
    let child_pid = shell
        .spawn(&serde_json::to_string(&exec).unwrap())
        .await
        .unwrap()
        .trim()
        .to_string();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if shell
                .ls("/agent/root/children")
                .await
                .unwrap()
                .contains(&child_pid)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("child AgentFS projection did not become visible");
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/proc/{child_pid}/parent"))
                .await
                .unwrap()
        )
        .unwrap(),
        root_pid.0.to_string()
    );
    assert_eq!(
        serde_json::from_slice::<BTreeMap<u32, String>>(
            &shell
                .cat(&format!("/proc/{child_pid}/descriptors"))
                .await
                .unwrap()
        )
        .unwrap(),
        BTreeMap::from([(
            alan_agent_protocol::AGENT_DEFINITION_DESCRIPTOR,
            "/lib/agents/root".to_string(),
        )])
    );

    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if shell
                .cat(&format!("/proc/{child_pid}/status"))
                .await
                .unwrap()
                == b"exited\n"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child Agent Process did not exit");
    let exit =
        String::from_utf8(shell.cat(&format!("/proc/{child_pid}/exit")).await.unwrap()).unwrap();
    let output = String::from_utf8(
        shell
            .cat(&format!("/proc/{child_pid}/io/output"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(exit, "0", "child Process output: {output}");
    let result = alan_agent_protocol::AgentExecutableResult::from_process_output(output.as_bytes())
        .unwrap_or_else(|error| panic!("invalid Agent Executable result `{output}`: {error}"));
    assert_eq!(
        result.status,
        alan_agent_protocol::AgentExecutableStatus::Completed
    );
    assert!(!result.output_text.is_empty());
    assert!(
        !shell
            .ls("/agent/root/children")
            .await
            .unwrap()
            .contains(&child_pid)
    );
    assert!(shell.stat(&format!("/agent/{child_pid}")).await.is_err());

    let cancelled_pid = shell
        .spawn(&serde_json::to_string(&exec).unwrap())
        .await
        .unwrap()
        .trim()
        .to_string();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if shell
                .ls("/agent/root/children")
                .await
                .unwrap()
                .contains(&cancelled_pid)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cancelled child AgentFS projection did not become visible");
    shell
        .write(&format!("/proc/{cancelled_pid}/ctl"), b"cancel")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if shell
                .stat(&format!("/agent/{cancelled_pid}"))
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("cancelled child AgentFS binding was not cleaned up");
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/proc/{cancelled_pid}/exit"))
                .await
                .unwrap()
        )
        .unwrap(),
        "130"
    );

    let mut forged_request = request.clone();
    forged_request
        .spawn
        .handles
        .push(alan_agent_protocol::SpawnHandle::Memory);
    let mut forged_exec = exec.clone();
    forged_exec.args = vec![serde_json::to_string(&forged_request).unwrap()];
    forged_exec.descriptors.insert(
        alan_agent_protocol::MEMORY_STORE_DESCRIPTOR,
        "/man".to_string(),
    );
    let forged_pid = shell
        .spawn(&serde_json::to_string(&forged_exec).unwrap())
        .await
        .unwrap()
        .trim()
        .to_string();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if shell
                .cat(&format!("/proc/{forged_pid}/status"))
                .await
                .unwrap()
                == b"exited\n"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("forged child Agent Process did not exit");
    let forged_output = String::from_utf8(
        shell
            .cat(&format!("/proc/{forged_pid}/io/output"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        forged_output.contains("Memory Store descriptor does not match"),
        "unexpected forged child output: {forged_output}"
    );
    assert_eq!(
        alan_agent_protocol::AgentExecutableResult::from_process_output(forged_output.as_bytes())
            .unwrap()
            .status,
        alan_agent_protocol::AgentExecutableStatus::Failed
    );
    assert!(shell.stat(&format!("/agent/{forged_pid}")).await.is_err());

    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn readiness_times_out_until_all_declared_handles_are_published() {
    let procfs = alan_kernel::ProcFs::new();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/test-service",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
        Access::ReadOnly,
    );
    let pid = spawn_process(
        &procfs,
        None,
        LiveNamespace::new(namespace),
        Credentials::system(),
        "/bin/test-service",
    )
    .await
    .unwrap();
    let unit = BootUnit::parse(
        r#"name = "test-service"
executable = "/bin/test-service"
required = true
timeout_ms = 20
restart = "never"
restart_limit = 0
initial_backoff_ms = 1
max_backoff_ms = 1
stable_reset_ms = 1
published_handles = ["test-service"]
"#,
    )
    .unwrap();
    let srvfs = Arc::new(alan_kernel::SrvFs::new());

    assert!(wait_unit_ready(&unit, pid, &procfs, &srvfs).await.is_err());
    srvfs
        .post(
            "test-service",
            InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            Access::ReadOnly,
        )
        .await;
    wait_unit_ready(&unit, pid, &procfs, &srvfs).await.unwrap();
}

#[tokio::test]
async fn exited_file_service_is_restarted_and_republishes_handles() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let old_pid = Pid(manager
        .state()
        .lock()
        .await
        .unit("connection")
        .unwrap()
        .pid
        .unwrap());
    manager.terminate_unit("connection", 1).await.unwrap();
    let new_pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let unit = manager.state().lock().await.unit("connection").unwrap();
            if unit.status == crate::UnitStatus::Ready && unit.pid != Some(old_pid.0) {
                break Pid(unit.pid.unwrap());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Connection Service was not restarted");
    assert_eq!(
        manager.procfs.try_observe_process_lifecycle(old_pid),
        Some((Status::Exited, Some(1)))
    );
    assert_eq!(
        manager.procfs.try_observe_process_lifecycle(new_pid),
        Some((Status::Running, None))
    );
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let services = alan_shell::Shell::new(InProcessTransport::new(namespace))
        .ls("/srv")
        .await
        .unwrap();
    assert!(services.iter().any(|service| service == "connection"));
    assert!(services.iter().any(|service| service == "llm"));
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn package_service_process_restart_republishes_its_catalog_handle() {
    let manager = ServiceManager::boot(ServiceManagerConfig::ephemeral(
        "test",
        AgentProcessConfig::default(),
        ProcessLaunchContext::root(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    ))
    .await
    .unwrap();
    let mut retained_context = ProcessLaunchContext::root();
    manager
        .reference_package(&mut retained_context, "alan-memory")
        .await
        .unwrap();
    let retained_package = alan_shell::Shell::new(InProcessTransport::new(Arc::new(
        alan_kernel::MountFs::new(retained_context.namespace.clone()),
    )));
    assert!(retained_package.ls("/lib/pkg/alan-memory").await.is_ok());
    let (_, _, namespace) = manager.local_entry().create_and_handoff().await.unwrap();
    let shell = alan_shell::Shell::new(InProcessTransport::new(namespace));
    assert_eq!(
        shell
            .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
            .await
            .unwrap()
            .exit_code,
        0
    );
    let old_pid = Pid(manager
        .state()
        .lock()
        .await
        .unit("package")
        .unwrap()
        .pid
        .unwrap());
    manager.terminate_unit("package", 1).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if !shell
                .ls("/srv")
                .await
                .unwrap()
                .iter()
                .any(|handle| handle == "package")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Package Service handle was not invalidated");
    let mut unavailable_context = ProcessLaunchContext::root();
    let error = manager
        .reference_package(&mut unavailable_context, "alan-memory")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Package Service is unavailable");
    assert!(unavailable_context.package_references.is_empty());
    assert!(retained_package.ls("/lib/pkg/alan-memory").await.is_ok());
    let unavailable = shell
        .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
        .await
        .unwrap();
    assert_eq!(unavailable.exit_code, 1);
    let unavailable_output = String::from_utf8(unavailable.output).unwrap();
    assert!(
        unavailable_output.contains("submit command failed"),
        "unexpected unavailable output: {unavailable_output}"
    );
    let new_pid = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let unit = manager.state().lock().await.unit("package").unwrap();
            if unit.status == crate::UnitStatus::Ready && unit.pid != Some(old_pid.0) {
                break Pid(unit.pid.unwrap());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Package Service was not restarted");
    assert_ne!(new_pid, old_pid);
    assert!(
        shell
            .ls("/srv")
            .await
            .unwrap()
            .iter()
            .any(|handle| handle == "package")
    );
    assert!(shell.ls("/mnt/package").await.is_ok());
    manager
        .reference_package(&mut unavailable_context, "alan-memory")
        .await
        .unwrap();
    assert_eq!(unavailable_context.package_references.len(), 1);
    let list = shell
        .run(QUARTERMASTER_EXECUTABLE, &["list".to_string()])
        .await
        .unwrap();
    assert_eq!(list.exit_code, 0);
    assert!(
        String::from_utf8(list.output)
            .unwrap()
            .contains("alan-memory")
    );
    manager.shutdown().await.unwrap();
}
