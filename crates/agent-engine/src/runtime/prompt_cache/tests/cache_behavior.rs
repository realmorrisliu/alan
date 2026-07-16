    #[test]
    fn prompt_cache_hits_on_repeated_builds() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let persona_dir = definition_root.join("persona");
        std::fs::create_dir_all(&definition_root).unwrap();
        ensure_definition_bootstrap_files_at(&persona_dir).unwrap();
        create_definition_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, vec![persona_dir]);
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Agent Definition Persona"));
        assert!(first.system_prompt.contains("## Skill: My Skill"));
        assert!(!first.skills_cache_hit);
        assert!(!first.persona_cache_hit);
        assert!(second.skills_cache_hit);
        assert!(second.persona_cache_hit);
        assert_eq!(second.metrics.builds, 2);
        assert_eq!(second.metrics.hits, 1);
    }

    #[test]
    fn prompt_cache_includes_memory_store_bootstrap_when_configured() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let persona_dir = definition_root.join("persona");
        let memory_dir = definition_root.join("memory-store");
        std::fs::create_dir_all(&definition_root).unwrap();
        ensure_definition_bootstrap_files_at(&persona_dir).unwrap();
        ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

        let mut cache = prompt_cache_for_definition_root(&definition_root, vec![persona_dir]);
        cache.set_memory_store_dir(Some(memory_dir.clone()));

        let first = cache.build(None);
        let second = cache.build(None);

        assert!(first.system_prompt.contains("Memory Store Bootstrap"));
        assert!(
            first
                .system_prompt
                .contains("Memory Store path: /memory/USER.md")
        );
        assert!(
            !first
                .system_prompt
                .contains(memory_dir.to_string_lossy().as_ref())
        );
        assert!(first.system_prompt.contains("# User Memory"));
        assert!(!first.persona_cache_hit);
        assert!(second.persona_cache_hit);
    }

    #[test]
    fn definition_persona_cache_uses_prefix_fingerprints_for_tracked_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let persona_dir = definition_root.join("persona");
        std::fs::create_dir_all(&definition_root).unwrap();
        ensure_definition_bootstrap_files_at(&persona_dir).unwrap();

        let snapshot = CachedDefinitionPersona::load(&[persona_dir]);

        assert!(!snapshot.tracked_paths.is_empty());
        assert!(snapshot.tracked_paths.iter().all(|fingerprint| {
            fingerprint.content_fingerprint_mode
                == ContentFingerprintMode::PrefixBytes(WORKSPACE_PERSONA_TRACKED_PREFIX_BYTES)
        }));
    }

    #[test]
    fn definition_memory_cache_uses_prefix_fingerprints_for_tracked_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let memory_dir = temp.path().join("memory");
        ensure_memory_store_layout_at(&memory_dir).unwrap();
        std::fs::write(
            memory_dir.join("daily/2026-04-17.md"),
            "# 2026-04-17\nappended daily note",
        )
        .unwrap();

        let snapshot = CachedMemoryStore::load(&memory_dir);

        assert!(!snapshot.tracked_paths.is_empty());
        assert!(snapshot.tracked_paths.iter().all(|fingerprint| {
            fingerprint.content_fingerprint_mode
                == ContentFingerprintMode::PrefixBytes(WORKSPACE_MEMORY_TRACKED_PREFIX_BYTES)
        }));
    }

    #[test]
    fn hash_file_contents_matches_sha256_for_large_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("large.txt");
        let content = "0123456789abcdef".repeat(16 * 1024);
        std::fs::write(&path, &content).unwrap();

        let digest = hash_file_contents(&path, None).unwrap();
        let mut expected = Sha256::new();
        expected.update(content.as_bytes());

        assert_eq!(digest, <[u8; 32]>::from(expected.finalize()));
    }

    #[test]
    fn hash_file_contents_respects_prefix_limit() {
        let temp = tempfile::TempDir::new().unwrap();
        let first = temp.path().join("first.txt");
        let second = temp.path().join("second.txt");
        std::fs::write(&first, "prefix-one-suffix-a").unwrap();
        std::fs::write(&second, "prefix-one-suffix-b").unwrap();

        assert_eq!(
            hash_file_contents(&first, Some(10)),
            hash_file_contents(&second, Some(10))
        );
        assert_ne!(
            hash_file_contents(&first, None),
            hash_file_contents(&second, None)
        );
    }

    #[test]
    fn prompt_cache_exposes_active_skill_envelopes_with_canonical_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);

        let active_skill = &prompt.active_skills[0];
        let expected_root = std::fs::canonicalize(definition_root.join("skills/my-skill")).unwrap();
        assert_eq!(active_skill.metadata.id, "my-skill");
        assert_eq!(
            active_skill.metadata.package_id.as_deref(),
            Some("skill:my-skill")
        );
        assert_eq!(active_skill.metadata.path, expected_root.join("SKILL.md"));
        assert_eq!(
            active_skill.metadata.package_root.as_deref(),
            Some(expected_root.as_path())
        );
        assert_eq!(
            active_skill.metadata.resource_root.as_deref(),
            Some(expected_root.as_path())
        );
        assert!(matches!(
            active_skill.activation_reason,
            SkillActivationReason::ExplicitMention { .. }
        ));
        assert!(prompt.system_prompt.contains(&format!(
            "canonical_path: {}",
            expected_root.join("SKILL.md").display()
        )));
        assert!(
            prompt
                .system_prompt
                .contains(&format!("resource_root: {}", expected_root.display()))
        );
        assert_eq!(
            active_skill.metadata.execution,
            ResolvedSkillExecution::Inline {
                source: SkillExecutionResolutionSource::NoChildAgentExports,
            }
        );
    }

    #[test]
    fn prompt_cache_invalidates_when_child_agent_exports_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(
            first.active_skills[0].metadata.execution,
            ResolvedSkillExecution::Inline {
                source: SkillExecutionResolutionSource::NoChildAgentExports,
            }
        );

        create_definition_child_agent(&definition_root, "my-skill", "my-skill");

        let second = cache.build(Some(&user_input));
        assert!(!second.skills_cache_hit);
        assert_eq!(
            second.active_skills[0].metadata.execution,
            ResolvedSkillExecution::Delegate {
                target: "my-skill".to_string(),
                source: SkillExecutionResolutionSource::SameNameSkillAndChildAgent,
            }
        );
        assert!(second.system_prompt.contains(
            "execution: delegate(target=my-skill, source=same_name_skill_and_child_agent)"
        ));
    }

    #[test]
    fn prompt_cache_revalidates_carried_active_skills_when_they_become_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "release-check",
            "Release Check",
            "Review risky release actions",
            "# Instructions\nUse this skill when asked.",
        );

        let host_capabilities =
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $release-check for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(first.active_skills.len(), 1);

        std::fs::write(
            definition_root.join("skills/release-check/SKILL.md"),
            r#"---
name: Release Check
description: Review risky release actions
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Do not use stale instructions.
"#,
        )
        .unwrap();

        let resumed = cache.build_with_active_skills(&first.active_skills, None);

        assert!(resumed.active_skills.is_empty());
        assert!(
            resumed
                .system_prompt
                .contains("Skill '$release-check' is unavailable")
        );
        assert!(
            resumed
                .system_prompt
                .contains("missing dependencies: tool:missing_tool")
        );
        assert!(!resumed.system_prompt.contains("## Skill: Release Check"));
        assert!(!resumed.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn prompt_cache_revalidates_carried_active_skills_when_they_disappear() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "release-check",
            "Release Check",
            "Review risky release actions",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $release-check for this task")];

        let first = cache.build(Some(&user_input));
        assert_eq!(first.active_skills.len(), 1);

        std::fs::remove_dir_all(definition_root.join("skills/release-check")).unwrap();

        let resumed = cache.build_with_active_skills(&first.active_skills, None);

        assert!(resumed.active_skills.is_empty());
        assert!(
            resumed
                .system_prompt
                .contains("Skill '$release-check' not found")
        );
        assert!(!resumed.system_prompt.contains("## Skill: Release Check"));
        assert!(!resumed.system_prompt.contains("Use this skill when asked."));
    }

    #[test]
    fn prompt_cache_renders_delegated_skill_as_stub() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_definition_child_agent(&definition_root, "repo-review", "repo-review");

        let host_capabilities = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);
        assert!(prompt.system_prompt.contains("execution: delegate("));
        assert!(prompt.system_prompt.contains("### Delegated Capability"));
        assert!(prompt.system_prompt.contains("invoke_delegated_skill"));
        assert!(!prompt.system_prompt.contains("SECRET INLINE REVIEW BODY"));
    }

    #[test]
    fn prompt_cache_falls_back_to_inline_when_delegated_invocation_is_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_definition_child_agent(&definition_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let prompt = cache.build(Some(&user_input));
        assert_eq!(prompt.active_skills.len(), 1);
        assert!(prompt.system_prompt.contains("execution: delegate("));
        assert!(prompt.system_prompt.contains("### Runtime Fallback"));
        assert!(prompt.system_prompt.contains("SECRET INLINE REVIEW BODY"));
        assert!(!prompt.system_prompt.contains("### Delegated Capability"));
    }

    #[test]
    fn prompt_cache_lists_delegated_skill_with_inline_guidance_when_runtime_lacks_delegated_support()
     {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_definition_child_agent(&definition_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let prompt = cache.build(Some(&[ContentPart::text(
            "please help with this definition",
        )]));

        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("skill_id: repo-review"));
        assert!(prompt.system_prompt.contains("skill_path: "));
        assert!(!prompt.system_prompt.contains("invoke_delegated_skill"));
    }

    #[test]
    fn prompt_cache_rebuilds_delegated_skill_when_invocation_support_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "repo-review",
            "Repo Review",
            "Review repository changes",
            "SECRET INLINE REVIEW BODY",
        );
        create_definition_child_agent(&definition_root, "repo-review", "repo-review");

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $repo-review for this task")];

        let before = cache.build(Some(&user_input));
        assert!(before.system_prompt.contains("### Runtime Fallback"));
        assert!(before.system_prompt.contains("SECRET INLINE REVIEW BODY"));

        let delegated_runtime = SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation();
        cache.set_host_capabilities(delegated_runtime);

        let after = cache.build(Some(&user_input));
        assert!(!after.skills_cache_hit);
        assert!(after.system_prompt.contains("### Delegated Capability"));
        assert!(after.system_prompt.contains("invoke_delegated_skill"));
        assert!(!after.system_prompt.contains("SECRET INLINE REVIEW BODY"));
    }

    #[test]
    fn explicit_mention_activation_reason_is_canonical() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert_eq!(prompt.active_skills.len(), 1);
        assert!(matches!(
            prompt.active_skills[0].activation_reason,
            SkillActivationReason::ExplicitMention { .. }
        ));
        assert!(
            prompt
                .system_prompt
                .contains("activation_reason: explicit_mention($my-skill)")
        );
    }

    #[test]
    fn obsolete_structured_trigger_aliases_do_not_activate_skill() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill_with_frontmatter(
            &definition_root,
            "my-skill",
            r#"name: My Skill
description: Custom test skill
capabilities:
  triggers:
    explicit: ["$Ship_It"]"#,
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $ship-it for this task")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.active_skills.is_empty());
        assert!(prompt.system_prompt.contains("Skill '$ship-it' not found"));
    }

    #[test]
    fn implicit_skill_is_listed_but_not_auto_activated() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "my-skill",
            "My Skill",
            "Custom test skill",
            "# Instructions\nUse this skill when asked.",
        );

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please help with this definition")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.active_skills.is_empty());
        assert!(prompt.system_prompt.contains("## Available Skills"));
        assert!(prompt.system_prompt.contains("skill_id: my-skill"));
        assert!(!prompt.system_prompt.contains("## Skill: My Skill"));
    }
