use alan_tui::history::{HistoryCell, RenderOpts};
use alan_tui::theme::Theme;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Paragraph, Widget},
};

#[test]
fn fenced_styles_keep_literal_body_inline_distinction_and_secondary_boundaries() {
    let theme = Theme::default();
    for width in [40, 60, 73, 80, 120] {
        for language in ["rust", "unknown_language"] {
            for closing in ["", "\n```"] {
                let source = format!(
                    "prose `inline`\n```{language}\n\t界e\u{301} 👩\u{200d}💻 **literal** `tick`{closing}"
                );
                let cell = HistoryCell::Assistant(source.clone());
                let rows = cell.render_styled_lines(RenderOpts::new(width, false));
                assert_eq!(rows[0].to_string(), "prose inline");
                assert!(
                    rows[0]
                        .spans
                        .iter()
                        .any(|s| s.style.fg == Some(theme.syntax_type))
                );
                assert!(
                    rows[0]
                        .spans
                        .iter()
                        .all(|s| !s.style.add_modifier.contains(Modifier::UNDERLINED))
                );
                assert_eq!(rows[1].to_string(), format!("▎ {language}"));
                assert_eq!(
                    rows[1]
                        .spans
                        .iter()
                        .filter(|s| s.style.fg == Some(theme.accent))
                        .map(|s| s.content.as_ref())
                        .collect::<String>(),
                    language
                );
                assert!(rows[1].spans.iter().any(|s| {
                    s.content.contains('▎') && s.style.add_modifier == Modifier::DIM
                }));
                let end = rows.len() - usize::from(!closing.is_empty());
                let body = &rows[2..end];
                assert_eq!(
                    body.iter()
                        .map(|r| {
                            let s = r.to_string();
                            s.strip_prefix("▎ ").unwrap_or(&s).to_string()
                        })
                        .collect::<String>(),
                    "    界e\u{301} 👩\u{200d}💻 **literal** `tick`"
                );
                if language == "unknown_language" {
                    assert!(
                        body.iter()
                            .flat_map(|r| r.spans.iter().skip(1))
                            .all(|s| s.style == Style::default())
                    );
                } else {
                    assert!(
                        body.iter()
                            .flat_map(|r| &r.spans)
                            .all(|s| !s.style.add_modifier.contains(Modifier::UNDERLINED))
                    );
                }
                if !closing.is_empty() {
                    assert_eq!(rows.last().unwrap().to_string(), "▎");
                    assert!(
                        rows.last()
                            .unwrap()
                            .spans
                            .iter()
                            .all(|s| s.style.add_modifier == Modifier::DIM)
                    );
                }
                assert_eq!(cell, HistoryCell::Assistant(source), "raw source unchanged");
            }
        }
        let rows = HistoryCell::Assistant("```diff\n+\t**new**\n-\t`old`\n context\n```".into())
            .render_styled_lines(RenderOpts::new(width, false));
        assert_eq!(rows[1].to_string(), "▎ +    **new**");
        assert_eq!(rows[2].to_string(), "▎ -    `old`");
        assert!(
            rows[1]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(theme.status_run) && s.style.add_modifier.is_empty())
        );
        assert!(
            rows[2]
                .spans
                .iter()
                .all(|s| s.style.fg == Some(theme.status_block) && s.style.add_modifier.is_empty())
        );
        assert!(
            rows[3]
                .spans
                .iter()
                .skip(1)
                .all(|s| s.style == Style::default())
        );
    }
}

#[test]
fn fenced_default_body_and_dim_boundaries_reach_native_ansi_at_73_by_22() {
    let theme = Theme::default();
    let rows = HistoryCell::Assistant("```rust\n    let 界 = \"**literal**\";\n```".into())
        .render_styled_lines(RenderOpts::new(73, false));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 73, 22));
    Paragraph::new(rows).render(buffer.area, &mut buffer);
    assert_eq!(buffer.cell((0, 0)).unwrap().modifier, Modifier::DIM);
    assert_eq!(buffer.cell((6, 1)).unwrap().fg, theme.syntax_keyword);
    assert!(buffer.cell((6, 1)).unwrap().modifier.is_empty());
    assert_eq!(buffer.cell((0, 2)).unwrap().modifier, Modifier::DIM);
    let mut ansi = Vec::new();
    CrosstermBackend::new(&mut ansi)
        .draw(Buffer::empty(buffer.area).diff(&buffer).into_iter())
        .unwrap();
    let ansi = String::from_utf8(ansi).unwrap();
    assert!(
        ansi.contains("\u{1b}[2m"),
        "secondary boundary SGR: {ansi:?}"
    );
    assert!(
        !ansi.contains("\u{1b}[4m"),
        "block must not emit underline SGR"
    );
    let mut terminal = vt100::Parser::new(22.try_into().unwrap(), 73.try_into().unwrap(), 0);
    terminal.process(ansi.as_bytes());
    assert!(
        terminal
            .screen()
            .contents()
            .contains("    let 界 = \"**literal**\";")
    );
}
