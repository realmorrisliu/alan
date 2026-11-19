use super::browser::{build_authorize_url, parse_http_request_target, read_http_request_headers};
use super::{
    BrowserLoginOptions, ChatgptAuthConfig, ChatgptAuthError, ChatgptAuthErrorKind,
    ChatgptAuthManager, ImportedChatgptTokenBundle,
};
use crate::storage::{AuthStorage, AuthStore, StoredChatgptAuth};
use crate::token_data::{ChatgptIdTokenInfo, ChatgptTokenData};
use base64::Engine;
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

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
        client_id: "client_123".to_string(),
        browser_callback_port: 1455,
    })
    .expect("manager");
    (temp_dir, manager)
}

#[test]
fn authorize_url_includes_workspace_binding_when_requested() {
    let url = build_authorize_url(
        "https://auth.example.com",
        "client_123",
        "http://localhost:1455/auth/callback",
        "challenge",
        "state",
        Some("workspace_123"),
    );
    assert!(url.contains("allowed_workspace_id=workspace_123"));
    assert!(url.contains("code_challenge=challenge"));
    assert!(url.contains("originator=codex_cli_rs"));
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
                    refresh_token: "refresh".to_string(),
                })
                .expect("stored auth"),
            ),
        })
        .expect("save");

    let snapshot = manager.status().await.expect("status").expect("snapshot");
    assert_eq!(snapshot.account_id, "acct_123");
    assert_eq!(snapshot.email.as_deref(), Some("user@example.com"));
}

#[tokio::test]
async fn browser_login_options_default_to_open_browser() {
    let options = BrowserLoginOptions::default();
    assert!(options.open_browser);
}

#[test]
fn begin_browser_login_returns_pending_flow_descriptor() {
    let (_temp_dir, manager) = test_manager();
    let pending = manager
        .begin_browser_login(BrowserLoginOptions {
            open_browser: false,
            forced_workspace_id: Some("ws_123".to_string()),
            timeout: Duration::from_secs(120),
            redirect_uri: None,
            login_id: None,
        })
        .expect("pending login");

    assert!(
        pending
            .auth_url
            .contains("https://auth.example.com/oauth/authorize")
    );
    assert!(pending.auth_url.contains("allowed_workspace_id=ws_123"));
    assert_eq!(pending.redirect_uri, "http://localhost:1455/auth/callback");
    assert!(!pending.login_id.is_empty());
}

#[test]
fn begin_browser_login_supports_host_owned_callback_descriptor() {
    let (_temp_dir, manager) = test_manager();
    let pending = manager
        .begin_browser_login(BrowserLoginOptions {
            open_browser: false,
            forced_workspace_id: None,
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
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": "pro",
            "chatgpt_user_id": "user_123",
            "chatgpt_account_id": "acct_123"
        }
    }));
    let access_token = build_jwt(json!({"exp": 4_102_444_800_i64}));

    let login = manager
        .import_token_bundle(
            ImportedChatgptTokenBundle {
                id_token,
                access_token,
                refresh_token: "refresh".to_string(),
            },
            None,
        )
        .expect("import login");

    assert_eq!(login.account_id, "acct_123");
    let snapshot = manager.status().await.expect("status").expect("snapshot");
    assert_eq!(snapshot.account_id, "acct_123");
}

#[tokio::test]
async fn import_token_bundle_requires_account_identity() {
    let (_temp_dir, manager) = test_manager();

    let error = manager
        .import_token_bundle(
            ImportedChatgptTokenBundle {
                id_token: build_jwt(json!({
                    "email": "user@example.com",
                    "https://api.openai.com/auth": {
                        "chatgpt_plan_type": "pro",
                        "chatgpt_user_id": "user_123"
                    }
                })),
                access_token: build_jwt(json!({"exp": 4_102_444_800_i64})),
                refresh_token: "refresh".to_string(),
            },
            None,
        )
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
                    refresh_token: "refresh".to_string(),
                })
                .expect("stored auth"),
            ),
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
