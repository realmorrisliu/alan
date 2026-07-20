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
fn runtime_assembly_call_paths_have_explicit_owners() {
    let service = include_str!("../src/agent_runtime.rs");
    for marker in [
        "spawn_unit_process(",
        "AgentFs::new()",
        "set_root_process(",
        "with_tool_process_context(",
        "self.host_mount.register_process(",
        "register_process_authority(",
        "impl ChildAgentProcessAssembler",
        "walk child /proc/clone",
        "LiveNamespace::new(child_namespace(",
        "self.procfs.record_exit(pid, 1).await;",
        "shutdown_root",
    ] {
        assert!(
            service.contains(marker),
            "Agent Runtime Service is missing root assembly marker `{marker}`"
        );
    }
    assert!(!service.contains("with_mount_grant_applicator("));

    let supervisor = include_str!("../src/runtime.rs");
    let production = supervisor
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    for displaced in [
        "AgentFs::new()",
        "set_root_process(",
        "with_tool_process_context(",
    ] {
        assert!(
            !production.contains(displaced),
            "Service Manager supervisor still owns `{displaced}`"
        );
    }

    let child_runtime = include_str!("../../agent-engine/src/runtime/child_agents.rs");
    let live_child_launch = rust_item_body(child_runtime, "async fn spawn_child_runtime_inner");
    assert!(child_runtime.contains("ChildLaunchRuntime"));
    assert!(live_child_launch.contains(".child_launch"));
    assert!(live_child_launch.contains(".assembler()"));
    for displaced in [
        "child_process_assembler()",
        "AgentFs::new()",
        "bind_process(",
        "LiveNamespace::new(",
        "for_spawner(",
        "with_tool_process_context(",
    ] {
        assert!(
            !live_child_launch.contains(displaced),
            "Agent Execution Engine still owns child assembly marker `{displaced}`"
        );
    }
}
