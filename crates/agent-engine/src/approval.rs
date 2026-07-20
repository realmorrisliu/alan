//! Pending interactive request types and checkpoint taxonomy.

use alan_agent_protocol::{AdaptivePresentationHint, ConfirmationYieldPayload};

use crate::skills::ActiveSkillEnvelope;

pub const TOOL_ESCALATION_CHECKPOINT_TYPE: &str = "tool_escalation";
pub const TOOL_ESCALATION_CHECKPOINT_PREFIX: &str = "tool_escalation_";
pub const TOOL_ESCALATION_CONTROL_KIND: &str = "tool_escalation_confirmation";

pub const EFFECT_REPLAY_CHECKPOINT_TYPE: &str = "effect_replay_confirmation";
pub const EFFECT_REPLAY_CHECKPOINT_PREFIX: &str = "effect_replay_";
pub const EFFECT_REPLAY_CONTROL_KIND: &str = "effect_replay_confirmation";

pub const RUNTIME_CONFIRMATION_CONTROL_SOURCE: &str = "runtime/submission_handlers";
pub const RUNTIME_CONFIRMATION_CONTROL_VERSION: u64 = 1;

pub fn runtime_confirmation_control_kind(checkpoint_type: &str) -> Option<&'static str> {
    match checkpoint_type {
        TOOL_ESCALATION_CHECKPOINT_TYPE => Some(TOOL_ESCALATION_CONTROL_KIND),
        EFFECT_REPLAY_CHECKPOINT_TYPE => Some(EFFECT_REPLAY_CONTROL_KIND),
        _ => None,
    }
}

pub fn runtime_confirmation_checkpoint_prefix(checkpoint_type: &str) -> Option<&'static str> {
    match checkpoint_type {
        TOOL_ESCALATION_CHECKPOINT_TYPE => Some(TOOL_ESCALATION_CHECKPOINT_PREFIX),
        EFFECT_REPLAY_CHECKPOINT_TYPE => Some(EFFECT_REPLAY_CHECKPOINT_PREFIX),
        _ => None,
    }
}

pub fn is_runtime_confirmation_checkpoint_type(checkpoint_type: &str) -> bool {
    runtime_confirmation_control_kind(checkpoint_type).is_some()
}

pub fn replays_tool_calls(checkpoint_type: &str) -> bool {
    matches!(
        checkpoint_type,
        TOOL_ESCALATION_CHECKPOINT_TYPE | EFFECT_REPLAY_CHECKPOINT_TYPE
    )
}

pub fn is_effect_replay_confirmation(checkpoint_type: &str) -> bool {
    checkpoint_type == EFFECT_REPLAY_CHECKPOINT_TYPE
}

#[derive(Debug, Clone)]
pub struct PendingStructuredInputRequest {
    pub request_id: String,
    pub title: String,
    pub prompt: String,
    pub questions: Vec<alan_agent_protocol::StructuredInputQuestion>,
}

#[derive(Debug, Clone)]
pub struct PendingConfirmation {
    pub checkpoint_id: String,
    pub checkpoint_type: String,
    pub summary: String,
    pub details: serde_json::Value,
    pub options: Vec<String>,
}

/// Builds the protocol payload for a runtime-owned confirmation checkpoint.
pub fn runtime_confirmation_yield_payload(
    pending: &PendingConfirmation,
) -> ConfirmationYieldPayload {
    let default_option = pending
        .options
        .iter()
        .find(|option| option.as_str() == "approve")
        .cloned()
        .or_else(|| pending.options.first().cloned());

    ConfirmationYieldPayload {
        checkpoint_type: pending.checkpoint_type.clone(),
        summary: pending.summary.clone(),
        details: Some(pending.details.clone()),
        options: pending.options.clone(),
        default_option,
        presentation_hints: if is_runtime_confirmation_checkpoint_type(&pending.checkpoint_type) {
            vec![AdaptivePresentationHint::Dangerous]
        } else {
            vec![]
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillPermissionHintContext {
    pub skill_id: String,
    pub activation_reason: String,
    pub permission_hints: Vec<String>,
}

pub fn active_skill_permission_hints(
    active_skills: &[ActiveSkillEnvelope],
) -> Vec<SkillPermissionHintContext> {
    active_skills
        .iter()
        .filter(|skill| !skill.metadata.alan_metadata.permission_hints.is_empty())
        .map(|skill| SkillPermissionHintContext {
            skill_id: skill.metadata.id.clone(),
            activation_reason: skill.activation_reason.render_label(),
            permission_hints: skill.metadata.alan_metadata.permission_hints.clone(),
        })
        .collect()
}

pub fn append_skill_permission_hints(
    details: serde_json::Value,
    active_skills: &[ActiveSkillEnvelope],
) -> serde_json::Value {
    let permission_hints = active_skill_permission_hints(active_skills);
    if permission_hints.is_empty() {
        return details;
    }

    let mut object = match details {
        serde_json::Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            if !other.is_null() {
                map.insert("value".to_string(), other);
            }
            map
        }
    };
    object.insert(
        "skill_permission_hints".to_string(),
        serde_json::to_value(permission_hints).unwrap_or_else(|_| serde_json::json!([])),
    );
    serde_json::Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_confirmation_checkpoint_type_identification() {
        assert!(is_runtime_confirmation_checkpoint_type(
            TOOL_ESCALATION_CHECKPOINT_TYPE
        ));
        assert!(is_runtime_confirmation_checkpoint_type(
            EFFECT_REPLAY_CHECKPOINT_TYPE
        ));
        assert!(!is_runtime_confirmation_checkpoint_type("review"));
    }

    #[test]
    fn test_runtime_confirmation_control_metadata_lookup() {
        assert_eq!(
            runtime_confirmation_control_kind(TOOL_ESCALATION_CHECKPOINT_TYPE),
            Some(TOOL_ESCALATION_CONTROL_KIND)
        );
        assert_eq!(
            runtime_confirmation_checkpoint_prefix(EFFECT_REPLAY_CHECKPOINT_TYPE),
            Some(EFFECT_REPLAY_CHECKPOINT_PREFIX)
        );
    }

    #[test]
    fn test_effect_replay_confirmation_identification() {
        assert!(is_effect_replay_confirmation(EFFECT_REPLAY_CHECKPOINT_TYPE));
        assert!(!is_effect_replay_confirmation(
            TOOL_ESCALATION_CHECKPOINT_TYPE
        ));
        assert!(replays_tool_calls(TOOL_ESCALATION_CHECKPOINT_TYPE));
        assert!(replays_tool_calls(EFFECT_REPLAY_CHECKPOINT_TYPE));
        assert!(!replays_tool_calls("review"));
    }

    #[test]
    fn test_pending_structured_input_request_creation() {
        let request = PendingStructuredInputRequest {
            request_id: "req-123".to_string(),
            title: "Test Title".to_string(),
            prompt: "Test Prompt".to_string(),
            questions: vec![alan_agent_protocol::StructuredInputQuestion {
                id: "q1".to_string(),
                label: "Question 1".to_string(),
                prompt: "What is your name?".to_string(),
                kind: alan_agent_protocol::StructuredInputKind::Text,
                required: true,
                placeholder: Some("Jane Doe".to_string()),
                help_text: None,
                default_value: None,
                default_values: Vec::new(),
                min_selected: None,
                max_selected: None,
                options: vec![],
                presentation_hints: vec![],
            }],
        };
        assert_eq!(request.request_id, "req-123");
        assert_eq!(request.title, "Test Title");
        assert_eq!(request.questions.len(), 1);
    }

    #[test]
    fn test_pending_confirmation_creation() {
        let pending = PendingConfirmation {
            checkpoint_id: "chk-123".to_string(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Escalate file write?".to_string(),
            details: serde_json::json!({"path": "/test/file.txt"}),
            options: vec!["approve".to_string(), "reject".to_string()],
        };
        assert_eq!(pending.checkpoint_id, "chk-123");
        assert_eq!(pending.checkpoint_type, TOOL_ESCALATION_CHECKPOINT_TYPE);
        assert_eq!(pending.options.len(), 2);
    }

    #[test]
    fn runtime_confirmation_yield_payload_is_dangerous_and_defaults_to_approve() {
        let pending = PendingConfirmation {
            checkpoint_id: "tool_escalation_call-1".to_string(),
            checkpoint_type: TOOL_ESCALATION_CHECKPOINT_TYPE.to_string(),
            summary: "Approve Tool execution".to_string(),
            details: serde_json::json!({"tool": "bash"}),
            options: vec!["approve".to_string(), "reject".to_string()],
        };

        let payload = runtime_confirmation_yield_payload(&pending);

        assert_eq!(payload.default_option.as_deref(), Some("approve"));
        assert_eq!(
            payload.presentation_hints,
            vec![AdaptivePresentationHint::Dangerous]
        );
    }

    #[test]
    fn test_append_skill_permission_hints() {
        let skill = ActiveSkillEnvelope::available(
            crate::skills::SkillMetadata {
                id: "deploy".to_string(),
                package_id: Some("skill:deploy".to_string()),
                name: "Deploy".to_string(),
                description: "Deploy service".to_string(),
                short_description: None,
                path: std::path::PathBuf::from("/tmp/deploy/SKILL.md"),
                package_root: None,
                resource_root: None,
                scope: crate::skills::SkillScope::Descriptor,
                tags: vec![],
                capabilities: None,
                compatibility: Default::default(),
                source: crate::skills::SkillContentSource::File(std::path::PathBuf::from(
                    "/tmp/deploy/SKILL.md",
                )),
                enabled: true,
                allow_implicit_invocation: true,
                alan_metadata: crate::skills::AlanSkillRuntimeMetadata {
                    permission_hints: vec!["may require network approval".to_string()],
                    execution: Default::default(),
                    allow_implicit_invocation: None,
                },
                compatible_metadata: Default::default(),
                execution: Default::default(),
            },
            crate::skills::SkillActivationReason::ExplicitMention {
                mention: "deploy".to_string(),
            },
        );

        let details =
            append_skill_permission_hints(serde_json::json!({"kind": "tool_escalation"}), &[skill]);
        let hints = details
            .get("skill_permission_hints")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap();

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0]["skill_id"], "deploy");
        assert_eq!(hints[0]["activation_reason"], "explicit_mention($deploy)");
        assert_eq!(
            hints[0]["permission_hints"][0],
            "may require network approval"
        );
    }
}
