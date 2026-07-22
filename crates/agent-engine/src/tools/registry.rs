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

/// Process-server host for Tool executables reached through `/proc/clone`.
#[derive(Clone)]
pub struct ToolProcessRunner {
    inner: Arc<ToolProcessRunnerInner>,
}

struct ToolProcessRunnerInner {
    tools: HashMap<String, Arc<dyn Tool>>,
    config: Arc<Config>,
    default_binding: Arc<Mutex<Option<ToolExecutionBinding>>>,
    process_bindings: Mutex<HashMap<u64, ToolExecutionBinding>>,
    process_authorities: Mutex<HashMap<u64, Arc<dyn super::ToolExecutionAuthority>>>,
}

/// Kernel-neutral inputs for one Tool executable invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProcessInvocation {
    pub pid: u64,
    pub parent: Option<u64>,
    pub executable: String,
    pub args: Vec<String>,
}

/// Kernel-neutral terminal result produced by one Tool executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolProcessOutcome {
    pub output: Vec<u8>,
    pub exit_code: i32,
}

impl ToolProcessOutcome {
    pub fn exited(exit_code: i32, output: impl Into<Vec<u8>>) -> Self {
        Self {
            output: output.into(),
            exit_code,
        }
    }
}

impl ToolProcessRunner {
    pub(crate) fn from_registry(registry: &ToolRegistry) -> Self {
        Self {
            inner: Arc::new(ToolProcessRunnerInner {
                tools: registry.tools.clone(),
                config: Arc::clone(&registry.config),
                default_binding: Arc::clone(&registry.default_binding),
                process_bindings: Mutex::new(HashMap::new()),
                process_authorities: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Bind host-resolved Tool execution authority to one assembled Process.
    pub fn register_process_binding(&self, pid: u64, binding: ToolExecutionBinding) {
        self.inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .insert(pid, binding);
    }

    pub(crate) fn process_binding(&self, pid: u64) -> Option<ToolExecutionBinding> {
        self.inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .get(&pid)
            .cloned()
            .or_else(|| {
                self.inner
                    .default_binding
                    .lock()
                    .expect("default binding mutex poisoned")
                    .clone()
            })
    }

    /// Install a late-bound authority resolver for one Agent Process.
    pub fn register_process_authority(
        &self,
        pid: u64,
        authority: Arc<dyn super::ToolExecutionAuthority>,
    ) {
        self.inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .insert(pid, authority);
    }

    /// Remove all Tool execution state when its owning Process exits.
    pub fn unregister_process(&self, pid: u64) {
        let inner = &self.inner;
        inner
            .process_bindings
            .lock()
            .expect("binding mutex poisoned")
            .remove(&pid);
        inner
            .process_authorities
            .lock()
            .expect("authority mutex poisoned")
            .remove(&pid);
    }

    pub(crate) fn capability_for_tool(
        &self,
        name: &str,
        arguments: &Value,
    ) -> Option<alan_agent_protocol::ToolCapability> {
        self.inner
            .tools
            .get(name)
            .map(|tool| tool.capability(arguments))
    }
}

impl ToolProcessRunner {
    /// Execute one Tool image after the Alan OS adapter has validated its namespace binding.
    pub async fn run(&self, invocation: ToolProcessInvocation) -> ToolProcessOutcome {
        let name = invocation
            .executable
            .rsplit('/')
            .next()
            .unwrap_or(invocation.executable.as_str());
        let Some(tool) = self.inner.tools.get(name) else {
            return ToolProcessOutcome::exited(127, b"Tool executable has no host\n");
        };
        let arguments = invocation
            .args
            .first()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .unwrap_or(Value::Null);
        let schema = tool.parameters_schema();
        let validator = match jsonschema::options()
            .with_draft(Draft::Draft7)
            .build(&schema)
        {
            Ok(validator) => validator,
            Err(error) => {
                return process_json_outcome(
                    1,
                    serde_json::json!({"success": false, "error": format!("invalid Tool schema: {error}")}),
                );
            }
        };
        let errors = validator
            .iter_errors(&arguments)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": errors.join("; ")}),
            );
        }
        let process_binding = self
            .inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .get(&invocation.parent.unwrap_or(invocation.pid))
            .cloned();
        let binding = process_binding.or_else(|| {
            self.inner
                .default_binding
                .lock()
                .expect("default binding mutex poisoned")
                .clone()
        });
        let Some(mut binding) = binding else {
            return process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": "Tool Process has no explicit execution binding"}),
            );
        };
        let authority_pid = invocation.parent.unwrap_or(invocation.pid);
        let authority = self
            .inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .get(&authority_pid)
            .cloned();
        if let Some(authority) = authority {
            binding = match authority.reconcile(authority_pid, binding) {
                Ok(binding) => binding,
                Err(error) => {
                    return process_json_outcome(
                        1,
                        serde_json::json!({
                            "success": false,
                            "error": format!("Tool Process authority is unavailable: {error}"),
                        }),
                    );
                }
            };
        }
        let context = ToolContext::from_binding(binding, Arc::clone(&self.inner.config));
        let timeout_secs = if self.inner.config.tool_timeout_secs != 30 {
            self.inner.config.tool_timeout_secs
        } else {
            tool.timeout_secs()
        };
        let execution = tool.execute(arguments, &context);
        let result = if timeout_secs == 0 {
            execution.await
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs as u64),
                execution,
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!(
                    "Tool execution timed out after {timeout_secs}s"
                )),
            }
        };
        match result {
            Ok(mut output) => {
                context.project_value(&mut output);
                process_json_outcome(0, output)
            }
            Err(error) => process_json_outcome(
                1,
                serde_json::json!({"success": false, "error": context.project_text(&format!("{error:#}"))}),
            ),
        }
    }
}

fn process_json_outcome(exit_code: i32, value: Value) -> ToolProcessOutcome {
    let mut bytes = serde_json::to_vec(&value).unwrap_or_else(|_| {
        serde_json::to_vec(&serde_json::json!({"success": exit_code == 0})).unwrap()
    });
    bytes.push(b'\n');
    ToolProcessOutcome::exited(exit_code, bytes)
}

#[cfg(test)]
#[async_trait::async_trait]
impl alan_kernel::ProcessRunner for ToolProcessRunner {
    async fn run(&self, invocation: alan_kernel::ProcessInvocation) -> alan_kernel::ProcessOutcome {
        if invocation
            .namespace
            .resolve(&invocation.exec.executable)
            .is_err()
        {
            return alan_kernel::ProcessOutcome::exited(127, b"executable is not mounted\n");
        }
        let outcome = self
            .run(ToolProcessInvocation {
                pid: invocation.pid.0,
                parent: invocation.parent.map(|pid| pid.0),
                executable: invocation.exec.executable,
                args: invocation.exec.args,
            })
            .await;
        alan_kernel::ProcessOutcome::exited(outcome.exit_code, outcome.output)
    }
}

#[cfg(test)]
mod tests;
