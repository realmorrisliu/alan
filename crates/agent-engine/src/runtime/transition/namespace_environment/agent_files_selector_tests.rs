use super::*;

#[test]
fn selector_envelope_preserves_semantic_rejection_identity() {
    let id = uuid::Uuid::new_v4();
    for path in [
        "/mnt/new",
        "/",
        "/mnt/../project",
        "relative",
        "",
        "/mnt//project",
        "/mnt/./project",
        "/mnt/\nproject",
    ] {
        let command = format!(
            "project-cwd-v1 {}",
            serde_json::json!({"id":id,"path":path})
        );
        let submission = machine_control_submission(&command).unwrap();
        assert_eq!(submission.id, id.to_string());
        assert_eq!(submission.intent, alan_agent_protocol::InputIntent::Agent);
        assert!(
            matches!(submission.op, Op::SelectProjectDirectory { path: selected } if selected == path)
        );
        assert_eq!(
            valid_project_directory(path),
            matches!(path, "/mnt/new" | "/")
        );
    }
}

#[test]
fn selector_envelope_rejects_malformed_unknown_fields_and_size_overflow() {
    let id = uuid::Uuid::new_v4();
    for json in [
        "{".to_owned(),
        serde_json::json!({"id":"invalid","path":"/mnt/new"}).to_string(),
        serde_json::json!({"id":id,"path":42}).to_string(),
        serde_json::json!({"id":id}).to_string(),
        serde_json::json!({"id":id,"path":"/mnt/new","extra":true}).to_string(),
        serde_json::json!({"id":id,"path":format!("/{}", "x".repeat(4096))}).to_string(),
        format!(
            "{}{}",
            " ".repeat(8192),
            serde_json::json!({"id":id,"path":"/mnt/new"})
        ),
    ] {
        assert!(machine_control_submission(&format!("project-cwd-v1 {json}")).is_none());
    }
    let path = format!("/{}", "x".repeat(4095));
    assert!(
        machine_control_submission(&format!(
            "project-cwd-v1 {}",
            serde_json::json!({"id":id,"path":path})
        ))
        .is_some()
    );
}

#[test]
fn owner_work_control_retains_identity_and_rejects_invalid_descriptors() {
    let id = uuid::Uuid::new_v4();
    let control = serde_json::json!({"id":id,"request":{"version":1,
        "question":"Who owns fids?","evaluator_profile":"eval",
        "candidates":[{"id":"hostfs","sources":[{"path":"/mnt/source/lib.rs",
            "start_line":1,"end_line":2}]}]}});
    let submission = machine_control_submission(&format!("owner-work-v1 {control}")).unwrap();
    assert_eq!(submission.id, id.to_string());
    assert_eq!(
        submission.intent,
        alan_agent_protocol::InputIntent::ForceAgent
    );
    assert!(matches!(
        submission.op,
        Op::Input {
            mode: InputMode::FollowUp,
            ..
        }
    ));
    for field in ["extra", "id", "request"] {
        let mut malformed = control.clone();
        malformed[field] = serde_json::json!(true);
        assert!(machine_control_submission(&format!("owner-work-v1 {malformed}")).is_none());
    }
    assert!(
        machine_control_submission(&format!("owner-work-v1 {}{control}", " ".repeat(64 * 1024)))
            .is_none()
    );
}
