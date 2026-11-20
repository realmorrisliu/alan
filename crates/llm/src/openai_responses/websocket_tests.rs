use super::websocket::{is_official_openai_host, response_create_frame, websocket_url};
use super::*;
use crate::StreamChunk;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router, extract::State};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::protocol::Message;

async fn collect_chunks(mut rx: tokio::sync::mpsc::Receiver<StreamChunk>) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();
    while let Some(chunk) = rx.recv().await {
        chunks.push(chunk);
    }
    chunks
}

fn sample_turn_events(response_id: &str, text_a: &str, text_b: &str) -> Vec<Value> {
    vec![
        json!({
            "type": "response.reasoning_summary_text.delta",
            "delta": "thinking...",
            "sequence_number": 1
        }),
        json!({
            "type": "response.output_text.delta",
            "delta": text_a,
            "sequence_number": 2
        }),
        json!({
            "type": "response.output_text.delta",
            "delta": " ",
            "sequence_number": 3
        }),
        json!({
            "type": "response.output_text.delta",
            "delta": text_b,
            "sequence_number": 4
        }),
        json!({
            "type": "response.function_call_arguments.delta",
            "output_index": 1,
            "call_id": "call_1",
            "name": "lookup",
            "delta": "{\"q\":",
            "sequence_number": 5
        }),
        json!({
            "type": "response.output_item.done",
            "output_index": 1,
            "sequence_number": 6,
            "item": {
                "type": "function_call",
                "call_id": "call_1",
                "name": "lookup",
                "arguments": "{\"q\":\"alan\"}"
            }
        }),
        json!({
            "type": "response.completed",
            "sequence_number": 7,
            "response": {
                "id": response_id,
                "status": "completed",
                "output": [
                    { "type": "reasoning", "encrypted_content": "enc_sig_ws" },
                    {
                        "type": "function_call",
                        "call_id": "call_1",
                        "name": "lookup",
                        "arguments": "{\"q\":\"alan\"}"
                    }
                ],
                "usage": {
                    "input_tokens": 20,
                    "input_tokens_details": { "cached_tokens": 12 },
                    "output_tokens": 10,
                    "total_tokens": 30,
                    "output_tokens_details": { "reasoning_tokens": 4 }
                }
            }
        }),
    ]
}

fn encode_sse(events: &[Value]) -> String {
    let mut body = String::new();
    for event in events {
        body.push_str("data: ");
        body.push_str(&event.to_string());
        body.push_str("\n\n");
    }
    body
}

#[test]
fn test_websocket_url_and_official_host_detection() {
    assert_eq!(
        websocket_url("https://api.openai.com/v1").as_deref(),
        Some("wss://api.openai.com/v1/responses")
    );
    assert_eq!(
        websocket_url("https://api.openai.com/v1/").as_deref(),
        Some("wss://api.openai.com/v1/responses")
    );
    assert_eq!(
        websocket_url("http://127.0.0.1:8080/v1").as_deref(),
        Some("ws://127.0.0.1:8080/v1/responses")
    );
    assert_eq!(websocket_url("ftp://example.com/v1"), None);

    assert!(is_official_openai_host("https://api.openai.com/v1"));
    assert!(is_official_openai_host("HTTPS://API.OPENAI.COM:443/v1/"));
    assert!(!is_official_openai_host("http://api.openai.com/v1"));
    assert!(!is_official_openai_host(
        "https://api.openai.com.evil.com/v1"
    ));
    assert!(!is_official_openai_host("https://proxy.internal/v1"));
    assert!(!is_official_openai_host("http://127.0.0.1:9000/v1"));
}

#[test]
fn test_transport_mode_gating_for_auto_sse_and_websocket() {
    let req = |client: &OpenAiResponsesClient| {
        client
            .build_openai_responses_request(GenerationRequest::new().with_user_message("hi"), true)
            .unwrap()
    };

    let official_auto =
        OpenAiResponsesClient::with_params("k", "https://api.openai.com/v1", "gpt-5.4");
    assert!(official_auto.should_try_websocket(&req(&official_auto)));

    let official_sse = official_auto.with_transport(OpenAiResponsesTransport::Sse);
    assert!(!official_sse.should_try_websocket(&req(&official_sse)));

    let local_auto = OpenAiResponsesClient::with_params("k", "http://127.0.0.1:9999/v1", "gpt-5.4");
    assert!(!local_auto.should_try_websocket(&req(&local_auto)));

    let local_ws = local_auto.with_transport(OpenAiResponsesTransport::WebSocket);
    assert!(local_ws.should_try_websocket(&req(&local_ws)));

    // Background requests never use WebSocket.
    let bg_req = local_ws
        .build_openai_responses_request(
            GenerationRequest::new()
                .with_user_message("bg")
                .with_extra_param("background", json!(true)),
            true,
        )
        .unwrap();
    assert!(!local_ws.should_try_websocket(&bg_req));
}

#[test]
fn test_response_create_frame_strips_stream_and_sets_type() {
    let client = OpenAiResponsesClient::with_params("k", "https://api.openai.com/v1", "gpt-5.4")
        .with_prompt_cache_key("fixed-key");
    let req = client
        .build_openai_responses_request(GenerationRequest::new().with_user_message("hello"), true)
        .unwrap();
    let frame: Value = serde_json::from_str(&response_create_frame(&req).unwrap()).unwrap();

    assert_eq!(frame["type"], json!("response.create"));
    assert!(frame.get("stream").is_none());
    assert!(frame.get("background").is_none());
    assert_eq!(frame["model"], json!("gpt-5.4"));
    assert_eq!(frame["prompt_cache_key"], json!("fixed-key"));
    assert_eq!(frame["prompt_cache_retention"], json!("24h"));
}

#[tokio::test]
#[allow(
    clippy::result_large_err,
    reason = "tungstenite handshake callback returns its own http::Response error type in test mock"
)]
async fn test_websocket_reuses_one_connection_and_matches_sse_chunks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_addr = listener.local_addr().unwrap();

    let accepted = Arc::new(AtomicUsize::new(0));
    let seen_auth = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen_frames = Arc::new(Mutex::new(Vec::<Value>::new()));

    let accepted_bg = Arc::clone(&accepted);
    let auth_bg = Arc::clone(&seen_auth);
    let frames_bg = Arc::clone(&seen_frames);

    let ws_server = tokio::spawn(async move {
        let mut turn = 0usize;
        while let Ok((stream, _)) = listener.accept().await {
            accepted_bg.fetch_add(1, Ordering::SeqCst);
            let auth_capture = Arc::clone(&auth_bg);
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |req: &Request, resp: Response| {
                    if let Some(value) = req.headers().get("authorization") {
                        auth_capture
                            .lock()
                            .unwrap()
                            .push(value.to_str().unwrap_or_default().to_string());
                    }
                    assert_eq!(req.uri().path(), "/v1/responses");
                    Ok(resp)
                },
            )
            .await
            .unwrap();

            while let Some(Ok(Message::Text(text))) = ws.next().await {
                turn += 1;
                let frame: Value = serde_json::from_str(text.as_str()).unwrap();
                frames_bg.lock().unwrap().push(frame);

                let events = if turn == 1 {
                    sample_turn_events("resp_ws_1", "Hello", "world")
                } else {
                    sample_turn_events("resp_ws_2", "Second", "turn")
                };
                for event in events {
                    ws.send(Message::text(event.to_string())).await.unwrap();
                }
            }
        }
    });

    let mut ws_client = OpenAiResponsesClient::with_params(
        "secret-token",
        &format!("http://{ws_addr}/v1"),
        "gpt-5.4",
    )
    .with_transport(OpenAiResponsesTransport::WebSocket);

    let rx1 = ws_client
        .generate_stream(GenerationRequest::new().with_user_message("turn 1"))
        .await
        .unwrap();
    let ws_chunks_1 = collect_chunks(rx1).await;

    let rx2 = ws_client
        .generate_stream(
            GenerationRequest::new()
                .with_user_message("turn 2")
                .with_previous_response_id("resp_ws_1")
                .with_store(false),
        )
        .await
        .unwrap();
    let ws_chunks_2 = collect_chunks(rx2).await;

    // One persistent TCP/WebSocket connection handled both turns.
    assert_eq!(ws_client.ws_connections_opened(), 1);
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    assert_eq!(
        seen_auth.lock().unwrap().as_slice(),
        &["Bearer secret-token".to_string()]
    );

    {
        let frames = seen_frames.lock().unwrap();
        assert_eq!(frames.len(), 2);
        for frame in frames.iter() {
            assert_eq!(frame["type"], json!("response.create"));
            assert!(frame.get("stream").is_none());
            assert_eq!(
                frame["prompt_cache_key"],
                json!(ws_client.prompt_cache_key())
            );
            assert_eq!(frame["prompt_cache_retention"], json!("24h"));
        }
        assert_eq!(frames[1]["previous_response_id"], json!("resp_ws_1"));
        assert_eq!(frames[1]["store"], json!(true));
    }

    // Now run the exact same event stream over HTTP SSE and assert identical chunks.
    let sse_body = encode_sse(&sample_turn_events("resp_ws_1", "Hello", "world"));
    let sse_app = Router::new().route(
        "/v1/responses",
        post(move || {
            let body = sse_body.clone();
            async move {
                (
                    [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                    body,
                )
            }
        }),
    );
    let sse_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sse_addr = sse_listener.local_addr().unwrap();
    let sse_server = tokio::spawn(async move {
        let _ = axum::serve(sse_listener, sse_app).await;
    });

    let mut sse_client = OpenAiResponsesClient::with_params(
        "secret-token",
        &format!("http://{sse_addr}/v1"),
        "gpt-5.4",
    )
    .with_transport(OpenAiResponsesTransport::Sse);
    let sse_rx = sse_client
        .generate_stream(GenerationRequest::new().with_user_message("turn 1"))
        .await
        .unwrap();
    let sse_chunks = collect_chunks(sse_rx).await;

    assert_eq!(ws_chunks_1.len(), sse_chunks.len());
    for (ws_chunk, sse_chunk) in ws_chunks_1.iter().zip(sse_chunks.iter()) {
        assert_eq!(ws_chunk.text, sse_chunk.text);
        assert_eq!(ws_chunk.thinking, sse_chunk.thinking);
        assert_eq!(ws_chunk.thinking_signature, sse_chunk.thinking_signature);
        assert_eq!(ws_chunk.usage, sse_chunk.usage);
        assert_eq!(
            ws_chunk.provider_response_id,
            sse_chunk.provider_response_id
        );
        assert_eq!(
            ws_chunk.provider_response_status,
            sse_chunk.provider_response_status
        );
        assert_eq!(ws_chunk.sequence_number, sse_chunk.sequence_number);
        assert_eq!(ws_chunk.is_finished, sse_chunk.is_finished);
        assert_eq!(ws_chunk.finish_reason, sse_chunk.finish_reason);
        assert_eq!(
            ws_chunk.tool_call_delta.as_ref().map(|d| (
                d.index,
                d.id.as_deref(),
                d.name.as_deref(),
                d.arguments_delta.as_deref(),
                d.arguments.as_deref(),
            )),
            sse_chunk.tool_call_delta.as_ref().map(|d| (
                d.index,
                d.id.as_deref(),
                d.name.as_deref(),
                d.arguments_delta.as_deref(),
                d.arguments.as_deref(),
            ))
        );
    }

    // Sanity-check turn 2 text and terminal fields.
    let turn2_text: String = ws_chunks_2
        .iter()
        .filter_map(|c| c.text.as_deref())
        .collect();
    assert_eq!(turn2_text, "Second turn");
    let last2 = ws_chunks_2.last().unwrap();
    assert!(last2.is_finished);
    assert_eq!(last2.provider_response_id.as_deref(), Some("resp_ws_2"));
    assert_eq!(last2.finish_reason.as_deref(), Some("tool_calls"));

    ws_server.abort();
    sse_server.abort();
}

#[tokio::test]
async fn test_websocket_reconnects_after_server_closes_idle_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_bg = Arc::clone(&accepted);

    let server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let conn_no = accepted_bg.fetch_add(1, Ordering::SeqCst) + 1;
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            if let Some(Ok(Message::Text(_))) = ws.next().await {
                let events = vec![
                    json!({
                        "type": "response.output_text.delta",
                        "delta": format!("conn-{conn_no}")
                    }),
                    json!({
                        "type": "response.completed",
                        "response": { "id": format!("resp_{conn_no}"), "status": "completed", "output": [] }
                    }),
                ];
                for event in events {
                    ws.send(Message::text(event.to_string())).await.unwrap();
                }
            }
            // Close the socket immediately after serving one turn.
            let _ = ws.close(None).await;
        }
    });

    let mut client =
        OpenAiResponsesClient::with_params("k", &format!("http://{addr}/v1"), "gpt-5.4")
            .with_transport(OpenAiResponsesTransport::WebSocket);

    let chunks_1 = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("first"))
            .await
            .unwrap(),
    )
    .await;
    let text_1: String = chunks_1.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text_1, "conn-1");

    // Let the server's close frame reach the client socket.
    tokio::time::sleep(Duration::from_millis(20)).await;

    let chunks_2 = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("second"))
            .await
            .unwrap(),
    )
    .await;
    let text_2: String = chunks_2.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text_2, "conn-2");
    assert_eq!(client.ws_connections_opened(), 2);
    assert!(!client.ws_is_cooling_down());

    server.abort();
}

#[derive(Clone, Default)]
struct FallbackMockState {
    ws_attempts: Arc<AtomicUsize>,
    sse_posts: Arc<Mutex<Vec<Value>>>,
}

#[tokio::test]
async fn test_websocket_handshake_rejected_falls_back_to_sse_and_cools_down() {
    let state = FallbackMockState::default();
    let app = Router::new()
        .route(
            "/v1/responses",
            get(|State(state): State<FallbackMockState>| async move {
                state.ws_attempts.fetch_add(1, Ordering::SeqCst);
                (StatusCode::UPGRADE_REQUIRED, "websocket unavailable")
            })
            .post(
                |State(state): State<FallbackMockState>, Json(body): Json<Value>| async move {
                    state.sse_posts.lock().unwrap().push(body);
                    let body = encode_sse(&[
                        json!({"type": "response.output_text.delta", "delta": "from-sse"}),
                        json!({
                            "type": "response.completed",
                            "response": { "id": "resp_sse", "status": "completed", "output": [] }
                        }),
                    ]);
                    (
                        [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
                        body,
                    )
                        .into_response()
                },
            ),
        )
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let mut client =
        OpenAiResponsesClient::with_params("test-key", &format!("http://{addr}/v1"), "gpt-5.4")
            .with_transport(OpenAiResponsesTransport::WebSocket)
            .with_ws_cooldown(Duration::from_secs(60));

    // Turn 1: WS handshake fails (426) -> falls back to HTTP SSE and streams cleanly.
    let chunks_1 = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("turn 1"))
            .await
            .unwrap(),
    )
    .await;
    let text_1: String = chunks_1.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text_1, "from-sse");
    assert_eq!(
        chunks_1.last().and_then(|c| c.finish_reason.as_deref()),
        Some("stop")
    );
    assert_eq!(state.ws_attempts.load(Ordering::SeqCst), 1);
    assert!(client.ws_is_cooling_down());

    // Turn 2: cooldown skips WS completely and goes straight to HTTP SSE.
    let chunks_2 = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("turn 2"))
            .await
            .unwrap(),
    )
    .await;
    let text_2: String = chunks_2.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text_2, "from-sse");
    assert_eq!(state.ws_attempts.load(Ordering::SeqCst), 1);

    let posts = state.sse_posts.lock().unwrap();
    assert_eq!(posts.len(), 2);
    assert_eq!(
        posts[0]["prompt_cache_key"],
        json!(client.prompt_cache_key())
    );
    assert_eq!(
        posts[1]["prompt_cache_key"],
        json!(client.prompt_cache_key())
    );

    server.abort();
}

#[tokio::test]
async fn test_websocket_mid_stream_close_surfaces_error_without_sse_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            if let Some(Ok(Message::Text(_))) = ws.next().await {
                ws.send(Message::text(
                    json!({"type": "response.output_text.delta", "delta": "partial"}).to_string(),
                ))
                .await
                .unwrap();
                let _ = ws.close(None).await;
            }
        }
    });

    let mut client =
        OpenAiResponsesClient::with_params("k", &format!("http://{addr}/v1"), "gpt-5.4")
            .with_transport(OpenAiResponsesTransport::WebSocket);

    let chunks = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("hi"))
            .await
            .unwrap(),
    )
    .await;

    let text: String = chunks.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text, "partial");
    let terminal = chunks.last().expect("terminal chunk");
    assert!(terminal.is_finished);
    assert_eq!(
        terminal.finish_reason.as_deref(),
        Some("stream_error:closed")
    );

    server.abort();
}

#[tokio::test]
async fn test_websocket_close_before_first_chunk_falls_back_to_sse() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let ws_accepted = Arc::new(AtomicUsize::new(0));
    let sse_served = Arc::new(AtomicUsize::new(0));
    let ws_bg = Arc::clone(&ws_accepted);
    let sse_bg = Arc::clone(&sse_served);

    let server = tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut peek = [0u8; 4];
            if stream.peek(&mut peek).await.is_err() {
                continue;
            }
            if &peek == b"GET " {
                ws_bg.fetch_add(1, Ordering::SeqCst);
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                // Read response.create, then immediately close with zero chunks emitted.
                let _ = ws.next().await;
                let _ = ws.close(None).await;
            } else {
                sse_bg.fetch_add(1, Ordering::SeqCst);
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let sse = encode_sse(&[
                    json!({"type": "response.output_text.delta", "delta": "recovered-via-sse"}),
                    json!({
                        "type": "response.completed",
                        "response": { "id": "resp_recovered", "status": "completed", "output": [] }
                    }),
                ]);
                let http_resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    sse.len(),
                    sse
                );
                let _ = stream.write_all(http_resp.as_bytes()).await;
            }
        }
    });

    let mut client =
        OpenAiResponsesClient::with_params("k", &format!("http://{addr}/v1"), "gpt-5.4")
            .with_transport(OpenAiResponsesTransport::WebSocket)
            .with_ws_cooldown(Duration::from_secs(60));

    let chunks = collect_chunks(
        client
            .generate_stream(GenerationRequest::new().with_user_message("hi"))
            .await
            .unwrap(),
    )
    .await;

    let text: String = chunks.iter().filter_map(|c| c.text.as_deref()).collect();
    assert_eq!(text, "recovered-via-sse");
    assert_eq!(
        chunks.last().and_then(|c| c.finish_reason.as_deref()),
        Some("stop")
    );
    assert_eq!(ws_accepted.load(Ordering::SeqCst), 1);
    assert_eq!(sse_served.load(Ordering::SeqCst), 1);
    assert!(client.ws_is_cooling_down());

    server.abort();
}
