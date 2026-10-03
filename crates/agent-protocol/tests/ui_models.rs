use alan_agent_protocol::UiModelSnapshot;

#[test]
fn model_observation_versions_are_process_local_and_restore_fields_fail_closed() {
    let first = UiModelSnapshot {
        known: true,
        process_path: "/proc/1".into(),
        publication_version: 2,
        ..Default::default()
    };
    let older = UiModelSnapshot {
        publication_version: 1,
        ..first.clone()
    };
    assert!(!older.supersedes(&first));
    let recovered = UiModelSnapshot {
        process_path: "/proc/2".into(),
        ..older
    };
    assert!(recovered.supersedes(&first));
    assert!(!UiModelSnapshot::default().supersedes(&first));
    let mut encoded = serde_json::to_value(first).unwrap();
    encoded["revision"] = "private-restore-evidence".into();
    assert!(serde_json::from_value::<UiModelSnapshot>(encoded).is_err());
}
