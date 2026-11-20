//! Shared MCP transport abstractions, endpoint safety checks, and configuration.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use alan_agent_protocol::ToolCapability;
use anyhow::{Result, bail, ensure};
use async_trait::async_trait;
use serde_json::Value;

/// Confined stdio MCP subprocess configuration resolved from an installed package revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpStdioTransportConfig {
    /// Absolute path to the immutable executable file inside the materialized package store.
    pub executable: PathBuf,
    /// Fixed argument vector declared in the package manifest.
    pub args: Vec<String>,
    /// Sanitized non-secret static environment pairs declared in the manifest.
    pub env: BTreeMap<String, String>,
    /// Working directory inside Alan's execution boundary.
    pub cwd: PathBuf,
    /// Scratch directory mounted cleanly if needed by the subprocess.
    pub scratch_dir: PathBuf,
    /// Whether outbound network is permitted inside the OS sandbox.
    pub allow_network: bool,
}

/// Streamable HTTP MCP endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHttpTransportConfig {
    /// Validated `http://` or `https://` endpoint URL.
    pub url: String,
    /// Static non-secret HTTP request headers.
    pub headers: BTreeMap<String, String>,
}

/// Transport mode backing an MCP server session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpTransportKind {
    /// Confined subprocess communicating over newline-delimited JSON-RPC stdio.
    Stdio(McpStdioTransportConfig),
    /// Remote or local Streamable HTTP endpoint supporting JSON and SSE responses.
    StreamableHttp(McpHttpTransportConfig),
}

/// Complete host-resolved specification for one MCP server export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerConfig {
    /// Canonical MCP server identifier used for tool naming (`mcp_<server>_<tool>`).
    pub server_id: String,
    /// Optional human-readable description.
    pub description: Option<String>,
    /// Selected stdio or Streamable HTTP transport configuration.
    pub transport: McpTransportKind,
    /// Conservative tool governance capability applied to all tools discovered from this server.
    pub capability: ToolCapability,
    /// Per-request/tool call timeout.
    pub timeout: Duration,
}

/// JSON-RPC transport implemented by [`super::stdio::StdioMcpTransport`] and
/// [`super::http::StreamableHttpMcpTransport`].
#[async_trait]
pub(crate) trait McpTransport: Send + Sync {
    async fn request(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value>;
    async fn notify(&self, method: &str, params: Option<Value>, timeout: Duration) -> Result<()>;
    async fn shutdown(&self) {}
}

/// Validates an HTTP/HTTPS endpoint URL and rejects embedded userinfo credentials.
pub fn validate_http_endpoint_url(raw: &str, label: &str) -> Result<reqwest::Url> {
    let trimmed = raw.trim();
    ensure!(!trimmed.is_empty(), "{label} URL must not be empty");
    let url = reqwest::Url::parse(trimmed)
        .map_err(|error| anyhow::anyhow!("invalid {label} URL `{raw}`: {error}"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https"),
        "{label} URL scheme `{}` must be http or https",
        url.scheme()
    );
    ensure!(url.has_host(), "{label} URL must specify a host");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "{label} URL must not embed credentials"
    );
    Ok(url)
}

/// Rejects credential-bearing HTTP headers inside package manifests.
pub fn validate_safe_http_headers(headers: &BTreeMap<String, String>, label: &str) -> Result<()> {
    for (name, value) in headers {
        ensure!(
            !name.trim().is_empty() && !name.contains(['\r', '\n', ':']),
            "invalid {label} header name `{name}`"
        );
        ensure!(
            !value.contains(['\r', '\n']),
            "invalid {label} header value for `{name}`"
        );
        let lower = name.trim().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "authorization"
                | "proxy-authorization"
                | "cookie"
                | "set-cookie"
                | "x-api-key"
                | "api-key"
                | "x-auth-token"
        ) || lower.contains("token")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.ends_with("-key")
        {
            bail!(
                "{label} manifest must not embed credential header `{name}`; credentials belong in Host Store"
            );
        }
    }
    Ok(())
}

/// Rejects credential-looking or host-internal environment variables in stdio MCP manifests.
pub fn validate_safe_stdio_env(env: &BTreeMap<String, String>) -> Result<()> {
    for (key, value) in env {
        ensure!(
            !key.trim().is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid MCP env variable name `{key}`"
        );
        ensure!(
            !value.contains('\0') && !value.contains("${"),
            "MCP env `{key}` must be a static non-interpolated value"
        );
        let upper = key.to_ascii_uppercase();
        if upper.starts_with("ALAN_")
            || upper.starts_with("ANTHROPIC_")
            || upper.starts_with("OPENAI_")
            || upper.starts_with("GOOGLE_")
            || upper.starts_with("OPENROUTER_")
            || upper.contains("SECRET")
            || upper.contains("TOKEN")
            || upper.contains("PASSWORD")
            || upper.contains("CREDENTIAL")
            || upper.ends_with("_KEY")
            || upper.ends_with("_AUTH")
        {
            bail!("MCP manifest must not inject credential or control env var `{key}`");
        }
    }
    Ok(())
}
