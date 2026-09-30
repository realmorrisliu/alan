use alan_tui::history::{HistoryCell, RenderOpts};

#[test]
fn long_word_wrapped_prose_keeps_byte_and_atom_cursors_aligned() {
    for width in [40, 60, 80, 120] {
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
    assert_eq!(cell.render_lines(opts)[0], format!("+{}", "a".repeat(39)));
    cell.trim_rendered_prefix(opts, 1);
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
        format!("{}done", "a".repeat(38))
    );
}

#[test]
fn partial_tab_and_unicode_cluster_have_stable_content_slots() {
    let opts = RenderOpts::new(40, false);
    let mut cell = HistoryCell::Assistant(format!("```\n{}\t界e\u{301}", "a".repeat(38)));
    cell.trim_rendered_prefix(opts, 1);
    assert_eq!(cell.render_lines(opts).concat(), "  界e\u{301}");
    if let HistoryCell::AssistantTail { text, .. } = &mut cell {
        text.push_str("\n```\n**ok**");
    }
    assert_eq!(
        cell.render_lines(RenderOpts::new(60, false)).concat(),
        "  界e\u{301}ok"
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
