//! MCP JSON-RPC 2.0 wire types and method payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Preferred MCP protocol revision negotiated during `initialize`.
pub(crate) const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

/// JSON-RPC 2.0 request envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 notification envelope (no `id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 error payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// JSON-RPC 2.0 response envelope matched by request `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JsonRpcResponse {
    #[serde(default)]
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<JsonRpcError>,
}

/// Parameters sent in `initialize`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InitializeParams {
    pub protocol_version: &'static str,
    pub capabilities: Value,
    pub client_info: ClientInfo,
}

/// Client identification metadata advertised during MCP initialization.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClientInfo {
    pub name: &'static str,
    pub version: &'static str,
}

/// Subset of `initialize` response retained by Alan's MCP client.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilities {
    /// Negotiated protocol version returned by the server.
    #[serde(default)]
    pub protocol_version: String,
    /// Optional server name/version reported by `serverInfo`.
    #[serde(default)]
    pub server_info: Option<McpServerInfo>,
}

/// Server name and version reported in `initialize`.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerInfo {
    /// Server identifier.
    #[serde(default)]
    pub name: String,
    /// Optional semantic version.
    #[serde(default)]
    pub version: Option<String>,
}

/// One tool schema returned by MCP `tools/list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRemoteToolDefinition {
    /// Remote MCP tool name.
    pub name: String,
    /// Optional human/model-readable tool description.
    #[serde(default)]
    pub description: Option<String>,
    /// JSON Schema for tool arguments.
    #[serde(default = "default_input_schema")]
    pub input_schema: Value,
}

fn default_input_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

/// Paginated `tools/list` response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListToolsResult {
    #[serde(default)]
    pub tools: Vec<McpRemoteToolDefinition>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

/// Parameters for MCP `tools/call`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Result returned by MCP `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCallOutput {
    /// Structured or textual content blocks emitted by the MCP server.
    #[serde(default)]
    pub content: Vec<Value>,
    /// Optional structured result payload (MCP 2025-03-26+).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    /// Whether the remote tool reported a domain error (`isError: true`).
    #[serde(default)]
    pub is_error: bool,
}
