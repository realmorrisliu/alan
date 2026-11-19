use serde::{Deserialize, Serialize};

use crate::PlanItem;

/// Payload-free authoritative admission receipt. Missing or invalid documents are unknown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UiQueueSnapshot {
    pub version: u16,
    pub revision: u64,
    pub known: bool,
    pub pending_submission_ids: Vec<String>,
    pub active_submission_ids: Vec<String>,
    pub paused: bool,
    pub deferred: bool,
    pub uncertain_submission_ids: Vec<String>,
}

impl Default for UiQueueSnapshot {
    fn default() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            revision: 0,
            known: false,
            pending_submission_ids: Vec::new(),
            active_submission_ids: Vec::new(),
            paused: false,
            deferred: false,
            uncertain_submission_ids: Vec::new(),
        }
    }
}

impl UiQueueSnapshot {
    pub fn is_valid(&self) -> bool {
        self.version == UI_SURFACE_VERSION
            && if self.known {
                self.revision > 0
            } else {
                self.pending_submission_ids.is_empty()
                    && self.active_submission_ids.is_empty()
                    && self.uncertain_submission_ids.is_empty()
                    && !self.paused
                    && !self.deferred
            }
    }
}

pub const UI_SURFACE_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiActivityState {
    #[default]
    Idle,
    Running,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiActivitySnapshot {
    pub version: u16,
    pub state: UiActivityState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    /// Inputs awaiting a response; empty for a queue-only pause.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting_submission_ids: Vec<String>,
}

impl UiActivitySnapshot {
    pub fn idle() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Idle,
            started_at_ms: None,
            waiting_submission_ids: Vec::new(),
        }
    }

    pub fn running(started_at_ms: u64) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Running,
            started_at_ms: Some(started_at_ms),
            waiting_submission_ids: Vec::new(),
        }
    }

    pub fn paused(started_at_ms: Option<u64>) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Paused,
            started_at_ms,
            waiting_submission_ids: Vec::new(),
        }
    }
}

impl Default for UiActivitySnapshot {
    fn default() -> Self {
        Self::idle()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiPlanSnapshot {
    pub version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(default)]
    pub items: Vec<PlanItem>,
}

impl UiPlanSnapshot {
    pub fn empty() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            explanation: None,
            items: Vec::new(),
        }
    }

    pub fn new(explanation: Option<String>, items: Vec<PlanItem>) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            explanation,
            items,
        }
    }
}

impl Default for UiPlanSnapshot {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiThinkingState {
    #[default]
    Idle,
    Streaming,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiThinkingSnapshot {
    pub version: u16,
    pub state: UiThinkingState,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

impl UiThinkingSnapshot {
    pub fn idle() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiThinkingState::Idle,
            text: String::new(),
            duration_secs: None,
        }
    }

    pub fn streaming(text: String) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiThinkingState::Streaming,
            text,
            duration_secs: None,
        }
    }

    pub fn complete(text: String, duration_secs: u64) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiThinkingState::Complete,
            text,
            duration_secs: Some(duration_secs),
        }
    }
}

impl Default for UiThinkingSnapshot {
    fn default() -> Self {
        Self::idle()
    }
}

/// Turn and session token/cost accounting. Counters are zero until a provider reports usage.
///
/// The unprefixed counters cover the current logical turn (all of its generations); the
/// `session_` counters cover the whole Agent Process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UiUsageSnapshot {
    pub version: u16,
    /// Provider-reported prompt tokens this turn.
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    /// Provider-reported completion tokens this turn.
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Prompt tokens of the most recent generation: the current context occupancy.
    #[serde(default)]
    pub context_tokens: u64,
    /// Model context window in tokens; zero when unknown.
    #[serde(default)]
    pub context_window: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub session_input_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub session_output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_reasoning_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_cost_usd: Option<f64>,
}

impl UiUsageSnapshot {
    /// No usage reported yet.
    pub fn empty() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            ..Self::default()
        }
    }

    /// Whether the document is a version this crate understands.
    pub fn is_valid(&self) -> bool {
        self.version == UI_SURFACE_VERSION
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiNoticeKind {
    #[default]
    None,
    Warning,
    Error,
    Rollback,
    Compaction,
    MemoryFlush,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiNoticeSnapshot {
    pub version: u16,
    pub kind: UiNoticeKind,
    #[serde(default)]
    pub message: String,
}

impl UiNoticeSnapshot {
    pub fn none() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            kind: UiNoticeKind::None,
            message: String::new(),
        }
    }

    pub fn new(kind: UiNoticeKind, message: impl Into<String>) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            kind,
            message: message.into(),
        }
    }
}

impl Default for UiNoticeSnapshot {
    fn default() -> Self {
        Self::none()
    }
}

/// Terminal outcome of advancing accepted input, distinct from Process exit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiInputStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    /// Emitted after settlement; Tape/Actions contain the correlated result.
    InputCompleted {
        submission_ids: Vec<String>,
        status: UiInputStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    Activity {
        snapshot: UiActivitySnapshot,
    },
    Plan {
        snapshot: UiPlanSnapshot,
    },
    Thinking {
        snapshot: UiThinkingSnapshot,
    },
    Notice {
        snapshot: UiNoticeSnapshot,
    },
    Error {
        message: String,
        recoverable: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_default_to_version_one() {
        assert_eq!(UiActivitySnapshot::default().version, UI_SURFACE_VERSION);
        assert_eq!(UiPlanSnapshot::default().version, UI_SURFACE_VERSION);
        assert_eq!(UiThinkingSnapshot::default().version, UI_SURFACE_VERSION);
        assert_eq!(UiNoticeSnapshot::default().version, UI_SURFACE_VERSION);
    }

    #[test]
    fn usage_snapshot_round_trips_and_omits_unknown_costs() {
        let empty = UiUsageSnapshot::empty();
        assert!(empty.is_valid());
        let json = serde_json::to_string(&empty).unwrap();
        assert!(!json.contains("cost_usd"));
        assert!(!json.contains("cached_input_tokens"));
        let usage = UiUsageSnapshot {
            input_tokens: 12,
            cached_input_tokens: Some(8),
            output_tokens: 3,
            reasoning_tokens: Some(1),
            context_tokens: 12,
            context_window: 128_000,
            cost_usd: Some(0.0012),
            session_input_tokens: 40,
            session_output_tokens: 9,
            ..UiUsageSnapshot::empty()
        };
        let parsed: UiUsageSnapshot =
            serde_json::from_str(&serde_json::to_string(&usage).unwrap()).unwrap();
        assert_eq!(parsed, usage);
        // Older writers that only know the turn fields still parse.
        let minimal: UiUsageSnapshot =
            serde_json::from_str(r#"{"version":1,"input_tokens":5}"#).unwrap();
        assert_eq!(minimal.input_tokens, 5);
        assert_eq!(minimal.session_output_tokens, 0);
    }

    #[test]
    fn ui_event_round_trips() {
        let event = UiEvent::Thinking {
            snapshot: UiThinkingSnapshot::complete("reasoning".to_string(), 2),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: UiEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }
}
