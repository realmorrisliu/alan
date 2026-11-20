//! OpenRouter OAuth PKCE (S256) browser and manual-code authentication.

use crate::browser::{LoopbackCallbackError, LoopbackCallbackServer, open_browser};
use crate::pkce::{PkceCodes, generate_pkce};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;
use thiserror::Error;
use url::Url;

pub const DEFAULT_OPENROUTER_AUTH_URL: &str = "https://openrouter.ai/auth";
pub const DEFAULT_OPENROUTER_KEY_EXCHANGE_URL: &str = "https://openrouter.ai/api/v1/auth/keys";

/// Configuration for OpenRouter PKCE browser login.
#[derive(Debug, Clone)]
pub struct OpenRouterOAuthConfig {
    /// OpenRouter browser authorization base URL (`https://openrouter.ai/auth` by default).
    pub auth_url: String,
    /// OpenRouter PKCE key-exchange endpoint (`https://openrouter.ai/api/v1/auth/keys` by default).
    pub key_exchange_url: String,
    /// Loopback TCP port on `127.0.0.1` (`0` binds an ephemeral port).
    pub callback_port: u16,
}

impl Default for OpenRouterOAuthConfig {
    fn default() -> Self {
        Self {
            auth_url: DEFAULT_OPENROUTER_AUTH_URL.to_string(),
            key_exchange_url: DEFAULT_OPENROUTER_KEY_EXCHANGE_URL.to_string(),
            callback_port: 0,
        }
    }
}

/// Errors returned by OpenRouter OAuth PKCE authentication.
#[derive(Debug, Error)]
pub enum OpenRouterAuthError {
    #[error("OpenRouter key exchange failed: {0}")]
    ExchangeFailed(String),
    #[error("OpenRouter authorization code was empty or missing")]
    MissingCode,
    #[error(transparent)]
    Callback(#[from] LoopbackCallbackError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Credentials returned from exchanging an OpenRouter PKCE authorization code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRouterLoginSuccess {
    pub api_key: String,
    pub user_id: Option<String>,
}

/// Active OpenRouter PKCE login session bound to `http://127.0.0.1:<port>/callback`.
#[derive(Debug)]
pub struct PendingOpenRouterBrowserLogin {
    authorize_url: String,
    callback_url: String,
    key_exchange_url: String,
    pkce: PkceCodes,
    server: LoopbackCallbackServer,
    client: reqwest::Client,
}

impl PendingOpenRouterBrowserLogin {
    /// Start a loopback OAuth listener and build the S256 PKCE authorization URL.
    pub async fn start(config: OpenRouterOAuthConfig) -> Result<Self, OpenRouterAuthError> {
        let server = LoopbackCallbackServer::bind(config.callback_port, "/callback").await?;
        let callback_url = server.callback_url("127.0.0.1");
        let pkce = generate_pkce();
        let authorize_url = build_authorize_url(&config.auth_url, &callback_url, &pkce)?;
        Ok(Self {
            authorize_url,
            callback_url,
            key_exchange_url: config.key_exchange_url,
            pkce,
            server,
            client: reqwest::Client::new(),
        })
    }

    /// URL the user opens in their browser to authorize Alan.
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    /// Bound loopback callback URL registered with OpenRouter.
    pub fn callback_url(&self) -> &str {
        &self.callback_url
    }

    /// PKCE code verifier matching the challenge in `authorize_url`.
    pub fn code_verifier(&self) -> &str {
        &self.pkce.code_verifier
    }

    /// Attempt to open `authorize_url` in the system browser.
    pub fn open_browser(&self) -> io::Result<()> {
        open_browser(&self.authorize_url)
    }

    /// Wait for the loopback callback up to `timeout` and exchange the code for an API key.
    pub async fn wait_and_exchange(
        self,
        timeout: Duration,
    ) -> Result<OpenRouterLoginSuccess, OpenRouterAuthError> {
        let mut receipt = self
            .server
            .wait_for_callback(timeout, None, "OpenRouter sign-in failed")
            .await?;

        match exchange_authorization_code(
            &self.client,
            &self.key_exchange_url,
            &receipt.code,
            &self.pkce.code_verifier,
        )
        .await
        {
            Ok(success) => {
                let _ = receipt
                    .respond_success(
                        "OpenRouter sign-in complete",
                        "You can close this window and return to Alan.",
                    )
                    .await;
                Ok(success)
            }
            Err(error) => {
                let _ = receipt
                    .respond_error("OpenRouter sign-in failed", &error.to_string())
                    .await;
                Err(error)
            }
        }
    }

    /// Exchange either a pasted authorization `code` or full redirected callback URL.
    pub async fn exchange_pasted_input(
        &self,
        pasted: &str,
    ) -> Result<OpenRouterLoginSuccess, OpenRouterAuthError> {
        let code = extract_authorization_code(pasted)?;
        exchange_authorization_code(
            &self.client,
            &self.key_exchange_url,
            &code,
            &self.pkce.code_verifier,
        )
        .await
    }
}

/// Exchange an OpenRouter PKCE authorization code via `POST /api/v1/auth/keys`.
pub async fn exchange_openrouter_code(
    key_exchange_url: &str,
    code: &str,
    code_verifier: &str,
) -> Result<OpenRouterLoginSuccess, OpenRouterAuthError> {
    exchange_authorization_code(
        &reqwest::Client::new(),
        key_exchange_url,
        code,
        code_verifier,
    )
    .await
}

/// Extract a raw authorization `code` or `?code=...` parameter from a pasted callback URL.
pub fn extract_authorization_code(input: &str) -> Result<String, OpenRouterAuthError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(OpenRouterAuthError::MissingCode);
    }

    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && let Ok(url) = Url::parse(trimmed)
    {
        if let Some((_, error)) = url.query_pairs().find(|(k, _)| k == "error") {
            let description = url
                .query_pairs()
                .find(|(k, _)| k == "error_description")
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "Authorization declined".to_string());
            return Err(OpenRouterAuthError::Callback(
                LoopbackCallbackError::ProviderError {
                    error: error.to_string(),
                    description,
                },
            ));
        }
        return url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| v.trim().to_string())
            .filter(|code| !code.is_empty())
            .ok_or(OpenRouterAuthError::MissingCode);
    }

    Ok(trimmed.to_string())
}

fn build_authorize_url(
    auth_url: &str,
    callback_url: &str,
    pkce: &PkceCodes,
) -> Result<String, OpenRouterAuthError> {
    let mut url = Url::parse(auth_url)?;
    url.query_pairs_mut()
        .append_pair("callback_url", callback_url)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

#[derive(Debug, Serialize)]
struct OpenRouterKeyExchangeRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
    code_challenge_method: &'static str,
}

#[derive(Debug, Deserialize)]
struct OpenRouterKeyExchangeResponse {
    key: Option<String>,
    user_id: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

async fn exchange_authorization_code(
    client: &reqwest::Client,
    key_exchange_url: &str,
    code: &str,
    code_verifier: &str,
) -> Result<OpenRouterLoginSuccess, OpenRouterAuthError> {
    let response = client
        .post(key_exchange_url)
        .json(&OpenRouterKeyExchangeRequest {
            code,
            code_verifier,
            code_challenge_method: "S256",
        })
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(OpenRouterAuthError::ExchangeFailed(format!(
            "HTTP {status}: {body}"
        )));
    }

    let parsed: OpenRouterKeyExchangeResponse = serde_json::from_str(&body).map_err(|error| {
        OpenRouterAuthError::ExchangeFailed(format!("invalid response JSON: {error}"))
    })?;
    let api_key = parsed
        .key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            let detail = parsed
                .error
                .map(|value| value.to_string())
                .unwrap_or_else(|| "response omitted `key`".to_string());
            OpenRouterAuthError::ExchangeFailed(detail)
        })?;
    let user_id = parsed.user_id.and_then(|value| match value {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    });

    Ok(OpenRouterLoginSuccess { api_key, user_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkce::PkceCodes;
    use base64::Engine;
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn extract_authorization_code_accepts_raw_code_and_callback_url() {
        assert_eq!(
            extract_authorization_code("  code-123  ").unwrap(),
            "code-123"
        );
        assert_eq!(
            extract_authorization_code("http://127.0.0.1:43210/callback?code=url-code&foo=bar")
                .unwrap(),
            "url-code"
        );
        assert!(extract_authorization_code("   ").is_err());
        assert!(
            extract_authorization_code("http://127.0.0.1:43210/callback?error=access_denied")
                .is_err()
        );
    }

    #[tokio::test]
    async fn browser_pkce_flow_exchanges_code_with_s256_verifier_against_mock_server() {
        let exchange_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let exchange_addr = exchange_listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let (mut socket, _) = exchange_listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let read = socket.read(&mut buf).await.unwrap();
            let request_text = String::from_utf8_lossy(&buf[..read]);
            let (_, body) = request_text.split_once("\r\n\r\n").unwrap();
            let payload: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
            assert_eq!(payload["code"], "or_code_xyz");
            assert_eq!(payload["code_challenge_method"], "S256");
            let verifier = payload["code_verifier"].as_str().unwrap().to_string();

            let resp_body = r#"{"key":"sk-or-v1-test-key","user_id":"usr_42"}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            verifier
        });

        let pending = PendingOpenRouterBrowserLogin::start(OpenRouterOAuthConfig {
            auth_url: "https://openrouter.ai/auth".to_string(),
            key_exchange_url: format!("http://{exchange_addr}/api/v1/auth/keys"),
            callback_port: 0,
        })
        .await
        .unwrap();

        let auth_url = Url::parse(pending.authorize_url()).unwrap();
        let query: std::collections::HashMap<_, _> = auth_url.query_pairs().into_owned().collect();
        assert_eq!(
            query.get("callback_url").map(String::as_str),
            Some(pending.callback_url())
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        let challenge = query.get("code_challenge").unwrap().clone();
        let expected_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(pending.code_verifier().as_bytes()));
        assert_eq!(challenge, expected_challenge);

        let callback_url = format!("{}?code=or_code_xyz", pending.callback_url());
        let client_task = tokio::spawn(async move {
            let response = reqwest::get(callback_url).await.unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK);
            let html = response.text().await.unwrap();
            assert!(html.contains("OpenRouter sign-in complete"));
        });

        let success = pending
            .wait_and_exchange(Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(success.api_key, "sk-or-v1-test-key");
        assert_eq!(success.user_id.as_deref(), Some("usr_42"));

        client_task.await.unwrap();
        let verifier = server_task.await.unwrap();
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_bytes())),
            challenge
        );
    }

    #[tokio::test]
    async fn manual_pasted_url_exchanges_code_for_api_key() {
        let exchange_listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let exchange_addr = exchange_listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = exchange_listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let read = socket.read(&mut buf).await.unwrap();
            let request_text = String::from_utf8_lossy(&buf[..read]);
            let (_, body) = request_text.split_once("\r\n\r\n").unwrap();
            let payload: serde_json::Value = serde_json::from_str(body.trim()).unwrap();
            assert_eq!(payload["code"], "manual_code_77");

            let resp_body = r#"{"key":"sk-or-v1-manual-key"}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
        });

        let pending = PendingOpenRouterBrowserLogin::start(OpenRouterOAuthConfig {
            auth_url: DEFAULT_OPENROUTER_AUTH_URL.to_string(),
            key_exchange_url: format!("http://{exchange_addr}/api/v1/auth/keys"),
            callback_port: 0,
        })
        .await
        .unwrap();

        let _unused = PkceCodes {
            code_verifier: pending.code_verifier().to_string(),
            code_challenge: String::new(),
        };
        let success = pending
            .exchange_pasted_input("http://127.0.0.1:44444/callback?code=manual_code_77")
            .await
            .unwrap();
        assert_eq!(success.api_key, "sk-or-v1-manual-key");
        assert_eq!(success.user_id, None);
    }
}
