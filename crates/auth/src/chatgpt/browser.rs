//! Browser OAuth flow for ChatGPT authentication.

use super::{
    BrowserLoginCallbackReceipt, ChatgptAuthError, ChatgptAuthManager, ChatgptLoginSuccess,
    to_login_success,
};
use crate::pkce::generate_pkce;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::Rng;
use reqwest::StatusCode;
use std::io;
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{debug, warn};
use url::Url;

const DEFAULT_AUTH_ORIGINATOR: &str = "codex_cli_rs";
const DEFAULT_LOGIN_TIMEOUT_SECS: u64 = 300;
const MAX_HTTP_REQUEST_HEADER_BYTES: usize = 64 * 1024;

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
        let listener =
            TcpListener::bind(("127.0.0.1", self.inner.config.browser_callback_port)).await?;
        let remaining = (pending.expires_at - Utc::now())
            .to_std()
            .unwrap_or_default();
        if remaining.is_zero() {
            return Err(ChatgptAuthError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for OAuth callback",
            )));
        }

        tokio::time::timeout(remaining, async {
            let (mut stream, _) = listener.accept().await?;
            let request = read_http_request_headers(&mut stream).await?;
            let path = parse_http_request_target(&request).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid OAuth callback request")
            })?;
            let callback = Url::parse(&format!("http://localhost{path}"))?;
            let query = callback.query_pairs().collect::<Vec<_>>();

            let code = query
                .iter()
                .find(|(key, _)| key == "code")
                .map(|(_, value)| value.to_string());
            let returned_state = query
                .iter()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.to_string());
            let error_code = query
                .iter()
                .find(|(key, _)| key == "error")
                .map(|(_, value)| value.to_string());
            let error_description = query
                .iter()
                .find(|(key, _)| key == "error_description")
                .map(|(_, value)| value.to_string());

            if let Some(error_code) = error_code {
                let message = error_description.unwrap_or_else(|| "Sign-in failed".to_string());
                write_http_response(
                    &mut stream,
                    StatusCode::BAD_REQUEST,
                    &render_html("ChatGPT Login Failed", &message),
                )
                .await?;
                return Err(ChatgptAuthError::LoginFailed(format!(
                    "{error_code}: {message}"
                )));
            }

            let returned_state = match returned_state {
                Some(state) => state,
                None => {
                    write_http_response(
                        &mut stream,
                        StatusCode::BAD_REQUEST,
                        &render_html(
                            "ChatGPT Login Failed",
                            "OAuth callback did not include state.",
                        ),
                    )
                    .await?;
                    return Err(ChatgptAuthError::LoginFailed(
                        "OAuth callback did not include state".to_string(),
                    ));
                }
            };

            if returned_state != pending.state {
                write_http_response(
                    &mut stream,
                    StatusCode::BAD_REQUEST,
                    &render_html("ChatGPT Login Failed", "State mismatch in OAuth callback."),
                )
                .await?;
                return Err(ChatgptAuthError::LoginFailed(
                    "OAuth state mismatch".to_string(),
                ));
            }

            let code = match code {
                Some(code) => code,
                None => {
                    write_http_response(
                        &mut stream,
                        StatusCode::BAD_REQUEST,
                        &render_html(
                            "ChatGPT Login Failed",
                            "OAuth callback did not include code.",
                        ),
                    )
                    .await?;
                    return Err(ChatgptAuthError::LoginFailed(
                        "OAuth callback did not include code".to_string(),
                    ));
                }
            };

            Ok(BrowserLoginCallbackReceipt {
                stream,
                completion: BrowserLoginCompletion {
                    code,
                    state: returned_state,
                },
            })
        })
        .await
        .map_err(|_| {
            ChatgptAuthError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "Timed out waiting for OAuth callback",
            ))
        })?
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
                "http://localhost:{}/auth/callback",
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

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn open_browser(url: &str) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        return Err(io::Error::other(
            "automatic browser launch is not supported on this platform",
        ));
    }

    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("browser launcher exited unsuccessfully"))
    }
}

pub(super) fn parse_http_request_target(request: &str) -> Option<String> {
    let mut lines = request.lines();
    let line = lines.next()?;
    let mut parts = line.split_whitespace();
    let _method = parts.next()?;
    let target = parts.next()?;
    Some(target.to_string())
}

pub(super) async fn read_http_request_headers<R>(reader: &mut R) -> io::Result<String>
where
    R: AsyncRead + Unpin,
{
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0u8; 1024];

    loop {
        if request
            .windows(b"\r\n\r\n".len())
            .any(|window| window == b"\r\n\r\n")
        {
            return Ok(String::from_utf8_lossy(&request).into_owned());
        }

        if request.len() >= MAX_HTTP_REQUEST_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "OAuth callback request headers exceeded limit",
            ));
        }

        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "OAuth callback request ended before headers were complete",
            ));
        }

        request.extend_from_slice(&buffer[..read]);
    }
}

async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: StatusCode,
    body: &str,
) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("OK"),
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await
}

fn render_html(title: &str, message: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body><h1>{}</h1><p>{}</p></body></html>",
        html_escape(title),
        html_escape(title),
        html_escape(message)
    )
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
