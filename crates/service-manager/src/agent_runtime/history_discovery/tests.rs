use super::*;
use alan_agent_engine::{CheckpointRecord, EffectRecord, EventRecord};
use tempfile::TempDir;

fn test_stores(dir: &Path) -> AgentRuntimeStoreBindings {
    let stores = AgentRuntimeStoreBindings {
        rollouts: dir.join("rollouts"),
        metadata: dir.join("metadata"),
        checkpoints: dir.join("checkpoints"),
        cache: dir.join("cache"),
        tmp: dir.join("tmp"),
    };
    fs::create_dir_all(&stores.rollouts).unwrap();
    fs::create_dir_all(&stores.metadata).unwrap();
    stores
}

fn meta_line(rollout_id: &str, cwd: &str, started_at: &str) -> String {
    serde_json::to_string(&RolloutItem::AgentMachineMeta(AgentMachineMeta {
        rollout_id: rollout_id.to_string(),
        process_path: "/proc/1".to_string(),
        started_at: started_at.to_string(),
        cwd: cwd.to_string(),
        model: "gpt-5.4".to_string(),
        reasoning_effort: None,
    }))
    .unwrap()
}

fn user_line(content: &str, timestamp: &str) -> String {
    serde_json::to_string(&RolloutItem::Message(MessageRecord {
        role: "user".to_string(),
        content: Some(content.to_string()),
        tool_name: None,
        message: None,
        timestamp: timestamp.to_string(),
    }))
    .unwrap()
}

#[test]
fn discovery_tolerates_corrupt_torn_and_oversized_lines_and_scrubs_secrets() {
    let temp = TempDir::new().unwrap();
    let stores = test_stores(temp.path());

    let valid_path = stores.rollouts.join("r-valid.jsonl");
    let torn_content = format!(
        "{}\n{}\n{{\"bad\": \n{}\n{}\n",
        meta_line("r-valid", "/mnt/repo-a", "2026-11-18T10:00:00Z"),
        user_line(
            "Fix sk-proj-supersecret123456 and AIzaSyDummyKey1234567",
            "2026-11-18T10:00:05Z"
        ),
        user_line("Follow-up turn", "2026-11-18T10:02:00Z"),
        serde_json::to_string(&RolloutItem::Checkpoint(CheckpointRecord {
            checkpoint_id: "cp-1".to_string(),
            checkpoint_type: "confirmation".to_string(),
            summary: "confirm".to_string(),
            choice: None,
            knowledge_root: None,
            timestamp: "2026-11-18T10:03:00Z".to_string(),
        }))
        .unwrap()
    );
    fs::write(&valid_path, torn_content).unwrap();

    let oversized_path = stores.rollouts.join("r-oversized.jsonl");
    let giant_line = format!(
        "{{\"type\":\"message\",\"role\":\"assistant\",\"content\":\"{}\",\"timestamp\":\"2026-11-18T11:00:00Z\"}}",
        "x".repeat(MAX_SCAN_LINE_BYTES + 1024)
    );
    fs::write(
        &oversized_path,
        format!(
            "{}\n{}\n{}\n",
            meta_line("r-oversized", "/mnt/repo-a", "2026-11-18T11:00:00Z"),
            giant_line,
            user_line("Still readable after giant line", "2026-11-18T11:01:00Z"),
        ),
    )
    .unwrap();

    let catalog = discover_rollout_history(
        &stores,
        &HistoryProjectScope::from_cwd(Some("/mnt/repo-a")),
        None,
        None,
        10,
    );
    assert_eq!(catalog.entries.len(), 2);
    assert!(
        catalog
            .diagnostics
            .iter()
            .any(|d| d.message.contains("exceeded")),
        "diagnostics: {:?}",
        catalog.diagnostics
    );
    assert!(
        catalog
            .diagnostics
            .iter()
            .any(|d| d.message.contains("corrupt")),
        "diagnostics: {:?}",
        catalog.diagnostics
    );

    let valid = catalog
        .entries
        .iter()
        .find(|e| e.rollout_id == "r-valid")
        .unwrap();
    assert_eq!(valid.turn_count, 2);
    assert_eq!(valid.last_status, "paused");
    assert!(valid.has_pending_checkpoint);
    assert!(!valid.goal_snippet.contains("supersecret"));
    assert!(!valid.goal_snippet.contains("AIzaSy"));
    assert!(valid.goal_snippet.contains("[REDACTED]"));
}

#[test]
fn discovery_enforces_strict_cross_project_isolation_and_ignores_symlinks() {
    let temp = TempDir::new().unwrap();
    let stores = test_stores(temp.path());

    let repo_a_file = stores.rollouts.join("a.jsonl");
    let repo_b_file = stores.rollouts.join("b.jsonl");
    fs::write(
        &repo_a_file,
        format!(
            "{}\n{}\n",
            meta_line("roll-a", "/mnt/project", "2026-11-18T09:00:00Z"),
            user_line("Alpha secret plan", "2026-11-18T09:01:00Z")
        ),
    )
    .unwrap();
    fs::write(
        &repo_b_file,
        format!(
            "{}\n{}\n",
            meta_line("roll-b", "/mnt/project", "2026-11-18T10:00:00Z"),
            user_line("Beta private plan", "2026-11-18T10:01:00Z")
        ),
    )
    .unwrap();

    let scope_a = HistoryProjectScope::from_cwd(Some("/mnt/project"))
        .with_fingerprint(Some(fingerprint_project_boundary("/host/repo-a")))
        .with_label(Some("repo-a".to_string()));
    let scope_b = HistoryProjectScope::from_cwd(Some("/mnt/project"))
        .with_fingerprint(Some(fingerprint_project_boundary("/host/repo-b")))
        .with_label(Some("repo-b".to_string()));

    record_rollout_project_index(&stores, &repo_a_file, &scope_a).unwrap();
    record_rollout_project_index(&stores, &repo_b_file, &scope_b).unwrap();

    #[cfg(unix)]
    {
        let outside = temp.path().join("outside.jsonl");
        fs::write(
            &outside,
            format!(
                "{}\n",
                meta_line("roll-outside", "/mnt/project", "2026-11-18T12:00:00Z")
            ),
        )
        .unwrap();
        std::os::unix::fs::symlink(&outside, stores.rollouts.join("sym.jsonl")).unwrap();
    }

    let catalog_a = discover_rollout_history(&stores, &scope_a, None, None, 10);
    assert_eq!(catalog_a.entries.len(), 1);
    assert_eq!(catalog_a.entries[0].rollout_id, "roll-a");
    assert_eq!(catalog_a.excluded_cross_project_count, 1);

    let err = resolve_authorized_rollout_file(&stores, &scope_a, "roll-b").unwrap_err();
    assert!(err.to_string().contains("another project"));
    assert!(resolve_authorized_rollout_file(&stores, &scope_a, "roll-a").is_ok());

    let eff_file = stores.rollouts.join("eff.jsonl");
    fs::write(
        &eff_file,
        format!(
            "{}\n{}\n",
            meta_line("roll-eff", "/mnt/other", "2026-11-18T12:00:00Z"),
            serde_json::to_string(&RolloutItem::Effect(EffectRecord {
                effect_id: "e1".to_string(),
                process_path: "/proc/1".to_string(),
                tool_call_id: "t1".to_string(),
                idempotency_key: "k1".to_string(),
                effect_type: "write".to_string(),
                request_fingerprint: "fp".to_string(),
                result_digest: None,
                result_payload: None,
                status: EffectStatus::Unknown,
                applied_at: None,
                reason: None,
                dedupe_hit: false,
                timestamp: "2026-11-18T12:01:00Z".to_string(),
            }))
            .unwrap()
        ),
    )
    .unwrap();
    let eff_cat = discover_rollout_history(
        &stores,
        &HistoryProjectScope::from_cwd(Some("/mnt/other")),
        None,
        None,
        10,
    );
    assert_eq!(eff_cat.entries.len(), 1);
    assert!(eff_cat.entries[0].has_unknown_effects);
    assert_eq!(eff_cat.entries[0].last_status, "attention");
    let _ = EventRecord {
        event_type: String::new(),
        payload: serde_json::json!({}),
        timestamp: String::new(),
    };
}

#[tokio::test]
async fn history_service_fs_projects_catalog_and_rejects_cross_project_or_ephemeral_switch() {
    let temp = TempDir::new().unwrap();
    let stores = test_stores(temp.path());
    let repo_a_file = stores.rollouts.join("a.jsonl");
    let repo_b_file = stores.rollouts.join("b.jsonl");
    fs::write(
        &repo_a_file,
        format!(
            "{}\n{}\n",
            meta_line("roll-a", "/mnt/repo-a", "2026-11-18T09:00:00Z"),
            user_line("Inspect logs", "2026-11-18T09:01:00Z")
        ),
    )
    .unwrap();
    fs::write(
        &repo_b_file,
        format!(
            "{}\n{}\n",
            meta_line("roll-b", "/mnt/repo-b", "2026-11-18T10:00:00Z"),
            user_line("Other workspace", "2026-11-18T10:01:00Z")
        ),
    )
    .unwrap();

    let switched: Arc<Mutex<Vec<Option<PathBuf>>>> = Arc::new(Mutex::new(Vec::new()));
    let ephemeral_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let svc = Arc::new(HistoryDiscoveryService::new(
        Arc::new({
            let stores = stores.clone();
            let ephemeral_flag = ephemeral_flag.clone();
            move || {
                let stores = stores.clone();
                let eph = ephemeral_flag.load(std::sync::atomic::Ordering::SeqCst);
                Box::pin(async move {
                    (
                        Some(stores),
                        HistoryProjectScope::from_cwd(Some("/mnt/repo-a")),
                        Some("roll-a".to_string()),
                        eph,
                    )
                })
            }
        }),
        Arc::new({
            let switched = switched.clone();
            move |next| {
                let switched = switched.clone();
                Box::pin(async move {
                    switched.lock().unwrap().push(next);
                    Ok(())
                })
            }
        }),
    ));
    let fs = Arc::new(crate::flat_fs::FlatServiceFs::new(svc));
    let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(fs));

    let catalog: RolloutHistoryCatalog =
        serde_json::from_slice(&shell.cat("/catalog").await.unwrap()).unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].rollout_id, "roll-a");
    assert_eq!(catalog.excluded_cross_project_count, 1);

    // Cross-project switch must fail.
    assert!(shell.write("/ctl", b"resume roll-b").await.is_err());
    // Authorized switch succeeds cleanly without side-effect replay.
    shell.write("/ctl", b"resume roll-a").await.unwrap();
    shell.write("/ctl", b"fresh").await.unwrap();
    let recorded = switched.lock().unwrap().clone();
    assert_eq!(recorded, vec![Some(repo_a_file), None]);

    ephemeral_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(shell.write("/ctl", b"resume roll-a").await.is_err());
}

#[tokio::test]
async fn history_summary_files_enforce_redaction_revision_and_delete() {
    let temp = TempDir::new().unwrap();
    let stores = test_stores(temp.path());
    let memory_dir = temp.path().join("memory-store");
    let ephemeral_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let svc = Arc::new(
        HistoryDiscoveryService::new(
            Arc::new({
                let stores = stores.clone();
                let ephemeral_flag = ephemeral_flag.clone();
                move || {
                    let stores = stores.clone();
                    let eph = ephemeral_flag.load(std::sync::atomic::Ordering::SeqCst);
                    Box::pin(async move {
                        (
                            Some(stores),
                            HistoryProjectScope::from_cwd(Some("/mnt/repo-a")),
                            None,
                            eph,
                        )
                    })
                }
            }),
            Arc::new(|_| Box::pin(async move { Ok(()) })),
        )
        .with_memory_store_provider(Arc::new({
            let memory_dir = memory_dir.clone();
            move || Some(memory_dir.clone())
        })),
    );
    let fs = Arc::new(crate::flat_fs::FlatServiceFs::new(svc));
    let shell = alan_shell::Shell::new(alan_ap::InProcessTransport::new(fs));

    let initial: alan_agent_engine::ProjectContinuitySummaryView =
        serde_json::from_slice(&shell.cat("/summary").await.unwrap()).unwrap();
    assert!(!initial.exists);
    assert!(!initial.deleted);

    let edit_payload = serde_json::json!({
        "action": "edit",
        "text": "goal: Harden sandbox with sk-proj-secret1234567890",
        "expected_revision": 0
    });
    shell
        .write("/summary_ctl", edit_payload.to_string().as_bytes())
        .await
        .unwrap();

    let updated: alan_agent_engine::ProjectContinuitySummaryView =
        serde_json::from_slice(&shell.cat("/summary").await.unwrap()).unwrap();
    assert!(updated.exists);
    let summary = updated.summary.unwrap();
    assert_eq!(summary.revision, 1);
    assert!(!summary.goal.contains("sk-proj-secret"));
    assert!(summary.goal.contains("[REDACTED]"));

    // Stale expected_revision must be rejected.
    let stale_edit = serde_json::json!({
        "action": "edit",
        "text": "goal: Stale write",
        "expected_revision": 99
    });
    assert!(
        shell
            .write("/summary_ctl", stale_edit.to_string().as_bytes())
            .await
            .is_err()
    );

    let delete_payload = serde_json::json!({
        "action": "delete",
        "expected_revision": 1
    });
    shell
        .write("/summary_ctl", delete_payload.to_string().as_bytes())
        .await
        .unwrap();

    let after_delete: alan_agent_engine::ProjectContinuitySummaryView =
        serde_json::from_slice(&shell.cat("/summary").await.unwrap()).unwrap();
    assert!(after_delete.deleted);
    assert!(!after_delete.markdown_recall_active);

    ephemeral_flag.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(shell.write("/summary_ctl", b"delete").await.is_err());
}
