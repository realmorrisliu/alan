use super::*;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_config_from_file() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test_config.toml");

    let toml_content = r#"
connection_profile = "openai-main"
llm_request_timeout_secs = 300
tool_timeout_secs = 60
streaming_mode = "off"
partial_stream_recovery_mode = "off"
"#;

    let mut file = std::fs::File::create(&config_path).unwrap();
    file.write_all(toml_content.as_bytes()).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.connection_profile.as_deref(), Some("openai-main"));
    assert_eq!(config.llm_request_timeout_secs, 300);
    assert_eq!(config.tool_timeout_secs, 60);
    assert_eq!(config.streaming_mode, StreamingMode::Off);
    assert_eq!(
        config.partial_stream_recovery_mode,
        PartialStreamRecoveryMode::Off
    );
}

#[test]
fn test_config_from_file_accepts_model_reasoning_effort() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test_config.toml");
    std::fs::write(
        &config_path,
        r#"
model_reasoning_effort = "high"
"#,
    )
    .unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.model_reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn test_config_from_file_rejects_retired_thinking_budget_as_unknown() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test_config.toml");
    std::fs::write(
        &config_path,
        r#"
model_reasoning_effort = "medium"
thinking_budget_tokens = 2048
"#,
    )
    .unwrap();

    let err = Config::from_file(&config_path).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("unknown field"));
    assert!(message.contains("thinking_budget_tokens"));
}

#[test]
fn test_request_control_resolver_uses_model_default_without_budget() {
    let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
    let resolved = crate::resolve_runtime_request_controls(
        &config,
        crate::provider_capabilities_for_config(&config),
        crate::RequestControlIntent::default(),
    )
    .unwrap();
    assert_eq!(resolved.reasoning_effort(), Some(ReasoningEffort::Medium));
}

#[test]
fn test_config_from_file_accepts_skill_overrides() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test_config.toml");

    std::fs::write(
        &config_path,
        r#"
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false
"#,
    )
    .unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.skill_overrides.len(), 1);
    assert_eq!(
        config
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "plan")
            .unwrap()
            .allow_implicit_invocation,
        Some(false)
    );
}

#[test]
fn test_config_from_file_rejects_legacy_skill_override_key() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("legacy-skill-override.toml");

    std::fs::write(
        &config_path,
        r#"
[[skill_overrides]]
skill_id = "plan"
allow_implicit_invocation = false
"#,
    )
    .unwrap();

    let err = Config::from_file(&config_path).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("failed to parse configuration file"));
    assert!(message.contains("skill_id"));
}

#[test]
fn test_config_from_file_rejects_noncanonical_skill_override_id() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("noncanonical-skill-override.toml");

    std::fs::write(
        &config_path,
        r#"
[[skill_overrides]]
skill = "repo.review"
allow_implicit_invocation = false
"#,
    )
    .unwrap();

    let err = Config::from_file(&config_path).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("failed to parse configuration file"));
    assert!(message.contains("repo.review"));
    assert!(message.contains("repo-review"));
}

#[test]
fn test_config_from_file_defaults_skill_overrides_when_omitted() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test_config.toml");

    std::fs::write(&config_path, "connection_profile = \"openai-main\"\n").unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.connection_profile.as_deref(), Some("openai-main"));
    assert!(config.skill_overrides.is_empty());
}

#[test]
fn test_with_definition_overlays_merges_skill_overrides_field_by_field() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    std::fs::write(
        &overlay_path,
        r#"
[[skill_overrides]]
skill = "plan"
allow_implicit_invocation = false
"#,
    )
    .unwrap();

    let base = Config {
        skill_overrides: vec![SkillOverride {
            skill_id: "plan".to_string(),
            enabled: Some(false),
            allow_implicit_invocation: None,
        }],
        ..Config::default()
    };
    let config = base
        .with_definition_overlays(std::slice::from_ref(&overlay_path))
        .unwrap();
    assert_eq!(
        config
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "plan")
            .unwrap()
            .enabled,
        Some(false)
    );
    assert_eq!(
        config
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "plan")
            .unwrap()
            .allow_implicit_invocation,
        Some(false)
    );
}

#[test]
fn test_with_definition_overlays_merges_skill_overrides_across_multiple_roots() {
    let temp = TempDir::new().unwrap();
    let first_overlay = temp.path().join("base-definition.toml");
    let second_overlay = temp.path().join("named-definition.toml");
    std::fs::write(
        &first_overlay,
        r#"
[[skill_overrides]]
skill = "release-checklist"
enabled = false
"#,
    )
    .unwrap();
    std::fs::write(
        &second_overlay,
        r#"
[[skill_overrides]]
skill = "deploy-checklist"
allow_implicit_invocation = false
"#,
    )
    .unwrap();

    let config = Config::default()
        .with_definition_overlays(&[first_overlay, second_overlay])
        .unwrap();

    assert_eq!(
        config
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "release-checklist")
            .unwrap()
            .enabled,
        Some(false)
    );
    assert_eq!(
        config
            .skill_overrides
            .iter()
            .find(|entry| entry.skill_id == "deploy-checklist")
            .unwrap()
            .allow_implicit_invocation,
        Some(false)
    );
}

#[test]
fn test_config_from_file_not_found() {
    let result = Config::from_file(Path::new("/nonexistent/path/config.toml"));
    assert!(result.is_err());
}

#[test]
fn test_config_from_file_invalid_toml() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("invalid.toml");

    std::fs::write(&config_path, "not valid toml {{").unwrap();

    let result = Config::from_file(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_builtin_launch_root_agent_configs_parse_as_agent_overlays() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let paths = [
        crate_root.join("skills/repo-coding/agents/repo-worker/agent.toml"),
        crate_root.join("skills/skill-creator/agents/skill-creator/agent.toml"),
    ];

    for path in paths {
        Config::from_file(&path)
            .unwrap_or_else(|err| panic!("failed to parse {}: {err:#}", path.display()));
    }
}

#[test]
fn test_config_from_file_rejects_deprecated_openai_key_names() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("legacy.toml");
    std::fs::write(
        &config_path,
        r#"
openai_api_key = "sk-test"
openai_model = "gpt-5"
"#,
    )
    .unwrap();

    let err = Config::from_file(&config_path).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("failed to parse configuration file"));
    assert!(message.contains(&config_path.display().to_string()));
}

#[test]
fn test_config_from_file_full() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("full_config.toml");

    let toml_content = r#"
connection_profile = "anthropic-main"
llm_request_timeout_secs = 240
tool_timeout_secs = 45
max_tool_loops = 10
tool_repeat_limit = 5
prompt_snapshot_enabled = true
prompt_snapshot_max_chars = 10000
context_window_tokens = 65536
compaction_hard_trigger_ratio = 0.75
streaming_mode = "on"
partial_stream_recovery_mode = "continue_once"

[memory]
enabled = false
strict_store = false

[durability]
required = true
"#;

    std::fs::write(&config_path, toml_content).unwrap();

    let config = Config::from_file(&config_path).unwrap();
    assert_eq!(config.connection_profile.as_deref(), Some("anthropic-main"));
    assert_eq!(config.llm_request_timeout_secs, 240);
    assert_eq!(config.tool_timeout_secs, 45);
    assert_eq!(config.max_tool_loops, Some(10));
    assert_eq!(config.tool_repeat_limit, 5);
    assert_eq!(config.context_window_tokens, Some(65_536));
    assert_eq!(config.compaction_hard_trigger_ratio, Some(0.75));
    assert!((config.effective_compaction_hard_trigger_ratio() - 0.75).abs() < f32::EPSILON);
    assert!((config.effective_compaction_soft_trigger_ratio() - 0.675).abs() < f32::EPSILON);
    assert!(config.prompt_snapshot_enabled);
    assert_eq!(config.prompt_snapshot_max_chars, 10000);
    assert_eq!(config.streaming_mode, StreamingMode::On);
    assert_eq!(
        config.partial_stream_recovery_mode,
        PartialStreamRecoveryMode::ContinueOnce
    );
    // Memory
    assert!(!config.memory.enabled);
    assert!(!config.memory.strict_store);
    assert!(config.durability.required);
}

#[test]
fn test_memory_config_default() {
    let memory = MemoryConfig::default();
    assert!(memory.enabled);
    assert!(memory.strict_store);
    assert!(memory.store_dir.is_none());
}

#[test]
fn test_effective_compaction_thresholds_default_soft_from_hard() {
    let config = Config::default();

    assert!((config.effective_compaction_hard_trigger_ratio() - 0.8).abs() < f32::EPSILON);
    assert!((config.effective_compaction_soft_trigger_ratio() - 0.72).abs() < f32::EPSILON);
}

#[test]
fn test_config_from_file_rejects_retired_compaction_threshold_as_unknown() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
connection_profile = "openai-main"
compaction_trigger_ratio = 0.8
"#,
    )
    .unwrap();

    let err = Config::from_file(&config_path).unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("unknown field"));
    assert!(message.contains("compaction_trigger_ratio"));
}

#[test]
fn test_memory_config_deserialization() {
    let toml_content = r#"
enabled = false
strict_store = false
store_dir = "/custom/path"
"#;
    let memory: MemoryConfig = toml::from_str(toml_content).unwrap();
    assert!(!memory.enabled);
    assert!(!memory.strict_store);
    assert_eq!(memory.store_dir, Some(PathBuf::from("/custom/path")));
}

#[test]
fn test_durability_config_deserialization() {
    let toml_content = r#"
[durability]
required = true
"#;
    let config: Config = toml::from_str(toml_content).unwrap();
    assert!(config.durability.required);
}

#[test]
fn test_effective_context_window_tokens_uses_explicit_override() {
    let config = Config {
        context_window_tokens: Some(42_000),
        ..Config::default()
    };

    assert_eq!(config.effective_context_window_tokens(), 42_000);
}

#[test]
fn test_effective_context_window_tokens_uses_provider_family_defaults() {
    let gemini =
        Config::for_google_gemini_generate_content("project", None, Some("gemini-2.5-pro"));
    assert_eq!(gemini.effective_context_window_tokens(), 1_048_576);

    let chatgpt = Config::for_chatgpt(None, Some("gpt-5.3-codex"));
    assert_eq!(chatgpt.effective_context_window_tokens(), 400_000);

    let anthropic = Config::for_anthropic_messages("key", None, Some("claude-3-5-sonnet-latest"));
    assert_eq!(anthropic.effective_context_window_tokens(), 200_000);

    let openai_responses = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
    assert_eq!(
        openai_responses.effective_context_window_tokens(),
        1_050_000
    );

    let openai_chat_completions =
        Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.4"));
    assert_eq!(
        openai_chat_completions.effective_context_window_tokens(),
        1_050_000
    );

    let openai_chat_completions_pro =
        Config::for_openai_chat_completions("sk-test", None, Some("gpt-5.2-pro"));
    assert_eq!(
        openai_chat_completions_pro.effective_context_window_tokens(),
        400_000
    );

    let openai_compat = Config::for_openai_chat_completions_compatible(
        "sk-test",
        None,
        Some("bailian/qwen3.5-plus-2026-02-15"),
    );
    assert_eq!(openai_compat.effective_context_window_tokens(), 1_000_000);

    let minimax =
        Config::for_openai_chat_completions_compatible("sk-test", None, Some("MiniMax-M2.5"));
    assert_eq!(minimax.effective_context_window_tokens(), 204_800);

    let glm = Config::for_openai_chat_completions_compatible("sk-test", None, Some("glm-5"));
    assert_eq!(glm.effective_context_window_tokens(), 200_000);

    let kimi = Config::for_openai_chat_completions_compatible("sk-test", None, Some("kimi-k2.5"));
    assert_eq!(kimi.effective_context_window_tokens(), 250_000);

    let deepseek =
        Config::for_openai_chat_completions_compatible("sk-test", None, Some("deepseek-reasoner"));
    assert_eq!(deepseek.effective_context_window_tokens(), 128_000);

    let unknown =
        Config::for_openai_chat_completions_compatible("sk-test", None, Some("my-custom-model"));
    assert_eq!(unknown.effective_context_window_tokens(), 32_768);
}

#[test]
fn test_with_definition_overlays_merges_model_reasoning_effort() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    std::fs::write(&overlay_path, "model_reasoning_effort = \"high\"\n").unwrap();

    let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
    let overlaid = config.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(
        crate::resolve_runtime_request_controls(
            &overlaid,
            crate::provider_capabilities_for_config(&overlaid),
            crate::RequestControlIntent::default(),
        )
        .unwrap()
        .reasoning_effort(),
        Some(ReasoningEffort::High)
    );
}

#[test]
fn test_with_definition_overlays_preserves_internal_provider_state_for_same_connection_profile() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    std::fs::write(&overlay_path, "tool_repeat_limit = 9\n").unwrap();

    let config = Config {
        connection_profile: Some("openai-main".to_string()),
        llm_provider: LlmProvider::OpenAiResponses,
        openai_responses_api_key: Some("sk-test".to_string()),
        openai_responses_model: "gpt-5.4".to_string(),
        ..Default::default()
    };

    let overlaid = config.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(overlaid.connection_profile.as_deref(), Some("openai-main"));
    assert_eq!(overlaid.llm_provider, LlmProvider::OpenAiResponses);
    assert_eq!(
        overlaid.openai_responses_api_key.as_deref(),
        Some("sk-test")
    );
    assert_eq!(overlaid.openai_responses_model, "gpt-5.4");
    assert_eq!(overlaid.tool_repeat_limit, 9);
}
