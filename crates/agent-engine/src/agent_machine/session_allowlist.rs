use std::collections::HashSet;

use serde_json::Value;

/// Strictly Process-local session-scoped tool approval key.
///
/// Keyed by tool name, capability label, and normalized command pattern. Never
/// persisted across Process restarts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SessionToolApprovalKey {
    pub(crate) tool_name: String,
    pub(crate) capability: String,
    pub(crate) command_pattern: Option<String>,
}

/// In-memory session allow-list scoped strictly to a single Agent Process.
#[derive(Debug, Clone, Default)]
pub(super) struct SessionToolAllowlist {
    allowed: HashSet<SessionToolApprovalKey>,
}

impl SessionToolAllowlist {
    pub(super) fn is_allowed(&self, tool_name: &str, capability: &str, arguments: &Value) -> bool {
        let keys = session_approval_keys_for_tool(tool_name, capability, arguments);
        !keys.is_empty() && keys.iter().all(|key| self.allowed.contains(key))
    }

    pub(super) fn allow(&mut self, tool_name: &str, capability: &str, arguments: &Value) {
        for key in session_approval_keys_for_tool(tool_name, capability, arguments) {
            self.allowed.insert(key);
        }
    }
}

fn session_approval_keys_for_tool(
    tool_name: &str,
    capability: &str,
    arguments: &Value,
) -> Vec<SessionToolApprovalKey> {
    if tool_name == "bash"
        && let Some(command) = arguments.get("command").and_then(Value::as_str)
    {
        let patterns = bash_session_command_patterns(command);
        if !patterns.is_empty() {
            return patterns
                .into_iter()
                .map(|pattern| SessionToolApprovalKey {
                    tool_name: tool_name.to_string(),
                    capability: capability.to_string(),
                    command_pattern: Some(pattern),
                })
                .collect();
        }
    }

    vec![SessionToolApprovalKey {
        tool_name: tool_name.to_string(),
        capability: capability.to_string(),
        command_pattern: None,
    }]
}

fn bash_session_command_patterns(command: &str) -> Vec<String> {
    split_shell_fragments(command)
        .into_iter()
        .filter_map(|fragment| extract_fragment_command_pattern(&fragment))
        .collect()
}

fn split_shell_fragments(command: &str) -> Vec<String> {
    let normalized = command.replace("\\\n", " ");
    let chars: Vec<char> = normalized.chars().collect();
    let mut fragments = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut idx = 0;

    while idx < chars.len() {
        let ch = chars[idx];
        if escaped {
            current.push(ch);
            escaped = false;
            idx += 1;
            continue;
        }
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            current.push(ch);
            idx += 1;
            continue;
        }
        if in_double {
            if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_double = false;
            }
            current.push(ch);
            idx += 1;
            continue;
        }
        match ch {
            '\\' => {
                escaped = true;
                current.push(ch);
            }
            '\'' => {
                in_single = true;
                current.push(ch);
            }
            '"' => {
                in_double = true;
                current.push(ch);
            }
            ';' | '|' | '&' | '\n' => {
                if matches!(ch, '|' | '&') && chars.get(idx + 1).copied() == Some(ch) {
                    idx += 1;
                }
                if !current.trim().is_empty() {
                    fragments.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
        idx += 1;
    }
    if !current.trim().is_empty() {
        fragments.push(current);
    }
    fragments
}

fn extract_fragment_command_pattern(fragment: &str) -> Option<String> {
    let tokens: Vec<String> = fragment
        .split_whitespace()
        .map(|tok| tok.trim_matches(['\'', '"']).to_string())
        .filter(|tok| !tok.is_empty())
        .collect();
    let mut idx = 0;
    while let Some(tok) = tokens.get(idx) {
        if is_env_assignment_token(tok)
            || matches!(tok.as_str(), "env" | "command" | "builtin" | "nohup")
        {
            idx += 1;
            continue;
        }
        break;
    }
    let head_raw = tokens.get(idx)?;
    let head = head_raw.rsplit('/').next().unwrap_or(head_raw.as_str());
    let rest = &tokens[idx + 1..];
    if head == "rm" {
        let recursive = rest.iter().any(|t| {
            t == "--recursive"
                || (t.starts_with('-')
                    && !t.starts_with("--")
                    && t.chars().skip(1).any(|c| c == 'r' || c == 'R'))
        });
        return Some(if recursive {
            "rm -r".to_string()
        } else {
            "rm".to_string()
        });
    }
    if head == "git" {
        let mut i = 0;
        let mut subcmd = None;
        while let Some(tok) = rest.get(i) {
            if matches!(tok.as_str(), "-c" | "-C" | "--git-dir" | "--work-tree") {
                i += 2;
                continue;
            }
            if tok.starts_with('-') {
                i += 1;
                continue;
            }
            subcmd = Some(tok.as_str());
            break;
        }
        if let Some(sub) = subcmd {
            if sub == "push"
                && rest.iter().any(|t| {
                    matches!(
                        t.as_str(),
                        "-f" | "--force" | "--mirror" | "--delete" | "-d" | "--prune"
                    ) || t.starts_with("--force-with-lease")
                        || (t.starts_with('+') && t.len() > 1)
                        || (t.starts_with(':') && t.len() > 1)
                })
            {
                return Some("git push --force".to_string());
            }
            if sub == "reset" && rest.iter().any(|t| t == "--hard") {
                return Some("git reset --hard".to_string());
            }
            return Some(format!("git {sub}"));
        }
        return Some("git".to_string());
    }
    if matches!(
        head,
        "cargo"
            | "npm"
            | "pnpm"
            | "yarn"
            | "bun"
            | "pip"
            | "pip3"
            | "uv"
            | "docker"
            | "kubectl"
            | "gh"
            | "make"
            | "just"
    ) {
        let mut i = 0;
        while let Some(tok) = rest.get(i) {
            if head == "cargo" && tok.starts_with('+') {
                i += 1;
                continue;
            }
            if matches!(
                tok.as_str(),
                "-C" | "--dir" | "--filter" | "--prefix" | "--cwd" | "--manifest-path" | "-p"
            ) {
                i += 2;
                continue;
            }
            if tok.starts_with('-') {
                i += 1;
                continue;
            }
            return Some(format!("{head} {tok}"));
        }
    }
    Some(head.to_string())
}

fn is_env_assignment_token(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
