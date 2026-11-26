//! Browser OAuth flow for Sign in with ChatGPT (Connected Apps / Plan Usage Sharing).

use super::{
    AUTHORIZE_PATH, BrowserLoginCallbackReceipt, ChatgptAuthError, ChatgptAuthManager,
    ChatgptLoginSuccess, DYNAMIC_CLIENT_ID, REQUIRED_SCOPE, to_login_success,
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
use std::io::{self, BufRead, IsTerminal};
use std::time::Duration;
use tracing::{debug, warn};
use url::Url;

const DEFAULT_LOGIN_TIMEOUT_SECS: u64 = 300;
const CALLBACK_PATH: &str = "/auth/callback";

#[derive(Debug, Clone)]
pub struct BrowserLoginOptions {
    pub open_browser: bool,
    pub timeout: Duration,
    pub redirect_uri: Option<String>,
    pub login_id: Option<String>,
}

impl Default for BrowserLoginOptions {
    fn default() -> Self {
        Self {
            open_browser: true,
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
    pub nonce: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    code_verifier: String,
}

#[derive(Debug, Clone)]
pub struct BrowserLoginCompletion {
    pub code: String,
    pub state: String,
    pub client_id: String,
}

impl ChatgptAuthManager {
    pub async fn login_with_browser(
        &self,
        options: BrowserLoginOptions,
    ) -> Result<ChatgptLoginSuccess, ChatgptAuthError> {
        let server = self.bind_browser_callback_server().await?;
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
        eprintln!(
            "Waiting for browser callback on {} (if running remotely or over SSH, paste the final redirect URL here and press Enter):",
            pending.redirect_uri
        );

        let (completion, callback_stream) = self.await_browser_or_manual(&server, &pending).await?;
        let result = self.complete_browser_login(&pending, completion).await;
        if let Some(mut stream) = callback_stream {
            self.write_browser_login_result(&mut stream, result.as_ref())
                .await?;
        }

        debug!("Sign in with ChatGPT browser login completed");
        result
    }

    async fn await_browser_or_manual(
        &self,
        server: &LoopbackCallbackServer,
        pending: &PendingBrowserLogin,
    ) -> Result<(BrowserLoginCompletion, Option<tokio::net::TcpStream>), ChatgptAuthError> {
        if !io::stdin().is_terminal() {
            let receipt = self.wait_for_browser_callback_on(server, pending).await?;
            return Ok((receipt.completion, Some(receipt.stream)));
        }

        let (manual_tx, mut manual_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let _reader_thread = std::thread::spawn(move || {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            let mut line = String::new();
            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let trimmed = line.trim().to_string();
                        if !trimmed.is_empty() && manual_tx.send(trimmed).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let callback_fut = self.wait_for_browser_callback_on(server, pending);
        tokio::pin!(callback_fut);

        loop {
            tokio::select! {
                receipt = &mut callback_fut => {
                    let _ = stop_tx.send(());
                    let receipt = receipt?;
                    return Ok((receipt.completion, Some(receipt.stream)));
                }
                maybe_line = manual_rx.recv() => {
                    match maybe_line {
                        Some(line) => match parse_manual_browser_callback(pending, &line) {
                            Ok(completion) => {
                                let _ = stop_tx.send(());
                                return Ok((completion, None));
                            }
                            Err(err) => {
                                eprintln!("{err}. Paste the full redirect URL starting with {}:", pending.redirect_uri);
                            }
                        },
                        None => {
                            let receipt = (&mut callback_fut).await?;
                            return Ok((receipt.completion, Some(receipt.stream)));
                        }
                    }
                }
            }
        }
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
                        "alan connected your ChatGPT account. You can close this window.",
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

    /// Bind the local loopback callback listener before launching the user's browser.
    pub async fn bind_browser_callback_server(
        &self,
    ) -> Result<LoopbackCallbackServer, ChatgptAuthError> {
        let port = self.inner.config.browser_callback_port;
        LoopbackCallbackServer::bind(port, CALLBACK_PATH)
            .await
            .map_err(|err| {
                if err.kind() == io::ErrorKind::AddrInUse {
                    ChatgptAuthError::LoginFailed(format!(
                        "Callback port {port} is already in use by another pending login session; cancel that login and try again"
                    ))
                } else {
                    ChatgptAuthError::Io(err)
                }
            })
    }

    pub async fn wait_for_browser_callback(
        &self,
        pending: &PendingBrowserLogin,
    ) -> Result<BrowserLoginCallbackReceipt, ChatgptAuthError> {
        let server = self.bind_browser_callback_server().await?;
        self.wait_for_browser_callback_on(&server, pending).await
    }

    pub async fn wait_for_browser_callback_on(
        &self,
        server: &LoopbackCallbackServer,
        pending: &PendingBrowserLogin,
    ) -> Result<BrowserLoginCallbackReceipt, ChatgptAuthError> {
        let remaining = (pending.expires_at - Utc::now())
            .to_std()
            .unwrap_or_default();
        let mut receipt = server
            .wait_for_callback(remaining, Some(&pending.state), "ChatGPT Login Failed")
            .await
            .map_err(map_loopback_error)?;

        let client_id = match extract_issued_client_id(receipt.query_param("client_id")) {
            Ok(client_id) => client_id,
            Err(err) => {
                let _ = receipt
                    .respond_error("ChatGPT Login Failed", &err.to_string())
                    .await;
                return Err(err);
            }
        };

        Ok(BrowserLoginCallbackReceipt {
            stream: receipt.stream,
            completion: BrowserLoginCompletion {
                code: receipt.code,
                state: receipt.state.unwrap_or_else(|| pending.state.clone()),
                client_id,
            },
        })
    }

    pub fn begin_browser_login(
        &self,
        options: BrowserLoginOptions,
    ) -> Result<PendingBrowserLogin, ChatgptAuthError> {
        let BrowserLoginOptions {
            open_browser: _,
            timeout,
            redirect_uri,
            login_id,
        } = options;
        let host_id = self.inner.storage.ensure_ext_agent_host_id()?;
        let pkce = generate_pkce();
        let login_id = login_id.unwrap_or_else(generate_state);
        let state = generate_state();
        let nonce = generate_state();
        let redirect_uri = redirect_uri.unwrap_or_else(|| {
            format!(
                "http://127.0.0.1:{}{CALLBACK_PATH}",
                self.inner.config.browser_callback_port
            )
        });
        let auth_url = build_authorize_url(
            &self.inner.config.issuer,
            &self.inner.config.agent_name_hint,
            &host_id,
            &self.inner.config.resource,
            &redirect_uri,
            &pkce.code_challenge,
            &state,
            &nonce,
        );
        Ok(PendingBrowserLogin {
            login_id,
            auth_url,
            redirect_uri,
            state,
            nonce,
            created_at: Utc::now(),
            expires_at: Utc::now()
                + chrono::Duration::from_std(timeout).unwrap_or(chrono::Duration::minutes(5)),
            code_verifier: pkce.code_verifier,
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
        let client_id = extract_issued_client_id(Some(&completion.client_id))?;
        let tokens = self
            .exchange_code_for_tokens(
                &client_id,
                &pending.redirect_uri,
                &pending.code_verifier,
                &completion.code,
            )
            .await?;
        let persisted = self.persist_tokens(
            tokens,
            client_id,
            super::parse_scopes(REQUIRED_SCOPE),
            Some(&pending.nonce),
        )?;
        Ok(to_login_success(&persisted))
    }
}

/// Parse a manually pasted browser redirect URL (for SSH or headless environments).
pub fn parse_manual_browser_callback(
    pending: &PendingBrowserLogin,
    input: &str,
) -> Result<BrowserLoginCompletion, ChatgptAuthError> {
    let url = Url::parse(input.trim()).map_err(|_| {
        ChatgptAuthError::LoginFailed("Paste the full callback URL from the browser".to_string())
    })?;
    let expected = Url::parse(&pending.redirect_uri)?;
    if url.origin() != expected.origin() || url.path() != expected.path() {
        return Err(ChatgptAuthError::LoginFailed(format!(
            "The pasted callback URL must start with {}",
            pending.redirect_uri
        )));
    }
    let query: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let find = |key: &str| {
        query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if let Some(error) = find("error") {
        let detail = find("error_description")
            .map(|desc| format!(": {desc}"))
            .unwrap_or_default();
        return Err(ChatgptAuthError::LoginFailed(format!(
            "ChatGPT authorization failed ({error}{detail})"
        )));
    }
    let state = find("state")
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ChatgptAuthError::LoginFailed("OAuth callback did not include state".to_string())
        })?;
    if state != pending.state {
        return Err(ChatgptAuthError::LoginFailed(
            "OAuth state mismatch".to_string(),
        ));
    }
    let code = find("code")
        .filter(|c| !c.trim().is_empty())
        .ok_or_else(|| {
            ChatgptAuthError::LoginFailed("OAuth callback did not include code".to_string())
        })?;
    let client_id = extract_issued_client_id(find("client_id"))?;
    Ok(BrowserLoginCompletion {
        code: code.to_string(),
        state: state.to_string(),
        client_id,
    })
}

fn extract_issued_client_id(raw: Option<&str>) -> Result<String, ChatgptAuthError> {
    let client_id = raw
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            ChatgptAuthError::LoginFailed(
                "OpenAI OAuth callback did not contain an issued client_id".to_string(),
            )
        })?;
    if client_id == DYNAMIC_CLIENT_ID {
        return Err(ChatgptAuthError::LoginFailed(
            "OpenAI OAuth callback returned placeholder dynamic_agent_client instead of an issued client_id".to_string(),
        ));
    }
    Ok(client_id.to_string())
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

#[allow(
    clippy::too_many_arguments,
    reason = "Explicit OAuth authorize query parameters"
)]
pub(super) fn build_authorize_url(
    issuer: &str,
    agent_name_hint: &str,
    ext_agent_host_id: &str,
    resource: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> String {
    let query = super::form_encode(&[
        ("client_id", DYNAMIC_CLIENT_ID),
        ("agent_name_hint", agent_name_hint),
        ("ext_agent_host_id", ext_agent_host_id),
        ("response_type", "code"),
        ("redirect_uri", redirect_uri),
        ("resource", resource),
        ("scope", REQUIRED_SCOPE),
        ("state", state),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("nonce", nonce),
    ]);
    format!("{}{AUTHORIZE_PATH}?{query}", issuer.trim_end_matches('/'))
}
