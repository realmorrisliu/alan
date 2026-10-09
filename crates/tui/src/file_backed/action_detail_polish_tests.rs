use super::*;

#[tokio::test]
async fn details_polish_direct_inline_structured_has_clean_output_label_and_raw_bytes() {
    let content = (1..=200)
        .map(|i| format!("inline{i}\n"))
        .collect::<String>();
    let raw = serde_json::json!({"type":"text","path":"inline-source","content":content,
        "total_lines":200,"start_line":1,"end_line":200,"truncated":false})
    .to_string();
    let result = serde_json::json!({"title":"Inline title",
        "result_preview":"duplicate inline preview sentinel"})
    .to_string();
    let (shell, path, id) = action_fixture(&raw, &result).await;
    let lines = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        lines
            .iter()
            .filter(|s| s.as_str() == "Original output")
            .count(),
        1
    );
    assert!(!lines.iter().any(|s| s == "Original raw Original output"));
    assert_eq!(
        lines
            .iter()
            .filter(|s| s.as_str() == "Original raw bytes")
            .count(),
        1
    );
    assert!(
        lines
            .iter()
            .any(|s| s == "Readable text: inline-source · lines 1–200 of 200")
    );
    for i in 1..=200 {
        assert_eq!(
            lines.iter().filter(|s| **s == format!("inline{i}")).count(),
            1
        );
    }
    assert!(
        !lines
            .iter()
            .any(|s| s == "duplicate inline preview sentinel")
    );
    assert!(lines.iter().any(|s| s == &raw));
    assert!(lines.iter().any(|s| s == &result));
    assert_eq!(
        shell
            .cat(&format!("{path}/actions/{id}/output"))
            .await
            .unwrap(),
        raw.as_bytes()
    );
}

#[tokio::test]
async fn details_polish_acquired_readable_omits_summary_and_metadata_duplicates() {
    let content = (1..=200)
        .map(|i| format!("readable{i}\n"))
        .collect::<String>();
    let raw = serde_json::json!({"type":"text","path":"source","content":content,
        "total_lines":200,"start_line":1,"end_line":200,"truncated":false})
    .to_string();
    for retained in [false, true] {
        let preview = "duplicate bounded summary sentinel";
        let mut result = if retained {
            serde_json::json!({"type":"evidence_projection","preview":preview,
                "truncation":{"full_content_recoverable":true,"original_bytes":raw.len(),"preview_bytes":preview.len()}})
        } else {
            serde_json::json!({})
        };
        result["title"] = "Read title".into();
        result["result_preview"] = preview.into();
        let (shell, path, id) = action_fixture(&raw, "").await;
        if retained {
            result["reference"] = serde_json::json!({"path":format!("{path}/actions/{id}/output"),"offset":0,"length":raw.len()});
        }
        let metadata = result.to_string();
        shell
            .write(&format!("{path}/actions/{id}/result"), metadata.as_bytes())
            .await
            .unwrap();
        let lines = action_detail_io::read_detail(&shell, &path, &id)
            .await
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            lines.iter().filter(|s| s.as_str() == preview).count(),
            0,
            "duplicate summary before acquired Readable200"
        );
        assert!(lines.iter().any(|s| s == "Read title"));
        assert!(lines.iter().any(|s| s == "readable200"));
        for label in ["Name", "Status", "Original output", "Result metadata"] {
            assert_eq!(lines.iter().filter(|s| s.as_str() == label).count(), 1);
            assert!(!lines.iter().any(|s| s == &format!("Original raw {label}")));
        }
        assert_eq!(
            lines
                .iter()
                .filter(|s| s.as_str() == "Original raw bytes")
                .count(),
            if retained { 2 } else { 1 }
        );
        assert!(lines.iter().any(|s| s == &raw));
        assert!(lines.iter().any(|s| s == &metadata));
        assert_eq!(
            shell
                .cat(&format!("{path}/actions/{id}/output"))
                .await
                .unwrap(),
            raw.as_bytes()
        );
    }
}

#[tokio::test]
async fn details_polish_fallback_retains_labelled_preview_and_reason() {
    let (shell, path, id) = action_fixture("", "").await;
    for (reference, truncation, reason) in [
        (
            serde_json::json!({"path":"/absent"}),
            serde_json::json!({"full_content_recoverable":true}),
            "unavailable",
        ),
        (
            serde_json::json!({}),
            serde_json::json!({"full_content_recoverable":false,"fallback_reason":"expired"}),
            "expired",
        ),
        (
            serde_json::json!({"path":format!("{path}/actions/{id}/output"),"offset":0,"length":262145}),
            serde_json::json!({"full_content_recoverable":true}),
            "display bound",
        ),
        (
            serde_json::json!({"path":format!("{path}/actions/{id}/output"),"offset":"invalid"}),
            serde_json::json!({"full_content_recoverable":true}),
            "invalid range",
        ),
    ] {
        let result = serde_json::json!({"type":"evidence_projection","title":"Fallback title","preview":"bounded sentinel","result_preview":"bounded sentinel","reference":reference,"truncation":truncation}).to_string();
        shell
            .write(&format!("{path}/actions/{id}/result"), result.as_bytes())
            .await
            .unwrap();
        let text = action_detail_io::read_detail(&shell, &path, &id)
            .await
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Bounded preview")
                && text.contains("bounded sentinel")
                && text.contains(reason),
            "{text}"
        );
    }
}

#[tokio::test]
async fn details_polish_newest_numeric_default_navigation_and_refresh() {
    let (shell, root, _, pid) = live_root_agent().await;
    let path = format!("/agent/{pid}");
    for _ in 0..11 {
        super::async_tests::allocate(&shell, &root, &path, "evidence").await;
    }
    let mut app = FileBackedApp::new(path);
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    async fn load(shell: &alan_shell::Shell, app: &mut FileBackedApp) {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        action_detail_io::start_pending(shell, app, &tx);
        for _ in 0..2 {
            app.dispatch(
                tokio::time::timeout(Duration::from_secs(2), rx.recv())
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
    }
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].id, "a10");
    assert_eq!(
        app.modal.actions[..2]
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["a0", "a1"]
    );
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].id, "a9");
    app.modal.pending = true;
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].id, "a9");
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    load(&shell, &mut app).await;
    assert_eq!(app.modal.actions[app.modal.selected].id, "a10");
}
