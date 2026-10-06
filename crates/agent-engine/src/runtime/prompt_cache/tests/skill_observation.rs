#[test]
fn skill_observation_uses_explicit_canonical_set_and_process_overrides() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("descriptor");
    std::fs::create_dir_all(&root).unwrap();
    create_definition_skill(&root, "explicit-only", "Display Name", "test", "body");
    create_definition_skill(&root, "disabled", "Disabled", "test", "body");
    let mut cache = prompt_cache_for_definition_root_with_overrides(
        &root,
        vec![
            SkillOverride {
                skill_id: "explicit-only".into(),
                enabled: Some(true),
                allow_implicit_invocation: Some(false),
            },
            SkillOverride {
                skill_id: "disabled".into(),
                enabled: Some(false),
                allow_implicit_invocation: None,
            },
        ],
        Vec::new(),
    );
    assert_eq!(cache.skill_observation(), (false, Vec::new()));
    cache.ensure_skills_snapshot().unwrap();
    let (known, ids) = cache.skill_observation();
    assert!(known);
    assert!(ids.contains(&"explicit-only".to_string()));
    assert!(!ids.contains(&"disabled".to_string()));
    assert!(!ids.contains(&"Display Name".to_string()));
    assert_eq!(cache.skill_observation(), (known, ids));
    assert!(
        cache
            .skills_snapshot
            .as_ref()
            .unwrap()
            .listed_skills
            .iter()
            .all(|skill| skill.id != "explicit-only")
    );
}

#[test]
fn skill_observation_filters_missing_tools_and_refreshes_owner_snapshot() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("descriptor");
    std::fs::create_dir_all(&root).unwrap();
    create_definition_skill_with_frontmatter(
        &root,
        "needs-tool",
        "name: Needs Tool\ndescription: test\ncapabilities:\n  required_tools: [custom_tool]",
        "body",
    );
    let mut cache = prompt_cache_for_definition_root(&root, Vec::new());
    cache.ensure_skills_snapshot().unwrap();
    let (known, ids) = cache.skill_observation();
    assert!(known);
    assert!(!ids.contains(&"needs-tool".to_string()));
    let mut caps = SkillHostCapabilities::default();
    caps.extend_tools(["custom_tool"]);
    cache.set_host_capabilities(caps);
    cache.ensure_skills_snapshot().unwrap();
    assert!(
        cache
            .skill_observation()
            .1
            .contains(&"needs-tool".to_string())
    );
    create_definition_skill(&root, "new-skill", "New Skill", "test", "body");
    assert!(!cache.skill_observation().1.contains(&"new-skill".to_string()));
    cache.ensure_skills_snapshot().unwrap();
    assert!(
        cache
            .skill_observation()
            .1
            .contains(&"new-skill".to_string())
    );
}
