use super::*;

#[test]
fn test_extract_mentions() {
    assert_eq!(
        extract_mentions("Use $test-skill for testing"),
        vec!["test-skill"]
    );

    assert!(extract_mentions("Run $my_skill please").is_empty());

    // Multiple mentions
    let mentions = extract_mentions("$skill-a and $skill-b");
    assert_eq!(mentions, vec!["skill-a", "skill-b"]);

    // With punctuation at end
    assert_eq!(extract_mentions("Use $skill-name."), vec!["skill-name"]);

    // No mentions
    assert!(extract_mentions("Plain text without mentions").is_empty());
}

#[test]
fn test_inject_skills() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "test".to_string(),
            package_id: None,
            name: "Test Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/tmp/test/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/tmp/test/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "# Instructions\n\nDo this and that.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("## Skill: Test Skill"));
    assert!(injected.contains("# Instructions"));
}

#[test]
fn test_inject_skills_conservatively_falls_back_inline_without_runtime_support() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "repo-review".to_string(),
            package_id: Some("pkg:repo-review".to_string()),
            name: "Repo Review".to_string(),
            description: "Review repository changes".to_string(),
            short_description: Some("Delegated review capability".to_string()),
            path: std::path::PathBuf::from("/tmp/repo-review/SKILL.md"),
            package_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
            resource_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/tmp/repo-review/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        },
        content: "SECRET INLINE BODY".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Repo Review".to_string(),
            description: "Review repository changes".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("### Runtime Fallback"));
    assert!(injected.contains("SECRET INLINE BODY"));
    assert!(!injected.contains("### Delegated Capability"));
}

#[test]
fn test_render_active_skill_prompt_can_render_delegated_stub_with_runtime_support() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "repo-review".to_string(),
            package_id: Some("pkg:repo-review".to_string()),
            name: "Repo Review".to_string(),
            description: "Review repository changes".to_string(),
            short_description: Some("Delegated review capability".to_string()),
            path: std::path::PathBuf::from("/tmp/repo-review/SKILL.md"),
            package_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
            resource_root: Some(std::path::PathBuf::from("/tmp/repo-review")),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/tmp/repo-review/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        },
        content: "SECRET INLINE BODY".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Repo Review".to_string(),
            description: "Review repository changes".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let envelope = ActiveSkillEnvelope::available(
        skill.metadata.clone(),
        SkillActivationReason::ExplicitMention {
            mention: "repo-review".to_string(),
        },
    );
    let rendered = render_active_skill_prompt_for_runtime(&skill, &envelope, true).rendered;

    assert!(rendered.contains("### Delegated Capability"));
    assert!(rendered.contains("invoke_delegated_skill"));
    assert!(rendered.contains("output_ref"));
    assert!(rendered.contains("namespace file at `output_ref.path`"));
    assert!(!rendered.contains("referenced child rollout/machine"));
    assert!(rendered.contains("child_run"));
    assert!(rendered.contains("\"target\": \"reviewer\""));
    assert!(!rendered.contains("SECRET INLINE BODY"));
}

#[test]
fn test_inject_skills_renders_unresolved_stub_without_full_body() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "skill-creator".to_string(),
            package_id: Some("pkg:skill-creator".to_string()),
            name: "Skill Creator".to_string(),
            description: "Create and grade skills".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/tmp/skill-creator/SKILL.md"),
            package_root: Some(std::path::PathBuf::from("/tmp/skill-creator")),
            resource_root: Some(std::path::PathBuf::from("/tmp/skill-creator")),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from(
                "/tmp/skill-creator/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Unresolved {
                reason: SkillExecutionUnresolvedReason::AmbiguousPackageShape {
                    skill_id: "skill-creator".to_string(),
                    child_agent_exports: vec![
                        "creator".to_string(),
                        "grader".to_string(),
                        "analyzer".to_string(),
                    ],
                },
            },
        },
        content: "INLINE BODY SHOULD NOT APPEAR".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Skill Creator".to_string(),
            description: "Create and grade skills".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("### Skill Execution Status"));
    assert!(injected.contains("reason: ambiguous_package_shape"));
    assert!(injected.contains("child_agent_exports: creator, grader, analyzer"));
    assert!(!injected.contains("INLINE BODY SHOULD NOT APPEAR"));
}

#[test]
fn test_build_prompt_with_skills() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "eval".to_string(),
            package_id: None,
            name: "Evaluation".to_string(),
            description: "Eval".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/tmp/eval/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/tmp/eval/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Follow these steps.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Evaluation".to_string(),
            description: "Eval".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let prompt = build_prompt_with_skills("Evaluate this", &[skill]);
    assert!(prompt.contains("Follow these steps"));
    assert!(prompt.contains("Evaluate this"));
}

#[test]
fn test_render_skills_list() {
    let skills = vec![
        SkillMetadata {
            id: "skill-a".to_string(),
            package_id: None,
            name: "Skill A".to_string(),
            description: "Does A".to_string(),
            short_description: Some("Short A".to_string()),
            path: std::path::PathBuf::from("/a/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/a/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: CompatibleSkillMetadata {
                interface: CompatibleSkillInterface {
                    display_name: Some("UI Skill A".to_string()),
                    short_description: Some("UI Short A".to_string()),
                    ..Default::default()
                },
                ..Default::default()
            },
            execution: Default::default(),
        },
        SkillMetadata {
            id: "skill-b".to_string(),
            package_id: None,
            name: "Skill B".to_string(),
            description: "Does B".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/b/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Descriptor,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/b/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        SkillMetadata {
            id: "skill-c".to_string(),
            package_id: None,
            name: "Skill C".to_string(),
            description: "Does C".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/c/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Descriptor,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/c/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: ResolvedSkillExecution::Delegate {
                target: "reviewer".to_string(),
                source: SkillExecutionResolutionSource::ExplicitMetadata,
            },
        },
    ];

    let list = render_skills_list(&skills, true).unwrap();
    assert!(list.contains("## Available Skills"));
    assert!(list.contains("- skill_id: skill-a"));
    assert!(list.contains("  name: Skill A"));
    assert!(list.contains("  description: Does A"));
    assert!(!list.contains("UI Skill A"));
    assert!(!list.contains("UI Short A"));
    assert!(list.contains("  skill_path: /a/SKILL.md"));
    assert!(list.contains("- skill_id: skill-b"));
    assert!(list.contains("  description: Does B"));
    assert!(list.contains("- skill_id: skill-c"));
    assert!(list.contains("  execution: delegate(target=reviewer)"));
    assert!(list.contains("  use: call `invoke_delegated_skill` directly"));
    assert!(list.contains("Available Skills"));
}

#[test]
fn test_render_skills_list_falls_back_to_inline_guidance_without_delegated_support() {
    let skills = vec![SkillMetadata {
        id: "skill-c".to_string(),
        package_id: None,
        name: "Skill C".to_string(),
        description: "Does C".to_string(),
        short_description: None,
        path: std::path::PathBuf::from("/c/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from("/c/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: "reviewer".to_string(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
    }];

    let list = render_skills_list(&skills, false).unwrap();
    assert!(list.contains("  skill_path: /c/SKILL.md"));
    assert!(!list.contains("invoke_delegated_skill"));
    assert!(!list.contains("execution: delegate("));
}

#[test]
fn test_render_skills_list_hides_builtin_package_paths() {
    let skills = vec![SkillMetadata {
        id: "memory".to_string(),
        package_id: Some("builtin:alan-memory".to_string()),
        name: "Memory".to_string(),
        description: "Persistent memory across Agent Processes".to_string(),
        short_description: None,
        path: std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
        ),
        package_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
        )),
        resource_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
        )),
        scope: SkillScope::Builtin,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
        )),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    }];

    let list = render_skills_list(&skills, false).unwrap();
    assert!(list.contains("  skill_source: builtin_capability_package"));
    assert!(list.contains("rely on the runtime-disclosed instructions"));
    assert!(!list.contains("skill_path:"));
    assert!(!list.contains("builtin-skill-packages"));
}

#[test]
fn test_render_skills_list_keeps_builtin_delegated_target_guidance() {
    let skills = vec![SkillMetadata {
        id: "skill-creator".to_string(),
        package_id: Some("builtin:alan-skill-creator".to_string()),
        name: "Skill Creator".to_string(),
        description: "Create or update alan skill packages".to_string(),
        short_description: None,
        path: std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
        ),
        package_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
        )),
        resource_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
        )),
        scope: SkillScope::Builtin,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
        )),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: "skill-creator".to_string(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
    }];

    let list = render_skills_list(&skills, true).unwrap();
    assert!(list.contains("  skill_source: builtin_capability_package"));
    assert!(list.contains("  execution: delegate(target=skill-creator)"));
    assert!(list.contains("invoke_delegated_skill"));
    assert!(!list.contains("skill_path:"));
    assert!(!list.contains("builtin-skill-packages"));
}

#[test]
fn test_render_skills_list_builtin_delegated_skill_degrades_without_path_leak() {
    let skills = vec![SkillMetadata {
        id: "skill-creator".to_string(),
        package_id: Some("builtin:alan-skill-creator".to_string()),
        name: "Skill Creator".to_string(),
        description: "Create or update alan skill packages".to_string(),
        short_description: None,
        path: std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
        ),
        package_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
        )),
        resource_root: Some(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator",
        )),
        scope: SkillScope::Builtin,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from(
            "/private/tmp/alan/builtin-skill-packages/0.1.0/123/skill-creator/SKILL.md",
        )),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: "skill-creator".to_string(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
    }];

    let list = render_skills_list(&skills, false).unwrap();
    assert!(list.contains("  skill_source: builtin_capability_package"));
    assert!(list.contains("this runtime cannot delegate the builtin capability directly"));
    assert!(!list.contains("invoke_delegated_skill"));
    assert!(!list.contains("skill_path:"));
    assert!(!list.contains("builtin-skill-packages"));
}

#[test]
fn test_active_skill_context_hides_builtin_package_paths() {
    let skill = Skill {
        metadata: SkillMetadata {
            id: "memory".to_string(),
            package_id: Some("builtin:alan-memory".to_string()),
            name: "Memory".to_string(),
            description: "Persistent memory across Agent Processes".to_string(),
            short_description: None,
            path: std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
            ),
            package_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
            )),
            resource_root: Some(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory",
            )),
            scope: SkillScope::Builtin,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from(
                "/private/tmp/alan/builtin-skill-packages/0.1.0/123/memory/SKILL.md",
            )),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "# Instructions\nPersist durable context.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Memory".to_string(),
            description: "Persistent memory across Agent Processes".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };
    let envelope = ActiveSkillEnvelope::available(
        skill.metadata.clone(),
        SkillActivationReason::ExplicitMention {
            mention: "memory".to_string(),
        },
    );

    let rendered = render_active_skill_prompt_for_runtime(&skill, &envelope, false).rendered;
    assert!(rendered.contains("canonical_path: builtin:memory"));
    assert!(rendered.contains("package_root: <builtin capability package>"));
    assert!(rendered.contains("resource_root: <builtin capability package>"));
    assert!(rendered.contains("Do not use tools to open builtin package files by path."));
    assert!(!rendered.contains("builtin-skill-packages"));
}

#[test]
fn test_extract_mentions_edge_cases() {
    // Empty input
    assert!(extract_mentions("").is_empty());

    // Only $ sign
    assert!(extract_mentions("$").is_empty());

    // $ at end
    assert!(extract_mentions("text $").is_empty());

    // Duplicate mentions (should dedupe)
    assert_eq!(extract_mentions("$skill-a and $skill-a"), vec!["skill-a"]);

    // Legacy underscore separator is rejected
    assert!(extract_mentions("$skill_name").is_empty());

    // Legacy dot separator is rejected
    assert!(extract_mentions("$repo.review").is_empty());
}

#[test]
fn test_extract_mentions_multiple_same_and_different() {
    // Multiple skills with duplicates in various positions
    let mentions = extract_mentions("$skill-a $skill-b $skill-a $skill-c $skill-b");
    assert_eq!(mentions, vec!["skill-a", "skill-b", "skill-c"]);
}

#[test]
fn test_extract_mentions_with_numbers() {
    assert_eq!(extract_mentions("Use $skill-123"), vec!["skill-123"]);
    assert!(extract_mentions("$test-v2.0").is_empty());
    assert_eq!(extract_mentions("Use $skill-name."), vec!["skill-name"]);
}

#[test]
fn test_inject_skills_empty() {
    let result = inject_skills(&[]);
    assert!(result.is_empty());
}
