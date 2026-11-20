//! Wire-mock tests for the first-party Magpie provider.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use super::*;
use crate::{Message, MessageContentPart, ReasoningEffort};

#[derive(Default)]
struct RecordedCall {
    auth: Option<String>,
    affinity: Option<String>,
    session_id: Option<String>,
    body: Value,
}

#[derive(Default)]
struct MockRouterState {
    models_requests: Mutex<usize>,
    models_payload: Mutex<Value>,
    chat_calls: Mutex<Vec<RecordedCall>>,
    stream_sse_body: Mutex<Option<String>>,
}

async fn start_mock_router(state: Arc<MockRouterState>) -> String {
    let app = Router::new()
        .route(
            "/v1/models",
            get(|State(state): State<Arc<MockRouterState>>| async move {
                *state.models_requests.lock().unwrap() += 1;
                let payload = state.models_payload.lock().unwrap().clone();
                Json(payload)
            }),
        )
        .route(
            "/v1/chat/completions",
            post(
                |State(state): State<Arc<MockRouterState>>,
                 headers: HeaderMap,
                 Json(body): Json<Value>| async move {
                    let record = RecordedCall {
                        auth: headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string),
                        affinity: headers
                            .get(MAGPIE_SESSION_AFFINITY_HEADER)
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string),
                        session_id: headers
                            .get(MAGPIE_SESSION_ID_HEADER)
                            .and_then(|value| value.to_str().ok())
                            .map(ToString::to_string),
                        body: body.clone(),
                    };
                    state.chat_calls.lock().unwrap().push(record);

                    if body.get("stream").and_then(Value::as_bool) == Some(true)
                        && let Some(sse) = state.stream_sse_body.lock().unwrap().clone()
                    {
                        return (
                            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                            sse,
                        )
                            .into_response();
                    }

                    Json(json!({
                        "id": "chatcmpl-magpie-1",
                        "object": "chat.completion",
                        "created": 1_700_000_000,
                        "model": body.get("model").and_then(Value::as_str).unwrap_or("default"),
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": "ok",
                                "reasoning_content": "plan"
                            },
                            "finish_reason": "stop"
                        }],
                        "usage": {
                            "prompt_tokens": 100,
                            "completion_tokens": 20,
                            "total_tokens": 120,
                            "prompt_tokens_details": { "cached_tokens": 80 },
                            "completion_tokens_details": { "reasoning_tokens": 12 }
                        }
                    }))
                    .into_response()
                },
            ),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn magpie_allows_empty_key_and_pins_stable_session_affinity_headers() {
    let state = Arc::new(MockRouterState::default());
    let base_url = start_mock_router(state.clone()).await;

    let mut client_a = MagpieClient::new(Some(&base_url), None, "qwen3-coder");
    let mut client_b = MagpieClient::new(Some(&base_url), Some("secret-token"), "qwen3-coder");
    assert_eq!(client_a.provider_name(), "magpie");
    assert_ne!(client_a.session_id(), client_b.session_id());

    let r1 = client_a
        .generate(GenerationRequest::new().with_user_message("turn 1"))
        .await
        .unwrap();
    assert_eq!(r1.content, "ok");
    assert_eq!(r1.thinking.as_deref(), Some("plan"));
    let usage = r1.usage.unwrap();
    assert_eq!(usage.cached_prompt_tokens, Some(80));
    assert_eq!(usage.reasoning_tokens, Some(12));

    client_a
        .generate(GenerationRequest::new().with_user_message("turn 2"))
        .await
        .unwrap();
    client_b
        .generate(
            GenerationRequest::new()
                .with_user_message("turn with session key")
                .with_session_key("boot-1/agent/root"),
        )
        .await
        .unwrap();

    let calls = state.chat_calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].auth, None);
    assert_eq!(calls[1].auth, None);
    assert_eq!(calls[0].affinity, Some(client_a.session_id().to_string()));
    assert_eq!(calls[0].affinity, calls[1].affinity);
    assert_eq!(calls[0].session_id, calls[1].session_id);

    assert_eq!(calls[2].auth.as_deref(), Some("Bearer secret-token"));
    assert_eq!(
        calls[2].affinity.as_deref(),
        Some(crate::session_affinity::derive_session_id("boot-1/agent/root").as_str())
    );
    assert_ne!(calls[2].affinity, calls[0].affinity);
}

#[tokio::test]
async fn magpie_discovers_live_models_and_applies_thinking_level_map() {
    let state = Arc::new(MockRouterState {
        models_payload: Mutex::new(json!({
            "object": "list",
            "data": [
                {
                    "id": "magpie-reasoner",
                    "contextWindow": 262_144,
                    "defaultThinkingLevel": "medium",
                    "thinkingLevelMap": {
                        "none": null,
                        "low": "fast",
                        "medium": "balanced",
                        "high": "deep"
                    }
                },
                {
                    "id": "magpie-fast",
                    "max_model_len": 65_536
                }
            ]
        })),
        ..Default::default()
    });
    let base_url = start_mock_router(state.clone()).await;

    let mut client = MagpieClient::new(Some(&base_url), Some(""), "magpie-reasoner");
    let catalog = client.model_catalog().await.unwrap().unwrap();
    assert_eq!(catalog.len(), 2);
    assert_eq!(catalog[0].slug, "magpie-reasoner");
    assert_eq!(catalog[0].context_window_tokens, 262_144);
    assert_eq!(
        catalog[0].default_reasoning_effort,
        Some(ReasoningEffort::Medium)
    );
    assert_eq!(
        catalog[0].supported_reasoning_efforts,
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High
        ]
    );
    assert_eq!(catalog[1].slug, "magpie-fast");
    assert_eq!(catalog[1].context_window_tokens, 65_536);

    client
        .generate(
            GenerationRequest::new()
                .with_user_message("think hard")
                .with_reasoning_effort(ReasoningEffort::High),
        )
        .await
        .unwrap();
    client
        .generate(
            GenerationRequest::new()
                .with_user_message("no thinking")
                .with_reasoning_effort(ReasoningEffort::None),
        )
        .await
        .unwrap();
    client
        .generate(
            GenerationRequest::new()
                .with_user_message("max effort")
                .with_reasoning_effort(ReasoningEffort::Ultra),
        )
        .await
        .unwrap();

    // One explicit model_catalog() call + one lazy thinkingLevelMap discovery on first reasoned turn.
    assert_eq!(*state.models_requests.lock().unwrap(), 2);
    let calls = state.chat_calls.lock().unwrap();
    assert_eq!(calls[0].body["reasoning_effort"], json!("deep"));
    assert!(calls[1].body.get("reasoning_effort").is_none());
    assert_eq!(calls[2].body["reasoning_effort"], json!("xhigh"));
}

#[tokio::test]
async fn magpie_streams_reasoning_deltas_and_projects_multimodal_images() {
    let sse = [
        r#"data: {"id":"cmpl-1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"reasoning_content":"step 1 "},"finish_reason":null}]}"#,
        r#"data: {"id":"cmpl-1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"content":"hello "},"finish_reason":null}]}"#,
        r#"data: {"id":"cmpl-1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"content":"world"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"prompt_tokens_details":{"cached_tokens":4}}}"#,
        "data: [DONE]",
    ]
    .join("\n\n")
        + "\n\n";

    let state = Arc::new(MockRouterState {
        stream_sse_body: Mutex::new(Some(sse)),
        ..Default::default()
    });
    let base_url = start_mock_router(state.clone()).await;

    let mut message = Message::user("describe both");
    message.content_parts = vec![
        MessageContentPart::Text {
            text: "describe both".to_string(),
        },
        MessageContentPart::Attachment {
            hash: "img-url".to_string(),
            mime_type: "image/png".to_string(),
            metadata: json!({"image_url": "https://example.com/a.png"}),
        },
        MessageContentPart::Attachment {
            hash: "img-b64".to_string(),
            mime_type: "image/jpeg".to_string(),
            metadata: json!({"data": "AQID"}),
        },
    ];
    let mut request = GenerationRequest::new();
    request.messages.push(message);

    let mut client = MagpieClient::new(Some(&base_url), None, "vision-model");
    let mut rx = client.generate_stream(request).await.unwrap();

    let mut thinking = String::new();
    let mut text = String::new();
    let mut final_usage = None;
    while let Some(chunk) = rx.recv().await {
        if let Some(t) = chunk.thinking {
            thinking.push_str(&t);
        }
        if let Some(t) = chunk.text {
            text.push_str(&t);
        }
        if chunk.usage.is_some() {
            final_usage = chunk.usage;
        }
    }
    assert_eq!(thinking, "step 1 ");
    assert_eq!(text, "hello world");
    assert_eq!(final_usage.unwrap().cached_prompt_tokens, Some(4));

    let calls = state.chat_calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].body["messages"][0]["content"],
        json!([
            {"type": "text", "text": "describe both"},
            {"type": "image_url", "image_url": {"url": "https://example.com/a.png"}},
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,AQID"}}
        ])
    );
}

#[tokio::test]
async fn magpie_model_catalog_parses_reasoning_levels_and_context_window() {
    let state = Arc::new(MockRouterState {
        models_payload: Mutex::new(json!({
            "data": [{
                "id": "auto/deepseek-v4.1-flash",
                "context_window": 1_048_576,
                "reasoning_levels": ["low", "high", "max"],
                "supported_reasoning_levels": [
                    {"effort": "low"},
                    {"effort": "high"},
                    {"effort": "max"}
                ],
                "default_reasoning_level": "high"
            }]
        })),
        ..Default::default()
    });
    let base_url = start_mock_router(state).await;
    let client = MagpieClient::new(Some(&base_url), None, "auto/deepseek-v4.1-flash");
    let catalog = client.model_catalog().await.unwrap().unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].slug, "auto/deepseek-v4.1-flash");
    assert_eq!(catalog[0].context_window_tokens, 1_048_576);
    assert_eq!(
        catalog[0].supported_reasoning_efforts,
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::High,
            ReasoningEffort::Max
        ]
    );
    assert_eq!(
        catalog[0].default_reasoning_effort,
        Some(ReasoningEffort::High)
    );
}
