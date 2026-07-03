//! Response guardrails for assistant outputs.
//!
//! These checks enforce runtime-level invariants before emitting a response.
//! They are intentionally independent from task/domain skills.

use super::agent_loop::RuntimeLoopState;
use crate::tape::{ContentPart, Message, ToolRequest, ToolResponse};
use alan_agent_protocol::ToolCapability;
use serde_json::Value;
use std::collections::HashMap;

const RULE_ID_CAPABILITY_CONTRADICTION: &str = "capability_contradiction";

#[derive(Default)]
struct RecentToolFailureContext {
    has_tool_failure: bool,
    has_network_failure: bool,
}

/// Model draft before output is emitted.
pub(super) struct AssistantDraft<'a> {
    content: &'a str,
    has_tool_calls: bool,
}

impl<'a> AssistantDraft<'a> {
    pub(super) fn new(content: &'a str, has_tool_calls: bool) -> Self {
        Self {
            content,
            has_tool_calls,
        }
    }
}

/// Guardrail evaluation context derived from runtime state.
pub(super) struct ResponseGuardrailContext {
    has_any_tools: bool,
    has_network_capability: bool,
    has_recent_tool_failure: bool,
    has_recent_network_failure: bool,
}

impl ResponseGuardrailContext {
    pub(super) fn from_state(state: &RuntimeLoopState) -> Self {
        let mut has_any_tools = false;
        let mut has_network_capability = false;
        let empty_args = serde_json::json!({});
        let recent_failures = current_turn_tool_failures(state);

        for tool_name in state.static_tool_names() {
            has_any_tools = true;
            if matches!(
                state.static_tool_capability(&tool_name, &empty_args),
                Some(ToolCapability::Network)
            ) {
                has_network_capability = true;
            }
        }

        for tool in state.session.dynamic_tools.values() {
            has_any_tools = true;
            if matches!(tool.capability, Some(ToolCapability::Network)) {
                has_network_capability = true;
            }
        }

        Self {
            has_any_tools,
            has_network_capability,
            has_recent_tool_failure: recent_failures.has_tool_failure,
            has_recent_network_failure: recent_failures.has_network_failure,
        }
    }

    #[cfg(test)]
    fn for_tests(
        has_any_tools: bool,
        has_network_capability: bool,
        has_recent_tool_failure: bool,
        has_recent_network_failure: bool,
    ) -> Self {
        Self {
            has_any_tools,
            has_network_capability,
            has_recent_tool_failure,
            has_recent_network_failure,
        }
    }
}

pub(super) enum GuardrailDecision {
    Accept,
    Recover {
        rule_id: &'static str,
        reason: String,
        instruction: String,
    },
}

/// Runtime guardrails pipeline.
pub(super) struct ResponseGuardrails {
    max_regenerations: usize,
    regeneration_count: usize,
}

impl Default for ResponseGuardrails {
    fn default() -> Self {
        Self {
            max_regenerations: 1,
            regeneration_count: 0,
        }
    }
}

impl ResponseGuardrails {
    pub(super) fn evaluate(
        &mut self,
        context: &ResponseGuardrailContext,
        draft: &AssistantDraft<'_>,
    ) -> GuardrailDecision {
        if self.regeneration_count >= self.max_regenerations
            || draft.has_tool_calls
            || draft.content.trim().is_empty()
        {
            return GuardrailDecision::Accept;
        }

        let normalized = draft.content.to_lowercase();

        if context.has_any_tools
            && !context.has_recent_tool_failure
            && claims_tools_unavailable(&normalized)
        {
            self.regeneration_count += 1;
            return GuardrailDecision::Recover {
                rule_id: RULE_ID_CAPABILITY_CONTRADICTION,
                reason: "Assistant claimed tools are unavailable despite registered tools."
                    .to_string(),
                instruction: "Correction: tools are available in this session. Do not claim tools are unavailable. If a tool is needed, call a relevant tool first. If it fails, report the observed failure.".to_string(),
            };
        }

        if context.has_network_capability
            && !context.has_recent_network_failure
            && claims_network_unavailable(&normalized)
        {
            self.regeneration_count += 1;
            return GuardrailDecision::Recover {
                rule_id: RULE_ID_CAPABILITY_CONTRADICTION,
                reason: "Assistant claimed external/current data access is unavailable despite network-capable tools."
                    .to_string(),
                instruction: "Correction: network-capable tools are available in this session. For requests requiring external or current data, call a relevant tool before stating limitations. If the tool fails, include the actual error.".to_string(),
            };
        }

        GuardrailDecision::Accept
    }
}

fn current_turn_tool_failures(state: &RuntimeLoopState) -> RecentToolFailureContext {
    let messages = state.session.tape.messages();
    let current_turn = active_turn_messages(messages, state.turn_state.active_turn_message_start());
    let tool_capabilities = current_turn_tool_capabilities(state, current_turn);
    let mut failures = RecentToolFailureContext::default();

    for message in current_turn.iter().rev() {
        let Message::Tool { responses } = message else {
            continue;
        };

        for response in responses {
            if !tool_response_has_failure(response) {
                continue;
            }

            failures.has_tool_failure = true;
            if tool_capabilities.get(&response.id).copied() == Some(ToolCapability::Network)
                || tool_response_failure_is_network_related(response)
            {
                failures.has_network_failure = true;
            }
        }

        if failures.has_tool_failure && failures.has_network_failure {
            break;
        }
    }

    failures
}

fn active_turn_messages(messages: &[Message], active_turn_start: Option<usize>) -> &[Message] {
    let turn_start = active_turn_start.unwrap_or(0).min(messages.len());
    &messages[turn_start..]
}

fn current_turn_tool_capabilities(
    state: &RuntimeLoopState,
    messages: &[Message],
) -> HashMap<String, ToolCapability> {
    let mut capabilities = HashMap::new();

    for message in messages {
        let Message::Assistant { tool_requests, .. } = message else {
            continue;
        };

        for request in tool_requests {
            if let Some(capability) = capability_for_tool_request(state, request) {
                capabilities.insert(request.id.clone(), capability);
            }
        }
    }

    capabilities
}

fn capability_for_tool_request(
    state: &RuntimeLoopState,
    request: &ToolRequest,
) -> Option<ToolCapability> {
    state
        .static_tool_capability(&request.name, &request.arguments)
        .or_else(|| {
            state
                .session
                .dynamic_tools
                .get(&request.name)
                .and_then(|tool| tool.capability)
        })
}

fn tool_response_has_failure(response: &ToolResponse) -> bool {
    response.content.iter().any(content_part_has_failure)
}

fn content_part_has_failure(part: &ContentPart) -> bool {
    match part {
        ContentPart::Structured { data } => structured_payload_has_failure(data),
        ContentPart::Text { text } => {
            let normalized = text.to_lowercase();
            normalized.starts_with("error:")
                || normalized.starts_with("failed:")
                || normalized.starts_with("denied:")
                || normalized.starts_with("blocked:")
        }
        _ => false,
    }
}

fn structured_payload_has_failure(data: &Value) -> bool {
    let Some(object) = data.as_object() else {
        return false;
    };

    object.contains_key("error")
        || object.get("success").and_then(Value::as_bool) == Some(false)
        || object
            .get("status")
            .and_then(Value::as_str)
            .is_some_and(status_indicates_failure)
}

fn status_indicates_failure(status: &str) -> bool {
    let normalized = status.to_lowercase();
    ["fail", "blocked", "denied", "error", "timeout"]
        .iter()
        .any(|pattern| normalized.contains(pattern))
}

fn tool_response_failure_is_network_related(response: &ToolResponse) -> bool {
    response
        .content
        .iter()
        .any(content_part_failure_is_network_related)
}

fn content_part_failure_is_network_related(part: &ContentPart) -> bool {
    match part {
        ContentPart::Structured { data } => structured_failure_is_network_related(data),
        ContentPart::Text { text } => {
            content_part_has_failure(part) && failure_text_is_network_related(text)
        }
        _ => false,
    }
}

fn structured_failure_is_network_related(data: &Value) -> bool {
    let Some(object) = data.as_object() else {
        return false;
    };

    ["error", "message", "reason", "status"]
        .iter()
        .filter_map(|field| object.get(*field))
        .any(value_contains_network_indicator)
}

fn value_contains_network_indicator(value: &Value) -> bool {
    match value {
        Value::String(text) => failure_text_is_network_related(text),
        Value::Array(values) => values.iter().any(value_contains_network_indicator),
        Value::Object(object) => object.values().any(value_contains_network_indicator),
        _ => false,
    }
}

fn failure_text_is_network_related(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        "network",
        "internet",
        "403",
        "429",
        "dns",
        "socket",
        "proxy",
        "tls",
        "ssl",
        "connection refused",
        "connection reset",
        "connection aborted",
        "connection timed out",
        "browse",
        "real-time",
        "real time",
        "live data",
        "current data",
    ]
    .iter()
    .any(|pattern| normalized.contains(pattern))
}

fn claims_tools_unavailable(text: &str) -> bool {
    [
        "don't have access to tools",
        "do not have access to tools",
        "can't use tools",
        "cannot use tools",
        "no tools available",
        "without tools",
        "unable to use tools",
        "无法使用工具",
        "没有可用工具",
    ]
    .iter()
    .any(|pattern| text.contains(pattern))
}

fn claims_network_unavailable(text: &str) -> bool {
    [
        "don't have access to real-time",
        "do not have access to real-time",
        "can't access the internet",
        "cannot access the internet",
        "no internet access",
        "can't browse the web",
        "cannot browse the web",
        "don't have web access",
        "do not have web access",
        "can't check current",
        "cannot check current",
        "unable to access live data",
        "无法访问互联网",
        "无法联网",
        "无法获取实时",
    ]
    .iter()
    .any(|pattern| text.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_unavailable_claim_triggers_recovery_when_tools_exist() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, false, false, false);
        let draft = AssistantDraft::new("I don't have access to tools in this environment.", false);

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(
            decision,
            GuardrailDecision::Recover {
                rule_id: RULE_ID_CAPABILITY_CONTRADICTION,
                ..
            }
        ));
    }

    #[test]
    fn network_unavailable_claim_triggers_recovery_when_network_tool_exists() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, false, false);
        let draft = AssistantDraft::new("I can't access the internet right now.", false);

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(
            decision,
            GuardrailDecision::Recover {
                rule_id: RULE_ID_CAPABILITY_CONTRADICTION,
                ..
            }
        ));
    }

    #[test]
    fn no_regeneration_when_claim_not_present() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, false, false);
        let draft = AssistantDraft::new("I'll check this for you.", false);

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(decision, GuardrailDecision::Accept));
    }

    #[test]
    fn no_regeneration_when_tool_call_exists() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, false, false);
        let draft = AssistantDraft::new("I can't access the internet right now.", true);

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(decision, GuardrailDecision::Accept));
    }

    #[test]
    fn recovery_decision_includes_instruction() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, false, false);
        let draft = AssistantDraft::new("I can't access the internet right now.", false);

        let decision = guardrails.evaluate(&context, &draft);
        match decision {
            GuardrailDecision::Recover {
                rule_id,
                instruction,
                ..
            } => {
                assert_eq!(rule_id, RULE_ID_CAPABILITY_CONTRADICTION);
                assert!(!instruction.is_empty());
            }
            _ => panic!("Expected recovery decision"),
        }
    }

    #[test]
    fn max_regeneration_limit_is_enforced() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, false, false);
        let draft = AssistantDraft::new("I can't access the internet right now.", false);

        let first = guardrails.evaluate(&context, &draft);
        let second = guardrails.evaluate(&context, &draft);

        assert!(matches!(first, GuardrailDecision::Recover { .. }));
        assert!(matches!(second, GuardrailDecision::Accept));
    }

    #[test]
    fn tools_unavailable_claim_is_accepted_after_real_tool_failure() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, false, true, false);
        let draft = AssistantDraft::new(
            "I can't use tools right now because that action failed.",
            false,
        );

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(decision, GuardrailDecision::Accept));
    }

    #[test]
    fn network_unavailable_claim_is_accepted_after_real_network_failure() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, true, true);
        let draft = AssistantDraft::new(
            "I can't access the internet right now because the request was blocked.",
            false,
        );

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(decision, GuardrailDecision::Accept));
    }

    #[test]
    fn unrelated_tool_failure_does_not_excuse_network_unavailability_claim() {
        let mut guardrails = ResponseGuardrails::default();
        let context = ResponseGuardrailContext::for_tests(true, true, true, false);
        let draft = AssistantDraft::new("I can't browse the web from this session.", false);

        let decision = guardrails.evaluate(&context, &draft);
        assert!(matches!(
            decision,
            GuardrailDecision::Recover {
                rule_id: RULE_ID_CAPABILITY_CONTRADICTION,
                ..
            }
        ));
    }

    #[test]
    fn generic_timeout_failure_is_not_treated_as_network_related() {
        let response = ToolResponse {
            id: "call_local".to_string(),
            content: vec![ContentPart::structured(serde_json::json!({
                "error": "local command timed out"
            }))],
        };

        assert!(!tool_response_failure_is_network_related(&response));
    }

    #[test]
    fn dns_failure_is_treated_as_network_related() {
        let response = ToolResponse {
            id: "call_network".to_string(),
            content: vec![ContentPart::structured(serde_json::json!({
                "error": "dns lookup failed"
            }))],
        };

        assert!(tool_response_failure_is_network_related(&response));
    }

    #[test]
    fn structured_error_object_counts_as_failure() {
        assert!(structured_payload_has_failure(&serde_json::json!({
            "error": {
                "code": "tool_failed",
                "message": "structured failure"
            }
        })));
    }

    #[test]
    fn nested_error_message_is_treated_as_network_related() {
        let response = ToolResponse {
            id: "call_network".to_string(),
            content: vec![ContentPart::structured(serde_json::json!({
                "error": {
                    "code": "dns_failure",
                    "message": "dns lookup failed"
                }
            }))],
        };

        assert!(tool_response_failure_is_network_related(&response));
    }

    #[test]
    fn active_turn_messages_include_mid_turn_steer_context() {
        let messages = vec![
            Message::user("earlier turn"),
            Message::user("current turn"),
            Message::Assistant {
                parts: Vec::new(),
                tool_requests: vec![ToolRequest {
                    id: "call_1".to_string(),
                    name: "network_probe".to_string(),
                    arguments: serde_json::json!({}),
                }],
            },
            Message::tool_structured("call_1", serde_json::json!({"error": "blocked by policy"})),
            Message::user("steer current turn"),
        ];

        assert_eq!(active_turn_messages(&messages, Some(1)), &messages[1..]);
    }

    #[test]
    fn active_turn_messages_ignore_prior_turn_without_completed_assistant_boundary() {
        let messages = vec![
            Message::user("earlier turn"),
            Message::Assistant {
                parts: Vec::new(),
                tool_requests: vec![ToolRequest {
                    id: "call_1".to_string(),
                    name: "network_probe".to_string(),
                    arguments: serde_json::json!({}),
                }],
            },
            Message::tool_structured("call_1", serde_json::json!({"error": "blocked by policy"})),
            Message::user("current turn"),
        ];

        assert_eq!(active_turn_messages(&messages, Some(3)), &messages[3..]);
    }
}
