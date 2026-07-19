//! Absence guards for the namespace-native Agent Execution Engine boundary.

fn read_runtime_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/runtime/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/runtime/{path}: {error}"))
}

fn read_crate_source(path: &str) -> String {
    std::fs::read_to_string(format!("{}/src/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|error| panic!("read src/{path}: {error}"))
}

fn read_rust_sources_under(path: &str) -> Vec<(std::path::PathBuf, String)> {
    fn visit(directory: &std::path::Path, sources: &mut Vec<(std::path::PathBuf, String)>) {
        for entry in std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                visit(&path, sources);
            } else if path.extension().and_then(std::ffi::OsStr::to_str) == Some("rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                sources.push((path, source));
            }
        }
    }

    let mut sources = Vec::new();
    visit(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(path),
        &mut sources,
    );
    sources
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
    let transition_source = read_runtime_source("transition.rs");
    let state = rust_item_body(&transition_source, "pub(super) struct RuntimeLoopState");
    for required_field in [
        "pub(super) machine: AgentMachine",
        "pub(super) environment: NamespaceRuntimeEnvironment",
        "pub(super) core_config: Config",
        "pub(super) runtime_config: RuntimeConfig",
        "pub(super) definition_persona_dirs: Vec<std::path::PathBuf>",
        "pub(super) prompt_cache: super::prompt_cache::PromptAssemblyCache",
    ] {
        assert!(
            state.contains(required_field),
            "runtime loop aggregate must retain {required_field}"
        );
    }
    assert_eq!(
        state
            .lines()
            .filter(|line| line.trim_start().starts_with("pub(super) "))
            .count(),
        6,
        "runtime loop aggregate must keep exactly its six cohesive fields"
    );
    for displaced_state in [
        "current_submission",
        "turn_state",
        "pending_yield",
        "tool_replay",
        "active_task",
        "deferred_action",
    ] {
        assert!(
            !state.contains(displaced_state),
            "runtime loop aggregate must not regain Machine state {displaced_state}"
        );
    }

    let engine_source = read_runtime_source("engine.rs");
    let runtime_module = read_runtime_source("mod.rs");
    assert!(!runtime_module.contains("use transition::RuntimeLoopState"));
    let controller_source = read_runtime_source("controller.rs");
    let handle = rust_item_body(&controller_source, "pub struct RuntimeHandle");
    let controller = rust_item_body(&controller_source, "pub struct RuntimeController");
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
    assert!(!transition_source.contains("pub enum RuntimeEnvironment"));
    assert!(!transition_source.contains("pub environment: RuntimeEnvironment"));
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
fn process_loop_uses_one_accepted_submission_transition() {
    let engine_source = read_runtime_source("engine.rs");
    let process_loop = rust_item_body(&engine_source, "fn spawn_with_prepared_runtime_environment");

    assert!(process_loop.contains("advance_accepted_submission("));
    for process_control in [
        "sub_rx.recv()",
        "submissions_closed",
        "cancel.cancel()",
        "heartbeat_interval.tick()",
    ] {
        assert!(
            process_loop.contains(process_control),
            "Process loop must retain {process_control}"
        );
    }

    for displaced_transition in [
        "handle_submission_with_cancel(",
        "drive_turn_submission_with_cancel(",
        ".machine.accept_submission(",
        ".machine.finish_submission(",
        ".machine.drain_deferred_runtime_actions(",
    ] {
        assert!(
            !process_loop.contains(displaced_transition),
            "Process loop must not bypass the accepted-submission transition via {displaced_transition}"
        );
    }
}

#[test]
fn namespace_environment_reaches_capabilities_only_through_files() {
    let environment = read_runtime_source("transition/namespace_environment.rs");
    let production = environment
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    let agent_files = read_runtime_source("transition/namespace_environment/agent_files.rs");
    let generation = read_runtime_source("transition/namespace_environment/generation.rs");

    for forbidden in ["LlmProvider", "ToolRegistry"] {
        assert!(!production.contains(forbidden));
        assert!(!agent_files.contains(forbidden));
        assert!(!generation.contains(forbidden));
    }
    for required in [
        "mod agent_files;",
        "mod generation;",
        "root: InProcessTransport",
        "/proc/clone",
    ] {
        assert!(production.contains(required), "missing {required:?}");
    }
    for required in [
        "write_agent_output",
        "{agent_path}/machine/tape",
        "write_confirmation_request",
        "write_structured_input_request",
    ] {
        assert!(
            agent_files.contains(required),
            "AgentFS file owner missing {required:?}"
        );
        assert!(
            !production.contains(required),
            "namespace environment coordinator still contains AgentFS detail {required:?}"
        );
    }
    assert!(
        generation.contains("/mnt/llm/connections/{llm_connection}/clone"),
        "llmfs generation owner must clone through its file surface"
    );
    for operation in ["generate_once_with_cancel", "generate_response_with_retry"] {
        assert!(
            generation.contains(operation),
            "llmfs generation handle missing {operation}"
        );
    }
    assert!(
        !production.contains("/mnt/llm/connections/{llm_connection}/clone"),
        "namespace environment coordinator must leave llmfs protocol details to generation owner"
    );
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
    for forbidden in ["RuntimeLoopState", "namespace_environment()"] {
        assert!(
            !generation.contains(forbidden),
            "generation workflow must receive its narrow handle, not {forbidden}"
        );
    }

    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let generation_handle = rust_item_body(&namespace, "pub(crate) struct NamespaceGeneration");
    assert!(generation_handle.contains("root: InProcessTransport"));
    assert!(generation_handle.contains("llm_connection: String"));
    for forbidden_field in ["agent_path", "tool_process_context", "child_run_registry"] {
        assert!(
            !generation_handle.contains(forbidden_field),
            "generation handle must not gain {forbidden_field}"
        );
    }

    let file_operations = read_runtime_source("transition/namespace_environment/generation.rs");
    assert!(file_operations.contains("impl NamespaceGeneration"));
    assert!(!file_operations.contains("impl NamespaceRuntimeEnvironment"));

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
fn transition_leaf_workflows_do_not_receive_the_runtime_loop_aggregate() {
    for path in [
        "delegated_skill_tool.rs",
        "delegated_skill_tool/runtime_inputs.rs",
        "delegated_skill_evidence.rs",
        "memory_flush.rs",
        "memory_promotion.rs",
        "memory_surfaces.rs",
        "response_guardrails.rs",
        "steering_queue.rs",
        "turn_support.rs",
    ] {
        let source = read_runtime_source(path);
        assert!(
            !source.contains("RuntimeLoopState"),
            "{path} must receive Agent Machine state and namespace handles directly"
        );
    }

    let executor = read_runtime_source("turn_executor.rs");
    let tool_definitions = &executor[executor
        .find("async fn turn_tool_definitions")
        .expect("find turn_tool_definitions")..];
    let tool_definitions = &tool_definitions[..tool_definitions
        .find("fn log_generation_failure")
        .expect("find end of turn_tool_definitions")];
    assert!(!tool_definitions.contains("RuntimeLoopState"));
    assert!(tool_definitions.contains("NamespaceToolExecution"));

    let tool_orchestrator = read_runtime_source("tool_orchestrator.rs");
    let tool_evidence = &tool_orchestrator[tool_orchestrator
        .find("async fn tool_payload_for_tape")
        .expect("find tool_payload_for_tape")..];
    let tool_evidence = &tool_evidence[..tool_evidence
        .find("async fn execute_tool_effect")
        .expect("find end of tool_payload_for_tape")];
    assert!(!tool_evidence.contains("RuntimeLoopState"));
    assert!(tool_evidence.contains("NamespaceAgentFiles"));

    let compaction = read_runtime_source("compaction.rs");
    let compaction_production = compaction
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .expect("compaction production source");
    assert!(!compaction_production.contains("RuntimeLoopState"));
    assert!(compaction_production.contains("CompactionRuntime"));
    let compaction_inputs = read_runtime_source("compaction/runtime_inputs.rs");
    for forbidden in [
        "RuntimeLoopState",
        "RuntimeConfig",
        "NamespaceRuntimeEnvironment",
    ] {
        assert!(!compaction_inputs.contains(forbidden));
    }

    for path in [
        "child_agents.rs",
        "child_agents/delegated_launch.rs",
        "child_agents/launch_context.rs",
        "child_agents/runtime_inputs.rs",
        "child_agents/task_context.rs",
    ] {
        let source = read_runtime_source(path);
        for forbidden in ["RuntimeLoopState", "NamespaceRuntimeEnvironment"] {
            assert!(
                !source.contains(forbidden),
                "{path} must receive only child-launch configuration, snapshots, and namespace handles"
            );
        }
    }
    let child_launch_inputs = read_runtime_source("child_agents/runtime_inputs.rs");
    assert!(!child_launch_inputs.contains("RuntimeConfig"));
    assert!(!child_launch_inputs.contains("Vec<Message>"));
    assert!(child_launch_inputs.contains("ChildLaunchRuntime"));
    assert!(child_launch_inputs.contains("ChildTaskContext"));
    let child_task_context = read_runtime_source("child_agents/task_context.rs");
    assert!(child_task_context.contains("spec.has_handle(SpawnHandle::ToolResults)"));

    let delegated_runtime = read_runtime_source("delegated_skill_tool/runtime_inputs.rs");
    for forbidden in [
        "RuntimeLoopState",
        "RuntimeConfig",
        "NamespaceRuntimeEnvironment",
    ] {
        assert!(
            !delegated_runtime.contains(forbidden),
            "delegated skill runtime must not regain {forbidden}"
        );
    }
    assert!(delegated_runtime.contains("DelegatedSkillRuntime"));
    assert!(delegated_runtime.contains("DelegatedChildRuntimeInputs"));
    assert!(delegated_runtime.contains("ChildLaunchRuntime"));
    let transition = read_runtime_source("transition.rs");
    assert!(transition.contains("fn delegated_skill_runtime("));

    for marker in [
        "fn compaction_submission_id",
        "async fn record_and_emit_compaction_attempt",
        "async fn record_and_emit_memory_flush_attempt",
        "async fn handle_compaction_generation_failure",
        "fn apply_tape_compaction",
    ] {
        assert!(
            !rust_item_body(&compaction, marker).contains("RuntimeLoopState"),
            "{marker} must receive only its explicit Machine and AgentFS dependencies"
        );
    }
}

#[test]
fn runtime_loop_does_not_forward_narrow_handle_operations() {
    let transition = read_runtime_source("transition.rs");
    for operation in [
        "write_namespace_confirmation_request",
        "write_namespace_structured_input_request",
        "generate_once_with_cancel",
        "generate_response_with_retry",
        "static_tool_names",
        "default_tool_cwd",
    ] {
        assert!(
            !transition.contains(operation),
            "RuntimeLoopState retained forwarding operation {operation}"
        );
    }
}

#[test]
fn agent_file_workflows_use_only_the_narrow_agent_files_handle() {
    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let agent_files_handle = rust_item_body(&namespace, "pub(crate) struct NamespaceAgentFiles");
    for required_field in ["root: InProcessTransport", "agent_path: String"] {
        assert!(
            agent_files_handle.contains(required_field),
            "agent files handle must retain {required_field}"
        );
    }
    for forbidden_field in [
        "llm_connection",
        "tool_process_context",
        "child_run_registry",
        "launch_context",
    ] {
        assert!(
            !agent_files_handle.contains(forbidden_field),
            "agent files handle must not gain {forbidden_field}"
        );
    }

    let file_operations = read_runtime_source("transition/namespace_environment/agent_files.rs");
    assert!(file_operations.contains("impl NamespaceAgentFiles"));
    assert!(!file_operations.contains("impl NamespaceRuntimeEnvironment"));
    assert!(file_operations.contains("action_output_reference"));

    let ui = read_runtime_source("ui_surfaces.rs");
    let ui_production = ui.split("\n#[cfg(test)]\nmod tests").next().unwrap();
    assert!(ui_production.contains("&NamespaceAgentFiles"));
    assert!(!ui_production.contains("NamespaceRuntimeEnvironment"));

    let supervisor = read_runtime_source("delegated_child_run/supervisor.rs");
    assert!(supervisor.contains("agent_files: NamespaceAgentFiles"));
    for operation in [
        "read_ui_activity_snapshot",
        "read_assistant_output",
        "request_ids",
        "action_ids",
    ] {
        assert!(
            supervisor.contains(&format!("agent_files.{operation}")),
            "delegated supervisor must read {operation} through NamespaceAgentFiles"
        );
    }
}

#[test]
fn process_file_workflows_use_only_the_narrow_process_files_handle() {
    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let process_files_handle =
        rust_item_body(&namespace, "pub(crate) struct NamespaceProcessFiles");
    for required_field in ["root: InProcessTransport", "agent_path: String"] {
        assert!(
            process_files_handle.contains(required_field),
            "process files handle must retain {required_field}"
        );
    }
    for forbidden_field in [
        "llm_connection",
        "tool_process_context",
        "child_run_registry",
        "input_offset",
        "control_offset",
    ] {
        assert!(
            !process_files_handle.contains(forbidden_field),
            "process files handle must not gain {forbidden_field}"
        );
    }

    let file_operations = read_runtime_source("transition/namespace_environment/process_files.rs");
    assert!(file_operations.contains("impl NamespaceProcessFiles"));
    assert!(!file_operations.contains("impl NamespaceRuntimeEnvironment"));

    let supervisor = read_runtime_source("delegated_child_run/supervisor.rs");
    assert!(supervisor.contains("process_files: NamespaceProcessFiles"));
    assert!(!supervisor.contains("NamespaceRuntimeEnvironment"));
    for operation in [
        "read_process_exit_code",
        "read_process_io_offsets",
        "write_process_control_for_pid",
    ] {
        assert!(
            supervisor.contains(&format!(".{operation}")),
            "delegated supervisor must access {operation} through NamespaceProcessFiles"
        );
    }
}

#[test]
fn tool_workflows_use_only_the_narrow_tool_execution_handle() {
    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let tool_handle = rust_item_body(&namespace, "pub(crate) struct NamespaceToolExecution");
    for required_field in [
        "root: InProcessTransport",
        "process_files: NamespaceProcessFiles",
        "agent_files: NamespaceAgentFiles",
        "tool_process_context: Option<NamespaceToolProcessContext>",
    ] {
        assert!(
            tool_handle.contains(required_field),
            "Tool execution handle must retain {required_field}"
        );
    }
    for forbidden_field in [
        "llm_connection",
        "launch_context",
        "child_run_registry",
        "mount_grant_applicator",
    ] {
        assert!(
            !tool_handle.contains(forbidden_field),
            "Tool execution handle must not gain {forbidden_field}"
        );
    }

    let tool_operations = read_runtime_source("transition/namespace_environment/tool_execution.rs");
    assert!(tool_operations.contains("impl NamespaceToolExecution"));
    assert!(!tool_operations.contains("impl NamespaceRuntimeEnvironment"));
    for operation in ["default_cwd", "static_tool_names"] {
        assert!(
            tool_operations.contains(operation),
            "Tool execution handle missing {operation}"
        );
    }

    for path in [
        "tool_orchestrator.rs",
        "turn_executor.rs",
        "response_guardrails.rs",
        "submission_handlers.rs",
    ] {
        let source = read_runtime_source(path);
        assert!(
            !source.contains("namespace_environment()"),
            "{path} must not reach complete namespace environment for Tool work"
        );
    }
}

#[test]
fn child_launch_workflows_use_only_the_narrow_child_launch_handle() {
    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let child_launch = rust_item_body(&namespace, "pub(crate) struct NamespaceChildLaunch");
    for required_field in [
        "llm_connection: String",
        "launch_context: Option<crate::ProcessLaunchContext>",
        "child_process_assembler: Option<Arc<dyn super::super::ChildAgentProcessAssembler>>",
    ] {
        assert!(
            child_launch.contains(required_field),
            "child launch handle must retain {required_field}"
        );
    }
    for forbidden_field in [
        "root: InProcessTransport",
        "agent_path",
        "tool_process_context",
        "mount_grant_applicator",
        "child_run_registry",
    ] {
        assert!(
            !child_launch.contains(forbidden_field),
            "child launch handle must not gain {forbidden_field}"
        );
    }

    let operations = read_runtime_source("transition/namespace_environment/child_launch.rs");
    assert!(operations.contains("impl NamespaceChildLaunch"));
    assert!(!operations.contains("impl NamespaceRuntimeEnvironment"));

    for path in [
        "child_agents.rs",
        "child_agents/launch_context.rs",
        "child_agents/delegated_launch.rs",
        "delegated_skill_tool.rs",
    ] {
        let source = read_runtime_source(path);
        assert!(
            !source.contains("namespace_environment()"),
            "{path} must not reach complete namespace environment for child launch work"
        );
    }
}

#[test]
fn mount_changes_use_only_the_narrow_namespace_mount_control() {
    let namespace = read_runtime_source("transition/namespace_environment.rs");
    let mount_control = rust_item_body(&namespace, "pub struct NamespaceMountControl<'a>");
    for required_field in [
        "launch_context: &'a mut Option<crate::ProcessLaunchContext>",
        "mount_grant_applicator: Option<Arc<dyn MountGrantApplicator>>",
        "tool_process_context: Option<NamespaceToolProcessContext>",
    ] {
        assert!(
            mount_control.contains(required_field),
            "Namespace mount control must retain {required_field}"
        );
    }
    for forbidden_field in [
        "root: InProcessTransport",
        "agent_path",
        "llm_connection",
        "child_run_registry",
        "child_process_assembler",
    ] {
        assert!(
            !mount_control.contains(forbidden_field),
            "Namespace mount control must not gain {forbidden_field}"
        );
    }

    let operations = read_runtime_source("transition/namespace_environment/mount_control.rs");
    assert!(operations.contains("impl NamespaceMountControl"));
    assert!(!operations.contains("impl NamespaceRuntimeEnvironment"));

    for displaced in [
        "apply_approved_mount_grant",
        "persist_approved_host_mount",
        "sync_tool_execution_binding",
    ] {
        assert!(
            !namespace.contains(displaced),
            "complete namespace environment retained displaced mount operation {displaced}"
        );
    }

    let submission_handlers = read_runtime_source("submission_handlers.rs");
    assert!(submission_handlers.contains(".mount_control()"));
    assert!(!submission_handlers.contains("state.environment"));

    let transition = read_runtime_source("transition.rs");
    let engine = read_runtime_source("engine.rs");
    assert!(!transition.contains("fn namespace_environment("));
    assert!(!engine.contains("namespace_environment()"));
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
    assert!(child.contains("ChildLaunchRuntime"));
    assert!(child.contains(".child_launch"));
    assert!(child.contains(".assembler()"));
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

#[test]
fn agent_machine_state_is_not_a_public_or_runtime_field_surface() {
    let public_api = read_crate_source("lib.rs");
    assert!(
        !public_api.contains("AgentMachine,"),
        "AgentMachine must not be a cross-crate integration surface"
    );

    let machine = read_crate_source("agent_machine.rs");
    let state = rust_item_body(&machine, "pub(crate) struct AgentMachine");
    for private_field in ["tape", "recorder", "has_active_task", "transition_state"] {
        assert!(state.contains(&format!("{private_field}:")));
        assert!(
            !state.contains(&format!("pub {private_field}:")),
            "AgentMachine field {private_field} must remain private"
        );
    }

    let transition_state = read_crate_source("agent_machine/transition_state.rs");
    let transition_fields = rust_item_body(
        &transition_state,
        "pub(super) struct MachineTransitionState",
    );
    assert!(transition_fields.contains("current_submission_id:"));
    assert!(!transition_fields.contains("pub current_submission_id:"));
    assert!(
        !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/runtime/turn_state.rs")
            .exists(),
        "turn-local state must not retain a second runtime owner"
    );

    let transition_source = read_runtime_source("transition.rs");
    let runtime_state = rust_item_body(&transition_source, "pub(super) struct RuntimeLoopState");
    for displaced_field in ["current_submission_id", "turn_state"] {
        assert!(
            !runtime_state.contains(&format!("{displaced_field}:")),
            "RuntimeLoopState must not retain Machine field {displaced_field}"
        );
    }

    for (path, source) in read_rust_sources_under("runtime") {
        for displaced_surface in ["TurnState", ".turn_state"] {
            assert!(
                !source.contains(displaced_surface),
                "{} retains displaced Machine state surface {displaced_surface}",
                path.display()
            );
        }
        for forbidden in [
            "machine.tape.",
            "machine.recorder.",
            "machine.has_active_task =",
            "state.machine.tape.",
            "state.machine.recorder.",
            "state.machine.has_active_task =",
        ] {
            assert!(
                !source.contains(forbidden),
                "{} reaches through AgentMachine via {forbidden}",
                path.display()
            );
        }
    }
}
