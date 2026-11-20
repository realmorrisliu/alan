//! Shared loopback OAuth HTTP callback server and OS browser launcher.

use base64::Engine;
use rand::Rng;
use reqwest::StatusCode;
use std::io;
use std::process::Command;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

const MAX_HTTP_REQUEST_HEADER_BYTES: usize = 64 * 1024;

/// Errors raised while waiting for a local OAuth loopback callback.
#[derive(Debug, Error)]
pub enum LoopbackCallbackError {
    #[error("Timed out waiting for OAuth callback")]
    TimedOut,
    #[error("{error}: {description}")]
    ProviderError { error: String, description: String },
    #[error("OAuth callback did not include state")]
    MissingState,
    #[error("OAuth state mismatch")]
    StateMismatch,
    #[error("OAuth callback did not include code")]
    MissingCode,
    #[error("{0}")]
    InvalidRequest(String),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

/// Active loopback listener bound to `127.0.0.1`.
#[derive(Debug)]
pub struct LoopbackCallbackServer {
    listener: TcpListener,
    port: u16,
    callback_path: String,
}

/// Captured callback connection and query parameters prior to writing the final HTML page.
#[derive(Debug)]
pub struct LoopbackCallbackReceipt {
    pub stream: TcpStream,
    pub code: String,
    pub state: Option<String>,
}

impl LoopbackCallbackReceipt {
    /// Send a success HTML page to the browser after code exchange succeeds.
    pub async fn respond_success(&mut self, title: &str, message: &str) -> io::Result<()> {
        write_http_response(
            &mut self.stream,
            StatusCode::OK,
            &render_html(title, message),
        )
        .await
    }

    /// Send an error HTML page to the browser when code exchange fails.
    pub async fn respond_error(&mut self, title: &str, message: &str) -> io::Result<()> {
        write_http_response(
            &mut self.stream,
            StatusCode::BAD_REQUEST,
            &render_html(title, message),
        )
        .await
    }
}

impl LoopbackCallbackServer {
    /// Bind a loopback HTTP listener on `127.0.0.1:port` (`0` selects an ephemeral port).
    pub async fn bind(port: u16, callback_path: impl Into<String>) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        let bound_port = listener.local_addr()?.port();
        let mut path = callback_path.into();
        if !path.starts_with('/') {
            path.insert(0, '/');
        }
        Ok(Self {
            listener,
            port: bound_port,
            callback_path: path,
        })
    }

    /// The bound local TCP port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Full callback URL for `host` (e.g. `127.0.0.1` or `localhost`).
    pub fn callback_url(&self, host: &str) -> String {
        format!("http://{host}:{}{}", self.port, self.callback_path)
    }

    /// Wait up to `timeout` for a callback matching `callback_path` (ignoring unrelated paths such
    /// as `/favicon.ico`).
    pub async fn wait_for_callback(
        &self,
        timeout: Duration,
        expected_state: Option<&str>,
        error_page_title: &str,
    ) -> Result<LoopbackCallbackReceipt, LoopbackCallbackError> {
        if timeout.is_zero() {
            return Err(LoopbackCallbackError::TimedOut);
        }

        tokio::time::timeout(timeout, async {
            loop {
                let (mut stream, _) = self.listener.accept().await?;
                let request = read_http_request_headers(&mut stream).await?;
                let Some(target) = parse_http_request_target(&request) else {
                    let _ = write_http_response(
                        &mut stream,
                        StatusCode::BAD_REQUEST,
                        &render_html(error_page_title, "Invalid OAuth callback request."),
                    )
                    .await;
                    return Err(LoopbackCallbackError::InvalidRequest(
                        "Invalid OAuth callback request".to_string(),
                    ));
                };

                let callback = Url::parse(&format!("http://localhost{target}"))?;
                if callback.path() != self.callback_path {
                    let _ = write_http_response(
                        &mut stream,
                        StatusCode::NOT_FOUND,
                        &render_html("Not Found", "Waiting for OAuth callback."),
                    )
                    .await;
                    continue;
                }

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

                if let Some(error) = error_code {
                    let description = error_description.unwrap_or_else(|| "Sign-in failed".into());
                    write_http_response(
                        &mut stream,
                        StatusCode::BAD_REQUEST,
                        &render_html(error_page_title, &description),
                    )
                    .await?;
                    return Err(LoopbackCallbackError::ProviderError { error, description });
                }

                if let Some(expected) = expected_state {
                    let Some(state) = returned_state.as_deref() else {
                        write_http_response(
                            &mut stream,
                            StatusCode::BAD_REQUEST,
                            &render_html(error_page_title, "OAuth callback did not include state."),
                        )
                        .await?;
                        return Err(LoopbackCallbackError::MissingState);
                    };
                    if state != expected {
                        write_http_response(
                            &mut stream,
                            StatusCode::BAD_REQUEST,
                            &render_html(error_page_title, "State mismatch in OAuth callback."),
                        )
                        .await?;
                        return Err(LoopbackCallbackError::StateMismatch);
                    }
                }

                let Some(code) = code.filter(|c| !c.trim().is_empty()) else {
                    write_http_response(
                        &mut stream,
                        StatusCode::BAD_REQUEST,
                        &render_html(error_page_title, "OAuth callback did not include code."),
                    )
                    .await?;
                    return Err(LoopbackCallbackError::MissingCode);
                };

                return Ok(LoopbackCallbackReceipt {
                    stream,
                    code,
                    state: returned_state,
                });
            }
        })
        .await
        .map_err(|_| LoopbackCallbackError::TimedOut)?
    }
}

/// Generate a random URL-safe OAuth `state` / `login_id` token.
pub fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Open `url` in the platform default browser (`open`, `xdg-open`, or `rundll32`).
pub fn open_browser(url: &str) -> io::Result<()> {
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

/// Parse the HTTP request target from the first request line.
pub fn parse_http_request_target(request: &str) -> Option<String> {
    let mut lines = request.lines();
    let line = lines.next()?;
    let mut parts = line.split_whitespace();
    let _method = parts.next()?;
    let target = parts.next()?;
    Some(target.to_string())
}

/// Read HTTP headers up to `\r\n\r\n` with bounded buffer growth.
pub async fn read_http_request_headers<R>(reader: &mut R) -> io::Result<String>
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

/// Write a plain HTML HTTP/1.1 response and close the connection.
pub async fn write_http_response(
    stream: &mut TcpStream,
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

/// Render an escaped minimal HTML status page for OAuth callback completion.
pub fn render_html(title: &str, message: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loopback_server_skips_favicon_and_captures_callback_code() {
        let server = LoopbackCallbackServer::bind(0, "/callback").await.unwrap();
        let port = server.port();
        assert!(port > 0);
        assert_eq!(
            server.callback_url("127.0.0.1"),
            format!("http://127.0.0.1:{port}/callback")
        );

        let client_task = tokio::spawn(async move {
            let client = reqwest::Client::new();
            let favicon = client
                .get(format!("http://127.0.0.1:{port}/favicon.ico"))
                .send()
                .await
                .unwrap();
            assert_eq!(favicon.status(), StatusCode::NOT_FOUND);

            let cb = client
                .get(format!(
                    "http://127.0.0.1:{port}/callback?code=auth-code-123&state=st-1"
                ))
                .send()
                .await
                .unwrap();
            assert_eq!(cb.status(), StatusCode::OK);
            cb.text().await.unwrap()
        });

        let mut receipt = server
            .wait_for_callback(Duration::from_secs(5), Some("st-1"), "Login Failed")
            .await
            .unwrap();
        assert_eq!(receipt.code, "auth-code-123");
        assert_eq!(receipt.state.as_deref(), Some("st-1"));
        receipt
            .respond_success("Login Complete", "Done.")
            .await
            .unwrap();

        let body = client_task.await.unwrap();
        assert!(body.contains("Login Complete"));
    }

    #[tokio::test]
    async fn loopback_server_rejects_state_mismatch() {
        let server = LoopbackCallbackServer::bind(0, "/callback").await.unwrap();
        let port = server.port();

        let client_task = tokio::spawn(async move {
            reqwest::Client::new()
                .get(format!(
                    "http://127.0.0.1:{port}/callback?code=c&state=wrong"
                ))
                .send()
                .await
                .unwrap()
        });

        let err = server
            .wait_for_callback(Duration::from_secs(5), Some("expected"), "Login Failed")
            .await
            .unwrap_err();
        assert!(matches!(err, LoopbackCallbackError::StateMismatch));
        let resp = client_task.await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
