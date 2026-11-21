//! Per-export path policy enforced inside `HostDirFs`.
//!
//! Default exports (such as `/memory`, `/lib/agents/root`, and Quartermaster
//! package trees) use [`HostDirPolicy::default`], which imposes no extra path
//! restrictions. Delegated Host Mounts construct a [`HostDirPolicy`] via
//! [`HostDirPolicy::for_host_mount`] so sensitive read exclusions and protected
//! write subpaths stay enforced at the Host file-server boundary across every
//! aP consumer.

use std::ffi::CStr;
use std::os::unix::io::IntoRawFd;
use std::path::{Component, Path, PathBuf};

use alan_ap::ErrorCode;

/// Read-denylist and protected-subpath write policy for one `HostDirFs` export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostDirPolicy {
    read_denied_subpaths: Vec<Vec<String>>,
    write_protected_components: Vec<String>,
    root_write_protected: bool,
}

impl HostDirPolicy {
    /// Derive a `HostDirPolicy` for one delegated Host Mount rooted at `host_root`.
    ///
    /// - Any entry in `read_denylist` at or above `host_root` denies the entire
    ///   export (`vec![]` prefix); entries inside `host_root` are stored as
    ///   mount-relative component prefixes; entries outside `host_root` are ignored.
    /// - Any path containing a component in `write_protected_components` (or any
    ///   path inside an export whose `host_root` itself contains one) is blocked
    ///   from write, create, and remove operations while remaining readable unless
    ///   also covered by `read_denylist`.
    pub fn for_host_mount(
        host_root: &Path,
        read_denylist: &[PathBuf],
        write_protected_components: &[&str],
    ) -> Self {
        let normalized_root = lexically_normalize(host_root);
        let canonical_root = resolve_with_existing_parents(&normalized_root);

        let protected: Vec<String> = write_protected_components
            .iter()
            .filter(|component| !component.is_empty())
            .map(|component| (*component).to_string())
            .collect();
        let root_write_protected = path_has_protected_component(&canonical_root, &protected)
            || path_has_protected_component(&normalized_root, &protected);

        let mut read_denied_subpaths = Vec::new();
        for deny_path in read_denylist {
            let normalized_deny = lexically_normalize(deny_path);
            let resolved_deny = resolve_with_existing_parents(&normalized_deny);

            let denies_root = strip_prefix_components(&canonical_root, &resolved_deny).is_some()
                || strip_prefix_components(&normalized_root, &normalized_deny).is_some();
            if denies_root {
                if !read_denied_subpaths.iter().any(Vec::is_empty) {
                    read_denied_subpaths.push(Vec::new());
                }
                continue;
            }

            if let Some(subpath) = strip_prefix_components(&resolved_deny, &canonical_root)
                .or_else(|| strip_prefix_components(&normalized_deny, &normalized_root))
                && !read_denied_subpaths.contains(&subpath)
            {
                read_denied_subpaths.push(subpath);
            }
        }

        Self {
            read_denied_subpaths,
            write_protected_components: protected,
            root_write_protected,
        }
    }

    /// Whether `rel` falls under any read-denied subpath.
    pub(crate) fn is_read_denied(&self, rel: &[String]) -> bool {
        self.read_denied_subpaths.iter().any(|prefix| {
            rel.len() >= prefix.len()
                && prefix
                    .iter()
                    .zip(rel)
                    .all(|(denied, actual)| component_matches(denied, actual))
        })
    }

    /// Whether `[parent_rel..., child_name]` falls under any read-denied subpath.
    pub(crate) fn is_child_read_denied(&self, parent_rel: &[String], child_name: &str) -> bool {
        self.read_denied_subpaths.iter().any(|prefix| {
            if prefix.len() > parent_rel.len() + 1 {
                return false;
            }
            prefix.iter().enumerate().all(|(idx, denied)| {
                let candidate = if idx < parent_rel.len() {
                    parent_rel[idx].as_str()
                } else {
                    child_name
                };
                component_matches(denied, candidate)
            })
        })
    }

    /// Whether writes, creates, or removes at `rel` are blocked by policy.
    pub(crate) fn is_write_protected(&self, rel: &[String]) -> bool {
        self.root_write_protected
            || self.is_read_denied(rel)
            || rel
                .iter()
                .any(|segment| self.is_protected_component(segment))
    }

    /// Whether creating `child_name` under `parent_rel` is blocked by policy.
    pub(crate) fn is_child_write_protected(&self, parent_rel: &[String], child_name: &str) -> bool {
        self.root_write_protected
            || self.is_child_read_denied(parent_rel, child_name)
            || self.is_protected_component(child_name)
            || parent_rel
                .iter()
                .any(|segment| self.is_protected_component(segment))
    }

    pub(crate) fn check_read(&self, rel: &[String]) -> Result<(), ErrorCode> {
        if self.is_read_denied(rel) {
            Err(ErrorCode::NoAccess)
        } else {
            Ok(())
        }
    }

    pub(crate) fn check_write(&self, rel: &[String]) -> Result<(), ErrorCode> {
        if self.is_write_protected(rel) {
            Err(ErrorCode::NoAccess)
        } else {
            Ok(())
        }
    }

    pub(crate) fn check_create(
        &self,
        parent_rel: &[String],
        child_name: &str,
    ) -> Result<(), ErrorCode> {
        if self.is_child_write_protected(parent_rel, child_name) {
            Err(ErrorCode::NoAccess)
        } else {
            Ok(())
        }
    }

    fn is_protected_component(&self, segment: &str) -> bool {
        self.write_protected_components
            .iter()
            .any(|protected| component_matches(protected, segment))
    }
}

pub(crate) fn directory_listing(
    file: std::fs::File,
    parent_rel: &[String],
    policy: &HostDirPolicy,
) -> Result<Vec<u8>, ErrorCode> {
    let fd = file.into_raw_fd();
    // SAFETY: fd is an owned open directory descriptor. On success fdopendir takes ownership.
    let dir = unsafe { libc::fdopendir(fd) };
    if dir.is_null() {
        // SAFETY: fdopendir failed and therefore did not take ownership of fd;
        // close it exactly once.
        unsafe {
            libc::close(fd);
        }
        return Err(ErrorCode::Io);
    }
    let mut names = Vec::new();
    loop {
        // SAFETY: dir is a non-null DIR pointer owned by this function until closedir below.
        let entry = unsafe { libc::readdir(dir) };
        if entry.is_null() {
            break;
        }
        // SAFETY: readdir returned a live dirent whose d_name field is NUL-terminated and remains
        // valid until the next readdir call.
        let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        let entry_name = String::from_utf8_lossy(name.to_bytes()).to_string();
        if policy.is_child_read_denied(parent_rel, &entry_name) {
            continue;
        }
        names.push(entry_name);
    }
    // SAFETY: dir is still owned here and has not previously been passed to closedir.
    let close_result = unsafe { libc::closedir(dir) };
    if close_result != 0 {
        return Err(ErrorCode::Io);
    }
    names.sort();
    Ok(names.join("\n").into_bytes())
}

fn component_matches(expected: &str, actual: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        expected.eq_ignore_ascii_case(actual)
    }
    #[cfg(not(target_os = "macos"))]
    {
        expected == actual
    }
}

fn path_has_protected_component(path: &Path, protected: &[String]) -> bool {
    path.components().any(|component| {
        let Component::Normal(part) = component else {
            return false;
        };
        part.to_str().is_some_and(|candidate| {
            protected
                .iter()
                .any(|item| component_matches(item, candidate))
        })
    })
}

fn lexically_normalize(path: &Path) -> PathBuf {
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

fn resolve_with_existing_parents(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    let mut tail = Vec::new();
    let mut current = path;
    while let Some(parent) = current.parent() {
        if let Some(name) = current.file_name() {
            tail.push(name.to_os_string());
        }
        if let Ok(mut canonical_parent) = std::fs::canonicalize(parent) {
            for part in tail.into_iter().rev() {
                canonical_parent.push(part);
            }
            return canonical_parent;
        }
        current = parent;
    }
    path.to_path_buf()
}

fn absolute_normal_components(path: &Path) -> Option<Vec<String>> {
    if !path.is_absolute() {
        return None;
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(part) => components.push(part.to_str()?.to_string()),
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(components)
}

fn strip_prefix_components(path: &Path, base: &Path) -> Option<Vec<String>> {
    let path_parts = absolute_normal_components(path)?;
    let base_parts = absolute_normal_components(base)?;
    if path_parts.len() < base_parts.len() {
        return None;
    }
    if base_parts
        .iter()
        .zip(&path_parts)
        .all(|(b, p)| component_matches(b, p))
    {
        Some(path_parts[base_parts.len()..].to_vec())
    } else {
        None
    }
}
