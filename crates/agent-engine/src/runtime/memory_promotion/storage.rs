use std::{
    fs::{File, OpenOptions},
    path::Path,
};

use anyhow::{Context, Result};
use tokio_util::sync::CancellationToken;

#[cfg(not(unix))]
use anyhow::anyhow;
#[cfg(unix)]
use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
pub(super) async fn acquire_promotion_lock(
    lock_path: &Path,
    cancel: &CancellationToken,
) -> Result<File> {
    let path = lock_path.to_path_buf();

    let file = tokio::task::spawn_blocking(move || {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW);
        options
            .open(&path)
            .with_context(|| format!("open promotion lock target {}", path.display()))
    })
    .await
    .context("join Memory Store promotion lock task")??;

    loop {
        if cancel.is_cancelled() {
            return Err(anyhow::anyhow!(
                "memory promotion was cancelled while waiting for its store lock"
            ));
        }
        // LOCK_NB keeps the Tokio worker responsive; retrying asynchronously lets
        // the existing cancellation token interrupt a wait behind another session.
        // SAFETY: file owns a valid descriptor for the lifetime of the acquired lock.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result == 0 {
            return Ok(file);
        }

        let error = std::io::Error::last_os_error();
        match error.kind() {
            std::io::ErrorKind::Interrupted => continue,
            std::io::ErrorKind::WouldBlock => {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        return Err(anyhow::anyhow!(
                            "memory promotion was cancelled while waiting for its store lock"
                        ));
                    }
                    _ = tokio::time::sleep(Duration::from_millis(25)) => {}
                }
            }
            _ => return Err(error).context("acquire Memory Store promotion lock"),
        }
    }
}

#[cfg(not(unix))]
pub(super) async fn acquire_promotion_lock(
    _lock_path: &Path,
    _cancel: &CancellationToken,
) -> Result<File> {
    Err(anyhow!(
        "Memory Store promotion locking requires Unix file locks"
    ))
}

pub(super) async fn write_text_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create directory {}", parent.display()))?;
    }
    tokio::fs::write(path, content)
        .await
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
