//! Queue snapshot is a distinct regular document, with ordinary AgentFS watch semantics.
use alan_agentfs::{AgentConformanceChecker, AgentFs};
use alan_ap::{Fid, FileServer, OpenMode};
use std::sync::Arc;

#[tokio::test]
async fn queue_surface_starts_unknown_and_watch_commit_is_independent_of_activity() {
    let fs = Arc::new(AgentFs::new());
    let names = ["machine".into(), "ui".into(), "queue".into()];
    fs.walk(Fid::ROOT, Fid(1), &names)
        .await
        .expect("queue file exists");
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let initial = fs.read(Fid(1), 0, 4096).await.unwrap();
    assert_eq!(initial, b"{\"version\":1,\"revision\":0,\"known\":false,\"pending_submission_ids\":[],\"active_submission_ids\":[],\"paused\":false,\"deferred\":false,\"uncertain_submission_ids\":[]}");
    fs.clunk(Fid(1)).await.unwrap();
    fs.walk(Fid::ROOT, Fid(2), &["events".into()])
        .await
        .unwrap();
    fs.open(Fid(2), OpenMode::Read).await.unwrap();
    let watcher = tokio::spawn({
        let fs = fs.clone();
        async move { fs.read(Fid(2), 0, 4096).await.unwrap() }
    });
    let next = b"{\"version\":1,\"revision\":1,\"known\":true,\"pending_submission_ids\":[\"Q\"],\"active_submission_ids\":[],\"paused\":true,\"deferred\":false,\"uncertain_submission_ids\":[]}";
    fs.walk(Fid::ROOT, Fid(3), &names).await.unwrap();
    fs.open(Fid(3), OpenMode::Write).await.unwrap();
    fs.write(Fid(3), 0, next).await.unwrap();
    fs.clunk(Fid(3)).await.unwrap();
    let watched = tokio::time::timeout(std::time::Duration::from_secs(1), watcher)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(watched, b"ui:queue\n");
    fs.walk(Fid::ROOT, Fid(4), &names).await.unwrap();
    fs.open(Fid(4), OpenMode::Read).await.unwrap();
    assert_eq!(fs.read(Fid(4), 0, 4096).await.unwrap(), next);
    fs.clunk(Fid(4)).await.unwrap();
    fs.clunk(Fid(2)).await.unwrap();
    fs.walk(
        Fid::ROOT,
        Fid(5),
        &["machine".into(), "ui".into(), "activity".into()],
    )
    .await
    .unwrap();
    fs.open(Fid(5), OpenMode::Read).await.unwrap();
    assert_eq!(
        fs.read(Fid(5), 0, 4096).await.unwrap(),
        b"{\"version\":1,\"state\":\"idle\"}"
    );
    fs.clunk(Fid(5)).await.unwrap();
}

#[tokio::test]
async fn queue_surface_keeps_agentfs_conformance() {
    let fs = AgentFs::new();
    fs.walk(Fid::ROOT, Fid(1), &["machine".into(), "ui".into()])
        .await
        .unwrap();
    fs.open(Fid(1), OpenMode::Read).await.unwrap();
    let listing = fs.read(Fid(1), 0, 4096).await.unwrap();
    fs.clunk(Fid(1)).await.unwrap();
    assert!(
        String::from_utf8(listing)
            .unwrap()
            .lines()
            .any(|name| name == "queue")
    );
    let proc = Arc::new(alan_kernel::ProcFs::new());
    let agent_root = Arc::new(alan_agentfs::AgentRootFs::new_with_process_events(
        proc.clone(),
        proc.clone(),
    ));
    let mut namespace = alan_kernel::Namespace::new();
    namespace.mount(
        "/proc",
        alan_ap::InProcessTransport::new(proc),
        alan_kernel::Access::ReadWrite,
    );
    namespace.mount(
        "/agent",
        alan_ap::InProcessTransport::new(agent_root.clone()),
        alan_kernel::Access::ReadWrite,
    );
    let root = alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(namespace)));
    let shell = alan_shell::Shell::new(root.clone());
    let pid = shell.spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#).await.unwrap();
    agent_root.bind_process(pid.clone(), Arc::new(fs)).await;
    AgentConformanceChecker::new(root)
        .check_agent_process(&format!("/agent/{pid}"))
        .await
        .assert_ok();
}
