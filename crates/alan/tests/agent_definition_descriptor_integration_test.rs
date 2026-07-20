use std::collections::BTreeMap;
use std::sync::Arc;

use alan_agent_engine::{
    AGENT_DEFINITION_DESCRIPTOR, AgentProcessConfig, ConfigSourceKind, ProcessDescriptor,
    ProcessFileTree, ProcessLaunchContext, ResolvedAgentDefinition,
    runtime::effective_core_config_for_runtime, skills::SkillScope,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, Credentials, Namespace};

fn namespace_with_mount(path: &str, access: Access) -> Namespace {
    let mut namespace = Namespace::new();
    namespace.mount(
        path,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        access,
    );
    namespace
}

#[test]
fn mounted_file_trees_are_not_agent_definitions_without_a_descriptor() {
    let context = ProcessLaunchContext::new(
        namespace_with_mount("/mnt/source", Access::ReadWrite),
        Credentials::user("integration-agent"),
        "/mnt/source",
    )
    .unwrap();
    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();

    assert!(resolved.namespace_root.is_none());
    assert!(resolved.persona_context.is_none());
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
fn explicit_descriptor_resolves_one_immutable_definition_tree() {
    let definition = ProcessFileTree::new(BTreeMap::from([
        (
            "agent.toml".to_string(),
            b"model_reasoning_effort = \"high\"\n".to_vec(),
        ),
        (
            "persona/SOUL.md".to_string(),
            b"descriptor persona".to_vec(),
        ),
        (
            "skills/reviewer/SKILL.md".to_string(),
            b"---\nname: Reviewer\ndescription: Review changes\n---\n".to_vec(),
        ),
        (
            "policy.yaml".to_string(),
            b"default_action: allow\n".to_vec(),
        ),
    ]))
    .unwrap();
    let context = ProcessLaunchContext::new(
        namespace_with_mount("/agent-definition", Access::ReadOnly),
        Credentials::user("integration-agent"),
        "/",
    )
    .unwrap()
    .with_descriptor(
        AGENT_DEFINITION_DESCRIPTOR,
        ProcessDescriptor::with_file_tree("/agent-definition", definition).unwrap(),
    );

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();
    assert!(resolved.root_dir.is_none());
    assert_eq!(
        resolved.namespace_root.as_deref(),
        Some(std::path::Path::new("/agent-definition"))
    );
    assert!(
        resolved
            .persona_context
            .as_deref()
            .is_some_and(|context| context.contains("descriptor persona"))
    );
    assert_eq!(
        resolved.policy_path.as_deref(),
        Some(std::path::Path::new("/agent-definition/policy.yaml"))
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
