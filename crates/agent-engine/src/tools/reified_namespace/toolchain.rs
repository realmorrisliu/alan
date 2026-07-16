//! User toolchain visibility checks for Linux reified namespace selection.

#[cfg(any(target_os = "linux", all(test, unix)))]
use std::os::unix::fs::PermissionsExt;
#[cfg(any(target_os = "linux", all(test, unix)))]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use super::super::sandbox_backend::LinuxReificationCapability;
use super::LINUX_REIFIED_COMMAND_PATH;
#[cfg(any(target_os = "linux", all(test, unix)))]
use super::plan::canonicalize_existing_host_path;
#[cfg(test)]
use super::plan::default_execution_substrate;

/// Smoke-check that selecting reified mode will not hide user PATH toolchains.
#[cfg(target_os = "linux")]
pub(crate) fn smoke_linux_reified_namespace_user_path() -> LinuxReificationCapability {
    match reified_namespace_user_path_unavailable_reason(std::env::var_os("PATH")) {
        Some(reason) => LinuxReificationCapability::unavailable(reason),
        None => LinuxReificationCapability::available(),
    }
}

#[cfg(target_os = "linux")]
fn reified_namespace_user_path_unavailable_reason(
    path: Option<std::ffi::OsString>,
) -> Option<String> {
    let visible_roots = reified_command_path_roots();
    reified_namespace_user_path_unavailable_reason_with_roots(
        path,
        &visible_roots,
        std::ffi::OsString::from(LINUX_REIFIED_COMMAND_PATH),
    )
}

#[cfg(target_os = "linux")]
fn reified_command_path_roots() -> Vec<PathBuf> {
    std::env::split_paths(LINUX_REIFIED_COMMAND_PATH)
        .map(|path| canonicalize_existing_host_path(&path))
        .collect()
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn reified_namespace_user_path_unavailable_reason_with_roots(
    path: Option<std::ffi::OsString>,
    visible_roots: &[PathBuf],
    reified_command_path: std::ffi::OsString,
) -> Option<String> {
    let Some(path) = path else {
        return Some(
            "current PATH is unset; preserve actual PATH/order before selecting \
             linux_reified_namespace"
                .to_string(),
        );
    };
    let visible_roots = visible_roots
        .iter()
        .map(|root| canonicalize_existing_host_path(root))
        .collect::<Vec<_>>();
    let mut unsupported = Vec::new();
    let mut current_executable_entries = Vec::new();

    for entry in std::env::split_paths(&path) {
        if entry.as_os_str().is_empty() {
            return Some(
                "current PATH contains an empty component for current-directory lookup; preserve \
                 actual PATH/order before selecting linux_reified_namespace"
                    .to_string(),
            );
        }
        if !entry.is_absolute() {
            unsupported.push(format!("relative PATH entry {}", entry.display()));
            continue;
        }

        let entry = canonicalize_existing_host_path(&entry);
        if !path_directory_has_executables(&entry) {
            continue;
        }
        if visible_roots
            .iter()
            .any(|root| entry == *root || entry.starts_with(root))
        {
            push_unique_path(&mut current_executable_entries, entry);
            continue;
        }

        unsupported.push(entry.display().to_string());
        if unsupported.len() >= 3 {
            break;
        }
    }

    if !unsupported.is_empty() {
        return Some(format!(
            "current PATH has executable entries outside the reified execution substrate: {}; \
             preserve user PATH/toolchain mounts before selecting linux_reified_namespace",
            unsupported.join(", ")
        ));
    }

    let reified_executable_entries = executable_path_entries(&reified_command_path);
    if current_executable_entries != reified_executable_entries {
        return Some(format!(
            "current PATH executable entry order differs from the reified command PATH: \
             current=[{}], reified=[{}]; preserve actual PATH/order before selecting \
             linux_reified_namespace",
            format_path_entries(&current_executable_entries),
            format_path_entries(&reified_executable_entries)
        ));
    }

    None
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn path_directory_has_executables(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return true;
    };
    entries.filter_map(Result::ok).any(|entry| {
        std::fs::metadata(entry.path())
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn executable_path_entries(path: &std::ffi::OsString) -> Vec<PathBuf> {
    let mut entries = Vec::new();
    for entry in std::env::split_paths(path) {
        if entry.as_os_str().is_empty() || !entry.is_absolute() {
            continue;
        }
        let entry = canonicalize_existing_host_path(&entry);
        if path_directory_has_executables(&entry) {
            push_unique_path(&mut entries, entry);
        }
    }
    entries
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn push_unique_path(entries: &mut Vec<PathBuf>, entry: PathBuf) {
    if !entries.contains(&entry) {
        entries.push(entry);
    }
}

#[cfg(any(target_os = "linux", all(test, unix)))]
fn format_path_entries(entries: &[PathBuf]) -> String {
    if entries.is_empty() {
        return "<none>".to_string();
    }
    entries
        .iter()
        .map(|entry| entry.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
#[path = "toolchain_tests.rs"]
mod tests;
