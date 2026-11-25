use std::fs;

use tempfile::TempDir;

use super::*;
use crate::rollout::AgentMachineMeta;
use crate::runtime::project_workstreams::{
    bind_project_dir, list_unread_inbox_digests, load_workstream_record,
};

fn test_spec(project_key: &str) -> CronStreamExecutionSpec {
    CronStreamExecutionSpec {
        workflow_id: "nightly-triage".to_string(),
        project_key: project_key.to_string(),
        stream_name: "triage-stream".to_string(),
        scope: Some("Nightly repo triage".to_string()),
        max_consecutive_failures: 2,
        compaction_threshold_tokens: Some(150),
        per_run_budget_usd: Some(0.25),
    }
}

#[tokio::test]
async fn cron_stream_defers_cleanly_on_interactive_lease_lock_contention_and_logs_inbox() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let binding = bind_project_dir(&projects_root, &repo_dir, Some("repo-rw".to_string())).unwrap();

    let mut existing = WorkstreamRecord::new(
        "triage-stream",
        "Nightly repo triage",
        WorkstreamPersistence::Persistent,
    );
    existing.state = WorkstreamState::Idle;
    existing.last_checkpoint_ref = Some("/tmp/original-ckpt.jsonl".to_string());
    upsert_workstream(&projects_root, &binding.project_key, existing).unwrap();

    // Interactive session acquires the per-stream lease lock.
    let interactive_lease = match StreamLeaseGuard::try_acquire(
        &projects_root,
        &binding.project_key,
        "triage-stream",
        StreamLeaseHolderKind::Interactive,
        None,
    )
    .unwrap()
    {
        StreamLeaseAcquireResult::Acquired(guard) => guard,
        StreamLeaseAcquireResult::Contention { .. } => panic!("expected interactive lease"),
    };

    let spec = test_spec(&binding.project_key);
    let prep = prepare_stateful_cron_stream_run(
        &projects_root,
        &spec,
        "run-contention-1",
        "/tmp/wf-run.log",
        || Ok(()),
    )
    .await
    .unwrap();

    match prep {
        CronStreamPrepareResult::DeferredLockContention { inbox_entry } => {
            assert_eq!(inbox_entry.status, InboxDigestStatus::Warning);
            assert_eq!(inbox_entry.stream_name.as_deref(), Some("triage-stream"));
            assert!(inbox_entry.summary.contains("lease lock"));
        }
        other => panic!("expected DeferredLockContention, got {other:?}"),
    }

    // Stream record & checkpoint remain completely untouched.
    let loaded = load_workstream_record(&projects_root, &binding.project_key, "triage-stream")
        .unwrap()
        .unwrap();
    assert_eq!(loaded.state, WorkstreamState::Idle);
    assert_eq!(
        loaded.last_checkpoint_ref.as_deref(),
        Some("/tmp/original-ckpt.jsonl")
    );
    assert_eq!(
        list_unread_inbox_digests(&projects_root, &binding.project_key)
            .unwrap()
            .len(),
        1
    );

    drop(interactive_lease);
}

#[tokio::test]
async fn cron_stream_compacts_over_threshold_enforces_budget_and_pauses_on_consecutive_failures() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("projects");
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    let binding = bind_project_dir(&projects_root, &repo_dir, Some("repo-rw".to_string())).unwrap();

    // Create a real rollout checkpoint exceeding 150 tokens.
    let ckpt_path = temp.path().join("triage-ckpt.jsonl");
    let meta = RolloutItem::AgentMachineMeta(AgentMachineMeta {
        rollout_id: "triage-ckpt".to_string(),
        process_path: "/proc/1".to_string(),
        cwd: repo_dir.display().to_string(),
        model: "mock".to_string(),
        started_at: Utc::now().to_rfc3339(),
        reasoning_effort: None,
    });
    let mut lines = vec![serde_json::to_string(&meta).unwrap()];
    for idx in 0..20 {
        let text = format!(
            "Message {idx}: inspecting repository diagnostics and regression logs with substantial context words."
        );
        let (role, rich_msg) = if idx % 2 == 1 {
            ("assistant", crate::tape::Message::assistant(&text))
        } else {
            ("user", crate::tape::Message::user(&text))
        };
        let msg = crate::rollout::MessageRecord {
            role: role.to_string(),
            content: Some(text),
            tool_name: None,
            message: Some(rich_msg),
            timestamp: Utc::now().to_rfc3339(),
        };
        lines.push(serde_json::to_string(&RolloutItem::Message(msg)).unwrap());
    }
    fs::write(&ckpt_path, format!("{}\n", lines.join("\n"))).unwrap();

    let mut seed = WorkstreamRecord::new(
        "triage-stream",
        "Nightly repo triage",
        WorkstreamPersistence::Persistent,
    );
    seed.state = WorkstreamState::Idle;
    seed.last_checkpoint_ref = Some(ckpt_path.display().to_string());
    upsert_workstream(&projects_root, &binding.project_key, seed).unwrap();

    let spec = test_spec(&binding.project_key);

    // Run 1: exceeds per_run_budget_usd ($0.40 > $0.25) -> failure 1/2, compacts checkpoint first.
    let (lease1, rec1, before, after) = match prepare_stateful_cron_stream_run(
        &projects_root,
        &spec,
        "run-1",
        "/tmp/run-1.log",
        || Ok(()),
    )
    .await
    .unwrap()
    {
        CronStreamPrepareResult::Ready {
            lease,
            record,
            compacted_tokens_before,
            compacted_tokens_after,
        } => (
            lease,
            record,
            compacted_tokens_before,
            compacted_tokens_after,
        ),
        other => panic!("expected Ready, got {other:?}"),
    };
    assert!(before.unwrap() >= 150);
    assert!(after.unwrap() < before.unwrap());

    let out1 = finalize_stateful_cron_stream_run(
        &projects_root,
        &spec,
        rec1,
        lease1,
        "run-1",
        "/tmp/run-1.log",
        true,
        Some(0.40),
        "Completed turn",
        None,
        None,
    )
    .unwrap();
    assert!(!out1.transitioned_to_paused_error);
    assert_eq!(out1.record.state, WorkstreamState::Idle);
    assert_eq!(
        out1.record
            .cron_binding
            .as_ref()
            .unwrap()
            .consecutive_failures,
        1
    );
    assert_eq!(out1.inbox_entry.status, InboxDigestStatus::Warning);

    // Run 2: grant revalidation fails -> reaches max_consecutive_failures (2/2) -> paused_error + inbox alert.
    let grant_err =
        prepare_stateful_cron_stream_run(&projects_root, &spec, "run-2", "/tmp/run-2.log", || {
            bail!("grant `repo-rw` was revoked")
        })
        .await
        .unwrap_err();
    assert!(grant_err.to_string().contains("consecutive_failures=2"));

    let paused = load_workstream_record(&projects_root, &binding.project_key, "triage-stream")
        .unwrap()
        .unwrap();
    assert_eq!(paused.state, WorkstreamState::PausedError);
    let unread = list_unread_inbox_digests(&projects_root, &binding.project_key).unwrap();
    assert!(
        unread
            .iter()
            .any(|e| e.status == InboxDigestStatus::PausedError
                && e.stream_name.as_deref() == Some("triage-stream")),
        "{unread:?}"
    );

    // Run 3 while paused_error skips cleanly without touching state.
    let skipped =
        prepare_stateful_cron_stream_run(&projects_root, &spec, "run-3", "/tmp/run-3.log", || {
            Ok(())
        })
        .await
        .unwrap();
    assert!(matches!(
        skipped,
        CronStreamPrepareResult::SkippedPausedError { .. }
    ));
}
