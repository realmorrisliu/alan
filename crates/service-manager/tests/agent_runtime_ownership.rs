fn rust_item_body<'a>(source: &'a str, marker: &str) -> &'a str {
    let item = &source[source
        .find(marker)
        .unwrap_or_else(|| panic!("find {marker}"))..];
    let open = item.find('{').unwrap_or_else(|| panic!("open {marker}"));
    let mut depth = 0_i32;
    for (index, character) in item[open..].char_indices() {
        match character {
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
fn agent_runtime_service_owns_the_alan_agent_process_image() {
    let service = include_str!("../src/agent_runtime.rs");
    for marker in [
        "const AGENT_EXECUTABLE: &str = \"/bin/alan-agent\"",
        "async fn launch_root",
        "async fn run_agent_process",
        "fn prepare_launch",
        "fn child_template",
        "AgentFs::new()",
        "bind_process(",
        "set_root_process(",
        "spawn_with_namespace_environment(",
        "register_process_authority(",
        "register_child_process(",
        "async fn release_process",
    ] {
        assert!(
            service.contains(marker),
            "Agent Runtime Service is missing Process-image ownership marker `{marker}`"
        );
    }

    let root_launch = rust_item_body(service, "async fn launch_root");
    assert!(root_launch.contains("walk Root Agent /proc/clone"));
    assert!(root_launch.contains("commit_clone("));
    assert!(root_launch.contains("ExecNamespaceManifest::from_namespace"));

    let process_image = rust_item_body(service, "async fn run_prepared_agent");
    for required in [
        "bind_live_namespace(",
        "NamespaceRuntimeEnvironment::new(",
        "wait_for_child_terminal(",
        "wait_for_root_stop(",
    ] {
        assert!(
            process_image.contains(required),
            "Agent executable image is missing `{required}`"
        );
    }

    let runner = include_str!("../src/process_runner.rs");
    let dispatch = rust_item_body(runner, "impl ProcessRunner for SystemProcessRunner");
    assert!(dispatch.contains("invocation.exec.executable == \"/bin/alan-agent\""));
    assert!(dispatch.contains("runtime.run_agent_process(invocation).await"));

    let engine_child = include_str!("../../agent-engine/src/runtime/child_agents.rs");
    let launch = rust_item_body(engine_child, "async fn spawn_child_runtime_inner");
    for required in [
        "read_process_namespace(",
        "read_process_descriptors(",
        ".spawn_agent_process(",
        "wait_for_child_process_startup(",
    ] {
        assert!(
            launch.contains(required),
            "Engine child launch is missing `{required}`"
        );
    }
    for displaced in [
        "AgentFs::new()",
        "bind_process(",
        "LiveNamespace::new(",
        "ProcessLaunchContext",
        "ChildAgentProcessAssembler",
        "AgentProcessLifecycle",
        ".assembler()",
    ] {
        assert!(
            !engine_child.contains(displaced),
            "Agent Execution Engine still owns child assembly through `{displaced}`"
        );
    }

    let engine_runtime =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../agent-engine/src/runtime");
    for retired in [
        "agent_process.rs",
        "child_agents/launch_context.rs",
        "child_agents/runtime_startup.rs",
    ] {
        assert!(
            !engine_runtime.join(retired).exists(),
            "retired Engine launch owner still exists at {retired}"
        );
    }
}
