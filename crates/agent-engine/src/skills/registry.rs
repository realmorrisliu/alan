//! Skills registry for managing discovered skills.

use crate::skills::loader;
use crate::skills::types::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, warn};

/// Registry of discovered skills.
#[derive(Clone, Default)]
pub struct SkillsRegistry {
    /// Skills indexed by ID.
    skills: HashMap<SkillId, SkillMetadata>,
    /// Non-fatal errors encountered during loading.
    errors: Vec<SkillError>,
    /// Filesystem paths whose metadata determines whether the registry is stale.
    tracked_paths: Vec<PathBuf>,
}

impl SkillsRegistry {
    pub fn load_capability_view(
        capability_view: &ResolvedCapabilityView,
        skill_overrides: &[SkillOverride],
    ) -> Result<Self, SkillsError> {
        let mut registry = Self::default();
        registry.reload_capability_view(capability_view, skill_overrides)?;
        Ok(registry)
    }

    #[cfg(test)]
    pub(crate) fn load_package_dirs(
        package_dirs: &[ScopedPackageDir],
    ) -> Result<Self, SkillsError> {
        let capability_view = ResolvedCapabilityView::from_package_dirs(package_dirs.to_vec());
        Self::load_capability_view(&capability_view, &[])
    }

    /// Get a skill's metadata by ID.
    pub fn get(&self, id: &SkillId) -> Option<&SkillMetadata> {
        self.skills.get(id)
    }

    /// Load full skill content by ID.
    pub fn load_skill(&self, id: &SkillId) -> Result<Skill, SkillsError> {
        let metadata = self
            .skills
            .get(id)
            .ok_or_else(|| SkillsError::NotFound(id.clone()))?;

        let mut skill = match &metadata.source {
            SkillContentSource::File(path) => loader::load_skill(path, metadata.scope)?,
            SkillContentSource::Embedded(content)
            | SkillContentSource::Descriptor { content, .. } => loader::load_skill_from_content(
                content,
                &metadata.path,
                metadata.scope,
                metadata.source.clone(),
                metadata.package_id.clone(),
            )?,
        };
        skill.metadata.package_id = metadata.package_id.clone();
        skill.metadata.source = metadata.source.clone();
        skill.metadata.enabled = metadata.enabled;
        skill.metadata.allow_implicit_invocation = metadata.allow_implicit_invocation;
        skill.metadata.package_root = metadata.package_root.clone();
        skill.metadata.resource_root = metadata.resource_root.clone();
        skill.metadata.capabilities = metadata.capabilities.clone();
        skill.metadata.compatibility = metadata.compatibility.clone();
        skill.metadata.alan_metadata = metadata.alan_metadata.clone();
        skill.metadata.compatible_metadata = metadata.compatible_metadata.clone();
        skill.metadata.execution = metadata.execution.clone();
        Ok(skill)
    }

    /// List all registered skills.
    pub fn list(&self) -> Vec<&SkillMetadata> {
        self.skills.values().collect()
    }

    /// List skill loading errors (if any).
    pub fn errors(&self) -> &[SkillError] {
        &self.errors
    }

    /// Return filesystem paths whose metadata determines whether the registry is stale.
    pub fn tracked_paths(&self) -> &[PathBuf] {
        &self.tracked_paths
    }

    /// List skills sorted by scope priority.
    pub fn list_sorted(&self) -> Vec<&SkillMetadata> {
        let mut skills: Vec<_> = self.skills.values().collect();
        skills.sort_by(|left, right| {
            left.scope
                .priority()
                .cmp(&right.scope.priority())
                .then_with(|| left.id.cmp(&right.id))
        });
        skills
    }

    /// Find skills matching a query using the portable selection surface.
    pub fn find_matches(&self, query: &str) -> Vec<&SkillMetadata> {
        let query_lower = query.to_lowercase();
        let keywords: Vec<_> = query_lower.split_whitespace().collect();

        self.skills
            .values()
            .filter(|skill| {
                let desc_lower = skill.description.to_lowercase();
                let name_lower = skill.name.to_lowercase();

                keywords
                    .iter()
                    .any(|keyword| name_lower.contains(keyword) || desc_lower.contains(keyword))
            })
            .collect()
    }

    /// Check if a skill exists.
    pub fn has(&self, id: &SkillId) -> bool {
        self.skills.contains_key(id)
    }

    /// Get the number of registered skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    fn reload_capability_view(
        &mut self,
        capability_view: &ResolvedCapabilityView,
        skill_overrides: &[SkillOverride],
    ) -> Result<(), SkillsError> {
        self.skills.clear();
        self.errors.clear();
        self.tracked_paths.clear();
        self.apply_capability_view(capability_view.refresh(), skill_overrides)
    }

    fn apply_capability_view(
        &mut self,
        capability_view: ResolvedCapabilityView,
        skill_overrides: &[SkillOverride],
    ) -> Result<(), SkillsError> {
        self.errors.extend(capability_view.errors);
        self.errors.extend(capability_view.descriptor_errors);
        self.tracked_paths.extend(capability_view.tracked_paths);
        let overrides_by_skill: HashMap<String, SkillOverride> = skill_overrides
            .iter()
            .cloned()
            .map(|override_config| (override_config.skill_id.clone(), override_config))
            .collect();

        for package in capability_view.packages {
            let track_package_paths = package.scope != SkillScope::Builtin;
            let package_root = package.root_dir.clone();
            let resource_root = package.root_dir.clone();
            let namespace_root = package.namespace_root.clone();
            let package_sidecar_path = package_root.as_deref().map(loader::package_sidecar_path);
            let compatibility_metadata_path = package_root
                .as_deref()
                .map(loader::compatibility_metadata_path);
            if track_package_paths && let Some(path) = package_sidecar_path.as_ref() {
                self.tracked_paths.push(path.clone());
            }
            if track_package_paths && let Some(path) = compatibility_metadata_path.as_ref() {
                self.tracked_paths.push(path.clone());
            }
            let package_sidecar = package.package_sidecar.clone().or_else(|| package_root
                .as_deref()
                .and_then(|root| match loader::load_package_sidecar(root) {
                    Ok(sidecar) => sidecar,
                    Err(err) => {
                        let sidecar_path = loader::package_sidecar_path(root);
                        warn!(
                            path = %sidecar_path.display(),
                            package_id = %package.id,
                            error = %err,
                            "Failed to load package sidecar metadata; continuing without package defaults"
                        );
                        self.errors.push(SkillError {
                            path: sidecar_path,
                            message: err.to_string(),
                        });
                        None
                    }
                }));
            let compatibility_metadata = package.compatible_metadata.clone().or_else(||
                package_root
                    .as_deref()
                    .and_then(|root| match loader::load_compatibility_metadata(root) {
                        Ok(metadata) => metadata,
                        Err(err) => {
                            let metadata_path = loader::compatibility_metadata_path(root);
                            warn!(
                                path = %metadata_path.display(),
                                package_id = %package.id,
                                error = %err,
                                "Failed to load compatibility metadata; continuing without compatibility hints"
                            );
                            self.errors.push(SkillError {
                                path: metadata_path,
                                message: err.to_string(),
                            });
                            None
                        }
                    }));

            let mut loaded_skills = Vec::new();
            let portable_skill = package.portable_skill;

            match &portable_skill.source {
                SkillContentSource::File(path) => {
                    match loader::load_skill_metadata(path, package.scope) {
                        Ok(mut metadata) => {
                            metadata.package_id = Some(package.id.clone());
                            metadata.source = portable_skill.source.clone();
                            metadata.package_root = package_root.clone();
                            metadata.resource_root = resource_root.clone();
                            if let Some(compatible_metadata) = compatibility_metadata.as_ref() {
                                metadata.compatible_metadata = compatible_metadata.clone();
                            }
                            self.apply_sidecar_metadata(
                                &mut metadata,
                                track_package_paths,
                                package.skill_sidecar.as_ref(),
                                package
                                    .package_sidecar
                                    .as_ref()
                                    .map(|sidecar| &sidecar.skill_defaults),
                                package_sidecar
                                    .as_ref()
                                    .zip(package_sidecar_path.as_deref())
                                    .map(|(sidecar, path)| (&sidecar.skill_defaults, path)),
                            );
                            loaded_skills.push(metadata);
                        }
                        Err(err) => {
                            warn!(
                                path = %path.display(),
                                package_id = %package.id,
                                error = %err,
                                "Failed to load portable skill metadata"
                            );
                            self.errors.push(SkillError {
                                path: path.to_path_buf(),
                                message: err.to_string(),
                            });
                        }
                    }
                }
                SkillContentSource::Embedded(content)
                | SkillContentSource::Descriptor { content, .. } => {
                    match loader::parse_skill_metadata_with_source(
                        content,
                        &portable_skill.path,
                        package.scope,
                        portable_skill.source.clone(),
                        Some(package.id.clone()),
                    ) {
                        Ok(mut metadata) => {
                            metadata.package_root = package_root.clone();
                            metadata.resource_root = resource_root.clone();
                            if let Some(compatible_metadata) = compatibility_metadata.as_ref() {
                                metadata.compatible_metadata = compatible_metadata.clone();
                            }
                            self.apply_sidecar_metadata(
                                &mut metadata,
                                track_package_paths,
                                package.skill_sidecar.as_ref(),
                                package
                                    .package_sidecar
                                    .as_ref()
                                    .map(|sidecar| &sidecar.skill_defaults),
                                package_sidecar
                                    .as_ref()
                                    .zip(package_sidecar_path.as_deref())
                                    .map(|(sidecar, path)| (&sidecar.skill_defaults, path)),
                            );
                            loaded_skills.push(metadata);
                        }
                        Err(err) => {
                            warn!(
                                path = %portable_skill.path.display(),
                                package_id = %package.id,
                                error = %err,
                                "Failed to parse embedded portable skill metadata"
                            );
                            self.errors.push(SkillError {
                                path: portable_skill.path.clone(),
                                message: err.to_string(),
                            });
                        }
                    }
                }
            }

            let child_agent_exports = package.exports.child_agent_export_names();

            for mut metadata in loaded_skills {
                for dependency in &package.dependencies {
                    if !metadata
                        .compatibility
                        .dependencies
                        .iter()
                        .any(|existing| existing.identity_key() == dependency.identity_key())
                    {
                        metadata.compatibility.dependencies.push(dependency.clone());
                    }
                }
                let skill_id = metadata.id.clone();
                if self.skills.contains_key(&skill_id) {
                    return Err(SkillsError::DuplicateSkill(skill_id));
                }
                self.resolve_runtime_exposure(
                    &mut metadata,
                    overrides_by_skill.get(skill_id.as_str()),
                );
                metadata.execution = resolve_skill_execution(&metadata, &child_agent_exports);
                if let Some(namespace_root) = namespace_root.as_ref() {
                    metadata.path = namespace_root.join("SKILL.md");
                    metadata.package_root = Some(namespace_root.clone());
                    metadata.resource_root = Some(namespace_root.clone());
                }
                debug!(
                    "Registering skill: {} (package: {}, scope: {:?}, enabled: {}, implicit: {}, path: {})",
                    metadata.id,
                    package.id,
                    package.scope,
                    metadata.enabled,
                    metadata.allow_implicit_invocation,
                    metadata.path.display()
                );
                self.skills.insert(metadata.id.clone(), metadata);
            }
        }

        self.tracked_paths.sort();
        self.tracked_paths.dedup();
        Ok(())
    }

    fn apply_sidecar_metadata(
        &mut self,
        metadata: &mut SkillMetadata,
        track_skill_sidecar_path: bool,
        supplied_skill_sidecar: Option<&AlanSkillSidecar>,
        supplied_package_defaults: Option<&AlanSkillSidecar>,
        package_defaults: Option<(&AlanSkillSidecar, &std::path::Path)>,
    ) {
        if supplied_skill_sidecar.is_some() || supplied_package_defaults.is_some() {
            let defaults = supplied_package_defaults
                .or_else(|| package_defaults.map(|(defaults, _)| defaults));
            if let Err(err) = metadata.apply_sidecar_metadata(defaults, supplied_skill_sidecar) {
                self.errors.push(SkillError {
                    path: metadata.path.clone(),
                    message: err.to_string(),
                });
            }
            return;
        }
        if let Some((defaults, sidecar_path)) = package_defaults {
            self.apply_sidecar_overlay(metadata, defaults, sidecar_path);
        }

        if !matches!(metadata.source, SkillContentSource::File(_)) {
            return;
        }

        let Some(skill_sidecar_path) = loader::skill_sidecar_path(&metadata.path) else {
            return;
        };
        if track_skill_sidecar_path {
            self.tracked_paths.push(skill_sidecar_path.clone());
        }
        let skill_sidecar = match loader::load_skill_sidecar(&metadata.path) {
            Ok(sidecar) => sidecar,
            Err(err) => {
                warn!(
                    path = %skill_sidecar_path.display(),
                    skill_id = %metadata.id,
                    error = %err,
                    "Failed to load skill sidecar metadata; continuing without this sidecar overlay"
                );
                self.errors.push(SkillError {
                    path: skill_sidecar_path.clone(),
                    message: err.to_string(),
                });
                None
            }
        };

        if let Some(sidecar) = skill_sidecar.as_ref() {
            self.apply_sidecar_overlay(metadata, sidecar, &skill_sidecar_path);
        }
    }

    fn apply_sidecar_overlay(
        &mut self,
        metadata: &mut SkillMetadata,
        sidecar: &AlanSkillSidecar,
        sidecar_path: &std::path::Path,
    ) {
        if let Err(err) = metadata.apply_sidecar_metadata(None, Some(sidecar)) {
            warn!(
                path = %sidecar_path.display(),
                skill_id = %metadata.id,
                error = %err,
                "Failed to merge sidecar metadata; continuing without this sidecar overlay"
            );
            self.errors.push(SkillError {
                path: sidecar_path.to_path_buf(),
                message: err.to_string(),
            });
        }
    }

    fn resolve_runtime_exposure(
        &self,
        metadata: &mut SkillMetadata,
        override_config: Option<&SkillOverride>,
    ) {
        metadata.enabled = override_config
            .and_then(|entry| entry.enabled)
            .unwrap_or(true);
        metadata.allow_implicit_invocation = metadata
            .alan_metadata
            .allow_implicit_invocation
            .or(metadata
                .compatible_metadata
                .policy
                .allow_implicit_invocation)
            .unwrap_or(true);

        if let Some(allow_implicit_invocation) =
            override_config.and_then(|entry| entry.allow_implicit_invocation)
        {
            metadata.allow_implicit_invocation = allow_implicit_invocation;
        }
    }
}

#[cfg(test)]
mod tests;
