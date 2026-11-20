//! Streamable HTTP MCP JSON-RPC 2.0 transport (application/json and SSE responses).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::Mutex;

use super::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use super::transport::{McpHttpTransportConfig, McpTransport, validate_http_endpoint_url};

const MAX_JSON_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MCP_SESSION_ID_HEADER: &str = "mcp-session-id";

/// Streamable HTTP JSON-RPC 2.0 client transport for MCP servers.
pub(crate) struct StreamableHttpMcpTransport {
    server_id: String,
    config: McpHttpTransportConfig,
    client: reqwest::Client,
    session_id: Mutex<Option<String>>,
    next_id: AtomicU64,
}

impl StreamableHttpMcpTransport {
    pub(crate) fn new(server_id: &str, config: McpHttpTransportConfig) -> Result<Self> {
        let _ = validate_http_endpoint_url(&config.url, "MCP server")?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build HTTP client for MCP transport")?;
        Ok(Self {
            server_id: server_id.to_string(),
            config,
            client,
            session_id: Mutex::new(None),
            next_id: AtomicU64::new(1),
        })
    }

    async fn build_post(
        &self,
        timeout: Duration,
        body: Vec<u8>,
    ) -> Result<reqwest::RequestBuilder> {
        let mut builder = self
            .client
            .post(&self.config.url)
            .timeout(timeout)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .body(body);
        for (name, value) in &self.config.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(session_id) = self.session_id.lock().await.as_deref() {
            builder = builder.header(MCP_SESSION_ID_HEADER, session_id);
        }
        Ok(builder)
    }

    async fn record_session_header(&self, headers: &reqwest::header::HeaderMap) {
        if let Some(value) = headers
            .get(MCP_SESSION_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            *self.session_id.lock().await = Some(value.to_string());
        }
    }

    async fn read_sse_response(
        &self,
        response: reqwest::Response,
        expected_id: u64,
        method: &str,
        timeout: Duration,
    ) -> Result<Value> {
        let mut events = response.bytes_stream().eventsource();
        tokio::time::timeout(timeout, async {
            while let Some(item) = events.next().await {
                let event = item.map_err(|error| {
                    anyhow!(
                        "invalid SSE frame from MCP server `{}`: {error}",
                        self.server_id
                    )
                })?;
                let payload = event.data.trim();
                if payload.is_empty() || payload == "[DONE]" {
                    continue;
                }
                let Ok(rpc) = serde_json::from_str::<JsonRpcResponse>(payload) else {
                    continue;
                };
                if rpc.id.as_ref().and_then(Value::as_u64) != Some(expected_id) {
                    continue;
                }
                return extract_rpc_outcome(&self.server_id, rpc);
            }
            bail!(
                "MCP SSE stream ended without response to `{method}` on `{}`",
                self.server_id
            )
        })
        .await
        .map_err(|_| {
            anyhow!(
                "MCP SSE response timed out for `{method}` on `{}`",
                self.server_id
            )
        })?
    }

    async fn read_json_response(
        &self,
        mut response: reqwest::Response,
        expected_id: u64,
        method: &str,
    ) -> Result<Value> {
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > MAX_JSON_RESPONSE_BYTES {
                bail!(
                    "MCP HTTP response exceeded size limit for `{}`",
                    self.server_id
                );
            }
            body.extend_from_slice(&chunk);
        }
        let rpc: JsonRpcResponse = serde_json::from_slice(&body).with_context(|| {
            format!(
                "failed to decode MCP JSON-RPC response for `{method}` on `{}`",
                self.server_id
            )
        })?;
        if let Some(id) = rpc.id.as_ref().and_then(Value::as_u64)
            && id != expected_id
        {
            bail!(
                "MCP HTTP response id mismatch on `{}` (expected {expected_id}, got {id})",
                self.server_id
            );
        }
        extract_rpc_outcome(&self.server_id, rpc)
    }
}

#[async_trait]
impl McpTransport for StreamableHttpMcpTransport {
    async fn request(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let payload = serde_json::to_vec(&JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        })?;
        let response = self
            .build_post(timeout, payload)
            .await?
            .send()
            .await
            .with_context(|| {
                format!(
                    "failed sending MCP HTTP request `{method}` to `{}`",
                    self.server_id
                )
            })?;
        self.record_session_header(response.headers()).await;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let detail = body.trim().chars().take(256).collect::<String>();
            bail!(
                "MCP HTTP server `{}` returned status {status} on `{method}`{}",
                self.server_id,
                if detail.is_empty() {
                    String::new()
                } else {
                    format!(": {detail}")
                }
            );
        }

        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"));
        if is_sse {
            self.read_sse_response(response, id, method, timeout).await
        } else {
            self.read_json_response(response, id, method).await
        }
    }

    async fn notify(&self, method: &str, params: Option<Value>, timeout: Duration) -> Result<()> {
        let payload = serde_json::to_vec(&JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        })?;
        let response = self
            .build_post(timeout, payload)
            .await?
            .send()
            .await
            .with_context(|| {
                format!(
                    "failed sending MCP HTTP notification `{method}` to `{}`",
                    self.server_id
                )
            })?;
        self.record_session_header(response.headers()).await;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "MCP HTTP notification `{method}` failed on `{}` with status {status}: {}",
                self.server_id,
                body.trim().chars().take(256).collect::<String>()
            );
        }
        Ok(())
    }
}

fn extract_rpc_outcome(server_id: &str, rpc: JsonRpcResponse) -> Result<Value> {
    if let Some(error) = rpc.error {
        bail!(
            "MCP error {} on `{server_id}`: {}",
            error.code,
            error.message
        );
    }
    Ok(rpc.result.unwrap_or(Value::Null))
}
