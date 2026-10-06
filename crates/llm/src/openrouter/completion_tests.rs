use super::*;

const DELTA: &str = "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n";

#[tokio::test]
async fn router_completion_owner_eof_and_malformed_emit_one_error() {
    for prefix in ["", DELTA] {
        for suffix in ["", "data: secret-token malformed\n\n"] {
            let (tx, rx) = mpsc::channel(10);
            let (status_tx, status) = tokio::sync::oneshot::channel();
            let mut rx = crate::failure::guard_stream(rx, status);
            let outcome = consume_router_bytes(
                futures::stream::iter([Ok(format!("{prefix}{suffix}").into_bytes())]),
                tx,
            )
            .await;
            status_tx
                .send(outcome.err().as_ref().map(crate::safe_failure_reason))
                .unwrap();
            let mut text = String::new();
            let mut terminals = Vec::new();
            while let Some(chunk) = rx.recv().await {
                text.push_str(chunk.text.as_deref().unwrap_or_default());
                if chunk.is_finished {
                    terminals.push(chunk);
                }
            }
            assert_eq!(text, if prefix.is_empty() { "" } else { "partial" });
            assert_eq!(terminals.len(), 1, "exactly one failure terminal");
            assert_eq!(
                terminals[0].finish_reason.as_deref(),
                Some(if suffix.is_empty() {
                    "stream_error:closed"
                } else {
                    "stream_error:parse"
                })
            );
        }
    }
}

#[tokio::test]
async fn router_completion_owner_done_is_success() {
    let (tx, mut rx) = mpsc::channel(10);
    consume_router_bytes(
        futures::stream::iter([Ok(format!("{DELTA}data: [DONE]\n\n").into_bytes())]),
        tx,
    )
    .await
    .unwrap();
    assert_eq!(rx.recv().await.unwrap().text.as_deref(), Some("partial"));
    assert!(rx.recv().await.unwrap().is_finished);
    assert!(rx.recv().await.is_none());
}

#[tokio::test]
async fn router_completion_owner_cancel_pending_transport() {
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        consume_router_bytes(futures::stream::pending(), tx),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn router_completion_owner_typed_error_survives_guard() {
    // Typed HTTP status generated without sockets; native body-error is in LLMFS HTTP fixture.
    let response = reqwest::Response::from(
        axum::http::Response::builder()
            .status(429)
            .body("secret-token")
            .unwrap(),
    );
    let error = response.error_for_status().unwrap_err();
    let (tx, rx) = mpsc::channel(10);
    let (status_tx, status) = tokio::sync::oneshot::channel();
    let mut output = crate::failure::guard_stream(rx, status);
    let result = consume_router_bytes(
        futures::stream::iter([Ok(DELTA.as_bytes().to_vec()), Err(error.into())]),
        tx,
    )
    .await;
    status_tx
        .send(result.err().as_ref().map(crate::safe_failure_reason))
        .unwrap();
    assert_eq!(
        output.recv().await.unwrap().text.as_deref(),
        Some("partial")
    );
    assert_eq!(
        output.recv().await.unwrap().finish_reason.as_deref(),
        Some("stream_error:rate_limit")
    );
    assert!(output.recv().await.is_none());
}
