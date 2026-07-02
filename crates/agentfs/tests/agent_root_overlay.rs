use std::sync::Arc;

use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::{ErrorCode, FileServer, InProcessTransport};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;

fn namespace_shell_with_agent_root() -> (Shell, Arc<AgentRootFs>) {
    let proc = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = proc.clone();
    let agent_root = Arc::new(AgentRootFs::new(proc_server));

    let mut namespace = Namespace::new();
    namespace.mount("/proc", InProcessTransport::new(proc), Access::ReadWrite);
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );

    let root = InProcessTransport::new(Arc::new(MountFs::new(namespace)));
    (Shell::new(root), agent_root)
}

#[tokio::test]
async fn agent_root_lists_only_proc_backed_agent_processes() {
    let (shell, agent_root) = namespace_shell_with_agent_root();

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
    let (shell, agent_root) = namespace_shell_with_agent_root();
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
