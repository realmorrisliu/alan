use super::disclosure::{
    DisclosedSkillResource, MAX_DISCLOSED_LEVEL2_BYTES, MAX_DISCLOSED_RESOURCE_BYTES,
    MAX_DISCLOSED_RESOURCE_CHARS, MAX_DISCLOSED_RESOURCE_COUNT, PendingDisclosedSkillResource,
    SkillResourceKind, content_contains_resource_reference, declared_resource_reference_candidates,
    extract_resource_references, format_disclosed_resources, load_disclosed_text_content,
    materialize_disclosed_resources,
};
use super::*;
use std::path::PathBuf;

#[test]
fn test_inject_skills_with_resources() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::create_dir(skill_dir.join("scripts")).unwrap();
    std::fs::create_dir(skill_dir.join("references")).unwrap();
    std::fs::create_dir(skill_dir.join("assets")).unwrap();

    // Create resource files
    std::fs::write(skill_dir.join("scripts/test.sh"), "#!/bin/bash").unwrap();
    std::fs::write(skill_dir.join("references/ref.md"), "# Reference").unwrap();
    std::fs::write(skill_dir.join("assets/logo.png"), [0_u8, 159, 146, 150]).unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level3: Level3Resources {
                        assets: vec!["logo.png".to_string()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Read `references/ref.md` before running `scripts/test.sh`.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("## Skill: Test Resource Skill"));
    assert!(injected.contains("### alan Runtime Context"));
    assert!(injected.contains("### Disclosed Resources"));
    assert!(injected.contains("#### script: scripts/test.sh"));
    assert!(injected.contains("#!/bin/bash"));
    assert!(injected.contains("#### reference: references/ref.md"));
    assert!(injected.contains("# Reference"));
    assert!(!injected.contains("#### asset: assets/logo.png"));
}

#[test]
fn test_inject_skills_only_expands_declared_resources_when_level2_references_them() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::create_dir(skill_dir.join("references")).unwrap();

    std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level3: Level3Resources {
                        references: vec!["quickstart.md".to_string()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Read `quickstart.md` before using this skill.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("#### reference: references/quickstart.md"));
    assert!(injected.contains("# Quickstart"));
}

#[test]
fn test_inject_skills_matches_declared_resources_with_fragment_or_query_suffixes() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::create_dir(skill_dir.join("references")).unwrap();

    std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

    let build_skill = |content: &str| Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level3: Level3Resources {
                        references: vec!["quickstart.md".to_string()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: content.to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let fragment_injected = inject_skills(&[build_skill(
        "Read `quickstart.md#setup` before using this skill.",
    )]);
    assert!(fragment_injected.contains("#### reference: references/quickstart.md"));

    let query_injected = inject_skills(&[build_skill(
        "Read `quickstart.md?view=plain` before using this skill.",
    )]);
    assert!(query_injected.contains("#### reference: references/quickstart.md"));
}

#[test]
fn test_inject_skills_matches_prefixed_declared_resources_from_bare_references() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::create_dir(skill_dir.join("references")).unwrap();

    std::fs::write(skill_dir.join("references/quickstart.md"), "# Quickstart").unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level3: Level3Resources {
                        references: vec!["references/quickstart.md".to_string()],
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Read `quickstart.md` before using this skill.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("#### reference: references/quickstart.md"));
    assert!(injected.contains("# Quickstart"));
}

#[test]
fn test_declared_resource_reference_candidates_normalize_windows_separators() {
    let candidates = declared_resource_reference_candidates(
        SkillResourceKind::Reference,
        "guides/setup.md",
        r"references\guides\setup.md",
    );

    assert!(candidates.contains(&"references/guides/setup.md".to_string()));
    assert!(content_contains_resource_reference(
        "Read `references/guides/setup.md` before running the skill.",
        "references/guides/setup.md",
    ));
}

#[test]
fn test_inject_skills_uses_custom_level2_file() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("details.md"), "Expanded instructions.").unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level2: "details.md".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Fallback instructions.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("source: details.md"));
    assert!(injected.contains("Expanded instructions."));
    assert!(!injected.contains("Fallback instructions."));
}

#[test]
fn test_descriptor_skill_disclosure_reads_only_its_file_tree() {
    let namespace_root = std::path::PathBuf::from("/lib/pkg/example/skills/review");
    let tree = crate::ProcessFileTree::new(std::collections::BTreeMap::from([
        ("SKILL.md".to_string(), b"Fallback instructions.".to_vec()),
        (
            "details.md".to_string(),
            b"Read `references/guide.md` before reviewing.".to_vec(),
        ),
        (
            "references/guide.md".to_string(),
            b"# Descriptor Guide".to_vec(),
        ),
    ]))
    .unwrap();
    let skill = Skill {
        metadata: SkillMetadata {
            id: "review".to_string(),
            package_id: Some("example".to_string()),
            name: "Review".to_string(),
            description: "Review a change".to_string(),
            short_description: None,
            path: namespace_root.join("SKILL.md"),
            package_root: Some(namespace_root.clone()),
            resource_root: Some(namespace_root),
            scope: SkillScope::Descriptor,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level2: "details.md".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::Descriptor {
                content: std::sync::Arc::from("Fallback instructions."),
                file_tree: tree,
            },
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Fallback instructions.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Review".to_string(),
            description: "Review a change".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);

    assert!(injected.contains("source: details.md"));
    assert!(injected.contains("Read `references/guide.md` before reviewing."));
    assert!(injected.contains("#### reference: references/guide.md"));
    assert!(injected.contains("# Descriptor Guide"));
    assert!(!injected.contains("Fallback instructions."));
}

#[test]
fn test_load_disclosed_text_content_caps_large_files_by_byte_budget() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.txt");
    std::fs::write(
        &path,
        "a".repeat(MAX_DISCLOSED_RESOURCE_BYTES as usize + 1024),
    )
    .unwrap();

    let content = load_disclosed_text_content(
        &path,
        MAX_DISCLOSED_RESOURCE_BYTES,
        Some(MAX_DISCLOSED_RESOURCE_CHARS),
    )
    .unwrap();

    assert!(content.starts_with('a'));
    assert!(content.contains(&format!("exceeded {MAX_DISCLOSED_RESOURCE_BYTES} bytes")));
}

#[test]
fn test_extract_resource_references_ignores_urls_and_trims_punctuation() {
    let references = extract_resource_references(
        "Use https://example.com/scripts/setup.sh, then read references/guide.md.",
    );

    assert_eq!(references, vec!["references/guide.md"]);
}

#[test]
fn test_inject_skills_extracts_sentence_refs_and_dot_slash_paths() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::create_dir(skill_dir.join("scripts")).unwrap();
    std::fs::create_dir(skill_dir.join("references")).unwrap();
    std::fs::write(
        skill_dir.join("scripts/setup.sh"),
        "#!/bin/bash\necho setup",
    )
    .unwrap();
    std::fs::write(skill_dir.join("references/guide.md"), "# Guide").unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Use https://example.com/scripts/setup.sh, then read references/guide.md. After that run ./scripts/setup.sh."
            .to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("#### reference: references/guide.md"));
    assert!(injected.contains("#### script: scripts/setup.sh"));
    assert_eq!(injected.matches("#### script: scripts/setup.sh").count(), 1);
}

#[test]
fn test_materialize_disclosed_resources_loads_only_capped_selection() {
    let load_count = std::cell::Cell::new(0);
    let resources = (0..12).map(|index| PendingDisclosedSkillResource {
        kind: SkillResourceKind::Reference,
        display_path: format!("references/ref-{index:02}.md"),
        path: PathBuf::from(format!("/tmp/ref-{index:02}.md")),
    });

    let loaded = materialize_disclosed_resources(resources, |_| {
        load_count.set(load_count.get() + 1);
        Some("content".to_string())
    });

    assert_eq!(loaded.len(), MAX_DISCLOSED_RESOURCE_COUNT);
    assert_eq!(load_count.get(), MAX_DISCLOSED_RESOURCE_COUNT);
    assert_eq!(loaded[0].display_path, "references/ref-00.md");
    assert_eq!(loaded[7].display_path, "references/ref-07.md");
}

#[test]
fn test_inject_skills_caps_large_level2_file_by_byte_budget() {
    let temp = tempfile::tempdir().unwrap();
    let skill_dir = temp.path().join("test-skill");
    std::fs::create_dir(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("details.md"),
        "b".repeat(MAX_DISCLOSED_LEVEL2_BYTES as usize + 1024),
    )
    .unwrap();

    let skill = Skill {
        metadata: SkillMetadata {
            id: "test-res".to_string(),
            package_id: None,
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: skill_dir.join("SKILL.md"),
            package_root: Some(skill_dir.clone()),
            resource_root: Some(skill_dir.clone()),
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: Some(SkillCapabilities {
                disclosure: DisclosureConfig {
                    level2: "details.md".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            compatibility: Default::default(),
            source: SkillContentSource::File(skill_dir.join("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Fallback instructions.".to_string(),
        frontmatter: SkillFrontmatter {
            name: "Test Resource Skill".to_string(),
            description: "A test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("source: details.md"));
    assert!(injected.contains(&format!("exceeded {MAX_DISCLOSED_LEVEL2_BYTES} bytes")));
    assert!(!injected.contains("Fallback instructions."));
}

#[test]
fn test_format_disclosed_resources_uses_safe_fence_for_embedded_backticks() {
    let rendered = format_disclosed_resources(&[DisclosedSkillResource {
        kind: SkillResourceKind::Reference,
        display_path: "references/ref.md".to_string(),
        tracked_path: PromptTrackedPath::prefix_bytes(PathBuf::from("/tmp/ref.md"), 16),
        content: Some("before\n```md\ninside\n```\nafter".to_string()),
    }]);

    assert!(rendered.contains("````md"));
    assert!(rendered.contains("\n````"));
}

#[test]
fn test_inject_skills_no_parent_path() {
    // Test the edge case where skill path has no parent
    let skill = Skill {
        metadata: SkillMetadata {
            id: "no-parent".to_string(),
            package_id: None,
            name: "No Parent".to_string(),
            description: "Test".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("SKILL.md"), // No parent
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        content: "Content".to_string(),
        frontmatter: SkillFrontmatter {
            name: "No Parent".to_string(),
            description: "Test".to_string(),
            metadata: Default::default(),
            capabilities: Default::default(),
            compatibility: Default::default(),
        },
    };

    let injected = inject_skills(&[skill]);
    assert!(injected.contains("## Skill: No Parent"));
    // Should not panic and should not have Resources section
    assert!(!injected.contains("### Disclosed Resources"));
}
