use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

use super::markdown::{Key, strip_inline_markers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Alignment {
    Left,
    Center,
    Right,
}

pub(super) fn try_project_table(
    lines: &[&str],
    offsets: &[usize],
    start: usize,
    total_len: usize,
    width: usize,
    cut: Key,
    theme: Theme,
) -> Option<(Vec<(Line<'static>, Key)>, usize)> {
    if start + 1 >= lines.len() {
        return None;
    }
    let header_raw = lines[start];
    let sep_raw = lines[start + 1];
    if !looks_like_table_row(header_raw) {
        return None;
    }
    let alignments = parse_separator_row(sep_raw)?;
    if alignments.is_empty() {
        return None;
    }
    let header = split_table_cells(header_raw);
    if header.len() != alignments.len() {
        return None;
    }

    let mut end = start + 2;
    let mut body_rows = Vec::new();
    while end < lines.len() && looks_like_table_row(lines[end]) {
        if parse_separator_row(lines[end]).is_some() {
            break;
        }
        let mut cells = split_table_cells(lines[end]);
        cells.resize(alignments.len(), String::new());
        body_rows.push((end, cells));
        end += 1;
    }

    let col_widths = fit_column_widths(&header, &body_rows, alignments.len(), width);
    let border_style = theme.rule_style();
    let header_style = theme.accent_bold();
    let cell_style = Style::default();

    let mut rendered = Vec::new();
    let header_start = offsets[start];
    let top_key = (header_start + 1, 0);
    if top_key > cut {
        rendered.push((
            Line::styled(horizontal_border(&col_widths, '┌', '┬', '┐'), border_style),
            top_key,
        ));
    }

    let header_key = (header_start + 1, 1);
    if header_key > cut {
        rendered.push((
            render_row(
                &header,
                &col_widths,
                &alignments,
                header_style,
                border_style,
            ),
            header_key,
        ));
    }

    let sep_end = offsets[start + 1] + sep_raw.len();
    let sep_key = (sep_end, 0);
    if sep_key > cut {
        rendered.push((
            Line::styled(horizontal_border(&col_widths, '├', '┼', '┤'), border_style),
            sep_key,
        ));
    }

    for (line_idx, cells) in &body_rows {
        let row_end = offsets[*line_idx] + lines[*line_idx].len();
        let row_key = (row_end, 0);
        if row_key > cut {
            rendered.push((
                render_row(cells, &col_widths, &alignments, cell_style, border_style),
                row_key,
            ));
        }
    }

    let last_idx = end - 1;
    let last_end = offsets[last_idx] + lines[last_idx].len();
    let bottom_key = (last_end + usize::from(last_end < total_len), usize::MAX);
    if bottom_key > cut {
        rendered.push((
            Line::styled(horizontal_border(&col_widths, '└', '┴', '┘'), border_style),
            bottom_key,
        ));
    }

    Some((rendered, end))
}

fn looks_like_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('|')
        && !trimmed.starts_with('>')
        && !trimmed.starts_with('#')
}

fn split_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let stripped = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let stripped = stripped.strip_suffix('|').unwrap_or(stripped);
    stripped
        .split('|')
        .map(|cell| strip_inline_markers(cell.trim()))
        .collect()
}

fn parse_separator_row(line: &str) -> Option<Vec<Alignment>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') && !trimmed.contains('-') {
        return None;
    }
    let stripped = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let stripped = stripped.strip_suffix('|').unwrap_or(stripped);
    let mut alignments = Vec::new();
    for part in stripped.split('|') {
        let cell = part.trim();
        let left_colon = cell.starts_with(':');
        let right_colon = cell.ends_with(':');
        let core = cell.trim_start_matches(':').trim_end_matches(':');
        if core.len() < 2 || !core.chars().all(|ch| ch == '-') {
            return None;
        }
        let align = match (left_colon, right_colon) {
            (true, true) => Alignment::Center,
            (false, true) => Alignment::Right,
            _ => Alignment::Left,
        };
        alignments.push(align);
    }
    Some(alignments)
}

fn fit_column_widths(
    header: &[String],
    rows: &[(usize, Vec<String>)],
    cols: usize,
    total_width: usize,
) -> Vec<usize> {
    let mut widths = vec![1usize; cols];
    for (idx, cell) in header.iter().enumerate() {
        widths[idx] = widths[idx].max(cell.width());
    }
    for (_, row) in rows {
        for (idx, cell) in row.iter().enumerate().take(cols) {
            widths[idx] = widths[idx].max(cell.width());
        }
    }
    // Each column consumes `w + 2` padding + `cols + 1` border chars.
    let chrome = cols * 3 + 1;
    let budget = total_width.saturating_sub(chrome).max(cols);
    let mut sum: usize = widths.iter().sum();
    while sum > budget {
        let Some((idx, _)) = widths.iter().enumerate().max_by_key(|(_, w)| **w) else {
            break;
        };
        if widths[idx] <= 1 {
            break;
        }
        widths[idx] -= 1;
        sum -= 1;
    }
    widths
}

fn horizontal_border(widths: &[usize], left: char, mid: char, right: char) -> String {
    let mut out = String::new();
    out.push(left);
    for (idx, width) in widths.iter().enumerate() {
        for _ in 0..(*width + 2) {
            out.push('─');
        }
        if idx + 1 < widths.len() {
            out.push(mid);
        }
    }
    out.push(right);
    out
}

fn render_row(
    cells: &[String],
    widths: &[usize],
    alignments: &[Alignment],
    cell_style: Style,
    border_style: Style,
) -> Line<'static> {
    let mut spans = vec![Span::styled("│", border_style)];
    for (idx, width) in widths.iter().enumerate() {
        let raw = cells.get(idx).map(String::as_str).unwrap_or("");
        let padded = align_cell(raw, *width, alignments[idx]);
        spans.push(Span::styled(format!(" {padded} "), cell_style));
        spans.push(Span::styled("│", border_style));
    }
    Line::from(spans)
}

fn align_cell(text: &str, width: usize, alignment: Alignment) -> String {
    let fitted = truncate_cell(text, width);
    let text_width = fitted.width();
    let pad = width.saturating_sub(text_width);
    match alignment {
        Alignment::Left => format!("{fitted}{}", " ".repeat(pad)),
        Alignment::Right => format!("{}{fitted}", " ".repeat(pad)),
        Alignment::Center => {
            let left = pad / 2;
            let right = pad - left;
            format!("{}{fitted}{}", " ".repeat(left), " ".repeat(right))
        }
    }
}

fn truncate_cell(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w >= max_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}
