use crate::agent_root::{AgentRootKind, AgentRootPaths};
use crate::skills::loader;
use crate::skills::types::{
    CapabilityChildAgentExport, CapabilityPackage, CapabilityPackageExports,
    CapabilityPackageResources, PortableSkill, ResolvedCapabilityView, ScopedPackageDir,
    SkillContentSource, SkillScope,
};
use crate::skills::{BUILTIN_PACKAGE_ASSETS, materialized_builtin_package};
use std::path::{Path, PathBuf};

impl ResolvedCapabilityView {
    pub fn from_package_dirs(package_dirs: Vec<ScopedPackageDir>) -> Self {
        let mut view = Self {
            package_dirs,
            ..Self::default()
        };

        view.packages.extend(builtin_capability_packages());

        let package_dirs = view.package_dirs.clone();
        for package_dir in package_dirs {
            let outcome = loader::scan_skills_dir(&package_dir.path, package_dir.scope);
            view.errors.extend(outcome.errors);
            view.tracked_paths.extend(outcome.tracked_paths);

            for skill in outcome.skills {
                let root_dir = skill.path.parent().map(Path::to_path_buf);
                let package_id = format!("skill:{}", skill.id);
                view.packages.push(CapabilityPackage {
                    id: package_id.clone(),
                    scope: package_dir.scope,
                    exports: package_exports_for_root_dir(&package_id, root_dir.as_deref()),
                    root_dir,
                    portable_skill: PortableSkill {
                        path: skill.path.clone(),
                        source: SkillContentSource::File(skill.path),
                    },
                });
            }
        }

        view.tracked_paths.sort();
        view.tracked_paths.dedup();
        view
    }

    pub fn refresh(&self) -> Self {
        Self::from_package_dirs(self.package_dirs.clone())
    }

    pub fn package(&self, package_id: &str) -> Option<&CapabilityPackage> {
        self.packages
            .iter()
            .rev()
            .find(|package| package.id == package_id)
    }

    pub fn resolve_child_agent_target(
        &self,
        target: &alan_agent_protocol::SpawnTarget,
    ) -> Option<PathBuf> {
        let alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id,
            export_name,
        } = target
        else {
            return None;
        };

        self.package(package_id)
            .and_then(|package| package.exports.child_agent_export(export_name))
            .map(|export| export.root_dir.clone())
    }
}

fn builtin_capability_packages() -> Vec<CapabilityPackage> {
    BUILTIN_PACKAGE_ASSETS
        .iter()
        .map(|asset| {
            let materialized = materialized_builtin_package(asset);
            CapabilityPackage {
                id: asset.package_id.to_string(),
                scope: SkillScope::Builtin,
                exports: package_exports_for_root_dir(
                    asset.package_id,
                    Some(materialized.root_dir.as_path()),
                ),
                root_dir: Some(materialized.root_dir.clone()),
                portable_skill: PortableSkill {
                    path: materialized.skill_path.clone(),
                    source: SkillContentSource::File(materialized.skill_path),
                },
            }
        })
        .collect()
}

fn package_exports_for_root_dir(
    package_id: &str,
    root_dir: Option<&Path>,
) -> CapabilityPackageExports {
    let Some(root_dir) = root_dir else {
        return CapabilityPackageExports::default();
    };

    CapabilityPackageExports {
        child_agents: child_agent_exports(package_id, root_dir),
        resources: CapabilityPackageResources {
            bin_dir: existing_dir(root_dir.join("bin")),
            scripts_dir: existing_dir(root_dir.join("scripts")),
            references_dir: existing_dir(root_dir.join("references")),
            assets_dir: existing_dir(root_dir.join("assets")),
        },
    }
}

fn child_agent_exports(package_id: &str, root_dir: &Path) -> Vec<CapabilityChildAgentExport> {
    let agents_dir = root_dir.join("agents");
    let canonical_package_root =
        std::fs::canonicalize(root_dir).unwrap_or_else(|_| root_dir.to_path_buf());
    let Ok(canonical_agents_dir) = std::fs::canonicalize(&agents_dir) else {
        return Vec::new();
    };
    if !canonical_agents_dir.starts_with(&canonical_package_root) {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(&agents_dir) else {
        return Vec::new();
    };

    let mut roots: Vec<CapabilityChildAgentExport> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let canonical_root = std::fs::canonicalize(&path).ok()?;
            if !canonical_root.starts_with(&canonical_agents_dir)
                || !canonical_root.is_dir()
                || !looks_like_child_agent_root(&canonical_root)
            {
                return None;
            }

            let name = path.file_name()?.to_str()?.to_string();
            Some(CapabilityChildAgentExport {
                handle: CapabilityChildAgentExport::package_handle(package_id, &name),
                name,
                root_dir: canonical_root,
            })
        })
        .collect();
    roots.sort_by(|left, right| left.name.cmp(&right.name));
    roots
}

fn looks_like_child_agent_root(root_dir: &Path) -> bool {
    let root = AgentRootPaths::new(AgentRootKind::LaunchRoot, root_dir.to_path_buf());
    root.config_path.is_file()
        || root.persona_dir.is_dir()
        || root.skills_dir.is_dir()
        || root.policy_path.is_file()
}

fn existing_dir(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolved_capability_view_includes_builtin_packages() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let package_ids: Vec<_> = view
            .packages
            .iter()
            .map(|package| package.id.as_str())
            .collect();

        assert!(package_ids.contains(&"builtin:alan-memory"));
        assert!(package_ids.contains(&"builtin:alan-plan"));
        assert!(package_ids.contains(&"builtin:alan-repo-coding"));
        assert!(package_ids.contains(&"builtin:alan-shell-control"));
        assert!(package_ids.contains(&"builtin:alan-skill-creator"));
        assert!(package_ids.contains(&"builtin:alan-swebench"));
        assert!(package_ids.contains(&"builtin:alan-workspace-inspect"));
        assert!(package_ids.contains(&"builtin:alan-workspace-manager"));
    }

    #[test]
    fn resolved_capability_view_materializes_all_builtin_packages_as_directory_backed() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());

        for package in view
            .packages
            .iter()
            .filter(|package| package.scope == SkillScope::Builtin)
        {
            assert!(package.root_dir.as_ref().is_some());
            match &package.portable_skill.source {
                SkillContentSource::File(path) => assert!(path.is_file()),
                SkillContentSource::Embedded(_) => {
                    panic!("builtin package {} should be directory-backed", package.id)
                }
            }
        }
    }

    #[test]
    fn builtin_skill_creator_package_exposes_directory_backed_resources() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let package = view
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-skill-creator")
            .unwrap();

        let root_dir = package.root_dir.as_ref().unwrap();
        assert!(root_dir.join("SKILL.md").is_file());
        assert!(root_dir.join("agents/openai.yaml").is_file());
        assert!(root_dir.join("evals/evals.json").is_file());
        assert!(root_dir.join("eval-viewer/viewer.html").is_file());

        assert_eq!(
            package.exports.child_agents[0].handle,
            alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "builtin:alan-skill-creator".to_string(),
                export_name: "skill-creator".to_string(),
            }
        );
        assert_eq!(package.exports.child_agents[0].name, "skill-creator");
        assert_eq!(
            package
                .exports
                .resources
                .scripts_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
        assert_eq!(
            package
                .exports
                .resources
                .references_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
        assert_eq!(
            package
                .exports
                .resources
                .assets_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
    }

    #[test]
    fn builtin_repo_coding_package_exposes_repo_worker_resources() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let package = view
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-repo-coding")
            .unwrap();

        let root_dir = package.root_dir.as_ref().unwrap();
        assert!(root_dir.join("SKILL.md").is_file());
        assert!(root_dir.join("agents/openai.yaml").is_file());
        assert!(root_dir.join("references/delivery_contract.md").is_file());
        assert!(root_dir.join("references/evaluator_boundary.md").is_file());
        assert!(root_dir.join("evals/evals.json").is_file());
        assert!(
            root_dir
                .join("scripts/validate_delivery_contract.sh")
                .is_file()
        );
        assert!(
            root_dir
                .join("scripts/check_evaluator_boundaries.sh")
                .is_file()
        );
        assert!(root_dir.join("scripts/run_benchmark_fixture.sh").is_file());
        assert!(root_dir.join("scripts/grade_benchmark_case.sh").is_file());
        assert!(root_dir.join("evals/evaluator_cases.json").is_file());
        assert!(root_dir.join("evals/files/benchmark_cases.json").is_file());

        assert_eq!(
            package.exports.child_agents[0].handle,
            alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "builtin:alan-repo-coding".to_string(),
                export_name: "repo-worker".to_string(),
            }
        );
        assert_eq!(package.exports.child_agents[0].name, "repo-worker");
        assert_eq!(
            package
                .exports
                .resources
                .scripts_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
        assert_eq!(
            package
                .exports
                .resources
                .references_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
    }

    #[test]
    fn builtin_swebench_package_exposes_operator_resources() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let package = view
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-swebench")
            .unwrap();

        let root_dir = package.root_dir.as_ref().unwrap();
        assert!(root_dir.join("SKILL.md").is_file());
        assert!(root_dir.join("skill.yaml").is_file());
        assert!(root_dir.join("references/package.md").is_file());
        assert!(root_dir.join("evals/README.md").is_file());
        assert!(
            root_dir
                .join("bin/swebench-lite-prepare-workspaces")
                .is_file()
        );
        assert!(
            root_dir
                .join("bin/swebench-lite-materialize-subset")
                .is_file()
        );
        assert!(
            root_dir
                .join("scripts/run_swebench_full_steward_case.sh")
                .is_file()
        );
        assert!(
            root_dir
                .join("scripts/run_swebench_full_steward_subset.sh")
                .is_file()
        );
        assert!(
            root_dir
                .join("scripts/score_swebench_predictions.sh")
                .is_file()
        );
        assert!(package.exports.child_agents.is_empty());
        assert_eq!(
            package
                .exports
                .resources
                .bin_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
        assert_eq!(
            package
                .exports
                .resources
                .scripts_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
        assert_eq!(
            package
                .exports
                .resources
                .references_dir
                .as_deref()
                .map(std::path::Path::is_dir),
            Some(true)
        );
    }

    #[test]
    fn builtin_workspace_inspect_package_exposes_workspace_reader_target() {
        let view = ResolvedCapabilityView::from_package_dirs(Vec::new());
        let package = view
            .packages
            .iter()
            .find(|package| package.id == "builtin:alan-workspace-inspect")
            .unwrap();

        let root_dir = package.root_dir.as_ref().unwrap();
        assert!(root_dir.join("SKILL.md").is_file());
        assert!(root_dir.join("skill.yaml").is_file());
        assert!(
            root_dir
                .join("agents/workspace-reader/agent.toml")
                .is_file()
        );
        assert_eq!(
            package.exports.child_agents[0].handle,
            alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "builtin:alan-workspace-inspect".to_string(),
                export_name: "workspace-reader".to_string(),
            }
        );
        assert_eq!(package.exports.child_agents[0].name, "workspace-reader");
    }

    #[test]
    fn resolved_capability_view_discovers_single_skill_packages_from_overlay_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: A test skill
---

Body
"#,
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);

        assert!(
            view.packages
                .iter()
                .any(|package| package.id == "skill:test-skill")
        );
    }

    #[test]
    fn resolved_capability_view_discovers_package_exports_from_skill_root() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(skill_dir.join("scripts")).unwrap();
        std::fs::create_dir_all(skill_dir.join("references")).unwrap();
        std::fs::create_dir_all(skill_dir.join("assets")).unwrap();
        std::fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
        std::fs::write(
            skill_dir.join("agents/reviewer/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: A test skill
---

Body
"#,
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);
        let package = view
            .packages
            .iter()
            .find(|package| package.id == "skill:test-skill")
            .unwrap();
        let canonical_skill_dir = std::fs::canonicalize(&skill_dir).unwrap();

        assert_eq!(package.exports.child_agents.len(), 1);
        assert_eq!(package.exports.child_agents[0].name, "reviewer");
        assert_eq!(
            package.exports.child_agents[0].handle,
            alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:test-skill".to_string(),
                export_name: "reviewer".to_string(),
            }
        );
        assert_eq!(
            view.resolve_child_agent_target(&package.exports.child_agents[0].handle),
            Some(canonical_skill_dir.join("agents/reviewer"))
        );
        assert_eq!(
            package
                .exports
                .resources
                .scripts_dir
                .as_deref()
                .and_then(|path| std::fs::canonicalize(path).ok())
                .as_deref(),
            Some(canonical_skill_dir.join("scripts").as_path())
        );
        assert_eq!(
            package
                .exports
                .resources
                .references_dir
                .as_deref()
                .and_then(|path| std::fs::canonicalize(path).ok())
                .as_deref(),
            Some(canonical_skill_dir.join("references").as_path())
        );
        assert_eq!(
            package
                .exports
                .resources
                .assets_dir
                .as_deref()
                .and_then(|path| std::fs::canonicalize(path).ok())
                .as_deref(),
            Some(canonical_skill_dir.join("assets").as_path())
        );
    }

    #[test]
    fn resolved_capability_view_does_not_register_child_agent_skills_in_parent_scan() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("parent-skill");
        std::fs::create_dir_all(skill_dir.join("agents/reviewer/skills/child-only")).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Parent Skill
description: Parent-visible skill
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/reviewer/skills/child-only/SKILL.md"),
            r#"---
name: Child Only
description: Child-agent-only skill
---

Body
"#,
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);
        let package_ids: Vec<_> = view
            .packages
            .iter()
            .map(|package| package.id.as_str())
            .collect();
        let parent_package = view
            .packages
            .iter()
            .find(|package| package.id == "skill:parent-skill")
            .unwrap();

        assert!(package_ids.contains(&"skill:parent-skill"));
        assert!(!package_ids.contains(&"skill:child-only"));
        assert_eq!(parent_package.exports.child_agents.len(), 1);
    }

    #[test]
    fn refresh_reloads_packages_after_filesystem_changes() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir.clone(),
            scope: SkillScope::Repo,
        }]);

        let skill_dir = skills_dir.join("new-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: New Skill
description: Freshly added
---

Body
"#,
        )
        .unwrap();

        let refreshed = view.refresh();

        assert!(
            refreshed
                .packages
                .iter()
                .any(|package| package.id == "skill:new-skill")
        );
    }

    #[test]
    fn resolve_child_agent_target_prefers_highest_precedence_package_overlay() {
        let temp = TempDir::new().unwrap();
        let user_skills_dir = temp.path().join("user-skills");
        let repo_skills_dir = temp.path().join("repo-skills");

        let user_skill_dir = user_skills_dir.join("repo-review");
        std::fs::create_dir_all(user_skill_dir.join("agents/user-reviewer")).unwrap();
        std::fs::write(
            user_skill_dir.join("agents/user-reviewer/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        std::fs::write(
            user_skill_dir.join("SKILL.md"),
            r#"---
name: Repo Review
description: User overlay
---

Body
"#,
        )
        .unwrap();

        let repo_skill_dir = repo_skills_dir.join("repo-review");
        std::fs::create_dir_all(repo_skill_dir.join("agents/repo-review")).unwrap();
        std::fs::write(
            repo_skill_dir.join("agents/repo-review/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        std::fs::write(
            repo_skill_dir.join("SKILL.md"),
            r#"---
name: Repo Review
description: Repo overlay
---

Body
"#,
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![
            ScopedPackageDir {
                path: user_skills_dir,
                scope: SkillScope::User,
            },
            ScopedPackageDir {
                path: repo_skills_dir,
                scope: SkillScope::Repo,
            },
        ]);

        assert_eq!(
            view.package("skill:repo-review")
                .and_then(|package| package.root_dir.as_deref())
                .and_then(|path| std::fs::canonicalize(path).ok()),
            Some(std::fs::canonicalize(&repo_skill_dir).unwrap())
        );
        assert_eq!(
            view.resolve_child_agent_target(&alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:repo-review".to_string(),
                export_name: "repo-review".to_string(),
            }),
            Some(std::fs::canonicalize(repo_skill_dir.join("agents/repo-review")).unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolved_capability_view_rejects_child_agent_symlink_outside_package_tree() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        let external_dir = temp.path().join("external-agent");
        std::fs::create_dir_all(skill_dir.join("agents")).unwrap();
        std::fs::create_dir_all(&external_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: A test skill
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            external_dir.join("agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        symlink(&external_dir, skill_dir.join("agents/reviewer")).unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);
        let package = view.package("skill:test-skill").unwrap();

        assert!(package.exports.child_agents.is_empty());
        assert_eq!(
            view.resolve_child_agent_target(&alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:test-skill".to_string(),
                export_name: "reviewer".to_string(),
            }),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolved_capability_view_rejects_agents_dir_symlink_outside_package_tree() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        let external_agents_dir = temp.path().join("external-agents");
        let external_reviewer_dir = external_agents_dir.join("reviewer");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::create_dir_all(&external_reviewer_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: A test skill
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            external_reviewer_dir.join("agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        symlink(&external_agents_dir, skill_dir.join("agents")).unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);
        let package = view.package("skill:test-skill").unwrap();

        assert!(package.exports.child_agents.is_empty());
        assert_eq!(
            view.resolve_child_agent_target(&alan_agent_protocol::SpawnTarget::PackageChildAgent {
                package_id: "skill:test-skill".to_string(),
                export_name: "reviewer".to_string(),
            }),
            None
        );
    }

    #[test]
    fn resolved_capability_view_retains_overlaid_packages_by_id() {
        let temp = TempDir::new().unwrap();
        let user_skills_dir = temp.path().join("user-skills");
        let repo_skills_dir = temp.path().join("repo-skills");

        let user_skill_dir = user_skills_dir.join("repo-review");
        std::fs::create_dir_all(&user_skill_dir).unwrap();
        std::fs::write(
            user_skill_dir.join("SKILL.md"),
            r#"---
name: Repo Review
description: User overlay
---

Body
"#,
        )
        .unwrap();

        let repo_skill_dir = repo_skills_dir.join("repo-review");
        std::fs::create_dir_all(&repo_skill_dir).unwrap();
        std::fs::write(
            repo_skill_dir.join("SKILL.md"),
            r#"---
name: Repo Review
description: Repo overlay
---

Body
"#,
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![
            ScopedPackageDir {
                path: user_skills_dir,
                scope: SkillScope::User,
            },
            ScopedPackageDir {
                path: repo_skills_dir,
                scope: SkillScope::Repo,
            },
        ]);

        let packages: Vec<_> = view
            .packages
            .iter()
            .filter(|package| package.id == "skill:repo-review")
            .collect();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].scope, SkillScope::User);
        assert_eq!(packages[1].scope, SkillScope::Repo);
        assert_eq!(
            packages[0]
                .root_dir
                .as_deref()
                .and_then(|path| std::fs::canonicalize(path).ok()),
            Some(std::fs::canonicalize(user_skill_dir).unwrap())
        );
        assert_eq!(
            packages[1]
                .root_dir
                .as_deref()
                .and_then(|path| std::fs::canonicalize(path).ok()),
            Some(std::fs::canonicalize(repo_skill_dir).unwrap())
        );
    }

    #[test]
    fn resolved_capability_view_ignores_empty_child_agent_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let skill_dir = skills_dir.join("test-skill");
        std::fs::create_dir_all(skill_dir.join("agents/empty-export")).unwrap();
        std::fs::create_dir_all(skill_dir.join("agents/reviewer/persona")).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Test Skill
description: A test skill
---

Body
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("agents/reviewer/persona/ROLE.md"),
            "# Reviewer\n",
        )
        .unwrap();

        let view = ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
            path: skills_dir,
            scope: SkillScope::Repo,
        }]);
        let package = view.package("skill:test-skill").unwrap();

        assert_eq!(
            package
                .exports
                .child_agents
                .iter()
                .map(|export| export.name.as_str())
                .collect::<Vec<_>>(),
            vec!["reviewer"]
        );
    }
}
