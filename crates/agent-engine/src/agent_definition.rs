use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::{
    AGENT_DEFINITION_DESCRIPTOR, ConfigSourceKind, ProcessLaunchContext,
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
        let Some(descriptor) = launch_context.descriptor(AGENT_DEFINITION_DESCRIPTOR) else {
            return Ok(Self::empty(base_skill_overrides));
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

        Ok(Self {
            root_dir: Some(root_dir),
            config_path: config_path.exists().then_some(config_path),
            persona_dirs: persona_dir
                .is_dir()
                .then_some(persona_dir)
                .into_iter()
                .collect(),
            capability_view: ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
                path: skills_dir,
                scope: SkillScope::Descriptor,
            }]),
            skill_overrides,
            policy_path: policy_path.exists().then_some(policy_path),
        })
    }

    fn empty(base_skill_overrides: &[SkillOverride]) -> Self {
        Self {
            root_dir: None,
            config_path: None,
            persona_dirs: Vec::new(),
            capability_view: ResolvedCapabilityView::from_package_dirs(Vec::new()),
            skill_overrides: base_skill_overrides.to_vec(),
            policy_path: None,
        }
    }
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
    use crate::{HostMountGrant, ProcessDescriptor};
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
