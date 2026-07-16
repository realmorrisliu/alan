use super::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_unknown_capability_escalates() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig {
            profile: alan_agent_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        },
        "dynamic_tool",
        &json!({"id":"123"}),
        alan_agent_protocol::ToolCapability::Unknown,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { details, .. } => {
            assert_eq!(details["capability"], "unknown");
            assert_eq!(details["policy"]["action"], "escalate");
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}

#[test]
fn escalated_bash_degrades_to_human_when_not_fully_confined() {
    use crate::policy::PolicyAction;
    // The 4th arg is `fully_confined` (network AND protected-subpath writes).
    // An *escalated* builtin bash, not fully confined → human.
    assert!(should_degrade_bash(
        PolicyAction::Escalate,
        "bash",
        "builtin_autonomous",
        false
    ));
    // Fully confined (Seatbelt) → the reviewer path is fine.
    assert!(!should_degrade_bash(
        PolicyAction::Escalate,
        "bash",
        "builtin_autonomous",
        true
    ));
    // Auto-allowed bash (`touch`, `echo`) is recognized + parser-confined, so
    // it is NOT downgraded even when not fully confined.
    assert!(!should_degrade_bash(
        PolicyAction::Allow,
        "bash",
        "builtin_autonomous",
        false
    ));
    // Explicit operator policies are respected (not downgraded).
    assert!(!should_degrade_bash(
        PolicyAction::Escalate,
        "bash",
        "definition_policy_file",
        false
    ));
    // Non-bash tools (contained by the path guard) are unaffected.
    assert!(!should_degrade_bash(
        PolicyAction::Escalate,
        "edit_file",
        "builtin_autonomous",
        false
    ));
    // Denials are never downgraded to escalation.
    assert!(!should_degrade_bash(
        PolicyAction::Deny,
        "bash",
        "builtin_autonomous",
        false
    ));
}

#[test]
fn catastrophic_root_delete_variants_are_denied() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "rm -rf /",
        "rm -fr /",
        "rm -R -f /",
        "rm -rf /*",
        "rm -rf ~",
        "rm -rf $HOME",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Write,
            None,
            SandboxConfinement::os_enforced(),
        );
        assert!(
            matches!(result, ToolPolicyDecision::Forbidden { .. }),
            "catastrophic delete not denied: {cmd}"
        );
    }
    // A scoped recursive delete is escalated for review, not denied outright.
    assert!(is_recursive_rm("rm -rf build") && !is_catastrophic_recursive_delete("rm -rf build"));
}

#[test]
fn destructive_find_actions_are_reviewed() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "find . -delete",
        "find . -name '*.tmp' -delete",
        "find . -exec rm {} +",
        "find /work -execdir sh -c 'x' \\;",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Write,
            None,
            SandboxConfinement::os_enforced(),
        );
        assert!(
            matches!(result, ToolPolicyDecision::Escalate { .. }),
            "destructive find not escalated: {cmd}"
        );
    }
    // A read-only find is not gated by this rule.
    assert!(!is_destructive_find("find . -name '*.rs'"));
}

#[test]
fn world_writable_chmod_variants_route_to_human() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "chmod 777 x",
        "chmod 0777 x",
        "chmod -R 777 dir",
        "chmod a+rwx x",
        "chmod o+w x",
        "chmod a=rw x",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Write,
            None,
            SandboxConfinement::os_enforced(),
        );
        match result {
            ToolPolicyDecision::Escalate { route, .. } => {
                assert_eq!(route, EscalationRoute::AlwaysHuman, "not human: {cmd}")
            }
            other => panic!("expected human escalation for {cmd}, got {:?}", other),
        }
    }
    // Owner-only grants, read-only modes, and revocations are not world-write.
    assert!(!is_world_writable_chmod("chmod u+w file"));
    assert!(!is_world_writable_chmod("chmod 644 file"));
    assert!(!is_world_writable_chmod("chmod 755 file"));
    assert!(!is_world_writable_chmod("chmod o-w file"));
}

#[test]
fn sudo_token_variants_route_to_human() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "sudo ls",
        "sudo\tls",
        "/usr/bin/sudo ls",
        "doas rm x",
        "pkexec sh",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Unknown,
            None,
            // Network confined, so degradation doesn't fire — the privilege
            // promotion alone must force always-human routing.
            SandboxConfinement::os_enforced(),
        );
        match result {
            ToolPolicyDecision::Escalate { route, .. } => {
                assert_eq!(route, EscalationRoute::AlwaysHuman, "not human: {cmd}")
            }
            other => panic!("expected human escalation for {cmd}, got {:?}", other),
        }
    }
    assert!(is_privilege_escalation("sudo\tls"));
    assert!(is_privilege_escalation("/usr/bin/sudo ls"));
    assert!(!is_privilege_escalation("echo hello"));
    assert!(!is_privilege_escalation("subprocess.run(cmd)"));
}

#[test]
fn code_runner_routes_to_human_under_landlock() {
    // Landlock confines Host Mounts + network but cannot carve out protected
    // subpaths, so a code runner whose test/build code could write `.git`
    // (`cargo test`, `pytest`) must go to a human, not auto-run or be
    // reviewer-routed — the sandbox does not fully confine bash here.
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in ["cargo test", "pytest -q", "make"] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            // Code runners classify as Unknown (→ review-unknown → Escalate).
            alan_agent_protocol::ToolCapability::Unknown,
            None,
            // Landlock: network confined, protected subpaths NOT confined.
            SandboxConfinement {
                permits_autonomous_bash: false,
                network: true,
            },
        );
        match result {
            ToolPolicyDecision::Escalate { route, .. } => {
                assert_eq!(route, EscalationRoute::AlwaysHuman, "not human: {cmd}")
            }
            other => panic!("expected human escalation for {cmd}, got {:?}", other),
        }
    }
    // A benign auto-allowed write is NOT downgraded under Landlock (recognized
    // + parser-confined), so it stays auto-approved.
    let touch = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({ "command": "touch hello.txt" }),
        alan_agent_protocol::ToolCapability::Write,
        None,
        SandboxConfinement {
            permits_autonomous_bash: false,
            network: true,
        },
    );
    assert!(
        matches!(touch, ToolPolicyDecision::Allow { .. }),
        "benign write should stay auto-approved under Landlock, got {touch:?}"
    );
}

#[test]
fn opaque_bash_routes_to_human_without_network_confinement() {
    // FS confined but network not (e.g. Landlock without network rules): an
    // opaque command that could open a socket must go to a human, not the
    // reviewer, since the sandbox can't contain its network after an allow.
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({"command":"python script.py"}),
        alan_agent_protocol::ToolCapability::Unknown,
        None,
        SandboxConfinement {
            permits_autonomous_bash: true,
            network: false,
        },
    );
    match result {
        ToolPolicyDecision::Escalate { route, .. } => {
            assert_eq!(route, EscalationRoute::AlwaysHuman)
        }
        other => panic!("expected human escalation, got {:?}", other),
    }
}

#[test]
fn test_force_push_routes_to_always_human() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({"command":"git push --force origin main"}),
        alan_agent_protocol::ToolCapability::Unknown,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { route, .. } => {
            assert_eq!(route, EscalationRoute::AlwaysHuman)
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}

#[test]
fn test_normal_git_push_routes_to_reviewer() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({"command":"git push origin main"}),
        alan_agent_protocol::ToolCapability::Unknown,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { route, .. } => {
            assert_eq!(route, EscalationRoute::Reviewer)
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}

#[test]
fn test_force_push_red_line_precedes_normal_push() {
    use alan_agent_protocol::ToolCapability::{Read, Unknown};
    // human- rules always route to a human regardless of capability.
    assert_eq!(
        escalation_route(Some("human-git-force-push"), Read, ""),
        EscalationRoute::AlwaysHuman
    );
    assert_eq!(
        escalation_route(Some("review-git-push"), Unknown, "git push origin main"),
        EscalationRoute::Reviewer
    );
    // Force-push in any token ordering routes to a human, even when the
    // matched rule is the plain `review-git-push` rule.
    for cmd in [
        "git push --force",
        "git push origin main --force",
        "git -C repo push --force",
        "git push -f origin main",
        "git push --force-with-lease=origin/main",
        // Path-qualified git matches by basename.
        "/usr/bin/git -C repo push --force origin main",
        // Mirror/delete/prune also rewrite or remove remote refs.
        "git push --mirror origin",
        "git push origin --delete feature",
        "git push -d origin feature",
        "git push --prune origin",
    ] {
        assert_eq!(
            escalation_route(Some("review-git-push"), Unknown, cmd),
            EscalationRoute::AlwaysHuman,
            "remote-rewrite push not routed to human: {cmd}"
        );
    }
    // A leading-`+` refspec forces a non-fast-forward update; a leading-`:`
    // refspec deletes the remote ref.
    assert!(is_force_push("git push origin +main:main"));
    assert!(is_force_push("git push origin +refs/heads/main"));
    assert!(is_force_push("git push origin :feature"));
    // Quoted flag forms are normalized and still detected.
    assert!(is_force_push("git push origin main '--force'"));
    assert!(is_force_push("git push \"-f\" origin main"));
    assert!(is_force_push("git push '--mirror' origin"));
    // A plain push (and a normal src:dst refspec) is not misclassified.
    assert!(!is_force_push("git push origin main"));
    assert!(!is_force_push("git push origin main:main"));
}

#[test]
fn test_reset_hard_variants_escalate_under_builtin() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "git reset --hard",
        "git -C repo reset --hard",
        "git reset HEAD --hard",
        "git reset '--hard'",
        "git -C repo reset \"--hard\"",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Write,
            None,
            SandboxConfinement::os_enforced(),
        );
        assert!(
            matches!(result, ToolPolicyDecision::Escalate { .. }),
            "reset --hard variant not escalated: {cmd}"
        );
    }
    assert!(is_reset_hard("git -C repo reset --hard"));
    assert!(!is_reset_hard("git reset HEAD~1")); // soft reset is not gated
}

#[test]
fn recursive_rm_permutations_escalate_under_builtin() {
    let policy = crate::policy::PolicyEngine::autonomous();
    for cmd in [
        "rm -rf build",
        "rm -fr build",
        "rm -R -f target",
        "rm --recursive node_modules",
        "rm -vrf dist",
        "/bin/rm -fr build",
    ] {
        let result = evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": cmd }),
            alan_agent_protocol::ToolCapability::Write,
            None,
            SandboxConfinement::os_enforced(),
        );
        assert!(
            matches!(result, ToolPolicyDecision::Escalate { .. }),
            "recursive rm not escalated: {cmd}"
        );
    }
    // Non-recursive rm and unrelated commands are not gated by this rule.
    assert!(!is_recursive_rm("rm file.txt"));
    assert!(!is_recursive_rm("rm -f file.txt"));
    assert!(!is_recursive_rm("rmdir build"));
    assert!(!is_recursive_rm("cargo run -- --recursive"));
}

#[test]
fn test_force_push_any_ordering_routes_to_human_end_to_end() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({"command":"git push origin main --force"}),
        alan_agent_protocol::ToolCapability::Unknown,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { route, .. } => {
            assert_eq!(route, EscalationRoute::AlwaysHuman)
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}

#[test]
fn test_network_route_follows_platform_containment() {
    // Network is reviewer-judged only when the active sandbox confines it.
    let expected = if crate::tools::confines_network() {
        EscalationRoute::Reviewer
    } else {
        EscalationRoute::AlwaysHuman
    };
    assert_eq!(
        escalation_route(
            Some("review-network"),
            alan_agent_protocol::ToolCapability::Network,
            "curl https://example.com"
        ),
        expected
    );
}

#[test]
fn test_network_escalates_under_autonomous() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig {
            profile: alan_agent_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        },
        "bash",
        &json!({"query":"rust"}),
        alan_agent_protocol::ToolCapability::Network,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { audit, .. } => {
            assert_eq!(audit.action, "escalate");
            assert_eq!(audit.capability, "network");
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}

#[test]
fn test_tool_policy_audit_reports_active_path_mode() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig::default(),
        "bash",
        &json!({"command":"ls"}),
        alan_agent_protocol::ToolCapability::Read,
        None,
        SandboxConfinement::os_enforced(),
    );

    match result {
        ToolPolicyDecision::Allow { audit } | ToolPolicyDecision::Escalate { audit, .. } => {
            assert_eq!(
                audit.path_mode.as_deref(),
                Some(crate::tools::active_backend_path_mode())
            );
        }
        other => panic!("expected policy decision with audit, got {:?}", other),
    }
}

#[test]
fn bash_shape_preflight_only_denies_without_os_sandbox() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let eval = |confinement: SandboxConfinement| {
        evaluate_tool_policy(
            &policy,
            &alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            "bash",
            &json!({"command":"bash -lc 'rg TODO src'"}),
            alan_agent_protocol::ToolCapability::Unknown,
            None,
            confinement,
        )
    };
    // Path-guard fallback: the syntactic preflight hard-denies the wrapper.
    match eval(SandboxConfinement::none()) {
        ToolPolicyDecision::Forbidden { reason, audit } => {
            assert!(
                reason.contains("rejects nested command evaluators")
                    || reason.contains("rejects shell wrappers")
            );
            assert_eq!(audit.policy_source, "sandbox_preflight");
            assert_eq!(audit.action, "deny");
        }
        other => panic!(
            "expected preflight denial without OS sandbox, got {:?}",
            other
        ),
    }
    // With a kernel-enforced OS sandbox, confinement is independent of
    // command syntax: the wrapper is not shape-denied (it routes through
    // policy instead — here escalated, not Forbidden by preflight).
    assert!(
        !matches!(
            eval(SandboxConfinement::os_enforced()),
            ToolPolicyDecision::Forbidden { audit, .. } if audit.policy_source == "sandbox_preflight"
        ),
        "OS-sandboxed bash must not be shape-denied by the preflight"
    );
}

#[test]
fn test_in_mount_write_auto_approves() {
    let policy = crate::policy::PolicyEngine::autonomous();
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig {
            profile: alan_agent_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        },
        "write_file",
        &json!({"path":"a.txt","content":"x"}),
        alan_agent_protocol::ToolCapability::Write,
        None,
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Allow { audit } => {
            assert_eq!(audit.action, "allow");
            assert_eq!(audit.capability, "write");
        }
        other => panic!("expected allow, got {:?}", other),
    }
}

#[test]
fn test_tool_policy_uses_current_cwd_for_relative_path_prefix_matching() {
    let tmp = TempDir::new().unwrap();
    let policy_dir = tmp.path().join("definition");
    std::fs::create_dir_all(&policy_dir).unwrap();
    std::fs::write(
        policy_dir.join("policy.yaml"),
        r#"
rules:
  - id: review-deploy
    tool: "*"
    capability: write
    match_path_prefix: "deploy/"
    action: escalate
    reason: deploy config updates require escalation
default_action: allow
"#,
    )
    .unwrap();
    let policy =
        crate::policy::PolicyEngine::load_or_default(Some(&policy_dir.join("policy.yaml")));
    let result = evaluate_tool_policy(
        &policy,
        &alan_agent_protocol::GovernanceConfig {
            profile: alan_agent_protocol::GovernanceProfile::Autonomous,
            policy_path: None,
        },
        "write_file",
        &json!({"path":"../deploy/prod.yaml","content":"version = 2"}),
        alan_agent_protocol::ToolCapability::Write,
        Some(Path::new("/mnt/source/src")),
        SandboxConfinement::os_enforced(),
    );
    match result {
        ToolPolicyDecision::Escalate { audit, .. } => {
            assert_eq!(audit.action, "escalate");
            assert_eq!(audit.rule_id.as_deref(), Some("review-deploy"));
        }
        other => panic!("expected escalation, got {:?}", other),
    }
}
