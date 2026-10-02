use super::{INLINE_PROMPT_CONTINUATION, clean_text};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::borrow::Cow;

/// Literal prompt projection. Each expanded character maps to its original
/// UTF-8 byte end and tab slot. Wrapping borrows slices of this exact buffer;
/// repeated text never participates in locating source positions.
pub(super) fn project(
    text: &str,
    width: usize,
    prefix: &str,
    cut: (usize, usize),
) -> Vec<(Line<'static>, (usize, usize))> {
    let body_width = width
        .saturating_sub(unicode_width::UnicodeWidthStr::width(prefix))
        .max(8);
    let mut rows = Vec::new();
    let mut offset = 0;
    for segment in text.split('\n') {
        let mut keys = Vec::new();
        let mut expanded = String::new();
        let mut clusters = Vec::new();
        for chunk in segment.split_inclusive('\t') {
            let literal = chunk.strip_suffix('\t').unwrap_or(chunk);
            for grapheme in Span::raw(literal).styled_graphemes(Style::default()) {
                let start = grapheme.symbol.as_ptr() as usize - segment.as_ptr() as usize;
                clusters.push((start + grapheme.symbol.len(), grapheme.symbol.to_owned()));
            }
            if chunk.ends_with('\t') {
                let end = chunk.as_ptr() as usize - segment.as_ptr() as usize + chunk.len();
                clusters.push((end, "\t".to_owned()));
            }
        }
        for (end, cluster) in clusters {
            let source_end = offset + end;
            for (slot, ch) in clean_text(&cluster).chars().enumerate() {
                keys.push((
                    expanded.len(),
                    (source_end, if cluster == "\t" { slot } else { 0 }),
                ));
                expanded.push(ch);
            }
        }
        let mut boundaries = vec![0];
        let mut end = 0;
        for grapheme in Span::raw(expanded.as_str()).styled_graphemes(Style::default()) {
            end += grapheme.symbol.len();
            boundaries.push(end);
        }
        let row_end = (
            offset + segment.len() + usize::from(offset + segment.len() < text.len()),
            usize::MAX,
        );
        let first = keys.iter().position(|(_, key)| *key > cut);
        let start = first.map_or(expanded.len(), |i| keys[i].0);
        let tail = &expanded[start..];
        // No hyphenation or indentation: textwrap returns borrowed source slices.
        let options =
            textwrap::Options::new(body_width).word_splitter(textwrap::WordSplitter::NoHyphenation);
        let wrapped = textwrap::wrap(tail, options);
        let mut projected = Vec::new();
        let mut cursor = start;
        let mut key_index = keys.partition_point(|(byte, _)| *byte < start);
        for row in wrapped {
            let Cow::Borrowed(row) = row else {
                unreachable!("literal wrapping without hyphenation must borrow its source");
            };
            let raw_start = row.as_ptr() as usize - expanded.as_ptr() as usize;
            let start = raw_start.max(cursor);
            let raw_end = raw_start + row.len();
            let end = boundaries[boundaries.partition_point(|byte| *byte < raw_end)];
            if end <= cursor && raw_end != raw_start {
                continue;
            }
            while key_index < keys.len() && keys[key_index].0 < start {
                key_index += 1;
            }
            if let Some((_, previous)) = projected.last_mut() {
                *previous = key_index.checked_sub(1).map_or(*previous, |i| keys[i].1);
            }
            while key_index < keys.len() && keys[key_index].0 < end {
                key_index += 1;
            }
            let key = key_index.checked_sub(1).map_or(cut, |i| keys[i].1);
            let row = &expanded[start..end];
            cursor = end;
            let label = if rows.is_empty() && projected.is_empty() && cut == (0, 0) {
                prefix
            } else {
                INLINE_PROMPT_CONTINUATION
            };
            projected.push((Line::from(format!("{label}{row}")), key));
        }
        if let Some(last) = projected.last_mut() {
            last.1 = row_end;
        }
        if row_end > cut && (first.is_some() || keys.is_empty()) {
            if projected.is_empty() {
                let label = if rows.is_empty() && cut == (0, 0) {
                    prefix
                } else {
                    INLINE_PROMPT_CONTINUATION
                };
                projected.push((Line::from(label.to_owned()), row_end));
            }
            rows.extend(projected);
        }
        offset += segment.len() + 1;
    }
    rows
}
