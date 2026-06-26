//! Tool loop guard for preventing infinite tool call loops.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Guardrail for tool-call loops inside a single agent turn.
pub struct ToolLoopGuard {
    max_tool_loops: Option<usize>,
    repeat_limit: usize,
    tool_loop_count: usize,
    last_tool_fingerprint: Option<String>,
    same_tool_streak: usize,
}

impl ToolLoopGuard {
    pub fn new(max_tool_loops: Option<usize>, repeat_limit: usize) -> Self {
        Self {
            max_tool_loops,
            repeat_limit,
            tool_loop_count: 0,
            last_tool_fingerprint: None,
            same_tool_streak: 0,
        }
    }

    /// Called before executing each tool call.
    /// Returns a user-facing stop message if repeated-call guard is triggered.
    pub fn before_tool_call(&mut self, tool_name: &str, arguments: &Value) -> Option<String> {
        if self.repeat_limit == 0 {
            return None;
        }

        let fingerprint = fingerprint_tool_call(tool_name, arguments);
        if self.last_tool_fingerprint.as_deref() == Some(fingerprint.as_str()) {
            self.same_tool_streak += 1;
        } else {
            self.same_tool_streak = 1;
            self.last_tool_fingerprint = Some(fingerprint);
        }

        if self.same_tool_streak > self.repeat_limit {
            return Some(format!(
                "Stopped due to repeated identical tool calls ({} in a row). You can adjust TOOL_REPEAT_LIMIT or revise the prompt to proceed.",
                self.same_tool_streak
            ));
        }

        None
    }

    /// Called after one tool-call batch is fully processed and before next LLM iteration.
    /// Returns a user-facing stop message if max-loop guard is triggered.
    pub fn after_tool_batch(&mut self) -> Option<String> {
        self.tool_loop_count += 1;
        if let Some(limit) = self.max_tool_loops
            && self.tool_loop_count >= limit
        {
            return Some(format!(
                "Stopped after reaching MAX_TOOL_LOOPS={} for this turn. You can continue by sending another user input or increase MAX_TOOL_LOOPS.",
                limit
            ));
        }

        None
    }
}

fn fingerprint_tool_call(tool_name: &str, arguments: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(tool_name.as_bytes());
    hasher.update(b"\n");
    hasher.update(arguments.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_call_guard_triggers() {
        let mut guard = ToolLoopGuard::new(None, 2);
        let args = serde_json::json!({"q":"x"});
        assert!(guard.before_tool_call("web_search", &args).is_none());
        assert!(guard.before_tool_call("web_search", &args).is_none());
        assert!(guard.before_tool_call("web_search", &args).is_some());
    }

    #[test]
    fn max_loop_guard_triggers() {
        let mut guard = ToolLoopGuard::new(Some(2), 0);
        assert!(guard.after_tool_batch().is_none());
        assert!(guard.after_tool_batch().is_some());
    }
}
