use super::*;

#[tokio::test]
async fn generation_directory_context_omits_standalone_default_without_pid_binding() {
    let mock = alan_llm::MockLlmProvider::new();
    let mut state = create_test_state_with_provider(mock.clone());
    let mut tools = ToolRegistry::new();
    tools.set_default_execution_binding(
        crate::tools::ToolExecutionBinding::awaiting_host_projection(
            "/private/standalone-host-root".into(),
            "/tmp".into(),
        ),
    );
    let runner = crate::tools::ToolProcessRunner::from_registry(&tools);
    state.environment = state
        .environment
        .clone()
        .with_tool_process_context(1, runner);
    assert!(process_directory_instruction(&state.tool_execution()).is_none());
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Read the selected project")]),
        &mut |_event| async {},
        &CancellationToken::new(),
        None,
    )
    .await
    .unwrap();
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 1);
    let prompt = requests[0].system_prompt.as_deref().unwrap_or_default();
    assert!(!prompt.contains("standalone-host-root"));
    assert!(!prompt.contains("Current selected Process directory:"));
    assert_eq!(
        tools.default_cwd().unwrap(),
        std::path::PathBuf::from("/private/standalone-host-root")
    );
}

#[tokio::test]
async fn generation_directory_context_refreshes_between_tool_iterations() {
    let first = GenerationResponse {
        content: String::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: Vec::new(),
        tool_calls: vec![ToolCall {
            id: Some("probe".into()),
            name: "local_probe".into(),
            arguments: json!({}),
        }],
        usage: None,
        finish_reason: None,
        provider_response_id: None,
        provider_response_status: None,
        warnings: Vec::new(),
    };
    let second = GenerationResponse {
        content: "ok".into(),
        tool_calls: Vec::new(),
        ..first.clone()
    };
    let mock = alan_llm::MockLlmProvider::new().with_responses(vec![first, second]);
    let mut tools = ToolRegistry::new();
    tools.register(ReadCapabilityTool);
    let mut state = create_test_state_with_provider_and_tools(mock.clone(), tools).await;
    let runner = crate::tools::ToolProcessRunner::from_registry(&ToolRegistry::new());
    let bind = |path: &str| {
        runner.register_process_binding(
            1,
            crate::tools::ToolExecutionBinding::awaiting_host_projection(
                path.into(),
                "/tmp".into(),
            ),
        )
    };
    bind("/mnt/before");
    state.environment = state
        .environment
        .clone()
        .with_tool_process_context(1, runner.clone());
    run_turn_with_cancel(
        &mut state,
        TurnRunKind::NewTurn,
        Some(vec![ContentPart::text("Run local_probe")]),
        &mut |event| {
            if matches!(event, Event::ToolCallCompleted { .. }) {
                bind("/mnt/after");
            }
            async {}
        },
        &CancellationToken::new(),
        None,
    )
    .await
    .unwrap();
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0]
            .system_prompt
            .as_deref()
            .unwrap()
            .contains("Current selected Process directory: \"/mnt/before\"")
    );
    let prompt = requests[1].system_prompt.as_deref().unwrap();
    assert!(prompt.contains("Current selected Process directory: \"/mnt/after\""));
    assert!(!prompt.contains("Current selected Process directory: \"/mnt/before\""));
}

#[tokio::test]
async fn generation_directory_context_refreshes_binding_and_escapes_path_data() {
    let mock = alan_llm::MockLlmProvider::new();
    let mut state = create_test_state_with_provider(mock.clone());
    assert!(process_directory_instruction(&state.tool_execution()).is_none());
    let runner = crate::tools::ToolProcessRunner::from_registry(&ToolRegistry::new());
    state.environment = state
        .environment
        .clone()
        .with_tool_process_context(1, runner.clone());
    let paths = ["/mnt/project\n\"quoted\"😀", "/mnt/replacement"];
    for path in paths {
        runner.register_process_binding(
            1,
            crate::tools::ToolExecutionBinding::awaiting_host_projection(
                path.into(),
                "/tmp".into(),
            ),
        );
        let instruction = process_directory_instruction(&state.tool_execution()).unwrap();
        let overhead = estimate_request_prompt_overhead_tokens(None, None, Some(&instruction));
        assert!(overhead > 0);
        assert_eq!(
            estimate_pending_turn_prompt_tokens(None, None, Some(&instruction)),
            overhead
        );
        run_turn_with_cancel(
            &mut state,
            TurnRunKind::NewTurn,
            Some(vec![ContentPart::text("Read the selected project")]),
            &mut |_event| async {},
            &CancellationToken::new(),
            None,
        )
        .await
        .unwrap();
    }
    let requests = mock.recorded_requests();
    assert_eq!(requests.len(), paths.len());
    for (request, path) in requests.iter().zip(paths) {
        let prompt = request.system_prompt.as_deref().unwrap();
        let data = prompt
            .lines()
            .find_map(|line| line.strip_prefix("Current selected Process directory: "))
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(data).unwrap(), path);
        assert!(prompt.contains("context, not a grant"));
    }
    assert!(!state.machine.messages().iter().any(|message| {
        message
            .text_content()
            .contains("Current selected Process directory:")
    }));
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        runner.register_process_binding(
            1,
            crate::tools::ToolExecutionBinding::awaiting_host_projection(
                std::ffi::OsString::from_vec(b"/mnt/invalid-\xff".to_vec()).into(),
                "/tmp".into(),
            ),
        );
        assert!(process_directory_instruction(&state.tool_execution()).is_none());
    }
}

#[tokio::test]
async fn test_turn_tool_definitions_include_runtime_delegated_schema_when_supported() {
    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.prompt_cache.set_host_capabilities(
        crate::skills::SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    let tool_execution = state.tool_execution();
    let (_, tools) = turn_tool_definitions(true, &tool_execution).await.unwrap();
    assert!(
        tools
            .iter()
            .any(|tool| tool.name == "invoke_delegated_skill")
    );
}

#[tokio::test]
async fn unmounted_tool_is_not_model_callable() {
    let state = create_test_state_with_provider(ContentMockProvider::new("ok"));

    let tool_execution = state.tool_execution();
    let (_, tools) = turn_tool_definitions(false, &tool_execution).await.unwrap();
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
    let prompt = build_domain_prompt_with_skills(&mut state.prompt_cache, Some(&user_input), None);

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
    crate::prompts::ensure_definition_bootstrap_files_at(&persona_dir).unwrap();
    std::fs::write(persona_dir.join("SOUL.md"), "custom fallback persona").unwrap();

    let mut state = create_test_state_with_provider(ContentMockProvider::new("ok"));
    state.prompt_cache = prompt_cache_for_definition_root(&definition_root, vec![persona_dir]);

    let prompt = build_domain_prompt_with_skills(&mut state.prompt_cache, None, None);

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
    state.prompt_cache.set_memory_store_dir(
        state
            .core_config
            .memory
            .enabled
            .then(|| state.core_config.memory.store_dir.clone())
            .flatten(),
    );

    let prompt = build_domain_prompt_with_skills(&mut state.prompt_cache, None, None);

    assert!(!prompt.system_prompt.contains("Memory Store Bootstrap"));
    assert!(!prompt.system_prompt.contains("# User Memory"));
}
