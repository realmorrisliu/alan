use super::*;
use crate::{Message, MessageRole, ToolCall, ToolDefinition};
use axum::{Router, routing::post};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

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
                cache_control: None,
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
        ContentBlockInput::Text { text, .. } => assert_eq!(text, "Hello"),
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
        cache_control: None,
    };

    match block {
        ContentBlockInput::Text { text, .. } => assert_eq!(text, "Hello"),
        _ => panic!("Expected Text variant"),
    }
}

#[test]
fn test_content_block_input_tool_use() {
    let block = ContentBlockInput::ToolUse {
        id: "call-123".to_string(),
        name: "my_tool".to_string(),
        input: serde_json::json!({"arg": "value"}),
        cache_control: None,
    };

    match block {
        ContentBlockInput::ToolUse {
            id, name, input, ..
        } => {
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
        cache_control: None,
    };

    match block {
        ContentBlockInput::ToolResult {
            tool_use_id,
            content,
            is_error,
            ..
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
            cache_control: None,
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
        ContentBlockInput::Text { text, .. } => assert_eq!(text, "Hello from assistant"),
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
        ContentBlockInput::ToolUse {
            id, name, input, ..
        } => {
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
        ContentBlockInput::Text { text, .. } => assert_eq!(text, "tool output"),
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
        ContentBlockInput::Text { text, .. } => assert_eq!(text, "{\"ok\":true}"),
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
            cache_control: None,
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
    assert_eq!(token_usage.cache_creation_tokens, Some(20));
    assert_eq!(token_usage.completion_tokens, 50);
    assert_eq!(token_usage.total_tokens, 200);
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
        ContentBlockInput::Text { text, .. } => {
            assert_eq!(text, "done");
        }
        _ => panic!("Expected Text block"),
    }
}

#[test]
fn rejects_unqualified_extended_reasoning_efforts() {
    for effort in [ReasoningEffort::Max, ReasoningEffort::Ultra] {
        assert!(anthropic_budget_for_effort(effort).is_err());
    }
}

#[test]
fn empty_model_defaults_to_claude_sonnet_4_5() {
    let client = AnthropicMessagesClient::with_params("k", "https://api.anthropic.com/v1", "");
    assert_eq!(client.model, DEFAULT_ANTHROPIC_MESSAGES_MODEL);
    assert_eq!(DEFAULT_ANTHROPIC_MESSAGES_MODEL, "claude-sonnet-4-5");
}

fn count_cache_controls(value: &Value) -> usize {
    match value {
        Value::Object(map) => {
            usize::from(map.contains_key("cache_control"))
                + map.values().map(count_cache_controls).sum::<usize>()
        }
        Value::Array(items) => items.iter().map(count_cache_controls).sum(),
        _ => 0,
    }
}

#[tokio::test]
async fn generate_emits_prompt_cache_breakpoints_and_parses_cache_usage() {
    let captured = Arc::new(Mutex::new(None::<Value>));
    let captured_handler = Arc::clone(&captured);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());

    let app = Router::new().route(
        "/v1/messages",
        post(move |axum::Json(body): axum::Json<Value>| {
            let captured_handler = Arc::clone(&captured_handler);
            async move {
                *captured_handler.lock().unwrap() = Some(body);
                axum::Json(json!({
                    "id": "msg_cache_1",
                    "type": "message",
                    "content": [{"type": "text", "text": "cached reply"}],
                    "stop_reason": "end_turn",
                    "usage": {
                        "input_tokens": 40,
                        "cache_creation_input_tokens": 120,
                        "cache_read_input_tokens": 800,
                        "output_tokens": 25
                    }
                }))
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut client = AnthropicMessagesClient::with_params("sk-ant-test", &url, "");
    let mut request = GenerationRequest::new()
        .with_system_prompt("You are Alan.")
        .with_tool(ToolDefinition::new("read_file", "Read a file"))
        .with_tool(ToolDefinition::new("write_file", "Write a file"));
    request.messages.push(Message::user("first turn"));
    request.messages.push(Message::assistant("first answer"));
    request.messages.push(Message::user("second turn"));
    request.messages.push(Message::assistant_with_tools(
        "",
        vec![ToolCall::new("read_file", json!({"path": "a.rs"})).with_id("toolu_1")],
    ));
    request
        .messages
        .push(Message::tool("toolu_1", "fn main() {}"));

    let response = client.generate(request).await.unwrap();
    server.abort();

    let sent = captured.lock().unwrap().clone().unwrap();
    let ephemeral = json!({"type": "ephemeral"});
    assert_eq!(sent["model"], "claude-sonnet-4-5");
    assert_eq!(count_cache_controls(&sent), 4);
    assert_eq!(
        sent["system"],
        json!([{"type": "text", "text": "You are Alan.", "cache_control": {"type": "ephemeral"}}])
    );
    assert!(sent["tools"][0].get("cache_control").is_none());
    assert_eq!(sent["tools"][1]["cache_control"], ephemeral);
    assert!(
        sent["messages"][0]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert_eq!(
        sent["messages"][2]["content"][0]["cache_control"],
        ephemeral
    );
    assert_eq!(
        sent["messages"][4]["content"][0]["cache_control"],
        ephemeral
    );

    assert_eq!(response.content, "cached reply");
    assert_eq!(
        response.usage,
        Some(TokenUsage {
            prompt_tokens: 960,
            cached_prompt_tokens: Some(800),
            completion_tokens: 25,
            total_tokens: 985,
            reasoning_tokens: None,
            cache_creation_tokens: Some(120),
            cost_usd: None,
        })
    );
}

#[tokio::test]
async fn generate_stream_emits_prompt_cache_breakpoints_and_parses_cache_usage() {
    let captured = Arc::new(Mutex::new(None::<Value>));
    let captured_handler = Arc::clone(&captured);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());

    let app = Router::new().route(
        "/v1/messages",
        post(move |axum::Json(body): axum::Json<Value>| {
            let captured_handler = Arc::clone(&captured_handler);
            async move {
                *captured_handler.lock().unwrap() = Some(body);
                let sse = concat!(
                    "event: message_start\n",
                    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_s1\",\"usage\":{\"input_tokens\":50,\"cache_creation_input_tokens\":90,\"cache_read_input_tokens\":600,\"output_tokens\":0}}}\n\n",
                    "event: content_block_delta\n",
                    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\n",
                    "event: message_delta\n",
                    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":15}}\n\n",
                    "event: message_stop\n",
                    "data: {\"type\":\"message_stop\"}\n\n"
                );
                (
                    [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                    sse,
                )
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let mut client = AnthropicMessagesClient::with_params("sk-ant-test", &url, "claude-sonnet-4-5");
    let request = GenerationRequest::new()
        .with_system_prompt("System")
        .with_user_message("Hello");
    let mut rx = client.generate_stream(request).await.unwrap();

    let mut chunks = Vec::new();
    while let Some(chunk) = rx.recv().await {
        chunks.push(chunk);
    }
    server.abort();

    let sent = captured.lock().unwrap().clone().unwrap();
    assert_eq!(sent["stream"], json!(true));
    assert_eq!(count_cache_controls(&sent), 2);
    assert_eq!(
        sent["system"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );
    assert_eq!(
        sent["messages"][0]["content"][0]["cache_control"],
        json!({"type": "ephemeral"})
    );

    let terminal = chunks.iter().find(|chunk| chunk.is_finished).unwrap();
    assert_eq!(
        terminal.usage,
        Some(TokenUsage {
            prompt_tokens: 740,
            cached_prompt_tokens: Some(600),
            completion_tokens: 15,
            total_tokens: 755,
            reasoning_tokens: None,
            cache_creation_tokens: Some(90),
            cost_usd: None,
        })
    );
}
