//! User toolchain visibility checks for Linux reified namespace selection.

#[cfg(target_os = "linux")]
use super::super::sandbox_backend::LinuxReificationCapability;
#[cfg(test)]
use super::LINUX_REIFIED_COMMAND_PATH;
#[cfg(any(target_os = "linux", all(test, unix)))]
use super::plan::{canonicalize_existing_host_path, contains_parent_component};
#[cfg(any(target_os = "linux", all(test, unix)))]
use super::{ReifiedExecutionSubstrateMount, default_execution_substrate};

/// Resolve the current supported command PATH without changing its order or spelling.
#[cfg(target_os = "linux")]
pub(crate) fn current_linux_command_path() -> Result<String, String> {
    validate_linux_command_path(std::env::var_os("PATH"), &default_execution_substrate())
}

/// Startup selection and per-command construction share the same validation.
#[cfg(target_os = "linux")]
pub(crate) fn smoke_linux_reified_namespace_user_path() -> LinuxReificationCapability {
    match current_linux_command_path() {
        Ok(_) => LinuxReificationCapability::available(),
        Err(reason) => LinuxReificationCapability::unavailable(reason),
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(super) fn validate_linux_command_path(
    path: Option<std::ffi::OsString>,
    substrate: &[ReifiedExecutionSubstrateMount],
) -> Result<String, String> {
    let path = path.ok_or_else(|| "current PATH is unset".to_string())?;
    let path = path
        .into_string()
        .map_err(|_| "current PATH is not UTF-8".to_string())?;
    if path.contains('\0') {
        return Err("current PATH contains NUL".to_string());
    }
    for entry in std::env::split_paths(&path) {
        if entry.as_os_str().is_empty() {
            return Err(
                "current PATH contains an empty component for current-directory lookup".to_string(),
            );
        }
        if !entry.is_absolute() || contains_parent_component(&entry) {
            return Err(format!("unsafe PATH entry {}", entry.display()));
        }
        let Some(mount) = substrate
            .iter()
            .find(|mount| entry.starts_with(&mount.namespace_path))
        else {
            return Err(format!(
                "PATH entry outside the reified execution substrate: {}",
                entry.display()
            ));
        };
        let root = canonicalize_existing_host_path(&mount.host_path);
        let relative = entry
            .strip_prefix(&mount.namespace_path)
            .expect("matched namespace prefix");
        let candidate = root.join(relative);
        validate_directory_aliases(&candidate, &root, &mount.namespace_path)?;
        let source = canonicalize_existing_host_path(&candidate);
        if !source.starts_with(&root) {
            return Err(format!(
                "PATH entry escapes execution substrate: {}",
                entry.display()
            ));
        }
        match std::fs::metadata(&source) {
            Ok(metadata) if metadata.is_dir() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if candidate
                    .ancestors()
                    .take_while(|path| path.starts_with(&root))
                    .any(|path| {
                        std::fs::symlink_metadata(path)
                            .is_ok_and(|metadata| metadata.file_type().is_symlink())
                    })
                {
                    return Err(format!(
                        "PATH entry contains an unresolved alias: {}",
                        entry.display()
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "PATH entry is not an accessible directory: {}",
                    entry.display()
                ));
            }
        }
    }
    Ok(path)
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn validate_directory_aliases(
    candidate: &std::path::Path,
    root: &std::path::Path,
    namespace_root: &std::path::Path,
) -> Result<(), String> {
    let mut path = candidate.components().collect::<std::path::PathBuf>();
    for _ in 0..40 {
        let alias = path
            .ancestors()
            .take_while(|ancestor| *ancestor != root && ancestor.starts_with(root))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .find_map(|ancestor| {
                std::fs::read_link(ancestor)
                    .ok()
                    .map(|target| (ancestor, target))
            });
        let Some((alias, target)) = alias else {
            return Ok(());
        };
        // Absolute aliases may resolve differently after the Host root is remapped.
        if target.is_absolute() && (root != namespace_root || !target.starts_with(root)) {
            return Err(format!(
                "absolute PATH alias in remapped substrate: {}",
                alias.display()
            ));
        }
        if contains_parent_component(&target) {
            return Err(format!("unsafe PATH alias: {}", alias.display()));
        }
        let target = if target.is_absolute() {
            target
        } else {
            alias
                .parent()
                .expect("alias below substrate root")
                .join(target)
        };
        path = target
            .join(path.strip_prefix(alias).expect("alias ancestor"))
            .components()
            .collect();
    }
    Err(format!(
        "PATH alias resolution limit exceeded: {}",
        candidate.display()
    ))
}

#[cfg(test)]
#[path = "toolchain_tests.rs"]
mod tests;
