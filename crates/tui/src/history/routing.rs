use ratatui::text::{Line, Span};

use super::{clean_text, metadata_style, wrap_styled_lines};
use crate::theme::Theme;

/// Render a calm inline routing/handoff/focus receipt wrapped for `width`.
pub(super) fn render_routing_receipt_lines(
    source: &str,
    target: &str,
    kind: &str,
    summary: Option<&str>,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let lead = if theme.is_dumb { "> " } else { "↳ " };
    let arrow = if theme.is_dumb { "->" } else { "→" };
    let clean_source = collapse_whitespace(source);
    let clean_target = collapse_whitespace(target);
    let clean_summary = summary
        .map(collapse_whitespace)
        .filter(|text| !text.is_empty());

    let (verb_style, bracket_style) = if kind == "dead_letter" {
        (theme.wait_style(), theme.wait_style())
    } else {
        (metadata_style(theme), theme.accent_style())
    };

    let mut spans = vec![Span::styled(lead, theme.muted_style())];
    match kind {
        "focus" => {
            spans.push(Span::styled("focused ", verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
        "reroute_request" | "reroute" => {
            spans.push(Span::styled("rerouted from ", verb_style));
            spans.push(Span::styled(format!("[{clean_source}]"), bracket_style));
            spans.push(Span::styled(format!(" {arrow} "), verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
        "cross_stream_dependency" => {
            spans.push(Span::styled("handoff from ", verb_style));
            spans.push(Span::styled(format!("[{clean_source}]"), bracket_style));
            spans.push(Span::styled(format!(" {arrow} "), verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
        "handoff_result" => {
            spans.push(Span::styled("handoff result ", verb_style));
            spans.push(Span::styled(format!("[{clean_source}]"), bracket_style));
            spans.push(Span::styled(format!(" {arrow} "), verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
        "dead_letter" => {
            spans.push(Span::styled("dead-letter from ", verb_style));
            spans.push(Span::styled(format!("[{clean_source}]"), bracket_style));
            spans.push(Span::styled(format!(" {arrow} "), verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
        _ => {
            spans.push(Span::styled("routed from ", verb_style));
            spans.push(Span::styled(format!("[{clean_source}]"), bracket_style));
            spans.push(Span::styled(format!(" {arrow} "), verb_style));
            spans.push(Span::styled(format!("[{clean_target}]"), bracket_style));
        }
    }

    if let Some(detail) = clean_summary {
        spans.push(Span::styled(format!(" · {detail}"), metadata_style(theme)));
    }

    wrap_styled_lines([Line::from(spans)], width.max(16))
}

fn collapse_whitespace(raw: &str) -> String {
    clean_text(raw)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
