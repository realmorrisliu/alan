use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat};
use alan_kernel::{Access, Credentials, MountFs, Namespace, Pid, ProcFs};
use alan_memfs::MemFs;
use alan_shell::Shell;
use async_trait::async_trait;
use tokio::sync::Notify;

fn namespace_shell_with_agent_root() -> (InProcessTransport, Shell, Arc<AgentRootFs>, Arc<ProcFs>) {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new(proc_server));

    let mut namespace = Namespace::new();
    namespace.mount(
        "/proc",
        InProcessTransport::new(proc.clone()),
        Access::ReadWrite,
    );
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );

    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    (root.clone(), Shell::new(root), agent_root, proc)
}

async fn spawn_on_proc(proc: &ProcFs, fid: Fid) -> String {
    proc.walk(Fid::ROOT, fid, &["clone".into()])
        .await
        .expect("walk clone");
    proc.open(fid, OpenMode::ReadWrite)
        .await
        .expect("open clone");
    let pid = String::from_utf8(proc.read(fid, 0, 64).await.expect("read pid")).unwrap();
    proc.write(fid, 0, br#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .expect("write exec");
    proc.clunk(fid).await.expect("commit process");
    pid
}

#[tokio::test]
async fn agent_root_lists_only_proc_backed_agent_processes() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();

    agent_root
        .bind_process("999", Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process("999").await;
    assert_eq!(shell.ls("/agent").await.unwrap(), Vec::<String>::new());
    assert!(matches!(
        shell.ls("/agent/999").await,
        Err(ErrorCode::NotFound)
    ));

    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let listing = shell.ls("/agent").await.unwrap();
    assert!(listing.iter().any(|entry| entry == &pid), "{listing:?}");
    assert!(listing.iter().any(|entry| entry == "root"), "{listing:?}");
    assert!(!listing.iter().any(|entry| entry == "999"), "{listing:?}");
}

#[tokio::test]
async fn agent_root_qid_version_changes_with_listing() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();

    let empty_qid = agent_root.stat(Fid::ROOT).await.unwrap().qid;
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let populated_qid = agent_root.stat(Fid::ROOT).await.unwrap().qid;
    assert_eq!(empty_qid.kind, FileKind::Dir);
    assert_eq!(empty_qid.path, populated_qid.path);
    assert_ne!(empty_qid.version, populated_qid.version);
}

#[tokio::test]
async fn agent_root_alias_forwards_to_the_root_agent_surface() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    shell
        .write("/agent/root/io/output", b"hello root")
        .await
        .unwrap();

    assert_eq!(
        String::from_utf8(shell.cat(&format!("/agent/{pid}/io/output")).await.unwrap()).unwrap(),
        "hello root"
    );
    assert!(matches!(
        shell.ls(&format!("/proc/{pid}/machine")).await,
        Err(ErrorCode::NotFound)
    ));
}

#[tokio::test]
async fn agent_children_are_derived_from_proc_parentage() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let parent = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let spawner = proc.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let child = spawn_on_proc(&spawner, Fid(10_000)).await;
    let unbound_child = spawn_on_proc(&spawner, Fid(10_001)).await;

    agent_root
        .bind_process(parent.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root
        .bind_process(child.clone(), Arc::new(AgentFs::new()))
        .await;

    let children = shell
        .ls(&format!("/agent/{parent}/children"))
        .await
        .unwrap();
    assert!(children.iter().any(|entry| entry == &child), "{children:?}");
    assert!(
        !children.iter().any(|entry| entry == &unbound_child),
        "{children:?}"
    );

    shell
        .write(
            &format!("/agent/{parent}/children/{child}/io/output"),
            b"hello child",
        )
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(
            shell
                .cat(&format!("/agent/{child}/io/output"))
                .await
                .unwrap()
        )
        .unwrap(),
        "hello child"
    );
}

#[tokio::test]
async fn agent_children_qid_versions_change_with_listing() {
    let (_, shell, agent_root, proc) = namespace_shell_with_agent_root();
    let parent = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(parent.clone(), Arc::new(AgentFs::new()))
        .await;

    let empty_fid = Fid(11_000);
    agent_root
        .walk(
            Fid::ROOT,
            empty_fid,
            &[parent.clone(), "children".to_string()],
        )
        .await
        .unwrap();
    let empty_qid = agent_root.stat(empty_fid).await.unwrap().qid;
    agent_root.clunk(empty_fid).await.unwrap();

    let spawner = proc.for_spawner(
        Some(Pid(parent.parse::<u64>().unwrap())),
        Namespace::new(),
        Credentials::user("alan"),
    );
    let child = spawn_on_proc(&spawner, Fid(11_001)).await;
    agent_root
        .bind_process(child.clone(), Arc::new(AgentFs::new()))
        .await;

    let child_fid = Fid(11_002);
    agent_root
        .walk(Fid::ROOT, child_fid, &[parent, "children".to_string()])
        .await
        .unwrap();
    let child_qid = agent_root.stat(child_fid).await.unwrap().qid;
    agent_root.clunk(child_fid).await.unwrap();

    assert_eq!(empty_qid.kind, FileKind::Dir);
    assert_eq!(empty_qid.path, child_qid.path);
    assert_ne!(empty_qid.version, child_qid.version);
}

#[tokio::test]
async fn agent_root_namespaces_backing_qids_by_pid() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let first = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let second = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(first.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root
        .bind_process(second.clone(), Arc::new(AgentFs::new()))
        .await;

    let first_fid = Fid(12_000);
    let first_qid = agent_root
        .walk(
            Fid::ROOT,
            first_fid,
            &[first, "io".to_string(), "output".to_string()],
        )
        .await
        .unwrap();
    let first_stat_qid = agent_root.stat(first_fid).await.unwrap().qid;

    let second_fid = Fid(12_001);
    let second_qid = agent_root
        .walk(
            Fid::ROOT,
            second_fid,
            &[second, "io".to_string(), "output".to_string()],
        )
        .await
        .unwrap();
    let second_stat_qid = agent_root.stat(second_fid).await.unwrap().qid;
    agent_root.clunk(first_fid).await.unwrap();
    agent_root.clunk(second_fid).await.unwrap();

    assert_eq!(first_qid, first_stat_qid);
    assert_eq!(second_qid, second_stat_qid);
    assert_eq!(first_qid.kind, second_qid.kind);
    assert_ne!(first_qid.path, second_qid.path);
}

#[tokio::test]
async fn agent_root_rejects_creates_for_overlay_reserved_names() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(MemFs::new()))
        .await;

    let dir_fid = Fid(13_000);
    agent_root
        .walk(Fid::ROOT, dir_fid, std::slice::from_ref(&pid))
        .await
        .unwrap();
    for (idx, name) in ["children", "status", "ctl"].into_iter().enumerate() {
        assert_eq!(
            agent_root
                .create(dir_fid, Fid(13_001 + idx as u64), name, FileKind::File)
                .await,
            Err(ErrorCode::BadRequest),
            "{name} should stay reserved for the overlay"
        );
    }
    agent_root.clunk(dir_fid).await.unwrap();
}

#[tokio::test]
async fn concurrent_walk_rechecks_newfid_before_insert() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    let backing = Arc::new(RacingWalkFs::new());
    agent_root.bind_process(pid.clone(), backing.clone()).await;

    let shared_fid = Fid(14_000);
    let first = {
        let agent_root = agent_root.clone();
        let pid = pid.clone();
        tokio::spawn(async move {
            agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await
        })
    };
    let second = {
        let agent_root = agent_root.clone();
        tokio::spawn(async move {
            agent_root
                .walk(Fid::ROOT, shared_fid, &[pid, "file".to_string()])
                .await
        })
    };
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    let ok_count = [first, second].into_iter().filter(Result::is_ok).count();
    let collision_count = [first, second]
        .into_iter()
        .filter(|result| matches!(result, Err(ErrorCode::BadRequest)))
        .count();

    assert_eq!(ok_count, 1);
    assert_eq!(collision_count, 1);
    assert_eq!(
        backing.bound_fid_count(),
        1,
        "the backing fid allocated by the losing walk should be clunked"
    );
    agent_root.clunk(shared_fid).await.unwrap();
    assert_eq!(backing.bound_fid_count(), 0);
}

#[tokio::test]
async fn agent_root_tracks_created_fids_forwarded_to_backing() {
    let (_, shell, agent_root, _) = namespace_shell_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
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
