//! Native OS CoW cloning (`clonefile` on macOS APFS, `FICLONE` on Linux)
//! and deterministic `git worktree` / sibling-volume fallback.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::gc::write_instance_metadata;
use super::types::{CloneFallbackPolicy, CowWorkspaceConfig, CowWorkspaceError, WorkspaceBacking};

/// Result of materializing the physical workspace directory.
pub(crate) struct MaterializedClone {
    pub(crate) tree_path: PathBuf,
    pub(crate) backing_kind: WorkspaceBacking,
    pub(crate) source_is_git: bool,
    pub(crate) sibling_root: Option<PathBuf>,
}

enum NativeCloneOutcome {
    Cloned,
    Unsupported(String),
}

pub(crate) fn is_git_repo_root(host_root: &Path) -> bool {
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let top = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if top.is_empty() {
        return false;
    }
    match (dunce::canonicalize(&top), dunce::canonicalize(host_root)) {
        (Ok(top_canon), Ok(host_canon)) => top_canon == host_canon,
        _ => false,
    }
}

pub(crate) fn materialize_clone(
    config: &CowWorkspaceConfig,
    canonical_host_root: &Path,
    workspace_dir: &Path,
    workspace_id: &str,
    fallback_policy: CloneFallbackPolicy,
) -> std::result::Result<MaterializedClone, CowWorkspaceError> {
    let source_is_git = is_git_repo_root(canonical_host_root);
    let tree_path = workspace_dir.join("tree");

    let staging_parent = workspace_dir
        .parent()
        .ok_or_else(|| CowWorkspaceError::InvalidInput("workspace_dir has no parent".into()))?;
    let host_meta = fs::symlink_metadata(canonical_host_root)?;
    let staging_meta = fs::symlink_metadata(staging_parent)?;
    let same_device = host_meta.dev() == staging_meta.dev();

    let force_cross_volume = fallback_policy == CloneFallbackPolicy::ForceCrossVolumeFallback;

    if same_device && !force_cross_volume {
        match native_cow_clone(canonical_host_root, &tree_path)? {
            NativeCloneOutcome::Cloned => {
                return Ok(MaterializedClone {
                    tree_path,
                    backing_kind: WorkspaceBacking::NativeCow,
                    source_is_git,
                    sibling_root: None,
                });
            }
            NativeCloneOutcome::Unsupported(reason) => {
                if fallback_policy == CloneFallbackPolicy::RequireNativeCow {
                    return Err(CowWorkspaceError::NativeCowRequiredButUnavailable(reason));
                }
            }
        }
    } else if fallback_policy == CloneFallbackPolicy::RequireNativeCow {
        return Err(CowWorkspaceError::NativeCowRequiredButUnavailable(format!(
            "host device {} differs from System Store staging device {}",
            host_meta.dev(),
            staging_meta.dev()
        )));
    }

    // Fallback path: git worktree (deterministic) or sibling project-volume staging for non-Git.
    if source_is_git {
        create_git_worktree(canonical_host_root, &tree_path)?;
        return Ok(MaterializedClone {
            tree_path,
            backing_kind: WorkspaceBacking::GitWorktree,
            source_is_git: true,
            sibling_root: None,
        });
    }

    if config.allow_sibling_project_volume_staging
        && let Some(host_parent) = canonical_host_root.parent()
    {
        let sibling_base = host_parent
            .join(".alan-cow-workspaces")
            .join(&config.instance_id);
        fs::create_dir_all(&sibling_base)?;
        write_instance_metadata(&sibling_base, &config.instance_id)
            .map_err(CowWorkspaceError::Other)?;
        let sibling_ws_dir = sibling_base.join(workspace_id);
        fs::create_dir_all(&sibling_ws_dir)?;
        let sibling_tree = sibling_ws_dir.join("tree");
        let parent_meta = fs::symlink_metadata(&sibling_ws_dir)?;
        if parent_meta.dev() == host_meta.dev()
            && let NativeCloneOutcome::Cloned =
                native_cow_clone(canonical_host_root, &sibling_tree)?
        {
            return Ok(MaterializedClone {
                tree_path: sibling_tree,
                backing_kind: WorkspaceBacking::SiblingVolumeCow,
                source_is_git: false,
                sibling_root: Some(sibling_ws_dir),
            });
        }
        let _ = fs::remove_dir_all(&sibling_ws_dir);
    }

    Err(CowWorkspaceError::CrossVolumeNonGitUnsupported)
}

pub(crate) fn remove_cloned_tree(
    host_root: &Path,
    tree_path: &Path,
    backing_kind: WorkspaceBacking,
    sibling_root: Option<&Path>,
) -> Result<()> {
    if backing_kind == WorkspaceBacking::GitWorktree && host_root.exists() {
        let _ = Command::new("git")
            .arg("-C")
            .arg(host_root)
            .args(["worktree", "remove", "--force"])
            .arg(tree_path)
            .output();
        let _ = Command::new("git")
            .arg("-C")
            .arg(host_root)
            .args(["worktree", "prune"])
            .output();
    }
    if tree_path.exists() {
        fs::remove_dir_all(tree_path)
            .with_context(|| format!("remove cloned workspace tree {}", tree_path.display()))?;
    }
    if let Some(sibling_dir) = sibling_root
        && sibling_dir.exists()
    {
        fs::remove_dir_all(sibling_dir).with_context(|| {
            format!("remove sibling CoW workspace dir {}", sibling_dir.display())
        })?;
    }
    Ok(())
}

fn create_git_worktree(
    host_root: &Path,
    tree_path: &Path,
) -> std::result::Result<(), CowWorkspaceError> {
    let head_check = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()?;
    if !head_check.status.success() {
        return Err(CowWorkspaceError::UnbornGitHead);
    }

    let output = Command::new("git")
        .arg("-C")
        .arg(host_root)
        .args(["worktree", "add", "--detach"])
        .arg(tree_path)
        .arg("HEAD")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CowWorkspaceError::Other(anyhow::anyhow!(
            "git worktree add failed: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn native_cow_clone(
    src: &Path,
    dst: &Path,
) -> std::result::Result<NativeCloneOutcome, CowWorkspaceError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    if dst.exists() {
        return Err(CowWorkspaceError::InvalidInput(format!(
            "clone destination already exists: {}",
            dst.display()
        )));
    }
    let src_c = CString::new(src.as_os_str().as_bytes())
        .map_err(|_| CowWorkspaceError::InvalidInput("NUL byte in source path".into()))?;
    let dst_c = CString::new(dst.as_os_str().as_bytes())
        .map_err(|_| CowWorkspaceError::InvalidInput("NUL byte in destination path".into()))?;

    // SAFETY: both pointers are valid NUL-terminated C strings; flags = 0 uses APFS CoW clone.
    let rc = unsafe { libc::clonefile(src_c.as_ptr(), dst_c.as_ptr(), 0) };
    if rc == 0 {
        return Ok(NativeCloneOutcome::Cloned);
    }
    let err = std::io::Error::last_os_error();
    let _ = fs::remove_dir_all(dst);
    match err.raw_os_error() {
        Some(code)
            if code == libc::EXDEV
                || code == libc::ENOTSUP
                || code == libc::EOPNOTSUPP
                || code == libc::EINVAL
                || code == libc::ENOSYS =>
        {
            Ok(NativeCloneOutcome::Unsupported(err.to_string()))
        }
        _ => Err(CowWorkspaceError::Io(err)),
    }
}

#[cfg(target_os = "linux")]
fn native_cow_clone(
    src: &Path,
    dst: &Path,
) -> std::result::Result<NativeCloneOutcome, CowWorkspaceError> {
    match reflink_walk(src, dst) {
        Ok(()) => Ok(NativeCloneOutcome::Cloned),
        Err(ReflinkWalkError::Unsupported(reason)) => {
            let _ = fs::remove_dir_all(dst);
            Ok(NativeCloneOutcome::Unsupported(reason))
        }
        Err(ReflinkWalkError::Io(err)) => {
            let _ = fs::remove_dir_all(dst);
            Err(CowWorkspaceError::Io(err))
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn native_cow_clone(
    _src: &Path,
    _dst: &Path,
) -> std::result::Result<NativeCloneOutcome, CowWorkspaceError> {
    Ok(NativeCloneOutcome::Unsupported(
        "native OS CoW cloning is only supported on macOS APFS and Linux Btrfs/XFS".to_string(),
    ))
}

#[cfg(target_os = "linux")]
enum ReflinkWalkError {
    Unsupported(String),
    Io(std::io::Error),
}

#[cfg(target_os = "linux")]
impl From<std::io::Error> for ReflinkWalkError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[cfg(target_os = "linux")]
fn reflink_walk(src_dir: &Path, dst_dir: &Path) -> std::result::Result<(), ReflinkWalkError> {
    use std::os::unix::fs::PermissionsExt;

    let src_meta = fs::symlink_metadata(src_dir)?;
    fs::create_dir_all(dst_dir)?;
    let mut dir_modes_to_restore: Vec<(PathBuf, u32)> = Vec::new();
    let mode = src_meta.mode() & 0o7777;
    if (mode & 0o200) == 0 {
        dir_modes_to_restore.push((dst_dir.to_path_buf(), mode));
    } else {
        fs::set_permissions(dst_dir, fs::Permissions::from_mode(mode))?;
    }

    reflink_walk_inner(src_dir, dst_dir, &mut dir_modes_to_restore)?;

    for (dir, mode) in dir_modes_to_restore.into_iter().rev() {
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn reflink_walk_inner(
    src_dir: &Path,
    dst_dir: &Path,
    dir_modes_to_restore: &mut Vec<(PathBuf, u32)>,
) -> std::result::Result<(), ReflinkWalkError> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::io::AsRawFd;

    // Linux FICLONE ioctl (`_IOW(0x94, 9, int)` = 0x40049409).
    const FICLONE_IOCTL: u32 = 0x4004_9409;

    for entry in fs::read_dir(src_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let src_path = entry.path();
        let dst_path = dst_dir.join(&file_name);
        let meta = fs::symlink_metadata(&src_path)?;
        let file_type = meta.file_type();

        if file_type.is_symlink() {
            let target = fs::read_link(&src_path)?;
            std::os::unix::fs::symlink(target, &dst_path)?;
        } else if file_type.is_dir() {
            fs::create_dir(&dst_path)?;
            let mode = meta.mode() & 0o7777;
            if (mode & 0o200) == 0 {
                dir_modes_to_restore.push((dst_path.clone(), mode));
            } else {
                fs::set_permissions(&dst_path, fs::Permissions::from_mode(mode))?;
            }
            reflink_walk_inner(&src_path, &dst_path, dir_modes_to_restore)?;
        } else if file_type.is_file() {
            let src_file = fs::File::open(&src_path)?;
            let dst_file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dst_path)?;
            // SAFETY: valid open file descriptors passed to Linux FICLONE ioctl.
            let rc = unsafe {
                libc::ioctl(
                    dst_file.as_raw_fd(),
                    FICLONE_IOCTL as _,
                    src_file.as_raw_fd() as libc::c_int,
                )
            };
            if rc != 0 {
                let err = std::io::Error::last_os_error();
                return match err.raw_os_error() {
                    Some(code)
                        if code == libc::EXDEV
                            || code == libc::EOPNOTSUPP
                            || code == libc::ENOTSUP
                            || code == libc::EINVAL
                            || code == libc::ENOSYS =>
                    {
                        Err(ReflinkWalkError::Unsupported(err.to_string()))
                    }
                    _ => Err(ReflinkWalkError::Io(err)),
                };
            }
            let mode = meta.mode() & 0o7777;
            fs::set_permissions(&dst_path, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}
