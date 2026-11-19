//! WebSocket transport for the OpenAI Responses API.
//!
//! One persistent connection per client carries at most one in-flight
//! response. Each turn sends a `response.create` frame; server events share the
//! SSE event shapes and go through the same [`ResponsesStreamProjector`]. Any
//! failure before the first chunk reaches the caller is reported as
//! [`WebSocketOutcome::Fallback`] so the caller can retry over HTTP SSE.

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, OwnedMutexGuard, mpsc};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

use super::stream_events::{EventOutcome, ResponsesStreamProjector};
use crate::StreamChunk;
use crate::failure::StreamClosed;
use crate::openai_chat_completions::OpenAiResponsesRequest;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// How long WebSocket is skipped after a transport failure.
pub(crate) const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// OpenAI closes Responses sockets after 60 minutes; rotate before that.
const MAX_CONNECTION_AGE: Duration = Duration::from_secs(55 * 60);

/// Result of attempting one response over WebSocket.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WebSocketOutcome {
    /// The response was delivered (or the receiver went away).
    Done,
    /// Nothing reached the caller; retry the same request over HTTP SSE.
    Fallback,
}

struct Connection {
    socket: Socket,
    connected_at: Instant,
}

enum Turn {
    /// Response finished; `reusable` says whether the socket may carry another,
    /// and `trailing_chunks` are delivered after releasing the connection slot
    /// so an immediate next turn can reuse the socket without racing.
    Done {
        reusable: bool,
        trailing_chunks: Vec<StreamChunk>,
    },
    /// Failed before any chunk was delivered.
    FailedEarly { error_event: bool },
    /// Failed after chunks were delivered; must surface as a stream error.
    Failed(anyhow::Error),
}

/// Shared WebSocket state for one `OpenAiResponsesClient` and its stream tasks.
pub(crate) struct ResponsesWebSocket {
    url: Option<String>,
    connection: Arc<Mutex<Option<Connection>>>,
    cooldown_until: std::sync::Mutex<Option<Instant>>,
    cooldown: Duration,
    connections_opened: std::sync::atomic::AtomicUsize,
}

impl ResponsesWebSocket {
    pub(crate) fn new(url: Option<String>, cooldown: Duration) -> Self {
        Self {
            url,
            connection: Arc::new(Mutex::new(None)),
            cooldown_until: std::sync::Mutex::new(None),
            cooldown,
            connections_opened: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Whether a WebSocket attempt is currently allowed.
    pub(crate) fn available(&self) -> bool {
        self.url.is_some() && !self.cooling_down()
    }

    #[cfg(test)]
    pub(crate) fn connections_opened(&self) -> usize {
        self.connections_opened
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(test)]
    pub(crate) fn is_cooling_down(&self) -> bool {
        self.cooling_down()
    }

    fn cooling_down(&self) -> bool {
        let mut until = self
            .cooldown_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match *until {
            Some(deadline) if Instant::now() < deadline => true,
            Some(_) => {
                *until = None;
                false
            }
            None => false,
        }
    }

    fn start_cooldown(&self) {
        let mut until = self
            .cooldown_until
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *until = Some(Instant::now() + self.cooldown);
    }

    /// Stream one response over the persistent socket.
    pub(crate) async fn stream(
        &self,
        api_key: &str,
        request: &OpenAiResponsesRequest,
        tx: &mpsc::Sender<StreamChunk>,
    ) -> Result<WebSocketOutcome> {
        let Some(url) = self.url.as_deref() else {
            return Ok(WebSocketOutcome::Fallback);
        };
        if self.cooling_down() {
            return Ok(WebSocketOutcome::Fallback);
        }
        // One in-flight response per socket; a concurrent turn uses SSE rather
        // than queueing on the lock.
        let Ok(mut slot) = Arc::clone(&self.connection).try_lock_owned() else {
            debug!("Responses WebSocket busy; using HTTP SSE for this response");
            return Ok(WebSocketOutcome::Fallback);
        };
        let frame = response_create_frame(request)?;

        loop {
            let reused = self.take_reusable(&mut slot);
            if !reused {
                match self.connect(url, api_key).await {
                    Ok(connection) => *slot = Some(connection),
                    Err(error) => {
                        warn!(error = %error, "Responses WebSocket connect failed; falling back to SSE");
                        self.start_cooldown();
                        return Ok(WebSocketOutcome::Fallback);
                    }
                }
            }
            let Some(connection) = slot.as_mut() else {
                return Ok(WebSocketOutcome::Fallback);
            };
            match run_turn(&mut connection.socket, &frame, tx).await {
                Turn::Done {
                    reusable,
                    trailing_chunks,
                } => {
                    if !reusable {
                        *slot = None;
                    }
                    // Release the socket lock before sending the terminal chunk
                    // so a caller that starts the next turn immediately upon
                    // observing `is_finished` reuses the connection cleanly.
                    drop(slot);
                    for chunk in trailing_chunks {
                        if tx.send(chunk).await.is_err() {
                            break;
                        }
                    }
                    return Ok(WebSocketOutcome::Done);
                }
                Turn::FailedEarly { error_event } => {
                    *slot = None;
                    if reused && !error_event {
                        debug!("Reused Responses WebSocket was closed; reconnecting");
                        continue;
                    }
                    if !error_event {
                        warn!("Responses WebSocket failed before output; falling back to SSE");
                        self.start_cooldown();
                    }
                    return Ok(WebSocketOutcome::Fallback);
                }
                Turn::Failed(error) => {
                    *slot = None;
                    return Err(error);
                }
            }
        }
    }

    /// Keep a live, young connection; drop an aged one. Returns whether reused.
    fn take_reusable(&self, slot: &mut OwnedMutexGuard<Option<Connection>>) -> bool {
        match slot.as_ref() {
            Some(connection) if connection.connected_at.elapsed() < MAX_CONNECTION_AGE => true,
            Some(_) => {
                **slot = None;
                false
            }
            None => false,
        }
    }

    async fn connect(&self, url: &str, api_key: &str) -> Result<Connection> {
        let mut request = url
            .into_client_request()
            .context("Invalid Responses WebSocket URL")?;
        if !api_key.trim().is_empty() {
            request.headers_mut().insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))
                    .context("Invalid API key for Responses WebSocket")?,
            );
        }
        let (socket, _response) =
            tokio::time::timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(request))
                .await
                .context("Responses WebSocket connect timed out")?
                .context("Responses WebSocket handshake failed")?;
        self.connections_opened
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Connection {
            socket,
            connected_at: Instant::now(),
        })
    }
}

async fn run_turn(socket: &mut Socket, frame: &str, tx: &mpsc::Sender<StreamChunk>) -> Turn {
    if let Err(error) = socket.send(Message::text(frame.to_owned())).await {
        debug!(error = %error, "Failed to send response.create frame");
        return Turn::FailedEarly { error_event: false };
    }
    let mut projector = ResponsesStreamProjector::new();
    let mut delivered = false;
    loop {
        let message = tokio::select! {
            biased;
            // The server keeps generating for an abandoned response; drop the
            // socket so its late events never leak into the next turn.
            _ = tx.closed() => return Turn::Done {
                reusable: false,
                trailing_chunks: Vec::new(),
            },
            message = socket.next() => message,
        };
        let text = match message {
            Some(Ok(Message::Text(text))) => text.as_str().to_owned(),
            Some(Ok(Message::Binary(bytes))) => match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(_) => continue,
            },
            Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => continue,
            Some(Ok(Message::Close(_))) | None => return closed(delivered, None),
            Some(Err(error)) => return closed(delivered, Some(error)),
        };
        let projected = projector.project_data(&text);
        if projected.outcome == EventOutcome::Error && !delivered && projected.chunks.is_empty() {
            debug!(event = %text, "Responses WebSocket error event before output");
            return Turn::FailedEarly { error_event: true };
        }
        match projected.outcome {
            EventOutcome::Continue => {
                for chunk in projected.chunks {
                    delivered = true;
                    if tx.send(chunk).await.is_err() {
                        return Turn::Done {
                            reusable: false,
                            trailing_chunks: Vec::new(),
                        };
                    }
                }
            }
            EventOutcome::Terminal => {
                return Turn::Done {
                    reusable: true,
                    trailing_chunks: projected.chunks,
                };
            }
            EventOutcome::Error => {
                return Turn::Done {
                    reusable: false,
                    trailing_chunks: projected.chunks,
                };
            }
        }
    }
}

fn closed(delivered: bool, error: Option<tokio_tungstenite::tungstenite::Error>) -> Turn {
    if !delivered {
        return Turn::FailedEarly { error_event: false };
    }
    let error = match error {
        Some(error) => anyhow::Error::new(StreamClosed).context(error.to_string()),
        None => anyhow::Error::new(StreamClosed),
    };
    Turn::Failed(error.context("Responses WebSocket closed mid-response"))
}

/// Serialize a Responses request as a WebSocket `response.create` frame.
pub(crate) fn response_create_frame(request: &OpenAiResponsesRequest) -> Result<String> {
    let mut body = serde_json::to_value(request).context("Failed to encode Responses request")?;
    let object = body
        .as_object_mut()
        .context("Responses request did not encode as an object")?;
    // Streaming is implicit on the socket; background mode is HTTP-only.
    object.remove("stream");
    object.remove("background");
    object.insert(
        "type".to_string(),
        serde_json::Value::String("response.create".to_string()),
    );
    serde_json::to_string(&body).context("Failed to encode response.create frame")
}

/// Derive the Responses WebSocket URL from an HTTP base URL.
pub(crate) fn websocket_url(base_url: &str) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    let rest = if base
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        format!("wss://{}", &base[8..])
    } else if base
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
    {
        format!("ws://{}", &base[7..])
    } else {
        return None;
    };
    Some(format!("{rest}/responses"))
}

/// Whether the base URL targets the official OpenAI API host over HTTPS.
pub(crate) fn is_official_openai_host(base_url: &str) -> bool {
    let trimmed = base_url.trim();
    if !trimmed
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return false;
    }
    let without_scheme = &trimmed[8..];
    let authority = without_scheme.split('/').next().unwrap_or_default();
    let host = authority.split(':').next().unwrap_or_default();
    host.eq_ignore_ascii_case("api.openai.com")
}
