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
    let has_git = tokens.iter().any(|t| t == "git" || t.ends_with("/git"));
    let has_push = tokens.iter().any(|t| t == "push");
    let rewrites_remote = tokens.iter().any(|t| {
        t == "-f"
            || t == "--force"
            || t.starts_with("--force-with-lease")
            // `--mirror` force-updates changed refs and deletes refs missing
            // locally; `--delete`/`-d` removes remote refs; `--prune` removes
            // remote refs with no local counterpart. All rewrite/remove remote
            // history like a force-push.
            || t == "--mirror"
            || t == "--delete"
            || t == "-d"
            || t == "--prune"
            // A leading `+` on a push refspec (e.g. `+main:main`) forces a
            // non-fast-forward update; a leading `:` (e.g. `:main`) deletes the
            // remote ref — equivalent to --force / --delete for that ref.
            || (t.starts_with('+') && t.len() > 1)
            || (t.starts_with(':') && t.len() > 1)
    });
    has_git && has_push && rewrites_remote
}

/// Detect a `git reset --hard` in any token ordering / quoting (errs toward
/// escalation).
fn is_reset_hard(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    tokens.iter().any(|t| t == "git" || t.ends_with("/git"))
        && tokens.iter().any(|t| t == "reset")
        && tokens.iter().any(|t| t == "--hard")
}

/// Detect a privilege-escalation command in any whitespace/quoting form (errs
/// toward always-human). The substring rule only catches `sudo ` with a trailing
/// space; tokenization catches `sudo\tls` and `/usr/bin/sudo` etc.
fn is_privilege_escalation(command: &str) -> bool {
    normalized_tokens(command).iter().any(|t| {
        matches!(t.as_str(), "sudo" | "doas" | "pkexec" | "su")
            || t.ends_with("/sudo")
            || t.ends_with("/doas")
            || t.ends_with("/pkexec")
            || t.ends_with("/su")
    })
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

/// Detect a destructive `find` action (`-delete`, `-exec`/`-execdir`,
/// `-ok`/`-okdir`) in any token form (errs toward escalation). `find` with these
/// actions deletes files or runs arbitrary commands, which the bare write
/// classification would otherwise auto-allow in-workspace.
fn is_destructive_find(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    let has_find = tokens.iter().any(|t| t == "find" || t.ends_with("/find"));
    has_find
        && tokens.iter().any(|t| {
            matches!(
                t.as_str(),
                "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir"
            )
        })
}

/// Detect a recursive `rm` whose target is a filesystem or home root, in any flag
/// ordering (errs toward deny). Catches `rm -rf /`, `rm -fr /`, `rm -rf /*`,
/// `rm -rf ~`, `rm -rf $HOME`.
fn is_catastrophic_recursive_delete(command: &str) -> bool {
    if !is_recursive_rm(command) {
        return false;
    }
    normalized_tokens(command).iter().any(|t| {
        matches!(
            t.as_str(),
            "/" | "/*" | "~" | "~/" | "$HOME" | "${HOME}" | "$HOME/" | "/*/"
        )
    })
}

/// Detect a `chmod` that grants write to "others"/"all" in any numeric or
/// symbolic form (errs toward always-human). Catches `777`, `0777`, `-R 777`,
/// `a+rwx`, `o+w`, `a=rw`; ignores owner-only (`u+w`) and revocations (`o-w`).
fn is_world_writable_chmod(command: &str) -> bool {
    let tokens = normalized_tokens(command);
    let has_chmod = tokens.iter().any(|t| t == "chmod" || t.ends_with("/chmod"));
    has_chmod && tokens.iter().any(|t| mode_grants_world_write(t))
}

fn mode_grants_world_write(token: &str) -> bool {
    // Numeric octal mode (e.g. 777, 0777, 1777): the "others" digit (last) has
    // the write bit (0o2) set.
    if (3..=4).contains(&token.len())
        && token.bytes().all(|b| b.is_ascii_digit() && b <= b'7')
        && let Some(others) = token.chars().last().and_then(|c| c.to_digit(8))
    {
        return others & 0o2 != 0;
    }
    // Symbolic mode granting write to others/all (e.g. `o+w`, `a+w`, `a=rwx`).
    if let Some(op) = token.find(['+', '=']) {
        let (who, rest) = token.split_at(op);
        let perms = &rest[1..];
        return (who.contains('o') || who.contains('a')) && perms.contains('w');
    }
    false
}

/// What the active sandbox backend permits/confines, used to gate the bash
/// degradation and the syntactic preflight.
#[derive(Debug, Clone, Copy)]
pub(super) struct SandboxConfinement {
    /// The backend is a complete bash boundary (workspace fs + network kernel-
    /// enforced — Seatbelt), so wrappers may run and escalated bash is reviewer-
    /// eligible rather than routed to a human. This does NOT mean `.git`/`.alan`/
    /// `.agents` are kernel-protected (they cannot be without breaking git) —
    /// protected-subpath tampering is blocked by the path-guard parser, and
    /// protected writes by approved code are caught by the reviewer policy.
    pub permits_autonomous_bash: bool,
    /// The OS backend confines network effects (else bash must go to a human).
    pub network: bool,
}

impl SandboxConfinement {
    /// Resolve from the active backend.
    pub fn detect() -> Self {
        Self {
            permits_autonomous_bash: crate::tools::detect_backend().permits_autonomous_bash(),
            network: crate::tools::confines_network(),
        }
    }

    #[cfg(test)]
    pub fn os_enforced() -> Self {
        Self {
            permits_autonomous_bash: true,
            network: true,
        }
    }

    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            permits_autonomous_bash: false,
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
    if !confinement.permits_autonomous_bash
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

    // Token-aware red-line gates applied under the builtin posture. The policy
    // engine's `match_command` rules are substring-based and miss equivalent
    // shell forms (flag ordering/bundling, quoting, path-qualified heads), so
    // these gates re-classify with whitespace/quote-normalized tokens. Each is a
    // documented invariant in the autonomous-review contract.
    let command_arg = arguments
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let builtin_bash = tool_name == "bash" && policy_source == "builtin_autonomous";

    // Catastrophic recursive delete of a filesystem/home root → hard deny in any
    // flag ordering (the substring `rm -rf /` rule misses `rm -fr /`).
    if builtin_bash && is_catastrophic_recursive_delete(command_arg) {
        action = crate::policy::PolicyAction::Deny;
        rule_id = Some("deny-recursive-root-delete".to_string());
        policy_reason =
            Some("recursive delete of a filesystem or home root is forbidden".to_string());
    }

    // Irreversible git reset in any token ordering/quoting → reviewer.
    if action == crate::policy::PolicyAction::Allow && builtin_bash && is_reset_hard(command_arg) {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("review-git-reset-hard".to_string());
        policy_reason = Some("irreversible git reset requires review".to_string());
    }

    // Recursive rm in any flag ordering/bundling (`rm -fr`, `rm -R -f`,
    // `rm --recursive`) → reviewer; destructive even when sandbox-contained.
    if action == crate::policy::PolicyAction::Allow && builtin_bash && is_recursive_rm(command_arg)
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("review-recursive-rm".to_string());
        policy_reason = Some("recursive delete requires review".to_string());
    }

    // Destructive `find` actions (`-delete`, `-exec …`) delete files or run
    // arbitrary commands; the bare write classification would auto-allow them
    // in-workspace, so escalate for review.
    if action == crate::policy::PolicyAction::Allow
        && builtin_bash
        && is_destructive_find(command_arg)
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("review-destructive-find".to_string());
        policy_reason = Some("destructive find action requires review".to_string());
    }

    // Privilege escalation (`sudo`/`doas`/`pkexec`/`su`, incl. `\t` and absolute
    // paths) → human, even when already reviewer-escalated, so the reviewer can
    // never approve it. The `human-` rule id forces always-human routing.
    if builtin_bash
        && action != crate::policy::PolicyAction::Deny
        && is_privilege_escalation(command_arg)
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("human-privilege-escalation".to_string());
        policy_reason = Some("privilege escalation requires human approval".to_string());
    }

    // World-writable chmod broadly weakens permissions — a human red line in any
    // numeric/symbolic form (`chmod 0777`, `chmod -R 777`, `chmod a+rwx`,
    // `chmod o+w`). The substring `chmod 777` rule misses all but the first.
    if builtin_bash
        && action != crate::policy::PolicyAction::Deny
        && is_world_writable_chmod(command_arg)
    {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("human-chmod-world-writable".to_string());
        policy_reason =
            Some("broadly weakening file permissions requires human approval".to_string());
    }

    // Safe degradation: when the sandbox does not FULLY confine bash — network
    // (it can open sockets) and protected-subpath writes (opaque code like
    // `cargo test`/`pytest` can write `.git`) — the reviewer is not a security
    // boundary for it, so an *escalated* bash command must go to a human instead.
    // Only Seatbelt confines both; Landlock cannot carve protected subpaths out of
    // the writable workspace, and the path-guard fallback confines nothing.
    //
    // This applies only to commands already escalated (`action == Escalate`):
    // destructive (`rm -rf build`), irreversible (`git reset --hard`), and opaque
    // / unknown ones (`cargo test`, `python script.py`, which all hit
    // `review-unknown`). Auto-allowed bash (`touch`, `echo`, `ls`) is left alone —
    // it is a recognized command whose path operands the parser already confined
    // to non-protected workspace paths. (`human-` ids never reach the reviewer.)
    let fully_confined = confinement.network && confinement.permits_autonomous_bash;
    if should_degrade_bash(action, tool_name, &policy_source, fully_confined) {
        action = crate::policy::PolicyAction::Escalate;
        rule_id = Some("human-bash-unconfined".to_string());
        policy_reason = Some(
            "bash is not fully sandbox-confined (network/protected paths); requires human approval"
                .to_string(),
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
    fully_confined: bool,
) -> bool {
    // Unless the sandbox FULLY confines bash (network + protected-subpath writes),
    // an *escalated* builtin bash command must go to a human rather than the
    // reviewer, which is not a security boundary here — destructive (`rm -rf
    // build`), irreversible (`git reset --hard`), and opaque/unknown commands
    // (`cargo test`, `python script.py`, all of which reach `review-unknown`).
    // Auto-allowed bash is recognized and parser-confined, so it is left alone.
    action == crate::policy::PolicyAction::Escalate
        && tool_name == "bash"
        && policy_source == "builtin_autonomous"
        && !fully_confined
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
            "workspace_policy_file",
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Write,
                None,
                SandboxConfinement::os_enforced(),
            );
            assert!(
                matches!(result, ToolPolicyDecision::Forbidden { .. }),
                "catastrophic delete not denied: {cmd}"
            );
        }
        // A scoped recursive delete is escalated for review, not denied outright.
        assert!(
            is_recursive_rm("rm -rf build") && !is_catastrophic_recursive_delete("rm -rf build")
        );
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Write,
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Write,
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
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                alan_protocol::ToolCapability::Unknown,
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
        // Landlock confines the workspace + network but cannot carve out protected
        // subpaths, so a code runner whose test/build code could write `.git`
        // (`cargo test`, `pytest`) must go to a human, not auto-run or be
        // reviewer-routed — the sandbox does not fully confine bash here.
        let policy = crate::policy::PolicyEngine::autonomous();
        for cmd in ["cargo test", "pytest -q", "make"] {
            let result = evaluate_tool_policy(
                &policy,
                &alan_protocol::GovernanceConfig::default(),
                "bash",
                &json!({ "command": cmd }),
                // Code runners classify as Unknown (→ review-unknown → Escalate).
                alan_protocol::ToolCapability::Unknown,
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
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({ "command": "touch hello.txt" }),
            alan_protocol::ToolCapability::Write,
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
            &alan_protocol::GovernanceConfig::default(),
            "bash",
            &json!({"command":"python script.py"}),
            alan_protocol::ToolCapability::Unknown,
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
