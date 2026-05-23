use crate::{
    AgentRootLayout, AlanHomePaths, ConfigSourceKind, ResolvedAgentRoots,
    config::merge_skill_override_overlays_from_paths,
    runtime::WorkspaceRuntimeConfig,
    skills::{ResolvedCapabilityView, ScopedPackageDir, SkillOverride, SkillScope},
    workspace_public_skills_dir,
};
use std::path::{Path, PathBuf};

/// Canonical resolved agent definition derived from runtime launch input.
#[derive(Debug, Clone)]
pub struct ResolvedAgentDefinition {
    pub agent_name: Option<String>,
    pub workspace_root_dir: Option<PathBuf>,
    pub workspace_alan_dir: Option<PathBuf>,
    pub roots: ResolvedAgentRoots,
    pub config_overlay_paths: Vec<PathBuf>,
    pub persona_dirs: Vec<PathBuf>,
    pub capability_view: ResolvedCapabilityView,
    pub skill_overrides: Vec<SkillOverride>,
    pub default_policy_path: Option<PathBuf>,
    pub writable_root_dir: Option<PathBuf>,
    pub writable_config_path: Option<PathBuf>,
    pub writable_persona_dir: Option<PathBuf>,
}

impl ResolvedAgentDefinition {
    pub fn from_runtime_config(config: &WorkspaceRuntimeConfig) -> anyhow::Result<Self> {
        let workspace_alan_dir = config.workspace_alan_dir.clone().or_else(|| {
            infer_workspace_alan_dir_from_memory_dir(
                config
                    .agent_config
                    .core_config
                    .memory
                    .workspace_dir
                    .as_deref(),
            )
        });
        let workspace_root_dir = config
            .workspace_root_dir
            .clone()
            .or_else(|| infer_workspace_root_from_alan_dir(workspace_alan_dir.as_deref()));
        let layout = AgentRootLayout::new();
        let agent_name = layout
            .normalize_agent_name(config.agent_name.as_deref())
            .map(str::to_owned);
        let home_paths = config
            .agent_home_paths
            .clone()
            .or_else(AlanHomePaths::detect);
        let mut roots = ResolvedAgentRoots::with_home_paths(
            home_paths.clone(),
            workspace_root_dir.as_deref(),
            agent_name.as_deref(),
        );
        if let Some(launch_root_dir) = config.launch_root_dir.clone() {
            roots = roots.with_appended_root(layout.launch_root(launch_root_dir));
        }
        let config_overlay_paths = overlay_config_paths(&roots, config.core_config_source);
        let persona_dirs = roots.persona_dirs();
        let package_dirs =
            package_dirs_for_roots(&roots, home_paths.as_ref(), workspace_root_dir.as_deref());
        let capability_view = ResolvedCapabilityView::from_package_dirs(package_dirs);
        let skill_overrides = merge_skill_override_overlays_from_paths(
            &config.agent_config.core_config.resolved_skill_overrides(),
            &config_overlay_paths,
        )?;

        Ok(Self {
            agent_name,
            workspace_root_dir,
            workspace_alan_dir,
            default_policy_path: roots.highest_precedence_policy_path(),
            writable_root_dir: roots.writable_root_dir(),
            writable_config_path: roots.writable_config_path(),
            writable_persona_dir: roots.writable_persona_dir(),
            roots,
            config_overlay_paths,
            persona_dirs,
            capability_view,
            skill_overrides,
        })
    }
}

fn package_dirs_for_roots(
    roots: &ResolvedAgentRoots,
    home_paths: Option<&AlanHomePaths>,
    workspace_root_dir: Option<&Path>,
) -> Vec<ScopedPackageDir> {
    let mut package_dirs = Vec::new();

    for root in roots.roots() {
        match root.kind {
            crate::AgentRootKind::GlobalDefault => {
                if let Some(home_paths) = home_paths {
                    package_dirs.push(ScopedPackageDir {
                        path: home_paths.global_public_skills_dir.clone(),
                        scope: SkillScope::User,
                    });
                }
                package_dirs.push(ScopedPackageDir {
                    path: root.skills_dir.clone(),
                    scope: SkillScope::User,
                });
            }
            crate::AgentRootKind::WorkspaceDefault => {
                if let Some(workspace_root_dir) = workspace_root_dir {
                    package_dirs.push(ScopedPackageDir {
                        path: workspace_public_skills_dir(workspace_root_dir),
                        scope: SkillScope::Repo,
                    });
                }
                package_dirs.push(ScopedPackageDir {
                    path: root.skills_dir.clone(),
                    scope: SkillScope::Repo,
                });
            }
            crate::AgentRootKind::GlobalNamed(_) => {
                package_dirs.push(ScopedPackageDir {
                    path: root.skills_dir.clone(),
                    scope: SkillScope::User,
                });
            }
            crate::AgentRootKind::WorkspaceNamed(_) => {
                package_dirs.push(ScopedPackageDir {
                    path: root.skills_dir.clone(),
                    scope: SkillScope::Repo,
                });
            }
            crate::AgentRootKind::LaunchRoot => {
                package_dirs.push(ScopedPackageDir {
                    path: root.skills_dir.clone(),
                    scope: SkillScope::Repo,
                });
            }
        }
    }

    package_dirs
}

fn overlay_config_paths(roots: &ResolvedAgentRoots, base_source: ConfigSourceKind) -> Vec<PathBuf> {
    roots
        .roots()
        .iter()
        .filter(|root| {
            !matches!(
                (&root.kind, base_source),
                (
                    crate::AgentRootKind::GlobalDefault,
                    ConfigSourceKind::GlobalAgentHome
                ) | (_, ConfigSourceKind::EnvOverride)
            )
        })
        .map(|root| root.config_path.clone())
        .collect()
}

fn infer_workspace_alan_dir_from_memory_dir(memory_dir: Option<&Path>) -> Option<PathBuf> {
    let memory_dir = memory_dir?;
    let is_memory_dir = memory_dir
        .file_name()
        .map(|name| name == std::ffi::OsStr::new("memory"))
        .unwrap_or(false);
    if !is_memory_dir {
        return None;
    }

    let alan_dir = memory_dir.parent()?;
    let is_alan_dir = alan_dir
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false);
    is_alan_dir.then(|| alan_dir.to_path_buf())
}

fn infer_workspace_root_from_alan_dir(alan_dir: Option<&Path>) -> Option<PathBuf> {
    let alan_dir = alan_dir?;
    let is_alan_dir = alan_dir
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false);
    if !is_alan_dir {
        return None;
    }

    alan_dir.parent().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlanHomePaths, Config, InstallChannel, skills::SkillOverride};
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_skill(root_dir: &Path, skill_dir_name: &str, skill_name: &str) {
        let skill_dir = root_dir.join("skills").join(skill_dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: test skill
---

Body
"#
            ),
        )
        .unwrap();
    }

    fn create_public_skill(root_dir: &Path, skill_dir_name: &str, skill_name: &str) {
        let skill_dir = root_dir.join(".agents/skills").join(skill_dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {skill_name}
description: public test skill
---

Body
"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolved_agent_definition_uses_named_agent_overlay_order() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_alan_dir.clone());
        config.agent_name = Some("coder".to_string());

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();
        let home_paths = AlanHomePaths::detect().unwrap();
        let layout = AgentRootLayout::new();
        let workspace_default_root =
            layout.workspace_default_root_from_alan_dir(&workspace_alan_dir);
        let workspace_named_root = layout.workspace_named_root(&workspace_root, "coder");

        assert_eq!(
            resolved.config_overlay_paths,
            vec![
                home_paths.global_agent_config_path.clone(),
                workspace_default_root.config_path.clone(),
                layout.global_named_root(&home_paths, "coder").config_path,
                workspace_named_root.config_path.clone(),
            ]
        );
        assert_eq!(resolved.agent_name.as_deref(), Some("coder"));
        assert_eq!(
            resolved.writable_root_dir,
            Some(workspace_named_root.root_dir)
        );
    }

    #[test]
    fn resolved_agent_definition_skips_global_default_overlay_for_global_home_source() {
        let workspace_root = TempDir::new().unwrap();
        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.path().to_path_buf());
        config.workspace_alan_dir = Some(workspace_root.path().join(".alan"));
        config.core_config_source = ConfigSourceKind::GlobalAgentHome;

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();
        let layout = AgentRootLayout::new();

        assert_eq!(
            resolved.config_overlay_paths,
            vec![
                layout
                    .workspace_default_root(workspace_root.path())
                    .config_path
            ]
        );
    }

    #[test]
    fn resolved_agent_definition_infers_workspace_paths_from_memory_dir() {
        let workspace_root = TempDir::new().unwrap();
        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.agent_config.core_config.memory.workspace_dir =
            Some(workspace_root.path().join(".alan/memory"));

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        assert_eq!(
            resolved.workspace_alan_dir,
            Some(workspace_root.path().join(".alan"))
        );
        assert_eq!(
            resolved.workspace_root_dir,
            Some(workspace_root.path().to_path_buf())
        );
        assert_eq!(
            resolved.writable_root_dir,
            Some(
                AgentRootLayout::new()
                    .workspace_default_root(workspace_root.path())
                    .root_dir
            )
        );
    }

    #[test]
    fn resolved_agent_definition_merges_skill_overrides_in_overlay_order() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let home_paths = AlanHomePaths::from_home_dir(&home);
        let layout = AgentRootLayout::new();
        let global_root = home_paths.global_agent_root_dir.clone();
        let workspace_agent_root = layout.workspace_default_root(&workspace_root).root_dir;
        let global_named_root = layout.global_named_root(&home_paths, "coder").root_dir;
        let workspace_named_root = layout
            .workspace_named_root(&workspace_root, "coder")
            .root_dir;

        create_test_skill(&workspace_agent_root, "test-skill", "Test Skill");
        std::fs::create_dir_all(&global_root).unwrap();
        std::fs::create_dir_all(&workspace_named_root).unwrap();
        std::fs::create_dir_all(&global_named_root).unwrap();

        std::fs::write(
            global_root.join("agent.toml"),
            r#"
[[skill_overrides]]
skill = "test-skill"
allow_implicit_invocation = false
"#,
        )
        .unwrap();
        std::fs::write(
            workspace_named_root.join("agent.toml"),
            r#"
[[skill_overrides]]
skill = "test-skill"
enabled = false
"#,
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_root.join(".alan"));
        config.agent_name = Some("coder".to_string());
        config.agent_home_paths = Some(home_paths);

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        let override_entry = resolved
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "test-skill")
            .unwrap();
        assert_eq!(override_entry.allow_implicit_invocation, Some(false));
        assert_eq!(override_entry.enabled, Some(false));
    }

    #[test]
    fn resolved_agent_definition_treats_explicit_default_as_default_root() {
        let temp = TempDir::new().unwrap();
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root);
        config.workspace_alan_dir = Some(workspace_alan_dir.clone());
        config.agent_name = Some(crate::DEFAULT_AGENT_NAME.to_string());

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();
        let home_paths = AlanHomePaths::detect().unwrap();
        let workspace_default_root =
            AgentRootLayout::new().workspace_default_root_from_alan_dir(&workspace_alan_dir);

        assert_eq!(
            resolved.config_overlay_paths,
            vec![
                home_paths.global_agent_config_path,
                workspace_default_root.config_path.clone(),
            ]
        );
        assert_eq!(
            resolved.writable_root_dir,
            Some(workspace_default_root.root_dir)
        );
    }

    #[test]
    fn resolved_agent_definition_ignores_legacy_singular_default_roots() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let home_paths = AlanHomePaths::from_home_dir(&home);
        let legacy_global_root = home.join(".alan/agent");
        let legacy_workspace_root = workspace_root.join(".alan/agent");

        std::fs::create_dir_all(legacy_global_root.join("skills/legacy-global")).unwrap();
        std::fs::write(
            legacy_global_root.join("agent.toml"),
            "[[skill_overrides]]\nskill = ",
        )
        .unwrap();
        std::fs::write(
            legacy_global_root.join("skills/legacy-global/SKILL.md"),
            "---\nname: legacy-global\ndescription: ignored\n---\n",
        )
        .unwrap();
        std::fs::create_dir_all(legacy_workspace_root.join("persona")).unwrap();
        std::fs::create_dir_all(legacy_workspace_root.join("skills/legacy-workspace")).unwrap();
        std::fs::write(legacy_workspace_root.join("policy.yaml"), "rules: []\n").unwrap();
        std::fs::write(
            legacy_workspace_root.join("skills/legacy-workspace/SKILL.md"),
            "---\nname: legacy-workspace\ndescription: ignored\n---\n",
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_root.join(".alan"));
        config.agent_home_paths = Some(home_paths);

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        assert!(
            resolved
                .config_overlay_paths
                .iter()
                .all(|path| !path.starts_with(&legacy_global_root)
                    && !path.starts_with(&legacy_workspace_root))
        );
        assert!(
            resolved
                .persona_dirs
                .iter()
                .all(|path| !path.starts_with(&legacy_workspace_root))
        );
        assert_ne!(
            resolved.default_policy_path,
            Some(legacy_workspace_root.join("policy.yaml"))
        );
        assert!(
            !resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id.contains("legacy"))
        );
    }

    #[test]
    fn resolved_agent_definition_honors_env_override_skill_overrides_without_root_parsing() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let home_paths = AlanHomePaths::from_home_dir(&home);
        let global_root = home_paths.global_agent_root_dir.clone();

        std::fs::create_dir_all(&global_root).unwrap();
        std::fs::write(
            global_root.join("agent.toml"),
            "[[skill_overrides]]\nskill = ",
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_root.join(".alan"));
        config.agent_home_paths = Some(home_paths);
        config.core_config_source = ConfigSourceKind::EnvOverride;
        config.agent_config.core_config.skill_overrides = vec![SkillOverride {
            skill_id: "plan".to_string(),
            enabled: None,
            allow_implicit_invocation: Some(false),
        }];

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();
        let override_entry = resolved
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "plan")
            .unwrap();
        assert_eq!(override_entry.allow_implicit_invocation, Some(false));
    }

    #[test]
    fn resolved_agent_definition_discovers_public_skill_directories() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let home_paths = AlanHomePaths::from_home_dir(&home);

        create_public_skill(&home, "global-public-skill", "Global Public Skill");
        create_public_skill(
            &workspace_root,
            "workspace-public-skill",
            "Workspace Public Skill",
        );

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_root.join(".alan"));
        config.agent_home_paths = Some(home_paths.clone());

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        assert!(resolved.capability_view.package_dirs.iter().any(|dir| {
            dir.path == home_paths.global_public_skills_dir && dir.scope == SkillScope::User
        }));
        assert!(resolved.capability_view.package_dirs.iter().any(|dir| {
            dir.path == workspace_public_skills_dir(&workspace_root)
                && dir.scope == SkillScope::Repo
        }));
        assert!(
            resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:global-public-skill")
        );
        assert!(
            resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:workspace-public-skill")
        );
    }

    #[test]
    fn resolved_agent_definition_uses_dev_public_skills_without_stable_fallback() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let home_paths = AlanHomePaths::from_home_dir_for_channel(&home, InstallChannel::Dev);

        create_public_skill(&home, "stable-only-skill", "Stable Only Skill");
        let dev_skill_dir = home_paths.global_public_skills_dir.join("dev-public-skill");
        std::fs::create_dir_all(&dev_skill_dir).unwrap();
        std::fs::write(
            dev_skill_dir.join("SKILL.md"),
            r#"---
name: Dev Public Skill
description: dev public test skill
---

Body
"#,
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_root.join(".alan"));
        config.agent_home_paths = Some(home_paths.clone());

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        assert!(resolved.capability_view.package_dirs.iter().any(|dir| {
            dir.path == home_paths.global_public_skills_dir && dir.scope == SkillScope::User
        }));
        assert!(
            resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:dev-public-skill")
        );
        assert!(
            !resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:stable-only-skill")
        );
    }

    #[test]
    fn resolved_agent_definition_appends_launch_root_to_overlay_and_writable_paths() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let workspace_root = temp.path().join("workspace");
        let workspace_alan_dir = workspace_root.join(".alan");
        let launch_root = workspace_alan_dir.join("agents/grader");
        let home_paths = AlanHomePaths::from_home_dir(&home);

        std::fs::create_dir_all(launch_root.join("persona")).unwrap();
        create_test_skill(&launch_root, "launch-only-skill", "Launch Only Skill");
        std::fs::write(
            launch_root.join("agent.toml"),
            r#"
tool_repeat_limit = 9
"#,
        )
        .unwrap();

        let mut config = WorkspaceRuntimeConfig::from(Config::default());
        config.workspace_root_dir = Some(workspace_root.clone());
        config.workspace_alan_dir = Some(workspace_alan_dir.clone());
        config.agent_home_paths = Some(home_paths);
        config.launch_root_dir = Some(launch_root.clone());

        let resolved = ResolvedAgentDefinition::from_runtime_config(&config).unwrap();

        assert!(matches!(
            resolved.roots.roots().last().map(|root| &root.kind),
            Some(crate::AgentRootKind::LaunchRoot)
        ));
        assert_eq!(
            resolved.config_overlay_paths.last(),
            Some(&launch_root.join("agent.toml"))
        );
        assert_eq!(resolved.writable_root_dir, Some(launch_root.clone()));
        assert_eq!(
            resolved.writable_config_path,
            Some(launch_root.join("agent.toml"))
        );
        assert_eq!(
            resolved.writable_persona_dir,
            Some(launch_root.join("persona"))
        );
        assert!(
            resolved
                .capability_view
                .package_dirs
                .iter()
                .any(|dir| dir.path == launch_root.join("skills") && dir.scope == SkillScope::Repo)
        );
        assert!(
            resolved
                .capability_view
                .packages
                .iter()
                .any(|package| package.id == "skill:launch-only-skill")
        );
    }
}
