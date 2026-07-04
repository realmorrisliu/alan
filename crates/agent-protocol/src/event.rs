//! System event definitions (Event Queue).
//!
//! These are events emitted by the agent to notify frontends.

use crate::{CompactionAttemptSnapshot, MemoryFlushAttemptSnapshot, PlanItem};
use serde::{Deserialize, Serialize};

/// Events emitted by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    // ========================================================================
    // Turn lifecycle
    // ========================================================================
    /// Start of a new logical user-initiated turn.
    TurnStarted {},

    /// End of the current logical turn.
    TurnCompleted {
        /// Optional summary for the completed turn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },

    // ========================================================================
    // Streaming output
    // ========================================================================
    /// Streaming text output from the assistant.
    TextDelta {
        /// Incremental text chunk.
        chunk: String,
        /// Whether this is the final chunk of the current text stream.
        is_final: bool,
    },

    /// Streaming thinking/reasoning output.
    ThinkingDelta {
        /// Incremental thinking text chunk.
        chunk: String,
        /// Whether this is the final chunk of the current thinking stream.
        is_final: bool,
    },

    // ========================================================================
    // Tool lifecycle
    // ========================================================================
    /// A tool call has started.
    ToolCallStarted {
        /// Stable tool call id emitted by the LLM/tool loop.
        id: String,
        /// Name of the tool being called.
        name: String,
        /// Optional human-readable title formatted by the runtime (e.g.
        /// `Read src/foo.rs`). Frontends display it without parsing tool args.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Optional audit metadata for the policy decision.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audit: Option<ToolDecisionAudit>,
    },

    /// A tool call has completed.
    ToolCallCompleted {
        /// Stable tool call id emitted by the LLM/tool loop.
        id: String,
        /// Name of the tool being completed, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Whether the tool completed successfully, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        success: Option<bool>,
        /// Human-readable preview of the tool result (fallback when no
        /// structured presentation is available).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result_preview: Option<String>,
        /// Optional structured, presentation-form result for rich rendering.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        presentation: Option<ToolResultPresentation>,
        /// Optional audit metadata for the policy decision.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audit: Option<ToolDecisionAudit>,
    },

    /// Transport-level plan snapshot published by `update_plan`.
    PlanUpdated {
        /// Optional explanation associated with the latest plan.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
        /// Ordered plan items for the current plan snapshot.
        items: Vec<PlanItem>,
    },

    /// Transport-level rollback notification published by `Op::Rollback`.
    SessionRolledBack {
        /// Number of logical turns removed from in-memory history.
        turns: u32,
        /// Number of tape messages removed by the rollback.
        removed_messages: usize,
    },

    // ========================================================================
    // Unified pending input
    // ========================================================================
    /// Engine is suspended, waiting for external input.
    /// Client responds with Op::Resume.
    Yield {
        /// Unique request ID — client uses this in Op::Resume.
        request_id: String,
        /// Kind of yield — tells the client what UI to render.
        kind: YieldKind,
        /// Payload with details for the specific yield kind.
        payload: serde_json::Value,
    },

    // ========================================================================
    // Warnings
    // ========================================================================
    /// A structured compaction attempt outcome was observed.
    CompactionObserved {
        /// Snapshot of the observed compaction attempt.
        attempt: CompactionAttemptSnapshot,
    },

    /// A structured memory-flush attempt outcome was observed.
    MemoryFlushObserved {
        /// Snapshot of the observed memory-flush attempt.
        attempt: MemoryFlushAttemptSnapshot,
    },

    /// A non-fatal warning occurred.
    Warning {
        /// Warning message.
        message: String,
    },

    // ========================================================================
    // Errors
    // ========================================================================
    /// An error occurred.
    Error {
        /// Error message.
        message: String,
        /// Whether the error is recoverable.
        recoverable: bool,
    },
}

/// Policy/execution-backend audit metadata attached to tool lifecycle events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDecisionAudit {
    /// Policy source identifier (e.g., builtin/workspace file).
    pub policy_source: String,
    /// Optional matched rule id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Effective action (`allow|deny|escalate`).
    pub action: String,
    /// Optional human-readable reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Classified capability (`read|write|network|unknown`).
    pub capability: String,
    /// Effective execution backend name.
    ///
    /// Field name is kept for compatibility with existing event consumers.
    pub sandbox_backend: String,
    /// Native subprocess path semantics for this backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_mode: Option<String>,
}

/// Presentation-form tool result, decoupled from tool identity. Built-in tools
/// map their output to one of these; dynamic/MCP/unknown tools fall back to
/// [`ToolResultPresentation::PlainText`]. Frontends render one renderer per form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "form", rename_all = "snake_case")]
pub enum ToolResultPresentation {
    /// A unified diff for edit/write tools.
    Diff { path: String, hunks: Vec<DiffHunk> },
    /// File content for read/glob tools.
    FileContent {
        path: String,
        lines: u64,
        #[serde(default)]
        truncated: bool,
    },
    /// A shell command and its outcome for bash tools.
    Command {
        cmdline: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        #[serde(default)]
        stdout: String,
        #[serde(default)]
        stderr: String,
        #[serde(default)]
        truncated: bool,
    },
    /// Tabular rows for grep/list_dir tools.
    Listing { rows: Vec<String> },
    /// Universal fallback for any tool result.
    PlainText { body: String },
}

/// A single diff hunk.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffHunk {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    pub lines: Vec<DiffLine>,
}

/// A single diff line, tagged by change kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DiffLine {
    Context { text: String },
    Added { text: String },
    Removed { text: String },
}

/// Kind of Yield — tells the client what UI to render.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum YieldKind {
    /// User confirmation required (approve/modify/reject).
    Confirmation,
    /// Structured user input form.
    StructuredInput,
    /// Dynamic tool call that must be executed by the client.
    DynamicTool,
    /// Extensible custom yield kind.
    Custom(String),
}

/// Event envelope used by server transports for stable cursors and replay.
///
/// The underlying event fields (including `"type"`) are flattened so existing
/// consumers can continue reading the same event payload shape while gaining
/// metadata like `event_id`, `turn_id`, and `item_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// Stable session-scoped event id (monotonic sequence encoded as string).
    pub event_id: String,
    /// Numeric session-scoped event sequence.
    pub sequence: u64,
    /// Session id the event belongs to.
    pub session_id: String,
    /// Submission id that triggered this event, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_id: Option<String>,
    /// Coarse logical turn id (monotonic within session).
    pub turn_id: String,
    /// Stable item id (monotonic within a turn).
    pub item_id: String,
    /// Server timestamp in unix epoch milliseconds.
    pub timestamp_ms: u64,
    /// Wrapped event payload (flattened to preserve the `type` field).
    #[serde(flatten)]
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_turn_started_serialization() {
        let event = Event::TurnStarted {};

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("turn_started"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Event::TurnStarted {}));
    }

    #[test]
    fn tool_presentation_fields_are_additive_and_round_trip() {
        // Absent fields stay absent in serialization (older consumers unaffected).
        let bare = Event::ToolCallCompleted {
            id: "t1".into(),
            name: Some("edit".into()),
            success: Some(true),
            result_preview: Some("ok".into()),
            presentation: None,
            audit: None,
        };
        let json = serde_json::to_string(&bare).unwrap();
        assert!(!json.contains("presentation"));

        // Each presentation primitive round-trips through JSON.
        for presentation in [
            ToolResultPresentation::Diff {
                path: "a.rs".into(),
                hunks: vec![DiffHunk {
                    header: None,
                    lines: vec![DiffLine::Added { text: "x".into() }],
                }],
            },
            ToolResultPresentation::FileContent {
                path: "a.rs".into(),
                lines: 10,
                truncated: true,
            },
            ToolResultPresentation::Command {
                cmdline: "ls".into(),
                exit_code: Some(0),
                stdout: "out".into(),
                stderr: String::new(),
                truncated: false,
            },
            ToolResultPresentation::Listing {
                rows: vec!["one".into()],
            },
            ToolResultPresentation::PlainText { body: "hi".into() },
        ] {
            let event = Event::ToolCallCompleted {
                id: "t1".into(),
                name: None,
                success: Some(true),
                result_preview: None,
                presentation: Some(presentation.clone()),
                audit: None,
            };
            let json = serde_json::to_string(&event).unwrap();
            let back: Event = serde_json::from_str(&json).unwrap();
            match back {
                Event::ToolCallCompleted {
                    presentation: Some(round),
                    ..
                } => assert_eq!(round, presentation),
                _ => panic!("expected ToolCallCompleted with presentation"),
            }
        }
    }

    #[test]
    fn tool_started_title_round_trips() {
        let event = Event::ToolCallStarted {
            id: "t1".into(),
            name: "edit".into(),
            title: Some("Edit a.rs".into()),
            audit: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        match back {
            Event::ToolCallStarted { title, .. } => {
                assert_eq!(title.as_deref(), Some("Edit a.rs"))
            }
            _ => panic!("expected ToolCallStarted"),
        }
    }

    #[test]
    fn test_event_turn_completed_serialization() {
        let event = Event::TurnCompleted { summary: None };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("turn_completed"));

        let deserialized: Event = serde_json::from_str(&json).unwrap();
        match deserialized {
            Event::TurnCompleted { summary } => assert!(summary.is_none()),
            _ => panic!("Expected TurnCompleted variant"),
        }
    }

    #[test]
    fn test_event_text_delta_serialization() {
        let event = Event::TextDelta {
            chunk: "hello".to_string(),
            is_final: false,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("text_delta"));
        assert!(json.contains("hello"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::TextDelta { chunk, is_final } => {
                assert_eq!(chunk, "hello");
                assert!(!is_final);
            }
            _ => panic!("Expected TextDelta"),
        }
    }

    #[test]
    fn test_event_thinking_delta_serialization() {
        let event = Event::ThinkingDelta {
            chunk: "reasoning".to_string(),
            is_final: true,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("thinking_delta"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::ThinkingDelta { chunk, is_final } => {
                assert_eq!(chunk, "reasoning");
                assert!(is_final);
            }
            _ => panic!("Expected ThinkingDelta"),
        }
    }

    #[test]
    fn test_event_tool_call_started_serialization() {
        let event = Event::ToolCallStarted {
            id: "call-1".to_string(),
            name: "web_search".to_string(),
            title: None,
            audit: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tool_call_started"));
        assert!(json.contains("web_search"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::ToolCallStarted {
                id,
                name,
                title: _,
                audit,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(name, "web_search");
                assert!(audit.is_none());
            }
            _ => panic!("Expected ToolCallStarted"),
        }
    }

    #[test]
    fn test_event_tool_call_completed_serialization() {
        let event = Event::ToolCallCompleted {
            id: "call-1".to_string(),
            name: Some("web_search".to_string()),
            success: Some(true),
            result_preview: Some("5 records".to_string()),
            presentation: None,
            audit: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tool_call_completed"));
        assert!(json.contains("5 records"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::ToolCallCompleted {
                id,
                name,
                success,
                result_preview,
                presentation: _,
                audit,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(name.as_deref(), Some("web_search"));
                assert_eq!(success, Some(true));
                assert_eq!(result_preview.as_deref(), Some("5 records"));
                assert!(audit.is_none());
            }
            _ => panic!("Expected ToolCallCompleted"),
        }
    }

    #[test]
    fn test_event_compaction_observed_serialization() {
        let event = Event::CompactionObserved {
            attempt: CompactionAttemptSnapshot {
                attempt_id: "attempt-1".to_string(),
                submission_id: Some("sub-1".to_string()),
                request: crate::CompactionRequestMetadata {
                    mode: crate::CompactionMode::Manual,
                    trigger: crate::CompactionTrigger::Manual,
                    reason: crate::CompactionReason::ExplicitRequest,
                    focus: Some("preserve todos".to_string()),
                },
                result: crate::CompactionResult::Success,
                pressure_level: Some(crate::CompactionPressureLevel::Hard),
                memory_flush_attempt_id: Some("flush-1".to_string()),
                input_messages: Some(12),
                output_messages: Some(3),
                input_prompt_tokens: Some(1234),
                output_prompt_tokens: Some(456),
                retry_count: 0,
                tape_mutated: true,
                warning_message: None,
                error_message: None,
                failure_streak: None,
                reference_context_revision_before: Some(4),
                reference_context_revision_after: Some(5),
                timestamp: "2026-03-17T12:00:00Z".to_string(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("compaction_observed"));
        assert!(json.contains("attempt-1"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::CompactionObserved { attempt } => {
                assert_eq!(attempt.attempt_id, "attempt-1");
                assert_eq!(attempt.submission_id.as_deref(), Some("sub-1"));
                assert_eq!(attempt.request.focus.as_deref(), Some("preserve todos"));
            }
            _ => panic!("Expected CompactionObserved"),
        }
    }

    #[test]
    fn test_event_memory_flush_observed_serialization() {
        let event = Event::MemoryFlushObserved {
            attempt: MemoryFlushAttemptSnapshot {
                attempt_id: "flush-1".to_string(),
                compaction_mode: crate::CompactionMode::AutoPreTurn,
                pressure_level: crate::CompactionPressureLevel::Soft,
                result: crate::MemoryFlushResult::Skipped,
                skip_reason: Some(crate::MemoryFlushSkipReason::ReadOnlyMemoryDir),
                source_messages: Some(8),
                output_path: None,
                warning_message: Some("memory dir is read-only".to_string()),
                error_message: None,
                timestamp: "2026-03-18T09:10:11Z".to_string(),
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("memory_flush_observed"));
        assert!(json.contains("flush-1"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::MemoryFlushObserved { attempt } => {
                assert_eq!(attempt.attempt_id, "flush-1");
                assert_eq!(attempt.result, crate::MemoryFlushResult::Skipped);
                assert_eq!(
                    attempt.skip_reason,
                    Some(crate::MemoryFlushSkipReason::ReadOnlyMemoryDir)
                );
            }
            _ => panic!("Expected MemoryFlushObserved"),
        }
    }

    #[test]
    fn test_event_tool_call_completed_without_preview() {
        let event = Event::ToolCallCompleted {
            id: "call-2".to_string(),
            name: None,
            success: None,
            result_preview: None,
            presentation: None,
            audit: None,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("tool_call_completed"));
        assert!(!json.contains("result_preview"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::ToolCallCompleted {
                id,
                name,
                success,
                result_preview,
                presentation: _,
                audit,
            } => {
                assert_eq!(id, "call-2");
                assert!(name.is_none());
                assert!(success.is_none());
                assert!(result_preview.is_none());
                assert!(audit.is_none());
            }
            _ => panic!("Expected ToolCallCompleted"),
        }
    }

    #[test]
    fn test_event_plan_updated_serialization() {
        let event = Event::PlanUpdated {
            explanation: Some("Current plan".to_string()),
            items: vec![crate::PlanItem {
                id: "p1".to_string(),
                content: "Render plan panel".to_string(),
                status: crate::PlanItemStatus::InProgress,
            }],
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("plan_updated"));
        assert!(json.contains("Current plan"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::PlanUpdated { explanation, items } => {
                assert_eq!(explanation.as_deref(), Some("Current plan"));
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, "p1");
            }
            _ => panic!("Expected PlanUpdated"),
        }
    }

    #[test]
    fn test_event_session_rolled_back_serialization() {
        let event = Event::SessionRolledBack {
            turns: 2,
            removed_messages: 4,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("session_rolled_back"));
        assert!(json.contains("\"turns\":2"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::SessionRolledBack {
                turns,
                removed_messages,
            } => {
                assert_eq!(turns, 2);
                assert_eq!(removed_messages, 4);
            }
            _ => panic!("Expected SessionRolledBack"),
        }
    }

    #[test]
    fn test_event_yield_serialization() {
        let event = Event::Yield {
            request_id: "req-1".to_string(),
            kind: YieldKind::StructuredInput,
            payload: serde_json::json!({"title": "Need input"}),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("yield"));
        assert!(json.contains("structured_input"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::Yield {
                request_id,
                kind,
                payload,
            } => {
                assert_eq!(request_id, "req-1");
                assert_eq!(kind, YieldKind::StructuredInput);
                assert_eq!(payload["title"], "Need input");
            }
            _ => panic!("Expected Yield"),
        }
    }

    #[test]
    fn test_event_error_serialization() {
        let event = Event::Error {
            message: "Something went wrong".to_string(),
            recoverable: true,
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("Something went wrong"));
        assert!(json.contains("\"recoverable\":true"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::Error {
                message,
                recoverable,
            } => {
                assert_eq!(message, "Something went wrong");
                assert!(recoverable);
            }
            _ => panic!("Expected Error"),
        }
    }

    #[test]
    fn test_event_warning_serialization() {
        let event = Event::Warning {
            message: "Stream interrupted".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("warning"));
        assert!(json.contains("Stream interrupted"));

        let parsed: Event = serde_json::from_str(&json).unwrap();
        match parsed {
            Event::Warning { message } => {
                assert_eq!(message, "Stream interrupted");
            }
            _ => panic!("Expected Warning"),
        }
    }

    #[test]
    fn test_event_envelope_serialization() {
        let envelope = EventEnvelope {
            event_id: "evt_1".to_string(),
            sequence: 1,
            session_id: "sess_1".to_string(),
            submission_id: Some("sub_1".to_string()),
            turn_id: "turn_1".to_string(),
            item_id: "item_1".to_string(),
            timestamp_ms: 1_701_000_000_000,
            event: Event::TextDelta {
                chunk: "hello".to_string(),
                is_final: false,
            },
        };

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("evt_1"));
        assert!(json.contains("text_delta"));

        let parsed: EventEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_id, "evt_1");
        assert_eq!(parsed.sequence, 1);
        assert_eq!(parsed.session_id, "sess_1");
        assert_eq!(parsed.submission_id.as_deref(), Some("sub_1"));
        assert_eq!(parsed.turn_id, "turn_1");
        assert_eq!(parsed.item_id, "item_1");
        assert_eq!(parsed.timestamp_ms, 1_701_000_000_000);
        match parsed.event {
            Event::TextDelta { chunk, is_final } => {
                assert_eq!(chunk, "hello");
                assert!(!is_final);
            }
            _ => panic!("Expected TextDelta event"),
        }
    }
}
