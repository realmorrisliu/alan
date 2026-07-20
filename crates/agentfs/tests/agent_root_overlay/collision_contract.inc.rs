
#[tokio::test]
async fn failed_concurrent_walk_does_not_delete_winning_fid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    let backing = Arc::new(DelayedFailWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let shared_fid = Fid(15_000);
    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    for _ in 0..2 {
        let agent_root = agent_root.clone();
        let pid = pid.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await;
            tx.send(result).await.unwrap();
        });
    }
    drop(tx);

    let winner = rx.recv().await.unwrap();
    assert!(winner.is_ok(), "one concurrent walk should bind the fid");
    backing.release_failure();
    let loser = rx.recv().await.unwrap();
    assert_eq!(loser, Err(ErrorCode::NotFound));

    agent_root
        .open(shared_fid, OpenMode::Read)
        .await
        .expect("failed concurrent walk must not remove the winning binding");
    agent_root.clunk(shared_fid).await.unwrap();
}

#[tokio::test]
async fn create_collision_rolls_back_the_losing_backing_file() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    let backing = Arc::new(RacingCreateFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let dir_fid = Fid(16_000);
    let shared_fid = Fid(16_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();

    let (tx, mut rx) = tokio::sync::mpsc::channel(2);
    for name in ["alpha", "beta"] {
        let agent_root = agent_root.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let result = agent_root
                .create(dir_fid, shared_fid, name, FileKind::File)
                .await;
            tx.send((name.to_string(), result)).await.unwrap();
        });
    }
    drop(tx);

    let first = rx.recv().await.unwrap();
    let second = rx.recv().await.unwrap();
    let results = [first, second];
    let ok_names: BTreeSet<String> = results
        .iter()
        .filter(|(_, result)| result.is_ok())
        .map(|(name, _)| name.clone())
        .collect();
    let collision_count = results
        .iter()
        .filter(|(_, result)| matches!(result, Err(ErrorCode::BadRequest)))
        .count();

    assert_eq!(ok_names.len(), 1);
    assert_eq!(collision_count, 1);
    assert_eq!(
        backing.file_names(),
        ok_names,
        "a create that returns BadRequest must not leave a hidden backing file"
    );
    agent_root.clunk(shared_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();
}

#[tokio::test]
async fn agent_root_tracks_created_fids_forwarded_to_backing() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(20_000);
    let file_fid = Fid(20_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    let qid = agent_root
        .create(dir_fid, file_fid, "facts", FileKind::File)
        .await
        .unwrap();
    assert_eq!(qid.kind, FileKind::File);
    agent_root.open(file_fid, OpenMode::Write).await.unwrap();
    agent_root.write(file_fid, 0, b"alpha").await.unwrap();
    agent_root.clunk(file_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/facts")).await.unwrap()).unwrap(),
        "alpha"
    );
}

#[tokio::test]
async fn agent_root_releases_outer_fid_after_delegated_remove() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation": 0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(21_000);
    let file_fid = Fid(21_001);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    agent_root
        .create(dir_fid, file_fid, "scratch", FileKind::File)
        .await
        .unwrap();
    agent_root.clunk(file_fid).await.unwrap();
    agent_root.clunk(dir_fid).await.unwrap();

    let remove_fid = Fid(21_002);
    agent_root
        .walk(Fid::ROOT, remove_fid, &[pid.clone(), "scratch".into()])
        .await
        .unwrap();
    agent_root.remove(remove_fid).await.unwrap();
    assert!(matches!(
        shell.cat(&format!("/agent/{pid}/scratch")).await,
        Err(ErrorCode::NotFound)
    ));

    agent_root
        .walk(Fid::ROOT, remove_fid, std::slice::from_ref(&pid))
        .await
        .expect("remove releases the outer fid for reuse");
}

struct ProcWalkCollisionFs {
    failed_fids: Mutex<Vec<Fid>>,
    clunked_fids: Mutex<Vec<Fid>>,
}

impl ProcWalkCollisionFs {
    fn new() -> Self {
        Self {
            failed_fids: Mutex::new(Vec::new()),
            clunked_fids: Mutex::new(Vec::new()),
        }
    }

    fn clunked_failed_fid(&self) -> bool {
        let failed = self
            .failed_fids
            .lock()
            .expect("failed fids lock should not be poisoned");
        let clunked = self
            .clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned");
        failed.iter().any(|fid| clunked.contains(fid))
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::Dir,
            version: 0,
            path: 0xC011_1510,
        }
    }
}

#[async_trait]
impl FileServer for ProcWalkCollisionFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT {
            return Err(ErrorCode::NotFound);
        }
        match names {
            [pid] if pid == "1" => Ok(Self::qid()),
            [pid, name] if pid == "1" && name == "status" => {
                self.failed_fids
                    .lock()
                    .expect("failed fids lock should not be poisoned")
                    .push(newfid);
                Err(ErrorCode::BadRequest)
            }
            [pid, name] if pid == "1" && name == "parent" => {
                self.failed_fids
                    .lock()
                    .expect("failed fids lock should not be poisoned")
                    .push(newfid);
                Err(ErrorCode::NotFound)
            }
            _ => Err(ErrorCode::NotFound),
        }
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned")
            .push(fid);
        Ok(())
    }
}

struct FailedBackingWalkFs {
    failed_fids: Mutex<Vec<Fid>>,
    clunked_fids: Mutex<Vec<Fid>>,
}

impl FailedBackingWalkFs {
    fn new() -> Self {
        Self {
            failed_fids: Mutex::new(Vec::new()),
            clunked_fids: Mutex::new(Vec::new()),
        }
    }

    fn clunked_failed_fid(&self) -> bool {
        let failed = self
            .failed_fids
            .lock()
            .expect("failed fids lock should not be poisoned");
        let clunked = self
            .clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned");
        failed.iter().any(|fid| clunked.contains(fid))
    }
}

#[async_trait]
impl FileServer for FailedBackingWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT && names == ["file"] {
            self.failed_fids
                .lock()
                .expect("failed fids lock should not be poisoned")
                .push(newfid);
            Err(ErrorCode::BadRequest)
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, _fid: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunked_fids
            .lock()
            .expect("clunked fids lock should not be poisoned")
            .push(fid);
        Ok(())
    }
}

struct StatFailWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
}

impl StatFailWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
        }
    }

    fn bound_fid_count(&self) -> usize {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .len()
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0x57A7_F411,
        }
    }
}

#[async_trait]
impl FileServer for StatFailWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names != ["file"] {
            return Err(ErrorCode::NotFound);
        }
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, ());
        Ok(Self::qid())
    }

    async fn open(&self, _fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Err(ErrorCode::Io)
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct RacingWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
    started_walks: AtomicUsize,
    release_walks: Notify,
}

impl RacingWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            started_walks: AtomicUsize::new(0),
            release_walks: Notify::new(),
        }
    }

    fn bound_fid_count(&self) -> usize {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .len()
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0xF11E,
        }
    }
}

#[async_trait]
impl FileServer for RacingWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 || names[0] != "file" {
            return Err(ErrorCode::NotFound);
        }
        let started = self.started_walks.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.release_walks.notify_waiters();
        } else {
            self.release_walks.notified().await;
        }
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, ());
        Ok(Self::qid())
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Self::qid())
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Stat {
                name: "file".to_string(),
                qid: Self::qid(),
                length: 0,
                executable: false,
                writable: true,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct DelayedFailWalkFs {
    fids: Mutex<HashMap<Fid, ()>>,
    started_walks: AtomicUsize,
    both_started: Notify,
    release_failed_walk: Notify,
}

impl DelayedFailWalkFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            started_walks: AtomicUsize::new(0),
            both_started: Notify::new(),
            release_failed_walk: Notify::new(),
        }
    }

    fn release_failure(&self) {
        self.release_failed_walk.notify_waiters();
    }

    fn qid() -> Qid {
        Qid {
            kind: FileKind::File,
            version: 0,
            path: 0xF11E_0002,
        }
    }
}

#[async_trait]
impl FileServer for DelayedFailWalkFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 || names[0] != "file" {
            return Err(ErrorCode::NotFound);
        }
        let started = self.started_walks.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.both_started.notify_waiters();
        }
        while self.started_walks.load(Ordering::SeqCst) < 2 {
            self.both_started.notified().await;
        }
        if started == 1 {
            self.fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .insert(newfid, ());
            Ok(Self::qid())
        } else {
            self.release_failed_walk.notified().await;
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Self::qid())
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, _fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        Ok(Vec::new())
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .contains_key(&fid)
        {
            Ok(Stat {
                name: "file".to_string(),
                qid: Self::qid(),
                length: 0,
                executable: false,
                writable: false,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.clunk(fid).await
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}

struct RacingCreateFs {
    fids: Mutex<HashMap<Fid, String>>,
    files: Mutex<BTreeSet<String>>,
    started_creates: AtomicUsize,
    release_creates: Notify,
}

impl RacingCreateFs {
    fn new() -> Self {
        Self {
            fids: Mutex::new(HashMap::new()),
            files: Mutex::new(BTreeSet::new()),
            started_creates: AtomicUsize::new(0),
            release_creates: Notify::new(),
        }
    }

    fn file_names(&self) -> BTreeSet<String> {
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .clone()
    }

    fn qid(kind: FileKind) -> Qid {
        Qid {
            kind,
            version: 0,
            path: match kind {
                FileKind::Dir => 0x0C0D_ED1A,
                FileKind::File => 0xC0DE_F11E,
                FileKind::Stream => 0xC0DE_57EA,
                FileKind::Clone => 0xC0DE_C10E,
            },
        }
    }
}

#[async_trait]
impl FileServer for RacingCreateFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || names.len() != 1 {
            return Err(ErrorCode::NotFound);
        }
        if self
            .files
            .lock()
            .expect("file set lock should not be poisoned")
            .contains(&names[0])
        {
            self.fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .insert(newfid, names[0].clone());
            Ok(Self::qid(FileKind::File))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        if fid == Fid::ROOT
            || self
                .fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .contains_key(&fid)
        {
            Ok(Self::qid(FileKind::File))
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn read(&self, fid: Fid, _offset: u64, _count: u32) -> Result<Vec<u8>, ErrorCode> {
        if fid == Fid::ROOT {
            Ok(self
                .file_names()
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n")
                .into_bytes())
        } else {
            Ok(Vec::new())
        }
    }

    async fn write(&self, _fid: Fid, _offset: u64, data: &[u8]) -> Result<u32, ErrorCode> {
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        if fid == Fid::ROOT
            || self
                .fids
                .lock()
                .expect("fid map lock should not be poisoned")
                .contains_key(&fid)
        {
            Ok(Stat {
                name: String::new(),
                qid: Self::qid(if fid == Fid::ROOT {
                    FileKind::Dir
                } else {
                    FileKind::File
                }),
                length: 0,
                executable: false,
                writable: true,
            })
        } else {
            Err(ErrorCode::NotFound)
        }
    }

    async fn create(
        &self,
        fid: Fid,
        newfid: Fid,
        name: &str,
        kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        if fid != Fid::ROOT || kind != FileKind::File {
            return Err(ErrorCode::BadRequest);
        }
        let started = self.started_creates.fetch_add(1, Ordering::SeqCst) + 1;
        if started >= 2 {
            self.release_creates.notify_waiters();
        }
        while self.started_creates.load(Ordering::SeqCst) < 2 {
            self.release_creates.notified().await;
        }
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .insert(name.to_string());
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .insert(newfid, name.to_string());
        Ok(Self::qid(FileKind::File))
    }

    async fn remove(&self, fid: Fid) -> Result<(), ErrorCode> {
        let name = self
            .fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid)
            .ok_or(ErrorCode::NotFound)?;
        self.files
            .lock()
            .expect("file set lock should not be poisoned")
            .remove(&name);
        Ok(())
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids
            .lock()
            .expect("fid map lock should not be poisoned")
            .remove(&fid);
        Ok(())
    }
}
