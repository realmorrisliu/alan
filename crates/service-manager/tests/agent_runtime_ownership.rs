#[test]
fn runtime_assembly_call_paths_have_explicit_owners() {
    let service = include_str!("../src/agent_runtime.rs");
    for marker in [
        "spawn_unit_process(",
        "AgentFs::new()",
        "set_root_process(",
        "with_process_context(",
        "with_mount_grant_applicator_factory(",
        "self.procfs.record_exit(pid, 1).await;",
        "shutdown_root",
    ] {
        assert!(
            service.contains(marker),
            "Agent Runtime Service is missing root assembly marker `{marker}`"
        );
    }

    let supervisor = include_str!("../src/runtime.rs");
    let production = supervisor
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    for displaced in [
        "AgentFs::new()",
        "set_root_process(",
        "with_process_context(",
    ] {
        assert!(
            !production.contains(displaced),
            "Service Manager supervisor still owns `{displaced}`"
        );
    }

    let child_runtime = include_str!("../../agent-engine/src/runtime/child_agents.rs");
    for transitional in [
        "spawn_child_namespace_runtime_environment",
        "child_namespace_from_launch_handles",
        "bind_process(pid.clone()",
        "LiveNamespace::new(child_namespace)",
    ] {
        assert!(
            child_runtime.contains(transitional),
            "child assembly call path changed without updating its ownership slice: `{transitional}`"
        );
    }
}
