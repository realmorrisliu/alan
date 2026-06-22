use serde_json::json;

#[derive(Debug, Clone)]
pub(super) enum ToolPolicyDecision {
    Allow {
        audit: alan_protocol::ToolDecisionAudit,
    },
    Escalate {
        summary: String,
        details: serde_json::Value,
        audit: alan_protocol::ToolDecisionAudit,
        route: EscalationRoute,
    },
    Forbidden {
        reason: String,
        audit: alan_protocol::ToolDecisionAudit,
    },
}

/// Where an escalation goes: to the guardian reviewer, or straight to a human
/// (the always-human red line, or effects the sandbox cannot contain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EscalationRoute {
    Reviewer,
    AlwaysHuman,
}

/// Classify an escalation's route. Always-human applies to the red-line rules
/// (rule ids prefixed `human-`), to force-push in any token ordering, and to
/// network when the active sandbox cannot confine it on this platform.
pub(super) fn escalation_route(
    rule_id: Option<&str>,
    capability: alan_protocol::ToolCapability,
    command: &str,
) -> EscalationRoute {
    if rule_id.is_some_and(|id| id.starts_with("human-")) {
        return EscalationRoute::AlwaysHuman;
    }
    // Force-push rewrites remote history regardless of flag ordering; the
    // substring rule only catches `git push --force`, so token-check here so the
    // reviewer can never approve a force-push.
    if is_force_push(command) {
        return EscalationRoute::AlwaysHuman;
    }
    if matches!(capability, alan_protocol::ToolCapability::Network)
        && !crate::tools::confines_network()
    {
        return EscalationRoute::AlwaysHuman;
    }
    EscalationRoute::Reviewer
}

/// Detect a `git push` with a force flag in any token ordering (errs toward
/// always-human). Coarse whitespace tokenization is sufficient — obfuscated
/// commands are caught by the sandbox/reviewer backstops.
/// Split a command into whitespace tokens with surrounding shell quotes stripped
/// (`'--hard'` → `--hard`), so quoted flag forms are detected like bare ones.
fn normalized_tokens(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(['\'', '"']).to_string())
        .collect()
}

fn is_force_push(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    let has_git = tokens.iter().any(|t| t == "git");
    let has_push = tokens.iter().any(|t| t == "push");
    let has_force = tokens.iter().any(|t| {
        t == "-f"
            || t == "--force"
            || t.starts_with("--force-with-lease")
            // A leading `+` on a push refspec (e.g. `+main:main`) forces a
            // non-fast-forward update — equivalent to --force for that ref.
            || (t.starts_with('+') && t.len() > 1)
    });
    has_git && has_push && has_force
}

/// Detect a `git reset --hard` in any token ordering / quoting (errs toward
/// escalation).
fn is_reset_hard(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    tokens.iter().any(|t| t == "git")
        && tokens.iter().any(|t| t == "reset")
        && tokens.iter().any(|t| t == "--hard")
}

/// Detect a recursive `rm` in any flag ordering / bundling (errs toward
/// escalation). Catches `-r`/`-R`/`--recursive` and bundles like `-rf`/`-fr`.
fn is_recursive_rm(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    let has_rm = tokens.iter().any(|t| t == "rm" || t.ends_with("/rm"));
    let has_recursive = tokens.iter().any(|t| {
        t == "--recursive"
            // Short-flag bundle (e.g. `-rf`, `-fr`, `-Rf`) — any cluster of
            // single-dash flags containing r/R, but not a `--long` option.
            || (t.starts_with('-')
                && !t.starts_with("--")
                && t.chars().skip(1).any(|c| c == 'r' || c == 'R'))
    });
    has_rm && has_recursive
}

/// What the active sandbox backend confines, used to gate degradation/preflight.
/// Filesystem and network are tracked separately because Landlock can confine the
/// filesystem on a kernel that lacks network-rule support.
#[derive(Debug, Clone, Copy)]
pub(super) struct SandboxConfinement {
    /// The OS backend confines filesystem effects (skip the syntactic preflight).
    pub fs: bool,
    /// The OS backend confines network effects (else bash must go to a human).
    pub network: bool,
}

impl SandboxConfinement {
    /// Resolve from the active backend.
    pub fn detect() -> Self {
        Self {
            fs: crate::tools::os_backend_active(),
            network: crate::tools::confines_network(),
        }
    }

    #[cfg(test)]
    pub fn os_enforced() -> Self {
        Self {
            fs: true,
            network: true,
        }
    }

    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            fs: false,
            network: false,
        }
    }
}

pub(super) fn evaluate_tool_policy(
    policy_engine: &crate::policy::PolicyEngine,
    governance: &alan_protocol::GovernanceConfig,
    tool_name: &str,
    arguments: &serde_json::Value,
    capability: alan_protocol::ToolCapability,
    current_cwd: Option<&std::path::Path>,
    confinement: SandboxConfinement,
) -> ToolPolicyDecision {
    let sandbox_backend = crate::tools::active_backend_name();
    // The bash-shape preflight is the workspace-path-guard parser standing in for
    // confinement. With a kernel-enforced OS sandbox active, confinement is
    // independent of command syntax, so this syntactic deny must not block
    // commands the sandbox would safely contain (e.g. `python -c ...`,
    // `bash -lc ...`). Apply it only on the path-guard fallback.
    if !confinement.fs
        && let Some(reason) = bash_shape_preflight_reason(tool_name, arguments)
    {
        return ToolPolicyDecision::Forbidden {
            reason: reason.clone(),
            audit: alan_protocol::ToolDecisionAudit {
                policy_source: "sandbox_preflight".to_string(),
                rule_id: None,
                action: "deny".to_string(),
                reason: Some(reason),
                capability: capability_label(capability).to_string(),
                sandbox_backend: sandbox_backend.to_string(),
            },
        };
    }

    let policy_decision = policy_engine.evaluate(crate::policy::PolicyContext {
        tool_name,
        arguments,
        capability,
        cwd: current_cwd,
    });
    let capability_kind = capability_label(capability).to_string();
    let mut policy_source = policy_decision.source.to_string();
    let mut rule_id = policy_decision.rule_id.clone();
    let mut policy_reason = policy_decision.reason.clone();
    let mut action = policy_decision.action;

    // Token-aware irreversible-git gate under the builtin posture: variant
    // orderings (e.g. `git -C repo reset --hard`, `git reset HEAD --hard`) miss
    // the substring rules and would fall through to allow, so escalate them for
    // review. (Force-push is already escalated via the `git push` rule and routed
    // to a human in `escalation_route`.)
    if action == crate::policy::PolicyAction::Allow
        && tool_name == "bash"
        && policy_source == "builtin_autonomous"
        && is_reset_hard(
            arguments
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        )
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("review-git-reset-hard".to_string());
        policy_reason = Some("irreversible git reset requires review".to_string());
    }

    // Token-aware recursive-rm gate: the substring rule only catches `rm -rf`, so
    // flag permutations (`rm -fr build`, `rm -R -f target`, `rm --recursive ...`)
    // fall through to allow. A recursive delete is destructive even when the OS
    // sandbox contains it to the workspace, so escalate it for review.
    if action == crate::policy::PolicyAction::Allow
        && tool_name == "bash"
        && policy_source == "builtin_autonomous"
        && is_recursive_rm(
            arguments
                .get("command")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(""),
        )
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("review-recursive-rm".to_string());
        policy_reason = Some("recursive delete requires review".to_string());
    }

    // Safe degradation: under the builtin autonomous posture, bash can open
    // sockets the sandbox may not contain. Filesystem confinement alone is not
    // enough — without network confinement (no OS sandbox, or Landlock on a
    // kernel without network rules) an opaque bash (`python script.py`, `./deploy`)
    // could reach the network after a reviewer allow. So any non-denied builtin
    // bash must go to a human whenever the network is unconfined. (`human-` rule
    // ids route to a person, never the reviewer.) Operator policies are untouched.
    if should_degrade_bash(action, tool_name, &policy_source, confinement.network) {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("human-bash-unconfined".to_string());
        policy_reason = Some(
            "bash is not fully sandbox-confined (network); requires human approval".to_string(),
        );
        policy_source = "safe_degradation".to_string();
    }

    match action {
        crate::policy::PolicyAction::Allow => ToolPolicyDecision::Allow {
            audit: alan_protocol::ToolDecisionAudit {
                policy_source: policy_source.clone(),
                rule_id: rule_id.clone(),
                action: "allow".to_string(),
                reason: policy_reason.clone(),
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
            },
        },
        crate::policy::PolicyAction::Deny => ToolPolicyDecision::Forbidden {
            reason: policy_reason
                .clone()
                .unwrap_or_else(|| format!("Tool '{}' denied by policy", tool_name)),
            audit: alan_protocol::ToolDecisionAudit {
                policy_source: policy_source.clone(),
                rule_id: rule_id.clone(),
                action: "deny".to_string(),
                reason: policy_reason.clone(),
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
            },
        },
        crate::policy::PolicyAction::Escalate => ToolPolicyDecision::Escalate {
            route: escalation_route(
                rule_id.as_deref(),
                capability,
                arguments
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            ),
            summary: format!("Escalate tool call '{}'? ", tool_name)
                .trim()
                .to_string(),
            details: json!({
                "kind": "tool_escalation",
                "tool_name": tool_name,
                "arguments": arguments,
                "capability": capability_label(capability),
                // Derived from arguments alone (pre-execution) so the approval
                // surface can show the diff/command being approved.
                "presentation": super::tool_presentation::tool_presentation(
                    tool_name,
                    arguments,
                    &serde_json::Value::Null,
                )
                .and_then(|p| serde_json::to_value(p).ok()),
                "governance": governance,
                "policy": {
                    "source": policy_source,
                    "rule_id": rule_id,
                    "reason": policy_reason,
                    "action": "escalate"
                },
                "sandbox_backend": sandbox_backend
            }),
            audit: alan_protocol::ToolDecisionAudit {
                policy_source,
                rule_id,
                action: "escalate".to_string(),
                reason: policy_reason,
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
            },
        },
    }
}

/// Whether a builtin-autonomous bash decision must be downgraded to a human
/// escalation because the active sandbox cannot confine its network effects.
fn should_degrade_bash(
    action: crate::policy::PolicyAction,
    tool_name: &str,
    policy_source: &str,
    network_confined: bool,
) -> bool {
    // Without network confinement the reviewer is not a security boundary for
    // bash, so *any* non-denied builtin bash must go to a human — including
    // commands already escalated to the reviewer (e.g. `rm -rf build`,
    // `git reset --hard`) and opaque ones that could open sockets
    // (`python script.py`), which would otherwise reach the network after a
    // reviewer allow. Denials stay denied.
    action != crate::policy::PolicyAction::Deny
        && tool_name == "bash"
        && policy_source == "builtin_autonomous"
        && !network_confined
}

fn bash_shape_preflight_reason(tool_name: &str, arguments: &serde_json::Value) -> Option<String> {
    if tool_name != "bash" {
        return None;
    }

    let command = arguments
        .get("command")
        .and_then(serde_json::Value::as_str)?;
    crate::tools::Sandbox::bash_preflight_reason(command)
}

pub(super) fn capability_label(capability: alan_protocol::ToolCapability) -> &'static str {
    match capability {
        alan_protocol::ToolCapability::Read => "read",
        alan_protocol::ToolCapability::Write => "write",
        alan_protocol::ToolCapability::Network => "network",
        alan_protocol::ToolCapability::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_unknown_capability_escalates() {
        let policy = crate::policy::PolicyEngine::autonomous();
        let result = evaluate_tool_policy(
            &policy,
            &alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            "dynamic_tool",
            &json!({"id":"123"}),
            alan_protocol::ToolCapability::Unknown,
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
    fn bash_degrades_to_human_when_network_unconfined_under_builtin() {
        use crate::policy::PolicyAction;
        // The 4th arg is `network_confined`. Builtin bash + no network
        // confinement (no OS sandbox, or Landlock without network rules) → human.
        assert!(should_degrade_bash(
            PolicyAction::Allow,
            "bash",
            "builtin_autonomous",
            false
        ));
        // Network confined (e.g. Seatbelt deny-network, Landlock with net) →
        // bash may auto-run / be reviewer-judged (sandbox contains it).
        assert!(!should_degrade_bash(
            PolicyAction::Allow,
            "bash",
            "builtin_autonomous",
            true
        ));
        // Explicit operator policies are respected (not downgraded).
        assert!(!should_degrade_bash(
            PolicyAction::Allow,
            "bash",
            "workspace_policy_file",
            false
        ));
        // Non-bash tools (contained by the path guard) are unaffected.
        assert!(!should_degrade_bash(
            PolicyAction::Allow,
            "edit_file",
            "builtin_autonomous",
            false
        ));
        // Already-escalated bash (e.g. `rm -rf`, `git reset --hard`) and opaque
        // bash (`python script.py`) must also go to a human when network is
        // unconfined — the reviewer is not a boundary.
        assert!(should_degrade_bash(
            PolicyAction::Escalate,
            "bash",
            "builtin_autonomous",
            false
        ));
        // ...but with network confinement the reviewer path is fine.
        assert!(!should_degrade_bash(
            PolicyAction::Escalate,
            "bash",
            "builtin_autonomous",
            true
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
    fn opaque_bash_routes_to_human_without_network_confinement() {
        // FS confined but network not (e.g. Landlock without network rules): an
        // opaque command that could open a socket must go to a human, not the
        // reviewer, since the sandbox can't contain its network after an allow.
        let policy = crate::policy::PolicyEngine::autonomous();
        let result = evaluate_tool_policy(
            &policy,
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({"command":"python script.py"}),
            alan_protocol::ToolCapability::Unknown,
            None,
            SandboxConfinement {
                fs: true,
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
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({"command":"git push --force origin main"}),
            alan_protocol::ToolCapability::Unknown,
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
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({"command":"git push origin main"}),
            alan_protocol::ToolCapability::Unknown,
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
        use alan_protocol::ToolCapability::{Read, Unknown};
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
        ] {
            assert_eq!(
                escalation_route(Some("review-git-push"), Unknown, cmd),
                EscalationRoute::AlwaysHuman,
                "force-push not routed to human: {cmd}"
            );
        }
        // A leading-`+` refspec forces a non-fast-forward update.
        assert!(is_force_push("git push origin +main:main"));
        assert!(is_force_push("git push origin +refs/heads/main"));
        // Quoted flag forms are normalized and still detected.
        assert!(is_force_push("git push origin main '--force'"));
        assert!(is_force_push("git push \"-f\" origin main"));
        // A plain push is not misclassified as force.
        assert!(!is_force_push("git push origin main"));
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Write,
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Write,
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
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({"command":"git push origin main --force"}),
            alan_protocol::ToolCapability::Unknown,
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
                alan_protocol::ToolCapability::Network,
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
            &alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            "bash",
            &json!({"query":"rust"}),
            alan_protocol::ToolCapability::Network,
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
    fn bash_shape_preflight_only_denies_without_os_sandbox() {
        let policy = crate::policy::PolicyEngine::autonomous();
        let eval = |confinement: SandboxConfinement| {
            evaluate_tool_policy(
                &policy,
                &alan_protocol::GovernanceConfig {
                    profile: alan_protocol::GovernanceProfile::Autonomous,
                    policy_path: None,
                },
                "bash",
                &json!({"command":"bash -lc 'rg TODO src'"}),
                alan_protocol::ToolCapability::Unknown,
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
    fn test_in_workspace_write_auto_approves() {
        let policy = crate::policy::PolicyEngine::autonomous();
        let result = evaluate_tool_policy(
            &policy,
            &alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            "write_file",
            &json!({"path":"a.txt","content":"x"}),
            alan_protocol::ToolCapability::Write,
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
        let policy_dir = tmp.path().join("workspace-alan");
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
        let policy = crate::policy::PolicyEngine::load_or_default(Some(policy_dir.as_path()));
        let result = evaluate_tool_policy(
            &policy,
            &alan_protocol::GovernanceConfig {
                profile: alan_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            "write_file",
            &json!({"path":"../deploy/prod.yaml","content":"version = 2"}),
            alan_protocol::ToolCapability::Write,
            Some(Path::new("/workspace/repo/src")),
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
}
