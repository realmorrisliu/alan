//! Maps built-in tool calls and results into protocol presentation forms.
//!
//! Formatting lives here (the layer that understands tool arguments) so the TUI
//! can render a small set of presentation primitives without parsing any tool's
//! argument schema. Unknown/dynamic/MCP tools return `None`; the frontend then
//! falls back to the flat `result_preview`.

use alan_protocol::{DiffHunk, DiffLine, ToolResultPresentation};
use serde_json::Value;

/// Human-readable title for a tool call, shown as the tool header.
pub fn tool_title(name: &str, args: &Value) -> Option<String> {
    let path = args.get("path").and_then(Value::as_str);
    match name {
        "read_file" => path.map(|p| format!("Read {p}")),
        "write_file" => path.map(|p| format!("Write {p}")),
        "edit_file" => path.map(|p| format!("Edit {p}")),
        "bash" => args
            .get("command")
            .and_then(Value::as_str)
            .map(|cmd| format!("Bash {}", first_line(cmd))),
        "grep" => args
            .get("pattern")
            .and_then(Value::as_str)
            .map(|pattern| format!("Grep {pattern}")),
        "glob" => args
            .get("pattern")
            .and_then(Value::as_str)
            .map(|pattern| format!("Glob {pattern}")),
        "list_dir" => Some(match path {
            Some(p) => format!("List {p}"),
            None => "List".to_string(),
        }),
        _ => None,
    }
}

/// Structured presentation for a completed tool call, or `None` to use the preview.
pub fn tool_presentation(
    name: &str,
    args: &Value,
    result: &Value,
) -> Option<ToolResultPresentation> {
    match name {
        "edit_file" => {
            let path = result_path(result, args)?;
            let old = args.get("old_string").and_then(Value::as_str).unwrap_or("");
            let new = args.get("new_string").and_then(Value::as_str).unwrap_or("");
            Some(ToolResultPresentation::Diff {
                path,
                hunks: vec![line_diff(old, new)],
            })
        }
        "write_file" => {
            let path = result_path(result, args)?;
            let content = args.get("content").and_then(Value::as_str).unwrap_or("");
            let mut lines: Vec<DiffLine> = content
                .lines()
                .map(|line| DiffLine::Added {
                    text: line.to_string(),
                })
                .collect();
            cap_diff_lines(&mut lines);
            Some(ToolResultPresentation::Diff {
                path,
                hunks: vec![DiffHunk {
                    header: Some("(new file)".to_string()),
                    lines,
                }],
            })
        }
        // read_file: the `FileContent` form carries only a path + line count, so
        // emitting it would hide the actual contents (the TUI prefers a
        // presentation over the preview). Return None so the content-bearing flat
        // preview renders instead.
        "read_file" => None,
        "bash" => {
            let (stdout, stdout_truncated) =
                cap_text(result.get("stdout").and_then(Value::as_str).unwrap_or(""));
            let (stderr, stderr_truncated) =
                cap_text(result.get("stderr").and_then(Value::as_str).unwrap_or(""));
            Some(ToolResultPresentation::Command {
                cmdline: args
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                exit_code: result
                    .get("exit_code")
                    .and_then(Value::as_i64)
                    .map(|code| code as i32),
                stdout,
                stderr,
                truncated: stdout_truncated
                    || stderr_truncated
                    || result
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
            })
        }
        "grep" | "glob" | "list_dir" => {
            let rows = listing_rows(result);
            // An empty listing carries no information the preview lacks; let the
            // flat preview render instead of an empty tool body.
            if rows.is_empty() {
                None
            } else {
                Some(ToolResultPresentation::Listing { rows })
            }
        }
        // Dynamic/MCP/unknown tools: fall back to the flat preview.
        _ => None,
    }
}

/// Maximum characters carried per text stream in a presentation payload, so a
/// single tool event cannot balloon to megabytes over the wire.
const PRESENTATION_MAX_STREAM_CHARS: usize = 16_000;
/// Maximum rows carried in a `Listing` / lines in a `Diff` presentation.
const PRESENTATION_MAX_ROWS: usize = 1_000;

/// Cap a text stream to a byte budget (on a char boundary). Returns the capped
/// text and whether it was truncated.
fn cap_text(text: &str) -> (String, bool) {
    if text.len() <= PRESENTATION_MAX_STREAM_CHARS {
        return (text.to_string(), false);
    }
    let mut end = PRESENTATION_MAX_STREAM_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (format!("{}\n… (output truncated)", &text[..end]), true)
}

fn result_path(result: &Value, args: &Value) -> Option<String> {
    result
        .get("path")
        .or_else(|| args.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn listing_rows(result: &Value) -> Vec<String> {
    // grep/glob use `matches`; list_dir uses `entries` ({name, type, size}).
    let rows: Vec<String> = if let Some(matches) = result.get("matches").and_then(Value::as_array) {
        matches.iter().map(match_row).collect()
    } else if let Some(entries) = result.get("entries").and_then(Value::as_array) {
        entries.iter().map(entry_row).collect()
    } else {
        Vec::new()
    };

    if rows.len() > PRESENTATION_MAX_ROWS {
        let hidden = rows.len() - PRESENTATION_MAX_ROWS;
        let mut capped: Vec<String> = rows.into_iter().take(PRESENTATION_MAX_ROWS).collect();
        capped.push(format!("… (+{hidden} more)"));
        capped
    } else {
        rows
    }
}

fn match_row(entry: &Value) -> String {
    if let Some(text) = entry.as_str() {
        return text.to_string();
    }
    let path = entry.get("path").and_then(Value::as_str).unwrap_or("");
    let content = entry.get("content").and_then(Value::as_str);
    match (entry.get("line").and_then(Value::as_i64), content) {
        (Some(line), Some(content)) => format!("{path}:{line}: {content}"),
        (Some(line), None) => format!("{path}:{line}"),
        (None, Some(content)) => format!("{path}: {content}"),
        (None, None) => path.to_string(),
    }
}

fn entry_row(entry: &Value) -> String {
    if let Some(text) = entry.as_str() {
        return text.to_string();
    }
    let name = entry.get("name").and_then(Value::as_str).unwrap_or("");
    if entry.get("type").and_then(Value::as_str) == Some("directory") {
        format!("{name}/")
    } else {
        name.to_string()
    }
}

/// A coarse line-level diff: removed `old` lines followed by added `new` lines,
/// capped so a huge edit cannot balloon the event.
fn line_diff(old: &str, new: &str) -> DiffHunk {
    let mut lines = Vec::new();
    for line in old.lines() {
        lines.push(DiffLine::Removed {
            text: line.to_string(),
        });
    }
    for line in new.lines() {
        lines.push(DiffLine::Added {
            text: line.to_string(),
        });
    }
    cap_diff_lines(&mut lines);
    DiffHunk {
        header: None,
        lines,
    }
}

/// Cap the number of diff lines carried in a presentation.
fn cap_diff_lines(lines: &mut Vec<DiffLine>) {
    if lines.len() > PRESENTATION_MAX_ROWS {
        let hidden = lines.len() - PRESENTATION_MAX_ROWS;
        lines.truncate(PRESENTATION_MAX_ROWS);
        lines.push(DiffLine::Context {
            text: format!("… (+{hidden} more lines)"),
        });
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or(text)
}

#[cfg(test)]
mod tests {
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
    fn edit_maps_to_diff() {
        let p = tool_presentation(
            "edit_file",
            &json!({"path": "a.rs", "old_string": "old", "new_string": "new"}),
            &json!({"path": "a.rs"}),
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
    fn empty_listing_falls_back_to_preview() {
        // No matches/entries → no presentation, so the flat preview renders.
        assert!(
            tool_presentation("list_dir", &json!({"path": "."}), &json!({"entries": []})).is_none()
        );
        assert!(
            tool_presentation("grep", &json!({"pattern": "x"}), &json!({"matches": []})).is_none()
        );
    }

    #[test]
    fn bash_caps_large_stdout() {
        let huge = "x".repeat(PRESENTATION_MAX_STREAM_CHARS * 2);
        let p = tool_presentation("bash", &json!({"command": "gen"}), &json!({"stdout": huge}))
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
    fn large_diff_is_capped() {
        let new = (0..PRESENTATION_MAX_ROWS + 500)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let p = tool_presentation(
            "write_file",
            &json!({"path": "a.rs", "content": new}),
            &json!({"path": "a.rs"}),
        )
        .unwrap();
        match p {
            ToolResultPresentation::Diff { hunks, .. } => {
                assert!(hunks[0].lines.len() <= PRESENTATION_MAX_ROWS + 1);
            }
            _ => panic!("expected diff"),
        }
    }
}
