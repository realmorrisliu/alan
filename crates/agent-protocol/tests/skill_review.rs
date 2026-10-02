use alan_agent_protocol::UiSkillSnapshot;
#[test]
fn review_rejects_unsafe_wire_authority() {
    for path in ["/proc/", "/proc/0", "/proc/no", "/proc/1/extra"] {
        let snapshot = UiSkillSnapshot {
            publication_version: 1,
            process_path: path.into(),
            known: true,
            ..Default::default()
        };
        assert!(!snapshot.is_valid(), "accepted {path}");
    }
    for id in [
        "",
        "../private",
        "/Users/private",
        "bad\nsecret",
        "C:\\private",
    ] {
        let snapshot = UiSkillSnapshot {
            publication_version: 1,
            process_path: "/proc/1".into(),
            known: true,
            mentionable_skill_ids: vec![id.into()],
            ..Default::default()
        };
        assert!(!snapshot.is_valid(), "accepted {id:?}");
    }
}
