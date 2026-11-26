//! Expiry-aware and single-flight refresh behaviour against a local token endpoint.

use super::tests::build_jwt;
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Minimal `/oauth/token` endpoint answering every request with `account`'s tokens.
struct TokenServer {
    issuer: String,
    refreshes: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for TokenServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn id_token(account: &str) -> String {
    build_jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": account}}))
}

fn access_token(exp: i64, label: &str) -> String {
    build_jwt(json!({"exp": exp, "token": label}))
}

fn far_future() -> i64 {
    4_102_444_800
}

async fn spawn_token_server(account: &'static str) -> TokenServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let refreshes = Arc::new(AtomicUsize::new(0));
    let counter = refreshes.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let count = counter.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let read = stream.read(&mut chunk).await.unwrap();
                    if read == 0 {
                        return;
                    }
                    request.extend_from_slice(&chunk[..read]);
                    let raw = String::from_utf8_lossy(&request);
                    let Some(end) = raw.find("\r\n\r\n") else {
                        continue;
                    };
                    let length = raw[..end]
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
                let request_raw = String::from_utf8_lossy(&request);
                assert!(request_raw.starts_with("POST /api/accounts/oauth/token "));
                let body_part = request_raw
                    .split_once("\r\n\r\n")
                    .map(|(_, b)| b)
                    .unwrap_or("");
                assert!(body_part.contains("grant_type=refresh_token"));
                assert!(body_part.contains("client_id=oaiapp_test"));
                assert!(body_part.contains("resource=https%3A%2F%2Fapi.openai.com%2Fv1"));
                let n = count.fetch_add(1, Ordering::SeqCst) + 1;
                // Hold the response so concurrent callers really overlap the refresh.
                tokio::time::sleep(Duration::from_millis(50)).await;
                let body = json!({
                    "id_token": id_token(account),
                    "access_token": access_token(far_future(), &format!("refreshed-{n}")),
                    "refresh_token": format!("rotated-{n}"),
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    TokenServer {
        issuer,
        refreshes,
        task,
    }
}

fn manager_for(temp: &TempDir, issuer: &str) -> ChatgptAuthManager {
    let mut manager = ChatgptAuthManager::new(ChatgptAuthConfig {
        storage_path: temp.path().join("auth.json"),
        issuer: issuer.to_string(),
        resource: "https://api.openai.com/v1".to_string(),
        agent_name_hint: "Alan".to_string(),
        browser_callback_port: 1455,
    })
    .unwrap();
    Arc::get_mut(&mut manager.inner).unwrap().client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    manager
}

fn seed(manager: &ChatgptAuthManager, access: String) {
    manager
        .import_token_bundle(ImportedChatgptTokenBundle {
            id_token: id_token("acct-test"),
            access_token: access,
            refresh_token: "old-refresh".into(),
            client_id: "oaiapp_test".into(),
            scopes: vec![
                "openid".into(),
                "offline_access".into(),
                "resource.invoke".into(),
                "chatgpt.tokens.use.direct".into(),
            ],
        })
        .unwrap();
}

#[tokio::test]
async fn token_inside_safety_window_refreshes_before_use() {
    let server = spawn_token_server("acct-test").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    let near_expiry = Utc::now().timestamp() + 60;
    seed(&manager, access_token(near_expiry, "near-expiry"));

    let auth = manager
        .request_auth_for_account(Some("acct-test"))
        .await
        .unwrap();
    assert_eq!(auth.access_token, access_token(far_future(), "refreshed-1"));
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);
    let stored = manager.inner.storage.load().unwrap().chatgpt.unwrap();
    assert_eq!(stored.tokens.refresh_token, "rotated-1");

    // A token comfortably outside the window is used as-is.
    manager.request_auth().await.unwrap();
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn token_outside_safety_window_is_not_refreshed() {
    let server = spawn_token_server("acct-test").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    let fresh = access_token(Utc::now().timestamp() + 3600, "fresh");
    seed(&manager, fresh.clone());

    assert_eq!(manager.request_auth().await.unwrap().access_token, fresh);
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_requests_with_expired_token_share_one_refresh() {
    let server = spawn_token_server("acct-test").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    seed(&manager, access_token(1, "expired"));
    // A second manager over the same Host store models another client/Process.
    let other = manager_for(&temp, &server.issuer);

    let mut tasks = Vec::new();
    for index in 0..8 {
        let manager = if index % 2 == 0 {
            manager.clone()
        } else {
            other.clone()
        };
        tasks.push(tokio::spawn(async move {
            manager.request_auth_for_account(Some("acct-test")).await
        }));
    }
    for task in tasks {
        let auth = task.await.unwrap().unwrap();
        assert_eq!(auth.access_token, access_token(far_future(), "refreshed-1"));
    }
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rejected_token_refreshes_once_for_concurrent_unauthorized_requests() {
    let server = spawn_token_server("acct-test").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    let rejected = access_token(far_future(), "rejected");
    seed(&manager, rejected.clone());

    let mut tasks = Vec::new();
    for _ in 0..4 {
        let manager = manager.clone();
        let rejected = rejected.clone();
        tasks.push(tokio::spawn(async move {
            manager
                .refresh_rejected_auth_for_account(Some("acct-test"), &rejected)
                .await
        }));
    }
    for task in tasks {
        let auth = task.await.unwrap().unwrap();
        assert_eq!(auth.access_token, access_token(far_future(), "refreshed-1"));
    }
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);

    // A late 401 for the already-replaced token reuses the rotation.
    let late = manager
        .refresh_rejected_auth_for_account(Some("acct-test"), &rejected)
        .await
        .unwrap();
    assert_eq!(late.access_token, access_token(far_future(), "refreshed-1"));
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refresh_resolving_another_account_is_rejected_and_not_persisted() {
    let server = spawn_token_server("acct-intruder").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    seed(&manager, access_token(1, "expired"));
    let before = manager.inner.storage.load().unwrap();

    for result in [
        manager.request_auth_for_account(Some("acct-test")).await,
        manager.request_auth().await,
    ] {
        assert!(matches!(
            result,
            Err(ChatgptAuthError::WorkspaceMismatch { ref expected, ref actual })
                if expected == "acct-test" && actual.as_deref() == Some("acct-intruder")
        ));
    }
    assert_eq!(manager.inner.storage.load().unwrap(), before);
}

#[tokio::test]
async fn mismatched_account_constraint_aborts_before_refreshing() {
    let server = spawn_token_server("acct-test").await;
    let temp = TempDir::new().unwrap();
    let manager = manager_for(&temp, &server.issuer);
    seed(&manager, access_token(1, "expired"));

    let error = manager
        .request_auth_for_account(Some("acct-expected"))
        .await
        .expect_err("bound account mismatch");
    assert!(matches!(
        error,
        ChatgptAuthError::WorkspaceMismatch { ref expected, ref actual }
            if expected == "acct-expected" && actual.as_deref() == Some("acct-test")
    ));
    assert_eq!(server.refreshes.load(Ordering::SeqCst), 0);
}
