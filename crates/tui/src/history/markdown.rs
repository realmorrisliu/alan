use std::borrow::Cow;

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

use super::clean_text;
use super::highlight::{self, HighlightState};
use super::table;

// A projected character belongs to a source byte end and, for expanded tabs,
// a stable sub-character slot. Geometry never participates in this identity.
pub(super) type Key = (usize, usize);

#[derive(Clone)]
struct Atom {
    ch: char,
    key: Key,
    style: Style,
}

pub(super) fn project(
    text: &str,
    width: usize,
    cut: Key,
    theme: Theme,
) -> Vec<(Line<'static>, Key)> {
    let mut rows = Vec::new();
    // This source-independent row has its own stable cut before the first byte.
    if !text.is_empty() && cut < (0, 1) {
        rows.push((Line::default(), (0, 1)));
    }
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let mut offsets = Vec::with_capacity(raw_lines.len());
    let mut cursor = 0usize;
    for raw in &raw_lines {
        offsets.push(cursor);
        cursor += raw.len() + 1;
    }

    let mut fence: Option<(char, usize, String, HighlightState)> = None;
    let mut index = 0usize;
    while index < raw_lines.len() {
        let raw = raw_lines[index];
        let offset = offsets[index];

        if fence.is_none()
            && let Some((table_rows, next_index)) =
                table::try_project_table(&raw_lines, &offsets, index, text.len(), width, cut, theme)
        {
            rows.extend(table_rows);
            index = next_index;
            continue;
        }

        let mut atoms = Vec::new();
        for (byte_idx, ch) in raw.char_indices() {
            let source_end = offset + byte_idx + ch.len_utf8();
            for (slot, ch) in clean_text(&ch.to_string()).chars().enumerate() {
                atoms.push(Atom {
                    ch,
                    key: (source_end, slot),
                    style: Style::default(),
                });
            }
        }
        let source: String = atoms.iter().map(|atom| atom.ch).collect();
        let trimmed = source.trim_start();
        let leading = source.len() - trimmed.len();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|ch| *ch == marker).count();
        let line_end = offset + raw.len();
        let row_end = (line_end + usize::from(line_end < text.len()), usize::MAX);

        if let Some((open, length, ref lang, ref mut hl_state)) = fence {
            if marker == open && count >= length && trimmed[count..].trim().is_empty() {
                fence = None;
                decorate_fence(
                    &mut atoms,
                    source[..leading].chars().count(),
                    count,
                    false,
                    theme,
                );
                append_rows(&mut rows, atoms, width, cut, row_end, false, None, None);
                index += 1;
                continue;
            }
            let styles = highlight::highlight_line(lang, &source, hl_state, theme);
            for (atom, style) in atoms.iter_mut().zip(styles) {
                atom.style = style;
            }
            let gutter_style = fence_body_gutter_style(lang, &source, theme);
            append_rows(
                &mut rows,
                atoms,
                width,
                cut,
                row_end,
                false,
                None,
                Some((theme.gutter(), gutter_style)),
            );
            index += 1;
            continue;
        }
        if matches!(marker, '`' | '~') && count >= 3 {
            let info = trimmed[count..].trim().to_string();
            fence = Some((marker, count, info, HighlightState::default()));
            decorate_fence(
                &mut atoms,
                source[..leading].chars().count(),
                count,
                true,
                theme,
            );
            append_rows(&mut rows, atoms, width, cut, row_end, false, None, None);
            index += 1;
            continue;
        }

        let heading = trimmed.chars().take_while(|ch| *ch == '#').count();
        let mut prefix = Vec::new();
        let mut wrap_prefix: Option<(String, Style)> = None;
        let (body_start, style) =
            if (1..=6).contains(&heading) && trimmed.as_bytes().get(heading) == Some(&b' ') {
                (leading + heading + 1, theme.heading_style(heading))
            } else if trimmed.starts_with('>') && parse_blockquote(trimmed).is_some() {
                let (depth, consumed_trimmed) = parse_blockquote(trimmed).expect("checked");
                let bar_style = theme.rule_style();
                let quote_bar = if theme.is_dumb {
                    "| ".repeat(depth)
                } else {
                    "│ ".repeat(depth)
                };
                let lead_chars = source[..leading].chars().count();
                prefix.extend(atoms[..lead_chars].iter().cloned());
                let consumed_chars = trimmed[..consumed_trimmed].chars().count();
                let quote_atoms = &atoms[lead_chars..lead_chars + consumed_chars];
                let mut quote_key_idx = 0usize;
                for ch in quote_bar.chars() {
                    let atom = quote_atoms
                        .get(quote_key_idx)
                        .or_else(|| quote_atoms.last())
                        .expect("blockquote has > atom");
                    prefix.push(Atom {
                        ch,
                        key: atom.key,
                        style: bar_style,
                    });
                    if quote_key_idx + 1 < quote_atoms.len() {
                        quote_key_idx += 1;
                    }
                }
                wrap_prefix = Some((quote_bar, bar_style));
                (leading + consumed_trimmed, Style::default())
            } else if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                let lead_chars = source[..leading].chars().count();
                prefix.extend(atoms[..lead_chars].iter().cloned());
                let marker_atom = atoms[lead_chars].clone();
                prefix.push(Atom {
                    ch: if theme.is_dumb { '-' } else { '•' },
                    ..marker_atom
                });
                prefix.push(atoms[lead_chars + 1].clone());
                (leading + 2, Style::default())
            } else {
                (0, Style::default())
            };

        let start = source[..body_start].chars().count();
        let body = &source[body_start..];
        inline(body, &atoms[start..], style, theme, &mut prefix);
        append_rows(
            &mut rows,
            prefix,
            width,
            cut,
            row_end,
            true,
            wrap_prefix.as_ref(),
            None,
        );
        index += 1;
    }
    rows
}

fn fence_body_gutter_style(lang: &str, source: &str, theme: Theme) -> Style {
    if theme.is_dumb {
        return Style::default();
    }
    let token = lang
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .next()
        .unwrap_or("");
    if token.eq_ignore_ascii_case("diff") || token.eq_ignore_ascii_case("patch") {
        if source.starts_with('+') {
            return theme.diff_add_style();
        }
        if source.starts_with('-') {
            return theme.diff_remove_style();
        }
        if source.starts_with("@@") {
            return theme.accent_style();
        }
    }
    theme.rule_style()
}

fn parse_blockquote(trimmed: &str) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    let mut byte = 0usize;
    let bytes = trimmed.as_bytes();
    while byte < bytes.len() && bytes[byte] == b'>' {
        depth += 1;
        byte += 1;
        if bytes.get(byte) == Some(&b' ') {
            byte += 1;
        }
    }
    (depth > 0).then_some((depth, byte))
}

pub(super) fn strip_inline_markers(text: &str) -> String {
    let atoms: Vec<Atom> = text
        .chars()
        .enumerate()
        .map(|(i, ch)| Atom {
            ch,
            key: (i + 1, 0),
            style: Style::default(),
        })
        .collect();
    let mut out = Vec::new();
    inline(text, &atoms, Style::default(), Theme::default(), &mut out);
    out.into_iter().map(|a| a.ch).collect()
}

fn decorate_fence(
    atoms: &mut Vec<Atom>,
    leading: usize,
    count: usize,
    opening: bool,
    theme: Theme,
) {
    let gutter_char = if theme.is_dumb { '|' } else { '▎' };
    let gutter_style = theme.rule_style();
    for atom in atoms.iter_mut().take(leading) {
        atom.style = gutter_style;
    }
    if count == 0 {
        return;
    }
    let has_info = opening
        && atoms
            .get(leading + count..)
            .is_some_and(|rest| rest.iter().any(|atom| !atom.ch.is_whitespace()));
    atoms[leading].ch = gutter_char;
    atoms[leading].style = gutter_style;
    if has_info {
        if count >= 2 {
            atoms[leading + 1].ch = ' ';
            atoms[leading + 1].style = gutter_style;
            if count > 2 {
                atoms.drain(leading + 2..leading + count);
            }
        }
        let info_start = leading + 2.min(count);
        for atom in &mut atoms[info_start..] {
            atom.style = theme.accent_style();
        }
    } else {
        atoms.truncate(leading + 1);
    }
}

fn inline(text: &str, atoms: &[Atom], style: Style, theme: Theme, output: &mut Vec<Atom>) {
    let mut byte = 0;
    let mut index = 0;
    while byte < text.len() {
        let rest = &text[byte..];
        if rest.starts_with('\\') && rest.len() > 1 {
            let next = rest[1..].chars().next().unwrap();
            if next.is_ascii_punctuation() {
                output.push(Atom {
                    style,
                    ..atoms[index + 1].clone()
                });
                byte += 1 + next.len_utf8();
                index += 2;
                continue;
            }
        }
        if rest.starts_with('[')
            && let Some((label, url, total_bytes)) = parse_markdown_link(rest)
        {
            let link_style = if theme.is_dumb {
                style
            } else {
                theme.underlined_if_capable(style.fg(theme.accent))
            };
            let url_style = theme.faint_style();
            let label_chars = label.chars().count();
            let url_chars = url.chars().count();
            let total_chars = rest[..total_bytes].chars().count();

            let label_start = index + 1;
            let label_end = label_start + label_chars;
            inline(
                label,
                &atoms[label_start..label_end],
                link_style,
                theme,
                output,
            );

            // Reuse existing source keys monotonically for ` (url)` display.
            let close_bracket_idx = label_end;
            let open_paren_idx = close_bracket_idx + 1;
            let url_start_idx = open_paren_idx + 1;
            let url_end_idx = url_start_idx + url_chars;
            let close_paren_idx = url_end_idx;

            output.push(Atom {
                ch: ' ',
                key: atoms[close_bracket_idx].key,
                style: url_style,
            });
            output.push(Atom {
                ch: '(',
                key: atoms[open_paren_idx].key,
                style: url_style,
            });
            for atom in &atoms[url_start_idx..url_end_idx] {
                output.push(Atom {
                    style: url_style,
                    ..atom.clone()
                });
            }
            output.push(Atom {
                ch: ')',
                key: atoms[close_paren_idx].key,
                style: url_style,
            });

            byte += total_bytes;
            index += total_chars;
            continue;
        }
        if (rest.starts_with("https://") || rest.starts_with("http://"))
            && (byte == 0
                || text[..byte]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_whitespace() || matches!(c, '(' | '<' | '[')))
        {
            let url_bytes = scan_bare_url_len(rest);
            if url_bytes > 0 {
                let url_chars = rest[..url_bytes].chars().count();
                let link_style = if theme.is_dumb {
                    style
                } else {
                    theme.underlined_if_capable(style.fg(theme.accent))
                };
                for atom in &atoms[index..index + url_chars] {
                    output.push(Atom {
                        style: link_style,
                        ..atom.clone()
                    });
                }
                byte += url_bytes;
                index += url_chars;
                continue;
            }
        }
        if let Some(delimiter) = ["`", "**", "__", "*", "_"].into_iter().find(|d| {
            rest.starts_with(d)
                && (!d.starts_with('_')
                    || !text[..byte]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| ch.is_alphanumeric() || ch == '_'))
        }) {
            let start = byte + delimiter.len();
            if let Some(end) = text[start..]
                .match_indices(delimiter)
                .map(|(end, _)| end)
                .find(|end| {
                    *end > 0
                        && (!delimiter.starts_with('_')
                            || !text[start + end + delimiter.len()..]
                                .chars()
                                .next()
                                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_'))
                })
            {
                let body = &text[start..start + end];
                let first = index + delimiter.len();
                let last = first + body.chars().count();
                let modifier = if theme.is_dumb {
                    Modifier::empty()
                } else {
                    match delimiter {
                        "`" => Modifier::UNDERLINED,
                        "**" | "__" => Modifier::BOLD,
                        _ => Modifier::ITALIC,
                    }
                };
                if delimiter == "`" {
                    output.extend(atoms[first..last].iter().map(|atom| Atom {
                        style: style.add_modifier(modifier),
                        ..*atom
                    }));
                } else {
                    inline(
                        body,
                        &atoms[first..last],
                        style.add_modifier(modifier),
                        theme,
                        output,
                    );
                }
                byte = start + end + delimiter.len();
                index = last + delimiter.len();
                continue;
            }
        }
        output.push(Atom {
            style,
            ..atoms[index].clone()
        });
        byte += atoms[index].ch.len_utf8();
        index += 1;
    }
}

fn parse_markdown_link(rest: &str) -> Option<(&str, &str, usize)> {
    let after_open = rest.strip_prefix('[')?;
    let close_bracket = after_open.find(']')?;
    let label = &after_open[..close_bracket];
    if label.contains('[') {
        return None;
    }
    let after_close = &after_open[close_bracket + 1..];
    let after_paren = after_close.strip_prefix('(')?;
    let close_paren = after_paren.find(')')?;
    let url = &after_paren[..close_paren];
    if url.is_empty() || url.chars().any(char::is_whitespace) {
        return None;
    }
    let total_bytes = 1 + close_bracket + 2 + close_paren + 1;
    Some((label, url, total_bytes))
}

fn scan_bare_url_len(rest: &str) -> usize {
    let mut end = rest
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace() || matches!(ch, '<' | '>'))
        .map_or(rest.len(), |(idx, _)| idx);
    while end > 0 {
        let trimmed = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']);
        if trimmed.len() == end {
            break;
        }
        end = trimmed.len();
    }
    end
}

#[allow(
    clippy::too_many_arguments,
    reason = "internal row emitter tracks cut keys alongside optional blockquote and code gutter prefixes"
)]
fn append_rows(
    rows: &mut Vec<(Line<'static>, Key)>,
    atoms: Vec<Atom>,
    width: usize,
    cut: Key,
    row_end: Key,
    word_wrap: bool,
    wrap_prefix: Option<&(String, Style)>,
    line_gutter: Option<(&str, Style)>,
) {
    let empty = atoms.is_empty();
    let atoms: Vec<_> = atoms.into_iter().filter(|atom| atom.key > cut).collect();
    if atoms.is_empty() {
        if empty && row_end > cut {
            if let Some((gutter, gstyle)) = line_gutter {
                rows.push((Line::styled(gutter.trim_end().to_string(), gstyle), row_end));
            } else {
                rows.push((Line::default(), row_end));
            }
        }
        return;
    }
    let active_gutter = line_gutter.filter(|(gutter, _)| width > gutter.width() + 2);
    let cont_prefix = wrap_prefix
        .filter(|(prefix, _)| width > prefix.width() + 4)
        .filter(|(prefix, _)| atoms.len() > prefix.chars().count());
    let wrap_width = if word_wrap {
        if let Some((prefix, _)) = cont_prefix {
            width.saturating_sub(prefix.width()).max(8)
        } else {
            width
        }
    } else {
        width
    };

    let text: String = atoms.iter().map(|atom| atom.ch).collect();
    let mut boundaries = vec![0];
    let mut boundary_end = 0;
    for grapheme in Span::raw(text.as_str()).styled_graphemes(Style::default()) {
        boundary_end += grapheme.symbol.len();
        boundaries.push(boundary_end);
    }
    let logical: Vec<Cow<'_, str>> = if word_wrap {
        let options =
            textwrap::Options::new(wrap_width).word_splitter(textwrap::WordSplitter::NoHyphenation);
        let wrapped = textwrap::wrap(&text, options);
        if wrapped.is_empty() {
            vec![Cow::Borrowed("")]
        } else {
            wrapped
        }
    } else {
        vec![Cow::Borrowed(text.as_str())]
    };
    let mut consumed = 0;
    let mut byte_cursor = 0;
    let mut pushed_for_line = false;
    for part in logical {
        let Cow::Borrowed(slice) = part else {
            unreachable!("markdown word wrapping without hyphenation must borrow its source");
        };
        let raw_start = slice.as_ptr() as usize - text.as_ptr() as usize;
        let start = raw_start.max(byte_cursor);
        let raw_end = raw_start + slice.len();
        let end = boundaries[boundaries.partition_point(|byte| *byte < raw_end)];
        if end <= byte_cursor && raw_end != raw_start {
            continue;
        }
        let skip_bytes = start - byte_cursor;
        consumed += text[byte_cursor..start].chars().count();
        byte_cursor = end;
        if skip_bytes > 0
            && pushed_for_line
            && let Some(previous) = rows.last_mut()
        {
            previous.1 = atoms[consumed - 1].key;
        }
        let part = &text[start..end];
        let mut spans = Vec::new();
        let mut cells = 0;
        let mut prefix_cells = 0;
        if let Some((gutter, gstyle)) = active_gutter {
            spans.push(Span::styled(gutter.to_string(), gstyle));
            let gw = gutter.width();
            cells += gw;
            prefix_cells += gw;
        }
        let active_prefix = pushed_for_line.then_some(cont_prefix).flatten();
        if let Some((prefix, pstyle)) = active_prefix {
            spans.push(Span::styled(prefix.clone(), *pstyle));
            let pw = prefix.width();
            cells += pw;
            prefix_cells += pw;
        }
        let mut key = cut;
        // Ratatui supplies grapheme segmentation; preserve combining/ZWJ clusters.
        for grapheme in Span::raw(part).styled_graphemes(Style::default()) {
            let size = unicode_width::UnicodeWidthStr::width(grapheme.symbol);
            if cells + size > width && cells > prefix_cells {
                rows.push((Line::from(std::mem::take(&mut spans)), key));
                cells = 0;
                prefix_cells = 0;
                if let Some((gutter, gstyle)) = active_gutter {
                    spans.push(Span::styled(gutter.to_string(), gstyle));
                    let gw = gutter.width();
                    cells += gw;
                    prefix_cells += gw;
                }
                if let Some((prefix, pstyle)) = cont_prefix {
                    spans.push(Span::styled(prefix.clone(), *pstyle));
                    let pw = prefix.width();
                    cells += pw;
                    prefix_cells += pw;
                }
            }
            let count = grapheme.symbol.chars().count();
            let style = atoms[consumed].style;
            key = atoms[consumed + count - 1].key;
            consumed += count;
            spans.push(Span::styled(grapheme.symbol.to_string(), style));
            cells += size;
        }
        rows.push((Line::from(spans), key));
        pushed_for_line = true;
    }
    if let Some(last) = rows.last_mut() {
        last.1 = row_end;
    }
}

#[cfg(test)]
#[path = "markdown_m2_tests.rs"]
mod m2_tests;
