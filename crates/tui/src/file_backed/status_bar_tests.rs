use std::sync::Arc;

use alan_agent_protocol::{
    ReasoningControls, ReasoningEffort, UI_SURFACE_VERSION, UiActivitySnapshot, UiBudgetSnapshot,
    UiModelBinding, UiModelCatalog, UiModelChoice, UiModelControlSource, UiModelSnapshot,
    UiQueueSnapshot, UiUsageSnapshot,
};
use alan_agentfs::{AgentFs, AgentRootFs};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace, ProcFs};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use unicode_width::UnicodeWidthStr;

use super::{row1_line, row2_line, status_bar_lines};
use crate::file_backed::app::FileBackedAction;
use crate::file_backed::app::action_modal::ModalMode;
use crate::file_backed::git_status::GitStatus;
use crate::file_backed::layout::draw;
use crate::file_backed::modal_view::{hit_test_modal_tab, modal_tab_bar};
use crate::file_backed::project::ProjectMountReceipt;
use crate::file_backed::resume_picker::{RolloutHistoryCatalogView, RolloutHistoryItem};
use crate::file_backed::status_panel::{self, StatusDiagnostics};
use crate::file_backed::streams::{WorkstreamCountsDto, WorkstreamOverviewDto};
use crate::file_backed::{FileBackedApp, ProjectAccess};
use crate::theme::Theme;

fn has_nerd_font_glyph(text: &str) -> bool {
    text.chars().any(|ch| {
        let code = ch as u32;
        (0xE000..=0xF8FF).contains(&code)
            || (0xF0000..=0xFFFFD).contains(&code)
            || (0x100000..=0x10FFFD).contains(&code)
    })
}

fn populated_app() -> FileBackedApp {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.project = Some(ProjectMountReceipt {
        grant_id: "g1".into(),
        namespace_path: "/mnt/project".into(),
        label: "alan".into(),
        access: ProjectAccess::ReadWrite,
    });
    app.git_status = Some(GitStatus {
        branch: "feat/tui".into(),
        dirty: true,
    });
    app.sandbox_backend = Some("seatbelt".into());
    app.trust_posture_label = None; // governed
    let binding = UiModelBinding {
        profile: "openai-main".into(),
        provider: "openai".into(),
        model: "gpt-5.4".into(),
        reasoning: ReasoningControls {
            effort: Some(ReasoningEffort::High),
        },
        control_source: UiModelControlSource::AgentConfig,
    };
    app.model.apply(
        "/agent/1",
        Some(UiModelSnapshot {
            version: UI_SURFACE_VERSION,
            process_path: "/proc/1".into(),
            publication_version: 1,
            known: true,
            selected_next: Some(binding.clone()),
            active: Some(binding),
            catalog: Some(UiModelCatalog {
                profile: "openai-main".into(),
                models: vec![
                    UiModelChoice {
                        model: "gpt-5.4".into(),
                        supported_reasoning_efforts: vec![
                            ReasoningEffort::Medium,
                            ReasoningEffort::High,
                        ],
                        default_reasoning_effort: Some(ReasoningEffort::High),
                    },
                    UiModelChoice {
                        model: "gpt-5.4-mini".into(),
                        supported_reasoning_efforts: vec![
                            ReasoningEffort::Low,
                            ReasoningEffort::Medium,
                        ],
                        default_reasoning_effort: Some(ReasoningEffort::Medium),
                    },
                ],
                profiles: Vec::new(),
            }),
            admitted: Vec::new(),
        }),
    );
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            version: UI_SURFACE_VERSION,
            input_tokens: 10_000,
            cached_input_tokens: Some(6_000),
            output_tokens: 500,
            context_tokens: 54_000,
            context_window: 128_000,
            session_input_tokens: 50_000,
            session_cached_input_tokens: Some(30_000),
            session_output_tokens: 2_000,
            session_cost_usd: Some(0.025),
            ..UiUsageSnapshot::empty()
        }),
    );
    app.budget.apply(
        "/agent/1",
        Some(UiBudgetSnapshot {
            version: UI_SURFACE_VERSION,
            elapsed_secs: 1_080,
            max_secs: 1_800,
            tool_calls: 84,
            max_tool_calls: 256,
            model_calls: 43,
            max_model_calls: 64,
            auto_extensions_used: Some(1),
            max_auto_extensions: Some(3),
        }),
    );
    app.streams.apply_overview(WorkstreamOverviewDto {
        project_key: Some("alan".into()),
        counts: WorkstreamCountsDto {
            running: 1,
            idle: 2,
            paused_error: 0,
            archived: 0,
            unread_inbox: 3,
        },
        streams: Vec::new(),
        unread_inbox: Vec::new(),
    });
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            ..Default::default()
        }),
    );
    app.cow_branch_count = 2;
    app.ephemeral = true;
    app.no_resume_update = true;
    app
}

fn render_screen(app: &FileBackedApp, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf.cell((x, y)).unwrap().symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

#[test]
fn wide_narrow_and_dumb_status_bar_layout_hides_sandbox_backend_and_uses_no_nerd_fonts() {
    let mut app = populated_app();
    app.activity = UiActivitySnapshot::running(1);
    app.queue.apply(
        "/agent/1",
        Some(UiQueueSnapshot {
            known: true,
            revision: 1,
            pending_submission_ids: vec!["q1".into(), "q2".into()],
            ..Default::default()
        }),
    );

    // Wide layout: activity above composer, 2-row status bar below composer.
    let rendered = render_screen(&app, 140, 10);
    let activity_idx = rendered
        .iter()
        .position(|l| l.contains("Working"))
        .expect("transient activity above composer");
    let composer_idx = rendered
        .iter()
        .position(|l| l.starts_with(':'))
        .expect("composer row");
    let row1_idx = rendered
        .iter()
        .position(|l| l.contains("alan") && l.contains("gpt-5.4"))
        .expect("status row 1 below composer");
    let row2_idx = rendered
        .iter()
        .position(|l| l.contains("ctx 42%") && l.contains("18m/30m 84/256t 43/64m"))
        .expect("status row 2 below composer");
    assert!(activity_idx < composer_idx, "{rendered:?}");
    assert!(
        composer_idx < row1_idx && row1_idx + 1 == row2_idx,
        "{rendered:?}"
    );

    let row1 = &rendered[row1_idx];
    let row2 = &rendered[row2_idx];
    assert!(row1.contains("feat/tui*"), "{row1}");
    assert!(row1.contains("rw·gov"), "{row1}");
    assert!(row1.contains("openai-main/gpt-5.4 · high"), "{row1}");
    assert!(row2.contains("60% hit"), "{row2}");
    assert!(row2.contains("$0.0250"), "{row2}");
    assert!(row2.contains("1a·2i·3in"), "{row2}");
    assert!(row2.contains("queued 2"), "{row2}");
    assert!(row2.contains("cow:2"), "{row2}");
    assert!(row2.contains("ephemeral"), "{row2}");
    assert!(row2.contains("no-resume"), "{row2}");

    // Daily status bar MUST NEVER leak host sandbox backend names or Nerd Font glyphs.
    for backend in ["seatbelt", "landlock"] {
        app.sandbox_backend = Some(backend.into());
        for width in [140usize, 100, 79, 64, 48, 32] {
            let [r1, r2] = status_bar_lines(&app, width);
            let s1 = r1.to_string();
            let s2 = r2.to_string();
            assert!(s1.width() <= width, "row1 overflow w={width}: {s1:?}");
            assert!(s2.width() <= width, "row2 overflow w={width}: {s2:?}");
            assert!(!s1.to_lowercase().contains(backend), "{s1}");
            assert!(!s2.to_lowercase().contains(backend), "{s2}");
            assert!(!has_nerd_font_glyph(&s1), "{s1:?}");
            assert!(!has_nerd_font_glyph(&s2), "{s2:?}");
        }
    }

    // Narrow (<80 cols) degrades profile -> git -> posture -> effort cleanly without overflow.
    let narrow_r1 = row1_line(&app, 48).to_string();
    assert!(narrow_r1.width() <= 48, "{narrow_r1:?}");
    assert!(narrow_r1.contains("gpt-5.4"), "{narrow_r1:?}");
    assert!(!narrow_r1.contains("openai-main/"), "{narrow_r1:?}");

    // Dumb ASCII theme uses pure ASCII everywhere on both status rows.
    app.theme = Theme::dumb();
    for width in [120usize, 72, 48, 28] {
        let [r1, r2] = status_bar_lines(&app, width);
        let s1 = r1.to_string();
        let s2 = r2.to_string();
        assert!(s1.is_ascii(), "non-ASCII in dumb row1 (w={width}): {s1:?}");
        assert!(s2.is_ascii(), "non-ASCII in dumb row2 (w={width}): {s2:?}");
        assert!(s1.width() <= width, "dumb row1 overflow w={width}: {s1:?}");
        assert!(s2.width() <= width, "dumb row2 overflow w={width}: {s2:?}");
    }
}

#[test]
fn unsandboxed_badge_and_semantic_threshold_colors_match_contract() {
    let mut app = populated_app();
    let theme = app.theme;

    // Unsandboxed posture renders bold red `! unsandboxed` and survives narrow widths.
    app.trust_posture_label = Some("unsandboxed".into());
    for width in [100usize, 60, 44, 34] {
        let line = row1_line(&app, width);
        let text = line.to_string();
        assert!(text.width() <= width, "w={width}: {text:?}");
        assert!(text.contains("! unsandboxed"), "w={width}: {text:?}");
        let badge_span = line
            .spans
            .iter()
            .find(|s| s.content.contains("! unsandboxed"))
            .expect("unsandboxed span");
        assert_eq!(badge_span.style, theme.bold_if_capable(theme.block_style()));
    }

    // Amber (>=75%) and Red (>=90%) for ctx%, budget ratio, and inbox/paused_error.
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            version: UI_SURFACE_VERSION,
            context_tokens: 96_000, // 75%
            context_window: 128_000,
            ..UiUsageSnapshot::empty()
        }),
    );
    app.budget.apply(
        "/agent/1",
        Some(UiBudgetSnapshot {
            version: UI_SURFACE_VERSION,
            elapsed_secs: 1_350, // 75%
            max_secs: 1_800,
            tool_calls: 10,
            max_tool_calls: 256,
            model_calls: 5,
            max_model_calls: 64,
            auto_extensions_used: None,
            max_auto_extensions: None,
        }),
    );
    app.streams.apply_overview(WorkstreamOverviewDto {
        project_key: Some("alan".into()),
        counts: WorkstreamCountsDto {
            running: 1,
            idle: 0,
            paused_error: 2,
            archived: 0,
            unread_inbox: 1,
        },
        streams: Vec::new(),
        unread_inbox: Vec::new(),
    });
    let amber_row = row2_line(&app, 140);
    let ctx_span = amber_row
        .spans
        .iter()
        .find(|s| s.content.contains("ctx 75%"))
        .expect("amber ctx span");
    assert_eq!(ctx_span.style, theme.wait_style());
    let budget_span = amber_row
        .spans
        .iter()
        .find(|s| s.content.contains("22m/30m"))
        .expect("amber budget span");
    assert_eq!(budget_span.style, theme.wait_style());
    let stream_span = amber_row
        .spans
        .iter()
        .find(|s| s.content.contains("1a·0i·2p·1in"))
        .expect("amber streams paused/inbox span");
    assert_eq!(stream_span.style, theme.wait_style());

    // >=90% upgrades ctx% and budget to red (block_style).
    app.usage.apply(
        "/agent/1",
        Some(UiUsageSnapshot {
            version: UI_SURFACE_VERSION,
            context_tokens: 118_000, // 92%
            context_window: 128_000,
            ..UiUsageSnapshot::empty()
        }),
    );
    app.budget.apply(
        "/agent/1",
        Some(UiBudgetSnapshot {
            version: UI_SURFACE_VERSION,
            elapsed_secs: 100,
            max_secs: 1_800,
            tool_calls: 240, // 93.75%
            max_tool_calls: 256,
            model_calls: 5,
            max_model_calls: 64,
            auto_extensions_used: None,
            max_auto_extensions: None,
        }),
    );
    let red_row = row2_line(&app, 140);
    let ctx_red = red_row
        .spans
        .iter()
        .find(|s| s.content.contains("ctx 92%"))
        .expect("red ctx span");
    assert_eq!(ctx_red.style, theme.block_style());
    let budget_red = red_row
        .spans
        .iter()
        .find(|s| s.content.contains("240/256t"))
        .expect("red budget span");
    assert_eq!(budget_red.style, theme.block_style());
}

#[test]
fn ctrl_t_and_ctrl_o_s_open_status_panel_with_sandbox_diagnostics() {
    let mut app = populated_app();
    app.sandbox_backend = Some("seatbelt".into());
    app.status_diagnostics = StatusDiagnostics {
        loaded: true,
        mnt_entries: vec!["branch".into(), "github".into(), "project".into()],
        host_grants: vec!["/mnt/project (read_write, active)".into()],
        fileserver_packages: vec!["github-fileserver -> /mnt/github (active)".into()],
        cow_branches: vec!["b1 (active)".into()],
        summary_line: Some("rev 4 · generated · recall on".into()),
    };

    // Ctrl+T opens Status panel directly.
    assert!(!app.modal.active);
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    assert_eq!(app.modal.mode, ModalMode::Status);

    let panel_text = render_screen(&app, 100, 38).join("\n");
    for expected in [
        "Status · /agent/1",
        "[s:Status]",
        "Task Budget",
        "18m00s / 30m00s",
        "84 / 256",
        "43 / 64",
        "auto-extensions: 1 / 3",
        "Connection & Model",
        "openai-main / openai / gpt-5.4 (effort: high)",
        "Mounts & Packages (/mnt/*)",
        "github-fileserver -> /mnt/github (active)",
        "CoW Branches (/mnt/branch)",
        "b1 (active)",
        "Project Continuity (/summary)",
        "rev 4 · generated · recall on",
        "System 1 (Fast Evaluator)",
        "OS Sandbox & Isolation",
        "seatbelt",
    ] {
        assert!(
            panel_text.contains(expected),
            "missing `{expected}` in:\n{panel_text}"
        );
    }

    // Esc closes Status panel.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.modal.active);

    // Ctrl+O opens Actions, then `s` switches to Status tab.
    app.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL));
    assert!(app.modal.active);
    assert_eq!(app.modal.mode, ModalMode::Actions);
    app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.modal.mode, ModalMode::Status);
    let rows_text: String = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rows_text.contains("OS Sandbox & Isolation"), "{rows_text}");
}

#[test]
fn modal_only_mouse_capture_scroll_and_tab_row_clicks_behave_deterministically() {
    let mut app = populated_app();
    app.viewport_size = (100, 30);

    // 1. Inline mode: mouse capture strictly OFF and mouse events ignored.
    assert!(!app.modal_mouse_capture_wanted());
    assert!(
        app.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 10,
                row: 5,
                modifiers: KeyModifiers::NONE,
            },
            false
        )
        .is_none()
    );

    // 2. Open Ctrl+T Status modal -> mouse capture ON (OFF in dumb mode).
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));
    assert!(app.modal_mouse_capture_wanted());
    app.theme = Theme::dumb();
    assert!(!app.modal_mouse_capture_wanted());
    app.theme = Theme::default();
    assert!(app.modal_mouse_capture_wanted());

    // Render to populate `last_frame_origin`.
    let _ = render_screen(&app, 100, 30);

    // Mouse wheel scrolls modal by 3 lines.
    assert_eq!(app.modal.scroll, 0);
    app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert_eq!(app.modal.scroll, 3);
    app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert_eq!(app.modal.scroll, 0);

    // Clicking tab bar hits `a:Actions`, `p:Plans`, `t:Tape`, `s:Status`.
    let (_, hits) = modal_tab_bar(ModalMode::Status, app.theme, 100);
    assert_eq!(hits.len(), 4);
    let actions_hit = &hits[0];
    assert_eq!(
        hit_test_modal_tab(ModalMode::Status, app.theme, 100, actions_hit.start_col, 1),
        Some(ModalMode::Actions)
    );
    app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: actions_hit.start_col,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert_eq!(app.modal.mode, ModalMode::Actions);

    // Close modal -> mouse capture OFF again.
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.modal_mouse_capture_wanted());

    // 3. `/resume` picker: mouse capture ON, wheel moves selection, click selects & double-click confirms.
    let generation = app.resume_picker.open("");
    app.resume_picker.apply_loaded(
        generation,
        Ok(RolloutHistoryCatalogView {
            entries: vec![
                RolloutHistoryItem {
                    rollout_id: "r1".into(),
                    file_name: None,
                    updated_at: "2026-11-26T00:00:00Z".into(),
                    project_cwd: None,
                    project_label: Some("alan".into()),
                    project_fingerprint: None,
                    turn_count: 2,
                    last_status: "idle".into(),
                    goal_snippet: "first task".into(),
                    has_pending_checkpoint: false,
                    has_unknown_effects: false,
                    corrupt_lines: 0,
                },
                RolloutHistoryItem {
                    rollout_id: "r2".into(),
                    file_name: None,
                    updated_at: "2026-11-25T00:00:00Z".into(),
                    project_cwd: None,
                    project_label: Some("alan".into()),
                    project_fingerprint: None,
                    turn_count: 4,
                    last_status: "idle".into(),
                    goal_snippet: "second task".into(),
                    has_pending_checkpoint: false,
                    has_unknown_effects: false,
                    corrupt_lines: 0,
                },
            ],
            ..Default::default()
        }),
    );
    assert!(app.modal_mouse_capture_wanted());
    let _ = render_screen(&app, 100, 24);
    assert_eq!(app.resume_picker.selected, 0);
    app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert_eq!(app.resume_picker.selected, 1);
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.modal_mouse_capture_wanted());

    // 4. `/model` chooser: mouse capture ON, wheel navigates, row click selects & second click applies.
    app.composer.set_text("/model");
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app.modal_mouse_capture_wanted());
    let _ = render_screen(&app, 100, 14);
    let menu_y = app
        .last_menu_origin_y
        .get()
        .expect("model chooser menu origin recorded");
    assert_eq!(app.model_chooser.index, 0);
    // First click on row 1 selects `gpt-5.4-mini`.
    let first_click = app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: menu_y + 1,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert!(first_click.is_none());
    assert_eq!(app.model_chooser.index, 1);
    // Second click on already-selected row confirms the selection.
    let second_click = app.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 4,
            row: menu_y + 1,
            modifiers: KeyModifiers::NONE,
        },
        false,
    );
    assert!(
        matches!(second_click, Some(FileBackedAction::SelectModel { .. })),
        "{second_click:?}"
    );
    assert!(!app.modal_mouse_capture_wanted());
}

#[tokio::test]
async fn budget_read_and_status_panel_diagnostics_read_live_namespace_files() {
    let procfs = Arc::new(ProcFs::new());
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let summary_json = serde_json::json!({
        "exists": true,
        "deleted": false,
        "project_key": "alan",
        "summary": {
            "goal": "Ship PR 2",
            "progress": "Done",
            "verification": "Green",
            "source_rollouts": [],
            "revision": 7,
            "updated_at": "2026-11-26T00:00:00Z",
            "origin": "generated",
            "deleted": false
        },
        "markdown_recall_active": true,
        "retention_note": "derived"
    })
    .to_string()
    .into_bytes();
    agent_root
        .bind_history_server(Arc::new(MemFs::with_read_only_file(
            "summary",
            summary_json,
        )))
        .await;

    let mount_fs = Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    live.mount(
        "/mnt/branch/branches",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_files([
            ("b-active".into(), br#"{"status":"active"}"#.to_vec()),
            ("b-merged".into(), br#"{"status":"merged"}"#.to_vec()),
        ]))),
        Access::ReadOnly,
    );
    let catalog_bytes = serde_json::json!({
        "packages": {
            "github-fileserver": {
                "state": "installed",
                "exports": [{"kind": "fileserver", "service_id": "github"}]
            }
        }
    })
    .to_string()
    .into_bytes();
    live.mount(
        "/mnt/package",
        InProcessTransport::new(Arc::new(MemFs::with_read_only_file(
            "catalog",
            catalog_bytes,
        ))),
        Access::ReadOnly,
    );
    live.mount(
        "/mnt/github",
        InProcessTransport::new(Arc::new(MemFs::new())),
        Access::ReadOnly,
    );

    let shell = alan_shell::Shell::new(InProcessTransport::new(mount_fs));
    let pid = shell
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(pid.clone(), Arc::new(AgentFs::new()))
        .await;
    let owner = format!("/agent/{pid}");

    // Write UiBudgetSnapshot into AgentFS machine/ui/budget and read back via TUI consumer.
    let expected = UiBudgetSnapshot {
        version: UI_SURFACE_VERSION,
        elapsed_secs: 600,
        max_secs: 1800,
        tool_calls: 25,
        max_tool_calls: 256,
        model_calls: 8,
        max_model_calls: 64,
        auto_extensions_used: None,
        max_auto_extensions: None,
    };
    shell
        .write(
            &format!("{owner}/machine/ui/budget"),
            &serde_json::to_vec(&expected).unwrap(),
        )
        .await
        .unwrap();
    let observed = crate::file_backed::budget::read_budget(&shell, &owner).await;
    assert_eq!(observed, Some(expected.clone()));

    let mut app = FileBackedApp::new(owner.clone());
    app.budget.apply(&owner, observed);
    assert_eq!(
        app.budget.compact_label(true).as_deref(),
        Some("10m/30m 25/256t 8/64m")
    );

    // Collect live Status Panel diagnostics + active CoW branch count.
    assert_eq!(status_panel::read_cow_branch_count(&shell).await, 1);
    let diag = status_panel::collect_diagnostics(&shell).await;
    assert!(diag.loaded);
    assert!(
        diag.fileserver_packages
            .iter()
            .any(|p| p.contains("github-fileserver -> /mnt/github (active)")),
        "{diag:?}"
    );
    assert!(
        diag.cow_branches.iter().any(|b| b == "b-active (active)"),
        "{diag:?}"
    );
    assert_eq!(
        diag.summary_line.as_deref(),
        Some("rev 7 · generated · recall on")
    );
}
