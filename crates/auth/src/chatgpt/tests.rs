use super::browser::{
    build_authorize_url, parse_http_request_target, parse_manual_browser_callback,
    read_http_request_headers,
};
use super::{
    BrowserLoginCompletion, BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthError,
    ChatgptAuthErrorKind, ChatgptAuthManager, DYNAMIC_CLIENT_ID, ImportedChatgptTokenBundle,
    REQUIRED_SCOPE,
};
use crate::storage::{AuthStorage, AuthStore, StoredChatgptAuth};
use crate::token_data::{ChatgptIdTokenInfo, ChatgptTokenData};
use base64::Engine;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(super) fn build_jwt(payload: serde_json::Value) -> String {
    let header =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.sig")
}

fn test_manager() -> (TempDir, ChatgptAuthManager) {
    let temp_dir = TempDir::new().expect("temp dir");
    let manager = ChatgptAuthManager::new(ChatgptAuthConfig {
        storage_path: temp_dir.path().join("auth.json"),
        issuer: "https://auth.example.com".to_string(),
        resource: "https://api.openai.com/v1".to_string(),
        agent_name_hint: "Alan".to_string(),
        browser_callback_port: 1455,
    })
    .expect("manager");
    (temp_dir, manager)
}

#[test]
fn authorize_url_uses_siwc_dynamic_registration_and_resource_scope() {
    let url = build_authorize_url(
        "https://auth.example.com",
        "Alan",
        "urn:uuid:e61bbe28-07ef-466d-8e5d-a344f94ab305",
        "https://api.openai.com/v1",
        "http://127.0.0.1:1455/auth/callback",
        "challenge",
        "state_xyz",
        "nonce_xyz",
    );
    let parsed = url::Url::parse(&url).expect("valid authorize URL");
    assert_eq!(parsed.path(), "/api/accounts/authorize");
    let params: std::collections::HashMap<String, String> = parsed
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    assert_eq!(
        params.get("client_id").map(String::as_str),
        Some("dynamic_agent_client")
    );
    assert_eq!(
        params.get("agent_name_hint").map(String::as_str),
        Some("Alan")
    );
    assert_eq!(
        params.get("ext_agent_host_id").map(String::as_str),
        Some("urn:uuid:e61bbe28-07ef-466d-8e5d-a344f94ab305")
    );
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("http://127.0.0.1:1455/auth/callback")
    );
    assert_eq!(
        params.get("resource").map(String::as_str),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(
        params.get("scope").map(String::as_str),
        Some(REQUIRED_SCOPE)
    );
    assert_eq!(params.get("state").map(String::as_str), Some("state_xyz"));
    assert_eq!(params.get("nonce").map(String::as_str), Some("nonce_xyz"));
    assert_eq!(
        params.get("code_challenge").map(String::as_str),
        Some("challenge")
    );
    assert_eq!(
        params.get("code_challenge_method").map(String::as_str),
        Some("S256")
    );
    assert!(!params.contains_key("originator"));
    assert!(!params.contains_key("codex_cli_simplified_flow"));
}

#[tokio::test]
async fn status_returns_none_when_not_logged_in() {
    let (_temp_dir, manager) = test_manager();
    assert!(manager.status().await.expect("status").is_none());
}

#[tokio::test]
async fn request_auth_requires_login() {
    let (_temp_dir, manager) = test_manager();
    let error = manager.request_auth().await.expect_err("missing auth");
    assert!(matches!(error, ChatgptAuthError::NotLoggedIn));
    assert_eq!(error.kind(), Some(ChatgptAuthErrorKind::NotLoggedIn));
}

#[tokio::test]
async fn status_reports_saved_login() {
    let (_temp_dir, manager) = test_manager();
    let storage = AuthStorage::new(manager.storage_path().to_path_buf()).expect("storage");
    let id_token = build_jwt(json!({
        "sub": "sub_123",
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));
    storage
        .save(&AuthStore {
            version: 1,
            ext_agent_host_id: Some("urn:uuid:e61bbe28-07ef-466d-8e5d-a344f94ab305".into()),
            chatgpt: Some(
                StoredChatgptAuth::from_tokens(
                    ChatgptTokenData {
                        id_token: ChatgptIdTokenInfo {
                            email: Some("user@example.com".to_string()),
                            plan_type: Some("pro".to_string()),
                            user_id: Some("user_123".to_string()),
                            account_id: Some("acct_123".to_string()),
                            subject: Some("sub_123".to_string()),
                            nonce: None,
                            raw_jwt: id_token,
                        },
                        access_token,
                        refresh_token: "refresh".to_string(),
                    },
                    "oaiapp_123".to_string(),
                    vec!["chatgpt.tokens.use.direct".to_string()],
                )
                .expect("stored auth"),
            ),
            google: None,
        })
        .expect("save");

    let snapshot = manager.status().await.expect("status").expect("snapshot");
    assert_eq!(snapshot.account_id, "acct_123");
    assert_eq!(snapshot.client_id, "oaiapp_123");
    assert_eq!(snapshot.email.as_deref(), Some("user@example.com"));
}

#[tokio::test]
async fn browser_login_options_default_to_open_browser() {
    let options = BrowserLoginOptions::default();
    assert!(options.open_browser);
}

#[test]
fn begin_browser_login_uses_127_0_0_1_and_persists_ext_agent_host_id() {
    let (_temp_dir, manager) = test_manager();
    let pending = manager
        .begin_browser_login(BrowserLoginOptions {
            open_browser: false,
            timeout: Duration::from_secs(120),
            redirect_uri: None,
            login_id: None,
        })
        .expect("pending login");

    assert!(
        pending
            .auth_url
            .starts_with("https://auth.example.com/api/accounts/authorize?")
    );
    assert!(pending.auth_url.contains("client_id=dynamic_agent_client"));
    assert!(pending.auth_url.contains("agent_name_hint=Alan"));
    assert!(pending.auth_url.contains("ext_agent_host_id=urn%3Auuid%3A"));
    assert!(pending.auth_url.contains("chatgpt.tokens.use.direct"));
    assert_eq!(pending.redirect_uri, "http://127.0.0.1:1455/auth/callback");
    assert!(!pending.login_id.is_empty());
    assert!(!pending.nonce.is_empty());
}

#[test]
fn begin_browser_login_supports_host_owned_callback_descriptor() {
    let (_temp_dir, manager) = test_manager();
    let pending = manager
        .begin_browser_login(BrowserLoginOptions {
            open_browser: false,
            timeout: Duration::from_secs(120),
            redirect_uri: Some(
                "https://alan.example.com/custom/browser/callback/browser_test".to_string(),
            ),
            login_id: Some("browser_test".to_string()),
        })
        .expect("pending login");

    assert_eq!(pending.login_id, "browser_test");
    assert_eq!(
        pending.redirect_uri,
        "https://alan.example.com/custom/browser/callback/browser_test"
    );
    assert!(pending.auth_url.contains(
        "redirect_uri=https%3A%2F%2Falan.example.com%2Fcustom%2Fbrowser%2Fcallback%2Fbrowser_test"
    ));
}

#[test]
fn manual_callback_parser_validates_issued_client_id_and_state() {
    let (_temp_dir, manager) = test_manager();
    let pending = manager
        .begin_browser_login(BrowserLoginOptions::default())
        .unwrap();

    let ok = parse_manual_browser_callback(
        &pending,
        &format!(
            "http://127.0.0.1:1455/auth/callback?code=code_1&state={}&client_id=oaiapp_issued",
            pending.state
        ),
    )
    .unwrap();
    assert_eq!(ok.code, "code_1");
    assert_eq!(ok.client_id, "oaiapp_issued");

    // Missing issued client_id is rejected
    let missing_client = parse_manual_browser_callback(
        &pending,
        &format!(
            "http://127.0.0.1:1455/auth/callback?code=code_1&state={}",
            pending.state
        ),
    )
    .unwrap_err();
    assert!(missing_client.to_string().contains("issued client_id"));

    // Placeholder dynamic_agent_client is rejected
    let placeholder_client = parse_manual_browser_callback(
        &pending,
        &format!(
            "http://127.0.0.1:1455/auth/callback?code=code_1&state={}&client_id={DYNAMIC_CLIENT_ID}",
            pending.state
        ),
    )
    .unwrap_err();
    assert!(
        placeholder_client
            .to_string()
            .contains("dynamic_agent_client")
    );
}

#[tokio::test]
async fn complete_browser_login_exchanges_form_encoded_tokens_and_checks_scope() {
    let temp = TempDir::new().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let seen_bodies: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen_clone = seen_bodies.clone();

    let mut manager = ChatgptAuthManager::new(ChatgptAuthConfig {
        storage_path: temp.path().join("auth.json"),
        issuer,
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

    let pending = manager
        .begin_browser_login(BrowserLoginOptions::default())
        .unwrap();
    let nonce = pending.nonce.clone();

    let server = tokio::spawn(async move {
        for scope in [
            "openid profile email", // First call: missing chatgpt.tokens.use.direct -> rejected
            REQUIRED_SCOPE,         // Second call: valid scope
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            loop {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                let s = String::from_utf8_lossy(&req);
                if let Some(end) = s.find("\r\n\r\n") {
                    let len = s[..end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if req.len() >= end + 4 + len {
                        seen_clone
                            .lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&req[end + 4..]).into_owned());
                        assert!(s.starts_with("POST /api/accounts/oauth/token "));
                        break;
                    }
                }
            }
            let body = json!({
                "id_token": build_jwt(json!({
                    "sub": "sub_user_42",
                    "nonce": nonce,
                    "email": "siwc@example.com"
                })),
                "access_token": "opaque_access_token",
                "refresh_token": "refresh_42",
                "expires_in": 3600,
                "scope": scope
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        }
    });

    let completion = BrowserLoginCompletion {
        code: "auth_code_1".into(),
        state: pending.state.clone(),
        client_id: "oaiapp_issued_1".into(),
    };
    let err = manager
        .complete_browser_login(&pending, completion.clone())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("chatgpt.tokens.use.direct"));
    assert!(manager.status().await.unwrap().is_none());

    let ok = manager
        .complete_browser_login(&pending, completion)
        .await
        .unwrap();
    assert_eq!(ok.account_id, "sub_user_42");
    assert_eq!(ok.client_id, "oaiapp_issued_1");
    assert_eq!(ok.email.as_deref(), Some("siwc@example.com"));

    server.await.unwrap();

    let bodies = seen_bodies.lock().unwrap();
    assert!(bodies[1].contains("grant_type=authorization_code"));
    assert!(bodies[1].contains("client_id=oaiapp_issued_1"));
    assert!(bodies[1].contains("resource=https%3A%2F%2Fapi.openai.com%2Fv1"));
}

#[tokio::test]
async fn logout_revokes_refresh_token_and_preserves_ext_agent_host_id() {
    let temp = TempDir::new().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let mut manager = ChatgptAuthManager::new(ChatgptAuthConfig {
        storage_path: temp.path().join("auth.json"),
        issuer,
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

    let host_id = manager.inner.storage.ensure_ext_agent_host_id().unwrap();
    manager
        .import_token_bundle(ImportedChatgptTokenBundle {
            id_token: build_jwt(json!({"sub": "sub_1", "email": "a@example.com"})),
            access_token: build_jwt(json!({"exp": 4_102_444_800_i64})),
            refresh_token: "rt_revoke_me".into(),
            client_id: "oaiapp_revoke".into(),
            scopes: vec!["chatgpt.tokens.use.direct".into()],
        })
        .unwrap();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let raw = String::from_utf8_lossy(&buf[..n]);
        assert!(raw.starts_with("POST /api/accounts/oauth/revoke "));
        assert!(raw.contains("client_id=oaiapp_revoke"));
        assert!(raw.contains("token=rt_revoke_me"));
        assert!(raw.contains("token_type_hint=refresh_token"));
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
    });

    assert!(manager.logout().await.unwrap());
    server.await.unwrap();
    let store = manager.inner.storage.load().unwrap();
    assert!(store.chatgpt.is_none());
    assert_eq!(store.ext_agent_host_id.as_deref(), Some(host_id.as_str()));
}

#[tokio::test]
async fn read_http_request_headers_waits_for_complete_header_block() {
    let (mut writer, mut reader) = tokio::io::duplex(256);
    let writer_task = tokio::spawn(async move {
        writer
            .write_all(b"GET /auth/callback?code=abc")
            .await
            .expect("partial request");
        writer
            .write_all(b"&state=xyz HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("remaining request");
    });

    let request = read_http_request_headers(&mut reader)
        .await
        .expect("request headers");
    writer_task.await.expect("writer task");

    assert_eq!(
        parse_http_request_target(&request),
        Some("/auth/callback?code=abc&state=xyz".to_string())
    );
}

#[tokio::test]
async fn import_token_bundle_persists_login_state() {
    let (_temp_dir, manager) = test_manager();
    let id_token = build_jwt(json!({
        "sub": "sub_123",
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));

    let login = manager
        .import_token_bundle(ImportedChatgptTokenBundle {
            id_token,
            access_token,
            refresh_token: "refresh".to_string(),
            client_id: "oaiapp_123".to_string(),
            scopes: vec!["chatgpt.tokens.use.direct".to_string()],
        })
        .expect("import login");

    assert_eq!(login.account_id, "acct_123");
    assert_eq!(login.client_id, "oaiapp_123");
    let snapshot = manager.status().await.expect("status").expect("snapshot");
    assert_eq!(snapshot.account_id, "acct_123");
}

#[tokio::test]
async fn import_token_bundle_requires_account_identity() {
    let (_temp_dir, manager) = test_manager();

    let error = manager
        .import_token_bundle(ImportedChatgptTokenBundle {
            id_token: build_jwt(json!({
                "email": "user@example.com",
                "https://api.openai.com/auth": {
                    "chatgpt_plan_type": "pro",
                    "chatgpt_user_id": "user_123"
                }
            })),
            access_token: build_jwt(json!({"exp": 4_102_444_800_i64})),
            refresh_token: "refresh".to_string(),
            client_id: "oaiapp_123".to_string(),
            scopes: vec!["chatgpt.tokens.use.direct".to_string()],
        })
        .expect_err("missing account identity");

    assert!(matches!(error, ChatgptAuthError::MissingAccountIdentity));
    assert_eq!(
        error.kind(),
        Some(ChatgptAuthErrorKind::MissingAccountIdentity)
    );
}

#[tokio::test]
async fn request_auth_rejects_mismatched_account_constraint() {
    let (_temp_dir, manager) = test_manager();
    let storage = AuthStorage::new(manager.storage_path().to_path_buf()).expect("storage");
    let id_token = build_jwt(json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));
    storage
        .save(&AuthStore {
            version: 1,
            ext_agent_host_id: None,
            chatgpt: Some(
                StoredChatgptAuth::from_tokens(
                    ChatgptTokenData {
                        id_token: ChatgptIdTokenInfo {
                            email: Some("user@example.com".to_string()),
                            plan_type: Some("pro".to_string()),
                            user_id: Some("user_123".to_string()),
                            account_id: Some("acct_123".to_string()),
                            subject: None,
                            nonce: None,
                            raw_jwt: id_token,
                        },
                        access_token,
                        refresh_token: "refresh".to_string(),
                    },
                    "oaiapp_123".to_string(),
                    vec!["chatgpt.tokens.use.direct".to_string()],
                )
                .expect("stored auth"),
            ),
            google: None,
        })
        .expect("save");

    let error = manager
        .request_auth_for_account(Some("acct_other"))
        .await
        .expect_err("workspace mismatch");
    assert!(matches!(
        error,
        ChatgptAuthError::WorkspaceMismatch { ref expected, ref actual }
        if expected == "acct_other" && actual.as_deref() == Some("acct_123")
    ));
    assert_eq!(error.kind(), Some(ChatgptAuthErrorKind::WorkspaceMismatch));
}
