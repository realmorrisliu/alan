use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::{
    AGENT_DEFINITION_DESCRIPTOR, ConfigSourceKind, ProcessLaunchContext, ProcessPackageKind,
    config::merge_skill_override_overlays_from_paths,
    skills::{ResolvedCapabilityView, ScopedPackageDir, SkillOverride, SkillScope},
};

/// Agent Definition resolved only from the descriptor passed at Process launch.
#[derive(Debug, Clone)]
pub struct ResolvedAgentDefinition {
    pub root_dir: Option<PathBuf>,
    pub namespace_root: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub persona_dirs: Vec<PathBuf>,
    pub capability_view: ResolvedCapabilityView,
    pub skill_overrides: Vec<SkillOverride>,
    pub policy_path: Option<PathBuf>,
    pub config_content: Option<String>,
    pub persona_context: Option<String>,
    pub descriptor_tree: Option<crate::ProcessFileTree>,
}

impl ResolvedAgentDefinition {
    fn config_overlay_source(&self) -> Option<PathBuf> {
        self.namespace_root
            .as_ref()
            .map(|root| root.join("agent.toml"))
    }

    pub fn apply_to_agent_config(&self, base: &crate::AgentConfig) -> Result<crate::AgentConfig> {
        if let Some(content) = self.config_content.as_deref() {
            let source = self
                .config_overlay_source()
                .unwrap_or_else(|| PathBuf::from("/agent-definition/agent.toml"));
            return base.with_definition_overlay_content(content, &source);
        }
        if let Some(config_path) = self.config_path.as_ref() {
            return base.with_definition_overlays(std::slice::from_ref(config_path));
        }
        Ok(base.clone())
    }

    pub fn from_launch_context(
        launch_context: &ProcessLaunchContext,
        base_skill_overrides: &[SkillOverride],
        base_source: ConfigSourceKind,
    ) -> Result<Self> {
        let (package_capabilities, package_errors) = resolve_package_references(launch_context)?;
        let Some(descriptor) = launch_context.descriptor(AGENT_DEFINITION_DESCRIPTOR) else {
            return Self::empty(base_skill_overrides, package_capabilities, package_errors);
        };
        if let Some(file_tree) = descriptor.file_tree.as_ref() {
            return Self::from_file_tree(
                descriptor,
                file_tree,
                base_skill_overrides,
                base_source,
                package_capabilities,
                package_errors,
            );
        }
        let declared_root = launch_context
            .host_path(&descriptor.path)
            .with_context(|| {
                format!(
                    "Agent Definition descriptor {} is not backed by an explicit Host Mount",
                    descriptor.path
                )
            })?;
        let canonical_root = std::fs::canonicalize(&declared_root).with_context(|| {
            format!(
                "failed to resolve Agent Definition descriptor {}",
                descriptor.path
            )
        })?;
        let resolved_namespace_path = launch_context
            .namespace_path(&canonical_root)
            .with_context(|| {
                format!(
                    "Agent Definition descriptor {} escapes its explicit Host Mount",
                    descriptor.path
                )
            })?;
        ensure!(
            resolved_namespace_path == descriptor.path,
            "Agent Definition descriptor {} resolves to a different Alan OS path: {}",
            descriptor.path,
            resolved_namespace_path
        );
        validate_definition_tree(&declared_root)?;
        let root_dir = declared_root;
        let config_path = root_dir.join("agent.toml");
        let persona_dir = root_dir.join("persona");
        let skills_dir = root_dir.join("skills");
        let policy_path = root_dir.join("policy.yaml");
        let config_paths = (base_source != ConfigSourceKind::EnvOverride && config_path.exists())
            .then_some(config_path.clone())
            .into_iter()
            .collect::<Vec<_>>();
        let skill_overrides =
            merge_skill_override_overlays_from_paths(base_skill_overrides, &config_paths)?;
        let resolved = Self {
            root_dir: Some(root_dir),
            namespace_root: Some(PathBuf::from(&descriptor.path)),
            config_path: config_path.exists().then_some(config_path),
            persona_dirs: persona_dir
                .is_dir()
                .then_some(persona_dir)
                .into_iter()
                .collect(),
            capability_view: capability_view_with_packages(
                vec![ScopedPackageDir {
                    path: skills_dir,
                    scope: SkillScope::Descriptor,
                }],
                package_capabilities,
                package_errors,
            ),
            skill_overrides,
            policy_path: policy_path.exists().then_some(policy_path),
            config_content: None,
            persona_context: None,
            descriptor_tree: None,
        };
        resolved
            .capability_view
            .validate_unique_runtime_skill_ids()
            .context("validate Process Skill package references")?;
        Ok(resolved)
    }

    fn empty(
        base_skill_overrides: &[SkillOverride],
        package_capabilities: Vec<crate::skills::CapabilityPackage>,
        package_errors: Vec<crate::skills::SkillError>,
    ) -> Result<Self> {
        let resolved = Self {
            root_dir: None,
            namespace_root: None,
            config_path: None,
            persona_dirs: Vec::new(),
            capability_view: capability_view_with_packages(
                Vec::new(),
                package_capabilities,
                package_errors,
            ),
            skill_overrides: base_skill_overrides.to_vec(),
            policy_path: None,
            config_content: None,
            persona_context: None,
            descriptor_tree: None,
        };
        resolved
            .capability_view
            .validate_unique_runtime_skill_ids()
            .context("validate Process Skill package references")?;
        Ok(resolved)
    }

    fn from_file_tree(
        descriptor: &crate::ProcessDescriptor,
        file_tree: &crate::ProcessFileTree,
        base_skill_overrides: &[SkillOverride],
        base_source: ConfigSourceKind,
        mut package_capabilities: Vec<crate::skills::CapabilityPackage>,
        mut package_errors: Vec<crate::skills::SkillError>,
    ) -> Result<Self> {
        let config_content = (base_source != ConfigSourceKind::EnvOverride)
            .then(|| file_tree.text("agent.toml"))
            .transpose()?
            .flatten()
            .map(str::to_string);
        let skill_overrides = match config_content.as_deref() {
            Some(content) => crate::config::merge_skill_override_overlay_from_content(
                base_skill_overrides,
                content,
                Path::new(&descriptor.path).join("agent.toml").as_path(),
            )?,
            None => base_skill_overrides.to_vec(),
        };
        for name in file_tree.child_dirs("skills") {
            let skill_id = crate::skills::name_to_id(&name);
            let prefix = format!("skills/{name}");
            let tree = file_tree.subtree(&prefix)?;
            if !tree.contains_file("SKILL.md") {
                continue;
            }
            let namespace_root = Path::new(&descriptor.path).join(&prefix);
            let (package, errors) = capability_package_from_descriptor(
                format!("skill:{skill_id}"),
                &skill_id,
                namespace_root.clone(),
                SkillScope::Descriptor,
                Vec::new(),
                &tree,
            )?;
            package_capabilities.push(package);
            package_errors.extend(errors);
        }
        let persona_context = crate::prompts::render_definition_persona_context_from_file_tree(
            file_tree,
            &descriptor.path,
        );
        let resolved = Self {
            root_dir: None,
            namespace_root: Some(PathBuf::from(&descriptor.path)),
            config_path: None,
            persona_dirs: Vec::new(),
            capability_view: capability_view_with_packages(
                Vec::new(),
                package_capabilities,
                package_errors,
            ),
            skill_overrides,
            policy_path: file_tree
                .contains_file("policy.yaml")
                .then(|| Path::new(&descriptor.path).join("policy.yaml")),
            config_content,
            persona_context: (!persona_context.is_empty()).then_some(persona_context),
            descriptor_tree: Some(file_tree.clone()),
        };
        resolved
            .capability_view
            .validate_unique_runtime_skill_ids()
            .context("validate Process Skill package references")?;
        Ok(resolved)
    }
}

fn resolve_package_references(
    launch_context: &ProcessLaunchContext,
) -> Result<(
    Vec<crate::skills::CapabilityPackage>,
    Vec<crate::skills::SkillError>,
)> {
    let mut packages = Vec::new();
    let mut errors = Vec::new();
    let mut selected_packages = std::collections::BTreeSet::new();
    for reference in &launch_context.package_references {
        ensure!(
            selected_packages.insert(reference.package_id.clone()),
            "duplicate package reference `{}`",
            reference.package_id
        );
        ensure!(
            launch_context
                .namespace
                .resolve(&reference.namespace_path)
                .is_ok(),
            "package reference {} is not mounted in the Process namespace",
            reference.namespace_path
        );
        let scope = match reference.kind {
            ProcessPackageKind::Preinstalled => SkillScope::Builtin,
            ProcessPackageKind::Installed => SkillScope::Installed,
        };
        let multiple_exports = reference.skills.len() > 1;
        for skill in &reference.skills {
            let namespace_root = PathBuf::from(format!(
                "{}/{}",
                reference.namespace_path.trim_end_matches('/'),
                skill.path
            ));
            let package_id = match (reference.kind, multiple_exports) {
                (ProcessPackageKind::Preinstalled, false) => {
                    format!("builtin:{}", reference.package_id)
                }
                (ProcessPackageKind::Preinstalled, true) => {
                    format!("builtin:{}:{}", reference.package_id, skill.skill_id)
                }
                (ProcessPackageKind::Installed, _) => {
                    format!("installed:{}:{}", reference.package_id, skill.skill_id)
                }
            };
            let (package, sidecar_errors) = capability_package_from_descriptor(
                package_id,
                &skill.skill_id,
                namespace_root.clone(),
                scope,
                skill.dependencies.clone(),
                &skill.descriptor,
            )?;
            packages.push(package);
            errors.extend(sidecar_errors);
            ensure!(
                packages
                    .last()
                    .map(|package| package.portable_skill.path.parent())
                    == Some(Some(namespace_root.as_path())),
                "package Skill descriptor root changed during capability assembly"
            );
        }
    }
    Ok((packages, errors))
}

fn capability_package_from_descriptor(
    package_id: String,
    expected_skill_id: &str,
    namespace_root: PathBuf,
    scope: SkillScope,
    dependencies: Vec<crate::skills::SkillTypedDependency>,
    descriptor: &crate::ProcessFileTree,
) -> Result<(
    crate::skills::CapabilityPackage,
    Vec<crate::skills::SkillError>,
)> {
    let document = descriptor
        .text("SKILL.md")?
        .context("Skill descriptor has no SKILL.md")?;
    let source = crate::skills::SkillContentSource::Descriptor {
        content: std::sync::Arc::from(document),
        file_tree: descriptor.clone(),
    };
    let metadata = crate::skills::parse_skill_metadata_with_source(
        document,
        &namespace_root.join("SKILL.md"),
        scope,
        source.clone(),
        Some(package_id.clone()),
    )?;
    ensure!(
        metadata.id == expected_skill_id,
        "package `{package_id}` declares Skill `{expected_skill_id}` but its runtime id is `{}`",
        metadata.id
    );
    let mut errors = Vec::new();
    let package_sidecar = parse_optional_descriptor_metadata(
        descriptor,
        crate::skills::PACKAGE_SIDECAR_FILE,
        &namespace_root,
        &package_id,
        &mut errors,
        crate::skills::parse_package_sidecar,
    );
    let skill_sidecar = parse_optional_descriptor_metadata(
        descriptor,
        crate::skills::SKILL_SIDECAR_FILE,
        &namespace_root,
        &package_id,
        &mut errors,
        crate::skills::parse_skill_sidecar,
    );
    let compatible_metadata = parse_optional_descriptor_metadata(
        descriptor,
        "agents/openai.yaml",
        &namespace_root,
        &package_id,
        &mut errors,
        |content| crate::skills::parse_compatibility_metadata(content, &namespace_root),
    )
    .flatten();
    let child_agents = descriptor
        .child_dirs("agents")
        .into_iter()
        .filter_map(|name| {
            let prefix = format!("agents/{name}");
            let looks_like_definition = descriptor.contains_file(&format!("{prefix}/agent.toml"))
                || descriptor.contains_dir(&format!("{prefix}/persona"))
                || descriptor.contains_dir(&format!("{prefix}/skills"))
                || descriptor.contains_file(&format!("{prefix}/policy.yaml"));
            looks_like_definition.then(|| {
                Ok(crate::skills::CapabilityChildAgentExport {
                    name: name.clone(),
                    root_dir: namespace_root.join(&prefix),
                    handle: crate::skills::CapabilityChildAgentExport::package_handle(
                        &package_id,
                        &name,
                    ),
                    file_tree: Some(descriptor.subtree(&prefix)?),
                })
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let resource = |name: &str| {
        descriptor
            .contains_dir(name)
            .then(|| namespace_root.join(name))
    };
    let package = crate::skills::CapabilityPackage {
        id: package_id,
        scope,
        root_dir: None,
        namespace_root: Some(namespace_root.clone()),
        exports: crate::skills::CapabilityPackageExports {
            child_agents,
            resources: crate::skills::CapabilityPackageResources {
                bin_dir: resource("bin"),
                scripts_dir: resource("scripts"),
                references_dir: resource("references"),
                assets_dir: resource("assets"),
            },
        },
        portable_skill: crate::skills::PortableSkill {
            path: namespace_root.join("SKILL.md"),
            source,
        },
        dependencies,
        package_sidecar,
        skill_sidecar,
        compatible_metadata,
    };
    Ok((package, errors))
}

fn parse_optional_descriptor_metadata<T, E>(
    descriptor: &crate::ProcessFileTree,
    relative_path: &str,
    namespace_root: &Path,
    package_id: &str,
    errors: &mut Vec<crate::skills::SkillError>,
    parse: impl FnOnce(&str) -> std::result::Result<T, E>,
) -> Option<T>
where
    E: std::fmt::Display,
{
    let path = namespace_root.join(relative_path);
    let result = descriptor
        .text(relative_path)
        .map_err(|error| error.to_string())
        .and_then(|content| match content {
            Some(content) => parse(content).map(Some).map_err(|error| error.to_string()),
            None => Ok(None),
        });
    match result {
        Ok(value) => value,
        Err(message) => {
            tracing::warn!(
                path = %path.display(),
                package_id,
                error = %message,
                "Failed to load descriptor sidecar metadata; continuing without its overlay"
            );
            errors.push(crate::skills::SkillError { path, message });
            None
        }
    }
}

fn capability_view_with_packages(
    package_dirs: Vec<ScopedPackageDir>,
    packages: Vec<crate::skills::CapabilityPackage>,
    errors: Vec<crate::skills::SkillError>,
) -> ResolvedCapabilityView {
    let mut view = ResolvedCapabilityView::from_package_dirs(package_dirs);
    view.packages.extend(packages);
    view.descriptor_errors.extend(errors);
    view
}

fn validate_definition_tree(root_dir: &Path) -> Result<()> {
    let root_metadata = std::fs::symlink_metadata(root_dir).with_context(|| {
        format!(
            "failed to inspect Agent Definition tree {}",
            root_dir.display()
        )
    })?;
    ensure!(
        root_metadata.file_type().is_dir() && !root_metadata.file_type().is_symlink(),
        "Agent Definition descriptor must reference a real directory: {}",
        root_dir.display()
    );

    let mut pending = vec![root_dir.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).with_context(|| {
            format!(
                "failed to read Agent Definition tree {}",
                directory.display()
            )
        })?;
        for entry in entries {
            let entry = entry.with_context(|| {
                format!(
                    "failed to read Agent Definition tree {}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path).with_context(|| {
                format!("failed to inspect Agent Definition path {}", path.display())
            })?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "Agent Definition tree must not contain symlinks: {}",
                path.display()
            );
            if metadata.file_type().is_dir() {
                pending.push(path);
            } else {
                ensure!(
                    metadata.file_type().is_file(),
                    "Agent Definition tree contains an unsupported file type: {}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::SkillTypedDependency;
    use crate::{
        HostMountGrant, ProcessDescriptor, ProcessPackageReference, ProcessPackageSkillReference,
    };
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
    fn definition_resolves_only_inside_its_descriptor_tree() {
        let host = tempfile::tempdir().unwrap();
        let definition = host.path().join("definition");
        std::fs::create_dir_all(definition.join("persona")).unwrap();
        std::fs::create_dir_all(definition.join("skills/reviewer")).unwrap();
        std::fs::write(definition.join("persona/ROLE.md"), "Reviewer").unwrap();
        std::fs::write(
            definition.join("skills/reviewer/SKILL.md"),
            "---\nname: reviewer\ndescription: review\n---\n",
        )
        .unwrap();
        let context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/mnt/import", host.path(), Access::ReadOnly).unwrap(),
            )
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/import/definition").unwrap(),
            );

        let resolved =
            ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
                .unwrap();

        assert_eq!(resolved.root_dir, Some(definition.clone()));
        assert_eq!(resolved.persona_dirs, vec![definition.join("persona")]);
        assert!(
            resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:reviewer")
        );
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
        assert!(context.host_mounts.is_empty());
    }

    #[test]
    fn file_tree_and_host_definitions_canonicalize_local_skill_ids_identically() {
        let host = tempfile::tempdir().unwrap();
        let definition = host.path().join("definition");
        std::fs::create_dir_all(definition.join("skills/Repo Review")).unwrap();
        std::fs::write(
            definition.join("skills/Repo Review/SKILL.md"),
            "---\nname: Repo Review\ndescription: Review changes.\n---\n",
        )
        .unwrap();
        let host_context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/mnt/import", host.path(), Access::ReadOnly).unwrap(),
            )
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/import/definition").unwrap(),
            );
        let tree = crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
            "skills/Repo Review/SKILL.md".to_string(),
            b"---\nname: Repo Review\ndescription: Review changes.\n---\n".to_vec(),
        )]))
        .unwrap();
        let tree_context = ProcessLaunchContext::root().with_descriptor(
            AGENT_DEFINITION_DESCRIPTOR,
            ProcessDescriptor::with_file_tree("/lib/pkg/review/agents/root", tree).unwrap(),
        );

        let host_resolved = ResolvedAgentDefinition::from_launch_context(
            &host_context,
            &[],
            ConfigSourceKind::Default,
        )
        .unwrap();
        let tree_resolved = ResolvedAgentDefinition::from_launch_context(
            &tree_context,
            &[],
            ConfigSourceKind::Default,
        )
        .unwrap();

        assert_eq!(
            tree_resolved.capability_view.packages[0].id,
            host_resolved.capability_view.packages[0].id
        );
        assert_eq!(
            tree_resolved.capability_view.packages[0].id,
            "skill:repo-review"
        );
        let host_registry = crate::skills::SkillsRegistry::load_capability_view(
            &host_resolved.capability_view,
            &host_resolved.skill_overrides,
        )
        .unwrap();
        let tree_registry = crate::skills::SkillsRegistry::load_capability_view(
            &tree_resolved.capability_view,
            &tree_resolved.skill_overrides,
        )
        .unwrap();
        assert!(host_registry.has(&"repo-review".to_string()));
        assert!(tree_registry.has(&"repo-review".to_string()));
    }

    #[test]
    fn host_directories_are_not_scanned_without_a_descriptor() {
        let host = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(host.path().join(".alan/agents/default/persona")).unwrap();
        let context = ProcessLaunchContext::root().with_host_mount(
            HostMountGrant::new("/mnt/source", host.path(), Access::ReadOnly).unwrap(),
        );

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
            .with_host_mount(
                HostMountGrant::new("/mnt/definition", &definition, Access::ReadOnly).unwrap(),
            )
            .with_package_reference(reference)
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/definition").unwrap(),
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

    #[cfg(unix)]
    #[test]
    fn definition_descriptor_cannot_escape_its_host_mount_through_a_symlink() {
        use std::os::unix::fs::symlink;

        let host = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("agent.toml"), "model = \"secret\"\n").unwrap();
        symlink(outside.path(), host.path().join("definition")).unwrap();
        let context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/mnt/import", host.path(), Access::ReadOnly).unwrap(),
            )
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/import/definition").unwrap(),
            );

        let error =
            ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
                .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("escapes its explicit Host Mount")
        );
    }

    #[cfg(unix)]
    #[test]
    fn definition_tree_rejects_nested_symlinks() {
        use std::os::unix::fs::symlink;

        let host = tempfile::tempdir().unwrap();
        let definition = host.path().join("definition");
        std::fs::create_dir_all(definition.join("persona")).unwrap();
        let outside = host.path().join("outside.md");
        std::fs::write(&outside, "secret").unwrap();
        symlink(&outside, definition.join("persona/SOUL.md")).unwrap();
        let context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/mnt/import", host.path(), Access::ReadOnly).unwrap(),
            )
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/import/definition").unwrap(),
            );

        let error =
            ResolvedAgentDefinition::from_launch_context(&context, &[], ConfigSourceKind::Default)
                .unwrap_err();

        assert!(error.to_string().contains("must not contain symlinks"));
    }
}
