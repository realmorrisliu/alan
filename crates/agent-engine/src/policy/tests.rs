use super::*;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn descriptor_policy_loads_without_a_host_path() {
    let tree = crate::ProcessFileTree::new(std::collections::BTreeMap::from([(
        "policy.yaml".to_string(),
        b"default_action: deny\nrules: []\n".to_vec(),
    )]))
    .unwrap();
    let engine = PolicyEngine::load_for_governance_from_file_tree(
        &tree,
        &alan_agent_protocol::GovernanceConfig::default(),
    );
    let decision = engine.evaluate(PolicyContext {
        tool_name: "read_file",
        arguments: &json!({"path": "README.md"}),
        capability: alan_agent_protocol::ToolCapability::Read,
        cwd: None,
    });
    assert_eq!(decision.action, PolicyAction::Deny);
    assert_eq!(decision.source, "descriptor_policy_file");
}

#[test]
fn auto_approve_boundary_matches_human_in_the_end_posture() {
    let engine = PolicyEngine::autonomous();
    let decide = |tool: &str, cap, args: serde_json::Value| {
        engine
            .evaluate(PolicyContext {
                tool_name: tool,
                arguments: &args,
                capability: cap,
                cwd: None,
            })
            .action
    };
    use alan_agent_protocol::ToolCapability as Cap;
    // Routine work auto-approves.
    assert_eq!(
        decide("read_file", Cap::Read, serde_json::json!({"path": "a.rs"})),
        PolicyAction::Allow
    );
    assert_eq!(
        decide("edit_file", Cap::Write, serde_json::json!({"path": "a.rs"})),
        PolicyAction::Allow
    );
    // Judgment-needing work escalates.
    assert_eq!(
        decide("fetch", Cap::Network, serde_json::json!({})),
        PolicyAction::Escalate
    );
    assert_eq!(
        decide(
            "bash",
            Cap::Unknown,
            serde_json::json!({"command": "git push origin main"})
        ),
        PolicyAction::Escalate
    );
    assert_eq!(
        decide(
            "bash",
            Cap::Unknown,
            serde_json::json!({"command": "git reset --hard HEAD~1"})
        ),
        PolicyAction::Escalate
    );
    assert_eq!(
        decide("custom_tool", Cap::Unknown, serde_json::json!({})),
        PolicyAction::Escalate
    );
    let mount_decision = engine.evaluate(PolicyContext {
        tool_name: "request_mount",
        arguments: &serde_json::json!({
            "namespace_path": "/mnt/project",
            "host_path": "/Users/morris/Developer/alan",
            "access": "read_only",
            "reason": "Need project files"
        }),
        capability: Cap::Write,
        cwd: None,
    });
    assert_eq!(mount_decision.action, PolicyAction::Escalate);
    assert_eq!(mount_decision.rule_id.as_deref(), Some("review-host-mount"));
    assert_eq!(
        mount_decision.reason.as_deref(),
        Some("host mount grants require approval")
    );
    // Catastrophic commands are denied outright.
    assert_eq!(
        decide(
            "bash",
            Cap::Unknown,
            serde_json::json!({"command": "rm -rf /"})
        ),
        PolicyAction::Deny
    );
}

#[test]
fn auto_approve_denies_dangerous_bash() {
    let engine = PolicyEngine::autonomous();
    let decision = engine.evaluate(PolicyContext {
        tool_name: "bash",
        arguments: &json!({"command":"rm -rf / --no-preserve-root"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });
    assert_eq!(decision.action, PolicyAction::Deny);
    assert_eq!(decision.rule_id.as_deref(), Some("deny-rm-root"));
}

#[test]
fn load_definition_policy_file_overrides_builtin() {
    let tmp = TempDir::new().unwrap();
    let policy_dir = tmp.path().join("definition");
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(
        policy_dir.join("policy.yaml"),
        r#"
rules:
  - id: deny-read-file
    tool: read_file
    action: deny
    reason: no reads
default_action: allow
"#,
    )
    .unwrap();

    let engine = PolicyEngine::load_or_default(Some(&policy_dir.join("policy.yaml")));
    let decision = engine.evaluate(PolicyContext {
        tool_name: "read_file",
        arguments: &json!({}),
        capability: alan_agent_protocol::ToolCapability::Read,
        cwd: None,
    });
    assert_eq!(decision.action, PolicyAction::Deny);
    assert_eq!(decision.rule_id.as_deref(), Some("deny-read-file"));
    assert_eq!(decision.source, "definition_policy_file");
}

#[test]
fn malformed_policy_file_fails_closed_not_permissive() {
    let tmp = TempDir::new().unwrap();
    let policy_dir = tmp.path().join("definition");
    std::fs::create_dir_all(&policy_dir).unwrap();
    // A present-but-broken policy file (YAML/schema error).
    std::fs::write(policy_dir.join("policy.yaml"), "rules: [ this is not valid").unwrap();

    let engine = PolicyEngine::load_or_default(Some(&policy_dir.join("policy.yaml")));
    // Must NOT silently become the permissive builtin; deny by default so the
    // misconfiguration surfaces instead of allowing routine writes.
    let decision = engine.evaluate(PolicyContext {
        tool_name: "write_file",
        arguments: &json!({"path": "a.txt"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });
    assert_eq!(decision.action, PolicyAction::Deny);
    assert_eq!(decision.source, "policy_load_failed");
}

#[test]
fn policy_rule_match_path_prefix_matches_write_path() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-workflows".to_string()),
            tool: Some("write_file".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some(".github/workflows/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("workflow edits require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "write_file",
        arguments: &json!({"path":"./.github/workflows/release.yml","content":"name: release"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-workflows"));
}

#[test]
fn policy_rule_match_path_prefix_matches_paths_array() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-deploy".to_string()),
            tool: Some("*".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some("deploy/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("deploy config updates require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "edit_file",
        arguments: &json!({"paths":["src/lib.rs","deploy/prod.yaml"]}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-deploy"));
}

#[test]
fn policy_rule_match_path_prefix_matches_absolute_write_path() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-workflows".to_string()),
            tool: Some("write_file".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some(".github/workflows/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("workflow edits require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "write_file",
        arguments: &json!({
            "path":"/mnt/source/.github/workflows/release.yml",
            "content":"name: release"
        }),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-workflows"));
}

#[test]
fn policy_rule_match_path_prefix_matches_request_mount_host_path() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("deny-private-mount".to_string()),
            tool: Some("request_mount".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some("/Users/me/private".to_string()),
            action: PolicyAction::Deny,
            reason: Some("private host mounts are not allowed".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "request_mount",
        arguments: &json!({
            "namespace_path": "/mnt/private",
            "host_path": "/Users/me/private/project",
            "access": "read_only",
            "reason": "Need files"
        }),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Deny);
    assert_eq!(decision.rule_id.as_deref(), Some("deny-private-mount"));
}

#[test]
fn policy_rule_match_path_prefix_matches_request_mount_namespace_path() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-private-namespace".to_string()),
            tool: Some("request_mount".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some("/mnt/private".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("private namespace mounts need review".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "request_mount",
        arguments: &json!({
            "namespace_path": "/mnt/private/project",
            "host_path": "/Users/me/project",
            "access": "read_only",
            "reason": "Need files"
        }),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(
        decision.rule_id.as_deref(),
        Some("review-private-namespace")
    );
}

#[test]
fn policy_rule_match_path_prefix_matches_parent_traversal_path() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-deploy".to_string()),
            tool: Some("*".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some("deploy/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("deploy config updates require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "edit_file",
        arguments: &json!({"path":"tmp/../deploy/prod.yaml"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-deploy"));
}

#[test]
fn policy_rule_match_path_prefix_matches_parent_traversal_against_current_cwd() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-deploy".to_string()),
            tool: Some("*".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some("deploy/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("deploy config updates require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "edit_file",
        arguments: &json!({"path":"../deploy/prod.yaml"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: Some(Path::new("/mnt/source/src")),
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-deploy"));
}

#[test]
fn policy_rule_match_path_prefix_matches_case_variants() {
    let engine = PolicyEngine {
        rules: vec![PolicyRule {
            id: Some("review-workflows".to_string()),
            tool: Some("write_file".to_string()),
            capability: Some("write".to_string()),
            match_command: None,
            match_path_prefix: Some(".github/workflows/".to_string()),
            action: PolicyAction::Escalate,
            reason: Some("workflow edits require escalation".to_string()),
        }],
        default_action: PolicyAction::Allow,
        source: "test",
    };

    let decision = engine.evaluate(PolicyContext {
        tool_name: "write_file",
        arguments: &json!({"path":".GitHub/Workflows/release.yml","content":"name: release"}),
        capability: alan_agent_protocol::ToolCapability::Write,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-workflows"));
}

#[test]
fn load_definition_policy_file_supports_match_path_prefix() {
    let tmp = TempDir::new().unwrap();
    let policy_dir = tmp.path().join("definition");
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(
        policy_dir.join("policy.yaml"),
        r#"
rules:
  - id: review-credentials
    tool: read_file
    capability: read
    match_path_prefix: ".env"
    action: escalate
    reason: credential reads require escalation
default_action: allow
"#,
    )
    .unwrap();

    let engine = PolicyEngine::load_or_default(Some(&policy_dir.join("policy.yaml")));
    let decision = engine.evaluate(PolicyContext {
        tool_name: "read_file",
        arguments: &json!({"path":".env.production"}),
        capability: alan_agent_protocol::ToolCapability::Read,
        cwd: None,
    });

    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-credentials"));
    assert_eq!(decision.source, "definition_policy_file");
}

#[test]
fn autonomous_escalates_unknown_capability() {
    let engine = PolicyEngine::autonomous();
    let decision = engine.evaluate(PolicyContext {
        tool_name: "bash",
        arguments: &json!({"command":"python3 script.py"}),
        capability: alan_agent_protocol::ToolCapability::Unknown,
        cwd: None,
    });
    assert_eq!(decision.action, PolicyAction::Escalate);
    assert_eq!(decision.rule_id.as_deref(), Some("review-unknown"));
}

#[test]
fn resolve_definition_policy_path_stays_inside_definition() {
    let tmp = TempDir::new().unwrap();
    let definition = tmp.path().join("definition");
    let resolved =
        resolve_definition_policy_path(Some(&definition), Path::new("policy.yaml")).unwrap();
    assert_eq!(resolved, definition.join("policy.yaml"));
}

#[test]
fn resolve_definition_policy_path_rejects_parent_escape() {
    let tmp = TempDir::new().unwrap();
    let definition = tmp.path().join("definition");
    let error =
        resolve_definition_policy_path(Some(&definition), Path::new("../policy.yaml")).unwrap_err();
    assert!(error.to_string().contains("stay relative"));
}
