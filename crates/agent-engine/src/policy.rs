//! Policy engine for runtime tool decisions.
//!
//! This layer expresses decision semantics ("should we do this now?"),
//! while the current execution backend remains a best-effort host-side guard.

use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Policy decision action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    Allow,
    Deny,
    Escalate,
}

fn default_action_allow() -> PolicyAction {
    PolicyAction::Allow
}

/// Rule loaded from policy file.
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyRule {
    /// Optional stable id for audit/reasoning.
    #[serde(default)]
    pub id: Option<String>,
    /// Tool name or "*".
    #[serde(default)]
    pub tool: Option<String>,
    /// Capability filter: read/write/network/unknown.
    #[serde(default)]
    pub capability: Option<String>,
    /// For bash: case-insensitive substring match against command.
    #[serde(default)]
    pub match_command: Option<String>,
    /// For file-oriented tools: normalized prefix match against common path arguments.
    #[serde(default)]
    pub match_path_prefix: Option<String>,
    /// Rule action.
    pub action: PolicyAction,
    /// Optional human-readable reason.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Policy file schema (`policy.yaml` inside an `AgentRoot`).
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyFile {
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
    #[serde(default = "default_action_allow")]
    pub default_action: PolicyAction,
}

/// Evaluation input.
pub struct PolicyContext<'a> {
    pub tool_name: &'a str,
    pub arguments: &'a serde_json::Value,
    pub capability: alan_agent_protocol::ToolCapability,
    pub cwd: Option<&'a Path>,
}

/// Evaluation output with lightweight audit metadata.
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub action: PolicyAction,
    pub reason: Option<String>,
    pub rule_id: Option<String>,
    pub source: &'static str,
}

/// Runtime policy engine.
#[derive(Debug, Clone)]
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
    default_action: PolicyAction,
    source: &'static str,
}

impl PolicyEngine {
    /// Fail-closed policy for when a policy file is present but cannot be read or
    /// parsed. The profile is locked to autonomous and `policy.yaml` is the only
    /// way to *tighten* rules, so silently falling back to the permissive builtin
    /// would let a malformed *restrictive* policy quietly allow everything. Deny
    /// by default so the misconfiguration surfaces immediately and nothing runs
    /// until it is fixed.
    fn fail_closed() -> Self {
        Self {
            rules: Vec::new(),
            default_action: PolicyAction::Deny,
            source: "policy_load_failed",
        }
    }

    pub fn autonomous() -> Self {
        Self {
            rules: vec![
                PolicyRule {
                    id: Some("deny-rm-root".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("rm -rf /".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Deny,
                    reason: Some("dangerous destructive command".to_string()),
                },
                PolicyRule {
                    id: Some("deny-filesystem-wipe".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("mkfs".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Deny,
                    reason: Some("dangerous filesystem operation".to_string()),
                },
                PolicyRule {
                    id: Some("deny-block-device-write".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("dd of=/dev/".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Deny,
                    reason: Some("writing a block device".to_string()),
                },
                PolicyRule {
                    id: Some("deny-git-hooks".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some(".git/hooks".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Deny,
                    reason: Some("modifying git hooks".to_string()),
                },
                // --- always-human red line (routed to a person, never the reviewer) ---
                PolicyRule {
                    id: Some("human-git-force-push".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("git push --force".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("force push rewrites remote history".to_string()),
                },
                PolicyRule {
                    id: Some("human-git-force-with-lease".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("git push --force-with-lease".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("force push rewrites remote history".to_string()),
                },
                PolicyRule {
                    id: Some("human-sudo".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("sudo ".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("privilege escalation".to_string()),
                },
                PolicyRule {
                    id: Some("human-chmod-777".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("chmod 777".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("broadly weakening file permissions".to_string()),
                },
                PolicyRule {
                    id: Some("review-network".to_string()),
                    tool: Some("*".to_string()),
                    capability: Some("network".to_string()),
                    match_command: None,
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("network access needs human judgment".to_string()),
                },
                PolicyRule {
                    id: Some("review-destructive-rm".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("rm -rf".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("recursive delete needs human judgment".to_string()),
                },
                PolicyRule {
                    id: Some("review-git-push".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("git push".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("publishing changes needs human judgment".to_string()),
                },
                PolicyRule {
                    id: Some("review-git-reset-hard".to_string()),
                    tool: Some("bash".to_string()),
                    capability: None,
                    match_command: Some("git reset --hard".to_string()),
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("irreversible reset needs human judgment".to_string()),
                },
                PolicyRule {
                    id: Some("review-unknown".to_string()),
                    tool: Some("*".to_string()),
                    capability: Some("unknown".to_string()),
                    match_command: None,
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("unknown capability needs human judgment".to_string()),
                },
                PolicyRule {
                    id: Some("review-host-mount".to_string()),
                    tool: Some("request_mount".to_string()),
                    capability: None,
                    match_command: None,
                    match_path_prefix: None,
                    action: PolicyAction::Escalate,
                    reason: Some("host mount grants require approval".to_string()),
                },
            ],
            // Reads and writes inside explicit Host Mount grants proceed
            // automatically; the execution path guard stops out-of-view writes.
            default_action: PolicyAction::Allow,
            source: "builtin_autonomous",
        }
    }

    /// Test-only: a policy that allows everything (to exercise execution paths
    /// independent of the locked auto-approve posture).
    #[cfg(test)]
    pub fn allow_all() -> Self {
        Self {
            rules: Vec::new(),
            default_action: PolicyAction::Allow,
            source: "test_allow_all",
        }
    }

    /// Test-only: a policy that escalates everything (to exercise approval paths).
    #[cfg(test)]
    pub fn escalate_all() -> Self {
        Self {
            rules: Vec::new(),
            default_action: PolicyAction::Escalate,
            source: "test_escalate_all",
        }
    }

    /// Test-only: a policy that denies everything.
    #[cfg(test)]
    pub fn deny_all() -> Self {
        Self {
            rules: Vec::new(),
            default_action: PolicyAction::Deny,
            source: "test_deny_all",
        }
    }

    pub fn load_for_governance(
        definition_root: Option<&Path>,
        governance: &alan_agent_protocol::GovernanceConfig,
    ) -> Self {
        Self::load_for_governance_with_default_policy_path(definition_root, None, governance)
    }

    pub fn load_for_governance_with_default_policy_path(
        definition_root: Option<&Path>,
        default_policy_path: Option<&Path>,
        governance: &alan_agent_protocol::GovernanceConfig,
    ) -> Self {
        // The governance profile is locked to the auto-approve posture; only an
        // explicit `policy.yaml` can fine-tune individual rules.
        let Some(policy_path) = governance.policy_path.as_deref() else {
            return Self::load_or_default_with_default_policy_path(default_policy_path);
        };

        let resolved = match resolve_definition_policy_path(definition_root, Path::new(policy_path))
        {
            Ok(path) => path,
            Err(err) => {
                tracing::error!(
                    policy_path,
                    error = %err,
                    "Explicit governance policy is outside the Agent Definition; failing closed"
                );
                return Self::fail_closed();
            }
        };
        match load_policy_file(&resolved) {
            Ok(policy_file) => Self {
                rules: policy_file.rules,
                default_action: policy_file.default_action,
                source: "governance_policy_file",
            },
            Err(err) => {
                tracing::error!(
                    path = %resolved.display(),
                    error = %err,
                    "Failed to load explicit governance policy file; failing closed (deny-all) \
                     instead of silently using the permissive builtin"
                );
                Self::fail_closed()
            }
        }
    }

    pub fn load_or_default(default_policy_path: Option<&Path>) -> Self {
        Self::load_or_default_with_default_policy_path(default_policy_path)
    }

    pub fn load_or_default_with_default_policy_path(default_policy_path: Option<&Path>) -> Self {
        let Some(policy_path) = default_policy_path else {
            return Self::autonomous();
        };

        if !policy_path.exists() {
            return Self::autonomous();
        }

        match load_policy_file(policy_path) {
            Ok(policy_file) => Self {
                rules: policy_file.rules,
                default_action: policy_file.default_action,
                source: "definition_policy_file",
            },
            Err(err) => {
                tracing::error!(
                    path = %policy_path.display(),
                    error = %err,
                    "Failed to parse present policy file; failing closed (deny-all) instead of \
                     silently using the permissive builtin"
                );
                Self::fail_closed()
            }
        }
    }

    pub fn evaluate(&self, ctx: PolicyContext<'_>) -> PolicyDecision {
        for rule in &self.rules {
            if rule_matches(rule, &ctx) {
                return PolicyDecision {
                    action: rule.action,
                    reason: rule.reason.clone(),
                    rule_id: rule.id.clone(),
                    source: self.source,
                };
            }
        }
        PolicyDecision {
            action: self.default_action,
            reason: None,
            rule_id: None,
            source: self.source,
        }
    }
}

fn resolve_definition_policy_path(
    definition_root: Option<&Path>,
    raw_path: &Path,
) -> anyhow::Result<PathBuf> {
    let definition_root = definition_root
        .context("relative governance policy requires an Agent Definition descriptor")?;
    ensure!(
        !raw_path.is_absolute()
            && raw_path
                .components()
                .all(|component| { matches!(component, Component::Normal(_) | Component::CurDir) }),
        "governance policy path must stay relative to the Agent Definition"
    );
    let resolved = definition_root.join(raw_path);
    if resolved.exists() {
        let canonical_root = std::fs::canonicalize(definition_root)?;
        let canonical_resolved = std::fs::canonicalize(&resolved)?;
        ensure!(
            canonical_resolved.starts_with(&canonical_root),
            "governance policy escaped the Agent Definition"
        );
        Ok(canonical_resolved)
    } else {
        Ok(resolved)
    }
}

fn load_policy_file(path: &Path) -> anyhow::Result<PolicyFile> {
    let content = std::fs::read_to_string(path)?;
    let policy = serde_yaml::from_str::<PolicyFile>(&content)?;
    Ok(policy)
}

fn rule_matches(rule: &PolicyRule, ctx: &PolicyContext<'_>) -> bool {
    if let Some(tool) = rule.tool.as_deref()
        && tool != "*"
        && tool != ctx.tool_name
    {
        return false;
    }

    if let Some(capability) = rule.capability.as_deref()
        && capability != capability_label(ctx.capability)
    {
        return false;
    }

    if let Some(pattern) = rule.match_command.as_deref() {
        if ctx.tool_name != "bash" {
            return false;
        }
        let command = ctx
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        if !command.contains(&pattern.to_lowercase()) {
            return false;
        }
    }

    if let Some(path_prefix) = rule.match_path_prefix.as_deref()
        && !arguments_match_path_prefix(ctx.arguments, path_prefix, ctx.cwd)
    {
        return false;
    }

    true
}

fn arguments_match_path_prefix(
    arguments: &serde_json::Value,
    path_prefix: &str,
    current_cwd: Option<&Path>,
) -> bool {
    let normalized_prefix = normalize_path_match_value(path_prefix);

    collect_path_candidates(arguments, current_cwd)
        .into_iter()
        .any(|candidate| candidate.matches_prefix(&normalized_prefix))
}

fn collect_path_candidates(
    arguments: &serde_json::Value,
    current_cwd: Option<&Path>,
) -> Vec<NormalizedPathMatchValue> {
    const PATH_KEYS: &[&str] = &[
        "path",
        "paths",
        "directory",
        "cwd",
        "host_path",
        "namespace_path",
    ];
    const BASE_PATH_KEYS: &[&str] = &["directory", "cwd"];

    let Some(object) = arguments.as_object() else {
        return Vec::new();
    };

    let raw_candidates = collect_path_values(object, PATH_KEYS);
    let mut candidates: Vec<_> = raw_candidates
        .iter()
        .copied()
        .map(normalize_path_match_value)
        .collect();
    let base_candidates = collect_base_path_candidates(object, current_cwd, BASE_PATH_KEYS);

    for raw_candidate in raw_candidates {
        let normalized_candidate = normalize_path_match_value(raw_candidate);
        if normalized_candidate.is_absolute {
            continue;
        }
        for base_candidate in &base_candidates {
            candidates.push(normalized_candidate.resolved_against(base_candidate));
        }
    }

    candidates
}

fn collect_path_values<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Vec<&'a str> {
    let mut candidates = Vec::new();
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        match value {
            serde_json::Value::String(path) => candidates.push(path.as_str()),
            serde_json::Value::Array(paths) => {
                for path in paths.iter().filter_map(serde_json::Value::as_str) {
                    candidates.push(path);
                }
            }
            _ => {}
        }
    }

    candidates
}

fn collect_base_path_candidates(
    object: &serde_json::Map<String, serde_json::Value>,
    current_cwd: Option<&Path>,
    keys: &[&str],
) -> Vec<NormalizedPathMatchValue> {
    let mut candidates: Vec<_> = collect_path_values(object, keys)
        .into_iter()
        .map(normalize_path_match_value)
        .collect();
    if let Some(cwd) = current_cwd {
        candidates.push(normalize_path_match_value(&cwd.to_string_lossy()));
    }
    candidates
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedPathMatchValue {
    is_absolute: bool,
    has_trailing_separator: bool,
    segments: Vec<String>,
}

impl NormalizedPathMatchValue {
    fn matches_prefix(&self, prefix: &Self) -> bool {
        let normalized_prefix = prefix.render_relative();
        if normalized_prefix.is_empty() {
            return false;
        }

        if prefix.is_absolute {
            return self.is_absolute
                && path_prefix_matches(
                    &self.render_absolute(),
                    &prefix.render_absolute(),
                    prefix.has_trailing_separator,
                );
        }

        if self.is_absolute {
            return self.relative_tails().into_iter().any(|candidate| {
                path_prefix_matches(
                    &candidate,
                    &normalized_prefix,
                    prefix.has_trailing_separator,
                )
            });
        }

        path_prefix_matches(
            &self.render_relative(),
            &normalized_prefix,
            prefix.has_trailing_separator,
        )
    }

    fn render_absolute(&self) -> String {
        if self.segments.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.render_relative())
        }
    }

    fn render_relative(&self) -> String {
        self.segments.join("/")
    }

    fn relative_tails(&self) -> Vec<String> {
        (0..self.segments.len())
            .map(|index| self.segments[index..].join("/"))
            .collect()
    }

    fn resolved_against(&self, base: &Self) -> Self {
        if self.is_absolute {
            return self.clone();
        }

        let mut combined = if base.is_absolute {
            PathBuf::from(base.render_absolute())
        } else {
            PathBuf::from(base.render_relative())
        };
        combined.push(self.render_relative());
        normalize_path_match_value(&combined.to_string_lossy())
    }
}

fn normalize_path_match_value(value: &str) -> NormalizedPathMatchValue {
    let normalized_separators = value.trim().replace('\\', "/");
    let has_trailing_separator = normalized_separators.ends_with('/');
    let path = Path::new(normalized_separators.as_str());
    let mut is_absolute = path.is_absolute();
    let mut segments = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                is_absolute = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(segments.last().map(String::as_str), Some(segment) if segment != "..") {
                    segments.pop();
                } else if !is_absolute {
                    segments.push("..".to_string());
                }
            }
            Component::Normal(segment) => {
                segments.push(segment.to_string_lossy().into_owned());
            }
        }
    }

    NormalizedPathMatchValue {
        is_absolute,
        has_trailing_separator,
        segments,
    }
}

fn path_prefix_matches(candidate: &str, prefix: &str, require_component_boundary: bool) -> bool {
    if prefix.is_empty() {
        return false;
    }

    let candidate = normalize_path_match_case(candidate);
    let prefix = normalize_path_match_case(prefix);

    if require_component_boundary {
        candidate == prefix
            || candidate
                .strip_prefix(prefix.as_str())
                .is_some_and(|remaining| remaining.starts_with('/'))
    } else {
        candidate.starts_with(prefix.as_str())
    }
}

fn normalize_path_match_case(value: &str) -> String {
    value.to_lowercase()
}

fn capability_label(capability: alan_agent_protocol::ToolCapability) -> &'static str {
    match capability {
        alan_agent_protocol::ToolCapability::Read => "read",
        alan_agent_protocol::ToolCapability::Write => "write",
        alan_agent_protocol::ToolCapability::Network => "network",
        alan_agent_protocol::ToolCapability::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

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
        let error = resolve_definition_policy_path(Some(&definition), Path::new("../policy.yaml"))
            .unwrap_err();
        assert!(error.to_string().contains("stay relative"));
    }
}
