use super::*;
use crate::{
    config::Config,
    rollout::{RolloutItem, RolloutRecorder},
    runtime::{
        ChildRunRecord, ChildRunStatus, NamespaceRuntimeEnvironment, RuntimeConfig,
        RuntimeEnvironment, TurnState, global_child_run_registry, turn_state::TurnActivityState,
    },
    session::Session,
    skills::{
        ActiveSkillEnvelope, ResolvedCapabilityView, ResolvedSkillExecution, ScopedPackageDir,
        SkillActivationReason, SkillExecutionResolutionSource, SkillHostCapabilities,
        SkillMetadata, SkillScope,
    },
    tools::ToolRegistry,
};
use alan_agentfs::AgentFs;
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_shell::Shell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn namespace_environment_for_virtual_tool_test() -> RuntimeEnvironment {
    let agentfs = Arc::new(AgentFs::new());
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent/1",
        InProcessTransport::new(agentfs),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
        root, "/agent/1", "default",
    ))
}

fn create_test_agent_loop_state() -> super::super::agent_loop::RuntimeLoopState {
    let config = Config::default();
    let session = Session::new();
    let mut tools = ToolRegistry::new();
    tools.set_default_cwd(PathBuf::from("/tmp/alan-delegated-parent"));
    let runtime_config = RuntimeConfig::default();
    let mut prompt_cache = crate::runtime::prompt_cache::PromptAssemblyCache::new(Vec::new());
    prompt_cache.set_host_capabilities(
        SkillHostCapabilities::default()
            .with_runtime_defaults()
            .with_delegated_skill_invocation(),
    );

    super::super::agent_loop::RuntimeLoopState {
        workspace_id: "test-workspace".to_string(),
        workspace_root_dir: None,
        session,
        current_submission_id: None,
        environment: namespace_environment_for_virtual_tool_test(),
        tool_catalog: tools,
        core_config: config,
        runtime_config,
        workspace_persona_dirs: Vec::new(),
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
    state.environment = RuntimeEnvironment::namespace(NamespaceRuntimeEnvironment::new(
        root, "/agent/1", "default",
    ));
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
        scope: SkillScope::Repo,
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

fn capability_view_for_workspace_skill(workspace_root: &std::path::Path) -> ResolvedCapabilityView {
    ResolvedCapabilityView::from_package_dirs(vec![ScopedPackageDir {
        path: workspace_root.join(".alan/agents/default/skills"),
        scope: SkillScope::Repo,
    }])
}

fn test_child_run_record(child_run_id: &str, parent_session_id: &str) -> ChildRunRecord {
    ChildRunRecord::new(
        child_run_id.to_string(),
        parent_session_id.to_string(),
        format!("child-session-{child_run_id}"),
        Some("/tmp/alan-delegated-parent".to_string()),
        Some("/proc/42".to_string()),
        Some("repo-coding".to_string()),
    )
}

fn tool_result_text_for_call(
    state: &super::super::agent_loop::RuntimeLoopState,
    call_id: &str,
) -> String {
    state
        .session
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

#[test]
fn test_virtual_tool_definitions_include_all_runtime_virtual_tools() {
    let defs = virtual_tool_definitions(false);
    assert_eq!(defs.len(), 4);
    assert!(defs.iter().any(|d| d.name == "request_confirmation"));
    assert!(defs.iter().any(|d| d.name == "request_mount"));
    assert!(defs.iter().any(|d| d.name == "request_user_input"));
    assert!(defs.iter().any(|d| d.name == "update_plan"));
    assert!(!defs.iter().any(|d| d.name == "invoke_delegated_skill"));
}

#[test]
fn test_virtual_tool_definitions_can_include_delegated_skill() {
    let defs = virtual_tool_definitions(true);
    assert!(defs.iter().any(|d| d.name == "invoke_delegated_skill"));
    assert!(defs.iter().any(|d| d.name == "terminate_child_run"));
}

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
fn test_request_mount_tool_definition() {
    let def = request_mount_tool_definition();
    assert_eq!(def.name, "request_mount");
    assert!(def.description.contains("host directory mount"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(
        def.parameters["properties"]["namespace_path"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["host_path"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["access"]["enum"],
        json!(["read_only", "read_write"])
    );
    assert_eq!(
        def.parameters["required"],
        json!(["namespace_path", "host_path", "access", "reason"])
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
fn test_invoke_delegated_skill_tool_definition() {
    let def = invoke_delegated_skill_tool_definition();
    assert_eq!(def.name, "invoke_delegated_skill");
    assert!(def.description.contains("delegated skill"));
    assert_eq!(def.parameters["type"], "object");
    assert_eq!(def.parameters["properties"]["skill_id"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["skill_id"]["maxLength"],
        MAX_DELEGATED_SKILL_ID_CHARS
    );
    assert_eq!(def.parameters["properties"]["target"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["target"]["maxLength"],
        MAX_DELEGATED_TARGET_CHARS
    );
    assert_eq!(def.parameters["properties"]["task"]["type"], "string");
    assert_eq!(
        def.parameters["properties"]["task"]["maxLength"],
        MAX_DELEGATED_TASK_CHARS
    );
    assert_eq!(
        def.parameters["properties"]["workspace_root"]["type"],
        "string"
    );
    assert_eq!(def.parameters["properties"]["cwd"]["type"], "string");
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

#[test]
fn test_parse_mount_request_valid() {
    let request = parse_mount_request(&json!({
        "namespace_path": "/mnt/project",
        "host_path": "/Users/morris/Developer/alan",
        "access": "read_only",
        "reason": "Read project docs"
    }))
    .expect("valid mount request");

    assert_eq!(request.namespace_path, "/mnt/project");
    assert_eq!(
        request.host_path,
        PathBuf::from("/Users/morris/Developer/alan")
    );
    assert_eq!(request.access, MountRequestAccess::ReadOnly);
    assert_eq!(request.reason, "Read project docs");
    assert_eq!(request.payload()["access"], "read_only");
}

#[test]
fn test_parse_mount_request_rejects_invalid_fields() {
    let cases = [
        (
            json!({
                "namespace_path": "/proc/project",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/../project",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/llm",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/llm/connections",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/mem",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/route/send",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "relative/path",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "C:\\",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "\\\\server\\share\\",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/./alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris//alan",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/alan/",
                "access": "read_only",
                "reason": "Read project docs"
            }),
            "host_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/Developer/alan",
                "access": "admin",
                "reason": "Read project docs"
            }),
            "access",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/morris/Developer/alan",
                "access": "read_only",
                "reason": "   "
            }),
            "reason",
        ),
    ];

    for (args, expected_error_field) in cases {
        let error = parse_mount_request(&args).expect_err("expected invalid mount request");
        assert!(
            error.contains(expected_error_field),
            "expected error for {expected_error_field}, got {error}"
        );
    }
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

#[test]
fn test_parse_delegated_skill_invocation_request_valid() {
    let args = json!({
        "skill_id": "repo-review",
        "target": "reviewer",
        "task": "Review the current diff and summarize risks."
    });

    let result = parse_delegated_skill_invocation_request(&args).unwrap();
    assert_eq!(result.skill_id, "repo-review");
    assert_eq!(result.target, "reviewer");
    assert_eq!(result.task, "Review the current diff and summarize risks.");
}

#[test]
fn test_parse_delegated_skill_invocation_request_treats_empty_optional_paths_as_absent() {
    let args = json!({
        "skill_id": "repo-review",
        "target": "reviewer",
        "task": "Review the current diff and summarize risks.",
        "workspace_root": "",
        "cwd": "   "
    });

    let result = parse_delegated_skill_invocation_request(&args).unwrap();
    assert_eq!(result.workspace_root, None);
    assert_eq!(result.cwd, None);
}

#[test]
fn test_parse_delegated_skill_invocation_request_rejects_empty_fields() {
    let missing = json!({
        "skill_id": "repo-review",
        "target": "reviewer"
    });
    assert!(parse_delegated_skill_invocation_request(&missing).is_none());

    let empty = json!({
        "skill_id": "repo-review",
        "target": "reviewer",
        "task": "   "
    });
    assert!(parse_delegated_skill_invocation_request(&empty).is_none());
}

#[test]
fn test_parse_delegated_skill_invocation_request_accepts_bounded_timeout() {
    let request = parse_delegated_skill_invocation_request(&json!({
        "skill_id": "repo-review",
        "target": "reviewer",
        "task": "Review the current diff.",
        "timeout_secs": 600
    }))
    .expect("expected delegated request");

    assert_eq!(request.timeout_secs, Some(600));
}

#[test]
fn test_parse_delegated_skill_invocation_request_rejects_invalid_timeout() {
    assert!(
        parse_delegated_skill_invocation_request(&json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff.",
            "timeout_secs": 0
        }))
        .is_none()
    );
    assert!(
        parse_delegated_skill_invocation_request(&json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff.",
            "timeout_secs": (MAX_DELEGATED_TIMEOUT_SECS + 1)
        }))
        .is_none()
    );
}

#[test]
fn test_build_bounded_delegated_invocation_persistence_truncates_fields() {
    let request = DelegatedSkillInvocationRequest {
        skill_id: "s".repeat(MAX_DELEGATED_SKILL_ID_CHARS + 40),
        target: "t".repeat(MAX_DELEGATED_TARGET_CHARS + 40),
        task: "x".repeat(MAX_DELEGATED_TASK_CHARS + 200),
        workspace_root: Some(PathBuf::from(format!(
            "/tmp/{}",
            "w".repeat(MAX_DELEGATED_PATH_CHARS + 20)
        ))),
        cwd: Some(PathBuf::from(format!(
            "/tmp/{}",
            "c".repeat(MAX_DELEGATED_PATH_CHARS + 20)
        ))),
        timeout_secs: Some(DEFAULT_DELEGATED_TIMEOUT_SECS),
    };
    let result = DelegatedSkillResult::failed(
        format!(
            "Delegated skill '{}' resolved to delegated target '{}', but delegated launch support is not yet available in this runtime.",
            request.skill_id, request.target
        ),
        Some(json!({
            "error_kind": "runtime_child_launch_unavailable"
        })),
    );

    let child_run = Some(DelegatedChildRunReference {
        session_id: "child-session".to_string(),
        child_run_id: None,
        process_path: Some("/proc/42".to_string()),
        state_ref: None,
        terminal_status: "completed".to_string(),
    });
    let (arguments, record, rollout_record) =
        build_bounded_delegated_invocation_persistence(&request, result, child_run);

    let skill_id = arguments["skill_id"].as_str().unwrap();
    let target = arguments["target"].as_str().unwrap();
    let task = arguments["task"].as_str().unwrap();
    assert!(skill_id.chars().count() <= MAX_DELEGATED_SKILL_ID_CHARS);
    assert!(target.chars().count() <= MAX_DELEGATED_TARGET_CHARS);
    assert!(task.chars().count() <= MAX_DELEGATED_TASK_CHARS);
    assert!(skill_id.ends_with("..."));
    assert!(target.ends_with("..."));
    assert!(task.ends_with("..."));
    assert!(
        arguments["workspace_root"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= MAX_DELEGATED_PATH_CHARS
    );
    assert!(arguments["cwd"].as_str().unwrap().chars().count() <= MAX_DELEGATED_PATH_CHARS);
    assert_eq!(
        arguments["timeout_secs"].as_u64(),
        Some(DEFAULT_DELEGATED_TIMEOUT_SECS)
    );
    assert!(record.result.summary.chars().count() <= MAX_DELEGATED_RESULT_SUMMARY_CHARS);
    assert!(record.result.summary.ends_with("..."));
    assert_eq!(
        rollout_record.child_run.as_ref().unwrap().session_id,
        "child-session"
    );
}

#[test]
fn test_build_bounded_delegated_invocation_persistence_keeps_child_run_out_of_tape_record() {
    let request = DelegatedSkillInvocationRequest {
        skill_id: "repo-review".to_string(),
        target: "reviewer".to_string(),
        task: "Review the current diff and summarize risks.".to_string(),
        workspace_root: Some(PathBuf::from("/tmp/repo")),
        cwd: Some(PathBuf::from("/tmp/repo/src")),
        timeout_secs: Some(600),
    };
    let result = DelegatedSkillResult::completed("Delegated review completed.", None);
    let child_run = Some(DelegatedChildRunReference {
        session_id: "child-session".to_string(),
        child_run_id: None,
        process_path: Some("/proc/42".to_string()),
        state_ref: None,
        terminal_status: "completed".to_string(),
    });

    let (_, tape_record, rollout_record) =
        build_bounded_delegated_invocation_persistence(&request, result, child_run);
    let tape_payload = serde_json::to_value(&tape_record).unwrap();
    let rollout_payload = serde_json::to_value(&rollout_record).unwrap();

    assert!(tape_payload.get("child_run").is_none());
    assert_eq!(
        rollout_payload["child_run"]["session_id"],
        json!("child-session")
    );
    assert_eq!(
        rollout_payload["child_run"]["process_path"],
        json!("/proc/42")
    );
    assert!(rollout_payload["child_run"].get("rollout_path").is_none());
    assert_eq!(tape_payload["workspace_root"], json!("/tmp/repo"));
    assert_eq!(tape_payload["cwd"], json!("/tmp/repo/src"));
    assert_eq!(tape_payload["timeout_secs"], json!(600));
}

#[test]
fn test_build_bounded_delegated_invocation_persistence_bounds_result_sidecars() {
    let request = DelegatedSkillInvocationRequest {
        skill_id: "repo-review".to_string(),
        target: "reviewer".to_string(),
        task: "Review the current diff and summarize risks.".to_string(),
        workspace_root: Some(PathBuf::from("/tmp/repo")),
        cwd: Some(PathBuf::from("/tmp/repo/src")),
        timeout_secs: None,
    };
    let mut result = DelegatedSkillResult::failed("Delegated review failed.", None);
    result.child_run = Some(json!({
        "id": "child-run-1",
        "status": "failed",
        "warnings": vec!["child-warning".repeat(200); MAX_DELEGATED_RESULT_WARNINGS + 8],
        "large_metadata": "x".repeat(MAX_DELEGATED_CHILD_RUN_METADATA_CHARS * 2)
    }));
    result.warnings = (0..(MAX_DELEGATED_RESULT_WARNINGS + 3))
        .map(|index| {
            format!(
                "warning-{index:03}-{}",
                "x".repeat(MAX_DELEGATED_RESULT_WARNING_CHARS)
            )
        })
        .collect();

    let (_, tape_record, _) =
        build_bounded_delegated_invocation_persistence(&request, result, None);

    assert_eq!(
        tape_record.result.warnings.len(),
        MAX_DELEGATED_RESULT_WARNINGS
    );
    assert!(tape_record.result.warnings[0].starts_with("warning-003-"));
    assert!(
        tape_record
            .result
            .warnings
            .iter()
            .all(|warning| warning.chars().count() <= MAX_DELEGATED_RESULT_WARNING_CHARS)
    );
    assert!(tape_record.result.warnings.last().unwrap().ends_with("..."));
    assert!(
        tape_record
            .result
            .child_run
            .as_ref()
            .unwrap()
            .to_string()
            .len()
            <= MAX_DELEGATED_CHILD_RUN_METADATA_CHARS
    );
    let truncation = tape_record.result.truncation.unwrap();
    assert!(truncation.child_run);
    assert!(truncation.warnings);
    assert!(truncation.original_child_run_chars.unwrap() > MAX_DELEGATED_CHILD_RUN_METADATA_CHARS);
    assert_eq!(
        truncation.original_warning_count,
        Some(MAX_DELEGATED_RESULT_WARNINGS + 3)
    );
}

#[test]
fn test_delegated_result_from_completed_child_prefers_structured_output_summary() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        session_id: "child-session".to_string(),
        child_run_id: None,
        rollout_path: None,
        output_text: "{\"status\":\"completed\",\"summary\":\"Structured delivery\"}".to_string(),
        turn_summary: Some("Turn summary".to_string()),
        structured_output: Some(json!({
            "status": "completed",
            "summary": "Structured delivery"
        })),
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    };

    let delegated = delegated_result_from_completed_child(&child_result, None);
    assert_eq!(delegated.summary, "Structured delivery");
    assert_eq!(
        delegated
            .structured_output
            .as_ref()
            .and_then(|value| value.get("status")),
        Some(&json!("completed"))
    );
}

#[test]
fn test_delegated_result_from_completed_child_prefers_output_text_over_turn_summary() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        session_id: "child-session".to_string(),
        child_run_id: None,
        rollout_path: None,
        output_text: "Verification was environment_blocked: pytest was not installed.".to_string(),
        turn_summary: Some("Task completed".to_string()),
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    };

    let delegated = delegated_result_from_completed_child(&child_result, None);
    assert_eq!(
        delegated.summary,
        "Verification was environment_blocked: pytest was not installed."
    );
    assert!(delegated.structured_output.is_none());
}

#[test]
fn test_delegated_result_from_completed_child_inlines_short_output() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        session_id: "child-session".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: "Short delegated delivery.".to_string(),
        turn_summary: Some("Turn summary".to_string()),
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    };

    let delegated = delegated_result_from_completed_child(&child_result, None);
    assert_eq!(
        delegated.output_text.as_deref(),
        Some("Short delegated delivery.")
    );
    assert!(delegated.output_ref.is_none());
    assert!(delegated.truncation.is_none());
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.get("child_run_id")),
        Some(&json!("child-run-1"))
    );
}

#[test]
fn test_delegated_result_from_completed_child_redacts_short_output() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        session_id: "child-session".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: "api_key=child-secret\nAuthorization: Bearer child-token".to_string(),
        turn_summary: Some("Turn summary".to_string()),
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    };

    let delegated = delegated_result_from_completed_child(&child_result, None);
    let output = delegated.output_text.expect("redacted inline output");
    assert!(!output.contains("child-secret"));
    assert!(!output.contains("child-token"));
    assert!(output.contains("[REDACTED reason=secret_key]"));
}

#[test]
fn test_delegated_result_from_completed_child_uses_ref_for_long_output() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Completed,
        session_id: "child-session".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: "x".repeat(MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1),
        turn_summary: Some("Turn summary".to_string()),
        structured_output: None,
        warnings: Vec::new(),
        error_message: None,
        pause: None,
        child_run: None,
    };

    let output_ref = DelegatedSkillOutputRef {
        path: "/agent/1/actions/a0/output".to_string(),
        offset: Some(0),
        length: Some((MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1) as u64),
        debug: Some(DelegatedSkillOutputDebugMetadata {
            session_id: "child-session".to_string(),
            rollout_path: None,
            field: "output_text".to_string(),
        }),
    };
    let delegated = delegated_result_from_completed_child(&child_result, Some(&output_ref));
    assert!(delegated.output_text.is_none());
    assert_eq!(
        delegated.output_ref.as_ref().map(|reference| (
            reference.path.as_str(),
            reference.offset,
            reference.length
        )),
        Some((
            "/agent/1/actions/a0/output",
            Some(0),
            Some((MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1) as u64)
        ))
    );
    assert_eq!(
        delegated
            .truncation
            .as_ref()
            .map(|truncation| truncation.output_text),
        Some(true)
    );
    assert_eq!(
        delegated
            .truncation
            .as_ref()
            .and_then(|truncation| truncation.original_output_chars),
        Some(MAX_DELEGATED_RESULT_OUTPUT_INLINE_CHARS + 1)
    );
}

#[test]
fn test_delegated_result_from_timed_out_child_includes_metadata() {
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::TimedOut,
        session_id: "child-session".to_string(),
        child_run_id: Some("child-run-1".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
        output_text: "partial output before timeout".to_string(),
        turn_summary: None,
        structured_output: None,
        warnings: vec!["child was idle".to_string()],
        error_message: Some("idle timeout exceeded".to_string()),
        pause: None,
        child_run: Some(test_child_run_record("child-run-1", "parent-session")),
    };

    let output_ref = DelegatedSkillOutputRef {
        path: "/agent/1/actions/a1/output".to_string(),
        offset: Some(0),
        length: Some(child_result.output_text.len() as u64),
        debug: Some(DelegatedSkillOutputDebugMetadata {
            session_id: "child-session".to_string(),
            rollout_path: None,
            field: "output_text".to_string(),
        }),
    };
    let delegated = delegated_result_from_child_result(&child_result, Some(output_ref));
    assert_eq!(delegated.status, DelegatedSkillResultStatus::Failed);
    assert_eq!(delegated.error_kind.as_deref(), Some("child_timed_out"));
    assert_eq!(
        delegated.error_message.as_deref(),
        Some("idle timeout exceeded")
    );
    assert_eq!(delegated.warnings, vec!["child was idle"]);
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.get("status")),
        Some(&json!("starting"))
    );
    assert_eq!(
        delegated.output_ref.as_ref().map(|reference| (
            reference.path.as_str(),
            reference.offset,
            reference.length
        )),
        Some((
            "/agent/1/actions/a1/output",
            Some(0),
            Some(child_result.output_text.len() as u64)
        ))
    );
    assert_eq!(
        delegated
            .truncation
            .as_ref()
            .map(|truncation| truncation.output_text),
        Some(true)
    );
    assert!(
        !serde_json::to_string(&delegated)
            .unwrap()
            .contains("/tmp/child-rollout.jsonl")
    );
}

#[test]
fn test_delegated_result_from_terminated_child_uses_namespace_state_reference() {
    let state_ref = DelegatedSkillOutputRef {
        path: "/agent/1/actions/a2/output".to_string(),
        offset: Some(0),
        length: Some(18),
        debug: Some(DelegatedSkillOutputDebugMetadata {
            session_id: "child-session".to_string(),
            rollout_path: None,
            field: "output_text".to_string(),
        }),
    };
    let child_result = ChildRuntimeResult {
        status: ChildRuntimeStatus::Terminated,
        session_id: "child-session".to_string(),
        child_run_id: Some("child-run-2".to_string()),
        rollout_path: Some(PathBuf::from("/tmp/terminated-child-rollout.jsonl")),
        output_text: "partial termination".to_string(),
        turn_summary: None,
        structured_output: None,
        warnings: Vec::new(),
        error_message: Some("operator requested stop".to_string()),
        pause: None,
        child_run: Some(
            test_child_run_record("child-run-2", "parent-session")
                .with_state_ref(state_ref.clone()),
        ),
    };

    let delegated = delegated_result_from_child_result(&child_result, Some(state_ref));
    let encoded = serde_json::to_string(&delegated).unwrap();
    assert_eq!(delegated.error_kind.as_deref(), Some("child_terminated"));
    assert_eq!(
        delegated
            .child_run
            .as_ref()
            .and_then(|value| value.pointer("/state_ref/path")),
        Some(&json!("/agent/1/actions/a2/output"))
    );
    assert!(encoded.contains("/proc/42"));
    assert!(!encoded.contains("/tmp/terminated-child-rollout.jsonl"));
}

#[test]
fn test_build_bounded_delegated_invocation_persistence_truncates_structured_output() {
    let request = DelegatedSkillInvocationRequest {
        skill_id: "repo-review".to_string(),
        target: "reviewer".to_string(),
        task: "Review the current diff and summarize risks.".to_string(),
        workspace_root: Some(PathBuf::from("/tmp/repo")),
        cwd: Some(PathBuf::from("/tmp/repo/src")),
        timeout_secs: None,
    };
    let result = DelegatedSkillResult::completed(
        "Delegated review completed.",
        Some(json!({
            "status": "completed",
            "summary": "Delegated review completed.",
            "details": "x".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2)
        })),
    );

    let (_, tape_record, _) =
        build_bounded_delegated_invocation_persistence(&request, result, None);
    let structured = tape_record.result.structured_output.unwrap();
    assert!(structured.to_string().len() <= MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
    assert_eq!(structured["status"], json!("completed"));
}

#[test]
fn test_build_bounded_delegated_invocation_persistence_truncates_oversized_summary() {
    let request = DelegatedSkillInvocationRequest {
        skill_id: "repo-review".to_string(),
        target: "reviewer".to_string(),
        task: "Review the current diff and summarize risks.".to_string(),
        workspace_root: Some(PathBuf::from("/tmp/repo")),
        cwd: Some(PathBuf::from("/tmp/repo/src")),
        timeout_secs: None,
    };
    let result = DelegatedSkillResult::completed(
        "Delegated review completed.",
        Some(json!({
            "status": "completed",
            "summary": "y".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2),
            "details": "x".repeat(MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS * 2)
        })),
    );

    let (_, tape_record, _) =
        build_bounded_delegated_invocation_persistence(&request, result, None);
    let structured = tape_record.result.structured_output.unwrap();
    let summary = structured["summary"]
        .as_str()
        .expect("summary should remain string");
    assert!(structured.to_string().len() <= MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
    assert!(summary.len() < MAX_DELEGATED_STRUCTURED_OUTPUT_CHARS);
    assert!(summary.ends_with("..."));
    assert_eq!(structured["status"], json!("completed"));
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
async fn test_try_handle_virtual_tool_call_request_mount_escalates_even_when_allowed() {
    let mut state = create_test_agent_loop_state();
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::allow_all();
    let temp = TempDir::new().unwrap();
    let host_path = std::fs::canonicalize(temp.path()).unwrap();
    let host_path_text = host_path.display().to_string();

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/project",
            "host_path": host_path_text.clone(),
            "access": "read_write",
            "reason": "Need to edit project files"
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

    let pending = state
        .turn_state
        .pending_confirmation()
        .expect("mount request should pause for confirmation");
    assert_eq!(
        pending.checkpoint_type,
        crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE
    );
    assert_eq!(pending.checkpoint_id, "mount_escalation_call_mount");
    assert_eq!(pending.details["tool_call_id"], "call_mount");
    assert_eq!(
        pending.details["mount_request"]["namespace_path"],
        "/mnt/project"
    );
    assert_eq!(
        pending.details["mount_request"]["host_path"],
        host_path_text
    );
    assert_eq!(pending.details["mount_request"]["access"], "read_write");
    assert_eq!(pending.details["policy"]["action"], "escalate");
    assert_eq!(
        pending.details["policy"]["reason"],
        "host mount grants require approval"
    );

    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallStarted { id, name, audit: None, .. }
            if id == "call_mount" && name == "request_mount"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { id, success: Some(true), audit: Some(audit), .. }
            if id == "call_mount"
                && audit.action == "escalate"
                && audit.reason.as_deref() == Some("host mount grants require approval")
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield { kind: alan_agent_protocol::YieldKind::Confirmation, payload, .. }
            if payload["checkpoint_type"] == json!(crate::approval::MOUNT_ESCALATION_CHECKPOINT_TYPE)
                && payload["details"]["mount_request"]["host_path"] == json!(host_path_text.clone())
                && payload["default_option"] == json!("reject")
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_mount_does_not_probe_host_existence() {
    let mut state = create_test_agent_loop_state();
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::allow_all();
    let missing_host_path = TempDir::new()
        .unwrap()
        .path()
        .join("missing-host-mount-root");
    let missing_host_path_text = missing_host_path.display().to_string();

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/missing",
            "host_path": missing_host_path_text.clone(),
            "access": "read_only",
            "reason": "Need to inspect files if available"
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

    let pending = state
        .turn_state
        .pending_confirmation()
        .expect("syntactically valid mount request should pause for confirmation");
    assert_eq!(
        pending.details["mount_request"]["host_path"],
        missing_host_path_text
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield { kind: alan_agent_protocol::YieldKind::Confirmation, payload, .. }
            if payload["details"]["mount_request"]["host_path"] == json!(missing_host_path_text.clone())
                && payload["default_option"] == json!("reject")
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_mount_denied_by_policy() {
    let mut state = create_test_agent_loop_state();
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::deny_all();
    let temp = TempDir::new().unwrap();
    let host_path = std::fs::canonicalize(temp.path()).unwrap();

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/project",
            "host_path": host_path.display().to_string(),
            "access": "read_only",
            "reason": "Need to inspect project files"
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
    assert!(state.turn_state.pending_confirmation().is_none());
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "deny"
    )));

    let tool_result = tool_result_text_for_call(&state, "call_mount");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("/mnt/project"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_mount_read_only_uses_read_policy() {
    let mut state = create_test_agent_loop_state();
    let temp = TempDir::new().unwrap();
    let host_dir = temp.path().join("ssh");
    let host_path = host_dir.display().to_string();
    std::fs::write(
        temp.path().join("policy.yaml"),
        format!(
            r#"
rules:
  - id: deny-sensitive-read-mount
    tool: request_mount
    capability: read
    match_path_prefix: "{}"
    action: deny
    reason: sensitive host reads are not allowed
default_action: allow
"#,
            host_path
        ),
    )
    .unwrap();
    state.runtime_config.policy_engine =
        crate::policy::PolicyEngine::load_or_default(Some(temp.path()));

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/ssh",
            "host_path": host_path,
            "access": "read_only",
            "reason": "Need to inspect SSH configuration"
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
    assert!(state.turn_state.pending_confirmation().is_none());
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "deny"
                && audit.capability == "read"
                && audit.rule_id.as_deref() == Some("deny-sensitive-read-mount")
    )));

    let tool_result = tool_result_text_for_call(&state, "call_mount");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("sensitive host reads are not allowed"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_mount_read_write_honors_read_denies() {
    let mut state = create_test_agent_loop_state();
    let temp = TempDir::new().unwrap();
    let host_dir = temp.path().join("ssh");
    let host_path = host_dir.display().to_string();
    std::fs::write(
        temp.path().join("policy.yaml"),
        format!(
            r#"
rules:
  - id: deny-sensitive-read-mount
    tool: request_mount
    capability: read
    match_path_prefix: "{}"
    action: deny
    reason: sensitive host reads are not allowed
default_action: allow
"#,
            host_path
        ),
    )
    .unwrap();
    state.runtime_config.policy_engine =
        crate::policy::PolicyEngine::load_or_default(Some(temp.path()));

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/ssh",
            "host_path": host_path,
            "access": "read_write",
            "reason": "Need to update SSH configuration"
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
    assert!(state.turn_state.pending_confirmation().is_none());
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { success: Some(false), audit: Some(audit), .. }
            if audit.action == "deny"
                && audit.capability == "read"
                && audit.rule_id.as_deref() == Some("deny-sensitive-read-mount")
    )));

    let tool_result = tool_result_text_for_call(&state, "call_mount");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("sensitive host reads are not allowed"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_request_mount_rejects_invalid_request() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/proc/project",
            "host_path": "relative/path",
            "access": "read_only",
            "reason": "Need files"
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
    assert!(state.turn_state.pending_confirmation().is_none());
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::Yield {
            kind: alan_agent_protocol::YieldKind::Confirmation,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted {
            success: Some(false),
            audit: None,
            ..
        }
    )));

    let tool_result = tool_result_text_for_call(&state, "call_mount");
    assert!(tool_result.contains("\"status\":\"invalid_request\""));
    assert!(tool_result.contains("namespace_path"));
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

    let prompt_view = state.session.tape.prompt_view();
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
    global_child_run_registry().register(test_child_run_record(&child_run_id, &state.session.id));

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

    let record = global_child_run_registry()
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
    global_child_run_registry().register(test_child_run_record(&child_run_id, &state.session.id));
    global_child_run_registry().mark_terminal(&child_run_id, ChildRunStatus::Completed, None);

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

    let record = global_child_run_registry()
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
    global_child_run_registry().register(test_child_run_record(&child_run_id, &state.session.id));

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

    let record = global_child_run_registry()
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
        crate::policy::PolicyEngine::load_or_default(Some(temp.path()));
    let child_run_id = format!("child-run-{}", uuid::Uuid::new_v4());
    global_child_run_registry().register(test_child_run_record(&child_run_id, &state.session.id));

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

    let record = global_child_run_registry()
        .get(tool_call.arguments["child_run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(record.status, ChildRunStatus::Starting);
    assert!(record.termination.is_none());

    let tool_result = tool_result_text_for_call(&state, "call_terminate");
    assert!(tool_result.contains("\"status\":\"blocked_by_policy\""));
    assert!(tool_result.contains("child termination disabled"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill() {
    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir =
        Some(PathBuf::from("/tmp/alan-delegated-parent/.alan/memory"));
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.target,
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        }
    );
    assert_eq!(
        spec.handles,
        vec![SpawnHandle::Workspace, SpawnHandle::ApprovalScope]
    );
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/tmp/alan-delegated-parent"))
    );
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/tmp/alan-delegated-parent"))
    );
    assert_eq!(
        spec.launch.timeout_secs,
        Some(DEFAULT_DELEGATED_TIMEOUT_SECS)
    );
    let requirements = &spec
        .delegated
        .as_ref()
        .expect("delegated launch should carry classified requirements")
        .requirements;
    assert!(requirements.contains(
        &alan_agent_protocol::DelegatedCapabilityRequirement::WorkspaceRead {
            path: Some(PathBuf::from("/tmp/alan-delegated-parent")),
        }
    ));
    assert!(
        requirements.contains(&alan_agent_protocol::DelegatedCapabilityRequirement::LlmConnection)
    );

    let prompt_view = state.session.tape.prompt_view();
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
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("\"task\":\"Review the current diff and summarize risks.\""));
    assert!(tool_result.contains("\"status\":\"completed\""));
    assert!(tool_result.contains("Delegated review completed."));
    assert!(tool_result.contains("child_run"));
    assert!(tool_result.contains("child-session"));
}

#[tokio::test]
async fn test_delegated_capability_rejection_is_recorded_on_parent_tape() {
    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir =
        Some(PathBuf::from("/tmp/alan-delegated-parent/.alan/memory"));
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let tool_call = NormalizedToolCall {
        id: "call_capability_mismatch".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review GitHub issue 42"
        }),
    };
    let decision = alan_agent_protocol::DelegatedCapabilityDecision {
        requirements: vec![alan_agent_protocol::DelegatedCapabilityRequirement::Github],
        namespace: alan_agent_protocol::DelegatedNamespaceSummary::default(),
        unsatisfied: vec![alan_agent_protocol::DelegatedCapabilityRequirement::Github],
        recovery: alan_agent_protocol::DelegatedCapabilityRecovery::ParentPath,
        narrowed_task: None,
    };
    let expected_decision = decision.clone();
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel| {
            Box::pin(async move { Err(anyhow::Error::new(DelegatedSpawnRejected { decision })) })
        },
    )
    .await
    .unwrap();

    let tool_result = tool_result_text_for_call(&state, "call_capability_mismatch");
    let record: DelegatedSkillInvocationRecord = serde_json::from_str(&tool_result).unwrap();
    assert_eq!(
        record.result.error_kind.as_deref(),
        Some("delegated_capability_mismatch")
    );
    assert_eq!(record.result.capability_decision, Some(expected_decision));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_from_catalog_without_activation()
{
    let temp = TempDir::new().unwrap();
    let workspace_root = temp.path().join("repo");
    let skill_dir = workspace_root.join(".alan/agents/default/skills/repo-review");
    std::fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Repo Review
description: Review repository changes
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("agents/reviewer/agent.toml"),
        "openai_responses_model = \"gpt-5.4\"\n",
    )
    .unwrap();

    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir =
        Some(PathBuf::from("/tmp/alan-delegated-parent/.alan/memory"));
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_skill(&workspace_root),
            Vec::new(),
            SkillHostCapabilities::default()
                .with_runtime_defaults()
                .with_delegated_skill_invocation(),
        );

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok());
    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.target,
        alan_agent_protocol::SpawnTarget::PackageChildAgent {
            package_id: "skill:repo-review".to_string(),
            export_name: "reviewer".to_string(),
        }
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_rejects_when_runtime_support_is_disabled()
 {
    let temp = TempDir::new().unwrap();
    let workspace_root = temp.path().join("repo");
    let skill_dir = workspace_root.join(".alan/agents/default/skills/repo-review");
    std::fs::create_dir_all(skill_dir.join("agents/reviewer")).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: Repo Review
description: Review repository changes
---

# Instructions
Use this skill when asked.
"#,
    )
    .unwrap();
    std::fs::write(
        skill_dir.join("agents/reviewer/agent.toml"),
        "openai_responses_model = \"gpt-5.4\"\n",
    )
    .unwrap();

    let mut state = create_test_agent_loop_state();
    state.prompt_cache =
        crate::runtime::prompt_cache::PromptAssemblyCache::with_fixed_capability_view(
            capability_view_for_workspace_skill(&workspace_root),
            Vec::new(),
            SkillHostCapabilities::default().with_runtime_defaults(),
        );

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            panic!("unsupported runtimes must not spawn delegated runtimes");
            #[allow(unreachable_code)]
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: String::new(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let prompt_view = state.session.tape.prompt_view();
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
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("delegated_invocation_unavailable"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_keeps_workspace_root_separate_from_nested_cwd()
 {
    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir =
        Some(PathBuf::from("/tmp/alan-delegated-parent/.alan/memory"));
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/tmp/alan-delegated-parent/nested/src"));
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/tmp/alan-delegated-parent/nested/src"))
    );
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/tmp/alan-delegated-parent"))
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_honors_explicit_workspace_root_and_cwd()
 {
    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir = Some(PathBuf::from("/tmp/alan-home/.alan/memory"));
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/tmp/alan-home/nested/src"));
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode.",
            "workspace_root": "/Users/morris/Developer/Alan",
            "cwd": "/Users/morris/Developer/Alan/docs"
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/Users/morris/Developer/Alan"))
    );
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/Users/morris/Developer/Alan/docs"))
    );
    let tool_profile = spec
        .runtime_overrides
        .tool_profile
        .expect("workspace-inspect should use a read-only tool profile");
    assert_eq!(
        tool_profile.allowed_tools,
        WORKSPACE_INSPECT_READ_ONLY_TOOLS
            .iter()
            .map(|tool| (*tool).to_string())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_does_not_promote_cwd_to_workspace_root()
 {
    let mut state = create_test_agent_loop_state();
    state.core_config.memory.workspace_dir = Some(PathBuf::from("/tmp/alan-home/.alan/memory"));
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/tmp/alan-home/nested/src"));
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode.",
            "cwd": "/Users/morris/Developer/Alan/docs"
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/tmp/alan-home"))
    );
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/Users/morris/Developer/Alan/docs"))
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_uses_bound_workspace_root_with_custom_memory_dir()
 {
    let mut state = create_test_agent_loop_state();
    state.workspace_root_dir = Some(PathBuf::from("/Users/morris/Developer/Alan"));
    state.core_config.memory.workspace_dir = Some(PathBuf::from("/tmp/custom-memory-layout"));
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/Users/morris/Developer/Alan/docs"));
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode."
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/Users/morris/Developer/Alan"))
    );
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/Users/morris/Developer/Alan/docs"))
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_normalizes_relative_workspace_root_and_cwd()
 {
    let mut state = create_test_agent_loop_state();
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/Users/morris/Developer"));
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode.",
            "workspace_root": "Alan",
            "cwd": "docs"
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.workspace_root,
        Some(PathBuf::from("/Users/morris/Developer/Alan"))
    );
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/Users/morris/Developer/Alan/docs"))
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_rejects_unresolvable_relative_cwd()
 {
    let mut state = create_test_agent_loop_state();
    *state.tool_catalog_mut_for_test() = ToolRegistry::new();
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode.",
            "cwd": "docs"
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            panic!("relative cwd without a base must not spawn a child runtime");
            #[allow(unreachable_code)]
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: String::new(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;

    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let prompt_view = state.session.tape.prompt_view();
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
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("relative_launch_path_unresolvable"));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_leaves_workspace_root_unset_without_memory_context()
 {
    let mut state = create_test_agent_loop_state();
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/tmp/alan-delegated-parent/nested/src"));
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let captured_spec = Arc::new(Mutex::new(None));
    let captured_spec_for_spawn = Arc::clone(&captured_spec);
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, spec, _cancel| {
            let captured_spec = Arc::clone(&captured_spec_for_spawn);
            Box::pin(async move {
                *captured_spec.lock().unwrap() = Some(spec);
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("done".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let spec = captured_spec
        .lock()
        .unwrap()
        .clone()
        .expect("expected delegated spawn spec");
    assert_eq!(
        spec.launch.cwd,
        Some(PathBuf::from("/tmp/alan-delegated-parent/nested/src"))
    );
    assert_eq!(spec.launch.workspace_root, None);
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_records_successful_tool_call() {
    let temp = TempDir::new().unwrap();
    let mut state = create_test_agent_loop_state();
    state.session = Session::new_with_recorder_in_dir("gpt-5-mini", temp.path())
        .await
        .unwrap();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let rollout_path = state.session.rollout_path().unwrap().clone();
    let mut tool_call = None;
    for _ in 0..20 {
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        tool_call = items.into_iter().find_map(|item| match item {
            RolloutItem::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        });
        if tool_call.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let tool_call = tool_call.expect("expected delegated tool-call rollout record");
    assert_eq!(tool_call.name, "invoke_delegated_skill");
    assert!(tool_call.success);
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_records_normalized_launch_paths()
{
    let temp = TempDir::new().unwrap();
    let mut state = create_test_agent_loop_state();
    state.session = Session::new_with_recorder_in_dir("gpt-5-mini", temp.path())
        .await
        .unwrap();
    state
        .tool_catalog_mut_for_test()
        .set_default_cwd(PathBuf::from("/Users/morris/Developer"));
    activate_test_delegated_skill(&mut state, "workspace-inspect", "workspace-reader");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "workspace-inspect",
            "target": "workspace-reader",
            "task": "Read docs and explain full steward mode.",
            "workspace_root": "Alan",
            "cwd": "docs"
        }),
    };

    let mut emit = |_event: Event| async {};
    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: Some("Delegated review completed.".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());

    let rollout_path = state.session.rollout_path().unwrap().clone();
    let mut recorded_tool_call = None;
    for _ in 0..20 {
        let items = RolloutRecorder::load_history(&rollout_path).await.unwrap();
        recorded_tool_call = items.into_iter().find_map(|item| match item {
            RolloutItem::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        });
        if recorded_tool_call.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let recorded_tool_call = recorded_tool_call.expect("expected delegated tool-call record");
    assert_eq!(
        recorded_tool_call.arguments["workspace_root"],
        json!("/Users/morris/Developer/Alan")
    );
    assert_eq!(
        recorded_tool_call.arguments["cwd"],
        json!("/Users/morris/Developer/Alan/docs")
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_bounds_preview_and_payload() {
    let mut state = create_test_agent_loop_state();
    let long_skill_id = format!("repo-review-{}", "x".repeat(150));
    let long_target = format!("reviewer-{}", "y".repeat(150));
    let long_task = "Review the current diff and summarize risks. ".repeat(80);
    activate_test_delegated_skill(&mut state, &long_skill_id, &long_target);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": long_skill_id,
            "target": long_target,
            "task": long_task
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            Box::pin(async {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: Some("delegated-result ".repeat(40)),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let preview = events
        .iter()
        .find_map(|event| match event {
            Event::ToolCallCompleted {
                id,
                result_preview: Some(preview),
                ..
            } if id == "call_1" => Some(preview.as_str()),
            _ => None,
        })
        .expect("expected delegated skill preview");
    assert!(preview.chars().count() <= 163);
    assert!(preview.ends_with("..."));

    let prompt_view = state.session.tape.prompt_view();
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
        .expect("expected delegated skill tool result");
    let payload: serde_json::Value = serde_json::from_str(&tool_result).unwrap();
    assert!(payload["skill_id"].as_str().unwrap().chars().count() <= MAX_DELEGATED_SKILL_ID_CHARS);
    assert!(payload["target"].as_str().unwrap().chars().count() <= MAX_DELEGATED_TARGET_CHARS);
    assert!(payload["task"].as_str().unwrap().chars().count() <= MAX_DELEGATED_TASK_CHARS);
    assert!(
        payload["result"]["summary"]
            .as_str()
            .unwrap()
            .chars()
            .count()
            <= MAX_DELEGATED_RESULT_SUMMARY_CHARS
    );
}

#[tokio::test]
async fn long_delegated_output_uses_parent_resolvable_namespace_reference() {
    let (mut state, shell) = create_namespace_agent_loop_state_and_shell();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    let tool_call = NormalizedToolCall {
        id: "call_long_child".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review local files"
        }),
    };
    let cancel = CancellationToken::new();
    let mut emit = |_event: Event| async {};

    let output_len = (1 << 20) + 1_024;
    handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        move |_state, _spec, _cancel| {
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: "child-session".to_string(),
                    child_run_id: Some("child-run".to_string()),
                    rollout_path: Some(PathBuf::from("/tmp/debug-child.jsonl")),
                    output_text: "x".repeat(output_len),
                    turn_summary: Some("Long child completed".to_string()),
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await
    .unwrap();

    let tool_result = tool_result_text_for_call(&state, "call_long_child");
    let record: DelegatedSkillInvocationRecord = serde_json::from_str(&tool_result).unwrap();
    let output_ref = record.result.output_ref.unwrap();
    assert_eq!(output_ref.path, "/agent/1/actions/a0/output");
    assert_eq!(
        output_ref
            .debug
            .as_ref()
            .and_then(|debug| debug.rollout_path.as_deref()),
        Some("/tmp/debug-child.jsonl")
    );
    let full = String::from_utf8(shell.cat(&output_ref.path).await.unwrap()).unwrap();
    assert_eq!(full.len(), output_len);
    let namespace_ref = crate::evidence::NamespaceEvidenceReference {
        path: output_ref.path,
        offset: output_ref.offset,
        length: output_ref.length,
    };
    let resolved = state
        .namespace_environment()
        .resolve_evidence_reference(&namespace_ref, None, None)
        .await
        .unwrap();
    assert_eq!(resolved.len(), full.len());
}

#[tokio::test]
async fn failed_delegated_evidence_uses_namespace_refs_without_rollout_paths() {
    for (index, status) in [ChildRuntimeStatus::TimedOut, ChildRuntimeStatus::Terminated]
        .into_iter()
        .enumerate()
    {
        let (state, shell) = create_namespace_agent_loop_state_and_shell();
        let request = DelegatedSkillInvocationRequest {
            skill_id: "repo-review".to_string(),
            target: "reviewer".to_string(),
            task: "Review local files".to_string(),
            workspace_root: None,
            cwd: None,
            timeout_secs: None,
        };
        let output_text = format!("partial child evidence {index}");
        let child_run_id = format!("failed-child-{index}");
        let result = ChildRuntimeResult {
            status,
            session_id: format!("failed-session-{index}"),
            child_run_id: Some(child_run_id.clone()),
            rollout_path: Some(PathBuf::from(format!("/tmp/private-child-{index}.jsonl"))),
            output_text: output_text.clone(),
            turn_summary: None,
            structured_output: None,
            warnings: Vec::new(),
            error_message: Some("delegated child did not complete".to_string()),
            pause: None,
            child_run: Some(test_child_run_record(&child_run_id, "parent-session")),
        };

        let output_ref = persist_delegated_child_evidence(&state, &request, &result)
            .await
            .expect("failed child output reference");
        assert!(output_ref.path.starts_with("/agent/1/actions/"));
        assert_eq!(
            output_ref
                .debug
                .as_ref()
                .and_then(|debug| debug.rollout_path.as_deref()),
            None
        );
        assert_eq!(
            String::from_utf8(shell.cat(&output_ref.path).await.unwrap()).unwrap(),
            output_text
        );
        assert!(
            !serde_json::to_string(&output_ref)
                .unwrap()
                .contains("/tmp/private-child-")
        );
    }
}

#[tokio::test]
async fn evidence_resolution_distinguishes_missing_and_retention_expired() {
    let (state, _shell) = create_namespace_agent_loop_state_and_shell();
    let preview = Some("bounded preview".to_string());
    let child_run = Some(json!({"child_run_id": "child-1"}));
    let missing_ref = crate::evidence::NamespaceEvidenceReference {
        path: "/agent/1/actions/missing/output".to_string(),
        offset: Some(0),
        length: None,
    };

    let missing = state
        .namespace_environment()
        .resolve_evidence_reference(&missing_ref, preview.clone(), child_run.clone())
        .await
        .unwrap_err();
    assert_eq!(
        missing.code,
        crate::evidence::EvidenceResolutionErrorCode::Missing
    );
    assert_eq!(missing.preview, preview);
    assert_eq!(missing.child_run, child_run);

    let expired_record = json!({
        "type": crate::evidence::RETENTION_EXPIRED_RECORD_TYPE,
        "reference": "/agent/1/actions/a0/output",
        "cause": "simulated_gc"
    });
    let action_id = state
        .namespace_environment()
        .write_action(
            NamespaceActionRecord::new("expired-test", "completed")
                .with_output(expired_record.to_string()),
        )
        .await
        .unwrap();
    let mut expired_ref = state
        .namespace_environment()
        .evidence_reference(format!("/agent/1/actions/{action_id}/output"))
        .await
        .unwrap();
    expired_ref.length = Some(10_000);
    let expired = state
        .namespace_environment()
        .resolve_evidence_reference(&expired_ref, None, None)
        .await
        .unwrap_err();
    assert_eq!(
        expired.code,
        crate::evidence::EvidenceResolutionErrorCode::RetentionExpired
    );
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_honors_interrupt() {
    let mut state = create_test_agent_loop_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state
        .turn_state
        .set_turn_activity(TurnActivityState::Running);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            let cancel_for_task = cancel_for_task.clone();
            Box::pin(async move {
                cancel_for_task.cancelled().await;
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Cancelled,
                    session_id: "child-session".to_string(),
                    child_run_id: None,
                    rollout_path: Some(PathBuf::from("/tmp/child-rollout.jsonl")),
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    );
    tokio::task::yield_now().await;
    cancel.cancel();

    let result = result.await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnCompleted { summary: Some(summary) } if summary == "Task cancelled by user"
    )));

    let prompt_view = state.session.tape.prompt_view();
    assert!(!prompt_view.messages.iter().any(|message| matches!(
        message,
        crate::tape::Message::Tool { responses }
            if responses.iter().any(|response| response.id == "call_1")
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invoke_delegated_skill_honors_interrupt_during_startup()
{
    let mut state = create_test_agent_loop_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");
    state
        .turn_state
        .set_turn_activity(TurnActivityState::Running);

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            let cancel_for_task = cancel_for_task.clone();
            Box::pin(async move {
                cancel_for_task.cancelled().await;
                Err(anyhow::anyhow!("Child-agent launch cancelled"))
            })
        },
    );
    tokio::task::yield_now().await;
    cancel.cancel();

    let result = result.await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::EndTurn));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::TurnCompleted { summary: Some(summary) } if summary == "Task cancelled by user"
    )));

    let prompt_view = state.session.tape.prompt_view();
    assert!(!prompt_view.messages.iter().any(|message| matches!(
        message,
        crate::tape::Message::Tool { responses }
            if responses.iter().any(|response| response.id == "call_1")
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_invalid_delegated_skill_request() {
    let mut state = create_test_agent_loop_state();

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer"
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
    assert!(events.iter().any(|event| {
        matches!(
            event,
            Event::Error {
                message,
                recoverable: true
            } if message.contains("delegated skill invocation")
        )
    }));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_rejects_target_mismatch() {
    let mut state = create_test_agent_loop_state();
    activate_test_delegated_skill(&mut state, "repo-review", "reviewer");

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "grader",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let cancel = CancellationToken::new();
    let result = handle_invoke_delegated_skill(
        &mut state,
        &tool_call,
        &tool_call.arguments,
        &cancel,
        &mut emit,
        |_state, _spec, _cancel| {
            panic!("target mismatch should not attempt child launch");
            #[allow(unreachable_code)]
            Box::pin(async move {
                Ok(ChildRuntimeResult {
                    status: ChildRuntimeStatus::Completed,
                    session_id: String::new(),
                    child_run_id: None,
                    rollout_path: None,
                    output_text: String::new(),
                    turn_summary: None,
                    structured_output: None,
                    warnings: Vec::new(),
                    error_message: None,
                    pause: None,
                    child_run: None,
                })
            })
        },
    )
    .await;
    assert!(result.is_ok());
    assert!(matches!(
        result.unwrap(),
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));

    let prompt_view = state.session.tape.prompt_view();
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
        .expect("expected delegated skill tool result");
    assert!(tool_result.contains("\"status\":\"failed\""));
    assert!(tool_result.contains("delegate_target_mismatch"));
    assert!(tool_result.contains("\"resolved_target\":\"reviewer\""));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ToolCallCompleted { id, .. } if id == "call_1"
    )));
}

#[tokio::test]
async fn test_try_handle_virtual_tool_call_deferred_to_dynamic_delegated_tool() {
    let mut state = create_test_agent_loop_state();
    state.session.dynamic_tools.insert(
        "invoke_delegated_skill".to_string(),
        alan_agent_protocol::DynamicToolSpec {
            name: "invoke_delegated_skill".to_string(),
            description: "Delegated execution bridge".to_string(),
            parameters: json!({"type": "object", "properties": {}}),
            capability: Some(alan_agent_protocol::ToolCapability::Read),
        },
    );

    let tool_call = NormalizedToolCall {
        id: "call_1".to_string(),
        name: "invoke_delegated_skill".to_string(),
        arguments: json!({
            "skill_id": "repo-review",
            "target": "reviewer",
            "task": "Review the current diff and summarize risks."
        }),
    };

    let mut events = vec![];
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let result = try_handle_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit).await;
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), VirtualToolOutcome::NotVirtual));
    assert!(events.is_empty());
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
