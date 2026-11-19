//! Embedder-supplied synchronous policy hook evaluated before every Tool execution.

use alan_agent_protocol::ToolCapability;
use serde_json::Value;

/// Decision returned by a synchronous embedder [`ToolPolicyHook`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookDecision {
    /// Defer to Alan's built-in policy engine, trust posture, and Guardian.
    Continue,
    /// Allow execution unless denied by a hard static policy rule.
    Allow,
    /// Synchronously deny execution with a domain reason (cannot be escalated).
    Deny(String),
    /// Force human/guardian confirmation escalation with the given reason.
    Escalate(String),
}

/// Host-supplied synchronous policy hook evaluated before Tool execution.
pub trait ToolPolicyHook: Send + Sync {
    /// Inspect tool name, parsed arguments, and classified capability before execution.
    fn evaluate(&self, tool_name: &str, args: &Value, capability: ToolCapability) -> HookDecision;
}
