use super::*;

#[tokio::test]
async fn reference_range_in_large_file_is_readable_and_nonrecoverable_is_honest() {
    let (shell, path, id) = action_fixture(
        &format!("{}RANGE sentinel{}", "x".repeat(300000), "y".repeat(300000)),
        "",
    )
    .await;
    let mut value = serde_json::json!({"type":"evidence_projection","preview":"preview","reference":{"path":format!("{path}/actions/{id}/output"),"offset":300000,"length":14},"truncation":{"full_content_recoverable":true,"original_bytes":14,"preview_bytes":7}});
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            value.to_string().as_bytes(),
        )
        .await
        .unwrap();
    let text = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("RANGE sentinel"), "{text}");
    value["truncation"]["full_content_recoverable"] = false.into();
    value["truncation"]["fallback_reason"] = "reference_unresolvable".into();
    shell
        .write(
            &format!("{path}/actions/{id}/result"),
            value.to_string().as_bytes(),
        )
        .await
        .unwrap();
    let text = action_detail_io::read_detail(&shell, &path, &id)
        .await
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("reference_unresolvable") && !text.contains("Recovered reference"),
        "{text}"
    );
}

#[tokio::test]
async fn full_diff_detail_keeps_late_hunk_style() {
    let lines = (0..65)
        .map(|i| serde_json::json!({"kind":"added","text":format!("  \tlate line {i}")}))
        .collect::<Vec<_>>();
    let result = serde_json::json!({"presentation":{"form":"diff","path":"src/file","hunks":[{"header":"@@ original @@","lines":lines}]}}).to_string();
    let (shell, path, id) = action_fixture("literal original", &result).await;
    let rows = action_detail_io::read_detail(&shell, &path, &id).await;
    assert!(
        rows.iter()
            .any(|row| row.to_string().contains("late line 64")
                && (row.style.fg == Some(ratatui::style::Color::Green)
                    || row
                        .spans
                        .iter()
                        .any(|s| s.style.fg == Some(ratatui::style::Color::Green)))),
        "{rows:?}"
    );
}
