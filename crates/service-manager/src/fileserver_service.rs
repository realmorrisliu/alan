//! Confined external `aP` file-server supervision, `/srv/<id>` publication, and `/mnt/<id>` binding.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use alan_agent_engine::TrustPosture;
use alan_agent_engine::tools::{
    ReifiedMountAccess, Sandbox, SandboxBackendKind, SandboxHostMount, SandboxSpec, detect_backend,
    seatbelt_profile,
};
use alan_ap::{Fid, FileServer, ImportedFileServer, InProcessTransport, WireTransportClient};
use alan_kernel::{Access, LiveNamespace, Namespace, SrvFs};
use anyhow::{Context, Result, bail, ensure};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::package::validate_package_id;
use crate::runtime::embed_mounts::RESERVED_MNT_ROOTS;
use crate::temporary_store::TemporaryStore;
use crate::{
    FileserverPackageExport, FileserverTransportSpec, PackageExportKind, PackageReferenceLease,
    PackageService,
};

mod proxy;
use proxy::{BoundedLineReader, SupervisedFileserver};

const MINIMAL_SAFE_PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";
const MAX_STDERR_LINES: usize = 64;
const UNIX_SOCKET_MAX_PATH_LEN: usize = 96;
const GITHUB_FILESERVER_TOKEN_TIMEOUT: Duration = Duration::from_secs(3);

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

    /// Set explicit confinement trust posture for this fileserver launch.
    pub fn with_trust_posture(mut self, trust_posture: TrustPosture) -> Self {
        self.trust_posture = trust_posture;
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

enum ServiceSlot {
    Starting(u64),
    Running(Box<RunningFileserver>),
}

struct StartingReservation {
    instances: Arc<StdMutex<BTreeMap<String, ServiceSlot>>>,
    service_id: String,
    launch_id: u64,
    armed: bool,
}

impl Drop for StartingReservation {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut guard) = self.instances.lock()
            && matches!(
                guard.get(&self.service_id),
                Some(ServiceSlot::Starting(id)) if *id == self.launch_id
            )
        {
            guard.remove(&self.service_id);
        }
    }
}

/// Supervises explicitly started external `fileserver` packages over confined aP wire transport.
pub struct FileserverManager {
    packages: Arc<PackageService>,
    srvfs: Arc<SrvFs>,
    instances: Arc<StdMutex<BTreeMap<String, ServiceSlot>>>,
    next_launch_id: AtomicU64,
}

impl FileserverManager {
    /// Create a supervisor wired to `packages` and Kernel `srvfs` (`/srv`).
    pub fn new(packages: Arc<PackageService>, srvfs: Arc<SrvFs>) -> Self {
        Self {
            packages,
            srvfs,
            instances: Arc::new(StdMutex::new(BTreeMap::new())),
            next_launch_id: AtomicU64::new(1),
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

        let launch_id = self.next_launch_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            ensure!(
                !guard.contains_key(&sid),
                "fileserver `{sid}` is already running or starting"
            );
            guard.insert(sid.clone(), ServiceSlot::Starting(launch_id));
        }
        let mut reservation = StartingReservation {
            instances: Arc::clone(&self.instances),
            service_id: sid.clone(),
            launch_id,
            armed: true,
        };

        ensure!(
            self.srvfs.lookup(&sid).await.is_none(),
            "/srv/{sid} is already occupied"
        );

        let running = RunningFileserver::launch(lease, export, &grant).await?;
        let (meta, child) = running.snapshot_parts();
        let status = complete_status(meta, &child).await;
        let still_reserved = {
            let guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            matches!(
                guard.get(&sid),
                Some(ServiceSlot::Starting(id)) if *id == launch_id
            )
        };
        if !still_reserved {
            running.shutdown().await;
            bail!("fileserver `{sid}` startup cancelled");
        }

        self.srvfs
            .post(&sid, running.transport.clone(), grant.mount_access)
            .await;

        let err_running = {
            let mut guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            if matches!(
                guard.get(&sid),
                Some(ServiceSlot::Starting(id)) if *id == launch_id
            ) {
                guard.insert(sid.clone(), ServiceSlot::Running(Box::new(running)));
                reservation.armed = false;
                None
            } else {
                Some((!guard.contains_key(&sid), running))
            }
        };
        if let Some((should_unpost, running)) = err_running {
            if should_unpost {
                self.srvfs.unpost(&sid).await;
            }
            running.shutdown().await;
            bail!("fileserver `{sid}` startup cancelled");
        }
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
            let guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            let ServiceSlot::Running(inst) = guard
                .get(service_id)
                .with_context(|| format!("fileserver `{service_id}` is not running"))?
            else {
                bail!("fileserver `{service_id}` is still starting");
            };
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
        let running = {
            let mut guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            match guard.get(service_id) {
                Some(ServiceSlot::Running(_)) => match guard.remove(service_id) {
                    Some(ServiceSlot::Running(running)) => *running,
                    _ => unreachable!(),
                },
                Some(ServiceSlot::Starting(_)) => {
                    bail!("fileserver `{service_id}` is still starting")
                }
                None => bail!("fileserver `{service_id}` is not running"),
            }
        };
        self.srvfs.unpost(service_id).await;
        running.shutdown().await;
        Ok(())
    }

    /// Stop all running `fileserver` instances cleanly.
    pub async fn stop_all(&self) -> Result<()> {
        let drained = {
            let mut guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            std::mem::take(&mut *guard)
        };
        for (service_id, slot) in drained {
            if let ServiceSlot::Running(running) = slot {
                self.srvfs.unpost(&service_id).await;
                (*running).shutdown().await;
            }
        }
        Ok(())
    }

    /// Return current runtime status for `service_id`, if started.
    pub async fn status(&self, service_id: &str) -> Option<FileserverStatus> {
        let (meta, child) = {
            let guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            match guard.get(service_id)? {
                ServiceSlot::Running(inst) => inst.snapshot_parts(),
                ServiceSlot::Starting(_) => return None,
            }
        };
        Some(complete_status(meta, &child).await)
    }

    /// List all supervised `fileserver` instances.
    pub async fn list(&self) -> Vec<FileserverStatus> {
        let items: Vec<_> = {
            let guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            guard
                .values()
                .filter_map(|slot| match slot {
                    ServiceSlot::Running(inst) => Some(inst.snapshot_parts()),
                    ServiceSlot::Starting(_) => None,
                })
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
        let sink = {
            let guard = self
                .instances
                .lock()
                .expect("fileserver instances lock poisoned");
            match guard.get(service_id)? {
                ServiceSlot::Running(inst) => Arc::clone(&inst.stderr_lines),
                ServiceSlot::Starting(_) => return None,
            }
        };
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
        let injected_gh_token = resolve_github_fileserver_token(&export, grant).await;
        let mut command = prepare_confined_command(
            grant,
            &export,
            &spec,
            &executable,
            &runtime_path,
            socket_path.as_deref(),
            injected_gh_token.as_deref(),
        )?;

        let sid = &export.service_id;
        let child = command.spawn().with_context(|| format!("spawn `{sid}`"))?;
        let pid = child.id();
        let guard = FileserverProcessGuard::new(child);
        let stderr_lines = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_STDERR_LINES)));

        let imported: Arc<dyn FileServer> = match async {
            let stderr = guard
                .take_stdio("fileserver missing stderr", |c| c.stderr.take())
                .await?;
            guard
                .push_task(tokio::spawn(drain_stderr(stderr, stderr_lines.clone())))
                .await;

            if let Some(sock) = socket_path.as_ref() {
                let mut stdout = guard
                    .take_stdio("unix_socket missing stdout", |c| c.stdout.take())
                    .await?;
                guard
                    .push_task(tokio::spawn(async move {
                        let _ = tokio::io::copy(&mut stdout, &mut tokio::io::sink()).await;
                    }))
                    .await;
                let stream =
                    connect_unix_until_ready(sock, &guard.child, timeout, &stderr_lines).await?;
                let (r, w) = stream.into_split();
                Ok::<Arc<dyn FileServer>, anyhow::Error>(Arc::new(ImportedFileServer::new(
                    BoundedLineReader::new(BufReader::new(r), export.max_message_bytes),
                    w,
                )))
            } else {
                let stdin = guard
                    .take_stdio("stdio fileserver missing stdin", |c| c.stdin.take())
                    .await?;
                let stdout = guard
                    .take_stdio("stdio missing stdout", |c| c.stdout.take())
                    .await?;
                Ok(Arc::new(ImportedFileServer::new(
                    BoundedLineReader::new(BufReader::new(stdout), export.max_message_bytes),
                    stdin,
                )) as Arc<dyn FileServer>)
            }
        }
        .await
        {
            Ok(imported) => imported,
            Err(err) => {
                guard.shutdown().await;
                return Err(err);
            }
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

async fn resolve_github_fileserver_token(
    export: &FileserverPackageExport,
    grant: &FileserverLaunchGrant,
) -> Option<String> {
    resolve_github_fileserver_token_with_timeout(export, grant, GITHUB_FILESERVER_TOKEN_TIMEOUT)
        .await
}

async fn resolve_github_fileserver_token_with_timeout(
    export: &FileserverPackageExport,
    grant: &FileserverLaunchGrant,
    timeout: Duration,
) -> Option<String> {
    if export.service_id != "github" || !export.allow_network || !grant.allow_network {
        return None;
    }
    let has_explicit_token = ["GH_TOKEN", "GITHUB_TOKEN"].into_iter().any(|key| {
        grant
            .runtime_env
            .get(key)
            .is_some_and(|val| !val.is_empty())
    });
    if has_explicit_token {
        return None;
    }

    let gh_bin = grant
        .runtime_env
        .get("ALAN_GITHUBFS_GH_BIN")
        .map(String::as_str)
        .filter(|val| !val.is_empty())
        .unwrap_or("gh");
    let mut cmd = Command::new(gh_bin);
    cmd.args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for key in ["GH_HOST", "GH_CONFIG_DIR"] {
        if let Some(val) = grant.runtime_env.get(key).filter(|val| !val.is_empty()) {
            cmd.env(key, val);
        }
    }

    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(_)) | Err(_) => return None,
    };
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let token = stdout.trim();
    if token.is_empty() || token.contains(['\0', '\n', '\r']) {
        return None;
    }
    Some(token.to_string())
}

fn prepare_confined_command(
    grant: &FileserverLaunchGrant,
    export: &FileserverPackageExport,
    spec: &SandboxSpec,
    executable: &Path,
    runtime_dir: &Path,
    socket_path: Option<&Path>,
    injected_gh_token: Option<&str>,
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
    if let Some(token) = injected_gh_token {
        command.env("GH_TOKEN", token);
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
    child: &Mutex<Option<Child>>,
    timeout: Duration,
    stderr_lines: &Arc<Mutex<VecDeque<String>>>,
) -> Result<UnixStream> {
    let deadline = Instant::now() + timeout;
    loop {
        let exited = {
            let mut guard = child.lock().await;
            match guard.as_mut() {
                Some(child) => child.try_wait()?,
                None => None,
            }
        };
        if let Some(status) = exited {
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

fn kill_child_process_group(child: &mut Child, process_group_id: Option<i32>) {
    let _ = process_group_id;
    #[cfg(unix)]
    if let Some(pgid) = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .or(process_group_id)
    {
        // SAFETY: `pgid` is the positive process group leader initialized with `process_group(0)`.
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
    }
    let _ = child.start_kill();
}

struct FileserverProcessGuard {
    child: Arc<Mutex<Option<Child>>>,
    process_group_id: Option<i32>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl FileserverProcessGuard {
    fn new(child: Child) -> Self {
        let process_group_id = child.id().and_then(|id| i32::try_from(id).ok());
        Self {
            child: Arc::new(Mutex::new(Some(child))),
            process_group_id,
            tasks: Mutex::new(Vec::new()),
        }
    }

    async fn push_task(&self, handle: JoinHandle<()>) {
        self.tasks.lock().await.push(handle);
    }

    async fn take_stdio<T>(
        &self,
        label: &str,
        take: impl FnOnce(&mut Child) -> Option<T>,
    ) -> Result<T> {
        let mut guard = self.child.lock().await;
        guard
            .as_mut()
            .and_then(take)
            .with_context(|| label.to_string())
    }

    async fn shutdown(&self) {
        if let Some(mut child) = self.child.lock().await.take() {
            kill_child_process_group(&mut child, self.process_group_id);
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
            kill_child_process_group(&mut child, self.process_group_id);
        }
        if let Ok(mut tasks) = self.tasks.try_lock() {
            for handle in tasks.drain(..) {
                handle.abort();
            }
        }
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
#[path = "fileserver_service_tests.rs"]
mod tests;
