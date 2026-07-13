use alan_agent_engine::{
    AGENT_DEFINITION_DESCRIPTOR, AgentProcessConfig, ConfigSourceKind, HostMountGrant,
    ProcessDescriptor, ProcessLaunchContext, ResolvedAgentDefinition,
    runtime::effective_core_config_for_runtime, skills::SkillScope,
};
use alan_kernel::{Access, Credentials, Namespace};

fn launch_context_with_mounts(mounts: Vec<HostMountGrant>, cwd: &str) -> ProcessLaunchContext {
    let mut namespace = Namespace::new();
    alan_os_host::host_mounts::apply_host_mount_declarations(&mut namespace, &mounts).unwrap();
    let mut context =
        ProcessLaunchContext::new(namespace, Credentials::user("integration-agent"), cwd).unwrap();
    context.host_mounts = mounts;
    context
}

#[test]
fn host_directories_are_not_agent_definitions_without_a_descriptor() {
    let source = tempfile::tempdir().unwrap();
    let implicit_definition = source.path().join(".alan/agents/default");
    std::fs::create_dir_all(implicit_definition.join("persona")).unwrap();
    std::fs::write(
        implicit_definition.join("agent.toml"),
        "model_reasoning_effort = \"high\"\n",
    )
    .unwrap();
    std::fs::write(
        implicit_definition.join("persona/SOUL.md"),
        "must not be discovered",
    )
    .unwrap();

    let context = launch_context_with_mounts(
        vec![HostMountGrant::new("/mnt/source", source.path(), Access::ReadWrite).unwrap()],
        "/mnt/source",
    );
    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();

    assert!(resolved.root_dir.is_none());
    assert!(resolved.persona_dirs.is_empty());
    assert!(
        resolved
            .capability_view
            .packages
            .iter()
            .all(|package| package.scope != SkillScope::Descriptor)
    );

    let config = AgentProcessConfig {
        launch_context: context,
        ..AgentProcessConfig::default()
    };
    let effective = effective_core_config_for_runtime(&config).unwrap();
    assert_eq!(effective.model_reasoning_effort, None);
}

#[test]
fn explicit_descriptor_resolves_one_definition_tree() {
    let source = tempfile::tempdir().unwrap();
    let definition = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(definition.path().join("persona")).unwrap();
    std::fs::create_dir_all(definition.path().join("skills/reviewer")).unwrap();
    std::fs::write(
        definition.path().join("agent.toml"),
        "model_reasoning_effort = \"high\"\n",
    )
    .unwrap();
    std::fs::write(
        definition.path().join("persona/SOUL.md"),
        "descriptor persona",
    )
    .unwrap();
    std::fs::write(
        definition.path().join("skills/reviewer/SKILL.md"),
        "---\nname: Reviewer\ndescription: Review changes\n---\n",
    )
    .unwrap();
    std::fs::write(
        definition.path().join("policy.yaml"),
        "default_action: allow\n",
    )
    .unwrap();

    let mut context = launch_context_with_mounts(
        vec![
            HostMountGrant::new("/mnt/source", source.path(), Access::ReadWrite).unwrap(),
            HostMountGrant::new("/agent-definition", definition.path(), Access::ReadOnly).unwrap(),
        ],
        "/mnt/source",
    );
    context.descriptors.insert(
        AGENT_DEFINITION_DESCRIPTOR.to_string(),
        ProcessDescriptor::new("/agent-definition").unwrap(),
    );

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();
    assert_eq!(resolved.root_dir.as_deref(), Some(definition.path()));
    assert_eq!(
        resolved.persona_dirs,
        vec![definition.path().join("persona")]
    );
    assert_eq!(
        resolved.policy_path.as_deref(),
        Some(definition.path().join("policy.yaml").as_path())
    );
    assert!(
        resolved
            .capability_view
            .packages
            .iter()
            .any(|package| package.id == "skill:reviewer")
    );

    let config = AgentProcessConfig {
        launch_context: context,
        ..AgentProcessConfig::default()
    };
    let effective = effective_core_config_for_runtime(&config).unwrap();
    assert_eq!(
        effective.model_reasoning_effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}
