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
    pub config_path: Option<PathBuf>,
    pub persona_dirs: Vec<PathBuf>,
    pub capability_view: ResolvedCapabilityView,
    pub skill_overrides: Vec<SkillOverride>,
    pub policy_path: Option<PathBuf>,
}

impl ResolvedAgentDefinition {
    pub fn from_launch_context(
        launch_context: &ProcessLaunchContext,
        base_skill_overrides: &[SkillOverride],
        base_source: ConfigSourceKind,
    ) -> Result<Self> {
        let package_roots = resolve_package_references(launch_context)?;
        let Some(descriptor) = launch_context.descriptor(AGENT_DEFINITION_DESCRIPTOR) else {
            return Self::empty(base_skill_overrides, package_roots);
        };
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
            config_path: config_path.exists().then_some(config_path),
            persona_dirs: persona_dir
                .is_dir()
                .then_some(persona_dir)
                .into_iter()
                .collect(),
            capability_view: ResolvedCapabilityView::from_package_sources(
                vec![ScopedPackageDir {
                    path: skills_dir,
                    scope: SkillScope::Descriptor,
                }],
                package_roots,
            ),
            skill_overrides,
            policy_path: policy_path.exists().then_some(policy_path),
        };
        resolved
            .capability_view
            .validate_unique_runtime_skill_ids()
            .context("validate Process Skill package references")?;
        Ok(resolved)
    }

    fn empty(
        base_skill_overrides: &[SkillOverride],
        package_roots: Vec<crate::skills::ScopedPackageRoot>,
    ) -> Result<Self> {
        let resolved = Self {
            root_dir: None,
            config_path: None,
            persona_dirs: Vec::new(),
            capability_view: ResolvedCapabilityView::from_package_sources(
                Vec::new(),
                package_roots,
            ),
            skill_overrides: base_skill_overrides.to_vec(),
            policy_path: None,
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
) -> Result<Vec<crate::skills::ScopedPackageRoot>> {
    let mut roots = Vec::new();
    let mut selected_packages = std::collections::BTreeSet::new();
    for reference in &launch_context.package_references {
        ensure!(
            selected_packages.insert(reference.package_id.clone()),
            "duplicate package reference `{}`",
            reference.package_id
        );
        let declared_root = launch_context
            .host_path(&reference.namespace_path)
            .with_context(|| {
                format!(
                    "package reference {} is not backed by an explicit package projection",
                    reference.namespace_path
                )
            })?;
        let canonical_root = std::fs::canonicalize(&declared_root).with_context(|| {
            format!(
                "failed to resolve package reference {}",
                reference.namespace_path
            )
        })?;
        let resolved_namespace_path = launch_context
            .namespace_path(&canonical_root)
            .with_context(|| {
                format!(
                    "package reference {} escapes its package projection",
                    reference.namespace_path
                )
            })?;
        ensure!(
            resolved_namespace_path == reference.namespace_path,
            "package reference {} resolves to a different Alan OS path: {}",
            reference.namespace_path,
            resolved_namespace_path
        );
        let scope = match reference.kind {
            ProcessPackageKind::Preinstalled => SkillScope::Builtin,
            ProcessPackageKind::Installed => SkillScope::Installed,
        };
        let multiple_exports = reference.skills.len() > 1;
        for skill in &reference.skills {
            let declared_skill_root = canonical_root.join(&skill.path);
            let canonical_skill_root =
                std::fs::canonicalize(&declared_skill_root).with_context(|| {
                    format!(
                        "failed to resolve Skill `{}` from package `{}`",
                        skill.skill_id, reference.package_id
                    )
                })?;
            ensure!(
                canonical_skill_root.starts_with(&canonical_root),
                "Skill `{}` escapes package `{}`",
                skill.skill_id,
                reference.package_id
            );
            let expected_namespace_path = format!(
                "{}/{}",
                reference.namespace_path.trim_end_matches('/'),
                skill.path
            );
            ensure!(
                launch_context
                    .namespace_path(&canonical_skill_root)
                    .as_deref()
                    == Some(expected_namespace_path.as_str()),
                "Skill `{}` resolves outside package `{}`",
                skill.skill_id,
                reference.package_id
            );
            let metadata =
                crate::skills::load_skill_metadata(&canonical_skill_root.join("SKILL.md"), scope)
                    .with_context(|| {
                    format!(
                        "validate Skill `{}` from package `{}`",
                        skill.skill_id, reference.package_id
                    )
                })?;
            ensure!(
                metadata.id == skill.skill_id,
                "package `{}` declares Skill `{}` but its runtime id is `{}`",
                reference.package_id,
                skill.skill_id,
                metadata.id
            );
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
            roots.push(crate::skills::ScopedPackageRoot {
                package_id,
                path: canonical_skill_root,
                namespace_root: Some(PathBuf::from(expected_namespace_path)),
                scope,
                dependencies: skill.dependencies.clone(),
            });
        }
    }
    Ok(roots)
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
                )
                .unwrap(),
            ],
        )
        .unwrap();
        let context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/lib/pkg/review-pack", &package, Access::ReadOnly).unwrap(),
            )
            .with_package_reference(reference);

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
        let metadata = registry.get(&"reviewer".to_string()).unwrap();
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
            crate::skills::SkillContentSource::File(path)
                if path == &package.root_dir.as_ref().unwrap().join("SKILL.md")
        ));
        assert!(!resolved.capability_view.packages.iter().any(|package| {
            package
                .portable_skill
                .path
                .ends_with("unreferenced/SKILL.md")
        }));
    }

    #[test]
    fn launch_rejects_skill_id_collision_across_package_and_definition_descriptors() {
        let host = tempfile::tempdir().unwrap();
        let package = host.path().join("package");
        let definition = host.path().join("definition");
        write_skill(&package, "reviewer");
        write_skill(&definition, "reviewer");
        let reference = ProcessPackageReference::new(
            "review-pack",
            "b".repeat(64),
            ProcessPackageKind::Installed,
            "/lib/pkg/review-pack",
            vec![
                ProcessPackageSkillReference::new("reviewer", "skills/reviewer", Vec::new())
                    .unwrap(),
            ],
        )
        .unwrap();
        let context = ProcessLaunchContext::root()
            .with_host_mount(
                HostMountGrant::new("/lib/pkg/review-pack", &package, Access::ReadOnly).unwrap(),
            )
            .with_host_mount(
                HostMountGrant::new("/mnt/definition", &definition, Access::ReadOnly).unwrap(),
            )
            .with_package_reference(reference)
            .with_descriptor(
                AGENT_DEFINITION_DESCRIPTOR,
                ProcessDescriptor::new("/mnt/definition").unwrap(),
            );

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
