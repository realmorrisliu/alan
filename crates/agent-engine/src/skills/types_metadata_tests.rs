use super::*;

#[test]
fn test_extract_frontmatter() {
    let content = r#"---
name: test-skill
description: A test skill
---

# Body content

This is the body.
"#;

    let (frontmatter, body) = extract_frontmatter(content).unwrap();
    assert!(frontmatter.contains("name: test-skill"));
    assert!(body.contains("# Body content"));
}

#[test]
fn test_extract_frontmatter_no_start_marker() {
    // Content without --- at start
    let content = "Just some content without frontmatter";
    assert!(extract_frontmatter(content).is_none());
}

#[test]
fn test_extract_frontmatter_no_end_marker() {
    // Content with start marker but no end marker
    let content = r#"---
name: test-skill
description: A test skill

# Body content"#;
    assert!(extract_frontmatter(content).is_none());
}

#[test]
fn test_name_to_id() {
    assert_eq!(name_to_id("Supplier Evaluation"), "supplier-evaluation");
    assert_eq!(name_to_id("RFQ_Generator"), "rfq-generator");
    assert_eq!(name_to_id("test skill"), "test-skill");
    assert_eq!(name_to_id("Mixed_Case-Name Here"), "mixed-case-name-here");
    assert_eq!(name_to_id("repo.review"), "repo-review");
    assert_eq!(name_to_id("Release__Check v2.0"), "release-check-v2-0");
    assert_eq!(name_to_id("UPPER CASE"), "upper-case");
    assert_eq!(name_to_id("lower case"), "lower-case");
    assert_eq!(name_to_id(""), "");
}

#[test]
fn test_canonical_skill_id_validation() {
    assert!(is_canonical_skill_id("ship-it"));
    assert!(!is_canonical_skill_id("Ship_It"));
    assert!(!is_canonical_skill_id("repo.review"));
    assert_eq!(
        validate_canonical_skill_id("repo.review"),
        Err(
            "Invalid runtime skill id `repo.review`; use canonical runtime skill id `repo-review`"
                .to_string()
        )
    );
    assert_eq!(
        validate_canonical_skill_id("  repo-review  "),
        Err("Invalid runtime skill id `  repo-review  `; use canonical runtime skill id `repo-review`".to_string())
    );
}

#[test]
fn test_skill_scope_priority() {
    assert!(SkillScope::Descriptor.priority() < SkillScope::Installed.priority());
    assert!(SkillScope::Installed.priority() < SkillScope::Builtin.priority());
    assert_eq!(SkillScope::Descriptor.priority(), 0);
    assert_eq!(SkillScope::Installed.priority(), 1);
    assert_eq!(SkillScope::Builtin.priority(), 2);
}

#[test]
fn test_skill_scope_serde() {
    // Test serialization/deserialization of SkillScope
    let descriptor = serde_json::to_string(&SkillScope::Descriptor).unwrap();
    assert_eq!(descriptor, "\"descriptor\"");

    let installed: SkillScope = serde_json::from_str("\"installed\"").unwrap();
    assert!(matches!(installed, SkillScope::Installed));

    let builtin = serde_json::to_string(&SkillScope::Builtin).unwrap();
    assert_eq!(builtin, "\"builtin\"");

    assert!(serde_json::from_str::<SkillScope>("\"repo\"").is_err());
    assert!(serde_json::from_str::<SkillScope>("\"user\"").is_err());
    assert!(serde_json::from_str::<SkillScope>("\"system\"").is_err());
}

#[test]
fn test_merge_skill_overrides_applies_latest_overlay_fields() {
    let merged = merge_skill_overrides(
        &[SkillOverride {
            skill_id: "repo-review".to_string(),
            enabled: Some(true),
            allow_implicit_invocation: Some(true),
        }],
        &[
            SkillOverride {
                skill_id: "repo-review".to_string(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            },
            SkillOverride {
                skill_id: "repo-review".to_string(),
                enabled: None,
                allow_implicit_invocation: Some(false),
            },
        ],
    );

    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged[0],
        SkillOverride {
            skill_id: "repo-review".to_string(),
            enabled: Some(false),
            allow_implicit_invocation: Some(false),
        }
    );
}

#[test]
fn test_skill_override_deserialization_rejects_legacy_alias_and_noncanonical_ids() {
    let legacy_key = toml::from_str::<SkillOverride>(
        r#"
skill_id = "repo-review"
enabled = true
"#,
    )
    .unwrap_err();
    assert!(legacy_key.to_string().contains("unknown field `skill_id`"));

    let noncanonical = toml::from_str::<SkillOverride>(
        r#"
skill = "repo.review"
enabled = true
"#,
    )
    .unwrap_err();
    assert!(
        noncanonical
            .to_string()
            .contains("canonical runtime skill id `repo-review`")
    );
}

#[test]
fn test_merge_skill_overrides_requires_exact_runtime_skill_ids() {
    let merged = merge_skill_overrides(
        &[SkillOverride {
            skill_id: "repo.review".to_string(),
            enabled: Some(true),
            allow_implicit_invocation: None,
        }],
        &[SkillOverride {
            skill_id: "repo_review".to_string(),
            enabled: None,
            allow_implicit_invocation: Some(false),
        }],
    );

    assert_eq!(merged.len(), 2);
}
