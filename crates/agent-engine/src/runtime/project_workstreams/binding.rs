//! Explicit project directory bindings stored in System Store.
//!
//! Bindings map an explicitly chosen canonical directory path to an opaque
//! `project_key` under `services/projects/<opaque_project_key>/binding.json`.
//! Foreign checkouts, matching Git remotes, and directory basenames are never
//! inferred or auto-bound.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::{
    atomic_write_json, ensure_projects_subdir, ensure_real_dir, validate_identifier,
    validate_projects_root, verify_existing_regular_file, verify_existing_subdir,
};

pub const PROJECT_BINDING_VERSION: u16 = 1;
const BINDING_FILE_NAME: &str = "binding.json";

const fn default_binding_version() -> u16 {
    PROJECT_BINDING_VERSION
}

/// Explicit binding between a canonical host project directory and its opaque System Store key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectBinding {
    #[serde(default = "default_binding_version")]
    pub version: u16,
    pub project_key: String,
    pub canonical_root_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_ref: Option<String>,
    pub created_at: String,
}

/// Validate an opaque project key segment.
pub fn validate_project_key(project_key: &str) -> Result<()> {
    validate_identifier("project_key", project_key)
}

/// Return `<projects_root>/<project_key>`.
pub fn project_dir(projects_root: &Path, project_key: &str) -> Result<PathBuf> {
    validate_projects_root(projects_root)?;
    validate_project_key(project_key)?;
    Ok(projects_root.join(project_key))
}

/// Return `<projects_root>/<project_key>/binding.json`.
pub fn binding_path(projects_root: &Path, project_key: &str) -> Result<PathBuf> {
    Ok(project_dir(projects_root, project_key)?.join(BINDING_FILE_NAME))
}

/// Explicitly bind `dir` under `projects_root`, returning its `ProjectBinding`.
///
/// Re-binding an already-bound canonical directory preserves its `project_key`
/// and `created_at`, updating `grant_ref` only when `Some(grant_ref)` is passed.
/// Sibling checkouts, Git remotes, and directory basenames are never matched.
pub fn bind_project_dir(
    projects_root: &Path,
    dir: &Path,
    grant_ref: Option<String>,
) -> Result<ProjectBinding> {
    validate_projects_root(projects_root)?;
    let canonical_dir = dunce::canonicalize(dir)
        .with_context(|| format!("canonicalize project directory {}", dir.display()))?;
    let meta = fs::metadata(&canonical_dir)
        .with_context(|| format!("inspect project directory {}", canonical_dir.display()))?;
    ensure!(
        meta.is_dir(),
        "project binding target must be a directory: {}",
        canonical_dir.display()
    );

    let normalized_grant = grant_ref
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if let Some(mut existing) = resolve_bound_canonical_dir(projects_root, &canonical_dir)? {
        if let Some(new_grant) = normalized_grant
            && existing.grant_ref.as_deref() != Some(new_grant.as_str())
        {
            existing.grant_ref = Some(new_grant);
            let dir_path = ensure_projects_subdir(projects_root, &[&existing.project_key])?;
            let path = dir_path.join(BINDING_FILE_NAME);
            atomic_write_json(&dir_path, &path, &existing)?;
        }
        return Ok(existing);
    }

    let project_key = format!("proj-{}", uuid::Uuid::new_v4().simple());
    let binding = ProjectBinding {
        version: PROJECT_BINDING_VERSION,
        project_key: project_key.clone(),
        canonical_root_path: canonical_dir,
        grant_ref: normalized_grant,
        created_at: Utc::now().to_rfc3339(),
    };

    let dir_path = ensure_projects_subdir(projects_root, &[&project_key])?;
    let path = dir_path.join(BINDING_FILE_NAME);
    atomic_write_json(&dir_path, &path, &binding)?;
    Ok(binding)
}

/// Load a specific project binding by `project_key` if present.
pub fn load_project_binding(
    projects_root: &Path,
    project_key: &str,
) -> Result<Option<ProjectBinding>> {
    validate_project_key(project_key)?;
    let Some(dir_path) = verify_existing_subdir(projects_root, &[project_key])? else {
        return Ok(None);
    };
    let path = dir_path.join(BINDING_FILE_NAME);
    if !verify_existing_regular_file(&dir_path, &path)? {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
    let binding = serde_json::from_slice::<ProjectBinding>(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    validate_project_key(&binding.project_key)?;
    ensure!(
        binding.project_key == project_key,
        "binding key mismatch in {}: expected `{project_key}`, found `{}`",
        path.display(),
        binding.project_key
    );
    Ok(Some(binding))
}

/// List all valid explicit project bindings stored under `projects_root`, sorted by `project_key`.
pub fn list_project_bindings(projects_root: &Path) -> Result<Vec<ProjectBinding>> {
    validate_projects_root(projects_root)?;
    match fs::symlink_metadata(projects_root) {
        Ok(_) => ensure_real_dir(projects_root, "projects root")?,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("inspect projects root {}", projects_root.display()));
        }
    }

    let mut bindings = Vec::new();
    for entry in fs::read_dir(projects_root)
        .with_context(|| format!("read projects root {}", projects_root.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_project_key(&name).is_err() {
            continue;
        }
        if let Some(binding) = load_project_binding(projects_root, &name)? {
            bindings.push(binding);
        }
    }
    bindings.sort_by(|a, b| a.project_key.cmp(&b.project_key));
    Ok(bindings)
}

/// Resolve an explicit `ProjectBinding` matching `dir` by canonical root path ONLY.
///
/// Never infers binding from Git remotes, directory basenames, parent directories,
/// or foreign checkouts. Returns `Ok(None)` when `dir` is unbound or missing.
pub fn resolve_bound_project(projects_root: &Path, dir: &Path) -> Result<Option<ProjectBinding>> {
    validate_projects_root(projects_root)?;
    let canonical_dir = match dunce::canonicalize(dir) {
        Ok(path) => path,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("canonicalize candidate project dir {}", dir.display()));
        }
    };
    if !fs::metadata(&canonical_dir).is_ok_and(|meta| meta.is_dir()) {
        return Ok(None);
    }
    resolve_bound_canonical_dir(projects_root, &canonical_dir)
}

fn resolve_bound_canonical_dir(
    projects_root: &Path,
    canonical_dir: &Path,
) -> Result<Option<ProjectBinding>> {
    for binding in list_project_bindings(projects_root)? {
        if binding.canonical_root_path == canonical_dir {
            return Ok(Some(binding));
        }
    }
    Ok(None)
}
