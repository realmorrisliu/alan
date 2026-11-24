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
mod execution_adapter;
mod exploration_gitignore;
mod exploration_tools;
mod file_tools;
mod namespace_fs;
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
        "Execute a shell command in the Process cwd inside the active OS sandbox and trust posture (falls back to bash). Shell features such as variables, pipes, subshells, globs, and inline interpreters work inside the sandboxed cwd. Prefer dedicated tools (`read_file`, `edit_file`, `write_file`, `grep`, `glob`, `list_dir`) for inspecting, searching, or editing files; use `bash` for builds, tests, git, package managers, and commands without a dedicated tool. Use cwd-relative paths for mounted project files; absolute alan9 paths such as /mnt/project/... inside command strings are not automatically translated."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command string to execute from the Process cwd."
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
        let sandbox = match ctx.shell_sandbox() {
            Ok(sandbox) => sandbox,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let cwd = match ctx.cwd() {
            Ok(cwd) => cwd,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let adapter = match ctx.execution_adapter() {
            Ok(adapter) => adapter,
            Err(err) => return Box::pin(async move { Err(err) }),
        };
        let command = args["command"].as_str().unwrap_or("").to_string();
        let capability = classify_bash_command(&command);
        let timeout_secs = args["timeout"].as_u64().unwrap_or(60).clamp(1, 300);
        let progress = ctx.progress_callback();

        Box::pin(async move {
            let adapter_for_progress = adapter.clone();
            let projected_progress = progress.map(|cb| {
                let wrapped: alan_agent_engine::tools::OutputProgressCallback = std::sync::Arc::new(
                    move |stdout_tail: &str, stderr_tail: &str, truncated: bool| {
                        let out = adapter_for_progress.project_text(stdout_tail);
                        let err = adapter_for_progress.project_text(stderr_tail);
                        cb(&out, &err, truncated);
                    },
                );
                wrapped
            });
            let result = sandbox
                .exec_with_timeout_capability_and_progress(
                    &command,
                    &cwd,
                    Some(std::time::Duration::from_secs(timeout_secs)),
                    Some(capability),
                    projected_progress,
                )
                .await?;

            let mut projected_stderr = adapter.project_text(&result.stderr);
            if result.exit_code != 0
                && let Some(hint) = sandbox.sensitive_read_deny_hint(&result.stderr)
            {
                if projected_stderr.is_empty() {
                    projected_stderr = hint;
                } else {
                    if !projected_stderr.ends_with('\n') {
                        projected_stderr.push('\n');
                    }
                    projected_stderr.push_str(&hint);
                }
            }

            Ok(json!({
                "stdout": adapter.project_text(&result.stdout),
                "stderr": projected_stderr,
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
    registry.set_default_execution_binding(execution_adapter::standalone_binding(
        host_root.clone(),
        host_root.join(".alan-runtime-tmp"),
    ));

    for tool in create_core_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with the 4 read-only tools pre-registered.
pub fn create_tool_registry_with_read_only_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(execution_adapter::standalone_binding(
        host_root.clone(),
        host_root.join(".alan-runtime-tmp"),
    ));

    for tool in create_read_only_tools() {
        registry.register_boxed(tool);
    }

    registry
}

/// Create a ToolRegistry with all 7 built-in tools pre-registered.
pub fn create_tool_registry_with_all_tools(host_root: std::path::PathBuf) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_builtin_tool_catalog(&mut registry);
    registry.set_default_execution_binding(execution_adapter::standalone_binding(
        host_root.clone(),
        host_root.join(".alan-runtime-tmp"),
    ));

    for tool in create_all_tools() {
        registry.register_boxed(tool);
    }

    registry
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_known_native_reads_opt_into_group_presentation() {
        use super::*;
        let tools: Vec<(Box<dyn Tool>, bool)> = vec![
            (Box::new(ReadFileTool), true),
            (Box::new(GrepTool), true),
            (Box::new(GlobTool), true),
            (Box::new(ListDirTool), true),
            (Box::new(WriteFileTool), false),
            (Box::new(EditFileTool), false),
            (Box::new(BashTool), false),
        ];
        for (tool, eligible) in tools {
            assert_eq!(
                tool.presentation_is_read_only(),
                eligible,
                "{}",
                tool.name()
            );
        }
    }
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
        let binding = execution_adapter::standalone_binding_at(
            mount_root.clone(),
            std::path::PathBuf::from("/mnt/source/subdir"),
            mount_root.join("tmp"),
        );
        let ctx = ToolContext::from_binding(binding, config);

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
    fn test_bash_tool_description_guides_dedicated_tools_and_sandbox() {
        let tool = BashTool::new();
        let description = tool.description();
        assert!(description.contains("read_file"));
        assert!(description.contains("edit_file"));
        assert!(description.contains("sandbox"));
        assert!(!description.contains("is unsupported"));
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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_bash_tool_appends_non_leaking_sensitive_read_deny_hint_before_projection() {
        use alan_agent_engine::tools::{
            Sandbox, SandboxSpec, ToolExecutionAdapter, ToolExecutionBinding,
        };
        use std::os::unix::fs::PermissionsExt;
        use std::path::Path;

        #[derive(Debug)]
        struct HomeMaskingAdapter {
            host_root: PathBuf,
            home_dir: PathBuf,
        }

        impl ToolExecutionAdapter for HomeMaskingAdapter {
            fn namespace_cwd(&self) -> PathBuf {
                PathBuf::from("/mnt/source")
            }

            fn cwd(&self) -> anyhow::Result<PathBuf> {
                Ok(self.host_root.clone())
            }

            fn resolve_path(&self, _namespace_cwd: &Path, path: &Path) -> anyhow::Result<PathBuf> {
                Ok(self.host_root.join(path))
            }

            fn visible_path(&self, _host_path: &Path) -> PathBuf {
                PathBuf::from("/mnt/source")
            }

            fn project_text(&self, text: &str) -> String {
                text.replace(
                    self.home_dir.to_string_lossy().as_ref(),
                    "<unmapped-host-path>",
                )
            }

            fn sandbox(&self) -> anyhow::Result<Sandbox> {
                Ok(Sandbox::from_spec(SandboxSpec::seed(
                    self.host_root.clone(),
                )))
            }
        }

        let Some(home_dir) = std::env::var_os("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
        else {
            return;
        };
        let temp = TempDir::new().unwrap();
        let mount_root = temp.path().to_path_buf();

        let deny_script = mount_root.join("emit_deny.sh");
        std::fs::write(
            &deny_script,
            "#!/bin/sh\nprintf 'open %s/.config/gh/hosts.yml: operation not permitted\\n' \"$HOME\" >&2\nexit 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&deny_script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let ok_script = mount_root.join("emit_ok.sh");
        std::fs::write(
            &ok_script,
            "#!/bin/sh\nprintf 'open %s/.config/gh/hosts.yml: operation not permitted\\n' \"$HOME\" >&2\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&ok_script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let binding = ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/source"),
            mount_root.join("tmp"),
        )
        .with_adapter(Arc::new(HomeMaskingAdapter {
            host_root: mount_root,
            home_dir: home_dir.clone(),
        }));
        let ctx = ToolContext::from_binding(binding, Arc::new(Config::default()));
        let tool = BashTool::new();

        let failed = tool
            .execute(json!({"command": "./emit_deny.sh"}), &ctx)
            .await
            .unwrap();
        assert_eq!(failed["exit_code"], 1);
        let stderr = failed["stderr"].as_str().unwrap();
        assert!(
            stderr.contains("hint: OS sandbox blocks host credentials (~/.config/gh). Mount /mnt/github (--fileserver github-fileserver:ro:net), pass a scoped GH_TOKEN, or run with --unsafe-host-account."),
            "unexpected stderr: {stderr}"
        );
        assert!(
            !stderr.contains(home_dir.to_string_lossy().as_ref()),
            "raw home leaked in stderr: {stderr}"
        );

        let succeeded = tool
            .execute(json!({"command": "./emit_ok.sh"}), &ctx)
            .await
            .unwrap();
        assert_eq!(succeeded["exit_code"], 0);
        assert!(
            !succeeded["stderr"]
                .as_str()
                .unwrap()
                .contains("hint: OS sandbox blocks host credentials"),
            "exit 0 must not emit hint"
        );
    }
}
