use super::*;
use alan_agent_protocol::{
    DiffHunk, DiffLine, PlanItem, PlanItemStatus, ToolResultPresentation, UiPlanSnapshot, YieldKind,
};
use alan_ap::InProcessTransport;
use alan_ap::reference::MemFs;
use alan_kernel::{Access, MountFs, Namespace};
use ratatui::style::Color;

#[test]
fn semantic_theme_drives_layout_dots_and_dumb_degrades_cleanly() {
    for (theme, expected_dot, expected_fg) in [
        (
            crate::theme::Theme::midnight(),
            "●",
            Some(Color::Rgb(34, 211, 166)),
        ),
        (
            crate::theme::Theme::paper(),
            "●",
            Some(Color::Rgb(0, 138, 102)),
        ),
        (crate::theme::Theme::dumb(), "*", None),
    ] {
        let mut app = FileBackedApp::new("/agent/1".into());
        app.theme = theme;
        app.project = Some(ProjectMountReceipt {
            grant_id: "g1".into(),
            namespace_path: "/mnt/project".into(),
            label: "alan".into(),
            access: ProjectAccess::ReadOnly,
        });
        let ctx = app.context_line(80);
        let text = ctx.to_string();
        assert!(text.starts_with(expected_dot), "{text}");
        assert!(text.contains("alan"), "{text}");
        assert_eq!(ctx.spans[0].style.fg, expected_fg);
        if theme.is_dumb {
            assert!(
                ctx.spans
                    .iter()
                    .all(|s| s.style.fg.is_none() && s.style.add_modifier.is_empty())
            );
        }
    }

    // Completed and failed action summaries include semantic dots and left-gutter diffs.
    let diff = ToolResultPresentation::Diff {
        path: "src/lib.rs".into(),
        hunks: vec![DiffHunk {
            header: None,
            lines: vec![
                DiffLine::Removed { text: "old".into() },
                DiffLine::Added { text: "new".into() },
            ],
        }],
    };
    let ok_cell = HistoryCell::Tool {
        action: None,
        title: "edit src/lib.rs".into(),
        status: ToolStatus::Complete,
        preview: None,
        presentation: Some(diff),
    };
    let fail_cell = HistoryCell::Tool {
        action: None,
        title: "bash".into(),
        status: ToolStatus::Failed,
        preview: Some("exit 1".into()),
        presentation: None,
    };
    let midnight = crate::theme::Theme::midnight();
    let ok_rows = crate::history::action_summary_with_theme(&ok_cell, 80, midnight);
    assert!(
        ok_rows[0].to_string().starts_with("● edit src/lib.rs"),
        "{ok_rows:?}"
    );
    assert!(
        ok_rows.iter().any(|r| r.to_string().contains("▎ +new")),
        "{ok_rows:?}"
    );
    let fail_rows = crate::history::action_summary_with_theme(&fail_cell, 80, midnight);
    assert!(
        fail_rows[0].to_string().starts_with("● bash"),
        "{fail_rows:?}"
    );

    let dumb_fail =
        crate::history::action_summary_with_theme(&fail_cell, 80, crate::theme::Theme::dumb());
    assert!(
        dumb_fail[0].to_string().starts_with("! bash"),
        "{dumb_fail:?}"
    );
}

#[test]
fn open_left_gutter_typography_avoids_four_sided_boxes() {
    let theme = crate::theme::Theme::midnight();
    let opts = RenderOpts::new(80, true).with_theme(theme);

    let plan_rows = HistoryCell::Plan {
        owner: "/agent/1".into(),
        revision: 2,
        snapshot: UiPlanSnapshot::new(
            None,
            vec![PlanItem {
                id: "step-1".into(),
                content: "wire theme".into(),
                status: PlanItemStatus::InProgress,
            }],
        ),
    }
    .render_styled_lines(opts);
    assert!(plan_rows[0].to_string().starts_with("▎ "), "{plan_rows:?}");

    let thinking_rows = HistoryCell::Thinking {
        text: "reasoning step".into(),
        duration_secs: 3,
    }
    .render_styled_lines(opts);
    assert!(
        thinking_rows
            .iter()
            .all(|r| r.to_string().starts_with("▎ "))
    );

    let yield_rows = HistoryCell::PendingYield(PendingYieldCell {
        request_id: "req-1".into(),
        kind: YieldKind::Confirmation,
        title: "Apply patch".into(),
        prompt: Some("Write src/main.rs".into()),
        options: vec!["approve".into(), "reject".into()],
        default_option: Some("approve".into()),
        questions: vec![],
        capability: Some("write".into()),
        reason: Some("Confirm write".into()),
        presentation: None,
    })
    .render_styled_lines(opts);
    let joined_yield = yield_rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(yield_rows.iter().all(|r| r.to_string().starts_with("▎ ")));
    for box_char in ['┌', '┐', '└', '┘', '╭', '╮', '╰', '╯'] {
        assert!(!joined_yield.contains(box_char), "{joined_yield}");
    }
}

#[test]
fn completion_and_help_render_in_aligned_two_columns_without_cyan_block_fill() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    let (lines, _) = layout::live_region_lines_at(&app, 80, 0);
    let completion_rows: Vec<&ratatui::text::Line<'static>> = lines
        .iter()
        .filter(|l| {
            let s = l.to_string();
            s.starts_with("▶ /") || s.starts_with("  /")
        })
        .collect();
    assert!(completion_rows.len() >= 2, "{lines:?}");
    for row in &completion_rows {
        assert!(
            !row.spans.iter().any(|s| s.style.bg == Some(Color::Cyan)),
            "completion must not use harsh Cyan bg: {row:?}"
        );
    }
    // Detail column starts at a uniform column across rows.
    let detail_cols: Vec<usize> = completion_rows
        .iter()
        .filter(|row| row.spans.len() >= 4)
        .map(|row| row.spans[..3].iter().map(|s| s.width()).sum::<usize>())
        .collect();
    assert!(detail_cols.len() >= 2);
    assert!(
        detail_cols.windows(2).all(|w| w[0] == w[1]),
        "{detail_cols:?}"
    );

    app.completion = None;
    app.composer.set_text("");
    let _ = app.handle_command("/help");
    let (help_lines, _) = layout::live_region_lines_at(&app, 80, 0);
    let help_text = help_lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(help_text.contains("Commands:"), "{help_text}");
    assert!(help_text.contains("/tape"), "{help_text}");
    assert!(help_text.contains("/man"), "{help_text}");
    assert!(help_lines.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.style.fg == Some(app.theme.accent) && s.content.contains("/tape"))
    }));

    // Model completion aligns model names cleanly without Cyan block highlight.
    let mut model_app = super::model_tests::ready();
    model_app.handle_command("/model");
    assert!(model_app.completion.is_some());
    let (model_lines, _) = layout::live_region_lines_at(&model_app, 80, 0);
    assert!(
        model_lines
            .iter()
            .filter(|l| l.to_string().contains("effort"))
            .all(|l| !l.spans.iter().any(|s| s.style.bg == Some(Color::Cyan)))
    );
}

#[tokio::test]
async fn slash_tape_man_and_welcome_header_contracts() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.effective_model = Some("gpt-5.4".into());
    app.sandbox_backend = Some("seatbelt".into());

    // Empty initial transcript inserts 4-line ASCII rabbit welcome banner at width >= 64.
    app.maybe_insert_welcome_header();
    assert!(matches!(
        app.transcript.as_slice(),
        [HistoryCell::Welcome(_)]
    ));
    let wide = app.styled_history_lines(80);
    let wide_str = wide
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(wide_str.contains("(\\_/)"), "{wide_str}");
    assert!(wide_str.contains("ALAN(1) · alan9"), "{wide_str}");
    assert!(wide_str.contains("gpt-5.4"), "{wide_str}");

    let narrow = app.styled_history_lines(48);
    let narrow_str = narrow
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(narrow_str.contains("■ ALAN · "), "{narrow_str}");
    assert!(!narrow_str.contains("(\\_/)"), "{narrow_str}");

    // Submit user/tool/assistant turns then inspect /tape and `t` modal toggle.
    app.transcript
        .push(HistoryCell::User("inspect /proc".into()));
    app.transcript.push(HistoryCell::Tool {
        action: None,
        title: "read_file".into(),
        status: ToolStatus::Complete,
        preview: None,
        presentation: None,
    });
    app.transcript
        .push(HistoryCell::Assistant("Checked /proc/1.".into()));

    assert!(matches!(
        app.handle_command("/tape"),
        Some(FileBackedAction::ReadTape)
    ));
    assert!(app.modal.active);
    let tape_str = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        tape_str.contains("0001") && tape_str.contains("[in]"),
        "{tape_str}"
    );
    assert!(tape_str.contains("[tool]"), "{tape_str}");
    assert!(tape_str.contains("[out]"), "{tape_str}");
    // Namespace path `/proc` gets Klein Blue accent.
    assert!(app.modal.rows.iter().any(|l| {
        l.spans
            .iter()
            .any(|s| s.content == "/proc" && s.style.fg == Some(app.theme.accent))
    }));

    // Press `t` switches Tape -> Actions, `t` again back to Tape.
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(app.modal.mode, super::app::action_modal::ModalMode::Actions);
    app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
    assert_eq!(app.modal.mode, super::app::action_modal::ModalMode::Tape);

    // `/man alan9` opens Man modal and async shell read resolves `/man/7/alan9`.
    let action = app.handle_command("/man alan9");
    assert!(matches!(action, Some(FileBackedAction::ReadMan { .. })));
    assert_eq!(app.modal.mode, super::app::action_modal::ModalMode::Man);

    let mount_fs = std::sync::Arc::new(MountFs::new(Namespace::new()));
    let man_fs = MemFs::with_read_only_tree(vec![(
        "7/alan9".into(),
        b"ALAN9(7)\n\nSTANDARD NAMESPACE\n  /proc   Mounted live kernel manpage".to_vec(),
    )]);
    mount_fs.live_namespace().mount(
        "/man",
        InProcessTransport::new(std::sync::Arc::new(man_fs)),
        Access::ReadOnly,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(mount_fs));
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    man_view::spawn_read(&shell, &tx, "alan9".into(), app.modal.generation);
    let ev = rx.recv().await.expect("man event");
    app.dispatch(ev);
    let man_text = app
        .modal
        .rows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        man_text.contains("Mounted live kernel manpage"),
        "{man_text}"
    );
    assert!(!man_text.to_lowercase().contains("herdr"), "{man_text}");
}

#[tokio::test]
async fn empty_hydrated_agent_preserves_welcome_header_on_first_frame() {
    let procfs = std::sync::Arc::new(alan_kernel::ProcFs::new());
    let agent_root = std::sync::Arc::new(alan_agentfs::AgentRootFs::new_with_process_events(
        procfs.clone(),
        procfs.clone(),
    ));
    let mount_fs = std::sync::Arc::new(MountFs::new(Namespace::new()));
    let live = mount_fs.live_namespace();
    live.mount("/proc", InProcessTransport::new(procfs), Access::ReadWrite);
    live.mount(
        "/agent",
        InProcessTransport::new(agent_root.clone()),
        Access::ReadWrite,
    );
    let pid = alan_shell::Shell::new(InProcessTransport::new(mount_fs.clone()))
        .spawn(r#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#)
        .await
        .unwrap();
    agent_root
        .bind_process(
            pid.clone(),
            std::sync::Arc::new(alan_agentfs::AgentFs::new()),
        )
        .await;
    agent_root.set_root_process(pid.clone()).await;
    live.mount(
        "/mnt/service-manager/units/root-agent",
        InProcessTransport::new(std::sync::Arc::new(MemFs::with_read_only_file(
            "pid",
            format!("{pid}\n").into_bytes(),
        ))),
        Access::ReadOnly,
    );
    let shell = alan_shell::Shell::new(InProcessTransport::new(mount_fs));
    let mut app = FileBackedApp::new("/agent/root".into());
    app.effective_model = Some("gpt-5.4".into());
    app.sandbox_backend = Some("seatbelt".into());
    let tails = super::hydrate_and_open_tails(&shell, "/agent/root", &mut app)
        .await
        .unwrap();
    assert!(
        app.transcript.is_empty(),
        "transcript before welcome: {:?}",
        app.transcript
    );
    app.maybe_insert_welcome_header();
    assert_eq!(app.transcript.len(), 1);
    let committed = app.drain_committed_scrollback(80, 24);
    assert!(committed.is_empty(), "committed: {committed:?}");
    assert_eq!(app.styled_history_lines(80).len(), 4);
    let height = super::inline_viewport_height(&app, 80, 24);
    assert!(height >= 8, "height={height}");
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height)).unwrap();
    terminal
        .draw(|frame| layout::draw_at(frame, &app, 0))
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .chunks(80)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("ALAN(1) · alan9"), "{rendered}");
    tails.close().await;
}

#[test]
fn transcript_turn_rhythm_anchors_prompts_calms_thinking_and_prunes_cleanly() {
    let mut app = FileBackedApp::new("/agent/1".into());
    app.effective_model = Some("auto/deepseek-v4.1-flash".into());
    app.sandbox_backend = Some("seatbelt".into());
    app.maybe_insert_welcome_header();
    app.transcript.push(HistoryCell::User("hello".into()));
    app.transcript.push(HistoryCell::Thinking {
        text: "quick thought".into(),
        duration_secs: 0,
    });
    app.transcript.push(HistoryCell::Thinking {
        text: "   ".into(),
        duration_secs: 0,
    });
    app.transcript
        .push(HistoryCell::Assistant("Hi there!".into()));
    app.transcript.push(HistoryCell::User("who are you".into()));
    app.transcript.push(HistoryCell::Thinking {
        text: "longer thought".into(),
        duration_secs: 3,
    });
    app.transcript
        .push(HistoryCell::Assistant("I am Alan.".into()));
    app.transcript
        .push(HistoryCell::Command("git status".into()));
    app.transcript.push(HistoryCell::Assistant("clean".into()));

    let lines = app.styled_history_lines(80);
    let strings: Vec<String> = lines.iter().map(ToString::to_string).collect();
    assert_eq!(strings[4], "", "blank row between Welcome and first turn");
    assert_eq!(strings[5], ": hello");
    assert_eq!(strings[6], "· thought · <1s");
    assert_eq!(strings[7], "Hi there!");
    assert_eq!(strings[8], "", "blank row between turn 1 and turn 2");
    assert_eq!(strings[9], ": who are you");
    assert_eq!(strings[10], "· thought · 3s");
    assert_eq!(strings[11], "I am Alan.");
    assert_eq!(strings[12], "", "blank row before command turn");
    assert_eq!(strings[13], "! git status");
    assert_eq!(strings[14], "clean");
    assert_eq!(
        strings.len(),
        15,
        "empty thinking block must be suppressed: {strings:?}"
    );

    // Semantic prompt anchor: accented bold prefix + bold default-fg body.
    assert_eq!(
        lines[5].spans[0].style,
        app.theme.user_prompt_prefix_style(false)
    );
    assert_eq!(lines[5].spans[1].style, app.theme.user_prompt_body_style());
    assert_eq!(
        lines[13].spans[0].style,
        app.theme.user_prompt_prefix_style(true)
    );
    assert_eq!(lines[13].spans[1].style, app.theme.user_prompt_body_style());

    // Partial drain through Welcome + turn-1 leading separator preserves `: hello` intact.
    let opts = app.render_opts(80);
    assert_eq!(app.prune_rendered_prefix(opts, 5), 5);
    let after_prune = app.styled_history_lines(80);
    assert_eq!(after_prune, lines[5..]);
    assert_eq!(after_prune[0].to_string(), ": hello");

    // TERM=dumb / NO_COLOR degrades cleanly with identical structural rhythm.
    let mut dumb_app = FileBackedApp::new("/agent/1".into());
    dumb_app.theme = crate::theme::Theme::dumb();
    dumb_app.transcript = vec![
        HistoryCell::User("hello".into()),
        HistoryCell::Thinking {
            text: "reason".into(),
            duration_secs: 0,
        },
        HistoryCell::Assistant("Hi".into()),
    ];
    let dumb_lines = dumb_app.styled_history_lines(80);
    assert_eq!(
        dumb_lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["", ": hello", "· thought · <1s", "Hi"]
    );
    assert!(
        dumb_lines
            .iter()
            .flat_map(|row| &row.spans)
            .all(|s| s.style.fg.is_none()
                && s.style.bg.is_none()
                && s.style.add_modifier.is_empty())
    );
}
