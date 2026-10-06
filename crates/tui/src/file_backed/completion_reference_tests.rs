use super::completion_layout_tests::render;
use super::*;
use crate::completion::{CompletionCandidate, CompletionKind};

#[test]
fn references_wrapped_unicode_and_selected_clipping_share_layout() {
    for width in [40, 60, 73, 80, 120] {
        for (trigger, kind) in [('$', CompletionKind::Skill), ('@', CompletionKind::File)] {
            for pane in [3, 5, 10, 30] {
                let mut app = FileBackedApp::new("/agent/root".into());
                let candidates = (0..9)
                    .map(|i| {
                        CompletionCandidate::new(format!("ref{i}"), Some("中文详情🙂 ".repeat(12)))
                    })
                    .collect::<Vec<_>>();
                if trigger == '$' {
                    app.set_skill_candidates(candidates);
                } else {
                    app.set_file_candidates(candidates);
                }
                for text in [
                    format!("界🙂 {trigger}"),
                    format!("{}\n界🙂 {trigger}", "中文🙂 ".repeat(18)),
                    format!("{}界🙂 {trigger}", "line\n".repeat(15)),
                ] {
                    app.composer.set_text(&text);
                    app.refresh_completion();
                    assert_eq!(app.completion.as_ref().unwrap().kind, kind);
                    let (before, cursor) = render(&app, width, pane);
                    assert_eq!(cursor.0, 8, "Unicode cursor X: {text}");
                    assert!(cursor.1 < pane);
                    let composer_rows = crate::transcript_ui::wrapped_line_count(
                        &app.composer_lines(),
                        width as usize,
                    );
                    if pane == 30 && composer_rows < MAX_COMPOSER_LINES {
                        let menu = before.iter().position(|r| r.contains("ref0")).unwrap();
                        assert_eq!(menu, 1 + composer_rows, "all composer rows precede menu");
                    }
                    for _ in 0..8 {
                        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                    }
                    let (selected, after) = render(&app, width, pane);
                    assert_eq!(after, cursor, "selection must not scroll input");
                    if (cursor.1 as usize) + 1
                        < inline_viewport_height(&app, width as usize, pane as usize) as usize
                    {
                        assert!(
                            selected.iter().any(|r| r.contains("ref8")),
                            "selected item hidden: {selected:?}"
                        );
                    }
                    assert_eq!(app.composer.text(), text);
                    assert!(app.transcript.is_empty());
                    assert!(
                        app.drain_committed_scrollback(width as usize, pane as usize)
                            .is_empty()
                    );
                    app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
                    assert!(app.composer.text().ends_with(&format!("{trigger}ref8 ")));
                }
            }
        }
    }
}
