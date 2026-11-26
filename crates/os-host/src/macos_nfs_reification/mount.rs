//! Zero-root macOS loopback NFSv3 mount lifecycle, clean unmount, and dead-PID startup GC.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use alan_ap::FileServer;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::paths::{SystemStorePaths, validate_store_segment};

use super::nfs_server::{NfsLoopbackServer, NfsServerConfig};

const INSTANCE_META_FILE: &str = "instance.json";
#[cfg(target_os = "macos")]
const MOUNT_NFS_BIN: &str = "/sbin/mount_nfs";
#[cfg(target_os = "macos")]
const UMOUNT_BIN: &str = "/sbin/umount";
#[cfg(target_os = "macos")]
const UMOUNT_GRACE_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(target_os = "macos")]
const DROP_UMOUNT_GRACE_TIMEOUT: Duration = Duration::from_millis(500);

/// Build the canonical zero-root macOS `/sbin/mount_nfs` option string for loopback port `port`.
pub fn mount_nfs_options(port: u16) -> String {
    format!(
        "vers=3,tcp,port={port},mountport={port},rdonly,locallocks,nfc,noatime,actimeo=1,rsize=65536,wsize=65536,soft,intr,dumbtimer,timeo=300,retrans=6,mutejukebox,nobrowse"
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ReifiedMountInstanceMetadata {
    instance_id: String,
    pid: u32,
    created_at_ms: u128,
}

/// Summary produced by [`gc_stale_reified_mounts`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReifiedMountGcReport {
    /// Dead-PID instance directories cleaned up.
    pub swept_instances: Vec<String>,
    /// Active other-PID instance directories preserved untouched.
    pub skipped_live_instances: Vec<String>,
    /// Stale kernel mount points unmounted during GC.
    pub unmounted_mounts: usize,
    /// Stale mount-point directories removed during GC.
    pub removed_mount_dirs: usize,
}

/// Sweep stale reified mount directories under `System Store/reified-mounts/` owned by dead PIDs.
///
/// Live instances (including `current_instance_id`) and directories lacking an
/// `instance.json` marker are left untouched.
pub fn gc_stale_reified_mounts(
    system_store: &SystemStorePaths,
    current_instance_id: &str,
) -> Result<ReifiedMountGcReport> {
    let reified_root = system_store.reified_mounts()?;
    let mut report = ReifiedMountGcReport::default();
    if !reified_root.exists() {
        return Ok(report);
    }

    for entry in fs::read_dir(&reified_root)
        .with_context(|| format!("read reified mounts root {}", reified_root.display()))?
    {
        let entry = entry?;
        let instance_dir = entry.path();
        if !instance_dir.is_dir() {
            continue;
        }
        let Some(name) = instance_dir.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == current_instance_id {
            continue;
        }

        let meta_path = instance_dir.join(INSTANCE_META_FILE);
        let Ok(raw) = fs::read(&meta_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<ReifiedMountInstanceMetadata>(&raw) else {
            continue;
        };

        if is_pid_alive(meta.pid) {
            report.skipped_live_instances.push(meta.instance_id);
            continue;
        }

        let mut all_mounts_cleared = true;
        if let Ok(children) = fs::read_dir(&instance_dir) {
            for child in children.flatten() {
                let child_path = child.path();
                if child_path == meta_path {
                    continue;
                }
                if child_path.is_dir() {
                    match clean_stale_mount_dir(&child_path) {
                        Ok(was_unmounted) => {
                            if was_unmounted {
                                report.unmounted_mounts += 1;
                            }
                            report.removed_mount_dirs += 1;
                        }
                        Err(error) => {
                            tracing::warn!(
                                path = %child_path.display(),
                                %error,
                                "failed to clean stale reified mount directory"
                            );
                            all_mounts_cleared = false;
                        }
                    }
                } else {
                    let _ = fs::remove_file(&child_path);
                }
            }
        }

        if all_mounts_cleared {
            let _ = fs::remove_file(&meta_path);
            if let Err(error) = fs::remove_dir(&instance_dir) {
                tracing::warn!(
                    path = %instance_dir.display(),
                    %error,
                    "failed to remove stale reified mount instance directory"
                );
            } else {
                report.swept_instances.push(meta.instance_id);
            }
        }
    }

    report.swept_instances.sort();
    report.skipped_live_instances.sort();
    Ok(report)
}

/// Live handle to one zero-root macOS loopback NFSv3 mount backed by an [`NfsLoopbackServer`].
///
/// Unmounts cleanly (`umount`, falling back to `umount -f`) before stopping the
/// loopback server on [`ReifiedMountHandle::unmount`] or [`Drop`].
pub struct ReifiedMountHandle {
    mount_point: PathBuf,
    port: u16,
    server: Option<NfsLoopbackServer>,
    mounted: Arc<AtomicBool>,
}

impl std::fmt::Debug for ReifiedMountHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReifiedMountHandle")
            .field("mount_point", &self.mount_point)
            .field("port", &self.port)
            .field("mounted", &self.is_mounted())
            .finish_non_exhaustive()
    }
}

impl ReifiedMountHandle {
    /// Start a loopback NFSv3 server exporting `file_server` and mount it read-only at `mount_point`.
    pub async fn mount_at(mount_point: PathBuf, file_server: Arc<dyn FileServer>) -> Result<Self> {
        Self::mount_at_with_config(mount_point, file_server, NfsServerConfig::default()).await
    }

    /// Start a loopback NFSv3 server with explicit `config` and mount it read-only at `mount_point`.
    pub async fn mount_at_with_config(
        mount_point: PathBuf,
        file_server: Arc<dyn FileServer>,
        config: NfsServerConfig,
    ) -> Result<Self> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (mount_point, file_server, config);
            bail!("zero-root macOS loopback NFSv3 reification is only supported on macOS");
        }

        #[cfg(target_os = "macos")]
        {
            let created_dir = !mount_point.exists();
            fs::create_dir_all(&mount_point)
                .with_context(|| format!("create reified mount point {}", mount_point.display()))?;
            set_private_dir_mode(&mount_point);

            let server = match NfsLoopbackServer::start_with_config(file_server, config).await {
                Ok(server) => server,
                Err(err) => {
                    if created_dir {
                        let _ = fs::remove_dir(&mount_point);
                    }
                    return Err(err);
                }
            };
            let port = server.port();
            let options = mount_nfs_options(port);

            let output = tokio::process::Command::new(MOUNT_NFS_BIN)
                .arg("-o")
                .arg(&options)
                .arg("127.0.0.1:/")
                .arg(&mount_point)
                .output()
                .await;

            match output {
                Ok(out) if out.status.success() => Ok(Self {
                    mount_point,
                    port,
                    server: Some(server),
                    mounted: Arc::new(AtomicBool::new(true)),
                }),
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    server.shutdown().await;
                    if created_dir {
                        let _ = fs::remove_dir(&mount_point);
                    }
                    bail!(
                        "mount_nfs failed for {} (status {}): stderr=`{}` stdout=`{}`",
                        mount_point.display(),
                        out.status,
                        stderr,
                        stdout
                    );
                }
                Err(err) => {
                    server.shutdown().await;
                    if created_dir {
                        let _ = fs::remove_dir(&mount_point);
                    }
                    Err(err).with_context(|| {
                        format!(
                            "execute {MOUNT_NFS_BIN} for mount point {}",
                            mount_point.display()
                        )
                    })
                }
            }
        }
    }

    /// Host filesystem path where the aP namespace is reified.
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Loopback TCP port serving MOUNT v3 and NFS v3 for this mount.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether this handle believes the mount is currently active.
    pub fn is_mounted(&self) -> bool {
        self.mounted.load(Ordering::Acquire)
    }

    /// Unmount cleanly (`umount`, falling back to `umount -f`), stop the server, and remove the dir.
    pub async fn unmount(mut self) -> Result<()> {
        let was_mounted = self.mounted.swap(false, Ordering::AcqRel);
        let unmount_result = if was_mounted {
            unmount_path_async(&self.mount_point).await
        } else {
            Ok(())
        };
        if let Some(server) = self.server.take() {
            server.shutdown().await;
        }
        if unmount_result.is_ok() {
            let _ = fs::remove_dir(&self.mount_point);
        }
        unmount_result
    }
}

impl Drop for ReifiedMountHandle {
    fn drop(&mut self) {
        if self.mounted.swap(false, Ordering::AcqRel) {
            let unmount_ok = unmount_path_sync(&self.mount_point, true).is_ok();
            if let Some(server) = self.server.take() {
                server.signal_shutdown();
                drop(server);
            }
            if unmount_ok {
                let _ = fs::remove_dir(&self.mount_point);
            }
        } else if let Some(server) = self.server.take() {
            server.signal_shutdown();
            drop(server);
        }
    }
}

struct ManagerInner {
    system_store: SystemStorePaths,
    instance_id: String,
    instance_dir: PathBuf,
    mounts: Mutex<BTreeMap<String, ReifiedMountHandle>>,
}

impl Drop for ManagerInner {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.mounts.lock() {
            guard.clear();
        }
        let _ = fs::remove_file(self.instance_dir.join(INSTANCE_META_FILE));
        let _ = fs::remove_dir(&self.instance_dir);
    }
}

/// Host-lifecycle manager owning one instance's reified loopback NFSv3 mounts and startup GC.
#[derive(Clone)]
pub struct ReifiedMountManager {
    inner: Arc<ManagerInner>,
}

impl std::fmt::Debug for ReifiedMountManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.inner.mounts.lock().map_or(0, |m| m.len());
        f.debug_struct("ReifiedMountManager")
            .field("instance_id", &self.inner.instance_id)
            .field("instance_dir", &self.inner.instance_dir)
            .field("active_mounts", &count)
            .finish_non_exhaustive()
    }
}

impl ReifiedMountManager {
    /// Initialize a reified mount manager for `instance_id` and GC dead-PID mounts on startup.
    pub fn new(
        system_store: SystemStorePaths,
        instance_id: impl Into<String>,
    ) -> Result<(Self, ReifiedMountGcReport)> {
        let instance_id = instance_id.into();
        let reified_root = system_store.reified_mounts()?;
        fs::create_dir_all(&reified_root).with_context(|| {
            format!(
                "create System Store reified mounts root {}",
                reified_root.display()
            )
        })?;
        set_private_dir_mode(&reified_root);

        let gc_report = gc_stale_reified_mounts(&system_store, &instance_id)?;
        let instance_dir = system_store.instance_reified_mounts(&instance_id)?;
        write_instance_metadata(&instance_dir, &instance_id)?;

        Ok((
            Self {
                inner: Arc::new(ManagerInner {
                    system_store,
                    instance_id,
                    instance_dir,
                    mounts: Mutex::new(BTreeMap::new()),
                }),
            },
            gc_report,
        ))
    }

    /// Directory containing this instance's reified mount points.
    pub fn instance_dir(&self) -> &Path {
        &self.inner.instance_dir
    }

    /// Re-run dead-PID garbage collection under `System Store/reified-mounts/`.
    pub fn gc_stale(&self) -> Result<ReifiedMountGcReport> {
        gc_stale_reified_mounts(&self.inner.system_store, &self.inner.instance_id)
    }

    /// Mount `file_server` read-only under `System Store/reified-mounts/<instance_id>/<name>`.
    pub async fn mount(&self, name: &str, file_server: Arc<dyn FileServer>) -> Result<PathBuf> {
        self.mount_with_config(name, file_server, NfsServerConfig::default())
            .await
    }

    /// Mount `file_server` read-only with explicit [`NfsServerConfig`].
    pub async fn mount_with_config(
        &self,
        name: &str,
        file_server: Arc<dyn FileServer>,
        config: NfsServerConfig,
    ) -> Result<PathBuf> {
        validate_store_segment("reified mount name", name)?;
        {
            let guard = self.inner.mounts.lock().expect("reified mounts poisoned");
            if guard.contains_key(name) {
                bail!("reified mount `{name}` is already active");
            }
        }
        let mount_point = self.inner.instance_dir.join(name);
        let handle =
            ReifiedMountHandle::mount_at_with_config(mount_point.clone(), file_server, config)
                .await?;

        let raced_handle = {
            let mut guard = self.inner.mounts.lock().expect("reified mounts poisoned");
            if guard.contains_key(name) {
                Some(handle)
            } else {
                guard.insert(name.to_string(), handle);
                None
            }
        };
        if let Some(handle) = raced_handle {
            let _ = handle.unmount().await;
            bail!("reified mount `{name}` raced with another mount");
        }
        Ok(mount_point)
    }

    /// Unmount a single named reified mount owned by this instance.
    pub async fn unmount(&self, name: &str) -> Result<bool> {
        let handle = {
            let mut guard = self.inner.mounts.lock().expect("reified mounts poisoned");
            guard.remove(name)
        };
        let Some(handle) = handle else {
            return Ok(false);
        };
        handle.unmount().await?;
        Ok(true)
    }

    /// Unmount every active reified mount owned by this instance and clean up its instance dir.
    pub async fn unmount_all(&self) -> Result<()> {
        let handles: Vec<(String, ReifiedMountHandle)> = {
            let mut guard = self.inner.mounts.lock().expect("reified mounts poisoned");
            std::mem::take(&mut *guard).into_iter().collect()
        };
        let mut first_error: Option<anyhow::Error> = None;
        for (name, handle) in handles {
            if let Err(err) = handle.unmount().await {
                tracing::warn!(mount = %name, %err, "failed to unmount reified mount");
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
        if first_error.is_none() {
            let _ = fs::remove_file(self.inner.instance_dir.join(INSTANCE_META_FILE));
            let _ = fs::remove_dir(&self.inner.instance_dir);
        }
        match first_error {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// List currently active `(name, mount_point)` pairs owned by this instance.
    pub fn active_mount_points(&self) -> Vec<(String, PathBuf)> {
        let guard = self.inner.mounts.lock().expect("reified mounts poisoned");
        guard
            .iter()
            .map(|(name, handle)| (name.clone(), handle.mount_point().to_path_buf()))
            .collect()
    }
}

fn write_instance_metadata(instance_dir: &Path, instance_id: &str) -> Result<()> {
    fs::create_dir_all(instance_dir).with_context(|| {
        format!(
            "create reified mount instance dir {}",
            instance_dir.display()
        )
    })?;
    set_private_dir_mode(instance_dir);
    let meta = ReifiedMountInstanceMetadata {
        instance_id: instance_id.to_string(),
        pid: std::process::id(),
        created_at_ms: now_ms(),
    };
    let encoded = serde_json::to_vec_pretty(&meta).context("serialize reified mount metadata")?;
    let tmp_path = instance_dir.join(format!(".{INSTANCE_META_FILE}.tmp-{}", std::process::id()));
    fs::write(&tmp_path, encoded)?;
    fs::rename(&tmp_path, instance_dir.join(INSTANCE_META_FILE))?;
    Ok(())
}

fn clean_stale_mount_dir(dir: &Path) -> Result<bool> {
    // Fast path: empty unmounted directory removes cleanly without invoking umount.
    if fs::remove_dir(dir).is_ok() {
        return Ok(false);
    }

    #[cfg(target_os = "macos")]
    {
        if is_active_mountpoint_macos(dir) {
            unmount_path_sync(dir, false)?;
            fs::remove_dir(dir)
                .with_context(|| format!("remove unmounted stale dir {}", dir.display()))?;
            return Ok(true);
        }
    }

    // Confirmed not an active mount point: remove leftover non-mounted directory contents.
    fs::remove_dir_all(dir)
        .with_context(|| format!("remove stale reified mount dir {}", dir.display()))?;
    Ok(false)
}

#[cfg(target_os = "macos")]
fn is_active_mountpoint_macos(dir: &Path) -> bool {
    let target = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let Ok(output) = std::process::Command::new("/sbin/mount").output() else {
        return true;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let needle = format!(" on {} (", target.display());
    let raw_needle = format!(" on {} (", dir.display());
    stdout
        .lines()
        .any(|line| line.contains(&needle) || line.contains(&raw_needle))
}

#[cfg(target_os = "macos")]
async fn unmount_path_async(mount_point: &Path) -> Result<()> {
    let soft = tokio::time::timeout(
        UMOUNT_GRACE_TIMEOUT,
        tokio::process::Command::new(UMOUNT_BIN)
            .arg(mount_point)
            .output(),
    )
    .await;
    if let Ok(Ok(out)) = &soft
        && out.status.success()
    {
        return Ok(());
    }

    let forced = tokio::process::Command::new(UMOUNT_BIN)
        .arg("-f")
        .arg(mount_point)
        .output()
        .await
        .with_context(|| format!("execute {UMOUNT_BIN} -f {}", mount_point.display()))?;
    if forced.status.success() || !is_active_mountpoint_macos(mount_point) {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&forced.stderr).trim().to_string();
    bail!(
        "failed to unmount {} (umount -f status {}): {}",
        mount_point.display(),
        forced.status,
        stderr
    );
}

#[cfg(not(target_os = "macos"))]
async fn unmount_path_async(_mount_point: &Path) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn unmount_path_sync(mount_point: &Path, try_graceful_first: bool) -> Result<()> {
    if try_graceful_first
        && let Ok(mut child) = std::process::Command::new(UMOUNT_BIN)
            .arg(mount_point)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    {
        let deadline = Instant::now() + DROP_UMOUNT_GRACE_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }

    let forced = std::process::Command::new(UMOUNT_BIN)
        .arg("-f")
        .arg(mount_point)
        .output()
        .with_context(|| format!("execute {UMOUNT_BIN} -f {}", mount_point.display()))?;
    if forced.status.success() || !is_active_mountpoint_macos(mount_point) {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&forced.stderr).trim().to_string();
    bail!(
        "failed to force-unmount {} (status {}): {}",
        mount_point.display(),
        forced.status,
        stderr
    );
}

#[cfg(not(target_os = "macos"))]
fn unmount_path_sync(_mount_point: &Path, _try_graceful_first: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_dir_mode(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn set_private_dir_mode(_path: &Path) {}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(pid_t) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: `kill(pid, 0)` performs an existence/permission check without delivering a signal.
    let rc = unsafe { libc::kill(pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    false
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
