//! Core types for the skills framework.

use semver::{BuildMetadata, Version};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Skill unique identifier (lowercase, hyphenated).
pub type SkillId = String;

/// Capability package unique identifier.
pub type CapabilityPackageId = String;

/// Optional skill sidecar filename.
pub const SKILL_SIDECAR_FILE: &str = "skill.yaml";
/// Optional package sidecar filename.
pub const PACKAGE_SIDECAR_FILE: &str = "package.yaml";
/// Compatibility metadata directory used by public Codex-style skills.
pub const COMPATIBILITY_METADATA_DIR: &str = "agents";
/// Compatibility metadata filename used by public Codex-style skills.
pub const COMPATIBILITY_METADATA_FILE: &str = "openai.yaml";

/// Skill scope determines precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillScope {
    /// Repository-level skills (highest priority).
    #[serde(rename = "repo")]
    Repo,
    /// User-level skills.
    #[serde(rename = "user")]
    User,
    /// Built-in first-party packages (lowest priority).
    #[serde(rename = "builtin", alias = "system")]
    Builtin,
}

impl SkillScope {
    /// Priority order: lower number = higher priority.
    pub fn priority(&self) -> u8 {
        match self {
            SkillScope::Repo => 0,
            SkillScope::User => 1,
            SkillScope::Builtin => 2,
        }
    }
}

/// Filesystem package discovery directory with its effective overlay scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedPackageDir {
    pub path: PathBuf,
    pub scope: SkillScope,
}

/// Skill content source.
#[derive(Debug, Clone)]
pub enum SkillContentSource {
    File(PathBuf),
    Embedded(&'static str),
}

impl Default for SkillContentSource {
    fn default() -> Self {
        Self::File(PathBuf::new())
    }
}

/// Portable skill exported by a capability package.
///
/// Current stable filesystem discovery produces exactly one portable skill per
/// package root (`SKILL.md`). The vector-based package container remains an
/// internal representation.
#[derive(Debug, Clone)]
pub struct PortableSkill {
    pub path: PathBuf,
    pub source: SkillContentSource,
}

/// Package-level resource directories exported by a capability package.
#[derive(Debug, Clone, Default)]
pub struct CapabilityPackageResources {
    pub bin_dir: Option<PathBuf>,
    pub scripts_dir: Option<PathBuf>,
    pub references_dir: Option<PathBuf>,
    pub assets_dir: Option<PathBuf>,
}

impl CapabilityPackageResources {
    pub fn is_empty(&self) -> bool {
        self.bin_dir.is_none()
            && self.scripts_dir.is_none()
            && self.references_dir.is_none()
            && self.assets_dir.is_none()
    }
}

/// Additional exports a capability package can expose beyond portable skills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityChildAgentExport {
    pub name: String,
    pub root_dir: PathBuf,
    pub handle: alan_agent_protocol::SpawnTarget,
}

impl CapabilityChildAgentExport {
    pub fn package_handle(package_id: &str, name: &str) -> alan_agent_protocol::SpawnTarget {
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: package_id.to_string(),
            export_name: name.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CapabilityPackageExports {
    pub child_agents: Vec<CapabilityChildAgentExport>,
    pub resources: CapabilityPackageResources,
}

impl CapabilityPackageExports {
    pub fn is_empty(&self) -> bool {
        self.child_agents.is_empty() && self.resources.is_empty()
    }

    pub fn child_agent_export_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .child_agents
            .iter()
            .map(|export| export.name.clone())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn child_agent_export(&self, name: &str) -> Option<&CapabilityChildAgentExport> {
        self.child_agents.iter().find(|export| export.name == name)
    }
}

/// Capability package available to an agent definition.
///
/// Stable directory-backed packages currently expose one portable skill plus
/// optional alan-native resources and package-local launch targets.
#[derive(Debug, Clone)]
pub struct CapabilityPackage {
    pub id: CapabilityPackageId,
    pub scope: SkillScope,
    pub root_dir: Option<PathBuf>,
    pub exports: CapabilityPackageExports,
    pub portable_skill: PortableSkill,
}

/// Runtime-facing resolved capability view assembled from package sources.
#[derive(Debug, Clone, Default)]
pub struct ResolvedCapabilityView {
    pub package_dirs: Vec<ScopedPackageDir>,
    pub packages: Vec<CapabilityPackage>,
    pub errors: Vec<SkillError>,
    pub tracked_paths: Vec<PathBuf>,
}

/// Per-skill runtime exposure override merged across resolved agent roots.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillOverride {
    #[serde(rename = "skill", deserialize_with = "deserialize_canonical_skill_id")]
    pub skill_id: SkillId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_implicit_invocation: Option<bool>,
}

impl SkillOverride {
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none() && self.allow_implicit_invocation.is_none()
    }

    pub fn apply_overlay(&mut self, overlay: &Self) {
        if let Some(enabled) = overlay.enabled {
            self.enabled = Some(enabled);
        }
        if let Some(allow_implicit_invocation) = overlay.allow_implicit_invocation {
            self.allow_implicit_invocation = Some(allow_implicit_invocation);
        }
    }
}

fn deserialize_canonical_skill_id<'de, D>(deserializer: D) -> Result<SkillId, D::Error>
where
    D: Deserializer<'de>,
{
    let skill_id = SkillId::deserialize(deserializer)?;
    validate_canonical_skill_id(&skill_id).map_err(serde::de::Error::custom)?;
    Ok(skill_id)
}

pub fn merge_skill_overrides(
    base_overrides: &[SkillOverride],
    overlays: &[SkillOverride],
) -> Vec<SkillOverride> {
    let mut merged: Vec<SkillOverride> = base_overrides.to_vec();

    for overlay in overlays {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| existing.skill_id == overlay.skill_id)
        {
            existing.apply_overlay(overlay);
        } else {
            merged.push(overlay.clone());
        }
    }

    merged
}

/// Skill metadata loaded at startup (lightweight).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub id: SkillId,
    pub package_id: Option<CapabilityPackageId>,
    pub name: String,
    pub description: String,
    pub short_description: Option<String>,
    /// Canonical path to the skill's `SKILL.md`.
    pub path: PathBuf,
    /// Canonical package root that exported this skill, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_root: Option<PathBuf>,
    /// Canonical resource root for resolving relative skill references.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_root: Option<PathBuf>,
    pub scope: SkillScope,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Skill capabilities (optional)
    #[serde(skip)]
    pub capabilities: Option<SkillCapabilities>,
    /// Compatibility requirements declared by the skill.
    #[serde(skip, default)]
    pub compatibility: SkillCompatibility,
    /// Skill content location.
    #[serde(skip, default)]
    pub source: SkillContentSource,
    /// Whether the skill is enabled for the current runtime.
    #[serde(default = "default_skill_enabled")]
    pub enabled: bool,
    /// Whether the skill may appear in the prompt catalog for implicit use.
    #[serde(default = "default_allow_implicit_invocation")]
    pub allow_implicit_invocation: bool,
    /// alan-native runtime/UI metadata loaded from optional sidecars.
    #[serde(skip, default)]
    pub alan_metadata: AlanSkillRuntimeMetadata,
    /// Public compatibility metadata loaded from tolerated sidecars such as
    /// `agents/openai.yaml`.
    #[serde(skip, default)]
    pub compatible_metadata: CompatibleSkillMetadata,
    /// Resolved skill execution state for the current capability package shape.
    #[serde(default)]
    pub execution: ResolvedSkillExecution,
}

impl SkillMetadata {
    pub fn is_builtin_package(&self) -> bool {
        self.package_id
            .as_deref()
            .is_some_and(|package_id| package_id.starts_with("builtin:"))
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn allows_implicit_invocation(&self) -> bool {
        self.allow_implicit_invocation
    }

    pub fn package_root(&self) -> Option<&Path> {
        self.package_root.as_deref()
    }

    pub fn resource_root(&self) -> Option<&Path> {
        self.resource_root
            .as_deref()
            .or_else(|| self.package_root())
    }

    pub fn display_name(&self) -> &str {
        self.compatible_metadata
            .interface
            .display_name
            .as_deref()
            .unwrap_or(&self.name)
    }

    pub fn effective_short_description(&self) -> Option<&str> {
        self.short_description.as_deref().or(self
            .compatible_metadata
            .interface
            .short_description
            .as_deref())
    }

    pub fn delegated_spawn_target(&self) -> Option<alan_agent_protocol::SpawnTarget> {
        let package_id = self.package_id.as_ref()?;
        let target = self.execution.delegate_target()?;
        Some(CapabilityChildAgentExport::package_handle(
            package_id, target,
        ))
    }

    pub fn apply_sidecar_metadata(
        &mut self,
        package_defaults: Option<&AlanSkillSidecar>,
        skill_sidecar: Option<&AlanSkillSidecar>,
    ) -> Result<(), SkillsError> {
        let mut merged = self.clone();
        if let Some(defaults) = package_defaults {
            merged.apply_skill_sidecar(defaults);
        }
        if let Some(sidecar) = skill_sidecar {
            merged.apply_skill_sidecar(sidecar);
        }
        *self = merged;
        Ok(())
    }

    fn apply_skill_sidecar(&mut self, sidecar: &AlanSkillSidecar) {
        if !sidecar.runtime.is_empty() {
            self.alan_metadata.apply_overlay(&sidecar.runtime);
        }
    }
}

fn default_skill_enabled() -> bool {
    true
}

fn default_allow_implicit_invocation() -> bool {
    true
}

/// Full skill content loaded on demand.
pub struct Skill {
    pub metadata: SkillMetadata,
    /// SKILL.md body content (without frontmatter).
    pub content: String,
    /// Parsed frontmatter.
    pub frontmatter: SkillFrontmatter,
}

/// YAML frontmatter in SKILL.md.
#[derive(Debug, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub metadata: FrontmatterMetadata,
    /// Skill capabilities
    #[serde(default)]
    pub capabilities: SkillCapabilities,
    /// Compatibility requirements
    #[serde(default)]
    pub compatibility: SkillCompatibility,
}

/// Optional metadata in frontmatter.
#[derive(Debug, Default, Deserialize)]
pub struct FrontmatterMetadata {
    #[serde(rename = "short-description")]
    pub short_description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Skill capabilities declaration (from SKILL.md frontmatter)
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SkillCapabilities {
    /// Required tools - must be available for skill to function
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// Progressive disclosure configuration (Level 3 resources)
    #[serde(default)]
    pub disclosure: DisclosureConfig,
}

/// Progressive disclosure configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DisclosureConfig {
    /// Level 2 content file (default: SKILL.md)
    #[serde(default = "default_level2")]
    pub level2: String,
    /// Level 3 resources (loaded on demand)
    #[serde(default)]
    pub level3: Level3Resources,
}

fn default_level2() -> String {
    "SKILL.md".to_string()
}

/// Level 3 resources configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Level3Resources {
    /// Reference documents (markdown, etc.)
    #[serde(default)]
    pub references: Vec<String>,
    /// Executable scripts
    #[serde(default)]
    pub scripts: Vec<String>,
    /// Template and resource files
    #[serde(default)]
    pub assets: Vec<String>,
}

/// Skill compatibility declaration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SkillCompatibility {
    /// Minimum version required
    #[serde(default)]
    pub min_version: Option<String>,
    /// Typed dependency requirements.
    #[serde(default)]
    pub dependencies: Vec<SkillTypedDependency>,
    /// Environment requirements description
    #[serde(default)]
    pub requirements: Option<String>,
}

impl SkillCompatibility {
    pub fn apply_overlay(&mut self, overlay: &SkillCompatibilityOverlay) {
        if let Some(min_version) = overlay.min_version.as_ref() {
            self.min_version = Some(min_version.clone());
        }
        if let Some(dependencies) = overlay.dependencies.as_ref() {
            self.dependencies = dependencies.clone();
        }
        if let Some(requirements) = overlay.requirements.as_ref() {
            self.requirements = Some(requirements.clone());
        }
    }
}

/// Partial compatibility overlay loaded from optional alan sidecars.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SkillCompatibilityOverlay {
    #[serde(default)]
    pub min_version: Option<String>,
    #[serde(default)]
    pub dependencies: Option<Vec<SkillTypedDependency>>,
    #[serde(default)]
    pub requirements: Option<String>,
}

impl SkillCompatibilityOverlay {
    pub fn is_empty(&self) -> bool {
        self.min_version.is_none() && self.dependencies.is_none() && self.requirements.is_none()
    }
}

/// Typed dependency declaration for skill availability and remediation.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillTypedDependency {
    EnvVar {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    Tool {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    RuntimeCapability {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl SkillTypedDependency {
    pub fn identity_key(&self) -> String {
        match self {
            Self::EnvVar { name, .. } => format!("env_var:{name}"),
            Self::Tool { name, .. } => format!("tool:{name}"),
            Self::RuntimeCapability { name, .. } => format!("runtime_capability:{name}"),
        }
    }
}

/// Public compatibility metadata loaded from tolerated sidecars such as
/// Codex-style `agents/openai.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibleSkillMetadata {
    #[serde(default)]
    pub interface: CompatibleSkillInterface,
    #[serde(default)]
    pub dependencies: CompatibleSkillDependencies,
    #[serde(default)]
    pub policy: CompatibleSkillPolicy,
}

impl CompatibleSkillMetadata {
    pub fn is_empty(&self) -> bool {
        self.interface.is_empty() && self.dependencies.is_empty() && self.policy.is_empty()
    }
}

/// UI-facing compatibility metadata for catalog surfaces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibleSkillInterface {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub short_description: Option<String>,
    #[serde(default)]
    pub icon_small: Option<PathBuf>,
    #[serde(default)]
    pub icon_large: Option<PathBuf>,
    #[serde(default)]
    pub brand_color: Option<String>,
    #[serde(default)]
    pub default_prompt: Option<String>,
}

impl CompatibleSkillInterface {
    pub fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.short_description.is_none()
            && self.icon_small.is_none()
            && self.icon_large.is_none()
            && self.brand_color.is_none()
            && self.default_prompt.is_none()
    }
}

/// Public compatibility dependency metadata parsed for later typed dependency
/// ingestion and remediation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibleSkillDependencies {
    #[serde(default)]
    pub tools: Vec<CompatibleSkillToolDependency>,
}

impl CompatibleSkillDependencies {
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibleSkillPolicy {
    #[serde(default)]
    pub allow_implicit_invocation: Option<bool>,
}

impl CompatibleSkillPolicy {
    pub fn is_empty(&self) -> bool {
        self.allow_implicit_invocation.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CompatibleSkillToolDependency {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Author-declared execution mode from alan sidecar metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlanSkillExecutionMode {
    Inline,
    Delegate,
}

/// alan-native execution metadata for a skill.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlanSkillExecutionMetadata {
    #[serde(default)]
    pub mode: Option<AlanSkillExecutionMode>,
    #[serde(default)]
    pub target: Option<String>,
}

impl AlanSkillExecutionMetadata {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.target.is_none()
    }

    pub fn apply_overlay(&mut self, overlay: &Self) {
        if let Some(mode) = overlay.mode {
            self.mode = Some(mode);
        }
        if let Some(target) = overlay.target.as_ref() {
            self.target = Some(target.clone());
        }
    }
}

/// alan-native runtime metadata for a skill.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlanSkillRuntimeMetadata {
    #[serde(default)]
    pub permission_hints: Vec<String>,
    #[serde(default)]
    pub execution: AlanSkillExecutionMetadata,
    #[serde(default)]
    pub allow_implicit_invocation: Option<bool>,
}

impl AlanSkillRuntimeMetadata {
    pub fn is_empty(&self) -> bool {
        self.permission_hints.is_empty()
            && self.execution.is_empty()
            && self.allow_implicit_invocation.is_none()
    }

    pub fn apply_overlay(&mut self, overlay: &Self) {
        for hint in &overlay.permission_hints {
            if !self.permission_hints.contains(hint) {
                self.permission_hints.push(hint.clone());
            }
        }
        self.execution.apply_overlay(&overlay.execution);
        if let Some(allow_implicit_invocation) = overlay.allow_implicit_invocation {
            self.allow_implicit_invocation = Some(allow_implicit_invocation);
        }
    }
}

/// Optional alan-native skill sidecar content.
///
/// Stable sidecar behavior is intentionally narrow: only runtime metadata is
/// consumed from `skill.yaml` / `package.yaml`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlanSkillSidecar {
    #[serde(default)]
    pub runtime: AlanSkillRuntimeMetadata,
}

/// Optional alan-native package sidecar content.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlanPackageSidecar {
    #[serde(default)]
    pub skill_defaults: AlanSkillSidecar,
}

/// Host/runtime capability context used to decide if a skill is runnable now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillHostCapabilities {
    pub alan_version: String,
    pub tools: BTreeSet<String>,
    pub executables: BTreeSet<String>,
    pub env_vars: BTreeSet<String>,
    pub delegated_skill_invocation_supported: bool,
}

impl Default for SkillHostCapabilities {
    fn default() -> Self {
        Self {
            alan_version: env!("CARGO_PKG_VERSION").to_string(),
            tools: BTreeSet::new(),
            executables: BTreeSet::new(),
            env_vars: BTreeSet::new(),
            delegated_skill_invocation_supported: false,
        }
    }
}

impl SkillHostCapabilities {
    pub fn with_tools<I, S>(tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            tools: tools.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    pub fn extend_tools<I, S>(&mut self, tools: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tools.extend(tools.into_iter().map(Into::into));
    }

    pub fn with_executables<I, S>(mut self, executables: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extend_executables(executables);
        self
    }

    pub fn extend_executables<I, S>(&mut self, executables: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.executables.extend(
            executables
                .into_iter()
                .map(Into::into)
                .map(|name: String| normalize_executable_name_for_host(&name)),
        );
    }

    pub fn with_path_executables<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        self.extend_executables_from_path_dirs(paths);
        self
    }

    pub fn with_env_vars<I, S>(mut self, env_vars: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extend_env_vars(env_vars);
        self
    }

    pub fn extend_env_vars<I, S>(&mut self, env_vars: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.env_vars.extend(
            env_vars
                .into_iter()
                .map(Into::into)
                .map(|name: String| normalize_env_var_name_for_host(&name)),
        );
    }

    fn extend_env_var_values<I, K, V>(&mut self, env_vars: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env_vars
            .extend(env_vars.into_iter().filter_map(|(name, value)| {
                let value = value.into();
                if value.is_empty() {
                    None
                } else {
                    let name: String = name.into();
                    Some(normalize_env_var_name_for_host(&name))
                }
            }));
    }

    pub fn with_process_env(mut self) -> Self {
        self.extend_env_var_values(std::env::vars_os().map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.to_string_lossy().into_owned(),
            )
        }));
        self
    }

    pub fn with_process_path_executables(mut self) -> Self {
        if let Some(path) = std::env::var_os("PATH") {
            self = self.with_path_executables(std::env::split_paths(&path));
        }
        self
    }

    pub fn supports_delegated_skill_invocation(&self) -> bool {
        self.delegated_skill_invocation_supported
    }

    pub fn supports_required_tool(&self, tool: &str) -> bool {
        match tool {
            "invoke_delegated_skill" => self.supports_delegated_skill_invocation(),
            _ if self.tools.contains(tool) => true,
            _ if is_reserved_runtime_tool_name(tool) => false,
            _ => self
                .executables
                .contains(&normalize_executable_name_for_host(tool)),
        }
    }

    pub fn supports_env_var(&self, name: &str) -> bool {
        self.env_vars
            .contains(&normalize_env_var_name_for_host(name))
    }

    pub fn supports_runtime_capability(&self, name: &str) -> bool {
        match name {
            "delegated_skill_invocation" => self.supports_delegated_skill_invocation(),
            _ => false,
        }
    }

    pub fn with_delegated_skill_invocation(mut self) -> Self {
        self.delegated_skill_invocation_supported = true;
        self.tools.insert("invoke_delegated_skill".to_string());
        self
    }

    pub fn with_runtime_defaults(mut self) -> Self {
        self.extend_tools([
            "request_confirmation",
            "request_mount",
            "request_user_input",
            "update_plan",
        ]);
        self
    }

    fn extend_executables_from_path_dirs<I, P>(&mut self, paths: I)
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        for dir in paths {
            let Ok(entries) = std::fs::read_dir(dir.as_ref()) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = host_executable_name(&path) {
                    self.executables.insert(name);
                }
            }
        }
    }
}

/// Build the canonical skill-availability surface shared by runtime prompt
/// assembly and host catalog inspection.
pub fn build_skill_host_capabilities<I, S>(
    tools: I,
    delegated_skill_invocation_supported: bool,
) -> SkillHostCapabilities
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let path_dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    build_skill_host_capabilities_with_path_dirs(
        tools,
        path_dirs,
        delegated_skill_invocation_supported,
    )
}

pub fn build_skill_host_capabilities_with_path_dirs<I, S, J, P>(
    tools: I,
    path_dirs: J,
    delegated_skill_invocation_supported: bool,
) -> SkillHostCapabilities
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    J: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut capabilities = SkillHostCapabilities::with_tools(tools)
        .with_process_env()
        .with_path_executables(path_dirs)
        .with_runtime_defaults();
    if delegated_skill_invocation_supported {
        capabilities = capabilities.with_delegated_skill_invocation();
    }
    capabilities
}

fn normalize_env_var_name_for_host(name: &str) -> String {
    normalize_env_var_name(name, cfg!(windows))
}

fn normalize_env_var_name(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_ascii_uppercase()
    } else {
        name.to_string()
    }
}

fn host_executable_name(path: &Path) -> Option<String> {
    if !path.is_file() || !is_host_executable(path) {
        return None;
    }

    #[cfg(windows)]
    {
        let allowed_extensions = allowed_windows_executable_extensions();
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_uppercase())?;
        if !allowed_extensions.contains(&extension) {
            return None;
        }

        return path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| normalize_executable_name_for_host(stem));
    }

    #[cfg(not(windows))]
    {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(normalize_executable_name_for_host)
    }
}

fn normalize_executable_name_for_host(name: &str) -> String {
    normalize_executable_name(name, cfg!(windows))
}

fn normalize_executable_name(name: &str, case_insensitive: bool) -> String {
    if case_insensitive {
        name.to_lowercase()
    } else {
        name.to_string()
    }
}

fn is_reserved_runtime_tool_name(tool: &str) -> bool {
    matches!(
        tool,
        "read_file"
            | "write_file"
            | "edit_file"
            | "bash"
            | "grep"
            | "glob"
            | "list_dir"
            | "request_confirmation"
            | "request_mount"
            | "request_user_input"
            | "update_plan"
            | "invoke_delegated_skill"
    )
}

#[cfg(unix)]
fn is_host_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_host_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(windows)]
fn allowed_windows_executable_extensions() -> BTreeSet<String> {
    let pathext = std::env::var_os("PATHEXT")
        .unwrap_or_else(|| std::ffi::OsString::from(".COM;.EXE;.BAT;.CMD"));
    pathext
        .to_string_lossy()
        .split(';')
        .map(str::trim)
        .map(|extension| extension.trim_start_matches('.'))
        .filter(|extension: &&str| !extension.is_empty())
        .map(|extension| extension.to_ascii_uppercase())
        .collect()
}

/// Reason a skill is not currently runnable in the active host/runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillAvailabilityIssue {
    MissingDependencies(Vec<SkillDependencyIssue>),
    UnresolvedExecution(String),
    MinVersionNotMet { required: String, current: String },
    InvalidMinVersion(String),
}

impl std::fmt::Display for SkillAvailabilityIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillAvailabilityIssue::MissingDependencies(dependencies) => {
                write!(
                    f,
                    "missing dependencies: {}",
                    dependencies
                        .iter()
                        .map(SkillDependencyIssue::render_label)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            SkillAvailabilityIssue::UnresolvedExecution(detail) => {
                write!(f, "unresolved execution: {detail}")
            }
            SkillAvailabilityIssue::MinVersionNotMet { required, current } => {
                write!(f, "requires alan >= {required} (current: {current})")
            }
            SkillAvailabilityIssue::InvalidMinVersion(version) => {
                write!(f, "invalid compatibility.min_version: {version}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SkillDependencyIssue {
    MissingEnvVar {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    MissingTool {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    MissingRuntimeCapability {
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl SkillDependencyIssue {
    pub fn render_label(&self) -> String {
        match self {
            Self::MissingEnvVar { name, .. } => format!("env_var:{name}"),
            Self::MissingTool { name, .. } => format!("tool:{name}"),
            Self::MissingRuntimeCapability { name, .. } => format!("runtime_capability:{name}"),
        }
    }
}

/// Why a delegated or inline execution state resolved the way it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillExecutionResolutionSource {
    ExplicitMetadata,
    NoChildAgentExports,
    SameNameSkillAndChildAgent,
    SingleSkillSingleChildAgent,
}

impl SkillExecutionResolutionSource {
    pub fn render_label(&self) -> &'static str {
        match self {
            Self::ExplicitMetadata => "explicit_metadata",
            Self::NoChildAgentExports => "no_child_agent_exports",
            Self::SameNameSkillAndChildAgent => "same_name_skill_and_child_agent",
            Self::SingleSkillSingleChildAgent => "single_skill_single_child_agent",
        }
    }
}

/// Why delegated execution could not be resolved for a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillExecutionUnresolvedReason {
    NotResolved,
    MissingChildAgentExports,
    DelegateTargetNotFound {
        target: String,
        available_targets: Vec<String>,
    },
    AmbiguousPackageShape {
        skill_id: String,
        child_agent_exports: Vec<String>,
    },
}

impl SkillExecutionUnresolvedReason {
    pub fn render_label(&self) -> String {
        match self {
            Self::NotResolved => "not_resolved".to_string(),
            Self::MissingChildAgentExports => "missing_child_agent_exports".to_string(),
            Self::DelegateTargetNotFound { target, .. } => {
                format!("delegate_target_not_found({target})")
            }
            Self::AmbiguousPackageShape { .. } => "ambiguous_package_shape".to_string(),
        }
    }
}

/// Resolved execution state for a skill after package-local inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedSkillExecution {
    Inline {
        source: SkillExecutionResolutionSource,
    },
    Delegate {
        target: String,
        source: SkillExecutionResolutionSource,
    },
    Unresolved {
        reason: SkillExecutionUnresolvedReason,
    },
}

impl Default for ResolvedSkillExecution {
    fn default() -> Self {
        Self::Unresolved {
            reason: SkillExecutionUnresolvedReason::NotResolved,
        }
    }
}

impl ResolvedSkillExecution {
    pub fn render_label(&self) -> String {
        match self {
            Self::Inline { source } => format!("inline({})", source.render_label()),
            Self::Delegate { target, source } => {
                format!(
                    "delegate(target={target}, source={})",
                    source.render_label()
                )
            }
            Self::Unresolved { reason } => format!("unresolved({})", reason.render_label()),
        }
    }

    pub fn delegate_target(&self) -> Option<&str> {
        match self {
            Self::Delegate { target, .. } => Some(target.as_str()),
            _ => None,
        }
    }

    pub fn renders_inline_body(&self) -> bool {
        matches!(self, Self::Inline { .. })
            || matches!(
                self,
                Self::Unresolved {
                    reason: SkillExecutionUnresolvedReason::NotResolved,
                }
            )
    }
}

/// Status returned to the parent for a delegated skill invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelegatedSkillResultStatus {
    Completed,
    Failed,
}

/// Bounded delegated-skill result returned to the parent runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegatedSkillResult {
    pub status: DelegatedSkillResultStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_run: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_ref: Option<DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_output_ref: Option<DelegatedSkillOutputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncation: Option<DelegatedSkillResultTruncation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Inspectable reference for omitted delegated child output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegatedSkillOutputRef {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollout_path: Option<String>,
    pub field: String,
}

/// Explicit truncation metadata for delegated child handoff fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DelegatedSkillResultTruncation {
    #[serde(default)]
    pub summary: bool,
    #[serde(default)]
    pub output_text: bool,
    #[serde(default)]
    pub structured_output: bool,
    #[serde(default)]
    pub child_run: bool,
    #[serde(default)]
    pub warnings: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_summary_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_output_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_child_run_chars: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_warning_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DelegatedSkillResult {
    pub fn completed(
        summary: impl Into<String>,
        structured_output: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status: DelegatedSkillResultStatus::Completed,
            summary: summary.into(),
            summary_preview: None,
            child_run: None,
            output_text: None,
            output_ref: None,
            structured_output,
            structured_output_ref: None,
            truncation: None,
            warnings: Vec::new(),
            error_kind: None,
            error_message: None,
        }
    }

    pub fn failed(
        summary: impl Into<String>,
        structured_output: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status: DelegatedSkillResultStatus::Failed,
            summary: summary.into(),
            summary_preview: None,
            child_run: None,
            output_text: None,
            output_ref: None,
            structured_output,
            structured_output_ref: None,
            truncation: None,
            warnings: Vec::new(),
            error_kind: None,
            error_message: None,
        }
    }
}

/// Parent-side record of a delegated invocation and its bounded result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegatedSkillInvocationRecord {
    pub skill_id: String,
    pub target: String,
    pub task: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    pub result: DelegatedSkillResult,
}

pub fn resolve_skill_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    match metadata.alan_metadata.execution.mode {
        Some(AlanSkillExecutionMode::Inline) => ResolvedSkillExecution::Inline {
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
        Some(AlanSkillExecutionMode::Delegate) => {
            resolve_explicit_delegate_execution(metadata, child_agent_exports)
        }
        None => infer_skill_execution(metadata, child_agent_exports),
    }
}

fn resolve_explicit_delegate_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    if let Some(target) = metadata.alan_metadata.execution.target.as_ref() {
        if child_agent_exports.iter().any(|name| name == target) {
            return ResolvedSkillExecution::Delegate {
                target: target.clone(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            };
        }

        return ResolvedSkillExecution::Unresolved {
            reason: SkillExecutionUnresolvedReason::DelegateTargetNotFound {
                target: target.clone(),
                available_targets: child_agent_exports.to_vec(),
            },
        };
    }

    if child_agent_exports.is_empty() {
        return ResolvedSkillExecution::Unresolved {
            reason: SkillExecutionUnresolvedReason::MissingChildAgentExports,
        };
    }

    match same_name_child_agent_target(&metadata.id, child_agent_exports) {
        SameNameChildAgentTarget::Matched(target) => {
            return ResolvedSkillExecution::Delegate {
                target,
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            };
        }
        SameNameChildAgentTarget::Ambiguous => {
            return ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: metadata.id.clone(),
                    child_agent_exports: child_agent_exports.to_vec(),
                },
            };
        }
        SameNameChildAgentTarget::NotFound => {}
    }

    if child_agent_exports.len() == 1 {
        return ResolvedSkillExecution::Delegate {
            target: child_agent_exports[0].clone(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        };
    }

    ResolvedSkillExecution::Unresolved {
        reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
            skill_id: metadata.id.clone(),
            child_agent_exports: child_agent_exports.to_vec(),
        },
    }
}

fn infer_skill_execution(
    metadata: &SkillMetadata,
    child_agent_exports: &[String],
) -> ResolvedSkillExecution {
    if child_agent_exports.is_empty() {
        return ResolvedSkillExecution::Inline {
            source: SkillExecutionResolutionSource::NoChildAgentExports,
        };
    }

    match same_name_child_agent_target(&metadata.id, child_agent_exports) {
        SameNameChildAgentTarget::Matched(target) => {
            return ResolvedSkillExecution::Delegate {
                target,
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            };
        }
        SameNameChildAgentTarget::Ambiguous => {
            return ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: metadata.id.clone(),
                    child_agent_exports: child_agent_exports.to_vec(),
                },
            };
        }
        SameNameChildAgentTarget::NotFound => {}
    }

    if child_agent_exports.len() == 1 {
        return ResolvedSkillExecution::Delegate {
            target: child_agent_exports[0].clone(),
            source: SkillExecutionResolutionSource::SingleSkillSingleChildAgent,
        };
    }

    ResolvedSkillExecution::Unresolved {
        reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
            skill_id: metadata.id.clone(),
            child_agent_exports: child_agent_exports.to_vec(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SameNameChildAgentTarget {
    Matched(String),
    Ambiguous,
    NotFound,
}

fn same_name_child_agent_target(
    skill_id: &str,
    child_agent_exports: &[String],
) -> SameNameChildAgentTarget {
    let normalized_skill_id = name_to_id(skill_id);
    let mut matching_target = None;

    for export_name in child_agent_exports {
        if name_to_id(export_name) != normalized_skill_id {
            continue;
        }

        if matching_target.is_some() {
            return SameNameChildAgentTarget::Ambiguous;
        }

        matching_target = Some(export_name.clone());
    }

    matching_target
        .map(SameNameChildAgentTarget::Matched)
        .unwrap_or(SameNameChildAgentTarget::NotFound)
}

/// Runtime-facing availability state for a selected skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SkillAvailabilityState {
    Available,
    Unavailable { issues: Vec<SkillAvailabilityIssue> },
}

impl SkillAvailabilityState {
    pub fn available() -> Self {
        Self::Available
    }

    pub fn from_issues(issues: Vec<SkillAvailabilityIssue>) -> Self {
        if issues.is_empty() {
            Self::Available
        } else {
            Self::Unavailable { issues }
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    pub fn render_label(&self) -> String {
        match self {
            Self::Available => "available".to_string(),
            Self::Unavailable { issues } => {
                format!("unavailable ({})", format_skill_availability_issues(issues))
            }
        }
    }
}

/// Why a skill was activated for the current turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkillActivationReason {
    ExplicitMention { mention: String },
}

impl SkillActivationReason {
    pub fn cache_key_fragment(&self) -> String {
        match self {
            Self::ExplicitMention { mention } => format!("explicit:{mention}"),
        }
    }

    pub fn render_label(&self) -> String {
        match self {
            Self::ExplicitMention { mention } => format!("explicit_mention(${mention})"),
        }
    }
}

/// Structured runtime envelope for each selected active skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSkillEnvelope {
    pub metadata: SkillMetadata,
    pub availability: SkillAvailabilityState,
    pub activation_reason: SkillActivationReason,
}

impl ActiveSkillEnvelope {
    pub fn available(metadata: SkillMetadata, activation_reason: SkillActivationReason) -> Self {
        Self {
            metadata,
            availability: SkillAvailabilityState::available(),
            activation_reason,
        }
    }

    pub fn with_issues(
        metadata: SkillMetadata,
        activation_reason: SkillActivationReason,
        issues: Vec<SkillAvailabilityIssue>,
    ) -> Self {
        Self {
            metadata,
            availability: SkillAvailabilityState::from_issues(issues),
            activation_reason,
        }
    }

    pub fn cache_key(&self) -> String {
        format!(
            "{}::{}",
            self.metadata.id,
            self.activation_reason.cache_key_fragment()
        )
    }
}

/// Skill resources (bin, scripts, references, assets).
#[derive(Debug, Default)]
pub struct SkillResources {
    pub bin: Vec<PathBuf>,
    pub scripts: Vec<PathBuf>,
    pub references: Vec<PathBuf>,
    pub assets: Vec<PathBuf>,
}

/// Skill loading error (non-fatal).
#[derive(Debug, Clone)]
pub struct SkillError {
    pub path: PathBuf,
    pub message: String,
}

/// Skill load outcome with errors.
#[derive(Debug, Clone, Default)]
pub struct SkillLoadOutcome {
    pub skills: Vec<SkillMetadata>,
    pub errors: Vec<SkillError>,
    pub tracked_paths: Vec<PathBuf>,
}

impl SkillLoadOutcome {
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// Skill loading error.
#[derive(Debug, thiserror::Error)]
pub enum SkillsError {
    #[error("IO error: {0}")]
    Io(#[source] std::io::Error),
    #[error("Missing or invalid YAML frontmatter")]
    MissingFrontmatter,
    #[error("Invalid YAML: {0}")]
    InvalidYaml(#[source] serde_yaml::Error),
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
    #[error("Skill not found: {0}")]
    NotFound(SkillId),
    #[error("Invalid capabilities declaration: {0}")]
    InvalidCapabilities(String),
}

impl From<std::io::Error> for SkillsError {
    fn from(e: std::io::Error) -> Self {
        SkillsError::Io(e)
    }
}

impl From<serde_yaml::Error> for SkillsError {
    fn from(e: serde_yaml::Error) -> Self {
        SkillsError::InvalidYaml(e)
    }
}

/// Extract YAML frontmatter from markdown content.
/// Returns (frontmatter_yaml, body) if successful.
pub fn extract_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();

    // Must start with ---
    let first = lines.next()?;
    if first.trim() != "---" {
        return None;
    }

    let mut frontmatter_lines = Vec::new();
    let mut found_end = false;

    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_end = true;
            break;
        }
        frontmatter_lines.push(line);
    }

    if !found_end || frontmatter_lines.is_empty() {
        return None;
    }

    let body = lines.collect::<Vec<_>>().join("\n");
    Some((frontmatter_lines.join("\n"), body))
}

/// Convert a skill/package name to a canonical runtime ID.
pub fn name_to_id(name: &str) -> SkillId {
    let mut id = String::new();
    let mut pending_separator = false;

    for ch in name.trim().chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            if pending_separator && !id.is_empty() {
                id.push('-');
            }
            id.push(lower);
            pending_separator = false;
        } else if !id.is_empty() {
            pending_separator = true;
        }
    }

    id
}

pub fn is_canonical_skill_id(skill_id: &str) -> bool {
    let trimmed = skill_id.trim();
    !trimmed.is_empty() && trimmed == skill_id && name_to_id(trimmed) == trimmed
}

pub fn validate_canonical_skill_id(skill_id: &str) -> Result<(), String> {
    if is_canonical_skill_id(skill_id) {
        return Ok(());
    }

    let trimmed = skill_id.trim();
    if trimmed.is_empty() {
        return Err("skill id must not be empty".to_string());
    }

    let canonical = name_to_id(trimmed);
    if canonical.is_empty() {
        Err(format!(
            "Invalid runtime skill id `{skill_id}`; expected a non-empty lower-case hyphenated runtime skill id"
        ))
    } else {
        Err(format!(
            "Invalid runtime skill id `{skill_id}`; use canonical runtime skill id `{canonical}`"
        ))
    }
}

/// Load skill resources from directory.
pub fn load_skill_resources(skill_dir: &Path) -> SkillResources {
    let mut resources = SkillResources::default();

    // Scan bin/
    let bin_dir = skill_dir.join("bin");
    if bin_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&bin_dir)
    {
        resources.bin = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
    }

    // Scan scripts/
    let scripts_dir = skill_dir.join("scripts");
    if scripts_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&scripts_dir)
    {
        resources.scripts = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
    }

    // Scan references/
    let refs_dir = skill_dir.join("references");
    if refs_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&refs_dir)
    {
        resources.references = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
    }

    // Scan assets/
    let assets_dir = skill_dir.join("assets");
    if assets_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&assets_dir)
    {
        resources.assets = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
    }

    resources
}

/// Read a reference file content.
pub fn read_reference(skill_dir: &Path, name: &str) -> Option<String> {
    let path = skill_dir.join("references").join(name);
    std::fs::read_to_string(path).ok()
}

/// Validates skill metadata fields and returns appropriate error for invalid values.
pub fn validate_skill_metadata(
    name: &str,
    description: &str,
    _short_description: Option<&str>,
) -> Result<(), SkillsError> {
    if name.trim().is_empty() {
        return Err(SkillsError::MissingField("name"));
    }

    if description.trim().is_empty() {
        return Err(SkillsError::MissingField("description"));
    }

    Ok(())
}

/// Validates skill capabilities declaration.
/// Returns Ok(()) if valid, Err otherwise.
pub fn validate_capabilities(cap: &SkillCapabilities) -> Result<(), SkillsError> {
    // Validate tool names (should not contain spaces or special chars)
    for tool in &cap.required_tools {
        validate_tool_name(tool)?;
    }

    Ok(())
}

pub fn validate_skill_compatibility(compatibility: &SkillCompatibility) -> Result<(), SkillsError> {
    for dependency in &compatibility.dependencies {
        validate_skill_dependency(dependency)?;
    }
    Ok(())
}

fn validate_skill_dependency(dependency: &SkillTypedDependency) -> Result<(), SkillsError> {
    match dependency {
        SkillTypedDependency::EnvVar { name, .. } => {
            validate_non_empty_dependency_name("environment variable", name)?;
            if name.contains('=') {
                return Err(SkillsError::InvalidCapabilities(format!(
                    "Invalid environment variable name: {}",
                    name
                )));
            }
        }
        SkillTypedDependency::Tool { name, .. } => validate_tool_name(name)?,
        SkillTypedDependency::RuntimeCapability { name, .. } => {
            validate_non_empty_dependency_name("dependency", name)?;
        }
    }

    Ok(())
}

fn validate_tool_name(name: &str) -> Result<(), SkillsError> {
    validate_non_empty_dependency_name("tool", name)?;
    if name.contains(' ') || name.contains('<') || name.contains('>') {
        return Err(SkillsError::InvalidCapabilities(format!(
            "Invalid tool name: {}",
            name
        )));
    }
    Ok(())
}

fn validate_non_empty_dependency_name(kind: &str, name: &str) -> Result<(), SkillsError> {
    if name.trim().is_empty() || name.chars().any(char::is_whitespace) {
        return Err(SkillsError::InvalidCapabilities(format!(
            "Invalid {kind} name: {}",
            name
        )));
    }
    Ok(())
}

fn collect_skill_dependencies(metadata: &SkillMetadata) -> Vec<SkillTypedDependency> {
    let mut dependencies = Vec::new();
    let mut seen = BTreeSet::new();

    let mut push_dependency = |dependency: SkillTypedDependency| {
        if seen.insert(dependency.identity_key()) {
            dependencies.push(dependency);
        }
    };

    if let Some(capabilities) = metadata.capabilities.as_ref() {
        for tool in &capabilities.required_tools {
            push_dependency(SkillTypedDependency::Tool {
                name: tool.clone(),
                description: None,
            });
        }
    }

    for dependency in &metadata.compatibility.dependencies {
        push_dependency(dependency.clone());
    }

    dependencies
}

pub fn skill_availability_issues(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> Vec<SkillAvailabilityIssue> {
    let mut issues = Vec::new();

    let missing_dependencies: Vec<SkillDependencyIssue> = collect_skill_dependencies(metadata)
        .into_iter()
        .filter_map(|dependency| match dependency {
            SkillTypedDependency::EnvVar { name, description }
                if !host_capabilities.supports_env_var(&name) =>
            {
                Some(SkillDependencyIssue::MissingEnvVar { name, description })
            }
            SkillTypedDependency::Tool { name, description }
                if !host_capabilities.supports_required_tool(&name) =>
            {
                Some(SkillDependencyIssue::MissingTool { name, description })
            }
            SkillTypedDependency::RuntimeCapability { name, description }
                if !host_capabilities.supports_runtime_capability(&name) =>
            {
                Some(SkillDependencyIssue::MissingRuntimeCapability { name, description })
            }
            _ => None,
        })
        .collect();
    if !missing_dependencies.is_empty() {
        issues.push(SkillAvailabilityIssue::MissingDependencies(
            missing_dependencies,
        ));
    }

    if let ResolvedSkillExecution::Unresolved { reason } = &metadata.execution
        && !matches!(reason, SkillExecutionUnresolvedReason::NotResolved)
    {
        issues.push(SkillAvailabilityIssue::UnresolvedExecution(
            metadata.execution.render_label(),
        ));
    }

    if let Some(required) = metadata.compatibility.min_version.as_deref() {
        match (
            parse_semver_version(required),
            parse_semver_version(&host_capabilities.alan_version),
        ) {
            (Some(required_version), Some(current_version)) => {
                if current_version < required_version {
                    issues.push(SkillAvailabilityIssue::MinVersionNotMet {
                        required: required.to_string(),
                        current: host_capabilities.alan_version.clone(),
                    });
                }
            }
            _ => issues.push(SkillAvailabilityIssue::InvalidMinVersion(
                required.to_string(),
            )),
        }
    }

    issues
}

pub fn is_skill_available(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> bool {
    skill_availability_issues(metadata, host_capabilities).is_empty()
}

pub fn format_skill_availability_issues(issues: &[SkillAvailabilityIssue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Structured remediation guidance for an unavailable skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRemediation {
    pub reasons: Vec<String>,
    pub next_steps: Vec<String>,
}

pub fn skill_remediation(
    metadata: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> Option<SkillRemediation> {
    let issues = skill_availability_issues(metadata, host_capabilities);
    skill_remediation_from_issues(metadata, &issues)
}

pub fn skill_remediation_from_issues(
    metadata: &SkillMetadata,
    issues: &[SkillAvailabilityIssue],
) -> Option<SkillRemediation> {
    if issues.is_empty() {
        return None;
    }

    let reasons = issues.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut next_steps = BTreeSet::new();

    for issue in issues {
        match issue {
            SkillAvailabilityIssue::MissingDependencies(dependencies) => {
                for dependency in dependencies {
                    match dependency {
                        SkillDependencyIssue::MissingEnvVar { name, .. } => {
                            next_steps
                                .insert(format!("Set the required environment variable: {name}."));
                        }
                        SkillDependencyIssue::MissingTool { name, .. } => {
                            next_steps
                                .insert(format!("Enable or register the required tool: {name}."));
                        }
                        SkillDependencyIssue::MissingRuntimeCapability { name, .. } => {
                            next_steps.insert(format!(
                                "Run this skill in a runtime that supports the required capability: {name}."
                            ));
                        }
                    }
                }
            }
            SkillAvailabilityIssue::UnresolvedExecution(_) => {
                next_steps.insert(
                    "Fix delegated execution metadata so this skill resolves to inline execution or a valid package-local delegate target.".to_string(),
                );
                next_steps.insert(
                    "If the skill should delegate, ensure the target launch entry exists under agents/ and matches any explicit target configuration.".to_string(),
                );
            }
            SkillAvailabilityIssue::MinVersionNotMet { required, .. } => {
                next_steps.insert(format!("Upgrade alan to version {required} or newer."));
            }
            SkillAvailabilityIssue::InvalidMinVersion(version) => {
                next_steps.insert(format!(
                    "Fix compatibility.min_version '{version}' in SKILL.md."
                ));
            }
        }
    }

    if let Some(requirements) = metadata.compatibility.requirements.as_deref()
        && !requirements.trim().is_empty()
    {
        next_steps.insert(format!("Review additional requirements: {requirements}."));
    }

    Some(SkillRemediation {
        reasons,
        next_steps: next_steps.into_iter().collect(),
    })
}

fn parse_semver_version(version: &str) -> Option<Version> {
    let mut version = Version::parse(version).ok()?;
    version.build = BuildMetadata::EMPTY;
    Some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Body content

This is the body.
"#;

        let (frontmatter, body) = extract_frontmatter(content).unwrap();
        assert!(frontmatter.contains("name: test-skill"));
        assert!(body.contains("# Body content"));
    }

    #[test]
    fn test_extract_frontmatter_no_start_marker() {
        // Content without --- at start
        let content = "Just some content without frontmatter";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn test_extract_frontmatter_no_end_marker() {
        // Content with start marker but no end marker
        let content = r#"---
name: test-skill
description: A test skill

# Body content"#;
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn test_name_to_id() {
        assert_eq!(name_to_id("Supplier Evaluation"), "supplier-evaluation");
        assert_eq!(name_to_id("RFQ_Generator"), "rfq-generator");
        assert_eq!(name_to_id("test skill"), "test-skill");
        assert_eq!(name_to_id("Mixed_Case-Name Here"), "mixed-case-name-here");
        assert_eq!(name_to_id("repo.review"), "repo-review");
        assert_eq!(name_to_id("Release__Check v2.0"), "release-check-v2-0");
        assert_eq!(name_to_id("UPPER CASE"), "upper-case");
        assert_eq!(name_to_id("lower case"), "lower-case");
        assert_eq!(name_to_id(""), "");
    }

    #[test]
    fn test_canonical_skill_id_validation() {
        assert!(is_canonical_skill_id("ship-it"));
        assert!(!is_canonical_skill_id("Ship_It"));
        assert!(!is_canonical_skill_id("repo.review"));
        assert_eq!(
            validate_canonical_skill_id("repo.review"),
            Err("Invalid runtime skill id `repo.review`; use canonical runtime skill id `repo-review`".to_string())
        );
        assert_eq!(
            validate_canonical_skill_id("  repo-review  "),
            Err("Invalid runtime skill id `  repo-review  `; use canonical runtime skill id `repo-review`".to_string())
        );
    }

    #[test]
    fn test_skill_scope_priority() {
        assert!(SkillScope::Repo.priority() < SkillScope::User.priority());
        assert!(SkillScope::User.priority() < SkillScope::Builtin.priority());
        assert_eq!(SkillScope::Repo.priority(), 0);
        assert_eq!(SkillScope::User.priority(), 1);
        assert_eq!(SkillScope::Builtin.priority(), 2);
    }

    #[test]
    fn test_skill_scope_serde() {
        // Test serialization/deserialization of SkillScope
        let repo = serde_json::to_string(&SkillScope::Repo).unwrap();
        assert_eq!(repo, "\"repo\"");

        let user: SkillScope = serde_json::from_str("\"user\"").unwrap();
        assert!(matches!(user, SkillScope::User));

        let builtin = serde_json::to_string(&SkillScope::Builtin).unwrap();
        assert_eq!(builtin, "\"builtin\"");

        let legacy_system: SkillScope = serde_json::from_str("\"system\"").unwrap();
        assert!(matches!(legacy_system, SkillScope::Builtin));
    }

    #[test]
    fn test_merge_skill_overrides_applies_latest_overlay_fields() {
        let merged = merge_skill_overrides(
            &[SkillOverride {
                skill_id: "repo-review".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: Some(true),
            }],
            &[
                SkillOverride {
                    skill_id: "repo-review".to_string(),
                    enabled: Some(false),
                    allow_implicit_invocation: None,
                },
                SkillOverride {
                    skill_id: "repo-review".to_string(),
                    enabled: None,
                    allow_implicit_invocation: Some(false),
                },
            ],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0],
            SkillOverride {
                skill_id: "repo-review".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: Some(false),
            }
        );
    }

    #[test]
    fn test_skill_override_deserialization_rejects_legacy_alias_and_noncanonical_ids() {
        let legacy_key = toml::from_str::<SkillOverride>(
            r#"
skill_id = "repo-review"
enabled = true
"#,
        )
        .unwrap_err();
        assert!(legacy_key.to_string().contains("unknown field `skill_id`"));

        let noncanonical = toml::from_str::<SkillOverride>(
            r#"
skill = "repo.review"
enabled = true
"#,
        )
        .unwrap_err();
        assert!(
            noncanonical
                .to_string()
                .contains("canonical runtime skill id `repo-review`")
        );
    }

    #[test]
    fn test_merge_skill_overrides_requires_exact_runtime_skill_ids() {
        let merged = merge_skill_overrides(
            &[SkillOverride {
                skill_id: "repo.review".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: None,
            }],
            &[SkillOverride {
                skill_id: "repo_review".to_string(),
                enabled: None,
                allow_implicit_invocation: Some(false),
            }],
        );

        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_skill_availability_tracks_tools_and_min_version() {
        let metadata = SkillMetadata {
            id: "test-skill".to_string(),
            package_id: Some("skill:test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/test-skill/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                required_tools: vec!["read_file".to_string()],
                ..Default::default()
            }),
            compatibility: SkillCompatibility {
                min_version: Some("0.2.0".to_string()),
                dependencies: Vec::new(),
                requirements: None,
            },
            source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let unavailable = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        );
        assert_eq!(unavailable.len(), 2);
        assert!(!is_skill_available(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults()
        ));

        let available_host =
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
        let issues = skill_availability_issues(
            &SkillMetadata {
                compatibility: SkillCompatibility {
                    min_version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    ..metadata.compatibility.clone()
                },
                ..metadata
            },
            &available_host,
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_skill_availability_accepts_host_executable_dependencies() {
        let metadata = SkillMetadata {
            id: "jq-summary".to_string(),
            package_id: Some("skill:jq-summary".to_string()),
            name: "JQ Summary".to_string(),
            description: "Summarize JSON with jq".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/jq-summary/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                required_tools: vec!["jq".to_string()],
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(PathBuf::from("/tmp/jq-summary/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let missing = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        );
        assert_eq!(
            missing,
            vec![SkillAvailabilityIssue::MissingDependencies(vec![
                SkillDependencyIssue::MissingTool {
                    name: "jq".to_string(),
                    description: None,
                }
            ])]
        );

        let available = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default()
                .with_executables(["jq"])
                .with_runtime_defaults(),
        );
        assert!(available.is_empty());
    }

    #[test]
    fn test_skill_remediation_suggests_next_steps() {
        let metadata = SkillMetadata {
            id: "test-skill".to_string(),
            package_id: Some("skill:test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/test-skill/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                required_tools: vec!["read_file".to_string(), "bash".to_string()],
                ..Default::default()
            }),
            compatibility: SkillCompatibility {
                min_version: Some("9.9.9".to_string()),
                dependencies: Vec::new(),
                requirements: Some("needs local Docker access".to_string()),
            },
            source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let remediation = skill_remediation(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        )
        .unwrap();

        assert!(
            remediation
                .reasons
                .iter()
                .any(|reason| reason.contains("missing dependencies:"))
        );
        assert!(
            remediation
                .next_steps
                .iter()
                .any(|step| step.contains("Enable or register the required tool:"))
        );
        assert!(
            remediation
                .next_steps
                .iter()
                .any(|step| step.contains("Upgrade alan"))
        );
        assert!(
            remediation
                .next_steps
                .iter()
                .any(|step| step.contains("needs local Docker access"))
        );
    }

    #[test]
    fn test_delegated_invocation_is_not_a_runtime_default_tool() {
        let metadata = SkillMetadata {
            id: "repo-review".to_string(),
            package_id: Some("skill:repo-review".to_string()),
            name: "Repo Review".to_string(),
            description: "Delegated repository review".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/repo-review/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                required_tools: vec!["invoke_delegated_skill".to_string()],
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(PathBuf::from("/tmp/repo-review/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let default_runtime = SkillHostCapabilities::default().with_runtime_defaults();
        let issues = skill_availability_issues(&metadata, &default_runtime);
        assert_eq!(
            issues,
            vec![SkillAvailabilityIssue::MissingDependencies(vec![
                SkillDependencyIssue::MissingTool {
                    name: "invoke_delegated_skill".to_string(),
                    description: None,
                }
            ])]
        );

        let delegated_runtime = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        assert!(skill_availability_issues(&metadata, &delegated_runtime).is_empty());
    }

    #[test]
    fn test_unresolved_execution_is_reported_as_unavailable() {
        let metadata = SkillMetadata {
            id: "skill-creator".to_string(),
            package_id: Some("skill:skill-creator".to_string()),
            name: "Skill Creator".to_string(),
            description: "Creates new skills".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/skill-creator/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(PathBuf::from("/tmp/skill-creator/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: "skill-creator".to_string(),
                    child_agent_exports: vec!["creator".to_string(), "grader".to_string()],
                },
            },
        };

        let issues = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        );
        assert_eq!(
            issues,
            vec![SkillAvailabilityIssue::UnresolvedExecution(
                "unresolved(ambiguous_package_shape)".to_string(),
            )]
        );

        let remediation =
            skill_remediation_from_issues(&metadata, &issues).expect("expected remediation");
        assert!(
            remediation
                .next_steps
                .iter()
                .any(|step| step.contains("Fix delegated execution metadata"))
        );
    }

    #[test]
    fn test_typed_env_var_dependencies_drive_availability_and_remediation() {
        let metadata = SkillMetadata {
            id: "openai-docs".to_string(),
            package_id: Some("skill:openai-docs".to_string()),
            name: "OpenAI Docs".to_string(),
            description: "Use official OpenAI docs".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/openai-docs/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: SkillCompatibility {
                min_version: None,
                dependencies: vec![SkillTypedDependency::EnvVar {
                    name: "OPENAI_API_KEY".to_string(),
                    description: Some("Required API key".to_string()),
                }],
                requirements: None,
            },
            source: SkillContentSource::File(PathBuf::from("/tmp/openai-docs/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let missing_issues = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        );
        assert_eq!(
            missing_issues,
            vec![SkillAvailabilityIssue::MissingDependencies(vec![
                SkillDependencyIssue::MissingEnvVar {
                    name: "OPENAI_API_KEY".to_string(),
                    description: Some("Required API key".to_string()),
                }
            ])]
        );

        let remediation =
            skill_remediation_from_issues(&metadata, &missing_issues).expect("remediation");
        assert!(
            remediation
                .next_steps
                .iter()
                .any(|step| step.contains("Set the required environment variable: OPENAI_API_KEY."))
        );

        let available_host = SkillHostCapabilities::default()
            .with_env_vars(["OPENAI_API_KEY"])
            .with_runtime_defaults();
        assert!(skill_availability_issues(&metadata, &available_host).is_empty());
    }

    #[test]
    fn test_process_env_ignores_empty_env_var_values() {
        let mut capabilities = SkillHostCapabilities::default();
        capabilities.extend_env_var_values([
            ("OPENAI_API_KEY".to_string(), "".to_string()),
            ("ANTHROPIC_API_KEY".to_string(), "sk-ant-123".to_string()),
        ]);

        assert!(!capabilities.supports_env_var("OPENAI_API_KEY"));
        assert!(capabilities.supports_env_var("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn test_normalize_env_var_name_supports_case_insensitive_hosts() {
        assert_eq!(
            normalize_env_var_name("openai_api_key", true),
            "OPENAI_API_KEY"
        );
        assert_eq!(
            normalize_env_var_name("OpenAi_Api_Key", true),
            "OPENAI_API_KEY"
        );
        assert_eq!(
            normalize_env_var_name("OpenAi_Api_Key", false),
            "OpenAi_Api_Key"
        );
    }

    #[test]
    fn test_compatibility_metadata_dependency_hints_do_not_gate_runtime_availability() {
        let metadata = SkillMetadata {
            id: "openai-docs".to_string(),
            package_id: Some("skill:openai-docs".to_string()),
            name: "OpenAI Docs".to_string(),
            description: "Use official OpenAI docs".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/openai-docs/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(PathBuf::from("/tmp/openai-docs/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: CompatibleSkillMetadata {
                interface: Default::default(),
                dependencies: CompatibleSkillDependencies {
                    tools: vec![
                        CompatibleSkillToolDependency {
                            kind: Some("env".to_string()),
                            value: Some("OPENAI_API_KEY".to_string()),
                            description: Some("Required API key".to_string()),
                            transport: None,
                            command: None,
                            url: None,
                        },
                        CompatibleSkillToolDependency {
                            kind: Some("mcp".to_string()),
                            value: Some("openaiDeveloperDocs".to_string()),
                            description: Some("OpenAI Developer Docs MCP server".to_string()),
                            transport: Some("streamable_http".to_string()),
                            command: None,
                            url: Some("https://developers.openai.com/mcp".to_string()),
                        },
                    ],
                },
                policy: Default::default(),
            },
            execution: Default::default(),
        };

        let issues = skill_availability_issues(
            &metadata,
            &SkillHostCapabilities::default().with_runtime_defaults(),
        );
        assert!(issues.is_empty());
    }

    #[test]
    fn test_typed_tool_dependencies_reject_blank_names() {
        let compatibility = SkillCompatibility {
            min_version: None,
            dependencies: vec![SkillTypedDependency::Tool {
                name: "".to_string(),
                description: Some("Broken dependency".to_string()),
            }],
            requirements: None,
        };

        let err = validate_skill_compatibility(&compatibility).expect_err("expected invalid tool");
        assert!(
            matches!(err, SkillsError::InvalidCapabilities(message) if message.contains("Invalid tool name"))
        );
    }

    #[test]
    fn test_required_tools_reject_whitespace_only_names() {
        let capabilities = SkillCapabilities {
            required_tools: vec!["\t".to_string()],
            ..Default::default()
        };

        let err = validate_capabilities(&capabilities).expect_err("expected invalid tool");
        assert!(
            matches!(err, SkillsError::InvalidCapabilities(message) if message.contains("Invalid tool name"))
        );
    }

    #[test]
    fn test_skill_host_capabilities_runtime_defaults_include_virtual_tools() {
        let capabilities = SkillHostCapabilities::default().with_runtime_defaults();
        assert!(capabilities.tools.contains("request_confirmation"));
        assert!(capabilities.tools.contains("request_mount"));
        assert!(capabilities.tools.contains("request_user_input"));
        assert!(capabilities.tools.contains("update_plan"));
        assert!(!capabilities.tools.contains("invoke_delegated_skill"));
        assert!(!capabilities.supports_delegated_skill_invocation());

        let delegated_capabilities = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        assert!(
            delegated_capabilities
                .tools
                .contains("invoke_delegated_skill")
        );
        assert!(delegated_capabilities.supports_delegated_skill_invocation());
    }

    #[test]
    fn test_required_tool_support_does_not_treat_dynamic_name_match_as_delegated_runtime_support() {
        let mut capabilities = SkillHostCapabilities::default().with_runtime_defaults();
        capabilities.extend_tools(["invoke_delegated_skill"]);

        assert!(capabilities.tools.contains("invoke_delegated_skill"));
        assert!(!capabilities.supports_delegated_skill_invocation());
        assert!(!capabilities.supports_required_tool("invoke_delegated_skill"));
    }

    #[test]
    fn test_request_mount_required_tool_is_runtime_backed() {
        let capabilities = SkillHostCapabilities::default().with_runtime_defaults();

        assert!(capabilities.supports_required_tool("request_mount"));

        let executable_only = SkillHostCapabilities::default().with_executables(["request_mount"]);
        assert!(!executable_only.supports_required_tool("request_mount"));
    }

    #[test]
    fn test_process_path_executables_collect_host_commands() {
        let temp = tempfile::tempdir().unwrap();
        let executable_path = {
            #[cfg(windows)]
            {
                temp.path().join("demo.cmd")
            }

            #[cfg(not(windows))]
            {
                temp.path().join("demo")
            }
        };
        std::fs::write(&executable_path, "echo demo\n").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&executable_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&executable_path, permissions).unwrap();
        }

        let mut capabilities = SkillHostCapabilities::default();
        capabilities.extend_executables_from_path_dirs([temp.path()]);

        assert!(capabilities.supports_required_tool("demo"));
    }

    #[test]
    fn test_executable_name_normalization_supports_case_insensitive_hosts() {
        assert_eq!(normalize_executable_name("JQ", true), "jq");
        assert_eq!(normalize_executable_name("jq", false), "jq");

        let capabilities = SkillHostCapabilities::default().with_executables(["JQ"]);
        assert!(
            capabilities
                .executables
                .contains(&normalize_executable_name_for_host("JQ"))
        );

        if cfg!(windows) {
            assert!(capabilities.supports_required_tool("jq"));
        } else {
            assert!(!capabilities.supports_required_tool("jq"));
        }
    }

    #[test]
    fn test_required_runtime_tool_is_not_satisfied_by_path_executable() {
        let capabilities = SkillHostCapabilities::default().with_executables(["bash"]);

        assert!(!capabilities.supports_required_tool("bash"));
    }

    #[test]
    fn test_delegated_skill_result_serializes_minimal_bounded_payload() {
        let result = DelegatedSkillResult::completed(
            "Delegated child finished review.",
            Some(serde_json::json!({
                "score": "pass"
            })),
        );

        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["summary"], "Delegated child finished review.");
        assert_eq!(value["structured_output"]["score"], "pass");
    }

    #[test]
    fn test_delegated_skill_invocation_record_captures_task_and_result() {
        let record = DelegatedSkillInvocationRecord {
            skill_id: "repo-review".to_string(),
            target: "repo-review".to_string(),
            task: "Review the current diff.".to_string(),
            workspace_root: Some("/tmp/repo".to_string()),
            cwd: Some("/tmp/repo/src".to_string()),
            timeout_secs: Some(600),
            result: DelegatedSkillResult::failed(
                "Child-agent spawn support is not yet available.",
                Some(serde_json::json!({
                    "error_kind": "runtime_child_launch_unavailable"
                })),
            ),
        };

        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["skill_id"], "repo-review");
        assert_eq!(value["target"], "repo-review");
        assert_eq!(value["workspace_root"], "/tmp/repo");
        assert_eq!(value["cwd"], "/tmp/repo/src");
        assert_eq!(value["timeout_secs"], 600);
        assert_eq!(value["task"], "Review the current diff.");
        assert_eq!(value["result"]["status"], "failed");
        assert_eq!(
            value["result"]["structured_output"]["error_kind"],
            "runtime_child_launch_unavailable"
        );
    }

    #[test]
    fn test_capability_child_agent_export_builds_package_handle() {
        let handle = CapabilityChildAgentExport::package_handle("skill:repo-review", "reviewer");
        assert_eq!(
            handle,
            alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            }
        );
    }

    #[test]
    fn test_skill_metadata_delegated_spawn_target_uses_package_handle() {
        let metadata = SkillMetadata {
            id: "repo-review".to_string(),
            package_id: Some("skill:repo-review".to_string()),
            name: "Repo Review".to_string(),
            description: "Review repository changes".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/repo-review/SKILL.md"),
            package_root: Some(PathBuf::from("/tmp/repo-review")),
            resource_root: None,
            scope: SkillScope::Repo,
            tags: Vec::new(),
            capabilities: None,
            compatibility: SkillCompatibility::default(),
            source: SkillContentSource::File(PathBuf::from("/tmp/repo-review/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: AlanSkillRuntimeMetadata::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            },
        };

        assert_eq!(
            metadata.delegated_spawn_target(),
            Some(alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "reviewer".to_string(),
            })
        );
    }

    #[test]
    fn test_skill_availability_respects_semver_prerelease_ordering() {
        let metadata = SkillMetadata {
            id: "test-skill".to_string(),
            package_id: Some("skill:test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/test-skill/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: SkillCompatibility {
                min_version: Some("1.2.3".to_string()),
                dependencies: Vec::new(),
                requirements: None,
            },
            source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };
        let host_capabilities = SkillHostCapabilities {
            alan_version: "1.2.3-alpha.1".to_string(),
            ..SkillHostCapabilities::default()
        }
        .with_runtime_defaults();

        let issues = skill_availability_issues(&metadata, &host_capabilities);
        assert_eq!(
            issues,
            vec![SkillAvailabilityIssue::MinVersionNotMet {
                required: "1.2.3".to_string(),
                current: "1.2.3-alpha.1".to_string(),
            }]
        );
    }

    #[test]
    fn test_skill_availability_accepts_semver_build_metadata() {
        let metadata = SkillMetadata {
            id: "test-skill".to_string(),
            package_id: Some("skill:test-skill".to_string()),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            short_description: None,
            path: PathBuf::from("/tmp/test-skill/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec![],
            capabilities: None,
            compatibility: SkillCompatibility {
                min_version: Some("1.2.3+build.5".to_string()),
                dependencies: Vec::new(),
                requirements: None,
            },
            source: SkillContentSource::File(PathBuf::from("/tmp/test-skill/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };
        let host_capabilities = SkillHostCapabilities {
            alan_version: "1.2.3".to_string(),
            ..SkillHostCapabilities::default()
        }
        .with_runtime_defaults();

        let issues = skill_availability_issues(&metadata, &host_capabilities);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_load_skill_resources() {
        let temp = std::env::temp_dir().join(format!("skill_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();

        let skill_dir = temp.join("test-skill");

        // Create bin directory with files
        let bin_dir = skill_dir.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("helper"), "#!/usr/bin/env bash").unwrap();

        // Create scripts directory with files
        let scripts_dir = skill_dir.join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("helper.sh"), "#!/bin/bash").unwrap();
        std::fs::write(scripts_dir.join("tool.py"), "#!/usr/bin/env python3").unwrap();

        // Create references directory with files
        let refs_dir = skill_dir.join("references");
        std::fs::create_dir_all(&refs_dir).unwrap();
        std::fs::write(refs_dir.join("guide.md"), "# Guide").unwrap();

        // Create assets directory with files
        let assets_dir = skill_dir.join("assets");
        std::fs::create_dir_all(&assets_dir).unwrap();
        std::fs::write(assets_dir.join("template.txt"), "Template").unwrap();

        let resources = load_skill_resources(&skill_dir);

        assert_eq!(resources.bin.len(), 1);
        assert_eq!(resources.scripts.len(), 2);
        assert_eq!(resources.references.len(), 1);
        assert_eq!(resources.assets.len(), 1);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_read_reference() {
        let temp = std::env::temp_dir().join(format!("ref_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();

        let refs_dir = temp.join("references");
        std::fs::create_dir_all(&refs_dir).unwrap();
        std::fs::write(
            refs_dir.join("guide.md"),
            "# Reference Guide\n\nContent here.",
        )
        .unwrap();

        let content = read_reference(&temp, "guide.md");
        assert_eq!(
            content,
            Some("# Reference Guide\n\nContent here.".to_string())
        );

        // Non-existent reference
        let not_found = read_reference(&temp, "nonexistent.md");
        assert_eq!(not_found, None);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_skills_error_display() {
        let err = SkillsError::MissingField("name");
        assert!(err.to_string().contains("name"));

        let err = SkillsError::MissingFrontmatter;
        assert!(err.to_string().contains("frontmatter"));

        let err = SkillsError::NotFound("test-skill".to_string());
        assert!(err.to_string().contains("test-skill"));

        let err = SkillsError::InvalidCapabilities("bad dependency".to_string());
        assert!(err.to_string().contains("bad dependency"));
    }

    #[test]
    fn test_skill_metadata_serde() {
        // Test serialization/deserialization of SkillMetadata
        let metadata = SkillMetadata {
            id: "test-skill".to_string(),
            package_id: None,
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            short_description: Some("Short".to_string()),
            path: PathBuf::from("/test/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Repo,
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(PathBuf::from("/test/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        };

        let json = serde_json::to_string(&metadata).unwrap();
        assert!(json.contains("test-skill"));
        assert!(json.contains("Test Skill"));

        let deserialized: SkillMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, metadata.id);
        assert_eq!(deserialized.name, metadata.name);
        assert_eq!(deserialized.scope, metadata.scope);
    }
}
