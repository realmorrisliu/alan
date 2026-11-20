//! Google Cloud Vertex AI authentication: Desktop OAuth2 PKCE, single-flight
//! refresh, standard ADC `authorized_user` exchange, `gcloud` fallback, and active
//! GCP project discovery.

use crate::browser::{LoopbackCallbackError, LoopbackCallbackServer, generate_state, open_browser};
use crate::pkce::{PkceCodes, generate_pkce};
use crate::storage::{AuthStorage, StoredGoogleAuth};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::sync::Mutex;
use url::Url;

pub const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
pub const GOOGLE_PROJECTS_URL: &str =
    "https://cloudresourcemanager.googleapis.com/v1/projects?filter=lifecycleState:ACTIVE";

pub const ENV_GOOGLE_OAUTH_CLIENT_ID: &str = "ALAN_GOOGLE_OAUTH_CLIENT_ID";
pub const ENV_GOOGLE_OAUTH_CLIENT_SECRET: &str = "ALAN_GOOGLE_OAUTH_CLIENT_SECRET";
pub const ENV_GOOGLE_APPLICATION_CREDENTIALS: &str = "GOOGLE_APPLICATION_CREDENTIALS";
const ENV_CLOUDSDK_CONFIG: &str = "CLOUDSDK_CONFIG";

const GCLOUD_ADC_FILE: &str = "application_default_credentials.json";
const GOOGLE_OAUTH_USER_CONFIG_FILE: &str = "google_oauth.json";

mod gcloud;
mod projects;
#[cfg(test)]
mod tests;

pub use gcloud::{gcloud_default_project, gcloud_is_available, run_gcloud_adc_login_streaming};
pub use projects::{GcpProject, list_active_gcp_projects, list_active_gcp_projects_at};

/// Errors raised by Google OAuth, ADC, `gcloud`, and project discovery.
#[derive(Debug, Error)]
pub enum GoogleAuthError {
    #[error(
        "Google OAuth client_id is not configured (set {ENV_GOOGLE_OAUTH_CLIENT_ID} or create google_oauth.json)"
    )]
    OAuthClientNotConfigured,
    #[error("Google OAuth login required; run `alan login vertex`")]
    NotLoggedIn,
    #[error("Google OAuth token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("Google OAuth token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("Google Cloud Resource Manager API error: {0}")]
    ProjectDiscoveryFailed(String),
    #[error("Standard Google ADC file `{path}` error: {detail}")]
    InvalidAdcFile { path: PathBuf, detail: String },
    #[error("Google authorization code was empty or missing")]
    MissingCode,
    #[error("Google OAuth callback returned a mismatched state")]
    StateMismatch,
    #[error(
        "No Google credentials found (checked auth.json, standard ADC JSON, and gcloud CLI): {0}"
    )]
    NoCredentialsAvailable(String),
    #[error("`gcloud` CLI fallback failed: {0}")]
    GcloudFailed(String),
    #[error(transparent)]
    Callback(#[from] LoopbackCallbackError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Discovered Google OAuth Desktop App client credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleOAuthClientConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
}

/// Configuration for [`GoogleAuthManager`] and browser PKCE sign-in.
#[derive(Debug, Clone)]
pub struct GoogleAuthConfig {
    /// Host-owned `auth.json` backing file.
    pub storage_path: PathBuf,
    pub auth_url: String,
    pub token_url: String,
    pub projects_url: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub callback_port: u16,
}

impl GoogleAuthConfig {
    /// Create config targeting `storage_path` and discovering optional OAuth client credentials.
    pub fn with_storage_path(storage_path: PathBuf) -> Self {
        let discovered = discover_google_oauth_client(None);
        Self {
            storage_path,
            auth_url: GOOGLE_AUTH_URL.to_string(),
            token_url: GOOGLE_TOKEN_URL.to_string(),
            projects_url: GOOGLE_PROJECTS_URL.to_string(),
            client_id: discovered.as_ref().map(|c| c.client_id.clone()),
            client_secret: discovered.and_then(|c| c.client_secret),
            callback_port: 0,
        }
    }
}

/// Successful Google OAuth login summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleLoginSuccess {
    pub email: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Origin of a resolved Vertex AI Bearer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoogleCredentialSource {
    ManagedOAuth,
    AdcAuthorizedUser,
    GcloudCli,
}

impl GoogleCredentialSource {
    /// Human-readable label for status and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::ManagedOAuth => "Alan Google OAuth (auth.json)",
            Self::AdcAuthorizedUser => "Google Application Default Credentials JSON",
            Self::GcloudCli => "gcloud CLI",
        }
    }
}

/// Resolved Google Bearer token along with which source provided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedGoogleToken {
    pub access_token: String,
    pub source: GoogleCredentialSource,
}

/// Manager owning single-flight refresh and token persistence for Google OAuth.
#[derive(Debug, Clone)]
pub struct GoogleAuthManager {
    inner: Arc<GoogleAuthManagerInner>,
}

#[derive(Debug)]
struct GoogleAuthManagerInner {
    config: GoogleAuthConfig,
    storage: AuthStorage,
    client: reqwest::Client,
    refresh_lock: Mutex<()>,
}

/// Pending Google OAuth2 Desktop PKCE browser login waiting on callback or pasted input.
#[derive(Debug)]
pub struct PendingGoogleBrowserLogin {
    authorize_url: String,
    callback_url: String,
    token_url: String,
    client_id: String,
    client_secret: Option<String>,
    state: String,
    pkce: PkceCodes,
    server: LoopbackCallbackServer,
    storage: AuthStorage,
    client: reqwest::Client,
}

impl GoogleAuthManager {
    /// Construct a manager using `config.storage_path`.
    pub fn new(config: GoogleAuthConfig) -> io::Result<Self> {
        let storage = AuthStorage::new(config.storage_path.clone())?;
        Ok(Self {
            inner: Arc::new(GoogleAuthManagerInner {
                config,
                storage,
                client: reqwest::Client::new(),
                refresh_lock: Mutex::new(()),
            }),
        })
    }

    /// Construct a manager pointing directly at `storage_path` with default Google endpoints.
    pub fn with_storage_path(storage_path: PathBuf) -> io::Result<Self> {
        Self::new(GoogleAuthConfig::with_storage_path(storage_path))
    }

    /// Read stored Google OAuth metadata without refreshing.
    pub fn stored_auth(&self) -> io::Result<Option<StoredGoogleAuth>> {
        Ok(self.inner.storage.load()?.google)
    }

    /// Clear Alan-managed Google OAuth tokens from `auth.json`.
    pub fn logout(&self) -> io::Result<()> {
        self.inner.storage.clear_google()
    }

    /// Begin a Google OAuth2 Desktop App PKCE browser login flow.
    pub async fn start_browser_login(&self) -> Result<PendingGoogleBrowserLogin, GoogleAuthError> {
        let client_id = self
            .inner
            .config
            .client_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or(GoogleAuthError::OAuthClientNotConfigured)?
            .to_string();
        let client_secret = self
            .inner
            .config
            .client_secret
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);

        let server =
            LoopbackCallbackServer::bind(self.inner.config.callback_port, "/callback").await?;
        let callback_url = server.callback_url("127.0.0.1");
        let pkce = generate_pkce();
        let state = generate_state();
        let authorize_url = build_authorize_url(
            &self.inner.config.auth_url,
            &client_id,
            &callback_url,
            &pkce,
            &state,
        )?;

        Ok(PendingGoogleBrowserLogin {
            authorize_url,
            callback_url,
            token_url: self.inner.config.token_url.clone(),
            client_id,
            client_secret,
            state,
            pkce,
            server,
            storage: self.inner.storage.clone(),
            client: self.inner.client.clone(),
        })
    }

    /// Return a valid Bearer access token, performing single-flight refresh if expiring soon.
    pub async fn access_token(&self) -> Result<String, GoogleAuthError> {
        let Some(stored) = self.inner.storage.load()?.google else {
            return Err(GoogleAuthError::NotLoggedIn);
        };
        if !stored.should_refresh(Utc::now()) && !stored.access_token.trim().is_empty() {
            return Ok(stored.access_token);
        }
        self.refresh_single_flight(false).await
    }

    /// Force a single-flight refresh of the stored Google OAuth credentials.
    pub async fn force_refresh_access_token(&self) -> Result<String, GoogleAuthError> {
        self.refresh_single_flight(true).await
    }

    async fn refresh_single_flight(&self, force: bool) -> Result<String, GoogleAuthError> {
        let _memory_guard = self.inner.refresh_lock.lock().await;
        let _file_guard = self.inner.storage.lock("auth.refresh.lock")?;

        let Some(existing) = self.inner.storage.load()?.google else {
            return Err(GoogleAuthError::NotLoggedIn);
        };
        if !force
            && !existing.should_refresh(Utc::now())
            && !existing.access_token.trim().is_empty()
        {
            return Ok(existing.access_token);
        }

        let token_uri = existing
            .token_uri
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or(&self.inner.config.token_url);
        let client_id = existing
            .client_id
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or(self.inner.config.client_id.as_deref())
            .ok_or(GoogleAuthError::OAuthClientNotConfigured)?;
        let client_secret = existing
            .client_secret
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .or(self.inner.config.client_secret.as_deref());

        let token_resp = exchange_refresh_token(
            &self.inner.client,
            token_uri,
            client_id,
            client_secret,
            &existing.refresh_token,
        )
        .await
        .map_err(GoogleAuthError::RefreshFailed)?;

        let expires_at = token_resp
            .expires_in
            .filter(|secs| *secs > 0)
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
        let updated = StoredGoogleAuth {
            refresh_token: token_resp
                .refresh_token
                .filter(|v| !v.trim().is_empty())
                .unwrap_or(existing.refresh_token),
            access_token: token_resp.access_token.clone(),
            expires_at,
            client_id: Some(client_id.to_string()),
            client_secret: client_secret.map(ToOwned::to_owned),
            token_uri: Some(token_uri.to_string()),
            email: token_resp
                .id_token
                .as_deref()
                .and_then(parse_jwt_email)
                .or(existing.email),
            last_refresh_at: Some(Utc::now()),
        };

        let saved_entry = updated.clone();
        self.inner.storage.update(move |store| {
            store.google = Some(saved_entry);
        })?;

        Ok(updated.access_token)
    }
}

impl PendingGoogleBrowserLogin {
    /// Browser authorization URL containing PKCE challenge and `state`.
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// Bound loopback redirect URI.
    pub fn callback_url(&self) -> &str {
        &self.callback_url
    }

    /// Attempt to launch the system browser at `authorize_url`.
    pub fn open_browser(&self) -> io::Result<()> {
        open_browser(&self.authorize_url)
    }

    /// Wait up to `timeout` for the loopback callback and persist credentials into `auth.json`.
    pub async fn wait_and_exchange(
        self,
        timeout: Duration,
    ) -> Result<GoogleLoginSuccess, GoogleAuthError> {
        let mut receipt = self
            .server
            .wait_for_callback(timeout, Some(&self.state), "Google sign-in failed")
            .await?;

        match self.finish_code_exchange(&receipt.code).await {
            Ok(success) => {
                let _ = receipt
                    .respond_success(
                        "Google Cloud sign-in complete",
                        "You can close this window and return to Alan.",
                    )
                    .await;
                Ok(success)
            }
            Err(error) => {
                let _ = receipt
                    .respond_error("Google sign-in failed", &error.to_string())
                    .await;
                Err(error)
            }
        }
    }

    /// Exchange a pasted authorization `code` or full redirected callback URL.
    pub async fn exchange_pasted_input(
        &self,
        pasted: &str,
    ) -> Result<GoogleLoginSuccess, GoogleAuthError> {
        let code = extract_google_code(pasted, Some(&self.state))?;
        self.finish_code_exchange(&code).await
    }

    async fn finish_code_exchange(
        &self,
        code: &str,
    ) -> Result<GoogleLoginSuccess, GoogleAuthError> {
        let mut params = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", self.callback_url.clone()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", self.pkce.code_verifier.clone()),
        ];
        if let Some(secret) = &self.client_secret {
            params.push(("client_secret", secret.clone()));
        }

        let encoded_form = encode_form_params(&params);
        let response = self
            .client
            .post(&self.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(encoded_form)
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(GoogleAuthError::TokenExchangeFailed(format!(
                "HTTP {status}: {body}"
            )));
        }

        let parsed: GoogleTokenResponse = serde_json::from_str(&body).map_err(|error| {
            GoogleAuthError::TokenExchangeFailed(format!("invalid response JSON: {error}"))
        })?;
        let refresh_token = parsed
            .refresh_token
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                GoogleAuthError::TokenExchangeFailed(
                    "Google token response omitted `refresh_token`".to_string(),
                )
            })?;
        let expires_at = parsed
            .expires_in
            .filter(|secs| *secs > 0)
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));
        let email = parsed.id_token.as_deref().and_then(parse_jwt_email);

        let stored = StoredGoogleAuth {
            refresh_token,
            access_token: parsed.access_token,
            expires_at,
            client_id: Some(self.client_id.clone()),
            client_secret: self.client_secret.clone(),
            token_uri: Some(self.token_url.clone()),
            email: email.clone(),
            last_refresh_at: Some(Utc::now()),
        };

        self.storage.update(move |store| {
            store.google = Some(stored);
        })?;

        Ok(GoogleLoginSuccess { email, expires_at })
    }
}

/// Return Alan's default Host Store `auth.json` location when available on this OS.
pub fn default_managed_auth_storage_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("Alan").join("Host Store").join("auth.json"))
}

/// Discover Google Desktop OAuth client credentials from environment or user config file.
pub fn discover_google_oauth_client(
    config_file_override: Option<&Path>,
) -> Option<GoogleOAuthClientConfig> {
    if let Ok(client_id) = std::env::var(ENV_GOOGLE_OAUTH_CLIENT_ID) {
        let trimmed = client_id.trim();
        if !trimmed.is_empty() {
            let client_secret = std::env::var(ENV_GOOGLE_OAUTH_CLIENT_SECRET)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            return Some(GoogleOAuthClientConfig {
                client_id: trimmed.to_string(),
                client_secret,
            });
        }
    }

    let candidate_paths: Vec<PathBuf> = match config_file_override {
        Some(path) => vec![path.to_path_buf()],
        None => user_google_oauth_config_paths(),
    };

    for path in candidate_paths {
        if let Ok(raw) = fs::read_to_string(&path)
            && let Some(parsed) = parse_google_oauth_client_json(&raw)
        {
            return Some(parsed);
        }
    }

    None
}

/// Candidate path(s) for user-configured `google_oauth.json`.
pub fn user_google_oauth_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("alan").join(GOOGLE_OAUTH_USER_CONFIG_FILE));
    }
    if let Some(home_dir) = dirs::home_dir() {
        let xdg = home_dir
            .join(".config")
            .join("alan")
            .join(GOOGLE_OAUTH_USER_CONFIG_FILE);
        if !paths.contains(&xdg) {
            paths.push(xdg);
        }
    }
    paths
}

/// Resolve the standard Google Application Default Credentials JSON file path if present.
pub fn resolve_adc_credentials_path() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(ENV_GOOGLE_APPLICATION_CREDENTIALS) {
        let path = PathBuf::from(explicit);
        if !path.as_os_str().is_empty() && path.is_file() {
            return Some(path);
        }
    }
    if let Some(cloudsdk_dir) = std::env::var_os(ENV_CLOUDSDK_CONFIG) {
        let path = PathBuf::from(cloudsdk_dir).join(GCLOUD_ADC_FILE);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(home_dir) = dirs::home_dir() {
        let path = home_dir
            .join(".config")
            .join("gcloud")
            .join(GCLOUD_ADC_FILE);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("gcloud").join(GCLOUD_ADC_FILE);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Exchange a standard Google ADC `authorized_user` JSON file directly without calling `gcloud`.
pub async fn adc_access_token_from_file(
    path: &Path,
    default_token_uri: Option<&str>,
) -> Result<String, GoogleAuthError> {
    let client = reqwest::Client::new();
    adc_access_token_from_file_with_client(path, &client, default_token_uri).await
}

async fn adc_access_token_from_file_with_client(
    path: &Path,
    client: &reqwest::Client,
    default_token_uri: Option<&str>,
) -> Result<String, GoogleAuthError> {
    let raw = fs::read_to_string(path).map_err(|error| GoogleAuthError::InvalidAdcFile {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })?;
    let parsed: AdcAuthorizedUserJson =
        serde_json::from_str(&raw).map_err(|error| GoogleAuthError::InvalidAdcFile {
            path: path.to_path_buf(),
            detail: error.to_string(),
        })?;

    if parsed.credential_type.as_deref() != Some("authorized_user") {
        let kind = parsed.credential_type.as_deref().unwrap_or("<missing>");
        return Err(GoogleAuthError::InvalidAdcFile {
            path: path.to_path_buf(),
            detail: format!("expected type `authorized_user`, found `{kind}`"),
        });
    }

    let client_id = parsed
        .client_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| GoogleAuthError::InvalidAdcFile {
            path: path.to_path_buf(),
            detail: "missing `client_id`".to_string(),
        })?;
    let refresh_token = parsed
        .refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| GoogleAuthError::InvalidAdcFile {
            path: path.to_path_buf(),
            detail: "missing `refresh_token`".to_string(),
        })?;
    let client_secret = parsed
        .client_secret
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let token_uri = default_token_uri
        .or(parsed.token_uri.as_deref())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(GOOGLE_TOKEN_URL);

    let resp = exchange_refresh_token(client, token_uri, client_id, client_secret, refresh_token)
        .await
        .map_err(|detail| GoogleAuthError::InvalidAdcFile {
            path: path.to_path_buf(),
            detail,
        })?;
    Ok(resp.access_token)
}

/// Run `gcloud auth application-default login` interactively as a fallback when no Desktop OAuth
/// client is configured.
pub async fn run_gcloud_adc_login() -> Result<(), GoogleAuthError> {
    let status = Command::new("gcloud")
        .args(["auth", "application-default", "login"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|error| {
            GoogleAuthError::GcloudFailed(format!(
                "failed to invoke `gcloud auth application-default login`: {error}"
            ))
        })?;
    if !status.success() {
        return Err(GoogleAuthError::GcloudFailed(format!(
            "`gcloud auth application-default login` exited with status {status}"
        )));
    }
    Ok(())
}

/// Print an access token via `gcloud auth application-default print-access-token` (falling back
/// to `gcloud auth print-access-token`).
pub async fn gcloud_print_access_token() -> Result<String, GoogleAuthError> {
    let candidates: [&[&str]; 2] = [
        &["auth", "application-default", "print-access-token"],
        &["auth", "print-access-token"],
    ];
    let mut errors = Vec::new();
    for args in candidates {
        match run_gcloud_print_command(args).await {
            Ok(token) => return Ok(token),
            Err(err) => errors.push(err),
        }
    }
    Err(GoogleAuthError::GcloudFailed(errors.join("; ")))
}

async fn run_gcloud_print_command(args: &[&str]) -> Result<String, String> {
    let output = Command::new("gcloud")
        .args(args)
        .output()
        .await
        .map_err(|error| format!("`gcloud {}` failed to spawn: {error}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "`gcloud {}` exited {}: {stderr}",
            args.join(" "),
            output.status
        ));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err(format!("`gcloud {}` returned empty token", args.join(" ")));
    }
    Ok(token)
}

/// Resolve a Vertex AI Bearer token via priority chain:
/// 1. Alan managed Google OAuth in `auth.json` (with single-flight refresh)
/// 2. Standard ADC `authorized_user` JSON file (`GOOGLE_APPLICATION_CREDENTIALS` or gcloud ADC)
/// 3. `gcloud auth application-default print-access-token` / `gcloud auth print-access-token`
pub async fn resolve_vertex_access_token(
    storage_path: Option<&Path>,
) -> Result<ResolvedGoogleToken, GoogleAuthError> {
    let managed_path = storage_path
        .map(Path::to_path_buf)
        .or_else(default_managed_auth_storage_path);
    let adc_path = resolve_adc_credentials_path();
    resolve_vertex_access_token_with_paths(managed_path.as_deref(), adc_path.as_deref(), None, true)
        .await
}

async fn resolve_vertex_access_token_with_paths(
    storage_path: Option<&Path>,
    adc_path: Option<&Path>,
    adc_token_uri_override: Option<&str>,
    allow_gcloud_fallback: bool,
) -> Result<ResolvedGoogleToken, GoogleAuthError> {
    let mut errors = Vec::new();

    if let Some(path) = storage_path
        && path.is_file()
    {
        match GoogleAuthManager::with_storage_path(path.to_path_buf()) {
            Ok(manager) => match manager.access_token().await {
                Ok(access_token) => {
                    return Ok(ResolvedGoogleToken {
                        access_token,
                        source: GoogleCredentialSource::ManagedOAuth,
                    });
                }
                Err(GoogleAuthError::NotLoggedIn) => {}
                Err(err) => errors.push(format!("managed auth: {err}")),
            },
            Err(err) => errors.push(format!("managed auth storage: {err}")),
        }
    }

    if let Some(adc_file) = adc_path {
        match adc_access_token_from_file(adc_file, adc_token_uri_override).await {
            Ok(access_token) => {
                return Ok(ResolvedGoogleToken {
                    access_token,
                    source: GoogleCredentialSource::AdcAuthorizedUser,
                });
            }
            Err(err) => errors.push(format!("ADC: {err}")),
        }
    }

    if allow_gcloud_fallback {
        match gcloud_print_access_token().await {
            Ok(access_token) => {
                return Ok(ResolvedGoogleToken {
                    access_token,
                    source: GoogleCredentialSource::GcloudCli,
                });
            }
            Err(err) => errors.push(err.to_string()),
        }
    }

    let detail = if errors.is_empty() {
        "run `alan login vertex` to authenticate".to_string()
    } else {
        errors.join("; ")
    };
    Err(GoogleAuthError::NoCredentialsAvailable(detail))
}

fn build_authorize_url(
    auth_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkceCodes,
    state: &str,
) -> Result<String, GoogleAuthError> {
    let mut url = Url::parse(auth_url)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", GOOGLE_CLOUD_PLATFORM_SCOPE)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("access_type", "offline")
        .append_pair("prompt", "consent");
    Ok(url.to_string())
}

fn extract_google_code(
    input: &str,
    expected_state: Option<&str>,
) -> Result<String, GoogleAuthError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(GoogleAuthError::MissingCode);
    }

    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && let Ok(url) = Url::parse(trimmed)
    {
        if let Some((_, error)) = url.query_pairs().find(|(k, _)| k == "error") {
            let description = url
                .query_pairs()
                .find(|(k, _)| k == "error_description")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "Google sign-in declined".to_string());
            return Err(GoogleAuthError::Callback(
                LoopbackCallbackError::ProviderError {
                    error: error.to_string(),
                    description,
                },
            ));
        }
        if let Some(expected) = expected_state
            && let Some((_, actual)) = url.query_pairs().find(|(k, _)| k == "state")
            && actual != expected
        {
            return Err(GoogleAuthError::StateMismatch);
        }
        return url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.trim().to_string())
            .filter(|code| !code.is_empty())
            .ok_or(GoogleAuthError::MissingCode);
    }

    Ok(trimmed.to_string())
}

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AdcAuthorizedUserJson {
    #[serde(rename = "type")]
    credential_type: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
    token_uri: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthClientJsonFlat {
    client_id: Option<String>,
    client_secret: Option<String>,
    installed: Option<GoogleOAuthClientSection>,
    web: Option<GoogleOAuthClientSection>,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthClientSection {
    client_id: Option<String>,
    client_secret: Option<String>,
}

fn parse_google_oauth_client_json(raw: &str) -> Option<GoogleOAuthClientConfig> {
    let parsed: GoogleOAuthClientJsonFlat = serde_json::from_str(raw).ok()?;
    let (id, secret) = if let Some(installed) = parsed.installed {
        (installed.client_id, installed.client_secret)
    } else if let Some(web) = parsed.web {
        (web.client_id, web.client_secret)
    } else {
        (parsed.client_id, parsed.client_secret)
    };
    let client_id = id.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())?;
    let client_secret = secret
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    Some(GoogleOAuthClientConfig {
        client_id,
        client_secret,
    })
}

async fn exchange_refresh_token(
    client: &reqwest::Client,
    token_uri: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<GoogleTokenResponse, String> {
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("client_id", client_id.to_string()),
        ("refresh_token", refresh_token.to_string()),
    ];
    if let Some(secret) = client_secret.map(str::trim).filter(|v| !v.is_empty()) {
        params.push(("client_secret", secret.to_string()));
    }

    let encoded_form = encode_form_params(&params);
    let response = client
        .post(token_uri)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(encoded_form)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("HTTP {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("invalid token JSON: {error}"))
}

fn encode_form_params(params: &[(&str, String)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
        .finish()
}

fn parse_jwt_email(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    claims
        .get("email")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}
