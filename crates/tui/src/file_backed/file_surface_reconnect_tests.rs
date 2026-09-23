use super::*;
use crate::file_backed::stdio_tests::{CloseTailOnPid, EXEC_SPEC, PID_MOUNT, live_root_agent};
use crate::history::ToolStatus;
use alan_agentfs::AgentFs;
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::Access;
use std::sync::Arc;

#[test]
fn reattached_action_indices_follow_removed_error_cells() {
    let mut reattached = FileBackedApp::new("/agent/root".to_string());
    reattached.transcript = vec![
        HistoryCell::User("previous task".to_string()),
        HistoryCell::Assistant("previous answer".to_string()),
        HistoryCell::User("current task".to_string()),
    ];
    let previous_transcript = std::mem::take(&mut reattached.transcript);
    reattached.reset_for_root_process_change();

    reattached.transcript = vec![
        HistoryCell::User("current task".to_string()),
        HistoryCell::Error("recoverable provider failure".to_string()),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Complete,
            preview: Some("first result".to_string()),
            presentation: None,
        },
        HistoryCell::Assistant("current answer".to_string()),
    ];
    reattached.action_cells.insert("action-1".to_string(), 2);

    let current_transcript = std::mem::take(&mut reattached.transcript);
    reattached.transcript = previous_transcript;
    let current_transcript =
        remove_error_cells_and_remap_actions(current_transcript, &mut reattached.action_cells);

    assert!(reattached.merge_reconnected_history(current_transcript, "current task", 0));
    assert_eq!(reattached.action_cells.get("action-1"), Some(&3));

    reattached.upsert_action_cell(
        "action-1".to_string(),
        HistoryCell::Tool {
            title: "bash".to_string(),
            status: ToolStatus::Failed,
            preview: Some("updated result".to_string()),
            presentation: None,
        },
    );
    assert!(matches!(
        reattached.transcript.get(3),
        Some(HistoryCell::Tool {
            status: ToolStatus::Failed,
            ..
        })
    ));
    assert_eq!(
        reattached.transcript.last(),
        Some(&HistoryCell::Assistant("current answer".to_string()))
    );
}

#[tokio::test]
async fn renderer_hydration_retries_all_streams_after_root_pid_changes() {
    let (shell, agent_root, namespace, old_pid) = live_root_agent().await;
    let new_pid = shell.spawn(EXEC_SPEC).await.unwrap();
    agent_root
        .bind_process(new_pid.clone(), Arc::new(AgentFs::new()))
        .await;

    shell
        .write(
            &format!("/agent/{new_pid}/machine/tape"),
            b"{\"version\":1,\"kind\":\"message\",\"role\":\"user\",\"content\":\"new task\"}\n{\"version\":1,\"kind\":\"message\",\"role\":\"assistant\",\"content\":\"new answer\"}\n",
        )
        .await
        .unwrap();

    let interceptor = Arc::new(CloseTailOnPid::new(agent_root.clone()));
    namespace.replace_mount(
        "/agent",
        InProcessTransport::new(interceptor.clone()),
        Access::ReadWrite,
    );
    let (walked, resume) = interceptor.pause_next_walk_with_suffix("/actions/events");
    let attach_shell = shell.clone();
    let attach = tokio::spawn(async move {
        let mut app = FileBackedApp::new("/agent/root".to_string());
        let tails = hydrate_and_open_tails(&attach_shell, "/agent/root", &mut app).await?;
        anyhow::Ok((app, tails))
    });

    tokio::time::timeout(std::time::Duration::from_secs(2), walked)
        .await
        .unwrap()
        .unwrap();
    agent_root.set_root_process(new_pid.clone()).await;
    namespace.replace_mount(
        PID_MOUNT,
        InProcessTransport::new(Arc::new(alan_ap::reference::MemFs::with_read_only_file(
            "pid",
            format!("{new_pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    resume.send(()).unwrap();

    let (app, mut tails) = attach.await.unwrap().unwrap();
    assert_eq!(tails.root_agent_pid, Some(new_pid.parse().unwrap()));
    assert!(
        app.transcript
            .contains(&HistoryCell::Assistant("new answer".to_string()))
    );

    assert_eq!(
        create_request(&agent_root, &old_pid, Fid(50_001)).await,
        "r0"
    );
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(10),
            tails.requests.read(1024)
        )
        .await
        .is_err(),
        "renderer must not retain a request tail from the previous PID"
    );
    assert_eq!(
        create_request(&agent_root, &new_pid, Fid(50_002)).await,
        "r0"
    );
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            tails.requests.read(1024)
        )
        .await
        .unwrap()
        .unwrap(),
        b"created:r0\n"
    );

    tails.requests.close().await.unwrap();
    tails.actions.close().await.unwrap();
    tails.ui.close().await.unwrap();
    tails.tape.close().await.unwrap();
    tails.output.close().await.unwrap();
}

async fn create_request(agent_root: &alan_agentfs::AgentRootFs, pid: &str, fid: Fid) -> String {
    agent_root
        .walk(
            Fid::ROOT,
            fid,
            &[pid.to_string(), "requests".to_string(), "clone".to_string()],
        )
        .await
        .unwrap();
    agent_root.open(fid, OpenMode::ReadWrite).await.unwrap();
    let id = String::from_utf8(agent_root.read(fid, 0, 64).await.unwrap()).unwrap();
    agent_root.clunk(fid).await.unwrap();
    id
}
