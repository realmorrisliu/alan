use super::super::{HistoryCell, PendingYieldCell, RenderOpts, action_summary};
use crate::theme::Theme;
use alan_agent_protocol::{DiffHunk, DiffLine, ToolResultPresentation, YieldKind};
use ratatui::style::Modifier;

#[test]
fn syntax_highlighting_styles_fenced_blocks_and_preserves_unknown_plain() {
    let kw = Theme::default().syntax_keyword;
    for (lang, sample, expected_ch, expected_color) in [
        ("rust", "fn main() { let x = \"ok\"; // c\n}", "f", kw),
        ("typescript", "const n: number = 42;", "c", kw),
        ("python", "def run(): return True", "d", kw),
        ("go", "func serve() string { return \"ok\" }", "f", kw),
        ("bash", "if true; then export A=\"1\"; fi", "e", kw),
        ("sql", "SELECT * FROM users WHERE id = 1;", "S", kw),
    ] {
        let src = format!("```{lang}\n{sample}\n```");
        let rows = HistoryCell::Assistant(src).render_styled_lines(RenderOpts::new(80, false));
        let line_str: String = rows
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(line_str.contains(expected_ch), "{lang}: {line_str}");
        let has_color = rows
            .iter()
            .flat_map(|l| &l.spans)
            .any(|s| s.content == expected_ch && s.style.fg == Some(expected_color));
        assert!(has_color, "{lang} missing keyword color: {rows:?}");
    }

    let unknown = HistoryCell::Assistant("```unknown_lang\nfn foo() {}\n```".into())
        .render_styled_lines(RenderOpts::new(80, false));
    assert!(
        unknown[2]
            .spans
            .iter()
            .skip(1)
            .all(|s| s.style.fg.is_none() && s.style.add_modifier.is_empty())
    );
}

#[test]
fn markdown_renders_tables_blockquotes_links_and_h1_to_h6_hierarchy() {
    let md = [
        "# H1",
        "## H2",
        "### H3",
        "#### H4",
        "##### H5",
        "###### H6",
        "",
        "> quoted text",
        "",
        "| Name | Score |",
        "| :--- | ---: |",
        "| Alice | 42 |",
        "",
        "See [docs](https://example.com/a) and https://example.org/b.",
    ]
    .join("\n");

    let mut cell = HistoryCell::Assistant(md);
    let opts = RenderOpts::new(60, false);
    let full = cell.render_styled_lines(opts);
    let joined = full
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");

    // Distinct h1..h6 styles
    let find_line = |needle: &str| {
        full.iter()
            .find(|line| line.to_string().trim() == needle)
            .expect("missing heading")
    };
    assert_ne!(
        find_line("H1").spans[0].style,
        find_line("H2").spans[0].style
    );
    assert_ne!(
        find_line("H2").spans[0].style,
        find_line("H3").spans[0].style
    );
    assert_ne!(
        find_line("H3").spans[0].style,
        find_line("H4").spans[0].style
    );
    assert_ne!(
        find_line("H4").spans[0].style,
        find_line("H5").spans[0].style
    );
    assert_ne!(
        find_line("H5").spans[0].style,
        find_line("H6").spans[0].style
    );

    // Blockquote bar
    assert!(joined.contains("│ quoted text"), "{joined}");

    // Table box drawing
    assert!(
        joined.contains("┌") && joined.contains("│ Name") && joined.contains("└"),
        "{joined}"
    );

    // Links underlined accent
    let theme = Theme::default();
    assert!(full.iter().flat_map(|l| &l.spans).any(|s| {
        s.content == "d"
            && s.style.fg == Some(theme.accent)
            && s.style.add_modifier.contains(Modifier::UNDERLINED)
    }));

    // Partial scrollback pruning across table preserves exact remainder!
    let before_len = full.len();
    assert!(cell.trim_rendered_prefix(opts, before_len / 2));
    let tail = cell.render_styled_lines(opts);
    assert_eq!(tail, full[before_len / 2..]);
}

#[test]
fn compact_action_summary_renders_colored_inline_diff_and_collapses_huge() {
    let lines = (0..20)
        .map(|i| DiffLine::Added {
            text: format!("line_{i}"),
        })
        .collect();
    let cell = HistoryCell::Tool {
        action: None,
        title: "Edit src/lib.rs".into(),
        status: super::super::ToolStatus::Complete,
        preview: None,
        presentation: Some(ToolResultPresentation::Diff {
            path: "src/lib.rs".into(),
            hunks: vec![DiffHunk {
                header: Some("@@ -1,1 +1,20 @@".into()),
                lines,
            }],
        }),
    };
    let theme = Theme::default();
    let rows = action_summary(&cell, 80);
    assert!(rows.iter().all(|r| r.width() <= 80));
    assert!(
        rows.iter()
            .any(|r| r.spans.iter().any(|s| s.style.fg == Some(theme.status_run)))
    );
    let rendered: String = rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("more diff lines"), "{rendered}");
}

#[test]
fn confirmation_yield_card_shows_numbered_hints_default_allow_session_and_colored_diff() {
    let pending = PendingYieldCell {
        request_id: "req-1".into(),
        kind: YieldKind::Confirmation,
        title: "Apply edit".into(),
        prompt: Some("Write changes to src/main.rs".into()),
        options: vec!["approve".into(), "allow_session".into(), "reject".into()],
        default_option: Some("approve".into()),
        questions: vec![],
        capability: Some("write".into()),
        reason: Some("Workspace write outside fast-path".into()),
        presentation: Some(ToolResultPresentation::Diff {
            path: "src/main.rs".into(),
            hunks: vec![DiffHunk {
                header: None,
                lines: vec![
                    DiffLine::Removed { text: "old".into() },
                    DiffLine::Added { text: "new".into() },
                ],
            }],
        }),
    };
    let rows = HistoryCell::PendingYield(pending).render_styled_lines(RenderOpts::new(100, false));
    let joined = rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("[1] approve (default)"), "{joined}");
    assert!(joined.contains("[2] allow_session"), "{joined}");
    assert!(joined.contains("[3] reject"), "{joined}");
    let theme = Theme::default();
    assert!(
        rows.iter().any(|r| {
            r.to_string().contains("-old")
                && r.spans
                    .iter()
                    .any(|s| s.style.fg == Some(theme.status_block))
        }),
        "{joined}"
    );
    assert!(
        rows.iter().any(|r| {
            r.to_string().contains("+new")
                && r.spans.iter().any(|s| s.style.fg == Some(theme.status_run))
        }),
        "{joined}"
    );
}
