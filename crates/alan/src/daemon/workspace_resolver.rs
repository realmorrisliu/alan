//! Workspace Resolver - path resolution layer that maps workspace identifiers to paths.
//!
//! Resolution priority:
//! 1. Alias from the CLI registry
//! 2. Short ID (6 chars) from the CLI registry
//! 3. Identifier interpreted as a path (if valid)
//! 4. Default workspace path

use crate::registry::{WorkspaceRegistry, generate_workspace_id};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::{OsStr, OsString},
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};
use tracing::{debug, warn};

/// Workspace resolution result
#[derive(Debug, Clone)]
pub struct ResolvedWorkspace {
    /// Workspace ID (short hash, 6 chars)
    #[allow(dead_code)]
    pub id: String,
    /// Canonical absolute path
    pub path: PathBuf,
    /// Workspace state directory (`.alan`)
    pub alan_dir: PathBuf,
    /// Optional alias
    #[allow(dead_code)]
    pub alias: Option<String>,
    /// Whether this workspace is registered in the registry
    #[allow(dead_code)]
    pub registered: bool,
}

/// Workspace path resolver
#[derive(Debug, Clone)]
pub struct WorkspaceResolver {
    registry: WorkspaceRegistry,
    alan_home_paths: alan_runtime::AlanHomePaths,
    default_workspace_dir: PathBuf,
}

impl WorkspaceResolver {
    /// Create a new resolver and load the CLI registry
    pub fn new() -> Result<Self> {
        let registry = WorkspaceRegistry::load()?;
        let alan_home_paths =
            alan_runtime::AlanHomePaths::detect().context("Cannot determine home directory")?;
        let default_workspace_dir =
            canonicalize_existing_or_self(alan_home_paths.alan_home_dir.clone());

        Ok(Self {
            registry,
            alan_home_paths,
            default_workspace_dir,
        })
    }

    /// Create with an explicit registry and default workspace directory.
    #[allow(dead_code)]
    pub fn with_registry(registry: WorkspaceRegistry, default_dir: PathBuf) -> Self {
        let alan_home_paths = alan_runtime::AlanHomePaths::from_alan_home_dir(&default_dir);
        Self {
            registry,
            alan_home_paths,
            default_workspace_dir: canonicalize_existing_or_self(default_dir),
        }
    }

    /// Return the resolver's default `.alan` home directory.
    pub fn alan_home_dir(&self) -> &Path {
        &self.default_workspace_dir
    }

    pub fn alan_home_paths(&self) -> &alan_runtime::AlanHomePaths {
        &self.alan_home_paths
    }

    pub fn install_channel(&self) -> alan_runtime::InstallChannel {
        self.alan_home_paths.channel
    }

    /// Get the default workspace directory (`~/.alan/`)
    #[allow(dead_code)]
    fn default_workspace_dir() -> Result<PathBuf> {
        alan_runtime::AlanHomePaths::detect()
            .map(|paths| paths.alan_home_dir)
            .context("Cannot determine home directory")
    }

    /// Resolve a workspace identifier to a path
    ///
    /// Supported identifier formats:
    /// - Registry alias (for example, `"my-project"`)
    /// - Short ID (for example, `"a1b2c3"`)
    /// - Absolute path (for example, `"/home/user/projects/myapp"`)
    /// - Relative path (relative to the current working directory)
    /// - `None` (returns the default workspace)
    pub fn resolve(&self, identifier: Option<&str>) -> Result<ResolvedWorkspace> {
        // `None` means "use the default workspace".
        if identifier.is_none() {
            return self.default_workspace();
        }
        let identifier = identifier.unwrap();

        // 1. Try resolving from the registry (alias or short ID).
        if let Some(entry) = self.registry.find(identifier) {
            let (workspace_path, workspace_alan_dir) =
                self.normalize_workspace_path_and_alan_dir(&entry.path);
            debug!(%identifier, path = %entry.path.display(), "Resolved workspace from registry");
            return Ok(ResolvedWorkspace {
                id: entry.id.clone(),
                path: workspace_path,
                alan_dir: workspace_alan_dir,
                alias: Some(entry.alias.clone()),
                registered: true,
            });
        }

        // 2. Try resolving it as a path.
        let path = Path::new(identifier);
        let canonical = Self::canonicalize_path(path)?;
        let (workspace_path, workspace_alan_dir) =
            self.normalize_workspace_path_and_alan_dir(&canonical);

        // Check whether the path contains a `.alan` directory (initialized workspace).
        if !self.is_valid_workspace(&workspace_path) {
            warn!(
                path = %workspace_path.display(),
                "Path is not a valid workspace (missing workspace state directory)"
            );
        }

        // Generate an ID using the same algorithm as the registry.
        let id = generate_workspace_id(&workspace_path);

        // Check whether this path is actually in the registry (path match).
        let registered = self.registry.find(&id).is_some();

        Ok(ResolvedWorkspace {
            id,
            path: workspace_path,
            alan_dir: workspace_alan_dir,
            alias: None,
            registered,
        })
    }

    /// Resolve a registered workspace identifier (alias or short ID).
    ///
    /// Unlike [`Self::resolve`], this never falls back to interpreting the
    /// identifier as an arbitrary filesystem path.
    pub fn resolve_registered(&self, identifier: &str) -> Result<ResolvedWorkspace> {
        let identifier = identifier.trim();
        let entry = self
            .registry
            .find_registered(identifier)
            .with_context(|| format!("Unknown registered workspace identifier: {identifier}"))?;
        let (workspace_path, workspace_alan_dir) =
            self.normalize_workspace_path_and_alan_dir(&entry.path);
        debug!(%identifier, path = %entry.path.display(), "Resolved workspace from registry");
        Ok(ResolvedWorkspace {
            id: entry.id.clone(),
            path: workspace_path,
            alan_dir: workspace_alan_dir,
            alias: Some(entry.alias.clone()),
            registered: true,
        })
    }

    /// Resolve and ensure the workspace directory exists
    ///
    /// If the path is not initialized (missing `.alan`), create the directory structure.
    pub fn resolve_or_create(&self, identifier: Option<&str>) -> Result<ResolvedWorkspace> {
        let resolved = self.resolve(identifier)?;
        let resolved = self.resolve_existing_workspace_for_creation(&resolved)?;

        // Ensure workspace state structure exists and is complete.
        if !resolved.alan_dir.exists() {
            debug!(path = %resolved.path.display(), "Creating workspace directory structure");
        }
        self.create_workspace_structure(&resolved)?;

        let workspace_path = if resolved.path == self.default_workspace_dir {
            self.default_workspace_dir.clone()
        } else {
            resolved.path.clone()
        };
        let alan_dir = if resolved.alan_dir == self.default_workspace_dir {
            self.default_workspace_dir.clone()
        } else {
            std::fs::canonicalize(workspace_path.join(".alan")).with_context(|| {
                format!(
                    "Failed to canonicalize workspace state directory: {}",
                    resolved.alan_dir.display()
                )
            })?
        };

        Ok(ResolvedWorkspace {
            path: workspace_path,
            alan_dir,
            ..resolved
        })
    }

    fn resolve_existing_workspace_for_creation(
        &self,
        resolved: &ResolvedWorkspace,
    ) -> Result<ResolvedWorkspace> {
        if resolved.path == self.default_workspace_dir {
            return Ok(resolved.clone());
        }

        let normalized_workspace_path = Self::normalize_creation_path(&resolved.path);
        let workspace_path =
            std::fs::canonicalize(&normalized_workspace_path).with_context(|| {
                format!(
                    "Workspace path must already exist before creating state directory: {}",
                    normalized_workspace_path.display()
                )
            })?;
        ensure!(
            workspace_path.is_dir(),
            "Workspace path must be a directory: {}",
            workspace_path.display()
        );
        let alan_dir = workspace_path.join(".alan");
        self.ensure_workspace_state_layout(&workspace_path, &alan_dir)?;

        Ok(ResolvedWorkspace {
            path: workspace_path,
            alan_dir,
            ..resolved.clone()
        })
    }

    /// Get the default workspace
    pub fn default_workspace(&self) -> Result<ResolvedWorkspace> {
        self.ensure_default_workspace_dir_exists()?;
        let path = self.default_workspace_dir.clone();

        let id = generate_workspace_id(&path);

        Ok(ResolvedWorkspace {
            id,
            path,
            alan_dir: self.default_workspace_dir.clone(),
            alias: Some("default".to_string()),
            registered: false,
        })
    }

    /// Get the `.alan` directory for a workspace
    pub fn workspace_alan_dir(&self, workspace_path: &Path) -> PathBuf {
        if workspace_path == self.default_workspace_dir
            || workspace_path
                .file_name()
                .map(|name| name == std::ffi::OsStr::new(".alan"))
                .unwrap_or(false)
        {
            workspace_path.to_path_buf()
        } else {
            alan_runtime::workspace_alan_dir(workspace_path)
        }
    }

    /// Get the channel-scoped generated workspace sessions directory.
    #[allow(dead_code)]
    pub fn workspace_sessions_dir(&self, workspace_path: &Path) -> PathBuf {
        alan_runtime::workspace_sessions_dir_for_channel_from_alan_dir(
            &self.workspace_alan_dir(workspace_path),
            self.install_channel(),
        )
    }

    /// Get the channel-scoped generated workspace memory directory.
    #[allow(dead_code)]
    pub fn workspace_memory_dir(&self, workspace_path: &Path) -> PathBuf {
        alan_runtime::workspace_memory_dir_for_channel_from_alan_dir(
            &self.workspace_alan_dir(workspace_path),
            self.install_channel(),
        )
    }

    /// Get the workspace `persona` directory
    #[allow(dead_code)]
    pub fn workspace_persona_dir(&self, workspace_path: &Path) -> PathBuf {
        alan_runtime::workspace_persona_dir_from_alan_dir(&self.workspace_alan_dir(workspace_path))
    }

    /// Check whether a path is a valid workspace (contains a workspace state directory)
    pub fn is_valid_workspace(&self, path: &Path) -> bool {
        self.workspace_alan_dir(path).is_dir()
    }

    /// Canonicalize a path
    fn canonicalize_path(path: &Path) -> Result<PathBuf> {
        if path.exists() {
            // Path exists, canonicalize it.
            std::fs::canonicalize(path)
                .with_context(|| format!("Failed to canonicalize path: {}", path.display()))
        } else {
            // Path does not exist, check whether it is relative.
            if path.is_relative() {
                let cwd = std::env::current_dir()?;
                let absolute = cwd.join(path);
                if absolute.exists() {
                    std::fs::canonicalize(&absolute).with_context(|| {
                        format!("Failed to canonicalize path: {}", absolute.display())
                    })
                } else {
                    // Path does not exist, but return absolute path (may be created later).
                    Ok(absolute)
                }
            } else {
                // Absolute path that does not exist.
                Ok(path.to_path_buf())
            }
        }
    }

    /// Create workspace directory structure
    fn create_workspace_structure(&self, resolved: &ResolvedWorkspace) -> Result<()> {
        self.ensure_workspace_state_layout(&resolved.path, &resolved.alan_dir)?;
        let alan_dir = if resolved.alan_dir == self.default_workspace_dir {
            self.ensure_default_workspace_dir_exists()?;
            self.default_workspace_dir.clone()
        } else {
            let workspace_path = self.ensure_workspace_root_exists(&resolved.path)?;
            let alan_dir = workspace_path.join(".alan");
            match std::fs::create_dir(&alan_dir) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "Failed to create workspace state directory: {}",
                            alan_dir.display()
                        )
                    });
                }
            }
            let alan_dir = std::fs::canonicalize(&alan_dir).with_context(|| {
                format!(
                    "Failed to canonicalize workspace state directory: {}",
                    alan_dir.display()
                )
            })?;
            self.ensure_workspace_state_layout(&workspace_path, &alan_dir)?;
            alan_dir
        };
        let agents_dir = Self::ensure_fixed_child_dir(&alan_dir, "agents")?;
        let default_agent_dir = Self::ensure_fixed_child_dir(&agents_dir, "default")?;
        let _skills_dir = Self::ensure_fixed_child_dir(&default_agent_dir, "skills")?;
        let channel = self.install_channel();
        let sessions_dir =
            alan_runtime::workspace_sessions_dir_for_channel_from_alan_dir(&alan_dir, channel);
        let memory_dir =
            alan_runtime::workspace_memory_dir_for_channel_from_alan_dir(&alan_dir, channel);
        std::fs::create_dir_all(&sessions_dir).with_context(|| {
            format!(
                "Failed to create workspace sessions directory: {}",
                sessions_dir.display()
            )
        })?;
        let sessions_dir = std::fs::canonicalize(&sessions_dir).with_context(|| {
            format!(
                "Failed to canonicalize workspace sessions directory: {}",
                sessions_dir.display()
            )
        })?;
        ensure!(
            sessions_dir.starts_with(&alan_dir),
            "Workspace sessions directory must stay within workspace state directory: {}",
            sessions_dir.display()
        );
        std::fs::create_dir_all(&memory_dir).with_context(|| {
            format!(
                "Failed to create workspace memory directory: {}",
                memory_dir.display()
            )
        })?;
        let memory_dir = std::fs::canonicalize(&memory_dir).with_context(|| {
            format!(
                "Failed to canonicalize workspace memory directory: {}",
                memory_dir.display()
            )
        })?;
        ensure!(
            memory_dir.starts_with(&alan_dir),
            "Workspace memory directory must stay within workspace state directory: {}",
            memory_dir.display()
        );
        let persona_dir = Self::ensure_fixed_child_dir(&default_agent_dir, "persona")?;

        alan_runtime::prompts::ensure_workspace_memory_layout_at(&memory_dir)?;
        alan_runtime::prompts::ensure_workspace_bootstrap_files_at(&persona_dir)?;

        debug!(path = %alan_dir.display(), "Created workspace directory structure");
        Ok(())
    }

    fn ensure_default_workspace_dir_exists(&self) -> Result<()> {
        self.ensure_workspace_root_exists(&self.default_workspace_dir)
            .with_context(|| {
                format!(
                    "Failed to create default workspace state directory: {}",
                    self.default_workspace_dir.display()
                )
            })?;
        Ok(())
    }

    fn ensure_workspace_root_exists(&self, workspace_path: &Path) -> Result<PathBuf> {
        let workspace_path = Self::normalize_creation_path(workspace_path);
        if workspace_path.exists() {
            return std::fs::canonicalize(&workspace_path).with_context(|| {
                format!(
                    "Failed to canonicalize workspace: {}",
                    workspace_path.display()
                )
            });
        }

        let (existing_ancestor, missing_components) =
            Self::split_existing_workspace_ancestor(&workspace_path)?;
        let mut current = std::fs::canonicalize(&existing_ancestor).with_context(|| {
            format!(
                "Failed to canonicalize workspace ancestor: {}",
                existing_ancestor.display()
            )
        })?;

        for component in missing_components {
            current.push(&component);
            match std::fs::create_dir(&current) {
                Ok(()) => {}
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
                Err(err) => {
                    return Err(err).with_context(|| {
                        format!(
                            "Failed to create workspace directory: {}",
                            current.display()
                        )
                    });
                }
            }
        }

        Ok(current)
    }

    fn ensure_fixed_child_dir(parent: &Path, child_name: &'static str) -> Result<PathBuf> {
        Self::ensure_single_normal_component(OsStr::new(child_name))?;
        let parent = std::fs::canonicalize(parent).with_context(|| {
            format!(
                "Failed to canonicalize parent directory: {}",
                parent.display()
            )
        })?;
        let child_dir = parent.join(child_name);
        match std::fs::create_dir(&child_dir) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(err).with_context(|| {
                    format!("Failed to create directory: {}", child_dir.display())
                });
            }
        }
        let child_dir = std::fs::canonicalize(&child_dir).with_context(|| {
            format!(
                "Failed to canonicalize directory after creation: {}",
                child_dir.display()
            )
        })?;
        ensure!(
            child_dir.parent() == Some(parent.as_path()),
            "Created directory escaped parent: {}",
            child_dir.display()
        );
        ensure!(
            child_dir.file_name() == Some(OsStr::new(child_name)),
            "Created directory name changed unexpectedly: {}",
            child_dir.display()
        );
        Ok(child_dir)
    }

    fn split_existing_workspace_ancestor(
        workspace_path: &Path,
    ) -> Result<(PathBuf, Vec<OsString>)> {
        let mut current = workspace_path;
        let mut missing_components = Vec::new();

        while !current.exists() {
            let component = current.file_name().with_context(|| {
                format!(
                    "Workspace path must have an existing parent: {}",
                    workspace_path.display()
                )
            })?;
            Self::ensure_single_normal_component(component)?;
            missing_components.push(component.to_os_string());
            current = current.parent().with_context(|| {
                format!(
                    "Workspace path must have an existing parent: {}",
                    workspace_path.display()
                )
            })?;
        }

        missing_components.reverse();
        Ok((current.to_path_buf(), missing_components))
    }

    fn ensure_single_normal_component(component: &OsStr) -> Result<()> {
        let mut components = Path::new(component).components();
        ensure!(
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
            "Workspace path component must be a single normal component: {}",
            Path::new(component).display()
        );
        Ok(())
    }

    fn normalize_creation_path(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
            }
        }
        normalized
    }

    fn ensure_workspace_state_layout(&self, workspace_path: &Path, alan_dir: &Path) -> Result<()> {
        if alan_dir == self.default_workspace_dir {
            ensure!(
                workspace_path == self.default_workspace_dir,
                "Default workspace state directory must resolve to {}",
                self.default_workspace_dir.display()
            );
            return Ok(());
        }

        ensure!(
            alan_dir.file_name() == Some(OsStr::new(".alan")),
            "Workspace state directory must end with .alan: {}",
            alan_dir.display()
        );
        ensure!(
            alan_dir.starts_with(workspace_path),
            "Workspace state directory must stay within workspace root: {}",
            alan_dir.display()
        );
        Ok(())
    }

    fn normalize_workspace_path_and_alan_dir(&self, canonical: &Path) -> (PathBuf, PathBuf) {
        let is_explicit_alan_dir = canonical
            .file_name()
            .map(|name| name == std::ffi::OsStr::new(".alan"))
            .unwrap_or(false);
        if is_explicit_alan_dir
            && canonical != self.default_workspace_dir
            && let Some(parent) = canonical.parent()
        {
            return (parent.to_path_buf(), canonical.to_path_buf());
        }

        let alan_dir = self.workspace_alan_dir(canonical);
        let comparable_alan_dir = canonicalize_existing_or_self(alan_dir.clone());
        if comparable_alan_dir == self.default_workspace_dir {
            return (
                self.default_workspace_dir.clone(),
                self.default_workspace_dir.clone(),
            );
        }

        (canonical.to_path_buf(), alan_dir)
    }

    /// Refresh the registry (if modified externally)
    #[allow(dead_code)]
    pub fn refresh_registry(&mut self) -> Result<()> {
        self.registry = WorkspaceRegistry::load()?;
        Ok(())
    }

    /// List all registered workspaces
    #[allow(dead_code)]
    pub fn list_registered(&self) -> &[crate::registry::WorkspaceEntry] {
        self.registry.list()
    }
}

fn canonicalize_existing_or_self(path: PathBuf) -> PathBuf {
    if path.is_relative() {
        return path;
    }
    if let Ok(canonical) = std::fs::canonicalize(&path) {
        return canonical;
    }

    let mut suffix = Vec::<OsString>::new();
    let mut cursor = path.as_path();
    loop {
        let Some(file_name) = cursor.file_name() else {
            return path;
        };
        suffix.push(file_name.to_os_string());
        let Some(parent) = cursor.parent() else {
            return path;
        };
        match std::fs::canonicalize(parent) {
            Ok(mut canonical_parent) => {
                for component in suffix.iter().rev() {
                    canonical_parent.push(component);
                }
                return canonical_parent;
            }
            Err(_) => {
                cursor = parent;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (WorkspaceRegistry, TempDir, String) {
        let temp = TempDir::new().unwrap();
        let workspace_dir = temp.path().join("test-workspace");
        std::fs::create_dir_all(&workspace_dir).unwrap();
        std::fs::create_dir_all(workspace_dir.join(".alan")).unwrap();

        let id = generate_workspace_id(&workspace_dir);
        let entry = crate::registry::WorkspaceEntry {
            id: id.clone(),
            path: workspace_dir.clone(),
            alias: "test-alias".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![entry],
        };

        (registry, temp, id)
    }

    #[test]
    fn test_resolve_by_alias() {
        let (registry, temp, expected_id) = create_test_registry();
        let default_dir = temp.path().join("default");
        let resolver = WorkspaceResolver::with_registry(registry, default_dir);

        let resolved = resolver.resolve(Some("test-alias")).unwrap();
        assert_eq!(resolved.id, expected_id);
        assert!(resolved.registered);
        assert_eq!(resolved.alias, Some("test-alias".to_string()));
        assert_eq!(resolved.alan_dir, resolved.path.join(".alan"));
    }

    #[test]
    fn test_resolve_by_short_id() {
        let (registry, temp, id) = create_test_registry();
        let default_dir = temp.path().join("default");
        let resolver = WorkspaceResolver::with_registry(registry, default_dir);

        let resolved = resolver.resolve(Some(&id)).unwrap();
        assert_eq!(resolved.id, id);
        assert!(resolved.registered);
    }

    #[test]
    fn test_resolve_by_path() {
        let (registry, temp, expected_id) = create_test_registry();
        let workspace_path = temp.path().join("test-workspace");
        let default_dir = temp.path().join("default");
        let resolver = WorkspaceResolver::with_registry(registry, default_dir);

        let resolved = resolver
            .resolve(Some(workspace_path.to_str().unwrap()))
            .unwrap();
        assert_eq!(resolved.id, expected_id);
    }

    #[test]
    fn test_resolve_unregistered_path() {
        let temp = TempDir::new().unwrap();
        let unregistered = temp.path().join("unregistered");
        std::fs::create_dir_all(&unregistered).unwrap();
        std::fs::create_dir_all(unregistered.join(".alan")).unwrap();

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let resolved = resolver
            .resolve(Some(unregistered.to_str().unwrap()))
            .unwrap();

        assert!(!resolved.registered);
        assert_eq!(resolved.alias, None);
        assert!(resolver.is_valid_workspace(&resolved.path));
    }

    #[test]
    fn test_resolve_registered_rejects_path_queries() {
        let (registry, temp, _expected_id) = create_test_registry();
        let workspace_path = temp.path().join("test-workspace");
        let default_dir = temp.path().join("default");
        let resolver = WorkspaceResolver::with_registry(registry, default_dir);

        let err = resolver
            .resolve_registered(workspace_path.to_str().unwrap())
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Unknown registered workspace identifier"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn test_resolve_default() {
        let temp = TempDir::new().unwrap();
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let default_dir = temp.path().join("default-workspace");
        let expected_default_dir = canonicalize_existing_or_self(default_dir.clone());
        let resolver = WorkspaceResolver::with_registry(registry, default_dir.clone());

        let resolved = resolver.resolve(None).unwrap();
        assert_eq!(resolved.path, expected_default_dir);
        assert_eq!(resolved.alan_dir, expected_default_dir);
        assert_eq!(resolved.alias, Some("default".to_string()));
    }

    #[test]
    fn test_resolve_or_create_home_path_uses_default_workspace() {
        let temp = TempDir::new().unwrap();
        let home_dir = temp.path().join("home");
        std::fs::create_dir_all(&home_dir).unwrap();
        let default_dir = home_dir.join(".alan");
        std::fs::create_dir_all(&default_dir).unwrap();
        let default_dir = std::fs::canonicalize(&default_dir).unwrap();

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, default_dir.clone());
        let resolved = resolver
            .resolve_or_create(Some(home_dir.to_str().unwrap()))
            .unwrap();

        assert_eq!(resolved.path, default_dir);
        assert_eq!(resolved.alan_dir, default_dir);
        assert!(resolved.alan_dir.join("runtime/stable/sessions").exists());
        assert!(!resolved.alan_dir.join("sessions").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_registered_home_symlink_uses_default_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let real_home_dir = temp.path().join("real-home");
        std::fs::create_dir_all(&real_home_dir).unwrap();
        let linked_home_dir = temp.path().join("linked-home");
        symlink(&real_home_dir, &linked_home_dir).unwrap();
        let default_dir = linked_home_dir.join(".alan");
        std::fs::create_dir_all(&default_dir).unwrap();
        let default_dir = std::fs::canonicalize(&default_dir).unwrap();

        let id = generate_workspace_id(&linked_home_dir);
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![crate::registry::WorkspaceEntry {
                id: id.clone(),
                path: linked_home_dir,
                alias: "home".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }],
        };

        let resolver = WorkspaceResolver::with_registry(registry, default_dir.clone());
        let resolved = resolver.resolve(Some("home")).unwrap();

        assert_eq!(resolved.id, id);
        assert_eq!(resolved.path, default_dir);
        assert_eq!(resolved.alan_dir, default_dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_or_create_missing_symlinked_home_uses_default_workspace() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let real_home_dir = temp.path().join("real-home");
        std::fs::create_dir_all(&real_home_dir).unwrap();
        let linked_home_dir = temp.path().join("linked-home");
        symlink(&real_home_dir, &linked_home_dir).unwrap();
        let default_dir = linked_home_dir.join(".alan");

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, default_dir);
        let resolved = resolver
            .resolve_or_create(Some(linked_home_dir.to_str().unwrap()))
            .unwrap();
        let expected_default_dir = std::fs::canonicalize(linked_home_dir.join(".alan")).unwrap();

        assert_eq!(resolved.path, expected_default_dir);
        assert_eq!(resolved.alan_dir, expected_default_dir);
        assert!(resolved.alan_dir.join("runtime/stable/sessions").exists());
        assert!(!resolved.alan_dir.join("sessions").exists());
    }

    #[test]
    fn test_default_workspace_dir_not_nested_workspace() {
        let default = WorkspaceResolver::default_workspace_dir().unwrap();
        assert_eq!(
            default
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
            ".alan"
        );
        assert!(
            !default.ends_with("workspace"),
            "default workspace dir should not be ~/.alan/workspace"
        );
    }

    #[test]
    fn test_resolve_or_create() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));

        // Should create state directories inside an existing workspace root.
        let resolved = resolver
            .resolve_or_create(Some(workspace.to_str().unwrap()))
            .unwrap();

        assert!(resolved.alan_dir.exists());
        assert!(resolved.alan_dir.join("runtime/stable/sessions").exists());
        assert!(!resolved.alan_dir.join("sessions").exists());
        assert!(
            resolved
                .alan_dir
                .join("runtime/stable/memory/MEMORY.md")
                .exists()
        );
        assert!(
            resolved
                .alan_dir
                .join("runtime/stable/memory/USER.md")
                .exists()
        );
        assert!(
            resolved
                .alan_dir
                .join("runtime/stable/memory/handoffs/LATEST.md")
                .exists()
        );
        assert!(
            resolved
                .alan_dir
                .join("agents/default/persona/SOUL.md")
                .exists()
        );
    }

    #[test]
    fn test_resolve_or_create_rejects_missing_workspace_root() {
        let temp = TempDir::new().unwrap();
        let missing_workspace = temp.path().join("missing-workspace");

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let err = resolver
            .resolve_or_create(Some(missing_workspace.to_str().unwrap()))
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("Workspace path must already exist before creating state directory")
        );
    }

    #[test]
    fn test_resolve_or_create_rejects_state_dir_outside_workspace_root() {
        let temp = TempDir::new().unwrap();
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };
        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let resolved = ResolvedWorkspace {
            id: "abc123".to_string(),
            path: temp.path().join("workspace"),
            alan_dir: temp.path().join("outside/.alan"),
            alias: None,
            registered: false,
        };

        let err = resolver.create_workspace_structure(&resolved).unwrap_err();

        assert!(
            err.to_string()
                .contains("Workspace state directory must stay within workspace root")
        );
    }

    #[test]
    fn test_is_valid_workspace() {
        let temp = TempDir::new().unwrap();
        let valid = temp.path().join("valid");
        let invalid = temp.path().join("invalid");

        std::fs::create_dir_all(valid.join(".alan")).unwrap();
        std::fs::create_dir_all(&invalid).unwrap();

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));

        assert!(resolver.is_valid_workspace(&valid));
        assert!(!resolver.is_valid_workspace(&invalid));
    }

    #[test]
    fn test_workspace_id_generation_consistency() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test-workspace");
        std::fs::create_dir_all(&path).unwrap();

        // Multiple generations should be stable.
        let id1 = generate_workspace_id(&path);
        let id2 = generate_workspace_id(&path);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 6);

        // Different paths should produce different IDs.
        let other_path = temp.path().join("other-workspace");
        std::fs::create_dir_all(&other_path).unwrap();
        let other_id = generate_workspace_id(&other_path);
        assert_ne!(id1, other_id);
    }

    #[test]
    fn test_workspace_dir_helpers() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));

        assert_eq!(
            resolver.workspace_sessions_dir(&workspace),
            workspace.join(".alan/runtime/stable/sessions")
        );
        assert_eq!(
            resolver.workspace_memory_dir(&workspace),
            workspace.join(".alan/runtime/stable/memory")
        );
        assert_eq!(
            resolver.workspace_persona_dir(&workspace),
            workspace.join(".alan/agents/default/persona")
        );
    }

    #[test]
    fn test_resolve_explicit_alan_path_uses_parent_as_workspace_root() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        let alan_dir = workspace.join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let resolved = resolver.resolve(Some(alan_dir.to_str().unwrap())).unwrap();

        assert_eq!(
            std::fs::canonicalize(&resolved.path).unwrap(),
            std::fs::canonicalize(&workspace).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&resolved.alan_dir).unwrap(),
            std::fs::canonicalize(&alan_dir).unwrap()
        );
    }

    #[test]
    fn test_resolve_registry_entry_with_alan_path_normalizes_to_parent_root() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        let alan_dir = workspace.join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();

        let entry = crate::registry::WorkspaceEntry {
            id: generate_workspace_id(&workspace),
            path: std::fs::canonicalize(&alan_dir).unwrap(),
            alias: "legacy-alan-path".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![entry],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let resolved = resolver.resolve(Some("legacy-alan-path")).unwrap();

        assert_eq!(
            std::fs::canonicalize(&resolved.path).unwrap(),
            std::fs::canonicalize(&workspace).unwrap()
        );
        assert_eq!(
            std::fs::canonicalize(&resolved.alan_dir).unwrap(),
            std::fs::canonicalize(&alan_dir).unwrap()
        );
    }

    #[test]
    fn test_resolve_or_create_normalizes_nonexistent_parent_segments() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("workspace")).unwrap();
        let target = temp.path().join("nested").join("..").join("workspace");

        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join("default"));
        let resolved = resolver
            .resolve_or_create(Some(target.to_str().unwrap()))
            .unwrap();

        assert_eq!(
            std::fs::canonicalize(&resolved.path).unwrap(),
            std::fs::canonicalize(temp.path().join("workspace")).unwrap()
        );
        assert!(resolved.alan_dir.join("runtime/stable/sessions").exists());
        assert!(!resolved.alan_dir.join("sessions").exists());
    }

    #[test]
    fn test_workspace_dir_helpers_use_dev_runtime_namespace_for_dev_home() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };
        let resolver = WorkspaceResolver::with_registry(registry, temp.path().join(".alan-dev"));

        assert_eq!(
            resolver.workspace_sessions_dir(&workspace),
            workspace.join(".alan/runtime/dev/sessions")
        );
        assert_eq!(
            resolver.workspace_memory_dir(&workspace),
            workspace.join(".alan/runtime/dev/memory")
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_dev_channel_survives_canonicalized_home_symlink() {
        let temp = TempDir::new().unwrap();
        let physical_home = temp.path().join("physical-dev-home");
        std::fs::create_dir_all(&physical_home).unwrap();
        let dev_home_link = temp.path().join(".alan-dev");
        std::os::unix::fs::symlink(&physical_home, &dev_home_link).unwrap();
        let workspace = temp.path().join("workspace");
        let registry = WorkspaceRegistry {
            version: 1,
            workspaces: vec![],
        };

        let resolver = WorkspaceResolver::with_registry(registry, dev_home_link.clone());
        let canonical_physical_home = std::fs::canonicalize(&physical_home).unwrap();

        assert_eq!(
            resolver.install_channel(),
            alan_runtime::InstallChannel::Dev
        );
        assert_eq!(resolver.alan_home_dir(), canonical_physical_home.as_path());
        assert_eq!(
            resolver.alan_home_paths().alan_home_dir.as_path(),
            dev_home_link.as_path()
        );
        assert_eq!(
            resolver.workspace_sessions_dir(&workspace),
            workspace.join(".alan/runtime/dev/sessions")
        );
    }
}
