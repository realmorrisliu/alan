//! Clickable OSC 8 terminal hyperlinks for URLs and file paths in rendered transcript buffers.
//!
//! Layout and line wrapping operate strictly on visible text before buffer rendering,
//! so OSC 8 sequences never alter display widths, wrapping, or cursor columns.
//! Disabled completely when `theme.is_dumb` (`TERM=dumb`) is active.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Size};
use unicode_width::UnicodeWidthStr;

/// A detected clickable link span within a single rendered row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedLink {
    pub byte_start: usize,
    pub byte_end: usize,
    pub uri: String,
}

/// Sanitize a URI for safe OSC 8 emission, rejecting control characters or empty targets.
pub(crate) fn sanitize_osc8_uri(uri: &str) -> Option<String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return None;
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Format an OSC 8 opening escape sequence (`\x1b]8;;<uri>\x1b\\`).
pub(crate) fn osc8_open(uri: &str) -> String {
    format!("\x1b]8;;{uri}\x1b\\")
}

/// OSC 8 closing escape sequence (`\x1b]8;;\x1b\\`).
pub(crate) const OSC8_CLOSE: &str = "\x1b]8;;\x1b\\";

/// Detect non-overlapping HTTP(S) URLs and file paths in a single visible line.
pub(crate) fn detect_links(line: &str, host_root: Option<&Path>) -> Vec<DetectedLink> {
    let mut links = Vec::new();
    let mut idx = 0usize;
    let bytes = line.as_bytes();

    while idx < bytes.len() {
        // Advance past whitespace and common opening delimiters.
        while idx < bytes.len()
            && matches!(
                bytes[idx],
                b' ' | b'\t' | b'(' | b'[' | b'<' | b'"' | b'\'' | b'`'
            )
        {
            idx += 1;
        }
        if idx >= bytes.len() {
            break;
        }
        let token_start = idx;
        while idx < bytes.len() && !bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }
        let token_end = idx;
        let raw = &line[token_start..token_end];
        let trimmed = trim_trailing_link_punct(raw);
        if trimmed.is_empty() {
            continue;
        }
        let byte_end = token_start + trimmed.len();
        if let Some(uri) = classify_link_token(trimmed, host_root) {
            links.push(DetectedLink {
                byte_start: token_start,
                byte_end,
                uri,
            });
        }
    }

    links
}

fn trim_trailing_link_punct(token: &str) -> &str {
    let mut s = token.trim_start_matches(['(', '[', '<', '"', '\'', '`']);
    loop {
        let next = s.trim_end_matches([',', ';', '"', '\'', '`', '>', ']', '.']);
        // Strip unmatched trailing ')' so `(https://example.com)` links cleanly.
        let next = if let Some(stripped) = next.strip_suffix(')') {
            let opens = next.chars().filter(|&c| c == '(').count();
            let closes = next.chars().filter(|&c| c == ')').count();
            if closes > opens { stripped } else { next }
        } else {
            next
        };
        let next = next.trim_end_matches(':');
        if next.len() == s.len() {
            return s;
        }
        s = next;
    }
}

fn classify_link_token(token: &str, host_root: Option<&Path>) -> Option<String> {
    if token.starts_with("https://") || token.starts_with("http://") {
        if token.len() > "https://a".len() {
            return sanitize_osc8_uri(token);
        }
        return None;
    }

    // Strip optional `:line[:col]` suffix for file-path classification.
    let path_part = strip_line_col_suffix(token);
    if !looks_like_file_path(path_part) {
        return None;
    }
    let uri = if path_part.starts_with('/') {
        // Never emit broken file:// links for internal alan9 namespace paths like /agent/1 or /proc/8.
        if is_alan9_namespace_path(path_part) {
            return None;
        }
        format!("file://{}", percent_encode_path(path_part))
    } else if let Some(root) = host_root {
        let joined = root.join(path_part);
        format!("file://{}", percent_encode_path(&joined.to_string_lossy()))
    } else {
        format!("file://{}", percent_encode_path(path_part))
    };
    sanitize_osc8_uri(&uri)
}

fn is_alan9_namespace_path(path: &str) -> bool {
    matches!(
        path.split('/').nth(1),
        Some("agent" | "proc" | "srv" | "memory" | "mnt")
    )
}

fn strip_line_col_suffix(token: &str) -> &str {
    let mut parts = token.rsplitn(3, ':');
    let last = parts.next().unwrap_or("");
    let prev = parts.next();
    let rest = parts.next();
    match (rest, prev) {
        (Some(path), Some(line))
            if !path.is_empty()
                && !line.is_empty()
                && line.bytes().all(|b| b.is_ascii_digit())
                && !last.is_empty()
                && last.bytes().all(|b| b.is_ascii_digit()) =>
        {
            path
        }
        (_, Some(path))
            if !path.is_empty() && !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) =>
        {
            path
        }
        _ => token,
    }
}

fn looks_like_file_path(path: &str) -> bool {
    if path.len() < 3 || path.contains("://") || path.starts_with("--") {
        return false;
    }
    if !path.contains('/') {
        return false;
    }
    let last = path.rsplit('/').next().unwrap_or("");
    let Some((stem, ext)) = last.rsplit_once('.') else {
        return false;
    };
    if stem.is_empty() || !stem.bytes().any(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    let valid_ext = !ext.is_empty()
        && ext.len() <= 10
        && ext.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    if !valid_ext {
        return false;
    }
    if !path.starts_with('/') && !path.starts_with("./") && !path.starts_with("../") {
        let first = path.split('/').next().unwrap_or("");
        if first.is_empty() || !first.bytes().any(|b| b.is_ascii_alphabetic() || b == b'_') {
            return false;
        }
    }
    true
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b':') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CellLinkSpan {
    pub y: u16,
    pub start_x: u16,
    pub end_x: u16,
    pub uri: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct CellLinkMap {
    spans: Vec<CellLinkSpan>,
}

impl CellLinkMap {
    pub(crate) fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub(crate) fn link_at(&self, x: u16, y: u16) -> Option<&str> {
        self.spans
            .iter()
            .find(|span| span.y == y && x >= span.start_x && x <= span.end_x)
            .map(|span| span.uri.as_str())
    }
}

/// Scan rendered Ratatui cells in `buffer` (rows `0..row_limit`) without altering `cell.symbol()`,
/// returning column spans for detected URLs and file paths.
pub(crate) fn collect_buffer_links(
    buffer: &Buffer,
    row_limit: u16,
    host_root: Option<&Path>,
) -> CellLinkMap {
    let width = buffer.area.width as usize;
    let height = buffer.area.height.min(row_limit);
    if width == 0 || height == 0 {
        return CellLinkMap::default();
    }

    let mut spans = Vec::new();
    for row_idx in 0..height {
        let y = buffer.area.y + row_idx;
        let mut line = String::with_capacity(width);
        let mut byte_to_col: Vec<(usize, usize, u16)> = Vec::with_capacity(width);

        for col_idx in 0..width {
            let x = buffer.area.x + col_idx as u16;
            let sym = buffer.cell((x, y)).map_or("", |c| c.symbol());
            if sym.is_empty() || sym.contains('\x1b') {
                continue;
            }
            let start = line.len();
            line.push_str(sym);
            let end = line.len();
            byte_to_col.push((start, end, x));
        }

        if line.trim().is_empty() {
            continue;
        }
        for link in detect_links(&line, host_root) {
            let start_col = byte_to_col
                .iter()
                .find(|&&(s, e, _)| link.byte_start >= s && link.byte_start < e)
                .map(|&(_, _, x)| x);
            let end_col = byte_to_col
                .iter()
                .rev()
                .find(|&&(s, e, _)| link.byte_end > s && link.byte_end <= e)
                .map(|&(_, _, x)| x);
            if let (Some(start_x), Some(end_x)) = (start_col, end_col) {
                spans.push(CellLinkSpan {
                    y,
                    start_x,
                    end_x,
                    uri: link.uri,
                });
            }
        }
    }

    CellLinkMap { spans }
}

/// Ratatui `Backend` wrapper around `CrosstermBackend<W>` that emits OSC 8 escape sequences at
/// write time while keeping `Buffer` cell symbols and `unicode_width` layout untouched.
pub struct HyperlinkBackend<W: Write> {
    inner: CrosstermBackend<W>,
    pending_links: Arc<Mutex<CellLinkMap>>,
}

impl<W: Write> HyperlinkBackend<W> {
    pub(crate) fn new(writer: W) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
            pending_links: Arc::new(Mutex::new(CellLinkMap::default())),
        }
    }

    pub(crate) fn link_sink(&self) -> Arc<Mutex<CellLinkMap>> {
        self.pending_links.clone()
    }
}

impl<W: Write> Write for HyperlinkBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

impl<W: Write> Backend for HyperlinkBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let links = self
            .pending_links
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        if links.is_empty() {
            return self.inner.draw(content);
        }

        let mut batch: Vec<(u16, u16, &'a Cell)> = Vec::new();
        let mut batch_uri: Option<String> = None;
        let mut last_row: Option<u16> = None;
        let mut expected_next_x: u16 = 0;
        let mut to_skip: usize = 0;

        let flush_batch = |inner: &mut CrosstermBackend<W>,
                           batch: &mut Vec<(u16, u16, &'a Cell)>,
                           batch_uri: &mut Option<String>|
         -> io::Result<()> {
            if batch.is_empty() {
                *batch_uri = None;
                return Ok(());
            }
            if let Some(uri) = batch_uri.take() {
                Write::write_all(inner, osc8_open(&uri).as_bytes())?;
                inner.draw(batch.drain(..))?;
                Write::write_all(inner, OSC8_CLOSE.as_bytes())?;
            } else {
                inner.draw(batch.drain(..))?;
            }
            Ok(())
        };

        for (x, y, cell) in content {
            if to_skip > 0 {
                to_skip -= 1;
                batch.push((x, y, cell));
                expected_next_x = x.saturating_add(1);
                continue;
            }
            let uri = links.link_at(x, y);
            let contiguous = last_row == Some(y) && expected_next_x == x;
            let same_group = contiguous && batch_uri.as_deref() == uri;
            if !same_group && !batch.is_empty() {
                flush_batch(&mut self.inner, &mut batch, &mut batch_uri)?;
            }
            if batch.is_empty() {
                batch_uri = uri.map(str::to_string);
            }
            let w = cell.symbol().width().max(1);
            to_skip = w.saturating_sub(1);
            last_row = Some(y);
            expected_next_x = x.saturating_add(1);
            batch.push((x, y, cell));
        }

        flush_batch(&mut self.inner, &mut batch, &mut batch_uri)?;
        Ok(())
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Backend::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::layout::Rect;
    use ratatui::text::Line;
    use ratatui::widgets::{Paragraph, Widget};

    #[test]
    fn detects_urls_and_file_paths_while_skipping_internal_namespace_paths() {
        let root = Path::new("/tmp/workspace");
        let links = detect_links(
            "See https://alanworks.app/docs and crates/tui/src/terminal.rs:42 (ignore /agent/1 and /proc/8 and openai-main/gpt-5.4 and 0/128.0k).",
            Some(root),
        );
        assert_eq!(links.len(), 2, "{links:?}");
        assert_eq!(links[0].uri, "https://alanworks.app/docs");
        assert_eq!(
            links[1].uri,
            "file:///tmp/workspace/crates/tui/src/terminal.rs"
        );
    }

    #[test]
    fn hyperlink_backend_preserves_cell_columns_and_vt100_layout() {
        #[derive(Clone, Default)]
        struct SharedVec(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedVec {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let width = 70u16;
        let text = "Check https://example.com and src/main.rs:10 now";
        let captured = SharedVec::default();
        let backend = HyperlinkBackend::new(captured.clone());
        let sink = backend.link_sink();
        let mut terminal = Terminal::new(backend).unwrap();
        let area = Rect::new(0, 0, width, 1);
        terminal.resize(area).unwrap();
        terminal
            .draw(|frame| {
                Paragraph::new(Line::from(text)).render(frame.area(), frame.buffer_mut());
                let area_height = frame.area().height;
                let links =
                    collect_buffer_links(frame.buffer_mut(), area_height, Some(Path::new("/repo")));
                if let Ok(mut guard) = sink.lock() {
                    *guard = links;
                }
            })
            .unwrap();

        let raw = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert!(raw.contains("\x1b]8;;https://example.com\x1b\\"), "{raw:?}");
        assert!(
            raw.contains("\x1b]8;;file:///repo/src/main.rs\x1b\\"),
            "{raw:?}"
        );
        assert!(raw.contains("\x1b]8;;\x1b\\"), "{raw:?}");

        let mut parser = vt100::Parser::new(1.try_into().unwrap(), width.try_into().unwrap(), 0);
        parser.process(raw.as_bytes());
        let screen_row = parser.screen().rows(0, width).next().unwrap();
        assert_eq!(screen_row.trim_end(), text);
    }

    #[test]
    fn trims_punctuation_percent_encodes_spaces_and_rejects_control_chars() {
        assert_eq!(sanitize_osc8_uri("https://a.test/\x1b]0;pwn"), None);
        assert_eq!(sanitize_osc8_uri("   "), None);

        let root = Path::new("/tmp/my repo");
        let links = detect_links(
            "Open (https://example.com/path_(1)). Edit `crates/tui/src/lib.rs:12:4`, ignore /mnt/project/a.rs and /memory/USER.md.",
            Some(root),
        );
        assert_eq!(links.len(), 2, "{links:?}");
        assert_eq!(links[0].uri, "https://example.com/path_(1)");
        assert_eq!(links[1].uri, "file:///tmp/my%20repo/crates/tui/src/lib.rs");
    }
}
