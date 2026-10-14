use super::*;
use crate::composer::Composer;
use crate::file_backed::app::Notice;
use crate::history::HistoryCell;

#[test]
fn retained_runtime_error_retires_only_its_matching_runtime_notice() {
    use alan_agent_protocol::{UiEvent, UiNoticeKind, UiNoticeSnapshot};
    for (local, kind, text, retire) in [
        (false, UiNoticeKind::Error, "delegation rejected", true),
        (true, UiNoticeKind::Error, "delegation rejected", false),
        (false, UiNoticeKind::Warning, "delegation rejected", false),
        (false, UiNoticeKind::Error, "other failure", false),
    ] {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.composer.set_text("draft 中文😀");
        let cursor = app.composer.cursor();
        if local {
            app.notice = Some(Notice::error(text));
        } else {
            app.apply_ui_event(UiEvent::Notice {
                snapshot: UiNoticeSnapshot::new(kind, text),
            });
        }
        app.apply_ui_event(UiEvent::Error {
            message: "delegation rejected".into(),
            recoverable: true,
        });
        assert_eq!(app.notice.is_none(), retire);
        if !retire {
            assert_eq!(app.notice.as_deref(), Some(text));
            assert_eq!(app.notice.as_ref().unwrap().kind, kind);
        }
        assert_eq!(
            app.transcript
                .iter()
                .filter(|cell| matches!(cell,
                    HistoryCell::Error(message) if message == "delegation rejected"
                ))
                .count(),
            1
        );
        for width in [48, 80, 120] {
            let (lines, _) = live_region_lines_at(&app, width, 0);
            if retire {
                assert!(!lines.iter().any(|line| line.to_string().contains(text)));
            }
            assert_eq!(app.composer.text(), "draft 中文😀");
            assert_eq!(app.composer.cursor(), cursor);
        }
    }
}

#[tokio::test]
async fn hydrated_runtime_error_keeps_history_without_a_duplicate_footer() {
    use crate::file_backed::{hydrate_and_open_tails, stdio_tests};
    use alan_agent_protocol::{UiEvent, UiInputStatus, UiNoticeKind, UiNoticeSnapshot};

    let (shell, _, _, _) = stdio_tests::live_root_agent().await;
    let events = [
        UiEvent::Notice {
            snapshot: UiNoticeSnapshot::new(UiNoticeKind::Error, "delegation rejected"),
        },
        UiEvent::Error {
            message: "delegation rejected".into(),
            recoverable: true,
        },
        UiEvent::InputCompleted {
            submission_ids: vec![stdio_tests::INPUT_ID.into()],
            status: UiInputStatus::Completed,
            error: None,
        },
    ];
    for event in events {
        shell
            .write(
                "/agent/root/machine/ui/events",
                format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
            )
            .await
            .unwrap();
    }
    let mut app = FileBackedApp::new("/agent/root".into());
    app.composer.set_text("draft 中文😀");
    let cursor = app.composer.cursor();
    let tails = hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    tails.close().await;
    assert_eq!(
        app.transcript,
        vec![HistoryCell::Error("delegation rejected".into())]
    );
    assert!(app.notice.is_none());
    assert!(!app.last_input_failed);
    assert_eq!(app.composer.text(), "draft 中文😀");
    assert_eq!(app.composer.cursor(), cursor);
}

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
    app.show_submission_sent("q");
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
                .skip(3)
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
        let header = lines[3].to_string().to_lowercase();
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
    assert!(lines[3].to_string().contains("Paused"));
    assert!(
        lines[3]
            .to_string()
            .contains("Working · 5s · Esc interrupt")
    );
}

#[test]
fn paused_status_real_wait_has_static_cue_without_clock() {
    use alan_agent_protocol::UiActivitySnapshot;
    let mut app = FileBackedApp::new("/agent/root".into());
    app.activity = UiActivitySnapshot::paused(Some(1));
    app.activity.waiting_submission_ids.push("r0".into());
    let (lines, _) = live_region_lines_at(&app, 80, 5_001);
    assert!(lines[3].to_string().contains("waiting for input"));
    assert!(!lines[3].to_string().contains("interrupt"));
    assert!(!lines[3].to_string().contains("5s"));
}

#[test]
fn paused_status_model_override_owns_narrow_color() {
    let mut app = FileBackedApp::new("/agent/root".into());
    crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6.1-sol");
    app.model_chooser.pending = Some(("/agent/1".into(), "B".into()));
    assert_eq!(attention_line(&app, 0).unwrap().style.fg, Some(Color::Cyan));
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
            assert!(app.attention_labels().join(" · ").contains(if uncertain {
                "uncertain"
            } else {
                "Selecting model"
            }));
        }
    }
    assert_eq!(attention_line(&app, 0).unwrap().style.fg, Some(Color::Red));
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
                    lines[3].to_string().contains(cue)
                        || lines[3].to_string()
                            == if cue == "waiting for approval" {
                                "Approval"
                            } else {
                                "Input required"
                            },
                    "cue={cue}, lines={lines:?}"
                );
                assert!(!lines[3].to_string().contains("interrupt"));
                assert!(!lines[3].to_string().contains("5s"));
                assert!(
                    !SPINNER
                        .iter()
                        .any(|glyph| { lines[3].to_string().starts_with(&format!("{glyph} ·")) })
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

#[test]
fn compact_response_cue_survives_overlapping_model_control() {
    use crate::history::PendingYieldCell;
    use alan_agent_protocol::{UiActivitySnapshot, YieldKind};
    for kind in [YieldKind::Confirmation, YieldKind::StructuredInput] {
        for uncertain in [false, true] {
            let mut app = FileBackedApp::new("/agent/1".into());
            app.activity = UiActivitySnapshot::running(0);
            app.model_chooser.pending = Some(("/agent/1".into(), "selection".into()));
            if uncertain {
                app.model_chooser.uncertain = app.model_chooser.pending.take();
            }
            app.set_pending_yield(PendingYieldCell {
                request_id: "response".into(), kind: kind.clone(),
                title: "Response needed".into(), prompt: None,
                options: vec!["approve".into(), "reject".into()], default_option: None,
                questions: if kind == YieldKind::StructuredInput {
                    vec![serde_json::from_value(serde_json::json!({
                        "id":"answer", "label":"Answer", "prompt":"Answer?", "kind":"text", "required":false
                    })).unwrap()]
                } else { Vec::new() },
                capability: None, reason: None, presentation: None,
            });
            for width in [16, 24, 40] {
                let line = attention_line_at_width(&app, 8000, width).unwrap();
                let text = line.to_string().to_lowercase();
                if kind == YieldKind::Confirmation {
                    assert!(text.contains("approval"), "{width}: {text}");
                } else {
                    assert!(text.contains("input"), "{width}: {text}");
                }
                assert!(!text.contains("esc") && !text.contains("8s"), "{text}");
                assert!(wrapped_line_count(&[line], width) <= 2);
            }
        }
    }
}

#[test]
fn compact_execution_keeps_actions_with_every_queue_cue_in_real_buffer() {
    use alan_agent_protocol::{UiActivityState, UiQueueSnapshot};
    use ratatui::{Terminal, backend::TestBackend};
    for state in [UiActivityState::Running, UiActivityState::Paused] {
        for queue_kind in [
            "uncertain",
            "paused",
            "deferred",
            "pending",
            "active",
            "unknown",
            "model pending",
            "model uncertain",
        ] {
            for width in [16, 24, 40] {
                let mut app = FileBackedApp::new("/agent/1".into());
                crate::file_backed::model_tests::install_header_model(&mut app, "gpt-6.1-sol");
                app.activity.state = state;
                let mut q = UiQueueSnapshot {
                    known: true,
                    revision: 1,
                    ..Default::default()
                };
                match queue_kind {
                    "uncertain" => q.uncertain_submission_ids.push("input".into()),
                    "paused" => q.paused = true,
                    "deferred" => q.deferred = true,
                    "pending" => q.pending_submission_ids.push("input".into()),
                    "active" => q.active_submission_ids.push("input".into()),
                    "model pending" => {
                        app.model_chooser.pending = Some(("/agent/1".into(), "selection".into()))
                    }
                    "model uncertain" => {
                        app.model_chooser.uncertain = Some(("/agent/1".into(), "selection".into()))
                    }
                    _ => q.known = false,
                }
                app.queue.apply("/agent/1", Some(q));
                let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
                terminal.draw(|f| draw_at(f, &app, 8000)).unwrap();
                let rows = (0..8)
                    .map(|y| {
                        (0..width)
                            .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>();
                let text = rows.join("\n");
                for cue in [
                    "gpt-6.1-sol",
                    if state == UiActivityState::Running {
                        "Esc"
                    } else {
                        "/continue"
                    },
                ] {
                    assert!(text.contains(cue), "{width} {state:?} {queue_kind}: {text}");
                }
                let cursor = terminal.backend().cursor_position();
                assert!(rows[cursor.y as usize].starts_with(": "));
            }
        }
    }
}
