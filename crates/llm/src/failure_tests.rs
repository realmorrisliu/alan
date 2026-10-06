use super::*;

#[tokio::test]
async fn receiver_abort_closes_pending_upstream() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let output = guard_stream(input, status);
    drop(output);
    tokio::time::timeout(std::time::Duration::from_secs(1), tx.closed())
        .await
        .unwrap();
    drop(status_tx);
}

#[test]
fn diagnostic_never_formats_unknown_sources() {
    let error = anyhow::anyhow!("secret-token https://private.example account=private");
    assert_eq!(safe_failure_reason(&error), "stream_error:unknown");
    assert_eq!(
        safe_finish_reason("stream_error:secret-token"),
        "stream_error:unknown"
    );
    assert_eq!(safe_finish_reason("stop"), "stop");
    assert_eq!(safe_finish_reason("refusal"), "stream_error:safety");
}

#[tokio::test]
async fn failure_before_payload_emits_one_terminal() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let mut output = guard_stream(input, status);
    drop(tx);
    status_tx.send(Some("stream_error:authentication")).unwrap();
    let terminal = output.recv().await.unwrap();
    assert!(terminal.is_finished);
    assert_eq!(
        terminal.finish_reason.as_deref(),
        Some("stream_error:authentication")
    );
    assert!(output.recv().await.is_none());
}

#[tokio::test]
async fn partial_payload_is_preserved_before_failure() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let mut output = guard_stream(input, status);
    let mut payload = failure_chunk("stop");
    payload.is_finished = false;
    payload.finish_reason = None;
    payload.text = Some("partial".into());
    tx.send(payload).await.unwrap();
    drop(tx);
    status_tx.send(Some("stream_error:unavailable")).unwrap();
    assert_eq!(
        output.recv().await.unwrap().text.as_deref(),
        Some("partial")
    );
    assert_eq!(
        output.recv().await.unwrap().finish_reason.as_deref(),
        Some("stream_error:unavailable")
    );
    assert!(output.recv().await.is_none());
}

#[tokio::test]
async fn normal_terminal_wins_over_late_failure() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let mut output = guard_stream(input, status);
    tx.send(failure_chunk("stop")).await.unwrap();
    tx.send(failure_chunk("stream_error:unknown"))
        .await
        .unwrap();
    drop(tx);
    let _ = status_tx.send(Some("stream_error:unavailable"));
    assert_eq!(
        output.recv().await.unwrap().finish_reason.as_deref(),
        Some("stop")
    );
    assert!(output.recv().await.is_none());
}

#[tokio::test]
async fn closed_terminal_reconciles_transport_status() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let mut output = guard_stream(input, status);
    tx.send(failure_chunk("stream_error:closed")).await.unwrap();
    drop(tx);
    status_tx.send(Some("stream_error:rate_limit")).unwrap();
    assert_eq!(
        output.recv().await.unwrap().finish_reason.as_deref(),
        Some("stream_error:rate_limit")
    );
    assert!(output.recv().await.is_none());
}

#[tokio::test]
async fn cancelled_status_and_empty_stream_are_closed_failure() {
    let (tx, input) = mpsc::channel(2);
    let (status_tx, status) = oneshot::channel();
    let mut output = guard_stream(input, status);
    drop(tx);
    drop(status_tx);
    assert_eq!(
        output.recv().await.unwrap().finish_reason.as_deref(),
        Some("stream_error:closed")
    );
    assert!(output.recv().await.is_none());
}

#[tokio::test]
async fn reqwest_response_body_failure_retains_typed_category() {
    use futures::StreamExt;

    // Exercise reqwest's response byte-stream error conversion without a socket.
    // HTTP body transport errors are represented as Decode, not necessarily Body.
    let body = reqwest::Body::wrap_stream(futures::stream::iter([Err::<Vec<u8>, _>(
        std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "secret-token https://private.example account=private",
        ),
    )]));
    let response = reqwest::Response::from(
        axum::http::Response::builder()
            .status(200)
            .body(reqwest::Body::wrap(body))
            .unwrap(),
    );
    let error = response.bytes_stream().next().await.unwrap().unwrap_err();
    assert!(
        error.is_decode(),
        "response bytes use the typed Decode kind"
    );
    assert!(
        !error.is_body(),
        "Decode is distinct from request Body kind"
    );
    let error = anyhow::Error::from(error).context("bounded response context");
    assert_eq!(safe_failure_reason(&error), "stream_error:body");

    // A Decode error can also lack a nested reqwest Body error entirely.
    // Exercise that public typed constructor path, without inspecting messages.
    let decode_only = reqwest::Response::from(
        axum::http::Response::builder()
            .status(200)
            .body("not-json secret-token")
            .unwrap(),
    )
    .json::<serde_json::Value>()
    .await
    .unwrap_err();
    assert!(decode_only.is_decode());
    assert!(!decode_only.is_body());
    assert_eq!(
        safe_failure_reason(&decode_only.into()),
        "stream_error:body"
    );

    for with_payload in [false, true] {
        let (tx, input) = mpsc::channel(2);
        let (status_tx, status) = oneshot::channel();
        let mut output = guard_stream(input, status);
        if with_payload {
            let mut payload = failure_chunk("stop");
            payload.is_finished = false;
            payload.finish_reason = None;
            payload.text = Some("partial".into());
            tx.send(payload).await.unwrap();
        }
        drop(tx);
        status_tx.send(Some(safe_failure_reason(&error))).unwrap();
        if with_payload {
            assert_eq!(
                output.recv().await.unwrap().text.as_deref(),
                Some("partial")
            );
        }
        let terminal = output.recv().await.unwrap();
        assert!(terminal.is_finished);
        assert_eq!(terminal.finish_reason.as_deref(), Some("stream_error:body"));
        assert!(terminal.text.is_none());
        assert!(output.recv().await.is_none(), "exactly one safe terminal");
    }
}
