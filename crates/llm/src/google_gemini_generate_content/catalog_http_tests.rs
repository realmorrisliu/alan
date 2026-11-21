//! Mock-HTTP tests for Vertex AI live `v1beta1/publishers/google/models` catalog discovery.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use serde_json::json;
use tokio::net::TcpListener;

use super::catalog::{
    fetch_live_vertex_model_catalog, fetch_live_vertex_model_catalog_with_limits,
};
use super::{
    GoogleGeminiGenerateContentClient, VertexAccessTokenSource, VertexAuth, VertexClientConfig,
};
use crate::gemini_catalog::{GEMINI_DEFAULT_CONTEXT_WINDOW, VERTEX_BASELINE_MODEL_SLUGS};
use crate::{LlmProvider, ReasoningEffort};

struct SequenceTokenSource {
    calls: AtomicUsize,
}

impl SequenceTokenSource {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl VertexAccessTokenSource for SequenceTokenSource {
    async fn access_token(&self) -> anyhow::Result<String> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(format!("vertex-secret-token-{n}"))
    }
}

#[derive(Clone, Default)]
struct RecordedGets {
    uris: Arc<Mutex<Vec<String>>>,
    auth_headers: Arc<Mutex<Vec<Option<String>>>>,
    user_projects: Arc<Mutex<Vec<Option<String>>>>,
}

type MockGetHandler =
    Arc<dyn Fn(usize, String, HeaderMap) -> axum::response::Response + Send + Sync>;

#[derive(Clone)]
struct MockState {
    recorded: RecordedGets,
    counter: Arc<AtomicUsize>,
    handler: MockGetHandler,
}

async fn handle_get(
    State(state): State<MockState>,
    uri: OriginalUri,
    headers: HeaderMap,
) -> axum::response::Response {
    let idx = state.counter.fetch_add(1, Ordering::SeqCst);
    state.recorded.uris.lock().unwrap().push(uri.0.to_string());
    state.recorded.auth_headers.lock().unwrap().push(
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string),
    );
    state.recorded.user_projects.lock().unwrap().push(
        headers
            .get("x-goog-user-project")
            .and_then(|v| v.to_str().ok())
            .map(ToString::to_string),
    );
    (state.handler)(idx, uri.0.to_string(), headers)
}

async fn spawn_catalog_server<F>(handler: F) -> (String, RecordedGets, tokio::task::JoinHandle<()>)
where
    F: Fn(usize, String, HeaderMap) -> axum::response::Response + Send + Sync + 'static,
{
    let recorded = RecordedGets::default();
    let state = MockState {
        recorded: recorded.clone(),
        counter: Arc::new(AtomicUsize::new(0)),
        handler: Arc::new(handler),
    };
    let app = Router::new()
        .route("/{*path}", get(handle_get))
        .with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), recorded, handle)
}

#[tokio::test]
async fn live_discovery_paginates_filters_deduplicates_merges_baseline_and_infers_future_gens() {
    let (base_url, recorded, handle) = spawn_catalog_server(|idx, uri, _| {
        if idx == 0 {
            assert!(uri.starts_with("/v1beta1/publishers/google/models?"));
            Json(json!({
                "publisherModels": [
                    {
                        "name": "publishers/google/models/gemini-4.0-pro-preview",
                        "versionId": "001",
                        "launchStage": "PUBLIC_PREVIEW",
                        "supportedActions": {
                            "openGenerationAiStudio": {}
                        }
                    },
                    {
                        "name": "publishers/google/models/gemini-4.0-pro-preview@002",
                        "versionId": "002",
                        "launchStage": "PUBLIC_PREVIEW"
                    },
                    {
                        "name": "publishers/google/models/imagen-3.0-generate-002",
                        "launchStage": "GA"
                    },
                    {
                        "name": "publishers/google/models/veo-2.0-generate-001",
                        "launchStage": "GA"
                    },
                    {
                        "name": "publishers/google/models/gemini-embedding-001",
                        "launchStage": "GA"
                    },
                    {
                        "name": "publishers/google/models/gemini-2.5-flash-preview-tts",
                        "launchStage": "PUBLIC_PREVIEW"
                    },
                    {
                        "name": "publishers/google/models/gemini-2.5-flash-image",
                        "launchStage": "GA"
                    },
                    {
                        "name": "publishers/google/models/gemini-2.0-flash-live-001",
                        "launchStage": "GA"
                    },
                    {
                        "name": "publishers/google/models/gemini-2.5-pro-deprecated",
                        "launchStage": "DEPRECATED"
                    },
                    {
                        "name": "publishers/google/models/gemini-3.5-embed-only",
                        "supportedActions": ["embedContent"]
                    },
                    {
                        "name": "publishers/google/models/gemini-3.5-disabled",
                        "supportedActions": {"generateContent": false}
                    },
                    {"malformed_entry": 123}
                ],
                "nextPageToken": "page-2+tok="
            }))
            .into_response()
        } else {
            assert!(uri.contains("pageToken=page-2%2Btok%3D"), "uri={uri}");
            Json(json!({
                "publisherModels": [
                    {
                        "name": "publishers/google/models/gemini-3.5-flash",
                        "versionId": "001",
                        "launchStage": "GA",
                        "supportedActions": ["generateContent", "streamGenerateContent"]
                    },
                    {
                        "name": "publishers/google/models/gemini-2.5-pro",
                        "launchStage": "GA"
                    }
                ]
            }))
            .into_response()
        }
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("gcp-demo-proj".to_string()),
            location: "global".to_string(),
            model: "gemini-2.5-flash".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source.clone(),
    )
    .unwrap();

    let models = client.model_catalog().await.unwrap().expect("catalog");
    let slugs: Vec<&str> = models.iter().map(|m| m.slug.as_str()).collect();

    assert_eq!(slugs[0], "gemini-4.0-pro-preview");
    assert_eq!(slugs[1], "gemini-3.5-flash");
    for baseline in VERTEX_BASELINE_MODEL_SLUGS {
        assert!(
            slugs.contains(baseline),
            "missing baseline {baseline}: {slugs:?}"
        );
    }
    assert_eq!(
        slugs
            .iter()
            .filter(|&&s| s == "gemini-4.0-pro-preview")
            .count(),
        1,
        "duplicate versioned entries must deduplicate"
    );
    for excluded in [
        "imagen-3.0-generate-002",
        "veo-2.0-generate-001",
        "gemini-embedding-001",
        "gemini-2.5-flash-preview-tts",
        "gemini-2.5-flash-image",
        "gemini-2.0-flash-live-001",
        "gemini-2.5-pro-deprecated",
        "gemini-3.5-embed-only",
        "gemini-3.5-disabled",
    ] {
        assert!(!slugs.contains(&excluded), "should exclude {excluded}");
    }

    let g4_pro = models
        .iter()
        .find(|m| m.slug == "gemini-4.0-pro-preview")
        .unwrap();
    assert_eq!(g4_pro.context_window_tokens, GEMINI_DEFAULT_CONTEXT_WINDOW);
    assert_eq!(
        g4_pro.supported_reasoning_efforts,
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High
        ]
    );
    assert_eq!(g4_pro.default_reasoning_effort, Some(ReasoningEffort::High));

    let g35_flash = models
        .iter()
        .find(|m| m.slug == "gemini-3.5-flash")
        .unwrap();
    assert_eq!(
        g35_flash.supported_reasoning_efforts,
        vec![
            ReasoningEffort::Minimal,
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High
        ]
    );
    assert_eq!(
        g35_flash.default_reasoning_effort,
        Some(ReasoningEffort::Medium)
    );

    assert_eq!(recorded.uris.lock().unwrap().len(), 2);
    assert_eq!(
        recorded.auth_headers.lock().unwrap().as_slice(),
        [
            Some("Bearer vertex-secret-token-1".to_string()),
            Some("Bearer vertex-secret-token-1".to_string()),
        ]
    );
    assert_eq!(
        recorded.user_projects.lock().unwrap().as_slice(),
        [
            Some("gcp-demo-proj".to_string()),
            Some("gcp-demo-proj".to_string()),
        ]
    );
    assert_eq!(token_source.call_count(), 1);
    handle.abort();
}

#[tokio::test]
async fn live_discovery_retries_401_once_with_refreshed_token() {
    let (base_url, recorded, handle) = spawn_catalog_server(|_, _, headers| {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if auth == "Bearer vertex-secret-token-1" {
            return (StatusCode::UNAUTHORIZED, "secret-upstream-401-body").into_response();
        }
        Json(json!({
            "publisherModels": [
                {"name": "publishers/google/models/gemini-4.1-flash"}
            ]
        }))
        .into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "us-central1".to_string(),
            model: "gemini-2.5-flash".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source.clone(),
    )
    .unwrap();

    let models = client.model_catalog().await.unwrap().expect("catalog");
    assert!(models.iter().any(|m| m.slug == "gemini-4.1-flash"));
    assert_eq!(token_source.call_count(), 2);
    assert_eq!(
        recorded.auth_headers.lock().unwrap().as_slice(),
        [
            Some("Bearer vertex-secret-token-1".to_string()),
            Some("Bearer vertex-secret-token-2".to_string()),
        ]
    );
    handle.abort();
}

#[tokio::test]
async fn forbidden_or_empty_live_discovery_sanitizes_errors_and_falls_back_to_baseline() {
    let (base_url, _, handle) = spawn_catalog_server(|_, _, _| {
        (
            StatusCode::FORBIDDEN,
            "sensitive-iam-body-with-secret-project-details",
        )
            .into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "global".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source,
    )
    .unwrap();

    // Strict live fetch fails cleanly without leaking auth token or raw response body.
    let strict_err = fetch_live_vertex_model_catalog(&client.transport)
        .await
        .unwrap_err()
        .to_string();
    assert!(strict_err.contains("403"), "err={strict_err}");
    assert!(!strict_err.contains("sensitive-iam-body"));
    assert!(!strict_err.contains("vertex-secret-token"));

    // Public model_catalog() falls back gracefully to curated baseline models.
    let fallback = client.model_catalog().await.unwrap().expect("baseline");
    let slugs: Vec<&str> = fallback.iter().map(|m| m.slug.as_str()).collect();
    for baseline in VERTEX_BASELINE_MODEL_SLUGS {
        assert!(slugs.contains(baseline), "missing baseline {baseline}");
    }
    handle.abort();
}

#[tokio::test]
async fn pagination_cap_and_size_limit_are_enforced() {
    let (base_url, recorded, handle) = spawn_catalog_server(|idx, _, _| {
        Json(json!({
            "publisherModels": [
                {"name": format!("publishers/google/models/gemini-4.{idx}-flash")}
            ],
            "nextPageToken": format!("next-page-{idx}")
        }))
        .into_response()
    })
    .await;

    let token_source = Arc::new(SequenceTokenSource::new());
    let client = GoogleGeminiGenerateContentClient::from_vertex_config_with_token_source(
        VertexClientConfig {
            project_id: Some("proj".to_string()),
            location: "europe-west4".to_string(),
            model: "gemini-2.5-pro".to_string(),
            auth: VertexAuth::Adc,
            base_url: Some(base_url),
        },
        token_source,
    )
    .unwrap();

    let capped = fetch_live_vertex_model_catalog_with_limits(
        &client.transport,
        Duration::from_secs(5),
        1024 * 1024,
        1024 * 1024,
        3,
    )
    .await
    .unwrap();
    assert_eq!(recorded.uris.lock().unwrap().len(), 3);
    assert!(capped.iter().any(|m| m.slug == "gemini-4.0-flash"));
    assert!(capped.iter().any(|m| m.slug == "gemini-4.2-flash"));
    assert!(!capped.iter().any(|m| m.slug == "gemini-4.3-flash"));

    // Tiny size limit rejects oversized page cleanly without leaking body.
    let oversized = fetch_live_vertex_model_catalog_with_limits(
        &client.transport,
        Duration::from_secs(5),
        16,
        16,
        1,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(oversized.contains("size limit"), "err={oversized}");
    handle.abort();
}

#[tokio::test]
async fn express_mode_skips_publisher_listing_and_returns_curated_baseline() {
    let (base_url, recorded, handle) = spawn_catalog_server(|_, _, _| {
        (StatusCode::INTERNAL_SERVER_ERROR, "must not be called").into_response()
    })
    .await;

    let client = GoogleGeminiGenerateContentClient::from_vertex_config(VertexClientConfig {
        project_id: None,
        location: "global".to_string(),
        model: "gemini-2.5-flash".to_string(),
        auth: VertexAuth::ApiKey("express-key-secret".to_string()),
        base_url: Some(base_url),
    })
    .unwrap();

    let models = client.model_catalog().await.unwrap().expect("baseline");
    assert!(recorded.uris.lock().unwrap().is_empty());
    let slugs: Vec<&str> = models.iter().map(|m| m.slug.as_str()).collect();
    for baseline in VERTEX_BASELINE_MODEL_SLUGS {
        assert!(slugs.contains(baseline), "missing baseline {baseline}");
    }
    handle.abort();
}
