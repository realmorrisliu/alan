use super::*;
use crate::composer::{Composer, ComposerKeyOutcome};
use crate::history::PendingYieldCell;
use alan_agent_protocol::{
    InputIntent, StructuredInputKind, StructuredInputQuestion, UiActivitySnapshot, UiModelCatalog,
    UiModelChoice, UiModelControlSource, UiModelSnapshot, UiQueueSnapshot, UiThinkingSnapshot,
    UiUsageSnapshot, YieldKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Modifier;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn ready_model_app() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            ..Default::default()
        }),
    );
    let mut snap = UiModelSnapshot {
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
            ],
            ..Default::default()
        }),
        ..Default::default()
    };
    snap.active = None;
    app.model.apply("/agent/1", Some(snap));
    app
}

fn structured_form_yield() -> PendingYieldCell {
    PendingYieldCell {
        request_id: "req-form".into(),
        kind: YieldKind::StructuredInput,
        title: "Form".into(),
        prompt: None,
        options: vec![],
        default_option: None,
        questions: ["first", "second"]
            .into_iter()
            .map(|id| StructuredInputQuestion {
                id: id.into(),
                label: id.into(),
                prompt: format!("{id}?"),
                kind: StructuredInputKind::Text,
                required: false,
                placeholder: None,
                help_text: None,
                default_value: None,
                default_values: vec![],
                min_selected: None,
                max_selected: None,
                options: vec![],
                presentation_hints: vec![],
            })
            .collect(),
        capability: None,
        reason: None,
        presentation: None,
    }
}

#[test]
fn ctrl_q_inside_modals_and_choosers_does_not_quit_or_leak() {
    // model_chooser
    let mut app = ready_model_app();
    app.handle_command("/model");
    assert!(app.model_chooser.active);
    assert!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(!app.should_quit);
    assert!(app.model_chooser.active);

    // ActionModal
    let mut app = ready_model_app();
    app.handle_key(key(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    assert!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(!app.should_quit);
    assert!(app.modal.active);

    // project_selection
    let mut app = ready_model_app();
    app.project_candidate = Some(std::path::PathBuf::from("/tmp/proj"));
    app.handle_command("/project");
    assert!(app.project_selection.is_some());
    assert!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(!app.should_quit);
    assert_eq!(app.composer.text(), "/tmp/proj");

    // FormState
    let mut app = ready_model_app();
    app.set_pending_yield(structured_form_yield());
    assert!(app.form.is_some());
    assert!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(!app.should_quit);
    assert_eq!(app.form.as_ref().unwrap().fields[0].value, "");

    // Idle prompt still quits on Ctrl+Q
    let mut app = ready_model_app();
    assert!(matches!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL)),
        Some(FileBackedAction::Quit)
    ));
    assert!(app.should_quit);
}

#[test]
fn unhandled_ctrl_and_alt_chars_do_not_leak_into_composer_or_form() {
    let mut composer = Composer::default();
    for mods in [
        KeyModifiers::CONTROL,
        KeyModifiers::ALT,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ] {
        assert_eq!(
            composer.handle_key(key(KeyCode::Char('x'), mods)),
            ComposerKeyOutcome::Ignored
        );
    }
    assert!(composer.text().is_empty());

    let mut app = ready_model_app();
    app.handle_key(key(KeyCode::Char('x'), KeyModifiers::CONTROL));
    app.handle_key(key(KeyCode::Char('k'), KeyModifiers::ALT));
    assert!(app.composer.text().is_empty());

    app.set_pending_yield(structured_form_yield());
    app.handle_key(key(KeyCode::Char('x'), KeyModifiers::CONTROL));
    app.handle_key(key(KeyCode::Char('k'), KeyModifiers::ALT));
    assert!(app.form.as_ref().unwrap().fields[0].value.is_empty());
}

#[test]
fn slash_model_submit_clears_composer_remembers_and_enter_selects_first_item() {
    let mut app = ready_model_app();
    for ch in "/model".chars() {
        app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert!(
        app.handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.model_chooser.active);
    assert!(
        app.composer.text().is_empty(),
        "composer must be cleared after /model submit"
    );

    let action = app
        .handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .expect("Enter on highlighted first item should select it");
    assert!(matches!(
        action,
        FileBackedAction::SelectModel {
            op: alan_agent_protocol::Op::SelectModel { ref model },
            ..
        } if model == "gpt-5.4"
    ));

    // Verify /model was remembered in composer history.
    app.handle_key(key(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.composer.text(), "/model");
}

#[test]
fn idle_esc_dismisses_completion_or_noops_without_interrupting() {
    let mut app = ready_model_app();
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.notice.is_none());

    app.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(app.completion.is_some());
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.completion.is_none());

    // Even when a project is already mounted, idle Esc dismisses completion or noops calmly.
    app.project = Some(ProjectMountReceipt {
        grant_id: "g1".into(),
        namespace_path: "/mnt/project".into(),
        label: "proj".into(),
        access: ProjectAccess::ReadOnly,
    });
    app.handle_key(key(KeyCode::Backspace, KeyModifiers::NONE));
    app.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    assert!(app.completion.is_some());
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.completion.is_none());
    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );

    // When a turn is active, Esc interrupts immediately.
    app.activity = UiActivitySnapshot::running(1);
    assert!(matches!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(FileBackedAction::Interrupt)
    ));
}

#[test]
fn fresh_boot_and_short_command_avoid_false_queue_model_and_uuid_flashes() {
    let mut app = ready_model_app();
    app.queue = queue::QueueProjection::default();
    app.queue
        .apply("/agent/1", Some(UiQueueSnapshot::default()));
    assert!(
        !app.attention_labels()
            .iter()
            .any(|label| label.contains("Queue unavailable")),
        "fresh boot queue must not warn: {:?}",
        app.attention_labels()
    );

    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    app.track_local_input(uuid, "/agent/1".into(), "pwd".into(), InputIntent::Command);
    app.show_submission_sent(uuid);
    assert!(app.notice.is_none(), "unexpected notice: {:?}", app.notice);

    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            active_submission_ids: vec![uuid.into()],
            ..Default::default()
        }),
    );
    app.activity = UiActivitySnapshot::running(10);
    app.refresh_local_input_hint(uuid, "/agent/1");
    assert!(
        app.notice.is_none(),
        "short command flashed notice: {:?}",
        app.notice
    );
    let header = app.context_line(80).to_string();
    assert!(
        header.contains("gpt-5.4") && !header.contains("model unknown"),
        "short command flashed model unknown: {header}"
    );
}

#[test]
fn streaming_thinking_renders_thinking_label_and_live_preview() {
    let mut app = ready_model_app();
    app.apply_ui_activity_snapshot(UiActivitySnapshot::running(1_000));
    app.apply_ui_thinking_snapshot(UiThinkingSnapshot::streaming(
        "Inspecting workspace layout\nChecking Cargo.toml".into(),
    ));
    let (lines, _) = layout::live_region_lines_at(&app, 80, 5_000);
    let text = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Thinking · 4s · Esc interrupt"), "{text}");
    assert!(text.contains("Inspecting workspace layout"), "{text}");
    assert!(text.contains("Checking Cargo.toml"), "{text}");
    assert!(
        lines.iter().any(|line| {
            line.to_string().contains("Checking Cargo.toml")
                && (line.style.add_modifier.contains(Modifier::DIM)
                    || line
                        .spans
                        .iter()
                        .all(|s| s.style.add_modifier.contains(Modifier::DIM)))
        }),
        "live thinking preview should be dimmed"
    );

    app.apply_ui_activity_snapshot(UiActivitySnapshot::idle());
    assert_eq!(
        app.thinking.state,
        alan_agent_protocol::UiThinkingState::Idle
    );
}

#[test]
fn usage_snapshot_renders_in_context_line_and_status() {
    let mut app = ready_model_app();
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 12_800,
            output_tokens: 500,
            context_window: 128_000,
            cost_usd: Some(0.0125),
            session_input_tokens: 12_800,
            session_output_tokens: 500,
            session_cost_usd: Some(0.0125),
            ..UiUsageSnapshot::empty()
        }),
    );
    let header = app.context_line(100).to_string();
    assert!(header.contains("ctx 10% (12.8k/128.0k)"), "{header}");
    assert!(header.contains("$0.0125"), "{header}");
    app.handle_command("/status");
    let status = app.notice.as_deref().unwrap();
    assert!(status.contains("usage: turn in 12800 out 500"), "{status}");
}

#[test]
fn usage_context_occupancy_uses_context_tokens_and_reports_cache_hit_rate() {
    let mut app = ready_model_app();

    // Multi-step tool turn: cumulative input_tokens is 2x the single-generation context_tokens.
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 25_600,
            cached_input_tokens: Some(19_200),
            output_tokens: 500,
            context_tokens: 12_800,
            context_window: 128_000,
            cost_usd: Some(0.0125),
            session_input_tokens: 51_200,
            session_cached_input_tokens: Some(30_720),
            session_output_tokens: 1_000,
            session_cost_usd: Some(0.025),
            ..UiUsageSnapshot::empty()
        }),
    );
    let header = app.context_line(140).to_string();
    assert!(
        header.contains("ctx 10% (12.8k/128.0k)"),
        "ctx must reflect single-generation context_tokens, not cumulative input_tokens: {header}"
    );
    assert!(header.contains("75% hit"), "{header}");
    assert!(header.contains("$0.0250"), "{header}");
    assert_eq!(app.usage.short_prompt_label().as_deref(), Some("ctx 10%"));

    app.handle_command("/status");
    let status = app.notice.as_deref().unwrap();
    assert!(
        status.contains("usage: turn in 25600 out 500 (cached 19200, 75%)"),
        "{status}"
    );
    assert!(
        status.contains("session in 51200 out 1000 (cached 30720, 60%)"),
        "{status}"
    );
    assert!(status.contains("ctx 10% (12800/128000)"), "{status}");

    // At turn start, reset_turn zeroes input_tokens & cached_input_tokens while preserving
    // context_tokens and session totals: ctx must not flicker to `... tok`, and cache falls back
    // cleanly to the session hit rate.
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 0,
            cached_input_tokens: None,
            output_tokens: 0,
            context_tokens: 12_800,
            context_window: 128_000,
            cost_usd: None,
            session_input_tokens: 51_200,
            session_cached_input_tokens: Some(30_720),
            session_output_tokens: 1_000,
            session_cost_usd: Some(0.025),
            ..UiUsageSnapshot::empty()
        }),
    );
    let reset_label = app.usage.prompt_label().unwrap();
    assert!(
        reset_label.contains("ctx 10% (12.8k/128.0k)") && reset_label.contains("60% hit"),
        "{reset_label}"
    );
    assert!(!reset_label.contains(" tok"), "{reset_label}");

    // Cold turn with input_tokens > 0 and 0 cached tokens must not reuse stale session hit rate.
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 10_000,
            cached_input_tokens: Some(0),
            output_tokens: 200,
            context_tokens: 10_000,
            context_window: 128_000,
            session_input_tokens: 61_200,
            session_cached_input_tokens: Some(30_720),
            session_output_tokens: 1_200,
            ..UiUsageSnapshot::empty()
        }),
    );
    let cold_label = app.usage.prompt_label().unwrap();
    assert!(
        cold_label.contains("ctx 8% (10.0k/128.0k)") && !cold_label.contains("% hit"),
        "{cold_label}"
    );

    // Defensive bounds: cached > input clamps to 100%, and input == 0 never divides by zero.
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 1_000,
            cached_input_tokens: Some(1_500),
            context_tokens: 1_000,
            context_window: 10_000,
            ..UiUsageSnapshot::empty()
        }),
    );
    assert!(
        app.usage.prompt_label().unwrap().contains("100% hit"),
        "{:?}",
        app.usage.prompt_label()
    );
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            input_tokens: 0,
            cached_input_tokens: Some(50),
            context_tokens: 500,
            context_window: 10_000,
            session_input_tokens: 0,
            session_cached_input_tokens: Some(50),
            ..UiUsageSnapshot::empty()
        }),
    );
    let zero_input_label = app.usage.prompt_label().unwrap();
    assert!(
        zero_input_label.contains("ctx 5%") && !zero_input_label.contains("% hit"),
        "{zero_input_label}"
    );
    let zero_input_status = app.usage.status().unwrap();
    assert!(
        zero_input_status.contains("(cached 50)"),
        "{zero_input_status}"
    );
}
