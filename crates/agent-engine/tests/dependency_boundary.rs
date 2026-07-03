//! Dependency boundary for `refactor-engine-namespace-native` D1/D2/D4.
//!
//! The namespace-native environment is the durable engine shape: the live state
//! owns one namespace environment handle, and the agent-loop-owned namespace
//! environment reaches LLM and agent state through aP file paths. Legacy
//! provider/tool environment variants and accessors must not remain in the
//! engine runtime path.

fn read_runtime_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/runtime/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/runtime/{path}: {error}"))
}

fn rust_item_body<'a>(source: &'a str, marker: &str) -> &'a str {
    let marker_index = source
        .find(marker)
        .unwrap_or_else(|| panic!("find {marker}"));
    let item = &source[marker_index..];
    let open = item
        .find('{')
        .unwrap_or_else(|| panic!("find opening brace for {marker}"));
    let mut depth = 0_i32;
    let mut end = None;
    for (index, ch) in item[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + index + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    &item[open..end.unwrap_or_else(|| panic!("find closing brace for {marker}"))]
}

#[test]
fn runtime_loop_state_has_one_environment_field_not_injected_capability_fields() {
    let source = read_runtime_source("agent_loop.rs");
    let state = rust_item_body(&source, "pub struct RuntimeLoopState");

    assert!(
        state.contains("pub environment: RuntimeEnvironment"),
        "RuntimeLoopState must hold the runtime environment as one field"
    );
    assert!(
        !state.contains("llm_client"),
        "RuntimeLoopState must not expose an injected LLM provider/client field"
    );
    assert!(
        !state.contains("tools"),
        "RuntimeLoopState must not expose an injected ToolRegistry field"
    );

    let environment = rust_item_body(&source, "pub enum RuntimeEnvironment");
    assert!(
        environment.contains("Namespace")
            && environment.contains("namespace: NamespaceRuntimeEnvironment"),
        "RuntimeEnvironment must retain the namespace-native environment variant"
    );
    assert!(
        !environment.contains("Legacy"),
        "RuntimeEnvironment must not retain a legacy injected-capability variant"
    );
    assert!(
        !source.contains("fn llm_client(")
            && !source.contains("fn llm_client_mut(")
            && !source.contains("legacy_generation_")
            && !source.contains("legacy_tool_registry_clone")
            && !source.contains("tools_mut("),
        "RuntimeLoopState must not expose LlmClient accessors on the engine path"
    );
}

#[test]
fn agent_loop_namespace_environment_uses_file_namespace_not_provider_or_registry() {
    let source = read_runtime_source("agent_loop/namespace_environment.rs");
    let production_source = source
        .split("\n#[cfg(test)]")
        .next()
        .expect("namespace environment production source");

    for forbidden in ["LlmProvider", "ToolRegistry"] {
        assert!(
            !production_source.contains(forbidden),
            "namespace-native production path must not reference {forbidden}"
        );
    }

    for required in [
        "root: InProcessTransport",
        "/mnt/llm/connections/{llm_connection}/clone",
        "write_agent_output",
        "machine/tape",
    ] {
        assert!(
            production_source.contains(required),
            "namespace-native path must keep {required:?} in the file-operation spine"
        );
    }
}
