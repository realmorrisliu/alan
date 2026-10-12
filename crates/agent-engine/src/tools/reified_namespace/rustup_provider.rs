//! Effective executable lookup shared by Rustup inspection and prelaunch revalidation.
use super::*;
use std::os::unix::ffi::OsStrExt;

pub(super) fn resolve(
    executable: &str,
    path: &str,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<Option<PathBuf>, String> {
    resolve_lookup(executable, path, proxies, false)
}

pub(super) fn resolve_projected(
    executable: &str,
    path: &str,
    mounts: &[ReifiedExecutionSubstrateMount],
) -> Result<Option<PathBuf>, String> {
    resolve_lookup(executable, path, mounts, true)
}

fn resolve_lookup(
    executable: &str,
    path: &str,
    projections: &[ReifiedExecutionSubstrateMount],
    authoritative: bool,
) -> Result<Option<PathBuf>, String> {
    let candidates = if executable.contains('/') {
        if !Path::new(executable).is_absolute() || contains_parent_component(Path::new(executable))
        {
            return Err(
                "relative or parent-traversing Rust executable needs an explicit absolute path"
                    .into(),
            );
        }
        vec![PathBuf::from(executable)]
    } else {
        std::env::split_paths(path)
            .map(|entry| entry.join(executable))
            .collect()
    };
    for candidate in candidates {
        let substrate = if authoritative {
            Vec::new()
        } else {
            super::super::default_execution_substrate()
        };
        let mount = projections
            .iter()
            .chain(&substrate)
            .find(|mount| candidate.starts_with(&mount.namespace_path));
        if authoritative && mount.is_none() {
            continue;
        }
        let namespace_root = mount.map_or(candidate.parent().unwrap(), |m| &m.namespace_path);
        let root = canonicalize_existing_host_path(mount.map_or(namespace_root, |m| &m.host_path));
        if mount.is_none() && root != namespace_root {
            return Err("Rust provider directory alias needs an explicit projection".into());
        }
        let source = root.join(candidate.strip_prefix(namespace_root).unwrap());
        let metadata = match std::fs::metadata(&source) {
            Ok(metadata) => metadata,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::NotADirectory
                        | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                continue;
            }
            Err(error) => return Err(format!("Rust command provider unavailable: {error}")),
        };
        if !metadata.is_file() {
            continue;
        }
        let name = std::ffi::CString::new(source.as_os_str().as_bytes())
            .map_err(|_| "Rust executable path contains NUL")?;
        // SAFETY: the owned CString remains valid throughout this synchronous access check.
        if unsafe { libc::access(name.as_ptr(), libc::X_OK) } != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound
                    | std::io::ErrorKind::NotADirectory
                    | std::io::ErrorKind::PermissionDenied
            ) {
                continue;
            }
            return Err(format!("Rust executable access unavailable: {error}"));
        }
        // shortcut: cross-root executable aliases need an explicit supported projection.
        if let Err(error) =
            super::super::toolchain::validate_directory_aliases(&source, &root, namespace_root)
        {
            let canonical = std::fs::canonicalize(&source).map_err(|_| error.clone())?;
            let system_roots = [
                Path::new("/usr"),
                Path::new("/bin"),
                Path::new("/lib"),
                Path::new("/etc"),
            ];
            let native_system_alias = [Path::new("/usr/bin"), Path::new("/bin")]
                .contains(&candidate.parent().unwrap())
                && canonical.starts_with("/usr")
                && projections
                    .iter()
                    .any(|mount| canonical.starts_with(&mount.namespace_path))
                && std::fs::canonicalize(namespace_root).is_ok_and(|native| native == root)
                && projections
                    .iter()
                    .filter(|mount| {
                        system_roots
                            .iter()
                            .any(|system| mount.namespace_path.starts_with(system))
                    })
                    .all(|mount| {
                        mount.namespace_path == mount.host_path
                            || std::fs::canonicalize(&mount.namespace_path)
                                .is_ok_and(|native| native == mount.host_path)
                    });
            // System cross-root aliases require unchanged system projections along their route.
            if !authoritative || !native_system_alias {
                return Err(error);
            }
        }
        return std::fs::canonicalize(&source)
            .map(Some)
            .map_err(|error| format!("Rust provider canonicalization failed: {error}"));
    }
    Ok(None)
}

pub(super) fn is_proxy(
    provider: Option<&Path>,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<bool, String> {
    let Some(provider) = provider else {
        return Ok(false);
    };
    let identity = std::fs::metadata(provider).map_err(|error| error.to_string())?;
    for proxy in proxies {
        let rustup =
            std::fs::metadata(proxy.host_path.join("rustup")).map_err(|error| error.to_string())?;
        if identity.dev() == rustup.dev() && identity.ino() == rustup.ino() {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn snapshot(
    executable: &str,
    path: &str,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<Option<(PathBuf, String, bool)>, String> {
    resolve(executable, path, proxies)?
        .map(|provider| {
            let digest = executable_digest(&provider)?;
            let proxy = is_proxy(Some(&provider), proxies)?;
            Ok((provider, digest, proxy))
        })
        .transpose()
}

pub(super) fn validate_shell_lookup(
    input: &ReifiedNamespacePlanInput,
    commands: &[Vec<String>],
) -> Result<(), String> {
    use crate::tools::sandbox::command_and_args;
    use crate::tools::sandbox::command_wrappers::{
        inspect_shell_lookup, shell_wrapper_inline_script,
    };
    let rust =
        |words: &[String]| command_and_args(words).is_some_and(|(name, _)| PROXIES.contains(&name));
    let changes_lookup = |words: &[String]| {
        command_and_args(words)
            .is_some_and(|(name, _)| matches!(name, "alias" | "unalias" | "hash"))
    };
    let mut rust_request = false;
    let mut changed = false;
    let mut scripts = shell_wrapper_inline_script(&input.argv)
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect::<Vec<_>>();
    if scripts.is_empty() {
        rust_request = commands.iter().any(|words| rust(words));
        changed = commands.iter().any(|words| changes_lookup(words));
    }
    let mut index = 0;
    while index < scripts.len() {
        let (modifies, actual) =
            inspect_shell_lookup(&scripts[index]).map_err(|e| e.to_string())?;
        changed |= modifies;
        for words in actual {
            rust_request |= rust(&words);
            if let Some(inner) = shell_wrapper_inline_script(&words).map_err(|e| e.to_string())? {
                scripts.push(inner);
            }
        }
        index += 1;
    }
    if rust_request && (changed || scripts.len() > 1) {
        return Err("shell syntax/function/alias/hash changes and nested shells cannot preserve inspected Rust lookup".into());
    }
    Ok(())
}
