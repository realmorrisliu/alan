use std::sync::Arc;

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::{ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, Credentials, MountFs, Namespace, Pid, ProcFs};
use alan_memfs::MemFs;
use alan_shell::Shell;

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
