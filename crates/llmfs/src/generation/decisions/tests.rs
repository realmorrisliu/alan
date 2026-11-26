use std::collections::BTreeMap;

use alan_llm::{
    CalibrationKind, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
};

use super::*;

#[tokio::test]
async fn abort_before_publication_discards_returned_decision_and_usage() {
    let connection = Arc::new(Connection::new(
        "test".into(),
        None,
        None,
        crate::provider_catalog::provider_capabilities_for_name("test"),
        ConnectionLimits::default(),
        Box::new(alan_llm::mock::MockLlmProvider::new()),
    ));
    let operation = Arc::new(Generation::new(
        connection,
        "test".into(),
        0,
        OperationKind::Decision,
    ));
    assert!(operation.claim(GenStatus::Running));
    let request = DecisionRequest::new(
        serde_json::json!({"tool": "bash"}),
        BTreeMap::from([(
            "safe".to_string(),
            DecisionQuestion::Predicate {
                prompt: "Is it safe?".to_string(),
            },
        )]),
    );
    // Hold publication. Tokio's FIFO mutex queues abort before the returned result.
    let guard = operation.finalize.lock().await;
    let mut abort = Box::pin(abort_generation(&operation));
    std::future::poll_fn(|cx| {
        assert!(abort.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let mut publication = Box::pin(publish_result(
        &operation,
        &request,
        DecisionResponse {
            model: "jev-1.13.0".to_string(),
            provider: "typesafe".to_string(),
            calibration: CalibrationKind::Calibrated,
            answers: BTreeMap::from([(
                "safe".to_string(),
                DecisionAnswer::Predicate {
                    yes_probability: 0.95,
                    confidence: 0.95,
                },
            )]),
            usage: Some(TokenUsage {
                prompt_tokens: 40,
                cached_prompt_tokens: None,
                completion_tokens: 0,
                total_tokens: 40,
                reasoning_tokens: None,
                ..Default::default()
            }),
        },
    ));
    std::future::poll_fn(|cx| {
        assert!(publication.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(guard);
    let (aborted, ()) = tokio::join!(abort, publication);
    aborted.unwrap();
    assert!(operation.status() == GenStatus::Aborted);
    assert!(operation.token_usage().is_none());
    assert_eq!(operation.connection.total_tokens.load(Ordering::Relaxed), 0);
    let bytes = operation.events.read(0, 65536).await;
    let event: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(event, serde_json::json!({"version":1,"aborted":true}));
}
