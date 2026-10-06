use alan_agent_protocol::UiSkillSnapshot;

#[test]
fn unknown_snapshot_is_safe_and_rejects_private_fields() {
    let unknown = UiSkillSnapshot::default();
    assert!(unknown.is_valid());
    let bytes = serde_json::to_vec(&unknown).unwrap();
    assert_eq!(
        serde_json::from_slice::<UiSkillSnapshot>(&bytes).unwrap(),
        unknown
    );
    assert!(serde_json::from_str::<UiSkillSnapshot>(r#"{"version":1,"publication_version":1,"process_path":"/proc/1","known":true,"mentionable_skill_ids":[],"host_path":"private"}"#).is_err());
}

#[test]
fn known_requires_process_and_revision_unknown_clears_ids() {
    let mut snapshot = UiSkillSnapshot {
        known: true,
        mentionable_skill_ids: vec!["explicit-only".into()],
        ..Default::default()
    };
    assert!(!snapshot.is_valid());
    snapshot.process_path = "/proc/42".into();
    snapshot.publication_version = 1;
    assert!(snapshot.is_valid());
    let unknown = snapshot.unknown();
    assert!(unknown.is_valid());
    assert!(unknown.mentionable_skill_ids.is_empty());
}
