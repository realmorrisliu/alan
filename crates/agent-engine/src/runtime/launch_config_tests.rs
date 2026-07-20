use super::*;
use std::path::Path;
use tempfile::TempDir;

fn write_agent_overlay(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

#[test]
fn test_agent_runtime_config_default() {
    let config = AgentProcessConfig::default();
    assert_eq!(config.namespace_cwd, Path::new("/"));
    assert!(!config.memory_store_bound);
    assert!(config.store_bindings.is_none());
    assert!(config.memory_store_backing.is_none());
}

#[test]
fn test_agent_runtime_config_from_core_config() {
    let core_config = crate::config::Config::default();
    let runtime_config = AgentProcessConfig::from(core_config);

    assert_eq!(runtime_config.namespace_cwd, Path::new("/"));
    assert!(runtime_config.store_bindings.is_none());
}

#[test]
fn test_agent_runtime_config_clone() {
    let config = AgentProcessConfig::default();
    let cloned = config.clone();
    assert_eq!(config.namespace_cwd, cloned.namespace_cwd);
}

#[test]
fn test_agent_runtime_config_debug() {
    let config = AgentProcessConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("AgentProcessConfig"));
    assert!(debug_str.contains("namespace_cwd"));
    assert!(!debug_str.contains("workspace_id"));
}

#[test]
fn test_agent_config_with_definition_overlays_updates_unmodified_runtime_fields() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    write_agent_overlay(
        &overlay_path,
        r#"
		tool_repeat_limit = 9
		prompt_snapshot_enabled = true
		"#,
    );

    let base = AgentConfig::from(crate::Config::default());
    let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(merged.core_config.tool_repeat_limit, 9);
    assert!(merged.core_config.prompt_snapshot_enabled);
    assert_eq!(merged.runtime_config.tool_repeat_limit, 9);
    assert!(merged.runtime_config.prompt_snapshot_enabled);
}

#[test]
fn test_agent_config_with_definition_overlays_updates_unmodified_reasoning_effort() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    write_agent_overlay(
        &overlay_path,
        r#"
model_reasoning_effort = "high"
"#,
    );

    let base = AgentConfig::from(crate::Config::default());
    let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(
        merged.core_config.model_reasoning_effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
    assert_eq!(
        merged
            .runtime_config
            .request_control_intent
            .reasoning_effort,
        Some(alan_agent_protocol::ReasoningEffort::High)
    );
}

#[test]
fn test_agent_config_with_definition_overlays_preserves_runtime_overrides() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    write_agent_overlay(
        &overlay_path,
        r#"
		tool_repeat_limit = 9
		streaming_mode = "off"
		model_reasoning_effort = "high"
		"#,
    );

    let mut base = AgentConfig::from(crate::Config::default());
    base.runtime_config.tool_repeat_limit = 42;
    base.set_model_override("gpt-5-mini");
    base.set_streaming_mode_override(crate::config::StreamingMode::On);
    base.set_model_reasoning_effort_override(Some(alan_agent_protocol::ReasoningEffort::Low));

    let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(merged.core_config.openai_responses_model, "gpt-5-mini");
    assert_eq!(merged.core_config.tool_repeat_limit, 9);
    assert_eq!(
        merged.core_config.streaming_mode,
        crate::config::StreamingMode::On
    );
    assert_eq!(
        merged.core_config.model_reasoning_effort,
        Some(alan_agent_protocol::ReasoningEffort::Low)
    );
    assert_eq!(
        merged.core_config.effective_context_window_tokens(),
        crate::Config::for_openai_responses("sk-test", None, Some("gpt-5-mini"))
            .effective_context_window_tokens()
    );
    assert_eq!(merged.runtime_config.tool_repeat_limit, 42);
    assert_eq!(
        merged.runtime_config.context_window_tokens,
        crate::Config::for_openai_responses("sk-test", None, Some("gpt-5-mini"))
            .effective_context_window_tokens()
    );
    assert_eq!(
        merged.runtime_config.streaming_mode,
        crate::config::StreamingMode::On
    );
    assert_eq!(
        merged
            .runtime_config
            .request_control_intent
            .reasoning_effort,
        Some(alan_agent_protocol::ReasoningEffort::Low)
    );
}

#[test]
fn test_set_model_override_refreshes_runtime_context_window_budget() {
    let mut config = AgentConfig::from(crate::Config::for_openai_responses(
        "sk-test",
        None,
        Some("gpt-5.4"),
    ));
    assert_eq!(config.runtime_config.context_window_tokens, 1_050_000);

    config.set_model_override("gpt-5-mini");

    assert_eq!(config.core_config.effective_model(), "gpt-5-mini");
    assert_eq!(config.runtime_config.context_window_tokens, 400_000);
}

#[test]
fn test_agent_config_with_definition_overlays_preserves_marked_same_value_runtime_overrides() {
    let temp = TempDir::new().unwrap();
    let overlay_path = temp.path().join("agent.toml");
    write_agent_overlay(
        &overlay_path,
        r#"
streaming_mode = "off"
partial_stream_recovery_mode = "off"
[durability]
required = true
"#,
    );

    let mut base = AgentConfig::from(crate::Config::default());
    base.set_streaming_mode_override(crate::config::StreamingMode::Auto);
    base.set_partial_stream_recovery_mode_override(
        crate::config::PartialStreamRecoveryMode::ContinueOnce,
    );
    base.set_durability_required_override(false);

    let merged = base.with_definition_overlays(&[overlay_path]).unwrap();

    assert_eq!(
        merged.core_config.streaming_mode,
        crate::config::StreamingMode::Auto
    );
    assert_eq!(
        merged.runtime_config.streaming_mode,
        crate::config::StreamingMode::Auto
    );
    assert_eq!(
        merged.core_config.partial_stream_recovery_mode,
        crate::config::PartialStreamRecoveryMode::ContinueOnce
    );
    assert_eq!(
        merged.runtime_config.partial_stream_recovery_mode,
        crate::config::PartialStreamRecoveryMode::ContinueOnce
    );
    assert!(!merged.core_config.durability.required);
    assert!(!merged.runtime_config.durability_required);
}

#[test]
fn test_agent_runtime_config_recovery_rollout_path() {
    let temp = TempDir::new().unwrap();
    let rollout_path = temp.path().join("rollout.jsonl");

    let config = AgentProcessConfig {
        recovery_rollout_path: Some(rollout_path.clone()),
        ..Default::default()
    };

    assert_eq!(config.recovery_rollout_path, Some(rollout_path));
}
