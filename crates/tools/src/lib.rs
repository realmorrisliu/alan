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
#[cfg(test)]
use exploration_tools::is_binary_file;
use serde_json::{Value, json};
#[cfg(test)]
use std::path::Path;

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

    #[tokio::test]
    async fn test_grep_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create test file
        tokio::fs::write(
            mount_root.join("search.txt"),
            "hello world\nfoo bar\nhello rust",
        )
        .await
        .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "search.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_grep_tool_case_insensitive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": false});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_grep_tool_case_sensitive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": true});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert_eq!(result["matches"][0]["content"], "hello");
    }

    #[tokio::test]
    async fn test_grep_tool_directory_recursive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
        tokio::fs::write(mount_root.join("src/a.rs"), "fn main() {}")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/b.rs"), "fn helper() {}")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.txt"), "fn root() {}")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "fn ", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_grep_tool_no_matches() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("file.txt"), "content here")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "nomatch", "path": "file.txt"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["matches"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_grep_tool_invalid_regex() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "[invalid", "path": "."});
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid regex"));
    }

    #[tokio::test]
    async fn test_grep_tool_skips_hidden_dirs() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join(".hidden"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join(".hidden/secret.txt"), "secret content")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("visible.txt"), "visible content")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "content", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(
            result["matches"][0]["path"]
                .as_str()
                .unwrap()
                .contains("visible.txt")
        );
    }

    #[tokio::test]
    async fn test_grep_tool_skips_binary_files() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create a binary file with some pattern in it
        let binary_content = vec![0x00, 0x01, 0x02, 0x03];
        tokio::fs::write(mount_root.join("data.bin"), binary_content)
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("text.txt"), "test data")
            .await
            .unwrap();

        let tool = GrepTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "data", "path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(
            result["matches"][0]["path"]
                .as_str()
                .unwrap()
                .contains("text.txt")
        );
    }

    #[tokio::test]
    async fn test_glob_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("a.rs"), "").await.unwrap();
        tokio::fs::write(mount_root.join("b.rs"), "").await.unwrap();
        tokio::fs::write(mount_root.join("c.txt"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.rs"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_glob_tool_recursive() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
        tokio::fs::create_dir(mount_root.join("src/nested"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/a.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("src/nested/b.rs"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.rs"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "**/*.rs"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 3);
    }

    #[tokio::test]
    async fn test_glob_tool_with_path() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::create_dir(mount_root.join("subdir"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("subdir/file.txt"), "")
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("root.txt"), "")
            .await
            .unwrap();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.txt", "path": "subdir"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
        assert!(result["matches"][0].as_str().unwrap().contains("subdir"));
    }

    #[tokio::test]
    async fn test_glob_tool_no_matches() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = GlobTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"pattern": "*.nonexistent"});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["matches"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_dir_tool() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create some files
        tokio::fs::write(mount_root.join("file1.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("dir1"))
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 2);
    }

    #[tokio::test]
    async fn test_list_dir_default_path() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        tokio::fs::write(mount_root.join("file.txt"), "")
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        // No path argument, should use cwd
        let args = json!({});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 1);
    }

    #[tokio::test]
    async fn test_list_dir_empty() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["entries"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_dir_sorting() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        // Create files and dirs in non-sorted order
        tokio::fs::write(mount_root.join("z.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("a_dir"))
            .await
            .unwrap();
        tokio::fs::write(mount_root.join("m.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(mount_root.join("z_dir"))
            .await
            .unwrap();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "."});
        let result = tool.execute(args, &ctx).await.unwrap();

        let entries = result["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 4);
        // Directories first, sorted alphabetically
        assert_eq!(entries[0]["name"], "a_dir");
        assert_eq!(entries[0]["type"], "directory");
        assert_eq!(entries[1]["name"], "z_dir");
        assert_eq!(entries[1]["type"], "directory");
        // Then files
        assert_eq!(entries[2]["name"], "m.txt");
        assert_eq!(entries[2]["type"], "file");
        assert_eq!(entries[3]["name"], "z.txt");
        assert_eq!(entries[3]["type"], "file");
    }

    #[tokio::test]
    async fn test_list_dir_not_found() {
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let tool = ListDirTool::new();
        let config = Arc::new(Config::default());
        let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

        let args = json!({"path": "nonexistent"});
        let result = tool.execute(args, &ctx).await;

        assert!(result.is_err());
    }

    // Helper function tests
    #[test]
    fn test_is_binary_file() {
        assert!(is_binary_file(Path::new("test.exe")));
        assert!(is_binary_file(Path::new("test.dll")));
        assert!(is_binary_file(Path::new("test.so")));
        assert!(is_binary_file(Path::new("test.dylib")));
        assert!(is_binary_file(Path::new("test.bin")));
        assert!(is_binary_file(Path::new("test.o")));
        assert!(is_binary_file(Path::new("test.a")));
        assert!(is_binary_file(Path::new("test.zip")));
        assert!(is_binary_file(Path::new("test.tar")));
        assert!(is_binary_file(Path::new("test.gz")));
        assert!(is_binary_file(Path::new("test.png")));
        assert!(is_binary_file(Path::new("test.pdf")));
        assert!(!is_binary_file(Path::new("test.txt")));
        assert!(!is_binary_file(Path::new("test.rs")));
        assert!(!is_binary_file(Path::new("test")));
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
    fn test_classify_bash_command_priority_network_over_write() {
        let cap = classify_bash_command("mkdir out && curl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_path_qualified_network_tool() {
        // Path-qualified executables classify by basename so an approved network
        // call isn't run with the sandbox network deny still in force.
        assert_eq!(
            classify_bash_command("/usr/bin/curl example.com"),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(
            classify_bash_command("/usr/bin/wget https://example.com/x"),
            alan_agent_protocol::ToolCapability::Network
        );
        // Path-qualified write tools likewise classify by basename.
        assert_eq!(
            classify_bash_command("/bin/rm file.txt"),
            alan_agent_protocol::ToolCapability::Write
        );
        // Path-qualified git subcommands classify via the basename gate too.
        assert_eq!(
            classify_bash_command("/usr/bin/git -C repo push"),
            alan_agent_protocol::ToolCapability::Network
        );
        assert_eq!(
            classify_bash_command("/usr/bin/git fetch origin"),
            alan_agent_protocol::ToolCapability::Network
        );
    }

    #[test]
    fn test_classify_bash_command_write() {
        let cap = classify_bash_command("git reset --hard");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_read() {
        let cap = classify_bash_command("rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_regex_pipe_inside_quotes_as_read() {
        let cap = classify_bash_command(
            "rg -n \"resolve_redirects|303|307|308|redirect\" requests tests",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_read_as_read() {
        let cap = classify_bash_command("cd /tmp/repo && ls");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_network_as_network() {
        let cap = classify_bash_command("cd /tmp/repo && curl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_treats_cd_then_write_as_write() {
        let cap = classify_bash_command("cd /tmp/repo && rm -f artifact.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_defaults_ambiguous_python_eval_to_unknown() {
        let cap = classify_bash_command("python -c \"print('hi')\"");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_python_script_file_as_unknown() {
        let cap = classify_bash_command("python3 script.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_script_file_as_unknown() {
        let cap = classify_bash_command("bash script.sh");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_awk_script_file_as_unknown() {
        let cap = classify_bash_command("awk -f script.awk input.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_eval_wrappers_as_unknown() {
        let cap = classify_bash_command("bash -lc \"rg TODO src\"");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_eval_wrappers_with_leading_options_as_unknown() {
        let cap = classify_bash_command("bash --noprofile -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_python_eval_wrappers_with_leading_options_as_unknown() {
        let cap = classify_bash_command("python3 -B -c 'print(\"hi\")'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_node_inline_long_eval_wrapper_as_unknown() {
        let cap =
            classify_bash_command("node --eval='require(\"fs\").writeFileSync(\"x\", \"y\")'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_shell_inline_long_command_wrapper_as_unknown() {
        let cap = classify_bash_command("sh --command='rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_eval_wrapper_with_line_continuation_as_unknown() {
        let cap = classify_bash_command("s\\\nh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_node_print_eval_wrappers_as_unknown() {
        let cap = classify_bash_command(
            "node --trace-warnings -p 'require(\"fs\").writeFileSync(\"x\", \"y\")'",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_allows_literal_sh_dash_c_arguments() {
        let cap = classify_bash_command("printf '%s %s' sh -c");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_treats_multiline_eval_wrapper_as_unknown() {
        let cap = classify_bash_command("echo ok\nsh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_post_comment_line_continuation_network_as_network() {
        let cap = classify_bash_command("echo ok #\\\ncurl https://example.com");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_treats_env_shell_eval_wrappers_as_unknown() {
        let cap = classify_bash_command("env FOO=bar sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_bang_prefixed_shell_eval_as_unknown() {
        let cap = classify_bash_command("! sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_then_prefixed_shell_eval_as_unknown() {
        let cap = classify_bash_command("if true; then sh -c 'rg TODO src'; fi");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_command_wrapper_shell_eval_as_unknown() {
        let cap = classify_bash_command("command -p sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_nice_wrapper_as_unknown() {
        let cap = classify_bash_command("nice -n 5 sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_time_wrapper_as_unknown() {
        let cap = classify_bash_command("time sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_command_query_mode_as_unknown() {
        let cap = classify_bash_command("command -v sh -c");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_timeout_query_mode_as_unknown() {
        let cap = classify_bash_command("timeout --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_timeout_query_with_line_continuation_as_unknown() {
        let cap = classify_bash_command("time\\\nout --ver\\\nsion");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_builtin_query_mode_as_unknown() {
        let cap = classify_bash_command("builtin -p eval");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_exec_wrapper_shell_eval_with_argv0_as_unknown() {
        let cap = classify_bash_command("exec -a alan sh -c 'rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_stdbuf_wrapped_read_command_as_unknown() {
        let cap = classify_bash_command("stdbuf -oL rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_env_split_string_as_unknown() {
        let cap = classify_bash_command("env -S 'sh -c rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_clustered_env_split_string_as_unknown() {
        let cap = classify_bash_command("env -iS 'sh -c rg TODO src'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_treats_direct_command_with_leading_env_assignment_as_read() {
        let cap = classify_bash_command("ALAN_TEST=1 rg TODO src");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_redirection_without_whitespace_is_write() {
        let cap = classify_bash_command("echo x>.git/config");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_fetch_is_network() {
        let cap = classify_bash_command("git fetch origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_fetch_with_global_options_is_network() {
        let cap = classify_bash_command("git -C /tmp/repo fetch --depth=1 origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_rev_parse_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo rev-parse --verify --quiet head");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_status_is_read() {
        let cap = classify_bash_command("git submodule status");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_init_is_write() {
        let cap = classify_bash_command("git submodule init");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_update_is_network() {
        let cap = classify_bash_command("git submodule update");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_submodule_update_no_fetch_is_write() {
        let cap = classify_bash_command("git submodule update --no-fetch");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_mutations_are_write() {
        let cap = classify_bash_command("git add .");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_branch_creation_is_write() {
        let cap = classify_bash_command("git branch release");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_branch_list_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo branch --list");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_branch_edit_description_is_write() {
        let cap = classify_bash_command("git branch --edit-description");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_tag_creation_is_write() {
        let cap = classify_bash_command("git tag v1.2.3");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_tag_list_with_global_options_is_read() {
        let cap = classify_bash_command("git -C /tmp/repo tag --list");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_is_write() {
        let cap =
            classify_bash_command("git remote add origin git@github.com:realmorrisliu/Alan.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_fetch_is_network() {
        let cap = classify_bash_command("git remote add -f origin https://example.com/repo.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_remote_add_long_fetch_is_network() {
        let cap =
            classify_bash_command("git remote add --fetch origin https://example.com/repo.git");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_ls_remote_is_network() {
        let cap = classify_bash_command("git ls-remote origin");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_git_push_is_network() {
        let cap = classify_bash_command("git push origin main");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
    }

    #[test]
    fn test_classify_bash_command_sed_in_place_is_write() {
        let cap = classify_bash_command("sed -i 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_sed_clustered_ei_is_write() {
        let cap = classify_bash_command("sed -Ei 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_sed_clustered_ni_is_write() {
        let cap = classify_bash_command("sed -ni 's/foo/bar/' src/lib.rs");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_exec_is_write() {
        let cap = classify_bash_command("find . -name '*.tmp' -exec rm -f {} \\;");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_fprint_is_write() {
        let cap = classify_bash_command("find . -name '*.rs' -fprint /tmp/files.txt");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_fprint0_is_write() {
        let cap = classify_bash_command("find . -name '*.rs' -fprint0 /tmp/files.bin");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_find_name_defaults_to_read() {
        let cap = classify_bash_command("find . -name '*.rs'");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_find_pipeline_is_read() {
        let cap = classify_bash_command(
            "find . -maxdepth 3 \\( -path './test*' -o -path './tests*' \\) -type d | sort",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_pytest_is_write() {
        let cap = classify_bash_command("pytest tests/test_requests.py -k redirect");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_module_pytest_is_write() {
        let cap = classify_bash_command("python -B -m pytest tests/test_requests.py -k redirect");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_local_runtests_script_is_write() {
        let cap = classify_bash_command("./tests/runtests.py utils_tests.test_html");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_local_runtests_script_is_write() {
        let cap = classify_bash_command("python3 -B tests/runtests.py utils_tests.test_html");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_manage_py_test_is_write() {
        let cap = classify_bash_command("python manage.py test auth_tests");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_manage_py_shell_stays_unknown() {
        let cap = classify_bash_command("python manage.py shell");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_local_gradle_test_wrapper_is_write() {
        let cap = classify_bash_command("./gradlew test");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_tox_version_is_read() {
        let cap = classify_bash_command("tox --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_nox_help_is_read() {
        let cap = classify_bash_command("nox --help");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_tox_run_is_write() {
        let cap = classify_bash_command("tox -e py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_nox_run_is_write() {
        let cap = classify_bash_command("nox -s tests");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_classify_bash_command_python_version_is_read() {
        let cap = classify_bash_command("python --version");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_print_is_read() {
        let cap = classify_bash_command("sed -n '1,80p' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_substitute_is_read() {
        let cap = classify_bash_command("sed 's#^./##' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_read_only_find_sed_pipeline_is_read() {
        let cap = classify_bash_command(
            "find . -maxdepth 2 -type f | sed 's#^./##' | sort | rg \"(^test|tests|requests/test)\"",
        );
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
    }

    #[test]
    fn test_classify_bash_command_sed_write_script_is_unknown() {
        let cap = classify_bash_command("sed -n '1,80w /tmp/out' test_requests.py");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
    }

    #[test]
    fn test_classify_bash_command_cargo_test_is_write() {
        let cap = classify_bash_command("cargo test -p alan-agent-engine delegated_skill --lib");
        assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
    }

    #[test]
    fn test_grep_tool_metadata() {
        let tool = GrepTool::new();
        assert_eq!(tool.name(), "grep");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_glob_tool_metadata() {
        let tool = GlobTool::new();
        assert_eq!(tool.name(), "glob");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
    }

    #[test]
    fn test_list_dir_tool_metadata() {
        let tool = ListDirTool::new();
        assert_eq!(tool.name(), "list_dir");
        assert_eq!(
            tool.capability(&json!({})),
            alan_agent_protocol::ToolCapability::Read
        );
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
