use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

use super::*;

fn init_test_git_repo(dir: &Path) {
    for args in [
        vec!["init"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test User"],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(&args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out);
    }
    fs::write(dir.join("README.md"), "initial\n").unwrap();
    for args in [vec!["add", "README.md"], vec!["commit", "-m", "init"]] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(&args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out);
    }
}

#[tokio::test]
async fn shared_write_queue_serializes_waiters_in_strict_fifo_order() {
    let queue = SharedWriteQueueRegistry::new();
    let cancel = CancellationToken::new();
    let order = Arc::new(Mutex::new(Vec::new()));

    let first = queue
        .acquire_grants_fifo(&["repo-grant".to_string()], "writer-1", &cancel)
        .await
        .unwrap();
    assert!(queue.is_grant_busy("repo-grant"));
    assert_eq!(
        queue.active_writer("repo-grant").as_deref(),
        Some("writer-1")
    );

    let mut handles = Vec::new();
    for idx in 2..=4 {
        let q = queue.clone();
        let c = cancel.clone();
        let o = Arc::clone(&order);
        handles.push(tokio::spawn(async move {
            let guard = q
                .acquire_grants_fifo(&["repo-grant".to_string()], &format!("writer-{idx}"), &c)
                .await
                .unwrap();
            o.lock().unwrap().push(idx);
            tokio::time::sleep(Duration::from_millis(10)).await;
            drop(guard);
        }));
        // Ensure deterministic FIFO registration order across tasks.
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    drop(first);
    for handle in handles {
        handle.await.unwrap();
    }
    assert_eq!(*order.lock().unwrap(), vec![2, 3, 4]);
    assert!(!queue.is_grant_busy("repo-grant"));
}

#[tokio::test]
async fn cancelled_waiter_does_not_starve_subsequent_fifo_waiters() {
    let queue = SharedWriteQueueRegistry::new();
    let hold_cancel = CancellationToken::new();
    let waiter2_cancel = CancellationToken::new();
    let waiter3_cancel = CancellationToken::new();

    let first = queue
        .acquire_grants_fifo(&["repo".to_string()], "writer-1", &hold_cancel)
        .await
        .unwrap();

    let q2 = queue.clone();
    let c2 = waiter2_cancel.clone();
    let h2 = tokio::spawn(async move {
        q2.acquire_grants_fifo(&["repo".to_string()], "writer-2", &c2)
            .await
    });
    tokio::time::sleep(Duration::from_millis(15)).await;

    let q3 = queue.clone();
    let c3 = waiter3_cancel.clone();
    let h3 = tokio::spawn(async move {
        q3.acquire_grants_fifo(&["repo".to_string()], "writer-3", &c3)
            .await
    });
    tokio::time::sleep(Duration::from_millis(15)).await;

    waiter2_cancel.cancel();
    assert!(h2.await.unwrap().is_err());

    drop(first);
    let g3 = tokio::time::timeout(Duration::from_secs(2), h3)
        .await
        .expect("waiter 3 must wake after cancelled waiter 2")
        .unwrap()
        .unwrap();
    drop(g3);
    assert!(!queue.is_grant_busy("repo"));
}

#[test]
fn resolve_effective_write_mode_tiers_and_escalates_on_contention() {
    let queue = SharedWriteQueueRegistry::new();
    let rw_mount = SpawnHostMount {
        grant: "repo-rw".to_string(),
        target: PathBuf::from("/mnt/repo"),
        access: SpawnMountAccess::ReadWrite,
    };
    let ro_mount = SpawnHostMount {
        grant: "repo-ro".to_string(),
        target: PathBuf::from("/mnt/repo"),
        access: SpawnMountAccess::ReadOnly,
    };

    assert_eq!(
        resolve_effective_write_mode(true, None, false, std::slice::from_ref(&ro_mount), &queue)
            .unwrap(),
        WorkstreamWriteMode::ReadOnly
    );
    assert!(
        resolve_effective_write_mode(
            true,
            Some(WorkstreamWriteMode::SharedQueue),
            false,
            std::slice::from_ref(&ro_mount),
            &queue,
        )
        .is_err()
    );
    assert_eq!(
        resolve_effective_write_mode(false, None, false, std::slice::from_ref(&rw_mount), &queue)
            .unwrap(),
        WorkstreamWriteMode::SharedQueue
    );

    // Simulate active shared_queue holder and request long-running contention escalation.
    {
        let mut guard = queue.queues.lock().unwrap();
        guard.insert(
            "repo-rw".to_string(),
            GrantQueueState {
                active_owner: Some("active-writer".to_string()),
                waiters: VecDeque::new(),
            },
        );
    }
    assert_eq!(
        resolve_effective_write_mode(false, None, true, &[rw_mount], &queue).unwrap(),
        WorkstreamWriteMode::Worktree
    );
}

#[tokio::test]
async fn git_worktree_isolation_port_narrows_grant_checks_3way_conflicts_and_syncs_cleanly() {
    let temp = TempDir::new().unwrap();
    let host_repo = temp.path().join("host-repo");
    fs::create_dir_all(&host_repo).unwrap();
    init_test_git_repo(&host_repo);

    let port =
        GitWorktreeIsolationPort::new(temp.path().join("staging"), temp.path().join("evidence"));
    port.register_grant("proj-rw", &host_repo, "/mnt/proj");

    let alloc = port
        .allocate_workspace(IsolatedWorkspaceRequest {
            workstream_name: "feature-a".to_string(),
            source_grant_id: "proj-rw".to_string(),
            source_namespace_path: PathBuf::from("/mnt/proj"),
            rollout_ref: Some("rollout://r1".to_string()),
            why: "Implement feature A".to_string(),
        })
        .await
        .unwrap();

    assert_eq!(alloc.backing, IsolatedWorkspaceBacking::GitWorktree);
    assert!(alloc.workspace_ref.starts_with("wt-"));
    assert_eq!(alloc.narrowed_grant.access, SpawnMountAccess::ReadWrite);
    assert!(
        !alloc
            .narrowed_grant
            .target
            .to_string_lossy()
            .contains(&temp.path().to_string_lossy().to_string())
    );

    let wt_host_path = port.host_worktree_path(&alloc.workspace_ref).unwrap();
    fs::write(wt_host_path.join("README.md"), "updated in worktree\n").unwrap();

    // Primary Host repo remains untouched while child edits worktree.
    assert_eq!(
        fs::read_to_string(host_repo.join("README.md")).unwrap(),
        "initial\n"
    );

    // Concurrent Host edit causes fail-closed 3-way conflict abort without overwriting Host.
    fs::write(host_repo.join("README.md"), "concurrent host edit\n").unwrap();
    let conflicted = port
        .sync_back(
            &alloc,
            "feature-a",
            Some("rollout://r1"),
            "Implement feature A",
        )
        .await
        .unwrap();
    assert_eq!(conflicted.sync_status, WorkspaceSyncStatus::ConflictAborted);
    assert_eq!(conflicted.conflicts.len(), 1);
    assert_eq!(conflicted.conflicts[0].path, "README.md");
    assert!(conflicted.patch_evidence_ref.is_some());
    assert_eq!(
        fs::read_to_string(host_repo.join("README.md")).unwrap(),
        "concurrent host edit\n"
    );

    // Restore Host file to clean base -> sync_back succeeds cleanly.
    fs::write(host_repo.join("README.md"), "initial\n").unwrap();
    let synced = port
        .sync_back(
            &alloc,
            "feature-a",
            Some("rollout://r1"),
            "Implement feature A",
        )
        .await
        .unwrap();
    assert_eq!(synced.sync_status, WorkspaceSyncStatus::SyncedBack);
    assert!(synced.conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(host_repo.join("README.md")).unwrap(),
        "updated in worktree\n"
    );

    port.discard_workspace(&alloc.workspace_ref).await.unwrap();
    assert!(!wt_host_path.exists());
}
