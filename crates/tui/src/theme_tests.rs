use super::*;
use crate::terminal::TerminalStylePolicy;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

#[test]
fn theme_tokens_match_alanworks_palette_and_preserve_transparent_background() {
    let midnight = Theme::midnight();
    assert_eq!(midnight.accent, Color::Rgb(82, 130, 255));
    assert_eq!(midnight.status_run, Color::Rgb(34, 211, 166));
    assert_eq!(midnight.status_wait, Color::Rgb(245, 158, 11));
    assert_eq!(midnight.status_block, Color::Rgb(248, 113, 113));
    assert_eq!(midnight.accent_style().bg, None);
    assert_eq!(midnight.muted_style().bg, None);

    let paper = Theme::paper();
    assert_eq!(paper.accent, Color::Rgb(0, 47, 167));
    assert_eq!(paper.status_run, Color::Rgb(0, 138, 102));
    assert_eq!(paper.status_wait, Color::Rgb(194, 106, 6));
    assert_eq!(paper.status_block, Color::Rgb(217, 56, 46));
    assert_eq!(paper.accent_style().bg, None);
}

#[test]
fn theme_resolve_respects_mode_colorterm_no_color_and_dumb() {
    assert_eq!(
        Theme::resolve(
            ThemeMode::Auto,
            false,
            "xterm-256color",
            Some("TrueColor"),
            256
        )
        .mode,
        ThemeMode::Midnight
    );
    assert_eq!(
        Theme::resolve(ThemeMode::Auto, false, "xterm-256color", Some("24BIT"), 256).mode,
        ThemeMode::Midnight
    );
    assert_eq!(
        Theme::resolve(
            ThemeMode::Paper,
            false,
            "xterm-256color",
            Some("truecolor"),
            256
        )
        .mode,
        ThemeMode::Paper
    );
    assert_eq!(
        Theme::resolve(ThemeMode::Auto, false, "ansi", None, 16).mode,
        ThemeMode::Ansi
    );
    assert_eq!(
        Theme::resolve(
            ThemeMode::Midnight,
            true,
            "xterm-256color",
            Some("truecolor"),
            u16::MAX
        )
        .mode,
        ThemeMode::Dumb
    );
    assert_eq!(
        Theme::resolve(
            ThemeMode::Midnight,
            false,
            "dumb",
            Some("truecolor"),
            u16::MAX
        )
        .mode,
        ThemeMode::Dumb
    );
    assert_eq!(Theme::dumb().status_dot(DotState::Run).0, "*");
    assert_eq!(Theme::dumb().status_dot(DotState::Wait).0, "?");
    assert_eq!(Theme::dumb().status_dot(DotState::Block).0, "!");
    assert_eq!(Theme::midnight().status_dot(DotState::Run).0, "●");
}

#[test]
fn terminal_style_policy_downgrades_truecolor_to_256_and_16_and_strips_dumb() {
    let midnight = Theme::midnight();
    let paper = Theme::paper();

    let lines = vec![Line::from(vec![
        ratatui::text::Span::styled("k", Style::default().fg(midnight.accent)),
        ratatui::text::Span::styled("p", Style::default().fg(paper.accent)),
        ratatui::text::Span::styled("r", Style::default().fg(midnight.status_run)),
        ratatui::text::Span::styled("w", Style::default().fg(midnight.status_wait)),
        ratatui::text::Span::styled("b", Style::default().fg(midnight.status_block)),
    ])];

    // TrueColor preserved when COLORTERM=truecolor even if available_color_count() reports 256.
    let mut truecolor_buf = Buffer::empty(Rect::new(0, 0, 10, 1));
    Paragraph::new(lines.clone()).render(truecolor_buf.area, &mut truecolor_buf);
    TerminalStylePolicy::from_env_capabilities(false, "xterm-256color", Some("TRUECOLOR"), 256)
        .apply(&mut truecolor_buf);
    assert_eq!(truecolor_buf.cell((0, 0)).unwrap().fg, midnight.accent);
    assert_eq!(truecolor_buf.cell((1, 0)).unwrap().fg, paper.accent);

    // 256-color downgrade maps Klein Blue and status tokens to calibrated xterm-256 indices.
    let mut c256_buf = Buffer::empty(Rect::new(0, 0, 10, 1));
    Paragraph::new(lines.clone()).render(c256_buf.area, &mut c256_buf);
    TerminalStylePolicy::from_env_capabilities(false, "xterm-256color", None, 256)
        .apply(&mut c256_buf);
    assert_eq!(c256_buf.cell((0, 0)).unwrap().fg, Color::Indexed(69));
    assert_eq!(c256_buf.cell((1, 0)).unwrap().fg, Color::Indexed(26));
    assert_ne!(c256_buf.cell((2, 0)).unwrap().fg, Color::Reset);
    assert_ne!(c256_buf.cell((3, 0)).unwrap().fg, Color::Reset);
    assert_ne!(c256_buf.cell((4, 0)).unwrap().fg, Color::Reset);

    // 16-color downgrade preserves ANSI hue instead of blanking to Reset.
    let mut c16_buf = Buffer::empty(Rect::new(0, 0, 10, 1));
    Paragraph::new(lines.clone()).render(c16_buf.area, &mut c16_buf);
    TerminalStylePolicy::from_env_capabilities(false, "xterm-16color", None, 16)
        .apply(&mut c16_buf);
    assert!(matches!(
        c16_buf.cell((0, 0)).unwrap().fg,
        Color::LightBlue | Color::Blue
    ));
    assert_eq!(c16_buf.cell((2, 0)).unwrap().fg, Color::Green);
    assert_eq!(c16_buf.cell((3, 0)).unwrap().fg, Color::Yellow);
    assert_eq!(c16_buf.cell((4, 0)).unwrap().fg, Color::Red);

    // NO_COLOR and TERM=dumb strip both colors and modifiers.
    for policy in [
        TerminalStylePolicy::from_env_capabilities(true, "xterm-256color", Some("truecolor"), 256),
        TerminalStylePolicy::from_env_capabilities(false, "dumb", Some("truecolor"), u16::MAX),
    ] {
        let mut dumb_buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        Paragraph::new(vec![Line::styled(
            "bold",
            midnight.accent_bold().bg(midnight.selection_bg),
        )])
        .render(dumb_buf.area, &mut dumb_buf);
        policy.apply(&mut dumb_buf);
        let cell = dumb_buf.cell((0, 0)).unwrap();
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
        assert_eq!(cell.modifier, Modifier::empty());
    }
}
