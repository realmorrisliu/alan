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
            Some(ToolResultPresentation::Diff {
                path,
                hunks: vec![DiffHunk {
                    header: Some("(new file)".to_string()),
                    lines: content
                        .lines()
                        .map(|line| DiffLine::Added {
                            text: line.to_string(),
                        })
                        .collect(),
                }],
            })
        }
        "read_file" => {
            let path = result.get("path").and_then(Value::as_str)?.to_string();
            let content = result.get("content").and_then(Value::as_str).unwrap_or("");
            Some(ToolResultPresentation::FileContent {
                path,
                lines: content.lines().count() as u64,
                truncated: result
                    .get("truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        "bash" => Some(ToolResultPresentation::Command {
            cmdline: args
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            exit_code: result
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| code as i32),
            stdout: result
                .get("stdout")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            stderr: result
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            truncated: result
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "grep" | "glob" | "list_dir" => Some(ToolResultPresentation::Listing {
            rows: listing_rows(result),
        }),
        // Dynamic/MCP/unknown tools: fall back to the flat preview.
        _ => None,
    }
}

fn result_path(result: &Value, args: &Value) -> Option<String> {
    result
        .get("path")
        .or_else(|| args.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn listing_rows(result: &Value) -> Vec<String> {
    let Some(matches) = result.get("matches").and_then(Value::as_array) else {
        return Vec::new();
    };
    matches
        .iter()
        .map(|entry| {
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
        })
        .collect()
}

/// A coarse line-level diff: removed `old` lines followed by added `new` lines.
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
    DiffHunk {
        header: None,
        lines,
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
    fn read_maps_to_file_content() {
        let p = tool_presentation(
            "read_file",
            &json!({"path": "a.rs"}),
            &json!({"path": "a.rs", "content": "l1\nl2\nl3"}),
        )
        .unwrap();
        match p {
            ToolResultPresentation::FileContent { lines, .. } => assert_eq!(lines, 3),
            _ => panic!("expected file content"),
        }
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
}
