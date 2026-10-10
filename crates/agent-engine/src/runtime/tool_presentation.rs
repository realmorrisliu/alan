//! Maps built-in tool calls and results into protocol presentation forms.
//!
//! Formatting lives here (the layer that understands tool arguments) so the TUI
//! can render a small set of presentation primitives without parsing any tool's
//! argument schema. Unknown/dynamic/MCP tools return `None`; the frontend then
//! falls back to the flat `result_preview`.

use alan_agent_protocol::{DiffHunk, DiffLine, ToolResultPresentation};
use serde_json::Value;

/// Populate the existing Action result envelope using the same mapper as Events.
pub(crate) fn write_action_metadata(
    envelope: &mut Value,
    name: &str,
    args: &Value,
    result: &Value,
) -> serde_json::Result<()> {
    let title_args = if matches!(name, "read_file" | "write_file" | "edit_file" | "list_dir")
        && result
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
    {
        result
    } else {
        args
    };
    if let Some(title) = tool_title(name, title_args) {
        envelope["title"] = Value::String(title);
    }
    if let Some(preview) = crate::runtime::turn_support::tool_result_preview(result) {
        envelope["result_preview"] = Value::String(preview);
    }
    if let Some(presentation) = tool_presentation(name, args, result) {
        envelope["presentation"] = serde_json::to_value(presentation)?;
    }
    Ok(())
}

/// Human-readable title for a tool call, shown as the tool header.
pub fn tool_title(name: &str, args: &Value) -> Option<String> {
    let path = args.get("path").and_then(Value::as_str);
    match name {
        "cd" => args
            .get("command")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "read_file" => path.map(|p| format!("Read {p}")),
        "write_file" => path.map(|p| format!("Write {p}")),
        "edit_file" => path.map(|p| format!("Edit {p}")),
        "bash" => Some(match args.get("command").and_then(Value::as_str) {
            Some(cmd) => format!("Bash {}", first_line(cmd)),
            None => "Bash".to_string(),
        }),
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
    if matches!(name, "edit_file" | "write_file")
        && (result.get("success").and_then(Value::as_bool) != Some(true)
            || result.get("error").is_some_and(|error| !error.is_null())
            || result
                .get("exit_code")
                .and_then(Value::as_i64)
                .is_some_and(|code| code != 0))
    {
        return None;
    }
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
            // Diagnostics without a Process result use the readable preview.
            let cmdline = args.get("command").and_then(Value::as_str)?;
            if result.get("exit_code").and_then(Value::as_i64).is_none()
                || (!result.get("stdout").is_some_and(Value::is_string)
                    && !result.get("stderr").is_some_and(Value::is_string))
            {
                return None;
            }
            let (stdout, stdout_truncated) =
                cap_text(result.get("stdout").and_then(Value::as_str).unwrap_or(""));
            let (stderr, stderr_truncated) =
                cap_text(result.get("stderr").and_then(Value::as_str).unwrap_or(""));
            Some(ToolResultPresentation::Command {
                cmdline: cmdline.to_string(),
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
/// Maximum characters carried per diff line (caps single huge/minified lines).
const PRESENTATION_MAX_LINE_CHARS: usize = 2_000;

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

/// Truncate a string to a char budget on a UTF-8 boundary, appending `…`.
fn truncate_chars(mut text: String, max: usize) -> String {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push('…');
    text
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

    // Cap each row's size (a single minified match line can be megabytes).
    let rows: Vec<String> = rows
        .into_iter()
        .map(|row| truncate_chars(row, PRESENTATION_MAX_LINE_CHARS))
        .collect();

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

/// Cap a diff presentation by both line count and per-line size, so a generated
/// or minified file (one very long line) cannot balloon the event.
fn cap_diff_lines(lines: &mut Vec<DiffLine>) {
    for line in lines.iter_mut() {
        let (DiffLine::Context { text } | DiffLine::Added { text } | DiffLine::Removed { text }) =
            line;
        if text.len() > PRESENTATION_MAX_LINE_CHARS {
            let mut end = PRESENTATION_MAX_LINE_CHARS;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push('…');
        }
    }
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
#[path = "tool_presentation_tests.rs"]
mod tests;
