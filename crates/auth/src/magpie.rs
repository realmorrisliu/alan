//! Fast Magpie router auto-probe for local or custom OpenAI-compatible routers.

use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

pub const DEFAULT_MAGPIE_BASE_URL: &str = "http://127.0.0.1:3425/v1";
pub const DEFAULT_MAGPIE_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Errors returned while probing a Magpie router endpoint.
#[derive(Debug, Error)]
pub enum MagpieProbeError {
    #[error("Magpie endpoint `{base_url}` did not respond cleanly: {detail}")]
    Unreachable { base_url: String, detail: String },
    #[error("Magpie endpoint `{base_url}` returned unexpected HTTP {status}: {body}")]
    UnexpectedStatus {
        base_url: String,
        status: StatusCode,
        body: String,
    },
    #[error("Magpie endpoint `{base_url}` returned invalid model catalog JSON: {detail}")]
    InvalidCatalog { base_url: String, detail: String },
}

/// Result of probing a Magpie router's `/models` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MagpieProbeResult {
    pub base_url: String,
    pub auth_required: bool,
    pub models: Vec<String>,
}

/// Normalize a Magpie base URL (`None` or blank defaults to [`DEFAULT_MAGPIE_BASE_URL`]).
pub fn normalize_magpie_base_url(base_url: Option<&str>) -> String {
    base_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MAGPIE_BASE_URL)
        .trim_end_matches('/')
        .to_string()
}

/// Probe `base_url` (or `http://127.0.0.1:3425/v1`) with the default timeout and no API key.
pub async fn probe_magpie(base_url: Option<&str>) -> Result<MagpieProbeResult, MagpieProbeError> {
    probe_magpie_with_options(base_url, None, DEFAULT_MAGPIE_PROBE_TIMEOUT).await
}

/// Probe `<base_url>/models` with optional Bearer API key and timeout.
pub async fn probe_magpie_with_options(
    base_url: Option<&str>,
    api_key: Option<&str>,
    timeout: Duration,
) -> Result<MagpieProbeResult, MagpieProbeError> {
    let normalized = normalize_magpie_base_url(base_url);
    let url = format!("{normalized}/models");
    let client = reqwest::Client::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()
        .map_err(|error| MagpieProbeError::Unreachable {
            base_url: normalized.clone(),
            detail: error.to_string(),
        })?;

    let mut request = client.get(&url);
    if let Some(key) = api_key.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|error| MagpieProbeError::Unreachable {
            base_url: normalized.clone(),
            detail: error.to_string(),
        })?;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Ok(MagpieProbeResult {
            base_url: normalized,
            auth_required: true,
            models: Vec::new(),
        });
    }

    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(MagpieProbeError::UnexpectedStatus {
            base_url: normalized,
            status,
            body,
        });
    }

    let parsed: Value =
        serde_json::from_str(&body).map_err(|error| MagpieProbeError::InvalidCatalog {
            base_url: normalized.clone(),
            detail: error.to_string(),
        })?;
    let models = extract_model_ids(&parsed).map_err(|detail| MagpieProbeError::InvalidCatalog {
        base_url: normalized.clone(),
        detail,
    })?;

    Ok(MagpieProbeResult {
        base_url: normalized,
        auth_required: false,
        models,
    })
}

fn extract_model_ids(value: &Value) -> Result<Vec<String>, String> {
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .unwrap_or(value)
        .as_array()
        .ok_or_else(|| "response JSON did not contain a models array".to_string())?;

    let mut models = Vec::new();
    for entry in entries {
        let id = match entry {
            Value::String(s) => Some(s.as_str()),
            Value::Object(map) => map
                .get("id")
                .or_else(|| map.get("slug"))
                .or_else(|| map.get("name"))
                .and_then(Value::as_str),
            _ => None,
        };
        if let Some(id) = id.map(str::trim).filter(|value| !value.is_empty())
            && !models.iter().any(|existing| existing == id)
        {
            models.push(id.to_string());
        }
    }
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn probe_detects_keyless_router_and_extracts_models() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let read = socket.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..read]);
            assert!(req.starts_with("GET /v1/models "));
            assert!(!req.to_ascii_lowercase().contains("authorization:"));

            let body =
                r#"{"data":[{"id":"qwen3-coder"},{"id":"deepseek-r1"},{"id":"qwen3-coder"}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(resp.as_bytes()).await.unwrap();
        });

        let result = probe_magpie(Some(&format!("http://{addr}/v1/")))
            .await
            .unwrap();
        assert_eq!(result.base_url, format!("http://{addr}/v1"));
        assert!(!result.auth_required);
        assert_eq!(result.models, vec!["qwen3-coder", "deepseek-r1"]);
    }

    #[tokio::test]
    async fn probe_flags_auth_required_on_401_and_sends_bearer_when_supplied() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            for expected_authorized in [false, true] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 2048];
                let read = socket.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..read]);
                if !expected_authorized {
                    let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    socket.write_all(resp.as_bytes()).await.unwrap();
                } else {
                    assert!(
                        req.contains("authorization: Bearer secret-magpie-key")
                            || req.contains("Authorization: Bearer secret-magpie-key")
                    );
                    let body = r#"{"models":[{"slug":"claude-sonnet-4-6"}]}"#;
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    socket.write_all(resp.as_bytes()).await.unwrap();
                }
            }
        });

        let base = format!("http://{addr}/v1");
        let first = probe_magpie(Some(&base)).await.unwrap();
        assert!(first.auth_required);
        assert!(first.models.is_empty());

        let second = probe_magpie_with_options(
            Some(&base),
            Some("secret-magpie-key"),
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(!second.auth_required);
        assert_eq!(second.models, vec!["claude-sonnet-4-6"]);
    }
}
