//! Skills framework for extending agent capabilities.
//!
//! Skills are directory-backed capability packages centered on a single
//! portable `SKILL.md`, with optional alan-native sidecars and package-local
//! launch targets.
//!
//! # Example Skill Structure
//!
//! ```text
//! my-skill/
//! ├── SKILL.md              # Required
//! ├── skill.yaml            # Optional alan-native runtime metadata
//! ├── package.yaml          # Optional package-level runtime defaults
//! ├── bin/                  # Optional: package-local executable tools
//! ├── scripts/              # Optional: executable code
//! ├── references/           # Optional: documentation
//! ├── assets/               # Optional: templates, resources
//! ├── evals/                # Optional: explicit authoring/eval manifests
//! ├── eval-viewer/          # Optional: static review/viewer assets
//! └── agents/               # Optional: package-local launch targets
//! ```
//!
//! # SKILL.md Format
//!
//! ```markdown
//! ---
//! name: skill-name
//! description: What this skill does and when to use it
//! metadata:
//!   short-description: Brief description
//!   tags: ["tag1", "tag2"]
//! ---
//!
//! # Instructions
//!
//! Step-by-step guidance for the agent...
//! ```
//!
//! Discovery is filesystem-based and deterministic. Runtime skill ids are
//! normalized lower-case hyphenated slugs derived from the package directory
//! name, while `SKILL.md` stays canonical for triggers, availability, and
//! instructions. Runtime exposure is resolved per skill through `enabled` and
//! `allow_implicit_invocation`. Delegated skills render lightweight
//! parent-runtime stubs and execute through package-local launch targets when
//! the runtime supports `invoke_delegated_skill`.

mod capability_view;
mod injector;
mod loader;
pub mod registry;
pub mod types;

pub use injector::*;
pub use loader::*;
pub use registry::SkillsRegistry;
pub use types::*;

// ============================================================================
// Built-in package assets
// ============================================================================

use include_dir::{Dir, DirEntry};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub(crate) const BUILTIN_MEMORY_PACKAGE_ID: &str = "builtin:alan-memory";
pub(crate) const BUILTIN_PLAN_PACKAGE_ID: &str = "builtin:alan-plan";
pub(crate) const BUILTIN_REPO_CODING_PACKAGE_ID: &str = "builtin:alan-repo-coding";
pub(crate) const BUILTIN_SHELL_CONTROL_PACKAGE_ID: &str = "builtin:alan-shell-control";
pub(crate) const BUILTIN_SKILL_CREATOR_PACKAGE_ID: &str = "builtin:alan-skill-creator";
pub(crate) const BUILTIN_SWEBENCH_PACKAGE_ID: &str = "builtin:alan-swebench";

static MEMORY_PACKAGE_DIR: Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/memory");
static PLAN_PACKAGE_DIR: Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/plan");
static REPO_CODING_PACKAGE_DIR: Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/repo-coding");
static SHELL_CONTROL_PACKAGE_DIR: Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/alan-shell-control");
static SKILL_CREATOR_PACKAGE_DIR: Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/skill-creator");
static SWEBENCH_PACKAGE_DIR: Dir<'_> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/skills/swebench");

#[derive(Clone, Copy)]
pub(crate) struct BuiltinPackageAsset {
    pub package_id: &'static str,
    pub skill_label: &'static str,
    pub dir: &'static Dir<'static>,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedBuiltinPackage {
    pub root_dir: PathBuf,
}

/// Embedded first-party Skill tree offered to Package Service for deterministic seeding.
#[derive(Debug, Clone)]
pub struct PreinstalledSkillPackageSource {
    pub package_id: String,
    pub root_dir: PathBuf,
}

/// Materialize the product's first-party Skill trees for Package Service import.
///
/// Returning source trees does not add them to any Agent capability view. The
/// caller must seed and explicitly reference them through Package Service.
pub fn preinstalled_skill_package_sources() -> Vec<PreinstalledSkillPackageSource> {
    BUILTIN_PACKAGE_ASSETS
        .iter()
        .map(|asset| {
            let materialized = materialized_builtin_package(asset);
            PreinstalledSkillPackageSource {
                package_id: asset
                    .package_id
                    .strip_prefix("builtin:")
                    .unwrap_or(asset.package_id)
                    .to_string(),
                root_dir: materialized.root_dir,
            }
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn preinstalled_package_roots_for_tests() -> Vec<ScopedPackageRoot> {
    preinstalled_skill_package_sources()
        .into_iter()
        .map(|source| ScopedPackageRoot {
            package_id: format!("builtin:{}", source.package_id),
            path: source.root_dir,
            scope: SkillScope::Builtin,
            dependencies: Vec::new(),
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn preinstalled_capability_view_for_tests() -> ResolvedCapabilityView {
    ResolvedCapabilityView::from_package_sources(Vec::new(), preinstalled_package_roots_for_tests())
}

static MATERIALIZED_BUILTIN_PACKAGES: OnceLock<HashMap<&'static str, MaterializedBuiltinPackage>> =
    OnceLock::new();

pub(crate) const BUILTIN_PACKAGE_ASSETS: [BuiltinPackageAsset; 6] = [
    BuiltinPackageAsset {
        package_id: BUILTIN_MEMORY_PACKAGE_ID,
        skill_label: "memory",
        dir: &MEMORY_PACKAGE_DIR,
    },
    BuiltinPackageAsset {
        package_id: BUILTIN_PLAN_PACKAGE_ID,
        skill_label: "plan",
        dir: &PLAN_PACKAGE_DIR,
    },
    BuiltinPackageAsset {
        package_id: BUILTIN_REPO_CODING_PACKAGE_ID,
        skill_label: "repo-coding",
        dir: &REPO_CODING_PACKAGE_DIR,
    },
    BuiltinPackageAsset {
        package_id: BUILTIN_SHELL_CONTROL_PACKAGE_ID,
        skill_label: "alan-shell-control",
        dir: &SHELL_CONTROL_PACKAGE_DIR,
    },
    BuiltinPackageAsset {
        package_id: BUILTIN_SKILL_CREATOR_PACKAGE_ID,
        skill_label: "skill-creator",
        dir: &SKILL_CREATOR_PACKAGE_DIR,
    },
    BuiltinPackageAsset {
        package_id: BUILTIN_SWEBENCH_PACKAGE_ID,
        skill_label: "swebench",
        dir: &SWEBENCH_PACKAGE_DIR,
    },
];

pub(crate) fn materialized_builtin_package(
    asset: &BuiltinPackageAsset,
) -> MaterializedBuiltinPackage {
    MATERIALIZED_BUILTIN_PACKAGES
        .get_or_init(materialize_builtin_packages)
        .get(asset.package_id)
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "builtin package `{}` did not materialize into a directory-backed package view",
                asset.package_id
            )
        })
}

fn materialize_builtin_packages() -> HashMap<&'static str, MaterializedBuiltinPackage> {
    let mut packages = HashMap::new();
    let base_dir = std::env::temp_dir()
        .join("alan")
        .join("builtin-skill-packages")
        .join(env!("CARGO_PKG_VERSION"))
        .join(std::process::id().to_string());

    for asset in BUILTIN_PACKAGE_ASSETS {
        let root_dir = base_dir.join(asset.skill_label);
        materialize_builtin_package_dir(asset.dir, &root_dir).unwrap_or_else(|err| {
            panic!(
                "failed to materialize builtin skill package `{}` at {}: {err}",
                asset.package_id,
                root_dir.display()
            )
        });
        let canonical_root = std::fs::canonicalize(&root_dir).unwrap_or_else(|_| root_dir.clone());
        packages.insert(
            asset.package_id,
            MaterializedBuiltinPackage {
                root_dir: canonical_root,
            },
        );
    }

    packages
}

fn materialize_builtin_package_dir(
    dir: &Dir<'static>,
    destination_root: &Path,
) -> std::io::Result<()> {
    match std::fs::remove_dir_all(destination_root) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    std::fs::create_dir_all(destination_root)?;
    write_embedded_dir_entries(dir.path(), dir.entries(), destination_root)
}

fn write_embedded_dir_entries(
    base_path: &Path,
    entries: &[DirEntry<'static>],
    destination_root: &Path,
) -> std::io::Result<()> {
    for entry in entries {
        match entry {
            DirEntry::Dir(dir) => {
                let relative = dir
                    .path()
                    .strip_prefix(base_path)
                    .unwrap_or_else(|_| dir.path());
                if relative.components().next().is_some_and(|component| {
                    component.as_os_str() == std::ffi::OsStr::new("tooling")
                }) {
                    continue;
                }
                let target_dir = destination_root.join(relative);
                std::fs::create_dir_all(&target_dir)?;
                write_embedded_dir_entries(base_path, dir.entries(), destination_root)?;
            }
            DirEntry::File(file) => {
                let relative = file
                    .path()
                    .strip_prefix(base_path)
                    .unwrap_or_else(|_| file.path());
                if relative.components().next().is_some_and(|component| {
                    component.as_os_str() == std::ffi::OsStr::new("tooling")
                }) {
                    continue;
                }
                let target_file = destination_root.join(relative);
                if let Some(parent) = target_file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target_file, file.contents())?;
                set_builtin_file_permissions(&target_file)?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn set_builtin_file_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let in_scripts_dir = path
        .components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("scripts"));
    let in_bin_dir = path
        .components()
        .any(|component| component.as_os_str() == std::ffi::OsStr::new("bin"));
    if !in_scripts_dir && !in_bin_dir {
        return Ok(());
    }

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_builtin_file_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

/// List resolved skills in a user-friendly format.
pub fn list_skills(registry: &SkillsRegistry, host_capabilities: &SkillHostCapabilities) -> String {
    let skills: Vec<_> = registry.list_sorted();
    if skills.is_empty() {
        return "No skills found.\n".to_string();
    }

    let mut available = Vec::new();
    let mut unavailable = Vec::new();
    let mut disabled = Vec::new();
    for skill in skills {
        if !skill.enabled {
            disabled.push(skill);
            continue;
        }
        let issues = skill_availability_issues(skill, host_capabilities);
        if issues.is_empty() {
            available.push(skill);
        } else {
            unavailable.push((skill, issues));
        }
    }

    let mut lines = vec![
        "Available Skills".to_string(),
        "================".to_string(),
        String::new(),
    ];

    for skill in available {
        let scope_str = match skill.scope {
            types::SkillScope::Descriptor => "[descriptor]",
            types::SkillScope::Installed => "[installed]",
            types::SkillScope::Builtin => "[builtin]",
        };

        lines.push(format!(
            "{} ${} - {}",
            scope_str,
            skill.id,
            skill.display_name()
        ));

        let desc = skill
            .effective_short_description()
            .unwrap_or(&skill.description);
        lines.push(format!("         {}", desc));
        lines.extend(render_skill_execution_lines(skill));
        lines.push(String::new());
    }

    if !unavailable.is_empty() {
        lines.extend([
            "Unavailable Skills".to_string(),
            "==================".to_string(),
            String::new(),
        ]);

        for (skill, issues) in unavailable {
            let scope_str = match skill.scope {
                types::SkillScope::Descriptor => "[descriptor]",
                types::SkillScope::Installed => "[installed]",
                types::SkillScope::Builtin => "[builtin]",
            };
            lines.push(format!(
                "{} ${} - {}",
                scope_str,
                skill.id,
                skill.display_name()
            ));
            let desc = skill
                .effective_short_description()
                .unwrap_or(&skill.description);
            lines.push(format!("         {}", desc));
            lines.extend(render_skill_execution_lines(skill));
            lines.push(format!(
                "         unavailable: {}",
                format_skill_availability_issues(&issues)
            ));
            lines.push(String::new());
        }
    }

    if !disabled.is_empty() {
        lines.extend([
            "Disabled Skills".to_string(),
            "===============".to_string(),
            String::new(),
        ]);

        for skill in disabled {
            let scope_str = match skill.scope {
                types::SkillScope::Descriptor => "[descriptor]",
                types::SkillScope::Installed => "[installed]",
                types::SkillScope::Builtin => "[builtin]",
            };
            lines.push(format!(
                "{} ${} - {}",
                scope_str,
                skill.id,
                skill.display_name()
            ));
            let desc = skill
                .effective_short_description()
                .unwrap_or(&skill.description);
            lines.push(format!("         {}", desc));
            lines.extend(render_skill_execution_lines(skill));
            lines.push("         disabled: true".to_string());
            lines.push(String::new());
        }
    }

    lines.join("\n")
}

fn render_skill_execution_lines(skill: &SkillMetadata) -> Vec<String> {
    let mut lines = vec![format!(
        "         execution: {}",
        skill.execution.render_label()
    )];
    if let Some(diagnostic) = render_skill_execution_diagnostic(&skill.execution) {
        lines.push(format!("         diagnostic: {diagnostic}"));
    }
    lines
}

fn render_skill_execution_diagnostic(execution: &ResolvedSkillExecution) -> Option<String> {
    match execution {
        ResolvedSkillExecution::Unresolved { reason } => match reason {
            SkillExecutionUnresolvedReason::NotResolved => None,
            SkillExecutionUnresolvedReason::MissingChildAgentExports => Some(
                "delegated execution was requested but the package exports no launch targets"
                    .to_string(),
            ),
            SkillExecutionUnresolvedReason::DelegateTargetNotFound {
                target,
                available_targets,
            } => Some(format!(
                "delegate target '{target}' was not found (available: {})",
                render_csv_or_none(available_targets)
            )),
            SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                skill_id,
                child_agent_exports,
            } => Some(format!(
                "ambiguous package shape; skill={skill_id}; launch targets={}",
                render_csv_or_none(child_agent_exports)
            )),
        },
        _ => None,
    }
}

fn render_csv_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_list_skills() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        std::fs::create_dir_all(&repo_skills).unwrap();

        // Create a test skill
        let skill_dir = repo_skills.join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let mut file = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        writeln!(
            file,
            r#"---
name: Test Skill
description: A test skill for testing
metadata:
  short-description: Short desc
---

Body
"#
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Descriptor,
        }])
        .unwrap();
        let output = list_skills(&registry, &SkillHostCapabilities::default());

        assert!(output.contains("Available Skills"));
        assert!(output.contains("test-skill"));
        assert!(output.contains("[descriptor]"));
        assert!(output.contains("Short desc"));
        assert!(output.contains("execution: inline(no_child_agent_exports)"));
    }

    #[test]
    fn test_list_skills_empty_registry() {
        let registry = SkillsRegistry::default();
        let output = list_skills(&registry, &SkillHostCapabilities::default());
        assert!(output.contains("No skills found"));
    }

    #[test]
    fn test_list_skills_surfaces_delegated_execution_and_diagnostics() {
        let temp = TempDir::new().unwrap();
        let repo_skills = temp.path().join("skills");
        std::fs::create_dir_all(&repo_skills).unwrap();

        let delegated_skill_dir = repo_skills.join("repo-review");
        std::fs::create_dir_all(delegated_skill_dir.join("agents/repo-review")).unwrap();
        std::fs::write(
            delegated_skill_dir.join("agents/repo-review/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        let mut delegated_skill =
            std::fs::File::create(delegated_skill_dir.join("SKILL.md")).unwrap();
        writeln!(
            delegated_skill,
            r#"---
name: Repo Review
description: Review the repo
---

Body
"#
        )
        .unwrap();

        let ambiguous_skill_dir = repo_skills.join("skill-creator");
        std::fs::create_dir_all(ambiguous_skill_dir.join("agents/creator")).unwrap();
        std::fs::create_dir_all(ambiguous_skill_dir.join("agents/grader")).unwrap();
        std::fs::write(
            ambiguous_skill_dir.join("agents/creator/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        std::fs::write(
            ambiguous_skill_dir.join("agents/grader/agent.toml"),
            "openai_responses_model = \"gpt-5.4\"\n",
        )
        .unwrap();
        let mut ambiguous_skill =
            std::fs::File::create(ambiguous_skill_dir.join("SKILL.md")).unwrap();
        writeln!(
            ambiguous_skill,
            r#"---
name: Skill Creator
description: Create skills
---

Body
"#
        )
        .unwrap();

        let registry = SkillsRegistry::load_package_dirs(&[ScopedPackageDir {
            path: repo_skills,
            scope: SkillScope::Descriptor,
        }])
        .unwrap();
        let output = list_skills(&registry, &SkillHostCapabilities::default());

        assert!(output.contains(
            "execution: delegate(target=repo-review, source=same_name_skill_and_child_agent)"
        ));
        assert!(output.contains("execution: unresolved(ambiguous_package_shape)"));
        assert!(output.contains(
            "diagnostic: ambiguous package shape; skill=skill-creator; launch targets=creator, grader"
        ));
    }

    #[test]
    fn test_list_skills_keeps_enabled_non_implicit_skills_visible() {
        let capability_view = preinstalled_capability_view_for_tests();
        let registry = SkillsRegistry::load_capability_view(
            &capability_view,
            &[SkillOverride {
                skill_id: "memory".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: Some(false),
            }],
        )
        .unwrap();
        let output = list_skills(&registry, &SkillHostCapabilities::default());

        assert!(output.contains("$memory"));
        assert!(output.contains("$plan"));
    }

    #[test]
    fn test_list_skills_surfaces_disabled_skills_for_operator_visibility() {
        let capability_view = preinstalled_capability_view_for_tests();
        let registry = SkillsRegistry::load_capability_view(
            &capability_view,
            &[SkillOverride {
                skill_id: "memory".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
        )
        .unwrap();
        let output = list_skills(&registry, &SkillHostCapabilities::default());

        assert!(output.contains("Disabled Skills"));
        assert!(output.contains("$memory"));
        assert!(output.contains("disabled: true"));
    }

    #[test]
    fn test_skill_load_outcome_is_empty() {
        let mut outcome = SkillLoadOutcome::default();
        assert!(outcome.is_empty());

        outcome.skills.push(SkillMetadata {
            id: "test".to_string(),
            package_id: None,
            name: "Test".to_string(),
            description: "Test".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/test"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Descriptor,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/test")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        });
        assert!(!outcome.is_empty());
    }

    #[test]
    fn test_skill_error_display() {
        let error = SkillError {
            path: std::path::PathBuf::from("/test/skill.md"),
            message: "Test error message".to_string(),
        };
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Test error message"));
        assert!(debug_str.contains("/test/skill.md"));
    }

    #[test]
    fn test_builtin_package_assets_do_not_use_legacy_always_active() {
        fn assert_dir_does_not_contain(dir: &Dir<'static>, needle: &str) {
            for entry in dir.entries() {
                match entry {
                    DirEntry::Dir(dir) => assert_dir_does_not_contain(dir, needle),
                    DirEntry::File(file) => {
                        let contents = file.contents_utf8().unwrap_or_default();
                        assert!(
                            !contents.contains(needle),
                            "{} contains legacy marker {needle}",
                            file.path().display()
                        );
                    }
                }
            }
        }

        for asset in BUILTIN_PACKAGE_ASSETS {
            assert_dir_does_not_contain(asset.dir, "always_active");
        }
    }
}
