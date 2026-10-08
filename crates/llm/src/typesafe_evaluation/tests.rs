use super::*;
use crate::EvaluationCandidate;
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::net::TcpListener;

fn request() -> ChoiceEvaluationRequest {
    ChoiceEvaluationRequest {
        input: "  original $(touch never)\n".into(),
        candidates: vec![EvaluationCandidate {
            id: "none".into(),
            description: "Caller-owned candidate, not adapter abstention".into(),
        }],
    }
}
fn response() -> serde_json::Value {
    serde_json::json!({"model":"jev-1.13.0","answers":{"selection":{
        "type":"choice","choice":"choice_0","probabilities":{"choice_0":1.0,"none":0.0},"confidence":1.0
    }},"usage":{"input_tokens":100,"output_tokens":5}})
}
#[test]
fn typed_response_validation_preserves_identity_and_rejects_bad_evidence() {
    let decode_value = |value: &serde_json::Value| {
        decode(
            &serde_json::to_vec(value).unwrap(),
            "jev-1.13.0",
            &request(),
        )
    };
    let good = response();
    let duplicate = serde_json::to_string(&good)
        .unwrap()
        .replace("\"choice_0\":1.0", "\"choice_0\":1.0,\"choice_0\":1.0");
    assert!(decode(duplicate.as_bytes(), "jev-1.13.0", &request()).is_err());
    let result = decode_value(&good).unwrap();
    assert_eq!(
        result.selection,
        EvaluationSelection::Selected("none".into())
    );
    assert_eq!(result.usage.unwrap().total_tokens, 105);
    let mut no_match = good.clone();
    no_match["answers"]["selection"]["choice"] = "none".into();
    no_match["answers"]["selection"]["probabilities"] =
        serde_json::json!({"choice_0":0.0,"none":1.0});
    assert_eq!(
        decode_value(&no_match).unwrap().selection,
        EvaluationSelection::NoMatch
    );
    for (pointer, value) in [
        ("/model", serde_json::json!("jev-unknown")),
        ("/answers/selection/type", serde_json::json!("noul")),
        ("/answers/selection/choice", serde_json::json!("command")),
        ("/answers/selection/choice", serde_json::json!("none")),
        ("/answers/selection/confidence", serde_json::json!(1.1)),
        (
            "/answers/selection/probabilities",
            serde_json::json!({"choice_0":0.5,"none":0.0}),
        ),
        (
            "/answers/selection/probabilities",
            serde_json::json!({"choice_0":1.0,"foreign":0.0}),
        ),
        ("/usage/input_tokens", serde_json::json!(-1)),
        ("/usage/input_tokens", serde_json::json!(i32::MAX)),
    ] {
        let mut malformed = good.clone();
        *malformed.pointer_mut(pointer).unwrap() = value;
        assert!(decode_value(&malformed).is_err(), "accepted {pointer}");
    }
    assert!(TypesafeEvaluationClient::new("test".into(), "jev-latest".into()).is_err());
    assert!(TypesafeEvaluationClient::new("".into(), "jev-1.13.0".into()).is_err());
}

#[test]
fn rounded_provider_probabilities_preserve_selection_and_usage() {
    let request = ChoiceEvaluationRequest {
        input: "diagnostic only".into(),
        candidates: (0..3)
            .map(|i| EvaluationCandidate {
                id: format!("candidate_{i}"),
                description: "finite choice".into(),
            })
            .collect(),
    };
    // Real jev-1.13.0 response: individually rounded probabilities sum to 0.99.
    let mut value = serde_json::json!({"model":"jev-1.13.0","answers":{"selection":{
        "type":"choice","choice":"choice_1","confidence":0.91,
        "probabilities":{"choice_0":0.03,"choice_2":0.01,"choice_1":0.93,"none":0.02}
    }},"usage":{"input_tokens":464,"output_tokens":53}});
    for (probability, valid) in [
        (0.03, true),
        (0.05, true),
        (0.01, false),
        (0.07, false),
        (0.0301, false),
    ] {
        value["answers"]["selection"]["probabilities"]["choice_0"] = probability.into();
        let result = decode(&serde_json::to_vec(&value).unwrap(), "jev-1.13.0", &request);
        if valid {
            let result = result.unwrap();
            assert_eq!(
                result.selection,
                EvaluationSelection::Selected("candidate_1".into())
            );
            assert_eq!(result.usage.unwrap().total_tokens, 517);
        } else {
            assert!(
                result.is_err(),
                "accepted impossible or unrounded distribution"
            );
        }
    }
}

#[tokio::test]
async fn http_posts_typed_original_input_once_and_never_generates() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/systemone",
                post(
                    move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
                        let count = count.clone();
                        async move {
                            count.fetch_add(1, Ordering::SeqCst);
                            assert_eq!(headers["authorization"], "Bearer private-test-key");
                            assert_eq!(body["state"], request().input);
                            assert_eq!(body["questions"]["selection"]["type"], "choice");
                            assert_eq!(
                                body["questions"]["selection"]["criteria"]["choice_0"]["id"],
                                "none"
                            );
                            assert!(body.get("messages").is_none());
                            Json(response())
                        }
                    },
                ),
            ),
        )
        .await
        .unwrap();
    });
    let mut client =
        TypesafeEvaluationClient::new("private-test-key".into(), "jev-1.13.0".into()).unwrap();
    client.endpoint = endpoint;
    assert!(!client.supports_generation());
    assert!(client.supports_choice_evaluation());
    assert!(client.chat(None, "never send").await.is_err());
    let result = client.evaluate_choice(request()).await.unwrap();
    assert_eq!(
        result.selection,
        EvaluationSelection::Selected("none".into())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn http_failure_is_single_attempt_bounded_and_secret_free() {
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::TEMPORARY_REDIRECT,
        StatusCode::OK,
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/v1/systemone",
                    post(move || {
                        let count = count.clone();
                        async move {
                            count.fetch_add(1, Ordering::SeqCst);
                            (
                                status,
                                [("location", "/v1/systemone")],
                                if status == StatusCode::OK {
                                    "x".repeat(MAX_RESPONSE_BYTES + 1)
                                } else {
                                    "private-test-key body must not appear".into()
                                },
                            )
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let mut client =
            TypesafeEvaluationClient::new("private-test-key".into(), "jev-1.13.0".into()).unwrap();
        client.endpoint = endpoint;
        let error = client.evaluate_choice(request()).await.unwrap_err();
        assert!(!format!("{error:#}").contains("private-test-key"));
        assert_eq!(
            error.is::<crate::MalformedEvaluationResponse>(),
            status == StatusCode::OK
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        server.abort();
    }
}

#[tokio::test]
async fn http_invalid_evidence_retains_the_malformed_category() {
    let mut wrong_model = response();
    wrong_model["model"] = "jev-unknown".into();
    let mut wrong_distribution = response();
    wrong_distribution["answers"]["selection"]["probabilities"]["choice_0"] = 0.5.into();
    for body in [
        "private-test-key invalid JSON".into(),
        wrong_model.to_string(),
        wrong_distribution.to_string(),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/v1/systemone",
                    post(move || {
                        count.fetch_add(1, Ordering::SeqCst);
                        let body = body.clone();
                        async move { body }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let mut client =
            TypesafeEvaluationClient::new("private-test-key".into(), "jev-1.13.0".into()).unwrap();
        client.endpoint = endpoint;
        let error = client.evaluate_choice(request()).await.unwrap_err();
        assert!(error.is::<crate::MalformedEvaluationResponse>());
        assert!(!format!("{error:#}").contains("private-test-key"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        server.abort();
    }
}

struct SilentBody {
    polled: Arc<tokio::sync::Notify>,
    dropped: Arc<tokio::sync::Notify>,
}
impl futures::Stream for SilentBody {
    type Item = Result<Vec<u8>, std::io::Error>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.polled.notify_one();
        std::task::Poll::Pending
    }
}
impl Drop for SilentBody {
    fn drop(&mut self) {
        self.dropped.notify_one();
    }
}

#[tokio::test]
async fn cancellation_drops_silent_http_body_without_a_second_attempt() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
    let polled = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(tokio::sync::Notify::new());
    let server_polled = polled.clone();
    let server_dropped = dropped.clone();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/v1/systemone",
                post(move || {
                    let body = SilentBody {
                        polled: server_polled.clone(),
                        dropped: server_dropped.clone(),
                    };
                    async { axum::body::Body::from_stream(body) }
                }),
            ),
        )
        .await
        .unwrap();
    });
    let mut client =
        TypesafeEvaluationClient::new("private-test-key".into(), "jev-1.13.0".into()).unwrap();
    client.endpoint = endpoint;
    let task = tokio::spawn(async move { client.evaluate_choice(request()).await });
    tokio::time::timeout(Duration::from_secs(2), polled.notified())
        .await
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    let closed = tokio::time::timeout(Duration::from_secs(2), dropped.notified()).await;
    server.abort();
    closed.expect("cancelled evaluation retained silent HTTP body");
}

#[tokio::test]
#[ignore = "requires explicitly supplied TYPESAFE_API_KEY; one live capability probe"]
async fn live_typesafe_choice_probe() {
    let key = std::env::var("TYPESAFE_API_KEY").expect("TYPESAFE_API_KEY required");
    let mut client = TypesafeEvaluationClient::new(key, "jev-1.13.0".into()).unwrap();
    let started = std::time::Instant::now();
    let result = client
        .evaluate_choice(ChoiceEvaluationRequest {
            input: "fn main() { println!(\"hello\"); }".into(),
            candidates: vec![
                EvaluationCandidate {
                    id: "rust".into(),
                    description: "Source code written in Rust".into(),
                },
                EvaluationCandidate {
                    id: "python".into(),
                    description: "Source code written in Python".into(),
                },
            ],
        })
        .await
        .expect("live TypeSafe Choice probe failed");
    assert_eq!(
        result.selection,
        EvaluationSelection::Selected("rust".into())
    );
    assert!(result.usage.is_some());
    eprintln!(
        "TypeSafe capability probe: model=jev-1.13.0 selection=rust elapsed_ms={} usage={:?}; not routing qualification",
        started.elapsed().as_millis(),
        result.usage
    );
}
