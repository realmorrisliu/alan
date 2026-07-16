use super::*;
use crate::{
    agent_machine::AgentMachine,
    config::Config,
    rollout::{RolloutItem, RolloutRecorder},
    runtime::{
        ChildRunRecord, ChildRunStatus, NamespaceRuntimeEnvironment, RuntimeConfig, TurnState,
        agent_loop::NamespaceActionRecord,
        delegated_child_run::{
            ChildRuntimeResult, ChildRuntimeStatus, MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS,
            MAX_DELEGATED_RESULT_SUMMARY_CHARS,
        },
        delegated_skill_evidence::persist_delegated_child_evidence,
        delegated_skill_tool::{
            DEFAULT_DELEGATED_TIMEOUT_SECS, DelegatedSkillInvocationRequest,
            MAX_DELEGATED_SKILL_ID_CHARS, MAX_DELEGATED_TARGET_CHARS, MAX_DELEGATED_TASK_CHARS,
            handle_invoke_delegated_skill_with_spawn,
        },
        delegation_capabilities::DelegatedSpawnRejected,
        mount_request_tool::MountRequestAccess,
        turn_state::TurnActivityState,
    },
    skills::{
        ActiveSkillEnvelope, DelegatedSkillInvocationRecord, ResolvedCapabilityView,
        ResolvedSkillExecution, ScopedPackageDir, SkillActivationReason,
        SkillExecutionResolutionSource, SkillHostCapabilities, SkillMetadata, SkillScope,
    },
    tools::ToolRegistry,
};
use alan_agent_protocol::SpawnHandle;
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_shell::Shell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn namespace_environment_for_virtual_tool_test(
    tools: &ToolRegistry,
) -> NamespaceRuntimeEnvironment {
    let agentfs = Arc::new(AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let runner = crate::tools::ToolProcessRunner::from_registry(tools);
    NamespaceRuntimeEnvironment::new(root, "/agent/1", "default")
        .with_tool_process_context(alan_kernel::Pid(1), runner)
}

fn create_test_agent_loop_state() -> super::super::agent_loop::RuntimeLoopState {
    let config = Config::default();
    let machine = AgentMachine::new();
    let host_source = PathBuf::from("/tmp/alan-delegated-parent");
    let mut launch_namespace = Namespace::new();
    launch_namespace.mount(
        "/mnt/source",
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
        Access::ReadWrite,
    );
    let launch_context = crate::ProcessLaunchContext::new(
        launch_namespace,
        alan_kernel::Credentials::user("parent-agent"),
        "/mnt/source",
    )
    .unwrap()
    .with_host_mount(
        crate::HostMountGrant::new("/mnt/source", &host_source, Access::ReadWrite).unwrap(),
    );
    let mut tools = ToolRegistry::new();
    tools.set_default_execution_binding(
        crate::tools::ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/alan-system-store/tmp"),
        )
        .unwrap(),
    );
    let runtime_config = RuntimeConfig::default();
    let mut prompt_cache = crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new());
    prompt_cache.set_host_capabilities(
        SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    super::super::agent_loop::RuntimeLoopState {
        machine,
        current_submission_id: None,
        environment: namespace_environment_for_virtual_tool_test(&tools)
            .with_launch_context(launch_context),
        core_config: config,
        runtime_config,
        definition_persona_dirs: Vec::new(),
        prompt_cache,
        turn_state: TurnState::default(),
    }
}

fn create_namespace_agent_loop_state_and_shell()
-> (super::super::agent_loop::RuntimeLoopState, Shell) {
    let agentfs = Arc::new(AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    let shell = Shell::new(root.clone());
    let mut state = create_test_agent_loop_state();
    state.environment = NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    (state, shell)
}

async fn read_shell_utf8(shell: &Shell, path: &str) -> String {
    String::from_utf8(shell.cat(path).await.expect("read agent file")).expect("agent file utf8")
}

fn delegated_test_skill_metadata(skill_id: &str, target: &str) -> SkillMetadata {
    SkillMetadata {
        id: skill_id.to_string(),
        package_id: Some(format!("skill:{skill_id}")),
        name: skill_id.to_string(),
        description: format!("Delegated test skill {skill_id}"),
        short_description: None,
        path: PathBuf::from(format!("/tmp/{skill_id}/SKILL.md")),
        package_root: Some(PathBuf::from(format!("/tmp/{skill_id}"))),
        resource_root: Some(PathBuf::from(format!("/tmp/{skill_id}"))),
        scope: SkillScope::Descriptor,
        tags: Vec::new(),
        capabilities: None,
        compatibility: Default::default(),
        source: Default::default(),
        enabled: true,
        allow_implicit_invocation: true,
        alan_metadata: Default::default(),
        compatible_metadata: Default::default(),
        execution: ResolvedSkillExecution::Delegate {
            target: target.to_string(),
            source: SkillExecutionResolutionSource::ExplicitMetadata,
        },
    }
}

fn activate_test_delegated_skill(
    state: &mut super::super::agent_loop::RuntimeLoopState,
    skill_id: &str,
    target: &str,
) {
    state
        .turn_state
        .set_active_skills(vec![ActiveSkillEnvelope::available(
            delegated_test_skill_metadata(skill_id, target),
            SkillActivationReason::ExplicitMention {
                mention: skill_id.to_string(),
            },
        )]);
}

fn capability_view_for_package_store(package_store: &std::path::Path) -> ResolvedCapabilityView {
    ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
        path: package_store.to_path_buf(),
        scope: SkillScope::Descriptor,
    }])
}

fn test_child_run_record(child_run_id: &str, parent_process_path: &str) -> ChildRunRecord {
    ChildRunRecord::new(
        child_run_id.to_string(),
        parent_process_path.to_string(),
        "/proc/42".to_string(),
        Some("/agent/42".to_string()),
        Some("definition:reviewer".to_string()),
    )
}

fn tool_result_text_for_call(
    state: &super::super::agent_loop::RuntimeLoopState,
    call_id: &str,
) -> String {
    state
        .machine
        .tape
        .prompt_view()
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == call_id)
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected tool result")
}

async fn try_handle_virtual_tool_call_for_test<E, F>(
    state: &mut super::super::agent_loop::RuntimeLoopState,
    tool_call: &NormalizedToolCall,
    emit: &mut E,
) -> Result<VirtualToolOutcome>
where
    E: FnMut(Event) -> F,
    F: std::future::Future<Output = ()>,
{
    let cancel = CancellationToken::new();
    try_handle_virtual_tool_call(state, tool_call, &tool_call.arguments, &cancel, false, emit).await
}

#[path = "delegated_skill_evidence_tests.rs"]
mod delegated_skill_evidence;
#[path = "delegated_skill_tool_tests.rs"]
mod delegated_skill_tool;
#[path = "mount_request_tool_tests.rs"]
mod mount_request_tool;

#[test]
fn test_request_confirmation_tool_definition_schema_shape() {
    let def = request_confirmation_tool_definition();
    assert_eq!(def.name, "request_confirmation");
    assert!(def.description.contains("confirmation"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(
        def.parameters["properties"]["checkpoint_id"]["type"],
        "string"
    );
    assert_eq!(
        def.parameters["properties"]["checkpoint_type"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["summary"]["type"], "string");
    assert_eq!(def.parameters["properties"]["details"]["type"], "object");
}

#[test]
fn test_request_user_input_tool_definition() {
    let def = request_user_input_tool_definition();
    assert_eq!(def.name, "request_user_input");
    assert!(def.description.contains("structured"));
    assert_eq!(def.parameters["type"], "object");
    assert!(def.parameters["properties"].get("title").is_some());
    assert!(def.parameters["properties"].get("prompt").is_some());
    assert!(def.parameters["properties"].get("questions").is_some());
    assert_eq!(
        def.parameters["properties"]["questions"]["items"]["properties"]["kind"]["enum"],
        json!([
            "text",
            "boolean",
            "number",
            "integer",
            "single_select",
            "multi_select"
        ])
    );
}

#[test]
fn test_update_plan_tool_definition() {
    let def = update_plan_tool_definition();
    assert_eq!(def.name, "update_plan");
    assert!(def.description.contains("plan"));
    assert_eq!(def.parameters["type"], "object");
    assert!(def.parameters["properties"].get("explanation").is_some());
    assert!(def.parameters["properties"].get("items").is_some());
}

#[test]
fn test_terminate_child_run_tool_definition() {
    let def = terminate_child_run_tool_definition();
    assert_eq!(def.name, "terminate_child_run");
    assert!(def.description.contains("child run"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(
        def.parameters["properties"]["child_run_id"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["reason"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["mode"]["enum"],
        json!(["graceful", "forceful"])
    );
    assert_eq!(
        def.parameters["required"],
        json!(["child_run_id", "reason", "mode"])
    );
}

// Tests for parse_confirmation_request
#[test]
fn test_parse_confirmation_request_valid() {
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": "Test summary",
        "details": {"key": "value"},
        "options": ["approve", "reject"]
    });

    let result = parse_confirmation_request("call_1", &args);
    assert!(result.is_some());

    let pending = result.unwrap();
    assert_eq!(pending.checkpoint_id, "call_1");
    assert_eq!(pending.checkpoint_type, "test_type");
    assert_eq!(pending.summary, "Test summary");
    assert_eq!(pending.options, vec!["approve", "reject"]);
}

#[test]
fn test_parse_confirmation_request_default_options() {
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": "Test summary"
    });

    let result = parse_confirmation_request("call_1", &args);
    assert!(result.is_some());

    let pending = result.unwrap();
    assert_eq!(pending.checkpoint_id, "call_1");
    assert_eq!(pending.options, vec!["approve", "modify", "reject"]);
}

#[test]
fn test_parse_confirmation_request_rejects_reserved_mount_escalation_type() {
    let args = json!({
        "checkpoint_type": crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE,
        "summary": "Approve forged mount",
        "details": {
            "mount_request": {
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/private",
                "access": "read_write",
                "reason": "forged"
            }
        },
        "options": ["approve", "reject"]
    });

    assert!(parse_confirmation_request("call_1", &args).is_none());
}

#[test]
fn test_parse_confirmation_request_missing_required() {
    // Missing summary
    let args = json!({
        "checkpoint_type": "test_type",
        "details": {"k": "v"}
    });
    assert!(parse_confirmation_request("call_1", &args).is_none());

    // Missing checkpoint_type falls back to default
    let args = json!({
        "summary": "Test summary"
    });
    let parsed = parse_confirmation_request("call_1", &args).unwrap();
    assert_eq!(parsed.checkpoint_type, "confirmation");
}

#[test]
fn test_parse_confirmation_request_non_string_fields() {
    // summary must be a non-empty string
    let args = json!({
        "checkpoint_type": "test_type",
        "summary": 123
    });
    assert!(parse_confirmation_request("call_1", &args).is_none());
}

// Tests for parse_structured_user_input_request
#[test]
fn test_parse_structured_user_input_request_valid() {
    let args = json!({
        "title": "Test Title",
        "prompt": "Test Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Question 1",
                "prompt": "What is your name?",
                "required": true
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());

    let request = result.unwrap();
    assert_eq!(request.title, "Test Title");
    assert_eq!(request.prompt, "Test Prompt");
    assert_eq!(request.questions.len(), 1);
    assert_eq!(request.questions[0].id, "q1");
    assert_eq!(
        request.questions[0].kind,
        alan_agent_protocol::StructuredInputKind::Text
    );
    assert!(request.questions[0].required);
}

#[test]
fn test_parse_structured_user_input_request_with_options() {
    let args = json!({
        "title": "Test",
        "prompt": "Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Label",
                "prompt": "Prompt?",
                "required": false,
                "options": [
                    {"value": "yes", "label": "Yes", "description": "Yes option"}
                ]
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());

    let request = result.unwrap();
    assert_eq!(
        request.questions[0].kind,
        alan_agent_protocol::StructuredInputKind::SingleSelect
    );
    assert_eq!(request.questions[0].options.len(), 1);
    assert_eq!(request.questions[0].options[0].value, "yes");
    assert_eq!(request.questions[0].options[0].label, "Yes");
}

#[test]
fn test_parse_structured_user_input_request_with_explicit_metadata() {
    let args = json!({
        "title": "Deployment settings",
        "prompt": "Review and adjust the requested values.",
        "questions": [
            {
                "id": "branch",
                "label": "Branch",
                "prompt": "Branch name",
                "kind": "text",
                "required": true,
                "placeholder": "feature/adaptive-yield-ui",
                "help_text": "Use the exact git ref that should be deployed.",
                "default": "main"
            },
            {
                "id": "envs",
                "label": "Environments",
                "prompt": "Pick deployment targets",
                "kind": "multi_select",
                "options": [
                    {"value": "staging", "label": "Staging"},
                    {"value": "prod", "label": "Production"}
                ],
                "defaults": ["prod", "staging", "prod"],
                "min_selected": 1,
                "max_selected": 2
            }
        ]
    });

    let result = parse_structured_user_input_request("call_1", &args).unwrap();
    assert_eq!(
        result.questions[0].placeholder.as_deref(),
        Some("feature/adaptive-yield-ui")
    );
    assert_eq!(
        result.questions[0].help_text.as_deref(),
        Some("Use the exact git ref that should be deployed.")
    );
    assert_eq!(result.questions[0].default_value.as_deref(), Some("main"));
    assert_eq!(
        result.questions[1].kind,
        alan_agent_protocol::StructuredInputKind::MultiSelect
    );
    assert_eq!(result.questions[1].default_values, vec!["prod", "staging"]);
    assert_eq!(result.questions[1].min_selected, Some(1));
    assert_eq!(result.questions[1].max_selected, Some(2));
}

#[test]
fn test_parse_structured_user_input_request_rejects_select_without_options() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [
            {
                "id": "q1",
                "label": "Label",
                "prompt": "Prompt?",
                "kind": "single_select"
            }
        ]
    });

    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_missing_required() {
    // Missing title
    let args = json!({
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing prompt
    let args = json!({
        "title": "Title",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing questions
    let args = json!({
        "title": "Title",
        "prompt": "Prompt"
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_empty_fields() {
    // Empty title
    let args = json!({
        "title": "",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Empty prompt
    let args = json!({
        "title": "Title",
        "prompt": "  ",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_empty_questions() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": []
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_invalid_question() {
    // Missing question id
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"label": "Label", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing question label
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "prompt": "Prompt?"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());

    // Missing question prompt
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label"}]
    });
    assert!(parse_structured_user_input_request("call_1", &args).is_none());
}

#[test]
fn test_parse_structured_user_input_request_custom_request_id() {
    let args = json!({
        "request_id": "custom_id",
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });

    let result = parse_structured_user_input_request("call_1", &args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().request_id, "call_1");
}

#[test]
fn test_parse_structured_user_input_request_fallback_request_id() {
    let args = json!({
        "title": "Title",
        "prompt": "Prompt",
        "questions": [{"id": "q1", "label": "Label", "prompt": "Prompt?"}]
    });

    let result = parse_structured_user_input_request("fallback_id", &args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().request_id, "fallback_id");
}

// Tests for parse_plan_update
#[test]
fn test_parse_plan_update_valid() {
    let args = json!({
        "explanation": "Test explanation",
        "items": [
            {"id": "1", "content": "Step 1", "status": "pending"},
            {"id": "2", "content": "Step 2", "status": "in_progress"}
        ]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());

    let (explanation, items) = result.unwrap();
    assert_eq!(explanation, Some("Test explanation".to_string()));
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, "1");
    assert_eq!(items[1].content, "Step 2");
}

#[test]
fn test_parse_plan_update_without_explanation() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1", "status": "completed"}]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());

    let (explanation, items) = result.unwrap();
    assert_eq!(explanation, None);
    assert_eq!(items.len(), 1);
}

#[test]
fn test_parse_plan_update_missing_items() {
    let args = json!({
        "explanation": "Test"
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_empty_items() {
    let args = json!({
        "items": []
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_id() {
    let args = json!({
        "items": [{"content": "Step 1", "status": "pending"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_content() {
    let args = json!({
        "items": [{"id": "1", "status": "pending"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_missing_status() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_invalid_status() {
    let args = json!({
        "items": [{"id": "1", "content": "Step 1", "status": "invalid_status"}]
    });
    assert!(parse_plan_update(&args).is_none());
}

#[test]
fn test_parse_plan_update_using_description() {
    // Tests that "description" field can be used as fallback for "content"
    let args = json!({
        "items": [{"id": "1", "description": "Step description", "status": "pending"}]
    });

    let result = parse_plan_update(&args);
    assert!(result.is_some());
    assert_eq!(result.unwrap().1[0].content, "Step description");
}

// Tests for parse_plan_status
#[test]
fn test_parse_plan_status_valid_values() {
    assert!(parse_plan_status("pending").is_some());
    assert!(parse_plan_status("blocked").is_some());
    assert!(parse_plan_status("in_progress").is_some());
    assert!(parse_plan_status("completed").is_some());
    assert!(parse_plan_status("skipped").is_some());
}

#[test]
fn test_parse_plan_status_invalid_values() {
    assert!(parse_plan_status("unknown").is_none());
    assert!(parse_plan_status("").is_none());
    assert!(parse_plan_status("PENDING").is_none()); // Case sensitive
}

// Tests for try_handle_virtual_tool_call
#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_confirmation() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            "checkpoint_id": "chk_123",
            "checkpoint_type": "test",
            "summary": "Test confirmation"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::PauseTurn));

    // Verify confirmation was set
    assert!(state.turn_state.pending_confirmation().is_some());
}

#[tokio::test]
async fn namespace_request_confirmation_writes_request_file_and_waits_on_file_id() {
    let (mut state, shell) = create_namespace_agent_loop_state_and_shell();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            "checkpoint_type": "test",
            "summary": "Test confirmation",
            "details": {"path": "demo.txt"},
            "options": ["approve", "reject"]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(result, VirtualToolOutcome::PauseTurn));
    assert_eq!(
        state.turn_state.pending_request_ids(),
        vec!["r0".to_string()]
    );
    assert_eq!(
        state
            .turn_state
            .pending_confirmation()
            .unwrap()
            .checkpoint_id,
        "call_1"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/kind").await,
        "confirmation"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/prompt").await,
        "Test confirmation"
    );
    let options: serde_json::Value =
        serde_json::from_str(&read_shell_utf8(&shell, "/agent/1/requests/r0/options").await)
            .unwrap();
    assert_eq!(options["checkpoint_id"], "call_1");
    assert_eq!(options["checkpoint_type"], "test");
    assert_eq!(options["details"]["path"], "demo.txt");
    assert_eq!(options["options"][0], "approve");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield {
            request_id,
            kind: alan_agent_protocol::YieldKind::Confirmation,
            ..
        } if request_id == "r0"
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_confirmation() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_confirmation".to_string(),
        arguments: json!({
            // Invalid summary type
            "summary": 42
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_user_input() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            "title": "Test Input",
            "prompt": "Enter value",
            "questions": [{"id": "q1", "label": "Q1", "prompt": "What?"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::PauseTurn));

    // Verify structured input was set
    assert!(state.turn_state.has_pending_interaction());
}

#[tokio::test]
async fn namespace_request_user_input_writes_request_file_and_waits_on_file_id() {
    let (mut state, shell) = create_namespace_agent_loop_state_and_shell();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            "title": "Test Input",
            "prompt": "Enter value",
            "questions": [{"id": "q1", "label": "Q1", "prompt": "What?"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(result, VirtualToolOutcome::PauseTurn));
    assert_eq!(
        state.turn_state.pending_request_ids(),
        vec!["r0".to_string()]
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/kind").await,
        "structured_input"
    );
    assert_eq!(
        read_shell_utf8(&shell, "/agent/1/requests/r0/prompt").await,
        "Enter value"
    );
    let options: serde_json::Value =
        serde_json::from_str(&read_shell_utf8(&shell, "/agent/1/requests/r0/options").await)
            .unwrap();
    assert_eq!(options["request_id"], "call_1");
    assert_eq!(options["title"], "Test Input");
    assert_eq!(options["questions"][0]["id"], "q1");
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield {
            request_id,
            kind: alan_agent_protocol::YieldKind::StructuredInput,
            ..
        } if request_id == "r0"
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_user_input() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "request_user_input".to_string(),
        arguments: json!({
            // Missing required fields
            "title": "Test"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_update_plan() {
    let mut state = create_test_agent_loop_state();
    let expected_items = vec![alan_agent_protocol::PlanItem {
        id: "1".to_string(),
        content: "Step 1".to_string(),
        status: alan_agent_protocol::PlanItemStatus::InProgress,
    }];

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "update_plan".to_string(),
        arguments: json!({
            "explanation": "Test plan",
            "items": [{"id": "1", "content": "Step 1", "status": "in_progress"}]
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::PlanUpdated { explanation, items }
            if explanation.as_deref() == Some("Test plan") && items == &expected_items
    )));

    let prompt_view = state.machine.tape.prompt_view();
    let tool_result = prompt_view
        .messages
        .iter()
        .find_map(|message| match message {
            crate::tape::Message::Tool { responses } => responses
                .iter()
                .find(|response| response.id == "call_1")
                .map(crate::tape::ToolResponse::text_content),
            _ => None,
        })
        .expect("expected update_plan tool payload");
    assert!(tool_result.contains("\"status\":\"plan_updated\""));
    assert!(tool_result.contains("\"items\":["));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_update_plan() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "update_plan".to_string(),
        arguments: json!({
            // Missing items
            "explanation": "Test"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: false
        }
    ));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_success() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "no longer needed",
            "mode": "forceful"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Terminating);
    let termination = record.termination.as_ref().unwrap();
    assert_eq!(termination.actor, "parent_runtime");
    assert_eq!(termination.reason, "no longer needed");

    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallStarted { audit: Some(audit), .. }
            if audit.action == "allow"
                && audit.capability == "write"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(true), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"termination_requested\""));
    assert!(tool_result.contains("\"status\":\"terminating\""));
    assert!(tool_result.contains("\"actor\":\"parent_runtime\""));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_unknown_child() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("missing-child-run-{}", uuid::Uuid::new_v4());

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "stop missing child",
            "mode": "graceful"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"not_found\""));
    assert!(tool_result.contains(tool_call.arguments["child_run_id"].as_str().unwrap()));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_already_terminal() {
    let mut state = create_test_agent_loop_state();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));
    state
        .child_run_registry()
        .mark_terminal(&child_run_id, ChildRunStatus::Completed, None);

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "already done",
            "mode": "graceful"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(true), audit: Some(audit), .. }
            if audit.action == "allow" && audit.capability == "write"
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Completed);
    assert!(record.termination.is_none());

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"already_terminal\""));
    assert!(tool_result.contains("\"status\":\"completed\""));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_escalates_under_escalating_policy() {
    let mut state = create_test_agent_loop_state();
    state.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::escalate_all();
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "needs review",
            "mode": "graceful"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::PauseTurn));
    assert!(state.turn_state.pending_confirmation().is_some());
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield { kind: alan_agent_protocol::YieldKind::Confirmation, payload, .. }
            if payload["details"]["replay_tool_call"]["tool_name"] == json!("terminate_child_run")
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Starting);
    assert!(record.termination.is_none());
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_terminate_child_run_denied_by_policy() {
    let mut state = create_test_agent_loop_state();
    let temp = TempDir::new().unwrap();
    std::fs::write(
        temp.path().join("policy.yaml"),
        r#"
rules:
  - id: deny-child-termination
    tool: terminate_child_run
    capability: write
    action: deny
    reason: child termination disabled
default_action: allow
"#,
    )
    .unwrap();
    state.runtime_config.governance = alan_agent_protocol::GovernanceConfig {
        profile: alan_agent_protocol::GovernanceProfile::Autonomous,
        policy_path: None,
    };
    state.runtime_config.policy_engine =
        crate::policy::PolicyEngine::load_or_default(Some(&temp.path().join("policy.yaml")));
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    state
        .child_run_registry()
        .register(test_child_run_record(&child_run_id, &state.process_path()));

    let tool_call = NormalizedToolCall {
        id: "call_terminate".to_string(),
        name: "terminate_child_run".to_string(),
        arguments: json!({
            "child_run_id": child_run_id,
            "reason": "policy should deny",
            "mode": "graceful"
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: false
        }
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "deny" && audit.rule_id.as_deref() == Some("deny-child-termination")
    )));

    let record = state
        .child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Starting);
    assert!(record.termination.is_none());

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("child termination disabled"));
}

#[tokio::test]
async fn test_try_handle_non_virtual_tool() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "test.txt"}),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::NotVirtual));
}

#[tokio::test]
async fn test_try_handle_unknown_tool() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "unknown_tool".to_string(),
        arguments: json!({}),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::NotVirtual));
}
