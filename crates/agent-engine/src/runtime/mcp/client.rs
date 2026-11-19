//! High-level MCP client lifecycle: initialize handshake, `tools/list`, and `tools/call`.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde_json::Value;

use crate::tools::Sandbox;

use super::http::StreamableHttpMcpTransport;
use super::protocol::{
    CallToolParams, ClientInfo, InitializeParams, ListToolsResult, MCP_PROTOCOL_VERSION,
    McpRemoteToolDefinition, McpServerCapabilities, McpToolCallOutput,
};
use super::stdio::StdioMcpTransport;
use super::transport::{McpServerConfig, McpTransport, McpTransportKind};

const MAX_TOOL_LIST_PAGES: usize = 32;

/// Live initialized MCP client bound to one stdio or Streamable HTTP server transport.
pub struct McpClient {
    server_id: String,
    timeout: Duration,
    capabilities: McpServerCapabilities,
    transport: Arc<dyn McpTransport>,
}

impl McpClient {
    /// Connects to `config` (confining stdio subprocesses via `sandbox`), runs `initialize`, and
    /// sends `notifications/initialized`.
    pub async fn connect(config: &McpServerConfig, sandbox: &Sandbox) -> Result<Self> {
        let transport: Arc<dyn McpTransport> = match &config.transport {
            McpTransportKind::Stdio(stdio) => {
                Arc::new(StdioMcpTransport::spawn(&config.server_id, stdio, sandbox).await?)
            }
            McpTransportKind::StreamableHttp(http) => Arc::new(StreamableHttpMcpTransport::new(
                &config.server_id,
                http.clone(),
            )?),
        };

        let init_result = match transport
            .request(
                "initialize",
                Some(serde_json::to_value(InitializeParams {
                    protocol_version: MCP_PROTOCOL_VERSION,
                    capabilities: serde_json::json!({}),
                    client_info: ClientInfo {
                        name: "alan",
                        version: env!("CARGO_PKG_VERSION"),
                    },
                })?),
                config.timeout,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                transport.shutdown().await;
                return Err(error).with_context(|| {
                    format!("MCP initialize failed for server `{}`", config.server_id)
                });
            }
        };

        let capabilities: McpServerCapabilities = serde_json::from_value(init_result)
            .with_context(|| {
                format!(
                    "MCP server `{}` returned invalid initialize result",
                    config.server_id
                )
            })?;

        if let Err(error) = transport
            .notify(
                "notifications/initialized",
                Some(serde_json::json!({})),
                config.timeout,
            )
            .await
        {
            transport.shutdown().await;
            return Err(error).with_context(|| {
                format!(
                    "MCP initialized notification failed for server `{}`",
                    config.server_id
                )
            });
        }

        Ok(Self {
            server_id: config.server_id.clone(),
            timeout: config.timeout,
            capabilities,
            transport,
        })
    }

    /// Returns the server's negotiated MCP capabilities.
    pub fn capabilities(&self) -> &McpServerCapabilities {
        &self.capabilities
    }

    /// Enumerates all tools exposed by this server via paginated `tools/list` requests.
    pub async fn list_tools(&self) -> Result<Vec<McpRemoteToolDefinition>> {
        let mut cursor: Option<String> = None;
        let mut seen_cursors = BTreeSet::new();
        let mut tools = Vec::new();
        for _ in 0..MAX_TOOL_LIST_PAGES {
            let params = cursor
                .as_deref()
                .map(|value| serde_json::json!({ "cursor": value }));
            let result = self
                .transport
                .request("tools/list", params, self.timeout)
                .await
                .with_context(|| format!("MCP tools/list failed for `{}`", self.server_id))?;
            let page: ListToolsResult = serde_json::from_value(result).with_context(|| {
                format!(
                    "MCP server `{}` returned invalid tools/list result",
                    self.server_id
                )
            })?;
            tools.extend(page.tools);
            match page.next_cursor.filter(|value| !value.is_empty()) {
                Some(next) => {
                    ensure!(
                        seen_cursors.insert(next.clone()),
                        "MCP server `{}` repeated pagination cursor `{next}`",
                        self.server_id
                    );
                    cursor = Some(next);
                }
                None => return Ok(tools),
            }
        }
        anyhow::bail!(
            "MCP server `{}` exceeded maximum `tools/list` page limit ({MAX_TOOL_LIST_PAGES})",
            self.server_id
        )
    }

    /// Invokes a remote MCP tool via `tools/call`.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpToolCallOutput> {
        let result = self
            .transport
            .request(
                "tools/call",
                Some(serde_json::to_value(CallToolParams {
                    name: name.to_string(),
                    arguments,
                })?),
                self.timeout,
            )
            .await
            .with_context(|| {
                format!(
                    "MCP tools/call `{name}` failed for server `{}`",
                    self.server_id
                )
            })?;
        serde_json::from_value(result).with_context(|| {
            format!(
                "MCP server `{}` returned invalid tools/call result for `{name}`",
                self.server_id
            )
        })
    }

    /// Gracefully terminates the underlying transport (killing stdio process groups).
    pub async fn shutdown(&self) {
        self.transport.shutdown().await;
    }
}
