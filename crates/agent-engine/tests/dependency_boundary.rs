//! Absence guards for the namespace-native Agent Execution Engine boundary.

fn read_runtime_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/runtime/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/runtime/{path}: {error}"))
}

fn read_crate_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/{path}: {error}"))
}

fn rust_item_body<'a>(source: &'a str, marker: &str) -> &'a str {
    let item = &source[source
        .find(marker)
        .unwrap_or_else(|| panic!("find {marker}"))..];
    let open = item.find('{').unwrap_or_else(|| panic!("open {marker}"));
    let mut depth = 0_i32;
    for (index, ch) in item[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &item[open..open + index + 1];
                }
            }
            _ => {}
        }
    }
    panic!("close {marker}")
}

#[test]
fn runtime_state_and_handles_have_no_parallel_capability_or_event_authority() {
    let loop_source = read_runtime_source("agent_loop.rs");
    let state = rust_item_body(&loop_source, "pub struct RuntimeLoopState");
    assert!(state.contains("pub environment: NamespaceRuntimeEnvironment"));

    let engine_source = read_runtime_source("engine.rs");
    let handle = rust_item_body(&engine_source, "pub struct RuntimeHandle");
    let controller = rust_item_body(&engine_source, "pub struct RuntimeController");
    let spawn = rust_item_body(&engine_source, "fn spawn_with_prepared_runtime_environment");

    for (owner, source) in [
        ("RuntimeLoopState", state),
        ("RuntimeHandle", handle),
        ("RuntimeController", controller),
        ("runtime construction", spawn),
    ] {
        for forbidden in [
            "LlmProvider",
            "LlmClient",
            "ToolRegistry",
            "RuntimeEventEnvelope",
            "event_sender",
            "event_sink",
            "broadcast::",
        ] {
            assert!(
                !source.contains(forbidden),
                "{owner} must not contain {forbidden}"
            );
        }
    }
    assert!(!loop_source.contains("pub enum RuntimeEnvironment"));
    assert!(!loop_source.contains("pub environment: RuntimeEnvironment"));
    assert!(!read_runtime_source("tool_orchestrator.rs").contains("ToolExecutionTarget"));

    for forbidden in [
        "RuntimeEventEnvelope",
        "event_sender",
        "liveness_sender",
        "event_task_handle",
        "tokio::sync::broadcast",
        "Sender<Event>",
        "Receiver<Event>",
        "project_runtime_event",
    ] {
        assert!(
            !engine_source.contains(forbidden),
            "engine live source must not contain {forbidden}"
        );
    }
}

#[test]
fn namespace_environment_reaches_capabilities_only_through_files() {
    let source = read_runtime_source("agent_loop/namespace_environment.rs");
    let production = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();

    for forbidden in ["LlmProvider", "ToolRegistry"] {
        assert!(!production.contains(forbidden));
    }
    for required in [
        "root: InProcessTransport",
        "/mnt/llm/connections/{llm_connection}/clone",
        "write_agent_output",
        "machine/tape",
        "/proc/clone",
    ] {
        assert!(production.contains(required), "missing {required:?}");
    }
}

#[test]
fn child_supervision_has_no_runtime_receiver_fallback() {
    let assembly_source = read_runtime_source("child_agents.rs");
    let assembly_production = assembly_source
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    let supervisor = read_runtime_source("delegated_child_run/supervisor.rs");
    for forbidden in [
        "RuntimeEventEnvelope",
        "RuntimeLivenessEnvelope",
        "event_rx",
        "liveness_rx",
        "broadcast::",
    ] {
        assert!(
            !assembly_production.contains(forbidden) && !supervisor.contains(forbidden),
            "delegated Child Run path contains {forbidden}"
        );
    }
    for required in [
        "read_process_exit_code",
        "read_process_io_offsets",
        "read_ui_activity_snapshot",
        "ui_events_offset",
        "request_events_offset",
        "action_events_offset",
    ] {
        assert!(
            supervisor.contains(required),
            "delegated Child Run supervisor missing {required}"
        );
    }
}

#[test]
fn turn_generation_uses_only_the_file_native_namespace_boundary() {
    let executor = read_runtime_source("turn_executor.rs");
    let production = executor.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    for forbidden in [
        "generate_response_with_retry",
        "with_provider_input",
        "with_previous_response_id",
        "with_context_management_compact_threshold",
    ] {
        assert!(
            !production.contains(forbidden),
            "turn transition loop retained provider-local generation path {forbidden}"
        );
    }

    let generation = read_runtime_source("turn_executor/namespace_generation.rs");
    assert!(generation.contains("generate_with_text_events_controlled"));
    assert!(generation.contains("neutralize_namespace_capabilities"));

    let projection = read_crate_source("llm/input_projection.rs");
    for forbidden in [
        "responses_input_items",
        "chat_completions_messages",
        "anthropic_messages",
    ] {
        assert!(
            !projection.contains(forbidden),
            "Agent Execution Engine retained provider-specific projection {forbidden}"
        );
    }
}

#[test]
fn engine_does_not_assemble_alan_os() {
    let source = read_runtime_source("engine.rs");
    let production = source.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    for forbidden in [
        "ProcFs::new",
        "SrvFs::new",
        "AgentRootFs::new",
        "LlmFs::new",
        "build_root_namespace_environment",
        "spawn_root_agent_process",
    ] {
        assert!(
            !production.contains(forbidden),
            "Agent Execution Engine must not assemble Alan OS through {forbidden}"
        );
    }
    assert!(production.contains("spawn_with_namespace_environment"));

    let manifest =
        std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let normal_dependencies = manifest.split("[dev-dependencies]").next().unwrap();
    for displaced in ["alan-agentfs", "alan-llmfs", "alan-routefs"] {
        assert!(
            !normal_dependencies.contains(displaced),
            "Agent Execution Engine retained assembly dependency {displaced}"
        );
    }
}

#[test]
fn engine_has_no_host_connection_store_or_provider_factory_authority() {
    let engine = read_runtime_source("engine.rs");
    let runtime = read_runtime_source("mod.rs");
    let child = read_runtime_source("child_agents.rs");
    let source = format!("{engine}\n{runtime}\n{child}");

    for forbidden in [
        "connection_store",
        "ConnectionStoreBindings",
        "resolve_connection_profile",
        "chatgpt_auth_storage_path",
        "from_core_config_with_chatgpt_auth_storage_path",
    ] {
        assert!(
            !source.contains(forbidden),
            "Agent Execution Engine retained Host Connection authority through {forbidden}"
        );
    }
    assert!(child.contains("ensure_child_connection_is_passed"));
    assert!(child.contains("child_process_assembler()"));
    assert!(child.contains("Agent Runtime Service child assembly capability"));
}

#[test]
fn engine_does_not_own_connection_profile_metadata_or_selection() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !manifest_dir.join("src/connections.rs").exists(),
        "Connection profile metadata must be owned by Connection Service"
    );

    let public_api = read_crate_source("lib.rs");
    for forbidden in [
        "mod connections",
        "ConnectionCredential",
        "ConnectionProfile",
        "ConnectionStoreBindings",
        "ConnectionsFile",
        "ResolvedConnectionProfile",
        "SecretStore",
    ] {
        assert!(
            !public_api.contains(forbidden),
            "Agent Execution Engine still exports Connection authority through {forbidden}"
        );
    }
}
