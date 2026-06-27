//! WebSocket handler for real-time communication.

use alan_agent_protocol::{Event, EventEnvelope, Submission};
use axum::{
    extract::{
        Extension, Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use super::{
    remote_control::{RemoteRequestContext, required_scope_for_op},
    state::AppState,
};

const MAX_WEBSOCKET_SESSION_ID_BYTES: usize = 256;
const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 256 * 1024;

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    remote_context: Option<Extension<RemoteRequestContext>>,
) -> Response {
    let bounded = bounded_session_id(&session_id);
    if bounded.len() != session_id.len() {
        warn!(
            session_id_len = session_id.len(),
            max_len = MAX_WEBSOCKET_SESSION_ID_BYTES,
            "Rejecting WebSocket connection with oversized session_id"
        );
        return StatusCode::BAD_REQUEST.into_response();
    }

    let session_id = bounded;
    info!(%session_id, "WebSocket connection requested");
    let remote_context = match remote_context {
        Some(ext) => Some(ext.0),
        None => None,
    };
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id, remote_context))
        .into_response()
}

/// Handle an active WebSocket connection
async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    session_id: String,
    remote_context: Option<RemoteRequestContext>,
) {
    let session_id = bounded_session_id(&session_id);

    // Check if session exists
    let session_exists = match state.get_session(&session_id).await {
        Ok(exists) => exists,
        Err(err) => {
            error!(
                %session_id,
                error = %err,
                "Failed to recover sessions before WebSocket connection"
            );
            let envelope = control_envelope(
                &session_id,
                Event::Error {
                    message: "Failed to recover session state".to_string(),
                    recoverable: true,
                },
            );
            if let Ok(payload) = serde_json::to_string(&envelope) {
                let _ = socket.send(Message::Text(payload.into())).await;
            }
            return;
        }
    };

    if !session_exists {
        warn!(%session_id, "Session not found for WebSocket");
        let envelope = control_envelope(
            &session_id,
            Event::Error {
                message: "Session not found".to_string(),
                recoverable: false,
            },
        );
        let error_msg = serde_json::to_string(&envelope)
            .unwrap_or_else(|_| r#"{"type":"error","message":"serialize failed"}"#.to_string());

        let _ = socket.send(Message::Text(error_msg.into())).await;
        return;
    }

    // Clone necessary channels outside of any await point
    let (mut events_rx, mut submission_tx) = {
        let sessions = state.sessions.read().await;
        match sessions.get(&session_id) {
            Some(session) => (session.events_tx.subscribe(), session.submission_tx.clone()),
            None => {
                warn!(%session_id, "Session not found for WebSocket after check");
                let envelope = control_envelope(
                    &session_id,
                    Event::Error {
                        message: "Session not found".to_string(),
                        recoverable: false,
                    },
                );
                let error_msg = serde_json::to_string(&envelope).unwrap_or_else(|_| {
                    r#"{"type":"error","message":"serialize failed"}"#.to_string()
                });

                let _ = socket.send(Message::Text(error_msg.into())).await;
                return;
            }
        }
    };

    info!(%session_id, "WebSocket connected");

    // Main message loop
    let mut last_event_id: Option<String> = None;
    loop {
        tokio::select! {
            // Handle incoming messages
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if text.len() > MAX_WEBSOCKET_MESSAGE_BYTES {
                            warn!(
                                %session_id,
                                message_len = text.len(),
                                max_len = MAX_WEBSOCKET_MESSAGE_BYTES,
                                "Rejecting oversized WebSocket text message"
                            );
                            let envelope = control_envelope(
                                &session_id,
                                Event::Error {
                                    message: "WebSocket message exceeds maximum size".to_string(),
                                    recoverable: true,
                                },
                            );
                            if let Ok(payload) = serde_json::to_string(&envelope) {
                                let _ = socket.send(Message::Text(payload.into())).await;
                            }
                            continue;
                        }
                        let parse_text = text.as_str();
                        debug!(%session_id, "Received WS message");

                        // Try to parse as a submission
                        match serde_json::from_str::<Submission>(parse_text) {
                            Ok(submission) => {
                                let required_scope = required_scope_for_op(&submission.op);
                                if let Some(context) = remote_context.as_ref()
                                    && !context.allows_scope(required_scope)
                                {
                                    warn!(
                                        %session_id,
                                        required_scope = ?required_scope,
                                        "Rejecting WebSocket submission due to insufficient remote scope"
                                    );
                                    let envelope = control_envelope(
                                        &session_id,
                                        Event::Error {
                                            message: format!(
                                                "Missing required scope for operation: {:?}",
                                                required_scope
                                            ),
                                            recoverable: true,
                                        },
                                    );
                                    if let Ok(payload) = serde_json::to_string(&envelope) {
                                        let _ = socket.send(Message::Text(payload.into())).await;
                                    }
                                    continue;
                                }
                                // Update session inbound activity
                                if let Err(err) = state.touch_session_inbound(&session_id).await {
                                    error!(
                                        %session_id,
                                        error = %err,
                                        "Failed to update inbound activity for WebSocket submission"
                                    );
                                    let envelope = control_envelope(
                                        &session_id,
                                        Event::Error {
                                            message: "Failed to update session activity".to_string(),
                                            recoverable: true,
                                        },
                                    );
                                    if let Ok(payload) = serde_json::to_string(&envelope) {
                                        let _ = socket.send(Message::Text(payload.into())).await;
                                    }
                                    continue;
                                }
                                // Use the cloned sender instead of holding the lock.
                                // If send fails (e.g. post-restart placeholder channel), resume runtime and retry once.
                                if submission_tx.send(submission.clone()).await.is_err() {
                                    warn!(%session_id, "Submission channel send failed; attempting runtime resume");
                                    match state.resume_session_runtime(&session_id).await {
                                        Ok(()) => {
                                            let refreshed_tx = {
                                                let sessions = state.sessions.read().await;
                                                sessions
                                                    .get(&session_id)
                                                    .map(|session| session.submission_tx.clone())
                                            };
                                            if let Some(tx) = refreshed_tx {
                                                submission_tx = tx;
                                                if submission_tx.send(submission).await.is_err() {
                                                    error!(%session_id, "Failed to send submission after runtime resume");
                                                    let envelope = control_envelope(
                                                        &session_id,
                                                        Event::Error {
                                                            message: "Failed to submit operation after runtime recovery".to_string(),
                                                            recoverable: true,
                                                        },
                                                    );
                                                    if let Ok(payload) = serde_json::to_string(&envelope) {
                                                        let _ = socket.send(Message::Text(payload.into())).await;
                                                    }
                                                }
                                            } else {
                                                error!(%session_id, "Session disappeared while refreshing submission channel");
                                                let envelope = control_envelope(
                                                    &session_id,
                                                    Event::Error {
                                                        message: "Session not found while retrying submission".to_string(),
                                                        recoverable: false,
                                                    },
                                                );
                                                if let Ok(payload) = serde_json::to_string(&envelope) {
                                                    let _ = socket.send(Message::Text(payload.into())).await;
                                                }
                                                break;
                                            }
                                        }
                                        Err(err) => {
                                            error!(%session_id, error = %err, "Failed to resume runtime after submission channel failure");
                                            let envelope = control_envelope(
                                                &session_id,
                                                Event::Error {
                                                    message: format!("Failed to resume runtime: {err}"),
                                                    recoverable: true,
                                                },
                                            );
                                            if let Ok(payload) = serde_json::to_string(&envelope) {
                                                let _ = socket.send(Message::Text(payload.into())).await;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(%session_id, ?e, "Failed to parse WebSocket message");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!(%session_id, "WebSocket closed by client");
                        break;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        debug!(%session_id, "Received pong");
                    }
                    Some(Err(e)) => {
                        error!(%session_id, ?e, "WebSocket error");
                        break;
                    }
                    None => {
                        info!(%session_id, "WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            // Forward agent events to websocket
            event = events_rx.recv() => {
                match event {
                    Ok(envelope) => {
                        // Update outbound activity tracking
                        if let Err(err) = state.touch_session_outbound(&session_id).await {
                            warn!(
                                %session_id,
                                error = %err,
                                "Failed to update outbound activity for WebSocket stream"
                            );
                        }
                        last_event_id = Some(envelope.event_id.clone());

                        let payload = serde_json::to_string(&envelope)
                            .unwrap_or_else(|_| "Failed to serialize event".to_string());
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            debug!(%session_id, "WebSocket closed (send failed)");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(%session_id, missed = count, "WebSocket lagged behind events");
                        let payload = serde_json::to_string(&stream_lagged_envelope(
                            &session_id,
                            count,
                            last_event_id.clone(),
                        ))
                        .unwrap_or_else(|_| "Failed to serialize event".to_string());
                        let _ = socket.send(Message::Text(payload.into())).await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        debug!(%session_id, "Event stream closed");
                        break;
                    }
                }
            }

            // Periodic ping to keep connection alive
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    debug!(%session_id, "WebSocket closed (ping failed)");
                    break;
                }
            }
        }
    }

    info!(%session_id, "WebSocket disconnected");
    // Note: We don't remove the session here because multiple clients may be
    // connected to the same session. The session is cleaned up by:
    // 1. TTL cleanup when session is idle
    // 2. Explicit DELETE request to the session endpoint
    // 3. Application shutdown
}

/// Create a control envelope for errors and other control messages.
/// Control envelopes use special IDs to distinguish them from regular events.
fn control_envelope(session_id: &str, event: Event) -> EventEnvelope {
    let event_type = match &event {
        Event::Error { .. } => "error",
        _ => "control",
    };

    EventEnvelope {
        event_id: format!("control_{}_{}", event_type, uuid::Uuid::new_v4()),
        sequence: 0,
        session_id: bounded_session_id(session_id),
        submission_id: None,
        turn_id: "turn_control".to_string(),
        item_id: "item_control".to_string(),
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        event,
    }
}

fn stream_lagged_envelope(
    session_id: &str,
    skipped: u64,
    replay_from_event_id: Option<String>,
) -> EventEnvelope {
    let replay_hint = replay_from_event_id
        .as_deref()
        .map(|event_id| format!(" Replay from event_id={event_id}."))
        .unwrap_or_default();
    EventEnvelope {
        event_id: format!("control_lagged_{}", uuid::Uuid::new_v4()),
        sequence: 0,
        session_id: bounded_session_id(session_id),
        submission_id: None,
        turn_id: "turn_control".to_string(),
        item_id: "item_control".to_string(),
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        event: Event::Error {
            message: format!(
                "Event stream lagged and skipped {skipped} event(s).{}",
                replay_hint
            ),
            recoverable: true,
        },
    }
}

fn bounded_session_id(session_id: &str) -> String {
    if session_id.len() <= MAX_WEBSOCKET_SESSION_ID_BYTES {
        return session_id.to_string();
    }
    bounded_prefix(session_id, MAX_WEBSOCKET_SESSION_ID_BYTES).to_string()
}

fn bounded_prefix(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

#[cfg(test)]
mod tests {

    use super::super::remote_control::{RemoteRequestContext, SessionScope};
    use super::super::state::{AppState, SessionEntry, SessionEventLog};
    use super::{
        MAX_WEBSOCKET_MESSAGE_BYTES, MAX_WEBSOCKET_SESSION_ID_BYTES, bounded_session_id, ws_handler,
    };
    use alan_agent_engine::{
        Config,
        runtime::{SessionDurabilityState, WorkspaceRuntimeConfig},
    };
    use alan_agent_protocol::{Event, EventEnvelope, Op, Submission};
    use axum::{Extension, Router, http::StatusCode, routing::get};
    use futures::{SinkExt, StreamExt};
    use tokio::sync::{broadcast, mpsc};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    async fn spawn_ws_server(state: AppState) -> Option<(String, tokio::task::JoinHandle<()>)> {
        spawn_ws_server_with_context(state, None).await
    }

    async fn spawn_ws_server_with_context(
        state: AppState,
        remote_context: Option<RemoteRequestContext>,
    ) -> Option<(String, tokio::task::JoinHandle<()>)> {
        let mut app = Router::new()
            .route("/api/v1/sessions/{id}/ws", get(ws_handler))
            .with_state(state);
        if let Some(context) = remote_context {
            app = app.layer(Extension(context));
        }
        let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping websocket test: cannot bind local tcp listener: {err}");
                return None;
            }
            Err(err) => panic!("failed to bind websocket test listener: {err}"),
        };
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Some((format!("ws://{}", addr), handle))
    }

    fn test_state() -> AppState {
        let base_dir =
            std::env::temp_dir().join(format!("agentd-ws-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base_dir).unwrap();

        // Create test resolver and runtime manager
        let resolver = crate::daemon::workspace_resolver::WorkspaceResolver::with_registry(
            crate::registry::WorkspaceRegistry {
                version: 1,
                workspaces: vec![],
            },
            base_dir.clone(),
        );
        let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        let runtime_manager = crate::daemon::runtime_manager::RuntimeManager::with_template(
            WorkspaceRuntimeConfig::from(config.clone()),
        );
        let store = std::sync::Arc::new(
            crate::daemon::session_store::SessionStore::with_dir(base_dir.join("sessions"))
                .unwrap(),
        );
        let task_store = std::sync::Arc::new(
            crate::daemon::task_store::TaskStore::new(
                crate::daemon::task_store::JsonFileTaskStoreBackend::with_storage_dir(
                    base_dir.join("tasks"),
                )
                .unwrap(),
            )
            .unwrap(),
        );

        AppState::from_parts(
            config,
            std::sync::Arc::new(resolver),
            std::sync::Arc::new(runtime_manager),
            store,
            task_store,
            3600,
        )
    }

    fn test_session_entry(
        workspace_path: &std::path::Path,
    ) -> (SessionEntry, mpsc::Receiver<Submission>) {
        let (submission_tx, submission_rx) = mpsc::channel(8);
        let (events_tx, _) = broadcast::channel(8);
        let event_log = std::sync::Arc::new(tokio::sync::RwLock::new(SessionEventLog::new(32)));
        let entry = SessionEntry::new(
            workspace_path.to_path_buf(),
            workspace_path.join(".alan"),
            None,
            None,
            None,
            "gpt-5.4".to_string(),
            Some(alan_agent_protocol::ReasoningEffort::Medium),
            alan_agent_protocol::GovernanceConfig {
                profile: alan_agent_protocol::GovernanceProfile::Autonomous,
                policy_path: None,
            },
            alan_agent_engine::StreamingMode::Auto,
            alan_agent_engine::PartialStreamRecoveryMode::ContinueOnce,
            SessionDurabilityState {
                durable: true,
                required: false,
            },
            submission_tx,
            events_tx,
            event_log,
            None,
            None,
        );
        (entry, submission_rx)
    }

    async fn next_text_message(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> String {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match msg {
            Message::Text(text) => text.to_string(),
            other => panic!("Expected text message, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn websocket_returns_error_envelope_for_missing_session() {
        let state = test_state();
        let Some((base, server)) = spawn_ws_server(state).await else {
            return;
        };
        let (mut ws, _) = connect_async(format!("{}/api/v1/sessions/missing/ws", base))
            .await
            .unwrap();

        let text = next_text_message(&mut ws).await;
        // Should receive EventEnvelope, not bare Event
        let envelope: EventEnvelope = serde_json::from_str(&text).unwrap();

        // Verify envelope metadata
        assert!(
            envelope.event_id.starts_with("control_error_"),
            "Expected control error event_id, got: {}",
            envelope.event_id
        );
        assert_eq!(envelope.session_id, "missing");
        assert_eq!(envelope.turn_id, "turn_control");
        assert_eq!(envelope.item_id, "item_control");
        assert_eq!(envelope.sequence, 0);

        // Verify the wrapped event
        match envelope.event {
            Event::Error {
                message,
                recoverable,
            } => {
                assert_eq!(message, "Session not found");
                assert!(!recoverable);
            }
            other => panic!("Unexpected event: {:?}", other),
        }

        server.abort();
    }

    #[tokio::test]
    async fn websocket_rejects_oversized_session_id() {
        let state = test_state();
        let Some((base, server)) = spawn_ws_server(state).await else {
            return;
        };

        let oversized = "s".repeat(MAX_WEBSOCKET_SESSION_ID_BYTES + 1);
        let err = connect_async(format!("{}/api/v1/sessions/{}/ws", base, oversized))
            .await
            .expect_err("oversized session id should fail websocket upgrade");

        match err {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            }
            other => panic!(
                "Expected HTTP error for oversized session id, got {:?}",
                other
            ),
        }

        server.abort();
    }

    #[test]
    fn bounded_session_id_preserves_utf8_boundaries() {
        let input = "🙂".repeat(70); // 280 bytes
        let bounded = bounded_session_id(&input);
        assert!(bounded.len() <= MAX_WEBSOCKET_SESSION_ID_BYTES);
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
        assert!(input.starts_with(&bounded));
    }

    #[tokio::test]
    async fn websocket_forwards_events_and_submissions() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = test_session_entry(temp.path());
        let events_tx = entry.events_tx.clone();
        state
            .sessions
            .write()
            .await
            .insert("sess-1".to_string(), entry);

        let Some((base, server)) = spawn_ws_server(state.clone()).await else {
            return;
        };
        let (mut ws, _) = connect_async(format!("{}/api/v1/sessions/sess-1/ws", base))
            .await
            .unwrap();

        events_tx
            .send(EventEnvelope {
                event_id: "evt_0000000000000001".to_string(),
                sequence: 1,
                session_id: "sess-1".to_string(),
                submission_id: Some("sub-test".to_string()),
                turn_id: "turn_000001".to_string(),
                item_id: "item_000001_0001".to_string(),
                timestamp_ms: 1,
                event: Event::ThinkingDelta {
                    chunk: "from-runtime".to_string(),
                    is_final: false,
                },
            })
            .unwrap();

        let text = next_text_message(&mut ws).await;
        let envelope: EventEnvelope = serde_json::from_str(&text).unwrap();
        assert_eq!(envelope.event_id, "evt_0000000000000001");
        match envelope.event {
            Event::ThinkingDelta { chunk, is_final } => {
                assert_eq!(chunk, "from-runtime");
                assert!(!is_final);
            }
            other => panic!("Unexpected event: {:?}", other),
        }

        let submission = Submission::new(Op::Interrupt);
        ws.send(Message::Text(
            serde_json::to_string(&submission).unwrap().into(),
        ))
        .await
        .unwrap();

        let forwarded =
            tokio::time::timeout(std::time::Duration::from_secs(2), submission_rx.recv())
                .await
                .unwrap()
                .unwrap();
        assert!(matches!(forwarded.op, Op::Interrupt));

        let _ = ws.close(None).await;
        server.abort();
    }

    #[tokio::test]
    async fn websocket_forbids_privileged_submission_with_write_only_scope() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = test_session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-scope".to_string(), entry);

        let remote_context = RemoteRequestContext {
            node_id: Some("node-1".to_string()),
            client_id: Some("client-1".to_string()),
            trace_id: Some("trace-1".to_string()),
            transport_mode: None,
            required_scope: Some(SessionScope::Write),
            granted_scopes: Some(std::collections::HashSet::from([SessionScope::Write])),
            auth_enabled: true,
            authenticated: true,
        };

        let Some((base, server)) = spawn_ws_server_with_context(state, Some(remote_context)).await
        else {
            return;
        };
        let (mut ws, _) = connect_async(format!("{}/api/v1/sessions/sess-scope/ws", base))
            .await
            .unwrap();

        ws.send(Message::Text(
            serde_json::to_string(&Submission::new(Op::Rollback { turns: 1 }))
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

        let text = next_text_message(&mut ws).await;
        let envelope: EventEnvelope = serde_json::from_str(&text).unwrap();
        match envelope.event {
            Event::Error {
                message,
                recoverable,
            } => {
                assert!(message.contains("Missing required scope"));
                assert!(recoverable);
            }
            other => panic!("Unexpected event: {:?}", other),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), submission_rx.recv())
                .await
                .is_err(),
            "forbidden op should not be forwarded to runtime"
        );

        let _ = ws.close(None).await;
        server.abort();
    }

    #[tokio::test]
    async fn websocket_rejects_oversized_text_message() {
        let state = test_state();
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, mut submission_rx) = test_session_entry(temp.path());
        state
            .sessions
            .write()
            .await
            .insert("sess-oversized".to_string(), entry);

        let Some((base, server)) = spawn_ws_server(state).await else {
            return;
        };
        let (mut ws, _) = connect_async(format!("{}/api/v1/sessions/sess-oversized/ws", base))
            .await
            .unwrap();

        ws.send(Message::Text(
            "x".repeat(MAX_WEBSOCKET_MESSAGE_BYTES + 1).into(),
        ))
        .await
        .unwrap();

        let text = next_text_message(&mut ws).await;
        let envelope: EventEnvelope = serde_json::from_str(&text).unwrap();
        match envelope.event {
            Event::Error {
                message,
                recoverable,
            } => {
                assert!(message.contains("exceeds maximum size"));
                assert!(recoverable);
            }
            other => panic!("Unexpected event: {:?}", other),
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), submission_rx.recv())
                .await
                .is_err(),
            "oversized payload should not be forwarded to runtime"
        );

        let _ = ws.close(None).await;
        server.abort();
    }

    #[tokio::test]
    async fn websocket_emits_error_when_submit_recovery_fails() {
        let base_dir =
            std::env::temp_dir().join(format!("agentd-ws-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base_dir).unwrap();
        let resolver = crate::daemon::workspace_resolver::WorkspaceResolver::with_registry(
            crate::registry::WorkspaceRegistry {
                version: 1,
                workspaces: vec![],
            },
            base_dir.clone(),
        );
        let config = Config::for_openai_responses("sk-test", None, Some("gpt-5.4"));
        let runtime_manager = crate::daemon::runtime_manager::RuntimeManager::new(
            crate::daemon::runtime_manager::RuntimeManagerConfig {
                max_concurrent_runtimes: 0,
                runtime_config_template: WorkspaceRuntimeConfig::from(config.clone()),
            },
        );
        let store = std::sync::Arc::new(
            crate::daemon::session_store::SessionStore::with_dir(base_dir.join("sessions"))
                .unwrap(),
        );
        let task_store = std::sync::Arc::new(
            crate::daemon::task_store::TaskStore::new(
                crate::daemon::task_store::JsonFileTaskStoreBackend::with_storage_dir(
                    base_dir.join("tasks"),
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let state = AppState::from_parts(
            config,
            std::sync::Arc::new(resolver),
            std::sync::Arc::new(runtime_manager),
            store,
            task_store,
            3600,
        );

        let temp = tempfile::TempDir::new().unwrap();
        let (mut entry, submission_rx) = test_session_entry(temp.path());
        drop(submission_rx);
        let (events_tx, _) = broadcast::channel(8);
        entry.events_tx = events_tx;
        state
            .sessions
            .write()
            .await
            .insert("sess-resume-fail".to_string(), entry);

        let Some((base, server)) = spawn_ws_server(state).await else {
            return;
        };
        let (mut ws, _) = connect_async(format!("{}/api/v1/sessions/sess-resume-fail/ws", base))
            .await
            .unwrap();

        ws.send(Message::Text(
            serde_json::to_string(&Submission::new(Op::Interrupt))
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();

        let text = next_text_message(&mut ws).await;
        let envelope: EventEnvelope = serde_json::from_str(&text).unwrap();
        match envelope.event {
            Event::Error {
                message,
                recoverable,
            } => {
                assert!(message.contains("Failed to resume runtime"));
                assert!(recoverable);
            }
            other => panic!("Unexpected event: {:?}", other),
        }

        let _ = ws.close(None).await;
        server.abort();
    }

    /// Test that all WebSocket messages follow EventEnvelope format consistently
    #[tokio::test]
    async fn websocket_consistent_envelope_format() {
        let state = test_state();
        let Some((base, server)) = spawn_ws_server(state.clone()).await else {
            return;
        };

        // Test 1: Missing session returns EventEnvelope
        let (mut ws1, _) = connect_async(format!("{}/api/v1/sessions/nonexistent/ws", base))
            .await
            .unwrap();
        let text1 = next_text_message(&mut ws1).await;
        let result1: Result<EventEnvelope, _> = serde_json::from_str(&text1);
        assert!(
            result1.is_ok(),
            "Missing session should return EventEnvelope, got: {}",
            text1
        );

        // Verify it's a control envelope
        let envelope1 = result1.unwrap();
        assert!(envelope1.event_id.starts_with("control_"));
        assert!(matches!(envelope1.event, Event::Error { .. }));
        let _ = ws1.close(None).await;

        // Test 2: Valid session also returns EventEnvelope
        let temp = tempfile::TempDir::new().unwrap();
        let (entry, _) = test_session_entry(temp.path());
        let events_tx = entry.events_tx.clone();
        state
            .sessions
            .write()
            .await
            .insert("sess-2".to_string(), entry);

        let (mut ws2, _) = connect_async(format!("{}/api/v1/sessions/sess-2/ws", base))
            .await
            .unwrap();

        // Send a test event
        events_tx
            .send(EventEnvelope {
                event_id: "evt_test_001".to_string(),
                sequence: 1,
                session_id: "sess-2".to_string(),
                submission_id: None,
                turn_id: "turn_001".to_string(),
                item_id: "item_001".to_string(),
                timestamp_ms: 12345,
                event: Event::TurnStarted {},
            })
            .unwrap();

        let text2 = next_text_message(&mut ws2).await;
        let result2: Result<EventEnvelope, _> = serde_json::from_str(&text2);
        assert!(
            result2.is_ok(),
            "Valid session should return EventEnvelope, got: {}",
            text2
        );

        let envelope2 = result2.unwrap();
        assert_eq!(envelope2.event_id, "evt_test_001");
        assert!(matches!(envelope2.event, Event::TurnStarted { .. }));

        let _ = ws2.close(None).await;
        server.abort();
    }
}
