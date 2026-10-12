//! Effective executable lookup shared by Rustup inspection and prelaunch revalidation.
use super::*;
use std::os::unix::ffi::OsStrExt;

pub(super) fn resolve(
    executable: &str,
    path: &str,
    proxies: &[ReifiedExecutionSubstrateMount],
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
        let substrate = super::super::default_execution_substrate();
        let mount = proxies
            .iter()
            .chain(&substrate)
            .find(|mount| candidate.starts_with(&mount.namespace_path));
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
        super::super::toolchain::validate_directory_aliases(&source, &root, namespace_root)?;
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
