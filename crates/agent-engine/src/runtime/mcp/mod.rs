//! Native Model Context Protocol (MCP) JSON-RPC 2.0 client and Tool projection.

mod client;
mod http;
mod protocol;
mod stdio;
#[cfg(test)]
mod tests;
mod tool_adapter;
mod transport;

pub use client::McpClient;
pub use protocol::{McpRemoteToolDefinition, McpServerCapabilities, McpToolCallOutput};
pub use tool_adapter::{McpProjectedTool, McpServerSession, canonical_mcp_tool_name};
pub use transport::{
    McpHttpTransportConfig, McpServerConfig, McpStdioTransportConfig, McpTransportKind,
    validate_http_endpoint_url, validate_safe_http_headers, validate_safe_stdio_env,
};
