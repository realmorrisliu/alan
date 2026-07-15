    #[test]
    fn prompt_cache_builtin_skills_do_not_expose_materialized_temp_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();

        let host_capabilities =
            SkillHostCapabilities::with_tools(["read_file", "write_file", "edit_file", "bash"])
                .with_runtime_defaults();
        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            host_capabilities,
        );
        let prompt = cache.build(Some(&[ContentPart::text(
            "please use $memory for this task",
        )]));

        assert_eq!(prompt.active_skills.len(), 1);
        assert_eq!(prompt.active_skills[0].metadata.id, "memory");
        assert!(prompt.system_prompt.contains("## Skill: memory"));
        assert!(prompt.system_prompt.contains("skill_id: memory"));
        assert!(
            prompt
                .system_prompt
                .contains("canonical_path: builtin:memory")
        );
        assert!(
            prompt
                .system_prompt
                .contains("resource_root: <builtin capability package>")
        );
        assert!(!prompt.system_prompt.contains("builtin-skill-packages"));
        assert!(!prompt.system_prompt.contains("/private/tmp/alan"));
    }

    #[test]
    fn prompt_cache_invalidates_when_skill_changes() {
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
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            definition_root.join("skills/my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Updated instructions.
"#,
        )
        .unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Use this skill when asked."));
        assert!(second.system_prompt.contains("Updated instructions."));
        assert!(!second.skills_cache_hit);
        assert_eq!(second.metrics.skills_misses, 2);
    }

    #[test]
    fn prompt_cache_invalidates_when_skill_contents_change_with_same_length() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();

        let initial = r#"---
name: My Skill
description: Custom test skill
---

# Instructions
ABCD
"#;
        let updated = r#"---
name: My Skill
description: Custom test skill
---

# Instructions
WXYZ
"#;
        assert_eq!(initial.len(), updated.len());

        let skill_path = definition_root.join("skills/my-skill/SKILL.md");
        std::fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        std::fs::write(&skill_path, initial).unwrap();

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::write(&skill_path, updated).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("# Instructions\nABCD"));
        assert!(second.system_prompt.contains("# Instructions\nWXYZ"));
        assert!(!second.skills_cache_hit);
    }

    #[test]
    fn prompt_cache_uses_disclosure_level2_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
capabilities:
  disclosure:
    level2: details.md
---

# Instructions
Fallback instructions.
"#,
        )
        .unwrap();
        std::fs::write(skill_dir.join("details.md"), "Expanded instructions.").unwrap();

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let prompt = cache.build(Some(&user_input));

        assert!(prompt.system_prompt.contains("source: details.md"));
        assert!(prompt.system_prompt.contains("Expanded instructions."));
        assert!(!prompt.system_prompt.contains("Fallback instructions."));
    }

    #[test]
    fn prompt_cache_invalidates_when_disclosed_resource_changes() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skill_dir = definition_root.join("skills/my-skill");
        let references_dir = skill_dir.join("references");
        std::fs::create_dir_all(&references_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Read `references/guide.md` before acting.
"#,
        )
        .unwrap();

        let initial = "ALPHA";
        let updated = "OMEGA";
        assert_eq!(initial.len(), updated.len());
        std::fs::write(references_dir.join("guide.md"), initial).unwrap();

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::write(references_dir.join("guide.md"), updated).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("ALPHA"));
        assert!(second.system_prompt.contains("OMEGA"));
        assert!(!second.skills_cache_hit);
    }

    #[cfg(unix)]
    #[test]
    fn prompt_cache_invalidates_when_skill_symlink_is_retargeted() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        let skills_root = definition_root.join("skills");
        let pack_v1 = temp.path().join("pack-v1");
        let pack_v2 = temp.path().join("pack-v2");
        let linked_pack = skills_root.join("linked-pack");

        std::fs::create_dir_all(&skills_root).unwrap();
        std::fs::create_dir_all(pack_v1.join("my-skill")).unwrap();
        std::fs::create_dir_all(pack_v2.join("my-skill")).unwrap();
        std::fs::write(
            pack_v1.join("my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Version one.
"#,
        )
        .unwrap();
        std::fs::write(
            pack_v2.join("my-skill/SKILL.md"),
            r#"---
name: My Skill
description: Custom test skill
---

# Instructions
Version two.
"#,
        )
        .unwrap();
        symlink(&pack_v1, &linked_pack).unwrap();

        let mut cache = prompt_cache_for_definition_root(&definition_root, Vec::new());
        let user_input = vec![ContentPart::text("please use $my-skill for this task")];

        let first = cache.build(Some(&user_input));
        std::fs::remove_file(&linked_pack).unwrap();
        symlink(&pack_v2, &linked_pack).unwrap();
        let second = cache.build(Some(&user_input));

        assert!(first.system_prompt.contains("Version one."));
        assert!(second.system_prompt.contains("Version two."));
        assert!(!second.skills_cache_hit);
    }

    #[test]
    fn implicit_false_skills_are_mentionable_but_not_implicitly_listed() {
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

        let mut cache = prompt_cache_for_definition_root_with_overrides(
            &definition_root,
            vec![SkillOverride {
                skill_id: "my-skill".to_string(),
                enabled: Some(true),
                allow_implicit_invocation: Some(false),
            }],
            Vec::new(),
        );

        let unmentioned = vec![ContentPart::text("please help with this task")];
        let unmentioned_prompt = cache.build(Some(&unmentioned));
        assert!(unmentioned_prompt.active_skills.is_empty());
        assert!(
            !unmentioned_prompt
                .system_prompt
                .contains("- skill_id: my-skill")
        );

        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let mentioned_prompt = cache.build(Some(&mentioned));
        assert!(
            mentioned_prompt
                .system_prompt
                .contains("## Skill: My Skill")
        );
    }

    #[test]
    fn disabled_skills_are_hidden_from_catalog_and_activation() {
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

        let mut cache = prompt_cache_for_definition_root_with_overrides(
            &definition_root,
            vec![SkillOverride {
                skill_id: "my-skill".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
            Vec::new(),
        );

        let mentioned = vec![ContentPart::text("please use $my-skill for this task")];
        let prompt = cache.build(Some(&mentioned));
        assert!(!prompt.system_prompt.contains("skill_id: my-skill"));
        assert!(!prompt.system_prompt.contains("## Skill: My Skill"));
        assert!(prompt.system_prompt.contains("Skill '$my-skill' not found"));
    }

    #[test]
    fn disabled_skills_with_missing_tools_still_render_as_not_found() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        let skill_dir = definition_root.join("skills/hidden-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Hidden Helper
description: Should stay hidden
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view_and_overrides(
            capability_view_for_definition_root(&definition_root),
            vec![SkillOverride {
                skill_id: "hidden-helper".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            }],
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );

        let mentioned = vec![ContentPart::text("please use $hidden-helper for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Hidden Helper"));
        assert!(
            !prompt
                .system_prompt
                .contains("Skill '$hidden-helper' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$hidden-helper' not found")
        );
    }

    #[test]
    fn skills_with_missing_required_tools_are_reported_as_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        let skill_dir = definition_root.join("skills/tool-heavy");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Tool Heavy
description: Needs extra tools
capabilities:
  required_tools: ["missing_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text("please use $tool-heavy for this task")];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Tool Heavy"));
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$tool-heavy' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("missing dependencies: tool:missing_tool")
        );
        assert!(prompt.system_prompt.contains("Suggested next steps:"));
        assert!(
            prompt
                .system_prompt
                .contains("Enable or register the required tool: missing_tool.")
        );
    }

    #[test]
    fn skills_with_unresolved_execution_are_reported_as_unavailable() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        create_definition_skill(
            &definition_root,
            "ambiguous-helper",
            "Ambiguous Helper",
            "Creates new skills",
            "# Instructions\nUse this skill when asked.",
        );
        create_definition_child_agent(&definition_root, "ambiguous-helper", "creator");
        create_definition_child_agent(&definition_root, "ambiguous-helper", "grader");

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["bash"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text(
            "please use $ambiguous-helper for this task",
        )];
        let prompt = cache.build(Some(&mentioned));

        assert!(!prompt.system_prompt.contains("## Skill: Ambiguous Helper"));
        assert!(
            prompt
                .system_prompt
                .contains("Skill '$ambiguous-helper' is unavailable")
        );
        assert!(
            prompt
                .system_prompt
                .contains("unresolved execution: unresolved(ambiguous_package_shape)")
        );
        assert!(prompt.system_prompt.contains("Suggested next steps:"));
        assert!(
            prompt
                .system_prompt
                .contains("Fix delegated execution metadata")
        );
    }

    #[test]
    fn builtin_skill_creator_uses_directory_backed_resource_root() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        let capability_view = capability_view_for_definition_root(&definition_root);
        let host_capabilities = SkillHostCapabilities::with_tools(["bash"]).with_runtime_defaults();
        let snapshot =
            CachedSkillsRegistry::load_capability_view(&capability_view, &[], &host_capabilities)
                .unwrap();

        assert!(snapshot.mentionable_skill_ids.contains("skill-creator"));
        assert!(
            snapshot
                .listed_skills
                .iter()
                .any(|skill| skill.id == "skill-creator")
        );
        assert!(
            !snapshot
                .unavailable_skill_messages
                .contains_key("skill-creator")
        );

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view,
            Vec::new(),
            host_capabilities,
        );
        let user_input = vec![ContentPart::text("please use $skill-creator for this task")];
        let prompt = cache.build(Some(&user_input));

        let active_skill = prompt
            .active_skills
            .iter()
            .find(|skill| skill.metadata.id == "skill-creator")
            .unwrap();
        let resource_root = active_skill.metadata.resource_root.as_ref().unwrap();

        assert_eq!(active_skill.metadata.id, "skill-creator");
        assert_eq!(
            active_skill.metadata.package_id.as_deref(),
            Some("builtin:alan-skill-creator")
        );
        assert!(resource_root.join("references/authoring.md").is_file());
        assert!(resource_root.join("scripts/quick_validate.py").is_file());
        assert!(resource_root.join("agents/openai.yaml").is_file());
        assert_eq!(
            active_skill.metadata.execution,
            ResolvedSkillExecution::Delegate {
                target: "skill-creator".to_string(),
                source: crate::skills::SkillExecutionResolutionSource::ExplicitMetadata,
            }
        );
        assert!(
            prompt
                .system_prompt
                .contains("resource_root: <builtin capability package>")
        );
        assert!(
            !prompt
                .system_prompt
                .contains(&resource_root.display().to_string())
        );
    }

    #[test]
    fn prompt_cache_invalidates_when_host_capabilities_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let definition_root = temp.path().join("repo");
        std::fs::create_dir_all(&definition_root).unwrap();
        let skill_dir = definition_root.join("skills/dynamic-helper");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: Dynamic Helper
description: Needs a dynamic tool
capabilities:
  required_tools: ["custom_tool"]
---

# Instructions
Use this skill when asked.
"#,
        )
        .unwrap();

        let mut cache = PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_definition_root(&definition_root),
            Vec::new(),
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults(),
        );
        let mentioned = vec![ContentPart::text(
            "please use $dynamic-helper for this task",
        )];

        let before = cache.build(Some(&mentioned));
        assert!(
            before
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );

        let mut refreshed =
            SkillHostCapabilities::with_tools(["read_file"]).with_runtime_defaults();
        refreshed.extend_tools(["custom_tool"]);
        cache.set_host_capabilities(refreshed);

        let after = cache.build(Some(&mentioned));
        assert!(after.system_prompt.contains("## Skill: Dynamic Helper"));
        assert!(
            !after
                .system_prompt
                .contains("Skill '$dynamic-helper' is unavailable")
        );
    }
