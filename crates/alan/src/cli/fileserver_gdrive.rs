//! Hidden internal CLI entrypoint backing the `gdrive-fileserver` Quartermaster package.

use std::path::PathBuf;
use std::sync::Arc;

use alan_ap::ErrorCode;
use alan_auth::google::{
    GOOGLE_DRIVE_FILE_SCOPE, GoogleAuthError, GoogleDriveAuthManager,
    default_managed_drive_auth_storage_path,
};
use alan_service_manager::gdrivefs::{
    DEFAULT_DRIVE_API_BASE_URL, DEFAULT_DRIVE_UPLOAD_BASE_URL, DriveAuthSummary, DriveAuthToken,
    DriveTokenFuture, DriveTokenProvider, GdriveBackendError, GdriveFs, GoogleDriveApiBackend,
    serve_stdio,
};
use anyhow::Result;
use clap::Args;

const ENV_GDRIVEFS_AUTH_STORAGE: &str = "ALAN_GDRIVEFS_AUTH_STORAGE";
const ENV_GDRIVEFS_API_BASE_URL: &str = "ALAN_GDRIVEFS_API_BASE_URL";
const ENV_GDRIVEFS_UPLOAD_BASE_URL: &str = "ALAN_GDRIVEFS_UPLOAD_BASE_URL";

/// Hidden arguments for `alan __fileserver-gdrive`.
#[derive(Debug, Default, Args)]
pub struct FileserverGdriveArgs {
    /// Optional explicit Host Store Google Drive `auth.json` path.
    #[arg(long = "auth-storage", hide = true)]
    pub auth_storage: Option<PathBuf>,
    /// Optional Drive API v3 base URL override.
    #[arg(long = "api-base-url", hide = true)]
    pub api_base_url: Option<String>,
    /// Optional Drive upload API v3 base URL override.
    #[arg(long = "upload-base-url", hide = true)]
    pub upload_base_url: Option<String>,
    /// Enforce read-only mode even if mounted read-write.
    #[arg(long = "read-only", hide = true)]
    pub read_only: bool,
}

/// Host-owned [`DriveTokenProvider`] backed by [`GoogleDriveAuthManager`] in Host Store.
pub struct HostDriveTokenProvider {
    manager: Option<GoogleDriveAuthManager>,
    init_error: Option<String>,
}

impl HostDriveTokenProvider {
    /// Construct a provider using `storage_path` (or the default Host Store path).
    pub fn new(storage_path: Option<PathBuf>) -> Self {
        let resolved = storage_path.or_else(default_managed_drive_auth_storage_path);
        match resolved {
            Some(path) => match GoogleDriveAuthManager::with_storage_path(path) {
                Ok(manager) => Self {
                    manager: Some(manager),
                    init_error: None,
                },
                Err(err) => Self {
                    manager: None,
                    init_error: Some(err.to_string()),
                },
            },
            None => Self {
                manager: None,
                init_error: Some("Host Store directory could not be resolved".to_string()),
            },
        }
    }

    /// Construct a provider from an explicit [`GoogleDriveAuthConfig`].
    #[cfg(test)]
    pub fn from_config(config: alan_auth::google::GoogleDriveAuthConfig) -> Result<Self> {
        let manager = GoogleDriveAuthManager::new(config)?;
        Ok(Self {
            manager: Some(manager),
            init_error: None,
        })
    }

    fn require_manager(&self) -> Result<&GoogleDriveAuthManager, GdriveBackendError> {
        self.manager.as_ref().ok_or_else(|| {
            GdriveBackendError::new(
                ErrorCode::NoAccess,
                self.init_error
                    .as_deref()
                    .unwrap_or("Google Drive OAuth Host Store is unavailable"),
            )
        })
    }
}

impl DriveTokenProvider for HostDriveTokenProvider {
    fn access_token(&self) -> DriveTokenFuture<'_> {
        Box::pin(async move {
            let manager = self.require_manager()?;
            let token = manager
                .access_token()
                .await
                .map_err(map_google_auth_error)?;
            Ok(DriveAuthToken {
                access_token: token.access_token,
                scopes: token.scopes,
                email: token.email,
            })
        })
    }

    fn force_refresh(&self) -> DriveTokenFuture<'_> {
        Box::pin(async move {
            let manager = self.require_manager()?;
            let token = manager
                .force_refresh_access_token()
                .await
                .map_err(map_google_auth_error)?;
            Ok(DriveAuthToken {
                access_token: token.access_token,
                scopes: token.scopes,
                email: token.email,
            })
        })
    }

    fn auth_summary(&self) -> DriveAuthSummary {
        let Some(manager) = self.manager.as_ref() else {
            return DriveAuthSummary::default();
        };
        let Ok(Some(stored)) = manager.stored_auth() else {
            return DriveAuthSummary::default();
        };
        let scopes = manager.granted_scopes().unwrap_or_default();
        let has_file_write_scope = scopes.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE);
        DriveAuthSummary {
            authenticated: true,
            scopes,
            has_file_write_scope,
            email: stored.email,
        }
    }
}

fn map_google_auth_error(err: GoogleAuthError) -> GdriveBackendError {
    let code = match err {
        GoogleAuthError::DriveNotLoggedIn
        | GoogleAuthError::NotLoggedIn
        | GoogleAuthError::InsufficientScope { .. }
        | GoogleAuthError::OAuthClientNotConfigured
        | GoogleAuthError::NoCredentialsAvailable(_) => ErrorCode::NoAccess,
        _ => ErrorCode::Io,
    };
    GdriveBackendError::new(code, err.to_string())
}

/// Serve `gdrivefs` over stdio using framed `aP` wire transport.
pub async fn run(args: FileserverGdriveArgs) -> Result<()> {
    let auth_storage = args.auth_storage.or_else(|| {
        std::env::var_os(ENV_GDRIVEFS_AUTH_STORAGE)
            .filter(|val| !val.is_empty())
            .map(PathBuf::from)
    });
    let api_base_url = args
        .api_base_url
        .or_else(|| {
            std::env::var(ENV_GDRIVEFS_API_BASE_URL)
                .ok()
                .filter(|val| !val.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_DRIVE_API_BASE_URL.to_string());
    let upload_base_url = args
        .upload_base_url
        .or_else(|| {
            std::env::var(ENV_GDRIVEFS_UPLOAD_BASE_URL)
                .ok()
                .filter(|val| !val.trim().is_empty())
        })
        .unwrap_or_else(|| DEFAULT_DRIVE_UPLOAD_BASE_URL.to_string());

    let provider = Arc::new(HostDriveTokenProvider::new(auth_storage));
    let backend = Arc::new(GoogleDriveApiBackend::with_base_urls(
        provider,
        api_base_url,
        upload_base_url,
    ));
    let server = Arc::new(GdriveFs::with_read_only(backend, args.read_only));
    serve_stdio(server)
        .await
        .map_err(|err| anyhow::anyhow!("gdrivefs stdio server failed: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::dispatch::{Cli, Commands, uses_product_stores};
    use alan_auth::google::{
        GOOGLE_AUTH_URL, GOOGLE_DRIVE_READONLY_SCOPE, GoogleDriveAuthConfig, GoogleDriveScope,
    };
    use alan_auth::{AuthStorage, AuthStore, StoredGoogleAuth};
    use chrono::{Duration as ChronoDuration, Utc};
    use clap::{CommandFactory, Parser};
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn hidden_fileserver_gdrive_subcommand_parses_and_stays_hidden_from_help() {
        let mut help = Vec::new();
        Cli::command().write_long_help(&mut help).unwrap();
        let help_text = String::from_utf8(help).unwrap();
        assert!(
            !help_text.contains("__fileserver-gdrive"),
            "hidden subcommand leaked into help:\n{help_text}"
        );

        let parsed = Cli::try_parse_from(["alan", "__fileserver-gdrive"]).unwrap();
        assert!(!uses_product_stores(parsed.command.as_ref()));
        assert!(matches!(
            parsed.command,
            Some(Commands::FileserverGdrive(FileserverGdriveArgs {
                auth_storage: None,
                api_base_url: None,
                upload_base_url: None,
                read_only: false,
            }))
        ));

        let with_flags = Cli::try_parse_from([
            "alan",
            "__fileserver-gdrive",
            "--auth-storage",
            "/tmp/gdrive/auth.json",
            "--api-base-url",
            "http://127.0.0.1:9000/drive/v3",
            "--read-only",
        ])
        .unwrap();
        assert!(matches!(
            with_flags.command,
            Some(Commands::FileserverGdrive(FileserverGdriveArgs {
                auth_storage: Some(ref path),
                api_base_url: Some(ref api),
                upload_base_url: None,
                read_only: true,
            })) if path == std::path::Path::new("/tmp/gdrive/auth.json")
                && api == "http://127.0.0.1:9000/drive/v3"
        ));
    }

    #[tokio::test]
    async fn host_drive_token_provider_reads_and_refreshes_without_token_leak() {
        let dir = TempDir::new().unwrap();
        let auth_path = dir.path().join("gdrive").join("auth.json");

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = r#"{"access_token":"ya29.host-refreshed","expires_in":3600}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(resp.as_bytes()).await.unwrap();
        });

        let storage = AuthStorage::new(auth_path.clone()).unwrap();
        storage
            .save(&AuthStore {
                version: 1,
                chatgpt: None,
                google: Some(StoredGoogleAuth {
                    refresh_token: "1//host-refresh".to_string(),
                    access_token: "ya29.host-stale".to_string(),
                    expires_at: Some(Utc::now() - ChronoDuration::minutes(10)),
                    client_id: Some("cid".to_string()),
                    client_secret: Some("csec".to_string()),
                    token_uri: Some(format!("http://{addr}/token")),
                    email: Some("host-drive@example.com".to_string()),
                    last_refresh_at: None,
                }),
            })
            .unwrap();

        let provider = HostDriveTokenProvider::from_config(GoogleDriveAuthConfig {
            storage_path: auth_path,
            auth_url: GOOGLE_AUTH_URL.to_string(),
            token_url: format!("http://{addr}/token"),
            client_id: Some("cid".to_string()),
            client_secret: Some("csec".to_string()),
            callback_port: 0,
            scope: GoogleDriveScope::ReadOnly,
        })
        .unwrap();

        let summary = provider.auth_summary();
        assert!(summary.authenticated);
        assert_eq!(summary.scopes, vec![GOOGLE_DRIVE_READONLY_SCOPE]);
        assert!(!summary.has_file_write_scope);
        assert_eq!(summary.email.as_deref(), Some("host-drive@example.com"));

        let token = provider.access_token().await.unwrap();
        assert_eq!(token.access_token, "ya29.host-refreshed");
        assert!(!format!("{token:?}").contains("ya29.host-refreshed"));
    }
}
