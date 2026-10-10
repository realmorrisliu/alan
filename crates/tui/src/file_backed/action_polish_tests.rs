use super::*;

#[test]
fn polish_partial_delimiter_drain_keeps_remaining_slots_and_late_language() {
    let mut cell = HistoryCell::Assistant("```rust".into());
    let opts = RenderOpts::new(16, false);
    cell.trim_rendered_prefix(opts, 2);
    cell.replace_assistant_source("```rust_extended_language界\n\tcode\t".into());
    let before = cell.render_lines(opts);
    assert!(before.concat().starts_with("_extended_language界"));
    cell.trim_rendered_prefix(opts, 1);
    assert_eq!(cell.render_lines(opts), before[1..]);
    let after = cell.render_lines(opts);
    cell.replace_assistant_source("```rust_extended_language界\n\tcode\t\n```".into());
    assert_eq!(&cell.render_lines(opts)[..after.len()], after.as_slice());
    assert_eq!(cell.render_lines(opts).last().unwrap(), "╰──");
}

#[tokio::test]
async fn polish_partial_json_reference_does_not_manufacture_structured_content() {
    let raw = r#"{"type":"text","path":"a","content":"line200","total_lines":200,"start_line":200,"end_line":200,"truncated":true}"#;
    let (shell, path, id) = action_fixture(raw, "").await;
    let projection = serde_json::json!({"type":"evidence_projection","preview":"preview only",
        "reference":{"path":format!("{path}/actions/{id}/output"),"offset":1,"length":raw.len()-1},
        "truncation":{"full_content_recoverable":true,"original_bytes":raw.len()-1,"preview_bytes":12}}).to_string();
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            projection.as_bytes(),
        )
        .await
        .unwrap();
    let lines = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let recovered = lines
        .iter()
        .position(|s| s.contains("Recovered reference range only"))
        .unwrap();
    assert!(
        !lines[recovered..]
            .iter()
            .any(|s| s.starts_with("Readable text:"))
    );
    assert!(
        !lines
            .iter()
            .any(|s| s == "Recovered reference (retained original)")
    );
    assert!(
        lines
            .iter()
            .any(|s| s.contains("lines 200–200 of 200 · truncated"))
    );
}

#[test]
fn polish_multiline_summary_does_not_join_physical_lines() {
    let cell = HistoryCell::Tool {
        action: None,
        title: "Read".into(),
        status: ToolStatus::Complete,
        preview: Some("\nfirst line\nsecond unrelated line".into()),
        presentation: None,
    };
    let rows = crate::history::action_summary(&cell, 73);
    assert_eq!(rows[1].to_string(), "  first line");
}

#[tokio::test]
async fn polish_structured_direct_and_retained_read_keep_readable_and_original() {
    let content = (1..=200).map(|i| format!("line{i}\n")).collect::<String>();
    let raw = serde_json::json!({"type":"text","path":"src/界.rs","content":content,
        "total_lines":200,"start_line":1,"end_line":200,"truncated":false})
    .to_string();
    for retained in [false, true] {
        let (shell, path, id) = action_fixture(
            if retained { &raw } else { "" },
            if retained { "" } else { &raw },
        )
        .await;
        if retained {
            let projection = serde_json::json!({"type":"evidence_projection","preview":"preview only",
                "reference":{"path":format!("{path}/actions/{id}/output"),"offset":0,"length":raw.len()},
                "truncation":{"full_content_recoverable":true,"original_bytes":raw.len(),"preview_bytes":12}}).to_string();
            shell
                .write(
                    &format!("{path}/actions/{id}/result"),
                    projection.as_bytes(),
                )
                .await
                .unwrap();
        }
        let rows = action_detail_io::read_detail(&shell, &path, &id).await;
        let lines = rows.iter().map(ToString::to_string).collect::<Vec<_>>();
        let readable = if retained {
            let recovered = lines
                .iter()
                .position(|line| line.starts_with("Recovered reference"))
                .unwrap();
            &lines[recovered..]
        } else {
            &lines[..]
        };
        assert!(
            readable.iter().any(|line| line == "line200"),
            "retained={retained}: no readable last line"
        );
        assert!(lines.iter().any(|line| line.contains("lines 1–200 of 200")));
        assert!(
            lines.iter().any(|line| line == &raw),
            "original separately accessible"
        );
        assert!(lines.iter().any(|line| line.starts_with("Original raw")));
    }
}

#[test]
fn polish_fence_language_boundary_stream_resize_and_second_drain() {
    for language in ["rust", "diff"] {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.push_output("```".into());
        for ch in language.chars() {
            app.push_output(ch.to_string());
        }
        app.push_output(format!("\n\t界{}\n+next\t", "x".repeat(100)));
        let before = app.styled_history_lines(73);
        assert_eq!(before[1].to_string(), format!("╭─ {language}"));
        assert_eq!(app.prune_rendered_prefix(app.render_opts(73), 3), 3);
        let resized = app.styled_history_lines(40);
        assert!(!resized.is_empty());
        assert_eq!(app.prune_rendered_prefix(app.render_opts(40), 1), 1);
        let tail = app.styled_history_lines(40);
        let source = format!("```{language}\n\t界{}\n+next\t\n```", "x".repeat(100));
        app.push_output("\n```".into());
        let closed = app.styled_history_lines(40);
        assert_eq!(closed.last().unwrap().to_string(), "╰──");
        assert_eq!(&closed[..closed.len() - 1], tail.as_slice());
        app.apply_tape_record(super::super::tests::tape_message("assistant", &source));
        assert_eq!(app.styled_history_lines(40), closed);
        app.merge_reconnected_idle_history(vec![HistoryCell::Assistant(source)]);
        assert_eq!(app.styled_history_lines(40), closed);
    }
}

#[test]
fn answer_spacing_and_fence_label_commit_once_before_late_code() {
    let opts = RenderOpts::new(24, false);
    let mut cell = HistoryCell::Assistant("```rust".into());
    assert_eq!(cell.render_lines(opts), ["", "╭─ rust"]);
    assert!(cell.trim_rendered_prefix(opts, 1));
    assert_eq!(cell.render_lines(opts), ["╭─ rust"]);
    assert!(cell.trim_rendered_prefix(opts, 1));
    cell.replace_assistant_source("```rust\n    let 界 = 1;\n```".into());
    assert_eq!(cell.render_lines(opts), ["    let 界 = 1;", "╰──"]);
    assert_eq!(
        cell.render_lines(RenderOpts::new(40, false)),
        cell.render_lines(opts)
    );
}
