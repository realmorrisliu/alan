use super::*;
use alan_ap::InProcessTransport;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

fn write_agent_overlay(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
}

#[test]
fn test_agent_runtime_config_default() {
    let config = AgentProcessConfig::default();
    assert_eq!(config.launch_context.cwd, "/");
    assert!(config.launch_context.host_mounts.is_empty());
    assert!(config.launch_context.descriptors.is_empty());
    assert!(config.store_bindings.is_none());
    assert!(config.memory_store_backing.is_none());
}

#[test]
fn runtime_tool_binding_uses_host_mount_when_process_cwd_is_virtual() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let mut launch_context = crate::ProcessLaunchContext::root();
    launch_context.namespace.mount(
        "/mnt/source",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        alan_kernel::Access::ReadWrite,
    );
    launch_context = launch_context.with_host_mount(
        crate::HostMountGrant::new("/mnt/source", &source, alan_kernel::Access::ReadWrite).unwrap(),
    );
    let store_root = temp.path().join("system-store");
    let config = AgentProcessConfig {
        launch_context,
        store_bindings: Some(crate::AgentRuntimeStoreBindings {
            rollouts: store_root.join("rollouts"),
            checkpoints: store_root.join("checkpoints"),
            cache: store_root.join("cache"),
            tmp: store_root.join("tmp"),
            metadata: store_root.join("metadata"),
        }),
        ..AgentProcessConfig::default()
    };
    let mut tools = crate::tools::ToolRegistry::new();

    configure_runtime_tool_execution_binding(&config, &mut tools).unwrap();

    let binding = tools
        .default_execution_binding()
        .expect("an explicit Host Mount must create a runtime Tool binding");
    assert_eq!(binding.cwd, dunce::canonicalize(&source).unwrap());
    assert_eq!(binding.namespace_cwd, PathBuf::from("/mnt/source"));
    assert_eq!(config.launch_context.cwd, "/");
    assert_eq!(binding.host_mounts, config.launch_context.host_mounts);
}

#[test]
fn package_projection_alone_does_not_require_runtime_tool_binding() {
    let mut launch_context = crate::ProcessLaunchContext::root();
    launch_context.add_package_reference(
        crate::ProcessPackageReference::new(
            "example",
            "a".repeat(64),
            crate::ProcessPackageKind::Installed,
            "/lib/pkg/example",
            Vec::new(),
            alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        )
        .unwrap(),
    );
    let config = AgentProcessConfig {
        launch_context,
        store_bindings: None,
        ..AgentProcessConfig::default()
    };
    let mut tools = crate::tools::ToolRegistry::new();

    configure_runtime_tool_execution_binding(&config, &mut tools).unwrap();

    assert!(tools.default_execution_binding().is_none());
}

#[test]
fn test_agent_runtime_config_from_core_config() {
    let core_config = crate::config::Config::default();
    let runtime_config = AgentProcessConfig::from(core_config);

    assert_eq!(runtime_config.launch_context.cwd, "/");
    assert!(runtime_config.launch_context.host_mounts.is_empty());
    assert!(runtime_config.store_bindings.is_none());
}

#[test]
fn test_agent_runtime_config_clone() {
    let config = AgentProcessConfig::default();
    let cloned = config.clone();
    assert_eq!(config.launch_context.cwd, cloned.launch_context.cwd);
    assert_eq!(
        config.launch_context.host_mounts,
        cloned.launch_context.host_mounts
    );
}

#[test]
fn test_agent_runtime_config_debug() {
    let config = AgentProcessConfig::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("AgentProcessConfig"));
    assert!(debug_str.contains("launch_context"));
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
