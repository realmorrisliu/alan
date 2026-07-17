use super::ChatgptResponsesClient;
use crate::factory::{ProviderConfig, ProviderType};
use crate::{GenerationRequest, LlmProvider};
use alan_auth::{
    AuthStorage, AuthStore, ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthManager,
    ChatgptIdTokenInfo, ChatgptTokenData, StoredChatgptAuth,
};
use axum::{Json, Router, extract::State, http::HeaderMap, response::IntoResponse, routing::post};
use base64::Engine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tokio::net::TcpListener;

#[derive(Clone)]
struct TestServerState {
    response_count: Arc<AtomicUsize>,
    refresh_count: Arc<AtomicUsize>,
    authorizations: Arc<Mutex<Vec<String>>>,
    account_ids: Arc<Mutex<Vec<String>>>,
    accept_headers: Arc<Mutex<Vec<String>>>,
    request_bodies: Arc<Mutex<Vec<serde_json::Value>>>,
    response_mode: TestResponseMode,
}

#[derive(Clone, Copy)]
enum TestResponseMode {
    AlwaysOk,
    RequireStream,
    UnauthorizedThenOk,
    AlwaysUnauthorized,
}

fn build_jwt(payload: serde_json::Value) -> String {
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.sig")
}

fn seed_chatgpt_auth(storage_path: PathBuf, access_token: String, refresh_token: &str) -> PathBuf {
    let storage = AuthStorage::new(storage_path.clone()).expect("storage");
    let id_token = build_jwt(serde_json::json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    storage
        .save(&AuthStore {
            version: 1,
            chatgpt: Some(
                StoredChatgptAuth::from_tokens(ChatgptTokenData {
                    id_token: ChatgptIdTokenInfo {
                        email: Some("user@example.com".to_string()),
                        plan_type: Some("pro".to_string()),
                        user_id: Some("user_123".to_string()),
                        account_id: Some("acct_123".to_string()),
                        raw_jwt: id_token,
                    },
                    access_token,
                    refresh_token: refresh_token.to_string(),
                })
                .expect("stored auth"),
            ),
        })
        .expect("save auth");
    storage_path
}

fn test_client(base_url: &str, storage_path: PathBuf) -> ChatgptResponsesClient {
    ChatgptResponsesClient {
        client: reqwest::Client::new(),
        auth_manager: ChatgptAuthManager::new(ChatgptAuthConfig {
            storage_path,
            issuer: base_url.to_string(),
            client_id: "client".to_string(),
            browser_callback_port: 1455,
        })
        .expect("auth manager"),
        base_url: base_url.trim_end_matches('/').to_string(),
        model: "gpt-5.3-codex".to_string(),
        custom_headers: HashMap::new(),
        expected_account_id: Some("acct_123".to_string()),
    }
}

async fn spawn_chatgpt_test_server(
    response_mode: TestResponseMode,
) -> (String, TestServerState, tokio::task::JoinHandle<()>) {
    fn ok_response_body(text: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "resp_123",
            "status": "completed",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": text}]
            }],
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1,
                "total_tokens": 2,
                "output_tokens_details": {"reasoning_tokens": 0}
            }
        })
    }

    fn streaming_response(text: &str) -> axum::response::Response {
        let delta = serde_json::json!({
            "type": "response.output_text.delta",
            "delta": text,
            "sequence_number": 0
        });
        let completed = serde_json::json!({
            "type": "response.completed",
            "sequence_number": 1,
            "response": ok_response_body(text)
        });
        let body = format!("data: {delta}\n\ndata: {completed}\n\n");
        (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            body,
        )
            .into_response()
    }

    fn json_response(
        status: axum::http::StatusCode,
        body: serde_json::Value,
    ) -> axum::response::Response {
        (status, Json(body)).into_response()
    }

    async fn refresh_token(State(state): State<TestServerState>) -> Json<serde_json::Value> {
        state.refresh_count.fetch_add(1, Ordering::SeqCst);
        Json(serde_json::json!({
            "id_token": build_jwt(serde_json::json!({
                "email": "user@example.com",
                "https://api.openai.com/auth": {
                    "chatgpt_plan_type": "pro",
                    "chatgpt_user_id": "user_123",
                    "chatgpt_account_id": "acct_123"
                }
            })),
            "access_token": build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "refreshed"})),
            "refresh_token": "refresh_token_rotated"
        }))
    }

    async fn responses(
        State(state): State<TestServerState>,
        headers: HeaderMap,
        axum::Json(request_body): axum::Json<serde_json::Value>,
    ) -> axum::response::Response {
        let count = state.response_count.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(auth) = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
        {
            state
                .authorizations
                .lock()
                .expect("authorizations")
                .push(auth.to_string());
        }
        if let Some(account_id) = headers
            .get("chatgpt-account-id")
            .and_then(|value| value.to_str().ok())
        {
            state
                .account_ids
                .lock()
                .expect("account ids")
                .push(account_id.to_string());
        }
        if let Some(accept) = headers.get("accept").and_then(|value| value.to_str().ok()) {
            state
                .accept_headers
                .lock()
                .expect("accept headers")
                .push(accept.to_string());
        }
        state
            .request_bodies
            .lock()
            .expect("request bodies")
            .push(request_body.clone());

        let stream_requested = request_body
            .get("stream")
            .and_then(serde_json::Value::as_bool)
            == Some(true);

        match state.response_mode {
            TestResponseMode::AlwaysOk => {
                if stream_requested {
                    streaming_response("ok")
                } else {
                    json_response(axum::http::StatusCode::OK, ok_response_body("ok"))
                }
            }
            TestResponseMode::RequireStream if !stream_requested => json_response(
                axum::http::StatusCode::BAD_REQUEST,
                serde_json::json!({"detail": "Stream must be set to true"}),
            ),
            TestResponseMode::RequireStream => streaming_response("ok"),
            TestResponseMode::UnauthorizedThenOk if count == 1 => json_response(
                axum::http::StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "expired"}),
            ),
            TestResponseMode::UnauthorizedThenOk => {
                if stream_requested {
                    streaming_response("retried")
                } else {
                    json_response(axum::http::StatusCode::OK, ok_response_body("retried"))
                }
            }
            TestResponseMode::AlwaysUnauthorized => json_response(
                axum::http::StatusCode::UNAUTHORIZED,
                serde_json::json!({"error": "still unauthorized"}),
            ),
        }
    }

    let state = TestServerState {
        response_count: Arc::new(AtomicUsize::new(0)),
        refresh_count: Arc::new(AtomicUsize::new(0)),
        authorizations: Arc::new(Mutex::new(Vec::new())),
        account_ids: Arc::new(Mutex::new(Vec::new())),
        accept_headers: Arc::new(Mutex::new(Vec::new())),
        request_bodies: Arc::new(Mutex::new(Vec::new())),
        response_mode,
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let address = listener.local_addr().expect("local addr");
    let app = Router::new()
        .route("/oauth/token", post(refresh_token))
        .route("/responses", post(responses))
        .with_state(state.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (format!("http://{}", address), state, server)
}

fn expired_access_token() -> String {
    build_jwt(serde_json::json!({"exp": 1_i64, "token": "expired"}))
}

fn valid_access_token() -> String {
    build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "initial"}))
}

fn refreshed_access_token() -> String {
    build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": "refreshed"}))
}

#[test]
fn provider_config_builds_chatgpt_client() {
    let config = ProviderConfig::chatgpt("gpt-5.3-codex")
        .with_base_url("https://chatgpt.com/backend-api/codex")
        .with_chatgpt_account_id("acct_123");
    assert_eq!(config.provider_type, ProviderType::ChatgptResponses);
    assert_eq!(config.expected_account_id.as_deref(), Some("acct_123"));
}

#[test]
fn client_requires_auth_manager_paths() {
    let client = ChatgptResponsesClient::with_params(
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.3-codex",
        HashMap::new(),
        None,
        None,
    );
    assert!(client.is_ok());
}

#[test]
fn client_uses_custom_auth_storage_path_when_provided() {
    let storage_path = std::env::temp_dir().join(format!(
        "alan-llm-chatgpt-auth-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let storage_path = storage_path.join("auth.json");
    let client = ChatgptResponsesClient::with_params(
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.3-codex",
        HashMap::new(),
        None,
        Some(storage_path.clone()),
    )
    .expect("client");

    assert_eq!(client.auth_manager.storage_path(), storage_path.as_path());
}

#[tokio::test]
async fn proactive_refresh_happens_before_dispatch() {
    let temp_dir = TempDir::new().expect("temp dir");
    let storage_path = seed_chatgpt_auth(
        temp_dir.path().join("auth.json"),
        expired_access_token(),
        "refresh",
    );
    let (base_url, state, server) = spawn_chatgpt_test_server(TestResponseMode::AlwaysOk).await;
    let mut client = test_client(&base_url, storage_path);

    let result = client.chat(None, "hello").await.expect("chat");
    assert_eq!(result, "ok");
    assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.response_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        state.authorizations.lock().expect("authorizations").clone(),
        vec![format!("Bearer {}", refreshed_access_token())]
    );
    assert_eq!(
        state.account_ids.lock().expect("account ids").clone(),
        vec!["acct_123".to_string()]
    );
    assert_eq!(
        state.accept_headers.lock().expect("accept headers").clone(),
        vec!["text/event-stream".to_string()]
    );
    let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
    assert_eq!(request_bodies.len(), 1);
    assert_eq!(request_bodies[0]["instructions"], "");
    assert_eq!(request_bodies[0]["stream"], true);
    assert_eq!(request_bodies[0]["input"][0]["role"], "user");

    server.abort();
}

#[tokio::test]
async fn unauthorized_response_triggers_single_refresh_and_retry() {
    let temp_dir = TempDir::new().expect("temp dir");
    let storage_path = seed_chatgpt_auth(
        temp_dir.path().join("auth.json"),
        valid_access_token(),
        "refresh",
    );
    let (base_url, state, server) =
        spawn_chatgpt_test_server(TestResponseMode::UnauthorizedThenOk).await;
    let mut client = test_client(&base_url, storage_path);

    let result = client.chat(None, "hello").await.expect("chat");
    assert_eq!(result, "retried");
    assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.response_count.load(Ordering::SeqCst), 2);
    let authorizations = state.authorizations.lock().expect("authorizations").clone();
    assert_eq!(authorizations.len(), 2);
    assert_eq!(
        authorizations[0],
        format!("Bearer {}", valid_access_token())
    );
    assert_eq!(
        authorizations[1],
        format!("Bearer {}", refreshed_access_token())
    );
    assert_eq!(
        state.account_ids.lock().expect("account ids").clone(),
        vec!["acct_123".to_string(), "acct_123".to_string()]
    );
    let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
    assert_eq!(request_bodies.len(), 2);
    assert_eq!(request_bodies[0]["stream"], true);
    assert_eq!(request_bodies[1]["stream"], true);
    assert_eq!(
        state.accept_headers.lock().expect("accept headers").clone(),
        vec![
            "text/event-stream".to_string(),
            "text/event-stream".to_string()
        ]
    );

    server.abort();
}

#[tokio::test]
async fn repeated_unauthorized_surfaces_first_class_auth_error() {
    let temp_dir = TempDir::new().expect("temp dir");
    let storage_path = seed_chatgpt_auth(
        temp_dir.path().join("auth.json"),
        valid_access_token(),
        "refresh",
    );
    let (base_url, state, server) =
        spawn_chatgpt_test_server(TestResponseMode::AlwaysUnauthorized).await;
    let mut client = test_client(&base_url, storage_path);

    let error = client.chat(None, "hello").await.expect_err("auth failure");
    let auth_error = error
        .downcast_ref::<ChatgptAuthError>()
        .expect("ChatGPT auth error");
    assert!(matches!(
        auth_error,
        ChatgptAuthError::UnauthorizedAfterRefresh(message)
            if message.contains("still unauthorized")
    ));
    assert_eq!(state.refresh_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.response_count.load(Ordering::SeqCst), 2);

    server.abort();
}

#[tokio::test]
async fn chatgpt_requests_send_instructions_separately_from_input() {
    let temp_dir = TempDir::new().expect("temp dir");
    let storage_path = seed_chatgpt_auth(
        temp_dir.path().join("auth.json"),
        valid_access_token(),
        "refresh",
    );
    let (base_url, state, server) = spawn_chatgpt_test_server(TestResponseMode::AlwaysOk).await;
    let mut client = test_client(&base_url, storage_path);

    let result = client
        .chat(Some("Follow the system prompt"), "hello")
        .await
        .expect("chat");
    assert_eq!(result, "ok");
    let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
    assert_eq!(request_bodies.len(), 1);
    assert_eq!(
        request_bodies[0]["instructions"],
        serde_json::Value::String("Follow the system prompt".to_string())
    );
    assert_eq!(request_bodies[0]["stream"], true);
    assert_eq!(request_bodies[0]["input"].as_array().map(Vec::len), Some(1));
    assert_eq!(request_bodies[0]["input"][0]["role"], "user");

    server.abort();
}

#[tokio::test]
async fn generate_uses_streaming_contract_when_server_requires_stream_true() {
    let temp_dir = TempDir::new().expect("temp dir");
    let storage_path = seed_chatgpt_auth(
        temp_dir.path().join("auth.json"),
        valid_access_token(),
        "refresh",
    );
    let (base_url, state, server) =
        spawn_chatgpt_test_server(TestResponseMode::RequireStream).await;
    let mut client = test_client(&base_url, storage_path);

    let response = client
        .generate(GenerationRequest::new().with_user_message("hello"))
        .await
        .expect("generate");

    assert_eq!(response.content, "ok");
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(response.provider_response_id.as_deref(), Some("resp_123"));
    assert_eq!(
        response.provider_response_status.as_deref(),
        Some("completed")
    );
    assert_eq!(response.usage.map(|usage| usage.total_tokens), Some(2));
    assert!(response.warnings.is_empty());

    let request_bodies = state.request_bodies.lock().expect("request bodies").clone();
    assert_eq!(request_bodies.len(), 1);
    assert_eq!(request_bodies[0]["stream"], true);
    assert_eq!(
        state.accept_headers.lock().expect("accept headers").clone(),
        vec!["text/event-stream".to_string()]
    );

    server.abort();
}

#[tokio::test]
async fn generate_rejects_previous_response_id_for_managed_chatgpt() {
    let storage_path = std::env::temp_dir().join(format!(
        "alan-llm-chatgpt-auth-continuation-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let storage_path = storage_path.join("auth.json");
    let mut client = ChatgptResponsesClient::with_params(
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.3-codex",
        HashMap::new(),
        None,
        Some(storage_path),
    )
    .expect("client");

    let error = client
        .generate(
            GenerationRequest::new()
                .with_user_message("hello")
                .with_previous_response_id("resp_prev"),
        )
        .await
        .expect_err("continuation should be rejected before dispatch");
    assert!(
        error
            .to_string()
            .contains("does not support previous_response_id continuation")
    );
}

#[test]
fn chatgpt_build_request_normalizes_managed_request_fields() {
    let client = ChatgptResponsesClient::with_params(
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.3-codex",
        HashMap::new(),
        Some("acct_123".to_string()),
        None,
    )
    .expect("client");

    let request = client
        .build_openai_responses_request(
            crate::GenerationRequest::new()
                .with_system_prompt("system")
                .with_user_message("hello")
                .with_previous_response_id("resp_prev")
                .with_temperature(0.7)
                .with_store(true),
            false,
        )
        .unwrap();

    assert_eq!(request.previous_response_id.as_deref(), Some("resp_prev"));
    assert_eq!(request.store, Some(false));
    assert_eq!(request.instructions.as_deref(), Some("system"));
    assert_eq!(request.temperature, None);
    assert_eq!(request.max_output_tokens, None);
}

#[tokio::test]
async fn generate_stream_surfaces_auth_errors_before_returning_receiver() {
    let storage_path = std::env::temp_dir().join(format!(
        "alan-llm-chatgpt-auth-stream-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let storage_path = storage_path.join("auth.json");
    let mut client = ChatgptResponsesClient::with_params(
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.3-codex",
        HashMap::new(),
        None,
        Some(storage_path),
    )
    .expect("client");

    let error = client
        .generate_stream(crate::GenerationRequest::new().with_user_message("hi"))
        .await
        .expect_err("missing auth should fail before returning a receiver");
    let auth_error = error
        .downcast_ref::<alan_auth::ChatgptAuthError>()
        .expect("chatgpt auth error");
    assert!(matches!(
        auth_error,
        alan_auth::ChatgptAuthError::NotLoggedIn
    ));
}
