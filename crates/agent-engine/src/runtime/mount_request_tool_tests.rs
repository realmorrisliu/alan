use super::*;

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
        .machine
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
        .machine
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
    assert!(state.machine.pending_confirmation().is_none());
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
        crate::policy::PolicyEngine::load_or_default(Some(&temp.path().join("policy.yaml")));

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
    assert!(state.machine.pending_confirmation().is_none());
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
        crate::policy::PolicyEngine::load_or_default(Some(&temp.path().join("policy.yaml")));

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
    assert!(state.machine.pending_confirmation().is_none());
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
    assert!(state.machine.pending_confirmation().is_none());
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
