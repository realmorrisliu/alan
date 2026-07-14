//! Tool execution context for dependency injection.

use super::sandbox::{Sandbox, SandboxSpec};
use crate::config::Config;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Explicit runtime-owned binding for tool execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolExecutionBinding {
    /// Host working directory used only by the native Tool adapter.
    pub cwd: PathBuf,
    /// Agent-visible Alan OS working directory.
    pub namespace_cwd: PathBuf,
    /// Exact Host Mount projections inherited by this Tool Process.
    pub host_mounts: Vec<crate::HostMountGrant>,
    /// Scratch directory for temporary files.
    pub scratch_dir: PathBuf,
    /// Runtime-projected sandbox authority for this execution binding.
    pub sandbox_spec: Option<SandboxSpec>,
}

/// Late-bound authority check applied immediately before a Tool Process starts.
///
/// Long-lived Process bindings can outlive a native grant. Hosts use this hook
/// to reconcile the cached path projection against the current service-owned
/// grant set, so revocation cannot leave authority in a future Tool Process.
pub trait ToolExecutionAuthority: std::fmt::Debug + Send + Sync {
    fn reconcile(
        &self,
        pid: alan_kernel::Pid,
        binding: ToolExecutionBinding,
    ) -> Result<ToolExecutionBinding>;
}

impl ToolExecutionBinding {
    /// Create a binding from Process cwd and explicitly owned scratch storage.
    pub fn new(cwd: PathBuf, scratch_dir: PathBuf) -> Self {
        Self {
            namespace_cwd: cwd.clone(),
            cwd,
            host_mounts: Vec::new(),
            scratch_dir,
            sandbox_spec: None,
        }
    }

    /// Create a native adapter binding from the same Process Launch Context
    /// that owns namespace reachability and sandbox authority.
    pub fn from_launch_context(
        context: &crate::ProcessLaunchContext,
        scratch_dir: PathBuf,
    ) -> Result<Self> {
        let mut tool_context = context.clone();
        tool_context.host_mounts = context.tool_host_mounts().cloned().collect();
        // A namespace cwd such as `/` may have no Host backing even after the
        // Process receives an explicit mount. Native adapters still need an OS
        // cwd inside their sandbox authority. Prefer the first writable Host
        // Mount so Bash can start; a read-only mount remains a valid fallback
        // for read-only Tools. That selected mount is the Tool Process cwd;
        // runtime scratch is storage, not Host authority.
        let (cwd, namespace_cwd) = if let Some(cwd) = tool_context.host_cwd() {
            (cwd, PathBuf::from(&tool_context.cwd))
        } else {
            let grant = tool_context
                .host_mounts
                .iter()
                .find(|grant| grant.access == alan_kernel::Access::ReadWrite)
                .or_else(|| context.host_mounts.first())
                .context("Process has no explicit Host Mount for native Tool execution")?;
            (
                dunce::canonicalize(&grant.host_path)
                    .unwrap_or_else(|_| dunce::simplified(&grant.host_path).to_path_buf()),
                PathBuf::from(&grant.namespace_path),
            )
        };
        Ok(Self {
            cwd,
            namespace_cwd,
            host_mounts: tool_context.host_mounts.clone(),
            scratch_dir,
            sandbox_spec: Some(SandboxSpec::from_host_mounts(&tool_context.host_mounts)),
        })
    }

    /// Return a copy of this binding with an explicit runtime sandbox projection.
    pub fn with_sandbox_spec(mut self, sandbox_spec: SandboxSpec) -> Self {
        self.sandbox_spec = Some(sandbox_spec);
        self
    }

    /// Remove service-managed Host Mounts and rebuild native authority from the
    /// remaining explicit grants.
    pub fn remove_host_mount_paths(&mut self, namespace_paths: &[String]) -> Result<()> {
        self.host_mounts.retain(|grant| {
            !namespace_paths
                .iter()
                .any(|path| path == &grant.namespace_path)
        });
        self.rebuild_host_authority()
    }

    /// Add or replace one Host-adapter-produced mount and rebuild the native
    /// sandbox from the same explicit grant set.
    pub fn apply_host_mount(&mut self, grant: crate::HostMountGrant) -> Result<()> {
        if let Some(existing) = self
            .host_mounts
            .iter_mut()
            .find(|existing| existing.namespace_path == grant.namespace_path)
        {
            *existing = grant;
        } else {
            self.host_mounts.push(grant);
        }
        self.rebuild_host_authority()
    }

    fn rebuild_host_authority(&mut self) -> Result<()> {
        if self.host_mounts.is_empty() {
            self.sandbox_spec = None;
            return Ok(());
        }

        let namespace_cwd = self.namespace_cwd.to_string_lossy();
        let current = self.host_mounts.iter().find(|grant| {
            namespace_cwd == grant.namespace_path
                || namespace_cwd
                    .strip_prefix(&grant.namespace_path)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        });
        let selected = current
            .or_else(|| {
                self.host_mounts
                    .iter()
                    .find(|grant| grant.access == alan_kernel::Access::ReadWrite)
            })
            .or_else(|| self.host_mounts.first())
            .context("Tool Process has no active Host Mount")?;
        if current.is_none() {
            self.namespace_cwd = PathBuf::from(&selected.namespace_path);
        }
        let namespace_cwd = self
            .namespace_cwd
            .to_str()
            .context("Tool Process namespace cwd is not UTF-8")?;
        self.cwd = selected
            .resolve_host_path(namespace_cwd)
            .context("Tool Process cwd is outside its active Host Mount")?;
        self.sandbox_spec = Some(SandboxSpec::from_host_mounts(&self.host_mounts));
        Ok(())
    }
}

/// Context provided to tools during execution.
/// Contains all dependencies needed by tools.
pub struct ToolContext {
    /// Host working directory used only inside the native adapter.
    pub cwd: PathBuf,
    /// Agent-visible Alan OS working directory.
    pub namespace_cwd: PathBuf,
    /// Exact Host Mount projections for path translation.
    pub host_mounts: Vec<crate::HostMountGrant>,
    /// Scratch directory for temporary files
    pub scratch_dir: PathBuf,
    /// Runtime-projected sandbox authority for this context.
    pub sandbox_spec: Option<SandboxSpec>,
    /// Global configuration
    pub config: Arc<Config>,
}

impl ToolContext {
    /// Create a Tool context with no implicit filesystem authority.
    pub fn new(cwd: PathBuf, scratch_dir: PathBuf, config: Arc<Config>) -> Self {
        Self::from_binding(ToolExecutionBinding::new(cwd, scratch_dir), config)
    }

    /// Create a tool context from an explicit execution binding.
    pub fn from_binding(binding: ToolExecutionBinding, config: Arc<Config>) -> Self {
        Self {
            cwd: binding.cwd,
            namespace_cwd: binding.namespace_cwd,
            host_mounts: binding.host_mounts,
            scratch_dir: binding.scratch_dir,
            sandbox_spec: binding.sandbox_spec,
            config,
        }
    }

    /// Return the current execution binding.
    pub fn binding(&self) -> ToolExecutionBinding {
        ToolExecutionBinding {
            cwd: self.cwd.clone(),
            namespace_cwd: self.namespace_cwd.clone(),
            host_mounts: self.host_mounts.clone(),
            scratch_dir: self.scratch_dir.clone(),
            sandbox_spec: self.sandbox_spec.clone(),
        }
    }

    /// Create a sandbox from the Host-projected authority for this Process.
    pub fn sandbox(&self) -> Result<Sandbox> {
        let spec = self
            .sandbox_spec
            .clone()
            .context("Tool Process has no explicit sandbox grant")?;
        Ok(Sandbox::from_spec(spec))
    }

    /// Resolve a path relative to working directory
    pub fn resolve_path(&self, path: impl AsRef<Path>) -> Result<PathBuf> {
        if path.as_ref().is_absolute() {
            let namespace_path = path.as_ref().to_string_lossy();
            self.host_mounts
                .iter()
                .filter_map(|grant| {
                    grant
                        .resolve_host_path(&namespace_path)
                        .map(|host_path| (grant.namespace_path.len(), host_path))
                })
                .max_by_key(|(prefix_len, _)| *prefix_len)
                .map(|(_, host_path)| host_path)
                .with_context(|| {
                    format!(
                        "Alan OS path is not backed by an explicit Host Mount: {}",
                        path.as_ref().display()
                    )
                })
        } else {
            Ok(self.cwd.join(path))
        }
    }

    /// Translate an internal Host adapter path back to an Agent-visible path.
    pub fn visible_path(&self, host_path: &Path) -> PathBuf {
        if self.host_mounts.is_empty() {
            return host_path.to_path_buf();
        }
        self.host_mounts
            .iter()
            .filter_map(|grant| {
                grant
                    .resolve_namespace_path(host_path)
                    .map(|path| (grant.host_path.components().count(), path))
            })
            .max_by_key(|(prefix_len, _)| *prefix_len)
            .map(|(_, path)| path)
            .unwrap_or_else(|| PathBuf::from("<unmapped-host-path>"))
    }

    /// Redact Host backing roots from native subprocess output.
    pub fn project_text(&self, text: &str) -> String {
        let mut projected = text.to_string();
        let mut projections = self
            .host_mounts
            .iter()
            .flat_map(|grant| {
                [
                    grant.host_path.clone(),
                    dunce::canonicalize(&grant.host_path)
                        .unwrap_or_else(|_| grant.host_path.clone()),
                ]
                .map(|candidate| (candidate, grant.namespace_path.as_str()))
            })
            .collect::<Vec<_>>();
        projections.sort_by_key(|(candidate, _)| std::cmp::Reverse(candidate.as_os_str().len()));
        for (candidate, namespace_path) in projections {
            projected = projected.replace(candidate.to_string_lossy().as_ref(), namespace_path);
        }
        projected
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
mod tests {
    use super::*;

    #[test]
    fn test_tool_context_resolve_path() {
        let config = Arc::new(Config::default());
        let source = tempfile::tempdir().unwrap();
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
        let binding = ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/scratch"),
        )
        .unwrap();
        let ctx = ToolContext::from_binding(binding, config);

        assert_eq!(
            ctx.resolve_path("file.txt").unwrap(),
            source.path().join("file.txt")
        );
        assert_eq!(
            ctx.resolve_path("/mnt/source/absolute/file.txt").unwrap(),
            source.path().join("absolute/file.txt")
        );
        assert!(ctx.resolve_path("/absolute/file.txt").is_err());
    }

    #[test]
    fn test_tool_context_exposes_process_binding() {
        let config = Arc::new(Config::default());
        let ctx = ToolContext::new(
            PathBuf::from("/mnt/source/src"),
            PathBuf::from("/tmp/scratch"),
            config,
        );

        assert_eq!(
            ctx.binding(),
            ToolExecutionBinding::new(
                PathBuf::from("/mnt/source/src"),
                PathBuf::from("/tmp/scratch")
            )
        );
    }

    #[test]
    fn test_tool_context_uses_explicit_sandbox_spec() {
        let config = Arc::new(Config::default());
        let source_dir = tempfile::tempdir().unwrap();
        let approved_dir = tempfile::tempdir().unwrap();
        let source = source_dir.path().to_path_buf();
        let approved = approved_dir.path().to_path_buf();
        let spec = SandboxSpec {
            host_mounts: Vec::new(),
            readable_roots: vec![source.clone(), approved.clone()],
            writable_roots: vec![source.clone(), approved.clone()],
            read_denylist: Vec::new(),
            network: crate::tools::NetworkPosture::Deny,
        };
        let binding = ToolExecutionBinding::new(source.clone(), PathBuf::from("/tmp/scratch"))
            .with_sandbox_spec(spec.clone());
        let ctx = ToolContext::from_binding(binding, config);

        assert_eq!(ctx.binding().sandbox_spec, Some(spec));
        let sandbox = ctx.sandbox().unwrap();
        assert!(sandbox.is_writable(&approved.join("file.txt")));
    }

    #[test]
    fn test_tool_context_projects_host_backing_paths_to_namespace_paths() {
        let source = tempfile::tempdir().unwrap();
        let launch_context = crate::ProcessLaunchContext::new(
            alan_kernel::Namespace::new(),
            alan_kernel::Credentials::user("agent"),
            "/mnt/source",
        )
        .unwrap()
        .with_host_mount(
            crate::HostMountGrant::new("/mnt/source", source.path(), alan_kernel::Access::ReadOnly)
                .unwrap(),
        );
        let binding = ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/scratch"),
        )
        .unwrap();
        let ctx = ToolContext::from_binding(binding, Arc::new(Config::default()));
        let host_path = source.path().join("private.txt");

        let projected = ctx.project_text(&format!("failed to read {}", host_path.display()));

        assert_eq!(projected, "failed to read /mnt/source/private.txt");
        assert!(!projected.contains(source.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn package_projection_is_excluded_from_native_tool_binding() {
        let package = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let mut launch_context = crate::ProcessLaunchContext::root()
            .with_host_mount(
                crate::HostMountGrant::new(
                    "/lib/pkg/example",
                    package.path(),
                    alan_kernel::Access::ReadOnly,
                )
                .unwrap(),
            )
            .with_host_mount(
                crate::HostMountGrant::new(
                    "/mnt/project",
                    project.path(),
                    alan_kernel::Access::ReadWrite,
                )
                .unwrap(),
            );
        launch_context.add_package_reference(
            crate::ProcessPackageReference::new(
                "example",
                "a".repeat(64),
                crate::ProcessPackageKind::Installed,
                "/lib/pkg/example",
                Vec::new(),
            )
            .unwrap(),
        );

        let binding = ToolExecutionBinding::from_launch_context(
            &launch_context,
            PathBuf::from("/tmp/scratch"),
        )
        .unwrap();

        assert_eq!(binding.host_mounts.len(), 1);
        assert_eq!(binding.host_mounts[0].namespace_path, "/mnt/project");
        assert_eq!(binding.cwd, dunce::canonicalize(project.path()).unwrap());
        let sandbox = binding.sandbox_spec.unwrap();
        assert!(
            !sandbox
                .readable_roots
                .iter()
                .any(|root| root.starts_with(package.path()))
        );
    }

    #[tokio::test]
    async fn binding_with_unbacked_namespace_cwd_uses_authorized_host_mount_fallback() {
        let source = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let launch_context = crate::ProcessLaunchContext::root().with_host_mount(
            crate::HostMountGrant::new(
                "/mnt/project",
                source.path(),
                alan_kernel::Access::ReadWrite,
            )
            .unwrap(),
        );

        let binding = ToolExecutionBinding::from_launch_context(
            &launch_context,
            scratch.path().to_path_buf(),
        )
        .unwrap();

        assert_eq!(binding.cwd, dunce::canonicalize(source.path()).unwrap());
        assert_eq!(binding.namespace_cwd, Path::new("/mnt/project"));
        let ctx = ToolContext::from_binding(binding, Arc::new(Config::default()));
        let sandbox = Sandbox::from_spec_with_backend(
            ctx.sandbox_spec.clone().unwrap(),
            crate::tools::SandboxBackendKind::HostMountPathGuard,
        );
        assert!(
            ctx.sandbox_spec
                .as_ref()
                .unwrap()
                .writable_roots
                .iter()
                .any(|root| root == &dunce::canonicalize(source.path()).unwrap())
        );
        assert!(
            !ctx.sandbox_spec
                .as_ref()
                .unwrap()
                .readable_roots
                .iter()
                .any(|root| root == &dunce::canonicalize(scratch.path()).unwrap())
        );

        let execution = sandbox.exec("pwd", &ctx.cwd).await.unwrap();
        assert_eq!(execution.exit_code, 0, "{execution:?}");
        assert_eq!(ctx.project_text(execution.stdout.trim()), "/mnt/project");
    }
}
