mod git;
mod read;
mod shell_syntax;
mod wrappers;
mod write;

use git::is_git_network_command;
use read::is_safe_read_command;
use shell_syntax::{normalize_shell_line_continuations, split_shell_fragments};
use wrappers::{
    contains_nested_eval_wrapper, contains_unsupported_shell_form, effective_command_tokens,
    is_wrapper_query_command,
};
use write::is_write_command;

pub(super) fn classify_bash_command(command: &str) -> alan_agent_protocol::ToolCapability {
    let normalized = normalize_shell_line_continuations(command).to_lowercase();
    let fragments = split_shell_fragments(&normalized);

    let mut saw_write = false;
    let mut saw_unknown = false;
    for fragment in fragments {
        let capability = classify_bash_fragment(fragment.trim());
        if matches!(capability, alan_agent_protocol::ToolCapability::Network) {
            return alan_agent_protocol::ToolCapability::Network;
        }
        if matches!(capability, alan_agent_protocol::ToolCapability::Write) {
            saw_write = true;
        }
        if matches!(capability, alan_agent_protocol::ToolCapability::Unknown) {
            saw_unknown = true;
        }
    }

    if saw_write {
        alan_agent_protocol::ToolCapability::Write
    } else if saw_unknown {
        alan_agent_protocol::ToolCapability::Unknown
    } else {
        alan_agent_protocol::ToolCapability::Read
    }
}

fn classify_bash_fragment(fragment: &str) -> alan_agent_protocol::ToolCapability {
    if fragment.is_empty() {
        return alan_agent_protocol::ToolCapability::Read;
    }

    let tokens: Vec<&str> = fragment.split_whitespace().collect();
    if tokens.is_empty() {
        return alan_agent_protocol::ToolCapability::Read;
    }
    let effective_tokens = effective_command_tokens(&tokens);
    let effective_tokens = effective_tokens.as_slice();

    if contains_unsupported_shell_form(&tokens) {
        return alan_agent_protocol::ToolCapability::Unknown;
    }
    if is_network_command(fragment, effective_tokens) {
        return alan_agent_protocol::ToolCapability::Network;
    }
    if contains_nested_eval_wrapper(&tokens) {
        return alan_agent_protocol::ToolCapability::Unknown;
    }
    if is_write_command(fragment, effective_tokens) {
        return alan_agent_protocol::ToolCapability::Write;
    }
    if is_safe_read_command(effective_tokens) || is_wrapper_query_command(&tokens) {
        return alan_agent_protocol::ToolCapability::Read;
    }
    alan_agent_protocol::ToolCapability::Unknown
}

fn is_network_command(fragment: &str, tokens: &[&str]) -> bool {
    // Match on the basename so path-qualified forms (`/usr/bin/curl`) classify
    // like the bare head; otherwise an approved network call would run with the
    // sandbox network deny still in force and fail.
    let head = wrappers::command_basename(tokens[0]);
    if matches!(
        head,
        "curl" | "wget" | "ssh" | "scp" | "sftp" | "nc" | "netcat" | "socat" | "telnet" | "ftp"
    ) {
        return true;
    }

    let pair = tokens.get(1).copied().unwrap_or_default();
    if (head == "git" && is_git_network_command(tokens))
        || (head == "docker" && pair == "pull")
        || (head == "npm" && pair == "install")
        || (head == "pnpm" && pair == "add")
        || (head == "yarn" && pair == "add")
        || ((head == "pip" || head == "pip3") && pair == "install")
        || (head == "cargo" && pair == "install")
        || (head == "brew" && pair == "install")
        || ((head == "apt" || head == "apt-get" || head == "yum" || head == "dnf")
            && pair == "install")
    {
        return true;
    }

    // Catch explicit http(s) fetch commands wrapped in generic shells.
    fragment.contains("http://") || fragment.contains("https://")
}
