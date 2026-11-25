use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::process::Command;

use alan_agent_engine::{
    InboxDigestStatus, StreamLeaseGuard, StreamLeaseHolderKind, WorkstreamPersistence,
    WorkstreamRecord, WorkstreamState, bind_project_dir, list_unread_inbox_digests,
    load_workstream_record, upsert_workstream,
};
use alan_os_host::{HostMountAccess, SavedProjectGrantStore};
use chrono::Utc;
use tempfile::TempDir;

use super::draft::{
    CrontabInstaller, WorkflowDraftRequest, draft_workflow_from_natural_language,
    install_drafted_workflow_with_confirmation, merge_crontab_entry,
};
use super::manifest::{
    WorkflowCronStreamSpec, WorkflowManifest, WorkflowStore, WorkflowTrustPosture,
};
use super::schedule::ScheduleTarget;
use super::{WorkflowAction, WorkflowArgs, run_with_store};

struct MockCrontabInstaller {
    existing: RefCell<String>,
    calls: RefCell<usize>,
}

impl MockCrontabInstaller {
    fn new(existing: &str) -> Self {
        Self {
            existing: RefCell::new(existing.to_string()),
            calls: RefCell::new(0),
        }
    }
}

impl CrontabInstaller for MockCrontabInstaller {
    fn install_entry(&self, workflow_id: &str, entry: &str) -> anyhow::Result<String> {
        *self.calls.borrow_mut() += 1;
        let updated = merge_crontab_entry(&self.existing.borrow(), workflow_id, entry);
        *self.existing.borrow_mut() = updated;
        Ok(format!("mock crontab ({workflow_id})"))
    }
}

fn write_mock_script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

#[test]
fn conversational_draft_requires_explicit_confirmation_and_digest_before_scheduler_install() {
    let temp = TempDir::new().unwrap();
    let store = WorkflowStore::from_root(temp.path().join("services/workflows")).unwrap();
    let home = temp.path().join("home");
    let fake_bin = temp.path().join("alan");
    let crontab = MockCrontabInstaller::new("0 1 * * * /usr/bin/backup\n");

    let draft_cron = draft_workflow_from_natural_language(
        &WorkflowDraftRequest {
            description: "Every weekday at 9am check open PRs on stream pr-triage with grant repo-ro max 2 failures budget $0.25".to_string(),
            id: Some("pr-triage-cron".to_string()),
            project_key: Some("proj-demo".to_string()),
            mounts: vec![],
            stream_name: None,
        },
        &store,
        &fake_bin,
        ScheduleTarget::Cron,
    )
    .unwrap();

    assert_eq!(draft_cron.manifest.id, "pr-triage-cron");
    assert_eq!(draft_cron.manifest.schedule, "0 9 * * 1-5");
    assert_eq!(draft_cron.manifest.mounts, vec!["repo-ro".to_string()]);
    assert_eq!(draft_cron.manifest.trust, WorkflowTrustPosture::Governed);
    let stream = draft_cron.manifest.cron_stream.as_ref().unwrap();
    assert_eq!(stream.stream_name, "pr-triage");
    assert_eq!(stream.max_consecutive_failures, 2);
    assert_eq!(stream.per_run_budget_usd, Some(0.25));

    // Refuses install when confirmed = false.
    let err = install_drafted_workflow_with_confirmation(
        &draft_cron,
        false,
        Some(&draft_cron.confirmation_digest),
        &store,
        Some(&home),
        &crontab,
        false,
    )
    .unwrap_err();
    assert!(err.to_string().contains("confirmation gate refused"));
    assert_eq!(*crontab.calls.borrow(), 0);
    assert!(!store.manifest_path("pr-triage-cron").unwrap().exists());

    // Refuses install when digest mismatches.
    let mismatch = install_drafted_workflow_with_confirmation(
        &draft_cron,
        true,
        Some("deadbeef"),
        &store,
        Some(&home),
        &crontab,
        false,
    )
    .unwrap_err();
    assert!(mismatch.to_string().contains("digest mismatch"));
    assert_eq!(*crontab.calls.borrow(), 0);

    // Explicit confirmation + matching digest installs crontab cleanly preserving existing jobs.
    let installed_cron = install_drafted_workflow_with_confirmation(
        &draft_cron,
        true,
        Some(&draft_cron.confirmation_digest),
        &store,
        Some(&home),
        &crontab,
        false,
    )
    .unwrap();
    assert_eq!(installed_cron.scheduler_target, ScheduleTarget::Cron);
    assert_eq!(*crontab.calls.borrow(), 1);
    assert!(store.manifest_path("pr-triage-cron").unwrap().exists());
    let merged = crontab.existing.borrow().clone();
    assert!(merged.contains("/usr/bin/backup"));
    assert!(merged.contains("# alan workflow: pr-triage-cron"));

    // Launchd confirmation gate also refuses until confirmed, then installs LaunchAgents plist.
    let draft_launchd = draft_workflow_from_natural_language(
        &WorkflowDraftRequest {
            description: "Every 30m summarize release readiness".to_string(),
            id: Some("release-summary".to_string()),
            project_key: None,
            mounts: vec![],
            stream_name: None,
        },
        &store,
        &fake_bin,
        ScheduleTarget::Launchd,
    )
    .unwrap();
    assert!(
        install_drafted_workflow_with_confirmation(
            &draft_launchd,
            false,
            None,
            &store,
            Some(&home),
            &crontab,
            false,
        )
        .is_err()
    );
    let plist_path = home.join("Library/LaunchAgents/app.alanworks.workflow.release-summary.plist");
    assert!(!plist_path.exists());

    let installed_launchd = install_drafted_workflow_with_confirmation(
        &draft_launchd,
        true,
        Some(&draft_launchd.confirmation_digest),
        &store,
        Some(&home),
        &crontab,
        false,
    )
    .unwrap();
    assert_eq!(
        installed_launchd.installed_location,
        plist_path.display().to_string()
    );
    assert!(plist_path.is_file());
}

#[tokio::test]
async fn stateful_cron_runner_guards_contention_compaction_budget_grants_and_paused_error() {
    let temp = TempDir::new().unwrap();
    let services_root = temp.path().join("services");
    let projects_root = services_root.join("projects");
    let grants_path = services_root.join("host-mount/grants.json");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&repo_dir)
        .output()
        .unwrap();

    let binding = bind_project_dir(&projects_root, &repo_dir, Some("repo-rw".to_string())).unwrap();
    let grant_store = SavedProjectGrantStore::from_path(&grants_path);
    grant_store
        .save_grant(
            "repo-rw",
            &repo_dir,
            HostMountAccess::ReadWrite,
            None,
            vec![],
        )
        .unwrap();

    let store = WorkflowStore::from_root(services_root.join("workflows"))
        .unwrap()
        .with_host_mount_grants_path(&grants_path);

    // Seed persistent stream with an existing rollout checkpoint large enough to trigger compaction.
    let ckpt_path = temp.path().join("cron-ckpt.jsonl");
    let meta = serde_json::json!({
        "type": "agent_machine_meta",
        "rollout_id": "cron-ckpt",
        "process_path": "/proc/1",
        "cwd": repo_dir.display().to_string(),
        "model": "mock",
        "started_at": Utc::now().to_rfc3339()
    });
    let mut lines = vec![serde_json::to_string(&meta).unwrap()];
    for idx in 0..18 {
        let text =
            format!("Cron message {idx} checking test diagnostics and regression telemetry.");
        let msg = serde_json::json!({
            "type": "message",
            "role": if idx % 2 == 0 { "user" } else { "assistant" },
            "content": text,
            "tool_name": serde_json::Value::Null,
            "message": if idx % 2 == 0 {
                alan_agent_engine::tape::Message::user(&text)
            } else {
                alan_agent_engine::tape::Message::assistant(&text)
            },
            "timestamp": Utc::now().to_rfc3339()
        });
        lines.push(serde_json::to_string(&msg).unwrap());
    }
    fs::write(&ckpt_path, format!("{}\n", lines.join("\n"))).unwrap();

    let mut stream_rec = WorkstreamRecord::new(
        "nightly-stream",
        "Nightly CI triage",
        WorkstreamPersistence::Persistent,
    );
    stream_rec.state = WorkstreamState::Idle;
    stream_rec.last_checkpoint_ref = Some(ckpt_path.display().to_string());
    upsert_workstream(&projects_root, &binding.project_key, stream_rec).unwrap();

    let ok_bin = write_mock_script(
        temp.path(),
        "cron-ok.sh",
        r#"#!/bin/sh
printf '{"type":"usage","input_tokens":100,"output_tokens":20,"context_window":128000,"cost_usd":0.05}\n'
printf '{"type":"turn_finished","status":"completed","answer":"Cron stream passed cleanly","rollout_id":"cron-rollout-2"}\n'
"#,
    );

    let manifest = WorkflowManifest {
        schema_version: 1,
        id: "nightly-ci".to_string(),
        schedule: "every 1h".to_string(),
        prompt: "Check CI telemetry".to_string(),
        model_profile: None,
        project_key: Some(binding.project_key.clone()),
        mounts: vec!["repo-rw".to_string()],
        packages: vec![],
        fileservers: vec![],
        trust: WorkflowTrustPosture::Governed,
        json: true,
        timeout_secs: Some(30),
        trigger_allowlist: None,
        release_gate: None,
        cron_stream: Some(WorkflowCronStreamSpec {
            stream_name: "nightly-stream".to_string(),
            scope: Some("Nightly CI triage".to_string()),
            max_consecutive_failures: 2,
            compaction_threshold_tokens: Some(120),
            per_run_budget_usd: Some(0.20),
        }),
    };
    store.save_manifest(&manifest, false).unwrap();

    // 1. Lock contention when interactive session holds `streams/nightly-stream.lock`.
    let interactive_lease = match StreamLeaseGuard::try_acquire(
        &projects_root,
        &binding.project_key,
        "nightly-stream",
        StreamLeaseHolderKind::Interactive,
        Some("interactive-session"),
    )
    .unwrap()
    {
        alan_agent_engine::StreamLeaseAcquireResult::Acquired(guard) => guard,
        other => panic!("expected interactive lease acquisition, got {other:?}"),
    };

    let (_out, err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "nightly-ci".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&ok_bin),
    )
    .await
    .unwrap();
    assert!(err.is_none());
    let ckpt_during_lock = fs::read_to_string(&ckpt_path).unwrap();
    assert!(
        !ckpt_during_lock.contains("\"compacted\""),
        "contention deferral must leave checkpoint untouched"
    );
    let inbox = list_unread_inbox_digests(&projects_root, &binding.project_key).unwrap();
    assert!(
        inbox
            .iter()
            .any(|d| d.stream_name.as_deref() == Some("nightly-stream")
                && d.summary.contains("Deferred"))
    );
    drop(interactive_lease);

    // 2. Released lock -> compacts checkpoint over threshold, succeeds under budget, updates rollout ref.
    let (_out, err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "nightly-ci".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&ok_bin),
    )
    .await
    .unwrap();
    assert!(err.is_none());
    let ckpt_after = fs::read_to_string(&ckpt_path).unwrap();
    assert!(
        ckpt_after.contains("\"compacted\""),
        "expected automatic compaction line in checkpoint"
    );
    let rec_after_ok =
        load_workstream_record(&projects_root, &binding.project_key, "nightly-stream")
            .unwrap()
            .unwrap();
    assert_eq!(rec_after_ok.rollout_ref.as_deref(), Some("cron-rollout-2"));

    // 3. Over-budget run fails strictly (failure 1/2).
    let pricey_bin = write_mock_script(
        temp.path(),
        "cron-pricey.sh",
        r#"#!/bin/sh
printf '{"type":"usage","input_tokens":500,"output_tokens":200,"context_window":128000,"cost_usd":0.99}\n'
sleep 5
"#,
    );
    let (_out, budget_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "nightly-ci".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&pricey_bin),
    )
    .await
    .unwrap();
    assert!(budget_err.is_some());
    let rec_after_budget =
        load_workstream_record(&projects_root, &binding.project_key, "nightly-stream")
            .unwrap()
            .unwrap();
    assert_eq!(rec_after_budget.state, WorkstreamState::Idle);
    assert_eq!(
        rec_after_budget
            .cron_binding
            .as_ref()
            .unwrap()
            .consecutive_failures,
        1
    );

    // 4. Revoked saved grant fails live revalidation (failure 2/2 -> paused_error + inbox alert).
    grant_store.revoke_grant("repo-rw").unwrap();
    let (_out, revoke_err) = run_with_store(
        &store,
        WorkflowArgs {
            action: WorkflowAction::Run {
                id: "nightly-ci".to_string(),
                dry_run: false,
                json: true,
            },
        },
        None,
        Some(&ok_bin),
    )
    .await
    .unwrap();
    assert!(revoke_err.is_some());

    let rec_paused = load_workstream_record(&projects_root, &binding.project_key, "nightly-stream")
        .unwrap()
        .unwrap();
    assert_eq!(rec_paused.state, WorkstreamState::PausedError);
    let unread = list_unread_inbox_digests(&projects_root, &binding.project_key).unwrap();
    assert!(
        unread
            .iter()
            .any(|d| d.status == InboxDigestStatus::PausedError
                && d.stream_name.as_deref() == Some("nightly-stream"))
    );
}
