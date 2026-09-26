use super::*;

#[test]
fn process_exit_requires_a_matching_agent_executable_result() {
    let completed = AgentExecutableResult::completed("done", Vec::new());
    assert!(agent_result_matches_exit_code(Some(0), &completed));
    assert!(!agent_result_matches_exit_code(Some(1), &completed));

    let failed = AgentExecutableResult::failed("failed");
    assert!(agent_result_matches_exit_code(Some(1), &failed));
    assert!(!agent_result_matches_exit_code(Some(0), &failed));

    let invalid = invalid_agent_result_observation(0, "raw".to_string(), Vec::new());
    assert_eq!(invalid.status, ChildRuntimeStatus::Failed);
    assert_eq!(invalid.output_text, "raw");
    assert!(
        invalid
            .error_message
            .unwrap()
            .contains("without a valid terminal result")
    );
}
#[tokio::test]
async fn idle_agent_activity_does_not_complete_a_live_child_process() {
    use alan_ap::InProcessTransport;
    use alan_kernel::{Access, MountFs, Namespace, ProcFs};
    use std::sync::Arc;
    let mut ns = Namespace::new();
    ns.mount(
        "/proc",
        InProcessTransport::new(Arc::new(ProcFs::new())),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/1",
        InProcessTransport::new(Arc::new(alan_agentfs::AgentFs::new())),
        Access::ReadWrite,
    );
    let root = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let shell = alan_shell::Shell::new(root.clone());
    let pid = shell
        .spawn(r#"{"executable":"/bin/agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#)
        .await
        .unwrap();
    assert_eq!(pid, "1");
    let idle_event = serde_json::to_string(&alan_agent_protocol::UiEvent::Activity {
        snapshot: alan_agent_protocol::UiActivitySnapshot::idle(),
    })
    .unwrap()
        + "\n";
    shell
        .write("/agent/1/machine/ui/events", idle_event.as_bytes())
        .await
        .unwrap();
    let environment = crate::runtime::NamespaceRuntimeEnvironment::new(root, "/agent/1", "default");
    let mut supervisor = DelegatedChildRunSupervisor::new(DelegatedChildRunSupervision {
        startup: ChildProcessStartup {
            process_path: "/proc/1".into(),
            rollout_path: None,
            warnings: Vec::new(),
        },
        child_run_id: "child".into(),
        child_run_registry: ChildRunRegistry::default(),
        timeout: None,
        agent_files: environment.agent_files(),
        process_files: environment.process_files(),
        process_pid: pid,
    });
    assert!(
        tokio::time::timeout(
            Duration::from_millis(30),
            supervisor.wait_for_terminal_event(None)
        )
        .await
        .is_err(),
        "UI idle alone must not fabricate a completed Process result"
    );
}
