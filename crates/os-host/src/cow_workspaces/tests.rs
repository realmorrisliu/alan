use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use alan_agent_engine::tools::{ToolExecutionAuthority, ToolExecutionBinding};
use alan_ap::{ErrorCode, Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{LiveNamespace, MountFs, Namespace, Pid};
use alan_service_manager::{HostMountAccess, HostMountService};

use super::hash::{Sha256, to_hex};
use super::*;
use crate::host_mounts::NativeHostMountExportAdapter;
use crate::paths::SystemStorePaths;

fn test_manager(
    temp: &Path,
    instance_id: &str,
    max_active: usize,
) -> (CowWorkspaceManager, GcReport) {
    let system_store = SystemStorePaths::from_root(&temp.join("System Store")).unwrap();
    let mut config = CowWorkspaceConfig::new(system_store, instance_id).unwrap();
    config.max_active_cow_workspaces = max_active;
    CowWorkspaceManager::new(config).unwrap()
}

fn init_git_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["init"]);
    run(&["config", "user.name", "Alan Test"]);
    run(&["config", "user.email", "alan-test@example.com"]);
}

fn git_commit_all(dir: &Path, msg: &str) {
    let add = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["add", "-A"])
        .output()
        .unwrap();
    assert!(add.status.success());
    let commit = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["commit", "-m", msg])
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

#[test]
fn sha256_matches_nist_test_vectors() {
    assert_eq!(
        to_hex(&Sha256::digest_bytes(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        to_hex(&Sha256::digest_bytes(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn cow_clone_preserves_uncommitted_edits_and_caches_while_isolating_git() {
    let temp = tempfile::tempdir().unwrap();
    let host_repo = temp.path().join("project");
    fs::create_dir_all(host_repo.join("src")).unwrap();
    fs::write(host_repo.join("src/lib.rs"), "pub fn v() -> u32 { 1 }\n").unwrap();
    init_git_repo(&host_repo);
    git_commit_all(&host_repo, "initial commit");

    // Add uncommitted edit and ignored build caches before cloning.
    fs::write(host_repo.join("src/dirty.rs"), "pub fn dirty() {}\n").unwrap();
    fs::create_dir_all(host_repo.join("target/debug")).unwrap();
    fs::write(host_repo.join("target/debug/cache.bin"), b"cached-build").unwrap();
    fs::create_dir_all(host_repo.join("node_modules/pkg")).unwrap();
    fs::write(
        host_repo.join("node_modules/pkg/index.js"),
        "module.exports = 1;",
    )
    .unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-1", 4);
    let summary = mgr
        .create_workspace(&host_repo, CreateWorkspaceOptions::default())
        .unwrap();

    #[cfg(target_os = "macos")]
    assert_eq!(summary.backing_kind, WorkspaceBacking::NativeCow);

    let host_head_before = fs::read(host_repo.join(".git/HEAD")).unwrap();
    let tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();

    if summary.backing_kind == WorkspaceBacking::NativeCow {
        assert_eq!(
            fs::read_to_string(tree_path.join("src/dirty.rs")).unwrap(),
            "pub fn dirty() {}\n"
        );
        assert_eq!(
            fs::read(tree_path.join("target/debug/cache.bin")).unwrap(),
            b"cached-build"
        );
        assert_eq!(
            fs::read_to_string(tree_path.join("node_modules/pkg/index.js")).unwrap(),
            "module.exports = 1;"
        );
    }

    // Mutate .git/, target/, and source file inside the clone.
    let clone_git_head = tree_path.join(".git/HEAD");
    if clone_git_head.exists() {
        fs::write(&clone_git_head, "ref: refs/heads/hijacked\n").unwrap();
    } else if tree_path.join(".git").is_file() {
        fs::write(tree_path.join(".git"), "corrupted").unwrap();
    }
    fs::create_dir_all(tree_path.join("target/debug")).unwrap();
    fs::write(tree_path.join("target/debug/new.o"), b"obj").unwrap();
    fs::write(tree_path.join("src/lib.rs"), "pub fn v() -> u32 { 2 }\n").unwrap();

    // Host .git and source remain untouched.
    assert_eq!(
        fs::read(host_repo.join(".git/HEAD")).unwrap(),
        host_head_before
    );
    assert_eq!(
        fs::read_to_string(host_repo.join("src/lib.rs")).unwrap(),
        "pub fn v() -> u32 { 1 }\n"
    );

    // Diff summary excludes .git/ and target/ / node_modules/.
    let diff = mgr.diff_summary(&summary.workspace_ref).unwrap();
    assert_eq!(diff.files_modified, 1);
    assert_eq!(diff.files_added, 0);
    assert_eq!(diff.files_deleted, 0);
    assert_eq!(diff.changed_paths[0].path, "src/lib.rs");
}

#[test]
fn exdev_cross_volume_falls_back_to_git_worktree_and_cleans_up_registration() {
    let temp = tempfile::tempdir().unwrap();
    let host_repo = temp.path().join("repo");
    fs::create_dir_all(host_repo.join("src")).unwrap();
    fs::write(host_repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    init_git_repo(&host_repo);
    git_commit_all(&host_repo, "init");

    let (mgr, _) = test_manager(temp.path(), "inst-exdev", 4);
    let summary = mgr
        .create_workspace(
            &host_repo,
            CreateWorkspaceOptions {
                fallback_policy: CloneFallbackPolicy::ForceCrossVolumeFallback,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(summary.backing_kind, WorkspaceBacking::GitWorktree);
    assert!(summary.source_is_git);

    let wt_list = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&wt_list.stdout).contains(summary.workspace_ref.as_str()),
        "git worktree list should include fallback worktree"
    );

    let tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();
    fs::write(
        tree_path.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(host_repo.join("src/main.rs")).unwrap(),
        "fn main() {}\n"
    );

    mgr.discard_workspace(&summary.workspace_ref, None, "test")
        .unwrap();
    assert!(!tree_path.exists());
    let wt_after = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&wt_after.stdout).contains(summary.workspace_ref.as_str()));
}

#[test]
fn cross_volume_non_git_errors_unless_sibling_volume_enabled() {
    let temp = tempfile::tempdir().unwrap();
    let non_git = temp.path().join("plain-dir");
    fs::create_dir_all(&non_git).unwrap();
    fs::write(non_git.join("notes.txt"), "hello\n").unwrap();

    let (mgr_strict, _) = test_manager(temp.path(), "inst-strict", 4);
    let err = mgr_strict
        .create_workspace(
            &non_git,
            CreateWorkspaceOptions {
                fallback_policy: CloneFallbackPolicy::ForceCrossVolumeFallback,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CowWorkspaceError::CrossVolumeNonGitUnsupported
    ));

    #[cfg(target_os = "macos")]
    {
        let system_store = SystemStorePaths::from_root(&temp.path().join("System Store")).unwrap();
        let mut cfg = CowWorkspaceConfig::new(system_store, "inst-sibling").unwrap();
        cfg.allow_sibling_project_volume_staging = true;
        let (mgr_sibling, _) = CowWorkspaceManager::new(cfg).unwrap();
        let summary = mgr_sibling
            .create_workspace(
                &non_git,
                CreateWorkspaceOptions {
                    fallback_policy: CloneFallbackPolicy::ForceCrossVolumeFallback,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(summary.backing_kind, WorkspaceBacking::SiblingVolumeCow);
        let tree = mgr_sibling
            .state
            .lock()
            .unwrap()
            .workspaces
            .get(&summary.workspace_ref)
            .unwrap()
            .tree_path
            .clone();
        fs::write(tree.join("notes.txt"), "hello sibling\n").unwrap();
        let prov = IntentProvenance::new("ws-1", "rollout-1", "sibling update").unwrap();
        mgr_sibling
            .sync_back(&summary.workspace_ref, &prov)
            .unwrap();
        assert_eq!(
            fs::read_to_string(non_git.join("notes.txt")).unwrap(),
            "hello sibling\n"
        );
        mgr_sibling
            .discard_workspace(&summary.workspace_ref, None, "test")
            .unwrap();
        assert!(!tree.exists());
    }
}

#[test]
fn concurrent_host_edits_fail_sync_back_closed_and_allow_patch_export() {
    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("project");
    fs::create_dir_all(&host_dir).unwrap();
    fs::write(host_dir.join("both_mod.txt"), "base\n").unwrap();
    fs::write(host_dir.join("ws_del_host_mod.txt"), "base\n").unwrap();
    fs::write(host_dir.join("ws_mod_host_del.txt"), "base\n").unwrap();
    fs::write(host_dir.join("clean.txt"), "clean base\n").unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-conflict", 4);
    let summary = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    let tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();

    // Workspace edits
    fs::write(tree_path.join("both_mod.txt"), "from workspace\n").unwrap();
    fs::write(tree_path.join("both_add.txt"), "ws created\n").unwrap();
    fs::remove_file(tree_path.join("ws_del_host_mod.txt")).unwrap();
    fs::write(tree_path.join("ws_mod_host_del.txt"), "ws modified\n").unwrap();
    fs::write(tree_path.join("clean.txt"), "clean updated\n").unwrap();

    // Concurrent Host edits
    fs::write(host_dir.join("both_mod.txt"), "from host\n").unwrap();
    fs::write(host_dir.join("both_add.txt"), "host created\n").unwrap();
    fs::write(host_dir.join("ws_del_host_mod.txt"), "host modified\n").unwrap();
    fs::remove_file(host_dir.join("ws_mod_host_del.txt")).unwrap();

    let prov = IntentProvenance::new("ws-conflict", "rollout-99", "Attempt merge").unwrap();
    let err = mgr.sync_back(&summary.workspace_ref, &prov).unwrap_err();
    match err {
        CowWorkspaceError::SyncConflict { conflicts } => {
            assert_eq!(conflicts.len(), 4);
            assert!(conflicts.contains(&SyncConflict {
                path: "both_add.txt".to_string(),
                reason: ConflictReason::CreatedInBoth,
            }));
            assert!(conflicts.contains(&SyncConflict {
                path: "both_mod.txt".to_string(),
                reason: ConflictReason::ModifiedInBoth,
            }));
            assert!(conflicts.contains(&SyncConflict {
                path: "ws_del_host_mod.txt".to_string(),
                reason: ConflictReason::DeletedInWorkspaceModifiedOnHost,
            }));
            assert!(conflicts.contains(&SyncConflict {
                path: "ws_mod_host_del.txt".to_string(),
                reason: ConflictReason::ModifiedInWorkspaceDeletedOnHost,
            }));
        }
        other => panic!("expected SyncConflict, got {other:?}"),
    }

    // Fail-closed guarantee: even non-conflicting `clean.txt` was NOT overwritten.
    assert_eq!(
        fs::read_to_string(host_dir.join("clean.txt")).unwrap(),
        "clean base\n"
    );
    assert_eq!(
        fs::read_to_string(host_dir.join("both_mod.txt")).unwrap(),
        "from host\n"
    );

    // Clean unified patch export remains available against BaseManifest!
    let patch = mgr.export_patch(&summary.workspace_ref, &prov).unwrap();
    assert!(patch.patch_text.contains("Alan-Workstream: ws-conflict"));
    assert!(patch.patch_text.contains("Alan-Rollout: rollout-99"));
    assert!(patch.patch_text.contains("-base\n+from workspace"));
}

#[test]
fn clean_sync_back_and_patch_export_work_on_non_git_directory_and_git_apply() {
    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("non-git");
    fs::create_dir_all(host_dir.join("sub")).unwrap();
    fs::write(host_dir.join("a.txt"), "line1\nline2\nline3\n").unwrap();
    fs::write(host_dir.join("remove_me.txt"), "bye\n").unwrap();
    fs::write(host_dir.join("host_untouched_by_ws.txt"), "initial\n").unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-nongit", 4);
    let summary = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    assert!(!summary.source_is_git);

    let tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();

    fs::write(tree_path.join("a.txt"), "line1\nline2-edited\nline3\n").unwrap();
    fs::write(tree_path.join("sub/added.txt"), "new file\n").unwrap();
    fs::remove_file(tree_path.join("remove_me.txt")).unwrap();
    // Ignored cache files in workspace should not sync back.
    fs::create_dir_all(tree_path.join("target")).unwrap();
    fs::write(tree_path.join("target/build.log"), "ignored").unwrap();

    // Host edits an unrelated file concurrently — should not conflict and must be preserved!
    fs::write(
        host_dir.join("host_untouched_by_ws.txt"),
        "host concurrent non-conflicting edit\n",
    )
    .unwrap();

    let prov = IntentProvenance::new(
        "ws-nongit",
        "rollout-7",
        "Refactor a.txt and add sub/added.txt",
    )
    .unwrap();
    let patch_report = mgr.export_patch(&summary.workspace_ref, &prov).unwrap();
    assert!(
        patch_report
            .patch_text
            .contains("Alan-Workstream: ws-nongit")
    );
    assert!(patch_report.patch_text.contains("Alan-Rollout: rollout-7"));
    assert!(
        patch_report
            .patch_text
            .contains("Alan-Why: Refactor a.txt and add sub/added.txt")
    );
    assert!(!patch_report.patch_text.contains("target/build.log"));

    // Verify `git apply` accepts our generated unified patch on a fresh git repo copy!
    let verify_repo = temp.path().join("verify-apply");
    fs::create_dir_all(verify_repo.join("sub")).unwrap();
    fs::write(verify_repo.join("a.txt"), "line1\nline2\nline3\n").unwrap();
    fs::write(verify_repo.join("remove_me.txt"), "bye\n").unwrap();
    init_git_repo(&verify_repo);
    git_commit_all(&verify_repo, "base");
    let patch_file = temp.path().join("out.patch");
    fs::write(&patch_file, &patch_report.patch_text).unwrap();
    let apply_out = Command::new("git")
        .arg("-C")
        .arg(&verify_repo)
        .args(["apply", "--check"])
        .arg(&patch_file)
        .output()
        .unwrap();
    assert!(
        apply_out.status.success(),
        "git apply --check failed: {}\nPATCH:\n{}",
        String::from_utf8_lossy(&apply_out.stderr),
        patch_report.patch_text
    );
    let apply_run = Command::new("git")
        .arg("-C")
        .arg(&verify_repo)
        .arg("apply")
        .arg(&patch_file)
        .output()
        .unwrap();
    assert!(apply_run.status.success());
    assert_eq!(
        fs::read_to_string(verify_repo.join("a.txt")).unwrap(),
        "line1\nline2-edited\nline3\n"
    );
    assert_eq!(
        fs::read_to_string(verify_repo.join("sub/added.txt")).unwrap(),
        "new file\n"
    );
    assert!(!verify_repo.join("remove_me.txt").exists());

    // Now sync back to host_dir.
    let report = mgr.sync_back(&summary.workspace_ref, &prov).unwrap();
    assert_eq!(report.applied_paths, vec!["a.txt", "sub/added.txt"]);
    assert_eq!(report.deleted_paths, vec!["remove_me.txt"]);
    assert_eq!(
        fs::read_to_string(host_dir.join("a.txt")).unwrap(),
        "line1\nline2-edited\nline3\n"
    );
    assert_eq!(
        fs::read_to_string(host_dir.join("host_untouched_by_ws.txt")).unwrap(),
        "host concurrent non-conflicting edit\n"
    );
    assert!(!host_dir.join("target/build.log").exists());

    // Audit file exists in System Store without raw Host paths.
    let audit = fs::read_to_string(
        temp.path()
            .join("System Store/services/host-mount/cow-sync-audit.jsonl"),
    )
    .unwrap();
    assert!(audit.contains(&report.audit_record_id));
    assert!(!audit.contains(tree_path.to_string_lossy().as_ref()));
}

#[test]
fn git_branch_export_creates_branch_with_trailers_without_touching_host_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let host_repo = temp.path().join("repo");
    fs::create_dir_all(host_repo.join("src")).unwrap();
    fs::write(host_repo.join("src/lib.rs"), "pub fn old() {}\n").unwrap();
    fs::write(host_repo.join("src/drop.rs"), "drop\n").unwrap();
    init_git_repo(&host_repo);
    git_commit_all(&host_repo, "base commit");

    let (mgr, _) = test_manager(temp.path(), "inst-branch", 4);
    let summary = mgr
        .create_workspace(&host_repo, CreateWorkspaceOptions::default())
        .unwrap();
    let tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();

    fs::write(tree_path.join("src/lib.rs"), "pub fn new_impl() {}\n").unwrap();
    fs::write(tree_path.join("src/added.rs"), "pub fn added() {}\n").unwrap();
    fs::remove_file(tree_path.join("src/drop.rs")).unwrap();
    fs::create_dir_all(tree_path.join("target")).unwrap();
    fs::write(tree_path.join("target/artifact.bin"), "ignore-me").unwrap();

    // Host has a dirty file and remains on original branch.
    fs::write(host_repo.join("host_dirty.txt"), "stay dirty\n").unwrap();
    let head_before = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();

    let prov = IntentProvenance::new(
        "ws-branch",
        "rollout-55",
        "Implement new_impl on isolated branch",
    )
    .unwrap();
    let branch_report = mgr
        .export_to_git_branch(&summary.workspace_ref, "feat/cow-winner", &prov)
        .unwrap();
    assert_eq!(branch_report.branch_name, "feat/cow-winner");

    let head_after = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    assert_eq!(head_before.stdout, head_after.stdout);
    assert_eq!(
        fs::read_to_string(host_repo.join("host_dirty.txt")).unwrap(),
        "stay dirty\n"
    );
    assert_eq!(
        fs::read_to_string(host_repo.join("src/lib.rs")).unwrap(),
        "pub fn old() {}\n"
    );

    let commit_msg = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["log", "-1", "--format=%B", "feat/cow-winner"])
        .output()
        .unwrap();
    let msg_str = String::from_utf8_lossy(&commit_msg.stdout);
    assert!(msg_str.contains("Implement new_impl on isolated branch"));
    assert!(msg_str.contains("Alan-Workstream: ws-branch"));
    assert!(msg_str.contains("Alan-Rollout: rollout-55"));

    let ls_tree = Command::new("git")
        .arg("-C")
        .arg(&host_repo)
        .args(["ls-tree", "-r", "--name-only", "feat/cow-winner"])
        .output()
        .unwrap();
    let files = String::from_utf8_lossy(&ls_tree.stdout);
    assert!(files.contains("src/added.rs"));
    assert!(!files.contains("src/drop.rs"));
    assert!(!files.contains("target/artifact.bin"));
}

#[test]
fn active_clone_cap_and_startup_dead_pid_orphan_gc() {
    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("project");
    fs::create_dir_all(&host_dir).unwrap();
    fs::write(host_dir.join("file.txt"), "hello\n").unwrap();

    // Seed one dead-PID orphan instance and one live-PID instance before manager startup.
    let workspaces_root = temp.path().join("System Store/workspaces");
    let dead_inst = workspaces_root.join("inst-dead");
    fs::create_dir_all(dead_inst.join("cow-ws-stale/tree")).unwrap();
    fs::write(
        dead_inst.join("instance.json"),
        serde_json::to_vec(&serde_json::json!({
            "instance_id": "inst-dead",
            "pid": 999_999_999u32,
            "created_at_ms": 1u64
        }))
        .unwrap(),
    )
    .unwrap();

    let live_inst = workspaces_root.join("inst-live-other");
    fs::create_dir_all(live_inst.join("cow-ws-live/tree")).unwrap();
    fs::write(
        live_inst.join("instance.json"),
        serde_json::to_vec(&serde_json::json!({
            "instance_id": "inst-live-other",
            "pid": std::process::id(),
            "created_at_ms": 1u64
        }))
        .unwrap(),
    )
    .unwrap();

    let (mgr, gc_report) = test_manager(temp.path(), "inst-current", 2);
    assert_eq!(gc_report.swept_instances, vec!["inst-dead"]);
    assert_eq!(gc_report.swept_workspaces, 1);
    assert_eq!(gc_report.skipped_live_instances, vec!["inst-live-other"]);
    assert!(!dead_inst.exists());
    assert!(live_inst.exists());

    // Cap enforcement: max_active = 2.
    let w1 = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    let _w2 = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    let err = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap_err();
    assert!(matches!(
        err,
        CowWorkspaceError::ActiveCapacityReached { active: 2, max: 2 }
    ));

    mgr.discard_workspace(&w1.workspace_ref, None, "test")
        .unwrap();
    let _w3 = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    assert_eq!(mgr.active_count(), 2);
}

#[tokio::test]
async fn mnt_projection_hides_raw_paths_and_enforces_rw_vs_ro_mounts() {
    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("project");
    fs::create_dir_all(&host_dir).unwrap();
    fs::write(host_dir.join("code.rs"), "fn main() {}\n").unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-mnt", 4);
    let summary = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();

    let staging_prefix = temp
        .path()
        .join("System Store/workspaces")
        .to_string_lossy()
        .to_string();
    let summary_json = serde_json::to_string(&summary).unwrap();
    assert!(!summary_json.contains(&staging_prefix));
    assert!(!summary_json.contains(host_dir.to_string_lossy().as_ref()));

    let service = HostMountService::new(Arc::new(NativeHostMountExportAdapter));
    let contender_ns = LiveNamespace::new(Namespace::new());
    let verifier_ns = LiveNamespace::new(Namespace::new());
    service.register_process(Pid(10), contender_ns.clone());
    service.register_process(Pid(11), verifier_ns.clone());

    let rw_grant = mgr
        .project_into_process(
            &summary.workspace_ref,
            &service,
            Pid(10),
            "/mnt/work",
            HostMountAccess::ReadWrite,
        )
        .unwrap();
    let ro_grant = mgr
        .project_into_process(
            &summary.workspace_ref,
            &service,
            Pid(11),
            "/mnt/verify",
            HostMountAccess::ReadOnly,
        )
        .unwrap();

    let rw_grant_json = serde_json::to_string(&rw_grant).unwrap();
    assert!(!rw_grant_json.contains(&staging_prefix));
    assert!(!rw_grant_json.contains(host_dir.to_string_lossy().as_ref()));
    assert_eq!(ro_grant.access, HostMountAccess::ReadOnly);

    // Contender can write via aP /mnt/work without touching Host.
    let contender_root =
        InProcessTransport::new(Arc::new(MountFs::from_live_namespace(contender_ns)));
    contender_root
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: vec!["mnt".into(), "work".into(), "code.rs".into()],
        })
        .await
        .unwrap();
    contender_root
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Write,
        })
        .await
        .unwrap();
    contender_root
        .call(Request::Write {
            fid: Fid(1),
            offset: 0,
            data: b"fn main() { /* contender */ }\n".to_vec(),
        })
        .await
        .unwrap();
    contender_root
        .call(Request::Clunk { fid: Fid(1) })
        .await
        .unwrap();

    assert_eq!(
        fs::read_to_string(host_dir.join("code.rs")).unwrap(),
        "fn main() {}\n"
    );

    // Verifier cannot write via aP /mnt/verify and sandbox is read-only.
    let verifier_root =
        InProcessTransport::new(Arc::new(MountFs::from_live_namespace(verifier_ns)));
    verifier_root
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(2),
            names: vec!["mnt".into(), "verify".into(), "code.rs".into()],
        })
        .await
        .unwrap();
    assert_eq!(
        verifier_root
            .call(Request::Open {
                fid: Fid(2),
                mode: OpenMode::Write,
            })
            .await,
        Err(ErrorCode::NoAccess)
    );
    let verifier_binding = service
        .reconcile(
            11,
            ToolExecutionBinding::awaiting_host_projection(
                "/mnt/verify".into(),
                "/tmp/scratch".into(),
            ),
        )
        .unwrap();
    let verifier_adapter = verifier_binding.adapter().unwrap();
    let clone_tree_path = mgr
        .state
        .lock()
        .unwrap()
        .workspaces
        .get(&summary.workspace_ref)
        .unwrap()
        .tree_path
        .clone();
    assert!(
        verifier_adapter
            .sandbox()
            .unwrap()
            .is_readable(&clone_tree_path)
    );
    assert!(
        !verifier_adapter
            .sandbox()
            .unwrap()
            .is_writable(&clone_tree_path)
    );

    // Read back contender edit via verifier aP view.
    verifier_root
        .call(Request::Open {
            fid: Fid(2),
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
    let Response::Read { data } = verifier_root
        .call(Request::Read {
            fid: Fid(2),
            offset: 0,
            count: 256,
        })
        .await
        .unwrap()
    else {
        panic!("expected Read response");
    };
    assert_eq!(data, b"fn main() { /* contender */ }\n");

    // Discard revokes both grants and deletes clone.
    mgr.discard_workspace(&summary.workspace_ref, Some(&service), "test")
        .unwrap();
    assert!(!clone_tree_path.exists());
    assert!(!service.grant_record(&rw_grant.id).unwrap().active);
    assert!(!service.grant_record(&ro_grant.id).unwrap().active);
}

#[test]
fn exit_flush_writes_unapplied_winner_patch_and_summary_before_deleting_clones() {
    let temp = tempfile::tempdir().unwrap();
    let host_dir = temp.path().join("project");
    fs::create_dir_all(&host_dir).unwrap();
    fs::write(host_dir.join("lib.rs"), "pub fn x() -> i32 { 0 }\n").unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-exit", 4);
    let winner = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();
    let loser = mgr
        .create_workspace(&host_dir, CreateWorkspaceOptions::default())
        .unwrap();

    let (winner_tree, loser_tree) = {
        let guard = mgr.state.lock().unwrap();
        (
            guard
                .workspaces
                .get(&winner.workspace_ref)
                .unwrap()
                .tree_path
                .clone(),
            guard
                .workspaces
                .get(&loser.workspace_ref)
                .unwrap()
                .tree_path
                .clone(),
        )
    };
    fs::write(winner_tree.join("lib.rs"), "pub fn x() -> i32 { 42 }\n").unwrap();
    fs::write(loser_tree.join("lib.rs"), "pub fn x() -> i32 { -1 }\n").unwrap();

    let prov = IntentProvenance::new("ws-arena", "rollout-88", "Winning contender implementation")
        .unwrap();
    let flush = mgr
        .flush_all_on_exit(
            None,
            Some((
                &winner.workspace_ref,
                &prov,
                Some("score=0.98 cargo-test=passed"),
            )),
            "exit",
        )
        .unwrap();

    assert_eq!(flush.flushed_workspaces.len(), 1);
    assert_eq!(flush.discarded_workspaces, vec![loser.workspace_ref]);
    assert_eq!(mgr.active_count(), 0);
    assert!(!winner_tree.exists());
    assert!(!loser_tree.exists());

    let evidence_dir = temp
        .path()
        .join("System Store/services/agent-runtime/rollouts/cow-evidence");
    let flushed = &flush.flushed_workspaces[0];
    let patch_text = fs::read_to_string(evidence_dir.join(&flushed.patch_file_name)).unwrap();
    let summary_text = fs::read_to_string(evidence_dir.join(&flushed.summary_file_name)).unwrap();
    assert!(patch_text.contains("+pub fn x() -> i32 { 42 }"));
    assert!(patch_text.contains("Alan-Workstream: ws-arena"));
    assert!(summary_text.contains("score=0.98 cargo-test=passed"));
    assert!(!summary_text.contains(winner_tree.to_string_lossy().as_ref()));
}

#[test]
fn validates_provenance_require_native_cow_and_error_guards() {
    assert!(IntentProvenance::new("ws", "rollout-1", "line1\nline2").is_err());
    assert!(IntentProvenance::new("", "rollout-1", "ok").is_err());
    assert!(WorkspaceRef::parse("../escape").is_err());

    let temp = tempfile::tempdir().unwrap();
    let non_git = temp.path().join("plain");
    fs::create_dir_all(&non_git).unwrap();
    fs::write(non_git.join("f.txt"), "hi\n").unwrap();

    let (mgr, _) = test_manager(temp.path(), "inst-guards", 4);
    let ws = mgr
        .create_workspace(&non_git, CreateWorkspaceOptions::default())
        .unwrap();
    let prov = IntentProvenance::new("ws", "rollout-1", "test").unwrap();
    assert!(matches!(
        mgr.export_patch(&ws.workspace_ref, &prov),
        Err(CowWorkspaceError::NoChangesToExport)
    ));
    assert!(matches!(
        mgr.export_to_git_branch(&ws.workspace_ref, "feat/nope", &prov),
        Err(CowWorkspaceError::NotAGitRepository)
    ));
}
