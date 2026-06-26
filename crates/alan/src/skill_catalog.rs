use alan_agent_engine::skills::{
    CapabilityPackage, CapabilityPackageResources, CompatibleSkillMetadata, ResolvedSkillExecution,
    SkillHostCapabilities, SkillMetadata, SkillRemediation, SkillsRegistry,
    build_skill_host_capabilities, skill_availability_issues, skill_remediation,
    validate_canonical_skill_id,
};
use alan_agent_engine::{
    AgentRootKind, Config, ResolvedAgentDefinition, ToolRegistry, WorkspaceRuntimeConfig,
};
use alan_tools::{create_core_tools, register_builtin_tool_catalog};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct SkillCatalogContext {
    pub resolved: ResolvedAgentDefinition,
    pub registry: SkillsRegistry,
    pub host_capabilities: SkillHostCapabilities,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillCatalogTarget {
    pub workspace_dir: Option<PathBuf>,
    pub agent_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCatalogSnapshot {
    pub cursor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_alan_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writable_root_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writable_config_path: Option<PathBuf>,
    pub packages: Vec<SkillCatalogPackageSnapshot>,
    pub skills: Vec<SkillCatalogSkillSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCatalogPackageSnapshot {
    pub id: String,
    pub scope: alan_agent_engine::skills::SkillScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_dir: Option<PathBuf>,
    pub exports: SkillCatalogPackageExportsSnapshot,
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCatalogPackageExportsSnapshot {
    pub child_agents: Vec<SkillCatalogChildAgentExportSnapshot>,
    pub resources: SkillCatalogResourceExportsSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCatalogChildAgentExportSnapshot {
    pub name: String,
    pub root_dir: PathBuf,
    pub handle: alan_agent_protocol::SpawnTarget,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillCatalogResourceExportsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCatalogSkillSnapshot {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_description: Option<String>,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_root: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_root: Option<PathBuf>,
    pub scope: alan_agent_engine::skills::SkillScope,
    pub enabled: bool,
    pub allow_implicit_invocation: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "CompatibleSkillMetadata::is_empty")]
    pub compatible_metadata: CompatibleSkillMetadata,
    pub execution: ResolvedSkillExecution,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub availability_issues: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<SkillRemediation>,
}

pub fn resolve_skill_catalog_context(
    runtime_config: &WorkspaceRuntimeConfig,
) -> Result<SkillCatalogContext> {
    let resolved = ResolvedAgentDefinition::from_runtime_config(runtime_config)?;
    let registry =
        SkillsRegistry::load_capability_view(&resolved.capability_view, &resolved.skill_overrides)?;
    let host_capabilities =
        resolve_skill_host_capabilities(&runtime_config.agent_config.core_config, &resolved)?;
    Ok(SkillCatalogContext {
        resolved,
        registry,
        host_capabilities,
    })
}

pub fn resolve_skill_host_capabilities(
    base_config: &Config,
    resolved: &ResolvedAgentDefinition,
) -> Result<SkillHostCapabilities> {
    let mut core_config = base_config.clone();
    if !resolved.config_overlay_paths.is_empty() {
        core_config = core_config.with_agent_root_overlays(&resolved.config_overlay_paths)?;
    }
    let mut tools = ToolRegistry::with_config(Arc::new(core_config));
    if resolved.workspace_root_dir.is_some() {
        register_builtin_tool_catalog(&mut tools);
        for tool in create_core_tools() {
            tools.register_boxed(tool);
        }
    }
    let delegated_supported = !resolved
        .roots
        .roots()
        .iter()
        .any(|root| matches!(root.kind, AgentRootKind::LaunchRoot));
    Ok(build_skill_host_capabilities(
        tools.list_tools().into_iter().map(str::to_string),
        delegated_supported,
    ))
}

pub fn build_skill_catalog_snapshot(context: &SkillCatalogContext) -> Result<SkillCatalogSnapshot> {
    let mut skill_ids_by_package: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut skills = Vec::new();

    for skill in context.registry.list_sorted().into_iter().cloned() {
        if let Some(package_id) = skill.package_id.as_deref() {
            skill_ids_by_package
                .entry(package_id.to_string())
                .or_default()
                .push(skill.id.clone());
        }
        skills.push(build_skill_snapshot(&skill, &context.host_capabilities));
    }

    for skill_ids in skill_ids_by_package.values_mut() {
        skill_ids.sort();
        skill_ids.dedup();
    }

    let mut packages: Vec<&CapabilityPackage> =
        context.resolved.capability_view.packages.iter().collect();
    packages.sort_by(|left, right| {
        left.scope
            .priority()
            .cmp(&right.scope.priority())
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut package_snapshots = Vec::with_capacity(packages.len());
    for package in packages {
        package_snapshots.push(SkillCatalogPackageSnapshot {
            id: package.id.clone(),
            scope: package.scope,
            root_dir: package.root_dir.clone(),
            exports: build_package_exports_snapshot(package),
            skill_ids: skill_ids_by_package
                .get(package.id.as_str())
                .cloned()
                .unwrap_or_default(),
        });
    }

    let mut snapshot = SkillCatalogSnapshot {
        cursor: String::new(),
        workspace_root_dir: context.resolved.workspace_root_dir.clone(),
        workspace_alan_dir: context.resolved.workspace_alan_dir.clone(),
        agent_name: context.resolved.agent_name.clone(),
        writable_root_dir: context.resolved.writable_root_dir.clone(),
        writable_config_path: context.resolved.writable_config_path.clone(),
        packages: package_snapshots,
        skills,
    };
    snapshot.cursor = compute_skill_catalog_cursor(&snapshot)?;
    Ok(snapshot)
}

pub fn write_skill_override(
    config_path: &Path,
    skill_id: &str,
    enabled: Option<Option<bool>>,
    allow_implicit_invocation: Option<Option<bool>>,
) -> Result<()> {
    validate_canonical_skill_id(skill_id).map_err(|message| anyhow::anyhow!(message))?;

    let mut root = load_config_table(config_path)?;
    let (existing_entry, mut skill_overrides) = match root.remove("skill_overrides") {
        Some(toml::Value::Array(entries)) => {
            let existing_entry = entries
                .iter()
                .find(|entry| skill_override_entry_matches(entry, skill_id).unwrap_or(false))
                .and_then(toml::Value::as_table)
                .cloned();
            let filtered = entries
                .into_iter()
                .filter_map(
                    |entry| match skill_override_entry_matches(&entry, skill_id) {
                        Ok(true) => None,
                        Ok(false) => Some(Ok(entry)),
                        Err(err) => Some(Err(err)),
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            (existing_entry, filtered)
        }
        Some(other) => {
            bail!(
                "Invalid skill_overrides in {}: expected array, found {}",
                config_path.display(),
                type_name_for_toml_value(&other)
            );
        }
        None => (None, Vec::new()),
    };

    if let Some(entry) = build_skill_override_entry(
        skill_id,
        existing_entry.as_ref(),
        enabled,
        allow_implicit_invocation,
    ) {
        skill_overrides.push(toml::Value::Table(entry));
    }

    if !skill_overrides.is_empty() {
        root.insert(
            "skill_overrides".to_string(),
            toml::Value::Array(skill_overrides),
        );
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create config directory for skill override: {}",
                parent.display()
            )
        })?;
    }

    if root.is_empty() {
        if config_path.exists() {
            std::fs::remove_file(config_path).with_context(|| {
                format!(
                    "Failed to remove empty agent config after clearing skill override: {}",
                    config_path.display()
                )
            })?;
        }
        return Ok(());
    }

    let rendered = toml::to_string_pretty(&toml::Value::Table(root))
        .context("Failed to encode agent.toml while writing skill override")?;
    write_atomically(config_path, &rendered)?;
    Ok(())
}

fn write_atomically(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("Config path has no parent directory: {}", path.display()))?;
    let tmp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("agent.toml"),
        std::process::id()
    ));
    let mut tmp_file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("Failed to create temp config file: {}", tmp_path.display()))?;
    tmp_file
        .write_all(content.as_bytes())
        .with_context(|| format!("Failed to write temp config file: {}", tmp_path.display()))?;
    if let Ok(metadata) = std::fs::metadata(path) {
        tmp_file
            .set_permissions(metadata.permissions())
            .with_context(|| {
                format!(
                    "Failed to preserve existing permissions on temp config file: {}",
                    tmp_path.display()
                )
            })?;
    }
    tmp_file
        .sync_all()
        .with_context(|| format!("Failed to sync temp config file: {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path).with_context(|| {
        format!(
            "Failed to atomically replace skill override config {} -> {}",
            tmp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn build_skill_snapshot(
    skill: &SkillMetadata,
    host_capabilities: &SkillHostCapabilities,
) -> SkillCatalogSkillSnapshot {
    let issues = skill_availability_issues(skill, host_capabilities);
    let remediation = skill_remediation(skill, host_capabilities);
    SkillCatalogSkillSnapshot {
        id: skill.id.clone(),
        package_id: skill.package_id.clone(),
        name: skill.name.clone(),
        description: skill.description.clone(),
        short_description: skill.short_description.clone(),
        path: skill.path.clone(),
        package_root: skill.package_root.clone(),
        resource_root: skill.resource_root.clone(),
        scope: skill.scope,
        enabled: skill.enabled,
        allow_implicit_invocation: skill.allow_implicit_invocation,
        tags: skill.tags.clone(),
        compatible_metadata: skill.compatible_metadata.clone(),
        execution: skill.execution.clone(),
        available: issues.is_empty(),
        availability_issues: issues.iter().map(ToString::to_string).collect::<Vec<_>>(),
        remediation,
    }
}

fn build_package_exports_snapshot(
    package: &CapabilityPackage,
) -> SkillCatalogPackageExportsSnapshot {
    SkillCatalogPackageExportsSnapshot {
        child_agents: package
            .exports
            .child_agents
            .iter()
            .map(|export| SkillCatalogChildAgentExportSnapshot {
                name: export.name.clone(),
                root_dir: export.root_dir.clone(),
                handle: export.handle.clone(),
            })
            .collect(),
        resources: build_resource_snapshot(&package.exports.resources),
    }
}

fn build_resource_snapshot(
    resources: &CapabilityPackageResources,
) -> SkillCatalogResourceExportsSnapshot {
    SkillCatalogResourceExportsSnapshot {
        bin_dir: resources.bin_dir.clone(),
        scripts_dir: resources.scripts_dir.clone(),
        references_dir: resources.references_dir.clone(),
        assets_dir: resources.assets_dir.clone(),
    }
}

fn compute_skill_catalog_cursor(snapshot: &SkillCatalogSnapshot) -> Result<String> {
    let mut cursor_input = snapshot.clone();
    cursor_input.cursor.clear();
    let encoded = serde_json::to_vec(&cursor_input)
        .context("Failed to serialize skill catalog snapshot for cursor computation")?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn load_config_table(config_path: &Path) -> Result<toml::Table> {
    if !config_path.exists() {
        return Ok(toml::Table::new());
    }

    let raw = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    if raw.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    let value: toml::Value = toml::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;
    match value {
        toml::Value::Table(table) => Ok(table),
        other => bail!(
            "Invalid agent config {}: expected table, found {}",
            config_path.display(),
            type_name_for_toml_value(&other)
        ),
    }
}

fn build_skill_override_entry(
    skill_id: &str,
    existing_entry: Option<&toml::Table>,
    enabled: Option<Option<bool>>,
    allow_implicit_invocation: Option<Option<bool>>,
) -> Option<toml::Table> {
    let mut entry = toml::Table::new();
    entry.insert(
        "skill".to_string(),
        toml::Value::String(skill_id.to_string()),
    );
    let resolved_enabled = match enabled {
        Some(value) => value,
        None => existing_entry
            .and_then(|table| table.get("enabled"))
            .and_then(toml::Value::as_bool),
    };
    if let Some(enabled) = resolved_enabled {
        entry.insert("enabled".to_string(), toml::Value::Boolean(enabled));
    }
    let resolved_allow_implicit_invocation = match allow_implicit_invocation {
        Some(value) => value,
        None => existing_entry
            .and_then(|table| table.get("allow_implicit_invocation"))
            .and_then(toml::Value::as_bool),
    };
    if let Some(allow_implicit_invocation) = resolved_allow_implicit_invocation {
        entry.insert(
            "allow_implicit_invocation".to_string(),
            toml::Value::Boolean(allow_implicit_invocation),
        );
    }

    (entry.len() > 1).then_some(entry)
}

fn skill_override_entry_matches(entry: &toml::Value, skill_id: &str) -> Result<bool> {
    let table = entry.as_table().ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid skill_overrides entry: expected table, found {}",
            type_name_for_toml_value(entry)
        )
    })?;
    let Some(value) = table.get("skill") else {
        if table.contains_key("skill_id") {
            bail!("Invalid skill_overrides entry: use `skill`, not legacy `skill_id`");
        }
        bail!("Invalid skill_overrides entry: missing required `skill` field");
    };
    let value = value.as_str().ok_or_else(|| {
        anyhow::anyhow!("Invalid skill_overrides entry: `skill` must be a string")
    })?;
    validate_canonical_skill_id(value).map_err(|message| anyhow::anyhow!(message))?;
    Ok(value == skill_id)
}

fn type_name_for_toml_value(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "string",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::Boolean(_) => "boolean",
        toml::Value::Datetime(_) => "datetime",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::skills::SkillScope;
    use alan_agent_engine::{AgentRootPaths, AlanHomePaths, Config, ResolvedAgentRoots};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn create_skill(root: &Path, skill_name: &str, frontmatter: &str) {
        let skill_dir = root.join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), frontmatter).unwrap();
    }

    #[test]
    fn build_skill_catalog_snapshot_includes_packages_skills_and_cursor() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let skills_dir = workspace_alan_dir.join("agents/default/skills");
        let review_dir = skills_dir.join("repo-review");
        fs::create_dir_all(review_dir.join("agents/repo-review")).unwrap();
        fs::write(
            review_dir.join("agents/repo-review/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        fs::create_dir_all(review_dir.join("agents")).unwrap();
        create_skill(
            &skills_dir,
            "repo-review",
            r#"---
name: Repo Review
description: Review the current diff
---

Body
"#,
        );
        fs::write(
            review_dir.join("agents/openai.yaml"),
            r#"
interface:
  display_name: "Repository Review"
  short_description: "Review the current diff"
  icon_small: "./assets/review-small.svg"
"#,
        )
        .unwrap();

        let mut runtime_config = WorkspaceRuntimeConfig::from(Config::default());
        runtime_config.workspace_root_dir = Some(workspace_root);
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
        runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp.path()));

        let context = resolve_skill_catalog_context(&runtime_config).unwrap();
        let snapshot = build_skill_catalog_snapshot(&context).unwrap();

        assert!(!snapshot.cursor.is_empty());
        assert!(
            snapshot
                .packages
                .iter()
                .any(|package| package.id == "skill:repo-review")
        );
        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "repo-review")
            .unwrap();
        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "repo-review".to_string(),
                source:
                    alan_agent_engine::skills::SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            }
        );
        assert_eq!(
            skill.compatible_metadata.interface.display_name.as_deref(),
            Some("Repository Review")
        );
        assert_eq!(
            skill
                .compatible_metadata
                .interface
                .short_description
                .as_deref(),
            Some("Review the current diff")
        );
        let expected_icon_small = std::fs::canonicalize(review_dir.join("assets/review-small.svg"))
            .unwrap_or_else(|_| {
                std::fs::canonicalize(&review_dir)
                    .unwrap()
                    .join("assets/review-small.svg")
            });
        assert_eq!(
            skill.compatible_metadata.interface.icon_small.as_deref(),
            Some(expected_icon_small.as_path())
        );
    }

    #[test]
    fn build_skill_catalog_snapshot_includes_builtin_skill_creator_package() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        fs::create_dir_all(&workspace_root).unwrap();

        let mut runtime_config = WorkspaceRuntimeConfig::from(Config::default());
        runtime_config.workspace_root_dir = Some(workspace_root);
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
        runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp.path()));

        let context = resolve_skill_catalog_context(&runtime_config).unwrap();
        let snapshot = build_skill_catalog_snapshot(&context).unwrap();

        let package = snapshot
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-skill-creator")
            .unwrap();
        assert_eq!(
            package.scope,
            alan_agent_engine::skills::SkillScope::Builtin
        );
        assert!(
            package
                .root_dir
                .as_ref()
                .is_some_and(|path| path.join("SKILL.md").is_file())
        );
        assert!(package.exports.resources.scripts_dir.is_some());
        assert!(package.exports.resources.references_dir.is_some());
        assert!(package.exports.resources.assets_dir.is_some());
        assert_eq!(package.exports.child_agents.len(), 1);
        assert_eq!(package.skill_ids, vec!["skill-creator".to_string()]);

        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "skill-creator")
            .unwrap();
        assert_eq!(
            skill.package_id.as_deref(),
            Some("builtin:alan-skill-creator")
        );
        assert_eq!(
            skill.compatible_metadata.interface.display_name.as_deref(),
            Some("Skill Creator")
        );
        assert!(skill.available);
        assert_eq!(
            skill.execution,
            ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: alan_agent_engine::skills::SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
    }

    #[test]
    fn build_skill_catalog_snapshot_includes_builtin_swebench_package() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        fs::create_dir_all(&workspace_root).unwrap();

        let mut runtime_config = WorkspaceRuntimeConfig::from(Config::default());
        runtime_config.workspace_root_dir = Some(workspace_root);
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
        runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp.path()));

        let context = resolve_skill_catalog_context(&runtime_config).unwrap();
        let snapshot = build_skill_catalog_snapshot(&context).unwrap();

        let package = snapshot
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-swebench")
            .unwrap();
        assert_eq!(
            package.scope,
            alan_agent_engine::skills::SkillScope::Builtin
        );
        assert!(
            package
                .root_dir
                .as_ref()
                .is_some_and(|path| path.join("SKILL.md").is_file())
        );
        assert!(package.exports.resources.bin_dir.is_some());
        assert!(package.exports.resources.scripts_dir.is_some());
        assert!(package.exports.resources.references_dir.is_some());
        assert!(package.exports.child_agents.is_empty());
        assert_eq!(package.skill_ids, vec!["swebench".to_string()]);

        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "swebench")
            .unwrap();
        assert_eq!(skill.package_id.as_deref(), Some("builtin:alan-swebench"));
        assert!(skill.available);
        assert!(matches!(
            skill.execution,
            ResolvedSkillExecution::Inline { .. }
        ));
    }

    #[test]
    fn build_skill_catalog_snapshot_preserves_skill_override_flags() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let skills_dir = workspace_alan_dir.join("agents/default/skills");
        create_skill(
            &skills_dir,
            "hidden-helper",
            r#"---
name: Hidden Helper
description: Hidden helper skill
---

Body
"#,
        );

        let mut runtime_config = WorkspaceRuntimeConfig::from(Config::default());
        runtime_config.workspace_root_dir = Some(workspace_root.clone());
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
        runtime_config.agent_home_paths = Some(AlanHomePaths::from_home_dir(temp.path()));

        let mut context = resolve_skill_catalog_context(&runtime_config).unwrap();
        context.resolved.skill_overrides = vec![alan_agent_engine::skills::SkillOverride {
            skill_id: "hidden-helper".to_string(),
            enabled: Some(true),
            allow_implicit_invocation: Some(false),
        }];
        context.registry = SkillsRegistry::load_capability_view(
            &context.resolved.capability_view,
            &context.resolved.skill_overrides,
        )
        .unwrap();

        let snapshot = build_skill_catalog_snapshot(&context).unwrap();

        let skill = snapshot
            .skills
            .iter()
            .find(|skill| skill.id == "hidden-helper")
            .unwrap();
        assert!(skill.enabled);
        assert!(!skill.allow_implicit_invocation);
        let package = snapshot
            .packages
            .iter()
            .find(|package| package.id == "skill:hidden-helper")
            .unwrap();
        assert_eq!(package.skill_ids, vec!["hidden-helper".to_string()]);
    }

    #[test]
    fn build_skill_catalog_snapshot_retains_overlaid_packages_by_id() {
        let temp = TempDir::new().unwrap();
        let home_paths = AlanHomePaths::from_home_dir(temp.path());
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let global_skills_dir = home_paths.global_agent_root_dir.join("skills");
        let workspace_skills_dir = workspace_alan_dir.join("agents/default/skills");

        create_skill(
            &global_skills_dir,
            "repo-review",
            r#"---
name: Repo Review
description: Global overlay
---

Body
"#,
        );
        create_skill(
            &workspace_skills_dir,
            "repo-review",
            r#"---
name: Repo Review
description: Workspace overlay
---

Body
"#,
        );

        let mut runtime_config = WorkspaceRuntimeConfig::from(Config::default());
        runtime_config.workspace_root_dir = Some(workspace_root);
        runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
        runtime_config.agent_home_paths = Some(home_paths);

        let context = resolve_skill_catalog_context(&runtime_config).unwrap();
        let snapshot = build_skill_catalog_snapshot(&context).unwrap();

        let packages: Vec<_> = snapshot
            .packages
            .iter()
            .filter(|package| package.id == "skill:repo-review")
            .collect();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].scope, SkillScope::Repo);
        assert_eq!(packages[1].scope, SkillScope::User);
        assert_eq!(packages[0].skill_ids, vec!["repo-review".to_string()]);
        assert_eq!(packages[1].skill_ids, vec!["repo-review".to_string()]);
    }

    #[test]
    fn write_skill_override_adds_updates_and_removes_entry() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("agent.toml");

        write_skill_override(
            &config_path,
            "repo-review",
            Some(Some(true)),
            Some(Some(false)),
        )
        .unwrap();
        let first = fs::read_to_string(&config_path).unwrap();
        assert!(first.contains("skill = \"repo-review\""));
        assert!(first.contains("enabled = true"));
        assert!(first.contains("allow_implicit_invocation = false"));

        write_skill_override(&config_path, "repo-review", Some(Some(false)), Some(None)).unwrap();
        let second = fs::read_to_string(&config_path).unwrap();
        assert!(second.contains("enabled = false"));
        assert!(!second.contains("allow_implicit_invocation = false"));

        write_skill_override(&config_path, "repo-review", Some(None), None).unwrap();
        assert!(!config_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn write_skill_override_preserves_existing_file_permissions() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("agent.toml");
        fs::write(&config_path, "connection_profile = \"openai-main\"\n").unwrap();
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_skill_override(
            &config_path,
            "repo-review",
            Some(Some(true)),
            Some(Some(false)),
        )
        .unwrap();

        let mode = std::fs::metadata(&config_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn write_skill_override_rejects_noncanonical_skill_id() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("agent.toml");

        let err =
            write_skill_override(&config_path, "repo.review", Some(Some(true)), None).unwrap_err();
        assert!(
            err.to_string()
                .contains("canonical runtime skill id `repo-review`")
        );
    }

    #[test]
    fn write_skill_override_rejects_legacy_skill_override_entries() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("agent.toml");
        fs::write(
            &config_path,
            r#"
[[skill_overrides]]
skill_id = "repo-review"
enabled = true
"#,
        )
        .unwrap();

        let err =
            write_skill_override(&config_path, "repo-review", Some(Some(false)), None).unwrap_err();
        assert!(err.to_string().contains("legacy `skill_id`"));
    }

    #[test]
    fn resolve_skill_host_capabilities_marks_delegated_invocation_for_top_level_catalogs() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        fs::create_dir_all(&workspace_root).unwrap();
        let resolved = ResolvedAgentDefinition {
            roots: ResolvedAgentRoots::default(),
            workspace_root_dir: Some(workspace_root.clone()),
            workspace_alan_dir: Some(workspace_root.join(".alan")),
            agent_name: None,
            config_overlay_paths: Vec::new(),
            persona_dirs: Vec::new(),
            capability_view: alan_agent_engine::skills::ResolvedCapabilityView::from_package_dirs(
                vec![alan_agent_engine::skills::ScopedPackageDir {
                    path: workspace_root.join(".alan/agents/default/skills"),
                    scope: SkillScope::Repo,
                }],
            ),
            skill_overrides: Vec::new(),
            default_policy_path: None,
            writable_root_dir: None,
            writable_config_path: None,
            writable_persona_dir: None,
        };

        let capabilities = resolve_skill_host_capabilities(&Config::default(), &resolved).unwrap();

        assert!(capabilities.supports_delegated_skill_invocation());
        assert!(capabilities.supports_required_tool("invoke_delegated_skill"));
    }

    #[test]
    fn resolve_skill_host_capabilities_keeps_delegated_invocation_off_for_launch_roots() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let launch_root = temp.path().join("child-agent");
        fs::create_dir_all(&workspace_root).unwrap();
        fs::create_dir_all(&launch_root).unwrap();

        let resolved = ResolvedAgentDefinition {
            roots: ResolvedAgentRoots::default()
                .with_appended_root(AgentRootPaths::new(AgentRootKind::LaunchRoot, launch_root)),
            workspace_root_dir: Some(workspace_root.clone()),
            workspace_alan_dir: Some(workspace_root.join(".alan")),
            agent_name: None,
            config_overlay_paths: Vec::new(),
            persona_dirs: Vec::new(),
            capability_view: alan_agent_engine::skills::ResolvedCapabilityView::default(),
            skill_overrides: Vec::new(),
            default_policy_path: None,
            writable_root_dir: None,
            writable_config_path: None,
            writable_persona_dir: None,
        };

        let capabilities = resolve_skill_host_capabilities(&Config::default(), &resolved).unwrap();

        assert!(!capabilities.supports_delegated_skill_invocation());
        assert!(!capabilities.supports_required_tool("invoke_delegated_skill"));
    }
}
