use std::{
    fs::{File, OpenOptions},
    path::Path,
};

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;

#[cfg(not(unix))]
use anyhow::anyhow;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};

pub(super) async fn acquire_promotion_lock(memory_dir: &Path) -> Result<File> {
    let path = memory_dir
        .parent()
        .context("Memory Store has no parent directory")?
        .join(".memory-promotion.lock");

    tokio::task::spawn_blocking(move || {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        let file = options
            .open(&path)
            .with_context(|| format!("open Memory Store promotion lock {}", path.display()))?;

        #[cfg(unix)]
        loop {
            // SAFETY: file owns a valid descriptor for the lifetime of the acquired lock.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
            if result == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error).context("acquire Memory Store promotion lock");
            }
        }

        Ok(file)
    })
    .await
    .context("join Memory Store promotion lock task")?
}

#[cfg(not(unix))]
pub(super) async fn acquire_promotion_lock(_memory_dir: &Path) -> Result<File> {
    Err(anyhow!(
        "Memory Store promotion locking requires Unix file locks"
    ))
}

pub(super) async fn write_text_file(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("Memory Store path has no parent: {}", path.display()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("create directory {}", parent.display()))?;

    #[cfg(unix)]
    let mode = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_file() => metadata.permissions().mode() & 0o7777,
        Ok(_) => 0o600,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0o600,
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };

    let file_name = path
        .file_name()
        .context("Memory Store path has no filename")?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{file_name}.tmp-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let result = async {
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(mode).custom_flags(libc::O_NOFOLLOW);
        let mut file = options.open(&temporary).await.with_context(|| {
            format!("create temporary Memory Store file {}", temporary.display())
        })?;
        #[cfg(unix)]
        file.set_permissions(std::fs::Permissions::from_mode(mode))
            .await
            .with_context(|| format!("set permissions on {}", temporary.display()))?;
        file.write_all(content.as_bytes())
            .await
            .with_context(|| format!("write {}", temporary.display()))?;
        file.sync_all()
            .await
            .with_context(|| format!("sync {}", temporary.display()))?;
        drop(file);
        tokio::fs::rename(&temporary, path)
            .await
            .with_context(|| format!("replace Memory Store file {}", path.display()))?;
        Ok(())
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}
