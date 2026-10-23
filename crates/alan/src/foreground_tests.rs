use super::*;
use alan_agent_protocol::InputIntent;
use alan_tui::composer::{Composer, HistoryEntry, load_history};

#[test]
fn guardian_policy_is_a_bounded_explicit_utf8_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("policy.md");
    assert!(load_guardian_policy(&path).is_err());
    std::fs::write(&path, "original policy").unwrap();
    let snapshot = load_guardian_policy(&path).unwrap();
    for contents in [vec![b' '; 10], vec![0xff], vec![b'x'; 65_537]] {
        std::fs::write(&path, contents).unwrap();
        assert!(load_guardian_policy(&path).is_err());
    }
    std::fs::write(&path, "界".repeat(21_846)).unwrap();
    assert!(
        load_guardian_policy(&path)
            .unwrap_err()
            .to_string()
            .contains("64 KiB")
    );
    std::fs::write(&path, "x".repeat(65_536)).unwrap();
    assert_eq!(load_guardian_policy(&path).unwrap().len(), 65_536);
    assert_eq!(snapshot, "original policy");
}

fn config(store: &alan_os_host::SystemStorePaths) -> alan_tui::FileBackedRunConfig {
    interactive_config(
        alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        ))),
        Some("model".into()),
        Some(PathBuf::from("/project-candidate")),
        store,
    )
    .unwrap()
}

#[test]
fn foreground_history_is_product_store_owned_and_restarts_typed() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let isolated =
        alan_os_host::SystemStorePaths::from_data_dir(&dir.path().join("other-host")).unwrap();
    let first = config(&product);
    assert_eq!(first.agent_path, "/agent/root");
    assert_eq!(first.effective_model.as_deref(), Some("model"));
    assert_eq!(
        first.project_candidate,
        Some(PathBuf::from("/project-candidate"))
    );
    assert!(first.host_file_completion_root.is_none());
    assert!(first.require_interactive_terminal);
    let path = first
        .history_path
        .expect("shipped interactive history configured");
    assert_eq!(
        path,
        product
            .service("shell-ui")
            .unwrap()
            .join("composer-history")
    );
    assert_ne!(Some(path.clone()), config(&isolated).history_path);
    let entries = [
        HistoryEntry {
            body: "!literal agent\n  ".into(),
            intent: InputIntent::Agent,
        },
        HistoryEntry {
            body: "printf '%s' 'a b'\n  ".into(),
            intent: InputIntent::Command,
        },
    ];
    let mut composer = Composer::with_history(load_history(&path, 1000), Some(path));
    for entry in &entries {
        composer.remember_input(&entry.body, entry.intent);
        composer.remember_input(&entry.body, entry.intent);
    }
    let restarted_path = config(&product).history_path.unwrap();
    assert_eq!(load_history(&restarted_path, 1000), entries);
    assert!(load_history(&config(&isolated).history_path.unwrap(), 1000).is_empty());
}
