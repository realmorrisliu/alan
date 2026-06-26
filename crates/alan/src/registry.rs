//! Workspace registry — persistent workspace registration.
//!
//! Maintains a `registry.json` under the active channel's alan home that tracks
//! all known workspaces across the filesystem. Each workspace is identified
//! by its canonical path, with a short hash ID and user-friendly alias.

use alan_agent_engine::AlanHomePaths;
#[cfg(test)]
use alan_agent_engine::InstallChannel;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Normalize a canonical workspace path to the workspace root.
///
/// If the input points to a non-default `.alan` directory, this returns its parent
/// as the workspace root. The active channel's default workspace remains unchanged.
pub fn normalize_workspace_root_path(path: &Path) -> PathBuf {
    let is_alan_dir = path
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false);
    if !is_alan_dir {
        return path.to_path_buf();
    }

    let is_default_workspace = AlanHomePaths::detect()
        .map(|paths| path == paths.alan_home_dir)
        .unwrap_or(false);
    if is_default_workspace {
        return path.to_path_buf();
    }

    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

/// The workspace registry, stored in the active channel's alan home.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRegistry {
    pub version: u32,
    pub workspaces: Vec<WorkspaceEntry>,
}

/// A single registered workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Short hash ID derived from canonical path (6 hex chars)
    pub id: String,
    /// Canonical absolute path to the workspace root
    pub path: PathBuf,
    /// User-friendly alias (defaults to directory name)
    pub alias: String,
    /// ISO 8601 timestamp
    pub created_at: String,
}

impl WorkspaceRegistry {
    #[cfg(test)]
    fn registry_path_from_home(home: &Path) -> PathBuf {
        Self::registry_path_from_home_for_channel(home, InstallChannel::Stable)
    }

    #[cfg(test)]
    fn registry_path_from_home_for_channel(home: &Path, channel: InstallChannel) -> PathBuf {
        AlanHomePaths::from_home_dir_for_channel(home, channel).global_registry_path
    }

    /// Default registry file path.
    pub fn registry_path() -> Result<PathBuf> {
        AlanHomePaths::detect()
            .map(|paths| paths.global_registry_path)
            .context("Cannot determine home directory")
    }

    /// Load registry from disk, creating an empty one if it doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::registry_path()?;
        Self::load_from_path(&path)
    }

    /// Load registry from a specific path, creating an empty one if it doesn't exist.
    pub(crate) fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                version: 1,
                workspaces: Vec::new(),
            });
        }

        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read registry: {}", path.display()))?;
        match serde_json::from_str(&content) {
            Ok(registry) => Ok(registry),
            Err(err) => {
                tracing::warn!(%err, path = %path.display(), "Failed to parse registry, backing it up and starting fresh");
                let backup_path = path.with_extension("json.bak");
                let _ = fs::rename(path, &backup_path);
                Ok(Self {
                    version: 1,
                    workspaces: Vec::new(),
                })
            }
        }
    }

    /// Save registry to disk atomically.
    pub fn save(&self) -> Result<()> {
        let path = Self::registry_path()?;
        self.save_to_path(&path)
    }

    /// Save registry to a specific path atomically.
    pub(crate) fn save_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;

        // Write atomically: write to temp file, then rename
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &content)
            .with_context(|| format!("Failed to write registry: {}", tmp_path.display()))?;
        fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to rename registry: {}", path.display()))?;
        Ok(())
    }

    /// Register a workspace. Returns the created entry.
    ///
    /// Fails if the path is already registered or the alias conflicts.
    pub fn register(
        &mut self,
        workspace_path: &Path,
        alias: Option<String>,
    ) -> Result<WorkspaceEntry> {
        let canonical = fs::canonicalize(workspace_path)
            .with_context(|| format!("Cannot resolve path: {}", workspace_path.display()))?;
        let canonical = normalize_workspace_root_path(&canonical);

        // Check for duplicate path
        if self.workspaces.iter().any(|w| w.path == canonical) {
            bail!("Workspace already registered: {}", canonical.display());
        }

        let id = generate_workspace_id(&canonical);
        let alias = alias.unwrap_or_else(|| default_alias(&canonical));

        // Check for duplicate alias
        if self.workspaces.iter().any(|w| w.alias == alias) {
            bail!(
                "Alias '{}' already in use. Use --name to specify a different alias.",
                alias
            );
        }

        let entry = WorkspaceEntry {
            id,
            path: canonical,
            alias,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        self.workspaces.push(entry.clone());
        Ok(entry)
    }

    /// Unregister a workspace by alias, ID, or path.
    pub fn unregister(&mut self, query: &str) -> Result<WorkspaceEntry> {
        let idx = self
            .find_index(query)
            .with_context(|| format!("Workspace not found: '{}'", query))?;
        Ok(self.workspaces.remove(idx))
    }

    /// Find a workspace entry by alias, short ID, or path.
    pub fn find(&self, query: &str) -> Option<&WorkspaceEntry> {
        let idx = self.find_index(query)?;
        self.workspaces.get(idx)
    }

    /// Find a registered workspace by alias or short ID only.
    ///
    /// This intentionally does not interpret the query as a filesystem path.
    pub fn find_registered(&self, query: &str) -> Option<&WorkspaceEntry> {
        let idx = self.find_registered_index(query)?;
        self.workspaces.get(idx)
    }

    /// Find the index of a workspace by alias, short ID, or path.
    fn find_index(&self, query: &str) -> Option<usize> {
        if let Some(idx) = self.find_registered_index(query) {
            return Some(idx);
        }

        // Try path (resolve to canonical for comparison)
        if let Ok(canonical) = fs::canonicalize(query)
            && let Some(idx) = self
                .workspaces
                .iter()
                .position(|w| w.path == normalize_workspace_root_path(&canonical))
        {
            return Some(idx);
        }

        // Try path as-is (might match stored path directly)
        let query_path = normalize_workspace_root_path(&PathBuf::from(query));
        self.workspaces.iter().position(|w| w.path == query_path)
    }

    fn find_registered_index(&self, query: &str) -> Option<usize> {
        if let Some(idx) = self.workspaces.iter().position(|w| w.alias == query) {
            return Some(idx);
        }

        self.workspaces.iter().position(|w| w.id == query)
    }

    /// List all registered workspaces.
    pub fn list(&self) -> &[WorkspaceEntry] {
        &self.workspaces
    }
}

/// Generate a short workspace ID from the canonical path.
pub fn generate_workspace_id(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let hash = hasher.finalize();
    hash.iter().take(3).map(|b| format!("{:02x}", b)).collect()
}

/// Derive a default alias from a directory path.
fn default_alias(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "workspace".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_registry(_tmp: &TempDir) -> WorkspaceRegistry {
        // Override the registry path for testing by creating registry manually
        WorkspaceRegistry {
            version: 1,
            workspaces: Vec::new(),
        }
    }

    #[test]
    fn test_generate_workspace_id() {
        let path = PathBuf::from("/Users/test/my-project");
        let id = generate_workspace_id(&path);
        assert_eq!(id.len(), 6);
        // Same path always produces same ID
        assert_eq!(id, generate_workspace_id(&path));
        // Different path produces different ID
        let other = generate_workspace_id(&PathBuf::from("/Users/test/other"));
        assert_ne!(id, other);
    }

    #[test]
    fn test_registry_path_from_home_is_channel_scoped() {
        let home = Path::new("/tmp/demo-home");

        assert_eq!(
            WorkspaceRegistry::registry_path_from_home(home),
            Path::new("/tmp/demo-home/.alan/registry.json")
        );
        assert_eq!(
            WorkspaceRegistry::registry_path_from_home_for_channel(home, InstallChannel::Dev),
            Path::new("/tmp/demo-home/.alan-dev/registry.json")
        );
    }

    #[test]
    fn test_default_alias() {
        assert_eq!(
            default_alias(Path::new("/Users/test/my-project")),
            "my-project"
        );
        assert_eq!(default_alias(Path::new("/Users/test/.alan")), ".alan");
        // Root path fallback
        assert_eq!(default_alias(Path::new("/")), "workspace");
        // Empty path fallback
        assert_eq!(default_alias(Path::new("")), "workspace");
    }

    #[test]
    fn test_register_and_find() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        // Create a workspace dir
        let ws_dir = tmp.path().join("my-workspace");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("test-ws".to_string()))
            .unwrap();
        assert_eq!(entry.alias, "test-ws");
        assert_eq!(entry.id.len(), 6);

        // Find by alias
        assert!(registry.find("test-ws").is_some());
        // Find by ID
        assert!(registry.find(&entry.id).is_some());
        // Find by path
        assert!(registry.find(ws_dir.to_str().unwrap()).is_some());
    }

    #[test]
    fn test_find_registered_only_matches_alias_or_id() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("my-workspace");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("test-ws".to_string()))
            .unwrap();

        assert!(registry.find_registered("test-ws").is_some());
        assert!(registry.find_registered(&entry.id).is_some());
        assert!(registry.find_registered(ws_dir.to_str().unwrap()).is_none());
    }

    #[test]
    fn test_find_by_canonical_path() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        // Create a workspace dir with nested structure
        let ws_dir = tmp.path().join("parent").join("workspace");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry.register(&ws_dir, None).unwrap();

        // Find by canonical path
        let canonical = fs::canonicalize(&ws_dir).unwrap();
        assert!(registry.find(canonical.to_str().unwrap()).is_some());

        // Find by stored path (non-canonical would fail canonicalize but match directly)
        assert!(registry.find(entry.path.to_str().unwrap()).is_some());
    }

    #[test]
    fn test_find_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let registry = setup_test_registry(&tmp);

        assert!(registry.find("nonexistent").is_none());
        assert!(registry.find("/path/that/does/not/exist").is_none());
    }

    #[test]
    fn test_register_duplicate_path_fails() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("dup");
        std::fs::create_dir_all(&ws_dir).unwrap();

        registry
            .register(&ws_dir, Some("first".to_string()))
            .unwrap();
        let err = registry.register(&ws_dir, Some("second".to_string()));
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("already registered"));
    }

    #[test]
    fn test_register_duplicate_alias_fails() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws1 = tmp.path().join("ws1");
        let ws2 = tmp.path().join("ws2");
        std::fs::create_dir_all(&ws1).unwrap();
        std::fs::create_dir_all(&ws2).unwrap();

        registry
            .register(&ws1, Some("same-name".to_string()))
            .unwrap();
        let err = registry.register(&ws2, Some("same-name".to_string()));
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("already in use"));
    }

    #[test]
    fn test_register_without_alias_uses_default() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("my-awesome-project");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry.register(&ws_dir, None).unwrap();
        assert_eq!(entry.alias, "my-awesome-project");
    }

    #[test]
    fn test_register_with_alan_dir_path_normalizes_to_parent_workspace_root() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("ws-root");
        let alan_dir = ws_dir.join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();

        let entry = registry.register(&alan_dir, None).unwrap();
        assert_eq!(entry.path, fs::canonicalize(&ws_dir).unwrap());
        assert_eq!(entry.alias, "ws-root");
    }

    #[test]
    fn test_register_invalid_path_fails() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let nonexistent = tmp.path().join("does-not-exist");
        let err = registry.register(&nonexistent, None);
        assert!(err.is_err());
    }

    #[test]
    fn test_unregister() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("to-remove");
        std::fs::create_dir_all(&ws_dir).unwrap();

        registry
            .register(&ws_dir, Some("removable".to_string()))
            .unwrap();
        assert_eq!(registry.list().len(), 1);

        let removed = registry.unregister("removable").unwrap();
        assert_eq!(removed.alias, "removable");
        assert_eq!(registry.list().len(), 0);
    }

    #[test]
    fn test_unregister_by_id() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("by-id");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("by-id-alias".to_string()))
            .unwrap();
        let id = entry.id.clone();

        let removed = registry.unregister(&id).unwrap();
        assert_eq!(removed.id, id);
        assert_eq!(registry.list().len(), 0);
    }

    #[test]
    fn test_unregister_by_path() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("by-path");
        std::fs::create_dir_all(&ws_dir).unwrap();

        registry
            .register(&ws_dir, Some("by-path-alias".to_string()))
            .unwrap();

        let removed = registry.unregister(ws_dir.to_str().unwrap()).unwrap();
        assert_eq!(removed.alias, "by-path-alias");
        assert_eq!(registry.list().len(), 0);
    }

    #[test]
    fn test_unregister_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let err = registry.unregister("nonexistent");
        assert!(err.is_err());
        let err_msg = err.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    #[test]
    fn test_unregister_after_path_deleted() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("deleted");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("deleted-ws".to_string()))
            .unwrap();

        // Delete the directory
        std::fs::remove_dir_all(&ws_dir).unwrap();

        // Should still be able to unregister by ID
        let removed = registry.unregister(&entry.id).unwrap();
        assert_eq!(removed.alias, "deleted-ws");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("roundtrip");
        std::fs::create_dir_all(&ws_dir).unwrap();

        registry
            .register(&ws_dir, Some("rt-test".to_string()))
            .unwrap();

        let json = serde_json::to_string_pretty(&registry).unwrap();
        let loaded: WorkspaceRegistry = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].alias, "rt-test");
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let registry_path = tmp.path().join("test-registry.json");

        // Create registry
        let mut registry = WorkspaceRegistry {
            version: 1,
            workspaces: Vec::new(),
        };

        let ws_dir = tmp.path().join("saved-ws");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("saved".to_string()))
            .unwrap();

        // Save to file
        let json = serde_json::to_string_pretty(&registry).unwrap();
        fs::write(&registry_path, json).unwrap();

        // Load from file
        let content = fs::read_to_string(&registry_path).unwrap();
        let loaded: WorkspaceRegistry = serde_json::from_str(&content).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.workspaces.len(), 1);
        assert_eq!(loaded.workspaces[0].id, entry.id);
        assert_eq!(loaded.workspaces[0].alias, "saved");
    }

    #[test]
    fn test_empty_registry_list() {
        let tmp = TempDir::new().unwrap();
        let registry = setup_test_registry(&tmp);

        let list = registry.list();
        assert!(list.is_empty());
    }

    #[test]
    fn test_multiple_workspaces() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws1 = tmp.path().join("ws1");
        let ws2 = tmp.path().join("ws2");
        let ws3 = tmp.path().join("ws3");
        std::fs::create_dir_all(&ws1).unwrap();
        std::fs::create_dir_all(&ws2).unwrap();
        std::fs::create_dir_all(&ws3).unwrap();

        let e1 = registry.register(&ws1, Some("alpha".to_string())).unwrap();
        let e2 = registry.register(&ws2, Some("beta".to_string())).unwrap();
        let e3 = registry.register(&ws3, Some("gamma".to_string())).unwrap();

        assert_eq!(registry.list().len(), 3);

        // Find each by different methods
        assert_eq!(registry.find("alpha").unwrap().id, e1.id);
        assert_eq!(registry.find(&e2.id).unwrap().alias, "beta");
        assert_eq!(registry.find(ws3.to_str().unwrap()).unwrap().id, e3.id);
    }

    #[test]
    fn test_workspace_entry_fields() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        let ws_dir = tmp.path().join("full-test");
        std::fs::create_dir_all(&ws_dir).unwrap();

        let entry = registry
            .register(&ws_dir, Some("test-alias".to_string()))
            .unwrap();

        assert_eq!(entry.alias, "test-alias");
        assert_eq!(entry.id.len(), 6);
        assert!(entry.path.to_string_lossy().contains("full-test"));
        // Check timestamp is valid ISO 8601
        assert!(entry.created_at.contains('T'));
        assert!(entry.created_at.contains('+') || entry.created_at.ends_with('Z'));
    }

    #[test]
    fn test_list_returns_all_workspaces() {
        let tmp = TempDir::new().unwrap();
        let mut registry = setup_test_registry(&tmp);

        for i in 0..5 {
            let ws_dir = tmp.path().join(format!("ws{}", i));
            std::fs::create_dir_all(&ws_dir).unwrap();
            registry
                .register(&ws_dir, Some(format!("workspace-{}", i)))
                .unwrap();
        }

        let list = registry.list();
        assert_eq!(list.len(), 5);

        // Verify order is preserved
        for (i, ws) in list.iter().enumerate() {
            assert_eq!(ws.alias, format!("workspace-{}", i));
        }
    }
}
