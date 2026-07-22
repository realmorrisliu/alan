use alan_kernel::LiveNamespace;

struct NamespaceRaceProcFs {
    inner: ProcFs,
    namespace: LiveNamespace,
    stat_count: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl FileServer for NamespaceRaceProcFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        self.inner.walk(fid, newfid, names).await
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        self.inner.open(fid, mode).await
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        self.inner.read(fid, offset, count).await
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        self.inner.write(fid, offset, data).await
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let stat = self.inner.stat(fid).await?;
        if self
            .stat_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            == 1
        {
            self.namespace.mount(
                "/mnt/project",
                InProcessTransport::new(Arc::new(MemFs::empty())),
                Access::ReadWrite,
            );
        }
        Ok(stat)
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        self.inner.create(fid, newfid, name, kind).await
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.remove(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.inner.clunk(fid).await
    }
}

#[tokio::test]
async fn shell_retries_when_namespace_changes_between_snapshot_and_clone() {
    let procfs = ProcFs::new();
    let mut namespace = Namespace::new();
    namespace.mount(
        "/bin/argv",
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadOnly,
    );
    namespace.mount(
        "/mnt",
        InProcessTransport::new(Arc::new(MemFs::empty())),
        Access::ReadWrite,
    );
    let bootstrap = procfs.for_spawner(None, namespace.clone(), Credentials::user("shell-test"));
    bootstrap
        .walk(Fid::ROOT, Fid(499_100), &["clone".to_string()])
        .await
        .unwrap();
    bootstrap
        .open(Fid(499_100), OpenMode::ReadWrite)
        .await
        .unwrap();
    let parent_pid = String::from_utf8(bootstrap.read(Fid(499_100), 0, 64).await.unwrap())
        .unwrap()
        .parse::<u64>()
        .unwrap();
    bootstrap
        .write(
            Fid(499_100),
            0,
            br#"{"executable":"/bin/argv","args":[],"namespace":{"generation":0,"mounts":[{"path":"/bin/argv","access":"ro"},{"path":"/mnt","access":"rw"}]}}"#,
        )
        .await
        .unwrap();
    bootstrap.clunk(Fid(499_100)).await.unwrap();

    let live_namespace = LiveNamespace::new(namespace);
    let spawner = procfs.with_runner(Arc::new(ArgvRunner)).for_live_spawner(
        Some(Pid(parent_pid)),
        live_namespace.clone(),
        Credentials::user("shell-test"),
    );
    let racing_procfs = NamespaceRaceProcFs {
        inner: spawner,
        namespace: live_namespace.clone(),
        stat_count: std::sync::atomic::AtomicUsize::new(0),
    };
    live_namespace.mount(
        "/proc",
        InProcessTransport::new(Arc::new(racing_procfs)),
        Access::ReadWrite,
    );
    let shell = Shell::new(InProcessTransport::new(Arc::new(
        MountFs::from_live_namespace(live_namespace),
    )));

    let result = shell
        .run("/bin/argv", &["retried".to_string()])
        .await
        .unwrap();
    assert_eq!(result.output, b"retried\n");
    let child_namespace = String::from_utf8(
        shell
            .cat(&format!("/proc/{}/namespace", result.pid))
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        child_namespace
            .lines()
            .any(|line| line == "/mnt/project rw"),
        "the successful retry must use one fresh explicit snapshot: {child_namespace:?}"
    );
}
