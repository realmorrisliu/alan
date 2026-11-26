//! Immutable scrollback emission for inline images and companion text badges.
//!
//! Inline graphic escape sequences (Kitty Graphics Protocol APC and iTerm2 OSC 1337) are emitted
//! strictly during immutable scrollback commit (`Terminal::insert_before`), never during live
//! inline viewport redraws in `layout.rs`.

use anyhow::{Context, Result};
use crossterm::cursor::{MoveTo, RestorePosition, SavePosition};
use crossterm::execute;
use ratatui::style::Style;
use ratatui::text::Line;
use std::io::Write;

use crate::graphics::{
    GraphicsCapability, ImageDimensions, ImageFormat, MAX_THUMBNAIL_ROWS,
    compute_thumbnail_geometry, encode_inline_image, format_image_badge, parse_image_dimensions,
};
use crate::terminal::TerminalSession;
use crate::theme::{Theme, ThemeMode};

/// An immutable scrollback item committed above the inline viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollbackItem {
    /// One or more styled text lines committed via normal Ratatui scrollback rendering.
    Lines(Vec<Line<'static>>),
    /// An inline image committed atomically with optional terminal graphics + companion badge line.
    Image {
        bytes: Vec<u8>,
        label: String,
        dimensions: Option<ImageDimensions>,
    },
}

/// Planned scrollback emission for a single [`ScrollbackItem::Image`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageScrollbackPlan {
    /// Blank scrollback rows reserved via `insert_before` (`0` for text-badge fallback).
    pub reserved_rows: u16,
    /// Kitty APC or iTerm2 OSC 1337 payload, or `None` when text-badge fallback is required.
    pub escape_payload: Option<String>,
    /// Companion text badge (`[image: label · WxH · size]`), always emitted.
    pub badge_text: String,
}

/// Plan scrollback reservation rows, escape payload, and companion badge for an image item.
///
/// Emits `0` graphic rows and `None` escape payload whenever `capability == None`,
/// `theme_mode == ThemeMode::Dumb`, `bytes` is empty, available rows are `0`, or the format is
/// unsupported by `capability` (for example non-PNG bytes on Kitty).
pub fn plan_image_scrollback(
    capability: GraphicsCapability,
    theme_mode: ThemeMode,
    max_cols: u16,
    max_rows: u16,
    bytes: &[u8],
    label: &str,
    dimensions: Option<ImageDimensions>,
) -> ImageScrollbackPlan {
    let dims = dimensions.or_else(|| parse_image_dimensions(bytes));
    let badge_text = format_image_badge(label, dims, bytes.len());
    let badge_only = ImageScrollbackPlan {
        reserved_rows: 0,
        escape_payload: None,
        badge_text: badge_text.clone(),
    };

    if theme_mode == ThemeMode::Dumb
        || capability == GraphicsCapability::None
        || bytes.is_empty()
        || max_cols == 0
        || max_rows == 0
    {
        return badge_only;
    }
    if capability == GraphicsCapability::Kitty && dims.is_some_and(|d| d.format != ImageFormat::Png)
    {
        return badge_only;
    }

    let geometry = compute_thumbnail_geometry(dims, max_cols, max_rows.min(MAX_THUMBNAIL_ROWS));
    let Some(payload) = encode_inline_image(capability, bytes, Some(label), geometry) else {
        return badge_only;
    };
    ImageScrollbackPlan {
        reserved_rows: geometry.rows,
        escape_payload: Some(payload),
        badge_text,
    }
}

/// Build the styled companion badge line (`[image: label · WxH · size]`).
pub fn badge_line(badge_text: &str, theme_mode: ThemeMode) -> Line<'static> {
    let style = if theme_mode == ThemeMode::Dumb {
        Style::default()
    } else {
        let theme = match theme_mode {
            ThemeMode::Paper => Theme::paper(),
            ThemeMode::Ansi => Theme::ansi(),
            ThemeMode::Dumb => Theme::dumb(),
            ThemeMode::Auto | ThemeMode::Midnight => Theme::midnight(),
        };
        crate::history::metadata_style(theme)
    };
    Line::styled(crate::history::clean_text(badge_text), style)
}

/// Write a Kitty APC or iTerm2 OSC 1337 payload into reserved scrollback rows starting at
/// `(0, image_top_y)`, saving and restoring the cursor position cleanly.
pub(crate) fn write_image_escape_at_row<W: Write>(
    writer: &mut W,
    image_top_y: u16,
    payload: &str,
) -> std::io::Result<()> {
    execute!(writer, SavePosition, MoveTo(0, image_top_y))?;
    writer.write_all(payload.as_bytes())?;
    execute!(writer, RestorePosition)?;
    writer.flush()
}

/// Commit typed scrollback items (lines and inline images) into the active terminal session.
pub(crate) fn write_scrollback_items(
    session: &mut TerminalSession,
    items: &[ScrollbackItem],
) -> Result<()> {
    for item in items {
        match item {
            ScrollbackItem::Lines(lines) => session.write_scrollback(lines)?,
            ScrollbackItem::Image {
                bytes,
                label,
                dimensions,
            } => emit_scrollback_image(session, bytes, label, *dimensions)?,
        }
    }
    Ok(())
}

fn emit_scrollback_image(
    session: &mut TerminalSession,
    bytes: &[u8],
    label: &str,
    dimensions: Option<ImageDimensions>,
) -> Result<()> {
    let size = session
        .terminal
        .size()
        .context("failed to read terminal size for inline image scrollback")?;
    let plan = plan_image_scrollback(
        session.graphics_capability,
        session.theme_mode,
        size.width,
        MAX_THUMBNAIL_ROWS.min(size.height),
        bytes,
        label,
        dimensions,
    );

    if plan.reserved_rows > 0
        && let Some(payload) = plan.escape_payload
    {
        let link_sink = session.terminal.backend().link_sink();
        if let Ok(mut guard) = link_sink.lock() {
            *guard = crate::hyperlink::CellLinkMap::default();
        }
        let image_sink = session.terminal.backend().image_sink();
        if let Ok(mut guard) = image_sink.lock() {
            *guard = Some(payload);
        }
        let insert_result = session
            .terminal
            .insert_before(plan.reserved_rows, |_buf| {});
        if let Ok(mut guard) = image_sink.lock() {
            *guard = None;
        }
        insert_result.context("failed to reserve scrollback rows for inline image")?;
    }

    let badge = badge_line(&plan.badge_text, session.theme_mode);
    session.write_scrollback(&[badge])
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
        [
            app0.as_slice(),
            sof0.as_slice(),
            &h.to_be_bytes(),
            &w.to_be_bytes(),
            tail.as_slice(),
        ]
        .concat()
    }

    #[test]
    fn kitty_capable_plans_reserved_rows_apc_payload_and_companion_badge() {
        let png = sample_png(800, 600);
        let plan = plan_image_scrollback(
            GraphicsCapability::Kitty,
            ThemeMode::Auto,
            80,
            MAX_THUMBNAIL_ROWS,
            &png,
            "chart.png",
            None,
        );
        assert!(plan.reserved_rows >= 1 && plan.reserved_rows <= MAX_THUMBNAIL_ROWS);
        let payload = plan.escape_payload.expect("kitty APC payload");
        assert!(payload.starts_with("\x1b_Ga=T,f=100,"));
        assert!(payload.ends_with("\x1b\\"));
        assert_eq!(plan.badge_text, "[image: chart.png · 800x600 · 24 B]");
    }

    #[test]
    fn iterm2_capable_plans_osc_1337_for_png_and_jpeg() {
        let jpeg = sample_jpeg(640, 480);
        let plan = plan_image_scrollback(
            GraphicsCapability::ITerm2,
            ThemeMode::Midnight,
            80,
            MAX_THUMBNAIL_ROWS,
            &jpeg,
            "photo.jpg",
            None,
        );
        assert!(plan.reserved_rows >= 1 && plan.reserved_rows <= MAX_THUMBNAIL_ROWS);
        let payload = plan.escape_payload.expect("iTerm2 OSC 1337 payload");
        assert!(payload.starts_with("\x1b]1337;File=inline=1;"));
        assert!(payload.ends_with('\x07'));
        assert!(plan.badge_text.contains("photo.jpg"));
        assert!(plan.badge_text.contains("640x480"));
    }

    #[test]
    fn none_capability_dumb_theme_and_non_png_on_kitty_emit_badge_only_with_zero_escapes() {
        let png = sample_png(320, 240);
        let jpeg = sample_jpeg(320, 240);

        let none_plan = plan_image_scrollback(
            GraphicsCapability::None,
            ThemeMode::Auto,
            80,
            MAX_THUMBNAIL_ROWS,
            &png,
            "shot.png",
            None,
        );
        assert_eq!(none_plan.reserved_rows, 0);
        assert!(none_plan.escape_payload.is_none());
        assert_eq!(none_plan.badge_text, "[image: shot.png · 320x240 · 24 B]");

        for cap in [GraphicsCapability::Kitty, GraphicsCapability::ITerm2] {
            let dumb_plan = plan_image_scrollback(
                cap,
                ThemeMode::Dumb,
                80,
                MAX_THUMBNAIL_ROWS,
                &png,
                "shot.png",
                None,
            );
            assert_eq!(dumb_plan.reserved_rows, 0);
            assert!(dumb_plan.escape_payload.is_none());
            assert_eq!(dumb_plan.badge_text, "[image: shot.png · 320x240 · 24 B]");
        }

        let kitty_jpeg = plan_image_scrollback(
            GraphicsCapability::Kitty,
            ThemeMode::Auto,
            80,
            MAX_THUMBNAIL_ROWS,
            &jpeg,
            "photo.jpg",
            None,
        );
        assert_eq!(kitty_jpeg.reserved_rows, 0);
        assert!(kitty_jpeg.escape_payload.is_none());
        assert!(kitty_jpeg.badge_text.contains("photo.jpg"));
    }

    #[test]
    fn write_image_escape_at_row_positions_and_restores_cursor_cleanly() {
        let png = sample_png(100, 100);
        let plan = plan_image_scrollback(
            GraphicsCapability::Kitty,
            ThemeMode::Auto,
            80,
            MAX_THUMBNAIL_ROWS,
            &png,
            "square.png",
            None,
        );
        let payload = plan.escape_payload.as_deref().expect("kitty payload");

        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        );
        // Simulate inline viewport top sitting at row 16 after reserving `plan.reserved_rows`.
        let viewport_top: u16 = 16;
        let image_top_y = viewport_top.saturating_sub(plan.reserved_rows);
        parser.process(format!("\x1b[{};1H", viewport_top + 1).as_bytes());
        assert_eq!(parser.screen().cursor_position(), (viewport_top, 0));

        let mut raw = Vec::new();
        write_image_escape_at_row(&mut raw, image_top_y, payload).unwrap();
        assert!(
            raw.windows(payload.len()).any(|w| w == payload.as_bytes()),
            "payload must be emitted verbatim"
        );
        parser.process(&raw);
        // Cursor must return exactly to viewport_top so Viewport::Inline tracking stays exact.
        assert_eq!(parser.screen().cursor_position(), (viewport_top, 0));
    }

    #[test]
    fn zero_available_rows_falls_back_to_badge_only() {
        let png = sample_png(200, 200);
        let plan = plan_image_scrollback(
            GraphicsCapability::Kitty,
            ThemeMode::Auto,
            80,
            0,
            &png,
            "full.png",
            None,
        );
        assert_eq!(plan.reserved_rows, 0);
        assert!(plan.escape_payload.is_none());
        assert!(plan.badge_text.contains("full.png"));
    }

    #[test]
    fn hyperlink_backend_emits_pending_image_once_and_zero_escapes_on_live_redraw() {
        use std::sync::{Arc, Mutex};

        #[derive(Clone, Default)]
        struct SharedVec(Arc<Mutex<Vec<u8>>>);
        impl Write for SharedVec {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let png = sample_png(640, 480);
        let plan = plan_image_scrollback(
            GraphicsCapability::ITerm2,
            ThemeMode::Auto,
            80,
            MAX_THUMBNAIL_ROWS,
            &png,
            "inline.png",
            None,
        );
        let payload = plan.escape_payload.expect("iterm2 payload");

        let captured = SharedVec::default();
        let backend = crate::hyperlink::HyperlinkBackend::new(captured.clone());
        let image_sink = backend.image_sink();
        let area = ratatui::layout::Rect::new(0, 2, 40, 4);
        let mut terminal = ratatui::Terminal::with_options(
            backend,
            ratatui::TerminalOptions {
                viewport: ratatui::Viewport::Fixed(area),
            },
        )
        .unwrap();

        *image_sink.lock().unwrap() = Some(payload.clone());
        terminal
            .draw(|frame| {
                // Force cell mutation so Backend::draw runs on rows starting at area.y (2).
                frame.render_widget(ratatui::widgets::Paragraph::new("."), frame.area());
            })
            .unwrap();
        let after_first = captured.0.lock().unwrap().len();
        let first_bytes = captured.0.lock().unwrap().clone();
        assert!(
            first_bytes
                .windows(payload.len())
                .any(|w| w == payload.as_bytes()),
            "first draw must emit queued image escape payload"
        );
        // Must position at row 3 (1-based for area.y == 2) inside Save/Restore cursor.
        assert!(
            first_bytes.windows(6).any(|w| w == b"\x1b[3;1H"),
            "expected MoveTo(0, 2) before image escape"
        );

        // Subsequent live redraw must emit zero image escapes and leave vt100 screen clean.
        terminal
            .draw(|frame| {
                frame.render_widget(ratatui::widgets::Paragraph::new("LIVE-OK"), frame.area());
                frame.set_cursor_position((area.x + 7, area.y));
            })
            .unwrap();
        let second_bytes = captured.0.lock().unwrap()[after_first..].to_vec();
        assert!(
            !second_bytes.windows(7).any(|w| w == b"\x1b]1337;"),
            "subsequent live redraw must emit zero OSC 1337 escapes"
        );
        let mut parser = vt100::Parser::new(
            std::num::NonZeroU16::new(24).unwrap(),
            std::num::NonZeroU16::new(80).unwrap(),
            0,
        );
        parser.process(&captured.0.lock().unwrap());
        assert!(parser.screen().contents().contains("LIVE-OK"));
        assert_eq!(parser.screen().cursor_position(), (area.y, area.x + 7));
    }
}
