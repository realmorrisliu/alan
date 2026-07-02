use std::sync::Arc;

use alan_agentfs::{AgentConformanceChecker, AgentFs, AgentRootFs};
use alan_ap::{FileServer, InProcessTransport};
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use alan_shell::Shell;

fn namespace_with_agent_root() -> (InProcessTransport, Shell, Arc<AgentRootFs>) {
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
    (root.clone(), Shell::new(root), agent_root)
}

#[tokio::test]
async fn conformance_checker_accepts_agent_overlay_process_layout() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_agent_process(&format!("/agent/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_verifies_dynamic_container_event_streams() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;

    let checker = AgentConformanceChecker::new(root);
    checker
        .check_dynamic_container_events(&format!("/agent/{pid}"))
        .await
        .assert_ok();
}

#[tokio::test]
async fn conformance_checker_verifies_root_alias_matches_current_pid() {
    let (root, shell, agent_root) = namespace_with_agent_root();
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[]}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    agent_root.set_root_process(pid.clone()).await;

    let checker = AgentConformanceChecker::new(root);
    checker.check_root_alias("/agent", &pid).await.assert_ok();
}
