//! Terminal inline graphics capability probing, image encoders, and text-badge fallbacks.
//!
//! Strictly follows ADR-0061: capability detection uses standard escape-sequence protocol queries
//! only (Kitty Graphics Protocol APC query and XTVERSION DCS query) and never inspects terminal
//! host environment variables.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use std::io::IsTerminal;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::time::{Duration, Instant};

use crate::theme::ThemeMode;

/// Kitty graphics query image ID used during startup capability probing.
pub const KITTY_PROBE_ID: u32 = 31;
/// Kitty graphics 1x1 RGB query sequence (`a=q`, `i=31`).
pub const KITTY_PROBE_QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
/// Standard XTVERSION DCS query used to detect iTerm2 OSC 1337 support via protocol response.
pub const XTVERSION_QUERY: &[u8] = b"\x1b[>0q";
/// Maximum bounded time budget for reading terminal graphics probe responses.
pub const PROBE_TIMEOUT_MS: u64 = 120;
/// Maximum height (in terminal rows) allowed for any inline image thumbnail.
pub const MAX_THUMBNAIL_ROWS: u16 = 12;
/// Maximum base64 payload bytes per Kitty APC chunk.
pub const KITTY_CHUNK_SIZE: usize = 4096;

/// Assumed terminal cell height-to-width visual aspect ratio (~2:1 for monospace fonts).
const CELL_HEIGHT_TO_WIDTH_RATIO: f64 = 2.0;
#[cfg(unix)]
const PROBE_IDLE_DRAIN_MS: u64 = 15;
#[cfg(unix)]
const PROBE_MAX_RESPONSE_BYTES: usize = 4096;

/// Detected terminal inline graphics protocol capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsCapability {
    #[default]
    None,
    Kitty,
    ITerm2,
}

/// Supported image container formats recognized from magic headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl ImageFormat {
    /// Canonical MIME type for the detected image format.
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }
}

/// Pixel dimensions and format extracted from an image header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDimensions {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

/// Terminal cell dimensions reserved for an inline image thumbnail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailGeometry {
    pub cols: u16,
    pub rows: u16,
}

/// Pure guard deciding whether graphics capability probing must be skipped immediately.
pub fn should_skip_probe(theme_mode: ThemeMode, term: &str, is_tty: bool) -> bool {
    !is_tty || theme_mode == ThemeMode::Dumb || term.eq_ignore_ascii_case("dumb")
}

/// Parse raw terminal bytes returned after sending [`KITTY_PROBE_QUERY`] and [`XTVERSION_QUERY`].
///
/// Recognizes Kitty APC `OK` for `i=31`, ignores Kitty `ENOENT`/errors, recognizes XTVERSION
/// `iTerm2` DCS replies, and returns [`GraphicsCapability::None`] for empty/malformed bytes.
pub fn parse_probe_response(raw: &[u8]) -> GraphicsCapability {
    let mut found_iterm2 = false;
    let mut i = 0usize;
    while i + 1 < raw.len() {
        if raw[i] != 0x1b {
            i += 1;
            continue;
        }
        let kind = raw[i + 1];
        if kind != b'_' && kind != b'P' {
            i += 1;
            continue;
        }
        let Some((body, next)) = extract_terminated_frame(raw, i + 2) else {
            i += 2;
            continue;
        };
        if kind == b'_' && is_kitty_probe_ok(body) {
            return GraphicsCapability::Kitty;
        }
        if kind == b'P' && is_xtversion_iterm2(body) {
            found_iterm2 = true;
        }
        i = next;
    }
    if found_iterm2 {
        GraphicsCapability::ITerm2
    } else {
        GraphicsCapability::None
    }
}

/// Extract a frame body starting at `start` terminated by ST (`\x1b\\`) or BEL (`\x07`).
fn extract_terminated_frame(raw: &[u8], start: usize) -> Option<(&[u8], usize)> {
    let mut j = start;
    while j < raw.len() {
        if raw[j] == 0x07 {
            return Some((&raw[start..j], j + 1));
        }
        if raw[j] == 0x1b {
            return (j + 1 < raw.len() && raw[j + 1] == b'\\').then_some((&raw[start..j], j + 2));
        }
        j += 1;
    }
    None
}

fn is_kitty_probe_ok(apc_body: &[u8]) -> bool {
    let Some(text) = apc_body
        .strip_prefix(b"G")
        .and_then(|rest| std::str::from_utf8(rest).ok())
    else {
        return false;
    };
    let Some((params, status)) = text.split_once(';') else {
        return false;
    };
    status.trim() == "OK" && params.split(',').any(|kv| kv.trim() == "i=31")
}

fn is_xtversion_iterm2(dcs_body: &[u8]) -> bool {
    dcs_body
        .strip_prefix(b">|")
        .and_then(|rest| std::str::from_utf8(rest).ok())
        .is_some_and(|text| text.to_ascii_lowercase().contains("iterm2"))
}

/// Probe the active terminal's inline graphics capability using standard escape sequences.
///
/// Must be called after raw mode is enabled and before entering the interactive input loop.
/// Immediately returns [`GraphicsCapability::None`] for [`ThemeMode::Dumb`], `TERM=dumb`, or
/// non-TTY streams. Bounded to `<= 120ms` and drains cleanly so zero bytes leak into input.
pub fn probe_terminal_graphics(theme_mode: ThemeMode) -> GraphicsCapability {
    let term = std::env::var("TERM").unwrap_or_default();
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if should_skip_probe(theme_mode, &term, is_tty) {
        return GraphicsCapability::None;
    }
    #[cfg(unix)]
    return probe_terminal_graphics_unix();
    #[cfg(not(unix))]
    GraphicsCapability::None
}

#[cfg(unix)]
fn probe_terminal_graphics_unix() -> GraphicsCapability {
    let mut out = std::io::stdout();
    if out.write_all(KITTY_PROBE_QUERY).is_err()
        || out.write_all(XTVERSION_QUERY).is_err()
        || out.flush().is_err()
    {
        return GraphicsCapability::None;
    }
    let response = read_stdin_bounded(
        Duration::from_millis(PROBE_TIMEOUT_MS),
        Duration::from_millis(PROBE_IDLE_DRAIN_MS),
    );
    parse_probe_response(&response)
}

#[cfg(unix)]
fn read_stdin_bounded(budget: Duration, idle_drain: Duration) -> Vec<u8> {
    let deadline = Instant::now() + budget;
    let mut collected = Vec::new();
    let mut last_byte_at: Option<Instant> = None;
    let mut scratch = [0u8; 512];

    loop {
        let now = Instant::now();
        let eff_deadline = last_byte_at.map_or(deadline, |last| deadline.min(last + idle_drain));
        if now >= eff_deadline {
            break;
        }
        let timeout_ms = eff_deadline
            .saturating_duration_since(now)
            .as_millis()
            .clamp(1, i32::MAX as u128) as libc::c_int;
        let mut pfd = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: `pfd` points to a valid single `pollfd` struct.
        let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if rc < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if rc == 0 || (pfd.revents & libc::POLLIN) == 0 {
            break;
        }
        // SAFETY: `scratch` is a valid mutable byte buffer of length `scratch.len()`.
        let n = unsafe {
            libc::read(
                libc::STDIN_FILENO,
                scratch.as_mut_ptr().cast::<libc::c_void>(),
                scratch.len(),
            )
        };
        if n <= 0 {
            break;
        }
        let space = PROBE_MAX_RESPONSE_BYTES.saturating_sub(collected.len());
        collected.extend_from_slice(&scratch[..(n as usize).min(space)]);
        last_byte_at = Some(Instant::now());
    }
    collected
}

/// Parse pixel width, height, and format from PNG, JPEG, GIF, or WebP headers.
pub fn parse_image_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    parse_png_dimensions(bytes)
        .or_else(|| parse_gif_dimensions(bytes))
        .or_else(|| parse_webp_dimensions(bytes))
        .or_else(|| parse_jpeg_dimensions(bytes))
}

fn parse_png_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let (width, height) = (
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    );
    non_zero_dims(width, height, ImageFormat::Png)
}

fn parse_gif_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 10 || (&bytes[0..6] != b"GIF87a" && &bytes[0..6] != b"GIF89a") {
        return None;
    }
    let w = u32::from(u16::from_le_bytes(bytes[6..8].try_into().ok()?));
    let h = u32::from(u16::from_le_bytes(bytes[8..10].try_into().ok()?));
    non_zero_dims(w, h, ImageFormat::Gif)
}

fn parse_webp_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 25 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    match &bytes[12..16] {
        b"VP8 " if bytes.len() >= 30 && bytes[23..26] == [0x9d, 0x01, 0x2a] => {
            let w = u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff);
            let h = u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff);
            non_zero_dims(w, h, ImageFormat::WebP)
        }
        b"VP8L" if bytes.len() >= 25 && bytes[20] == 0x2f => {
            let bits = u32::from_le_bytes(bytes[21..25].try_into().ok()?);
            non_zero_dims(
                (bits & 0x3fff) + 1,
                ((bits >> 14) & 0x3fff) + 1,
                ImageFormat::WebP,
            )
        }
        b"VP8X" if bytes.len() >= 30 => {
            let w = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1;
            let h = u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1;
            non_zero_dims(w, h, ImageFormat::WebP)
        }
        _ => None,
    }
}

fn parse_jpeg_dimensions(bytes: &[u8]) -> Option<ImageDimensions> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut pos = 2usize;
    while pos < bytes.len() {
        if bytes[pos] != 0xff {
            return None;
        }
        while pos < bytes.len() && bytes[pos] == 0xff {
            pos += 1;
        }
        let marker = *bytes.get(pos)?;
        pos += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x00 || marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        let seg_len = usize::from(u16::from_be_bytes([*bytes.get(pos)?, *bytes.get(pos + 1)?]));
        if seg_len < 2 || pos + seg_len > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf
        ) {
            if seg_len < 7 {
                return None;
            }
            let height = u32::from(u16::from_be_bytes([bytes[pos + 3], bytes[pos + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[pos + 5], bytes[pos + 6]]));
            return non_zero_dims(width, height, ImageFormat::Jpeg);
        }
        pos += seg_len;
    }
    None
}

fn non_zero_dims(width: u32, height: u32, format: ImageFormat) -> Option<ImageDimensions> {
    (width > 0 && height > 0).then_some(ImageDimensions {
        width,
        height,
        format,
    })
}

/// Compute aspect-ratio-preserving terminal cell geometry strictly clamped to `<= 12` rows.
pub fn compute_thumbnail_geometry(
    dimensions: Option<ImageDimensions>,
    max_cols: u16,
    max_rows: u16,
) -> ThumbnailGeometry {
    let clamped_rows = max_rows.clamp(1, MAX_THUMBNAIL_ROWS);
    let clamped_cols = max_cols.max(1);
    let Some(dims) = dimensions else {
        return ThumbnailGeometry {
            cols: clamped_cols.clamp(1, 24),
            rows: clamped_rows.clamp(1, 6),
        };
    };

    let (w, h) = (f64::from(dims.width), f64::from(dims.height));
    let cols_at_max_rows = (w / h) * CELL_HEIGHT_TO_WIDTH_RATIO * f64::from(clamped_rows);
    if cols_at_max_rows <= f64::from(clamped_cols) {
        ThumbnailGeometry {
            cols: (cols_at_max_rows.round() as u16).clamp(1, clamped_cols),
            rows: clamped_rows,
        }
    } else {
        let rows_at_max_cols = (h / w) / CELL_HEIGHT_TO_WIDTH_RATIO * f64::from(clamped_cols);
        ThumbnailGeometry {
            cols: clamped_cols,
            rows: (rows_at_max_cols.round() as u16).clamp(1, clamped_rows),
        }
    }
}

/// Encode PNG bytes into Kitty Graphics Protocol APC chunks (`a=T,f=100,c=...,r=...,m=1/0`),
/// keeping every chunk's base64 payload `<= 4096` bytes.
pub fn encode_kitty_chunks(data: &[u8], geometry: ThumbnailGeometry) -> Vec<String> {
    if data.is_empty() {
        return Vec::new();
    }
    let cols = geometry.cols.max(1);
    let rows = geometry.rows.clamp(1, MAX_THUMBNAIL_ROWS);
    let encoded = BASE64_STANDARD.encode(data);
    let total = encoded.len().div_ceil(KITTY_CHUNK_SIZE);

    encoded
        .as_bytes()
        .chunks(KITTY_CHUNK_SIZE)
        .enumerate()
        .map(|(idx, chunk)| {
            let more = usize::from(idx + 1 < total);
            let payload = std::str::from_utf8(chunk).unwrap_or_default();
            if idx == 0 {
                format!("\x1b_Ga=T,f=100,c={cols},r={rows},m={more},q=2;{payload}\x1b\\")
            } else {
                format!("\x1b_Gm={more};{payload}\x1b\\")
            }
        })
        .collect()
}

/// Encode PNG bytes as a concatenated Kitty APC escape payload.
pub fn encode_kitty_image(data: &[u8], geometry: ThumbnailGeometry) -> String {
    encode_kitty_chunks(data, geometry).concat()
}

/// Encode image bytes using iTerm2 OSC 1337 inline file protocol (`\x1b]1337;File=inline=1;...\x07`).
pub fn encode_iterm2_image(data: &[u8], name: Option<&str>, geometry: ThumbnailGeometry) -> String {
    if data.is_empty() {
        return String::new();
    }
    let cols = geometry.cols.max(1);
    let rows = geometry.rows.clamp(1, MAX_THUMBNAIL_ROWS);
    let encoded_data = BASE64_STANDARD.encode(data);
    let mut params = format!(
        "inline=1;size={};width={cols};height={rows};preserveAspectRatio=1",
        data.len()
    );
    if let Some(clean) = name.map(sanitize_label).filter(|s| !s.is_empty()) {
        params.push_str(";name=");
        params.push_str(&BASE64_STANDARD.encode(clean.as_bytes()));
    }
    format!("\x1b]1337;File={params}:{encoded_data}\x07")
}

/// Encode an inline image for `capability`, returning `None` when text-badge fallback is required.
pub fn encode_inline_image(
    capability: GraphicsCapability,
    data: &[u8],
    label: Option<&str>,
    geometry: ThumbnailGeometry,
) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    match capability {
        GraphicsCapability::None => None,
        GraphicsCapability::Kitty => {
            if parse_image_dimensions(data).is_some_and(|d| d.format != ImageFormat::Png) {
                return None;
            }
            Some(encode_kitty_image(data, geometry))
        }
        GraphicsCapability::ITerm2 => Some(encode_iterm2_image(data, label, geometry)),
    }
}

/// Format a calm text-only badge fallback for terminals without inline image support.
pub fn format_image_badge(
    label: &str,
    dimensions: Option<ImageDimensions>,
    byte_len: usize,
) -> String {
    let clean = sanitize_label(label);
    let mut parts = Vec::new();
    if !clean.is_empty() {
        parts.push(clean);
    }
    if let Some(dims) = dimensions {
        parts.push(format!("{}x{}", dims.width, dims.height));
    }
    if byte_len > 0 {
        parts.push(format_byte_size(byte_len));
    }
    if parts.is_empty() {
        "[image]".to_string()
    } else {
        format!("[image: {}]", parts.join(" · "))
    }
}

fn sanitize_label(label: &str) -> String {
    let mut out = String::with_capacity(label.len().min(128));
    let mut prev_space = false;
    for ch in label.chars() {
        if out.chars().count() >= 128 {
            break;
        }
        if ch.is_control() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
            continue;
        }
        prev_space = ch == ' ';
        out.push(ch);
    }
    out.trim().to_string()
}

fn format_byte_size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let head = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR";
        [head.as_slice(), &w.to_be_bytes(), &h.to_be_bytes()].concat()
    }

    fn sample_jpeg(w: u16, h: u16) -> Vec<u8> {
        let app0 = [0xff, 0xd8, 0xff, 0xe0, 0x00, 0x06, b'J', b'F', b'I', b'F'];
        let sof0 = [0xff, 0xc0, 0x00, 0x0b, 0x08];
        let tail = [0x03, 0x01, 0x11, 0x00, 0xff, 0xd9];
        [&app0[..], &sof0, &h.to_be_bytes(), &w.to_be_bytes(), &tail].concat()
    }

    fn sample_gif(w: u16, h: u16) -> Vec<u8> {
        [b"GIF89a".as_slice(), &w.to_le_bytes(), &h.to_le_bytes()].concat()
    }

    fn sample_webp_vp8(w: u16, h: u16) -> Vec<u8> {
        let head = b"RIFF\x16\0\0\0WEBPVP8 \x0a\0\0\0\0\0\0\x9d\x01\x2a";
        [head.as_slice(), &w.to_le_bytes(), &h.to_le_bytes()].concat()
    }

    fn sample_webp_vp8l(w: u32, h: u32) -> Vec<u8> {
        let bits = ((w - 1) & 0x3fff) | (((h - 1) & 0x3fff) << 14);
        let head = b"RIFF\x11\0\0\0WEBPVP8L\x05\0\0\0\x2f";
        [head.as_slice(), &bits.to_le_bytes()].concat()
    }

    fn sample_webp_vp8x(w: u32, h: u32) -> Vec<u8> {
        let (wb, hb) = ((w - 1).to_le_bytes(), (h - 1).to_le_bytes());
        let head = b"RIFF\x16\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0";
        [head.as_slice(), &wb[..3], &hb[..3]].concat()
    }

    #[test]
    fn parse_probe_response_handles_kitty_enoent_xtversion_timeout_and_malformed() {
        use GraphicsCapability as Cap;
        let cases: &[(&[u8], Cap)] = &[
            (b"\x1b_Gi=31;OK\x1b\\", Cap::Kitty),
            (b"\x1b_Gi=31;OK\x07", Cap::Kitty),
            (b"\x1b_Gi=31;OK\x1b\\\x1bP>|iTerm2 3.5.0\x1b\\", Cap::Kitty),
            (b"\x1b_Gi=31;ENOENT:not found\x1b\\", Cap::None),
            (
                b"\x1b_Gi=31;EINVAL:bad\x1b\\\x1bP>|iTerm2 3.5.10\x1b\\",
                Cap::ITerm2,
            ),
            (b"\x1bP>|iTerm2 3.4.23\x1b\\", Cap::ITerm2),
            (b"\x1bP>|tmux 3.4\x1b\\", Cap::None),
            (b"", Cap::None),
            (b"\x1b_Gi=31;OK", Cap::None),
            (b"\x1b_Gi=99;OK\x1b\\", Cap::None),
            (b"\x1b_G\xff\xfe;OK\x1b\\garbage\x1bP>|iTerm2", Cap::None),
        ];
        for &(raw, expected) in cases {
            assert_eq!(parse_probe_response(raw), expected, "raw={raw:?}");
        }
    }

    #[test]
    fn dumb_theme_and_non_tty_skip_probe_immediately() {
        assert!(should_skip_probe(ThemeMode::Dumb, "xterm-256color", true));
        assert!(should_skip_probe(ThemeMode::Auto, "dumb", true));
        assert!(should_skip_probe(ThemeMode::Auto, "DUMB", true));
        assert!(should_skip_probe(ThemeMode::Auto, "xterm-256color", false));
        assert!(!should_skip_probe(ThemeMode::Auto, "xterm-256color", true));
        assert_eq!(
            probe_terminal_graphics(ThemeMode::Dumb),
            GraphicsCapability::None
        );
    }

    #[test]
    fn parses_png_jpeg_gif_webp_headers_and_rejects_malformed() {
        use ImageFormat as Fmt;
        let cases = [
            (sample_png(1280, 720), 1280, 720, Fmt::Png, "image/png"),
            (sample_jpeg(640, 480), 640, 480, Fmt::Jpeg, "image/jpeg"),
            (sample_gif(320, 240), 320, 240, Fmt::Gif, "image/gif"),
            (sample_webp_vp8(800, 600), 800, 600, Fmt::WebP, "image/webp"),
            (
                sample_webp_vp8l(512, 256),
                512,
                256,
                Fmt::WebP,
                "image/webp",
            ),
            (
                sample_webp_vp8x(1024, 768),
                1024,
                768,
                Fmt::WebP,
                "image/webp",
            ),
        ];
        for (bytes, width, height, format, mime) in cases {
            let dims = parse_image_dimensions(&bytes).expect("valid header");
            assert_eq!(
                dims,
                ImageDimensions {
                    width,
                    height,
                    format
                }
            );
            assert_eq!(dims.format.mime_type(), mime);
        }
        for invalid in [b"".as_slice(), b"not an image", &sample_png(0, 100)] {
            assert_eq!(parse_image_dimensions(invalid), None);
        }
    }

    #[test]
    fn thumbnail_geometry_preserves_aspect_ratio_and_clamps_rows_to_12() {
        let dim = |w, h| {
            Some(ImageDimensions {
                width: w,
                height: h,
                format: ImageFormat::Png,
            })
        };
        let cases = [
            (dim(1000, 1000), 80, 40, (24, 12)),
            (dim(1920, 1080), 80, 24, (43, 12)),
            (dim(2000, 200), 80, 12, (80, 4)),
            (dim(200, 2000), 80, 100, (2, 12)),
        ];
        for (dims, max_c, max_r, (cols, rows)) in cases {
            assert_eq!(
                compute_thumbnail_geometry(dims, max_c, max_r),
                ThumbnailGeometry { cols, rows }
            );
        }
        let fallback = compute_thumbnail_geometry(None, 80, 50);
        assert!((1..=MAX_THUMBNAIL_ROWS).contains(&fallback.rows) && fallback.cols >= 1);
    }

    #[test]
    fn kitty_apc_chunks_enforce_4096_limit_and_m1_to_m0_sequence() {
        let payload = vec![0xABu8; 7000];
        let chunks = encode_kitty_chunks(&payload, ThumbnailGeometry { cols: 40, rows: 12 });
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].starts_with("\x1b_Ga=T,f=100,c=40,r=12,m=1,q=2;"));
        assert!(chunks[1].starts_with("\x1b_Gm=1;"));
        assert!(chunks[2].starts_with("\x1b_Gm=0;"));

        let mut reassembled = String::new();
        for chunk in &chunks {
            let inner = chunk
                .strip_prefix("\x1b_G")
                .and_then(|s| s.strip_suffix("\x1b\\"))
                .unwrap();
            let (_, b64_part) = inner.split_once(';').unwrap();
            assert!(b64_part.len() <= KITTY_CHUNK_SIZE);
            reassembled.push_str(b64_part);
        }
        assert_eq!(BASE64_STANDARD.decode(reassembled).unwrap(), payload);

        let small = encode_kitty_chunks(b"tiny", ThumbnailGeometry { cols: 10, rows: 25 });
        assert_eq!(small.len(), 1);
        assert!(small[0].starts_with("\x1b_Ga=T,f=100,c=10,r=12,m=0,q=2;"));
    }

    #[test]
    fn iterm2_osc_1337_framing_and_text_badge_fallback() {
        use GraphicsCapability as Cap;
        let png = sample_png(640, 480);
        let geom = ThumbnailGeometry { cols: 32, rows: 12 };
        let osc = encode_iterm2_image(&png, Some("chart\x1b[31m.png"), geom);
        let prefix =
            "\x1b]1337;File=inline=1;size=24;width=32;height=12;preserveAspectRatio=1;name=";
        assert!(osc.starts_with(prefix));
        assert!(osc.ends_with(&format!(":{}\x07", BASE64_STANDARD.encode(&png))));

        assert_eq!(
            encode_inline_image(Cap::None, &png, Some("a.png"), geom),
            None
        );
        assert!(encode_inline_image(Cap::Kitty, &png, Some("a.png"), geom).is_some());
        let jpeg = sample_jpeg(800, 600);
        assert_eq!(
            encode_inline_image(Cap::Kitty, &jpeg, Some("a.jpg"), geom),
            None
        );
        assert!(encode_inline_image(Cap::ITerm2, &jpeg, Some("a.jpg"), geom).is_some());

        let badge = format_image_badge(
            "diagram\x1b]0;bad\x07.png",
            parse_image_dimensions(&png),
            2048,
        );
        assert_eq!(badge, "[image: diagram ]0;bad .png · 640x480 · 2.0 KB]");
        assert_eq!(format_image_badge("", None, 0), "[image]");
    }
}
