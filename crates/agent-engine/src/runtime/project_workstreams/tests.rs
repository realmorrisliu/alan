use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::*;

fn init_fake_git_repo(dir: &Path, remote_url: &str) {
    let git_dir = dir.join(".git");
    fs::create_dir_all(&git_dir).unwrap();
    fs::write(
        git_dir.join("config"),
        format!("[remote \"origin\"]\n\turl = {remote_url}\n"),
    )
    .unwrap();
}

#[test]
fn explicit_bind_and_resolve_match_only_canonical_path_never_foreign_checkout() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("System Store/services/projects");

    let primary = temp.path().join("workspace/alan");
    let foreign_same_remote = temp.path().join("other/alan");
    let nested_worktree = primary.join(".worktrees/feature-a");
    fs::create_dir_all(&primary).unwrap();
    fs::create_dir_all(&foreign_same_remote).unwrap();
    fs::create_dir_all(&nested_worktree).unwrap();

    // Both checkouts share the exact same Git remote and directory basename (`alan`).
    init_fake_git_repo(&primary, "git@github.com:example/alan.git");
    init_fake_git_repo(&foreign_same_remote, "git@github.com:example/alan.git");
    init_fake_git_repo(&nested_worktree, "git@github.com:example/alan.git");

    // Nothing bound yet.
    assert!(
        resolve_bound_project(&projects_root, &primary)
            .unwrap()
            .is_none()
    );

    // Explicit bind creates opaque key + binding.json.
    let binding = bind_project_dir(&projects_root, &primary, Some("grant-1".to_string())).unwrap();
    assert!(binding.project_key.starts_with("proj-"));
    assert!(!binding.project_key.contains("alan"));
    assert_eq!(
        binding.canonical_root_path,
        dunce::canonicalize(&primary).unwrap()
    );
    assert_eq!(binding.grant_ref.as_deref(), Some("grant-1"));
    assert!(
        binding_path(&projects_root, &binding.project_key)
            .unwrap()
            .is_file()
    );

    // Exact bound path resolves.
    let resolved = resolve_bound_project(&projects_root, &primary)
        .unwrap()
        .expect("bound dir must resolve");
    assert_eq!(resolved.project_key, binding.project_key);

    // Re-binding same path is idempotent on project_key and created_at, updating grant_ref if provided.
    let rebound = bind_project_dir(&projects_root, &primary, Some("grant-2".to_string())).unwrap();
    assert_eq!(rebound.project_key, binding.project_key);
    assert_eq!(rebound.created_at, binding.created_at);
    assert_eq!(rebound.grant_ref.as_deref(), Some("grant-2"));

    // Foreign checkout with same Git remote and basename NEVER resolves.
    assert!(
        resolve_bound_project(&projects_root, &foreign_same_remote)
            .unwrap()
            .is_none()
    );

    // Nested foreign worktree inside the bound repo NEVER resolves either.
    assert!(
        resolve_bound_project(&projects_root, &nested_worktree)
            .unwrap()
            .is_none()
    );

    // Symlink pointing directly to the bound directory resolves to the same canonical binding.
    #[cfg(unix)]
    {
        let alias = temp.path().join("alias-to-primary");
        std::os::unix::fs::symlink(&primary, &alias).unwrap();
        let via_alias = resolve_bound_project(&projects_root, &alias)
            .unwrap()
            .expect("symlink to canonical bound root resolves");
        assert_eq!(via_alias.project_key, binding.project_key);
    }
}

#[test]
fn ledger_and_stream_transitions_persist_atomically() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("System Store/services/projects");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();

    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let key = &binding.project_key;

    // Empty ledger before any workstreams exist (and does not create ledger.json on read).
    let initial = load_project_ledger(&projects_root, key).unwrap();
    assert!(initial.streams.is_empty());
    assert!(!ledger_path(&projects_root, key).unwrap().exists());

    // Create a persistent stream and a one-off stream.
    let mut ui_stream = WorkstreamRecord::new(
        "ui-polish",
        "Polish terminal viewport with sk-ant-secret1234567890",
        WorkstreamPersistence::Persistent,
    );
    ui_stream.budget_profile = Some("default".to_string());
    ui_stream.model_override = Some("gpt-5.4".to_string());
    let persisted_ui = upsert_workstream(&projects_root, key, ui_stream).unwrap();
    assert_eq!(persisted_ui.state, WorkstreamState::Running);
    assert!(!persisted_ui.scope.contains("sk-ant-secret"));
    assert!(persisted_ui.scope.contains("[REDACTED]"));

    let quick_fix = WorkstreamRecord::new(
        "quick-fix",
        "Fix typo in README",
        WorkstreamPersistence::OneOff,
    );
    upsert_workstream(&projects_root, key, quick_fix).unwrap();

    // Both streams exist on disk under streams/<name>.json and in ledger.json.
    assert!(
        stream_path(&projects_root, key, "ui-polish")
            .unwrap()
            .is_file()
    );
    assert!(
        stream_path(&projects_root, key, "quick-fix")
            .unwrap()
            .is_file()
    );
    assert!(ledger_path(&projects_root, key).unwrap().is_file());

    let counts_running = workstream_counts(&projects_root, key).unwrap();
    assert_eq!(counts_running.running, 2);
    assert_eq!(counts_running.idle, 0);
    assert_eq!(counts_running.archived, 0);
    assert_eq!(counts_running.active_non_archived(), 2);

    // Persistent stream completion transitions Running -> Idle with checkpoint/rollout refs.
    let idled = complete_workstream(
        &projects_root,
        key,
        "ui-polish",
        Some("ckpt://rollout-1".to_string()),
        Some("rollouts/rollout-1.jsonl".to_string()),
    )
    .unwrap();
    assert_eq!(idled.state, WorkstreamState::Idle);
    assert_eq!(
        idled.last_checkpoint_ref.as_deref(),
        Some("ckpt://rollout-1")
    );
    assert_eq!(
        idled.rollout_ref.as_deref(),
        Some("rollouts/rollout-1.jsonl")
    );

    // One-off stream completion transitions Running -> Archived and collapses from active roster.
    let archived = complete_workstream(
        &projects_root,
        key,
        "quick-fix",
        None,
        Some("rollouts/quick-fix.jsonl".to_string()),
    )
    .unwrap();
    assert_eq!(archived.state, WorkstreamState::Archived);
    assert_eq!(
        archived.rollout_ref.as_deref(),
        Some("rollouts/quick-fix.jsonl")
    );

    let ledger = load_project_ledger(&projects_root, key).unwrap();
    assert_eq!(ledger.active_streams().len(), 1);
    assert_eq!(ledger.active_streams()[0].name, "ui-polish");
    assert_eq!(ledger.archived_streams().len(), 1);
    assert_eq!(ledger.archived_streams()[0].name, "quick-fix");

    let counts = ledger.counts();
    assert_eq!(counts.running, 0);
    assert_eq!(counts.idle, 1);
    assert_eq!(counts.archived, 1);
    assert_eq!(counts.active_non_archived(), 1);
    assert_eq!(counts.total(), 2);

    // Persistent stream can resume from Idle -> Running and keeps its checkpoint ref.
    let resumed = resume_workstream(&projects_root, key, "ui-polish").unwrap();
    assert_eq!(resumed.state, WorkstreamState::Running);
    assert_eq!(
        resumed.last_checkpoint_ref.as_deref(),
        Some("ckpt://rollout-1")
    );

    // Archived / one-off stream cannot be resumed.
    assert!(resume_workstream(&projects_root, key, "quick-fix").is_err());

    // Pause-on-error and resume recovery also work.
    let paused = pause_workstream_error(
        &projects_root,
        key,
        "ui-polish",
        Some("ckpt://rollout-2".to_string()),
        None,
    )
    .unwrap();
    assert_eq!(paused.state, WorkstreamState::PausedError);
    assert_eq!(
        paused.last_checkpoint_ref.as_deref(),
        Some("ckpt://rollout-2")
    );
    let resumed_from_pause = resume_workstream(&projects_root, key, "ui-polish").unwrap();
    assert_eq!(resumed_from_pause.state, WorkstreamState::Running);
}

#[test]
fn inbox_writes_atomically_scrubs_secrets_and_tracks_digested_ack() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("System Store/services/projects");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();

    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let key = &binding.project_key;

    assert!(
        list_unread_inbox_digests(&projects_root, key)
            .unwrap()
            .is_empty()
    );

    let long_secret_text = format!(
        "Nightly check failed with Bearer super_secret_token_abc and AIzaSyDummyKeyValue123456789 {}",
        "x".repeat(MAX_INBOX_SUMMARY_CHARS + 200)
    );
    let entry_1 = InboxDigestEntry {
        version: INBOX_DIGEST_VERSION,
        run_id: "wf-001".to_string(),
        workflow_id: "nightly-ci".to_string(),
        stream_name: Some("ci-watch".to_string()),
        status: InboxDigestStatus::PausedError,
        summary: long_secret_text,
        evidence_ref: "services/workflows/runs/nightly-ci/runs.ndjson".to_string(),
        created_at: "2026-11-24T10:00:00Z".to_string(),
        digested_at: None,
    };
    let written_1 = write_inbox_digest(&projects_root, key, entry_1).unwrap();
    assert!(
        inbox_entry_path(&projects_root, key, "wf-001")
            .unwrap()
            .is_file()
    );
    assert!(!written_1.summary.contains("super_secret_token_abc"));
    assert!(!written_1.summary.contains("AIzaSyDummy"));
    assert!(written_1.summary.contains("[REDACTED]"));
    assert!(written_1.summary.chars().count() <= MAX_INBOX_SUMMARY_CHARS);
    assert!(written_1.summary.ends_with("..."));

    let entry_2 = InboxDigestEntry {
        version: INBOX_DIGEST_VERSION,
        run_id: "wf-002".to_string(),
        workflow_id: "daily-triage".to_string(),
        stream_name: None,
        status: InboxDigestStatus::Ok,
        summary: "All 3 PR checks green".to_string(),
        evidence_ref: "services/workflows/runs/daily-triage/runs.ndjson".to_string(),
        created_at: "2026-11-24T11:00:00Z".to_string(),
        digested_at: None,
    };
    write_inbox_digest(&projects_root, key, entry_2).unwrap();

    let unread = list_unread_inbox_digests(&projects_root, key).unwrap();
    assert_eq!(unread.len(), 2);
    assert_eq!(unread[0].run_id, "wf-001");
    assert_eq!(unread[1].run_id, "wf-002");

    // Mark single entry digested.
    let acked = mark_inbox_digest_digested(&projects_root, key, "wf-001").unwrap();
    assert!(acked.digested_at.is_some());
    let unread_after_single = list_unread_inbox_digests(&projects_root, key).unwrap();
    assert_eq!(unread_after_single.len(), 1);
    assert_eq!(unread_after_single[0].run_id, "wf-002");

    // Idempotent re-ack preserves original digested_at timestamp.
    let reacked = mark_inbox_digest_digested(&projects_root, key, "wf-001").unwrap();
    assert_eq!(reacked.digested_at, acked.digested_at);

    // Mark remaining unread entries digested.
    let acked_all = mark_all_inbox_digests_digested(&projects_root, key).unwrap();
    assert_eq!(acked_all.len(), 1);
    assert_eq!(acked_all[0].run_id, "wf-002");
    assert!(acked_all[0].digested_at.is_some());

    assert!(
        list_unread_inbox_digests(&projects_root, key)
            .unwrap()
            .is_empty()
    );
    assert_eq!(list_inbox_digests(&projects_root, key).unwrap().len(), 2);
}

#[test]
fn rejects_path_traversal_and_symlinks_in_store_paths() {
    let temp = TempDir::new().unwrap();
    let projects_root = temp.path().join("System Store/services/projects");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();

    let binding = bind_project_dir(&projects_root, &repo, None).unwrap();
    let key = &binding.project_key;

    assert!(validate_project_key("../escape").is_err());
    assert!(validate_project_key("bad/key").is_err());
    assert!(
        upsert_workstream(
            &projects_root,
            key,
            WorkstreamRecord::new("../evil", "nope", WorkstreamPersistence::OneOff),
        )
        .is_err()
    );
    assert!(
        write_inbox_digest(
            &projects_root,
            key,
            InboxDigestEntry::new("../evil", "wf", None, InboxDigestStatus::Ok, "ok", "ref",),
        )
        .is_err()
    );

    #[cfg(unix)]
    {
        let target = temp.path().join("outside.json");
        fs::write(&target, "{}").unwrap();
        let s_dir = streams_dir(&projects_root, key).unwrap();
        fs::create_dir_all(&s_dir).unwrap();
        std::os::unix::fs::symlink(&target, s_dir.join("evil.json")).unwrap();
        assert!(load_workstream_record(&projects_root, key, "evil").is_err());
    }
}
