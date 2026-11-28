//! Tool registry for managing and executing tools.

use super::context::{ToolContext, ToolExecutionBinding};
use anyhow::{Context, Result};
use jsonschema::{Draft, Validator};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tracing::debug;

use crate::config::Config;
use crate::llm::ToolDefinition;

mod process_runner;

pub use process_runner::{ToolProcessInvocation, ToolProcessOutcome, ToolProcessRunner};

/// Result type for tool execution
pub type ToolResult = Pin<Box<dyn Future<Output = Result<Value>> + Send>>;
type ToolFactory = dyn Fn() -> Box<dyn Tool> + Send + Sync;

/// A tool that can be executed by the agent
pub trait Tool: Send + Sync {
    /// Get the tool's name
    fn name(&self) -> &str;

    /// Get the tool's description
    fn description(&self) -> &str;

    /// Get the JSON Schema for the tool's parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with the given arguments and context
    fn execute(&self, arguments: Value, ctx: &ToolContext) -> ToolResult;

    /// Coarse capability classification used by runtime governance policy.
    fn capability(&self, _arguments: &Value) -> alan_agent_protocol::ToolCapability {
        alan_agent_protocol::ToolCapability::Read
    }

    /// Whether capability depends on the concrete invocation arguments.
    fn capability_is_argument_dependent(&self) -> bool {
        false
    }

    /// Positive native implementation guarantee; package capability claims do not opt in.
    fn presentation_is_read_only(&self) -> bool {
        false
    }

    /// Whether repeating this exact invocation is safe because it has no
    /// externally observable side effect for these arguments.
    ///
    /// Runtime governance still evaluates policy and sandbox authority for the
    /// call; this only lets an idempotent `Write` call skip duplicate-effect
    /// replay bookkeeping.
    fn allows_idempotent_retry(&self, _arguments: &Value) -> bool {
        false
    }

    /// Get the recommended timeout for this tool in seconds.
    fn timeout_secs(&self) -> usize {
        30
    }
}

/// Registry for managing tools
#[derive(Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    config: Arc<Config>,
    schema_cache: Arc<Mutex<HashMap<String, Arc<Validator>>>>,
    tool_factories: HashMap<String, Arc<ToolFactory>>,
    /// Shared by ordinary clones so runtime state and namespace tool runners
    /// observe approval-time changes to default execution authority.
    default_binding: Arc<Mutex<Option<ToolExecutionBinding>>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        let config = Arc::new(Config::default());
        Self::with_config(config)
    }

    pub fn with_config(config: Arc<Config>) -> Self {
        Self {
            tools: HashMap::new(),
            config,
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_factories: HashMap::new(),
            default_binding: Self::default_binding_cell(None),
        }
    }

    pub fn set_config(&mut self, config: Arc<Config>) {
        self.config = config;
    }

    /// Set a default execution binding for `execute()` calls that don't provide context.
    pub fn set_default_execution_binding(&mut self, binding: ToolExecutionBinding) {
        let mut default_binding = self
            .default_binding
            .lock()
            .expect("default binding mutex poisoned");
        *default_binding = Some(binding);
    }

    /// Get the configured default execution binding, if any.
    pub fn default_execution_binding(&self) -> Option<ToolExecutionBinding> {
        self.default_binding_snapshot()
    }

    /// Get the configured Alan OS working directory, if any.
    pub fn default_cwd(&self) -> Option<std::path::PathBuf> {
        self.default_binding_snapshot()
            .as_ref()
            .map(|binding| binding.namespace_cwd.clone())
    }

    /// Register a tool
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        debug!(%name, "Registering tool");
        self.schema_cache
            .lock()
            .expect("schema cache mutex poisoned")
            .remove(&name);
        self.tools.insert(name, Arc::new(tool));
    }

    /// Register a boxed tool (for dynamic tools)
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        debug!(%name, "Registering boxed tool");
        self.schema_cache
            .lock()
            .expect("schema cache mutex poisoned")
            .remove(&name);
        self.tools.insert(name, Arc::from(tool));
    }

    /// Register a shared tool instance.
    pub fn register_shared(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        debug!(%name, "Registering shared tool");
        self.schema_cache
            .lock()
            .expect("schema cache mutex poisoned")
            .remove(&name);
        self.tools.insert(name, tool);
    }

    /// Register a tool factory that can materialize a fresh catalog instance.
    pub fn register_tool_factory<F>(&mut self, name: &str, factory: F)
    where
        F: Fn() -> Box<dyn Tool> + Send + Sync + 'static,
    {
        self.tool_factories
            .insert(name.to_string(), Arc::new(factory));
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Materialize a fresh tool instance from the catalog.
    pub fn materialize(&self, name: &str) -> Option<Box<dyn Tool>> {
        self.tool_factories.get(name).map(|factory| factory())
    }

    /// Whether the catalog can materialize the named tool.
    pub fn has_tool_factory(&self, name: &str) -> bool {
        self.tool_factories.contains_key(name)
    }

    /// Check if a tool exists
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Resolve a tool's coarse capability classification.
    pub fn capability_for_tool(
        &self,
        name: &str,
        arguments: &Value,
    ) -> Option<alan_agent_protocol::ToolCapability> {
        self.get(name).map(|tool| tool.capability(arguments))
    }

    /// Whether the named Tool declares this exact invocation safe to repeat.
    pub fn allows_idempotent_retry_for_tool(&self, name: &str, arguments: &Value) -> bool {
        self.get(name)
            .is_some_and(|tool| tool.allows_idempotent_retry(arguments))
    }

    /// Get all registered tool names
    pub fn list_tools(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Clone this registry while rebinding runtime config.
    pub fn clone_with_config(&self, config: Arc<Config>) -> Self {
        Self {
            tools: self
                .tools
                .iter()
                .map(|(name, tool)| (name.clone(), Arc::clone(tool)))
                .collect(),
            config,
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_factories: self.tool_factories.clone(),
            default_binding: Self::default_binding_cell(self.default_binding_snapshot()),
        }
    }

    /// Clone this registry while keeping only the named tools.
    pub fn filtered_clone<I, S>(&self, allowed: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.filtered_clone_with_config(allowed, Arc::clone(&self.config))
    }

    /// Clone this registry while rebinding runtime config and keeping only the named tools.
    pub fn filtered_clone_with_config<I, S>(&self, allowed: I, config: Arc<Config>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let allowed = allowed
            .into_iter()
            .map(|name| name.as_ref().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        let tools = self
            .tools
            .iter()
            .filter(|(name, _)| allowed.contains(name.as_str()))
            .map(|(name, tool)| (name.clone(), Arc::clone(tool)))
            .collect();
        let tool_factories = self
            .tool_factories
            .iter()
            .filter(|(name, _)| allowed.contains(name.as_str()))
            .map(|(name, factory)| (name.clone(), Arc::clone(factory)))
            .collect();

        Self {
            tools,
            config,
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_factories,
            default_binding: Self::default_binding_cell(self.default_binding_snapshot()),
        }
    }

    /// Clone this registry by preserving already-registered tools first and
    /// materializing missing names from the catalog when possible.
    ///
    /// Missing allowed names are omitted. Callers that need strict allowlist
    /// enforcement should validate the result with `validate_required_tools`.
    pub fn catalog_filtered_clone_with_config<I, S>(&self, allowed: I, config: Arc<Config>) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let allowed = allowed
            .into_iter()
            .map(|name| name.as_ref().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        let mut cloned = Self {
            tools: HashMap::new(),
            config,
            schema_cache: Arc::new(Mutex::new(HashMap::new())),
            tool_factories: self
                .tool_factories
                .iter()
                .filter(|(name, _)| allowed.contains(name.as_str()))
                .map(|(name, factory)| (name.clone(), Arc::clone(factory)))
                .collect(),
            default_binding: Self::default_binding_cell(self.default_binding_snapshot()),
        };

        for name in allowed {
            if let Some(tool) = self.get(&name) {
                cloned.register_shared(tool);
                continue;
            }

            if let Some(materialized) = self.materialize(&name) {
                cloned.register_boxed(materialized);
            }
        }

        cloned
    }

    /// Get tool definitions for LLM function calling
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect()
    }

    /// Get the timeout the registry will apply for a tool execution.
    pub fn execution_timeout_secs(&self, name: &str) -> Option<usize> {
        self.get(name)
            .map(|tool| self.effective_timeout_secs(tool.as_ref()))
    }

    /// Serialize the namespace package manifest for one registered Tool.
    ///
    /// Alan OS Host uses this when assembling `/lib/exec/<tool>`; the manifest
    /// shape remains owned and validated by Agent Execution Engine.
    pub fn package_manifest_bytes(&self, name: &str) -> Result<Vec<u8>> {
        let tool = self
            .get(name)
            .with_context(|| format!("materialize Tool package metadata for {name}"))?;
        let manifest = crate::runtime::ToolPackageManifest::from_tool(
            tool.as_ref(),
            self.execution_timeout_secs(name).unwrap_or(30),
        )?;
        serde_json::to_vec(&manifest).with_context(|| format!("serialize Tool manifest for {name}"))
    }

    /// Create the Process runner used by `/proc` for Tool executables.
    pub fn process_runner(&self) -> ToolProcessRunner {
        ToolProcessRunner::from_registry(self)
    }

    fn effective_timeout_secs(&self, tool: &dyn Tool) -> usize {
        if self.config.tool_timeout_secs != 30 {
            self.config.tool_timeout_secs
        } else {
            tool.timeout_secs()
        }
    }

    /// Execute a tool by name with the given context
    pub async fn execute_with_context(
        &self,
        name: &str,
        arguments: Value,
        ctx: &ToolContext,
    ) -> Result<Value> {
        if let Some(tool) = self.get(name) {
            debug!(%name, "Executing tool");
            self.validate_tool_args(tool.as_ref(), &arguments)?;

            // Use tool-specific timeout unless the runtime config overrides it.
            let timeout_secs = self.effective_timeout_secs(tool.as_ref());

            let result = if timeout_secs == 0 {
                tool.execute(arguments, ctx).await
            } else {
                let timeout = std::time::Duration::from_secs(timeout_secs as u64);
                match tokio::time::timeout(timeout, tool.execute(arguments, ctx)).await {
                    Ok(result) => result,
                    Err(_) => anyhow::bail!("Tool execution timed out after {}s", timeout_secs),
                }
            };
            match result {
                Ok(mut value) => {
                    ctx.project_value(&mut value);
                    Ok(value)
                }
                Err(err) => Err(anyhow::anyhow!(ctx.project_text(&format!("{err:#}")))),
            }
        } else {
            anyhow::bail!("Tool not found: {}", name)
        }
    }

    /// Execute a tool by name (backward compatible, uses default context)
    /// Note: This creates a default ToolContext. Prefer execute_with_context for production use.
    pub async fn execute(&self, name: &str, arguments: Value) -> Result<Value> {
        let binding = self
            .default_binding_snapshot()
            .context("Tool execution requires an explicit Process binding")?;
        let ctx = ToolContext::from_binding(binding, self.config.clone());
        self.execute_with_context(name, arguments, &ctx).await
    }

    fn validate_tool_args(&self, tool: &dyn Tool, arguments: &Value) -> Result<()> {
        let tool_name = tool.name().to_string();
        let compiled = {
            let mut cache = self
                .schema_cache
                .lock()
                .expect("schema cache mutex poisoned");
            if let Some(schema) = cache.get(&tool_name) {
                Arc::clone(schema)
            } else {
                let schema = tool.parameters_schema();
                let compiled = Arc::new(
                    jsonschema::options()
                        .with_draft(Draft::Draft7)
                        .build(&schema)
                        .map_err(|e| {
                            anyhow::anyhow!("Invalid tool schema for {}: {}", tool.name(), e)
                        })?,
                );
                cache.insert(tool_name, Arc::clone(&compiled));
                compiled
            }
        };

        let details: Vec<String> = compiled
            .iter_errors(arguments)
            .map(|err| {
                let path = err.instance_path().to_string();
                let path = if path.is_empty() {
                    "/".to_string()
                } else {
                    path
                };
                format!("{}: {}", path, err)
            })
            .collect();
        if !details.is_empty() {
            anyhow::bail!(
                "Tool arguments validation failed for {}: {}",
                tool.name(),
                details.join("; ")
            );
        }

        Ok(())
    }

    fn default_binding_cell(
        binding: Option<ToolExecutionBinding>,
    ) -> Arc<Mutex<Option<ToolExecutionBinding>>> {
        Arc::new(Mutex::new(binding))
    }

    fn default_binding_snapshot(&self) -> Option<ToolExecutionBinding> {
        self.default_binding
            .lock()
            .expect("default binding mutex poisoned")
            .clone()
    }

    /// Validate that required tools are available
    pub fn validate_required_tools(&self, required: &[String]) -> Result<Vec<String>> {
        let mut missing = Vec::new();
        for tool in required {
            if !self.has(tool) {
                missing.push(tool.clone());
            }
        }
        Ok(missing)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
