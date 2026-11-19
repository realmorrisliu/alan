use super::*;
use crate::history::{ActionHistory, PendingYieldCell, RunningTool, ToolStatus};
use alan_agent_protocol::{
    ContentPart, DiffHunk, DiffLine, InputMode, ReasoningEffort, StructuredInputKind,
    StructuredInputOption, StructuredInputQuestion, ToolResultPresentation, UiActivitySnapshot,
    UiModelCatalog, UiModelChoice, UiModelControlSource, UiModelSnapshot, UiNoticeKind,
    UiNoticeSnapshot, UiQueueSnapshot, UiUsageSnapshot, YieldKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn ready_app() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            ..Default::default()
        }),
    );
    app.model.apply(
        "/agent/1",
        Some(UiModelSnapshot {
            known: true,
            process_path: "/proc/1".into(),
            publication_version: 1,
            selected_next: Some(alan_agent_protocol::UiModelBinding {
                profile: "openai-main".into(),
                provider: "openai_responses".into(),
                model: "gpt-5.4".into(),
                reasoning: Default::default(),
                control_source: UiModelControlSource::AgentConfig,
            }),
            catalog: Some(UiModelCatalog {
                profile: "openai-main".into(),
                models: vec![
                    UiModelChoice {
                        model: "gpt-5.4".into(),
                        supported_reasoning_efforts: vec![],
                        default_reasoning_effort: None,
                    },
                    UiModelChoice {
                        model: "gpt-5.4-mini".into(),
                        supported_reasoning_efforts: vec![],
                        default_reasoning_effort: None,
                    },
                    UiModelChoice {
                        model: "claude-sonnet-4-5".into(),
                        supported_reasoning_efforts: vec![
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                        ],
                        default_reasoning_effort: Some(ReasoningEffort::Medium),
                    },
                ],
            }),
            ..Default::default()
        }),
    );
    app
}

fn render_to_string(app: &FileBackedApp, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| layout::draw_at(frame, app, 5_000))
        .expect("draw");
    let buf = terminal.backend().buffer().clone();
    let mut rows = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = String::new();
        for x in 0..width {
            row.push_str(buf[(x, y)].symbol());
        }
        rows.push(row.trim_end().to_string());
    }
    rows.join("\n")
}

#[test]
fn new_session_resets_idle_view_blocks_active_turn_and_unpauses_empty_queue() {
    let mut app = ready_app();
    app.transcript.push(HistoryCell::User("hello".into()));
    app.activity = UiActivitySnapshot::running(10);
    assert!(app.handle_command("/new").is_none());
    assert!(!app.transcript.is_empty());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("Wait for or interrupt"))
    );

    app.activity = UiActivitySnapshot::idle();
    app.action_cells.insert("act-1".into(), 0);
    app.running_tools.push(RunningTool {
        id: "act-1".into(),
        title: "bash".into(),
        tail_lines: vec!["working".into()],
    });
    assert!(app.handle_command("/new").is_none());
    assert!(app.transcript.is_empty());
    assert!(app.action_cells.is_empty());
    assert!(app.running_tools.is_empty());
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("Started a fresh session view"))
    );

    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 2,
            paused: true,
            ..Default::default()
        }),
    );
    assert!(matches!(
        app.handle_command("/new"),
        Some(FileBackedAction::MachineCtl { command, .. }) if command == "queue-v1 continue"
    ));
}

#[test]
fn help_command_renders_multiline_shortcuts_with_empty_transcript() {
    let mut app = ready_app();
    assert!(app.handle_command("/help").is_none());
    assert!(app.transcript.is_empty());
    let rendered = render_to_string(&app, 100, 26);
    assert!(rendered.contains("Commands:"), "{rendered}");
    assert!(rendered.contains("/new"), "{rendered}");
    assert!(rendered.contains("Ctrl+S"), "{rendered}");
    assert!(rendered.contains("Ctrl+G"), "{rendered}");
    assert!(rendered.contains("Ctrl+_"), "{rendered}");
    assert!(rendered.contains("Ctrl+O"), "{rendered}");
}

#[test]
fn model_picker_fuzzy_filters_cycles_effort_and_formats_human_receipt() {
    let mut app = ready_app();
    app.handle_command("/model");
    assert!(app.model_chooser.active);

    for ch in "snnt".chars() {
        app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert_eq!(app.model_chooser.filtered.len(), 1);
    assert!(
        app.completion
            .as_ref()
            .is_some_and(|c| c.matches[0].value == "claude-sonnet-4-5")
    );
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("Filter: \"snnt\" (1 match)"))
    );

    // Tab cycles supported reasoning efforts on highlighted model.
    app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    let detail = app
        .completion
        .as_ref()
        .and_then(|c| c.matches[0].detail.as_deref())
        .unwrap_or("");
    assert!(detail.contains("effort:"), "{detail}");

    // Backspace clears query and restores all three models.
    for _ in 0..4 {
        app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    }
    assert_eq!(app.model_chooser.filtered.len(), 3);

    // Jump back to claude-sonnet-4-5 and select it.
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    let action = app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE));
    let id = match action {
        Some(FileBackedAction::SelectModel { id, .. }) => id,
        other => panic!("expected SelectModel, got {other:?}"),
    };
    app.observe_model_receipt(
        "/agent/1",
        &alan_agent_protocol::UiEvent::InputCompleted {
            submission_ids: vec![id],
            status: alan_agent_protocol::UiInputStatus::Completed,
            error: None,
        },
    );
    let receipt = app.notice.as_deref().unwrap_or("");
    assert!(receipt.contains("completed"), "{receipt}");
    assert!(
        !receipt.contains("projection owns selected-next"),
        "{receipt}"
    );

    // Effort & profile subcommands report helpful guidance.
    app.handle_command("/model effort high");
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("Preferred reasoning effort noted: high"))
    );
    app.handle_command("/model profile");
    assert!(
        app.notice
            .as_deref()
            .is_some_and(|n| n.contains("alan connection list"))
    );
}

#[test]
fn form_state_supports_text_cursor_and_select_navigation_inside_app() {
    let mut app = ready_app();
    app.set_pending_yield(PendingYieldCell {
        request_id: "req-multi-form".into(),
        kind: YieldKind::StructuredInput,
        title: "Details".into(),
        prompt: None,
        options: vec![],
        default_option: None,
        questions: vec![
            StructuredInputQuestion {
                id: "name".into(),
                label: "Name".into(),
                prompt: "Enter name".into(),
                kind: StructuredInputKind::Text,
                required: true,
                placeholder: None,
                help_text: None,
                default_value: None,
                default_values: vec![],
                min_selected: None,
                max_selected: None,
                options: vec![],
                presentation_hints: vec![],
            },
            StructuredInputQuestion {
                id: "mode".into(),
                label: "Mode".into(),
                prompt: "Choose mode".into(),
                kind: StructuredInputKind::SingleSelect,
                required: true,
                placeholder: None,
                help_text: None,
                default_value: None,
                default_values: vec![],
                min_selected: None,
                max_selected: None,
                options: vec![
                    StructuredInputOption {
                        value: "fast".into(),
                        label: "fast".into(),
                        description: None,
                    },
                    StructuredInputOption {
                        value: "thorough".into(),
                        label: "thorough".into(),
                        description: None,
                    },
                ],
                presentation_hints: vec![],
            },
            StructuredInputQuestion {
                id: "checks".into(),
                label: "Checks".into(),
                prompt: "Choose checks".into(),
                kind: StructuredInputKind::MultiSelect,
                required: false,
                placeholder: None,
                help_text: None,
                default_value: None,
                default_values: vec![],
                min_selected: None,
                max_selected: None,
                options: vec![
                    StructuredInputOption {
                        value: "fmt".into(),
                        label: "fmt".into(),
                        description: None,
                    },
                    StructuredInputOption {
                        value: "clippy".into(),
                        label: "clippy".into(),
                        description: None,
                    },
                ],
                presentation_hints: vec![],
            },
        ],
        capability: None,
        reason: None,
        presentation: None,
    });

    for ch in "ab".chars() {
        app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    app.handle_key(key(KeyCode::Left, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('X'), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Home, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('Z'), KeyModifiers::NONE));
    assert_eq!(app.form.as_ref().unwrap().fields[0].value, "ZaXb");

    // Tab to SingleSelect, Down to "thorough", Space selects.
    app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));

    // Tab to MultiSelect, Space selects "fmt", Down, Space selects "clippy".
    app.handle_key(key(KeyCode::Tab, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char(' '), KeyModifiers::NONE));

    match app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE)) {
        Some(FileBackedAction::Resume { retry_input, .. }) => {
            let parsed: serde_json::Value = serde_json::from_str(&retry_input).unwrap();
            assert_eq!(parsed["name"], "ZaXb");
            assert_eq!(parsed["mode"], "thorough");
            assert_eq!(parsed["checks"], "fmt,clippy");
        }
        other => panic!("expected Resume action, got {other:?}"),
    }
}

#[test]
fn action_modal_collapses_raw_json_until_d_toggles_and_scrolls() {
    let mut app = ready_app();
    app.transcript.push(HistoryCell::Tool {
        action: Some(ActionHistory {
            owner: "/agent/1".into(),
            id: "act-1".into(),
            read_only: None,
            frozen_rows: None,
        }),
        title: "edit_file src/main.rs".into(),
        status: ToolStatus::Complete,
        preview: Some("updated".into()),
        presentation: Some(ToolResultPresentation::Diff {
            path: "src/main.rs".into(),
            hunks: vec![DiffHunk {
                header: Some("@@ -1 +1 @@".into()),
                lines: vec![
                    DiffLine::Removed {
                        text: "old_fn()".into(),
                    },
                    DiffLine::Added {
                        text: "new_fn()".into(),
                    },
                ],
            }],
        }),
    });

    app.handle_key(key(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    assert!(!app.modal.show_raw_json);
    app.modal.actions = vec![action_detail_io::ActionEntry {
        owner: "/agent/1".into(),
        id: "act-1".into(),
    }];
    app.modal.pending = false;
    app.modal.rows = vec![
        Line::from("status: completed"),
        Line::from("diff header"),
        Line::from("Result metadata"),
        Line::from("  {\"secret_raw\":true}"),
    ];

    let collapsed = render_to_string(&app, 90, 20);
    assert!(collapsed.contains("status: completed"), "{collapsed}");
    assert!(collapsed.contains("press d to expand"), "{collapsed}");
    assert!(!collapsed.contains("secret_raw"), "{collapsed}");

    // Toggle raw JSON with `d`, then scroll with `j`/`k`.
    app.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(app.modal.show_raw_json);
    let expanded = render_to_string(&app, 90, 20);
    assert!(expanded.contains("secret_raw"), "{expanded}");

    app.handle_key(key(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 1);
    app.handle_key(key(KeyCode::Char('k'), KeyModifiers::NONE));
    assert_eq!(app.modal.scroll, 0);
}

#[test]
fn ctrl_s_steers_running_turn_and_empty_cancel_auto_continues() {
    let mut app = ready_app();
    app.activity = UiActivitySnapshot::running(10);
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 2,
            active_submission_ids: vec!["sub-1".into()],
            ..Default::default()
        }),
    );
    for ch in "steer msg".chars() {
        app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    match app.handle_key(key(KeyCode::Char('s'), KeyModifiers::CONTROL)) {
        Some(FileBackedAction::Submit(record)) => {
            assert_eq!(record.mode, InputMode::Steer);
            assert_eq!(record.body, "steer msg");
        }
        other => panic!("expected Steer submit, got {other:?}"),
    }

    // Cancelling while queue is empty suppresses Paused · /continue & requests continue.
    assert!(matches!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 3,
            paused: true,
            ..Default::default()
        }),
    );
    app.activity = UiActivitySnapshot {
        version: 1,
        state: alan_agent_protocol::UiActivityState::Paused,
        started_at_ms: None,
        waiting_submission_ids: Vec::new(),
    };
    assert!(
        !app.attention_labels()
            .iter()
            .any(|label| label.contains("Paused")),
        "empty cancel should not lock in Paused: {:?}",
        app.attention_labels()
    );
    assert!(app.take_ready_auto_continue_on_empty_cancel());
    assert!(!app.take_ready_auto_continue_on_empty_cancel());

    // Cancelling when another item is queued keeps Paused · /continue visible.
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 4,
            active_submission_ids: vec!["sub-2".into()],
            pending_submission_ids: vec!["sub-3".into()],
            ..Default::default()
        }),
    );
    app.activity = UiActivitySnapshot::running(20);
    assert!(matches!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));
    assert!(!app.auto_continue_on_empty_cancel);
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 5,
            paused: true,
            pending_submission_ids: vec!["sub-3".into()],
            ..Default::default()
        }),
    );
    assert!(
        app.attention_labels()
            .iter()
            .any(|label| label.contains("Paused")),
        "queued cancel should keep Paused: {:?}",
        app.attention_labels()
    );
}

#[test]
fn context_line_shows_git_usage_sandbox_and_respects_narrow_widths() {
    let mut app = ready_app();
    app.project = Some(ProjectMountReceipt {
        grant_id: "g1".into(),
        namespace_path: "/mnt/project".into(),
        label: "alan-repo".into(),
        access: ProjectAccess::ReadWrite,
    });
    app.git_status = Some(super::git_status::GitStatus {
        branch: "feat/tui".into(),
        dirty: true,
    });
    app.sandbox_backend = Some("seatbelt".into());
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 12_800,
            output_tokens: 400,
            context_window: 128_000,
            cost_usd: None,
            ..UiUsageSnapshot::empty()
        }),
    );

    let wide = app.context_line(140).to_string();
    assert!(wide.contains("alan-repo"), "{wide}");
    assert!(wide.contains("git:feat/tui*"), "{wide}");
    assert!(wide.contains("gpt-5.4"), "{wide}");
    assert!(wide.contains("ctx 10% (12.8k/128.0k)"), "{wide}");
    assert!(wide.contains("seatbelt/read-write"), "{wide}");

    for width in [18usize, 32, 48, 72] {
        let narrow = app.context_line(width).to_string();
        assert!(
            UnicodeWidthStr::width(narrow.as_str()) <= width,
            "width {width} exceeded: {narrow:?}"
        );
    }
}

#[test]
fn notices_humanize_jargon_and_live_bash_tail_and_at_images_work() {
    let mut app = ready_app();
    app.apply_ui_notice_snapshot(UiNoticeSnapshot {
        version: 1,
        kind: UiNoticeKind::Error,
        message: "stream_error:authentication invalid_api_key".into(),
    });
    let auth = app.notice.as_deref().unwrap_or("");
    assert!(auth.contains("alan connection login"), "{auth}");
    assert!(!auth.contains("stream_error:"), "{auth}");

    app.apply_ui_notice_snapshot(UiNoticeSnapshot {
        version: 1,
        kind: UiNoticeKind::Warning,
        message: "no connection service profile selected".into(),
    });
    let missing = app.notice.as_deref().unwrap_or("");
    assert!(missing.contains("alan connection add"), "{missing}");

    // Running bash tool renders tail lines in live region.
    app.activity = UiActivitySnapshot::running(10);
    app.running_tools.push(RunningTool {
        id: "act-bash".into(),
        title: "bash: cargo test".into(),
        tail_lines: vec!["compiling tui".into(), "running 42 tests".into()],
    });
    let (live_lines, _) = layout::live_region_lines_at(&app, 90, 2_000);
    let live_text = live_lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(live_text.contains("running 42 tests"), "{live_text}");

    // @image attachment inside mounted project_host_root attaches ContentPart::Attachment.
    let dir = tempfile::tempdir().expect("tempdir");
    let png = dir.path().join("shot.png");
    std::fs::write(
        &png,
        b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15c4",
    )
    .expect("write png");
    app.project_host_root = Some(dir.path().to_path_buf());
    app.activity = UiActivitySnapshot::idle();
    app.composer.set_text("inspect @shot.png please");
    match app.submit_with_mode(InputMode::FollowUp) {
        Some(FileBackedAction::Submit(record)) => {
            assert_eq!(record.parts.len(), 1, "{record:?}");
            match &record.parts[0] {
                ContentPart::Attachment {
                    mime_type,
                    metadata,
                    ..
                } => {
                    assert_eq!(mime_type, "image/png");
                    assert_eq!(metadata["filename"], "shot.png");
                }
                other => panic!("unexpected part: {other:?}"),
            }
        }
        other => panic!("expected Submit, got {other:?}"),
    }
}
