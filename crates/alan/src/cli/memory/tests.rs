use super::*;
use tempfile::TempDir;

#[test]
fn memory_cli_list_show_edit_revert_and_bm25_cjk_round_trip() {
    let temp = TempDir::new().unwrap();
    let stores = temp.path().join("stores");

    // 1. Initial edit creates `topics/preferences.md` in `personal` domain and scrubs secrets.
    let edit1 = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Edit {
                target: "topics/preferences.md".to_string(),
                content: Some(
                    "# Preferences\n用户偏好：使用 Rust 和中文。\nToken: sk-ant-secret1234567890\n"
                        .to_string(),
                ),
                file: None,
                domain: Some("personal".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    let edit1_report: MemoryEditReport = serde_json::from_str(&edit1).unwrap();
    assert_eq!(edit1_report.domain, "personal");
    assert!(edit1_report.snapshotted_revision.is_none());

    // 2. Second edit snapshots rev-0001 into `.revisions/` before updating.
    let edit2 = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Edit {
                target: "topics/preferences.md".to_string(),
                content: Some("# Preferences\nUpdated preference: English only.\n".to_string()),
                file: None,
                domain: Some("personal".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    let edit2_report: MemoryEditReport = serde_json::from_str(&edit2).unwrap();
    let rev1 = edit2_report
        .snapshotted_revision
        .expect("second edit must snapshot prior revision");
    assert_eq!(rev1.revision_id, "rev-0001");

    // 3. `show` reports sanitized content and revision history metadata.
    let show_out = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Show {
                target: "topics/preferences.md".to_string(),
                domain: Some("personal".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    let show_report: MemoryShowReport = serde_json::from_str(&show_out).unwrap();
    assert!(
        show_report
            .content
            .contains("Updated preference: English only.")
    );
    assert_eq!(show_report.revisions.len(), 1);
    assert_eq!(show_report.revisions[0].revision_id, "rev-0001");

    // 4. `revert` restores rev-0001 cleanly without leaking the scrubbed secret.
    let revert_out = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Revert {
                target: "topics/preferences.md".to_string(),
                revision: None,
                domain: Some("personal".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    let revert_report: MemoryRevertReport = serde_json::from_str(&revert_out).unwrap();
    assert_eq!(revert_report.restored_revision.revision_id, "rev-0001");
    assert!(revert_report.pre_revert_snapshot.is_some());

    let show_after = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Show {
                target: "topics/preferences.md".to_string(),
                domain: Some("personal".to_string()),
                json: false,
            },
        },
        None,
    )
    .unwrap();
    assert!(show_after.contains("用户偏好：使用 Rust 和中文。"));
    assert!(!show_after.contains("sk-ant-secret1234567890"));
    assert!(show_after.contains("[REDACTED"));

    // 5. Add a file to `project:alan` and verify BM25+CJK query + domain isolation.
    run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Edit {
                target: "cargo-errors.md".to_string(),
                content: Some("Rust 项目里的 Cargo 报错可通过 cargo check 排查。".to_string()),
                file: None,
                domain: Some("project:alan".to_string()),
                json: false,
            },
        },
        None,
    )
    .unwrap();

    // Default domain must NOT see `personal` or `project:alan`.
    let default_list = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::List {
                domain: None,
                query: Some("Rust 项目里的 Cargo 报错".to_string()),
                json: false,
            },
        },
        None,
    )
    .unwrap();
    assert!(default_list.contains("No memory matches"), "{default_list}");

    // Explicit `project:alan` search ranks `cargo-errors.md`.
    let proj_list = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::List {
                domain: Some("project:alan".to_string()),
                query: Some("Rust 项目里的 Cargo 报错".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    assert!(proj_list.contains("cargo-errors.md"), "{proj_list}");

    // Explicit `--domain all` finds both CJK hits.
    let all_list = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::List {
                domain: Some("all".to_string()),
                query: Some("用户偏好用哪种语言？".to_string()),
                json: true,
            },
        },
        None,
    )
    .unwrap();
    assert!(all_list.contains("topics/preferences.md"), "{all_list}");
}

#[cfg(unix)]
#[test]
fn memory_cli_rejects_traversal_and_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let stores = temp.path().join("stores");
    let default_dir = stores.join("default");
    fs::create_dir_all(&default_dir).unwrap();
    let outside = temp.path().join("outside.md");
    fs::write(&outside, "outside secret").unwrap();
    symlink(&outside, default_dir.join("escape.md")).unwrap();

    let err_traversal = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Show {
                target: "../outside.md".to_string(),
                domain: None,
                json: false,
            },
        },
        None,
    )
    .unwrap_err();
    assert!(err_traversal.to_string().contains("traversal"));

    let err_symlink = run_with_stores_root(
        &stores,
        MemoryArgs {
            action: MemoryAction::Edit {
                target: "escape.md".to_string(),
                content: Some("overwrite".to_string()),
                file: None,
                domain: None,
                json: false,
            },
        },
        None,
    )
    .unwrap_err();
    assert!(err_symlink.to_string().contains("symlink"));
}
