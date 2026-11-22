//! Confined external `aP` file-server supervision, `/srv/<id>` publication, and `/mnt/<id>` binding.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use alan_agent_engine::TrustPosture;
use alan_agent_engine::tools::{
    ReifiedMountAccess, Sandbox, SandboxBackendKind, SandboxHostMount, SandboxSpec, detect_backend,
    seatbelt_profile,
};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, ImportedFileServer, InProcessTransport,
    MAX_WIRE_FRAME_BYTES, Offset, OpenMode, Qid, Stat, WireTransportClient,
};
use alan_kernel::{Access, LiveNamespace, Namespace, SrvFs};
use anyhow::{Context, Result, bail, ensure};
use async_trait::async_trait;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::package::validate_package_id;
use crate::runtime::embed_mounts::RESERVED_MNT_ROOTS;
use crate::temporary_store::TemporaryStore;
use crate::{
    FileserverPackageExport, FileserverTransportSpec, PackageExportKind, PackageReferenceLease,
    PackageService,
};

const MINIMAL_SAFE_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";
const MAX_STDERR_LINES: usize = 64;
const UNIX_SOCKET_MAX_PATH_LEN: usize = 96;

/// Explicit launch-time authorization grant for starting a Quartermaster `fileserver` package.
#[derive(Clone, Debug)]
pub struct FileserverLaunchGrant {
    pub package_id: String,
    pub expected_service_id: Option<String>,
    pub mount_access: Access,
    pub allow_network: bool,
    pub host_mounts: Vec<SandboxHostMount>,
    pub runtime_env: BTreeMap<String, String>,
    pub trust_posture: TrustPosture,
}

impl FileserverLaunchGrant {
    /// Create a least-privilege launch grant (`ReadOnly` mount, network denied, no Host mounts).
    pub fn new(package_id: impl Into<String>) -> Self {
        Self {
            package_id: package_id.into(),
            expected_service_id: None,
            mount_access: Access::ReadOnly,
            allow_network: false,
            host_mounts: Vec::new(),
            runtime_env: BTreeMap::new(),
            trust_posture: TrustPosture::Governed,
        }
    }

    /// Set the access level (`ReadOnly` or `ReadWrite`) granted for `/srv/<id>` and `/mnt/<id>`.
    pub fn with_mount_access(mut self, mount_access: Access) -> Self {
        self.mount_access = mount_access;
        self
    }

    /// Authorize outbound network inside the OS sandbox (only effective if manifest allows it).
    pub fn with_allow_network(mut self, allow_network: bool) -> Self {
        self.allow_network = allow_network;
        self
    }

    /// Supply explicit Host directory mounts granted to the confined subprocess.
    pub fn with_host_mounts(mut self, host_mounts: Vec<SandboxHostMount>) -> Self {
        self.host_mounts = host_mounts;
        self
    }

    /// Supply explicit runtime environment variables from a Host adapter.
    pub fn with_runtime_env(mut self, runtime_env: BTreeMap<String, String>) -> Self {
        self.runtime_env = runtime_env;
        self
    }
}

/// Observable runtime status of one supervised `fileserver` service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileserverStatus {
    pub package_id: String,
    pub service_id: String,
    pub revision: String,
    pub srv_path: String,
    pub mount_path: String,
    pub mount_access: Access,
    pub allow_network: bool,
    pub pid: Option<u32>,
    pub running: bool,
}

/// Supervises explicitly started external `fileserver` packages over confined aP wire transport.
pub struct FileserverManager {
    packages: Arc<PackageService>,
    srvfs: Arc<SrvFs>,
    instances: Mutex<BTreeMap<String, RunningFileserver>>,
}

impl FileserverManager {
    /// Create a supervisor wired to `packages` and Kernel `srvfs` (`/srv`).
    pub fn new(packages: Arc<PackageService>, srvfs: Arc<SrvFs>) -> Self {
        Self {
            packages,
            srvfs,
            instances: Mutex::new(BTreeMap::new()),
        }
    }

    /// Launch a confined external `fileserver` package, verify aP framing, and publish `/srv/<id>`.
    pub async fn start(&self, grant: FileserverLaunchGrant) -> Result<FileserverStatus> {
        validate_package_id(&grant.package_id)?;
        let lease = self.packages.acquire(&grant.package_id)?;
        let record = lease.record().clone();
        let pkg = &grant.package_id;
        ensure!(
            record.export_kind == PackageExportKind::Fileserver,
            "package `{pkg}` is `{}` rather than `fileserver`",
            record.export_kind.as_str()
        );
        let export = record
            .fileserver_export
            .clone()
            .with_context(|| format!("package `{pkg}` lacks a fileserver export"))?;
        let sid = export.service_id.clone();
        if let Some(expected) = grant.expected_service_id.as_deref() {
            ensure!(
                sid == expected,
                "package `{pkg}` exports `{sid}` rather than `{expected}`"
            );
        }
        ensure!(
            !RESERVED_MNT_ROOTS.contains(&sid.as_str()),
            "service `{sid}` is reserved"
        );
        ensure!(
            !self.instances.lock().await.contains_key(&sid),
            "fileserver `{sid}` is running"
        );
        ensure!(
            self.srvfs.lookup(&sid).await.is_none(),
            "/srv/{sid} is already occupied"
        );

        let running = RunningFileserver::launch(lease, export, &grant).await?;
        let (meta, child) = running.snapshot_parts();
        let status = complete_status(meta, &child).await;
        self.srvfs
            .post(&sid, running.transport.clone(), grant.mount_access)
            .await;

        let mut instances = self.instances.lock().await;
        if instances.contains_key(&sid) {
            self.srvfs.unpost(&sid).await;
            running.shutdown().await;
            bail!("fileserver `{sid}` started concurrently");
        }
        instances.insert(sid, running);
        Ok(status)
    }

    /// Explicitly bind `/srv/<service_id>` into `namespace` at `/mnt/<service_id>`.
    pub async fn mount_into_namespace(
        &self,
        service_id: &str,
        namespace: &mut Namespace,
        requested_access: Access,
    ) -> Result<String> {
        let (transport, mount_path) = self.resolve_handle(service_id, requested_access).await?;
        namespace.mount(&mount_path, transport, requested_access);
        Ok(mount_path)
    }

    /// Explicitly bind `/srv/<service_id>` into a live `namespace` at `/mnt/<service_id>`.
    pub async fn mount_into_live_namespace(
        &self,
        service_id: &str,
        namespace: &LiveNamespace,
        requested_access: Access,
    ) -> Result<String> {
        let (transport, mount_path) = self.resolve_handle(service_id, requested_access).await?;
        namespace.replace_mount(&mount_path, transport, requested_access);
        Ok(mount_path)
    }

    async fn resolve_handle(
        &self,
        service_id: &str,
        requested_access: Access,
    ) -> Result<(InProcessTransport, String)> {
        ensure!(
            !RESERVED_MNT_ROOTS.contains(&service_id),
            "cannot bind reserved `/mnt/{service_id}`"
        );
        let (transport, posted) = self
            .srvfs
            .lookup(service_id)
            .await
            .with_context(|| format!("/srv/{service_id} is not published"))?;
        let allowed = matches!(
            (posted, requested_access),
            (Access::ReadWrite, Access::ReadWrite | Access::ReadOnly)
                | (Access::ReadOnly, Access::ReadOnly)
        );
        ensure!(
            allowed,
            "/srv/{service_id} is ReadOnly and cannot escalate to ReadWrite"
        );
        Ok((transport, format!("/mnt/{service_id}")))
    }

    /// Open an independent [`WireTransportClient`] against a running `unix_socket` fileserver.
    pub async fn connect_wire_client(
        &self,
        service_id: &str,
    ) -> Result<WireTransportClient<BufReader<OwnedReadHalf>, OwnedWriteHalf>> {
        let sock = {
            let guard = self.instances.lock().await;
            let inst = guard
                .get(service_id)
                .with_context(|| format!("fileserver `{service_id}` is not running"))?;
            inst.socket_path
                .clone()
                .with_context(|| format!("fileserver `{service_id}` does not use unix_socket"))?
        };
        let stream = UnixStream::connect(&sock)
            .await
            .with_context(|| format!("connect unix wire socket `{}`", sock.display()))?;
        let (read_half, write_half) = stream.into_split();
        Ok(WireTransportClient::new(
            BufReader::new(read_half),
            write_half,
        ))
    }

    /// Stop one running `fileserver`, unpost `/srv/<service_id>`, and kill its process group.
    pub async fn stop(&self, service_id: &str) -> Result<()> {
        let running = self
            .instances
            .lock()
            .await
            .remove(service_id)
            .with_context(|| format!("fileserver `{service_id}` is not running"))?;
        self.srvfs.unpost(service_id).await;
        running.shutdown().await;
        Ok(())
    }

    /// Stop all running `fileserver` instances cleanly.
    pub async fn stop_all(&self) -> Result<()> {
        let drained = std::mem::take(&mut *self.instances.lock().await);
        for (service_id, running) in drained {
            self.srvfs.unpost(&service_id).await;
            running.shutdown().await;
        }
        Ok(())
    }

    /// Return current runtime status for `service_id`, if started.
    pub async fn status(&self, service_id: &str) -> Option<FileserverStatus> {
        let (meta, child) = {
            let guard = self.instances.lock().await;
            guard
                .get(service_id)
                .map(RunningFileserver::snapshot_parts)?
        };
        Some(complete_status(meta, &child).await)
    }

    /// List all supervised `fileserver` instances.
    pub async fn list(&self) -> Vec<FileserverStatus> {
        let items: Vec<_> = {
            let guard = self.instances.lock().await;
            guard
                .values()
                .map(RunningFileserver::snapshot_parts)
                .collect()
        };
        let mut out = Vec::with_capacity(items.len());
        for (meta, child) in items {
            out.push(complete_status(meta, &child).await);
        }
        out
    }

    /// Return recent bounded stderr lines captured from `service_id`.
    pub async fn recent_stderr(&self, service_id: &str) -> Option<String> {
        let sink = Arc::clone(&self.instances.lock().await.get(service_id)?.stderr_lines);
        Some(read_stderr_lines(&sink).await)
    }
}

async fn complete_status(
    mut meta: FileserverStatus,
    child: &Mutex<Option<Child>>,
) -> FileserverStatus {
    meta.running = matches!(
        child.lock().await.as_mut().map(Child::try_wait),
        Some(Ok(None))
    );
    meta
}

struct RunningFileserver {
    base_status: FileserverStatus,
    socket_path: Option<PathBuf>,
    proxy: Arc<SupervisedFileserver>,
    transport: InProcessTransport,
    guard: FileserverProcessGuard,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
    _lease: Arc<PackageReferenceLease>,
    _runtime_dir: TemporaryStore,
    _socket_dir: Option<tempfile::TempDir>,
}

impl RunningFileserver {
    async fn launch(
        lease: Arc<PackageReferenceLease>,
        export: FileserverPackageExport,
        grant: &FileserverLaunchGrant,
    ) -> Result<Self> {
        validate_runtime_env(&grant.runtime_env)?;
        let content_root = lease.disk_content_root()?;
        let runtime_dir = TemporaryStore::new("fileserver")?;
        let runtime_path = fs_canonical_or_owned(runtime_dir.path());
        let allow_network = export.allow_network && grant.allow_network;
        let timeout = Duration::from_secs(export.timeout_secs.max(1) as u64);

        let (executable, socket_dir, socket_path) = match &export.transport {
            FileserverTransportSpec::Stdio { command, .. } => {
                (lease.content_file_path(command)?, None, None)
            }
            FileserverTransportSpec::UnixSocket { socket_name } => {
                let exe = lease.content_file_path(&export.entrypoint)?;
                let dir = create_private_socket_dir()?;
                let sock = fs_canonical_or_owned(dir.path()).join(socket_name);
                ensure!(
                    sock.as_os_str().len() <= UNIX_SOCKET_MAX_PATH_LEN,
                    "unix_socket path `{}` exceeds OS sun_path budget",
                    sock.display()
                );
                (exe, Some(dir), Some(sock))
            }
        };

        let (ro, rw) = (ReifiedMountAccess::ReadOnly, ReifiedMountAccess::ReadWrite);
        let mut mounts = vec![
            host_mount("/tmp/fileserver-runtime", &runtime_path, rw),
            host_mount("/lib/pkg", &content_root, ro),
        ];
        if let Some(dir) = socket_dir.as_ref() {
            mounts.push(host_mount(
                "/tmp/fileserver-sock",
                &fs_canonical_or_owned(dir.path()),
                rw,
            ));
        }
        mounts.extend(grant.host_mounts.iter().cloned());

        let spec = SandboxSpec::from_host_mounts(&mounts);
        let mut command = prepare_confined_command(
            grant,
            &export,
            &spec,
            &executable,
            &runtime_path,
            socket_path.as_deref(),
        )?;

        let sid = &export.service_id;
        let mut child = command.spawn().with_context(|| format!("spawn `{sid}`"))?;
        let pid = child.id();
        let stderr = child.stderr.take().context("fileserver missing stderr")?;
        let stderr_lines = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_STDERR_LINES)));
        let mut drain_tasks = vec![tokio::spawn(drain_stderr(stderr, stderr_lines.clone()))];

        let imported: Arc<dyn FileServer> = if let Some(sock) = socket_path.as_ref() {
            let mut stdout = child.stdout.take().context("unix_socket missing stdout")?;
            drain_tasks.push(tokio::spawn(async move {
                let _ = tokio::io::copy(&mut stdout, &mut tokio::io::sink()).await;
            }));
            let stream = connect_unix_until_ready(sock, &mut child, timeout, &stderr_lines).await?;
            let (r, w) = stream.into_split();
            Arc::new(ImportedFileServer::new(
                BoundedLineReader::new(BufReader::new(r), export.max_message_bytes),
                w,
            ))
        } else {
            let stdin = child
                .stdin
                .take()
                .context("stdio fileserver missing stdin")?;
            let stdout = child.stdout.take().context("stdio missing stdout")?;
            Arc::new(ImportedFileServer::new(
                BoundedLineReader::new(BufReader::new(stdout), export.max_message_bytes),
                stdin,
            ))
        };

        let guard = FileserverProcessGuard {
            child: Arc::new(Mutex::new(Some(child))),
            tasks: Mutex::new(drain_tasks),
        };
        let proxy = Arc::new(SupervisedFileserver::new(
            imported,
            timeout,
            export.max_message_bytes,
        ));
        if let Err(err) = match tokio::time::timeout(timeout, proxy.stat(Fid::ROOT)).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(code)) => Err(anyhow::anyhow!("stat failed: {code:?}")),
            Err(elapsed) => Err(anyhow::anyhow!("timed out: {elapsed}")),
        } {
            guard.shutdown().await;
            let stderr = read_stderr_lines(&stderr_lines).await;
            return Err(err.context(format!("fileserver `{sid}` startup failed ({stderr})")));
        }

        let base_status = FileserverStatus {
            package_id: grant.package_id.clone(),
            srv_path: format!("/srv/{sid}"),
            mount_path: format!("/mnt/{sid}"),
            service_id: export.service_id,
            revision: lease.record().revision.clone(),
            mount_access: grant.mount_access,
            allow_network,
            pid,
            running: false,
        };
        Ok(Self {
            base_status,
            socket_path,
            transport: InProcessTransport::new(proxy.clone()),
            proxy,
            guard,
            stderr_lines,
            _lease: lease,
            _runtime_dir: runtime_dir,
            _socket_dir: socket_dir,
        })
    }

    fn snapshot_parts(&self) -> (FileserverStatus, Arc<Mutex<Option<Child>>>) {
        (self.base_status.clone(), Arc::clone(&self.guard.child))
    }

    async fn shutdown(self) {
        self.proxy.deactivate().await;
        self.guard.shutdown().await;
    }
}

fn host_mount(ns: &str, host: &Path, access: ReifiedMountAccess) -> SandboxHostMount {
    SandboxHostMount {
        namespace_path: PathBuf::from(ns),
        host_path: host.to_path_buf(),
        access,
    }
}

fn fs_canonical_or_owned(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn create_private_socket_dir() -> Result<tempfile::TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("alan-sock-")
        .tempdir_in("/tmp")
        .context("create private unix socket directory in /tmp")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

fn prepare_confined_command(
    grant: &FileserverLaunchGrant,
    export: &FileserverPackageExport,
    spec: &SandboxSpec,
    executable: &Path,
    runtime_dir: &Path,
    socket_path: Option<&Path>,
) -> Result<Command> {
    let allow_network = export.allow_network && grant.allow_network;
    let (args, manifest_env) = match &export.transport {
        FileserverTransportSpec::Stdio { args, env, .. } => (args.as_slice(), Some(env)),
        FileserverTransportSpec::UnixSocket { .. } => (&[][..], None),
    };
    let is_unix_socket = socket_path.is_some();
    let mut command = if is_unix_socket
        && !allow_network
        && !grant.trust_posture.is_unsandboxed_host_account()
        && detect_backend() == SandboxBackendKind::Seatbelt
    {
        // Seatbelt `(deny network*)` rejects AF_UNIX bind/connect unless `(local unix)` is granted.
        let mut profile = seatbelt_profile(&spec.writable_roots, &spec.read_denylist, false);
        profile = profile.replacen(
            "(deny network*)\n",
            "(deny network*)\n(allow network-bind (local unix))\n(allow network-inbound (local unix))\n(allow network-outbound (local unix))\n",
            1,
        );
        profile.push_str(&seatbelt_read_only_rules(spec));
        let mut cmd = Command::new("/usr/bin/sandbox-exec");
        cmd.arg("-p").arg(profile).arg(executable).args(args);
        cmd.current_dir(runtime_dir);
        cmd
    } else {
        let sandbox = Sandbox::from_spec(spec.clone()).with_trust_posture(grant.trust_posture);
        sandbox.build_confined_program_command(executable, args, runtime_dir, allow_network)?
    };

    let path_env = std::env::var_os("PATH").unwrap_or_else(|| MINIMAL_SAFE_PATH.into());
    command
        .env_clear()
        .env("PATH", path_env)
        .env("HOME", runtime_dir)
        .env("TMPDIR", runtime_dir)
        .env("ALAN_FILESERVER_ID", &export.service_id)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stdin(if is_unix_socket {
            std::process::Stdio::null()
        } else {
            std::process::Stdio::piped()
        })
        .kill_on_drop(true);
    for (k, v) in manifest_env.into_iter().flatten().chain(&grant.runtime_env) {
        command.env(k, v);
    }
    if let Some(sock) = socket_path {
        command.env("ALAN_FILESERVER_SOCKET", sock);
    }
    #[cfg(unix)]
    command.process_group(0);
    Ok(command)
}

fn seatbelt_read_only_rules(spec: &SandboxSpec) -> String {
    let esc = |p: &Path| {
        fs_canonical_or_owned(p)
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    };
    spec.readable_roots
        .iter()
        .filter(|root| !spec.writable_roots.contains(root))
        .map(|root| {
            let ex: Vec<_> = spec
                .writable_roots
                .iter()
                .filter(|w| w.starts_with(root))
                .map(|w| format!("(require-not (subpath \"{}\"))", esc(w)))
                .collect();
            if ex.is_empty() {
                format!("(deny file-write* (subpath \"{}\"))\n", esc(root))
            } else {
                format!(
                    "(deny file-write* (require-all (subpath \"{}\") {}))\n",
                    esc(root),
                    ex.join(" ")
                )
            }
        })
        .collect()
}

const BAD_ENV: &[&str] = &[
    "ALAN_FILESERVER_ID",
    "ALAN_FILESERVER_SOCKET",
    "HOME",
    "TMPDIR",
    "PATH",
];

fn validate_runtime_env(env: &BTreeMap<String, String>) -> Result<()> {
    for (key, value) in env {
        let ok =
            !key.trim().is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        ensure!(ok, "invalid runtime env `{key}`");
        ensure!(!value.contains('\0'), "runtime env `{key}` has null bytes");
        ensure!(
            !BAD_ENV.contains(&key.as_str()),
            "runtime env must not override `{key}`"
        );
    }
    Ok(())
}

async fn connect_unix_until_ready(
    socket_path: &Path,
    child: &mut Child,
    timeout: Duration,
    stderr_lines: &Arc<Mutex<VecDeque<String>>>,
) -> Result<UnixStream> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            let stderr = read_stderr_lines(stderr_lines).await;
            bail!("unix_socket exited before listening ({status}; stderr: {stderr})");
        }
        if socket_path.exists()
            && let Ok(stream) = UnixStream::connect(socket_path).await
        {
            return Ok(stream);
        }
        if Instant::now() >= deadline {
            let stderr = read_stderr_lines(stderr_lines).await;
            bail!(
                "unix_socket did not bind `{}` (stderr: {stderr})",
                socket_path.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn kill_child_process_group(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id().and_then(|id| i32::try_from(id).ok()) {
        // SAFETY: `pid` is the positive process group leader initialized with `process_group(0)`.
        unsafe { libc::kill(-pid, libc::SIGKILL) };
    }
    let _ = child.start_kill();
}

struct FileserverProcessGuard {
    child: Arc<Mutex<Option<Child>>>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl FileserverProcessGuard {
    async fn shutdown(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            kill_child_process_group(&mut child);
            let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
        }
        for handle in self.tasks.lock().await.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for FileserverProcessGuard {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.child.try_lock()
            && let Some(mut child) = guard.take()
        {
            kill_child_process_group(&mut child);
        }
        if let Ok(mut tasks) = self.tasks.try_lock() {
            for handle in tasks.drain(..) {
                handle.abort();
            }
        }
    }
}

struct SupervisedFileserver {
    inner: RwLock<Option<Arc<dyn FileServer>>>,
    timeout: Duration,
    max_payload_chunk: usize,
    stream_fids: Mutex<HashSet<Fid>>,
}

impl SupervisedFileserver {
    fn new(inner: Arc<dyn FileServer>, timeout: Duration, max_message_bytes: usize) -> Self {
        // JSON byte arrays can take up to 5 bytes per byte ("255, "), plus envelope overhead.
        let max_payload_chunk =
            (max_message_bytes.saturating_sub(256) / 5).clamp(128, MAX_WIRE_FRAME_BYTES / 8);
        Self {
            inner: RwLock::new(Some(inner)),
            timeout,
            max_payload_chunk,
            stream_fids: Mutex::new(HashSet::new()),
        }
    }

    async fn deactivate(&self) {
        *self.inner.write().await = None;
        self.stream_fids.lock().await.clear();
    }

    async fn server(&self) -> Result<Arc<dyn FileServer>, ErrorCode> {
        self.inner
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or(ErrorCode::NoAccess)
    }

    async fn timed<T>(
        &self,
        fut: impl std::future::Future<Output = Result<T, ErrorCode>>,
    ) -> Result<T, ErrorCode> {
        tokio::time::timeout(self.timeout, fut)
            .await
            .map_err(|_| ErrorCode::Io)?
    }

    async fn track_stream_fid(&self, fid: Fid, qid: Qid) {
        if qid.kind == FileKind::Stream {
            self.stream_fids.lock().await.insert(fid);
        }
    }
}

#[async_trait]
impl FileServer for SupervisedFileserver {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let qid = self
            .timed(self.server().await?.walk(fid, newfid, names))
            .await?;
        self.track_stream_fid(newfid, qid).await;
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let qid = self.timed(self.server().await?.open(fid, mode)).await?;
        self.track_stream_fid(fid, qid).await;
        Ok(qid)
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let server = self.server().await?;
        let bounded = count.min(self.max_payload_chunk as u32);
        if self.stream_fids.lock().await.contains(&fid) {
            server.read(fid, offset, bounded).await
        } else {
            self.timed(server.read(fid, offset, bounded)).await
        }
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let server = self.server().await?;
        if data.is_empty() {
            return self.timed(server.write(fid, offset, data)).await;
        }
        let mut accepted_total = 0usize;
        for chunk in data.chunks(self.max_payload_chunk) {
            let off = offset
                .checked_add(accepted_total as u64)
                .ok_or(ErrorCode::BadRequest)?;
            let accepted = self.timed(server.write(fid, off, chunk)).await? as usize;
            if accepted > chunk.len() {
                return Err(ErrorCode::BadRequest);
            }
            accepted_total = accepted_total
                .checked_add(accepted)
                .ok_or(ErrorCode::BadRequest)?;
            if accepted < chunk.len() {
                break;
            }
        }
        u32::try_from(accepted_total).map_err(|_| ErrorCode::BadRequest)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        self.timed(self.server().await?.stat(fid)).await
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        let qid = self
            .timed(self.server().await?.create(fid, newfid, name, kind))
            .await?;
        self.track_stream_fid(newfid, qid).await;
        Ok(qid)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.stream_fids.lock().await.remove(&fid);
        self.timed(self.server().await?.remove(fid)).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.stream_fids.lock().await.remove(&fid);
        self.timed(self.server().await?.clunk(fid)).await
    }
}

/// Enforces `max_message_bytes` per newline-delimited wire response frame.
struct BoundedLineReader<R> {
    inner: R,
    max_bytes: usize,
    line_bytes: usize,
    last_nl_offset: Option<usize>,
}

impl<R> BoundedLineReader<R> {
    fn new(inner: R, max_bytes: usize) -> Self {
        Self {
            inner,
            max_bytes,
            line_bytes: 0,
            last_nl_offset: None,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for BoundedLineReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<R: AsyncBufRead + Unpin> AsyncBufRead for BoundedLineReader<R> {
    fn poll_fill_buf(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
    ) -> Poll<std::io::Result<&[u8]>> {
        let me = self.get_mut();
        let buf = std::task::ready!(Pin::new(&mut me.inner).poll_fill_buf(cx))?;
        let (frame_len, slice, nl_offset) = match buf.iter().position(|b| *b == b'\n') {
            Some(nl) => (nl + 1, &buf[..=nl], Some(nl)),
            None => (buf.len(), buf, None),
        };
        if me.line_bytes.saturating_add(frame_len) > me.max_bytes {
            let msg = format!("response frame exceeded {} bytes", me.max_bytes);
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                msg,
            )));
        }
        me.last_nl_offset = nl_offset;
        Poll::Ready(Ok(slice))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let me = self.get_mut();
        let done = me.last_nl_offset.is_some_and(|nl| amt > nl);
        me.line_bytes = if done {
            0
        } else {
            me.line_bytes.saturating_add(amt)
        };
        me.last_nl_offset = None;
        Pin::new(&mut me.inner).consume(amt);
    }
}

async fn drain_stderr(stderr: ChildStderr, sink: Arc<Mutex<VecDeque<String>>>) {
    let mut lines = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut sink = sink.lock().await;
        if sink.len() >= MAX_STDERR_LINES {
            sink.pop_front();
        }
        sink.push_back(trimmed.chars().take(512).collect());
    }
}

async fn read_stderr_lines(sink: &Arc<Mutex<VecDeque<String>>>) -> String {
    Vec::from_iter(sink.lock().await.iter().cloned()).join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    struct MockTarget(StdMutex<Vec<usize>>);

    fn qid(kind: FileKind, path: u64) -> Qid {
        Qid {
            kind,
            version: 0,
            path,
        }
    }

    #[async_trait]
    impl FileServer for MockTarget {
        async fn walk(&self, _: Fid, nf: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
            let k = match names.first().map(String::as_str) {
                Some("events") => FileKind::Stream,
                Some(_) => FileKind::File,
                None => FileKind::Dir,
            };
            Ok(qid(k, nf.0))
        }
        async fn open(&self, fid: Fid, _: OpenMode) -> Result<Qid, ErrorCode> {
            let k = if fid.0 == 2 {
                FileKind::Stream
            } else {
                FileKind::File
            };
            Ok(qid(k, fid.0))
        }
        async fn read(&self, fid: Fid, _: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
            if fid.0 == 2 || fid.0 == 3 {
                tokio::time::sleep(Duration::from_millis(85)).await;
                return Ok(if fid.0 == 2 { b"ev".to_vec() } else { vec![] });
            }
            Ok(vec![b'x'; count as usize])
        }
        async fn write(&self, _: Fid, _: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
            self.0.lock().unwrap().push(data.len());
            Ok(data.len() as u32)
        }
        async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
            Ok(Stat {
                name: "m".into(),
                qid: qid(FileKind::File, fid.0),
                length: 0,
                executable: false,
                writable: false,
            })
        }
        async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
            Err(ErrorCode::Unsupported)
        }
        async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
            Err(ErrorCode::Unsupported)
        }
        async fn clunk(&self, _: Fid) -> Result<(), ErrorCode> {
            Ok(())
        }
    }

    #[test]
    fn runtime_env_and_seatbelt_rules_validate_boundaries() {
        for &bad in BAD_ENV {
            assert!(validate_runtime_env(&BTreeMap::from([(bad.into(), "x".into())])).is_err());
        }
        assert!(validate_runtime_env(&BTreeMap::from([("T".into(), "a\0b".into())])).is_err());
        assert!(validate_runtime_env(&BTreeMap::from([("T".into(), "ok".into())])).is_ok());

        let d = tempfile::tempdir().unwrap();
        let rw_dir = d.path().join("rw");
        std::fs::create_dir(&rw_dir).unwrap();
        let ro = host_mount("/ro", d.path(), ReifiedMountAccess::ReadOnly);
        let rw = host_mount("/rw", &rw_dir, ReifiedMountAccess::ReadWrite);
        let rules = seatbelt_read_only_rules(&SandboxSpec::from_host_mounts(&[ro, rw]));
        assert!(rules.contains("deny file-write*") && rules.contains("require-not (subpath"));
    }

    #[tokio::test]
    async fn bounded_line_reader_and_supervised_proxy_enforce_limits() {
        let mut ok_r = BoundedLineReader::new(BufReader::new(&b"a\nb\n"[..]), 8);
        assert_eq!(ok_r.read_line(&mut String::new()).await.unwrap(), 2);
        let bad = format!("{}\n", "a".repeat(200));
        let mut err_r = BoundedLineReader::new(BufReader::new(bad.as_bytes()), 128);
        let err = err_r.read_line(&mut String::new()).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        let mock = Arc::new(MockTarget(StdMutex::new(Vec::new())));
        let p = SupervisedFileserver::new(mock.clone(), Duration::from_millis(50), 1024);
        let ev_q = p.walk(Fid::ROOT, Fid(2), &["events".into()]).await.unwrap();
        let sl_q = p.walk(Fid::ROOT, Fid(3), &["slow".into()]).await.unwrap();
        assert_eq!((ev_q.kind, sl_q.kind), (FileKind::Stream, FileKind::File));
        assert_eq!(p.read(Fid(2), 0, 64).await.unwrap(), b"ev");
        assert_eq!(p.read(Fid(3), 0, 64).await, Err(ErrorCode::Io));
        assert_eq!(p.write(Fid(1), 0, &[b'a'; 500]).await.unwrap(), 500);
        assert!(*mock.0.lock().unwrap().iter().max().unwrap() <= p.max_payload_chunk);
        let n = p.read(Fid(1), 0, 500).await.unwrap().len();
        assert_eq!(n, p.max_payload_chunk);
    }
}
