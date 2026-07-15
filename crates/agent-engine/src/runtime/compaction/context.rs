//! Compaction context shaping and deterministic fallback summaries.

pub(crate) const COMPACTION_TOOL_OUTPUT_CHAR_LIMIT: usize = 4_000;
const COMPACTION_TOOL_OUTPUT_HEAD_LINES: usize = 12;
const COMPACTION_TOOL_OUTPUT_TAIL_LINES: usize = 12;
const COMPACTION_TOOL_OUTPUT_IDENTIFIER_LINES: usize = 24;
const COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT: usize = 80;
const COMPACTION_TOOL_OUTPUT_RENDER_LINE_MAX_CHARS: usize = 240;
const COMPACTION_TOOL_OUTPUT_RENDER_LINE_MIN_CHARS: usize = 32;
const DEGRADED_COMPACTION_SNIPPET_CHARS: usize = 240;
const DEGRADED_COMPACTION_SUMMARY_MESSAGES: usize = 6;
pub(crate) const DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS: usize = 800;
pub(crate) const DEGRADED_COMPACTION_SUMMARY_MAX_CHARS: usize = 2_400;

pub(crate) fn sanitize_messages_for_compaction(
    messages: &[crate::tape::Message],
) -> Vec<crate::tape::Message> {
    messages
        .iter()
        .map(sanitize_message_for_compaction)
        .collect()
}

fn sanitize_message_for_compaction(message: &crate::tape::Message) -> crate::tape::Message {
    match message {
        crate::tape::Message::Tool { responses } => crate::tape::Message::tool_multi(
            responses
                .iter()
                .map(sanitize_tool_response_for_compaction)
                .collect(),
        ),
        _ => message.clone(),
    }
}

fn sanitize_tool_response_for_compaction(
    response: &crate::tape::ToolResponse,
) -> crate::tape::ToolResponse {
    let text = response.text_content();
    if text.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && text.lines().count() <= COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT
    {
        return response.clone();
    }

    crate::tape::ToolResponse::text(
        response.id.clone(),
        sanitize_tool_text_for_compaction(&text),
    )
}

pub(crate) fn sanitize_tool_text_for_compaction(text: &str) -> String {
    let line_count = text.lines().count();
    let char_count = text.chars().count();
    if char_count <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && line_count <= COMPACTION_TOOL_OUTPUT_INLINE_LINE_LIMIT
    {
        return text.to_string();
    }

    let lines: Vec<&str> = text.lines().collect();
    let mut keep = std::collections::BTreeSet::new();
    let mut critical_lines = std::collections::BTreeSet::new();
    let tail_start = lines
        .len()
        .saturating_sub(COMPACTION_TOOL_OUTPUT_TAIL_LINES);

    for idx in 0..lines.len().min(COMPACTION_TOOL_OUTPUT_HEAD_LINES) {
        keep.insert(idx);
    }
    for idx in tail_start..lines.len() {
        keep.insert(idx);
    }

    let mut identifier_lines = 0usize;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || line_looks_like_compaction_noise(trimmed) {
            continue;
        }
        if line_contains_critical_identifier(trimmed) {
            keep.insert(idx);
            critical_lines.insert(idx);
            identifier_lines += 1;
            if identifier_lines >= COMPACTION_TOOL_OUTPUT_IDENTIFIER_LINES {
                break;
            }
        }
    }

    let header = format!(
        "[tool output trimmed for compaction; original {line_count} lines / {char_count} chars]"
    );
    let required: std::collections::BTreeSet<usize> = keep
        .iter()
        .copied()
        .filter(|idx| *idx >= tail_start || critical_lines.contains(idx))
        .collect();
    let optional: Vec<usize> = keep
        .iter()
        .copied()
        .filter(|idx| !required.contains(idx))
        .collect();

    render_tool_output_with_cap(&header, &lines, &required, &optional)
}

fn line_contains_critical_identifier(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.contains('/')
        || line.contains('\\')
        || lower.contains("call_")
        || lower.contains("tool_call")
        || lower.contains("id=")
        || lower.contains("id:")
        || lower.contains("uuid")
        || lower.contains("sha256:")
        || lower.contains("sha1:")
        || lower.contains("path:")
        || lower.contains("command:")
        || looks_like_shell_command(&lower)
}

fn looks_like_shell_command(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("$ ")
        || [
            "cargo ", "git ", "just ", "bash ", "sh ", "npm ", "pnpm ", "bun ", "make ",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

fn line_looks_like_compaction_noise(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("debug")
        || lower.starts_with("[debug]")
        || lower.starts_with("trace")
        || lower.starts_with("[trace]")
        || lower.contains(" debug ")
        || lower.contains(" trace ")
}

pub(crate) fn build_degraded_compaction_summary(
    messages: &[crate::tape::Message],
    existing_summary: Option<&str>,
) -> Option<String> {
    let bounded_existing_summary = existing_summary
        .filter(|summary| !summary.trim().is_empty())
        .map(|summary| truncate_compaction_text(summary, DEGRADED_COMPACTION_PRIOR_SUMMARY_CHARS));

    let mut sections = Vec::new();
    if let Some(summary) = bounded_existing_summary.as_deref() {
        sections.push("Prior summary excerpt:".to_string());
        sections.push(summary.to_string());
    }

    let snippets: Vec<String> = messages
        .iter()
        .filter_map(degraded_compaction_snippet)
        .rev()
        .take(DEGRADED_COMPACTION_SUMMARY_MESSAGES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if snippets.is_empty() {
        return bounded_existing_summary;
    }

    sections.push("Deterministic fallback summary after compaction failure:".to_string());
    sections.push("Recent preserved context:".to_string());
    sections.extend(snippets.into_iter().map(|snippet| format!("- {snippet}")));
    Some(truncate_compaction_text(
        &sections.join("\n"),
        DEGRADED_COMPACTION_SUMMARY_MAX_CHARS,
    ))
}

fn degraded_compaction_snippet(message: &crate::tape::Message) -> Option<String> {
    match message {
        crate::tape::Message::User { .. } => {
            let text = message.text_content();
            if text.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "user: {}",
                    truncate_compaction_text(&text, DEGRADED_COMPACTION_SNIPPET_CHARS)
                ))
            }
        }
        crate::tape::Message::Assistant { .. } => {
            let text = message.non_thinking_text_content();
            if text.trim().is_empty() {
                None
            } else {
                Some(format!(
                    "assistant: {}",
                    truncate_compaction_text(&text, DEGRADED_COMPACTION_SNIPPET_CHARS)
                ))
            }
        }
        crate::tape::Message::Tool { responses } => {
            let tool_summaries: Vec<String> = responses
                .iter()
                .filter_map(|response| {
                    let text = sanitize_tool_text_for_compaction(&response.text_content());
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "tool[{}]: {}",
                            response.id,
                            truncate_compaction_text(trimmed, DEGRADED_COMPACTION_SNIPPET_CHARS)
                        ))
                    }
                })
                .collect();
            if tool_summaries.is_empty() {
                None
            } else {
                Some(tool_summaries.join(" | "))
            }
        }
        crate::tape::Message::System { .. } | crate::tape::Message::Context { .. } => None,
    }
}

fn truncate_compaction_text(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    truncate_text_with_suffix(trimmed, max_chars, "...")
}

fn render_tool_output_with_cap(
    header: &str,
    lines: &[&str],
    required: &std::collections::BTreeSet<usize>,
    optional: &[usize],
) -> String {
    let mut line_limit = COMPACTION_TOOL_OUTPUT_RENDER_LINE_MAX_CHARS;
    let mut rendered = render_tool_output_selection(header, lines, required, line_limit);

    while rendered.chars().count() > COMPACTION_TOOL_OUTPUT_CHAR_LIMIT
        && line_limit > COMPACTION_TOOL_OUTPUT_RENDER_LINE_MIN_CHARS
    {
        line_limit = line_limit.saturating_sub(16);
        rendered = render_tool_output_selection(header, lines, required, line_limit);
    }

    let mut included = required.clone();
    for idx in optional {
        let mut candidate = included.clone();
        candidate.insert(*idx);
        let candidate_rendered =
            render_tool_output_selection(header, lines, &candidate, line_limit);
        if candidate_rendered.chars().count() <= COMPACTION_TOOL_OUTPUT_CHAR_LIMIT {
            included = candidate;
            rendered = candidate_rendered;
        }
    }

    rendered
}

fn render_tool_output_selection(
    header: &str,
    lines: &[&str],
    included: &std::collections::BTreeSet<usize>,
    line_limit: usize,
) -> String {
    let mut output = vec![header.to_string()];
    let mut previous = None;
    let mut truncated_line = false;

    for idx in included {
        if let Some(prev) = previous
            && *idx > prev + 1
        {
            output.push(format!("[... {} lines omitted ...]", idx - prev - 1));
        }

        let rendered_line = truncate_text_with_suffix(lines[*idx], line_limit, "...");
        truncated_line |= rendered_line.chars().count() < lines[*idx].chars().count();
        output.push(rendered_line);
        previous = Some(*idx);
    }

    if let Some(prev) = previous
        && prev + 1 < lines.len()
    {
        output.push(format!(
            "[... {} lines omitted ...]",
            lines.len() - prev - 1
        ));
    }

    if truncated_line {
        output.push("[truncated for compaction]".to_string());
    }

    output.join("\n")
}

fn truncate_text_with_suffix(text: &str, max_chars: usize, suffix: &str) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    if max_chars == 0 {
        return String::new();
    }

    let suffix_chars = suffix.chars().count();
    if suffix_chars >= max_chars {
        return suffix.chars().take(max_chars).collect();
    }

    let mut truncated = text
        .chars()
        .take(max_chars.saturating_sub(suffix_chars))
        .collect::<String>();
    truncated.push_str(suffix);
    truncated
}
