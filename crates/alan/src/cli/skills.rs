use crate::cli::load_agent_config_metadata_with_notice;
use crate::cli::skill_authoring::{
    SkillTemplateKind, eval_skill_package, init_skill_package, validate_skill_package,
};
use crate::registry::normalize_workspace_root_path;
use crate::skill_catalog::resolve_skill_catalog_context;
use alan_agent_engine::skills::{
    ResolvedSkillExecution, SkillExecutionUnresolvedReason, SkillHostCapabilities, SkillMetadata,
    SkillsRegistry, format_skill_availability_issues, list_skills, skill_availability_issues,
};
use alan_agent_engine::{
    AgentRootKind, LoadedConfig, ResolvedAgentDefinition, WorkspaceRuntimeConfig,
    workspace_alan_dir,
};
use alan_swebench_tooling::{
    MaterializeSwebenchLiteSubsetOptions, PrepareSwebenchLiteWorkspacesOptions,
    materialize_swebench_lite_subset, prepare_swebench_lite_workspaces,
};
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn run_list_skills(workspace: Option<PathBuf>, agent_name: Option<&str>) -> Result<()> {
    let (_, registry, host_capabilities) = resolve_registry(workspace, agent_name)?;
    print!("{}", list_skills(&registry, &host_capabilities));
    Ok(())
}

pub fn run_list_packages(workspace: Option<PathBuf>, agent_name: Option<&str>) -> Result<()> {
    let (resolved, registry, host_capabilities) = resolve_registry(workspace, agent_name)?;
    println!(
        "{}",
        render_packages(&resolved, &registry, &host_capabilities)
    );
    Ok(())
}

pub fn run_init_skill_package(
    path: PathBuf,
    template: SkillTemplateKind,
    name: Option<&str>,
    description: Option<&str>,
    short_description: Option<&str>,
    force: bool,
) -> Result<()> {
    let result = init_skill_package(&path, template, name, description, short_description, force)?;
    print!("{}", result.render_text());
    Ok(())
}

pub fn run_validate_skill_package(path: Option<PathBuf>, json: bool, strict: bool) -> Result<bool> {
    let package_root = canonicalize_package_input(path)?;
    let report = validate_skill_package(&package_root);
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", report.render_text());
    }
    Ok(report.passes(strict))
}

pub fn run_eval_skill_package(
    path: Option<PathBuf>,
    manifest: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    require_hook: bool,
) -> Result<bool> {
    let package_root = canonicalize_package_input(path)?;
    let result = eval_skill_package(
        &package_root,
        manifest.as_deref(),
        output_dir.as_deref(),
        require_hook,
    )?;
    print!("{}", result.render_text());
    Ok(result.passed(require_hook))
}

pub fn run_prepare_swebench_lite_workspaces(
    options: PrepareSwebenchLiteWorkspacesOptions,
) -> Result<bool> {
    let result = prepare_swebench_lite_workspaces(&options)?;
    println!("workspace_root\t{}", result.workspace_root.display());
    println!("workspace_map\t{}", result.workspace_map_path.display());
    println!("report\t{}", result.report_path.display());
    println!("prepared_count\t{}", result.report.prepared_count);
    println!("reused_count\t{}", result.report.reused_count);
    println!("recreated_count\t{}", result.report.recreated_count);
    println!("failed_count\t{}", result.report.failed_count);
    for (repo, count) in &result.report.repos {
        println!("repo_count\t{repo}\t{count}");
    }
    if result.report.failed_count > 0 {
        for failure in &result.report.failures {
            eprintln!("failure\t{}\t{}", failure.instance_id, failure.reason);
        }
        return Ok(false);
    }
    Ok(true)
}

pub fn run_materialize_swebench_lite_subset(
    options: MaterializeSwebenchLiteSubsetOptions,
) -> Result<()> {
    let result = materialize_swebench_lite_subset(&options)?;
    println!("suite_json\t{}", result.suite_path.display());
    println!("instance_count\t{}", result.report.instance_count);
    for (repo, count) in &result.report.repos {
        println!("repo_count\t{repo}\t{count}");
    }
    if !result.report.missing_workspace_dirs.is_empty() {
        eprintln!(
            "warning\tmissing_workspaces\t{}",
            result.report.missing_workspace_dirs.join(",")
        );
    }
    Ok(())
}

fn resolve_registry(
    workspace: Option<PathBuf>,
    agent_name: Option<&str>,
) -> Result<(
    ResolvedAgentDefinition,
    SkillsRegistry,
    SkillHostCapabilities,
)> {
    let loaded = load_agent_config_metadata_with_notice()?;
    resolve_registry_with_loaded_config(workspace, agent_name, loaded, None)
}

fn resolve_registry_with_loaded_config(
    workspace: Option<PathBuf>,
    agent_name: Option<&str>,
    loaded: LoadedConfig,
    home_paths: Option<alan_agent_engine::AlanHomePaths>,
) -> Result<(
    ResolvedAgentDefinition,
    SkillsRegistry,
    SkillHostCapabilities,
)> {
    let (workspace_root, workspace_alan_dir) = resolve_workspace_context(workspace)?;
    let mut runtime_config = WorkspaceRuntimeConfig::from(loaded.clone());
    runtime_config.workspace_root_dir = Some(workspace_root.clone());
    runtime_config.workspace_alan_dir = Some(workspace_alan_dir);
    runtime_config.agent_name = agent_name.map(str::to_owned);
    runtime_config.agent_home_paths = home_paths;

    let context = resolve_skill_catalog_context(&runtime_config)?;
    Ok((
        context.resolved,
        context.registry,
        context.host_capabilities,
    ))
}

fn canonicalize_workspace_input(workspace: Option<PathBuf>) -> Result<PathBuf> {
    let workspace = workspace.unwrap_or(
        std::env::current_dir()
            .context("Cannot determine current directory for skill inspection")?,
    );
    std::fs::canonicalize(&workspace)
        .with_context(|| format!("Cannot resolve workspace path: {}", workspace.display()))
}

fn canonicalize_package_input(path: Option<PathBuf>) -> Result<PathBuf> {
    let path = path.unwrap_or(
        std::env::current_dir()
            .context("Cannot determine current directory for skill package operation")?,
    );
    std::fs::canonicalize(&path)
        .with_context(|| format!("Cannot resolve skill package path: {}", path.display()))
}

#[cfg(test)]
fn resolve_workspace_root(workspace: Option<PathBuf>) -> Result<PathBuf> {
    let canonical = canonicalize_workspace_input(workspace)?;
    Ok(normalize_workspace_root_path(&canonical))
}

fn resolve_workspace_context(workspace: Option<PathBuf>) -> Result<(PathBuf, PathBuf)> {
    let canonical = canonicalize_workspace_input(workspace)?;
    let workspace_root = normalize_workspace_root_path(&canonical);
    let workspace_alan_dir = if canonical
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false)
    {
        canonical
    } else {
        workspace_alan_dir(&workspace_root)
    };
    Ok((workspace_root, workspace_alan_dir))
}

fn render_packages(
    resolved: &ResolvedAgentDefinition,
    registry: &SkillsRegistry,
    host_capabilities: &SkillHostCapabilities,
) -> String {
    let mut lines = vec![
        "Resolved Agent Roots".to_string(),
        "====================".to_string(),
        String::new(),
    ];

    if resolved.roots.roots().is_empty() {
        lines.push("No agent roots resolved.".to_string());
    } else {
        for root in resolved.roots.roots() {
            lines.push(format!(
                "[{}] {}",
                root_kind_label(&root.kind),
                root.root_dir.display()
            ));
        }
    }

    lines.extend([
        String::new(),
        "Resolved Packages".to_string(),
        "=================".to_string(),
        String::new(),
    ]);

    let mut skills_by_package: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for skill in registry.list_sorted() {
        if let Some(package_id) = skill.package_id.as_deref() {
            skills_by_package
                .entry(package_id)
                .or_default()
                .push(render_skill_label(skill, host_capabilities));
        }
    }

    let mut packages: Vec<_> = resolved.capability_view.packages.iter().collect();
    packages.sort_by(|left, right| {
        left.scope
            .priority()
            .cmp(&right.scope.priority())
            .then_with(|| left.id.cmp(&right.id))
    });

    for package in packages {
        let source_label = package
            .root_dir
            .as_deref()
            .map(render_path)
            .unwrap_or_else(|| "<embedded>".to_string());
        let skills_label = skills_by_package
            .get(package.id.as_str())
            .map(|skills| skills.join(", "))
            .unwrap_or_else(|| "not exposed".to_string());

        lines.push(format!("[{}] {}", scope_label(package.scope), package.id));
        lines.push(format!("         source: {}", source_label));
        if let Some(exports_label) = render_package_exports(package) {
            lines.push(format!("         exports: {}", exports_label));
        }
        lines.push(format!("         skills: {}", skills_label));
        lines.push(String::new());
    }

    lines.join("\n")
}

fn root_kind_label(kind: &AgentRootKind) -> &'static str {
    match kind {
        AgentRootKind::GlobalDefault => "global-default",
        AgentRootKind::WorkspaceDefault => "workspace-default",
        AgentRootKind::GlobalNamed(_) => "global-named",
        AgentRootKind::WorkspaceNamed(_) => "workspace-named",
        AgentRootKind::LaunchRoot => "launch-root",
    }
}

fn scope_label(scope: alan_agent_engine::skills::SkillScope) -> &'static str {
    match scope {
        alan_agent_engine::skills::SkillScope::Repo => "repo",
        alan_agent_engine::skills::SkillScope::User => "user",
        alan_agent_engine::skills::SkillScope::Builtin => "builtin",
    }
}

fn render_path(path: &Path) -> String {
    path.display().to_string()
}

fn render_skill_label(skill: &SkillMetadata, host_capabilities: &SkillHostCapabilities) -> String {
    let issues = skill_availability_issues(skill, host_capabilities);
    let mut annotations = Vec::new();
    if let Some(execution_tag) = render_skill_execution_tag(&skill.execution) {
        annotations.push(execution_tag);
    }
    if !skill.enabled {
        annotations.push("disabled".to_string());
    } else if !skill.allow_implicit_invocation {
        annotations.push("implicit: false".to_string());
    }
    if !issues.is_empty() {
        annotations.push(format!(
            "unavailable: {}",
            format_skill_availability_issues(&issues)
        ));
    }

    if annotations.is_empty() {
        format!("${}", skill.id)
    } else {
        format!("${} [{}]", skill.id, annotations.join("] ["))
    }
}

fn render_skill_execution_tag(execution: &ResolvedSkillExecution) -> Option<String> {
    match execution {
        ResolvedSkillExecution::Delegate { target, .. } => Some(format!("delegate: {target}")),
        ResolvedSkillExecution::Unresolved { reason } => match reason {
            SkillExecutionUnresolvedReason::NotResolved => None,
            _ => Some(format!("execution unresolved: {}", reason.render_label())),
        },
        ResolvedSkillExecution::Inline { .. } => None,
    }
}

fn render_package_exports(
    package: &alan_agent_engine::skills::CapabilityPackage,
) -> Option<String> {
    if package.exports.is_empty() {
        return None;
    }

    let mut exports = Vec::new();
    if !package.exports.child_agents.is_empty() {
        exports.push(format!(
            "child_agents={}",
            package.exports.child_agents.len()
        ));
    }

    let mut resources = Vec::new();
    if package.exports.resources.bin_dir.is_some() {
        resources.push("bin");
    }
    if package.exports.resources.scripts_dir.is_some() {
        resources.push("scripts");
    }
    if package.exports.resources.references_dir.is_some() {
        resources.push("references");
    }
    if package.exports.resources.assets_dir.is_some() {
        resources.push("assets");
    }
    if !resources.is_empty() {
        exports.push(format!("resources={}", resources.join("+")));
    }

    (!exports.is_empty()).then(|| exports.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::{AlanHomePaths, Config};
    use std::fs;
    use tempfile::TempDir;

    fn create_skill(root: &Path, skill_name: &str, title: &str) {
        let skill_dir = root.join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {title}
description: {title} description
---

Body
"#
            ),
        )
        .unwrap();
    }

    fn create_skill_with_frontmatter(root: &Path, skill_name: &str, frontmatter: &str) {
        let skill_dir = root.join(skill_name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), frontmatter).unwrap();
    }

    #[test]
    fn render_packages_shows_roots_and_exposed_skills() {
        let temp = TempDir::new().unwrap();
        let home_paths = AlanHomePaths::from_home_dir(temp.path());
        let workspace_root = temp.path().join("workspace");
        let workspace_skills_dir = workspace_root.join(".alan/agents/default/skills");
        create_skill(&workspace_skills_dir, "repo-skill", "Repo Skill");

        let (resolved, registry, host_capabilities) = resolve_registry_with_loaded_config(
            Some(workspace_root),
            None,
            LoadedConfig {
                config: Config::default(),
                path: None,
                source: alan_agent_engine::ConfigSourceKind::Default,
            },
            Some(home_paths),
        )
        .unwrap();

        let rendered = render_packages(&resolved, &registry, &host_capabilities);
        assert!(rendered.contains("Resolved Agent Roots"));
        assert!(rendered.contains("Resolved Packages"));
        assert!(rendered.contains("[builtin] builtin:alan-plan"));
        assert!(rendered.contains("[builtin] builtin:alan-skill-creator"));
        assert!(rendered.contains("[builtin] builtin:alan-swebench"));
        assert!(rendered.contains("[repo] skill:repo-skill"));
        assert!(rendered.contains("skills: $repo-skill"));
        assert!(rendered.contains("skills: $memory"));
        assert!(rendered.contains("skills: $skill-creator [delegate: skill-creator]"));
        assert!(
            rendered.contains("exports: child_agents=1, resources=bin+scripts+references+assets")
        );
        assert!(rendered.contains("exports: resources=bin+scripts+references"));
        assert!(!rendered.contains("$memory [unavailable:"));
    }

    #[test]
    fn render_packages_shows_package_exports_and_unavailable_skills() {
        let temp = TempDir::new().unwrap();
        let home_paths = AlanHomePaths::from_home_dir(temp.path());
        let workspace_root = temp.path().join("workspace");
        let workspace_skills_dir = workspace_root.join(".alan/agents/default/skills");
        let skill_dir = workspace_skills_dir.join("tool-heavy");
        fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
        fs::write(
            skill_dir.join("agents/reviewer/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        create_skill_with_frontmatter(
            &workspace_skills_dir,
            "tool-heavy",
            r#"---
name: Tool Heavy
description: Needs extra tools
capabilities:
  required_tools: ["missing_tool"]
---

Body
"#,
        );

        let (resolved, registry, host_capabilities) = resolve_registry_with_loaded_config(
            Some(workspace_root),
            None,
            LoadedConfig {
                config: Config::default(),
                path: None,
                source: alan_agent_engine::ConfigSourceKind::Default,
            },
            Some(home_paths),
        )
        .unwrap();

        let rendered = render_packages(&resolved, &registry, &host_capabilities);
        assert!(rendered.contains("exports: child_agents=1, resources=scripts"));
        assert!(
            rendered.contains(
                "skills: $tool-heavy [delegate: reviewer] [unavailable: missing dependencies: tool:missing_tool]"
            )
        );
    }

    #[test]
    fn render_packages_surfaces_unresolved_execution_tags() {
        let temp = TempDir::new().unwrap();
        let home_paths = AlanHomePaths::from_home_dir(temp.path());
        let workspace_root = temp.path().join("workspace");
        let workspace_skills_dir = workspace_root.join(".alan/agents/default/skills");
        let skill_dir = workspace_skills_dir.join("skill-creator");
        fs::create_dir_all(skill_dir.join("agents/creator")).unwrap();
        fs::create_dir_all(skill_dir.join("agents/grader")).unwrap();
        fs::write(
            skill_dir.join("agents/creator/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        fs::write(
            skill_dir.join("agents/grader/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        create_skill_with_frontmatter(
            &workspace_skills_dir,
            "skill-creator",
            r#"---
name: Skill Creator
description: Delegated package with ambiguous child agents
---

Body
"#,
        );

        let (resolved, registry, host_capabilities) = resolve_registry_with_loaded_config(
            Some(workspace_root),
            None,
            LoadedConfig {
                config: Config::default(),
                path: None,
                source: alan_agent_engine::ConfigSourceKind::Default,
            },
            Some(home_paths),
        )
        .unwrap();

        let rendered = render_packages(&resolved, &registry, &host_capabilities);
        assert!(
            rendered
                .contains("skills: $skill-creator [execution unresolved: ambiguous_package_shape]")
        );
    }

    #[test]
    fn resolve_workspace_root_defaults_to_current_directory() {
        let current = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(
            resolve_workspace_root(None).unwrap(),
            normalize_workspace_root_path(&current)
        );
    }

    #[test]
    fn resolve_workspace_context_keeps_explicit_alan_state_dir() {
        let temp = TempDir::new().unwrap();
        let default_workspace = temp.path().join(".alan");
        fs::create_dir_all(&default_workspace).unwrap();
        let canonical_workspace_root = std::fs::canonicalize(temp.path()).unwrap();
        let canonical_alan_dir = std::fs::canonicalize(&default_workspace).unwrap();

        let (workspace_root, alan_dir) =
            resolve_workspace_context(Some(default_workspace.clone())).unwrap();
        assert_eq!(workspace_root, canonical_workspace_root);
        assert_eq!(alan_dir, canonical_alan_dir);
    }
}
