//! Session identity, prompt-cache routing and auth refresh/retry for managed ChatGPT.

use super::{build_jwt, seed_chatgpt_auth, test_client};
use crate::{GenerationRequest, LlmProvider};
use alan_auth::{AuthStorage, ChatgptAuthError};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    AlwaysOk,
    /// `401` for the seeded access token, success for any refreshed one.
    RejectInitial,
    AlwaysUnauthorized,
}

#[derive(Clone, Debug, PartialEq)]
struct Seen {
    authorization: String,
    account: String,
    session: String,
    conversation: String,
    originator: String,
    user_agent: String,
    cache_key: serde_json::Value,
}

#[derive(Clone)]
struct Server {
    mode: Mode,
    refresh_account: &'static str,
    initial_token: String,
    refreshes: Arc<AtomicUsize>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

fn header(headers: &HeaderMap, name: &str) -> String {
    headers
        .get_all(name)
        .iter()
        .map(|value| value.to_str().unwrap().to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn completed(text: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "resp_1",
        "status": "completed",
        "output": [{"type": "message", "content": [{"type": "output_text", "text": text}]}],
        "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
    })
}

async fn responses(
    State(server): State<Server>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let authorization = header(&headers, "authorization");
    server.seen.lock().unwrap().push(Seen {
        authorization: authorization.clone(),
        account: header(&headers, "chatgpt-account-id"),
        session: header(&headers, "session_id"),
        conversation: header(&headers, "conversation_id"),
        originator: header(&headers, "originator"),
        user_agent: header(&headers, "user-agent"),
        cache_key: body["prompt_cache_key"].clone(),
    });
    let rejected = match server.mode {
        Mode::AlwaysOk => false,
        Mode::RejectInitial => authorization == format!("Bearer {}", server.initial_token),
        Mode::AlwaysUnauthorized => true,
    };
    if rejected {
        return (StatusCode::UNAUTHORIZED, "token rejected").into_response();
    }
    if body["stream"] == true {
        let event = serde_json::json!({
            "type": "response.completed",
            "sequence_number": 0,
            "response": completed("streamed")
        });
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/event-stream")],
            format!("data: {event}\n\n"),
        )
            .into_response();
    }
    Json(completed("plain")).into_response()
}

async fn refresh(State(server): State<Server>) -> Json<serde_json::Value> {
    let count = server.refreshes.fetch_add(1, Ordering::SeqCst) + 1;
    // Keep the refresh in flight long enough for concurrent requests to overlap it.
    tokio::time::sleep(Duration::from_millis(50)).await;
    Json(serde_json::json!({
        "id_token": build_jwt(serde_json::json!({
            "https://api.openai.com/auth": {"chatgpt_account_id": server.refresh_account}
        })),
        "access_token": refreshed_token(count),
        "refresh_token": format!("rotated-{count}"),
    }))
}

fn refreshed_token(count: usize) -> String {
    build_jwt(serde_json::json!({"exp": 4_102_444_800_i64, "token": format!("refreshed-{count}")}))
}

fn token_expiring_in(seconds: i64) -> String {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + seconds;
    build_jwt(serde_json::json!({"exp": exp, "token": "initial"}))
}

struct Fixture {
    url: String,
    server: Server,
    path: std::path::PathBuf,
    _temp: TempDir,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    fn seen(&self) -> Vec<Seen> {
        self.server.seen.lock().unwrap().clone()
    }

    fn refreshes(&self) -> usize {
        self.server.refreshes.load(Ordering::SeqCst)
    }

    fn client(&self) -> super::ChatgptResponsesClient {
        test_client(&self.url, self.path.clone())
    }
}

async fn fixture(mode: Mode, initial_token: String, refresh_account: &'static str) -> Fixture {
    let temp = TempDir::new().unwrap();
    let path = seed_chatgpt_auth(temp.path().join("auth.json"), initial_token.clone(), "r");
    let server = Server {
        mode,
        refresh_account,
        initial_token,
        refreshes: Arc::default(),
        seen: Arc::default(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/oauth/token", post(refresh))
        .route("/responses", post(responses))
        .with_state(server.clone());
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Fixture {
        url,
        server,
        path,
        _temp: temp,
        task,
    }
}

fn user(text: &str) -> GenerationRequest {
    GenerationRequest::new().with_user_message(text)
}

async fn plain_request(client: &super::ChatgptResponsesClient) -> anyhow::Result<()> {
    let request = client.build_openai_responses_request(user("plain"), false)?;
    client.openai_responses(request).await.map(|_| ())
}

#[tokio::test]
async fn session_headers_and_cache_key_are_stable_per_client_and_distinct_across_clients() {
    let fixture = fixture(Mode::AlwaysOk, token_expiring_in(3600), "acct_123").await;
    let mut first = fixture.client();
    // Profile headers cannot replace the client-owned session identity, while
    // originator / User-Agent headers pass through untouched.
    first.custom_headers = HashMap::from([
        ("Session_Id".to_string(), "spoofed".to_string()),
        ("conversation_id".to_string(), "spoofed".to_string()),
        ("originator".to_string(), "codex_cli_rs".to_string()),
        ("User-Agent".to_string(), "alan-test/1.0".to_string()),
    ]);
    let mut second = fixture.client();

    first.chat(None, "streaming").await.unwrap();
    plain_request(&first).await.unwrap();
    let mut receiver = first.generate_stream(user("stream")).await.unwrap();
    while receiver.recv().await.is_some() {}
    second.chat(None, "other client").await.unwrap();

    let seen = fixture.seen();
    assert_eq!(seen.len(), 4);
    let session = first.prompt_cache_key().to_string();
    for request in &seen[..3] {
        assert_eq!(request.session, session);
        assert_eq!(request.conversation, session);
        assert_eq!(request.originator, "codex_cli_rs");
        assert_eq!(request.user_agent, "alan-test/1.0");
        assert_eq!(request.cache_key, serde_json::json!(session));
        assert_eq!(request.account, "acct_123");
    }
    assert_ne!(second.prompt_cache_key(), session);
    assert_eq!(seen[3].session, second.prompt_cache_key());
    assert_eq!(
        seen[3].cache_key,
        serde_json::json!(second.prompt_cache_key())
    );
    assert_eq!(fixture.refreshes(), 0);
}

#[tokio::test]
async fn request_session_key_derives_stable_opaque_affinity_across_clients() {
    let fixture = fixture(Mode::AlwaysOk, token_expiring_in(3600), "acct_123").await;
    let mut first = fixture.client();
    let mut second = fixture.client();

    let req_a = user("turn 1").with_session_key("boot-1/agent/root");
    let req_b = user("turn 2").with_session_key("boot-1/agent/root");
    first.generate(req_a).await.unwrap();
    let mut receiver = second.generate_stream(req_b).await.unwrap();
    while receiver.recv().await.is_some() {}

    let seen = fixture.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].session, seen[1].session);
    assert_eq!(seen[0].conversation, seen[0].session);
    assert_eq!(seen[0].cache_key, serde_json::json!(seen[0].session));
    assert_eq!(seen[1].cache_key, serde_json::json!(seen[1].session));
    assert!(!seen[0].session.contains("agent"));
}

#[tokio::test]
async fn pinned_session_id_and_explicit_cache_key_are_respected() {
    let fixture = fixture(Mode::AlwaysOk, token_expiring_in(3600), "acct_123").await;
    let client = fixture.client().with_prompt_cache_key("process-42");
    plain_request(&client).await.unwrap();
    let mut request = client
        .build_openai_responses_request(user("explicit"), false)
        .unwrap();
    request
        .extra_params
        .insert("prompt_cache_key".into(), serde_json::json!("caller-key"));
    client.openai_responses(request).await.unwrap();

    let seen = fixture.seen();
    assert_eq!(seen[0].session, "process-42");
    assert_eq!(seen[0].cache_key, serde_json::json!("process-42"));
    assert_eq!(seen[1].session, "process-42");
    assert_eq!(seen[1].cache_key, serde_json::json!("caller-key"));
}

#[tokio::test]
async fn token_inside_safety_window_refreshes_before_dispatch() {
    let fixture = fixture(Mode::RejectInitial, token_expiring_in(60), "acct_123").await;
    let client = fixture.client();
    plain_request(&client).await.unwrap();

    assert_eq!(fixture.refreshes(), 1);
    let seen = fixture.seen();
    assert_eq!(seen.len(), 1, "the near-expiry token is never sent");
    assert_eq!(
        seen[0].authorization,
        format!("Bearer {}", refreshed_token(1))
    );
}

#[tokio::test]
async fn unauthorized_retries_once_with_refreshed_token_and_same_session() {
    for stream in [false, true] {
        let fixture = fixture(Mode::RejectInitial, token_expiring_in(3600), "acct_123").await;
        let mut client = fixture.client();
        if stream {
            client.chat(None, "streaming").await.unwrap();
        } else {
            plain_request(&client).await.unwrap();
        }
        assert_eq!(fixture.refreshes(), 1);
        let seen = fixture.seen();
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen[1].authorization,
            format!("Bearer {}", refreshed_token(1))
        );
        assert_eq!(seen[0].session, seen[1].session);
        assert_eq!(seen[0].cache_key, seen[1].cache_key);
    }
}

#[tokio::test]
async fn second_unauthorized_surfaces_error_without_looping() {
    let fixture = fixture(
        Mode::AlwaysUnauthorized,
        token_expiring_in(3600),
        "acct_123",
    )
    .await;
    let error = plain_request(&fixture.client()).await.unwrap_err();
    assert!(matches!(
        error.downcast_ref::<ChatgptAuthError>(),
        Some(ChatgptAuthError::UnauthorizedAfterRefresh(body)) if body == "token rejected"
    ));
    assert_eq!(fixture.refreshes(), 1);
    assert_eq!(fixture.seen().len(), 2);
}

#[tokio::test]
async fn refresh_resolving_another_account_is_rejected_before_dispatch() {
    for (mode, token, dispatched) in [
        (Mode::AlwaysOk, token_expiring_in(-10), 0),
        (Mode::RejectInitial, token_expiring_in(3600), 1),
    ] {
        let fixture = fixture(mode, token, "acct_other").await;
        let before = AuthStorage::new(fixture.path.clone())
            .unwrap()
            .load()
            .unwrap();
        let error = plain_request(&fixture.client()).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref::<ChatgptAuthError>(),
            Some(ChatgptAuthError::WorkspaceMismatch { expected, actual })
                if expected == "acct_123" && actual.as_deref() == Some("acct_other")
        ));
        assert_eq!(fixture.refreshes(), 1);
        assert_eq!(fixture.seen().len(), dispatched);
        let after = AuthStorage::new(fixture.path.clone())
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(
            after, before,
            "a foreign-account refresh is never persisted"
        );
    }
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    for (mode, token, sent) in [
        (Mode::AlwaysOk, token_expiring_in(-10), 6),
        (Mode::RejectInitial, token_expiring_in(3600), 12),
    ] {
        let fixture = fixture(mode, token, "acct_123").await;
        let client = fixture.client();
        let results = futures::future::join_all((0..6).map(|_| plain_request(&client))).await;
        assert!(results.iter().all(Result::is_ok), "{results:?}");
        assert_eq!(fixture.refreshes(), 1);
        let seen = fixture.seen();
        assert_eq!(seen.len(), sent);
        let refreshed = format!("Bearer {}", refreshed_token(1));
        assert_eq!(
            seen.iter()
                .filter(|request| request.authorization == refreshed)
                .count(),
            6
        );
        assert!(
            seen.iter()
                .all(|request| request.session == client.prompt_cache_key())
        );
    }
}
