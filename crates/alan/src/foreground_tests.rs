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

#[test]
fn connection_configured_tracks_root_model_presence() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let configured = config(&product);
    assert_eq!(configured.connection_configured, Some(true));

    let unconfigured = interactive_config(
        alan_ap::InProcessTransport::new(Arc::new(alan_kernel::MountFs::new(
            alan_kernel::Namespace::new(),
        ))),
        None,
        None,
        &product,
    )
    .unwrap();
    assert_eq!(unconfigured.connection_configured, Some(false));
}

#[test]
fn should_auto_mount_cwd_excludes_root_home_and_tmp_roots_but_allows_project_dirs() {
    assert!(!should_auto_mount_cwd(Path::new("/")));
    assert!(!should_auto_mount_cwd(Path::new("/tmp")));
    assert!(!should_auto_mount_cwd(&std::env::temp_dir()));
    if let Some(home) = dirs::home_dir() {
        assert!(!should_auto_mount_cwd(&home));
    }
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("my-project");
    std::fs::create_dir_all(&project).unwrap();
    assert!(should_auto_mount_cwd(&project));
    assert!(!should_auto_mount_cwd(&temp.path().join("missing-dir")));
}

#[test]
fn interactive_config_defaults_update_notice_to_none_and_passive_check_uses_system_store() {
    let dir = tempfile::tempdir().unwrap();
    let product = alan_os_host::SystemStorePaths::from_data_dir(dir.path()).unwrap();
    let cfg = config(&product);
    assert!(cfg.update_notice.is_none());
    assert!(cli::update::prepare_passive_tui_update_notice(&product, false).is_none());
    assert_eq!(
        cli::update::passive_check::passive_check_cache_path(&product).unwrap(),
        product.service("updater").unwrap().join("last-check.json")
    );
}
