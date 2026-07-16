mod shell_syntax;

use shell_syntax::{normalize_shell_line_continuations, split_shell_fragments};

pub(super) fn classify_bash_command(command: &str) -> alan_agent_protocol::ToolCapability {
    let normalized = normalize_shell_line_continuations(command).to_lowercase();
    let fragments = split_shell_fragments(&normalized);

    let mut saw_write = false;
    let mut saw_unknown = false;
    for fragment in fragments {
        let capability = super::classify_bash_fragment(fragment.trim());
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
