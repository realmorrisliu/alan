use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical cross-provider reasoning effort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    /// Explicitly request no provider-side reasoning where supported.
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    pub const VALUES: [ReasoningEffort; 6] = [
        ReasoningEffort::None,
        ReasoningEffort::Minimal,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::XHigh,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ReasoningEffort::None => "none",
            ReasoningEffort::Minimal => "minimal",
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
            ReasoningEffort::XHigh => "xhigh",
        }
    }

    pub fn supported_values() -> &'static str {
        "none, minimal, low, medium, high, xhigh"
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical reasoning controls carried through runtime and provider requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningControls {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
}

impl ReasoningControls {
    pub fn is_empty(&self) -> bool {
        self.effort.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reasoning_effort_round_trips_supported_values() {
        for effort in ReasoningEffort::VALUES {
            let json = serde_json::to_string(&effort).unwrap();
            assert_eq!(json, format!("\"{}\"", effort.as_str()));
            let parsed: ReasoningEffort = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, effort);
        }
    }

    #[test]
    fn reasoning_effort_rejects_unknown_values() {
        let err = serde_json::from_value::<ReasoningEffort>(json!("extreme")).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("unknown variant"));
        assert!(message.contains("minimal"));
        assert!(message.contains("xhigh"));
    }

    #[test]
    fn reasoning_controls_distinguish_unset_from_none() {
        let unset: ReasoningControls = serde_json::from_value(json!({})).unwrap();
        assert_eq!(unset.effort, None);

        let explicit_none: ReasoningControls =
            serde_json::from_value(json!({ "effort": "none" })).unwrap();
        assert_eq!(explicit_none.effort, Some(ReasoningEffort::None));
    }

    #[test]
    fn reasoning_controls_reject_raw_budget_tokens() {
        let err = serde_json::from_value::<ReasoningControls>(json!({ "budget_tokens": 512 }))
            .unwrap_err();
        assert!(err.to_string().contains("budget_tokens"));
    }
}
