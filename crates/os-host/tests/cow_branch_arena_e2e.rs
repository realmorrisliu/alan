use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alan_agent_engine::tools::SandboxBackendKind;
use alan_agent_engine::{
    ArenaContenderRequest, ArenaMode, ArenaSpawnFn, ArenaSubagentOutput, ArenaVerifierRequest,
    BranchFsApClient, DEFAULT_ARENA_CONTENDER_MOUNT_TARGET, ExecutableArenaError,
    ExecutableArenaRequest, LlmClient, ToolRegistry,
};
use alan_agent_protocol::{SpawnHandle, SpawnMountAccess};
use alan_ap::InProcessTransport;
use alan_branchfs::contains_raw_host_path;
use alan_kernel::{Access, LiveNamespace, MountFs, Namespace, Pid};
use alan_llm::MockLlmProvider;
use alan_os_host::{
    AlanOsHost, CowHostRuntime, CowWorkspaceConfig, CowWorkspaceManager, HostBootConfig,
    HostEndpointPaths, LocalAttachment, SystemStorePaths, host_mounts,
};
use alan_service_manager::{HostMountAccess, HostMountService};
use alan_shell::Shell;
use serde_json::json;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

fn init_temp_git_repo(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::write(
        repo_dir.join("src/lib.rs"),
        "pub fn val() -> i32 {\n    1\n}\n",
    )
    .unwrap();
    fs::write(repo_dir.join("README.md"), "e2e test repo\n").unwrap();

    for args in [
        vec!["init"],
        vec!["config", "user.name", "Alan Test"],
        vec!["config", "user.email", "alan@example.com"],
        vec!["add", "."],
        vec!["commit", "-m", "initial"],
    ] {
        let output = Command::new("git")
            .args(&args)
            .current_dir(repo_dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Include an uncommitted local edit that the CoW clone must preserve while keeping Host isolated.
    fs::write(repo_dir.join("DIRTY.txt"), "local uncommitted edit\n").unwrap();
}

fn platform_os_sandbox_backend() -> SandboxBackendKind {
    #[cfg(target_os = "macos")]
    {
        SandboxBackendKind::Seatbelt
    }
    #[cfg(target_os = "linux")]
    {
        SandboxBackendKind::LinuxReifiedNamespace
    }
}

#[tokio::test]
async fn cow_manager_binds_branchfs_and_executable_arena_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let host_repo = temp.path().join("project");
    init_temp_git_repo(&host_repo);
    let canonical_repo = fs::canonicalize(&host_repo).unwrap();
    let repo_str = canonical_repo.to_string_lossy().to_string();

    let system_store = SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
    let (cow_manager, gc_report) = CowWorkspaceManager::new(
        CowWorkspaceConfig::new(system_store.clone(), "e2e-instance").unwrap(),
    )
    .unwrap();
    assert_eq!(gc_report.swept_workspaces, 0);

    let host_mount = HostMountService::new(Arc::new(host_mounts::NativeHostMountExportAdapter));
    let parent_pid = Pid(42);
    let parent_live_ns = LiveNamespace::new(Namespace::new());
    host_mount.register_process(parent_pid, parent_live_ns.clone());
    let req_id = host_mount
        .request_project_mount(parent_pid, HostMountAccess::ReadWrite)
        .unwrap();
    let source_grant = host_mounts::approve_host_mount(
        &host_mount,
        &req_id,
        &canonical_repo,
        "test-approval",
        "local-user",
    )
    .unwrap();

    let runtime = CowHostRuntime::new(cow_manager.clone());
    runtime.attach_host_mount(host_mount.clone());
    runtime.ensure_base_branch().await.unwrap();
    runtime
        .branch_adapter()
        .set_default_source_grant(Some(source_grant.id.clone()));

    let mut root_ns = Namespace::new();
    root_ns.mount(
        "/mnt/branch",
        InProcessTransport::new(runtime.branch_fs().clone()),
        Access::ReadWrite,
    );
    let branch_transport = InProcessTransport::new(Arc::new(MountFs::new(root_ns)));
    let branch_client = BranchFsApClient::at_namespace_mount(branch_transport);

    let arena_request = ExecutableArenaRequest {
        mode: ArenaMode::Executable,
        problem_statement: "Update val() to return 42 cleanly.".to_string(),
        acceptance_criteria: "src/lib.rs returns 42 and uncommitted DIRTY.txt stays intact."
            .to_string(),
        source_grant: Some(source_grant.id.clone()),
        base_branch_id: "base".to_string(),
        workstream: Some("fix-val".to_string()),
        rollout_ref: Some("rollout-e2e-1".to_string()),
        why: Some("Verify executable alan-arena end-to-end CoW lifecycle".to_string()),
        network_denied: true,
        max_contenders: 4,
        contenders: vec![
            ArenaContenderRequest {
                candidate_id: "cand-alpha".to_string(),
                role: "arena-contender-1".to_string(),
                model: Some("model-a".to_string()),
                task: "Set val() to 42 in src/lib.rs".to_string(),
                timeout_secs: Some(60),
            },
            ArenaContenderRequest {
                candidate_id: "cand-beta".to_string(),
                role: "arena-contender-2".to_string(),
                model: Some("model-b".to_string()),
                task: "Set val() to 99 in src/lib.rs".to_string(),
                timeout_secs: Some(60),
            },
        ],
        verifier: ArenaVerifierRequest::default(),
        read_only_mounts: Vec::new(),
    };

    // 1. Fail-closed gate: HostMountPathGuard MUST refuse before creating any CoW clone.
    let noop_spawn: ArenaSpawnFn =
        Arc::new(|_, _| Box::pin(async { Ok(ArenaSubagentOutput::completed("unexpected")) }));
    let blocked_coord = runtime
        .arena_coordinator(host_mount.clone(), parent_pid)
        .with_sandbox_backend(SandboxBackendKind::HostMountPathGuard);
    let err = blocked_coord
        .run_bakeoff(&arena_request, noop_spawn, &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ExecutableArenaError::UnsafeSandboxBackend {
            backend: "host_mount_path_guard"
        }
    ));
    assert_eq!(cow_manager.active_count(), 0);

    // 2. Full executable bakeoff behind OS-enforced sandbox backend.
    let coordinator = runtime
        .arena_coordinator(host_mount.clone(), parent_pid)
        .with_sandbox_backend(platform_os_sandbox_backend());
    let next_child_pid = Arc::new(AtomicU64::new(100));
    let hm_for_spawn = host_mount.clone();
    let repo_str_for_spawn = repo_str.clone();
    let source_grant_id = source_grant.id.clone();

    let spawn_fn: ArenaSpawnFn = Arc::new(move |spec, _cancel| {
        let hm = hm_for_spawn.clone();
        let next_pid = next_child_pid.clone();
        let repo_str = repo_str_for_spawn.clone();
        let source_grant_id = source_grant_id.clone();
        Box::pin(async move {
            let role = match &spec.target {
                alan_agent_protocol::SpawnTarget::AdHocSubagent { role } => role.clone(),
                _ => String::new(),
            };
            assert_eq!(spec.handles, vec![SpawnHandle::ApprovalScope]);
            assert!(
                !spec.launch.task.contains(&repo_str),
                "raw Host path leaked into subagent task: {}",
                spec.launch.task
            );
            assert!(
                !contains_raw_host_path(&spec.launch.task),
                "raw Host path pattern leaked into subagent task: {}",
                spec.launch.task
            );

            let child_pid = Pid(next_pid.fetch_add(1, Ordering::Relaxed));
            let child_ns = LiveNamespace::new(Namespace::new());
            hm.register_child_process(parent_pid, child_pid, child_ns.clone(), &spec.host_mounts)
                .unwrap();
            let shell = Shell::new(InProcessTransport::new(Arc::new(
                MountFs::from_live_namespace(child_ns),
            )));

            if role == "arena-verifier" {
                assert!(spec.runtime_overrides.read_only);
                assert_eq!(spec.host_mounts.len(), 2);
                assert!(
                    spec.host_mounts
                        .iter()
                        .all(|m| m.access == SpawnMountAccess::ReadOnly)
                );

                // Read-only view sees candidate mutations directly via delegated HostMountService.
                let alpha_src = String::from_utf8(
                    shell
                        .cat("/mnt/arena-candidates/cand-alpha/src/lib.rs")
                        .await
                        .unwrap(),
                )
                .unwrap();
                let beta_src = String::from_utf8(
                    shell
                        .cat("/mnt/arena-candidates/cand-beta/src/lib.rs")
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert!(alpha_src.contains("42"));
                assert!(beta_src.contains("99"));

                // Verifier write must fail even if attempted on its ReadOnly mount.
                assert!(
                    shell
                        .write("/mnt/arena-candidates/cand-alpha/src/lib.rs", b"tamper")
                        .await
                        .is_err()
                );

                hm.unregister_process(child_pid);
                Ok(
                    ArenaSubagentOutput::completed("verified").with_structured_output(json!({
                        "scores": [
                            {"candidate": "Contender A", "score": 0.98, "summary": "val() == 42"},
                            {"candidate": "Contender B", "score": 0.40, "summary": "val() == 99"}
                        ],
                        "winner": "Contender A",
                        "rationale": "Contender A satisfies src/lib.rs == 42"
                    })),
                )
            } else {
                assert!(!spec.runtime_overrides.read_only);
                assert_eq!(spec.host_mounts.len(), 1);
                assert_eq!(spec.host_mounts[0].access, SpawnMountAccess::ReadWrite);
                assert_ne!(spec.host_mounts[0].grant, source_grant_id);
                assert_eq!(
                    spec.host_mounts[0].target,
                    PathBuf::from(DEFAULT_ARENA_CONTENDER_MOUNT_TARGET)
                );

                // Verify uncommitted DIRTY.txt was cloned and `.git/HEAD` cannot be written.
                let dirty =
                    String::from_utf8(shell.cat("/mnt/work/DIRTY.txt").await.unwrap()).unwrap();
                assert_eq!(dirty, "local uncommitted edit\n");
                assert!(
                    shell
                        .write("/mnt/work/.git/HEAD", b"corrupt")
                        .await
                        .is_err()
                );

                let new_val = if spec.launch.task.contains("cand-alpha") {
                    42
                } else {
                    99
                };
                shell
                    .write(
                        "/mnt/work/src/lib.rs",
                        format!("pub fn val() -> i32 {{\n    {new_val}\n}}\n").as_bytes(),
                    )
                    .await
                    .unwrap();

                hm.unregister_process(child_pid);
                Ok(ArenaSubagentOutput::completed(format!(
                    "Updated src/lib.rs to {new_val}"
                )))
            }
        })
    });

    let result = coordinator
        .run_bakeoff(&arena_request, spawn_fn, &CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(result.mode, ArenaMode::Executable);
    assert_eq!(result.selected_candidate_id, "cand-alpha");
    let winner_ws_ref = result.selected_workspace_ref.clone().unwrap();
    assert_eq!(result.purged_losers, vec!["cand-beta".to_string()]);
    assert_eq!(cow_manager.active_count(), 1);

    // Inspect /mnt/branch records via aP: opaque workspace_ref, zero raw Host paths.
    let winner_branch = branch_client.read_branch("cand-alpha").await.unwrap();
    assert_eq!(
        winner_branch.workspace_ref.as_deref(),
        Some(winner_ws_ref.as_str())
    );
    assert!(
        winner_branch
            .diff_summary
            .as_deref()
            .is_some_and(|s| s.contains("src/lib.rs"))
    );
    assert!(!contains_raw_host_path(
        winner_branch.diff_summary.as_deref().unwrap()
    ));
    assert!(branch_client.read_branch("cand-beta").await.is_err());

    let selected_doc = branch_client.read_selected().await.unwrap();
    let selected_text = serde_json::to_string(&selected_doc).unwrap();
    assert!(!selected_text.contains(&repo_str));
    assert!(!contains_raw_host_path(&selected_text));

    let events_text = branch_client.read_events().await.unwrap();
    assert!(!events_text.contains(&repo_str));
    assert!(!contains_raw_host_path(&events_text));

    // Host project files remain completely untouched.
    assert_eq!(
        fs::read_to_string(canonical_repo.join("src/lib.rs")).unwrap(),
        "pub fn val() -> i32 {\n    1\n}\n"
    );

    // 3. Exit flush writes unified .patch + scorecard summary and deletes physical clone.
    let flush_report = runtime.flush_on_exit(Some(&host_mount)).await.unwrap();
    assert_eq!(cow_manager.active_count(), 0);
    assert!(flush_report.flushed_workspaces.is_empty());

    let evidence_dir = system_store.cow_workspace_evidence().unwrap();
    let patch_path = evidence_dir.join(format!("{winner_ws_ref}.patch"));
    let summary_path = evidence_dir.join(format!("{winner_ws_ref}.summary.json"));
    assert!(patch_path.is_file(), "expected flushed winner patch");
    assert!(summary_path.is_file(), "expected flushed winner summary");

    let patch_text = fs::read_to_string(&patch_path).unwrap();
    assert!(
        patch_text.contains("Alan-Workstream: fix-val"),
        "{patch_text}"
    );
    assert!(
        patch_text.contains("Alan-Rollout: rollout-e2e-1"),
        "{patch_text}"
    );
    assert!(
        patch_text.contains("Alan-Why: Verify executable alan-arena end-to-end CoW lifecycle"),
        "{patch_text}"
    );
    assert!(patch_text.contains("+    42"), "{patch_text}");
}

#[tokio::test]
async fn alan_os_host_boots_mounts_branchfs_and_flushes_cow_on_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let host_repo = temp.path().join("repo");
    init_temp_git_repo(&host_repo);
    let canonical_repo = fs::canonicalize(&host_repo).unwrap();

    let system_store = SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
    let (cow_manager, _) = CowWorkspaceManager::new(
        CowWorkspaceConfig::new(system_store.clone(), "host-e2e").unwrap(),
    )
    .unwrap();

    let boot_config = HostBootConfig::ephemeral(
        Default::default(),
        LlmClient::new(MockLlmProvider::new()),
        ToolRegistry::new(),
    )
    .with_cow_workspace_manager(cow_manager.clone());
    boot_config
        .cow_runtime()
        .unwrap()
        .branch_adapter()
        .set_default_host_root(Some(canonical_repo.clone()));

    let runtime_dir = temp.path().join("runtime");
    fs::create_dir_all(&runtime_dir).unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(&runtime_dir).unwrap();
    let host = AlanOsHost::boot(boot_config, paths.clone()).await.unwrap();

    let (tx, rx) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        host.serve_until(async move {
            let _ = rx.await;
        })
        .await
    });

    let attachment = LocalAttachment::new(paths).connect().await.unwrap();
    let shell = Shell::new(attachment.root.clone());

    // `/mnt/branch` is live inside the booted Host namespace and triggers CoW creation via `ctl`.
    let entries = shell.ls("/mnt/branch").await.unwrap();
    assert!(entries.iter().any(|e| e == "ctl"));

    shell
        .write(
            "/mnt/branch/ctl",
            br#"{"op":"fork","id":"cand-host","from":"base","delta":"host e2e branch"}"#,
        )
        .await
        .unwrap();
    assert_eq!(cow_manager.active_count(), 1);

    let record_raw =
        String::from_utf8(shell.cat("/mnt/branch/branches/cand-host").await.unwrap()).unwrap();
    let record: serde_json::Value = serde_json::from_str(&record_raw).unwrap();
    let ws_ref = record["workspace_ref"].as_str().unwrap().to_string();
    assert!(!contains_raw_host_path(&record_raw));

    shell
        .write(
            "/mnt/branch/ctl",
            br#"{"op":"score","id":"cand-host","score":0.91,"summary":"host branch ready"}"#,
        )
        .await
        .unwrap();
    shell
        .write("/mnt/branch/ctl", br#"{"op":"select","id":"cand-host"}"#)
        .await
        .unwrap();

    let _ = tx.send(());
    server.await.unwrap().unwrap();

    // Host exit must purge the active CoW clone and persist evidence.
    assert_eq!(cow_manager.active_count(), 0);
    let evidence_dir = system_store.cow_workspace_evidence().unwrap();
    assert!(
        evidence_dir.join(format!("{ws_ref}.patch")).is_file(),
        "expected Host shutdown to flush selected /mnt/branch workspace evidence"
    );
}
