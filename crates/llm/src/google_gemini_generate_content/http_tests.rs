//! End-to-end axum mock tests for Vertex AI HTTP transport, dual auth,
//! `thoughtSignature` multi-turn round-trips, SSE streaming, multimodal
//! `inlineData`, and usage parsing.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::{
    GoogleGeminiGenerateContentClient, VertexAccessTokenSource, VertexAuth, VertexClientConfig,
};
use crate::{
    GenerationRequest, LlmProvider, Message, MessageContentPart, ReasoningEffort, ToolDefinition,
};

struct SequenceTokenSource {
    calls: AtomicUsize,
}

impl SequenceTokenSource {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl VertexAccessTokenSource for SequenceTokenSource {
    async fn access_token(&self) -> anyhow::Result<String> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("adc-token-{n}"))
    }
}

#[derive(Clone, Default)]
struct RecordedRequests {
    uris: Arc<Mutex<Vec<String>>>,
    auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    api_key_headers: Arc<Mutex<Vec<Option<String>>>>,
    bodies: Arc<Mutex<Vec<Value>>>,
}

impl RecordedRequests {
    fn record(&self, uri: &OriginalUri, headers: &HeaderMap, body: Value) {
        self.uris.lock().unwrap().push(uri.0.to_string());
        self.auth_headers.lock().unwrap().push(
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string),
        );
        self.api_key_headers.lock().unwrap().push(
            headers
                .get("x-goog-api-key")
                .and_then(|v| v.to_str().ok())
                .map(ToString::to_string),
        );
        self.bodies.lock().unwrap().push(body);
    }
}

type MockHandler =
    Arc<dyn Fn(usize, String, HeaderMap, Value) -> axum::response::Response + Send + Sync>;

#[derive(Clone)]
struct AppState {
    recorded: RecordedRequests,
    counter: Arc<AtomicUsize>,
    handler: MockHandler,
}

async fn handle_mock_post(
    State(state): State<AppState>,
    uri: OriginalUri,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> axum::response::Response {
    let index = state.counter.fetch_add(1, Ordering::SeqCst);
    state.recorded.record(&uri, &headers, body.clone());
    (state.handler)(index, uri.0.to_string(), headers, body)
}

async fn spawn_mock_server<F>(handler: F) -> (String, RecordedRequests, tokio::task::JoinHandle<()>)
where
    F: Fn(usize, String, HeaderMap, Value) -> axum::response::Response + Send + Sync + 'static,
{
    let recorded = RecordedRequests::default();
    let state = AppState {
        recorded: recorded.clone(),
        counter: Arc::new(AtomicUsize::new(0)),
        handler: Arc::new(handler),
    };

    let app = Router::new()
        .route("/{*path}", post(handle_mock_post))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), recorded, handle)
}

#[tokio::test]
async fn express_mode_uses_api_key_header_and_projectless_path() {
    let (base_url, recorded, handle) = spawn_mock_server(|_, _, _, _| {
        Json(json!({
            "responseId": "resp-express",
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "express ok"}]},
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 12,
                "cachedContentTokenCount": 4,
                "candidatesTokenCount": 8,
                "thoughtsTokenCount": 3,
                "totalTokenCount": 23
            }
        }))
        .into_response()
    })
    .await;

    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-flash".to_string(),
        auth: VertexAuth::ApiKey("express-secret-key".to_string()),
        base_url: Some(base_url),
    })
    .unwrap();

    let response = client
        .generate(GenerationRequest::new().with_user_message("hi"))
        .await
        .unwrap();

    assert_eq!(response.content, "express ok");
    assert_eq!(
        response.provider_response_id.as_deref(),
        Some("resp-express")
    );
    let usage = response.usage.unwrap();
    assert_eq!(usage.prompt_tokens, 12);
    assert_eq!(usage.cached_prompt_tokens, Some(4));
    assert_eq!(usage.reasoning_tokens, Some(3));
    assert_eq!(usage.completion_tokens, 11);
    assert_eq!(usage.total_tokens, 23);

    let uris = recorded.uris.lock().unwrap();
    assert_eq!(
        uris.as_slice(),
        ["/v1/publishers/google/models/gemini-2.5-flash:generateContent"]
    );
    assert_eq!(
        recorded.api_key_headers.lock().unwrap().as_slice(),
        [Some("express-secret-key".to_string())]
    );
    assert_eq!(recorded.auth_headers.lock().unwrap().as_slice(), [None]);
    handle.abort();
}

#[tokio::test]
async fn adc_caches_token_and_retries_once_after_401() {
    let (base_url, recorded, handle) = spawn_mock_server(|idx, _, headers, _| {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if idx == 1 && auth == "Bearer adc-token-1" {
            return (StatusCode::UNAUTHORIZED, Json(json!({"error": "expired"}))).into_response();
        }
        Json(json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": format!("ok-{idx}")}]},
                "finishReason": "STOP"
            }]
        }))
        .into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("my-proj".to_string()),
            location: "global".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source.clone(),
    )
    .unwrap();

    // First call fetches adc-token-1.
    let first = client
        .generate(GenerationRequest::new().with_user_message("first"))
        .await
        .unwrap();
    assert_eq!(first.content, "ok-0");
    assert_eq!(token_source.call_count(), 1);

    // Second call reuses cached adc-token-1, gets 401, refreshes once to adc-token-2, succeeds.
    let second = client
        .generate(GenerationRequest::new().with_user_message("second"))
        .await
        .unwrap();
    assert_eq!(second.content, "ok-2");
    assert_eq!(token_source.call_count(), 2);

    // Third call reuses cached adc-token-2 without fetching again.
    let third = client
        .generate(GenerationRequest::new().with_user_message("third"))
        .await
        .unwrap();
    assert_eq!(third.content, "ok-3");
    assert_eq!(token_source.call_count(), 2);

    assert_eq!(
        recorded.auth_headers.lock().unwrap().as_slice(),
        [
            Some("Bearer adc-token-1".to_string()),
            Some("Bearer adc-token-1".to_string()),
            Some("Bearer adc-token-2".to_string()),
            Some("Bearer adc-token-2".to_string()),
        ]
    );
    handle.abort();
}

#[tokio::test]
async fn adc_401_does_not_retry_more_than_once() {
    let (base_url, recorded, handle) = spawn_mock_server(|_, _, _, _| {
        (StatusCode::UNAUTHORIZED, Json(json!({"error": "denied"}))).into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("my-proj".to_string()),
            location: "us-central1".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source.clone(),
    )
    .unwrap();

    let err = client
        .generate(GenerationRequest::new().with_user_message("hi"))
        .await
        .unwrap_err();
    assert_eq!(
        crate::safe_failure_reason(&err),
        "stream_error:authentication"
    );
    assert_eq!(token_source.call_count(), 2);
    assert_eq!(recorded.uris.lock().unwrap().len(), 2);
    handle.abort();
}

#[tokio::test]
async fn thought_signature_round_trips_across_two_turn_function_call() {
    let (base_url, recorded, handle) = spawn_mock_server(|idx, _, _, body| {
        if idx == 0 {
            Json(json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [
                            {"text": "Need to read Cargo.toml", "thought": true},
                            {
                                "functionCall": {
                                    "id": "call_1",
                                    "name": "read_file",
                                    "args": {"path": "Cargo.toml"}
                                },
                                "thoughtSignature": "opaque-sig-turn1-call0"
                            }
                        ]
                    },
                    "finishReason": "STOP"
                }]
            }))
            .into_response()
        } else {
            // Simulate Gemini 3 strict validation: 400 if thoughtSignature is missing.
            let model_parts = &body["contents"][1]["parts"];
            let has_sig = model_parts.as_array().unwrap().iter().any(|p| {
                p["functionCall"]["name"] == "read_file"
                    && p["thoughtSignature"] == "opaque-sig-turn1-call0"
            });
            if !has_sig {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "Function call is missing a thought_signature"})),
                )
                    .into_response();
            }
            Json(json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [
                            {"text": "Got file contents", "thought": true},
                            {"text": "The crate is alan-llm."}
                        ]
                    },
                    "finishReason": "STOP"
                }]
            }))
            .into_response()
        }
    })
    .await;

    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "global".to_string(),
            model: "gemini-3-pro-preview".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        Arc::new(SequenceTokenSource::new()),
    )
    .unwrap();

    let tool = ToolDefinition::new("read_file", "Read a file").with_string_param("path", "Path");
    let turn1_req = GenerationRequest::new()
        .with_user_message("What crate is this?")
        .with_tool(tool.clone())
        .with_reasoning_effort(ReasoningEffort::High);
    let turn1 = client.generate(turn1_req).await.unwrap();

    assert_eq!(turn1.content, "");
    assert_eq!(turn1.thinking.as_deref(), Some("Need to read Cargo.toml"));
    assert!(turn1.thinking_signature.is_some());
    assert_eq!(turn1.tool_calls.len(), 1);
    assert_eq!(turn1.tool_calls[0].name, "read_file");

    // Turn 2: feed back assistant message (with thinking + thinking_signature + tool_calls) and tool response.
    let mut assistant = Message::assistant_with_tools(turn1.content, turn1.tool_calls);
    assistant.thinking = turn1.thinking;
    assistant.thinking_signature = turn1.thinking_signature;
    let tool_msg = Message::tool("call_1", r#"{"name":"alan-llm"}"#);

    let turn2_req = GenerationRequest {
        messages: vec![Message::user("What crate is this?"), assistant, tool_msg],
        tools: vec![tool],
        ..GenerationRequest::new().with_reasoning_effort(ReasoningEffort::High)
    };
    let turn2 = client.generate(turn2_req).await.unwrap();
    assert_eq!(turn2.content, "The crate is alan-llm.");
    assert_eq!(turn2.thinking.as_deref(), Some("Got file contents"));

    let bodies = recorded.bodies.lock().unwrap();
    let fn_resp = &bodies[1]["contents"][2]["parts"][0]["functionResponse"];
    assert_eq!(fn_resp["name"], "read_file");
    assert_eq!(fn_resp["id"], "call_1");
    assert_eq!(fn_resp["response"]["name"], "alan-llm");
    handle.abort();
}

#[tokio::test]
async fn sse_streaming_emits_thinking_partial_tool_calls_signature_and_usage() {
    let (base_url, recorded, handle) = spawn_mock_server(|_, _, _, _| {
        let events = [
            json!({
                "responseId": "resp-sse-1",
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Plan step 1. ", "thought": true}]
                    }
                }]
            }),
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [
                            {"text": "Plan step 2.", "thought": true},
                            {
                                "functionCall": {
                                    "id": "call_stream_1",
                                    "name": "write_file",
                                    "willContinue": true
                                },
                                "thoughtSignature": "sig-stream-call-0"
                            }
                        ]
                    }
                }]
            }),
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [{
                            "functionCall": {
                                "partialArgs": [
                                    {"jsonPath": "$.path", "stringValue": "hello.txt"},
                                    {"jsonPath": "$.body", "stringValue": "hi ", "willContinue": true}
                                ],
                                "willContinue": true
                            }
                        }]
                    }
                }]
            }),
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [{
                            "functionCall": {
                                "partialArgs": [
                                    {"jsonPath": "$.body", "stringValue": "vertex"}
                                ],
                                "willContinue": false
                            }
                        }]
                    }
                }]
            }),
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {
                        "role": "model",
                        "parts": [{"text": "Done!"}]
                    },
                    "finishReason": "STOP"
                }],
                "usageMetadata": {
                    "promptTokenCount": 50,
                    "cachedContentTokenCount": 20,
                    "candidatesTokenCount": 10,
                    "thoughtsTokenCount": 30,
                    "totalTokenCount": 90
                }
            }),
        ];
        let body = events
            .iter()
            .map(|e| format!("data: {e}\n\n"))
            .collect::<String>();
        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            body,
        )
            .into_response()
    })
    .await;

    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "global".to_string(),
            model: "gemini-3-flash-preview".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        Arc::new(SequenceTokenSource::new()),
    )
    .unwrap();

    let tool = ToolDefinition::new("write_file", "Write a file")
        .with_string_param("path", "File path")
        .with_string_param("body", "File body");
    let req = GenerationRequest::new()
        .with_user_message("write hello")
        .with_tool(tool)
        .with_reasoning_effort(ReasoningEffort::Medium);

    let mut rx = client.generate_stream(req).await.unwrap();
    let mut thinking = String::new();
    let mut text = String::new();
    let mut call_name = None;
    let mut call_id = None;
    let mut call_deltas = Vec::new();
    let mut final_args = None;
    let mut thinking_signature = None;
    let mut usage = None;
    let mut finish_reason = None;
    let mut response_id = None;

    while let Some(chunk) = rx.recv().await {
        if let Some(t) = chunk.thinking {
            thinking.push_str(&t);
        }
        if let Some(t) = chunk.text {
            text.push_str(&t);
        }
        if let Some(delta) = chunk.tool_call_delta {
            assert_eq!(delta.index, 0);
            if let Some(id) = delta.id {
                call_id = Some(id);
            }
            if let Some(name) = delta.name {
                call_name = Some(name);
            }
            if let Some(d) = delta.arguments_delta {
                call_deltas.push(d);
            }
            if let Some(args) = delta.arguments {
                final_args = Some(args);
            }
        }
        if let Some(sig) = chunk.thinking_signature {
            thinking_signature = Some(sig);
        }
        if let Some(u) = chunk.usage {
            usage = Some(u);
        }
        if chunk.is_finished {
            finish_reason = chunk.finish_reason;
            response_id = chunk.provider_response_id;
        }
    }

    assert_eq!(thinking, "Plan step 1. Plan step 2.");
    assert_eq!(text, "Done!");
    assert_eq!(call_name.as_deref(), Some("write_file"));
    assert_eq!(call_id.as_deref(), Some("call_stream_1"));
    assert!(!call_deltas.is_empty());
    let parsed_args: Value = serde_json::from_str(final_args.as_deref().unwrap()).unwrap();
    assert_eq!(
        parsed_args,
        json!({"path": "hello.txt", "body": "hi vertex"})
    );
    assert!(thinking_signature.is_some());
    assert_eq!(finish_reason.as_deref(), Some("STOP"));
    assert_eq!(response_id.as_deref(), Some("resp-sse-1"));

    let usage = usage.unwrap();
    assert_eq!(usage.prompt_tokens, 50);
    assert_eq!(usage.cached_prompt_tokens, Some(20));
    assert_eq!(usage.reasoning_tokens, Some(30));
    assert_eq!(usage.completion_tokens, 40);
    assert_eq!(usage.total_tokens, 90);

    // Verify request hit :streamGenerateContent?alt=sse with includeThoughts + streamFunctionCallArguments.
    assert_eq!(
        recorded.uris.lock().unwrap().as_slice(),
        [
            "/v1/projects/proj/locations/global/publishers/google/models/gemini-3-flash-preview:streamGenerateContent?alt=sse"
        ]
    );
    let body = &recorded.bodies.lock().unwrap()[0];
    assert_eq!(
        body["generationConfig"]["thinkingConfig"]["includeThoughts"],
        true
    );
    assert_eq!(
        body["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "medium"
    );
    assert_eq!(
        body["toolConfig"]["functionCallingConfig"]["streamFunctionCallArguments"],
        true
    );

    handle.abort();
}

#[tokio::test]
async fn inline_data_attachment_is_serialized_on_wire() {
    let (base_url, recorded, handle) = spawn_mock_server(|_, _, _, _| {
        Json(json!({
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [{"text": "I see a cat."}]},
                "finishReason": "STOP"
            }]
        }))
        .into_response()
    })
    .await;

    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-flash".to_string(),
        auth: VertexAuth::ApiKey("k".to_string()),
        base_url: Some(base_url),
    })
    .unwrap();

    let mut msg = Message::user("");
    msg.content_parts = vec![
        MessageContentPart::Text {
            text: "Describe this".to_string(),
        },
        MessageContentPart::Attachment {
            hash: "cat".to_string(),
            mime_type: "image/png".to_string(),
            metadata: json!({"data": "iVBORw0KGgo="}),
        },
    ];
    let resp = client
        .generate(GenerationRequest {
            messages: vec![msg],
            ..GenerationRequest::new()
        })
        .await
        .unwrap();
    assert_eq!(resp.content, "I see a cat.");

    let body = &recorded.bodies.lock().unwrap()[0];
    assert_eq!(body["contents"][0]["parts"][0]["text"], "Describe this");
    assert_eq!(
        body["contents"][0]["parts"][1]["inlineData"],
        json!({"mimeType": "image/png", "data": "iVBORw0KGgo="})
    );
    handle.abort();
}

#[tokio::test]
async fn streaming_thought_signature_round_trips_to_second_turn() {
    let (base_url, _recorded, handle) = spawn_mock_server(|idx, _, _, body| {
        if idx == 0 {
            let sse = format!(
                "data: {}\n\ndata: {}\n\n",
                json!({
                    "candidates": [{
                        "index": 0,
                        "content": {
                            "role": "model",
                            "parts": [
                                {"text": "Thinking before calling tool", "thought": true},
                                {
                                    "functionCall": {
                                        "id": "call_sse_1",
                                        "name": "lookup",
                                        "args": {"key": "version"}
                                    },
                                    "thoughtSignature": "sig-from-sse-turn1"
                                }
                            ]
                        }
                    }]
                }),
                json!({
                    "candidates": [{
                        "index": 0,
                        "finishReason": "STOP"
                    }]
                })
            );
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                sse,
            )
                .into_response()
        } else {
            let has_sig = body["contents"][1]["parts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| {
                    p["functionCall"]["name"] == "lookup"
                        && p["thoughtSignature"] == "sig-from-sse-turn1"
                });
            if !has_sig {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "missing thought_signature"})),
                )
                    .into_response();
            }
            let sse = format!(
                "data: {}\n\n",
                json!({
                    "candidates": [{
                        "index": 0,
                        "content": {"role": "model", "parts": [{"text": "Version is 1.0"}]},
                        "finishReason": "STOP"
                    }]
                })
            );
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                sse,
            )
                .into_response()
        }
    })
    .await;

    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "global".to_string(),
            model: "gemini-3-pro-preview".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        Arc::new(SequenceTokenSource::new()),
    )
    .unwrap();

    let tool = ToolDefinition::new("lookup", "Lookup key").with_string_param("key", "Key");
    let mut rx1 = client
        .generate_stream(
            GenerationRequest::new()
                .with_user_message("check version")
                .with_tool(tool.clone()),
        )
        .await
        .unwrap();

    let mut thinking = String::new();
    let mut sig = None;
    let mut call = None;
    while let Some(chunk) = rx1.recv().await {
        if let Some(t) = chunk.thinking {
            thinking.push_str(&t);
        }
        if let Some(s) = chunk.thinking_signature {
            sig = Some(s);
        }
        if let Some(delta) = chunk.tool_call_delta
            && let (Some(name), Some(args)) = (delta.name, delta.arguments)
        {
            call = Some(crate::ToolCall {
                id: delta.id,
                name,
                arguments: serde_json::from_str(&args).unwrap(),
            });
        }
    }

    let mut assistant = Message::assistant_with_tools("", vec![call.unwrap()]);
    assistant.thinking = Some(thinking);
    assistant.thinking_signature = sig;
    let mut rx2 = client
        .generate_stream(GenerationRequest {
            messages: vec![
                Message::user("check version"),
                assistant,
                Message::tool("call_sse_1", r#"{"version":"1.0"}"#),
            ],
            tools: vec![tool],
            ..GenerationRequest::new()
        })
        .await
        .unwrap();

    let mut answer = String::new();
    while let Some(chunk) = rx2.recv().await {
        if let Some(t) = chunk.text {
            answer.push_str(&t);
        }
    }
    assert_eq!(answer, "Version is 1.0");
    handle.abort();
}

#[tokio::test]
async fn sse_streaming_refreshes_adc_token_once_on_401() {
    let (base_url, recorded, handle) = spawn_mock_server(|_, _, headers, _| {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if auth == "Bearer adc-token-1" {
            return (StatusCode::UNAUTHORIZED, Json(json!({"error": "expired"}))).into_response();
        }
        let sse = format!(
            "data: {}\n\n",
            json!({
                "candidates": [{
                    "index": 0,
                    "content": {"role": "model", "parts": [{"text": "streamed after refresh"}]},
                    "finishReason": "STOP"
                }]
            })
        );
        (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            sse,
        )
            .into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "global".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source.clone(),
    )
    .unwrap();

    let mut rx = client
        .generate_stream(GenerationRequest::new().with_user_message("hi"))
        .await
        .unwrap();
    let mut text = String::new();
    while let Some(chunk) = rx.recv().await {
        if let Some(t) = chunk.text {
            text.push_str(&t);
        }
    }
    assert_eq!(text, "streamed after refresh");
    assert_eq!(token_source.call_count(), 2);
    assert_eq!(
        recorded.auth_headers.lock().unwrap().as_slice(),
        [
            Some("Bearer adc-token-1".to_string()),
            Some("Bearer adc-token-2".to_string()),
        ]
    );
    handle.abort();
}
