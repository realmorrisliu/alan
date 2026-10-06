use crate::StreamChunk;
use alan_auth::{ChatgptAuthError, ChatgptAuthErrorKind};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
pub(crate) struct StreamClosed;
impl std::fmt::Display for StreamClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("provider stream ended before protocol completion")
    }
}
impl std::error::Error for StreamClosed {}

/// Bounded provider diagnostic. Never formats an error or any of its sources.
pub fn safe_failure_reason(error: &anyhow::Error) -> &'static str {
    for source in error.chain() {
        if source.is::<StreamClosed>() {
            return "stream_error:closed";
        }
        if source.is::<serde_json::Error>() {
            return "stream_error:parse";
        }
        if let Some(auth) = source.downcast_ref::<ChatgptAuthError>() {
            match auth.kind() {
                Some(
                    ChatgptAuthErrorKind::NotLoggedIn
                    | ChatgptAuthErrorKind::TokenExpired
                    | ChatgptAuthErrorKind::UnauthorizedAfterRefresh,
                ) => return "stream_error:authentication",
                Some(_) => return "stream_error:authentication",
                None => {}
            }
        }
        if let Some(http) = source.downcast_ref::<reqwest::Error>() {
            if http.is_timeout() {
                return "stream_error:timeout";
            }
            if http.is_connect() {
                return "stream_error:connect";
            }
            // Reqwest reports response-byte transport failures (including a
            // truncated Content-Length) as Decode, distinct from request Body.
            if http.is_body() || http.is_decode() {
                return "stream_error:body";
            }
            if let Some(status) = http.status() {
                return match status.as_u16() {
                    401 | 403 => "stream_error:authentication",
                    408 => "stream_error:timeout",
                    429 => "stream_error:rate_limit",
                    500..=599 => "stream_error:unavailable",
                    _ => "stream_error:http",
                };
            }
        }
    }
    "stream_error:unknown"
}

/// Only known protocol reasons may enter a namespace projection.
pub fn safe_finish_reason(reason: &str) -> &'static str {
    match reason {
        "stop" => "stop",
        "end_turn" => "end_turn",
        "tool_calls" => "tool_calls",
        "tool_use" => "tool_use",
        "length" => "length",
        "max_tokens" => "max_tokens",
        "stop_sequence" => "stop_sequence",
        "completed" => "completed",
        "content_filter" => "content_filter",
        "STOP" => "STOP",
        "MAX_TOKENS" => "MAX_TOKENS",
        "stream_error:authentication" => "stream_error:authentication",
        "stream_error:rate_limit" => "stream_error:rate_limit",
        "stream_error:unavailable" => "stream_error:unavailable",
        "stream_error:timeout" => "stream_error:timeout",
        "stream_error:connect" => "stream_error:connect",
        "stream_error:body" => "stream_error:body",
        "stream_error:parse" => "stream_error:parse",
        "stream_error:http" => "stream_error:http",
        "refusal" | "stream_error:safety" | "stream_error:prompt_blocked:safety" => {
            "stream_error:safety"
        }
        "stream_error:recitation" | "stream_error:prompt_blocked:recitation" => {
            "stream_error:recitation"
        }
        "stream_error:closed" | "stream_closed" => "stream_error:closed",
        "stream_error" => "stream_error",
        _ => "stream_error:unknown",
    }
}

pub(crate) fn failure_chunk(reason: &'static str) -> StreamChunk {
    StreamChunk {
        text: None,
        thinking: None,
        thinking_signature: None,
        redacted_thinking: None,
        usage: None,
        sequence_number: None,
        tool_call_delta: None,
        is_finished: true,
        finish_reason: Some(reason.into()),
        provider_response_id: None,
        provider_response_status: None,
    }
}

// One owner forwards payload and emits the sole terminal. Receiver drop cancels
// delivery; an already-observed normal terminal wins over later transport close.
pub(crate) fn guard_stream(
    mut input: mpsc::Receiver<StreamChunk>,
    status: oneshot::Receiver<Option<&'static str>>,
) -> mpsc::Receiver<StreamChunk> {
    let (tx, rx) = mpsc::channel(100);
    tokio::spawn(async move {
        loop {
            let chunk = tokio::select! {
                _ = tx.closed() => return,
                chunk = input.recv() => match chunk { Some(chunk) => chunk, None => break },
            };
            let mut chunk = chunk;
            if let Some(reason) = chunk.finish_reason.as_deref() {
                chunk.finish_reason = Some(safe_finish_reason(reason).into());
            }
            let finished = chunk.is_finished;
            if finished && matches!(chunk.finish_reason.as_deref(), Some("stream_error:closed")) {
                let reason = tokio::select! {
                    _ = tx.closed() => return,
                    result = status => result.ok().flatten().unwrap_or("stream_error:closed"),
                };
                chunk.finish_reason = Some(reason.into());
                let _ = tx.send(chunk).await;
                return;
            }
            if tx.send(chunk).await.is_err() || finished {
                return;
            }
        }
        let reason = tokio::select! {
            _ = tx.closed() => return,
            result = status => result.ok().flatten().unwrap_or("stream_error:closed"),
        };
        let _ = tx.send(failure_chunk(reason)).await;
    });
    rx
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
