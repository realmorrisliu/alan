use super::*;

#[test]
fn complete_frame_transaction_orders_native_boundaries_and_closes_on_errors() {
    // Inject only IO at the existing frame owner, not renderer geometry.
    for failure in [
        None,
        Some("begin"),
        Some("scrollback"),
        Some("clear"),
        Some("draw"),
        Some("end"),
    ] {
        let mut output = Vec::new();
        let result = synchronized_frame(
            &mut output,
            |out, begin| {
                synchronized_boundary(out, begin)?;
                let stage = if begin { "begin" } else { "end" };
                anyhow::ensure!(failure != Some(stage), "{stage} failed");
                Ok(())
            },
            |out| {
                for stage in ["scrollback", "clear", "draw"] {
                    out.extend_from_slice(stage.as_bytes());
                    anyhow::ensure!(failure != Some(stage), "{stage} failed");
                }
                Ok(())
            },
        );
        assert!(
            output.starts_with(b"\x1b[?2026h"),
            "{failure:?}: {output:?}"
        );
        assert!(output.ends_with(b"\x1b[?2026l"), "{failure:?}: {output:?}");
        match failure {
            None => {
                assert!(result.is_ok());
                assert_eq!(output, b"\x1b[?2026hscrollbackcleardraw\x1b[?2026l");
            }
            Some(stage) => assert!(format!("{:#}", result.unwrap_err()).contains(stage)),
        }
    }
    let error = synchronized_frame(
        &mut (),
        |_, begin| {
            if begin {
                Ok(())
            } else {
                anyhow::bail!("cleanup failed")
            }
        },
        |_| anyhow::bail!("original draw failed"),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("original draw failed"));
    assert!(format!("{error:#}").contains("cleanup failed"));
}

#[test]
fn terminal_error_names_bare_alan_contract() {
    assert!(terminal_capability_error().contains("bare `alan`"));
    assert!(!terminal_capability_error().contains("alan-tui"));
}

#[test]
fn terminal_restoration_ends_the_inline_prompt_line() {
    let mut output = Vec::new();

    restore_terminal_input_and_line(&mut output).unwrap();

    assert_eq!(output, b"\x1b[?2004l\x1b[1E");
}

#[test]
fn kitty_keyboard_flags_push_and_pop_standard_escape_sequences() {
    assert!(!should_enable_kitty_keyboard("dumb", true));
    assert!(!should_enable_kitty_keyboard("xterm-256color", false));
    assert!(should_enable_kitty_keyboard("xterm-256color", true));

    let mut pushed = Vec::new();
    push_kitty_keyboard_flags(&mut pushed).unwrap();
    assert_eq!(pushed, b"\x1b[>1u");

    let mut popped = Vec::new();
    pop_kitty_keyboard_flags(&mut popped).unwrap();
    assert_eq!(popped, b"\x1b[<1u");
}

#[test]
fn osc_title_strips_control_sequences_and_formats_cleanly() {
    assert_eq!(
        format_osc_title(Some("my\x1b]0;bad\x07repo"), "running\ntools"),
        "alan · my ]0;bad repo · running tools"
    );
    let mut out = Vec::new();
    write_osc_title_to(&mut out, "alan · repo · ready").unwrap();
    assert_eq!(out, b"\x1b]0;alan \xc2\xb7 repo \xc2\xb7 ready\x07");
}

#[test]
fn inline_clear_y_clears_upward_expansion_origins() {
    // Welcome drained to rows 0..3 -> top=4; /help grows viewport to 24 rows -> must clear from 0.
    assert_eq!(inline_clear_y(4, 24, 24), 0);
    // Partial upward growth from row 12 to 22-row viewport at 24-row screen -> clear from row 2.
    assert_eq!(inline_clear_y(12, 24, 22), 2);
    // Shrinking from row 2 to 5 rows -> keeps clearing from row 2 downward.
    assert_eq!(inline_clear_y(2, 24, 5), 2);
    // Zero-anchored expansion stays at 0.
    assert_eq!(inline_clear_y(0, 24, 15), 0);
}

#[test]
fn clear_startup_screen_clears_visible_screen_without_purging_scrollback() {
    let mut vt_out = Vec::new();
    clear_startup_screen(&mut vt_out, "xterm-256color", true).unwrap();
    assert_eq!(vt_out, b"\x1b[2J\x1b[1;1H");
    assert!(!vt_out.windows(4).any(|w| w == b"\x1b[3J"));

    for (term, interactive) in [("dumb", true), ("DUMB", true), ("xterm-256color", false)] {
        let mut out = Vec::new();
        clear_startup_screen(&mut out, term, interactive).unwrap();
        assert!(
            out.is_empty(),
            "expected no bytes for ({term}, {interactive})"
        );
    }
}

#[test]
fn prepare_inline_height_transition_handles_fullscreen_upward_scroll_and_in_bounds() {
    // Case 1a: expanding to full-screen (`new_height == screen_height`) with committed rows
    // above `top` clears the old live viewport, scrolls committed rows into scrollback, and
    // clears from row 0 for the rebuilt full-screen viewport.
    let mut fullscreen = Vec::new();
    prepare_inline_height_transition(&mut fullscreen, 4, 24, 24).unwrap();
    assert_eq!(
        fullscreen,
        b"\x1b[5;1H\x1b[J\x1b[24;1H\n\n\n\n\x1b[1;1H\x1b[J\x1b[1;1H"
    );

    let mut fs_parser = vt100::Parser::new(
        std::num::NonZeroU16::new(24).unwrap(),
        std::num::NonZeroU16::new(80).unwrap(),
        32,
    );
    for row in 0..4 {
        fs_parser.process(format!("banner-{row:02}\r\n").as_bytes());
    }
    for row in 4..8 {
        fs_parser.process(format!("old-live-{row:02}\r\n").as_bytes());
    }
    fs_parser.process(&fullscreen);
    assert_eq!(fs_parser.screen().cursor_position(), (0, 0));
    let (fs_sb, fs_scr) = dump_vt_parser(&mut fs_parser);
    assert_eq!(
        fs_sb,
        vec!["banner-00", "banner-01", "banner-02", "banner-03"]
    );
    assert!(
        fs_scr.iter().all(|row| row.is_empty()),
        "fullscreen transition left stale rows on active screen: {fs_scr:?}"
    );

    // Case 1b: full-screen transition when `top == 0` clears from row 0 without extra scrolling.
    let mut zero_top_fullscreen = Vec::new();
    prepare_inline_height_transition(&mut zero_top_fullscreen, 0, 24, 24).unwrap();
    assert_eq!(zero_top_fullscreen, b"\x1b[1;1H\x1b[J\x1b[1;1H");

    // Case 2: partial upward expansion clears old live viewport, scrolls committed rows up,
    // and leaves the rebuilt viewport anchored at clear_y.
    let mut partial = Vec::new();
    prepare_inline_height_transition(&mut partial, 12, 24, 15).unwrap();
    assert_eq!(
        partial,
        b"\x1b[13;1H\x1b[J\x1b[24;1H\n\n\n\x1b[10;1H\x1b[J\x1b[10;1H"
    );

    let mut parser = vt100::Parser::new(
        std::num::NonZeroU16::new(24).unwrap(),
        std::num::NonZeroU16::new(80).unwrap(),
        16,
    );
    for row in 0..12 {
        parser.process(format!("committed-{row:02}\r\n").as_bytes());
    }
    for row in 12..16 {
        parser.process(format!("old-live-{row:02}\r\n").as_bytes());
    }
    parser.process(&partial);
    assert_eq!(parser.screen().cursor_position(), (9, 0));
    let active = parser.screen().contents();
    assert!(
        !active.contains("old-live-"),
        "old live viewport leaked:\n{active}"
    );
    for row in 3..12 {
        assert!(
            active.contains(&format!("committed-{row:02}")),
            "missing committed-{row:02} on active screen:\n{active}"
        );
    }
    parser.screen_mut().set_scrollback(usize::MAX);
    let scrolled = parser.screen().contents();
    for row in 0..3 {
        assert!(
            scrolled.contains(&format!("committed-{row:02}")),
            "missing committed-{row:02} in scrollback:\n{scrolled}"
        );
    }

    // Case 3: in-bounds growth and shrink keep clearing from `top` without scrolling.
    let mut in_bounds_grow = Vec::new();
    prepare_inline_height_transition(&mut in_bounds_grow, 10, 24, 8).unwrap();
    assert_eq!(in_bounds_grow, b"\x1b[11;1H\x1b[J\x1b[11;1H");

    let mut in_bounds_shrink = Vec::new();
    prepare_inline_height_transition(&mut in_bounds_shrink, 2, 24, 5).unwrap();
    assert_eq!(in_bounds_shrink, b"\x1b[3;1H\x1b[J\x1b[3;1H");
}

#[derive(Clone)]
struct VtWriter(std::sync::Arc<std::sync::Mutex<vt100::Parser>>);

impl Write for VtWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().process(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct VtBackend {
    inner: ratatui::backend::CrosstermBackend<VtWriter>,
    parser: std::sync::Arc<std::sync::Mutex<vt100::Parser>>,
    link_sink: std::sync::Arc<std::sync::Mutex<crate::hyperlink::CellLinkMap>>,
    image_sink: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    last_cursor_pos: Option<ratatui::layout::Position>,
}

impl VtBackend {
    fn new(parser: std::sync::Arc<std::sync::Mutex<vt100::Parser>>) -> Self {
        Self {
            inner: ratatui::backend::CrosstermBackend::new(VtWriter(parser.clone())),
            parser,
            link_sink: std::sync::Arc::new(std::sync::Mutex::new(
                crate::hyperlink::CellLinkMap::default(),
            )),
            image_sink: std::sync::Arc::new(std::sync::Mutex::new(None)),
            last_cursor_pos: None,
        }
    }
}

impl Write for VtBackend {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

impl ratatui::backend::Backend for VtBackend {
    type Error = std::io::Error;

    fn draw<'a, I>(&mut self, content: I) -> std::io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        let mut last = None;
        let res = self.inner.draw(content.inspect(|&(x, y, _)| {
            last = Some(ratatui::layout::Position::new(x, y));
        }));
        if res.is_ok()
            && let Some(pos) = last
        {
            self.last_cursor_pos = Some(pos);
        }
        res
    }

    fn append_lines(&mut self, n: u16) -> std::io::Result<()> {
        self.inner.append_lines(n)
    }

    fn hide_cursor(&mut self) -> std::io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> std::io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> std::io::Result<ratatui::layout::Position> {
        let guard = self.parser.lock().unwrap();
        let (y, x) = guard.screen().cursor_position();
        Ok(ratatui::layout::Position::new(x, y))
    }

    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> std::io::Result<()> {
        let pos = position.into();
        let res = self.inner.set_cursor_position(pos);
        if res.is_ok() {
            self.last_cursor_pos = Some(pos);
        }
        res
    }

    fn clear(&mut self) -> std::io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> std::io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> std::io::Result<ratatui::layout::Size> {
        let guard = self.parser.lock().unwrap();
        let (rows, cols) = guard.screen().size();
        Ok(ratatui::layout::Size::new(cols.get(), rows.get()))
    }

    fn window_size(&mut self) -> std::io::Result<ratatui::backend::WindowSize> {
        let size = self.size()?;
        Ok(ratatui::backend::WindowSize {
            columns_rows: size,
            pixels: ratatui::layout::Size::new(0, 0),
        })
    }

    fn flush(&mut self) -> std::io::Result<()> {
        ratatui::backend::Backend::flush(&mut self.inner)
    }
}

impl InlineSessionBackend for VtBackend {
    fn link_sink(&self) -> std::sync::Arc<std::sync::Mutex<crate::hyperlink::CellLinkMap>> {
        self.link_sink.clone()
    }

    fn image_sink(&self) -> std::sync::Arc<std::sync::Mutex<Option<String>>> {
        self.image_sink.clone()
    }

    fn last_cursor_pos(&self) -> Option<ratatui::layout::Position> {
        self.last_cursor_pos
    }

    fn rebuild_inline(&self, height: u16) -> Result<Terminal<Self>> {
        Terminal::with_options(
            VtBackend::new(self.parser.clone()),
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )
        .context("failed to rebuild VtBackend inline terminal")
    }
}

fn dump_vt_parser(guard: &mut vt100::Parser) -> (Vec<String>, Vec<String>) {
    let (rows, cols) = guard.screen().size();
    let rows = rows.get() as usize;
    let cols = cols.get();
    guard.screen_mut().set_scrollback(usize::MAX);
    let mut pos = guard.screen().scrollback();
    let mut scrollback = Vec::new();
    while pos > 0 && rows > 0 {
        guard.screen_mut().set_scrollback(pos);
        let actual = guard.screen().scrollback();
        let take = actual.min(rows);
        scrollback.extend(
            guard
                .screen()
                .rows(0, cols)
                .take(take)
                .map(|r| r.trim_end().to_string()),
        );
        pos = actual.saturating_sub(take);
    }
    guard.screen_mut().set_scrollback(0);
    let screen = guard
        .screen()
        .rows(0, cols)
        .map(|r| r.trim_end().to_string())
        .collect();
    (scrollback, screen)
}

fn dump_vt(parser: &std::sync::Mutex<vt100::Parser>) -> (Vec<String>, Vec<String>) {
    let mut guard = parser.lock().unwrap();
    dump_vt_parser(&mut guard)
}

fn new_vt_session(
    rows: u16,
    cols: u16,
    scrollback_len: usize,
    initial_height: u16,
) -> (
    std::sync::Arc<std::sync::Mutex<vt100::Parser>>,
    InlineSession<VtBackend>,
) {
    use std::num::NonZeroU16;
    use std::sync::{Arc, Mutex};

    let parser = Arc::new(Mutex::new(vt100::Parser::new(
        NonZeroU16::new(rows).unwrap(),
        NonZeroU16::new(cols).unwrap(),
        scrollback_len,
    )));
    let terminal = Terminal::with_options(
        VtBackend::new(parser.clone()),
        TerminalOptions {
            viewport: Viewport::Inline(initial_height),
        },
    )
    .unwrap();
    let session = InlineSession::from_inline_terminal(terminal, initial_height).unwrap();
    (parser, session)
}

fn prefixed_lines(prefix: &str, range: std::ops::Range<usize>) -> Vec<Line<'static>> {
    range
        .map(|i| Line::from(format!("{prefix}-{i:02}")))
        .collect()
}

fn draw_labeled_live(frame: &mut Frame<'_>, prefix: &str, cursor_offset: Option<u16>) {
    let area = frame.area();
    let lines = prefixed_lines(prefix, 0..area.height as usize);
    frame.render_widget(Paragraph::new(lines), area);
    if let Some(offset) = cursor_offset {
        let y = area.y + offset.min(area.height.saturating_sub(1));
        frame.set_cursor_position((area.x + 2.min(area.width.saturating_sub(1)), y));
    }
}

#[test]
fn inline_height_expansion_to_fullscreen_preserves_scrollback_and_shrinks_cleanly() {
    let (parser, mut session) = new_vt_session(24, 40, 128, 4);

    // Commit 6 lines above a 4-row live viewport.
    session
        .draw_inline_frame(&prefixed_lines("COMMIT", 0..6), 4, 4, |f| {
            draw_labeled_live(f, "LIVE1", Some(2));
        })
        .unwrap();

    // Expand live viewport to full screen (new_height == screen_height == 24).
    session
        .draw_inline_frame(&[], 24, 24, |f| {
            draw_labeled_live(f, "FULL", Some(20));
        })
        .unwrap();

    let (sb_full, scr_full) = dump_vt(&parser);
    assert_eq!(
        sb_full,
        vec![
            "COMMIT-00",
            "COMMIT-01",
            "COMMIT-02",
            "COMMIT-03",
            "COMMIT-04",
            "COMMIT-05",
        ]
    );
    assert!(
        !sb_full.iter().any(|l| l.contains("LIVE1-")),
        "old live viewport leaked into scrollback: {sb_full:?}"
    );
    assert_eq!(scr_full[0], "FULL-00");
    assert_eq!(scr_full[23], "FULL-23");

    // Shrinking back after full-screen keeps committed scrollback intact.
    session
        .draw_inline_frame(&[], 4, 4, |f| {
            draw_labeled_live(f, "LIVE2", Some(2));
        })
        .unwrap();

    let (sb_after, scr_after) = dump_vt(&parser);
    assert_eq!(sb_after, sb_full);
    assert_eq!(
        &scr_after[..4],
        &["LIVE2-00", "LIVE2-01", "LIVE2-02", "LIVE2-03"]
    );
}

#[test]
fn partial_height_expansion_with_overflow_scrolls_oldest_committed_rows_only() {
    let (parser, mut session) = new_vt_session(24, 40, 128, 4);

    session
        .draw_inline_frame(&prefixed_lines("COMMIT", 0..12), 4, 4, |f| {
            draw_labeled_live(f, "LIVE1", Some(2));
        })
        .unwrap();

    // Expand from 4 -> 15 rows while 12 committed rows sit at 0..12 (overflow = 3 rows).
    session
        .draw_inline_frame(&[], 15, 15, |f| {
            draw_labeled_live(f, "LIVE2", Some(10));
        })
        .unwrap();

    let (sb, scr) = dump_vt(&parser);
    assert_eq!(sb, vec!["COMMIT-00", "COMMIT-01", "COMMIT-02"]);
    for (row, actual) in scr.iter().enumerate().take(9) {
        assert_eq!(actual, &format!("COMMIT-{:02}", row + 3));
    }
    assert_eq!(scr[9], "LIVE2-00");
    assert_eq!(scr[23], "LIVE2-14");
    assert!(
        !sb.iter().any(|l| l.contains("LIVE1-")) && !scr.iter().any(|l| l.contains("LIVE1-")),
        "old live viewport leaked: sb={sb:?} scr={scr:?}"
    );
}

#[test]
fn same_frame_insert_before_and_height_changes_preserve_ordered_scrollback() {
    let (parser, mut session) = new_vt_session(24, 40, 128, 4);

    // Frame 1: 6 committed lines + 4-row live viewport.
    session
        .draw_inline_frame(&prefixed_lines("MSG", 0..6), 4, 4, |f| {
            draw_labeled_live(f, "L1", Some(1));
        })
        .unwrap();

    // Frame 2: same-frame 6 more committed lines + partial overflow height expansion (4 -> 20).
    session
        .draw_inline_frame(&prefixed_lines("MSG", 6..12), 20, 20, |f| {
            draw_labeled_live(f, "L2", Some(10));
        })
        .unwrap();

    let (sb2, scr2) = dump_vt(&parser);
    assert_eq!(
        sb2,
        (0..8).map(|i| format!("MSG-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(&scr2[..4], &["MSG-08", "MSG-09", "MSG-10", "MSG-11"]);
    assert_eq!(scr2[4], "L2-00");
    assert_eq!(scr2[23], "L2-19");

    // Frame 3: same-frame commit + full-screen expansion (20 -> 24).
    session
        .draw_inline_frame(&prefixed_lines("MSG", 12..14), 24, 24, |f| {
            draw_labeled_live(f, "L3", Some(15));
        })
        .unwrap();

    let (sb3, scr3) = dump_vt(&parser);
    assert_eq!(
        sb3,
        (0..14).map(|i| format!("MSG-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(scr3[0], "L3-00");
    assert_eq!(scr3[23], "L3-23");

    // Frame 4: same-frame commit + shrink from full-screen (24 -> 4).
    session
        .draw_inline_frame(&prefixed_lines("MSG", 14..16), 4, 4, |f| {
            draw_labeled_live(f, "L4", Some(2));
        })
        .unwrap();

    let (sb4, scr4) = dump_vt(&parser);
    assert_eq!(
        sb4,
        (0..16).map(|i| format!("MSG-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(&scr4[..4], &["L4-00", "L4-01", "L4-02", "L4-03"]);
}

#[test]
fn terminal_resize_shrink_preserves_committed_lines_without_dropping_same_frame_writes() {
    use std::num::NonZeroU16;

    // Case 1: shrink 24 -> 16 with `viewport_height <= new_height`, then same-frame insert_before.
    let (parser, mut session) = new_vt_session(24, 40, 128, 6);
    session
        .draw_inline_frame(&prefixed_lines("C", 0..18), 6, 6, |f| {
            draw_labeled_live(f, "LIVE", Some(2));
        })
        .unwrap();

    parser
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(16).unwrap(), NonZeroU16::new(40).unwrap());

    session
        .draw_inline_frame(&prefixed_lines("NEW", 0..2), 6, 6, |f| {
            draw_labeled_live(f, "AFTER", Some(2));
        })
        .unwrap();

    let (sb, scr) = dump_vt(&parser);
    let mut combined = sb;
    combined.extend(scr[..10].iter().cloned());
    let expected: Vec<String> = (0..18)
        .map(|i| format!("C-{i:02}"))
        .chain((0..2).map(|i| format!("NEW-{i:02}")))
        .collect();
    assert_eq!(combined, expected);
    assert_eq!(
        &scr[10..16],
        &[
            "AFTER-00", "AFTER-01", "AFTER-02", "AFTER-03", "AFTER-04", "AFTER-05"
        ]
    );

    // Case 2: shrink 24 -> 12 when `viewport_height` (14) > `new_height` (12) must not push
    // live viewport rows into scrollback.
    let (parser2, mut session2) = new_vt_session(24, 40, 128, 14);
    session2
        .draw_inline_frame(&prefixed_lines("BANNER", 0..6), 14, 14, |f| {
            draw_labeled_live(f, "LIVE5", Some(4));
        })
        .unwrap();

    parser2
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(12).unwrap(), NonZeroU16::new(40).unwrap());

    session2
        .draw_inline_frame(&[], 12, 12, |f| {
            draw_labeled_live(f, "SHRUNK", Some(4));
        })
        .unwrap();

    let (sb2, scr2) = dump_vt(&parser2);
    assert_eq!(
        sb2,
        vec![
            "BANNER-00",
            "BANNER-01",
            "BANNER-02",
            "BANNER-03",
            "BANNER-04",
            "BANNER-05",
        ]
    );
    assert!(
        !sb2.iter().any(|l| l.contains("LIVE5-")),
        "live viewport leaked into scrollback on shrink: {sb2:?}"
    );
    assert_eq!(scr2[0], "SHRUNK-00");
    assert_eq!(scr2[11], "SHRUNK-11");

    // Case 3: shrink 24 -> 14 when previous frame did not call `set_cursor_position`.
    let (parser3, mut session3) = new_vt_session(24, 40, 128, 6);
    session3
        .draw_inline_frame(&prefixed_lines("HEAD", 0..10), 6, 6, |f| {
            draw_labeled_live(f, "FORM", None);
        })
        .unwrap();
    parser3
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(14).unwrap(), NonZeroU16::new(40).unwrap());
    session3
        .draw_inline_frame(&[], 6, 6, |f| {
            draw_labeled_live(f, "FORM2", None);
        })
        .unwrap();
    let (sb3, scr3) = dump_vt(&parser3);
    let mut combined3 = sb3;
    combined3.extend(scr3[..8].iter().cloned());
    assert_eq!(
        combined3,
        (0..10).map(|i| format!("HEAD-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(
        &scr3[8..14],
        &[
            "FORM2-00", "FORM2-01", "FORM2-02", "FORM2-03", "FORM2-04", "FORM2-05"
        ]
    );

    // Case 4: horizontal shrink 80 -> 40 preserves committed rows above the viewport in scrollback.
    let (parser4, mut session4) = new_vt_session(24, 80, 128, 6);
    session4
        .draw_inline_frame(&prefixed_lines("WIDE", 0..6), 6, 6, |f| {
            draw_labeled_live(f, "WLIVE", Some(3));
        })
        .unwrap();
    parser4
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(24).unwrap(), NonZeroU16::new(40).unwrap());
    session4
        .draw_inline_frame(&[], 6, 6, |f| {
            draw_labeled_live(f, "NARROW", Some(3));
        })
        .unwrap();
    let (sb4, scr4) = dump_vt(&parser4);
    assert_eq!(
        sb4,
        vec![
            "WIDE-00", "WIDE-01", "WIDE-02", "WIDE-03", "WIDE-04", "WIDE-05"
        ]
    );
    assert_eq!(
        &scr4[..6],
        &[
            "NARROW-00",
            "NARROW-01",
            "NARROW-02",
            "NARROW-03",
            "NARROW-04",
            "NARROW-05"
        ]
    );
}

#[test]
fn terminal_resize_grow_preserves_scrollback_and_allows_viewport_expansion() {
    use std::num::NonZeroU16;

    // Vertical grow 16 -> 24 and horizontal grow 40 -> 80 keep committed lines intact.
    let (parser, mut session) = new_vt_session(16, 40, 128, 6);
    session
        .draw_inline_frame(&prefixed_lines("HIST", 0..12), 6, 6, |f| {
            draw_labeled_live(f, "L1", Some(3));
        })
        .unwrap();

    parser
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(24).unwrap(), NonZeroU16::new(80).unwrap());

    session
        .draw_inline_frame(&[], 10, 10, |f| {
            draw_labeled_live(f, "GROWN", Some(5));
        })
        .unwrap();

    let (sb, scr) = dump_vt(&parser);
    let mut combined = sb;
    // Growing height pulls the 2 scrolled-back rows back onto the screen, leaving all 12
    // committed lines at rows 0..12 above the 10-row grown viewport at rows 12..22.
    combined.extend(scr[..12].iter().cloned());
    assert_eq!(
        combined,
        (0..12).map(|i| format!("HIST-{i:02}")).collect::<Vec<_>>()
    );
    assert_eq!(scr[12], "GROWN-00");
    assert_eq!(scr[21], "GROWN-09");

    // Grow from clamped full-screen 12 -> 24 preserves scrollback and expands cleanly.
    let (parser2, mut session2) = new_vt_session(12, 40, 128, 12);
    session2
        .draw_inline_frame(&prefixed_lines("TOP", 0..4), 12, 12, |f| {
            draw_labeled_live(f, "SMALL", Some(6));
        })
        .unwrap();
    parser2
        .lock()
        .unwrap()
        .screen_mut()
        .set_size(NonZeroU16::new(24).unwrap(), NonZeroU16::new(40).unwrap());
    session2
        .draw_inline_frame(&[], 18, 18, |f| {
            draw_labeled_live(f, "TALL", Some(10));
        })
        .unwrap();
    let (sb2, scr2) = dump_vt(&parser2);
    let mut combined2 = sb2;
    // Growing 12 -> 24 pulls the 4 committed rows back onto rows 0..4 above the 18-row viewport.
    combined2.extend(scr2[..4].iter().cloned());
    assert_eq!(combined2, vec!["TOP-00", "TOP-01", "TOP-02", "TOP-03"]);
    assert_eq!(scr2[4], "TALL-00");
    assert_eq!(scr2[21], "TALL-17");
}
