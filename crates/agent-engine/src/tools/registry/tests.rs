use super::*;
use crate::config::Config;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct TestAdapter {
    namespace_cwd: PathBuf,
    cwd: PathBuf,
}

impl crate::tools::ToolExecutionAdapter for TestAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.namespace_cwd.clone()
    }

    fn cwd(&self) -> Result<PathBuf> {
        Ok(self.cwd.clone())
    }

    fn resolve_path(
        &self,
        _namespace_cwd: &std::path::Path,
        path: &std::path::Path,
    ) -> Result<PathBuf> {
        Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.cwd.join(path)
        })
    }

    fn visible_path(&self, path: &std::path::Path) -> PathBuf {
        path.to_path_buf()
    }

    fn project_text(&self, text: &str) -> String {
        text.to_string()
    }

    fn sandbox(&self) -> Result<crate::tools::Sandbox> {
        Ok(crate::tools::Sandbox::new(self.cwd.clone()))
    }
}

fn test_binding(namespace_cwd: &str, cwd: PathBuf, scratch: PathBuf) -> ToolExecutionBinding {
    ToolExecutionBinding::awaiting_host_projection(PathBuf::from(namespace_cwd), scratch)
        .with_adapter(Arc::new(TestAdapter {
            namespace_cwd: PathBuf::from(namespace_cwd),
            cwd,
        }))
}

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
        let cwd = ctx.cwd();
        Box::pin(async move { Ok(serde_json::json!({"cwd": cwd?.display().to_string()})) })
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
    ToolContext::from_binding(
        test_binding(
            "/mnt/source",
            PathBuf::from("/mnt/source"),
            PathBuf::from("/tmp"),
        ),
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
        runner.capability_for_tool("argument_capability", &serde_json::json!({"network": true})),
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

    let filtered =
        registry.catalog_filtered_clone_with_config(["override_tool"], Arc::new(Config::default()));
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
            _pid: u64,
            _binding: ToolExecutionBinding,
        ) -> Result<ToolExecutionBinding> {
            anyhow::bail!("grant was revoked")
        }
    }

    let mut registry = ToolRegistry::new();
    registry.register(TestTool);
    registry.set_default_execution_binding(test_binding(
        "/mnt/source",
        PathBuf::from("/host/source"),
        PathBuf::from("/tmp/scratch"),
    ));
    let runner = ToolProcessRunner::from_registry(&registry);
    runner.register_process_authority(7, Arc::new(Revoked));
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
        7,
        test_binding(
            "/mnt/child",
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
    registry.set_default_execution_binding(test_binding(
        "/mnt/source",
        PathBuf::from("/host/source"),
        PathBuf::from("/system-store/tmp"),
    ));

    let result = registry
        .execute("cwd_echo", serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(result["cwd"], "/host/source");
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

    let result = registry.validate_required_tools(&["tool-a".to_string(), "tool-b".to_string()]);
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
