use super::*;
use crate::MessageRole;

#[test]
fn anthropic_messages_url_appends_v1_when_missing() {
    let client = AnthropicMessagesClient::with_params("k", "https://api.kimi.com/coding", "k2p5");
    assert_eq!(
        client.anthropic_messages_url(),
        "https://api.kimi.com/coding/v1/messages"
    );
}

#[test]
fn anthropic_messages_url_preserves_existing_v1() {
    let client =
        AnthropicMessagesClient::with_params("k", "https://api.anthropic.com/v1", "claude");
    assert_eq!(
        client.anthropic_messages_url(),
        "https://api.anthropic.com/v1/messages"
    );
}

#[test]
fn test_anthropic_messages_client_with_params() {
    let client = AnthropicMessagesClient::with_params(
        "test-key",
        "https://api.anthropic.com/v1",
        "claude-3-opus",
    );
    // Just verify client creation works
    drop(client);
}

#[test]
fn test_message_request_serialization() {
    let request = AnthropicMessagesRequest {
        model: "claude-3-opus".to_string(),
        messages: vec![AnthropicMessagesMessage {
            role: "user".to_string(),
            content: vec![ContentBlockInput::Text {
                text: "Hello".to_string(),
            }],
        }],
        max_tokens: 1024,
        system: None,
        temperature: Some(0.7),
        tools: None,
        stream: None,
        thinking: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("claude-3-opus"));
    assert!(json.contains("messages"));
    assert!(json.contains("max_tokens"));
}

#[test]
fn test_message_response_deserialization() {
    let json = r#"{
            "id": "msg_123",
            "type": "message",
            "content": [
                {"type": "text", "text": "Hello!"}
            ],
            "model": "claude-3-opus",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        }"#;

    let response: AnthropicMessagesResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.id, "msg_123");
    assert_eq!(response.content.len(), 1);
    assert_eq!(response.usage.as_ref().unwrap().input_tokens, 10);
}

#[test]
fn test_convert_anthropic_response_propagates_id_and_finish_reason() {
    let response = AnthropicMessagesResponse {
        id: "msg_123".to_string(),
        content: vec![
            ContentBlock {
                block_type: "thinking".to_string(),
                text: None,
                thinking: Some("step".to_string()),
                signature: Some("sig_123".to_string()),
                data: None,
                id: None,
                name: None,
                input: None,
            },
            ContentBlock {
                block_type: "text".to_string(),
                text: Some("Done".to_string()),
                thinking: None,
                signature: None,
                data: None,
                id: None,
                name: None,
                input: None,
            },
        ],
        usage: None,
        stop_reason: Some("end_turn".to_string()),
    };

    let converted = convert_anthropic_response(response);
    assert_eq!(converted.content, "Done");
    assert_eq!(converted.thinking.as_deref(), Some("step"));
    assert_eq!(converted.thinking_signature.as_deref(), Some("sig_123"));
    assert_eq!(converted.finish_reason.as_deref(), Some("end_turn"));
    assert_eq!(converted.provider_response_id.as_deref(), Some("msg_123"));
    assert_eq!(
        converted.provider_response_status.as_deref(),
        Some("end_turn")
    );
}

#[test]
fn test_content_block() {
    let block = ContentBlock {
        block_type: "text".to_string(),
        text: Some("Hello".to_string()),
        thinking: None,
        signature: None,
        data: None,
        id: None,
        name: None,
        input: None,
    };

    assert_eq!(block.block_type, "text");
    assert_eq!(block.text, Some("Hello".to_string()));
}

#[test]
fn test_message_user_text() {
    let msg = AnthropicMessagesMessage::user_text("Hello");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content.len(), 1);
    match &msg.content[0] {
        ContentBlockInput::Text { text } => assert_eq!(text, "Hello"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_message_user_tool_result() {
    let msg =
        AnthropicMessagesMessage::user_tool_result("tool-call-123", "Tool output".to_string());
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content.len(), 1);
    match &msg.content[0] {
        ContentBlockInput::ToolResult {
            tool_use_id,
            content,
            ..
        } => {
            assert_eq!(tool_use_id, "tool-call-123");
            assert_eq!(content, "Tool output");
        }
        _ => panic!("Expected ToolResult variant"),
    }
}

#[test]
fn test_tool_definition_new() {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "query": {"type": "string"}
        }
    });

    let tool = AnthropicMessagesToolDefinition::new("search", "Search tool", schema.clone());
    assert_eq!(tool.name, "search");
    assert_eq!(tool.description, "Search tool");
    assert_eq!(tool.input_schema, schema);
}

#[test]
fn test_content_block_input_text() {
    let block = ContentBlockInput::Text {
        text: "Hello".to_string(),
    };

    match block {
        ContentBlockInput::Text { text } => assert_eq!(text, "Hello"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_content_block_input_tool_use() {
    let block = ContentBlockInput::ToolUse {
        id: "call-123".to_string(),
        name: "my_tool".to_string(),
        input: serde_json::json!({"arg": "value"}),
    };

    match block {
        ContentBlockInput::ToolUse { id, name, input } => {
            assert_eq!(id, "call-123");
            assert_eq!(name, "my_tool");
            assert_eq!(input["arg"], "value");
        }
        _ => panic!("Expected ToolUse variant"),
    }
}

#[test]
fn test_content_block_input_tool_result() {
    let block = ContentBlockInput::ToolResult {
        tool_use_id: "call-456".to_string(),
        content: "Result".to_string(),
        is_error: Some(false),
    };

    match block {
        ContentBlockInput::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => {
            assert_eq!(tool_use_id, "call-456");
            assert_eq!(content, "Result");
            assert_eq!(is_error, Some(false));
        }
        _ => panic!("Expected ToolResult variant"),
    }
}

#[test]
fn test_stream_event_deserialization() {
    let json = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello"
            }
        }"#;

    let event: StreamEvent = serde_json::from_str(json).unwrap();
    assert_eq!(event.event_type, "content_block_delta");
    assert_eq!(event.index, Some(0));
    assert!(event.delta.is_some());
}

#[test]
fn test_message_serialization() {
    let msg = AnthropicMessagesMessage {
        role: "assistant".to_string(),
        content: vec![ContentBlockInput::Text {
            text: "Hi!".to_string(),
        }],
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("assistant"));
    assert!(json.contains("Hi!"));
}

#[test]
fn test_usage() {
    let usage = Usage {
        input_tokens: 100,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        output_tokens: 50,
    };

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
}

#[test]
fn test_stream_delta() {
    let delta = StreamDelta {
        delta_type: Some("text_delta".to_string()),
        text: Some("Hello".to_string()),
        thinking: None,
        signature: None,
        partial_json: None,
        stop_reason: None,
    };

    assert_eq!(delta.delta_type, Some("text_delta".to_string()));
    assert_eq!(delta.text, Some("Hello".to_string()));
}

#[test]
fn test_stream_message() {
    let msg = StreamMessage {
        id: Some("msg_123".to_string()),
        stop_reason: Some("end_turn".to_string()),
        usage: None,
    };
    assert_eq!(msg.id.as_deref(), Some("msg_123"));
    assert_eq!(msg.stop_reason, Some("end_turn".to_string()));
}

#[test]
fn test_stream_error() {
    let err = StreamError {
        error: Some(serde_json::json!({"type": "error"})),
        message: Some("Something went wrong".to_string()),
        r#type: Some("api_error".to_string()),
    };
    assert_eq!(err.message, Some("Something went wrong".to_string()));
}

#[test]
fn test_message_assistant() {
    let msg = AnthropicMessagesMessage::assistant_text("Hello from assistant");
    assert_eq!(msg.role, "assistant");
    match &msg.content[0] {
        ContentBlockInput::Text { text } => assert_eq!(text, "Hello from assistant"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_convert_messages_for_anthropic_keeps_assistant_tool_use() {
    let assistant = crate::Message::assistant_with_tools(
        "",
        vec![
            crate::ToolCall::new("web_search", serde_json::json!({"query": "laptop"}))
                .with_id("toolu_123"),
        ],
    );
    let tool = crate::Message::tool("toolu_123", "{\"ok\":true}");

    let (converted, _) =
        convert_messages_for_anthropic_messages(vec![assistant, tool], None).unwrap();
    assert_eq!(converted.len(), 2);

    assert_eq!(converted[0].role, "assistant");
    assert_eq!(converted[0].content.len(), 1);
    match &converted[0].content[0] {
        ContentBlockInput::ToolUse { id, name, input } => {
            assert_eq!(id, "toolu_123");
            assert_eq!(name, "web_search");
            assert_eq!(input["query"], "laptop");
        }
        _ => panic!("Expected ToolUse variant"),
    }

    assert_eq!(converted[1].role, "user");
    assert_eq!(converted[1].content.len(), 1);
    match &converted[1].content[0] {
        ContentBlockInput::ToolResult { tool_use_id, .. } => {
            assert_eq!(tool_use_id, "toolu_123");
        }
        _ => panic!("Expected ToolResult variant"),
    }
}

#[test]
fn test_convert_messages_for_anthropic_empty_tool_call_id_falls_back_to_text() {
    let tool_msg = crate::Message {
        role: MessageRole::Tool,
        content: "tool output".to_string(),
        content_parts: Vec::new(),
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        tool_calls: None,
        tool_call_id: Some("   ".to_string()),
    };

    let (converted, _) = convert_messages_for_anthropic_messages(vec![tool_msg], None).unwrap();
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role, "user");
    assert_eq!(converted[0].content.len(), 1);
    match &converted[0].content[0] {
        ContentBlockInput::Text { text } => assert_eq!(text, "tool output"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_convert_messages_for_anthropic_unknown_tool_call_id_falls_back_to_text() {
    let assistant = crate::Message::assistant_with_tools(
        "",
        vec![
            crate::ToolCall::new("web_search", serde_json::json!({"q": "rust"}))
                .with_id("toolu_known"),
        ],
    );
    let unmatched_tool_result = crate::Message::tool("toolu_unknown", "{\"ok\":true}");

    let (converted, _) =
        convert_messages_for_anthropic_messages(vec![assistant, unmatched_tool_result], None)
            .unwrap();
    assert_eq!(converted.len(), 2);
    assert_eq!(converted[1].role, "user");
    assert_eq!(converted[1].content.len(), 1);
    match &converted[1].content[0] {
        ContentBlockInput::Text { text } => assert_eq!(text, "{\"ok\":true}"),
        _ => panic!("Expected Text fallback for unknown tool_use_id"),
    }
}

#[test]
fn test_build_thinking_params_omits_thinking_when_effort_unset() {
    let (thinking, temperature, max_tokens) = build_thinking_params(None, Some(0.7), 2048).unwrap();
    assert!(thinking.is_none());
    assert_eq!(temperature, Some(0.7));
    assert_eq!(max_tokens, 2048);
}

#[test]
fn test_build_thinking_params_adjusts_max_tokens_and_temperature() {
    let (thinking, temperature, max_tokens) =
        build_thinking_params(Some(ReasoningEffort::Low), Some(0.2), 1000).unwrap();

    assert!(thinking.is_some());
    assert_eq!(temperature, Some(1.0));
    assert_eq!(max_tokens, 1025);
}

#[test]
fn test_build_thinking_params_maps_reasoning_effort_to_budget() {
    let (thinking, temperature, max_tokens) =
        build_thinking_params(Some(ReasoningEffort::High), Some(0.2), 4096).unwrap();

    assert_eq!(thinking.unwrap().budget_tokens, 8192);
    assert_eq!(temperature, Some(1.0));
    assert_eq!(max_tokens, 8193);
}

#[test]
fn test_build_request_headers_supports_beta_and_interleaved() {
    let mut extra_params = HashMap::from([
        (
            "anthropic_beta".to_string(),
            serde_json::json!(["tools-2024-05-16"]),
        ),
        ("interleaved_thinking".to_string(), serde_json::json!(true)),
    ]);

    let headers = build_request_headers(&[], &mut extra_params).unwrap();
    assert!(extra_params.is_empty());
    let value = headers.get("anthropic-beta").unwrap().to_str().unwrap();
    assert!(value.contains("tools-2024-05-16"));
    assert!(value.contains(INTERLEAVED_THINKING_BETA));
}

#[test]
fn test_build_request_headers_adds_files_beta_for_file_sources() {
    let messages = vec![AnthropicMessagesMessage {
        role: "user".to_string(),
        content: vec![ContentBlockInput::Document {
            source: serde_json::json!({
                "type": "file",
                "file_id": "file_123"
            }),
            title: Some("Spec".to_string()),
            citations: None,
        }],
    }];
    let mut extra_params = HashMap::new();

    let headers = build_request_headers(&messages, &mut extra_params).unwrap();

    let value = headers.get("anthropic-beta").unwrap().to_str().unwrap();
    assert!(value.contains(FILES_API_BETA));
}

#[test]
fn test_convert_usage_extracts_cached_prompt_tokens() {
    let usage = Usage {
        input_tokens: 100,
        cache_creation_input_tokens: Some(20),
        cache_read_input_tokens: Some(30),
        output_tokens: 50,
    };

    let token_usage = convert_usage(usage);
    assert_eq!(token_usage.prompt_tokens, 150);
    assert_eq!(token_usage.cached_prompt_tokens, Some(30));
}

#[test]
fn test_convert_messages_for_anthropic_preserves_thinking_signature_and_redacted() {
    let message = crate::Message {
        role: MessageRole::Assistant,
        content: "done".to_string(),
        content_parts: Vec::new(),
        thinking: Some("step by step".to_string()),
        thinking_signature: Some("sig_123".to_string()),
        redacted_thinking: Some(vec!["ciphertext".to_string()]),
        tool_calls: None,
        tool_call_id: None,
    };

    let (converted, _) = convert_messages_for_anthropic_messages(vec![message], None).unwrap();
    assert_eq!(converted.len(), 1);
    assert_eq!(converted[0].role, "assistant");
    assert_eq!(converted[0].content.len(), 3);

    match &converted[0].content[0] {
        ContentBlockInput::Thinking {
            thinking,
            signature,
        } => {
            assert_eq!(thinking, "step by step");
            assert_eq!(signature.as_deref(), Some("sig_123"));
        }
        _ => panic!("Expected Thinking block"),
    }
    match &converted[0].content[1] {
        ContentBlockInput::RedactedThinking { data } => {
            assert_eq!(data, "ciphertext");
        }
        _ => panic!("Expected RedactedThinking block"),
    }
    match &converted[0].content[2] {
        ContentBlockInput::Text { text } => {
            assert_eq!(text, "done");
        }
        _ => panic!("Expected Text block"),
    }
}
