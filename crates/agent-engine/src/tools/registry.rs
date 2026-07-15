//! Tool registry for managing and executing tools.

use super::{
    context::{ToolContext, ToolExecutionBinding},
    sandbox::SandboxSpec,
};
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

    /// Get the runtime-projected sandbox spec for default tool execution, if configured.
    pub fn default_sandbox_spec(&self) -> Option<SandboxSpec> {
        self.default_binding_snapshot()
            .as_ref()
            .and_then(|binding| binding.sandbox_spec.clone())
    }

    /// Get the writable roots that currently define default tool sandbox authority.
    pub fn default_sandbox_writable_roots(&self) -> Vec<std::path::PathBuf> {
        let Some(binding) = self.default_binding_snapshot() else {
            return Vec::new();
        };
        if let Some(spec) = binding.sandbox_spec.as_ref() {
            return spec.writable_roots.clone();
        }
        Vec::new()
    }

    /// Get the configured default working directory, if any.
    pub fn default_cwd(&self) -> Option<std::path::PathBuf> {
        self.default_binding_snapshot()
            .as_ref()
            .map(|binding| binding.cwd.clone())
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
    process_bindings: Mutex<HashMap<alan_kernel::Pid, ToolExecutionBinding>>,
    process_authorities: Mutex<HashMap<alan_kernel::Pid, Arc<dyn super::ToolExecutionAuthority>>>,
}

impl ToolProcessRunner {
    pub(crate) fn empty(config: Arc<Config>) -> Self {
        Self {
            inner: Arc::new(ToolProcessRunnerInner {
                tools: HashMap::new(),
                config,
                default_binding: Arc::new(Mutex::new(None)),
                process_bindings: Mutex::new(HashMap::new()),
                process_authorities: Mutex::new(HashMap::new()),
            }),
        }
    }

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

    pub(crate) fn register_process_binding(
        &self,
        pid: alan_kernel::Pid,
        binding: ToolExecutionBinding,
    ) {
        self.inner
            .process_bindings
            .lock()
            .expect("process binding mutex poisoned")
            .insert(pid, binding);
    }

    pub(crate) fn process_binding(&self, pid: alan_kernel::Pid) -> Option<ToolExecutionBinding> {
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
        pid: alan_kernel::Pid,
        authority: Arc<dyn super::ToolExecutionAuthority>,
    ) {
        self.inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
            .insert(pid, authority);
    }

    /// Drop the resolver when its owning Process exits.
    pub fn unregister_process_authority(&self, pid: alan_kernel::Pid) {
        self.inner
            .process_authorities
            .lock()
            .expect("process authority mutex poisoned")
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
        let name = invocation
            .exec
            .executable
            .rsplit('/')
            .next()
            .unwrap_or(invocation.exec.executable.as_str());
        let Some(tool) = self.inner.tools.get(name) else {
            return alan_kernel::ProcessOutcome::exited(127, b"Tool executable has no host\n");
        };
        let arguments = invocation
            .exec
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

fn process_json_outcome(exit_code: i32, value: Value) -> alan_kernel::ProcessOutcome {
    let mut bytes = serde_json::to_vec(&value).unwrap_or_else(|_| {
        serde_json::to_vec(&serde_json::json!({"success": exit_code == 0})).unwrap()
    });
    bytes.push(b'\n');
    alan_kernel::ProcessOutcome::exited(exit_code, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestTool;

    impl Tool for TestTool {
        fn name(&self) -> &str {
            "test_tool"
        }

        fn description(&self) -> &str {
            "A test tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            })
        }

        fn execute(&self, arguments: Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move {
                let input = arguments["input"].as_str().unwrap_or("default");
                Ok(serde_json::json!({"result": input}))
            })
        }
    }

    struct CwdEchoTool;

    impl Tool for CwdEchoTool {
        fn name(&self) -> &str {
            "cwd_echo"
        }

        fn description(&self) -> &str {
            "Return cwd from tool context"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }

        fn execute(&self, _arguments: Value, ctx: &ToolContext) -> ToolResult {
            let cwd = ctx.cwd.display().to_string();
            Box::pin(async move { Ok(serde_json::json!({"cwd": cwd})) })
        }
    }

    // Tool with custom capability and timeout
    struct NetworkTool;

    impl Tool for NetworkTool {
        fn name(&self) -> &str {
            "network_tool"
        }

        fn description(&self) -> &str {
            "Network tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }

        fn capability(&self, _arguments: &Value) -> alan_agent_protocol::ToolCapability {
            alan_agent_protocol::ToolCapability::Network
        }

        fn timeout_secs(&self) -> usize {
            120
        }

        fn execute(&self, _args: Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(serde_json::json!({"status": "ok"})) })
        }
    }

    struct ArgumentCapabilityTool;

    impl Tool for ArgumentCapabilityTool {
        fn name(&self) -> &str {
            "argument_capability"
        }

        fn description(&self) -> &str {
            "Argument capability tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }

        fn capability(&self, arguments: &Value) -> alan_agent_protocol::ToolCapability {
            if arguments["network"].as_bool() == Some(true) {
                alan_agent_protocol::ToolCapability::Network
            } else {
                alan_agent_protocol::ToolCapability::Read
            }
        }

        fn capability_is_argument_dependent(&self) -> bool {
            true
        }

        fn execute(&self, _args: Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(serde_json::json!({"status": "ok"})) })
        }
    }

    struct CatalogTestTool;

    impl Tool for CatalogTestTool {
        fn name(&self) -> &str {
            "catalog_test_tool"
        }

        fn description(&self) -> &str {
            "Catalog test Tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }

        fn execute(&self, _arguments: Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(serde_json::json!({"ok": true})) })
        }
    }

    fn test_ctx() -> ToolContext {
        ToolContext::new(
            PathBuf::from("/mnt/source"),
            PathBuf::from("/tmp"),
            Arc::new(Config::default()),
        )
    }

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.list_tools().is_empty());
    }

    #[test]
    fn test_tool_registry_default() {
        let registry: ToolRegistry = Default::default();
        assert!(registry.list_tools().is_empty());
    }

    #[test]
    fn test_tool_registry_register() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        assert!(registry.has("test_tool"));
        assert_eq!(registry.list_tools().len(), 1);
    }

    #[test]
    fn test_tool_registry_register_boxed() {
        let mut registry = ToolRegistry::new();
        registry.register_boxed(Box::new(TestTool));
        assert!(registry.has("test_tool"));
        assert_eq!(registry.list_tools().len(), 1);
    }

    #[test]
    fn test_tool_registry_get() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let tool = registry.get("test_tool");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "test_tool");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_tool_registry_capability_for_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        registry.register(NetworkTool);

        let args = serde_json::json!({});
        assert_eq!(
            registry.capability_for_tool("test_tool", &args),
            Some(alan_agent_protocol::ToolCapability::Read)
        );
        assert_eq!(
            registry.capability_for_tool("network_tool", &args),
            Some(alan_agent_protocol::ToolCapability::Network)
        );
        assert_eq!(registry.capability_for_tool("nonexistent", &args), None);
    }

    #[test]
    fn process_runner_resolves_capability_from_concrete_arguments() {
        let mut registry = ToolRegistry::new();
        registry.register(ArgumentCapabilityTool);
        let runner = ToolProcessRunner::from_registry(&registry);

        assert_eq!(
            runner
                .capability_for_tool("argument_capability", &serde_json::json!({"network": true})),
            Some(alan_agent_protocol::ToolCapability::Network)
        );
        assert_eq!(
            runner.capability_for_tool(
                "argument_capability",
                &serde_json::json!({"network": false})
            ),
            Some(alan_agent_protocol::ToolCapability::Read)
        );
    }

    #[test]
    fn test_execution_timeout_uses_tool_timeout_when_runtime_default() {
        let mut registry = ToolRegistry::new();
        registry.register(NetworkTool);

        assert_eq!(registry.execution_timeout_secs("network_tool"), Some(120));
        assert_eq!(registry.execution_timeout_secs("nonexistent"), None);
    }

    #[test]
    fn test_execution_timeout_uses_runtime_override() {
        let config = Arc::new(Config {
            tool_timeout_secs: 45,
            ..Default::default()
        });
        let mut registry = ToolRegistry::with_config(config);
        registry.register(NetworkTool);

        assert_eq!(registry.execution_timeout_secs("network_tool"), Some(45));
    }

    #[test]
    fn test_filtered_clone_with_config_prunes_tool_factories_to_allowed_tools() {
        let mut registry = ToolRegistry::new();
        registry.register_tool_factory("allowed_factory", || Box::new(CatalogTestTool));
        registry.register_tool_factory("blocked_factory", || Box::new(CatalogTestTool));

        let filtered =
            registry.filtered_clone_with_config(["allowed_factory"], Arc::new(Config::default()));

        assert!(filtered.materialize("allowed_factory").is_some());
        assert!(filtered.materialize("blocked_factory").is_none());
    }

    #[test]
    fn test_catalog_filtered_clone_with_config_prunes_tool_factories_to_allowed_tools() {
        let mut registry = ToolRegistry::new();
        registry.register_tool_factory("allowed_factory", || Box::new(CatalogTestTool));
        registry.register_tool_factory("blocked_factory", || Box::new(CatalogTestTool));

        let filtered = registry
            .catalog_filtered_clone_with_config(["allowed_factory"], Arc::new(Config::default()));

        assert!(filtered.materialize("allowed_factory").is_some());
        assert!(filtered.materialize("blocked_factory").is_none());
    }

    struct MarkerTool {
        name: &'static str,
        marker: &'static str,
    }

    impl Tool for MarkerTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "marker tool"
        }

        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }

        fn execute(&self, _arguments: Value, _ctx: &ToolContext) -> ToolResult {
            let marker = self.marker;
            Box::pin(async move { Ok(serde_json::json!({ "marker": marker })) })
        }
    }

    #[tokio::test]
    async fn test_catalog_filtered_clone_with_config_preserves_registered_overrides() {
        let mut registry = ToolRegistry::new();
        registry.register(MarkerTool {
            name: "override_tool",
            marker: "override",
        });
        registry.register_tool_factory("override_tool", || {
            Box::new(MarkerTool {
                name: "override_tool",
                marker: "factory",
            })
        });

        let filtered = registry
            .catalog_filtered_clone_with_config(["override_tool"], Arc::new(Config::default()));
        let result = filtered
            .execute_with_context("override_tool", serde_json::json!({}), &test_ctx())
            .await
            .unwrap();

        assert_eq!(result["marker"], serde_json::json!("override"));
    }

    #[test]
    fn test_tool_registry_get_tool_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let definitions = registry.get_tool_definitions();
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].name, "test_tool");
        assert_eq!(definitions[0].description, "A test tool");
        assert!(definitions[0].parameters.get("type").is_some());
    }

    #[tokio::test]
    async fn test_tool_registry_execute() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let ctx = test_ctx();
        let args = serde_json::json!({"input": "hello"});
        let result = registry.execute_with_context("test_tool", args, &ctx).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap()["result"], "hello");
    }

    #[tokio::test]
    async fn test_tool_registry_execute_nonexistent() {
        let registry = ToolRegistry::new();
        let ctx = test_ctx();
        let args = serde_json::json!({});

        let result = registry
            .execute_with_context("nonexistent", args, &ctx)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn process_server_rejects_unmounted_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        let runner = ToolProcessRunner::from_registry(&registry);
        let invocation = alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(1),
            parent: None,
            credentials: alan_kernel::Credentials::user("agent"),
            namespace: alan_kernel::Namespace::new(),
            exec: alan_kernel::ExecSpec {
                executable: "/bin/test_tool".to_string(),
                args: vec!["{}".to_string()],
                namespace: None,
                descriptors: Default::default(),
            },
        };

        let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;
        assert_eq!(outcome.exit_code, 127);
        assert_eq!(outcome.output, b"executable is not mounted\n");
    }

    #[tokio::test]
    async fn process_server_reconciles_late_bound_authority_before_tool_execution() {
        #[derive(Debug)]
        struct Revoked;

        impl crate::tools::ToolExecutionAuthority for Revoked {
            fn reconcile(
                &self,
                _pid: alan_kernel::Pid,
                _binding: ToolExecutionBinding,
            ) -> Result<ToolExecutionBinding> {
                anyhow::bail!("grant was revoked")
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(TestTool);
        registry.set_default_execution_binding(ToolExecutionBinding::new(
            PathBuf::from("/host/source"),
            PathBuf::from("/tmp/scratch"),
        ));
        let runner = ToolProcessRunner::from_registry(&registry);
        runner.register_process_authority(alan_kernel::Pid(7), Arc::new(Revoked));
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/bin/test_tool",
            alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::empty())),
            alan_kernel::Access::ReadOnly,
        );
        let invocation = alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(8),
            parent: Some(alan_kernel::Pid(7)),
            credentials: alan_kernel::Credentials::user("agent"),
            namespace,
            exec: alan_kernel::ExecSpec {
                executable: "/bin/test_tool".to_string(),
                args: vec![r#"{"input":"hello"}"#.to_string()],
                namespace: None,
                descriptors: Default::default(),
            },
        };

        let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;
        assert_eq!(outcome.exit_code, 1);
        assert!(
            String::from_utf8(outcome.output)
                .unwrap()
                .contains("grant was revoked")
        );
    }

    #[tokio::test]
    async fn process_server_uses_spawning_agent_execution_binding() {
        let mut registry = ToolRegistry::new();
        registry.register(CwdEchoTool);
        let runner = ToolProcessRunner::from_registry(&registry);
        runner.register_process_binding(
            alan_kernel::Pid(7),
            ToolExecutionBinding::new(
                PathBuf::from("/tmp/child-cwd"),
                PathBuf::from("/tmp/child-scratch"),
            ),
        );
        let mut namespace = alan_kernel::Namespace::new();
        namespace.mount(
            "/bin/cwd_echo",
            alan_ap::InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::new())),
            alan_kernel::Access::ReadOnly,
        );
        let invocation = alan_kernel::ProcessInvocation {
            pid: alan_kernel::Pid(8),
            parent: Some(alan_kernel::Pid(7)),
            credentials: alan_kernel::Credentials::user("agent"),
            namespace,
            exec: alan_kernel::ExecSpec {
                executable: "/bin/cwd_echo".to_string(),
                args: vec!["{}".to_string()],
                namespace: None,
                descriptors: Default::default(),
            },
        };

        let outcome = alan_kernel::ProcessRunner::run(&runner, invocation).await;
        assert_eq!(outcome.exit_code, 0);
        let value: Value = serde_json::from_slice(&outcome.output).unwrap();
        assert_eq!(value["cwd"], "/tmp/child-cwd");
    }

    #[tokio::test]
    async fn execute_requires_an_explicit_process_binding() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let error = registry
            .execute("test_tool", serde_json::json!({"input": "test"}))
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("requires an explicit Process binding")
        );
    }

    #[tokio::test]
    async fn execute_uses_the_configured_process_binding() {
        let mut registry = ToolRegistry::new();
        registry.register(CwdEchoTool);
        registry.set_default_execution_binding(ToolExecutionBinding::new(
            PathBuf::from("/host/source"),
            PathBuf::from("/system-store/tmp"),
        ));

        let result = registry
            .execute("cwd_echo", serde_json::json!({}))
            .await
            .unwrap();

        assert_eq!(result["cwd"], "/host/source");
    }

    #[test]
    fn complete_process_binding_updates_path_projection_and_sandbox_together() {
        let source = tempfile::tempdir().unwrap();
        let approved = tempfile::tempdir().unwrap();
        let launch_context = crate::ProcessLaunchContext::new(
            alan_kernel::Namespace::new(),
            alan_kernel::Credentials::user("agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new(
                "/mnt/source",
                source.path(),
                alan_kernel::Access::ReadWrite,
            )
            .unwrap(),
        );
        let registry = ToolRegistry::new();
        let runner = ToolProcessRunner::from_registry(&registry);
        runner.register_process_binding(
            alan_kernel::Pid(7),
            ToolExecutionBinding::from_launch_context(
                &launch_context,
                source.path().join("scratch"),
            )
            .unwrap(),
        );
        let grant = crate::HostMountGrant::new(
            "/mnt/approved",
            approved.path(),
            alan_kernel::Access::ReadOnly,
        )
        .unwrap();

        let updated_context = launch_context.with_host_mount(grant);
        runner.register_process_binding(
            alan_kernel::Pid(7),
            ToolExecutionBinding::from_launch_context(
                &updated_context,
                source.path().join("scratch"),
            )
            .unwrap(),
        );
        let binding = runner.process_binding(alan_kernel::Pid(7)).unwrap();
        assert!(
            binding
                .host_mounts
                .iter()
                .any(|mount| mount.namespace_path == "/mnt/approved")
        );
        let sandbox = binding.sandbox_spec.unwrap();
        assert!(
            sandbox
                .host_mounts
                .iter()
                .any(|mount| mount.namespace_path == "/mnt/approved")
        );
        assert!(
            sandbox
                .readable_roots
                .iter()
                .any(|path| path == &dunce::canonicalize(approved.path()).unwrap())
        );
        assert!(
            !sandbox
                .writable_roots
                .iter()
                .any(|path| path == &dunce::canonicalize(approved.path()).unwrap())
        );
    }

    #[tokio::test]
    async fn test_tool_registry_validation_failure() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let ctx = test_ctx();
        // Missing required "input" field
        let args = serde_json::json!({});
        let result = registry.execute_with_context("test_tool", args, &ctx).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("validation failed") || err_msg.contains("required"));
    }

    struct SlowTestTool {
        delay_ms: u64,
    }

    impl Tool for SlowTestTool {
        fn name(&self) -> &str {
            "slow_tool"
        }
        fn description(&self) -> &str {
            "A slow tool"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        fn timeout_secs(&self) -> usize {
            60 // Tool specifies 60s timeout
        }
        fn execute(&self, _args: Value, _ctx: &ToolContext) -> ToolResult {
            let delay = self.delay_ms;
            Box::pin(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                Ok(serde_json::json!({"status": "done"}))
            })
        }
    }

    #[tokio::test]
    async fn test_tool_timeout() {
        let config = Arc::new(Config {
            tool_timeout_secs: 1,
            ..Default::default()
        });
        let mut registry = ToolRegistry::with_config(config);
        registry.register(SlowTestTool { delay_ms: 2000 });

        let ctx = test_ctx();
        let result = registry
            .execute_with_context("slow_tool", serde_json::json!({}), &ctx)
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn test_tool_no_timeout_when_zero() {
        let config = Arc::new(Config {
            tool_timeout_secs: 0, // No timeout
            ..Default::default()
        });
        let mut registry = ToolRegistry::with_config(config);
        registry.register(SlowTestTool { delay_ms: 50 });

        let ctx = test_ctx();
        let result = registry
            .execute_with_context("slow_tool", serde_json::json!({}), &ctx)
            .await;

        assert!(result.is_ok());
    }

    struct CountingTool {
        calls: Arc<AtomicUsize>,
    }

    impl Tool for CountingTool {
        fn name(&self) -> &str {
            "counting_tool"
        }
        fn description(&self) -> &str {
            "Counts schema calls"
        }
        fn parameters_schema(&self) -> Value {
            self.calls.fetch_add(1, Ordering::SeqCst);
            serde_json::json!({"type": "object"})
        }
        fn execute(&self, _args: Value, _ctx: &ToolContext) -> ToolResult {
            Box::pin(async move { Ok(serde_json::json!({})) })
        }
    }

    #[tokio::test]
    async fn test_schema_caching() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = ToolRegistry::new();
        registry.register(CountingTool {
            calls: Arc::clone(&calls),
        });

        let ctx = test_ctx();
        registry
            .execute_with_context("counting_tool", serde_json::json!({}), &ctx)
            .await
            .unwrap();
        registry
            .execute_with_context("counting_tool", serde_json::json!({}), &ctx)
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_validate_required_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        let result = registry.validate_required_tools(&["test_tool".to_string()]);
        assert!(result.unwrap().is_empty());

        let result =
            registry.validate_required_tools(&["test_tool".to_string(), "missing".to_string()]);
        let missing = result.unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], "missing");
    }

    #[test]
    fn test_validate_required_tools_all_missing() {
        let registry = ToolRegistry::new();

        let result =
            registry.validate_required_tools(&["tool-a".to_string(), "tool-b".to_string()]);
        let missing = result.unwrap();
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"tool-a".to_string()));
        assert!(missing.contains(&"tool-b".to_string()));
    }

    #[test]
    fn test_tool_trait_defaults() {
        struct DefaultTool;
        impl Tool for DefaultTool {
            fn name(&self) -> &str {
                "default_tool"
            }
            fn description(&self) -> &str {
                "Default"
            }
            fn parameters_schema(&self) -> Value {
                serde_json::json!({})
            }
            fn execute(&self, _args: Value, _ctx: &ToolContext) -> ToolResult {
                Box::pin(async move { Ok(serde_json::json!({})) })
            }
        }

        let tool = DefaultTool;
        assert_eq!(
            tool.capability(&serde_json::json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
        assert_eq!(tool.timeout_secs(), 30);
    }

    #[test]
    fn test_tool_registry_re_register() {
        let mut registry = ToolRegistry::new();
        registry.register(TestTool);

        // Re-register should clear schema cache
        registry.register(TestTool);

        assert!(registry.has("test_tool"));
        assert_eq!(registry.list_tools().len(), 1);
    }
}
