//! Installation directory resolution, ownership verification, and atomic swap.

use super::archive::{is_valid_sha256_hex, sha256_file};
use anyhow::{Context, Result, bail, ensure};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const LOCK_DIR_NAME: &str = ".alan-cli-install.lock";
const MANIFEST_FILE_NAME: &str = ".alan-cli-manifest";
const LEGACY_STABLE_MANIFEST: &str = ".alan-cli-manifest-stable";
const LEGACY_DEV_MANIFEST: &str = ".alan-cli-manifest-dev";
const LEGACY_BINARY_NAMES: [&str; 3] = ["alan-os-host", "alan-dev", "alan-os-host-dev"];

/// RAII lock guard owning `<install_dir>/.alan-cli-install.lock`.
pub struct InstallLock {
    path: PathBuf,
}

impl InstallLock {
    /// Acquire `<install_dir>/.alan-cli-install.lock` atomically, failing cleanly if held or unwritable.
    pub fn acquire(install_dir: &Path) -> Result<Self> {
        ensure_writable_install_dir(install_dir)?;
        let lock_path = install_dir.join(LOCK_DIR_NAME);
        match std::fs::create_dir(&lock_path) {
            Ok(()) => Ok(Self { path: lock_path }),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => bail!(
                "installation lock exists at `{}`; stop any running installer or updater before recovering an interrupted installation",
                lock_path.display()
            ),
            Err(err) if err.kind() == ErrorKind::PermissionDenied => bail!(
                "installation directory `{}` is not writable ({err}); adjust directory permissions or pass `--install-dir <DIR>`",
                install_dir.display()
            ),
            Err(err) => Err(err).with_context(|| {
                format!(
                    "create installation lock directory `{}`",
                    lock_path.display()
                )
            }),
        }
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}

/// Summary of ownership checks and legacy artifacts scheduled for retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipPreflight {
    /// Legacy binary names owned by `.alan-cli-manifest-stable` / `.alan-cli-manifest-dev` to retire.
    pub retired_legacy_binaries: Vec<String>,
    /// Legacy manifest filenames present that will be retired on install.
    pub retired_legacy_manifests: Vec<String>,
}

/// Resolve target installation directory from `--install-dir`, `current_exe()`, or `~/.local/bin`.
pub fn resolve_install_dir(
    explicit_install_dir: Option<&Path>,
    current_exe: Option<&Path>,
    home_dir: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(dir) = explicit_install_dir {
        ensure!(
            !dir.as_os_str().is_empty(),
            "`--install-dir` path cannot be empty"
        );
        let absolute = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            std::env::current_dir()
                .context("resolve current working directory for `--install-dir`")?
                .join(dir)
        };
        if let Ok(meta) = std::fs::symlink_metadata(&absolute) {
            ensure!(
                std::fs::metadata(&absolute).is_ok_and(|m| m.is_dir()),
                "`--install-dir` path `{}` exists and is not a directory",
                absolute.display()
            );
            let _ = meta;
        }
        return canonicalize_with_existing_ancestor(&absolute);
    }

    if let Some(exe) = current_exe {
        let resolved_exe = std::fs::canonicalize(exe).unwrap_or_else(|_| exe.to_path_buf());
        if let Some(parent) = resolved_exe.parent()
            && parent.is_dir()
        {
            return Ok(std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()));
        }
    }

    let home = home_dir.context(
        "cannot determine installation directory from current executable or home directory; pass `--install-dir <DIR>`",
    )?;
    canonicalize_with_existing_ancestor(&home.join(".local").join("bin"))
}

fn canonicalize_with_existing_ancestor(path: &Path) -> Result<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Ok(canonical);
    }
    for ancestor in path.ancestors() {
        if let Ok(canonical) = std::fs::canonicalize(ancestor) {
            let suffix = path
                .strip_prefix(ancestor)
                .unwrap_or_else(|_| Path::new(""));
            return Ok(if suffix.as_os_str().is_empty() {
                canonical
            } else {
                canonical.join(suffix)
            });
        }
    }
    Ok(path.to_path_buf())
}

fn ensure_writable_install_dir(install_dir: &Path) -> Result<()> {
    match std::fs::metadata(install_dir) {
        Ok(meta) => {
            ensure!(
                meta.is_dir(),
                "installation target `{}` is not a directory",
                install_dir.display()
            );
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {
            if let Err(create_err) = std::fs::create_dir_all(install_dir) {
                if create_err.kind() == ErrorKind::PermissionDenied {
                    bail!(
                        "cannot create installation directory `{}` ({create_err}); adjust permissions or pass `--install-dir <DIR>`",
                        install_dir.display()
                    );
                }
                return Err(create_err).with_context(|| {
                    format!("create installation directory `{}`", install_dir.display())
                });
            }
        }
        Err(err) => {
            return Err(err).with_context(|| {
                format!("inspect installation directory `{}`", install_dir.display())
            });
        }
    }
    Ok(())
}

/// Validate `.alan-cli-manifest*` ownership receipts and preflight `<install_dir>/alan` + legacy binaries.
pub fn preflight_ownership(install_dir: &Path, force: bool) -> Result<OwnershipPreflight> {
    let manifest_path = install_dir.join(MANIFEST_FILE_NAME);
    let stable_path = install_dir.join(LEGACY_STABLE_MANIFEST);
    let dev_path = install_dir.join(LEGACY_DEV_MANIFEST);

    let main_receipts = match parse_manifest_file(&manifest_path, &["alan"]) {
        Ok(map) => map,
        Err(err) if force => {
            let _ = err;
            BTreeMap::new()
        }
        Err(err) => return Err(err),
    };
    let stable_receipts = parse_manifest_file(&stable_path, &["alan", "alan-os-host"])?;
    let dev_receipts = parse_manifest_file(&dev_path, &["alan-dev", "alan-os-host-dev"])?;

    let expected_alan = resolve_expected_digest(
        install_dir,
        "alan",
        [
            main_receipts.get("alan").map(String::as_str),
            stable_receipts.get("alan").map(String::as_str),
            None,
        ],
        force,
    )?;

    let alan_path = install_dir.join("alan");
    preflight_target_binary(&alan_path, expected_alan.as_deref(), force)?;

    let owning_legacy_installation = expected_alan.is_some()
        || !stable_receipts.is_empty()
        || !dev_receipts.is_empty()
        || path_exists_or_symlink(&stable_path)
        || path_exists_or_symlink(&dev_path);

    let mut retired_legacy_binaries = Vec::new();
    let mut retired_legacy_manifests = Vec::new();

    if owning_legacy_installation {
        for legacy_name in LEGACY_BINARY_NAMES {
            let expected = match legacy_name {
                "alan-os-host" => stable_receipts.get("alan-os-host").map(String::as_str),
                "alan-dev" => dev_receipts.get("alan-dev").map(String::as_str),
                "alan-os-host-dev" => dev_receipts.get("alan-os-host-dev").map(String::as_str),
                _ => None,
            };
            if let Some(expected_digest) = expected {
                let legacy_path = install_dir.join(legacy_name);
                if path_exists_or_symlink(&legacy_path) {
                    preflight_target_binary(&legacy_path, Some(expected_digest), force)?;
                    retired_legacy_binaries.push(legacy_name.to_string());
                }
            }
        }
        if path_exists_or_symlink(&stable_path) {
            retired_legacy_manifests.push(LEGACY_STABLE_MANIFEST.to_string());
        }
        if path_exists_or_symlink(&dev_path) {
            retired_legacy_manifests.push(LEGACY_DEV_MANIFEST.to_string());
        }
    }

    Ok(OwnershipPreflight {
        retired_legacy_binaries,
        retired_legacy_manifests,
    })
}

fn resolve_expected_digest(
    install_dir: &Path,
    binary_name: &str,
    candidates: [Option<&str>; 3],
    force: bool,
) -> Result<Option<String>> {
    let mut found: Option<String> = None;
    for candidate in candidates.into_iter().flatten() {
        if let Some(prev) = &found {
            if prev != candidate && !force {
                bail!(
                    "conflicting ownership receipts for `{}`",
                    install_dir.join(binary_name).display()
                );
            }
        } else {
            found = Some(candidate.to_string());
        }
    }
    Ok(found)
}

fn preflight_target_binary(path: &Path, expected_sha256: Option<&str>, force: bool) -> Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err).with_context(|| format!("inspect existing `{}`", path.display()));
        }
    };

    ensure!(
        !(meta.is_dir() && !meta.file_type().is_symlink()),
        "directory exists at target binary path `{}`",
        path.display()
    );

    if force {
        return Ok(());
    }

    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "non-regular file at `{}` (pass `--force` to replace)",
        path.display()
    );
    let expected = expected_sha256.with_context(|| {
        format!(
            "refusing to replace unowned existing `{}` (pass `--force` to overwrite)",
            path.display()
        )
    })?;
    let actual = sha256_file(path)?;
    ensure!(
        actual == expected,
        "refusing to replace locally modified `{}` (manifest digest mismatch; pass `--force` to overwrite)",
        path.display()
    );
    Ok(())
}

fn parse_manifest_file(path: &Path, allowed_names: &[&str]) -> Result<BTreeMap<String, String>> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("inspect ownership manifest `{}`", path.display()));
        }
    };
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "non-regular ownership manifest at `{}`",
        path.display()
    );
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read ownership manifest `{}`", path.display()))?;
    let mut entries = BTreeMap::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('|');
        let name = parts.next().unwrap_or("");
        let digest = parts.next().unwrap_or("");
        ensure!(
            parts.next().is_none() && allowed_names.contains(&name),
            "invalid ownership entry in `{}`",
            path.display()
        );
        ensure!(
            is_valid_sha256_hex(digest),
            "invalid ownership digest in `{}`",
            path.display()
        );
        ensure!(
            !entries.contains_key(name),
            "duplicate ownership entry `{name}` in `{}`",
            path.display()
        );
        entries.insert(name.to_string(), digest.to_ascii_lowercase());
    }
    Ok(entries)
}

fn path_exists_or_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Atomically install `extracted_bin` to `<install_dir>/alan`, write `.alan-cli-manifest`,
/// and retire owned legacy binaries/manifests. Returns `(installed_binary_sha256, retired_items)`.
pub fn atomic_install_binary(
    install_dir: &Path,
    extracted_bin: &Path,
    preflight: &OwnershipPreflight,
) -> Result<(String, Vec<String>)> {
    let op_id = uuid::Uuid::new_v4();
    let staged_bin = install_dir.join(format!(".alan.staging.{op_id}"));
    let staged_manifest = install_dir.join(format!(".alan-cli-manifest.staging.{op_id}"));
    let backup_dir = install_dir.join(format!(".alan-update-backup.{op_id}"));
    let _cleanup = SwapCleanupGuard {
        staged_bin: staged_bin.clone(),
        staged_manifest: staged_manifest.clone(),
        backup_dir: backup_dir.clone(),
    };

    if let Err(err) = std::fs::copy(extracted_bin, &staged_bin) {
        if err.kind() == ErrorKind::PermissionDenied {
            bail!(
                "installation directory `{}` is not writable ({err}); adjust permissions or pass `--install-dir <DIR>`",
                install_dir.display()
            );
        }
        return Err(err)
            .with_context(|| format!("stage extracted binary into `{}`", staged_bin.display()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged_bin, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("chmod 0755 `{}`", staged_bin.display()))?;
    }
    if let Ok(file) = std::fs::File::open(&staged_bin) {
        let _ = file.sync_all();
    }

    let new_binary_sha256 = sha256_file(&staged_bin)?;
    let manifest_body = format!("alan|{new_binary_sha256}\n");
    std::fs::write(&staged_manifest, manifest_body.as_bytes())
        .with_context(|| format!("stage ownership manifest `{}`", staged_manifest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&staged_manifest, std::fs::Permissions::from_mode(0o644));
    }
    if let Ok(file) = std::fs::File::open(&staged_manifest) {
        let _ = file.sync_all();
    }

    std::fs::create_dir(&backup_dir)
        .with_context(|| format!("create swap backup directory `{}`", backup_dir.display()))?;

    let target_bin = install_dir.join("alan");
    let target_manifest = install_dir.join(MANIFEST_FILE_NAME);
    let had_prior_bin = copy_if_regular_file(&target_bin, &backup_dir.join("alan"))?;
    let had_prior_manifest =
        copy_if_regular_file(&target_manifest, &backup_dir.join(MANIFEST_FILE_NAME))?;

    let mut moved_legacy = Vec::new();
    for name in preflight
        .retired_legacy_binaries
        .iter()
        .chain(preflight.retired_legacy_manifests.iter())
    {
        let src = install_dir.join(name);
        if path_exists_or_symlink(&src) {
            let dst = backup_dir.join(name);
            if let Err(err) = std::fs::rename(&src, &dst) {
                restore_moved_legacy(install_dir, &backup_dir, &moved_legacy);
                return Err(err)
                    .with_context(|| format!("retire legacy artifact `{}`", src.display()));
            }
            moved_legacy.push(name.clone());
        }
    }

    if let Err(err) = std::fs::rename(&staged_bin, &target_bin) {
        restore_moved_legacy(install_dir, &backup_dir, &moved_legacy);
        return Err(err).with_context(|| format!("publish `{}`", target_bin.display()));
    }

    if let Err(err) = std::fs::rename(&staged_manifest, &target_manifest) {
        if had_prior_bin {
            let _ = std::fs::rename(backup_dir.join("alan"), &target_bin);
        } else {
            let _ = std::fs::remove_file(&target_bin);
        }
        if had_prior_manifest {
            let _ = std::fs::rename(backup_dir.join(MANIFEST_FILE_NAME), &target_manifest);
        }
        restore_moved_legacy(install_dir, &backup_dir, &moved_legacy);
        return Err(err).with_context(|| format!("publish `{}`", target_manifest.display()));
    }

    Ok((new_binary_sha256, moved_legacy))
}

fn copy_if_regular_file(src: &Path, dst: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(src) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            std::fs::copy(src, dst).with_context(|| format!("back up `{}`", src.display()))?;
            if let Some(perm) = std::fs::metadata(src).ok().map(|m| m.permissions()) {
                let _ = std::fs::set_permissions(dst, perm);
            }
            Ok(true)
        }
        Ok(_) | Err(_) => Ok(false),
    }
}

fn restore_moved_legacy(install_dir: &Path, backup_dir: &Path, names: &[String]) {
    for name in names {
        let src = backup_dir.join(name);
        let dst = install_dir.join(name);
        if path_exists_or_symlink(&src) && !path_exists_or_symlink(&dst) {
            let _ = std::fs::rename(src, dst);
        }
    }
}

struct SwapCleanupGuard {
    staged_bin: PathBuf,
    staged_manifest: PathBuf,
    backup_dir: PathBuf,
}

impl Drop for SwapCleanupGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.staged_bin);
        let _ = std::fs::remove_file(&self.staged_manifest);
        let _ = std::fs::remove_dir_all(&self.backup_dir);
    }
}
