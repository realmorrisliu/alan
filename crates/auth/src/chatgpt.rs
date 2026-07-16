use crate::storage::{AuthStorage, StoredChatgptAuth};
use crate::token_data::{ChatgptTokenData, parse_chatgpt_jwt_claims};
use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

mod browser;

pub use browser::{BrowserLoginCompletion, BrowserLoginOptions, PendingBrowserLogin};

#[derive(Debug)]
pub struct BrowserLoginCallbackReceipt {
    pub stream: tokio::net::TcpStream,
    pub completion: BrowserLoginCompletion,
}

const INSTALL_CHANNEL_ENV: &str = "ALAN_INSTALL_CHANNEL";
const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_BROWSER_CALLBACK_PORT: u16 = 1455;

#[derive(Debug, Clone)]
pub struct ChatgptAuthConfig {
    pub storage_path: PathBuf,
    pub issuer: String,
    pub client_id: String,
    pub browser_callback_port: u16,
}

impl ChatgptAuthConfig {
    pub fn detect() -> io::Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| io::Error::other("Could not determine application data directory"))?;
        Ok(Self::with_storage_path(default_host_store_auth_path(
            &data_dir,
            detected_install_channel_id(),
        )))
    }

    pub fn with_storage_path(storage_path: PathBuf) -> Self {
        Self {
            storage_path,
            issuer: DEFAULT_ISSUER.to_string(),
            client_id: DEFAULT_CLIENT_ID.to_string(),
            browser_callback_port: DEFAULT_BROWSER_CALLBACK_PORT,
        }
    }
}

fn detected_install_channel_id() -> &'static str {
    if let Ok(channel) = std::env::var(INSTALL_CHANNEL_ENV) {
        match channel.trim() {
            "dev" => return "dev",
            "stable" => return "stable",
            _ => {}
        }
    }

    let argv0 = std::env::args_os().next();
    let raw_executable_name = argv0
        .as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let executable_name = raw_executable_name
        .strip_suffix(".exe")
        .unwrap_or(raw_executable_name);

    if matches!(executable_name, "alan-dev") {
        "dev"
    } else {
        "stable"
    }
}

fn default_host_store_auth_path(data_dir: &Path, channel: &str) -> PathBuf {
    data_dir
        .join("Alan")
        .join("Host Store")
        .join(channel)
        .join("auth.json")
}

#[derive(Debug, Clone)]
pub struct ChatgptAuthManager {
    inner: Arc<ChatgptAuthManagerInner>,
}

#[derive(Debug)]
struct ChatgptAuthManagerInner {
    config: ChatgptAuthConfig,
    storage: AuthStorage,
    client: reqwest::Client,
    refresh_lock: Mutex<()>,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceCodeLoginOptions {
    pub forced_workspace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeviceCodePrompt {
    pub verification_url: String,
    pub user_code: String,
    device_auth_id: String,
    interval_secs: u64,
}

impl DeviceCodePrompt {
    pub fn interval_secs(&self) -> u64 {
        self.interval_secs
    }

    pub fn device_auth_id(&self) -> &str {
        &self.device_auth_id
    }
}

#[derive(Debug, Clone)]
pub struct ChatgptLoginSuccess {
    pub account_id: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatgptAuthSnapshot {
    pub storage_path: PathBuf,
    pub account_id: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub user_id: Option<String>,
    pub access_token_expires_at: Option<DateTime<Utc>>,
    pub last_refresh_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ChatgptRequestAuth {
    pub access_token: String,
    pub account_id: String,
}

#[derive(Debug, Clone)]
pub struct ImportedChatgptTokenBundle {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Error)]
pub enum ChatgptAuthError {
    #[error("not logged in to ChatGPT; run `alan connection login <profile-id> browser` first")]
    NotLoggedIn,
    #[error("ChatGPT login did not resolve an account/workspace identity")]
    MissingAccountIdentity,
    #[error(
        "ChatGPT login is bound to workspace/account `{expected}` but current login resolved `{actual:?}`"
    )]
    WorkspaceMismatch {
        expected: String,
        actual: Option<String>,
    },
    #[error("ChatGPT token is expired and refresh is required")]
    TokenExpired,
    #[error("ChatGPT token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("ChatGPT request remained unauthorized after refresh: {0}")]
    UnauthorizedAfterRefresh(String),
    #[error("ChatGPT login failed: {0}")]
    LoginFailed(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatgptAuthErrorKind {
    NotLoggedIn,
    MissingAccountIdentity,
    WorkspaceMismatch,
    TokenExpired,
    RefreshFailed,
    UnauthorizedAfterRefresh,
    LoginFailed,
}

impl ChatgptAuthError {
    pub fn kind(&self) -> Option<ChatgptAuthErrorKind> {
        match self {
            Self::NotLoggedIn => Some(ChatgptAuthErrorKind::NotLoggedIn),
            Self::MissingAccountIdentity => Some(ChatgptAuthErrorKind::MissingAccountIdentity),
            Self::WorkspaceMismatch { .. } => Some(ChatgptAuthErrorKind::WorkspaceMismatch),
            Self::TokenExpired => Some(ChatgptAuthErrorKind::TokenExpired),
            Self::RefreshFailed(_) => Some(ChatgptAuthErrorKind::RefreshFailed),
            Self::UnauthorizedAfterRefresh(_) => {
                Some(ChatgptAuthErrorKind::UnauthorizedAfterRefresh)
            }
            Self::LoginFailed(_) => Some(ChatgptAuthErrorKind::LoginFailed),
            Self::Io(_) | Self::Http(_) | Self::Json(_) | Self::Url(_) => None,
        }
    }
}

impl ChatgptAuthManager {
    pub fn detect() -> io::Result<Self> {
        Self::new(ChatgptAuthConfig::detect()?)
    }

    pub fn new(config: ChatgptAuthConfig) -> io::Result<Self> {
        Ok(Self {
            inner: Arc::new(ChatgptAuthManagerInner {
                storage: AuthStorage::new(config.storage_path.clone())?,
                config,
                client: reqwest::Client::new(),
                refresh_lock: Mutex::new(()),
            }),
        })
    }

    pub fn storage_path(&self) -> &std::path::Path {
        &self.inner.config.storage_path
    }

    pub fn issuer(&self) -> &str {
        &self.inner.config.issuer
    }

    pub async fn status(&self) -> Result<Option<ChatgptAuthSnapshot>, ChatgptAuthError> {
        let store = self.inner.storage.load()?;
        Ok(store.chatgpt.map(|auth| ChatgptAuthSnapshot {
            storage_path: self.inner.config.storage_path.clone(),
            account_id: auth.account_id,
            email: auth.email,
            plan_type: auth.plan_type,
            user_id: auth.user_id,
            access_token_expires_at: auth.access_token_expires_at,
            last_refresh_at: auth.last_refresh_at,
        }))
    }

    pub async fn logout(&self) -> Result<bool, ChatgptAuthError> {
        let had_auth = self.inner.storage.load()?.chatgpt.is_some();
        self.inner.storage.clear_chatgpt()?;
        Ok(had_auth)
    }

    pub async fn start_device_code(&self) -> Result<DeviceCodePrompt, ChatgptAuthError> {
        #[derive(Deserialize)]
        struct DeviceCodeResponse {
            device_auth_id: String,
            #[serde(alias = "user_code", alias = "usercode")]
            user_code: String,
            interval: String,
        }

        let url = format!(
            "{}/api/accounts/deviceauth/usercode",
            self.inner.config.issuer.trim_end_matches('/')
        );
        let response = self
            .inner
            .client
            .post(url)
            .json(&serde_json::json!({ "client_id": self.inner.config.client_id }))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ChatgptAuthError::LoginFailed(format!(
                "device code request failed ({status}): {body}"
            )));
        }
        let payload: DeviceCodeResponse = response.json().await?;
        Ok(DeviceCodePrompt {
            verification_url: format!(
                "{}/codex/device",
                self.inner.config.issuer.trim_end_matches('/')
            ),
            user_code: payload.user_code,
            device_auth_id: payload.device_auth_id,
            interval_secs: payload.interval.trim().parse::<u64>().unwrap_or(5),
        })
    }

    pub async fn complete_device_code(
        &self,
        prompt: &DeviceCodePrompt,
        options: DeviceCodeLoginOptions,
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        #[derive(Deserialize)]
        struct DeviceCodeTokenResponse {
            authorization_code: String,
            code_verifier: String,
        }

        let url = format!(
            "{}/api/accounts/deviceauth/token",
            self.inner.config.issuer.trim_end_matches('/')
        );
        let started_at = std::time::Instant::now();
        let max_wait = Duration::from_secs(15 * 60);

        loop {
            let response = self
                .inner
                .client
                .post(&url)
                .json(&serde_json::json!({
                    "device_auth_id": prompt.device_auth_id,
                    "user_code": prompt.user_code,
                }))
                .send()
                .await?;

            if response.status().is_success() {
                let code: DeviceCodeTokenResponse = response.json().await?;
                let redirect_uri = format!(
                    "{}/deviceauth/callback",
                    self.inner.config.issuer.trim_end_matches('/')
                );
                let tokens = self
                    .exchange_code_for_tokens(
                        &redirect_uri,
                        &code.code_verifier,
                        &code.authorization_code,
                    )
                    .await?;
                let persisted =
                    self.persist_tokens(tokens, options.forced_workspace_id.as_deref())?;
                return Ok(to_login_success(&persisted));
            }

            match response.status() {
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => {
                    if started_at.elapsed() >= max_wait {
                        return Err(ChatgptAuthError::Io(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "device code login timed out after 15 minutes",
                        )));
                    }
                    tokio::time::sleep(Duration::from_secs(prompt.interval_secs)).await;
                }
                status => {
                    let body = response.text().await.unwrap_or_default();
                    return Err(ChatgptAuthError::LoginFailed(format!(
                        "device code login failed ({status}): {body}"
                    )));
                }
            }
        }
    }

    pub async fn request_auth(&self) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.request_auth_for_account(None).await
    }

    pub async fn request_auth_for_account(
        &self,
        expected_account_id: Option<&str>,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        let auth = self.refresh_if_needed(false).await?;
        ensure_expected_account_matches(expected_account_id, &auth)?;
        Ok(ChatgptRequestAuth {
            access_token: auth.tokens.access_token,
            account_id: auth.account_id,
        })
    }

    pub async fn force_refresh_auth(&self) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.force_refresh_auth_for_account(None).await
    }

    pub async fn force_refresh_auth_for_account(
        &self,
        expected_account_id: Option<&str>,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        let auth = self.refresh_if_needed(true).await?;
        ensure_expected_account_matches(expected_account_id, &auth)?;
        Ok(ChatgptRequestAuth {
            access_token: auth.tokens.access_token,
            account_id: auth.account_id,
        })
    }

    pub fn import_token_bundle(
        &self,
        bundle: ImportedChatgptTokenBundle,
        forced_workspace_id: Option<&str>,
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        let persisted = self.persist_tokens(
            TokenExchangeResponse {
                id_token: bundle.id_token,
                access_token: bundle.access_token,
                refresh_token: bundle.refresh_token,
            },
            forced_workspace_id,
        )?;
        Ok(to_login_success(&persisted))
    }

    fn persist_tokens(
        &self,
        tokens: TokenExchangeResponse,
        forced_workspace_id: Option<&str>,
    ) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        if let Some(expected) = forced_workspace_id {
            ensure_workspace_allowed(expected, &tokens.id_token)?;
        }

        let id_token = parse_chatgpt_id_token(&tokens.id_token)?;
        let persisted = StoredChatgptAuth::from_tokens(ChatgptTokenData {
            id_token,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        })?;
        let mut store = self.inner.storage.load()?;
        store.version = 1;
        store.chatgpt = Some(persisted.clone());
        self.inner.storage.save(&store)?;
        Ok(persisted)
    }

    async fn refresh_if_needed(&self, force: bool) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        let maybe_auth = self.inner.storage.load()?.chatgpt;
        let auth = maybe_auth.ok_or(ChatgptAuthError::NotLoggedIn)?;
        let now = Utc::now();

        if !force && !auth.should_refresh(now) {
            return Ok(auth);
        }

        let _guard = self.inner.refresh_lock.lock().await;
        let auth = self
            .inner
            .storage
            .load()?
            .chatgpt
            .ok_or(ChatgptAuthError::NotLoggedIn)?;
        if !force && !auth.should_refresh(Utc::now()) {
            return Ok(auth);
        }

        if auth.tokens.refresh_token.trim().is_empty() {
            return Err(ChatgptAuthError::TokenExpired);
        }

        self.refresh_inner(auth.tokens.refresh_token.clone()).await
    }

    async fn refresh_inner(
        &self,
        refresh_token: String,
    ) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        #[derive(Deserialize)]
        struct RefreshResponse {
            id_token: Option<String>,
            access_token: String,
            refresh_token: Option<String>,
        }

        let response = self
            .inner
            .client
            .post(format!(
                "{}/oauth/token",
                self.inner.config.issuer.trim_end_matches('/')
            ))
            .json(&serde_json::json!({
                "client_id": self.inner.config.client_id,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let error = if status == StatusCode::UNAUTHORIZED {
                ChatgptAuthError::RefreshFailed(body)
            } else {
                ChatgptAuthError::RefreshFailed(format!("{status}: {body}"))
            };
            return Err(error);
        }

        let refreshed: RefreshResponse = response.json().await?;
        let mut store = self.inner.storage.load()?;
        let existing = store.chatgpt.take().ok_or(ChatgptAuthError::NotLoggedIn)?;
        let id_token = refreshed
            .id_token
            .unwrap_or_else(|| existing.tokens.id_token.raw_jwt.clone());
        let access_token = refreshed.access_token;
        let refresh_token = refreshed
            .refresh_token
            .unwrap_or_else(|| existing.tokens.refresh_token.clone());
        let persisted = StoredChatgptAuth::from_tokens(ChatgptTokenData {
            id_token: parse_chatgpt_id_token(&id_token)?,
            access_token,
            refresh_token,
        })?;
        store.version = 1;
        store.chatgpt = Some(persisted.clone());
        self.inner.storage.save(&store)?;
        Ok(persisted)
    }

    async fn exchange_code_for_tokens(
        &self,
        redirect_uri: &str,
        code_verifier: &str,
        code: &str,
    ) -> Result<TokenExchangeResponse, ChatgptAuthError> {
        let body = format!(
            "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
            urlencoding::encode(code),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&self.inner.config.client_id),
            urlencoding::encode(code_verifier),
        );
        let response = self
            .inner
            .client
            .post(format!(
                "{}/oauth/token",
                self.inner.config.issuer.trim_end_matches('/')
            ))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ChatgptAuthError::LoginFailed(format!(
                "token endpoint returned {status}: {body}"
            )));
        }
        Ok(response.json().await?)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

fn parse_chatgpt_id_token(
    jwt: &str,
) -> Result<crate::token_data::ChatgptIdTokenInfo, ChatgptAuthError> {
    let info = parse_chatgpt_jwt_claims(jwt)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if info.account_id.is_none() {
        return Err(ChatgptAuthError::MissingAccountIdentity);
    }
    Ok(info)
}

fn ensure_workspace_allowed(expected: &str, id_token: &str) -> Result<(), ChatgptAuthError> {
    let info = parse_chatgpt_jwt_claims(id_token)
        .with_context(|| "Failed to parse id token workspace claims")
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if info.account_id.as_deref() == Some(expected) {
        return Ok(());
    }
    Err(ChatgptAuthError::WorkspaceMismatch {
        expected: expected.to_string(),
        actual: info.account_id,
    })
}

fn ensure_expected_account_matches(
    expected_account_id: Option<&str>,
    auth: &StoredChatgptAuth,
) -> Result<(), ChatgptAuthError> {
    let Some(expected_account_id) = expected_account_id else {
        return Ok(());
    };

    if auth.account_id == expected_account_id {
        return Ok(());
    }

    Err(ChatgptAuthError::WorkspaceMismatch {
        expected: expected_account_id.to_string(),
        actual: Some(auth.account_id.clone()),
    })
}

fn to_login_success(auth: &StoredChatgptAuth) -> ChatgptLoginSuccess {
    ChatgptLoginSuccess {
        account_id: auth.account_id.clone(),
        email: auth.email.clone(),
        plan_type: auth.plan_type.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::browser::{
        build_authorize_url, parse_http_request_target, read_http_request_headers,
    };
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

    fn build_jwt(payload: serde_json::Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
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
}
