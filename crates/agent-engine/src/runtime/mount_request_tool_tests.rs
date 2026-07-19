use super::*;

#[test]
fn request_mount_schema_accepts_only_logical_intent() {
    let definition = request_mount_tool_definition();
    assert_eq!(definition.name, "request_mount");
    assert_eq!(definition.parameters["type"], "object");
    assert_eq!(definition.parameters["additionalProperties"], false);
    assert!(
        definition.parameters["properties"]
            .get("host_path")
            .is_none()
    );
    assert_eq!(
        definition.parameters["properties"]["label"]["type"],
        "string"
    );
    assert_eq!(
        definition.parameters["required"],
        json!(["namespace_path", "access", "reason"])
    );
}

#[test]
fn parse_mount_request_normalizes_logical_document() {
    let request = parse_mount_request(&json!({
        "namespace_path": "/mnt/project",
        "access": "read_only",
        "reason": "  Read project docs  ",
        "label": "  Project  "
    }))
    .expect("valid logical mount request");

    assert_eq!(request.namespace_path, "/mnt/project");
    assert_eq!(request.access, MountRequestAccess::ReadOnly);
    assert_eq!(request.reason, "Read project docs");
    assert_eq!(request.label.as_deref(), Some("Project"));
    assert_eq!(
        request.payload(),
        json!({
            "namespace_path": "/mnt/project",
            "access": "read_only",
            "reason": "Read project docs",
            "label": "Project"
        })
    );
}

#[test]
fn parse_mount_request_rejects_invalid_or_host_owned_fields() {
    let cases = [
        (
            json!({
                "namespace_path": "/proc/project",
                "access": "read_only",
                "reason": "Read docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/../project",
                "access": "read_only",
                "reason": "Read docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/llm",
                "access": "read_only",
                "reason": "Read docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/host-mount",
                "access": "read_only",
                "reason": "Read docs"
            }),
            "namespace_path",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "access": "admin",
                "reason": "Read docs"
            }),
            "access",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "access": "read_only",
                "reason": "   "
            }),
            "reason",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "access": "read_only",
                "reason": "Read docs",
                "label": "   "
            }),
            "label",
        ),
        (
            json!({
                "namespace_path": "/mnt/project",
                "host_path": "/Users/example/project",
                "access": "read_only",
                "reason": "Read docs"
            }),
            "unsupported fields",
        ),
    ];

    for (arguments, expected) in cases {
        let error = parse_mount_request(&arguments).expect_err("request must be rejected");
        assert!(error.contains(expected), "expected {expected} in {error}");
    }
}

#[test]
fn durable_mount_request_arguments_keep_only_valid_logical_values() {
    let raw_host_path = "/Users/example/private-project";
    let rejected = durable_mount_request_arguments(&json!({
        "namespace_path": "/mnt/project",
        "host_path": raw_host_path,
        "access": "read_only",
        "reason": "Read docs"
    }));
    assert_eq!(rejected, json!({"invalid_request": true}));
    assert!(!rejected.to_string().contains(raw_host_path));

    let accepted = durable_mount_request_arguments(&json!({
        "namespace_path": "/mnt/project",
        "access": "read_only",
        "reason": "  Read docs  "
    }));
    assert_eq!(
        accepted,
        json!({
            "namespace_path": "/mnt/project",
            "access": "read_only",
            "reason": "Read docs",
            "label": null
        })
    );
}

#[tokio::test]
async fn policy_allow_commits_one_service_request_and_yields_authorization_wait() {
    let mut state = create_test_transition_state();
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::allow_all();
    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/project",
            "access": "read_write",
            "reason": "Need to edit project files",
            "label": "Project"
        }),
    };
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(outcome, VirtualToolOutcome::PauseTurn));

    let pending = state
        .machine
        .pending_host_mount("request-1")
        .expect("Machine owns the opaque service wait");
    assert_eq!(pending.tool_call_id, "call_mount");
    assert_eq!(pending.namespace_path, "/mnt/project");
    assert_eq!(pending.request_events_offset, 0);
    assert!(state.machine.pending_confirmation().is_none());

    let shell = Shell::new(state.environment.root_transport());
    let request: serde_json::Value = serde_json::from_slice(
        &shell
            .cat("/mnt/host-mount/requests/request-1/request")
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(request, tool_call.arguments);
    assert_eq!(
        shell
            .cat("/mnt/host-mount/requests/request-1/status")
            .await
            .unwrap(),
        b"pending\n"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Yield { request_id, kind: alan_agent_protocol::YieldKind::Custom(kind), payload }
            if request_id == "request-1"
                && kind == "authorization_wait"
                && payload["details"]["request_reference"] == "request-1"
                && !payload.to_string().contains("host_path")
    )));
}

#[tokio::test]
async fn raw_host_path_is_rejected_without_service_request_or_yield() {
    let mut state = create_test_transition_state();
    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/project",
            "host_path": "/Users/example/project",
            "access": "read_only",
            "reason": "Need files"
        }),
    };
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        VirtualToolOutcome::Continue {
            refresh_context: true
        }
    ));
    assert!(!state.machine.has_pending_interaction());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Yield { .. }))
    );
    let shell = Shell::new(state.environment.root_transport());
    assert_eq!(
        shell.ls("/mnt/host-mount/requests").await.unwrap(),
        vec!["clone".to_string()]
    );
    let result = tool_result_text_for_call(&state, "call_mount");
    assert!(!result.contains("/Users/example/project"));
    assert!(!result.contains("host_path"));
}

#[tokio::test]
async fn policy_deny_does_not_create_service_request() {
    let mut state = create_test_transition_state();
    state.runtime_config.policy_engine = crate::policy::PolicyEngine::deny_all();
    let tool_call = NormalizedToolCall {
        id: "call_mount".to_string(),
        name: "request_mount".to_string(),
        arguments: json!({
            "namespace_path": "/mnt/project",
            "access": "read_only",
            "reason": "Need files"
        }),
    };
    let mut events = Vec::new();
    let mut emit = |event: Event| {
        events.push(event);
        async {}
    };

    let outcome = dispatch_virtual_tool_call_for_test(&mut state, &tool_call, &mut emit)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        VirtualToolOutcome::Continue {
            refresh_context: false
        }
    ));
    assert!(!state.machine.has_pending_interaction());
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Yield { .. }))
    );
    let shell = Shell::new(state.environment.root_transport());
    assert_eq!(
        shell.ls("/mnt/host-mount/requests").await.unwrap(),
        vec!["clone".to_string()]
    );
}
