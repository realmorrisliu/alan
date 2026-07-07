use serde::{Deserialize, Serialize};

use crate::PlanItem;

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
}

impl UiActivitySnapshot {
    pub fn idle() -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Idle,
            started_at_ms: None,
        }
    }

    pub fn running(started_at_ms: u64) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Running,
            started_at_ms: Some(started_at_ms),
        }
    }

    pub fn paused(started_at_ms: Option<u64>) -> Self {
        Self {
            version: UI_SURFACE_VERSION,
            state: UiActivityState::Paused,
            started_at_ms,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiNoticeKind {
    #[default]
    None,
    Warning,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Activity { snapshot: UiActivitySnapshot },
    Plan { snapshot: UiPlanSnapshot },
    Thinking { snapshot: UiThinkingSnapshot },
    Notice { snapshot: UiNoticeSnapshot },
    Error { message: String, recoverable: bool },
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
    fn ui_event_round_trips() {
        let event = UiEvent::Thinking {
            snapshot: UiThinkingSnapshot::complete("reasoning".to_string(), 2),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: UiEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, event);
    }
}
