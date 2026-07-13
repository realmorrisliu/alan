use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use alan_ap::{ImportedFileServer, InProcessTransport, export_file_server};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::composition::{BOOT_ID_PATH, BOOT_STATE_PATH, FixedBootConfig, FixedComposition};

const STATUS_VERSION: u16 = 1;
const SOCKET_FILE: &str = "namespace.ap.sock";
const STATUS_FILE: &str = "host.json";
const LOCK_FILE: &str = "host.lock";
const ATTACHMENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostEndpointPaths {
    pub channel_id: String,
    pub root: PathBuf,
    pub socket: PathBuf,
    pub status: PathBuf,
    pub lock: PathBuf,
}

impl HostEndpointPaths {
    pub fn detect(channel_id: &str) -> Result<Self> {
        let base = dirs::runtime_dir()
            .unwrap_or_else(|| std::env::temp_dir().join(format!("alan-os-{}", current_uid())));
        Self::from_runtime_dir(&base, channel_id)
    }

    pub fn from_runtime_dir(runtime_dir: &Path, channel_id: &str) -> Result<Self> {
        validate_channel_id(channel_id)?;
        validate_absolute_path("platform runtime directory", runtime_dir)?;
        let root = runtime_dir.join("Alan OS").join(channel_id);
        let paths = Self {
            channel_id: channel_id.to_string(),
            socket: root.join(SOCKET_FILE),
            status: root.join(STATUS_FILE),
            lock: root.join(LOCK_FILE),
            root,
        };
        #[cfg(target_os = "macos")]
        ensure!(
            paths.socket.as_os_str().len() < 104,
            "Alan OS Host socket path exceeds the macOS limit: {}",
            paths.socket.display()
        );
        Ok(paths)
    }

    fn prepare_private_root(&self) -> Result<()> {
        let product_root = self
            .root
            .parent()
            .context("Host runtime root has no product parent")?;
        let runtime_root = product_root
            .parent()
            .context("Host runtime root has no platform parent")?;
        ensure_private_directory(runtime_root)?;
        ensure_private_directory(product_root)?;
        ensure_private_directory(&self.root)?;
        Ok(())
    }

    pub fn read_status(&self) -> Result<HostStatus> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.status)
            .with_context(|| format!("open Host status {}", self.status.display()))?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.file_type().is_file(),
            "Host status path is not a file"
        );
        ensure!(
            metadata.uid() == current_uid(),
            "Host status has a foreign owner"
        );
        ensure!(metadata.mode() & 0o077 == 0, "Host status is not private");
        let status: HostStatus = serde_json::from_reader(file)?;
        status.validate_for(self)?;
        Ok(status)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostReadiness {
    Ready,
    Stopping,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostStatus {
    pub version: u16,
    pub channel_id: String,
    pub boot_id: Uuid,
    pub pid: u32,
    pub readiness: HostReadiness,
    pub socket: PathBuf,
}

impl HostStatus {
    fn validate_for(&self, paths: &HostEndpointPaths) -> Result<()> {
        ensure!(
            self.version == STATUS_VERSION,
            "unsupported Host status version"
        );
        ensure!(
            self.channel_id == paths.channel_id,
            "Host status channel mismatch"
        );
        ensure!(self.pid > 0, "Host status pid must be positive");
        ensure!(self.socket == paths.socket, "Host status socket mismatch");
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostProcessReference {
    pub boot_id: Uuid,
    pub pid: u64,
}

impl HostProcessReference {
    pub fn validate(self, attachment: &AttachedNamespace) -> Result<u64> {
        ensure!(
            self.boot_id == attachment.boot_id,
            "Process Reference belongs to an earlier Alan OS boot"
        );
        Ok(self.pid)
    }
}

pub struct AlanOsHost {
    paths: HostEndpointPaths,
    status: HostStatus,
    listener: UnixListener,
    singleton: SingletonLock,
    composition: FixedComposition,
}

impl AlanOsHost {
    pub async fn boot(config: FixedBootConfig, paths: HostEndpointPaths) -> Result<Self> {
        ensure!(
            config.channel_id == paths.channel_id,
            "Host channel/path mismatch"
        );
        paths.prepare_private_root()?;
        let singleton = SingletonLock::acquire(&paths.lock)?;
        remove_stale_owned_file(&paths.socket)?;
        remove_stale_owned_file(&paths.status)?;

        let composition = FixedComposition::boot(config).await?;
        let listener = UnixListener::bind(&paths.socket)
            .with_context(|| format!("bind Alan OS attachment {}", paths.socket.display()))?;
        std::fs::set_permissions(&paths.socket, std::fs::Permissions::from_mode(0o600))?;
        verify_owned_private_file(&paths.socket, true)?;

        let status = HostStatus {
            version: STATUS_VERSION,
            channel_id: paths.channel_id.clone(),
            boot_id: composition.boot_id(),
            pid: std::process::id(),
            readiness: HostReadiness::Ready,
            socket: paths.socket.clone(),
        };
        write_status(&paths.status, &status)?;

        Ok(Self {
            paths,
            status,
            listener,
            singleton,
            composition,
        })
    }

    pub fn status(&self) -> &HostStatus {
        &self.status
    }

    pub async fn serve_until<F>(mut self, shutdown: F) -> Result<()>
    where
        F: std::future::Future<Output = ()>,
    {
        tokio::pin!(shutdown);
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted.context("accept Alan OS attachment")?;
                    if peer_uid(&stream)? != current_uid() {
                        tracing::warn!("rejected Alan OS attachment from foreign uid");
                        continue;
                    }
                    let namespace = self.composition.attachment_server();
                    connections.spawn(async move {
                        let (read, write) = stream.into_split();
                        let result = export_file_server(
                            namespace.clone(),
                            BufReader::new(read),
                            write,
                        )
                        .await;
                        namespace.clunk_all().await;
                        result
                    });
                }
                completed = connections.join_next(), if !connections.is_empty() => {
                    match completed {
                        Some(Ok(Err(error))) => {
                            tracing::warn!(%error, "Alan OS attachment transport failed");
                        }
                        Some(Err(error)) => {
                            tracing::warn!(%error, "Alan OS attachment task failed");
                        }
                        _ => {}
                    }
                }
            }
        }

        self.status.readiness = HostReadiness::Stopping;
        let _ = write_status(&self.paths.status, &self.status);
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        let shutdown = self.composition.shutdown().await;
        remove_owned_file(&self.paths.socket);
        remove_owned_file(&self.paths.status);
        drop(self.singleton);
        shutdown
    }
}

#[derive(Clone, Debug)]
pub struct LocalAttachment {
    paths: HostEndpointPaths,
}

impl LocalAttachment {
    pub fn new(paths: HostEndpointPaths) -> Self {
        Self { paths }
    }

    pub fn detect(channel_id: &str) -> Result<Self> {
        Ok(Self::new(HostEndpointPaths::detect(channel_id)?))
    }

    pub async fn connect(&self) -> Result<AttachedNamespace> {
        let status = self.paths.read_status()?;
        ensure!(
            status.readiness == HostReadiness::Ready,
            "Alan OS Host is not ready"
        );
        tokio::time::timeout(ATTACHMENT_CONNECT_TIMEOUT, async {
            let stream = UnixStream::connect(&self.paths.socket)
                .await
                .with_context(|| {
                    format!("attach to Alan OS Host at {}", self.paths.socket.display())
                })?;
            let (read, write) = stream.into_split();
            let imported = Arc::new(ImportedFileServer::new(BufReader::new(read), write));
            let root = InProcessTransport::new(imported);
            let shell = alan_shell::Shell::new(root.clone());
            let boot_id = String::from_utf8(shell.cat(BOOT_ID_PATH).await?)?
                .trim()
                .parse::<Uuid>()
                .context("Alan OS namespace boot ID is invalid")?;
            ensure!(
                boot_id == status.boot_id,
                "Host status and namespace boot ID differ"
            );
            ensure!(
                shell.cat(BOOT_STATE_PATH).await? == b"ready\n",
                "Alan OS namespace is not ready"
            );
            shell.ls("/agent/root").await.context("read /agent/root")?;
            Ok(AttachedNamespace {
                boot_id,
                root,
                status,
            })
        })
        .await
        .with_context(|| {
            format!(
                "timed out attaching to Alan OS Host at {}",
                self.paths.socket.display()
            )
        })?
    }
}

pub struct AttachedNamespace {
    pub boot_id: Uuid,
    pub root: InProcessTransport,
    pub status: HostStatus,
}

impl AttachedNamespace {
    pub fn process_reference(&self, pid: u64) -> HostProcessReference {
        HostProcessReference {
            boot_id: self.boot_id,
            pid,
        }
    }
}

struct SingletonLock {
    file: File,
}

impl SingletonLock {
    fn acquire(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("open Host singleton lock {}", path.display()))?;
        verify_owned_private_file(path, false)?;
        // SAFETY: file owns a valid descriptor for the lifetime of the lock.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            if matches!(error.raw_os_error(), Some(libc::EWOULDBLOCK)) {
                bail!("Alan OS Host is already running")
            }
            return Err(error).context("acquire Host singleton lock");
        }
        Ok(Self { file })
    }
}

impl Drop for SingletonLock {
    fn drop(&mut self) {
        // SAFETY: the descriptor remains valid until file is dropped after this method.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

fn write_status(path: &Path, status: &HostStatus) -> Result<()> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)?;
    serde_json::to_writer_pretty(&mut file, status)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    verify_owned_private_file(path, false)
}

fn remove_stale_owned_file(path: &Path) -> Result<()> {
    let Some(metadata) = std::fs::symlink_metadata(path).ok() else {
        return Ok(());
    };
    ensure!(
        !metadata.file_type().is_symlink(),
        "refusing to remove symlink {}",
        path.display()
    );
    ensure!(
        metadata.uid() == current_uid(),
        "refusing to remove foreign file {}",
        path.display()
    );
    std::fs::remove_file(path).with_context(|| format!("remove stale {}", path.display()))
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    match std::fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("create directory {}", path.display()));
        }
    }
    let directory = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("open private directory {}", path.display()))?;
    let metadata = directory.metadata()?;
    ensure!(
        metadata.uid() == current_uid(),
        "directory has a foreign owner: {}",
        path.display()
    );
    directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn remove_owned_file(path: &Path) {
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && !metadata.file_type().is_symlink()
        && metadata.uid() == current_uid()
    {
        let _ = std::fs::remove_file(path);
    }
}

fn verify_owned_private_file(path: &Path, socket: bool) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "path is a symlink: {}",
        path.display()
    );
    if socket {
        ensure!(
            metadata.file_type().is_socket(),
            "path is not a socket: {}",
            path.display()
        );
    } else {
        ensure!(
            metadata.file_type().is_file(),
            "path is not a file: {}",
            path.display()
        );
    }
    ensure!(
        metadata.uid() == current_uid(),
        "path has a foreign owner: {}",
        path.display()
    );
    ensure!(
        metadata.mode() & 0o077 == 0,
        "path is not private: {}",
        path.display()
    );
    Ok(())
}

fn validate_channel_id(channel_id: &str) -> Result<()> {
    ensure!(
        matches!(channel_id, "stable" | "dev" | "test"),
        "invalid channel {channel_id}"
    );
    Ok(())
}

fn validate_absolute_path(label: &str, path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute(),
        "{label} must be absolute: {}",
        path.display()
    );
    ensure!(
        !path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir)),
        "{label} must not contain relative components: {}",
        path.display()
    );
    Ok(())
}

fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() }
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let mut uid = 0;
    let mut gid = 0;
    // SAFETY: descriptor and both output pointers are valid.
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if result == 0 {
        Ok(uid)
    } else {
        Err(std::io::Error::last_os_error()).context("read Unix peer identity")
    }
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: descriptor and the buffer/length pair are valid.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result == 0 {
        Ok(credentials.uid)
    } else {
        Err(std::io::Error::last_os_error()).context("read Unix peer credentials")
    }
}

pub async fn run_host_process(channel_id: &str) -> Result<()> {
    let paths = HostEndpointPaths::detect(channel_id)?;
    let config = FixedBootConfig::product(channel_id)?;
    let host = AlanOsHost::boot(config, paths).await?;
    host.serve_until(shutdown_signal()).await
}

/// Request whole-system shutdown after proving the status file and namespace
/// describe the same live boot.
pub async fn request_host_stop(paths: &HostEndpointPaths) -> Result<HostStatus> {
    let mut status = paths.read_status()?;
    let attachment = LocalAttachment::new(paths.clone()).connect().await?;
    ensure!(
        attachment.boot_id == status.boot_id,
        "refusing to stop a Host whose boot identity changed"
    );
    let pid = i32::try_from(status.pid).context("Host pid does not fit platform pid_t")?;
    // SAFETY: kill is called with a validated positive pid and SIGTERM.
    let result = unsafe { libc::kill(pid, libc::SIGTERM) };
    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("request Alan OS Host shutdown");
    }
    status.readiness = HostReadiness::Stopping;
    Ok(status)
}

async fn shutdown_signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_status_rejects_zero_pid() {
        let paths = HostEndpointPaths::from_runtime_dir(&std::env::temp_dir(), "test").unwrap();
        let status = HostStatus {
            version: STATUS_VERSION,
            channel_id: paths.channel_id.clone(),
            boot_id: Uuid::new_v4(),
            pid: 0,
            readiness: HostReadiness::Ready,
            socket: paths.socket.clone(),
        };

        assert_eq!(
            status.validate_for(&paths).unwrap_err().to_string(),
            "Host status pid must be positive"
        );
    }
}
