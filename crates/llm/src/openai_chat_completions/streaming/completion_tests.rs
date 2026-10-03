use super::*;

const DELTA: &str = "data: {\"id\":\"test\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"test\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":\"stop\"}]}\n\n";

#[tokio::test]
async fn completion_owner_rejects_eof_and_malformed_after_finish_delta() {
    for prefix in ["", DELTA] {
        for suffix in ["", "data: secret-token malformed\n\n"] {
            let (tx, mut rx) = tokio::sync::mpsc::channel(10);
            let bytes = format!("{prefix}{suffix}").into_bytes();
            let outcome = consume_chat_bytes(futures::stream::iter([Ok(bytes)]), tx).await;
            let error = outcome.expect_err("EOF/parser failure must not become success");
            assert_eq!(
                crate::safe_failure_reason(&error),
                if suffix.is_empty() {
                    "stream_error:closed"
                } else {
                    "stream_error:parse"
                }
            );
            if !prefix.is_empty() {
                assert_eq!(
                    rx.recv().await.unwrap().choices[0].delta.content.as_deref(),
                    Some("partial")
                );
            }
            assert!(rx.recv().await.is_none());
        }
    }
}

#[tokio::test]
async fn completion_owner_done_and_receiver_cancel() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    consume_chat_bytes(
        futures::stream::iter([Ok(format!("{DELTA}data: [DONE]\n\n").into_bytes())]),
        tx,
    )
    .await
    .unwrap();
    assert!(rx.recv().await.is_some());
    assert!(rx.recv().await.is_none());
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx);
    consume_chat_bytes(futures::stream::iter([Ok(DELTA.as_bytes().to_vec())]), tx)
        .await
        .unwrap();
}

#[tokio::test]
async fn completion_owner_cancel_pending_transport() {
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    drop(rx);
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        consume_chat_bytes(futures::stream::pending(), tx),
    )
    .await
    .unwrap()
    .unwrap();
}
