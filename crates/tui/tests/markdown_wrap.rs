use alan_tui::history::{HistoryCell, RenderOpts};
use ratatui::style::Modifier;

#[test]
fn styled_markdown_wraps_at_word_boundaries_and_preserves_cuts() {
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
        (
            "code",
            "`alpha beta gamma delta epsilon`",
            Some(Modifier::UNDERLINED),
        ),
        (
            "heading",
            "# Alpha Beta Gamma Delta Epsilon",
            Some(Modifier::BOLD),
        ),
        ("bullet", "- alpha beta **gamma delta** epsilon", None),
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
            // Verify source-key cuts remain exact across partial scrollback drain & resize.
            if styled_rows.len() > 2 {
                assert!(cell.trim_rendered_prefix(opts, 2));
                assert_eq!(cell.render_styled_lines(opts), styled_rows[2..]);
                let rewrapped = cell.render_styled_lines(RenderOpts::new(32, false));
                let rewrapped_text: String = rewrapped.iter().map(ToString::to_string).collect();
                assert!(rewrapped_text.to_lowercase().contains("epsilon"));
            }
        }
    }
}
