use super::*;

#[test]
fn test_load_skill_resources() {
    let temp = std::env::temp_dir().join(format!("skill_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();

    let skill_dir = temp.join("test-skill");

    // Create bin directory with files
    let bin_dir = skill_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    std::fs::write(bin_dir.join("helper"), "#!/usr/bin/env bash").unwrap();

    // Create scripts directory with files
    let scripts_dir = skill_dir.join("scripts");
    std::fs::create_dir_all(&scripts_dir).unwrap();
    std::fs::write(scripts_dir.join("helper.sh"), "#!/bin/bash").unwrap();
    std::fs::write(scripts_dir.join("tool.py"), "#!/usr/bin/env python3").unwrap();

    // Create references directory with files
    let refs_dir = skill_dir.join("references");
    std::fs::create_dir_all(&refs_dir).unwrap();
    std::fs::write(refs_dir.join("guide.md"), "# Guide").unwrap();

    // Create assets directory with files
    let assets_dir = skill_dir.join("assets");
    std::fs::create_dir_all(&assets_dir).unwrap();
    std::fs::write(assets_dir.join("template.txt"), "Template").unwrap();

    let resources = load_skill_resources(&skill_dir);

    assert_eq!(resources.bin.len(), 1);
    assert_eq!(resources.scripts.len(), 2);
    assert_eq!(resources.references.len(), 1);
    assert_eq!(resources.assets.len(), 1);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_read_reference() {
    let temp = std::env::temp_dir().join(format!("ref_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();

    let refs_dir = temp.join("references");
    std::fs::create_dir_all(&refs_dir).unwrap();
    std::fs::write(
        refs_dir.join("guide.md"),
        "# Reference Guide\n\nContent here.",
    )
    .unwrap();

    let content = read_reference(&temp, "guide.md");
    assert_eq!(
        content,
        Some("# Reference Guide\n\nContent here.".to_string())
    );

    // Non-existent reference
    let not_found = read_reference(&temp, "nonexistent.md");
    assert_eq!(not_found, None);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn test_skills_error_display() {
    let err = SkillsError::MissingField("name");
    assert!(err.to_string().contains("name"));

    let err = SkillsError::MissingFrontmatter;
    assert!(err.to_string().contains("frontmatter"));

    let err = SkillsError::NotFound("test-skill".to_string());
    assert!(err.to_string().contains("test-skill"));

    let err = SkillsError::InvalidCapabilities("bad dependency".to_string());
    assert!(err.to_string().contains("bad dependency"));
}

#[test]
fn test_skill_metadata_serde() {
    // Test serialization/deserialization of SkillMetadata
    let metadata = SkillMetadata {
        id: "test-skill".to_string(),
        package_id: None,
        name: "Test Skill".to_string(),
        description: "A test skill".to_string(),
        short_description: Some("Short".to_string()),
        path: PathBuf::from("/test/SKILL.md"),
        package_root: None,
        resource_root: None,
        scope: SkillScope::Descriptor,
        tags: vec!["tag1".to_string(), "tag2".to_string()],
        capabilities: None,
        compatibility: Default::default(),
        source: SkillContentSource::File(PathBuf::from("/test/SKILL.md")),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: Default::default(),
    };

    let json = serde_json::to_string(&metadata).unwrap();
    assert!(json.contains("test-skill"));
    assert!(json.contains("Test Skill"));

    let deserialized: SkillMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.id, metadata.id);
    assert_eq!(deserialized.name, metadata.name);
    assert_eq!(deserialized.scope, metadata.scope);
}
