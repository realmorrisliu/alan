//! Pure projection from provider-neutral generation requests to OpenAI wire requests.

use anyhow::Result;
use std::collections::HashMap;
use tracing::debug;

use crate::message::reject_retired_message_overrides;
use crate::{
    GenerationRequest, Message as LlmMessage, ReasoningEffort, ToolDefinition as LlmToolDefinition,
};

#[cfg(test)]
use super::OpenAiResponsesInputTokensRequest;
use super::input_projection::{
    convert_messages_for_openai_chat_completions_with_instruction_role,
    convert_messages_for_openai_responses, is_non_empty, openai_chat_completions_message_value,
};
use super::{
    OpenAiChatCompletionsFunctionDefinition, OpenAiChatCompletionsRequest,
    OpenAiChatCompletionsStreamOptions, OpenAiChatCompletionsToolDefinition,
    OpenAiResponsesReasoning, OpenAiResponsesRequest, OpenAiResponsesToolDefinition,
};

#[cfg(test)]
pub(crate) fn convert_messages_for_openai_chat_completions(
    messages: Vec<LlmMessage>,
) -> Vec<serde_json::Value> {
    convert_messages_for_openai_chat_completions_with_instruction_role(messages, "system")
        .expect("test messages should have representable Chat Completions content")
}

pub(crate) fn convert_tools_for_openai_chat_completions(
    tools: Vec<LlmToolDefinition>,
) -> (
    Option<Vec<OpenAiChatCompletionsToolDefinition>>,
    Option<String>,
) {
    if tools.is_empty() {
        (None, None)
    } else {
        (
            Some(
                tools
                    .into_iter()
                    .map(|tool| OpenAiChatCompletionsToolDefinition {
                        r#type: "function".to_string(),
                        function: OpenAiChatCompletionsFunctionDefinition {
                            name: tool.name,
                            description: tool.description,
                            parameters: tool.parameters,
                        },
                    })
                    .collect(),
            ),
            Some("auto".to_string()),
        )
    }
}

pub(crate) fn convert_tools_for_openai_responses(
    tools: Vec<LlmToolDefinition>,
) -> (Option<Vec<OpenAiResponsesToolDefinition>>, Option<String>) {
    if tools.is_empty() {
        (None, None)
    } else {
        (
            Some(
                tools
                    .into_iter()
                    .map(|tool| {
                        OpenAiResponsesToolDefinition::new(
                            &tool.name,
                            &tool.description,
                            tool.parameters,
                        )
                    })
                    .collect(),
            ),
            Some("auto".to_string()),
        )
    }
}

pub(crate) fn normalize_responses_instructions(system_prompt: Option<String>) -> Option<String> {
    system_prompt.filter(|value| is_non_empty(value))
}

pub(crate) fn build_responses_request_for_model(
    model: String,
    request: GenerationRequest,
    stream: bool,
) -> Result<OpenAiResponsesRequest> {
    reject_retired_message_overrides(&request)?;
    let GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature,
        max_tokens,
        reasoning,
        mut extra_params,
    } = request;

    let previous_response_id = take_string_extra_param("previous_response_id", &mut extra_params);
    let background = take_bool_extra_param("background", &mut extra_params);
    let mut store = take_bool_extra_param("store", &mut extra_params);
    if (matches!(background, Some(true)) || previous_response_id.is_some()) && store.is_none() {
        store = Some(true);
    }

    let reasoning = build_openai_responses_reasoning(reasoning.effort, &mut extra_params);
    let include = normalize_responses_include(
        take_string_array_extra_param("include", &mut extra_params),
        should_include_reasoning_encrypted_content(&messages, &tools, reasoning.is_some()),
    );
    let (response_tools, tool_choice) = convert_tools_for_openai_responses(tools);
    let input = convert_messages_for_openai_responses(messages)?;

    Ok(OpenAiResponsesRequest {
        model,
        instructions: normalize_responses_instructions(system_prompt),
        previous_response_id,
        store,
        background,
        include,
        input,
        tools: response_tools,
        tool_choice,
        temperature,
        max_output_tokens: build_max_completion_tokens(max_tokens, &mut extra_params),
        reasoning,
        stream: Some(stream),
        extra_params,
    })
}

#[cfg(test)]
pub(crate) fn build_responses_input_tokens_request_for_model(
    model: String,
    request: GenerationRequest,
) -> Result<OpenAiResponsesInputTokensRequest> {
    reject_retired_message_overrides(&request)?;
    let GenerationRequest {
        system_prompt,
        messages,
        tools,
        temperature: _,
        max_tokens: _,
        reasoning,
        mut extra_params,
    } = request;

    let reasoning = build_openai_responses_reasoning(reasoning.effort, &mut extra_params);
    let (response_tools, tool_choice) = convert_tools_for_openai_responses(tools);
    let input = convert_messages_for_openai_responses(messages)?;

    extra_params.remove("previous_response_id");
    extra_params.remove("background");
    extra_params.remove("store");
    extra_params.remove("include");
    extra_params.remove("context_management");
    extra_params.remove("stream");
    extra_params.remove("max_completion_tokens");

    Ok(OpenAiResponsesInputTokensRequest {
        model,
        instructions: normalize_responses_instructions(system_prompt),
        input,
        tools: response_tools,
        tool_choice,
        reasoning,
        extra_params,
    })
}

pub(super) fn build_chat_completions_request_for_model(
    model: String,
    instruction_role: &'static str,
    request: GenerationRequest,
    stream: bool,
) -> Result<OpenAiChatCompletionsRequest> {
    reject_retired_message_overrides(&request)?;
    let GenerationRequest {
        system_prompt,
        messages: request_messages,
        tools: request_tools,
        temperature,
        max_tokens,
        reasoning,
        mut extra_params,
    } = request;

    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(system) = system_prompt {
        messages.push(openai_chat_completions_message_value(
            instruction_role,
            Some(serde_json::Value::String(system)),
            None,
            None,
            None,
            None,
        ));
    }
    messages.extend(
        convert_messages_for_openai_chat_completions_with_instruction_role(
            request_messages,
            instruction_role,
        )?,
    );

    let (tools, tool_choice) = convert_tools_for_openai_chat_completions(request_tools);
    let reasoning_effort = build_reasoning_effort(reasoning.effort, &mut extra_params);
    let max_completion_tokens = build_max_completion_tokens(max_tokens, &mut extra_params);

    Ok(OpenAiChatCompletionsRequest {
        model,
        messages,
        tools,
        tool_choice,
        temperature,
        max_completion_tokens,
        reasoning_effort,
        stream: Some(stream),
        stream_options: stream.then_some(OpenAiChatCompletionsStreamOptions {
            include_usage: true,
        }),
        extra_params,
    })
}

fn build_openai_responses_reasoning(
    reasoning_effort: Option<ReasoningEffort>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<OpenAiResponsesReasoning> {
    build_reasoning_effort(reasoning_effort, extra_params)
        .map(|effort| OpenAiResponsesReasoning { effort })
}

fn is_valid_reasoning_effort(effort: &str) -> bool {
    matches!(
        effort,
        "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
    )
}

fn take_string_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<String> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::String(value) if is_non_empty(&value) => Some(value),
        other => {
            debug!(key, value = %other, "Ignoring non-string or empty Responses extra_param");
            None
        }
    }
}

fn take_bool_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<bool> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::Bool(value) => Some(value),
        other => {
            debug!(key, value = %other, "Ignoring non-boolean Responses extra_param");
            None
        }
    }
}

fn take_string_array_extra_param(
    key: &str,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<Vec<String>> {
    let value = extra_params.remove(key)?;
    match value {
        serde_json::Value::Array(values) => {
            let collected: Vec<String> = values
                .into_iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .filter(|value| is_non_empty(value))
                .collect();
            if collected.is_empty() {
                None
            } else {
                Some(collected)
            }
        }
        other => {
            debug!(key, value = %other, "Ignoring non-array Responses extra_param");
            None
        }
    }
}

fn should_include_reasoning_encrypted_content(
    messages: &[LlmMessage],
    tools: &[LlmToolDefinition],
    reasoning_requested: bool,
) -> bool {
    reasoning_requested
        || !tools.is_empty()
        || messages.iter().any(|message| {
            message
                .thinking_signature
                .as_deref()
                .is_some_and(is_non_empty)
        })
}

fn normalize_responses_include(
    include: Option<Vec<String>>,
    require_reasoning_encrypted_content: bool,
) -> Option<Vec<String>> {
    let mut include = include.unwrap_or_default();
    if require_reasoning_encrypted_content
        && !include
            .iter()
            .any(|value| value == "reasoning.encrypted_content")
    {
        include.push("reasoning.encrypted_content".to_string());
    }
    if include.is_empty() {
        None
    } else {
        Some(include)
    }
}

pub(crate) fn build_reasoning_effort(
    reasoning_effort: Option<ReasoningEffort>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<String> {
    if let Some(effort) = reasoning_effort {
        extra_params.remove("reasoning_effort");
        return Some(effort.as_str().to_string());
    }

    if let Some(value) = extra_params.remove("reasoning_effort") {
        if let Some(effort) = value.as_str() {
            if is_valid_reasoning_effort(effort) {
                return Some(effort.to_string());
            }
            debug!(
                effort,
                "Ignoring invalid `reasoning_effort`; expected one of: none, minimal, low, medium, high, xhigh"
            );
        } else {
            debug!(
                value = %value,
                "Ignoring non-string `reasoning_effort` in extra_params"
            );
        }
    }

    None
}

pub(crate) fn build_max_completion_tokens(
    max_tokens: Option<i32>,
    extra_params: &mut HashMap<String, serde_json::Value>,
) -> Option<i32> {
    if let Some(value) = extra_params.remove("max_completion_tokens") {
        if let Some(tokens) = value.as_i64() {
            return i32::try_from(tokens).ok();
        }
        debug!(
            value = %value,
            "Ignoring non-integer `max_completion_tokens` in extra_params"
        );
    }
    max_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_builder_owns_instruction_role_and_stream_contract() {
        let request = GenerationRequest::new()
            .with_system_prompt("compatibility instructions")
            .with_user_message("hello");

        let built = build_chat_completions_request_for_model(
            "compatible-model".to_string(),
            "system",
            request,
            true,
        )
        .unwrap();

        assert_eq!(built.messages[0]["role"], "system");
        assert_eq!(built.messages[1]["role"], "user");
        assert_eq!(built.stream, Some(true));
        assert!(
            built
                .stream_options
                .is_some_and(|options| options.include_usage)
        );
    }
}
