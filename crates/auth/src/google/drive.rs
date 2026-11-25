//! Host-owned Google Drive OAuth2 PKCE login and single-flight refresh.
//!
//! Credentials remain strictly in Host Store (`AuthStorage`) and default to the
//! least-privilege `drive.readonly` scope unless `drive.file` is explicitly requested.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::browser::{LoopbackCallbackServer, generate_state, open_browser};
use crate::pkce::{PkceCodes, generate_pkce};
use crate::storage::{AuthStorage, StoredGoogleAuth};

use super::{
    GOOGLE_AUTH_URL, GOOGLE_DRIVE_FILE_SCOPE, GOOGLE_DRIVE_READONLY_SCOPE, GOOGLE_TOKEN_URL,
    GoogleAuthError, build_authorize_url_with_scope, discover_google_oauth_client,
    exchange_authorization_code, extract_google_code, refresh_stored_google_auth,
};

const DRIVE_SCOPES_METADATA_FILE: &str = "drive_scopes.json";

/// Requested OAuth scope level for Host-owned Google Drive credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GoogleDriveScope {
    /// Default least-privilege read-only Drive scope (`drive.readonly`).
    #[default]
    ReadOnly,
    /// Scoped write/create access to files opened or created by Alan (`drive.file`).
    File,
    /// Combined read-only + file-creation scopes (`drive.readonly` + `drive.file`).
    ReadOnlyAndFile,
}

impl GoogleDriveScope {
    /// Return the canonical Google OAuth scope URIs represented by this selection.
    pub fn scopes(self) -> &'static [&'static str] {
        match self {
            Self::ReadOnly => &[GOOGLE_DRIVE_READONLY_SCOPE],
            Self::File => &[GOOGLE_DRIVE_FILE_SCOPE],
            Self::ReadOnlyAndFile => &[GOOGLE_DRIVE_READONLY_SCOPE, GOOGLE_DRIVE_FILE_SCOPE],
        }
    }

    /// Return the space-separated `scope` query parameter value.
    pub fn as_scope_param(self) -> String {
        self.scopes().join(" ")
    }

    /// Whether this scope permits creating or uploading files via `drive.file`.
    pub fn allows_file_write(self) -> bool {
        matches!(self, Self::File | Self::ReadOnlyAndFile)
    }

    /// Check whether `granted` satisfies the requirements of `self`.
    pub fn is_satisfied_by(self, granted: &[String]) -> bool {
        let has_ro = granted.iter().any(|s| s == GOOGLE_DRIVE_READONLY_SCOPE);
        let has_file = granted.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE);
        match self {
            Self::ReadOnly => has_ro || has_file,
            Self::File => has_file,
            Self::ReadOnlyAndFile => has_ro && has_file,
        }
    }
}

/// Configuration for [`GoogleDriveAuthManager`] and Drive PKCE browser sign-in.
#[derive(Debug, Clone)]
pub struct GoogleDriveAuthConfig {
    /// Host-owned `auth.json` backing file inside Host Store.
    pub storage_path: PathBuf,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub callback_port: u16,
    /// OAuth scope requested on interactive login (defaults to `drive.readonly`).
    pub scope: GoogleDriveScope,
}

impl GoogleDriveAuthConfig {
    /// Create config targeting `storage_path` with default `drive.readonly` scope.
    pub fn with_storage_path(storage_path: PathBuf) -> Self {
        let discovered = discover_google_oauth_client(None);
        Self {
            storage_path,
            auth_url: GOOGLE_AUTH_URL.to_string(),
            token_url: GOOGLE_TOKEN_URL.to_string(),
            client_id: discovered.as_ref().map(|c| c.client_id.clone()),
            client_secret: discovered.and_then(|c| c.client_secret),
            callback_port: 0,
            scope: GoogleDriveScope::ReadOnly,
        }
    }

    /// Override the requested Google Drive OAuth scope.
    pub fn with_scope(mut self, scope: GoogleDriveScope) -> Self {
        self.scope = scope;
        self
    }
}

/// Resolved Google Drive Bearer token and non-secret scope metadata.
///
/// `Debug` redacts `access_token` so tokens are never printed to diagnostics.
#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedGoogleDriveToken {
    pub access_token: String,
    pub scopes: Vec<String>,
    pub email: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ResolvedGoogleDriveToken {
    /// Whether the granted OAuth scopes include `drive.file`.
    pub fn has_file_write_scope(&self) -> bool {
        self.scopes.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE)
    }
}

impl fmt::Debug for ResolvedGoogleDriveToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedGoogleDriveToken")
            .field("access_token", &"[REDACTED]")
            .field("scopes", &self.scopes)
            .field("email", &self.email)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Successful Google Drive OAuth login summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleDriveLoginSuccess {
    pub email: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub scopes: Vec<String>,
}

/// Return Alan's default Host Store Google Drive `auth.json` location when available.
pub fn default_managed_drive_auth_storage_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| {
        dir.join("Alan")
            .join("Host Store")
            .join("gdrive")
            .join("auth.json")
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DriveScopeMetadata {
    scopes: Vec<String>,
}

/// Manager owning single-flight refresh and Host Store persistence for Google Drive OAuth.
#[derive(Debug, Clone)]
pub struct GoogleDriveAuthManager {
    inner: Arc<GoogleDriveAuthManagerInner>,
}

#[derive(Debug)]
struct GoogleDriveAuthManagerInner {
    config: GoogleDriveAuthConfig,
    storage: AuthStorage,
    client: reqwest::Client,
    refresh_lock: Mutex<()>,
}

/// Pending Google Drive OAuth2 Desktop PKCE login waiting on loopback callback or pasted input.
#[derive(Debug)]
pub struct PendingGoogleDriveBrowserLogin {
    authorize_url: String,
    callback_url: String,
    token_url: String,
    client_id: String,
    client_secret: Option<String>,
    requested_scopes: Vec<String>,
    state: String,
    pkce: PkceCodes,
    server: LoopbackCallbackServer,
    storage: AuthStorage,
    client: reqwest::Client,
}

impl GoogleDriveAuthManager {
    /// Construct a Drive auth manager using `config.storage_path`.
    pub fn new(config: GoogleDriveAuthConfig) -> io::Result<Self> {
        let storage = AuthStorage::new(config.storage_path.clone())?;
        Ok(Self {
            inner: Arc::new(GoogleDriveAuthManagerInner {
                config,
                storage,
                client: reqwest::Client::new(),
                refresh_lock: Mutex::new(()),
            }),
        })
    }

    /// Construct a Drive auth manager at `storage_path` with default `drive.readonly` scope.
    pub fn with_storage_path(storage_path: PathBuf) -> io::Result<Self> {
        Self::new(GoogleDriveAuthConfig::with_storage_path(storage_path))
    }

    /// Construct a Drive auth manager at `storage_path` with explicit `scope`.
    pub fn with_storage_path_and_scope(
        storage_path: PathBuf,
        scope: GoogleDriveScope,
    ) -> io::Result<Self> {
        Self::new(GoogleDriveAuthConfig::with_storage_path(storage_path).with_scope(scope))
    }

    /// Read stored Google Drive OAuth entry without refreshing.
    pub fn stored_auth(&self) -> io::Result<Option<StoredGoogleAuth>> {
        Ok(self.inner.storage.load()?.google)
    }

    /// Read persisted non-secret granted scopes (defaults conservatively to `drive.readonly`).
    pub fn granted_scopes(&self) -> io::Result<Vec<String>> {
        let scopes_path = scope_metadata_path(&self.inner.storage)?;
        if let Ok(raw) = fs::read_to_string(&scopes_path)
            && let Ok(meta) = serde_json::from_str::<DriveScopeMetadata>(&raw)
            && !meta.scopes.is_empty()
        {
            return Ok(meta.scopes);
        }
        if self.stored_auth()?.is_some() {
            return Ok(vec![GOOGLE_DRIVE_READONLY_SCOPE.to_string()]);
        }
        Ok(Vec::new())
    }

    /// Whether stored credentials include `drive.file` scope.
    pub fn has_file_write_scope(&self) -> bool {
        self.granted_scopes()
            .map(|scopes| scopes.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE))
            .unwrap_or(false)
    }

    /// Clear stored Drive OAuth credentials and scope metadata from Host Store.
    pub fn logout(&self) -> io::Result<()> {
        self.inner.storage.clear_google()?;
        let scopes_path = scope_metadata_path(&self.inner.storage)?;
        match fs::remove_file(scopes_path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Begin a Google Drive Desktop App PKCE browser login flow.
    pub async fn start_browser_login(
        &self,
    ) -> Result<PendingGoogleDriveBrowserLogin, GoogleAuthError> {
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
        let scope_param = self.inner.config.scope.as_scope_param();
        let authorize_url = build_authorize_url_with_scope(
            &self.inner.config.auth_url,
            &client_id,
            &callback_url,
            &scope_param,
            &pkce,
            &state,
        )?;

        Ok(PendingGoogleDriveBrowserLogin {
            authorize_url,
            callback_url,
            token_url: self.inner.config.token_url.clone(),
            client_id,
            client_secret,
            requested_scopes: self
                .inner
                .config
                .scope
                .scopes()
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            state,
            pkce,
            server,
            storage: self.inner.storage.clone(),
            client: self.inner.client.clone(),
        })
    }

    /// Return a valid Drive Bearer token, performing single-flight refresh if expiring soon.
    pub async fn access_token(&self) -> Result<ResolvedGoogleDriveToken, GoogleAuthError> {
        let Some(stored) = self.inner.storage.load()?.google else {
            return Err(GoogleAuthError::DriveNotLoggedIn);
        };
        let resolved_stored =
            if !stored.should_refresh(Utc::now()) && !stored.access_token.trim().is_empty() {
                stored
            } else {
                refresh_stored_google_auth(
                    &self.inner.storage,
                    &self.inner.client,
                    &self.inner.refresh_lock,
                    &self.inner.config.token_url,
                    self.inner.config.client_id.as_deref(),
                    self.inner.config.client_secret.as_deref(),
                    false,
                    GoogleAuthError::DriveNotLoggedIn,
                )
                .await?
            };
        let scopes = self.granted_scopes()?;
        Ok(ResolvedGoogleDriveToken {
            access_token: resolved_stored.access_token,
            scopes,
            email: resolved_stored.email,
            expires_at: resolved_stored.expires_at,
        })
    }

    /// Return a valid Drive Bearer token and enforce that `required` scope was granted.
    pub async fn access_token_for_scope(
        &self,
        required: GoogleDriveScope,
    ) -> Result<ResolvedGoogleDriveToken, GoogleAuthError> {
        let token = self.access_token().await?;
        if !required.is_satisfied_by(&token.scopes) {
            return Err(GoogleAuthError::InsufficientScope {
                required: required.as_scope_param(),
                granted: if token.scopes.is_empty() {
                    "<none>".to_string()
                } else {
                    token.scopes.join(" ")
                },
            });
        }
        Ok(token)
    }

    /// Force a single-flight refresh of the stored Drive OAuth credentials.
    pub async fn force_refresh_access_token(
        &self,
    ) -> Result<ResolvedGoogleDriveToken, GoogleAuthError> {
        let updated = refresh_stored_google_auth(
            &self.inner.storage,
            &self.inner.client,
            &self.inner.refresh_lock,
            &self.inner.config.token_url,
            self.inner.config.client_id.as_deref(),
            self.inner.config.client_secret.as_deref(),
            true,
            GoogleAuthError::DriveNotLoggedIn,
        )
        .await?;
        let scopes = self.granted_scopes()?;
        Ok(ResolvedGoogleDriveToken {
            access_token: updated.access_token,
            scopes,
            email: updated.email,
            expires_at: updated.expires_at,
        })
    }
}

impl PendingGoogleDriveBrowserLogin {
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

    /// Wait up to `timeout` for loopback callback and persist Drive credentials into Host Store.
    pub async fn wait_and_exchange(
        self,
        timeout: Duration,
    ) -> Result<GoogleDriveLoginSuccess, GoogleAuthError> {
        let mut receipt = self
            .server
            .wait_for_callback(timeout, Some(&self.state), "Google Drive sign-in failed")
            .await?;

        match self.finish_code_exchange(&receipt.code).await {
            Ok(success) => {
                let _ = receipt
                    .respond_success(
                        "Google Drive sign-in complete",
                        "You can close this window and return to Alan.",
                    )
                    .await;
                Ok(success)
            }
            Err(error) => {
                let _ = receipt
                    .respond_error("Google Drive sign-in failed", &error.to_string())
                    .await;
                Err(error)
            }
        }
    }

    /// Exchange a pasted authorization `code` or full redirected callback URL.
    pub async fn exchange_pasted_input(
        &self,
        pasted: &str,
    ) -> Result<GoogleDriveLoginSuccess, GoogleAuthError> {
        let code = extract_google_code(pasted, Some(&self.state))?;
        self.finish_code_exchange(&code).await
    }

    async fn finish_code_exchange(
        &self,
        code: &str,
    ) -> Result<GoogleDriveLoginSuccess, GoogleAuthError> {
        let (stored, returned_scope) = exchange_authorization_code(
            &self.client,
            &self.storage,
            &self.token_url,
            &self.callback_url,
            &self.client_id,
            self.client_secret.as_deref(),
            &self.pkce.code_verifier,
            code,
        )
        .await?;

        let scopes = match returned_scope {
            Some(raw) if !raw.trim().is_empty() => raw
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>(),
            _ => self.requested_scopes.clone(),
        };
        persist_scope_metadata(&self.storage, &scopes)?;

        Ok(GoogleDriveLoginSuccess {
            email: stored.email,
            expires_at: stored.expires_at,
            scopes,
        })
    }
}

/// Resolve a Host-owned Google Drive Bearer token and enforce `required_scope`.
pub async fn resolve_drive_access_token(
    storage_path: Option<&Path>,
    required_scope: GoogleDriveScope,
) -> Result<ResolvedGoogleDriveToken, GoogleAuthError> {
    let path = storage_path
        .map(Path::to_path_buf)
        .or_else(default_managed_drive_auth_storage_path)
        .ok_or(GoogleAuthError::DriveNotLoggedIn)?;
    let manager = GoogleDriveAuthManager::with_storage_path(path)?;
    manager.access_token_for_scope(required_scope).await
}

/// Persist non-secret Drive OAuth scope metadata alongside `auth.json` in Host Store.
pub(super) fn persist_scope_metadata(storage: &AuthStorage, scopes: &[String]) -> io::Result<()> {
    let path = scope_metadata_path(storage)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("drive scope metadata path missing parent"))?;
    fs::create_dir_all(parent)?;
    let payload = serde_json::to_vec_pretty(&DriveScopeMetadata {
        scopes: scopes.to_vec(),
    })
    .map_err(io::Error::other)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&payload)?;
    staged.write_all(b"\n")?;
    staged.as_file().sync_all()?;
    staged.persist(&path).map_err(|err| err.error)?;
    Ok(())
}

fn scope_metadata_path(storage: &AuthStorage) -> io::Result<PathBuf> {
    let auth_path = storage.path()?;
    let parent = auth_path
        .parent()
        .ok_or_else(|| io::Error::other("auth storage path missing parent"))?;
    Ok(parent.join(DRIVE_SCOPES_METADATA_FILE))
}
