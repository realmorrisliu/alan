//! M6 unit tests for `/help` wrapping & `Esc` dismiss, `/login` wizard, honest onboarding,
//! trust posture cycling (`Shift+Tab` / `/permissions`), `Ctrl+Z` suspend dispatch,
//! true `/new`, `/fork [query]`, `/project` ReadWrite default, and `/` completion ordering + counter.

use alan_agent_protocol::{
    InputIntent, StructuredInputKind, StructuredInputQuestion, UiModelCatalog, UiModelChoice,
    UiModelControlSource, UiModelSnapshot, UiQueueSnapshot, YieldKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::app::humanize_protocol_message;
use super::help::try_render_help_lines;
use super::login::{
    LoginInputField, LoginMethod, LoginProgress, LoginProviderChoice, LoginResult,
    LoginSelectOption,
};
use super::man_view;
use super::stdio_runner::{StdioTaskSnapshot, finish_stdio_task_if_ready};
use super::welcome::{from_app, render_banner_lines};
use super::*;
use crate::history::PendingYieldCell;
use crate::theme::Theme;

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
                models: vec![UiModelChoice {
                    model: "gpt-5.4".into(),
                    supported_reasoning_efforts: vec![],
                    default_reasoning_effort: None,
                }],
                ..Default::default()
            }),
            ..Default::default()
        }),
    );
    app
}

#[test]
fn help_wraps_with_hanging_indent_and_idle_esc_dismisses_notice() {
    let mut app = ready_app();
    assert!(app.handle_command("/help").is_none());
    assert!(app.notice.is_some());

    let lines = try_render_help_lines(app.notice.as_deref().unwrap(), Theme::midnight(), 80)
        .expect("help lines");
    let rendered: Vec<String> = lines.iter().map(ToString::to_string).collect();
    let ctrl_o_idx = rendered
        .iter()
        .position(|line| line.starts_with("  Ctrl+O"))
        .expect("Ctrl+O row present");
    let wrapped = rendered
        .get(ctrl_o_idx + 1)
        .expect("Ctrl+O continuation line at 80 cols");
    assert!(
        wrapped.starts_with(&" ".repeat(29)),
        "expected hanging indent on wrapped description: {wrapped:?}"
    );

    assert!(
        app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.notice.is_none(), "idle Esc must dismiss /help notice");
}

#[test]
fn in_tui_login_wizard_supports_provider_selection_oauth_url_secondary_list_and_masked_secret() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.connection_configured = Some(false);
    app.login_available = true;
    app.maybe_insert_welcome_header();

    let action = app
        .handle_command("/login")
        .expect("/login requests providers");
    let login_gen = match action {
        FileBackedAction::LoginListProviders { generation } => generation,
        other => panic!("expected LoginListProviders, got {other:?}"),
    };

    app.apply_login_providers(
        login_gen,
        Ok(LoginResult::Providers(vec![
            LoginProviderChoice {
                id: "chatgpt".into(),
                label: "ChatGPT (Plus / Pro OAuth)".into(),
                detail: "Browser OAuth".into(),
                default_profile: "chatgpt-main".into(),
                method: LoginMethod::BrowserOrAuto,
            },
            LoginProviderChoice {
                id: "vertex".into(),
                label: "Google Cloud Vertex AI".into(),
                detail: "Google OAuth / ADC".into(),
                default_profile: "vertex-main".into(),
                method: LoginMethod::CloudProject,
            },
            LoginProviderChoice {
                id: "openai".into(),
                label: "OpenAI API".into(),
                detail: "API key".into(),
                default_profile: "openai-main".into(),
                method: LoginMethod::ApiKey {
                    allow_base_url: false,
                },
            },
        ])),
    );
    assert!(app.login.is_active());
    assert!(
        app.completion
            .as_ref()
            .is_some_and(|c| c.matches.len() == 3)
    );

    // Ctrl+Q does not quit during /login.
    assert!(
        app.handle_key(key(KeyCode::Char('q'), KeyModifiers::CONTROL))
            .is_none()
    );
    assert!(!app.should_quit);

    // Filter to vertex and select it.
    for ch in "vert".chars() {
        app.handle_key(key(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    assert_eq!(app.completion.as_ref().unwrap().matches.len(), 1);
    let submit = app
        .handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .expect("Enter submits selected provider");
    let login_gen = match submit {
        FileBackedAction::LoginSubmit {
            generation,
            submission,
            ..
        } => {
            assert_eq!(submission.provider_id, "vertex");
            generation
        }
        other => panic!("expected LoginSubmit, got {other:?}"),
    };

    // Host streams a browser URL while waiting.
    app.apply_login_progress(
        login_gen,
        LoginProgress::BrowserPrompt {
            url: "https://accounts.google.com/o/oauth2/v2/auth?state=abc".into(),
            user_code: Some("ABCD-1234".into()),
            hint: Some("Finish sign-in in browser".into()),
        },
    );
    let notice = app.notice.as_deref().unwrap_or("");
    assert!(notice.contains("https://accounts.google.com/o/oauth2/v2/auth?state=abc"));
    assert!(notice.contains("ABCD-1234"));

    // Host returns secondary GCP project selection list.
    app.apply_login_step_result(
        login_gen,
        Ok(LoginResult::NeedsSelection {
            title: "Select GCP Project".into(),
            prompt: "Choose active project".into(),
            options: vec![
                LoginSelectOption {
                    id: "proj-alpha".into(),
                    label: "Alpha Research".into(),
                    detail: Some("proj-alpha".into()),
                },
                LoginSelectOption {
                    id: "proj-prod".into(),
                    label: "Prod Vertex".into(),
                    detail: Some("proj-prod".into()),
                },
            ],
        }),
    );
    app.handle_key(key(KeyCode::Down, KeyModifiers::NONE));
    let next = app
        .handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .expect("selecting GCP project emits LoginSubmit");
    let gen2 = match next {
        FileBackedAction::LoginSubmit {
            generation,
            submission,
            ..
        } => {
            assert_eq!(submission.selection.as_deref(), Some("proj-prod"));
            assert_eq!(submission.selections, vec!["proj-prod"]);
            generation
        }
        other => panic!("expected LoginSubmit, got {other:?}"),
    };

    // Host asks for masked API key input, verify masking & Debug redaction.
    app.apply_login_step_result(
        gen2,
        Ok(LoginResult::NeedsInput {
            field: LoginInputField::Secret,
            title: "API Key".into(),
            prompt: "Paste your API key".into(),
            masked: true,
            allow_empty: false,
            default_value: None,
        }),
    );
    assert!(app.login.is_entering_input());
    app.dispatch(FileBackedEvent::Terminal(TerminalEvent::Paste(
        "sk-secret-1234".into(),
    )));
    let composer_rendered: String = app
        .composer_lines()
        .iter()
        .map(ToString::to_string)
        .collect();
    assert!(
        !composer_rendered.contains("sk-secret-1234"),
        "secret must be masked in UI: {composer_rendered:?}"
    );
    assert!(composer_rendered.contains("**************"));
    let next2 = app
        .handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .expect("Enter submits masked secret");
    let gen3 = match next2 {
        FileBackedAction::LoginSubmit {
            generation,
            submission,
            ..
        } => {
            assert_eq!(submission.secret.as_deref(), Some("sk-secret-1234"));
            let dbg = format!("{submission:?}");
            assert!(
                !dbg.contains("sk-secret-1234"),
                "Debug leaked secret: {dbg}"
            );
            assert!(dbg.contains("<redacted>"));
            generation
        }
        other => panic!("expected LoginSubmit, got {other:?}"),
    };

    // Final Connected result hot-refreshes effective_model, status line, and WelcomeBanner.
    let synced = app.apply_login_step_result(
        gen3,
        Ok(LoginResult::Connected {
            profile: "vertex-main".into(),
            provider: "vertex".into(),
            model: Some("gemini-2.5-pro".into()),
            message: None,
        }),
    );
    assert_eq!(
        synced,
        Some((
            "vertex-main".to_string(),
            Some("gemini-2.5-pro".to_string())
        ))
    );
    assert_eq!(app.connection_configured, Some(true));
    assert_eq!(app.effective_model.as_deref(), Some("gemini-2.5-pro"));
    assert!(!app.is_connection_unconfigured());
    let ctx = app.context_line(100).to_string();
    assert!(!ctx.contains("Not connected"), "{ctx}");
}

#[test]
fn unconfigured_first_run_is_honest_and_never_fakes_model() {
    let mut app = FileBackedApp::new("/agent/root".into());
    app.connection_configured = Some(false);
    assert!(app.is_connection_unconfigured());
    let ctx = app.context_line(100).to_string();
    assert!(ctx.contains("Not connected · run /login"), "{ctx}");
    assert!(!ctx.contains("gpt-5.4"), "{ctx}");

    let banner = from_app(&app);
    assert_eq!(banner.model, "Not connected");
    let wide = render_banner_lines(&banner, 80, Theme::midnight())
        .into_iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(wide.contains("Not connected"), "{wide}");
    assert!(wide.contains("/login to connect a provider"), "{wide}");
}

#[test]
fn protocol_and_stdio_errors_humanize_no_callable_connection_and_fix_man_and_model_hints() {
    let msg = humanize_protocol_message(
        "No callable Connection is available. Configure and select a Connection profile before submitting tasks.",
    );
    assert!(
        msg.contains("/login") && msg.contains("alan login"),
        "{msg}"
    );

    let mut snapshot = StdioTaskSnapshot {
        task_started: true,
        waiting_for_response: false,
        assistant_answer: None,
        command_output: None,
        activity_state: None,
        task_error: Some("No callable Connection is available.".into()),
        completion: Some(alan_agent_protocol::UiInputStatus::Failed),
    };
    let err = finish_stdio_task_if_ready(&mut snapshot)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("/login") && err.contains("alan login"),
        "{err}"
    );

    let (_, man_conn) = man_view::fallback_content("connection");
    assert!(!man_conn.contains("vertex_gemini"), "{man_conn}");
    assert!(man_conn.contains("vertex"), "{man_conn}");

    let mut app = ready_app();
    app.handle_command("/model effort");
    assert!(
        !app.notice
            .as_deref()
            .unwrap_or("")
            .contains("model_reasoning_effort")
    );
    app.handle_command("/model effort high");
    assert!(
        !app.notice
            .as_deref()
            .unwrap_or("")
            .contains("model_reasoning_effort")
    );
}

#[test]
fn shift_tab_and_permissions_cycle_trust_posture_and_respect_unsandboxed_and_forms() {
    let mut app = ready_app();
    app.maybe_insert_welcome_header();

    let first = app
        .handle_key(key(KeyCode::BackTab, KeyModifiers::SHIFT))
        .expect("Shift+Tab switches to confined-trusted");
    assert!(matches!(
        first,
        FileBackedAction::MachineCtl { ref command, .. } if command == "trust-posture confined-trusted"
    ));
    assert_eq!(app.trust_posture_label.as_deref(), Some("confined/trusted"));
    assert!(
        app.context_line(100)
            .to_string()
            .contains("confined/trusted")
    );

    let second = app
        .handle_key(key(KeyCode::Tab, KeyModifiers::SHIFT))
        .expect("Shift+Tab switches back to governed");
    assert!(matches!(
        second,
        FileBackedAction::MachineCtl { ref command, .. } if command == "trust-posture governed"
    ));
    assert_eq!(app.trust_posture_label.as_deref(), Some("governed"));
    assert!(app.context_line(100).to_string().contains("governed"));

    // /permissions explicit subcommands work too.
    assert!(matches!(
        app.handle_command("/permissions trusted"),
        Some(FileBackedAction::MachineCtl { ref command, .. }) if command == "trust-posture confined-trusted"
    ));

    // --unsafe-host-account blocks runtime posture switching cleanly.
    app.trust_posture_label = Some("UNSANDBOXED host-account".into());
    assert!(
        app.handle_key(key(KeyCode::BackTab, KeyModifiers::NONE))
            .is_none()
    );
    assert!(
        app.notice
            .as_deref()
            .unwrap_or("")
            .contains("--unsafe-host-account")
    );
    assert!(app.handle_command("/permissions").is_none());

    // Inside FormState, BackTab navigates fields instead of toggling trust posture.
    app.trust_posture_label = None;
    app.set_pending_yield(PendingYieldCell {
        request_id: "req-f".into(),
        kind: YieldKind::StructuredInput,
        title: "Form".into(),
        prompt: None,
        options: vec![],
        default_option: None,
        questions: ["a", "b"]
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
    });
    assert!(
        app.handle_key(key(KeyCode::BackTab, KeyModifiers::NONE))
            .is_none()
    );
    assert!(app.trust_posture_label.is_none());
}

#[test]
fn ctrl_z_dispatches_suspend_and_new_sends_reset_session() {
    let mut app = ready_app();
    assert!(matches!(
        app.handle_key(key(KeyCode::Char('z'), KeyModifiers::CONTROL)),
        Some(FileBackedAction::Suspend)
    ));

    app.transcript
        .push(crate::history::HistoryCell::User("old".into()));
    app.turn_prompts.push(("old".into(), false));
    assert!(matches!(
        app.handle_command("/new"),
        Some(FileBackedAction::MachineCtl { ref command, .. }) if command == "reset-session"
    ));
    assert!(app.transcript.is_empty());
    assert!(app.turn_prompts.is_empty());
}

#[test]
fn fork_command_picks_prompt_rolls_back_n_turns_and_populates_composer() {
    let mut app = ready_app();
    assert!(app.handle_command("/fork").is_none());
    assert!(
        app.notice
            .as_deref()
            .unwrap_or("")
            .contains("No earlier prompt")
    );

    app.turn_prompts = vec![
        ("first prompt".into(), false),
        ("ls -la".into(), true),
        ("latest prompt".into(), false),
    ];
    app.transcript = vec![
        crate::history::HistoryCell::User("first prompt".into()),
        crate::history::HistoryCell::Assistant("a1".into()),
        crate::history::HistoryCell::Command("ls -la".into()),
        crate::history::HistoryCell::User("latest prompt".into()),
        crate::history::HistoryCell::Assistant("a3".into()),
    ];

    // `/fork` defaults to latest turn (-1t) at index 0.
    assert!(app.handle_command("/fork").is_none());
    assert!(app.fork_picker.active);
    assert_eq!(app.completion.as_ref().unwrap().matches[0].label, "-1t");
    app.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.fork_picker.active);
    assert_eq!(app.turn_prompts.len(), 3);

    // `/fork first` filters directly to first prompt (-3t) and rolls back 3 turns.
    assert!(app.handle_command("/fork first").is_none());
    let action = app
        .handle_key(key(KeyCode::Enter, KeyModifiers::NONE))
        .expect("Enter forks selected prompt");
    assert!(matches!(
        action,
        FileBackedAction::MachineCtl { ref command, .. } if command == "rollback 3"
    ));
    assert!(!app.fork_picker.active);
    assert!(app.turn_prompts.is_empty());
    assert!(app.transcript.is_empty());
    assert_eq!(app.composer.text(), "first prompt");
    assert_eq!(app.input_intent, InputIntent::Agent);
}

#[test]
fn slash_completion_prioritizes_commands_and_renders_truncation_counter() {
    let mut app = ready_app();
    app.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
    let comp = app.completion.as_ref().expect("slash completion open");
    let top: Vec<&str> = comp
        .matches
        .iter()
        .take(6)
        .map(|c| c.value.as_str())
        .collect();
    assert_eq!(
        top,
        ["login", "model", "resume", "new", "fork", "permissions"]
    );

    let mut terminal = Terminal::new(TestBackend::new(90, 20)).unwrap();
    terminal
        .draw(|frame| layout::draw_at(frame, &app, 0))
        .unwrap();
    let buf = terminal.backend().buffer();
    let rendered: String = (0..20)
        .map(|y| (0..90).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    let expected_counter = format!("1-6/{}", comp.matches.len());
    assert!(
        rendered.contains(&expected_counter),
        "expected {expected_counter} counter in:\n{rendered}"
    );
}

#[test]
fn nondispatched_failed_submission_clears_unchanged_composer_and_preserves_edited_draft() {
    use super::interrupt::{PendingRootAgentTurn, observe_root_agent_completion};
    use alan_agent_protocol::{UiEvent, UiInputStatus};
    use std::collections::VecDeque;

    let mut app = FileBackedApp::new("/agent/8".into());
    app.connection_configured = Some(false);
    app.queue.apply("/agent/8", None);

    // 1. Unchanged draft submitted while unconfigured clears composer & enters history on Failed.
    app.composer.set_text("hello");
    app.track_local_input(
        "sub-1",
        "/agent/8".into(),
        "hello".into(),
        InputIntent::Agent,
    );
    app.show_submission_sent("sub-1");
    let mut pending = VecDeque::from([PendingRootAgentTurn {
        input: "hello".into(),
        submission_id: "sub-1".into(),
        submitted_process: Some(8),
        submitted_at_ms: 0,
    }]);
    let failed = UiEvent::InputCompleted {
        submission_ids: vec!["sub-1".into()],
        status: UiInputStatus::Failed,
        error: Some("No confirmed callable binding; generation input not admitted.".into()),
    };
    observe_root_agent_completion(&mut pending, &failed, &mut app);
    app.apply_ui_event(failed);

    assert!(pending.is_empty());
    assert!(
        app.composer.text().is_empty(),
        "hello stuck in composer: {:?}",
        app.composer.text()
    );
    assert!(app.last_input_failed);
    assert!(
        !app.rendered_history_lines(80)
            .iter()
            .any(|line| line.contains("hello")),
        "nondispatched failure must not push a User cell"
    );
    assert!(
        app.rendered_history_lines(80)
            .iter()
            .any(|line| line.contains("No active model connection is configured")),
        "missing humanized connection guidance in transcript: {:?}",
        app.rendered_history_lines(80)
    );
    // Up recalls the failed input from composer history, and clearing lets /login run cleanly.
    assert!(
        app.handle_key(key(KeyCode::Up, KeyModifiers::NONE))
            .is_none()
    );
    assert_eq!(app.composer.text(), "hello");

    // 2. If the user already edited a newer draft while waiting, Failed must not clobber it.
    app.composer.set_text("first");
    app.track_local_input(
        "sub-2",
        "/agent/8".into(),
        "first".into(),
        InputIntent::Agent,
    );
    app.composer.set_text("newer draft");
    let mut pending2 = VecDeque::from([PendingRootAgentTurn {
        input: "first".into(),
        submission_id: "sub-2".into(),
        submitted_process: Some(8),
        submitted_at_ms: 1,
    }]);
    let failed2 = UiEvent::InputCompleted {
        submission_ids: vec!["sub-2".into()],
        status: UiInputStatus::Failed,
        error: Some("No confirmed callable binding; generation input not admitted.".into()),
    };
    observe_root_agent_completion(&mut pending2, &failed2, &mut app);
    app.apply_ui_event(failed2);
    assert_eq!(app.composer.text(), "newer draft");
}
