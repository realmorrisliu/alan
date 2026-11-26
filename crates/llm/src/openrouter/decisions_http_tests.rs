use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::openrouter::{OpenRouterClient, OpenRouterClientMetadata};
use crate::{
    CalibrationKind, ChoiceEvaluationRequest, DecisionAnswer, DecisionLevel, DecisionOption,
    DecisionQuestion, DecisionRequest, EvaluationCandidate, EvaluationSelection, LlmProvider,
    MalformedDecisionResponse,
};

#[tokio::test]
async fn openrouter_decide_posts_to_alpha_decisions_and_validates_calibrated_batch() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let recorded_body = Arc::new(Mutex::new(Value::Null));
    let recorded_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));

    let calls_srv = calls.clone();
    let body_srv = recorded_body.clone();
    let headers_srv = recorded_headers.clone();

    let server = tokio::spawn(async move {
        let app = Router::new().route(
            "/api/alpha/decisions",
            post(move |headers: HeaderMap, Json(body): Json<Value>| {
                let calls_srv = calls_srv.clone();
                let body_srv = body_srv.clone();
                let headers_srv = headers_srv.clone();
                async move {
                    calls_srv.fetch_add(1, Ordering::SeqCst);
                    *body_srv.lock().unwrap() = body;
                    *headers_srv.lock().unwrap() = headers
                        .iter()
                        .filter_map(|(k, v)| {
                            v.to_str()
                                .ok()
                                .map(|val| (k.as_str().to_string(), val.to_string()))
                        })
                        .collect();
                    Json(json!({
                        "id": "dec-or-1",
                        "model": "typesafe/jev-1.13",
                        "provider": "typesafe",
                        "answers": {
                            "safe": { "type": "noul", "noul": 0.92 },
                            "action": {
                                "type": "choice",
                                "choice": "allow",
                                "probabilities": { "allow": 0.91, "deny": 0.09 },
                                "confidence": 0.91
                            },
                            "risk": {
                                "type": "score",
                                "score": 0.1,
                                "probabilities": { "low": 0.9, "high": 0.1 },
                                "confidence": 0.9
                            }
                        },
                        "usage": {
                            "input_tokens": 144,
                            "output_tokens": 0,
                            "cost": 0.00025
                        }
                    }))
                }
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    let mut client = OpenRouterClient::with_metadata(
        "secret-openrouter-key",
        &format!("http://{addr}/api/v1"),
        "typesafe/jev-1.13",
        OpenRouterClientMetadata {
            http_referer: Some("https://alanworks.app".to_string()),
            x_title: Some("Alan".to_string()),
            app_categories: Some(vec!["cli-agent".to_string()]),
        },
    )
    .unwrap();

    let mut questions = BTreeMap::new();
    questions.insert(
        "safe".to_string(),
        DecisionQuestion::Predicate {
            prompt: "Is git status safe?".to_string(),
        },
    );
    questions.insert(
        "action".to_string(),
        DecisionQuestion::Choice {
            prompt: "Select policy.".to_string(),
            options: vec![
                DecisionOption {
                    id: "allow".to_string(),
                    description: "Permit".to_string(),
                },
                DecisionOption {
                    id: "deny".to_string(),
                    description: "Block".to_string(),
                },
            ],
        },
    );
    questions.insert(
        "risk".to_string(),
        DecisionQuestion::Score {
            prompt: "Score risk.".to_string(),
            levels: vec![
                DecisionLevel {
                    id: "low".to_string(),
                    description: "Low".to_string(),
                    score: 0.0,
                },
                DecisionLevel {
                    id: "high".to_string(),
                    description: "High".to_string(),
                    score: 1.0,
                },
            ],
        },
    );

    let req = DecisionRequest::new(json!({"cmd": "git status"}), questions);
    let resp = client.decide(req.clone()).await.unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(resp.calibration, CalibrationKind::Calibrated);
    assert_eq!(resp.provider, "typesafe");
    assert_eq!(resp.model, "typesafe/jev-1.13");
    assert_eq!(resp.usage.unwrap().prompt_tokens, 144);
    assert_eq!(resp.usage.unwrap().completion_tokens, 0);
    assert_eq!(resp.usage.unwrap().cost_usd, Some(0.00025));
    assert!(matches!(
        resp.answers["safe"],
        DecisionAnswer::Predicate {
            yes_probability,
            ..
        } if (yes_probability - 0.92).abs() < 1e-9
    ));

    let body = recorded_body.lock().unwrap().clone();
    assert_eq!(body["model"], json!("typesafe/jev-1.13"));
    assert_eq!(body["state"], json!({"cmd": "git status"}));
    assert_eq!(body["questions"]["safe"]["type"], json!("noul"));
    assert_eq!(body["questions"]["action"]["type"], json!("choice"));
    assert_eq!(body["questions"]["risk"]["type"], json!("score"));

    let headers = recorded_headers.lock().unwrap().clone();
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "authorization" && v == "Bearer secret-openrouter-key")
    );
    assert!(
        headers
            .iter()
            .any(|(k, v)| k == "x-openrouter-title" && v == "Alan")
    );

    server.abort();
}

#[tokio::test]
async fn openrouter_decide_rejects_malformed_payloads_and_keeps_http_errors_secret_free() {
    for (status, body, expect_malformed) in [
        (
            StatusCode::OK,
            "secret-openrouter-key not valid json".to_string(),
            true,
        ),
        (
            StatusCode::OK,
            json!({
                "model": "typesafe/jev-1.13",
                "answers": {
                    "selection": {
                        "type": "choice",
                        "choice": "injected_option",
                        "probabilities": { "choice_0": 0.1, "injected_option": 0.9 },
                        "confidence": 0.9
                    }
                },
                "usage": { "input_tokens": 10, "output_tokens": 0 }
            })
            .to_string(),
            true,
        ),
        (
            StatusCode::UNAUTHORIZED,
            "secret-openrouter-key unauthorized".to_string(),
            false,
        ),
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let app = Router::new().route(
                "/api/alpha/decisions",
                post(move || {
                    let body = body.clone();
                    async move {
                        (
                            status,
                            [(axum::http::header::CONTENT_TYPE, "application/json")],
                            body,
                        )
                            .into_response()
                    }
                }),
            );
            axum::serve(listener, app).await.unwrap();
        });

        let mut client = OpenRouterClient::with_params(
            "secret-openrouter-key",
            &format!("http://{addr}/api/v1"),
            "typesafe/jev-1.13",
        )
        .unwrap();

        let mut questions = BTreeMap::new();
        questions.insert(
            "selection".to_string(),
            DecisionQuestion::Choice {
                prompt: "Pick".to_string(),
                options: vec![
                    DecisionOption {
                        id: "choice_0".to_string(),
                        description: "A".to_string(),
                    },
                    DecisionOption {
                        id: "none".to_string(),
                        description: "None".to_string(),
                    },
                ],
            },
        );
        let req = DecisionRequest::new(json!("input"), questions);
        let err = client.decide(req).await.unwrap_err();
        assert!(!format!("{err:#}").contains("secret-openrouter-key"));
        assert_eq!(err.is::<MalformedDecisionResponse>(), expect_malformed);
        server.abort();
    }
}

#[tokio::test]
async fn openrouter_evaluate_choice_delegates_via_calibrated_decisions() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let app = Router::new().route(
            "/api/alpha/decisions",
            post(|| async move {
                Json(json!({
                    "model": "typesafe/jev-1.13",
                    "answers": {
                        "selection": {
                            "type": "choice",
                            "choice": "choice_0",
                            "probabilities": { "choice_0": 0.94, "none": 0.06 },
                            "confidence": 0.94
                        }
                    },
                    "usage": { "input_tokens": 32, "output_tokens": 0 }
                }))
            }),
        );
        axum::serve(listener, app).await.unwrap();
    });

    let mut client = OpenRouterClient::with_params(
        "secret-openrouter-key",
        &format!("http://{addr}/api/v1"),
        "typesafe/jev-1.13",
    )
    .unwrap();

    let eval = client
        .evaluate_choice(ChoiceEvaluationRequest {
            input: "cargo test".to_string(),
            candidates: vec![EvaluationCandidate {
                id: "none".to_string(),
                description: "Candidate literally named none".to_string(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(eval.selection, EvaluationSelection::Selected("none".into()));
    server.abort();
}
