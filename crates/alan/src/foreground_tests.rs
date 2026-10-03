use super::*;
use alan_agent_protocol::InputIntent;
use alan_tui::composer::{Composer, HistoryEntry, load_history};

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
fn foreground_history_is_channel_system_store_owned_and_restarts_typed() {
    let dir = tempfile::tempdir().unwrap();
    let stable = alan_os_host::SystemStorePaths::from_data_dir(dir.path(), "stable").unwrap();
    let dev = alan_os_host::SystemStorePaths::from_data_dir(dir.path(), "dev").unwrap();
    let first = config(&stable);
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
        stable.service("shell-ui").unwrap().join("composer-history")
    );
    assert_ne!(Some(path.clone()), config(&dev).history_path);
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
    let restarted_path = config(&stable).history_path.unwrap();
    assert_eq!(load_history(&restarted_path, 1000), entries);
    assert!(load_history(&config(&dev).history_path.unwrap(), 1000).is_empty());
}
