use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alan_agent_protocol::{SpawnHandle, SpawnHostMount, SpawnMountAccess};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_kernel::{Access, MountFs, Namespace};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::orchestrator::ArenaSpawnFn;
use super::specs::VerifierCandidateSummary;
use super::*;
use crate::tools::SandboxBackendKind;

#[derive(Default)]
struct MockArenaWorkspacePort {
    created: Mutex<Vec<ArenaWorkspaceCreateRequest>>,
    discarded: Mutex<Vec<String>>,
    flushed: Mutex<Vec<ArenaWinnerFlushRequest>>,
    inject_rw_verifier_mount: AtomicBool,
    inject_parent_grant_for_contender: AtomicBool,
    inject_host_path_in_diff: AtomicBool,
}

#[async_trait]
impl ArenaWorkspacePort for MockArenaWorkspacePort {
    async fn create_contender_workspace(
        &self,
        request: &ArenaWorkspaceCreateRequest,
    ) -> Result<ArenaContenderWorkspace, String> {
        self.created.lock().await.push(request.clone());
        let ws_ref = format!("ws-{}", request.candidate_id);
        let grant = if self
            .inject_parent_grant_for_contender
            .load(Ordering::Acquire)
        {
            request.source_grant.clone()
        } else {
            format!("clone-grant-{}", request.candidate_id)
        };
        let verifier_access = if self.inject_rw_verifier_mount.load(Ordering::Acquire) {
            SpawnMountAccess::ReadWrite
        } else {
            SpawnMountAccess::ReadOnly
        };
        Ok(ArenaContenderWorkspace {
            workspace_ref: ws_ref,
            backing_kind: ArenaWorkspaceBackingKind::NativeCow,
            contender_mount: SpawnHostMount {
                grant,
                target: PathBuf::from(DEFAULT_ARENA_CONTENDER_MOUNT_TARGET),
                access: SpawnMountAccess::ReadWrite,
            },
            verifier_read_only_mount: Some(SpawnHostMount {
                grant: format!("ro-grant-{}", request.candidate_id),
                target: PathBuf::from(format!("/mnt/candidates/{}", request.candidate_id)),
                access: verifier_access,
            }),
        })
    }

    async fn inspect_workspace(
        &self,
        workspace_ref: &str,
    ) -> Result<ArenaWorkspaceInspection, String> {
        if self.inject_host_path_in_diff.load(Ordering::Acquire) {
            return Ok(ArenaWorkspaceInspection {
                diff_summary: "modified /Users/alice/System Store/workspaces/1/ws/src/lib.rs"
                    .to_string(),
                checks_summary: Some("ok".to_string()),
                has_changes: true,
            });
        }
        Ok(ArenaWorkspaceInspection {
            diff_summary: format!("1 file changed (+12/-2) in {workspace_ref}: src/lib.rs"),
            checks_summary: Some(format!("cargo test ({workspace_ref}): 4 passed, 0 failed")),
            has_changes: true,
        })
    }

    async fn discard_workspace(&self, workspace_ref: &str) -> Result<(), String> {
        self.discarded.lock().await.push(workspace_ref.to_string());
        Ok(())
    }

    async fn flush_winner_patch_on_exit(
        &self,
        request: &ArenaWinnerFlushRequest,
    ) -> Result<ArenaWinnerFlushOutcome, String> {
        self.flushed.lock().await.push(request.clone());
        Ok(ArenaWinnerFlushOutcome {
            patch_artifact_ref: Some(format!("arena-winner-{}.patch", request.candidate_id)),
            scorecard_artifact_ref: Some(format!("arena-scorecard-{}.json", request.candidate_id)),
            clone_purged: true,
        })
    }
}

/// Minimal in-process `/mnt/branch` `FileServer` validating the `aP` wire contract.
#[derive(Default)]
struct InProcessBranchServer {
    state: Mutex<BranchServerState>,
}

#[derive(Default)]
struct BranchServerState {
    branches: BTreeMap<String, Value>,
    selected: Option<String>,
    events: Vec<Value>,
    fids: HashMap<Fid, (String, Option<OpenMode>, Vec<u8>, bool)>,
}

impl InProcessBranchServer {
    fn new_with_base(base_id: &str) -> Arc<Self> {
        let server = Arc::new(Self::default());
        let mut state = server.state.try_lock().expect("new server");
        state.branches.insert(
            base_id.to_string(),
            json!({
                "version": 1,
                "id": base_id,
                "root": "base-root-hash",
                "status": "active",
            }),
        );
        drop(state);
        server
    }

    async fn recorded_events(&self) -> Vec<Value> {
        self.state.lock().await.events.clone()
    }

    async fn branch_exists(&self, id: &str) -> bool {
        self.state.lock().await.branches.contains_key(id)
    }
}

#[async_trait]
impl FileServer for InProcessBranchServer {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let base = if fid == Fid::ROOT {
            String::new()
        } else {
            state
                .fids
                .get(&fid)
                .map(|entry| entry.0.clone())
                .ok_or(ErrorCode::NotFound)?
        };
        let mut path = base;
        for name in names {
            if path.is_empty() {
                path = name.clone();
            } else {
                path = format!("{path}/{name}");
            }
        }
        let kind = match path.as_str() {
            "" | "branches" => FileKind::Dir,
            "ctl" | "selected" => FileKind::File,
            "events" => FileKind::Stream,
            other if other.starts_with("branches/") => {
                let id = &other["branches/".len()..];
                if !state.branches.contains_key(id) {
                    return Err(ErrorCode::NotFound);
                }
                FileKind::File
            }
            _ => return Err(ErrorCode::NotFound),
        };
        state.fids.insert(newfid, (path, None, Vec::new(), false));
        Ok(Qid {
            kind,
            version: 0,
            path: newfid.0,
        })
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> Result<Qid, ErrorCode> {
        let mut state = self.state.lock().await;
        let entry = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        entry.1 = Some(mode);
        Ok(Qid {
            kind: FileKind::File,
            version: 0,
            path: fid.0,
        })
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let state = self.state.lock().await;
        let path = &state.fids.get(&fid).ok_or(ErrorCode::NotFound)?.0;
        let bytes = match path.as_str() {
            "selected" => match state.selected.as_deref() {
                Some(id) => {
                    let branch = state.branches.get(id).ok_or(ErrorCode::NotFound)?;
                    serde_json::to_vec(&json!({
                        "id": id,
                        "root": branch["root"],
                        "score": branch["score"],
                        "workspace_ref": branch["workspace_ref"],
                        "backing_kind": branch["backing_kind"],
                    }))
                    .unwrap()
                }
                None => b"null\n".to_vec(),
            },
            "events" => {
                let mut out = Vec::new();
                for event in &state.events {
                    out.extend_from_slice(&serde_json::to_vec(event).unwrap());
                    out.push(b'\n');
                }
                out
            }
            other if other.starts_with("branches/") => {
                let id = &other["branches/".len()..];
                let branch = state.branches.get(id).ok_or(ErrorCode::NotFound)?;
                serde_json::to_vec(branch).unwrap()
            }
            _ => Vec::new(),
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, fid: Fid, offset: Offset, data: &[u8]) -> Result<u32, ErrorCode> {
        let mut state = self.state.lock().await;
        let entry = state.fids.get_mut(&fid).ok_or(ErrorCode::NotFound)?;
        let start = offset as usize;
        let end = start + data.len();
        if entry.2.len() < end {
            entry.2.resize(end, 0);
        }
        entry.2[start..end].copy_from_slice(data);
        entry.3 = true;
        Ok(data.len() as u32)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let state = self.state.lock().await;
        let path = &state.fids.get(&fid).ok_or(ErrorCode::NotFound)?.0;
        let length = match path.as_str() {
            "events" => state
                .events
                .iter()
                .map(|e| serde_json::to_vec(e).unwrap().len() as u64 + 1)
                .sum(),
            "selected" => 256,
            other if other.starts_with("branches/") => {
                let id = &other["branches/".len()..];
                serde_json::to_vec(state.branches.get(id).ok_or(ErrorCode::NotFound)?)
                    .unwrap()
                    .len() as u64
            }
            _ => 0,
        };
        Ok(Stat {
            name: String::new(),
            qid: Qid {
                kind: if path == "events" {
                    FileKind::Stream
                } else {
                    FileKind::File
                },
                version: 0,
                path: fid.0,
            },
            length,
            executable: false,
            writable: path == "ctl",
        })
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

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        let mut state = self.state.lock().await;
        let Some((path, _, buf, wrote)) = state.fids.remove(&fid) else {
            return Err(ErrorCode::NotFound);
        };
        if !wrote || path != "ctl" {
            return Ok(());
        }
        let cmd: Value = serde_json::from_slice(&buf).map_err(|_| ErrorCode::BadRequest)?;
        match cmd["op"].as_str() {
            Some("fork") => {
                let id = cmd["id"].as_str().ok_or(ErrorCode::BadRequest)?.to_string();
                let record = json!({
                    "version": 1,
                    "id": id,
                    "base": cmd["from"],
                    "root": format!("root-{id}"),
                    "status": "active",
                    "workspace_ref": cmd["workspace_ref"],
                    "backing_kind": cmd["backing_kind"],
                    "diff_summary": cmd["diff_summary"],
                    "checks_summary": cmd["checks_summary"],
                });
                state.branches.insert(id, record);
                state.events.push(json!({"type": "fork", "id": cmd["id"]}));
            }
            Some("update_workspace") => {
                let id = cmd["id"].as_str().ok_or(ErrorCode::BadRequest)?;
                let branch = state.branches.get_mut(id).ok_or(ErrorCode::NotFound)?;
                branch["workspace_ref"] = cmd["workspace_ref"].clone();
                branch["backing_kind"] = cmd["backing_kind"].clone();
                branch["diff_summary"] = cmd["diff_summary"].clone();
                branch["checks_summary"] = cmd["checks_summary"].clone();
                state
                    .events
                    .push(json!({"type": "workspace_updated", "id": id}));
            }
            Some("score") => {
                let id = cmd["id"].as_str().ok_or(ErrorCode::BadRequest)?;
                let branch = state.branches.get_mut(id).ok_or(ErrorCode::NotFound)?;
                branch["score"] = cmd["score"].clone();
                branch["summary"] = cmd["summary"].clone();
                state.events.push(json!({
                    "type": "score",
                    "id": id,
                    "score": cmd["score"],
                }));
            }
            Some("select") => {
                let id = cmd["id"].as_str().ok_or(ErrorCode::BadRequest)?.to_string();
                let branch = state.branches.get_mut(&id).ok_or(ErrorCode::NotFound)?;
                branch["status"] = json!("selected");
                state.selected = Some(id.clone());
                state.events.push(json!({"type": "select", "id": id}));
            }
            Some("discard") => {
                let id = cmd["id"].as_str().ok_or(ErrorCode::BadRequest)?;
                state.branches.remove(id).ok_or(ErrorCode::NotFound)?;
                if state.selected.as_deref() == Some(id) {
                    state.selected = None;
                }
                state.events.push(json!({"type": "discard", "id": id}));
            }
            _ => return Err(ErrorCode::BadRequest),
        }
        Ok(())
    }
}

fn sample_request(mode: ArenaMode) -> ExecutableArenaRequest {
    ExecutableArenaRequest {
        mode,
        problem_statement: "Fix off-by-one parsing bug in token stream".to_string(),
        acceptance_criteria: "Correctness, minimal diff, all unit tests pass".to_string(),
        source_grant: (mode == ArenaMode::Executable).then(|| "grant-project".to_string()),
        base_branch_id: "base".to_string(),
        workstream: Some("fix-parser".to_string()),
        rollout_ref: Some("rollout-42".to_string()),
        why: Some("Compare two parser fixes with verified test runs".to_string()),
        network_denied: true,
        max_contenders: DEFAULT_MAX_EXECUTABLE_ARENA_CONTENDERS,
        contenders: vec![
            ArenaContenderRequest {
                candidate_id: "cand-alpha".to_string(),
                role: "arena-contender-1".to_string(),
                model: Some("model-a".to_string()),
                task: "Implement fix approach A".to_string(),
                timeout_secs: Some(300),
            },
            ArenaContenderRequest {
                candidate_id: "cand-beta".to_string(),
                role: "arena-contender-2".to_string(),
                model: Some("model-b".to_string()),
                task: "Implement fix approach B".to_string(),
                timeout_secs: Some(300),
            },
        ],
        verifier: ArenaVerifierRequest {
            role: "arena-verifier".to_string(),
            model: Some("model-v".to_string()),
            timeout_secs: Some(180),
        },
        read_only_mounts: if mode == ArenaMode::ReadOnly {
            vec![SpawnHostMount {
                grant: "grant-project".to_string(),
                target: PathBuf::from("/mnt/project"),
                access: SpawnMountAccess::ReadOnly,
            }]
        } else {
            Vec::new()
        },
    }
}

#[test]
fn arena_mode_defaults_to_read_only_and_parses_explicit_modes() {
    assert_eq!(ArenaMode::default(), ArenaMode::ReadOnly);
    assert_eq!(
        ArenaMode::from_optional_str(None).unwrap(),
        ArenaMode::ReadOnly
    );
    assert_eq!(
        ArenaMode::from_optional_str(Some("read_only")).unwrap(),
        ArenaMode::ReadOnly
    );
    assert_eq!(
        ArenaMode::from_optional_str(Some("executable")).unwrap(),
        ArenaMode::Executable
    );
    assert!(ArenaMode::from_optional_str(Some("unsafe")).is_err());
}

#[test]
fn sandbox_gate_allows_os_enforced_backends_and_rejects_host_mount_path_guard() {
    assert!(ensure_executable_arena_sandbox(SandboxBackendKind::Seatbelt).is_ok());
    assert!(ensure_executable_arena_sandbox(SandboxBackendKind::LinuxReifiedNamespace).is_ok());
    assert!(ensure_executable_arena_sandbox(SandboxBackendKind::Landlock).is_ok());

    let err = ensure_executable_arena_sandbox(SandboxBackendKind::HostMountPathGuard).unwrap_err();
    assert!(matches!(
        err,
        ExecutableArenaError::UnsafeSandboxBackend {
            backend: "host_mount_path_guard"
        }
    ));
    let msg = err.to_string();
    assert!(msg.contains("read_only"), "{msg}");
}

#[tokio::test]
async fn executable_mode_fails_closed_under_host_mount_path_guard_before_cow_creation() {
    let port = Arc::new(MockArenaWorkspacePort::default());
    let coordinator = ExecutableArenaCoordinator::new(ArenaExitFlushRegistry::new())
        .with_sandbox_backend(SandboxBackendKind::HostMountPathGuard)
        .with_workspace_port(port.clone());

    let spawned = Arc::new(AtomicBool::new(false));
    let spawned_flag = spawned.clone();
    let spawn_fn: ArenaSpawnFn = Arc::new(move |_spec, _cancel| {
        spawned_flag.store(true, Ordering::Release);
        Box::pin(async { Ok(ArenaSubagentOutput::completed("unexpected")) })
    });

    let err = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::Executable),
            spawn_fn,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ExecutableArenaError::UnsafeSandboxBackend {
            backend: "host_mount_path_guard"
        }
    ));
    assert!(
        port.created.lock().await.is_empty(),
        "must not create any CoW workspace under HostMountPathGuard"
    );
    assert!(
        !spawned.load(Ordering::Acquire),
        "must not spawn any contender under HostMountPathGuard"
    );
}

#[tokio::test]
async fn read_only_default_mode_runs_without_cow_workspaces_even_under_host_mount_path_guard() {
    let port = Arc::new(MockArenaWorkspacePort::default());
    let coordinator = ExecutableArenaCoordinator::new(ArenaExitFlushRegistry::new())
        .with_sandbox_backend(SandboxBackendKind::HostMountPathGuard)
        .with_workspace_port(port.clone());

    let seen_specs = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = seen_specs.clone();
    let spawn_fn: ArenaSpawnFn = Arc::new(move |spec, _cancel| {
        let seen = seen_clone.clone();
        Box::pin(async move {
            let role = match &spec.target {
                alan_agent_protocol::SpawnTarget::AdHocSubagent { role } => role.clone(),
                _ => String::new(),
            };
            seen.lock().await.push((role.clone(), spec));
            if role == "arena-verifier" {
                Ok(ArenaSubagentOutput::completed(
                    r#"{"scores":[{"candidate":"Contender A","score":0.9,"summary":"clean"},{"candidate":"Contender B","score":0.7,"summary":"wordy"}],"winner":"Contender A","rationale":"A is simpler"}"#,
                ))
            } else {
                Ok(ArenaSubagentOutput::completed("proposal output"))
            }
        })
    });

    let res = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::ReadOnly),
            spawn_fn,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(res.mode, ArenaMode::ReadOnly);
    assert_eq!(res.selected_candidate_id, "cand-alpha");
    assert!(res.selected_workspace_ref.is_none());
    assert!(port.created.lock().await.is_empty());

    let specs = seen_specs.lock().await;
    assert_eq!(specs.len(), 3);
    for (role, spec) in specs.iter() {
        assert!(
            spec.runtime_overrides.read_only,
            "role `{role}` must run read_only=true in default mode"
        );
        assert!(
            spec.host_mounts
                .iter()
                .all(|m| m.access == SpawnMountAccess::ReadOnly)
        );
    }
}

#[test]
fn verifier_spawn_spec_enforces_read_only_and_rejects_read_write_mounts() {
    let request = sample_request(ArenaMode::Executable);
    let rw_mount = SpawnHostMount {
        grant: "clone-grant-cand-alpha".to_string(),
        target: PathBuf::from("/mnt/candidates/cand-alpha"),
        access: SpawnMountAccess::ReadWrite,
    };
    let err = build_verifier_spawn_spec(&request, &[rw_mount], &[]).unwrap_err();
    assert!(matches!(err, ExecutableArenaError::VerifierNotReadOnly(_)));

    let ro_mount = SpawnHostMount {
        grant: "ro-grant-cand-alpha".to_string(),
        target: PathBuf::from("/mnt/candidates/cand-alpha"),
        access: SpawnMountAccess::ReadOnly,
    };
    let spec = build_verifier_spawn_spec(
        &request,
        &[ro_mount],
        &[VerifierCandidateSummary {
            neutral_label: "Contender A",
            branch_id: "cand-alpha",
            workspace_ref: Some("ws-cand-alpha"),
            backing_kind: Some(ArenaWorkspaceBackingKind::NativeCow),
            diff_summary: Some("1 file changed"),
            checks_summary: Some("passed"),
            contender_output: "fixed bug",
        }],
    )
    .unwrap();
    assert!(spec.runtime_overrides.read_only);
    assert_eq!(spec.handles, vec![SpawnHandle::ApprovalScope]);
}

#[tokio::test]
async fn executable_mode_isolates_contenders_coordinates_branchfs_purges_losers_and_flushes_on_exit()
 {
    let port = Arc::new(MockArenaWorkspacePort::default());
    let branch_server = InProcessBranchServer::new_with_base("base");
    let mut ns = Namespace::new();
    ns.mount(
        BRANCH_MOUNT_PATH,
        InProcessTransport::new(branch_server.clone()),
        Access::ReadWrite,
    );
    let root_transport = InProcessTransport::new(Arc::new(MountFs::new(ns)));
    let branch_client = BranchFsApClient::at_namespace_mount(root_transport);

    let exit_registry = ArenaExitFlushRegistry::new();
    let coordinator = ExecutableArenaCoordinator::new(exit_registry.clone())
        .with_sandbox_backend(SandboxBackendKind::Seatbelt)
        .with_workspace_port(port.clone())
        .with_branch_client(branch_client.clone());

    let seen_specs = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = seen_specs.clone();
    let spawn_fn: ArenaSpawnFn = Arc::new(move |spec, _cancel| {
        let seen = seen_clone.clone();
        Box::pin(async move {
            let role = match &spec.target {
                alan_agent_protocol::SpawnTarget::AdHocSubagent { role } => role.clone(),
                _ => String::new(),
            };
            seen.lock().await.push((role.clone(), spec));
            if role == "arena-verifier" {
                Ok(ArenaSubagentOutput::completed(
                    r#"{"scores":[{"candidate":"Contender A","score":0.95,"summary":"tests green"},{"candidate":"Contender B","score":0.55,"summary":"edge case missed"}],"winner":"Contender B","rationale":"WAIT - actually testing B win"}"#,
                )
                .with_structured_output(json!({
                    "scores": [
                        {"candidate": "Contender A", "score": 0.95, "summary": "tests green"},
                        {"candidate": "Contender B", "score": 0.55, "summary": "edge case missed"}
                    ],
                    "winner": "Contender A",
                    "rationale": "Contender A passed all tests cleanly"
                })))
            } else {
                Ok(ArenaSubagentOutput::completed(format!(
                    "Implemented fix in {role}"
                )))
            }
        })
    });

    let result = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::Executable),
            spawn_fn,
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(result.mode, ArenaMode::Executable);
    assert_eq!(result.selected_candidate_id, "cand-alpha");
    assert_eq!(
        result.selected_workspace_ref.as_deref(),
        Some("ws-cand-alpha")
    );
    assert_eq!(result.purged_losers, vec!["cand-beta".to_string()]);

    // Verify immediate losing-clone purge happened on verdict.
    assert_eq!(
        *port.discarded.lock().await,
        vec!["ws-cand-beta".to_string()]
    );
    assert!(branch_server.branch_exists("cand-alpha").await);
    assert!(!branch_server.branch_exists("cand-beta").await);

    // Verify /mnt/branch/selected & events recorded opaque metadata with zero Host paths.
    let selected = branch_client.read_selected().await.unwrap();
    assert_eq!(selected["id"], "cand-alpha");
    assert_eq!(selected["workspace_ref"], "ws-cand-alpha");
    let events = branch_server.recorded_events().await;
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "select" && e["id"] == "cand-alpha")
    );
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "discard" && e["id"] == "cand-beta")
    );

    // Verify contender vs verifier isolation invariants on SpawnSpecs.
    let specs = seen_specs.lock().await;
    assert_eq!(specs.len(), 3);
    for (role, spec) in specs.iter() {
        assert_eq!(spec.handles, vec![SpawnHandle::ApprovalScope]);
        if role == "arena-verifier" {
            assert!(spec.runtime_overrides.read_only);
            assert!(
                spec.host_mounts
                    .iter()
                    .all(|m| m.access == SpawnMountAccess::ReadOnly)
            );
            assert_eq!(spec.host_mounts.len(), 2);
        } else {
            assert!(!spec.runtime_overrides.read_only);
            assert_eq!(spec.host_mounts.len(), 1);
            assert_eq!(spec.host_mounts[0].access, SpawnMountAccess::ReadWrite);
            assert_ne!(spec.host_mounts[0].grant, "grant-project");
            assert_eq!(
                spec.host_mounts[0].target,
                PathBuf::from(DEFAULT_ARENA_CONTENDER_MOUNT_TARGET)
            );
        }
    }

    // Simulate normal `alan` exit before syncing winner back to Host.
    assert_eq!(exit_registry.pending_count(), 1);
    let reports = exit_registry.flush_all_on_exit().await.unwrap();
    assert_eq!(reports.len(), 1);
    assert!(!reports[0].was_applied);
    assert_eq!(reports[0].candidate_id, "cand-alpha");
    assert_eq!(
        reports[0].patch_artifact_ref.as_deref(),
        Some("arena-winner-cand-alpha.patch")
    );
    assert!(reports[0].clone_purged);

    let flushed = port.flushed.lock().await;
    assert_eq!(flushed.len(), 1);
    assert_eq!(flushed[0].workstream.as_deref(), Some("fix-parser"));
    assert_eq!(flushed[0].rollout_ref.as_deref(), Some("rollout-42"));
    assert_eq!(
        flushed[0].why.as_deref(),
        Some("Compare two parser fixes with verified test runs")
    );
}

#[tokio::test]
async fn executable_mode_rejects_parent_grant_or_rw_verifier_or_raw_host_paths_and_cleans_up() {
    // 1. Port returning parent source_grant directly to contender is rejected & cleaned up.
    let port = Arc::new(MockArenaWorkspacePort::default());
    port.inject_parent_grant_for_contender
        .store(true, Ordering::Release);
    let coordinator = ExecutableArenaCoordinator::new(ArenaExitFlushRegistry::new())
        .with_sandbox_backend(SandboxBackendKind::Landlock)
        .with_workspace_port(port.clone());
    let noop_spawn: ArenaSpawnFn =
        Arc::new(|_, _| Box::pin(async { Ok(ArenaSubagentOutput::completed("ok")) }));
    let err = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::Executable),
            noop_spawn.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ExecutableArenaError::ContenderMountViolation(_)
    ));
    assert_eq!(port.discarded.lock().await.len(), 1);

    // 2. Port returning ReadWrite verifier mount is rejected & cleaned up.
    let port = Arc::new(MockArenaWorkspacePort::default());
    port.inject_rw_verifier_mount.store(true, Ordering::Release);
    let coordinator = ExecutableArenaCoordinator::new(ArenaExitFlushRegistry::new())
        .with_sandbox_backend(SandboxBackendKind::LinuxReifiedNamespace)
        .with_workspace_port(port.clone());
    let err = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::Executable),
            noop_spawn.clone(),
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutableArenaError::VerifierNotReadOnly(_)));
    assert_eq!(port.discarded.lock().await.len(), 1);

    // 3. Raw Host path leaked in diff_summary is rejected & purges all clones.
    let port = Arc::new(MockArenaWorkspacePort::default());
    port.inject_host_path_in_diff.store(true, Ordering::Release);
    let coordinator = ExecutableArenaCoordinator::new(ArenaExitFlushRegistry::new())
        .with_sandbox_backend(SandboxBackendKind::Seatbelt)
        .with_workspace_port(port.clone());
    let err = coordinator
        .run_bakeoff(
            &sample_request(ArenaMode::Executable),
            noop_spawn,
            &CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ExecutableArenaError::RawHostPathRejected(_)));
    assert_eq!(port.discarded.lock().await.len(), 2);
}

#[tokio::test]
async fn exit_registry_skips_unapplied_patch_when_winner_already_applied_but_purges_clone() {
    let port = Arc::new(MockArenaWorkspacePort::default());
    let registry = ArenaExitFlushRegistry::new();
    registry.register_winner(
        port.clone(),
        ArenaWinnerFlushRequest {
            candidate_id: "cand-alpha".to_string(),
            workspace_ref: "ws-cand-alpha".to_string(),
            backing_kind: ArenaWorkspaceBackingKind::GitWorktree,
            workstream: Some("ws".to_string()),
            rollout_ref: Some("r1".to_string()),
            why: Some("why".to_string()),
            score: Some(0.9),
            verifier_summary: None,
            diff_summary: None,
            checks_summary: None,
        },
    );
    assert!(registry.mark_winner_applied("ws-cand-alpha"));
    let reports = registry.flush_all_on_exit().await.unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].was_applied);
    assert!(reports[0].clone_purged);
    assert!(reports[0].patch_artifact_ref.is_none());
    assert!(port.flushed.lock().await.is_empty());
    assert_eq!(
        *port.discarded.lock().await,
        vec!["ws-cand-alpha".to_string()]
    );
}
