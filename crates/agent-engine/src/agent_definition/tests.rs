use super::*;
use crate::skills::SkillTypedDependency;
use crate::{ProcessDescriptor, ProcessPackageReference, ProcessPackageSkillReference};
use alan_kernel::Access;

fn package_descriptor(id: &str) -> crate::ProcessFileTree {
    crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
        "SKILL.md".to_string(),
        format!("---\nname: {id}\ndescription: Test Skill.\n---\n").into_bytes(),
    )]))
    .unwrap()
}

fn package_descriptor_with_malformed_sidecars(id: &str) -> crate::ProcessFileTree {
    crate::ProcessFileTree::new(std::collections::BTreeMap::from([
        (
            "SKILL.md".to_string(),
            format!("---\nname: {id}\ndescription: Test Skill.\n---\n").into_bytes(),
        ),
        ("package.yaml".to_string(), b"not: [valid".to_vec()),
        ("skill.yaml".to_string(), b"not: [valid".to_vec()),
        ("agents/openai.yaml".to_string(), b"not: [valid".to_vec()),
    ]))
    .unwrap()
}

fn package_handle() -> alan_ap::InProcessTransport {
    alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_ap::reference::MemFs::new()))
}

#[test]
fn file_tree_definition_resolves_without_host_backing() {
    let tree = crate::ProcessFileTree::new(std::collections::BTreeMap::from([
        (
            "agent.toml".to_string(),
            b"tool_repeat_limit = 7\n".to_vec(),
        ),
        ("persona/ROLE.md".to_string(), b"Package reviewer".to_vec()),
        (
            "skills/reviewer/SKILL.md".to_string(),
            b"---\nname: Reviewer\ndescription: Review changes.\n---\n".to_vec(),
        ),
        (
            "skills/reviewer/agents/critic/agent.toml".to_string(),
            b"tool_repeat_limit = 3\n".to_vec(),
        ),
        (
            "policy.yaml".to_string(),
            b"default_action: deny\nrules: []\n".to_vec(),
        ),
    ]))
    .unwrap();
    let context = ProcessLaunchContext::root().with_descriptor(
        AGENT_DEFINITION_DESCRIPTOR,
        ProcessDescriptor::with_file_tree("/lib/pkg/review/agents/root", tree).unwrap(),
    );

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();

    assert!(resolved.root_dir.is_none());
    assert_eq!(
        resolved.namespace_root.as_deref(),
        Some(Path::new("/lib/pkg/review/agents/root"))
    );
    assert!(resolved.config_path.is_none());
    assert!(resolved.persona_dirs.is_empty());
    assert_eq!(
        resolved.config_content.as_deref(),
        Some("tool_repeat_limit = 7\n")
    );
    assert!(
        resolved
            .persona_context
            .as_deref()
            .is_some_and(|context| context.contains("Package reviewer"))
    );
    assert_eq!(
        resolved.policy_path.as_deref(),
        Some(Path::new("/lib/pkg/review/agents/root/policy.yaml"))
    );
    let registry = crate::skills::SkillsRegistry::load_capability_view(
        &resolved.capability_view,
        &resolved.skill_overrides,
    )
    .unwrap();
    assert!(registry.has(&"reviewer".to_string()));
    let package = resolved
        .capability_view
        .packages
        .iter()
        .find(|package| package.id == "skill:reviewer")
        .unwrap();
    let export = package.exports.child_agent_export("critic").unwrap();
    assert!(
        export
            .file_tree
            .as_ref()
            .is_some_and(|tree| tree.contains_file("agent.toml"))
    );
}

#[test]
fn file_tree_definitions_canonicalize_local_skill_ids() {
    let tree = crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
        "skills/Repo Review/SKILL.md".to_string(),
        b"---\nname: Repo Review\ndescription: Review changes.\n---\n".to_vec(),
    )]))
    .unwrap();
    let tree_context = ProcessLaunchContext::root().with_descriptor(
        AGENT_DEFINITION_DESCRIPTOR,
        ProcessDescriptor::with_file_tree("/lib/pkg/review/agents/root", tree).unwrap(),
    );

    let tree_resolved =
        ResolvedAgentDefinition::from_launch_context(&tree_context, &[], ConfigSourceKind::Default)
            .unwrap();

    assert_eq!(
        tree_resolved.capability_view.packages[0].id,
        "skill:repo-review"
    );
    let tree_registry = crate::skills::SkillsRegistry::load_capability_view(
        &tree_resolved.capability_view,
        &tree_resolved.skill_overrides,
    )
    .unwrap();
    assert!(tree_registry.has(&"repo-review".to_string()));
}

#[test]
fn process_without_definition_descriptor_has_no_definition() {
    let context = ProcessLaunchContext::root();

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();

    assert!(resolved.root_dir.is_none());
    assert!(resolved.persona_dirs.is_empty());
}

fn write_skill(root: &Path, id: &str) {
    std::fs::create_dir_all(root.join("skills").join(id)).unwrap();
    std::fs::write(
        root.join("skills").join(id).join("SKILL.md"),
        format!("---\nname: {id}\ndescription: Test Skill.\n---\n"),
    )
    .unwrap();
}

#[test]
fn typed_package_reference_selects_only_manifest_skill_roots() {
    let host = tempfile::tempdir().unwrap();
    let package = host.path().join("package");
    write_skill(&package, "reviewer");
    write_skill(&package, "unreferenced");
    let dependency = SkillTypedDependency::RuntimeCapability {
        name: "review-runtime".to_string(),
        description: None,
    };
    let handle = package_handle();
    let reference = ProcessPackageReference::new(
        "review-pack",
        "a".repeat(64),
        ProcessPackageKind::Installed,
        "/lib/pkg/review-pack",
        vec![
            ProcessPackageSkillReference::new(
                "reviewer",
                "skills/reviewer",
                vec![dependency.clone()],
                package_descriptor("reviewer"),
            )
            .unwrap(),
        ],
        handle.clone(),
    )
    .unwrap();
    let mut context = ProcessLaunchContext::root().with_package_reference(reference);
    context
        .namespace
        .mount("/lib/pkg/review-pack", handle, Access::ReadOnly);

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();
    assert_eq!(resolved.capability_view.packages.len(), 1);
    let package = &resolved.capability_view.packages[0];
    assert_eq!(package.id, "installed:review-pack:reviewer");
    assert_eq!(package.dependencies, vec![dependency]);
    assert_eq!(
        package.namespace_root.as_deref(),
        Some(Path::new("/lib/pkg/review-pack/skills/reviewer"))
    );
    let registry = crate::skills::SkillsRegistry::load_capability_view(
        &resolved.capability_view,
        &resolved.skill_overrides,
    )
    .unwrap();
    let metadata = registry
        .get(&"reviewer".to_string())
        .unwrap_or_else(|| panic!("registry errors: {:?}", registry.errors()));
    assert_eq!(
        metadata.path,
        PathBuf::from("/lib/pkg/review-pack/skills/reviewer/SKILL.md")
    );
    assert_eq!(
        metadata.package_root.as_deref(),
        Some(Path::new("/lib/pkg/review-pack/skills/reviewer"))
    );
    assert_eq!(metadata.package_root, metadata.resource_root);
    assert!(matches!(
        &metadata.source,
        crate::skills::SkillContentSource::Descriptor { .. }
    ));
    assert!(!resolved.capability_view.packages.iter().any(|package| {
        package
            .portable_skill
            .path
            .ends_with("unreferenced/SKILL.md")
    }));
}

#[test]
fn package_reference_requires_its_exact_read_only_namespace_mount() {
    let handle = package_handle();
    let reference = ProcessPackageReference::new(
        "review-pack",
        "e".repeat(64),
        ProcessPackageKind::Installed,
        "/lib/pkg/review-pack",
        vec![
            ProcessPackageSkillReference::new(
                "reviewer",
                "skills/reviewer",
                Vec::new(),
                package_descriptor("reviewer"),
            )
            .unwrap(),
        ],
        handle.clone(),
    )
    .unwrap();
    let mut context = ProcessLaunchContext::root().with_package_reference(reference);

    let error =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("requires one exact read-only Process namespace mount")
    );

    context.namespace.mount(
        "/lib/pkg/review-pack",
        handle,
        alan_kernel::Access::ReadWrite,
    );
    let error =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("requires one exact read-only Process namespace mount")
    );
}

#[test]
fn malformed_package_descriptor_sidecars_are_non_fatal_registry_errors() {
    let handle = package_handle();
    let reference = ProcessPackageReference::new(
        "review-pack",
        "f".repeat(64),
        ProcessPackageKind::Installed,
        "/lib/pkg/review-pack",
        vec![
            ProcessPackageSkillReference::new(
                "reviewer",
                "skills/reviewer",
                Vec::new(),
                package_descriptor_with_malformed_sidecars("reviewer"),
            )
            .unwrap(),
        ],
        handle.clone(),
    )
    .unwrap();
    let mut context = ProcessLaunchContext::root().with_package_reference(reference);
    context
        .namespace
        .mount("/lib/pkg/review-pack", handle, Access::ReadOnly);

    let resolved =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap();
    let registry = crate::skills::SkillsRegistry::load_capability_view(
        &resolved.capability_view,
        &resolved.skill_overrides,
    )
    .unwrap();

    assert!(registry.get(&"reviewer".to_string()).is_some());
    let error_paths = registry
        .errors()
        .iter()
        .map(|error| error.path.as_path())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        error_paths,
        std::collections::BTreeSet::from([
            Path::new("/lib/pkg/review-pack/skills/reviewer/agents/openai.yaml"),
            Path::new("/lib/pkg/review-pack/skills/reviewer/package.yaml"),
            Path::new("/lib/pkg/review-pack/skills/reviewer/skill.yaml"),
        ])
    );
}

#[test]
fn launch_rejects_skill_id_collision_across_package_and_definition_descriptors() {
    let host = tempfile::tempdir().unwrap();
    let package = host.path().join("package");
    let definition = host.path().join("definition");
    write_skill(&package, "reviewer");
    write_skill(&definition, "reviewer");
    let handle = package_handle();
    let reference = ProcessPackageReference::new(
        "review-pack",
        "b".repeat(64),
        ProcessPackageKind::Installed,
        "/lib/pkg/review-pack",
        vec![
            ProcessPackageSkillReference::new(
                "reviewer",
                "skills/reviewer",
                Vec::new(),
                package_descriptor("reviewer"),
            )
            .unwrap(),
        ],
        handle.clone(),
    )
    .unwrap();
    let mut context = ProcessLaunchContext::root()
        .with_package_reference(reference)
        .with_descriptor(
            AGENT_DEFINITION_DESCRIPTOR,
            ProcessDescriptor::with_file_tree(
                "/agent-definition",
                crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
                    "skills/reviewer/SKILL.md".to_string(),
                    b"---\nname: reviewer\ndescription: Test Skill.\n---\n".to_vec(),
                )]))
                .unwrap(),
            )
            .unwrap(),
        );
    context
        .namespace
        .mount("/lib/pkg/review-pack", handle, Access::ReadOnly);

    let error =
        ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
            .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("Duplicate runtime Skill id"), "{message}");
}
