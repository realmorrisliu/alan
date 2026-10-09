use super::*;

async fn complete_pending(shell: &alan_shell::Shell, app: &mut FileBackedApp) -> FileBackedEvent {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    action_detail_io::start_pending(shell, app, &tx);
    let catalog = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    app.dispatch(catalog);
    tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn reopen_reads_fresh_evidence_and_stale_async_results_are_rejected() {
    let (shell, path, id) = action_fixture("fresh sentinel", "").await;
    let mut app = FileBackedApp::new(path.clone());
    let open = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL);
    app.handle_key(open);
    let stale = complete_pending(&shell, &mut app).await;
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app.handle_key(open);
    app.dispatch(stale);
    assert!(app.modal.rows.is_empty());
    let response = complete_pending(&shell, &mut app).await;
    app.dispatch(response);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("fresh sentinel"))
    );
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            br#"{"type":"evidence_retention_expired","cause":"expired"}"#,
        )
        .await
        .unwrap();
    app.handle_key(open);
    let response = complete_pending(&shell, &mut app).await;
    app.dispatch(response);
    assert!(
        app.modal
            .rows
            .iter()
            .any(|r| r.to_string().contains("Evidence retention expired"))
    );
    assert!(app.transcript.is_empty());
    let response_path = app.agent_path.clone();
    let generation = app.modal.generation;
    app.reset_for_root_process_change();
    app.agent_path = "/agent/other".into();
    app.handle_key(open);
    app.dispatch(FileBackedEvent::ActionDetails {
        path: response_path.clone(),
        generation,
        actions: Ok(vec![action_detail_io::ActionEntry {
            owner: response_path.clone(),
            id: id.clone(),
        }]),
        selected: Some(action_detail_io::ActionEntry {
            owner: response_path,
            id,
        }),
        rows: vec![ratatui::text::Line::from("wrong process")],
    });
    assert!(app.modal.rows.is_empty());
}

#[tokio::test]
async fn missing_empty_redacted_and_display_bound_are_distinct() {
    let (shell, path, id) = action_fixture("", "[REDACTED reason=secret_key]").await;
    let text = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("(empty)") && text.contains("Redacted evidence (not truncation)"));
    let text = action_detail_io::read_detail(&shell, &path, "absent")
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("unavailable:") && !text.contains("(empty)"));
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            "x".repeat(262145).as_bytes(),
        )
        .await
        .unwrap();
    let text = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("display bound:"));
}

#[tokio::test]
async fn projection_reference_recovers_original_and_expiry_is_not_empty() {
    let (shell, path, id) = action_fixture("original retained sentinel", "").await;
    let projection = serde_json::json!({"type":"evidence_projection", "preview":"preview only", "reference":{"path":format!("{path}/actions/{id}/output"),"offset":0,"length":26},"truncation":{"full_content_recoverable":true}}).to_string();
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            projection.as_bytes(),
        )
        .await
        .unwrap();
    let rows = action_detail_io::read_detail(&shell, &path, &id).await;
    let text = rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Recovered reference") && text.contains("original retained sentinel"),
        "{text}"
    );
    let expiry = r#"{"type":"evidence_retention_expired","reference":"old","cause":"pruned"}"#;
    shell
        .write(&format!("{path}/actions/{id}/result"), expiry.as_bytes())
        .await
        .unwrap();
    let text = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Evidence retention expired"), "{text}");
}
