//! Real Package Service upgrade plus retained Service Manager Process authority.
use super::*;
use alan_agent_engine::{ProcessDescriptor, ProcessFileTree};

fn body(marker: &str) -> Vec<u8> {
    format!("---\nname: Check\ndescription: Lifecycle qualification\n---\n{marker}\n").into_bytes()
}

fn descriptor(source: &std::path::Path) -> ProcessDescriptor {
    ProcessDescriptor::with_file_tree(
        "/lib/agents/root",
        ProcessFileTree::new(BTreeMap::from([(
            "skills/descriptor-check/SKILL.md".into(),
            std::fs::read(source).unwrap(),
        )]))
        .unwrap(),
    )
    .unwrap()
}

async fn start(
    context: &ProcessLaunchContext,
    pid: u64,
) -> (
    alan_agent_engine::runtime::RuntimeController,
    alan_shell::Shell,
    MockLlmProvider,
) {
    let definition = alan_agent_engine::ResolvedAgentDefinition::from_process_inputs(
        context.descriptor(alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR),
        &context.package_references,
        &[],
        alan_agent_engine::ConfigSourceKind::Default,
    )
    .unwrap();
    let mock = MockLlmProvider::new();
    let llm = Arc::new(alan_llmfs::LlmFs::new());
    llm.register_connection("default", Box::new(mock.clone()));
    let mut ns = context.namespace_snapshot();
    ns.mount(
        &format!("/agent/{pid}"),
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        Access::ReadWrite,
    );
    ns.mount("/mnt/llm", InProcessTransport::new(llm), Access::ReadWrite);
    let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let mut core = alan_agent_engine::Config::default();
    core.memory.enabled = false;
    core.streaming_mode = serde_json::from_str("\"off\"").unwrap();
    let mut runtime = alan_agent_engine::runtime::spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: alan_agent_engine::AgentConfig::from(core.clone()),
            agent_definition: definition,
            ..Default::default()
        },
        alan_agent_engine::runtime::NamespaceRuntimeEnvironment::new(
            root,
            format!("/agent/{pid}"),
            "default",
        ),
        Default::default(),
        alan_agent_engine::provider_capabilities_for_config(&core),
    )
    .unwrap();
    runtime.wait_until_ready().await.unwrap();
    (runtime, shell, mock)
}

async fn invoke(runtime: &alan_agent_engine::runtime::RuntimeController, mock: &MockLlmProvider) {
    let before = mock.recorded_requests().len();
    runtime
        .handle
        .submission_tx
        .send(alan_agent_protocol::Submission::new(
            alan_agent_protocol::Op::Input {
                parts: vec![alan_agent_protocol::ContentPart::text(
                    "$descriptor-check $package-check",
                )],
                mode: alan_agent_protocol::InputMode::FollowUp,
            },
        ))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while mock.recorded_requests().len() <= before {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn real_upgrade_keeps_live_process_revision_until_lease_release() {
    let source = tempfile::TempDir::new().unwrap();
    let descriptor_path = source.path().join("descriptor.md");
    let package_dir = source.path().join("package-source");
    std::fs::create_dir_all(package_dir.join("package-check")).unwrap();
    let package_path = package_dir.join("package-check/SKILL.md");
    std::fs::write(&descriptor_path, body("OLD_DESCRIPTOR_SOURCE")).unwrap();
    std::fs::write(&package_path, body("OLD_PACKAGE_SOURCE")).unwrap();
    let service = PackageService::ephemeral("test").unwrap();
    assert!(
        service
            .execute(crate::PackageCommand::Install {
                request_id: "install".into(),
                package_id: "qualification".into(),
                snapshot: crate::PackageSnapshot::from_directory(&package_dir).unwrap(),
            })
            .unwrap()
            .success
    );
    let old_revision = service.catalog().unwrap().packages["qualification"]
        .revision
        .clone();
    let old_root = service
        .acquire("qualification")
        .unwrap()
        .content_root()
        .to_path_buf();
    let mut old_context = ProcessLaunchContext::root().with_descriptor(
        alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
        descriptor(&descriptor_path),
    );
    project_package_reference(&service, &mut old_context, "qualification").unwrap();
    let (old, old_shell, old_mock) = start(&old_context, 71).await;
    invoke(&old, &old_mock).await;
    std::fs::write(&descriptor_path, body("NEW_DESCRIPTOR_SOURCE")).unwrap();
    std::fs::write(&package_path, body("NEW_PACKAGE_SOURCE")).unwrap();
    assert!(
        service
            .execute(crate::PackageCommand::Upgrade {
                request_id: "upgrade".into(),
                package_id: "qualification".into(),
                snapshot: crate::PackageSnapshot::from_directory(&package_dir).unwrap(),
            })
            .unwrap()
            .success
    );
    let new_revision = service.catalog().unwrap().packages["qualification"]
        .revision
        .clone();
    assert_ne!(old_revision, new_revision);
    assert_eq!(old_context.package_references[0].revision, old_revision);
    assert!(
        old_root.exists(),
        "upgrade GC must retain leased live revision"
    );
    invoke(&old, &old_mock).await;
    let requests = old_mock.recorded_requests();
    let prompt = requests.last().unwrap().system_prompt.as_ref().unwrap();
    assert!(prompt.contains("OLD_DESCRIPTOR_SOURCE") && prompt.contains("OLD_PACKAGE_SOURCE"));
    assert!(!prompt.contains("NEW_DESCRIPTOR_SOURCE") && !prompt.contains("NEW_PACKAGE_SOURCE"));
    assert!(
        String::from_utf8(
            old_shell
                .cat("/lib/pkg/qualification/skills/package-check/SKILL.md")
                .await
                .unwrap()
        )
        .unwrap()
        .contains("OLD_PACKAGE_SOURCE")
    );
    let mut fresh_context = ProcessLaunchContext::root().with_descriptor(
        alan_agent_engine::AGENT_DEFINITION_DESCRIPTOR,
        descriptor(&descriptor_path),
    );
    project_package_reference(&service, &mut fresh_context, "qualification").unwrap();
    assert_eq!(fresh_context.package_references[0].revision, new_revision);
    let (fresh, fresh_shell, fresh_mock) = start(&fresh_context, 72).await;
    invoke(&fresh, &fresh_mock).await;
    fresh.shutdown().await.unwrap();
    let requests = fresh_mock.recorded_requests();
    let prompt = requests.last().unwrap().system_prompt.as_ref().unwrap();
    assert!(prompt.contains("NEW_DESCRIPTOR_SOURCE") && prompt.contains("NEW_PACKAGE_SOURCE"));
    assert!(!prompt.contains("OLD_DESCRIPTOR_SOURCE") && !prompt.contains("OLD_PACKAGE_SOURCE"));
    assert!(
        String::from_utf8(
            fresh_shell
                .cat("/lib/pkg/qualification/skills/package-check/SKILL.md")
                .await
                .unwrap()
        )
        .unwrap()
        .contains("NEW_PACKAGE_SOURCE")
    );
    old.shutdown().await.unwrap();
    drop(old_shell);
    drop(old_context);
    std::fs::write(&package_path, body("THIRD_PACKAGE_SOURCE")).unwrap();
    assert!(
        service
            .execute(crate::PackageCommand::Upgrade {
                request_id: "post-release-gc".into(),
                package_id: "qualification".into(),
                snapshot: crate::PackageSnapshot::from_directory(&package_dir).unwrap(),
            })
            .unwrap()
            .success
    );
    assert!(
        !old_root.exists(),
        "unleased obsolete revision must be collected"
    );
}
