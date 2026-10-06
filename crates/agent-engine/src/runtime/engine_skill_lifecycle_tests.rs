//! Actual Runtime lifecycle regressions; no presentation/cache injection.
use super::*;

async fn start_skill_runtime(
    temp: &std::path::Path,
    pid: u64,
    definition: crate::ResolvedAgentDefinition,
    recovery: Option<std::path::PathBuf>,
) -> (
    crate::runtime::RuntimeController,
    alan_shell::Shell,
    MockLlmProvider,
    crate::runtime::RuntimeStartupMetadata,
) {
    let mock = MockLlmProvider::new();
    let llmfs = Arc::new(alan_llmfs::LlmFs::new());
    llmfs.register_connection("default", Box::new(mock.clone()));
    let path = format!("/agent/{pid}");
    let mut ns = alan_kernel::Namespace::new();
    ns.mount(
        &path,
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
    let mut runtime = spawn_with_namespace_environment(
        AgentProcessConfig {
            agent_config: crate::AgentConfig::from(core.clone()),
            agent_definition: definition,
            recovery_rollout_path: recovery,
            store_bindings: Some(crate::AgentRuntimeStoreBindings {
                rollouts: temp.join("rollouts"),
                checkpoints: temp.join("checkpoints"),
                cache: temp.join("cache"),
                tmp: temp.join("tmp"),
                metadata: temp.join("metadata"),
            }),
            ..Default::default()
        },
        NamespaceRuntimeEnvironment::new(root, &path, "default"),
        crate::skills::SkillHostCapabilities::default(),
        crate::provider_capabilities_for_config(&core),
    )
    .unwrap();
    let ready = runtime.wait_until_ready().await.unwrap();
    (runtime, shell, mock, ready)
}

async fn observation(shell: &alan_shell::Shell, pid: u64) -> alan_agent_protocol::UiSkillSnapshot {
    serde_json::from_slice(
        &shell
            .cat(&format!("/agent/{pid}/machine/ui/skills"))
            .await
            .unwrap(),
    )
    .unwrap()
}

async fn request(runtime: &crate::runtime::RuntimeController, mock: &MockLlmProvider, text: &str) {
    let before = mock.recorded_requests().len();
    runtime
        .handle
        .submission_tx
        .send(Submission::new(Op::Input {
            parts: vec![ContentPart::text(text)],
            mode: InputMode::FollowUp,
        }))
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
async fn alive_runtime_catalog_collision_failed_ensure_clears_public_ids() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("catalog");
    std::fs::create_dir_all(source.join("release-check")).unwrap();
    let content = "---\nname: Release\ndescription: test\n---\nOLD_CATALOG_BODY\n";
    std::fs::write(source.join("release-check/SKILL.md"), content).unwrap();
    let mut definition = super::skill_qualification::definition("descriptor", "package");
    definition.capability_view = crate::skills::ResolvedCapabilityView::from_package_dirs(vec![
        crate::skills::ScopedPackageDir {
            path: source.clone(),
            scope: crate::skills::SkillScope::Descriptor,
        },
    ]);
    let (runtime, shell, mock, _) = start_skill_runtime(temp.path(), 51, definition, None).await;
    let known = observation(&shell, 51).await;
    assert!(known.known);
    assert_eq!(known.mentionable_skill_ids, vec!["release-check"]);
    let events_before = shell.cat("/agent/51/events").await.unwrap();
    // Distinct writable roots normalize to the same canonical runtime Skill id.
    std::fs::create_dir_all(source.join("Release Check")).unwrap();
    std::fs::write(source.join("Release Check/SKILL.md"), content).unwrap();
    request(&runtime, &mock, "$release-check").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while observation(&shell, 51).await.known {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let unknown = observation(&shell, 51).await;
    assert!(!unknown.known);
    assert!(unknown.mentionable_skill_ids.is_empty());
    assert_eq!(unknown.process_path, known.process_path);
    assert!(unknown.publication_version > known.publication_version);
    let events = shell.cat("/agent/51/events").await.unwrap();
    assert!(
        events[events_before.len()..]
            .windows(b"ui:skills\n".len())
            .any(|record| record == b"ui:skills\n")
    );
    runtime.shutdown().await.unwrap();
    assert!(
        !mock
            .recorded_requests()
            .last()
            .unwrap()
            .system_prompt
            .as_ref()
            .unwrap()
            .contains("OLD_CATALOG_BODY")
    );
}

#[tokio::test]
async fn explicit_rollout_recovery_resets_skill_projection_and_current_authority() {
    let temp = TempDir::new().unwrap();
    let (old, shell, mock, ready) = start_skill_runtime(
        temp.path(),
        61,
        super::skill_qualification::definition("OLD_DESCRIPTOR_AUTHORITY", "OLD_PACKAGE_AUTHORITY"),
        None,
    )
    .await;
    request(&old, &mock, "$descriptor-check $package-check").await;
    let known = observation(&shell, 61).await;
    assert!(known.known);
    old.shutdown().await.unwrap();
    let rollout = ready.rollout_path.unwrap();
    for (pid, definition, present) in [
        (
            62,
            super::skill_qualification::definition(
                "NEW_DESCRIPTOR_AUTHORITY",
                "NEW_PACKAGE_AUTHORITY",
            ),
            true,
        ),
        (
            63,
            crate::ResolvedAgentDefinition::from_process_inputs(
                None,
                &[],
                &[],
                crate::ConfigSourceKind::Default,
            )
            .unwrap(),
            false,
        ),
    ] {
        let (fresh, shell, mock, ready) =
            start_skill_runtime(temp.path(), pid, definition, Some(rollout.clone())).await;
        assert!(
            mock.recorded_requests().is_empty(),
            "completed old input must not automatically execute on recovery"
        );
        assert_ne!(ready.rollout_path.as_ref().unwrap(), &rollout);
        let snapshot = observation(&shell, pid).await;
        assert_eq!(snapshot.process_path, format!("/proc/{pid}"));
        assert_ne!(snapshot.process_path, known.process_path);
        assert_eq!(snapshot.publication_version, 1);
        assert!(snapshot.known);
        if present {
            assert_eq!(
                snapshot.mentionable_skill_ids,
                vec!["descriptor-check", "package-check"]
            );
        } else {
            assert!(snapshot.mentionable_skill_ids.is_empty());
        }
        request(&fresh, &mock, "$descriptor-check $package-check").await;
        fresh.shutdown().await.unwrap();
        let requests = mock.recorded_requests();
        let prompt = requests.last().unwrap().system_prompt.as_ref().unwrap();
        assert!(!prompt.contains("OLD_DESCRIPTOR_AUTHORITY"));
        assert!(!prompt.contains("OLD_PACKAGE_AUTHORITY"));
        assert_eq!(prompt.contains("NEW_DESCRIPTOR_AUTHORITY"), present);
        assert_eq!(prompt.contains("NEW_PACKAGE_AUTHORITY"), present);
    }
}
