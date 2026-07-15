//! User operation definitions (Submission Queue).
//!
//! These are the operations that users can submit to the agent.

use serde::{Deserialize, Serialize};

use crate::{ContentPart, ReasoningEffort};

/// Coarse capability class for tool policy decisions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapability {
    Read,
    Write,
    Network,
    Unknown,
}

/// Governance profile for tool policy behavior.
///
/// The agent runs a single, locked `Autonomous` posture: routine reads and
/// in-workspace writes proceed automatically; operations needing judgment
/// (network, destructive/irreversible commands, unknown capability) escalate
/// and are routed to the reviewer (see the `autonomous-review-mode` capability),
/// with a deterministic red line bypassing the reviewer to deny or to the human.
/// There is intentionally no mode switcher or alternate serialized profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceProfile {
    #[default]
    Autonomous,
}

/// Agent Process governance configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GovernanceConfig {
    /// Builtin profile baseline.
    #[serde(default)]
    pub profile: GovernanceProfile,
    /// Optional policy file path override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_path: Option<String>,
}

/// Input handling mode for `Op::Input`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    /// Inject guidance into the currently active execution.
    Steer,
    /// Queue intent and execute immediately after current execution completes.
    FollowUp,
    /// Queue context for the next explicit `Op::Turn` only.
    NextTurn,
}

/// Status for a plan item in transport-level progress updates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanItemStatus {
    Pending,
    InProgress,
    Completed,
}

/// Transport-level plan item for UI synchronization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanItem {
    pub id: String,
    pub content: String,
    pub status: PlanItemStatus,
}

/// User-submitted operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    // ========================================================================
    // New unified operations (Phase 2)
    // ========================================================================
    /// Start a new reasoning turn.
    /// This is a user-initiated conversation turn with full context metadata.
    Turn {
        /// Content parts for the turn input.
        parts: Vec<ContentPart>,
        /// Optional turn context metadata.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<TurnContext>,
    },

    /// Append user input with explicit routing semantics.
    Input {
        /// User's input content parts.
        parts: Vec<ContentPart>,
        /// Input routing mode (`steer`, `follow_up`, `next_turn`).
        mode: InputMode,
    },

    /// Resume a suspended Yield request.
    /// Unified replacement for Confirm, StructuredUserInput, DynamicToolResult.
    Resume {
        /// The request_id from the corresponding Yield event.
        request_id: String,
        /// Resume payload content.
        content: Vec<ContentPart>,
    },

    /// Interrupt current execution.
    Interrupt,

    /// Compact the current Agent Machine context with optional guidance.
    CompactWithOptions {
        /// Optional focus for the summary handoff, for example "preserve todos".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        focus: Option<String>,
    },

    /// Roll back the last N user turns from in-memory Agent Machine context.
    Rollback {
        /// Number of user turns to remove (must be >= 1)
        turns: u32,
    },
}

/// Turn context metadata — attached to Turn ops.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TurnContext {
    /// Optional one-turn reasoning effort override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
}

/// A submission wrapping an operation with an ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    /// Unique submission ID
    pub id: String,
    /// The operation being submitted
    pub op: Op,
}

impl Submission {
    /// Create a new submission with a generated UUID
    pub fn new(op: Op) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            op,
        }
    }

    /// Create a new submission with a specific ID (useful for testing)
    #[cfg(test)]
    pub fn with_id(id: &str, op: Op) -> Self {
        Self {
            id: id.to_string(),
            op,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::{
        AdaptivePresentationHint, StructuredInputKind, StructuredInputOption,
        StructuredInputQuestion,
    };
    use serde_json::json;

    #[test]
    fn test_op_serialization_compact_with_options() {
        let op = Op::CompactWithOptions {
            focus: Some("preserve todos and constraints".to_string()),
        };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("compact_with_options"));
        assert!(json.contains("preserve todos and constraints"));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::CompactWithOptions { focus } => {
                assert_eq!(focus.as_deref(), Some("preserve todos and constraints"));
            }
            _ => panic!("Expected CompactWithOptions variant"),
        }
    }

    #[test]
    fn test_op_serialization_rollback() {
        let op = Op::Rollback { turns: 2 };
        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("rollback"));
        assert!(json.contains("\"turns\":2"));
        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Rollback { turns } => assert_eq!(turns, 2),
            _ => panic!("Expected Rollback variant"),
        }
    }

    #[test]
    fn test_structured_input_question_serialization_includes_kind_and_metadata() {
        let question = StructuredInputQuestion {
            id: "environment".to_string(),
            label: "Environment".to_string(),
            prompt: "Choose the deployment target".to_string(),
            kind: StructuredInputKind::MultiSelect,
            required: true,
            placeholder: None,
            help_text: Some("Select every environment you want to deploy.".to_string()),
            default_value: None,
            default_values: vec!["staging".to_string()],
            min_selected: Some(1),
            max_selected: Some(2),
            options: vec![
                StructuredInputOption {
                    value: "staging".to_string(),
                    label: "Staging".to_string(),
                    description: None,
                },
                StructuredInputOption {
                    value: "production".to_string(),
                    label: "Production".to_string(),
                    description: Some("Requires approval".to_string()),
                },
            ],
            presentation_hints: vec![AdaptivePresentationHint::Searchable],
        };

        let value = serde_json::to_value(&question).unwrap();
        assert_eq!(value["kind"], "multi_select");
        assert_eq!(value["defaults"], json!(["staging"]));
        assert_eq!(value["min_selected"], 1);
        assert_eq!(value["max_selected"], 2);
        assert_eq!(value["presentation_hints"], json!(["searchable"]));
    }

    #[test]
    fn test_structured_input_question_deserialization_defaults_to_text_kind() {
        let value = json!({
            "id": "branch",
            "label": "Branch",
            "prompt": "Branch name",
            "required": false
        });

        let question: StructuredInputQuestion = serde_json::from_value(value).unwrap();
        assert_eq!(question.kind, StructuredInputKind::Text);
        assert_eq!(question.placeholder, None);
        assert_eq!(question.default_value, None);
        assert!(question.default_values.is_empty());
        assert!(question.presentation_hints.is_empty());
    }

    #[test]
    fn test_governance_profile_serialization() {
        let json = serde_json::to_string(&GovernanceProfile::Autonomous).unwrap();
        assert_eq!(json, "\"autonomous\"");
        let parsed: GovernanceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, GovernanceProfile::Autonomous);

        for bad in [
            "\"auto_approve\"",
            "\"auto-approve\"",
            "\"autoapprove\"",
            "\"conservative\"",
            "\"conservativ\"",
            "\"strict\"",
            "false",
            "{}",
            "5",
        ] {
            assert!(
                serde_json::from_str::<GovernanceProfile>(bad).is_err(),
                "retired or malformed governance profile accepted: {bad}"
            );
        }
    }

    #[test]
    fn test_governance_config_default() {
        let cfg = GovernanceConfig::default();
        assert_eq!(cfg.profile, GovernanceProfile::Autonomous);
        assert_eq!(cfg.policy_path, None);
    }

    #[test]
    fn test_submission_new() {
        let op = Op::Interrupt;
        let submission = Submission::new(op);

        assert!(!submission.id.is_empty());
        assert!(matches!(submission.op, Op::Interrupt));
    }

    #[test]
    fn test_submission_serialization() {
        let submission = Submission::with_id("test-id-123", Op::Interrupt);

        let json = serde_json::to_string(&submission).unwrap();
        assert!(json.contains("test-id-123"));
        assert!(json.contains("interrupt"));

        let deserialized: Submission = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "test-id-123");
        assert!(matches!(deserialized.op, Op::Interrupt));
    }

    // ========================================================================
    // Tests for new Phase 2 Op variants
    // ========================================================================

    #[test]
    fn test_op_serialization_turn() {
        let op = Op::Turn {
            parts: vec![ContentPart::text("Hello agent")],
            context: Some(TurnContext {
                reasoning_effort: Some(ReasoningEffort::High),
            }),
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("turn"));
        assert!(json.contains("Hello agent"));
        assert!(json.contains("high"));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Turn { parts, context } => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].as_text(), Some("Hello agent"));
                let ctx = context.unwrap();
                assert_eq!(ctx.reasoning_effort, Some(ReasoningEffort::High));
            }
            _ => panic!("Expected Turn variant"),
        }
    }

    #[test]
    fn test_op_serialization_turn_minimal() {
        let op = Op::Turn {
            parts: vec![ContentPart::text("Hi")],
            context: None,
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("turn"));
        assert!(!json.contains("context")); // None should be skipped

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Turn { parts, context } => {
                assert_eq!(parts[0].as_text(), Some("Hi"));
                assert!(context.is_none());
            }
            _ => panic!("Expected Turn variant"),
        }
    }

    #[test]
    fn test_op_rejects_legacy_thinking_budget_in_turn_context() {
        let payload = json!({
            "type": "turn",
            "parts": [
                { "type": "text", "text": "Hi" }
            ],
            "context": {
                "thinking_budget_tokens": 2048
            }
        });

        let err = serde_json::from_value::<Op>(payload).unwrap_err();
        assert!(err.to_string().contains("thinking_budget_tokens"));
    }

    #[test]
    fn test_op_serialization_input() {
        let op = Op::Input {
            parts: vec![ContentPart::text("follow up")],
            mode: InputMode::Steer,
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("input"));
        assert!(json.contains("follow up"));
        assert!(json.contains("\"mode\":\"steer\""));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Input { parts, mode } => {
                assert_eq!(parts[0].as_text(), Some("follow up"));
                assert_eq!(mode, InputMode::Steer);
            }
            _ => panic!("Expected Input variant"),
        }
    }

    #[test]
    fn test_op_serialization_input_with_mode_follow_up() {
        let op = Op::Input {
            parts: vec![ContentPart::text("after this")],
            mode: InputMode::FollowUp,
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("\"mode\":\"follow_up\""));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Input { parts, mode } => {
                assert_eq!(parts[0].as_text(), Some("after this"));
                assert_eq!(mode, InputMode::FollowUp);
            }
            _ => panic!("Expected Input variant"),
        }
    }

    #[test]
    fn test_input_without_mode_is_rejected() {
        let json = r#"{"type":"input","parts":[{"type":"text","text":"missing mode"}]}"#;
        assert!(serde_json::from_str::<Op>(json).is_err());
    }

    #[test]
    fn test_retired_steer_operation_is_rejected() {
        let json =
            r#"{"type":"steer","parts":[{"type":"text","text":"legacy steer"}],"mode":"steer"}"#;
        assert!(serde_json::from_str::<Op>(json).is_err());
    }

    #[test]
    fn test_op_serialization_resume() {
        let op = Op::Resume {
            request_id: "yield-123".to_string(),
            content: vec![ContentPart::structured(
                serde_json::json!({"choice": "approve"}),
            )],
        };

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("resume"));
        assert!(json.contains("yield-123"));
        assert!(json.contains("\"content\""));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        match deserialized {
            Op::Resume {
                request_id,
                content,
            } => {
                assert_eq!(request_id, "yield-123");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    ContentPart::Structured { data } => assert_eq!(data["choice"], "approve"),
                    _ => panic!("Expected structured resume content"),
                }
            }
            _ => panic!("Expected Resume variant"),
        }
    }

    #[test]
    fn test_op_serialization_interrupt() {
        let op = Op::Interrupt;

        let json = serde_json::to_string(&op).unwrap();
        assert!(json.contains("interrupt"));

        let deserialized: Op = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Op::Interrupt));
    }
}
