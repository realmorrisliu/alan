//! Turing Tape timeline formatting and syntax-inked rendering (`/tape` & `Ctrl+O` `t`).

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use serde_json::Value;

use crate::history::{HistoryCell, clean_text};
use crate::theme::Theme;

use super::action_detail_io;
use super::app::{FileBackedApp, FileBackedEvent};
use super::file_surface::TapeRecordV1;

/// Spawn an async read of `{owner_path}/machine/tape` and `{owner_path}/machine/checkpoints/current`.
pub(in crate::file_backed) fn start(
    shell: &alan_shell::Shell,
    app: &mut FileBackedApp,
    tx: &tokio::sync::mpsc::Sender<FileBackedEvent>,
    owner_path: &str,
) {
    app.modal.owner_path = owner_path.to_string();
    let fallback = build_tape_lines_from_app(app);
    if app.modal.rows.is_empty() {
        app.modal.rows = fallback.clone();
    }
    if owner_path == "/agent/root" {
        app.modal.rows = fallback;
        return;
    }
    let shell = shell.clone();
    let tx = tx.clone();
    let path = owner_path.to_string();
    let generation = app.modal.generation;
    let theme = app.theme;
    tokio::spawn(async move {
        let rows = load_tape_rows(&shell, &path, &fallback, theme).await;
        let _ = tx
            .send(FileBackedEvent::TapeDetails {
                path,
                generation,
                rows,
            })
            .await;
    });
}

/// Build a column-aligned Turing Tape timeline directly from the in-memory transcript/actions.
pub(in crate::file_backed) fn build_tape_lines_from_app(app: &FileBackedApp) -> Vec<Line<'static>> {
    let mut entries = Vec::new();
    for cell in &app.transcript {
        match cell {
            HistoryCell::User(text) => {
                entries.push(("[in]", format!("{:?}", single_line_summary(text))))
            }
            HistoryCell::Command(cmd) => {
                entries.push(("[in]", format!("! {}", single_line_summary(cmd))))
            }
            HistoryCell::InputTail { text, command, .. } => {
                let formatted = if *command {
                    format!("! {}", single_line_summary(text))
                } else {
                    format!("{:?}", single_line_summary(text))
                };
                entries.push(("[in]", formatted));
            }
            HistoryCell::Thinking { duration_secs, .. } => {
                entries.push(("[transition]", format!("reason -> {duration_secs}s")))
            }
            HistoryCell::Plan {
                snapshot, revision, ..
            } => entries.push((
                "[transition]",
                format!("plan#{revision} ({} steps)", snapshot.items.len()),
            )),
            HistoryCell::Tool { title, status, .. } => {
                let outcome = match status {
                    crate::history::ToolStatus::Complete => "completed",
                    crate::history::ToolStatus::Failed => "failed",
                    crate::history::ToolStatus::Rejected => "rejected",
                    crate::history::ToolStatus::Cancelled => "cancelled",
                };
                entries.push((
                    "[tool]",
                    format!("{} -> {outcome}", single_line_summary(title)),
                ));
            }
            HistoryCell::PendingYield(pending) => entries.push((
                "[transition]",
                format!(
                    "yield({}) -> {}",
                    pending.request_id,
                    single_line_summary(&pending.title)
                ),
            )),
            HistoryCell::Error(msg) => entries.push(("[error]", single_line_summary(msg))),
            HistoryCell::Assistant(text) | HistoryCell::AssistantTail { text, .. } => {
                entries.push(("[out]", format!("{:?}", single_line_summary(text))))
            }
            HistoryCell::Rendered(_) | HistoryCell::Styled(_) | HistoryCell::Welcome(_) => {}
        }
    }
    if let Some(usage) = app.usage.status() {
        entries.push(("[checkpoint]", usage));
    }
    if entries.is_empty() {
        return vec![Line::styled(
            "No Turing Tape entries recorded yet (submit a task or !command).",
            app.theme.muted_style(),
        )];
    }
    entries
        .into_iter()
        .enumerate()
        .map(|(idx, (tag, body))| {
            let formatted = format!("{:04}  {:<12} {}", idx + 1, tag, body);
            ink_tape_line(&formatted, app.theme)
        })
        .collect()
}

/// Read `/agent/<pid>/machine/tape` (and checkpoint marker) and return syntax-inked rows.
pub(in crate::file_backed) async fn load_tape_rows(
    shell: &alan_shell::Shell,
    owner_path: &str,
    fallback: &[Line<'static>],
    theme: Theme,
) -> Vec<Line<'static>> {
    let tape_raw = action_detail_io::field(shell, &format!("{owner_path}/machine/tape")).await;
    let ckpt_raw =
        action_detail_io::field(shell, &format!("{owner_path}/machine/checkpoints/current"))
            .await
            .ok();

    let Ok(raw) = tape_raw else {
        return fallback.to_vec();
    };
    let mut entries = parse_tape_entries(&raw);
    if entries.is_empty() {
        if !fallback.is_empty() {
            return fallback.to_vec();
        }
        return vec![Line::styled(
            format!("Tape empty at {owner_path}/machine/tape"),
            theme.muted_style(),
        )];
    }

    // Include intermediate [tool] and [transition] entries from live transcript when tape only has messages.
    let has_non_message = entries
        .iter()
        .any(|(tag, _)| *tag == "[tool]" || *tag == "[transition]");
    if !has_non_message && fallback.len() > entries.len() {
        return fallback.to_vec();
    }

    if let Some(ckpt) = ckpt_raw
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        && !entries
            .iter()
            .any(|(tag, body)| *tag == "[checkpoint]" && body.contains(&ckpt))
    {
        entries.push((
            "[checkpoint]",
            format!("{ckpt}  (persisted to System Store)"),
        ));
    }

    entries
        .into_iter()
        .enumerate()
        .map(|(idx, (tag, body))| {
            let formatted = format!("{:04}  {:<12} {}", idx + 1, tag, body);
            ink_tape_line(&formatted, theme)
        })
        .collect()
}

fn parse_tape_entries(raw: &str) -> Vec<(&'static str, String)> {
    let mut entries = Vec::new();
    for line in raw.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Ok(record) = serde_json::from_str::<TapeRecordV1>(line) {
            match record.kind.as_str() {
                "message" => {
                    let tag = match record.role.as_str() {
                        "user" => "[in]",
                        "assistant" => "[out]",
                        _ => "[transition]",
                    };
                    let body =
                        if record.input_intent == Some(alan_agent_protocol::InputIntent::Command) {
                            format!("! {}", single_line_summary(&record.content))
                        } else {
                            format!("{:?}", single_line_summary(&record.content))
                        };
                    entries.push((tag, body));
                }
                "tool" => entries.push(("[tool]", single_line_summary(&record.content))),
                "checkpoint" => {
                    entries.push(("[checkpoint]", single_line_summary(&record.content)))
                }
                "error" => entries.push(("[error]", single_line_summary(&record.content))),
                _ => entries.push(("[transition]", single_line_summary(&record.content))),
            }
            continue;
        }
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            let kind = val
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("transition");
            let tag = match kind {
                "in" | "input" => "[in]",
                "out" | "output" => "[out]",
                "tool" => "[tool]",
                "checkpoint" => "[checkpoint]",
                "error" => "[error]",
                _ => "[transition]",
            };
            let detail = val
                .get("summary")
                .or_else(|| val.get("content"))
                .or_else(|| val.get("checkpoint_id"))
                .and_then(Value::as_str)
                .map(single_line_summary)
                .unwrap_or_else(|| single_line_summary(line));
            entries.push((tag, detail));
            continue;
        }
        // Support plain pre-formatted tape lines (`0001  [in]  ...`) gracefully.
        let rest = line
            .split_once("  ")
            .filter(|(head, _)| head.chars().all(|ch| ch.is_ascii_digit()))
            .map_or(line, |(_, tail)| tail.trim_start());
        let (tag, body) = if let Some(b) = rest.strip_prefix("[in]") {
            ("[in]", b.trim())
        } else if let Some(b) = rest.strip_prefix("[out]") {
            ("[out]", b.trim())
        } else if let Some(b) = rest.strip_prefix("[tool]") {
            ("[tool]", b.trim())
        } else if let Some(b) = rest.strip_prefix("[checkpoint]") {
            ("[checkpoint]", b.trim())
        } else if let Some(b) = rest.strip_prefix("[error]") {
            ("[error]", b.trim())
        } else if let Some(b) = rest.strip_prefix("[transition]") {
            ("[transition]", b.trim())
        } else {
            ("[transition]", rest)
        };
        entries.push((tag, single_line_summary(body)));
    }
    entries
}

/// Syntax-ink a Turing Tape line: sequence numbers, `[tag]` tokens, and `/path/` tokens.
pub(in crate::file_backed) fn ink_tape_line(raw: &str, theme: Theme) -> Line<'static> {
    let cleaned = clean_text(raw);
    if theme.is_dumb {
        return Line::raw(cleaned);
    }
    let mut spans = Vec::new();
    let mut rest = cleaned.as_str();

    // Leading 4-digit sequence prefix (`0001  `)
    if let Some((seq, after)) = split_leading_seq(rest) {
        spans.push(Span::styled(seq.to_string(), theme.faint_style()));
        rest = after;
    }

    while !rest.is_empty() {
        if let Some((idx, tok, style)) = find_next_token(rest, theme) {
            if idx > 0 {
                spans.push(Span::raw(rest[..idx].to_string()));
            }
            spans.push(Span::styled(tok.to_string(), style));
            rest = &rest[idx + tok.len()..];
        } else {
            spans.push(Span::raw(rest.to_string()));
            break;
        }
    }
    Line::from(spans)
}

fn split_leading_seq(input: &str) -> Option<(&str, &str)> {
    let digits_end = input
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    if digits_end >= 4 {
        let after_digits = &input[digits_end..];
        let spaces = after_digits
            .char_indices()
            .take_while(|(_, ch)| *ch == ' ')
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        if spaces > 0 {
            let split = digits_end + spaces;
            return Some((&input[..split], &input[split..]));
        }
    }
    None
}

fn find_next_token(text: &str, theme: Theme) -> Option<(usize, &str, Style)> {
    let mut best: Option<(usize, &str, Style)> = None;
    for (tag, style) in [
        ("[in]", theme.accent_bold()),
        ("[out]", theme.accent_bold()),
        ("[tool]", theme.bold_if_capable(theme.run_style())),
        ("[checkpoint]", theme.bold_if_capable(theme.wait_style())),
        ("[transition]", theme.muted_style()),
        ("[error]", theme.bold_if_capable(theme.block_style())),
    ] {
        if let Some(idx) = text.find(tag)
            && best.as_ref().is_none_or(|(b_idx, _, _)| idx < *b_idx)
        {
            best = Some((idx, tag, style));
        }
    }
    if let Some((path_idx, path_len)) = find_namespace_path(text)
        && best.as_ref().is_none_or(|(b_idx, _, _)| path_idx < *b_idx)
    {
        best = Some((
            path_idx,
            &text[path_idx..path_idx + path_len],
            theme.accent_style(),
        ));
    }
    best
}

fn find_namespace_path(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    for (idx, b) in bytes.iter().enumerate() {
        if *b != b'/' {
            continue;
        }
        let valid_before = idx == 0
            || matches!(
                bytes[idx - 1],
                b' ' | b'(' | b'"' | b'\'' | b':' | b'[' | b'\t'
            );
        if !valid_before {
            continue;
        }
        let mut end = idx + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || matches!(bytes[end], b'/' | b'-' | b'_' | b'.'))
        {
            end += 1;
        }
        if end > idx + 1 {
            return Some((idx, end - idx));
        }
    }
    None
}

fn single_line_summary(text: &str) -> String {
    let clean = clean_text(text);
    let compact = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > 96 {
        let mut out: String = compact.chars().take(95).collect();
        out.push('…');
        out
    } else {
        compact
    }
}
