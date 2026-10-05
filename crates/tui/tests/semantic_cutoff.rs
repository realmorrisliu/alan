use alan_tui::history::{HistoryCell, RenderOpts};

#[test]
fn review_control_unicode_original_offsets() {
    for command in [false, true] {
        for control in ['\r', '\u{1b}', '\0'] {
            let source = format!("{control}{}\n\n  界e\u{301}👩‍💻\tend", "界".repeat(30));
            let mut cell = if command {
                HistoryCell::Command(source.clone())
            } else {
                HistoryCell::User(source.clone())
            };
            let opts = RenderOpts::new(18, false);
            let before = cell.render_lines(opts);
            cell.trim_rendered_prefix(opts, 1);
            let HistoryCell::InputTail { committed: cut, .. } = &cell else {
                panic!()
            };
            assert!(source.is_char_boundary(cut.0), "original UTF8 cut={cut:?}");
            assert_eq!(cell.render_lines(opts), before[1..]);
            for width in [24, 80, 18] {
                let expected = HistoryCell::InputTail {
                    text: source.clone(),
                    command,
                    committed: *cut,
                }
                .render_lines(RenderOpts::new(width, false));
                assert_eq!(cell.render_lines(RenderOpts::new(width, false)), expected);
            }
        }
    }
}

#[test]
fn review_literal_grapheme_boundary() {
    for cluster in ["👨‍👩‍👧‍👦", "e\u{301}"] {
        for width in [16, 18, 24] {
            let n = width - 3;
            let source = format!(
                "{}{cluster}{}\nrepeat\trepeat **literal**",
                "a".repeat(n),
                "b".repeat(width * 3)
            );
            let opts = RenderOpts::new(width, false);
            let mut cell = HistoryCell::User(source.clone());
            let rows = cell.render_lines(opts);
            assert!(
                rows.iter().any(|row| row.contains(cluster)),
                "cluster split: {rows:?}"
            );
            let body = rows.iter().map(|row| &row[2..]).collect::<String>();
            // Word wrapping intentionally elides inter-row whitespace; compare
            // non-whitespace content while checking clusters separately above.
            let visible = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            assert_eq!(visible(&body), visible(&source));
            for count in [1, 1] {
                cell.trim_rendered_prefix(opts, count);
                let HistoryCell::InputTail {
                    text, committed, ..
                } = &cell
                else {
                    panic!()
                };
                assert_eq!(text, &source);
                let mut grapheme_boundaries = vec![0];
                for g in ratatui::text::Span::raw(source.as_str())
                    .styled_graphemes(ratatui::style::Style::default())
                {
                    let end =
                        g.symbol.as_ptr() as usize - source.as_ptr() as usize + g.symbol.len();
                    grapheme_boundaries.push(end);
                }
                for (index, ch) in source.char_indices() {
                    if ch.is_control() {
                        grapheme_boundaries.push(index + ch.len_utf8());
                    }
                }
                assert!(
                    grapheme_boundaries.contains(&committed.0),
                    "cut split original grapheme: {committed:?}"
                );
                assert!(
                    committed.1 == 0
                        || committed.1 == usize::MAX
                        || source[..committed.0].ends_with('\t')
                );
                let expected = source[committed.0..]
                    .replace('\n', "")
                    .replace('\t', "    ");
                for resized in [16, 80, 24] {
                    let actual = cell
                        .render_lines(RenderOpts::new(resized, false))
                        .iter()
                        .map(|row| &row[2..])
                        .collect::<String>();
                    assert_eq!(visible(&actual), visible(&expected));
                }
            }
        }
    }
}

#[test]
fn literal_exact_tab_blank_indent_and_grapheme_cuts() {
    for command in [false, true] {
        let source = "\t  leading\n\n  indented\nend";
        let mut cell = if command {
            HistoryCell::Command(source.into())
        } else {
            HistoryCell::User(source.into())
        };
        let opts = RenderOpts::new(80, false);
        let before = cell.render_lines(opts);
        assert_eq!(
            before,
            vec![
                format!("{}      leading", if command { "! " } else { ": " }),
                "  ".into(),
                "    indented".into(),
                "  end".into()
            ]
        );
        cell.trim_rendered_prefix(opts, 1);
        assert_eq!(cell.render_lines(opts), before[1..]);
        cell.trim_rendered_prefix(opts, 1);
        assert_eq!(cell.render_lines(opts), before[2..]);
        for width in [24, 80] {
            assert_eq!(
                cell.render_lines(RenderOpts::new(width, false)),
                before[2..]
            );
        }
    }
    let source = format!("{}\tZ", "a".repeat(12));
    let mut cell = HistoryCell::User(source.clone());
    let opts = RenderOpts::new(16, false);
    let before = cell.render_lines(opts);
    cell.trim_rendered_prefix(opts, 1);
    assert_eq!(cell.render_lines(opts), before[1..]);
    let HistoryCell::InputTail { committed: cut, .. } = &cell else {
        panic!()
    };
    assert!(source.is_char_boundary(cut.0));
}

#[test]
fn literal_inputs_keep_exact_source_cut_across_repeated_wraps_and_resize() {
    for command in [false, true] {
        for width in [18, 24, 40, 80] {
            let source =
                "same same same same 界e\u{301} 👩\u{200d}💻\t**literal**\n\nlast uncommitted"
                    .repeat(4);
            let mut cell = if command {
                HistoryCell::Command(source.clone())
            } else {
                HistoryCell::User(source.clone())
            };
            let opts = RenderOpts::new(width, false);
            let before = cell.render_lines(opts);
            assert!(cell.trim_rendered_prefix(opts, 2));
            let HistoryCell::InputTail {
                text,
                command: role,
                committed,
            } = &cell
            else {
                panic!("literal source must survive drain");
            };
            assert_eq!(text, &source);
            assert_eq!(*role, command);
            let first_cut = *committed;
            let after = cell.render_lines(opts);
            assert_eq!(after, before[2..]);
            assert!(cell.trim_rendered_prefix(opts, 1));
            assert_eq!(cell.render_lines(opts), before[3..]);
            let HistoryCell::InputTail { committed, .. } = &cell else {
                unreachable!()
            };
            assert!(*committed > first_cut);
            for resized in [18, 80, 24] {
                let projected = cell
                    .render_lines(RenderOpts::new(resized, false))
                    .join("\n");
                assert!(projected.contains("uncommitted"));
                assert!(!projected.starts_with(if command { "alan! " } else { "alan: " }));
            }
        }
    }
}

#[test]
fn long_word_wrapped_prose_keeps_byte_and_atom_cursors_aligned() {
    for width in [40, 60, 73, 80, 120] {
        let source = "界e\u{301} 👩\u{200d}💻 word\t spaced  ".repeat(1000);
        let opts = RenderOpts::new(width, false);
        let mut cell = HistoryCell::Assistant(source);
        let before = cell.render_lines(opts);
        assert!(before.len() > 100);
        assert!(cell.trim_rendered_prefix(opts, 7));
        assert_eq!(cell.render_lines(opts), before[7..]);
        assert!(cell.trim_rendered_prefix(opts, 11));
        assert_eq!(cell.render_lines(opts), before[18..]);
    }
}

#[test]
fn late_fence_close_keeps_uncommitted_literal_code() {
    let opts = RenderOpts::new(40, false);
    let mut cell = HistoryCell::Assistant(format!("```diff\n+{}", "a".repeat(77)));
    assert_eq!(cell.render_lines(opts)[1], format!("+{}", "a".repeat(39)));
    cell.trim_rendered_prefix(opts, 2);
    if let HistoryCell::AssistantTail { text, .. } = &mut cell {
        text.push_str("\n``");
    }
    assert_eq!(
        cell.render_lines(opts).concat(),
        format!("{}{}", "a".repeat(38), "``")
    );
    if let HistoryCell::AssistantTail { text, .. } = &mut cell {
        text.push_str("`\n**done**");
    }
    assert_eq!(
        cell.render_lines(opts).concat(),
        format!("{}```done", "a".repeat(38))
    );
}

#[test]
fn partial_tab_and_unicode_cluster_have_stable_content_slots() {
    let opts = RenderOpts::new(40, false);
    let mut cell = HistoryCell::Assistant(format!("```\n{}\t界e\u{301}", "a".repeat(38)));
    cell.trim_rendered_prefix(opts, 2);
    assert_eq!(cell.render_lines(opts).concat(), "  界e\u{301}");
    if let HistoryCell::AssistantTail { text, .. } = &mut cell {
        text.push_str("\n```\n**ok**");
    }
    assert_eq!(
        cell.render_lines(RenderOpts::new(60, false)).concat(),
        "  界e\u{301}```ok"
    );
}
#[test]
fn late_emphasis_close_preserves_uncommitted_content() {
    let mut cell = HistoryCell::Assistant(format!("**{}", "a".repeat(78)));
    let opts = RenderOpts::new(40, false);
    let committed = cell.render_lines(opts)[0].clone();
    assert_eq!(committed, format!("**{}", "a".repeat(38)));
    assert!(cell.trim_rendered_prefix(opts, 1));
    if let HistoryCell::AssistantTail { text, .. } = &mut cell {
        text.push_str("**");
    } else {
        panic!("partial drain must retain assistant source");
    }
    assert_eq!(cell.render_lines(opts).concat(), "a".repeat(40));
}

#[test]
fn markdown_intraword_underscores_preserve_identifiers_and_source_cut() {
    use ratatui::style::Modifier;
    let source = "model_control_available foo__bar__baz 中_文_名 _emphasis_ __strong__ `code_name`";
    for width in [40, 60, 73, 80, 120] {
        let opts = RenderOpts::new(width, false);
        let mut cell = HistoryCell::Assistant(source.into());
        let rows = cell.render_styled_lines(opts);
        let text = rows.iter().map(ToString::to_string).collect::<String>();
        assert!(text.contains("model_control_available"), "{text}");
        assert!(
            text.contains("foo__bar__baz") && text.contains("中_文_名"),
            "{text}"
        );
        assert_eq!(
            rows.iter()
                .flat_map(|r| &r.spans)
                .filter(|s| s.style.add_modifier.contains(Modifier::ITALIC))
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "emphasis"
        );
        assert_eq!(
            rows.iter()
                .flat_map(|r| &r.spans)
                .filter(|s| s.style.add_modifier.contains(Modifier::BOLD))
                .map(|s| s.content.as_ref())
                .collect::<String>(),
            "strong"
        );
        let escaped = HistoryCell::Assistant(r"\_escaped\_ _name_with_underscores_".into());
        let escaped_text = escaped
            .render_styled_lines(opts)
            .iter()
            .map(ToString::to_string)
            .collect::<String>();
        assert!(
            escaped_text.contains("_escaped_") && escaped_text.contains("name_with_underscores")
        );
        let before = cell.render_styled_lines(opts);
        cell.trim_rendered_prefix(opts, 1);
        assert_eq!(cell.render_styled_lines(opts), before[1..]);
        let HistoryCell::AssistantTail { text, committed } = &cell else {
            panic!()
        };
        assert_eq!(text, source);
        assert!(source.is_char_boundary(committed.0));
        let cut = *committed;
        for resized in [40, 120, 60] {
            assert_eq!(
                cell.render_styled_lines(RenderOpts::new(resized, false)),
                HistoryCell::AssistantTail {
                    text: source.into(),
                    committed: cut
                }
                .render_styled_lines(RenderOpts::new(resized, false))
            );
        }
    }
}
