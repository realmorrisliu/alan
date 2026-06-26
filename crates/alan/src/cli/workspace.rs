//! `alan workspace` — workspace management subcommands.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::registry::WorkspaceRegistry;
use crate::registry::normalize_workspace_root_path;

/// Shorten a path by replacing the home directory prefix with ~.
fn shorten_path(path: &Path, home: Option<&Path>) -> String {
    let path_display = path.display().to_string();

    if let Some(home_dir) = home
        && let Ok(stripped) = path.strip_prefix(home_dir)
    {
        return format!("~/{}", stripped.display());
    }

    path_display
}

fn workspace_alan_dir(path: &Path) -> PathBuf {
    if path
        .file_name()
        .map(|name| name == std::ffi::OsStr::new(".alan"))
        .unwrap_or(false)
    {
        path.to_path_buf()
    } else {
        path.join(".alan")
    }
}

fn current_install_channel() -> alan_agent_engine::InstallChannel {
    alan_agent_engine::InstallChannel::detect_current()
}

fn count_rollout_jsonl_files(sessions_dir: &Path) -> usize {
    if !sessions_dir.exists() {
        return 0;
    }

    let mut count = 0usize;
    let mut dirs = vec![sessions_dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(kind) => kind,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }

            let is_jsonl = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("jsonl"))
                .unwrap_or(false);
            if is_jsonl {
                count += 1;
            }
        }
    }

    count
}

/// List all registered workspaces.
pub fn list_workspaces() -> Result<()> {
    let registry = WorkspaceRegistry::load()?;
    let workspaces = registry.list();

    if workspaces.is_empty() {
        println!("No workspaces registered.");
        println!("Run `alan init` in a project directory to create one.");
        return Ok(());
    }

    // Print header
    println!("{:<10} {:<20} PATH", "ID", "ALIAS");
    println!("{}", "-".repeat(60));

    let home = dirs::home_dir();
    for ws in workspaces {
        let path_short = shorten_path(&ws.path, home.as_deref());
        println!("{:<10} {:<20} {}", ws.id, ws.alias, path_short);
    }

    println!("\n{} workspace(s)", workspaces.len());
    Ok(())
}

/// Register an existing workspace directory.
pub fn add_workspace(path: PathBuf, name: Option<String>) -> Result<()> {
    add_workspace_with_registry_path(path, name, None)
}

fn add_workspace_with_registry_path(
    path: PathBuf,
    name: Option<String>,
    registry_path: Option<&Path>,
) -> Result<()> {
    let canonical = std::fs::canonicalize(&path)
        .with_context(|| format!("Cannot resolve path: {}", path.display()))?;
    let workspace_root = normalize_workspace_root_path(&canonical);

    // Verify .alan directory exists
    if !workspace_alan_dir(&workspace_root).exists() {
        anyhow::bail!(
            "Directory {} does not contain an .alan/ directory.\nRun `alan init --path {}` first.",
            workspace_root.display(),
            workspace_root.display(),
        );
    }

    let mut registry = match registry_path {
        Some(path) => WorkspaceRegistry::load_from_path(path)?,
        None => WorkspaceRegistry::load()?,
    };
    let entry = registry.register(&workspace_root, name)?;
    match registry_path {
        Some(path) => registry.save_to_path(path)?,
        None => registry.save()?,
    }

    println!("✅ Workspace registered: {}", workspace_root.display());
    println!("   Alias: {}", entry.alias);
    println!("   ID:    {}", entry.id);
    Ok(())
}

/// Unregister a workspace (does not delete files).
pub fn remove_workspace(workspace: &str) -> Result<()> {
    let mut registry = WorkspaceRegistry::load()?;
    let removed = registry.unregister(workspace)?;
    registry.save()?;

    println!("Workspace '{}' unregistered.", removed.alias);
    println!("Files at {} were not deleted.", removed.path.display());
    Ok(())
}

/// Show workspace details.
pub fn workspace_info(workspace: &str) -> Result<()> {
    let registry = WorkspaceRegistry::load()?;
    let entry = registry
        .find(workspace)
        .with_context(|| format!("Workspace not found: '{}'", workspace))?;

    println!("Workspace: {}", entry.alias);
    println!("  ID:         {}", entry.id);
    println!("  Path:       {}", entry.path.display());
    println!("  Created:    {}", entry.created_at);

    // Check if .alan directory exists
    let alan_dir = workspace_alan_dir(&entry.path);
    if alan_dir.exists() {
        println!("  Status:     ✅ initialized");

        // Check for sessions
        let sessions_dir = alan_agent_engine::workspace_sessions_dir_for_channel_from_alan_dir(
            &alan_dir,
            current_install_channel(),
        );
        if sessions_dir.exists() {
            let count = count_rollout_jsonl_files(&sessions_dir);
            println!("  Sessions:   {}", count);
        }
    } else {
        println!("  Status:     ⚠️  .alan/ directory missing");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::WorkspaceRegistry;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_shorten_path_with_home_prefix() {
        let home = PathBuf::from("/Users/test");
        let path = PathBuf::from("/Users/test/projects/myapp");

        let result = shorten_path(&path, Some(&home));
        assert_eq!(result, "~/projects/myapp");
    }

    #[test]
    fn test_shorten_path_without_home_prefix() {
        let home = PathBuf::from("/Users/test");
        let path = PathBuf::from("/opt/projects/myapp");

        let result = shorten_path(&path, Some(&home));
        assert_eq!(result, "/opt/projects/myapp");
    }

    #[test]
    fn test_shorten_path_no_home() {
        let path = PathBuf::from("/Users/test/projects/myapp");

        let result = shorten_path(&path, None);
        assert_eq!(result, "/Users/test/projects/myapp");
    }

    #[test]
    fn test_shorten_path_exact_home() {
        let home = PathBuf::from("/Users/test");
        let path = PathBuf::from("/Users/test");

        let result = shorten_path(&path, Some(&home));
        assert_eq!(result, "~/");
    }

    #[test]
    fn test_shorten_path_with_trailing_slash() {
        let home = PathBuf::from("/Users/test/");
        let path = PathBuf::from("/Users/test/projects/myapp");

        let result = shorten_path(&path, Some(&home));
        // strip_prefix should still work correctly
        assert_eq!(result, "~/projects/myapp");
    }

    #[test]
    fn test_shorten_path_relative() {
        let home = PathBuf::from("/Users/test");
        let path = PathBuf::from("./relative/path");

        let result = shorten_path(&path, Some(&home));
        // Relative paths don't start with home, so should remain unchanged
        assert_eq!(result, "./relative/path");
    }

    #[test]
    fn test_add_workspace_accepts_dot_alan_path_and_registers_parent_root() {
        let tmp = TempDir::new().unwrap();
        let registry_path = tmp.path().join("registry.json");

        let workspace_root = tmp.path().join("repo");
        let alan_dir = workspace_root.join(".alan");
        std::fs::create_dir_all(&alan_dir).unwrap();

        let result = add_workspace_with_registry_path(
            alan_dir.clone(),
            Some("repo".to_string()),
            Some(&registry_path),
        );
        assert!(result.is_ok());

        let registry = WorkspaceRegistry::load_from_path(&registry_path).unwrap();
        let entry = registry.find("repo").unwrap();
        assert_eq!(entry.path, std::fs::canonicalize(&workspace_root).unwrap());
    }

    #[test]
    fn test_count_rollout_jsonl_files_includes_nested_sessions() {
        let temp = TempDir::new().unwrap();
        let sessions_dir = temp.path().join("sessions");
        let nested = sessions_dir.join("2026").join("02").join("28");
        std::fs::create_dir_all(&nested).unwrap();

        std::fs::write(sessions_dir.join("top.jsonl"), "{}\n").unwrap();
        std::fs::write(nested.join("nested.jsonl"), "{}\n").unwrap();
        std::fs::write(nested.join("ignore.txt"), "x").unwrap();

        assert_eq!(count_rollout_jsonl_files(&sessions_dir), 2);
    }
}
