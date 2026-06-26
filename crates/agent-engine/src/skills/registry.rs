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
        registry.reload_capability_view(capability_view, skill_overrides);
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
            SkillContentSource::Embedded(content) => loader::load_skill_from_content(
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
    ) {
        self.skills.clear();
        self.errors.clear();
        self.tracked_paths.clear();
        self.apply_capability_view(capability_view.refresh(), skill_overrides);
    }

    fn apply_capability_view(
        &mut self,
        capability_view: ResolvedCapabilityView,
        skill_overrides: &[SkillOverride],
    ) {
        self.errors.extend(capability_view.errors);
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
            let package_sidecar = package_root
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
                });
            let compatibility_metadata =
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
                    });

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
                SkillContentSource::Embedded(content) => {
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
                let skill_id = metadata.id.clone();
                self.resolve_runtime_exposure(
                    &mut metadata,
                    overrides_by_skill.get(skill_id.as_str()),
                );
                metadata.execution = resolve_skill_execution(&metadata, &child_agent_exports);
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
    }

    fn apply_sidecar_metadata(
        &mut self,
        metadata: &mut SkillMetadata,
        track_skill_sidecar_path: bool,
        package_defaults: Option<(&AlanSkillSidecar, &std::path::Path)>,
    ) {
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
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    fn create_test_skill(dir: &std::path::Path, name: &str, skill_name: &str, description: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let mut file = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        writeln!(
            file,
            r#"---
name: {}
description: {}
---

Body
"#,
            skill_name, description
        )
        .unwrap();
    }

    fn create_skill_file(
        dir: &Path,
        skill_dir_name: &str,
        skill_name: &str,
        description: &str,
    ) -> PathBuf {
        let skill_dir = dir.join(skill_dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: {description}
---

Body
"#
            ),
        )
        .unwrap();
        skill_dir.join("SKILL.md")
    }

    fn capability_view_for_manual_package(
        package_id: &str,
        package_root: &Path,
        skill_path: &Path,
        child_agent_names: &[&str],
    ) -> ResolvedCapabilityView {
        let canonical_root = std::fs::canonicalize(package_root).unwrap();
        let child_agents: Vec<CapabilityChildAgentExport> = child_agent_names
            .iter()
            .map(|name| {
                let dir = package_root.join("agents").join(name);
                std::fs::create_dir_all(&dir).unwrap();
                let root_dir = std::fs::canonicalize(dir).unwrap();
                CapabilityChildAgentExport {
                    name: (*name).to_string(),
                    handle: CapabilityChildAgentExport::package_handle(package_id, name),
                    root_dir,
                }
            })
            .collect();
        let canonical_skill = std::fs::canonicalize(skill_path).unwrap();

        ResolvedCapabilityView {
            package_dirs: Vec::new(),
            packages: vec![CapabilityPackage {
                id: package_id.to_string(),
                scope: SkillScope::Repo,
                root_dir: Some(canonical_root),
                exports: CapabilityPackageExports {
                    child_agents,
                    resources: CapabilityPackageResources::default(),
                },
                portable_skill: PortableSkill {
                    path: canonical_skill.clone(),
                    source: SkillContentSource::File(canonical_skill),
                },
            }],
            errors: Vec::new(),
            tracked_paths: Vec::new(),
        }
    }

    #[test]
    fn load_package_dirs_registers_discovered_skill() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "repo-skill", "Repo Skill", "From repo");

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();

        assert!(registry.has(&"repo-skill".to_string()));
        assert_eq!(
            registry.get(&"repo-skill".to_string()).unwrap().scope,
            SkillScope::Repo
        );
    }

    #[test]
    fn load_capability_view_prefers_later_entries_for_same_skill_id() {
        let temp = TempDir::new().unwrap();
        let global_dir = temp.path().join("global");
        let workspace_dir = temp.path().join("workspace");

        create_test_skill(&global_dir, "shared-skill", "Shared Skill", "From global");
        create_test_skill(
            &workspace_dir,
            "shared-skill",
            "Shared Skill",
            "From workspace",
        );

        let capability_view = ResolvedCapabilityView::from_package_dirs(vec![
            ScopedPackageDir {
                path: global_dir,
                scope: SkillScope::User,
            },
            ScopedPackageDir {
                path: workspace_dir,
                scope: SkillScope::Repo,
            },
        ]);

        let registry = SkillsRegistry::load_capability_view(&capability_view, &[]).unwrap();
        let skill = registry.get(&"shared-skill".to_string()).unwrap();

        assert_eq!(skill.description, "From workspace");
        assert_eq!(skill.scope, SkillScope::Repo);
    }

    #[test]
    fn load_capability_view_applies_skill_overrides() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(
            &capability_view,
            &[
                SkillOverride {
                    skill_id: "memory".to_string(),
                    enabled: Some(true),
                    allow_implicit_invocation: Some(false),
                },
                SkillOverride {
                    skill_id: "plan".to_string(),
                    enabled: Some(false),
                    allow_implicit_invocation: None,
                },
            ],
        )
        .unwrap();
        let memory = registry.get(&"memory".to_string()).unwrap();
        let plan = registry.get(&"plan".to_string()).unwrap();

        assert!(memory.enabled);
        assert!(!memory.allow_implicit_invocation);
        assert!(!plan.enabled);
        assert!(registry.get(&"repo-coding".to_string()).is_some());
        assert!(registry.get(&"alan-shell-control".to_string()).is_some());
        assert!(registry.get(&"workspace-inspect".to_string()).is_some());
        assert!(registry.get(&"workspace-manager".to_string()).is_some());
    }

    #[test]
    fn load_capability_view_requires_exact_override_skill_ids() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(
            &capability_view,
            &[SkillOverride {
                skill_id: "workspace_manager".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: Some(false),
            }],
        )
        .unwrap();
        let workspace_manager = registry.get(&"workspace-manager".to_string()).unwrap();

        assert!(workspace_manager.enabled);
        assert!(workspace_manager.allow_implicit_invocation);
    }

    #[test]
    fn find_matches_uses_only_name_and_description() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        let skill_path = create_skill_file(
            &repo_skills,
            "test-skill",
            "Test Skill",
            "A skill for testing purposes",
        );
        std::fs::write(
            &skill_path,
            r#"---
name: Test Skill
description: A skill for testing purposes
metadata:
  tags: ["hidden-tag"]
---

Body
"#,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();

        let matches = registry.find_matches("test");
        assert!(!matches.is_empty(), "Should find at least one match");
        assert!(
            registry.find_matches("hidden-tag").is_empty(),
            "Tags should not participate in portable selection matching"
        );
    }

    #[test]
    fn list_sorted_is_stable_within_scope() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "b-skill", "B Skill", "B");
        create_test_skill(&repo_skills, "a-skill", "A Skill", "A");

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let ids: Vec<_> = registry
            .list_sorted()
            .into_iter()
            .filter(|skill| skill.scope == SkillScope::Repo)
            .map(|skill| skill.id.clone())
            .collect();

        assert_eq!(ids, vec!["a-skill".to_string(), "b-skill".to_string()]);
    }

    #[test]
    fn load_capability_view_applies_runtime_sidecar_metadata() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::write(
            repo_skills.join("test-skill").join(SKILL_SIDECAR_FILE),
            r#"
runtime:
  permission_hints:
    - "requires approval"
"#,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let skill = registry.get(&"test-skill".to_string()).unwrap();

        assert_eq!(
            skill.alan_metadata.permission_hints,
            vec!["requires approval".to_string()]
        );
    }

    #[test]
    fn load_capability_view_skill_sidecar_merges_runtime_metadata_with_package_defaults() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::write(
            repo_skills.join("test-skill").join(PACKAGE_SIDECAR_FILE),
            r#"
skill_defaults:
  runtime:
    permission_hints:
      - "package hint"
"#,
        )
        .unwrap();
        std::fs::write(
            repo_skills.join("test-skill").join(SKILL_SIDECAR_FILE),
            r#"
runtime:
  permission_hints:
    - "skill hint"
"#,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let skill = registry.get(&"test-skill".to_string()).unwrap();

        assert_eq!(
            skill.alan_metadata.permission_hints,
            vec!["package hint".to_string(), "skill hint".to_string()]
        );
    }

    #[test]
    fn load_capability_view_preserves_skill_md_contract() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        let skill_dir = repo_skills.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: From repo
capabilities:
  required_tools: ["read_file"]
  disclosure:
    level2: "instructions/expanded.md"
    level3:
      references: ["references/base.md"]
      scripts: ["scripts/base.sh"]
      assets: ["assets/base.txt"]
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join(SKILL_SIDECAR_FILE),
            r#"
runtime:
  permission_hints:
    - "review before use"
"#,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let capabilities = registry
            .get(&"test-skill".to_string())
            .unwrap()
            .capabilities
            .as_ref()
            .unwrap();

        assert_eq!(capabilities.required_tools, vec!["read_file".to_string()]);
        assert_eq!(capabilities.disclosure.level2, "instructions/expanded.md");
        assert_eq!(
            capabilities.disclosure.level3.references,
            vec!["references/base.md".to_string()]
        );
        assert_eq!(
            capabilities.disclosure.level3.scripts,
            vec!["scripts/base.sh".to_string()]
        );
        assert_eq!(
            capabilities.disclosure.level3.assets,
            vec!["assets/base.txt".to_string()]
        );
        assert_eq!(
            registry
                .get(&"test-skill".to_string())
                .unwrap()
                .alan_metadata
                .permission_hints,
            vec!["review before use".to_string()]
        );
    }

    #[test]
    fn load_capability_view_tracks_sidecar_files_for_cache_invalidation() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        let package_sidecar_path = repo_skills.join("test-skill").join(PACKAGE_SIDECAR_FILE);
        let skill_sidecar_path = repo_skills.join("test-skill").join(SKILL_SIDECAR_FILE);
        std::fs::write(&package_sidecar_path, "skill_defaults: {}\n").unwrap();
        std::fs::write(&skill_sidecar_path, "runtime: {}\n").unwrap();
        let package_sidecar = std::fs::canonicalize(package_sidecar_path).unwrap();
        let skill_sidecar = std::fs::canonicalize(skill_sidecar_path).unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();

        assert!(registry.tracked_paths().contains(&package_sidecar));
        assert!(registry.tracked_paths().contains(&skill_sidecar));
    }

    #[test]
    fn load_capability_view_ingests_openai_compatibility_metadata() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        let skill_root = repo_skills.join("test-skill");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::create_dir_all(skill_root.join("agents")).unwrap();
        std::fs::write(
            skill_root.join("agents").join(COMPATIBILITY_METADATA_FILE),
            r##"
interface:
  display_name: "Compatibility Title"
  short_description: "Compatibility short description"
  icon_small: "./assets/icon-small.svg"
  icon_large: "assets/icon-large.svg"
  brand_color: "#00aa44"
  default_prompt: "Use this skill carefully."
dependencies:
  tools:
    - type: "mcp"
      value: "openaiDeveloperDocs"
      description: "OpenAI Docs MCP server"
"##,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let skill = registry.get(&"test-skill".to_string()).unwrap();
        let expected_icon_small = std::fs::canonicalize(skill_root.join("assets/icon-small.svg"))
            .unwrap_or_else(|_| {
                std::fs::canonicalize(&skill_root)
                    .unwrap()
                    .join("assets/icon-small.svg")
            });
        let expected_icon_large = std::fs::canonicalize(skill_root.join("assets/icon-large.svg"))
            .unwrap_or_else(|_| {
                std::fs::canonicalize(&skill_root)
                    .unwrap()
                    .join("assets/icon-large.svg")
            });

        assert_eq!(
            skill.compatible_metadata.interface.display_name.as_deref(),
            Some("Compatibility Title")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .short_description
                .as_deref(),
            Some("Compatibility short description")
        );
        assert_eq!(
            skill.compatible_metadata.interface.icon_small.as_deref(),
            Some(expected_icon_small.as_path())
        );
        assert_eq!(
            skill.compatible_metadata.interface.icon_large.as_deref(),
            Some(expected_icon_large.as_path())
        );
        assert_eq!(
            skill.compatible_metadata.interface.brand_color.as_deref(),
            Some("#00aa44")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .default_prompt
                .as_deref(),
            Some("Use this skill carefully.")
        );
        assert_eq!(skill.display_name(), "Compatibility Title");
        assert_eq!(
            skill.effective_short_description(),
            Some("Compatibility short description")
        );
        assert_eq!(skill.compatible_metadata.dependencies.tools.len(), 1);
        assert_eq!(
            skill.compatible_metadata.dependencies.tools[0]
                .kind
                .as_deref(),
            Some("mcp")
        );
    }

    #[test]
    fn load_skill_preserves_compatible_metadata_from_registry() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        let skill_root = repo_skills.join("test-skill");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::create_dir_all(skill_root.join("agents")).unwrap();
        std::fs::write(
            skill_root.join("agents").join(COMPATIBILITY_METADATA_FILE),
            r##"
interface:
  display_name: "Compatibility Title"
  short_description: "Compatibility short description"
"##,
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();

        let skill = registry.load_skill(&"test-skill".to_string()).unwrap();

        assert_eq!(
            skill
                .metadata
                .compatible_metadata
                .interface
                .display_name
                .as_deref(),
            Some("Compatibility Title")
        );
        assert_eq!(
            skill.metadata.effective_short_description(),
            Some("Compatibility short description")
        );
    }

    #[test]
    fn load_capability_view_tracks_openai_compatibility_metadata_for_invalidation() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        let compatibility_path = repo_skills
            .join("test-skill")
            .join(COMPATIBILITY_METADATA_DIR)
            .join(COMPATIBILITY_METADATA_FILE);
        std::fs::create_dir_all(compatibility_path.parent().unwrap()).unwrap();
        std::fs::write(&compatibility_path, "interface: {}\n").unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let expected_path = std::fs::canonicalize(compatibility_path).unwrap();

        assert!(registry.tracked_paths().contains(&expected_path));
    }

    #[test]
    fn load_capability_view_tracks_child_agent_export_dir_for_execution_invalidation() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills.clone(),
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let expected_agents_dir = std::fs::canonicalize(repo_skills.join("test-skill"))
            .unwrap()
            .join("agents");

        assert!(registry.tracked_paths().contains(&expected_agents_dir));
    }

    #[test]
    fn load_capability_view_does_not_track_synthetic_builtin_skill_sidecars() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(&capability_view, &[]).unwrap();

        assert!(registry.has(&"memory".to_string()));
        assert!(!registry.tracked_paths().iter().any(|path| {
            path.to_string_lossy().contains("builtin-skill-packages")
                && (path.ends_with(std::path::Path::new(SKILL_SIDECAR_FILE))
                    || path.ends_with(std::path::Path::new(PACKAGE_SIDECAR_FILE))
                    || path.ends_with(std::path::Path::new(COMPATIBILITY_METADATA_FILE)))
        }));
    }

    #[test]
    fn load_capability_view_loads_builtin_skill_creator_compatibility_metadata() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(&capability_view, &[]).unwrap();
        let skill = registry.get(&"skill-creator".to_string()).unwrap();

        assert_eq!(
            skill.package_id.as_deref(),
            Some("builtin:alan-skill-creator")
        );
        assert!(skill.enabled);
        assert!(skill.allow_implicit_invocation);
        assert_eq!(skill.display_name(), "Skill Creator");
        assert_eq!(
            skill.effective_short_description(),
            Some("Create or update alan skill packages")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .default_prompt
                .as_deref(),
            Some(
                "Use this package when the task is to create, update, validate, or iterate on a skill package."
            )
        );
        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
        assert!(
            skill
                .resource_root
                .as_deref()
                .is_some_and(|path| path.join("references/authoring.md").is_file())
        );
    }

    #[test]
    fn load_capability_view_loads_builtin_repo_coding_compatibility_metadata() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(&capability_view, &[]).unwrap();
        let skill = registry.get(&"repo-coding".to_string()).unwrap();

        assert_eq!(
            skill.package_id.as_deref(),
            Some("builtin:alan-repo-coding")
        );
        assert!(skill.enabled);
        assert!(skill.allow_implicit_invocation);
        assert_eq!(skill.display_name(), "Repo Coding");
        assert_eq!(
            skill.effective_short_description(),
            Some("Launch a repo-scoped coding worker")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .short_description
                .as_deref(),
            Some("Delegate bounded repo-scoped coding work")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .default_prompt
                .as_deref(),
            Some(
                "Use this package when alan should hand off focused coding work to a repo-scoped child worker with a clear verification and delivery contract."
            )
        );
        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "repo-worker".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
        assert!(
            skill
                .resource_root
                .as_deref()
                .is_some_and(|path| path.join("references/delivery_contract.md").is_file())
        );
    }

    #[test]
    fn load_capability_view_loads_builtin_workspace_inspect_as_delegated_skill() {
        let capability_view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let registry = SkillsRegistry::load_capability_view(&capability_view, &[]).unwrap();
        let skill = registry.get(&"workspace-inspect".to_string()).unwrap();

        assert_eq!(
            skill.package_id.as_deref(),
            Some("builtin:alan-workspace-inspect")
        );
        assert!(skill.enabled);
        assert!(skill.allow_implicit_invocation);
        assert_eq!(skill.display_name(), "Workspace Inspect");
        assert_eq!(
            skill.effective_short_description(),
            Some("Launch a read-only workspace reader")
        );
        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "workspace-reader".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
    }

    #[test]
    fn load_capability_view_invalid_sidecar_is_non_fatal() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::write(
            repo_skills.join("test-skill").join(SKILL_SIDECAR_FILE),
            "runtime: [",
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let skill = registry.get(&"test-skill".to_string()).unwrap();

        assert_eq!(skill.description, "From repo");
        assert!(skill.alan_metadata.permission_hints.is_empty());
        assert!(registry.errors().iter().any(|error| {
            error
                .path
                .ends_with(std::path::Path::new(SKILL_SIDECAR_FILE))
        }));
    }

    #[test]
    fn load_capability_view_invalid_openai_compatibility_metadata_is_non_fatal() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        let skill_root = repo_skills.join("test-skill");
        create_test_skill(&repo_skills, "test-skill", "Test Skill", "From repo");
        std::fs::create_dir_all(skill_root.join(COMPATIBILITY_METADATA_DIR)).unwrap();
        std::fs::write(
            skill_root
                .join(COMPATIBILITY_METADATA_DIR)
                .join(COMPATIBILITY_METADATA_FILE),
            "interface: [",
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Repo,
        }])
        .unwrap();
        let skill = registry.get(&"test-skill".to_string()).unwrap();

        assert_eq!(skill.description, "From repo");
        assert!(skill.compatible_metadata.is_empty());
        assert!(registry.errors().iter().any(|error| {
            error
                .path
                .ends_with(std::path::Path::new(COMPATIBILITY_METADATA_FILE))
        }));
    }

    #[test]
    fn load_capability_view_defaults_skill_without_child_agents_to_inline_execution() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("inline-package");
        let skill_path = create_skill_file(
            &package_root.join("skills"),
            "repo-review",
            "Repo Review",
            "Review a repo",
        );
        let capability_view = capability_view_for_manual_package(
            "pkg:inline-package",
            &package_root,
            &skill_path,
            &[],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"repo-review".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Inline {
                source: SkillExecutionResolutionSource::NoChildAgentExports,
            }
        );
    }

    #[test]
    fn load_capability_view_infers_same_name_delegated_skill_target() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("delegated-package");
        let skill_path = create_skill_file(
            &package_root.join("skills"),
            "repo-review",
            "Repo Review",
            "Review a repo",
        );
        let capability_view = capability_view_for_manual_package(
            "pkg:delegated-package",
            &package_root,
            &skill_path,
            &["repo-review"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"repo-review".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "repo-review".to_string(),
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            }
        );
    }

    #[test]
    fn load_capability_view_infers_same_name_delegate_from_normalized_export_name() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("delegated-package");
        let skill_path = create_skill_file(
            &package_root.join("skills"),
            "repo.review",
            "Repo Review",
            "Review a repo",
        );
        let capability_view = capability_view_for_manual_package(
            "pkg:delegated-package",
            &package_root,
            &skill_path,
            &["repo_review", "grader"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"repo-review".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "repo_review".to_string(),
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            }
        );
    }

    #[test]
    fn load_capability_view_marks_normalized_same_name_collisions_unresolved() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("delegated-package");
        let skill_path = create_skill_file(
            &package_root.join("skills"),
            "repo-review",
            "Repo Review",
            "Review a repo",
        );
        let capability_view = capability_view_for_manual_package(
            "pkg:delegated-package",
            &package_root,
            &skill_path,
            &["repo-review", "repo_review"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"repo-review".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: "repo-review".to_string(),
                    child_agent_exports: vec!["repo-review".to_string(), "repo_review".to_string()],
                },
            }
        );
    }

    #[test]
    fn load_capability_view_infers_single_skill_single_child_agent_delegate() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("single-skill-single-agent");
        let skill_path = create_skill_file(
            &package_root.join("skills"),
            "lint-summary",
            "Lint Summary",
            "Summarize lint output",
        );
        let capability_view = capability_view_for_manual_package(
            "pkg:single-skill-single-agent",
            &package_root,
            &skill_path,
            &["reviewer"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"lint-summary".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::SingleSkillSingleChildAgent,
            }
        );
    }

    #[test]
    fn load_capability_view_marks_ambiguous_package_shapes_unresolved() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("ambiguous-package");
        let foo = create_skill_file(&package_root.join("skills"), "foo", "Foo", "First");
        let capability_view = capability_view_for_manual_package(
            "pkg:ambiguous-package",
            &package_root,
            &foo,
            &["reviewer", "grader"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let foo_skill = registry.get(&"foo".to_string()).unwrap();

        assert_eq!(
            foo_skill.execution,
            ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: "foo".to_string(),
                    child_agent_exports: vec!["grader".to_string(), "reviewer".to_string()],
                },
            }
        );
    }

    #[test]
    fn load_capability_view_explicit_delegate_target_overrides_default_inference() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("explicit-delegate-package");
        let skill_dir = package_root.join("skills");
        let skill_path = create_skill_file(
            &skill_dir,
            "skill-creator",
            "Skill Creator",
            "Create a skill",
        );
        std::fs::write(
            skill_dir.join("skill-creator").join(SKILL_SIDECAR_FILE),
            r#"
runtime:
  execution:
    mode: delegate
    target: creator
"#,
        )
        .unwrap();
        let capability_view = capability_view_for_manual_package(
            "pkg:explicit-delegate-package",
            &package_root,
            &skill_path,
            &["creator", "grader", "analyzer"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"skill-creator".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "creator".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
    }

    #[test]
    fn load_capability_view_invalid_explicit_delegate_target_is_unresolved() {
        let temp = TempDir::new().unwrap();
        let package_root = temp.path().join("invalid-explicit-target-package");
        let skill_dir = package_root.join("skills");
        let skill_path = create_skill_file(
            &skill_dir,
            "skill-creator",
            "Skill Creator",
            "Create a skill",
        );
        std::fs::write(
            skill_dir.join("skill-creator").join(SKILL_SIDECAR_FILE),
            r#"
runtime:
  execution:
    mode: delegate
    target: missing-target
"#,
        )
        .unwrap();
        let capability_view = capability_view_for_manual_package(
            "pkg:invalid-explicit-target-package",
            &package_root,
            &skill_path,
            &["creator", "grader"],
        );

        let mut registry = SkillsRegistry::default();
        registry.apply_capability_view(capability_view, &[]);
        let skill = registry.get(&"skill-creator".to_string()).unwrap();

        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::DelegateTargetNotFound {
                    target: "missing-target".to_string(),
                    available_targets: vec!["creator".to_string(), "grader".to_string()],
                },
            }
        );
    }
}
