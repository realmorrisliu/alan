use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{clean_text, metadata_style, wrap_plain_text};

// A projected character belongs to a source byte end and, for expanded tabs,
// a stable sub-character slot. Geometry never participates in this identity.
type Key = (usize, usize);
#[derive(Clone)]
struct Atom {
    ch: char,
    key: Key,
    style: Style,
}

pub(super) fn project(text: &str, width: usize, cut: Key) -> Vec<(Line<'static>, Key)> {
    let mut rows = Vec::new();
    let mut fence: Option<(char, usize, bool)> = None;
    let mut offset = 0;
    for raw in text.split('\n') {
        let mut atoms = Vec::new();
        for (index, ch) in raw.char_indices() {
            let source_end = offset + index + ch.len_utf8();
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
        offset = row_end.0;
        if let Some((open, length, diff)) = fence {
            if marker == open && count >= length && trimmed[count..].trim().is_empty() {
                fence = None;
                for atom in &mut atoms {
                    atom.style = metadata_style();
                }
                append_rows(&mut rows, atoms, width, cut, row_end, false);
                continue;
            }
            let style = if diff && source.starts_with('+') {
                Style::default().fg(Color::Green)
            } else if diff && source.starts_with('-') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
            };
            for atom in &mut atoms {
                atom.style = style;
            }
            append_rows(&mut rows, atoms, width, cut, row_end, false);
            continue;
        }
        if matches!(marker, '`' | '~') && count >= 3 {
            fence = Some((marker, count, trimmed[count..].trim() == "diff"));
            for atom in &mut atoms {
                atom.style = metadata_style();
            }
            append_rows(&mut rows, atoms, width, cut, row_end, false);
            continue;
        }
        let heading = trimmed.chars().take_while(|ch| *ch == '#').count();
        let mut prefix = Vec::new();
        let (body_start, style) =
            if (1..=6).contains(&heading) && trimmed.as_bytes().get(heading) == Some(&b' ') {
                (
                    leading + heading + 1,
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else if trimmed.starts_with("- ")
                || trimmed.starts_with("* ")
                || trimmed.starts_with("+ ")
            {
                prefix.extend(atoms[..source[..leading].chars().count()].iter().cloned());
                let marker_atom = atoms[source[..leading].chars().count()].clone();
                prefix.push(Atom {
                    ch: '•',
                    ..marker_atom
                });
                prefix.push(atoms[source[..leading].chars().count() + 1].clone());
                (leading + 2, Style::default())
            } else {
                (0, Style::default())
            };
        let start = source[..body_start].chars().count();
        let body = &source[body_start..];
        inline(body, &atoms[start..], style, &mut prefix);
        let plain = prefix.iter().all(|atom| atom.style == Style::default())
            && body == prefix.iter().map(|atom| atom.ch).collect::<String>();
        append_rows(&mut rows, prefix, width, cut, row_end, plain);
    }
    rows
}

fn inline(text: &str, atoms: &[Atom], style: Style, output: &mut Vec<Atom>) {
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
                let modifier = match delimiter {
                    "`" => Modifier::UNDERLINED,
                    "**" | "__" => Modifier::BOLD,
                    _ => Modifier::ITALIC,
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

fn append_rows(
    rows: &mut Vec<(Line<'static>, Key)>,
    atoms: Vec<Atom>,
    width: usize,
    cut: Key,
    row_end: Key,
    plain: bool,
) {
    let empty = atoms.is_empty();
    let atoms: Vec<_> = atoms.into_iter().filter(|atom| atom.key > cut).collect();
    if atoms.is_empty() {
        if empty && row_end > cut {
            rows.push((Line::default(), row_end));
        }
        return;
    }
    let text: String = atoms.iter().map(|atom| atom.ch).collect();
    let logical = if plain {
        wrap_plain_text(&text, width)
    } else {
        vec![text.clone()]
    };
    let mut consumed = 0;
    let mut byte_cursor = 0;
    for part in logical {
        let remaining = &text[byte_cursor..];
        let skip = remaining.find(&part).unwrap_or(0);
        consumed += remaining[..skip].chars().count();
        byte_cursor += skip + part.len();
        if skip > 0
            && let Some(previous) = rows.last_mut()
        {
            previous.1 = atoms[consumed - 1].key;
        }
        let end = consumed + part.chars().count();
        let mut spans = Vec::new();
        let mut cells = 0;
        let mut key = cut;
        // Ratatui supplies grapheme segmentation; preserve combining/ZWJ clusters.
        for grapheme in Span::raw(part).styled_graphemes(Style::default()) {
            let size = unicode_width::UnicodeWidthStr::width(grapheme.symbol);
            if cells + size > width && cells > 0 {
                rows.push((Line::from(std::mem::take(&mut spans)), key));
                cells = 0;
            }
            let count = grapheme.symbol.chars().count();
            let style = atoms[consumed].style;
            key = atoms[consumed + count - 1].key;
            consumed += count;
            spans.push(Span::styled(grapheme.symbol.to_string(), style));
            cells += size;
        }
        rows.push((Line::from(spans), key));
        consumed = end;
    }
    if let Some(last) = rows.last_mut() {
        last.1 = row_end;
    }
}
