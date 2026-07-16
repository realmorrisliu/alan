use serde_json::json;

#[derive(Debug, Clone)]
pub(super) enum ToolPolicyDecision {
    Allow {
        audit: alan_agent_protocol::ToolDecisionAudit,
    },
    Escalate {
        summary: String,
        details: serde_json::Value,
        audit: alan_agent_protocol::ToolDecisionAudit,
        route: EscalationRoute,
    },
    Forbidden {
        reason: String,
        audit: alan_agent_protocol::ToolDecisionAudit,
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
    capability: alan_agent_protocol::ToolCapability,
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
    if matches!(capability, alan_agent_protocol::ToolCapability::Network)
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
/// classification would otherwise auto-allow inside an explicit writable Host Mount.
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
    /// The backend is a complete bash boundary (Host Mount file view + network kernel-
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
    governance: &alan_agent_protocol::GovernanceConfig,
    tool_name: &str,
    arguments: &serde_json::Value,
    capability: alan_agent_protocol::ToolCapability,
    current_cwd: Option<&std::path::Path>,
    confinement: SandboxConfinement,
) -> ToolPolicyDecision {
    let sandbox_backend = crate::tools::active_backend_name();
    let path_mode = crate::tools::active_backend_path_mode().to_string();
    // The bash-shape preflight is the Host-Mount path-guard parser standing in for
    // confinement. With a kernel-enforced OS sandbox active, confinement is
    // independent of command syntax, so this syntactic deny must not block
    // commands the sandbox would safely contain (e.g. `python -c ...`,
    // `bash -lc ...`). Apply it only on the path-guard fallback.
    if !confinement.permits_autonomous_bash
        && let Some(reason) = bash_shape_preflight_reason(tool_name, arguments)
    {
        return ToolPolicyDecision::Forbidden {
            reason: reason.clone(),
            audit: alan_agent_protocol::ToolDecisionAudit {
                policy_source: "sandbox_preflight".to_string(),
                rule_id: None,
                action: "deny".to_string(),
                reason: Some(reason),
                capability: capability_label(capability).to_string(),
                sandbox_backend: sandbox_backend.to_string(),
                path_mode: Some(path_mode),
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
    // inside a writable Host Mount, so escalate for review.
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
    // a writable Host Mount, and the path-guard fallback confines nothing.
    //
    // This applies only to commands already escalated (`action == Escalate`):
    // destructive (`rm -rf build`), irreversible (`git reset --hard`), and opaque
    // / unknown ones (`cargo test`, `python script.py`, which all hit
    // `review-unknown`). Auto-allowed bash (`touch`, `echo`, `ls`) is left alone —
    // it is a recognized command whose path operands the parser already confined
    // to non-protected Host Mount paths. (`human-` ids never reach the reviewer.)
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
            audit: alan_agent_protocol::ToolDecisionAudit {
                policy_source,
                rule_id,
                action: "allow".to_string(),
                reason: policy_reason,
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
                path_mode: Some(path_mode),
            },
        },
        crate::policy::PolicyAction::Deny => ToolPolicyDecision::Forbidden {
            reason: policy_reason
                .clone()
                .unwrap_or_else(|| format!("Tool '{}' denied by policy", tool_name)),
            audit: alan_agent_protocol::ToolDecisionAudit {
                policy_source,
                rule_id,
                action: "deny".to_string(),
                reason: policy_reason,
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
                path_mode: Some(path_mode),
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
                "sandbox_backend": sandbox_backend,
                "path_mode": path_mode
            }),
            audit: alan_agent_protocol::ToolDecisionAudit {
                policy_source,
                rule_id,
                action: "escalate".to_string(),
                reason: policy_reason,
                capability: capability_kind,
                sandbox_backend: sandbox_backend.to_string(),
                path_mode: Some(path_mode),
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

pub(super) fn capability_label(capability: alan_agent_protocol::ToolCapability) -> &'static str {
    match capability {
        alan_agent_protocol::ToolCapability::Read => "read",
        alan_agent_protocol::ToolCapability::Write => "write",
        alan_agent_protocol::ToolCapability::Network => "network",
        alan_agent_protocol::ToolCapability::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests;
