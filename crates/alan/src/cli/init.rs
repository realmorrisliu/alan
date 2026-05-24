//! `alan init` — initialize a directory as a workspace.

use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsStr,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use crate::registry::WorkspaceRegistry;
use crate::registry::normalize_workspace_root_path;

/// Run the `alan init` command.
pub fn run_init(path: Option<PathBuf>, name: Option<String>, silent: bool) -> Result<()> {
    run_init_with_registry_path(path, name, silent, None)
}

fn run_init_with_registry_path(
    path: Option<PathBuf>,
    name: Option<String>,
    silent: bool,
    registry_path: Option<&Path>,
) -> Result<()> {
    let target_path = resolve_target_path(path)?;
    let target_path = normalize_workspace_root_path(&target_path);
    init_workspace_with_registry_path(&target_path, name, silent, registry_path)
}

/// Resolve the target path from optional input path.
fn resolve_target_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(p) => {
            std::fs::create_dir_all(&p)
                .with_context(|| format!("Cannot create directory: {}", p.display()))?;
            std::fs::canonicalize(&p)
                .with_context(|| format!("Cannot resolve path: {}", p.display()))
        }
        None => std::env::current_dir().context("Cannot determine current directory"),
    }
}

/// Initialize the workspace directory structure.
/// Returns true if the .alan directory was newly created.
pub fn create_alan_directory(alan_dir: &Path) -> Result<bool> {
    create_alan_directory_for_channel(alan_dir, alan_runtime::InstallChannel::detect_current())
}

fn create_alan_directory_for_channel(
    alan_dir: &Path,
    channel: alan_runtime::InstallChannel,
) -> Result<bool> {
    let created = !alan_dir.exists();
    let alan_dir = ensure_workspace_alan_dir(alan_dir)?;
    let workspace_root = alan_dir
        .parent()
        .context("Workspace .alan directory must have a parent workspace root")?;

    let layout = alan_runtime::AgentRootLayout::new();
    let default_root = layout.workspace_default_root_from_alan_dir(&alan_dir);
    let _agents_dir = ensure_workspace_layout_dir(
        workspace_root,
        &layout.agent_roots_dir_from_alan_dir(&alan_dir),
    )?;
    let _agent_dir = ensure_workspace_layout_dir(workspace_root, &default_root.root_dir)?;
    let _skills_dir = ensure_workspace_layout_dir(workspace_root, &default_root.skills_dir)?;
    let _sessions_dir = ensure_workspace_layout_dir(
        workspace_root,
        &alan_runtime::workspace_sessions_dir_for_channel_from_alan_dir(&alan_dir, channel),
    )?;
    let memory_dir = ensure_workspace_layout_dir(
        workspace_root,
        &alan_runtime::workspace_memory_dir_for_channel_from_alan_dir(&alan_dir, channel),
    )?;
    let persona_dir = ensure_workspace_layout_dir(workspace_root, &default_root.persona_dir)?;
    let public_agents_dir = ensure_fixed_child_dir(workspace_root, ".agents")?;
    let _public_skills_dir = ensure_fixed_child_dir(&public_agents_dir, "skills")?;

    alan_runtime::prompts::ensure_workspace_memory_layout_at(&memory_dir)?;
    alan_runtime::prompts::ensure_workspace_bootstrap_files_at(&persona_dir)?;

    Ok(created)
}

fn ensure_workspace_layout_dir(workspace_root: &Path, path: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("Cannot create directory: {}", path.display()))?;
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("Cannot resolve directory: {}", path.display()))?;
    ensure!(
        canonical.starts_with(workspace_root),
        "Workspace directory escaped workspace root: {}",
        canonical.display()
    );
    Ok(canonical)
}

fn ensure_workspace_alan_dir(alan_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(alan_dir)?;
    let canonical = std::fs::canonicalize(alan_dir)
        .with_context(|| format!("Cannot resolve .alan directory: {}", alan_dir.display()))?;
    anyhow::ensure!(
        canonical.file_name() == Some(std::ffi::OsStr::new(".alan")),
        "Workspace state directory must end with .alan: {}",
        canonical.display()
    );
    Ok(canonical)
}

fn ensure_fixed_child_dir(parent: &Path, child_name: &'static str) -> Result<PathBuf> {
    let mut components = Path::new(child_name).components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none(),
        "Workspace path component must be a single normal component: {}",
        child_name
    );

    let parent = std::fs::canonicalize(parent)
        .with_context(|| format!("Cannot resolve parent directory: {}", parent.display()))?;
    let child_dir = parent.join(child_name);
    match std::fs::create_dir(&child_dir) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
        Err(err) => {
            return Err(err)
                .with_context(|| format!("Cannot create directory: {}", child_dir.display()));
        }
    }

    let canonical = std::fs::canonicalize(&child_dir)
        .with_context(|| format!("Cannot resolve directory: {}", child_dir.display()))?;
    ensure!(
        canonical.parent() == Some(parent.as_path()),
        "Workspace directory escaped parent: {}",
        canonical.display()
    );
    ensure!(
        canonical.file_name() == Some(OsStr::new(child_name)),
        "Workspace directory name changed unexpectedly: {}",
        canonical.display()
    );
    Ok(canonical)
}

fn init_workspace_with_registry_path(
    target_path: &Path,
    name: Option<String>,
    silent: bool,
    registry_path: Option<&Path>,
) -> Result<()> {
    let alan_dir = alan_runtime::workspace_alan_dir(target_path);

    // Create .alan directory structure if it doesn't already exist
    let _created = create_alan_directory(&alan_dir)?;

    // Register in the workspace registry
    let mut registry = match registry_path {
        Some(path) => WorkspaceRegistry::load_from_path(path)?,
        None => WorkspaceRegistry::load()?,
    };

    // Check if already registered
    if let Some(existing) = registry.find(target_path.to_str().unwrap_or("")) {
        if !silent {
            println!("Workspace already initialized at {}", target_path.display());
            println!("  Alias: {}", existing.alias);
            println!("  ID:    {}", existing.id);
        }
        return Ok(());
    }

    let entry = registry.register(target_path, name)?;
    match registry_path {
        Some(path) => registry.save_to_path(path)?,
        None => registry.save()?,
    }

    if !silent {
        println!("✅ Workspace initialized: {}", target_path.display());
        println!("   Alias: {}", entry.alias);
        println!("   ID:    {}", entry.id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_target_path_with_none() {
        // Should return current directory
        let result = resolve_target_path(None);
        assert!(result.is_ok());
        // Current directory should exist
        assert!(result.unwrap().exists());
    }

    #[test]
    fn test_resolve_target_path_with_existing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("existing");
        std::fs::create_dir_all(&path).unwrap();

        let result = resolve_target_path(Some(path.clone())).unwrap();
        assert!(result.exists());
    }

    #[test]
    fn test_resolve_target_path_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new/nested/dir");

        assert!(!path.exists());
        let result = resolve_target_path(Some(path.clone())).unwrap();
        assert!(result.exists());
    }

    #[test]
    fn test_create_alan_directory_new() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        assert!(!alan_dir.exists());
        let created = create_alan_directory(&alan_dir).unwrap();

        assert!(created);
        assert!(alan_dir.exists());
        assert!(
            alan_dir
                .join("agents")
                .join("default")
                .join("skills")
                .exists()
        );
        assert!(!alan_dir.join("agent").exists());
        let runtime_sessions_dir = alan_dir.join("runtime/stable/sessions");
        let runtime_memory_dir = alan_dir.join("runtime/stable/memory");
        assert!(runtime_sessions_dir.exists());
        assert!(runtime_memory_dir.exists());
        assert!(!alan_dir.join("sessions").exists());
        assert!(!alan_dir.join("memory").exists());
        assert!(runtime_memory_dir.join("USER.md").exists());
        assert!(
            alan_dir
                .join("agents")
                .join("default")
                .join("persona")
                .exists()
        );
        assert!(runtime_memory_dir.join("MEMORY.md").exists());
        assert!(runtime_memory_dir.join("handoffs/LATEST.md").exists());
        assert!(runtime_memory_dir.join("daily").exists());
        assert!(runtime_memory_dir.join("sessions").exists());
        assert!(runtime_memory_dir.join("working").exists());
        assert!(runtime_memory_dir.join("topics").exists());
        assert!(runtime_memory_dir.join("inbox").exists());
        assert!(tmp.path().join(".agents").join("skills").exists());
        assert!(
            alan_dir
                .join("agents")
                .join("default")
                .join("persona")
                .join("SOUL.md")
                .exists()
        );
    }

    #[test]
    fn test_create_alan_directory_existing() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        // Create manually first
        std::fs::create_dir_all(&alan_dir).unwrap();

        let created = create_alan_directory(&alan_dir).unwrap();
        assert!(!created);
        assert!(
            alan_dir
                .join("agents")
                .join("default")
                .join("skills")
                .exists()
        );
        assert!(!alan_dir.join("agent").exists());
        let runtime_memory_dir = alan_dir.join("runtime/stable/memory");
        assert!(alan_dir.join("runtime/stable/sessions").exists());
        assert!(!alan_dir.join("sessions").exists());
        assert!(runtime_memory_dir.join("MEMORY.md").exists());
        assert!(runtime_memory_dir.join("USER.md").exists());
        assert!(runtime_memory_dir.join("handoffs/LATEST.md").exists());
        assert!(tmp.path().join(".agents").join("skills").exists());
        assert!(
            alan_dir
                .join("agents")
                .join("default")
                .join("persona")
                .join("SOUL.md")
                .exists()
        );
    }

    #[test]
    fn test_create_alan_directory_preserves_existing_content() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        // Create manually with custom content
        std::fs::create_dir_all(alan_dir.join("custom")).unwrap();
        std::fs::write(alan_dir.join("custom/file.txt"), "hello").unwrap();

        let created = create_alan_directory(&alan_dir).unwrap();
        assert!(!created);

        // Custom content should still exist
        assert!(alan_dir.join("custom/file.txt").exists());
        let content = std::fs::read_to_string(alan_dir.join("custom/file.txt")).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_memory_md_content() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        create_alan_directory(&alan_dir).unwrap();

        let memory_content =
            std::fs::read_to_string(alan_dir.join("runtime/stable/memory/MEMORY.md")).unwrap();
        assert_eq!(memory_content, "# Memory\n");
    }

    #[test]
    fn test_pure_text_memory_foundation_content() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        create_alan_directory(&alan_dir).unwrap();

        let user_content =
            std::fs::read_to_string(alan_dir.join("runtime/stable/memory/USER.md")).unwrap();
        let latest_handoff =
            std::fs::read_to_string(alan_dir.join("runtime/stable/memory/handoffs/LATEST.md"))
                .unwrap();

        assert_eq!(user_content, "# User Memory\n");
        assert_eq!(latest_handoff, "# Latest Handoff\n");
    }

    #[test]
    fn test_create_alan_directory_rejects_non_dot_alan_path() {
        let tmp = TempDir::new().unwrap();
        let invalid = tmp.path().join("workspace-state");

        let err = create_alan_directory(&invalid).unwrap_err();
        assert!(
            err.to_string()
                .contains("Workspace state directory must end with .alan")
        );
    }

    #[test]
    fn test_run_init_with_dot_alan_path_initializes_parent_workspace() {
        let tmp = TempDir::new().unwrap();
        let registry_path = tmp.path().join("registry.json");
        let workspace_root = tmp.path().join("repo");
        let explicit_alan_dir = workspace_root.join(".alan");
        std::fs::create_dir_all(&explicit_alan_dir).unwrap();

        run_init_with_registry_path(
            Some(explicit_alan_dir.clone()),
            Some("repo-init".to_string()),
            true,
            Some(&registry_path),
        )
        .unwrap();

        assert!(
            workspace_root
                .join(".alan")
                .join("runtime")
                .join("stable")
                .join("sessions")
                .exists()
        );
        assert!(
            workspace_root
                .join(".alan")
                .join("runtime")
                .join("stable")
                .join("memory")
                .join("MEMORY.md")
                .exists()
        );
        assert!(!workspace_root.join(".alan").join("sessions").exists());
        assert!(!workspace_root.join(".alan").join("memory").exists());
        assert!(!workspace_root.join(".alan").join(".alan").exists());
        let registry = WorkspaceRegistry::load_from_path(&registry_path).unwrap();
        let entry = registry.find("repo-init").unwrap();
        assert_eq!(entry.path, std::fs::canonicalize(&workspace_root).unwrap());
    }

    #[test]
    fn test_create_alan_directory_for_dev_uses_dev_runtime_namespace() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");

        create_alan_directory_for_channel(&alan_dir, alan_runtime::InstallChannel::Dev).unwrap();

        assert!(alan_dir.join("runtime/dev/sessions").exists());
        assert!(alan_dir.join("runtime/dev/memory/MEMORY.md").exists());
        assert!(!alan_dir.join("sessions").exists());
        assert!(!alan_dir.join("memory").exists());
    }

    #[test]
    fn test_create_alan_directory_stable_preserves_existing_legacy_memory() {
        let tmp = TempDir::new().unwrap();
        let alan_dir = tmp.path().join(".alan");
        std::fs::create_dir_all(alan_dir.join("memory")).unwrap();

        create_alan_directory_for_channel(&alan_dir, alan_runtime::InstallChannel::Stable).unwrap();

        assert!(alan_dir.join("memory/MEMORY.md").exists());
        assert!(!alan_dir.join("runtime/stable/memory/MEMORY.md").exists());
    }
}
