use super::*;
use std::collections::BTreeMap;

fn tree(path: &str, body: &str) -> crate::ProcessFileTree {
    crate::ProcessFileTree::new(BTreeMap::from([(
        path.into(),
        format!("---\nname: Test Skill\ndescription: test\n---\n{body}\n").into_bytes(),
    )]))
    .unwrap()
}

pub(super) fn definition(
    descriptor_body: &str,
    package_body: &str,
) -> crate::ResolvedAgentDefinition {
    let descriptor = crate::ProcessDescriptor::with_file_tree(
        "/agent-definition",
        tree("skills/descriptor-check/SKILL.md", descriptor_body),
    )
    .unwrap();
    let package = crate::ProcessPackageReference::new(
        "qualification",
        "a".repeat(64),
        crate::ProcessPackageKind::Installed,
        "/lib/pkg/qualification",
        vec![
            crate::ProcessPackageSkillReference::new(
                "package-check",
                "skills/package-check",
                Vec::new(),
                tree("SKILL.md", package_body),
            )
            .unwrap(),
        ],
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
    )
    .unwrap();
    crate::ResolvedAgentDefinition::from_process_inputs(
        Some(&descriptor),
        &[package],
        &[],
        crate::ConfigSourceKind::Default,
    )
    .unwrap()
}

#[tokio::test]
async fn skill_runtime_startup_and_separate_immutable_references() {
    let temp = TempDir::new().unwrap();
    let old = definition("OLD_DESCRIPTOR_AUTHORITY", "OLD_PACKAGE_AUTHORITY");
    // Capture different explicit revisions while retaining the old resolved
    // file-tree handles. No ActiveSkillEnvelope is carried between Processes.
    let fresh = definition("NEW_DESCRIPTOR_AUTHORITY", "NEW_PACKAGE_AUTHORITY");
    let mut failed = old.clone();
    failed
        .capability_view
        .packages
        .push(failed.capability_view.packages[0].clone());
    for (pid, authority, expected, excluded) in [
        (41, old.clone(), "OLD", "NEW"),
        (42, fresh, "NEW", "OLD"),
        (43, failed, "UNKNOWN", "OLD"),
    ] {
        let mock = MockLlmProvider::new();
        let llmfs = Arc::new(alan_llmfs::LlmFs::new());
        llmfs.register_connection("default", Box::new(mock.clone()));
        let agent_path = format!("/agent/{pid}");
        let mut ns = alan_kernel::Namespace::new();
        ns.mount(
            &agent_path,
            InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
            alan_kernel::Access::ReadWrite,
        );
        ns.mount(
            "/mnt/llm",
            InProcessTransport::new(llmfs),
            alan_kernel::Access::ReadWrite,
        );
        let root = InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(ns)));
        let shell = alan_shell::Shell::new(root.clone());
        let mut core = crate::Config::default();
        core.memory.enabled = false;
        core.streaming_mode = crate::config::StreamingMode::Off;
        let env = NamespaceRuntimeEnvironment::new(root, &agent_path, "default");
        let mut runtime = spawn_with_namespace_environment(
            AgentProcessConfig {
                agent_config: crate::AgentConfig::from(core.clone()),
                agent_definition: authority,
                store_bindings: Some(crate::AgentRuntimeStoreBindings {
                    rollouts: temp.path().join("rollouts"),
                    checkpoints: temp.path().join("checkpoints"),
                    cache: temp.path().join("cache"),
                    tmp: temp.path().join("tmp"),
                    metadata: temp.path().join("metadata"),
                }),
                ..Default::default()
            },
            env,
            crate::skills::SkillHostCapabilities::default(),
            crate::provider_capabilities_for_config(&core),
        )
        .unwrap();
        runtime.wait_until_ready().await.unwrap();
        let snapshot: alan_agent_protocol::UiSkillSnapshot = serde_json::from_slice(
            &shell
                .cat(&format!("{agent_path}/machine/ui/skills"))
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot.process_path, format!("/proc/{pid}"));
        assert_eq!(snapshot.publication_version, 1);
        if expected == "UNKNOWN" {
            assert!(!snapshot.known);
            assert!(snapshot.mentionable_skill_ids.is_empty());
            runtime.shutdown().await.unwrap();
            continue;
        }
        assert!(snapshot.known);
        assert_eq!(
            snapshot.mentionable_skill_ids,
            vec!["descriptor-check", "package-check"]
        );
        assert!(
            String::from_utf8(shell.cat(&format!("{agent_path}/events")).await.unwrap())
                .unwrap()
                .contains("ui:skills\n")
        );
        runtime
            .handle
            .submission_tx
            .send(Submission::new(Op::Input {
                parts: vec![ContentPart::text("$descriptor-check $package-check")],
                mode: InputMode::FollowUp,
            }))
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while mock.recorded_requests().is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        runtime.shutdown().await.unwrap();
        let requests = mock.recorded_requests();
        let prompt = requests[0].system_prompt.as_ref().unwrap();
        assert!(
            prompt.contains(&format!("{expected}_DESCRIPTOR_AUTHORITY")),
            "{prompt}"
        );
        assert!(
            prompt.contains(&format!("{expected}_PACKAGE_AUTHORITY")),
            "{prompt}"
        );
        assert!(!prompt.contains(&format!("{excluded}_DESCRIPTOR_AUTHORITY")));
        assert!(!prompt.contains(&format!("{excluded}_PACKAGE_AUTHORITY")));
    }
}
