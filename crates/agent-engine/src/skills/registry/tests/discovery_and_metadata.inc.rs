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
                    file_tree: None,
                }
            })
            .collect();
        let canonical_skill = std::fs::canonicalize(skill_path).unwrap();

        ResolvedCapabilityView {
            package_dirs: Vec::new(),
            package_roots: Vec::new(),
            packages: vec![CapabilityPackage {
                id: package_id.to_string(),
                scope: SkillScope::Descriptor,
                root_dir: Some(canonical_root),
                namespace_root: None,
                exports: CapabilityPackageExports {
                    child_agents,
                    resources: CapabilityPackageResources::default(),
                },
                portable_skill: PortableSkill {
                    path: canonical_skill.clone(),
                    source: SkillContentSource::File(canonical_skill),
                },
                dependencies: Vec::new(),
                package_sidecar: None,
                skill_sidecar: None,
                compatible_metadata: None,
            }],
            errors: Vec::new(),
            descriptor_errors: Vec::new(),
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
            scope: SkillScope::Descriptor,
        }])
        .unwrap();

        assert!(registry.has(&"repo-skill".to_string()));
        assert_eq!(
            registry.get(&"repo-skill".to_string()).unwrap().scope,
            SkillScope::Descriptor
        );
    }

    #[test]
    fn load_capability_view_rejects_duplicate_runtime_skill_ids() {
        let temp = TempDir::new().unwrap();
        let global_dir = temp.path().join("global");
        let package_dir = temp.path().join("packages");

        create_test_skill(&global_dir, "shared-skill", "Shared Skill", "From global");
        create_test_skill(
            &package_dir,
            "shared-skill",
            "Shared Skill",
            "From descriptor",
        );

        let capability_view = ResolvedCapabilityView::from_package_dirs(vec![
            ScopedPackageDir {
                path: global_dir,
                scope: SkillScope::Installed,
            },
            ScopedPackageDir {
                path: package_dir,
                scope: SkillScope::Descriptor,
            },
        ]);

        assert!(matches!(
            SkillsRegistry::load_capability_view(&capability_view, &[]),
            Err(SkillsError::DuplicateSkill(id)) if id == "shared-skill"
        ));
    }

    #[test]
    fn load_capability_view_applies_skill_overrides() {
        let capability_view = crate::skills::preinstalled_capability_view_for_tests();
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
        }])
        .unwrap();
        let ids: Vec<_> = registry
            .list_sorted()
            .into_iter()
            .filter(|skill| skill.scope == SkillScope::Descriptor)
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
