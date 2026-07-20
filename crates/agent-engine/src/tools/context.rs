//! Tool execution context for dependency injection.

use super::sandbox::Sandbox;
use crate::config::Config;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Host-owned native projection used by builtin Tool implementations.
///
/// Agent Execution Engine retains this object only as an opaque adapter. Raw
/// Host backing paths and native sandbox roots stay inside the implementation
/// supplied by the Host adapter.
pub trait ToolExecutionAdapter: std::fmt::Debug + Send + Sync {
    /// Agent-visible cwd selected for this native Tool projection.
    fn namespace_cwd(&self) -> PathBuf;

    /// Resolve the native cwd selected for this Tool Process.
    fn cwd(&self) -> Result<PathBuf>;

    /// Resolve an Agent-visible absolute path or cwd-relative path.
    fn resolve_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf>;

    /// Translate one native adapter path back into the Process namespace.
    fn visible_path(&self, host_path: &Path) -> PathBuf;

    /// Redact native backing paths from text crossing back into Alan OS.
    fn project_text(&self, text: &str) -> String;

    /// Return the Host-derived native sandbox for this Tool Process.
    fn sandbox(&self) -> Result<Sandbox>;
}

/// Explicit Process binding for Tool execution.
#[derive(Clone)]
pub struct ToolExecutionBinding {
    /// Agent-visible Alan OS working directory.
    pub namespace_cwd: PathBuf,
    /// Scratch directory for temporary files.
    pub scratch_dir: PathBuf,
    adapter: Option<Arc<dyn ToolExecutionAdapter>>,
}

impl std::fmt::Debug for ToolExecutionBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ToolExecutionBinding")
            .field("namespace_cwd", &self.namespace_cwd)
            .field("scratch_dir", &self.scratch_dir)
            .field("has_adapter", &self.adapter.is_some())
            .finish()
    }
}

/// Late-bound authority check applied immediately before a Tool Process starts.
///
/// Long-lived Process bindings can outlive a native grant. Hosts use this hook
/// to reconcile the cached path projection against the current service-owned
/// grant set, so revocation cannot leave authority in a future Tool Process.
pub trait ToolExecutionAuthority: std::fmt::Debug + Send + Sync {
    fn reconcile(&self, pid: u64, binding: ToolExecutionBinding) -> Result<ToolExecutionBinding>;
}

impl ToolExecutionBinding {
    /// Create a binding whose native Host projection will be supplied by the Host adapter.
    pub fn awaiting_host_projection(namespace_cwd: PathBuf, scratch_dir: PathBuf) -> Self {
        Self {
            namespace_cwd,
            scratch_dir,
            adapter: None,
        }
    }

    /// Bind the opaque Host adapter selected for the current service grant set.
    pub fn with_adapter(mut self, adapter: Arc<dyn ToolExecutionAdapter>) -> Self {
        self.namespace_cwd = adapter.namespace_cwd();
        self.adapter = Some(adapter);
        self
    }

    /// Replace the opaque Host adapter after live grant reconciliation.
    pub fn set_adapter(&mut self, adapter: Arc<dyn ToolExecutionAdapter>) {
        self.namespace_cwd = adapter.namespace_cwd();
        self.adapter = Some(adapter);
    }

    /// Remove native Host authority after revocation or empty delegation.
    pub fn clear_adapter(&mut self) {
        self.adapter = None;
    }

    pub fn has_adapter(&self) -> bool {
        self.adapter.is_some()
    }

    pub fn adapter(&self) -> Option<Arc<dyn ToolExecutionAdapter>> {
        self.adapter.clone()
    }
}

/// Context provided to tools during execution.
/// Contains all dependencies needed by tools.
pub struct ToolContext {
    /// Agent-visible Alan OS working directory.
    pub namespace_cwd: PathBuf,
    /// Scratch directory for temporary files
    pub scratch_dir: PathBuf,
    /// Global configuration
    pub config: Arc<Config>,
    adapter: Option<Arc<dyn ToolExecutionAdapter>>,
}

impl ToolContext {
    /// Create a tool context from an explicit execution binding.
    pub fn from_binding(binding: ToolExecutionBinding, config: Arc<Config>) -> Self {
        Self {
            namespace_cwd: binding.namespace_cwd,
            scratch_dir: binding.scratch_dir,
            config,
            adapter: binding.adapter,
        }
    }

    /// Return the current execution binding.
    pub fn binding(&self) -> ToolExecutionBinding {
        ToolExecutionBinding {
            namespace_cwd: self.namespace_cwd.clone(),
            scratch_dir: self.scratch_dir.clone(),
            adapter: self.adapter.clone(),
        }
    }

    /// Create a sandbox from the Host-projected authority for this Process.
    pub fn sandbox(&self) -> Result<Sandbox> {
        self.execution_adapter()?.sandbox()
    }

    /// Return the Host-selected native cwd without retaining it in engine state.
    pub fn cwd(&self) -> Result<PathBuf> {
        self.execution_adapter()?.cwd()
    }

    pub fn execution_adapter(&self) -> Result<Arc<dyn ToolExecutionAdapter>> {
        self.adapter
            .clone()
            .context("Tool Process has no explicit Host execution adapter")
    }

    /// Resolve a path relative to working directory
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        self.execution_adapter()?
            .resolve_path(&self.namespace_cwd, path.as_ref())
    }

    /// Translate an internal Host adapter path back to an Agent-visible path.
    pub fn visible_path(&self, host_path: &Path) -> PathBuf {
        self.adapter.as_ref().map_or_else(
            || PathBuf::from("<unmapped-host-path>"),
            |adapter| adapter.visible_path(host_path),
        )
    }

    /// Redact Host backing roots from native subprocess output.
    pub fn project_text(&self, text: &str) -> String {
        self.adapter
            .as_ref()
            .map_or_else(|| text.to_string(), |adapter| adapter.project_text(text))
    }

    /// Project every string in a Tool result before it crosses back into AgentFS.
    pub fn project_value(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = self.project_text(text),
            serde_json::Value::Array(values) => {
                for value in values {
                    self.project_value(value);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    self.project_value(value);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct TestToolExecutionAdapter {
    namespace_root: PathBuf,
    host_root: PathBuf,
    sandbox: Sandbox,
}

#[cfg(test)]
impl TestToolExecutionAdapter {
    fn read_write(namespace_root: impl Into<PathBuf>, host_root: PathBuf) -> Self {
        let host_root = dunce::canonicalize(&host_root)
            .unwrap_or_else(|_| dunce::simplified(&host_root).to_path_buf());
        Self {
            namespace_root: namespace_root.into(),
            sandbox: Sandbox::from_spec(crate::tools::SandboxSpec::seed(host_root.clone())),
            host_root,
        }
    }

    fn host_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf> {
        let visible = if path.is_absolute() {
            path.to_path_buf()
        } else {
            namespace_cwd.join(path)
        };
        let suffix = visible
            .strip_prefix(&self.namespace_root)
            .with_context(|| format!("path {} is outside the test namespace", visible.display()))?;
        Ok(self.host_root.join(suffix))
    }
}

#[cfg(test)]
impl ToolExecutionAdapter for TestToolExecutionAdapter {
    fn namespace_cwd(&self) -> PathBuf {
        self.namespace_root.clone()
    }

    fn cwd(&self) -> Result<PathBuf> {
        Ok(self.host_root.clone())
    }

    fn resolve_path(&self, namespace_cwd: &Path, path: &Path) -> Result<PathBuf> {
        self.host_path(namespace_cwd, path)
    }

    fn visible_path(&self, host_path: &Path) -> PathBuf {
        host_path.strip_prefix(&self.host_root).map_or_else(
            |_| PathBuf::from("<unmapped-host-path>"),
            |suffix| self.namespace_root.join(suffix),
        )
    }

    fn project_text(&self, text: &str) -> String {
        text.replace(
            self.host_root.to_string_lossy().as_ref(),
            self.namespace_root.to_string_lossy().as_ref(),
        )
    }

    fn sandbox(&self) -> Result<Sandbox> {
        Ok(self.sandbox.clone())
    }
}

#[cfg(test)]
pub(crate) fn test_execution_binding(
    namespace_root: impl Into<PathBuf>,
    host_root: PathBuf,
    scratch_dir: PathBuf,
) -> ToolExecutionBinding {
    let namespace_root = namespace_root.into();
    ToolExecutionBinding::awaiting_host_projection(namespace_root.clone(), scratch_dir)
        .with_adapter(Arc::new(TestToolExecutionAdapter::read_write(
            namespace_root,
            host_root,
        )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(namespace_root: &str, host_root: PathBuf) -> ToolContext {
        let binding = test_execution_binding(
            namespace_root,
            host_root.clone(),
            host_root.join(".scratch"),
        );
        ToolContext::from_binding(binding, Arc::new(Config::default()))
    }

    #[test]
    fn test_tool_context_resolve_path() {
        let source = tempfile::tempdir().unwrap();
        let ctx = context("/mnt/source", source.path().to_path_buf());

        assert_eq!(
            ctx.resolve_path("file.txt").unwrap(),
            dunce::canonicalize(source.path()).unwrap().join("file.txt")
        );
        assert_eq!(
            ctx.resolve_path("/mnt/source/absolute/file.txt").unwrap(),
            dunce::canonicalize(source.path())
                .unwrap()
                .join("absolute/file.txt")
        );
        assert!(ctx.resolve_path("/absolute/file.txt").is_err());
    }

    #[test]
    fn test_tool_context_exposes_process_binding() {
        let source = tempfile::tempdir().unwrap();
        let ctx = context("/mnt/source", source.path().to_path_buf());
        let binding = ctx.binding();

        assert_eq!(binding.namespace_cwd, Path::new("/mnt/source"));
        assert!(binding.has_adapter());
    }

    #[test]
    fn test_tool_context_uses_adapter_sandbox() {
        let source = tempfile::tempdir().unwrap();
        let ctx = context("/mnt/source", source.path().to_path_buf());
        let sandbox = ctx.sandbox().unwrap();
        assert!(sandbox.is_writable(&source.path().join("file.txt")));
    }

    #[test]
    fn test_tool_context_projects_host_backing_paths_to_namespace_paths() {
        let source = tempfile::tempdir().unwrap();
        let ctx = context("/mnt/source", source.path().to_path_buf());
        let host_path = dunce::canonicalize(source.path())
            .unwrap()
            .join("private.txt");

        let projected = ctx.project_text(&format!("failed to read {}", host_path.display()));

        assert_eq!(projected, "failed to read /mnt/source/private.txt");
        assert!(!projected.contains(host_path.parent().unwrap().to_string_lossy().as_ref()));
    }

    #[test]
    fn an_unresolved_binding_has_no_native_tool_authority() {
        let binding = ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/lib/pkg/example"),
            PathBuf::from("/tmp/scratch"),
        );
        let ctx = ToolContext::from_binding(binding, Arc::new(Config::default()));

        assert!(ctx.sandbox().is_err());
        assert!(ctx.cwd().is_err());
    }

    #[tokio::test]
    async fn adapter_supplies_the_native_cwd_and_projects_it_back() {
        let source = tempfile::tempdir().unwrap();
        let ctx = context("/mnt/project", source.path().to_path_buf());
        let cwd = ctx.cwd().unwrap();
        let execution = ctx.sandbox().unwrap().exec("pwd", &cwd).await.unwrap();
        assert_eq!(execution.exit_code, 0, "{execution:?}");
        assert_eq!(ctx.project_text(execution.stdout.trim()), "/mnt/project");
    }

    #[test]
    fn replacing_an_adapter_does_not_change_logical_process_state() {
        let project = tempfile::tempdir().unwrap();
        let replacement = tempfile::tempdir().unwrap();
        let mut binding = ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/project"),
            PathBuf::from("/tmp/scratch"),
        );
        binding.set_adapter(Arc::new(TestToolExecutionAdapter::read_write(
            "/mnt/project",
            project.path().to_path_buf(),
        )));
        binding.set_adapter(Arc::new(TestToolExecutionAdapter::read_write(
            "/mnt/project",
            replacement.path().to_path_buf(),
        )));

        assert_eq!(binding.namespace_cwd, Path::new("/mnt/project"));
        assert!(binding.has_adapter());
    }
}
