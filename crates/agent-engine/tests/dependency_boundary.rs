//! Absence guards for the namespace-native Agent Execution Engine boundary.

fn read_runtime_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/runtime/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/runtime/{path}: {error}"))
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
    let production = source.split("\n#[cfg(test)]").next().unwrap();

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
    let source = read_runtime_source("child_agents.rs");
    for forbidden in [
        "RuntimeEventEnvelope",
        "RuntimeLivenessEnvelope",
        "event_rx",
        "liveness_rx",
        "broadcast::",
    ] {
        assert!(
            !source.contains(forbidden),
            "child path contains {forbidden}"
        );
    }
    for required in [
        "observe_process_files",
        "read_ui_activity_snapshot",
        "ui_events_offset",
        "request_events_offset",
        "action_events_offset",
    ] {
        assert!(source.contains(required), "child path missing {required}");
    }
}
