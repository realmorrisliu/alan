
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
        }])
        .unwrap();
        let expected_agents_dir = std::fs::canonicalize(repo_skills.join("test-skill"))
            .unwrap()
            .join("agents");

        assert!(registry.tracked_paths().contains(&expected_agents_dir));
    }

    #[test]
    fn load_capability_view_does_not_track_synthetic_builtin_skill_sidecars() {
        let capability_view = crate::skills::preinstalled_capability_view_for_tests();
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
        let capability_view = crate::skills::preinstalled_capability_view_for_tests();
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
        let capability_view = crate::skills::preinstalled_capability_view_for_tests();
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
            scope: SkillScope::Descriptor,
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
            scope: SkillScope::Descriptor,
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
        registry
            .apply_capability_view(capability_view, &[])
            .unwrap();
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
