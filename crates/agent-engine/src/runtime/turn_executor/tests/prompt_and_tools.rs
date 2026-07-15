use super::*;

#[tokio::test]
async fn test_turn_tool_definitions_include_runtime_delegated_schema_when_supported() {
    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.prompt_cache.set_host_capabilities(
        crate::skills::SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    let (_, tools) = turn_tool_definitions(&state).await.unwrap();
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "invoke_delegated_skill")
    );
}

#[tokio::test]
async fn unmounted_tool_is_not_model_callable() {
    let state = create_test_state_with_provider(ContentMockProvider::new("ok"));

    let (_, tools) = turn_tool_definitions(&state).await.unwrap();
    assert!(!tools.iter().any(|tool| tool.name == "network_probe"));
}

#[test]
fn test_build_domain_prompt_with_skills_includes_mentioned_repo_skill_instructions() {
    let temp = TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    std::fs::create_dir_all(&definition_root).unwrap();
    create_repo_skill(
        &definition_root,
        "my-skill",
        "My Skill",
        "Custom test skill",
        "# Instructions\nUse this skill when asked.",
    );

    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let user_input = vec![ContentPart::text("please use $my-skill for this task")];
    let prompt = build_domain_prompt_with_skills(&mut state, Some(&user_input), None);

    assert!(prompt.system_prompt.contains("## Available Skills"));
    assert!(
        prompt
            .system_prompt
            .contains("## Active Skill Instructions")
    );
    assert!(prompt.system_prompt.contains("## Skill: My Skill"));
    assert!(prompt.system_prompt.contains("Use this skill when asked."));
}

#[test]
fn test_build_domain_prompt_with_skills_uses_explicit_definition_persona() {
    let temp = TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let alan_dir = definition_root.join(".alan");
    let persona_dir = alan_dir.join("agents/default/persona");
    let memory_dir = alan_dir.join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    crate::prompts::ensure_definition_bootstrap_files_at(&persona_dir).unwrap();
    std::fs::write(persona_dir.join("SOUL.md"), "custom fallback persona").unwrap();

    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.core_config.memory.store_dir = Some(memory_dir);
    state.definition_persona_dirs = vec![persona_dir];
    state.prompt_cache =
        prompt_cache_for_definition_root(&definition_root, state.definition_persona_dirs.clone());

    let prompt = build_domain_prompt_with_skills(&mut state, None, None);

    assert!(prompt.system_prompt.contains("Agent Definition Persona"));
    assert!(prompt.system_prompt.contains("custom fallback persona"));
}

#[test]
fn test_build_domain_prompt_with_skills_omits_memory_bootstrap_when_memory_disabled() {
    let temp = TempDir::new().unwrap();
    let definition_root = temp.path().join("repo");
    let alan_dir = definition_root.join(".alan");
    let memory_dir = alan_dir.join("memory");
    crate::prompts::ensure_memory_store_layout_at(&memory_dir).unwrap();
    std::fs::write(memory_dir.join("USER.md"), "# User Memory\n- Morris\n").unwrap();

    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.core_config.memory.store_dir = Some(memory_dir);
    state.core_config.memory.enabled = false;
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, Vec::new());

    let prompt = build_domain_prompt_with_skills(&mut state, None, None);

    assert!(!prompt.system_prompt.contains("Memory Store Bootstrap"));
    assert!(!prompt.system_prompt.contains("# User Memory"));
}
