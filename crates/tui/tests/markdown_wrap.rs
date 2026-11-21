use alan_tui::history::{HistoryCell, RenderOpts};
use alan_tui::theme::Theme;
use ratatui::style::Modifier;

#[test]
fn styled_markdown_wraps_at_word_boundaries_and_preserves_cuts() {
    let theme = Theme::default();
    let cases = [
        (
            "bold",
            "**alpha beta gamma delta epsilon**",
            Some(Modifier::BOLD),
        ),
        (
            "italic",
            "*alpha beta gamma delta epsilon*",
            Some(Modifier::ITALIC),
        ),
        ("code", "`alpha beta gamma delta epsilon`", None),
        (
            "heading",
            "# Alpha Beta Gamma Delta Epsilon",
            Some(Modifier::BOLD),
        ),
        ("bullet", "- alpha beta **gamma delta** epsilon", None),
        ("ordered", "10. alpha beta **gamma delta** epsilon", None),
    ];

    for (name, source, expected_mod) in cases {
        for width in [16, 20, 24] {
            let opts = RenderOpts::new(width, false);
            let mut cell = HistoryCell::Assistant(source.into());
            let styled_rows = cell.render_styled_lines(opts);
            let lines: Vec<String> = styled_rows.iter().map(ToString::to_string).collect();
            let joined = lines.join("\n");
            for word in ["alpha", "beta", "gamma", "delta", "epsilon"] {
                let lower = joined.to_lowercase();
                assert!(
                    lower.lines().any(|line| line.contains(word)),
                    "{name} at width={width} broke '{word}' mid-word:\n{joined}"
                );
            }
            if let Some(modifier) = expected_mod {
                assert!(
                    styled_rows
                        .iter()
                        .flat_map(|row| &row.spans)
                        .any(|span| span.style.add_modifier.contains(modifier)),
                    "{name} lost modifier {modifier:?}"
                );
            }
            if name == "code" {
                assert!(
                    styled_rows
                        .iter()
                        .flat_map(|row| &row.spans)
                        .any(|span| span.style.fg == Some(theme.syntax_type)),
                    "inline code lost semantic tint"
                );
                assert!(
                    styled_rows
                        .iter()
                        .flat_map(|row| &row.spans)
                        .all(|span| !span.style.add_modifier.contains(Modifier::UNDERLINED)),
                    "inline code must not be underlined"
                );
            }
            if name == "bullet" && lines.len() > 1 {
                for cont in &lines[1..] {
                    assert!(
                        cont.starts_with("  "),
                        "bullet continuation lost hanging indent at width={width}:\n{joined}"
                    );
                }
            }
            if name == "ordered" && lines.len() > 1 {
                for cont in &lines[1..] {
                    assert!(
                        cont.starts_with("    "),
                        "ordered list continuation lost hanging indent at width={width}:\n{joined}"
                    );
                }
            }
            // Verify source-key cuts remain exact across partial scrollback drain & resize.
            if styled_rows.len() > 1 {
                let trim = if styled_rows.len() > 2 { 2 } else { 1 };
                assert!(cell.trim_rendered_prefix(opts, trim));
                assert_eq!(cell.render_styled_lines(opts), styled_rows[trim..]);
                let rewrapped = cell.render_styled_lines(RenderOpts::new(32, false));
                let rewrapped_text: String = rewrapped.iter().map(ToString::to_string).collect();
                assert!(rewrapped_text.to_lowercase().contains("epsilon"));
            }
        }
    }
}
