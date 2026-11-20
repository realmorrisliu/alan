use crate::storage::{AuthStorage, StoredChatgptAuth};
use crate::token_data::{ChatgptTokenData, parse_chatgpt_jwt_claims};
use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

mod browser;
mod refresh;
mod session;

pub use browser::{BrowserLoginCompletion, BrowserLoginOptions, PendingBrowserLogin};
pub use session::new_chatgpt_session_id;

use refresh::RefreshMode;

#[derive(Debug)]
pub struct BrowserLoginCallbackReceipt {
    pub stream: tokio::net::TcpStream,
    pub completion: BrowserLoginCompletion,
}

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEFAULT_BROWSER_CALLBACK_PORT: u16 = 1455;

#[derive(Debug, Clone)]
pub struct ChatgptAuthConfig {
    /// Explicit Host-owned backing; auth never infers a product or home directory.
    pub storage_path: PathBuf,
    pub issuer: String,
    pub client_id: String,
    pub browser_callback_port: u16,
}

impl ChatgptAuthConfig {
    pub fn with_storage_path(storage_path: PathBuf) -> Self {
        Self {
            storage_path,
            issuer: DEFAULT_ISSUER.to_string(),
            client_id: DEFAULT_CLIENT_ID.to_string(),
            browser_callback_port: DEFAULT_BROWSER_CALLBACK_PORT,
        }
    }
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

    /// Bind a callable to the currently stored account without refreshing credentials.
    /// Later requests must still validate this identity after any managed refresh.
    pub fn bound_account_id(
        &self,
        expected_account_id: Option<&str>,
    ) -> Result<Option<String>, ChatgptAuthError> {
        let Some(auth) = self.inner.storage.load()?.chatgpt else {
            return Ok(expected_account_id.map(str::to_string));
        };
        ensure_expected_account_matches(expected_account_id, &auth)?;
        Ok(Some(auth.account_id))
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

    /// Return credentials for a request, refreshing first when the access token is
    /// expired or expires within the safety window.
    pub async fn request_auth_for_account(
        &self,
        expected_account_id: Option<&str>,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.request_auth_with_mode(expected_account_id, RefreshMode::IfNeeded)
            .await
    }

    pub async fn force_refresh_auth(&self) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.force_refresh_auth_for_account(None).await
    }

    pub async fn force_refresh_auth_for_account(
        &self,
        expected_account_id: Option<&str>,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.request_auth_with_mode(expected_account_id, RefreshMode::Force)
            .await
    }

    /// Recover from a backend `401` for `rejected_access_token`.
    ///
    /// Refreshes only while the rejected token is still the stored one, so concurrent
    /// requests that were all rejected with the same token share a single refresh.
    pub async fn refresh_rejected_auth_for_account(
        &self,
        expected_account_id: Option<&str>,
        rejected_access_token: &str,
    ) -> Result<ChatgptRequestAuth, ChatgptAuthError> {
        self.request_auth_with_mode(
            expected_account_id,
            RefreshMode::Rejected(rejected_access_token),
        )
        .await
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
        let persisted_clone = persisted.clone();
        self.inner.storage.update(move |store| {
            store.chatgpt = Some(persisted_clone);
        })?;
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
mod tests;

#[cfg(test)]
mod concurrency_tests;

#[cfg(test)]
mod refresh_tests;
