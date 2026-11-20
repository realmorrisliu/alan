//! Browser OAuth flow for ChatGPT authentication.

use super::{
    BrowserLoginCallbackReceipt, ChatgptAuthError, ChatgptAuthManager, ChatgptLoginSuccess,
    to_login_success,
};
use crate::browser::{
    LoopbackCallbackError, LoopbackCallbackServer, generate_state, open_browser, render_html,
    write_http_response,
};
#[cfg(test)]
pub(super) use crate::browser::{parse_http_request_target, read_http_request_headers};
use crate::pkce::generate_pkce;
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use std::io;
use std::time::Duration;
use tracing::{debug, warn};

const DEFAULT_AUTH_ORIGINATOR: &str = "codex_cli_rs";
const DEFAULT_LOGIN_TIMEOUT_SECS: u64 = 300;
const CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, Clone)]
pub struct BrowserLoginOptions {
    pub open_browser: bool,
    pub forced_workspace_id: Option<String>,
    pub timeout: Duration,
    pub redirect_uri: Option<String>,
    pub login_id: Option<String>,
}

impl Default for BrowserLoginOptions {
    fn default() -> Self {
        Self {
            open_browser: true,
            forced_workspace_id: None,
            timeout: Duration::from_secs(DEFAULT_LOGIN_TIMEOUT_SECS),
            redirect_uri: None,
            login_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingBrowserLogin {
    pub login_id: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    code_verifier: String,
    forced_workspace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserLoginCompletion {
    pub code: String,
    pub state: String,
}

impl ChatgptAuthManager {
    pub async fn login_with_browser(
        &self,
        options: BrowserLoginOptions,
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        let pending = self.begin_browser_login(options.clone())?;
        if options.open_browser {
            if let Err(error) = open_browser(&pending.auth_url) {
                warn!(
                    ?error,
                    "Failed to open browser automatically for ChatGPT login"
                );
                eprintln!("Open this URL in your browser:\n{}", pending.auth_url);
            }
        } else {
            println!("Open this URL in your browser:\n{}", pending.auth_url);
        }

        let mut receipt = self.wait_for_browser_callback(&pending).await?;
        let result = self
            .complete_browser_login(&pending, receipt.completion.clone())
            .await;
        self.write_browser_login_result(&mut receipt.stream, result.as_ref())
            .await?;

        debug!("ChatGPT browser login callback completed");
        result
    }

    pub async fn write_browser_login_result(
        &self,
        stream: &mut tokio::net::TcpStream,
        result: Result<&ChatgptLoginSuccess, &ChatgptAuthError>,
    ) -> io::Result<()> {
        match result {
            Ok(_) => {
                write_http_response(
                    stream,
                    StatusCode::OK,
                    &render_html(
                        "ChatGPT Login Complete",
                        "alan captured your ChatGPT session. You can close this window.",
                    ),
                )
                .await
            }
            Err(error) => {
                write_http_response(
                    stream,
                    StatusCode::BAD_REQUEST,
                    &render_html("ChatGPT Login Failed", &error.to_string()),
                )
                .await
            }
        }
    }

    pub async fn wait_for_browser_callback(
        &self,
        pending: &PendingBrowserLogin,
    ) -> Result<BrowserLoginCallbackReceipt, ChatgptAuthError> {
        let server =
            LoopbackCallbackServer::bind(self.inner.config.browser_callback_port, CALLBACK_PATH)
                .await?;
        let remaining = (pending.expires_at - Utc::now())
            .to_std()
            .unwrap_or_default();
        let receipt = server
            .wait_for_callback(remaining, Some(&pending.state), "ChatGPT Login Failed")
            .await
            .map_err(map_loopback_error)?;

        Ok(BrowserLoginCallbackReceipt {
            stream: receipt.stream,
            completion: BrowserLoginCompletion {
                code: receipt.code,
                state: receipt.state.unwrap_or_else(|| pending.state.clone()),
            },
        })
    }

    pub fn begin_browser_login(
        &self,
        options: BrowserLoginOptions,
    ) -> Result<PendingBrowserLogin, ChatgptAuthError> {
        let BrowserLoginOptions {
            open_browser: _,
            forced_workspace_id,
            timeout,
            redirect_uri,
            login_id,
        } = options;
        let pkce = generate_pkce();
        let login_id = login_id.unwrap_or_else(generate_state);
        let state = generate_state();
        let redirect_uri = redirect_uri.unwrap_or_else(|| {
            format!(
                "http://localhost:{}{CALLBACK_PATH}",
                self.inner.config.browser_callback_port
            )
        });
        let auth_url = build_authorize_url(
            &self.inner.config.issuer,
            &self.inner.config.client_id,
            &redirect_uri,
            &pkce.code_challenge,
            &state,
            forced_workspace_id.as_deref(),
        );
        Ok(PendingBrowserLogin {
            login_id,
            auth_url,
            redirect_uri,
            state,
            created_at: Utc::now(),
            expires_at: Utc::now()
                + chrono::Duration::from_std(timeout).unwrap_or(chrono::Duration::minutes(5)),
            code_verifier: pkce.code_verifier,
            forced_workspace_id,
        })
    }

    pub async fn complete_browser_login(
        &self,
        pending: &PendingBrowserLogin,
        completion: BrowserLoginCompletion,
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        if pending.state != completion.state {
            return Err(ChatgptAuthError::LoginFailed(
                "OAuth state mismatch".to_string(),
            ));
        }
        if Utc::now() > pending.expires_at {
            return Err(ChatgptAuthError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "browser login attempt expired before completion",
            )));
        }
        let tokens = self
            .exchange_code_for_tokens(
                &pending.redirect_uri,
                &pending.code_verifier,
                &completion.code,
            )
            .await?;
        let persisted = self.persist_tokens(tokens, pending.forced_workspace_id.as_deref())?;
        Ok(to_login_success(&persisted))
    }
}

fn map_loopback_error(error: LoopbackCallbackError) -> ChatgptAuthError {
    match error {
        LoopbackCallbackError::TimedOut => ChatgptAuthError::Io(io::Error::new(
            io::ErrorKind::TimedOut,
            "Timed out waiting for OAuth callback",
        )),
        LoopbackCallbackError::ProviderError { error, description } => {
            ChatgptAuthError::LoginFailed(format!("{error}: {description}"))
        }
        LoopbackCallbackError::MissingState => {
            ChatgptAuthError::LoginFailed("OAuth callback did not include state".to_string())
        }
        LoopbackCallbackError::StateMismatch => {
            ChatgptAuthError::LoginFailed("OAuth state mismatch".to_string())
        }
        LoopbackCallbackError::MissingCode => {
            ChatgptAuthError::LoginFailed("OAuth callback did not include code".to_string())
        }
        LoopbackCallbackError::InvalidRequest(message) => {
            ChatgptAuthError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
        }
        LoopbackCallbackError::Io(error) => ChatgptAuthError::Io(error),
        LoopbackCallbackError::Url(error) => ChatgptAuthError::Url(error),
    }
}

pub(super) fn build_authorize_url(
    issuer: &str,
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    forced_workspace_id: Option<&str>,
) -> String {
    let mut query = vec![
        ("response_type".to_string(), "code".to_string()),
        ("client_id".to_string(), client_id.to_string()),
        ("redirect_uri".to_string(), redirect_uri.to_string()),
        (
            "scope".to_string(),
            "openid profile email offline_access api.connectors.read api.connectors.invoke"
                .to_string(),
        ),
        ("code_challenge".to_string(), code_challenge.to_string()),
        ("code_challenge_method".to_string(), "S256".to_string()),
        ("id_token_add_organizations".to_string(), "true".to_string()),
        ("codex_cli_simplified_flow".to_string(), "true".to_string()),
        ("state".to_string(), state.to_string()),
        (
            "originator".to_string(),
            DEFAULT_AUTH_ORIGINATOR.to_string(),
        ),
    ];
    if let Some(workspace_id) = forced_workspace_id {
        query.push(("allowed_workspace_id".to_string(), workspace_id.to_string()));
    }
    let query = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", urlencoding::encode(&value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}/oauth/authorize?{query}", issuer.trim_end_matches('/'))
}
