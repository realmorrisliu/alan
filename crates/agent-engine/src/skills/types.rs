//! Core types for the skills framework.

use serde::{Deserialize, Deserializer, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod availability;
mod execution;

pub use availability::{
    SkillAvailabilityIssue, SkillAvailabilityState, SkillDependencyIssue, SkillHostCapabilities,
    SkillRemediation, build_skill_host_capabilities, build_skill_host_capabilities_with_path_dirs,
    format_skill_availability_issues, is_skill_available, skill_availability_issues,
    skill_declared_dependencies, skill_remediation, skill_remediation_from_issues,
    validate_capabilities, validate_skill_compatibility,
};
#[cfg(test)]
use availability::{
    normalize_env_var_name, normalize_executable_name, normalize_executable_name_for_host,
};
pub use execution::{
    DelegatedSkillInvocationRecord, DelegatedSkillOutputDebugMetadata, DelegatedSkillOutputRef,
    DelegatedSkillResult, DelegatedSkillResultStatus, DelegatedSkillResultTruncation,
    ResolvedSkillExecution, SkillExecutionResolutionSource, SkillExecutionUnresolvedReason,
    resolve_skill_execution,
};

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

/// Explicit Skill source determines precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillScope {
    /// Skills passed through an Agent Definition or Skill descriptor (highest priority).
    #[serde(rename = "descriptor")]
    Descriptor,
    /// Skills installed in the Alan OS Package Store.
    #[serde(rename = "installed")]
    Installed,
    /// Built-in first-party packages (lowest priority).
    #[serde(rename = "builtin")]
    Builtin,
}

impl SkillScope {
    /// Priority order: lower number = higher priority.
    pub fn priority(&self) -> u8 {
        match self {
            SkillScope::Descriptor => 0,
            SkillScope::Installed => 1,
            SkillScope::Builtin => 2,
        }
    }
}

/// Explicit package tree with its effective precedence scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedPackageDir {
    pub path: PathBuf,
    pub scope: SkillScope,
}

/// One explicitly referenced Skill package root with its owning package id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedPackageRoot {
    pub package_id: CapabilityPackageId,
    pub path: PathBuf,
    pub namespace_root: Option<PathBuf>,
    pub scope: SkillScope,
    pub dependencies: Vec<SkillTypedDependency>,
}

/// Skill content source.
#[derive(Debug, Clone)]
pub enum SkillContentSource {
    File(PathBuf),
    Embedded(Arc<str>),
    Descriptor {
        content: Arc<str>,
        file_tree: crate::ProcessFileTree,
    },
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
    #[serde(skip, default)]
    pub file_tree: Option<crate::ProcessFileTree>,
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
    pub namespace_root: Option<PathBuf>,
    pub exports: CapabilityPackageExports,
    pub portable_skill: PortableSkill,
    pub dependencies: Vec<SkillTypedDependency>,
    pub package_sidecar: Option<AlanPackageSidecar>,
    pub skill_sidecar: Option<AlanSkillSidecar>,
    pub compatible_metadata: Option<CompatibleSkillMetadata>,
}

/// Runtime-facing resolved capability view assembled from package sources.
#[derive(Debug, Clone, Default)]
pub struct ResolvedCapabilityView {
    pub package_dirs: Vec<ScopedPackageDir>,
    pub package_roots: Vec<ScopedPackageRoot>,
    pub packages: Vec<CapabilityPackage>,
    pub errors: Vec<SkillError>,
    pub descriptor_errors: Vec<SkillError>,
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
    #[error("Duplicate runtime Skill id: {0}")]
    DuplicateSkill(SkillId),
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

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
