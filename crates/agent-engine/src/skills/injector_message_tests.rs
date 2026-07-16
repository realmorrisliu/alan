use super::*;

#[test]
fn test_build_prompt_with_skills_empty() {
    let prompt = build_prompt_with_skills("Just user input", &[]);
    assert_eq!(prompt, "Just user input");
}

#[test]
fn test_render_skills_list_empty() {
    assert!(render_skills_list(&[], true).is_none());
}

#[test]
fn test_render_skill_not_found_with_similar() {
    let available = vec![
        SkillMetadata {
            id: "test-skill".to_string(),
            package_id: None,
            name: "Test Skill".to_string(),
            description: "A test".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/test/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/test/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
        SkillMetadata {
            id: "testing".to_string(),
            package_id: None,
            name: "Testing".to_string(),
            description: "Testing skill".to_string(),
            short_description: None,
            path: std::path::PathBuf::from("/testing/SKILL.md"),
            package_root: None,
            resource_root: None,
            scope: SkillScope::Installed,
            tags: vec![],
            capabilities: None,
            compatibility: Default::default(),
            source: SkillContentSource::File(std::path::PathBuf::from("/testing/SKILL.md")),
            enabled: true,
            allow_implicit_invocation: true,
            alan_metadata: Default::default(),
            compatible_metadata: Default::default(),
            execution: Default::default(),
        },
    ];

    let msg = render_skill_not_found("test", &available);
    assert!(msg.contains("Skill '$test' not found"));
    assert!(msg.contains("Did you mean:"));
    assert!(msg.contains("$test-skill"));
}

#[test]
fn test_render_skill_not_found_no_similar() {
    let available = vec![SkillMetadata {
        id: "other".to_string(),
        package_id: None,
        name: "Other".to_string(),
        description: "Other skill".to_string(),
        short_description: None,
        path: std::path::PathBuf::from("/other/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Installed,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from("/other/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    }];

    let msg = render_skill_not_found("xyz", &available);
    assert!(msg.contains("Skill '$xyz' not found"));
    assert!(msg.contains("Use `/skills` to see available skills"));
    assert!(!msg.contains("Did you mean:"));
}

#[test]
fn test_render_skill_not_found_partial_match() {
    // Test when the mention contains the skill id
    let available = vec![SkillMetadata {
        id: "rust".to_string(),
        package_id: None,
        name: "Rust".to_string(),
        description: "Rust skill".to_string(),
        short_description: None,
        path: std::path::PathBuf::from("/rust/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Installed,
        tags: vec![],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(std::path::PathBuf::from("/rust/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    }];

    // "rustacean" contains "rust"
    let msg = render_skill_not_found("rustacean", &available);
    assert!(msg.contains("Did you mean:"));
    assert!(msg.contains("$rust"));
}
