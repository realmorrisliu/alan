use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

use crate::{
    AGENT_DEFINITION_DESCRIPTOR, ConfigSourceKind, ProcessLaunchContext, ProcessPackageKind,
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
        let file_tree = descriptor.file_tree.as_ref().with_context(|| {
            format!(
                "Agent Definition descriptor {} must carry an immutable file-tree handle",
                descriptor.path
            )
        })?;
        Self::from_file_tree(
            descriptor,
            file_tree,
            base_skill_overrides,
            base_source,
            package_capabilities,
            package_errors,
        )
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
        let exact_mounts = launch_context.namespace.union_at(&reference.namespace_path);
        ensure!(
            exact_mounts.len() == 1 && exact_mounts[0].access == alan_kernel::Access::ReadOnly,
            "package reference {} requires one exact read-only Process namespace mount",
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

#[cfg(test)]
mod tests;
