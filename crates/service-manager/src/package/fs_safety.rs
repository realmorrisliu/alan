//! Owned-file checks and durable tree synchronization for Package Store content.

use std::fs::{self, File};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

pub(super) fn ensure_owned_directory(path: &Path, message: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("inspect {path:?}"))?;
    ensure!(
        metadata.file_type().is_dir() && !metadata.file_type().is_symlink(),
        "{message}"
    );
    Ok(())
}

pub(super) fn ensure_owned_file(path: &Path, message: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path).with_context(|| format!("inspect {path:?}"))?;
    ensure!(
        metadata.file_type().is_file() && !metadata.file_type().is_symlink(),
        "{message}"
    );
    Ok(())
}

pub(super) fn sync_tree(root: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                pending.push(path);
            } else if metadata.file_type().is_file() {
                File::open(path)?.sync_all()?;
            } else {
                bail!("package staging contains an unsupported entry");
            }
        }
    }
    for directory in directories.into_iter().rev() {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}
