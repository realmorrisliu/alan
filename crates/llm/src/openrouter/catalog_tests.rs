use super::*;
use crate::LlmProvider;
use axum::{Router, http::HeaderMap, routing::get};
use serde_json::{Value, json};
use tokio::net::TcpListener;

fn model(id: &str, extra: Value) -> Value {
    let mut model = json!({
        "id": id,
        "name": id,
        "created": 1.0,
        "context_length": 128000.0,
        "architecture": {"output_modalities": ["text"]},
        "top_provider": {"context_length": 64000.0, "is_moderated": false},
        "pricing": {"prompt": "0", "completion": "0"},
        "supported_parameters": ["tools"]
    });
    model
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    model
}

fn decode(models: Vec<Value>) -> Result<Vec<ProviderModel>> {
    decode_models(&serde_json::to_vec(&json!({ "data": models })).unwrap())
}

#[test]
fn catalog_projects_context_and_reasoning_metadata() {
    let parsed = decode(vec![
        model(
            "openai/gpt-5",
            json!({
                "supported_parameters": ["reasoning", "tools"],
                "reasoning": {
                    "mandatory": false,
                    "default_effort": "medium",
                    "supported_efforts": ["minimal", "low", "medium", "high", "max", "low"]
                }
            }),
        ),
        model(
            "deepseek/deepseek-r1:free",
            json!({
                "supported_parameters": ["reasoning_effort"],
                "context_length": null
            }),
        ),
        model("meta/llama-3", json!({})),
    ])
    .unwrap();

    assert_eq!(
        parsed,
        vec![
            ProviderModel {
                slug: "openai/gpt-5".into(),
                context_window_tokens: 128_000,
                supported_reasoning_efforts: vec![
                    ReasoningEffort::Minimal,
                    ReasoningEffort::Low,
                    ReasoningEffort::Medium,
                    ReasoningEffort::High,
                ],
                default_reasoning_effort: Some(ReasoningEffort::Medium),
            },
            ProviderModel {
                slug: "deepseek/deepseek-r1:free".into(),
                context_window_tokens: 64_000,
                supported_reasoning_efforts: DEFAULT_REASONING_EFFORTS.to_vec(),
                default_reasoning_effort: None,
            },
            ProviderModel {
                slug: "meta/llama-3".into(),
                context_window_tokens: 128_000,
                supported_reasoning_efforts: Vec::new(),
                default_reasoning_effort: None,
            },
        ]
    );
}

#[test]
fn catalog_skips_unusable_entries_and_rejects_empty_results() {
    let parsed = decode(vec![
        model(
            "image/only",
            json!({"architecture": {"output_modalities": ["image"]}}),
        ),
        model("embed/only", json!({"endpoints": ["/api/v1/embeddings"]})),
        model(
            "no/context",
            json!({"context_length": null, "top_provider": {"context_length": null}}),
        ),
        model("bad slug!", json!({})),
        model(
            "chat/ok",
            json!({"endpoints": ["/api/v1/chat/completions"]}),
        ),
        model("CHAT/OK", json!({})),
    ])
    .unwrap();
    assert_eq!(
        parsed
            .iter()
            .map(|model| model.slug.as_str())
            .collect::<Vec<_>>(),
        ["chat/ok"]
    );

    let error = decode(vec![model(
        "image/only",
        json!({"architecture": {"output_modalities": ["image"]}}),
    )])
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no selectable models (1 returned)")
    );
    assert_eq!(
        decode_models(b"not json").unwrap_err().to_string(),
        "Invalid OpenRouter model catalog metadata"
    );
}

#[tokio::test]
async fn model_catalog_fetches_authorized_models_endpoint() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://{}/api/v1", listener.local_addr().unwrap());
    let body = json!({ "data": [model("anthropic/claude-sonnet-4.5", json!({}))] });
    let app = Router::new().route(
        "/api/v1/models",
        get(move |headers: HeaderMap| {
            let body = body.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer sk-or-test");
                assert_eq!(headers["x-title"], "alan");
                axum::Json(body)
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = OpenRouterClient::with_metadata(
        "sk-or-test",
        &url,
        "unused",
        super::super::OpenRouterClientMetadata {
            x_title: Some("alan".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let models = client.model_catalog().await.unwrap().unwrap();
    server.abort();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].slug, "anthropic/claude-sonnet-4.5");
    assert_eq!(models[0].context_window_tokens, 128_000);
}

#[tokio::test]
async fn model_catalog_reports_status_without_body() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().route(
        "/models",
        get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "secret-token") }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = OpenRouterClient::with_params("sk-or-test", &url, "unused").unwrap();
    let error = client.model_catalog().await.unwrap_err().to_string();
    server.abort();

    assert_eq!(error, "OpenRouter model catalog HTTP 401 Unauthorized");
}
