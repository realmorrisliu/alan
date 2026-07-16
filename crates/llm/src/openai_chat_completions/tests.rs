//! White-box tests for the OpenAI-compatible adapter.

use super::*;

#[test]
fn test_openai_client_with_params() {
    let client = OpenAiChatCompletionsClient::official_with_params(
        "test-key",
        "https://api.openai.com/v1",
        "gpt-5.4",
    );
    assert_eq!(client.provider_name(), "openai_chat_completions");
    drop(client);
}

#[test]
fn test_openai_chat_completions_compatible_client_with_params() {
    let client = OpenAiChatCompletionsClient::compatible_with_params(
        "test-key",
        "https://proxy.example/v1",
        "qwen3.5-plus",
    );
    assert_eq!(client.provider_name(), "openai_chat_completions_compatible");
    drop(client);
}

#[test]
fn test_chat_message_system() {
    let msg = OpenAiChatCompletionsMessage::system("You are a helpful assistant");
    assert_eq!(msg.role, "system");
    assert_eq!(msg.content, Some("You are a helpful assistant".to_string()));
}

#[test]
fn test_chat_message_user() {
    let msg = OpenAiChatCompletionsMessage::user("Hello!");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content, Some("Hello!".to_string()));
}

#[test]
fn test_chat_message_assistant() {
    let msg = OpenAiChatCompletionsMessage::assistant("Hi there!");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content, Some("Hi there!".to_string()));
}

#[test]
fn test_chat_message_tool() {
    let msg = OpenAiChatCompletionsMessage::tool("call-123", "Tool result");
    assert_eq!(msg.role, "tool");
    assert_eq!(msg.content, Some("Tool result".to_string()));
    assert_eq!(msg.tool_call_id, Some("call-123".to_string()));
}

#[test]
fn test_tool_definition_new() {
    let params = serde_json::json!({
        "type": "object",
        "properties": {
            "query": {"type": "string"}
        },
        "required": ["query"]
    });

    let tool =
        OpenAiChatCompletionsToolDefinition::new("web_search", "Search the web", params.clone());
    assert_eq!(tool.r#type, "function");
    assert_eq!(tool.function.name, "web_search");
    assert_eq!(tool.function.description, "Search the web");
    assert_eq!(tool.function.parameters, params);
}

#[test]
fn test_chat_completion_request_serialization() {
    let request = OpenAiChatCompletionsRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            openai_chat_completions_message_value(
                "system",
                Some(serde_json::Value::String("Be helpful".to_string())),
                None,
                None,
                None,
                None,
            ),
            openai_chat_completions_message_value(
                "user",
                Some(serde_json::Value::String("Hello".to_string())),
                None,
                None,
                None,
                None,
            ),
        ],
        tools: None,
        tool_choice: None,
        temperature: Some(0.7),
        max_completion_tokens: Some(100),
        reasoning_effort: None,
        stream: Some(false),
        stream_options: None,
        extra_params: HashMap::new(),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("model"));
    assert!(json.contains("gpt-4"));
    assert!(json.contains("messages"));
    assert!(json.contains("temperature"));
}

#[test]
fn test_chat_completion_request_with_tools() {
    let tool = OpenAiChatCompletionsToolDefinition::new(
        "search",
        "Search",
        serde_json::json!({"type": "object"}),
    );

    let request = OpenAiChatCompletionsRequest {
        model: "gpt-4".to_string(),
        messages: vec![openai_chat_completions_message_value(
            "user",
            Some(serde_json::Value::String(
                "Search for something".to_string(),
            )),
            None,
            None,
            None,
            None,
        )],
        tools: Some(vec![tool]),
        tool_choice: Some("auto".to_string()),
        temperature: None,
        max_completion_tokens: None,
        reasoning_effort: None,
        stream: None,
        stream_options: None,
        extra_params: HashMap::new(),
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("tools"));
    assert!(json.contains("tool_choice"));
    assert!(json.contains("auto"));
}

#[test]
fn test_chat_completion_response_deserialization() {
    let json = r#"{
        "id": "chatcmpl-123",
        "object": "chat.completion",
        "created": 1677652288,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you?"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 20,
            "total_tokens": 30
        }
    }"#;

    let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "chatcmpl-123");
    assert_eq!(response.model, "gpt-4");
    assert_eq!(response.choices.len(), 1);
    assert_eq!(response.usage.as_ref().unwrap().total_tokens, 30);
}

#[test]
fn test_responses_stream_text_delta_preserves_whitespace_only_chunks() {
    let event = serde_json::json!({
        "type": "response.output_text.delta",
        "delta": " ",
    });
    assert_eq!(responses_stream_text_delta(&event), Some(" "));

    let empty_event = serde_json::json!({
        "type": "response.output_text.delta",
        "delta": "",
    });
    assert_eq!(responses_stream_text_delta(&empty_event), None);
}

#[test]
fn test_chat_completion_response_with_tool_calls() {
    let json = r#"{
        "id": "chatcmpl-456",
        "object": "chat.completion",
        "created": 1677652289,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-123",
                    "type": "function",
                    "function": {
                        "name": "web_search",
                        "arguments": "{\"query\": \"test\"}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }],
        "usage": null
    }"#;

    let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
    let message = &response.choices[0].message;
    assert!(message.content.is_none());
    assert!(message.tool_calls.is_some());
    let tool_calls = message.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls[0].id, "call-123");
    assert_eq!(tool_calls[0].function.name, "web_search");
}

#[test]
fn test_chat_completion_response_with_reasoning_tokens() {
    let json = r#"{
        "id": "chatcmpl-rsn",
        "object": "chat.completion",
        "created": 1677652289,
        "model": "deepseek-reasoner",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Final answer"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 11,
            "completion_tokens": 22,
            "total_tokens": 33,
            "completion_tokens_details": {
                "reasoning_tokens": 7
            }
        }
    }"#;

    let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
    let usage = response.usage.unwrap();
    assert_eq!(
        usage.completion_tokens_details.unwrap().reasoning_tokens,
        Some(7)
    );
}

#[test]
fn test_convert_openai_chat_completions_response_propagates_id_and_finish_reason() {
    let response = OpenAiChatCompletionsResponse {
        id: "chatcmpl-123".to_string(),
        object: "chat.completion".to_string(),
        created: 1,
        model: "gpt-5.4".to_string(),
        choices: vec![OpenAiChatCompletionsChoice {
            index: 0,
            message: OpenAiChatCompletionsMessage::assistant("Hello!"),
            finish_reason: Some("stop".to_string()),
        }],
        usage: None,
    };

    let converted = convert_openai_chat_completions_response(response).unwrap();
    assert_eq!(converted.content, "Hello!");
    assert_eq!(converted.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        converted.provider_response_id.as_deref(),
        Some("chatcmpl-123")
    );
}

#[test]
fn test_chat_completion_response_deserialization_with_reasoning_content() {
    let json = r#"{
        "id": "chatcmpl-rsn-content",
        "object": "chat.completion",
        "created": 1677652290,
        "model": "deepseek-reasoner",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "Final answer",
                "reasoning_content": "Internal reasoning trail"
            },
            "finish_reason": "stop"
        }],
        "usage": null
    }"#;

    let response: OpenAiChatCompletionsResponse = serde_json::from_str(json).unwrap();
    let message = &response.choices[0].message;
    assert_eq!(
        message.reasoning_content.as_deref(),
        Some("Internal reasoning trail")
    );
}

#[test]
fn test_chat_completion_chunk_deserialization() {
    let json = r#"{
        "id": "chatcmpl-789",
        "object": "chat.completion.chunk",
        "created": 1677652290,
        "model": "gpt-4",
        "choices": [{
            "index": 0,
            "delta": {
                "role": "assistant",
                "content": "Hello"
            },
            "finish_reason": null
        }]
    }"#;

    let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
    assert_eq!(chunk.id, "chatcmpl-789");
    assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
}

#[test]
fn test_chat_completion_chunk_deserialization_with_reasoning_content() {
    let json = r#"{
        "id": "chatcmpl-791",
        "object": "chat.completion.chunk",
        "created": 1677652292,
        "model": "deepseek-reasoner",
        "choices": [{
            "index": 0,
            "delta": {
                "reasoning_content": "Thinking..."
            },
            "finish_reason": null
        }]
    }"#;

    let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
    assert_eq!(
        chunk.choices[0].delta.reasoning_content.as_deref(),
        Some("Thinking...")
    );
}

#[test]
fn test_chat_completion_chunk_deserialization_with_usage() {
    let json = r#"{
        "id": "chatcmpl-790",
        "object": "chat.completion.chunk",
        "created": 1677652291,
        "model": "deepseek-reasoner",
        "choices": [],
        "usage": {
            "prompt_tokens": 1,
            "completion_tokens": 2,
            "total_tokens": 3,
            "completion_tokens_details": {
                "reasoning_tokens": 1
            }
        }
    }"#;

    let chunk: OpenAiChatCompletionsChunk = serde_json::from_str(json).unwrap();
    assert_eq!(chunk.choices.len(), 0);
    assert_eq!(
        chunk
            .usage
            .and_then(|u| u.completion_tokens_details)
            .and_then(|d| d.reasoning_tokens),
        Some(1)
    );
}

#[test]
fn test_usage_deserialization() {
    let json = r#"{
        "prompt_tokens": 100,
        "completion_tokens": 50,
        "total_tokens": 150
    }"#;

    let usage: OpenAiChatCompletionsUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.prompt_tokens, 100);
    assert_eq!(usage.completion_tokens, 50);
    assert_eq!(usage.total_tokens, 150);
}

#[test]
fn test_function_definition() {
    let func = OpenAiChatCompletionsFunctionDefinition {
        name: "test_func".to_string(),
        description: "Test function".to_string(),
        parameters: serde_json::json!({"type": "object"}),
    };

    assert_eq!(func.name, "test_func");
    assert_eq!(func.description, "Test function");
}

#[test]
fn test_function_call() {
    let fc = OpenAiChatCompletionsFunctionCall {
        name: "my_func".to_string(),
        arguments: "{\"arg\": 123}".to_string(),
    };

    assert_eq!(fc.name, "my_func");
    assert_eq!(fc.arguments, "{\"arg\": 123}");
}

#[test]
fn test_delta_message_default() {
    let delta: OpenAiChatCompletionsDeltaMessage = Default::default();
    assert!(delta.role.is_none());
    assert!(delta.content.is_none());
    assert!(delta.reasoning_content.is_none());
    assert!(delta.reasoning.is_none());
    assert!(delta.tool_calls.is_none());
}

#[test]
fn test_build_reasoning_effort_prefers_canonical_effort() {
    let mut extra_params = HashMap::from([(
        "reasoning_effort".to_string(),
        serde_json::Value::String("high".to_string()),
    )]);

    let effort = build_reasoning_effort(Some(ReasoningEffort::Low), &mut extra_params);
    assert_eq!(effort.as_deref(), Some("low"));
    assert!(!extra_params.contains_key("reasoning_effort"));
}

#[test]
fn test_build_reasoning_effort_accepts_compat_extra_params() {
    let mut extra_params = HashMap::from([(
        "reasoning_effort".to_string(),
        serde_json::Value::String("high".to_string()),
    )]);

    let effort = build_reasoning_effort(None, &mut extra_params);
    assert_eq!(effort.as_deref(), Some("high"));
    assert!(!extra_params.contains_key("reasoning_effort"));
}

#[test]
fn test_build_reasoning_effort_accepts_extended_values() {
    let mut extra_params = HashMap::from([(
        "reasoning_effort".to_string(),
        serde_json::Value::String("xhigh".to_string()),
    )]);
    let effort = build_reasoning_effort(None, &mut extra_params);
    assert_eq!(effort.as_deref(), Some("xhigh"));
    assert!(!extra_params.contains_key("reasoning_effort"));
}

#[test]
fn test_build_responses_request_uses_canonical_reasoning_effort() {
    let request = GenerationRequest::new()
        .with_user_message("Hello")
        .with_extra_param("reasoning_effort", serde_json::json!("high"))
        .with_reasoning_effort(ReasoningEffort::Low);

    let built = build_responses_request_for_model("gpt-5.4".to_string(), request, false).unwrap();

    assert_eq!(
        built.reasoning.map(|reasoning| reasoning.effort),
        Some("low".to_string())
    );
    assert!(!built.extra_params.contains_key("reasoning_effort"));
}

#[test]
fn test_build_chat_completions_request_uses_canonical_reasoning_effort() {
    let request = GenerationRequest::new()
        .with_user_message("Hello")
        .with_extra_param("reasoning_effort", serde_json::json!("high"))
        .with_reasoning_effort(ReasoningEffort::Low);

    let built = build_chat_completions_request_for_model(
        "gpt-5.4".to_string(),
        "developer",
        request,
        false,
    )
    .unwrap();

    assert_eq!(built.reasoning_effort.as_deref(), Some("low"));
    assert!(!built.extra_params.contains_key("reasoning_effort"));
}

#[test]
fn test_instruction_role_name_differs_by_api_flavor() {
    let official = OpenAiChatCompletionsClient::official_with_params(
        "sk-test",
        "https://api.openai.com/v1",
        "gpt-5.4",
    );
    let compatible = OpenAiChatCompletionsClient::compatible_with_params(
        "sk-test",
        "https://proxy.example/v1",
        "qwen3.5-plus",
    );

    assert_eq!(official.instruction_role_name(), "developer");
    assert_eq!(compatible.instruction_role_name(), "system");
}

#[test]
fn test_convert_messages_for_openai_preserves_assistant_reasoning_content() {
    let messages = vec![crate::Message {
        role: MessageRole::Assistant,
        content: "Done".to_string(),
        content_parts: Vec::new(),
        thinking: Some("step by step".to_string()),
        thinking_signature: Some("encrypted_state".to_string()),
        redacted_thinking: None,
        tool_calls: None,
        tool_call_id: None,
    }];

    let converted = convert_messages_for_openai_chat_completions(messages);
    assert_eq!(converted.len(), 1);
    assert_eq!(
        converted[0]
            .get("reasoning_content")
            .and_then(serde_json::Value::as_str),
        Some("step by step")
    );
    assert_eq!(
        converted[0]
            .get("reasoning")
            .and_then(|value| value.get("encrypted_content"))
            .and_then(serde_json::Value::as_str),
        Some("encrypted_state")
    );
}

#[test]
fn test_build_max_completion_tokens_prefers_extra_params() {
    let mut extra_params = HashMap::from([(
        "max_completion_tokens".to_string(),
        serde_json::Value::Number(serde_json::Number::from(1234)),
    )]);

    let max_tokens = build_max_completion_tokens(Some(100), &mut extra_params);
    assert_eq!(max_tokens, Some(1234));
    assert!(!extra_params.contains_key("max_completion_tokens"));
}

#[test]
fn test_convert_usage_extracts_reasoning_tokens() {
    let usage = OpenAiChatCompletionsUsage {
        prompt_tokens: 10,
        prompt_tokens_details: None,
        completion_tokens: 20,
        total_tokens: 30,
        completion_tokens_details: Some(OpenAiChatCompletionsCompletionTokensDetails {
            reasoning_tokens: Some(7),
            audio_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }),
    };

    let token_usage = convert_usage(usage);
    assert_eq!(token_usage.prompt_tokens, 10);
    assert_eq!(token_usage.completion_tokens, 20);
    assert_eq!(token_usage.total_tokens, 30);
    assert_eq!(token_usage.reasoning_tokens, Some(7));
}

#[test]
fn test_convert_usage_extracts_cached_prompt_tokens() {
    let usage = OpenAiChatCompletionsUsage {
        prompt_tokens: 10,
        prompt_tokens_details: Some(OpenAiChatCompletionsPromptTokensDetails {
            cached_tokens: Some(6),
            audio_tokens: None,
        }),
        completion_tokens: 20,
        total_tokens: 30,
        completion_tokens_details: None,
    };

    let token_usage = convert_usage(usage);
    assert_eq!(token_usage.cached_prompt_tokens, Some(6));
}

#[test]
fn test_allocate_stream_tool_index_is_stable_per_choice_and_tool_index() {
    let mut tool_index_map = HashMap::new();
    let mut next_tool_index = 0usize;

    let first = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 0, 0);
    let first_repeat = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 0, 0);
    let second = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 1, 0);
    let third = allocate_stream_tool_index(&mut tool_index_map, &mut next_tool_index, 1, 1);

    assert_eq!(first, first_repeat);
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(next_tool_index, 3);
}

#[test]
fn test_select_stream_choice_index_prefers_zero_then_falls_back() {
    let choices_non_zero = vec![
        OpenAiChatCompletionsChunkChoice {
            index: 2,
            delta: OpenAiChatCompletionsDeltaMessage::default(),
            finish_reason: None,
        },
        OpenAiChatCompletionsChunkChoice {
            index: 3,
            delta: OpenAiChatCompletionsDeltaMessage::default(),
            finish_reason: None,
        },
    ];
    assert_eq!(
        select_stream_choice_index(None, false, &choices_non_zero),
        Some(2)
    );

    let choices_zero = vec![
        OpenAiChatCompletionsChunkChoice {
            index: 5,
            delta: OpenAiChatCompletionsDeltaMessage::default(),
            finish_reason: None,
        },
        OpenAiChatCompletionsChunkChoice {
            index: 0,
            delta: OpenAiChatCompletionsDeltaMessage::default(),
            finish_reason: None,
        },
    ];
    assert_eq!(
        select_stream_choice_index(None, false, &choices_zero),
        Some(0)
    );

    // If no payload has been emitted yet, switch to index=0 when it appears.
    assert_eq!(
        select_stream_choice_index(Some(2), false, &choices_zero),
        Some(0)
    );
    // Once payload has been emitted, keep stable selection to avoid mixed output.
    assert_eq!(
        select_stream_choice_index(Some(2), true, &choices_zero),
        Some(2)
    );
}

#[test]
fn test_select_primary_choice_prefers_index_zero() {
    let choices = vec![
        OpenAiChatCompletionsChoice {
            index: 1,
            message: OpenAiChatCompletionsMessage::assistant("secondary"),
            finish_reason: Some("stop".to_string()),
        },
        OpenAiChatCompletionsChoice {
            index: 0,
            message: OpenAiChatCompletionsMessage::assistant("primary"),
            finish_reason: Some("stop".to_string()),
        },
    ];
    let selected = select_primary_choice(&choices).expect("expected choice");
    assert_eq!(selected.index, 0);
    assert_eq!(selected.message.content.as_deref(), Some("primary"));
}

#[test]
fn test_stream_tool_call_deserialization() {
    let json = r#"{
        "index": 0,
        "id": "call-123",
        "type": "function",
        "function": {
            "name": "web_search",
            "arguments": "{\"query\": \"test\"}"
        }
    }"#;

    let call: OpenAiChatCompletionsStreamToolCall = serde_json::from_str(json).unwrap();
    assert_eq!(call.index, 0);
    assert_eq!(call.id, Some("call-123".to_string()));
    assert_eq!(call.r#type, Some("function".to_string()));
    assert!(call.function.is_some());
}

#[test]
fn test_stream_function_call_deserialization() {
    let json = r#"{
        "name": "my_func",
        "arguments": "{\"key\": \"value\"}"
    }"#;

    let func: OpenAiChatCompletionsStreamFunctionCall = serde_json::from_str(json).unwrap();
    assert_eq!(func.name, Some("my_func".to_string()));
    assert_eq!(func.arguments, Some("{\"key\": \"value\"}".to_string()));
}
