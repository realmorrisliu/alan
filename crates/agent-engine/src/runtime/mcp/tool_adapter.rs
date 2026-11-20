//! Adapts MCP server sessions into ordinary Alan `Tool` trait implementations and manifests.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use alan_agent_protocol::ToolCapability;
use anyhow::{Result, bail, ensure};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::runtime::tool_packages::{
    ToolExecutionHints, ToolPackageManifest, resolve_execution_sandbox_and_cwd,
    validate_canonical_tool_name, validate_tool_parameters_schema,
};
use crate::tools::{Sandbox, Tool, ToolContext, ToolResult};

use super::client::McpClient;
use super::protocol::{McpRemoteToolDefinition, McpToolCallOutput};
use super::transport::{McpServerConfig, McpTransportKind};

struct SessionState {
    client: Option<Arc<McpClient>>,
    scope_key: Option<String>,
}

/// Process-scoped session managing one MCP client connection and Host-authority restarts.
pub struct McpServerSession {
    config: McpServerConfig,
    state: Mutex<SessionState>,
}

impl McpServerSession {
    /// Creates a lazy MCP server session that connects using Process `Sandbox` on first use or
    /// during [`Self::discover_tools`].
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(SessionState {
                client: None,
                scope_key: None,
            }),
        }
    }

    /// Returns the server configuration backing this session.
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Connects (or reuses an active connection matching `scope_key`), discovers tools, and
    /// builds [`McpProjectedTool`] adapters validated for canonical collision-free naming.
    pub async fn discover_tools(
        self: &Arc<Self>,
        sandbox: &Sandbox,
        scope_key: &str,
    ) -> Result<Vec<Arc<McpProjectedTool>>> {
        let client = self.ensure_client(sandbox, scope_key, None).await?;
        let remote_tools = client.list_tools().await?;
        let mut seen_names = BTreeSet::new();
        let mut projected = Vec::with_capacity(remote_tools.len());
        for remote in remote_tools {
            let projected_name = canonical_mcp_tool_name(&self.config.server_id, &remote.name)?;
            ensure!(
                seen_names.insert(projected_name.clone()),
                "MCP server `{}` exposes tools colliding on canonical name `{projected_name}`",
                self.config.server_id
            );
            let adapter = McpProjectedTool::new(
                Arc::clone(self),
                projected_name,
                remote,
                self.config.capability,
                self.config.timeout.as_secs().max(1) as usize,
            )?;
            projected.push(Arc::new(adapter));
        }
        Ok(projected)
    }

    async fn ensure_client(
        &self,
        sandbox: &Sandbox,
        scope_key: &str,
        cwd_override: Option<&Path>,
    ) -> Result<Arc<McpClient>> {
        let mut guard = self.state.lock().await;
        let needs_restart = match &guard.client {
            Some(_) => {
                matches!(self.config.transport, McpTransportKind::Stdio(_))
                    && guard.scope_key.as_deref() != Some(scope_key)
            }
            None => true,
        };
        if needs_restart {
            if let Some(old) = guard.client.take() {
                old.shutdown().await;
            }
            let effective_config = match (&self.config.transport, cwd_override) {
                (McpTransportKind::Stdio(stdio), Some(cwd)) => {
                    let mut next_stdio = stdio.clone();
                    next_stdio.cwd = cwd.to_path_buf();
                    McpServerConfig {
                        transport: McpTransportKind::Stdio(next_stdio),
                        ..self.config.clone()
                    }
                }
                _ => self.config.clone(),
            };
            let client = Arc::new(McpClient::connect(&effective_config, sandbox).await?);
            guard.client = Some(Arc::clone(&client));
            guard.scope_key = Some(scope_key.to_string());
            return Ok(client);
        }
        Ok(Arc::clone(
            guard
                .client
                .as_ref()
                .expect("checked existing MCP client above"),
        ))
    }

    /// Terminates any live MCP transport associated with this session.
    pub async fn shutdown(&self) {
        let client = self.state.lock().await.client.take();
        if let Some(client) = client {
            client.shutdown().await;
        }
    }
}

/// Ordinary Alan [`Tool`] projecting one remote tool exposed by an MCP server.
pub struct McpProjectedTool {
    session: Arc<McpServerSession>,
    projected_name: String,
    remote_name: String,
    description: String,
    parameters: Value,
    capability: ToolCapability,
    timeout_secs: usize,
}

impl McpProjectedTool {
    /// Creates a validated MCP tool projection.
    pub fn new(
        session: Arc<McpServerSession>,
        projected_name: String,
        remote: McpRemoteToolDefinition,
        capability: ToolCapability,
        timeout_secs: usize,
    ) -> Result<Self> {
        validate_canonical_tool_name(&projected_name)?;
        let parameters = if remote.input_schema.is_object() {
            remote.input_schema
        } else {
            serde_json::json!({ "type": "object", "properties": {} })
        };
        validate_tool_parameters_schema(&parameters)?;
        let description = remote
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "MCP tool `{}` from `{}`",
                    remote.name, session.config.server_id
                )
            });
        Ok(Self {
            session,
            projected_name,
            remote_name: remote.name,
            description,
            parameters,
            capability,
            timeout_secs,
        })
    }

    /// Builds a [`ToolPackageManifest`] describing this projected MCP tool in `/lib/exec`.
    pub fn manifest(&self) -> Result<ToolPackageManifest> {
        let manifest = ToolPackageManifest {
            version: ToolPackageManifest::schema_version(),
            name: self.projected_name.clone(),
            description: self.description.clone(),
            parameters: self.parameters.clone(),
            capability: self.capability,
            capability_is_argument_dependent: false,
            timeout_secs: self.timeout_secs,
            execution: ToolExecutionHints {
                arguments: "json_first_arg".to_string(),
                result: "stdout_json".to_string(),
            },
        };
        manifest.validate_for_name(&self.projected_name)?;
        Ok(manifest)
    }
}

impl Tool for McpProjectedTool {
    fn name(&self) -> &str {
        &self.projected_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.parameters.clone()
    }

    fn capability(&self, _args: &Value) -> ToolCapability {
        self.capability
    }

    fn timeout_secs(&self) -> usize {
        self.timeout_secs
    }

    fn execute(&self, arguments: Value, ctx: &ToolContext) -> ToolResult {
        let session = Arc::clone(&self.session);
        let remote_name = self.remote_name.clone();
        let (sandbox_res, cwd_res) = resolve_execution_sandbox_and_cwd(ctx);
        let scope_key = context_scope_key(ctx);

        Box::pin(async move {
            let sandbox = sandbox_res?;
            let cwd = cwd_res?;
            let full_scope = format!("{scope_key}:{}", cwd.display());
            let client = session
                .ensure_client(&sandbox, &full_scope, Some(&cwd))
                .await?;
            let output = client.call_tool(&remote_name, arguments).await?;
            mcp_output_to_json(output, &remote_name)
        })
    }
}

/// Computes deterministic canonical tool name `mcp_<server>_<tool>` safe for `/bin` and LLM APIs.
pub fn canonical_mcp_tool_name(server_id: &str, remote_tool_name: &str) -> Result<String> {
    let server = sanitize_segment(server_id);
    let tool = sanitize_segment(remote_tool_name);
    ensure!(
        !server.is_empty() && !tool.is_empty(),
        "cannot derive canonical MCP tool name from server `{server_id}` and tool `{remote_tool_name}`"
    );
    let name = format!("mcp_{server}_{tool}");
    ensure!(
        name.len() <= 64,
        "canonical MCP tool name `{name}` exceeds 64 characters"
    );
    validate_canonical_tool_name(&name)?;
    Ok(name)
}

fn sanitize_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_underscore = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

fn context_scope_key(ctx: &ToolContext) -> String {
    format!(
        "{:?}|{}|{}",
        ctx.trust_posture(),
        ctx.namespace_cwd.display(),
        ctx.cwd_grant_id.as_deref().unwrap_or("<none>")
    )
}

fn mcp_output_to_json(output: McpToolCallOutput, remote_name: &str) -> Result<Value> {
    if output.is_error {
        let message = extract_text_summary(&output.content)
            .unwrap_or_else(|| "MCP tool returned isError=true".to_string());
        bail!("MCP tool `{remote_name}` failed: {message}");
    }
    if let Some(structured) = output.structured_content {
        return Ok(structured);
    }
    if output.content.len() == 1
        && let Some(text) = output.content[0].get("text").and_then(Value::as_str)
        && let Ok(parsed) = serde_json::from_str::<Value>(text)
    {
        return Ok(parsed);
    }
    Ok(serde_json::json!({
        "success": true,
        "text": extract_text_summary(&output.content).unwrap_or_default(),
        "content": output.content,
    }))
}

fn extract_text_summary(content: &[Value]) -> Option<String> {
    let parts = content
        .iter()
        .filter_map(|item| {
            if item.get("type").and_then(Value::as_str) == Some("text") {
                item.get("text").and_then(Value::as_str)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}
