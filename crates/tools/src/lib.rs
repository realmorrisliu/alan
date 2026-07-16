//! Builtin tool implementations for the alan agent runtime.
//!
//! This crate provides 7 built-in tools as independent implementations of the
//! `Tool` trait defined in `alan-agent-engine`.
//!
//! Tool profiles:
//! - Core (default): read_file, write_file, edit_file, bash
//! - Read-only exploration: read_file, grep, glob, list_dir
//! - All: core + read-only exploration tools

mod bash_classifier;
mod exploration_tools;
mod file_tools;
#[cfg(test)]
mod test_support;

pub use exploration_tools::{GlobTool, GrepTool, ListDirTool};
pub use file_tools::{EditFileTool, ReadFileTool, WriteFileTool};

use alan_agent_engine::tools::{Tool, ToolContext, ToolRegistry, ToolResult};
use bash_classifier::classify_bash_command;
use serde_json::{Value, json};

// ============================================================================
// Bash
// ============================================================================

/// bash - Execute shell commands
#[derive(Default)]
pub struct BashTool;

impl BashTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands from the Process cwd, subject to namespace authority, policy, and execution-backend constraints. Prefer direct commands like rg, sed, git status, or curl. Avoid opaque interpreter wrappers like python -, python -c, bash -c, or sh -c unless genuinely required, because sandbox preflight may reject them conservatively."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute. Prefer direct commands instead of wrappers like python -, python -c, bash -c, or sh -c."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (max 300)",
                    "minimum": 1,
                    "maximum": 300,
                    "default": 60
                }
            }
        })
    }

    fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let sandbox = match ctx.sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let cwd = ctx.cwd.clone();
        let host_mounts = ctx.host_mounts.clone();
        let command = args["command"].as_str().unwrap_or("").to_string();
        let capability = classify_bash_command(&command);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(60).clamp(1, 300);

        Box::pin(async move {
            let result = sandbox
                .exec_with_timeout_and_capability(
                    &command,
                    &cwd,
                    Some(std::time::Duration::from_secs(timeout_secs)),
                    Some(capability),
                )
                .await?;

            Ok(json!({
                "stdout": project_host_paths(&result.stdout, &host_mounts),
                "stderr": project_host_paths(&result.stderr, &host_mounts),
                "exit_code": result.exit_code,
                "success": result.exit_code == 0
            }))
        })
    }

    fn capability(&self, args: &Value) -> alan_agent_protocol::ToolCapability {
        let command = args["command"].as_str().unwrap_or("");
        classify_bash_command(command)
    }

    fn capability_is_argument_dependent(&self) -> bool {
        true
    }

    fn timeout_secs(&self) -> usize {
        300 // Must be >= user-configurable timeout upper bound in schema
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn project_host_paths(text: &str, mounts: &[alan_agent_engine::HostMountGrant]) -> String {
    let mut projected = text.to_string();
    let mut mounts = mounts.iter().collect::<Vec<_>>();
    mounts.sort_by_key(|grant| std::cmp::Reverse(grant.host_path.as_os_str().len()));
    for grant in mounts {
        for root in [
            grant.host_path.clone(),
            dunce::canonicalize(&grant.host_path).unwrap_or_else(|_| grant.host_path.clone()),
        ] {
            projected = projected.replace(root.to_string_lossy().as_ref(), &grant.namespace_path);
        }
    }
    projected
}

// ============================================================================
// Factory
// ============================================================================

/// Register built-in tool catalog factories.
fn register_builtin_tool_factories(registry: &mut ToolRegistry) {
    registry.register_tool_factory("read_file", || Box::new(ReadFileTool::new()));
    registry.register_tool_factory("write_file", || Box::new(WriteFileTool::new()));
    registry.register_tool_factory("edit_file", || Box::new(EditFileTool::new()));
    registry.register_tool_factory("bash", || Box::new(BashTool::new()));
    registry.register_tool_factory("grep", || Box::new(GrepTool::new()));
    registry.register_tool_factory("glob", || Box::new(GlobTool::new()));
    registry.register_tool_factory("list_dir", || Box::new(ListDirTool::new()));
}

/// Register the built-in tool catalog on an existing registry.
pub fn register_builtin_tool_catalog(registry: &mut ToolRegistry) {
    register_builtin_tool_factories(registry);
}

/// Create the default core toolset (4 tools).
///
/// Core tools:
/// - read_file
/// - write_file
/// - edit_file
/// - bash
pub fn create_core_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ReadFileTool::new()),
        Box::new(WriteFileTool::new()),
        Box::new(EditFileTool::new()),
        Box::new(BashTool::new()),
    ]
}

/// Create the read-only exploration toolset (4 tools).
///
/// Read-only tools:
/// - read_file
/// - grep
/// - glob
/// - list_dir
pub fn create_read_only_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(ReadFileTool::new()),
        Box::new(GrepTool::new()),
        Box::new(GlobTool::new()),
        Box::new(ListDirTool::new()),
    ]
}

/// Create all 7 built-in tools.
pub fn create_all_tools() -> Vec<Box<dyn Tool>> {
    let mut tools = create_core_tools();
    tools.push(Box::new(GrepTool::new()));
    tools.push(Box::new(GlobTool::new()));
    tools.push(Box::new(ListDirTool::new()));
    tools
}

/// Create a ToolRegistry with the 4 core tools pre-registered.
pub fn create_tool_registry_with_core_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_core_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with the 4 read-only tools pre-registered.
pub fn create_tool_registry_with_read_only_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_read_only_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with all 7 built-in tools pre-registered.
pub fn create_tool_registry_with_all_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(
        alan_agent_engine::tools::ToolExecutionBinding::new(
            host_root.clone(),
            host_root.join(".alan-runtime-tmp"),
        )
        .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(host_root)),
    );

    for tool in create_all_tools() {
        registry.register_boxed(tool);
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use alan_agent_engine::Config;
    use alan_agent_engine::tools::ToolContext;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::test_support::tool_context_with_root;

    #[tokio::test]
    async fn test_bash_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "echo test_output"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());
        assert!(result["stdout"].as_str().unwrap().contains("test_output"));
    }

    #[tokio::test]
    async fn test_bash_tool_failure() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "exit 42"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(!result["success"].as_bool().unwrap());
        assert_eq!(result["exit_code"], 42);
    }

    #[tokio::test]
    async fn test_bash_tool_stderr() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"command": "echo error_msg >&2"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["success"].as_bool().unwrap());
        assert!(result["stderr"].as_str().unwrap().contains("error_msg"));
    }

    #[tokio::test]
    async fn test_bash_tool_working_directory() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create subdirectory
        tokio::fs::create_dir(mount_root.join("subdir"))
            .await
            .unwrap();

        let tool = BashTool::new();
        let config = Arc::new(Config::default());
        let ctx = ToolContext::from_binding(
            alan_agent_engine::tools::ToolExecutionBinding::new(
                mount_root.join("subdir"),
                mount_root.join("tmp"),
            )
            .with_sandbox_spec(alan_agent_engine::tools::SandboxSpec::seed(
                mount_root.clone(),
            )),
            config,
        );

        let args = json!({"command": "pwd"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert!(result["stdout"].as_str().unwrap().contains("subdir"));
    }

    // Tool trait method tests
    #[test]
    fn test_bash_tool_metadata() {
        let tool = BashTool::new();
        assert_eq!(tool.name(), "bash");
        assert_eq!(
            tool.capability(&json!({"command":"ls -la"})),
            alan_agent_protocol::ToolCapability::Read
        );
        assert_eq!(
            tool.capability(&json!({"command":"mkdir build"})),
            alan_agent_protocol::ToolCapability::Write
        );
        assert_eq!(
            tool.capability(&json!({"command":"curl https://example.com"})),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(tool.timeout_secs(), 300);
    }

    #[test]
    fn test_bash_tool_description_warns_about_eval_wrappers() {
        let tool = BashTool::new();
        let description = tool.description();
        assert!(description.contains("python -c"));
        assert!(description.contains("bash -c"));
        assert!(description.contains("Prefer direct commands"));
    }

    #[test]
    fn test_parameter_schemas_are_valid() {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ReadFileTool::new()),
            Box::new(WriteFileTool::new()),
            Box::new(EditFileTool::new()),
            Box::new(BashTool::new()),
            Box::new(GrepTool::new()),
            Box::new(GlobTool::new()),
            Box::new(ListDirTool::new()),
        ];

        for tool in tools {
            let schema = tool.parameters_schema();
            assert_eq!(
                schema["type"],
                "object",
                "{} schema missing type",
                tool.name()
            );
            assert!(
                schema.get("properties").is_some(),
                "{} schema missing properties",
                tool.name()
            );
        }
    }

    #[test]
    fn test_create_core_tools() {
        let tools = create_core_tools();
        assert_eq!(tools.len(), 4);

        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"write_file"));
        assert!(tool_names.contains(&"edit_file"));
        assert!(tool_names.contains(&"bash"));
    }

    #[test]
    fn test_create_read_only_tools() {
        let tools = create_read_only_tools();
        assert_eq!(tools.len(), 4);

        let tool_names: Vec<&str> = tools.iter().map(|tool| tool.name()).collect();
        assert!(tool_names.contains(&"read_file"));
        assert!(tool_names.contains(&"grep"));
        assert!(tool_names.contains(&"glob"));
        assert!(tool_names.contains(&"list_dir"));
    }

    #[test]
    fn test_create_all_tools() {
        let tools = create_all_tools();
        assert_eq!(tools.len(), 7);
    }

    #[test]
    fn test_create_tool_registry_with_core_tools() {
        let registry = create_tool_registry_with_core_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("bash").is_some());
        assert!(registry.get("grep").is_none());
        assert!(registry.get("glob").is_none());
        assert!(registry.get("list_dir").is_none());
    }

    #[tokio::test]
    async fn test_core_registry_materializes_missing_read_only_tool_for_child_mount_root() {
        let temp = TempDir::new().unwrap();
        let parent_mount_root = temp.path().join("parent");
        let child_mount_root = temp.path().join("child");
        tokio::fs::create_dir_all(&parent_mount_root).await.unwrap();
        tokio::fs::create_dir_all(&child_mount_root).await.unwrap();
        tokio::fs::write(child_mount_root.join("notes.txt"), "mount_root inspect\n")
            .await
            .unwrap();

        let registry = create_tool_registry_with_core_tools(parent_mount_root);
        assert!(registry.get("grep").is_none());

        let grep_tool = registry
            .materialize("grep")
            .expect("core registry should materialize grep from the catalog");
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(
            child_mount_root.clone(),
            child_mount_root.join("tmp"),
            config,
        );
        let result = grep_tool
            .execute(
                json!({
                    "pattern": "inspect",
                    "path": "."
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(result["total"], json!(1));
    }

    #[test]
    fn test_create_tool_registry_with_read_only_tools() {
        let registry = create_tool_registry_with_read_only_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("grep").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("list_dir").is_some());
        assert!(registry.get("write_file").is_none());
        assert!(registry.get("edit_file").is_none());
        assert!(registry.get("bash").is_none());
    }

    #[test]
    fn test_create_tool_registry_with_all_tools() {
        let registry = create_tool_registry_with_all_tools(PathBuf::from("/tmp"));
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("edit_file").is_some());
        assert!(registry.get("bash").is_some());
        assert!(registry.get("grep").is_some());
        assert!(registry.get("glob").is_some());
        assert!(registry.get("list_dir").is_some());
    }
}
