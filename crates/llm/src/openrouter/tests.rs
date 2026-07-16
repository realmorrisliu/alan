use super::*;
use serde_json::json;

#[test]
fn projects_messages_tools_and_reasoning_effort() {
    let mut request = GenerationRequest::new()
        .with_system_prompt("system")
        .with_user_message("hello")
        .with_assistant_message("thinking done")
        .with_tool(ToolDefinition::new("lookup", "Lookup data"))
        .with_reasoning_effort(ReasoningEffort::Low);
    request.messages.push(Message::assistant_with_tools(
        "",
        vec![ToolCall::new("lookup", json!({"q":"rust"})).with_id("call-1")],
    ));
    request
        .messages
        .push(Message::tool("call-1", "tool result"));

    let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
    let value = serde_json::to_value(projected).unwrap();

    assert_eq!(value["model"], "openrouter/model");
    assert_eq!(value["messages"][0]["role"], "system");
    assert_eq!(value["messages"][1]["role"], "user");
    assert_eq!(value["messages"][3]["tool_calls"][0]["id"], "call-1");
    assert_eq!(value["messages"][4]["role"], "tool");
    assert_eq!(value["messages"][4]["tool_call_id"], "call-1");
    assert_eq!(value["tools"][0]["function"]["name"], "lookup");
    assert_eq!(value["tool_choice"], "auto");
    assert_eq!(value["reasoning"]["effort"], "low");
}

#[test]
fn request_payload_preserves_assistant_reasoning_fields() {
    let mut request = GenerationRequest::new().with_user_message("hello");
    let mut assistant = Message::assistant("answer");
    assistant.thinking = Some("step by step".to_string());
    assistant.thinking_signature = Some("encrypted_state".to_string());
    request.messages.push(assistant);

    let value = build_openrouter_chat_request_payload("openrouter/model", request).unwrap();

    assert_eq!(value["messages"][1]["role"], "assistant");
    assert_eq!(value["messages"][1]["reasoning_content"], "step by step");
    assert_eq!(
        value["messages"][1]["reasoning"]["encrypted_content"],
        "encrypted_state"
    );
}

#[test]
fn missing_tool_call_id_fails_projection_before_dispatch() {
    let mut request = GenerationRequest::new().with_user_message("hello");
    let mut tool = Message::tool("", "tool result");
    tool.tool_call_id = None;
    request.messages.push(tool);

    let error = build_openrouter_chat_request("openrouter/model", request).unwrap_err();

    assert!(error.to_string().contains("tool_call_id"));
}

#[test]
fn unsupported_extra_parameter_fails_projection() {
    let mut request = GenerationRequest::new().with_user_message("hello");
    request
        .extra_params
        .insert("unsupported".to_string(), json!(true));

    let error = build_openrouter_chat_request("openrouter/model", request).unwrap_err();
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn supported_extra_parameters_are_projected() {
    let mut request = GenerationRequest::new().with_user_message("hello");
    request
        .extra_params
        .insert("route".to_string(), json!("fallback"));
    request.extra_params.insert(
        "provider".to_string(),
        json!({ "allow_fallbacks": false, "require_parameters": true }),
    );
    request
        .extra_params
        .insert("transforms".to_string(), json!(["middle-out"]));
    request
        .extra_params
        .insert("reasoning_effort".to_string(), json!("high"));

    let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
    let value = serde_json::to_value(projected).unwrap();

    assert_eq!(value["route"], "fallback");
    assert_eq!(value["provider"]["allow_fallbacks"], false);
    assert_eq!(value["transforms"][0], "middle-out");
    assert_eq!(value["reasoning"]["effort"], "high");
}

#[test]
fn canonical_effort_overrides_reasoning_effort_extra_parameter() {
    let request = GenerationRequest::new()
        .with_user_message("hello")
        .with_reasoning_effort(ReasoningEffort::Low)
        .with_extra_param("reasoning_effort", json!("high"));

    let projected = build_openrouter_chat_request("openrouter/model", request).unwrap();
    let value = serde_json::to_value(projected).unwrap();

    assert_eq!(value["reasoning"]["effort"], "low");
    assert!(value["reasoning"].get("max_tokens").is_none());
}

#[test]
fn maps_content_reasoning_usage_finish_and_response_id() {
    let response = response_with_choice(json!({
        "finish_reason": "stop",
        "native_finish_reason": null,
        "message": {
            "content": "answer",
            "role": "assistant",
            "reasoning": "because"
        },
        "error": null,
        "index": 0,
        "logprobs": null
    }));

    let converted = convert_openrouter_response(response);
    assert_eq!(converted.content, "answer");
    assert_eq!(converted.thinking.as_deref(), Some("because"));
    assert_eq!(converted.finish_reason.as_deref(), Some("stop"));
    assert_eq!(converted.provider_response_id.as_deref(), Some("resp-1"));
    assert_eq!(converted.usage.unwrap().total_tokens, 7);
}

#[test]
fn maps_tool_call_and_drops_malformed_arguments() {
    let response = response_with_choice(json!({
        "finish_reason": "tool_calls",
        "native_finish_reason": null,
        "message": {
            "content": "",
            "role": "assistant",
            "tool_calls": [
                {
                    "id": "call-ok",
                    "type": "function",
                    "function": {
                        "name": "lookup",
                        "arguments": "{\"q\":\"rust\"}"
                    },
                    "index": 0
                },
                {
                    "id": "call-bad",
                    "type": "function",
                    "function": {
                        "name": "broken",
                        "arguments": "{bad"
                    },
                    "index": 1
                }
            ]
        },
        "error": null,
        "index": 0,
        "logprobs": null
    }));

    let converted = convert_openrouter_response(response);
    assert_eq!(converted.tool_calls.len(), 1);
    assert_eq!(converted.tool_calls[0].id.as_deref(), Some("call-ok"));
    assert_eq!(converted.tool_calls[0].arguments["q"], "rust");
    assert_eq!(converted.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(converted.warnings.len(), 1);
    assert!(converted.warnings[0].contains("broken"));
}

#[test]
fn maps_reasoning_detail_signature() {
    let response = response_with_choice(json!({
        "finish_reason": "stop",
        "native_finish_reason": null,
        "message": {
            "content": "answer",
            "role": "assistant",
            "reasoning_details": [
                {
                    "type": "reasoning.text",
                    "text": "detail",
                    "signature": "sig"
                }
            ]
        },
        "error": null,
        "index": 0,
        "logprobs": null
    }));

    let converted = convert_openrouter_response(response);
    assert_eq!(converted.thinking.as_deref(), Some("detail"));
    assert_eq!(converted.thinking_signature.as_deref(), Some("sig"));
}

#[test]
fn maps_stream_tool_delta() {
    let delta = tool_delta_from_value(json!({
        "index": 2,
        "id": "call-1",
        "type": "function",
        "function": {
            "name": "lookup",
            "arguments": "{\"q\""
        }
    }))
    .unwrap();

    assert_eq!(delta.index, 2);
    assert_eq!(delta.id.as_deref(), Some("call-1"));
    assert_eq!(delta.name.as_deref(), Some("lookup"));
    assert_eq!(delta.arguments_delta.as_deref(), Some("{\"q\""));
}

#[tokio::test]
async fn maps_stream_text_reasoning_completion_and_errors() {
    let events = futures::stream::iter(vec![
        UnifiedStreamEvent::ContentDelta("hel".to_string()),
        UnifiedStreamEvent::ReasoningDelta("why".to_string()),
        UnifiedStreamEvent::ToolDelta(json!({
            "index": 0,
            "id": "call-1",
            "type": "function",
            "function": { "name": "lookup", "arguments": "{}" }
        })),
        UnifiedStreamEvent::Done {
            source: openrouter_rs::types::UnifiedStreamSource::Chat,
            id: Some("resp-stream".to_string()),
            model: Some("model".to_string()),
            finish_reason: Some("tool_calls".to_string()),
            usage: Some(json!({
                "prompt_tokens": 2,
                "completion_tokens": 3,
                "total_tokens": 5
            })),
        },
    ])
    .boxed();
    let (tx, mut rx) = mpsc::channel(10);
    consume_openrouter_stream(events, tx).await;

    assert_eq!(rx.recv().await.unwrap().text.as_deref(), Some("hel"));
    assert_eq!(rx.recv().await.unwrap().thinking.as_deref(), Some("why"));
    assert_eq!(
        rx.recv()
            .await
            .unwrap()
            .tool_call_delta
            .unwrap()
            .name
            .as_deref(),
        Some("lookup")
    );
    let done = rx.recv().await.unwrap();
    assert!(done.is_finished);
    assert_eq!(done.provider_response_id.as_deref(), Some("resp-stream"));
    assert_eq!(done.finish_reason.as_deref(), Some("tool_calls"));
    assert_eq!(done.usage.unwrap().total_tokens, 5);
}

#[tokio::test]
async fn stream_error_after_partial_output_emits_terminal_error_chunk() {
    let events = futures::stream::iter(vec![
        UnifiedStreamEvent::ContentDelta("partial".to_string()),
        UnifiedStreamEvent::Error(openrouter_rs::error::OpenRouterError::Unknown(
            "boom".to_string(),
        )),
    ])
    .boxed();
    let (tx, mut rx) = mpsc::channel(10);
    consume_openrouter_stream(events, tx).await;

    assert_eq!(rx.recv().await.unwrap().text.as_deref(), Some("partial"));
    let done = rx.recv().await.unwrap();
    assert!(done.is_finished);
    assert_eq!(done.finish_reason.as_deref(), Some("stream_error"));
}

fn response_with_choice(choice: Value) -> CompletionsResponse {
    serde_json::from_value(json!({
        "id": "resp-1",
        "choices": [choice],
        "created": 1,
        "model": "openrouter/model",
        "object": "chat.completion",
        "provider": "openrouter",
        "system_fingerprint": null,
        "usage": {
            "prompt_tokens": 3,
            "completion_tokens": 4,
            "total_tokens": 7
        }
    }))
    .unwrap()
}
