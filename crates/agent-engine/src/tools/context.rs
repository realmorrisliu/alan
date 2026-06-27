//! Tool execution context for dependency injection.

use super::sandbox::Sandbox;
use crate::config::Config;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Explicit runtime-owned binding for tool execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolExecutionBinding {
    /// Bound workspace root for workspace-local tools.
    pub workspace_root: Option<PathBuf>,
    /// Working directory for relative path resolution and process execution.
    pub cwd: PathBuf,
    /// Scratch directory for temporary files.
    pub scratch_dir: PathBuf,
}

impl ToolExecutionBinding {
    /// Create a binding with an optional workspace root.
    pub fn new(workspace_root: Option<PathBuf>, cwd: PathBuf, scratch_dir: PathBuf) -> Self {
        Self {
            workspace_root,
            cwd,
            scratch_dir,
        }
    }

    /// Create a binding for tools that do not require a workspace root.
    pub fn without_workspace(cwd: PathBuf, scratch_dir: PathBuf) -> Self {
        Self::new(None, cwd, scratch_dir)
    }

    /// Create a binding for workspace-local tools.
    pub fn with_workspace(workspace_root: PathBuf, cwd: PathBuf, scratch_dir: PathBuf) -> Self {
        Self::new(Some(workspace_root), cwd, scratch_dir)
    }
}

/// Context provided to tools during execution.
/// Contains all dependencies needed by tools.
pub struct ToolContext {
    /// Bound workspace root for workspace-local tools.
    pub workspace_root: Option<PathBuf>,
    /// Working directory for the tool
    pub cwd: PathBuf,
    /// Scratch directory for temporary files
    pub scratch_dir: PathBuf,
    /// Global configuration
    pub config: Arc<Config>,
}

impl ToolContext {
    /// Create a new workspace-local tool context where `cwd == workspace_root`.
    pub fn new(cwd: PathBuf, scratch_dir: PathBuf, config: Arc<Config>) -> Self {
        Self::with_workspace(cwd.clone(), cwd, scratch_dir, config)
    }

    /// Create a new tool context without a workspace binding.
    pub fn without_workspace(cwd: PathBuf, scratch_dir: PathBuf, config: Arc<Config>) -> Self {
        Self::from_binding(
            ToolExecutionBinding::without_workspace(cwd, scratch_dir),
            config,
        )
    }

    /// Create a new tool context for a workspace-local execution site.
    pub fn with_workspace(
        workspace_root: PathBuf,
        cwd: PathBuf,
        scratch_dir: PathBuf,
        config: Arc<Config>,
    ) -> Self {
        Self::from_binding(
            ToolExecutionBinding::with_workspace(workspace_root, cwd, scratch_dir),
            config,
        )
    }

    /// Create a tool context from an explicit execution binding.
    pub fn from_binding(binding: ToolExecutionBinding, config: Arc<Config>) -> Self {
        Self {
            workspace_root: binding.workspace_root,
            cwd: binding.cwd,
            scratch_dir: binding.scratch_dir,
            config,
        }
    }

    /// Return the current execution binding.
    pub fn binding(&self) -> ToolExecutionBinding {
        ToolExecutionBinding {
            workspace_root: self.workspace_root.clone(),
            cwd: self.cwd.clone(),
            scratch_dir: self.scratch_dir.clone(),
        }
    }

    /// Return the bound workspace root if present.
    pub fn workspace_root(&self) -> Option<&Path> {
        self.workspace_root.as_deref()
    }

    /// Require an explicit workspace root for workspace-local tools.
    pub fn require_workspace_root(&self) -> Result<&Path> {
        self.workspace_root()
            .ok_or_else(|| anyhow!("Workspace-local tool requires explicit workspace binding"))
    }

    /// Create a sandbox bound to the current workspace root.
    pub fn workspace_sandbox(&self) -> Result<Sandbox> {
        Ok(Sandbox::new(self.require_workspace_root()?.to_path_buf()))
    }

    /// Resolve a path relative to working directory
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> PathBuf {
        if path.as_ref().is_absolute() {
            path.as_ref().to_path_buf()
        } else {
            self.cwd.join(path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_context_resolve_path() {
        let config = Arc::new(Config::default());
        let ctx = ToolContext::with_workspace(
            PathBuf::from("/workspace"),
            PathBuf::from("/workspace"),
            PathBuf::from("/tmp/scratch"),
            config,
        );

        // Relative path
        assert_eq!(
            ctx.resolve_path("file.txt"),
            PathBuf::from("/workspace/file.txt")
        );

        // Absolute path
        assert_eq!(
            ctx.resolve_path("/absolute/file.txt"),
            PathBuf::from("/absolute/file.txt")
        );
    }

    #[test]
    fn test_tool_context_exposes_workspace_binding() {
        let config = Arc::new(Config::default());
        let ctx = ToolContext::with_workspace(
            PathBuf::from("/workspace"),
            PathBuf::from("/workspace/src"),
            PathBuf::from("/tmp/scratch"),
            config,
        );

        assert_eq!(ctx.workspace_root(), Some(Path::new("/workspace")));
        assert_eq!(
            ctx.binding(),
            ToolExecutionBinding::with_workspace(
                PathBuf::from("/workspace"),
                PathBuf::from("/workspace/src"),
                PathBuf::from("/tmp/scratch")
            )
        );
    }
}
