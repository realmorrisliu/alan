use super::*;
use crate::composer::Composer;

#[test]
fn queue_completion_preserves_runtime_notice_with_identical_text() {
    use alan_agent_protocol::{InputIntent, UiNoticeKind, UiNoticeSnapshot, UiQueueSnapshot};
    let mut app = FileBackedApp::new("/agent/1".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec!["q".into()],
            ..Default::default()
        }),
    );
    app.track_local_input("q", "/agent/1".into(), "body".into(), InputIntent::Agent);
    app.refresh_local_input_hint("q", "/agent/1");
    let text = app.notice.as_deref().unwrap().to_owned();
    app.apply_ui_notice_snapshot(UiNoticeSnapshot::new(UiNoticeKind::Warning, &text));
    app.refresh_local_input_hint("q", "/agent/1");
    assert_eq!(app.notice.as_ref().unwrap().kind, UiNoticeKind::Warning);
    app.local_inputs.get_mut("q").unwrap().terminal = true;
    app.refresh_queue_hint();
    assert_eq!(app.notice.as_deref(), Some(text.as_str()));
}

#[test]
fn notice_severity_survives_hydration_and_layout_without_role_labels() {
    use alan_agent_protocol::{UiNoticeKind, UiNoticeSnapshot};
    let mut app = FileBackedApp::new("/agent/1".into());
    app.composer.set_text("draft 中文😀");
    let cursor = app.composer.cursor();
    for (kind, prefix, color) in [
        (UiNoticeKind::Error, "Error · ", Color::Red),
        (UiNoticeKind::Warning, "Warning · ", Color::Yellow),
        (UiNoticeKind::Compaction, "· ", Color::DarkGray),
        (UiNoticeKind::Rollback, "· ", Color::DarkGray),
        (UiNoticeKind::MemoryFlush, "· ", Color::DarkGray),
    ] {
        app.apply_ui_notice_snapshot(UiNoticeSnapshot::new(kind, "server> ready; a > b"));
        for width in [48, 80, 120] {
            let (lines, prompt) = live_region_lines_at(&app, width, 0);
            let notice = lines
                .iter()
                .find(|line| line.to_string().contains("server>"))
                .unwrap();
            assert_eq!(notice.to_string(), format!("{prefix}server> ready; a > b"));
            assert_eq!(notice.style.fg, Some(color));
            assert!(prompt.is_some());
            assert_eq!(app.composer.text(), "draft 中文😀");
            assert_eq!(app.composer.cursor(), cursor);
        }
    }
    for snapshot in [
        UiNoticeSnapshot::none(),
        UiNoticeSnapshot::new(UiNoticeKind::Warning, "  "),
    ] {
        app.apply_ui_notice_snapshot(snapshot);
        assert!(app.notice.is_none());
    }
}

#[test]
fn settled_input_uses_header_queue_status_without_a_duplicate_notice() {
    use alan_agent_protocol::{InputIntent, UiQueueSnapshot};
    let mut app = FileBackedApp::new("/agent/1".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            ..Default::default()
        }),
    );
    app.track_local_input("q", "/agent/1".into(), "body".into(), InputIntent::Agent);
    app.refresh_local_input_hint("q", "/agent/1");
    app.local_inputs.get_mut("q").unwrap().terminal = true;
    app.refresh_queue_hint();
    assert!(app.notice.is_none());
    for width in [48, 80, 120] {
        let (lines, _) = live_region_lines_at(&app, width, 0);
        assert!(
            !lines
                .iter()
                .skip(1)
                .any(|line| line.to_string().contains("queued 0"))
        );
    }
    app.queue.apply("/agent/1", None);
    app.refresh_queue_hint();
    assert_eq!(app.queue.label(), "queue unknown");
    assert!(app.context_line(80).to_string().contains("unknown"));
}

#[test]
fn paused_status_regression() {
    use alan_agent_protocol::{UiActivitySnapshot, UiQueueSnapshot};
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            paused: true,
            ..Default::default()
        }),
    );
    app.activity = UiActivitySnapshot::paused(Some(1));
    app.notice = Some("Engine queue paused; use /continue".into());
    app.composer.set_text("draft 界");
    for width in [40, 60, 73, 80, 120] {
        let (lines, prompt) = live_region_lines_at(&app, width, 5_001);
        let header = lines[0].to_string();
        assert_eq!(header.matches("paused").count(), 1, "{header}");
        let text = lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains("interrupt") && !text.contains("5s"),
            "{text}"
        );
        assert!(!lines.iter().any(|line| {
            SPINNER
                .iter()
                .any(|glyph| line.to_string().starts_with(&format!("{glyph} ·")))
        }));
        assert!(text.contains("Engine queue paused"));
        assert!(prompt.is_some());
        assert_eq!(app.composer.text(), "draft 界");
    }
    app.activity = UiActivitySnapshot::running(1);
    let (lines, _) = live_region_lines_at(&app, 120, 5_001);
    assert!(lines[0].to_string().contains("working · paused"));
    assert!(lines[1].to_string().contains("interrupt · 5s"));
}

#[test]
fn paused_status_real_wait_has_static_cue_without_clock() {
    use alan_agent_protocol::UiActivitySnapshot;
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity = UiActivitySnapshot::paused(Some(1));
    app.activity.waiting_submission_ids.push("r0".into());
    let (lines, _) = live_region_lines_at(&app, 80, 5_001);
    assert!(lines[1].to_string().contains("waiting for input"));
    assert!(!lines[1].to_string().contains("interrupt"));
    assert!(!lines[1].to_string().contains("5s"));
}

#[test]
fn paused_status_model_override_owns_narrow_color() {
    let mut app = FileBackedApp::new("/agent/root".into());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    app.model_chooser.pending = Some(("/agent/1".into(), "B".into()));
    assert_eq!(app.context_line(24).style.fg, Some(Color::Cyan));
    for uncertain in [false, true] {
        if uncertain {
            app.model_chooser.uncertain = Some(("/agent/1".into(), "B".into()));
        }
        for width in 16..=80 {
            let line = app.context_line(width);
            assert!(line.width() <= width);
            assert!(
                line.spans.iter().any(|span| span.content.contains("gpt")
                    || (span.content.starts_with('g') && span.content.ends_with('l'))),
                "{width}: {line}"
            );
            assert!(
                line.to_string()
                    .contains(if uncertain { "uncertain" } else { "pending" })
            );
        }
    }
    assert_eq!(app.context_line(24).style.fg, Some(Color::Red));
}

#[test]
fn paused_status_confirmation_and_form_keep_static_controls() {
    use crate::history::PendingYieldCell;
    use alan_agent_protocol::{UiActivitySnapshot, UiEvent, YieldKind};
    for kind in [YieldKind::Confirmation, YieldKind::StructuredInput] {
        let mut app = FileBackedApp::new("/agent/root".into());
        // The request watcher installs controls before the activity watcher publishes Paused.
        app.activity = UiActivitySnapshot::running(1);
        let questions = if matches!(kind, YieldKind::StructuredInput) {
            ["first", "second"]
                .into_iter()
                .map(|id| {
                    serde_json::from_value(serde_json::json!({
                        "id": id, "label": "Answer", "prompt": "Answer?",
                        "kind": "text", "required": false
                    }))
                    .unwrap()
                })
                .collect()
        } else {
            Vec::new()
        };
        app.set_pending_yield(PendingYieldCell {
            request_id: "r0".into(),
            kind,
            title: "Response needed".into(),
            prompt: None,
            options: vec!["approve".into(), "reject".into()],
            default_option: None,
            questions,
            capability: None,
            reason: None,
            presentation: None,
        });
        let cue = if app.form.is_some() {
            "input form"
        } else {
            "waiting for approval"
        };
        for activity in [
            UiActivitySnapshot::running(1),
            UiActivitySnapshot::paused(Some(1)),
            UiActivitySnapshot::idle(),
        ] {
            app.apply_ui_event(UiEvent::Activity { snapshot: activity });
            assert_eq!(app.activity_label(), Some("waiting for input"));
            assert!(!frame_needs_redraw(false, &app, 5_001, Some(4)));
            assert!(!frame_needs_redraw(false, &app, 6_001, Some(5)));
            assert!(frame_needs_redraw(true, &app, 5_001, Some(5)));
            for width in [40, 60, 73, 80, 120] {
                let (lines, _) = live_region_lines_at(&app, width, 5_001);
                assert!(
                    lines[1].to_string().contains(cue),
                    "cue={cue}, lines={lines:?}"
                );
                assert!(!lines[1].to_string().contains("interrupt"));
                assert!(!lines[1].to_string().contains("5s"));
                assert!(
                    !SPINNER
                        .iter()
                        .any(|glyph| { lines[1].to_string().starts_with(&format!("{glyph} ·")) })
                );
            }
        }
    }
}

#[test]
fn history_failure_is_visible_without_private_path_or_body_and_submission_survives() {
    let dir = tempfile::tempdir().unwrap();
    let blocked = dir.path().join("private-history-path");
    std::fs::write(&blocked, "private-error").unwrap();
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer = Composer::from_history_path(blocked.join("history"));
    app.composer.set_text("private-submission");
    assert_eq!(
        app.composer.take_submit().as_deref(),
        Some("private-submission")
    );
    app.composer.remember("private-submission");
    let (lines, prompt) = live_region_lines_at(&app, 100, 0);
    let rendered = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Composer history unavailable; recall is session-only"));
    assert!(!rendered.contains("private-history-path"));
    assert!(!rendered.contains("private-error"));
    assert!(!rendered.contains("private-submission"));
    assert!(prompt.is_some());
}
