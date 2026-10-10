use super::*;
use serde_json::json;

#[test]
fn titles_format_from_args() {
    assert_eq!(
        tool_title("read_file", &json!({"path": "src/a.rs"})).as_deref(),
        Some("Read src/a.rs")
    );
    assert_eq!(
        tool_title("bash", &json!({"command": "cargo test\nmore"})).as_deref(),
        Some("Bash cargo test")
    );
    assert!(tool_title("mcp_custom", &json!({})).is_none());
}

#[test]
fn completed_action_titles_use_the_result_path_without_reinterpreting_other_tools() {
    let args = json!({"path":"sample.rs", "old_string":"a + b", "new_string":"a - b",
        "content":"source", "command":"printf marker"});
    let result = json!({"path":"/mnt/project-request-2/sample.rs", "success":true});
    for (name, verb) in [
        ("read_file", "Read"),
        ("write_file", "Write"),
        ("edit_file", "Edit"),
        ("list_dir", "List"),
    ] {
        let mut envelope = json!({});
        write_action_metadata(&mut envelope, name, &args, &result).unwrap();
        assert_eq!(
            envelope["title"],
            format!("{verb} /mnt/project-request-2/sample.rs")
        );
        for result in [json!({}), json!({"path":""}), json!({"path":3})] {
            let mut envelope = json!({});
            write_action_metadata(&mut envelope, name, &args, &result).unwrap();
            assert_eq!(envelope["title"], format!("{verb} sample.rs"));
        }
    }
    let mut envelope = json!({});
    write_action_metadata(&mut envelope, "bash", &args, &result).unwrap();
    assert_eq!(envelope["title"], "Bash printf marker");
    let mut envelope = json!({});
    write_action_metadata(&mut envelope, "mcp_custom", &args, &result).unwrap();
    assert!(envelope.get("title").is_none());
}

#[test]
fn edit_maps_to_diff() {
    let p = tool_presentation(
        "edit_file",
        &json!({"path": "a.rs", "old_string": "old", "new_string": "new"}),
        &json!({"path": "a.rs", "success": true}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Diff { path, hunks } => {
            assert_eq!(path, "a.rs");
            assert_eq!(hunks[0].lines.len(), 2);
        }
        _ => panic!("expected diff"),
    }
}

#[test]
fn bash_maps_to_command() {
    let p = tool_presentation(
        "bash",
        &json!({"command": "ls"}),
        &json!({"stdout": "a\nb", "exit_code": 0}),
    )
    .unwrap();
    assert!(matches!(
        p,
        ToolResultPresentation::Command {
            exit_code: Some(0),
            ..
        }
    ));
}

#[test]
fn read_file_uses_preview_to_keep_contents_visible() {
    // No presentation → the content-bearing flat preview renders instead of
    // hiding the file behind a path + line count.
    assert!(
        tool_presentation(
            "read_file",
            &json!({"path": "a.rs"}),
            &json!({"path": "a.rs", "content": "l1\nl2\nl3"}),
        )
        .is_none()
    );
}

#[test]
fn grep_maps_to_listing() {
    let p = tool_presentation(
        "grep",
        &json!({"pattern": "x"}),
        &json!({"matches": [{"path": "a.rs", "line": 4, "content": "x here"}]}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Listing { rows } => {
            assert_eq!(rows, vec!["a.rs:4: x here".to_string()])
        }
        _ => panic!("expected listing"),
    }
}

#[test]
fn unknown_tool_has_no_presentation() {
    assert!(tool_presentation("mcp_custom", &json!({}), &json!({"ok": true})).is_none());
}

#[test]
fn list_dir_maps_entries_to_rows() {
    let p = tool_presentation(
        "list_dir",
        &json!({"path": "."}),
        &json!({"path": ".", "entries": [
                {"name": "src", "type": "directory", "size": 0},
                {"name": "Cargo.toml", "type": "file", "size": 12}
            ], "total": 2}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Listing { rows } => {
            assert_eq!(rows, vec!["src/".to_string(), "Cargo.toml".to_string()]);
        }
        _ => panic!("expected listing"),
    }
}

#[test]
fn huge_listing_row_is_capped() {
    let huge = "x".repeat(PRESENTATION_MAX_LINE_CHARS * 4);
    let p = tool_presentation(
        "grep",
        &json!({"pattern": "x"}),
        &json!({"matches": [{"path": "min.js", "line": 1, "content": huge}]}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Listing { rows } => {
            assert_eq!(rows.len(), 1);
            assert!(rows[0].len() <= PRESENTATION_MAX_LINE_CHARS + 64);
        }
        _ => panic!("expected listing"),
    }
}

#[test]
fn empty_listing_falls_back_to_preview() {
    // No matches/entries → no presentation, so the flat preview renders.
    assert!(
        tool_presentation("list_dir", &json!({"path": "."}), &json!({"entries": []})).is_none()
    );
    assert!(tool_presentation("grep", &json!({"pattern": "x"}), &json!({"matches": []})).is_none());
}

#[test]
fn bash_caps_large_stdout() {
    let huge = "x".repeat(PRESENTATION_MAX_STREAM_CHARS * 2);
    let p = tool_presentation(
        "bash",
        &json!({"command": "gen"}),
        &json!({"stdout": huge, "exit_code": 0}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Command {
            stdout, truncated, ..
        } => {
            assert!(truncated);
            assert!(stdout.len() < PRESENTATION_MAX_STREAM_CHARS + 100);
            assert!(stdout.contains("output truncated"));
        }
        _ => panic!("expected command"),
    }
}

#[test]
fn single_huge_diff_line_is_capped_on_char_boundary() {
    // One very long line of multi-byte chars must be capped without panicking.
    let huge = "界".repeat(PRESENTATION_MAX_LINE_CHARS);
    let p = tool_presentation(
        "write_file",
        &json!({"path": "min.js", "content": huge}),
        &json!({"path": "min.js", "success": true}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Diff { hunks, .. } => {
            let DiffLine::Added { text } = &hunks[0].lines[0] else {
                panic!("expected added line");
            };
            assert!(text.len() <= PRESENTATION_MAX_LINE_CHARS + 4);
        }
        _ => panic!("expected diff"),
    }
}

#[test]
fn large_diff_is_capped() {
    let new = (0..PRESENTATION_MAX_ROWS + 500)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let p = tool_presentation(
        "write_file",
        &json!({"path": "a.rs", "content": new}),
        &json!({"path": "a.rs", "success": true}),
    )
    .unwrap();
    match p {
        ToolResultPresentation::Diff { hunks, .. } => {
            assert!(hunks[0].lines.len() <= PRESENTATION_MAX_ROWS + 1);
        }
        _ => panic!("expected diff"),
    }
}
