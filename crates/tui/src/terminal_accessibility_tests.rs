use super::{TerminalStylePolicy, render_scrollback_with_policy};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
};

#[test]
fn native_scrollback_keeps_wide_characters_and_row_boundaries() {
    for width in [48, 80, 120] {
        let text = "中文😀计划 · server> ready · a > b";
        let mut buffer = Buffer::empty(Rect::new(0, 0, width, 2));
        render_scrollback_with_policy(
            &mut buffer,
            vec![Line::from(text), Line::from("next row")],
            TerminalStylePolicy::from_capabilities(false, "xterm-256color", 256),
        );
        let mut output = Vec::new();
        // insert_before sends the full cell grid, unlike ordinary frame diffs.
        CrosstermBackend::new(&mut output)
            .draw(buffer.content.iter().enumerate().map(|(index, cell)| {
                (
                    (index % width as usize) as u16,
                    (index / width as usize) as u16,
                    cell,
                )
            }))
            .unwrap();
        let mut parser = vt100::Parser::new(2, width, 0);
        parser.process(&output);
        let rows = parser.screen().rows(0, width).collect::<Vec<_>>();
        assert_eq!(rows[0].trim_end(), text);
        assert_eq!(rows[1].trim_end(), "next row");
    }
}

#[test]
fn accessibility_native_lowcolor_filters_extended_styles_in_backend_output() {
    let lines = vec![Line::styled(
        "literal",
        Style::default()
            .fg(Color::Rgb(17, 29, 43))
            .bg(Color::Indexed(200))
            .add_modifier(Modifier::ITALIC | Modifier::UNDERLINED),
    )];
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));
    render_scrollback_with_policy(
        &mut buffer,
        lines.clone(),
        TerminalStylePolicy::from_capabilities(false, "ansi", 8),
    );
    assert_eq!(buffer.cell((0, 0)).unwrap().fg, Color::Reset);
    assert_eq!(buffer.cell((0, 0)).unwrap().bg, Color::Reset);
    assert!(
        buffer
            .cell((0, 0))
            .unwrap()
            .modifier
            .contains(Modifier::UNDERLINED)
    );
    assert!(
        !buffer
            .cell((0, 0))
            .unwrap()
            .modifier
            .contains(Modifier::ITALIC)
    );
    let mut output = Vec::new();
    CrosstermBackend::new(&mut output)
        .draw(
            buffer
                .content
                .iter()
                .enumerate()
                .map(|(x, cell)| (x as u16, 0, cell)),
        )
        .unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("literal"));
    assert!(
        !output.contains("38;2") && !output.contains("48;5"),
        "{output:?}"
    );
    render_scrollback_with_policy(
        &mut buffer,
        lines,
        TerminalStylePolicy::from_capabilities(false, "xterm-direct", u16::MAX),
    );
    assert_eq!(buffer.cell((0, 0)).unwrap().fg, Color::Rgb(17, 29, 43));
    assert_eq!(buffer.cell((0, 0)).unwrap().bg, Color::Indexed(200));
}
