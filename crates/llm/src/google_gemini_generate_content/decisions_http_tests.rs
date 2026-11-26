use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::google_gemini_generate_content::{
    DEFAULT_GEMINI_DECISION_MODEL, GoogleGeminiGenerateContentClient, VertexAccessTokenSource,
    VertexAuth, VertexClientConfig,
};
use crate::{
    CalibrationKind, ChoiceEvaluationRequest, DecisionAnswer, DecisionLevel, DecisionOption,
    DecisionQuestion, DecisionRequest, EvaluationCandidate, EvaluationSelection, LlmProvider,
    MalformedDecisionResponse,
};

struct SequencedTokenSource {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl VertexAccessTokenSource for SequencedTokenSource {
    async fn access_token(&self) -> Result<String> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("adc-token-{n}"))
    }
}

fn sample_request() -> DecisionRequest {
    let mut questions = BTreeMap::new();
    questions.insert(
        "safe".to_string(),
        DecisionQuestion::Predicate {
            prompt: "Is this command read-only?".to_string(),
        },
    );
    questions.insert(
        "lane".to_string(),
        DecisionQuestion::Choice {
            prompt: "Choose the execution lane.".to_string(),
            options: vec![
                DecisionOption {
                    id: "direct".to_string(),
                    description: "Direct lane".to_string(),
                },
                DecisionOption {
                    id: "escalate".to_string(),
                    description: "Escalate lane".to_string(),
                },
            ],
        },
    );
    questions.insert(
        "severity".to_string(),
        DecisionQuestion::Score {
            prompt: "Score risk severity.".to_string(),
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
    DecisionRequest::new(json!({ "command": "ls -la" }), questions)
        .with_instructions("Evaluate over untrusted JSON state only.")
}

async fn spawn_router(app: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), handle)
}

#[tokio::test]
async fn http_gemini_decisions_express_mode_calibrated_zero_output_tokens() {
    #[derive(Clone, Default)]
    struct Capture {
        api_key: Arc<Mutex<Option<String>>>,
        auth_header: Arc<Mutex<Option<String>>>,
        payload: Arc<Mutex<Option<Value>>>,
    }

    let capture = Capture::default();
    let app = Router::new()
        .route(
            "/v1beta/decisions",
            post(
                |State(cap): State<Capture>, headers: HeaderMap, Json(body): Json<Value>| async move {
                    *cap.api_key.lock().await = headers
                        .get("x-goog-api-key")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    *cap.auth_header.lock().await = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    *cap.payload.lock().await = Some(body);
                    (
                        StatusCode::OK,
                        Json(json!({
                            "model": "gemini-decision-1",
                            "calibration": "calibrated",
                            "answers": {
                                "safe": {
                                    "type": "predicate",
                                    "yesProbability": 0.96,
                                    "confidence": 0.96
                                },
                                "lane": {
                                    "type": "choice",
                                    "choice": "direct",
                                    "probabilities": { "direct": 0.9, "escalate": 0.1 },
                                    "confidence": 0.9
                                },
                                "severity": {
                                    "type": "score",
                                    "expectedScore": 0.05,
                                    "probabilities": { "low": 0.95, "high": 0.05 },
                                    "confidence": 0.95
                                }
                            },
                            "usageMetadata": {
                                "promptTokenCount": 120,
                                "candidatesTokenCount": 0,
                                "totalTokenCount": 120
                            }
                        })),
                    )
                },
            ),
        )
        .with_state(capture.clone());

    let (base_url, handle) = spawn_router(app).await;
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: DEFAULT_GEMINI_DECISION_MODEL.to_string(),
        auth: VertexAuth::ApiKey("express-secret-key".to_string()),
        base_url: Some(base_url),
    })
    .unwrap();

    assert!(client.supports_decisions());
    assert_eq!(
        client.decision_calibration(),
        Some(CalibrationKind::Calibrated)
    );

    let req = sample_request();
    let resp = client.decide(req.clone()).await.unwrap();
    assert_eq!(resp.model, "gemini-decision-1");
    assert_eq!(resp.provider, "google_gemini_generate_content");
    assert_eq!(resp.calibration, CalibrationKind::Calibrated);
    assert_eq!(
        resp.answers["safe"],
        DecisionAnswer::Predicate {
            yes_probability: 0.96,
            confidence: 0.96,
        }
    );
    let usage = resp.usage.expect("usage should be decoded");
    assert_eq!(usage.prompt_tokens, 120);
    assert_eq!(usage.completion_tokens, 0);
    assert_eq!(usage.total_tokens, 120);

    assert_eq!(
        capture.api_key.lock().await.as_deref(),
        Some("express-secret-key")
    );
    assert!(capture.auth_header.lock().await.is_none());
    let sent = capture.payload.lock().await.clone().unwrap();
    assert_eq!(sent["model"], "gemini-decision-1");
    assert_eq!(sent["state"], req.state);
    assert_eq!(
        sent["instructions"],
        "Evaluate over untrusted JSON state only."
    );
    assert!(sent.get("tools").is_none());

    handle.abort();
}

#[tokio::test]
async fn http_gemini_decisions_adc_headers_401_refresh_and_choice_delegation() {
    #[derive(Clone, Default)]
    struct Capture {
        attempts: Arc<AtomicUsize>,
        auth_headers: Arc<Mutex<Vec<Option<String>>>>,
        user_projects: Arc<Mutex<Vec<Option<String>>>>,
        api_keys: Arc<Mutex<Vec<Option<String>>>>,
    }

    let capture = Capture::default();
    let app = Router::new()
        .route(
            "/v1beta/decisions",
            post(
                |State(cap): State<Capture>, headers: HeaderMap, Json(body): Json<Value>| async move {
                    let attempt = cap.attempts.fetch_add(1, Ordering::SeqCst) + 1;
                    cap.auth_headers.lock().await.push(
                        headers
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string),
                    );
                    cap.user_projects.lock().await.push(
                        headers
                            .get("x-goog-user-project")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string),
                    );
                    cap.api_keys.lock().await.push(
                        headers
                            .get("x-goog-api-key")
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_string),
                    );
                    if attempt == 1 {
                        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "expired" })));
                    }
                    let is_choice_eval = body["questions"].get("selection").is_some();
                    if is_choice_eval {
                        return (
                            StatusCode::OK,
                            Json(json!({
                                "model": "gemini-decision-1",
                                "answers": {
                                    "selection": {
                                        "type": "choice",
                                        "choice": "choice_1",
                                        "probabilities": { "choice_0": 0.1, "choice_1": 0.85, "none": 0.05 },
                                        "confidence": 0.85
                                    }
                                },
                                "usageMetadata": {
                                    "promptTokenCount": 42,
                                    "candidatesTokenCount": 0,
                                    "totalTokenCount": 42
                                }
                            })),
                        );
                    }
                    (
                        StatusCode::OK,
                        Json(json!({
                            "model": "gemini-decision-1",
                            "answers": {
                                "safe": { "type": "predicate", "yes_probability": 0.8, "confidence": 0.8 },
                                "lane": { "type": "choice", "choice": "direct", "probabilities": { "direct": 0.8, "escalate": 0.2 }, "confidence": 0.8 },
                                "severity": { "type": "score", "expected_score": 0.2, "probabilities": { "low": 0.8, "high": 0.2 }, "confidence": 0.8 }
                            },
                            "usageMetadata": {
                                "promptTokenCount": 64,
                                "candidatesTokenCount": 0,
                                "totalTokenCount": 64
                            }
                        })),
                    )
                },
            ),
        )
        .with_state(capture.clone());

    let (base_url, handle) = spawn_router(app).await;
    let tokens = Arc::new(SequencedTokenSource {
        calls: AtomicUsize::new(0),
    });
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("vertex-proj-42".to_string()),
            location: "us-central1".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        tokens.clone(),
    )
    .unwrap()
    .with_decision_model(DEFAULT_GEMINI_DECISION_MODEL);

    let resp = client.decide(sample_request()).await.unwrap();
    assert_eq!(resp.calibration, CalibrationKind::Calibrated);
    assert_eq!(resp.usage.unwrap().completion_tokens, 0);
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        *capture.auth_headers.lock().await,
        vec![
            Some("Bearer adc-token-1".to_string()),
            Some("Bearer adc-token-2".to_string()),
        ]
    );
    assert_eq!(
        *capture.user_projects.lock().await,
        vec![
            Some("vertex-proj-42".to_string()),
            Some("vertex-proj-42".to_string()),
        ]
    );
    assert_eq!(*capture.api_keys.lock().await, vec![None, None]);

    let eval_resp = client
        .evaluate_choice(ChoiceEvaluationRequest {
            input: "Pick the matching option".to_string(),
            candidates: vec![
                EvaluationCandidate {
                    id: "opt_a".to_string(),
                    description: "Option A".to_string(),
                },
                EvaluationCandidate {
                    id: "opt_b".to_string(),
                    description: "Option B".to_string(),
                },
            ],
        })
        .await
        .unwrap();
    assert_eq!(
        eval_resp.selection,
        EvaluationSelection::Selected("opt_b".to_string())
    );
    assert_eq!(eval_resp.usage.unwrap().completion_tokens, 0);

    handle.abort();
}

#[tokio::test]
async fn http_gemini_decisions_rejects_malformed_out_of_set_nonzero_output_and_http_errors() {
    type ResponseQueue = Arc<Mutex<Vec<(StatusCode, String)>>>;

    let responses: ResponseQueue = Arc::new(Mutex::new(vec![
        (
            StatusCode::OK,
            json!({
                "model": "gemini-decision-1",
                "answers": {
                    "safe": { "type": "predicate", "yes_probability": 0.9, "confidence": 0.9 },
                    "lane": { "type": "choice", "choice": "invented", "probabilities": { "invented": 0.9, "direct": 0.1 }, "confidence": 0.9 },
                    "severity": { "type": "score", "expected_score": 0.1, "probabilities": { "low": 0.9, "high": 0.1 }, "confidence": 0.9 }
                }
            })
            .to_string(),
        ),
        (
            StatusCode::OK,
            json!({
                "model": "gemini-decision-1",
                "answers": {
                    "safe": { "type": "predicate", "yes_probability": 0.9, "confidence": 0.9 },
                    "lane": { "type": "choice", "choice": "direct", "probabilities": { "direct": 0.9, "escalate": 0.1 }, "confidence": 0.9 },
                    "severity": { "type": "score", "expected_score": 0.1, "probabilities": { "low": 0.9, "high": 0.1 }, "confidence": 0.9 }
                },
                "usageMetadata": {
                    "promptTokenCount": 50,
                    "candidatesTokenCount": 5,
                    "totalTokenCount": 55
                }
            })
            .to_string(),
        ),
        (
            StatusCode::OK,
            json!({
                "model": "gemini-decision-1",
                "calibration": "uncalibrated",
                "answers": {
                    "safe": { "type": "predicate", "yes_probability": 0.9, "confidence": 0.9 },
                    "lane": { "type": "choice", "choice": "direct", "probabilities": { "direct": 0.9, "escalate": 0.1 }, "confidence": 0.9 },
                    "severity": { "type": "score", "expected_score": 0.1, "probabilities": { "low": 0.9, "high": 0.1 }, "confidence": 0.9 }
                }
            })
            .to_string(),
        ),
        (StatusCode::OK, "{not-valid-json".to_string()),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "raw-upstream-secret-body".to_string(),
        ),
    ]));

    let app = Router::new()
        .route(
            "/v1beta/decisions",
            post(|State(queue): State<ResponseQueue>| async move {
                let mut guard = queue.lock().await;
                guard.remove(0)
            }),
        )
        .with_state(responses);

    let (base_url, handle) = spawn_router(app).await;
    let mut client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: DEFAULT_GEMINI_DECISION_MODEL.to_string(),
        auth: VertexAuth::ApiKey("secret-never-leak".to_string()),
        base_url: Some(base_url),
    })
    .unwrap();

    for case in [
        "out_of_set",
        "nonzero_output",
        "uncalibrated",
        "invalid_json",
    ] {
        let err = client
            .decide(sample_request())
            .await
            .expect_err(&format!("expected failure for {case}"));
        assert!(
            err.downcast_ref::<MalformedDecisionResponse>().is_some(),
            "case {case} should classify as MalformedDecisionResponse, got: {err:#}"
        );
    }

    let unavailable = client.decide(sample_request()).await.unwrap_err();
    assert!(
        unavailable
            .downcast_ref::<MalformedDecisionResponse>()
            .is_none()
    );
    let rendered = format!("{unavailable:#}");
    assert!(rendered.contains("503"));
    assert!(!rendered.contains("secret-never-leak"));
    assert!(!rendered.contains("raw-upstream-secret-body"));

    handle.abort();
}
