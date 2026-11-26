use crate::storage::{AuthStorage, StoredChatgptAuth};
use crate::token_data::{ChatgptTokenData, parse_chatgpt_jwt_claims};
use chrono::{DateTime, Utc};
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

pub use browser::{
    BrowserLoginCompletion, BrowserLoginOptions, PendingBrowserLogin, parse_manual_browser_callback,
};
pub use session::new_chatgpt_session_id;

use refresh::RefreshMode;

#[derive(Debug)]
pub struct BrowserLoginCallbackReceipt {
    pub stream: tokio::net::TcpStream,
    pub completion: BrowserLoginCompletion,
}

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
pub(crate) const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
const DEFAULT_AGENT_NAME_HINT: &str = "Alan";
const DEFAULT_RESOURCE: &str = "https://api.openai.com/v1";
pub(crate) const DIRECT_TOKEN_SCOPE: &str = "chatgpt.tokens.use.direct";
pub(crate) const REQUIRED_SCOPE: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const AUTHORIZE_PATH: &str = "/api/accounts/authorize";
const TOKEN_PATH: &str = "/api/accounts/oauth/token";
const REVOKE_PATH: &str = "/api/accounts/oauth/revoke";
const DEFAULT_BROWSER_CALLBACK_PORT: u16 = 1455;
const TOKEN_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const REVOKE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ChatgptAuthConfig {
    /// Explicit Host-owned backing; auth never infers a product or home directory.
    pub storage_path: PathBuf,
    pub issuer: String,
    pub resource: String,
    pub agent_name_hint: String,
    pub browser_callback_port: u16,
}

impl ChatgptAuthConfig {
    pub fn with_storage_path(storage_path: PathBuf) -> Self {
        Self {
            storage_path,
            issuer: DEFAULT_ISSUER.to_string(),
            resource: DEFAULT_RESOURCE.to_string(),
            agent_name_hint: DEFAULT_AGENT_NAME_HINT.to_string(),
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

#[derive(Debug, Clone)]
pub struct ChatgptLoginSuccess {
    pub account_id: String,
    pub client_id: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatgptAuthSnapshot {
    pub storage_path: PathBuf,
    pub account_id: String,
    pub client_id: String,
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
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ChatgptAuthError {
    #[error(
        "not logged in to ChatGPT; run `alan login chatgpt` or `alan connection login <profile-id>` first"
    )]
    NotLoggedIn,
    #[error("ChatGPT login did not resolve an account or subject identity")]
    MissingAccountIdentity,
    #[error(
        "ChatGPT login is bound to account `{expected}` but current login resolved `{actual:?}`"
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
            client_id: auth.client_id,
            email: auth.email,
            plan_type: auth.plan_type,
            user_id: auth.user_id,
            access_token_expires_at: auth.access_token_expires_at,
            last_refresh_at: auth.last_refresh_at,
        }))
    }

    pub async fn logout(&self) -> Result<bool, ChatgptAuthError> {
        let stored = self.inner.storage.load()?.chatgpt;
        let had_auth = stored.is_some();
        if let Some(auth) = stored
            && !auth.tokens.refresh_token.trim().is_empty()
            && auth.has_issued_client_id()
        {
            let body = form_encode(&[
                ("client_id", &auth.client_id),
                ("token", &auth.tokens.refresh_token),
                ("token_type_hint", "refresh_token"),
            ]);
            let _ = self
                .inner
                .client
                .post(format!(
                    "{}{REVOKE_PATH}",
                    self.inner.config.issuer.trim_end_matches('/')
                ))
                .timeout(REVOKE_REQUEST_TIMEOUT)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await;
        }
        self.inner.storage.clear_chatgpt()?;
        Ok(had_auth)
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
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        let persisted = self.persist_tokens(
            TokenExchangeResponse {
                id_token: bundle.id_token,
                access_token: bundle.access_token,
                refresh_token: bundle.refresh_token,
                expires_in: None,
                scope: None,
            },
            bundle.client_id,
            bundle.scopes,
            None,
        )?;
        Ok(to_login_success(&persisted))
    }

    fn persist_tokens(
        &self,
        tokens: TokenExchangeResponse,
        client_id: String,
        default_scopes: Vec<String>,
        expected_nonce: Option<&str>,
    ) -> Result<StoredChatgptAuth, ChatgptAuthError> {
        let id_token = parse_chatgpt_id_token(&tokens.id_token)?;
        if let (Some(expected), Some(actual)) = (expected_nonce, id_token.nonce.as_deref())
            && actual != expected
        {
            return Err(ChatgptAuthError::LoginFailed(
                "ChatGPT ID token nonce mismatch".to_string(),
            ));
        }
        let scopes = tokens
            .scope
            .as_deref()
            .map(parse_scopes)
            .filter(|parsed| !parsed.is_empty())
            .unwrap_or(default_scopes);
        let mut persisted = StoredChatgptAuth::from_tokens(
            ChatgptTokenData {
                id_token,
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            },
            client_id,
            scopes,
        )?;
        if persisted.access_token_expires_at.is_none()
            && let Some(expires_in) = tokens.expires_in.filter(|secs| *secs >= 0)
        {
            persisted.access_token_expires_at =
                Some(Utc::now() + chrono::Duration::seconds(expires_in));
        }
        let persisted_clone = persisted.clone();
        self.inner.storage.update(move |store| {
            store.chatgpt = Some(persisted_clone);
        })?;
        Ok(persisted)
    }

    async fn exchange_code_for_tokens(
        &self,
        client_id: &str,
        redirect_uri: &str,
        code_verifier: &str,
        code: &str,
    ) -> Result<TokenExchangeResponse, ChatgptAuthError> {
        let body = form_encode(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", code_verifier),
            ("redirect_uri", redirect_uri),
            ("resource", &self.inner.config.resource),
        ]);
        let response = self
            .inner
            .client
            .post(format!(
                "{}{TOKEN_PATH}",
                self.inner.config.issuer.trim_end_matches('/')
            ))
            .timeout(TOKEN_REQUEST_TIMEOUT)
            .header("Accept", "application/json")
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
        let payload: TokenExchangeResponse = response.json().await?;
        let granted_scopes = payload
            .scope
            .as_deref()
            .map(parse_scopes)
            .unwrap_or_default();
        if !granted_scopes
            .iter()
            .any(|scope| scope == DIRECT_TOKEN_SCOPE)
        {
            return Err(ChatgptAuthError::LoginFailed(format!(
                "OpenAI OAuth grant did not include `{DIRECT_TOKEN_SCOPE}` (plan usage permission is required)"
            )));
        }
        Ok(payload)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenExchangeResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

pub(crate) fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub(crate) fn parse_scopes(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_chatgpt_id_token(
    jwt: &str,
) -> Result<crate::token_data::ChatgptIdTokenInfo, ChatgptAuthError> {
    let info = parse_chatgpt_jwt_claims(jwt)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if info.effective_account_id().is_none() {
        return Err(ChatgptAuthError::MissingAccountIdentity);
    }
    Ok(info)
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
        client_id: auth.client_id.clone(),
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
