
#[test]
fn selecting_tab_and_space_repairs_focus_to_target_pane() {
    let state = base_state()
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Second".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::CreateTerminalSpace {
            title: Some("Other".to_string()),
            tab_title: Some("Other Shell".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            presentation_icon: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;

    let selected_tab = state
        .reduce(ReducerOperation::SelectTab {
            tab_id: "tab_main".to_string(),
        })
        .expect("select tab succeeds");
    assert_eq!(
        selected_tab.state.focused_space_id.as_deref(),
        Some("space_main")
    );
    assert_eq!(
        selected_tab.state.focused_tab_id.as_deref(),
        Some("tab_main")
    );
    assert_eq!(
        selected_tab.state.focused_pane_id.as_deref(),
        Some("pane_1")
    );

    let selected_space = selected_tab
        .state
        .reduce(ReducerOperation::SelectSpace {
            space_id: "space_2".to_string(),
        })
        .expect("select space succeeds");
    assert_eq!(
        selected_space.state.focused_space_id.as_deref(),
        Some("space_2")
    );
    assert_eq!(
        selected_space.state.focused_tab_id.as_deref(),
        Some("tab_3")
    );
    assert_eq!(
        selected_space.state.focused_pane_id.as_deref(),
        Some("pane_3")
    );

    let missing_tab = selected_space
        .state
        .reduce(ReducerOperation::SelectTab {
            tab_id: "missing".to_string(),
        })
        .unwrap_err();
    assert_eq!(missing_tab.code, ReducerErrorCode::TabNotFound);
}

#[test]
fn pane_move_reducers_preserve_content_identity_and_repair_trees() {
    let state = base_state()
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: Some("Second".to_string()),
            working_directory: Some("/tmp/second".to_string()),
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;

    let moved_within_tab = state
        .reduce(ReducerOperation::MovePaneWithinTab {
            pane_slot_id: "pane_2".to_string(),
            placement: SplitPlacement::Left,
        })
        .expect("move pane within tab succeeds");
    assert_eq!(
        moved_within_tab.state.spaces[0].tabs[0]
            .pane_tree
            .pane_ids(),
        vec!["pane_2".to_string(), "pane_1".to_string()]
    );
    assert_eq!(
        moved_within_tab
            .state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == "pane_2")
            .unwrap()
            .content_id,
        "content_pane_2"
    );
    assert_eq!(
        moved_within_tab.state.focused_pane_id.as_deref(),
        Some("pane_2")
    );

    let lifted = moved_within_tab
        .state
        .reduce(ReducerOperation::MovePaneToNewTab {
            pane_slot_id: "pane_2".to_string(),
            title: Some("Lifted".to_string()),
        })
        .expect("move pane to new tab succeeds");
    assert_eq!(lifted.state.spaces[0].tabs[1].tab_id, "tab_2");
    assert_eq!(
        lifted.state.spaces[0].tabs[1].pane_tree.pane_ids(),
        vec!["pane_2".to_string()]
    );
    assert_eq!(
        lifted
            .state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == "pane_2")
            .unwrap()
            .tab_id,
        "tab_2"
    );

    let target_tab_state = lifted
        .state
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Target".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;
    let cross_tab = target_tab_state
        .reduce(ReducerOperation::MovePaneToTab {
            pane_slot_id: "pane_2".to_string(),
            target_tab_id: "tab_3".to_string(),
            direction: SplitDirection::Vertical,
        })
        .expect("move pane to another tab succeeds");
    assert!(cross_tab.state.tab("tab_2").is_none());
    let target_tab = cross_tab.state.tab("tab_3").unwrap();
    assert_eq!(
        target_tab.pane_tree.pane_ids(),
        vec!["pane_3".to_string(), "pane_2".to_string()]
    );
    assert_eq!(
        cross_tab
            .state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == "pane_2")
            .unwrap()
            .tab_id,
        "tab_3"
    );
}

#[test]
fn lifting_pane_without_title_inherits_moved_pane_content_title() {
    let state = base_state()
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: Some("My Server".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;
    assert_eq!(content_title(&state, "content_pane_2"), Some("My Server"));

    let lifted = state
        .reduce(ReducerOperation::MovePaneToNewTab {
            pane_slot_id: "pane_2".to_string(),
            title: None,
        })
        .expect("lift without title succeeds");

    let lifted_tab = lifted.state.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.pane_tree.pane_ids() == vec!["pane_2".to_string()])
        .expect("lifted tab exists");
    assert_eq!(
        lifted_tab.title.as_deref(),
        Some("My Server"),
        "lift without explicit title must inherit the moved pane content title"
    );
}

#[test]
fn pane_zoom_reducers_scope_zoom_to_tab_and_prune_invalid_zoom_state() {
    let state = base_state()
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: Some("Second".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;

    let zoomed = state
        .reduce(ReducerOperation::ZoomPane {
            pane_slot_id: "pane_2".to_string(),
        })
        .expect("zoom succeeds");
    assert_eq!(
        zoomed
            .state
            .tab("tab_main")
            .unwrap()
            .zoomed_pane_id
            .as_deref(),
        Some("pane_2")
    );
    assert_eq!(zoomed.state.focused_pane_id.as_deref(), Some("pane_2"));
    assert_eq!(zoomed.changed_ids.updated_tab_ids, vec!["tab_main"]);

    let unzoomed = zoomed
        .state
        .reduce(ReducerOperation::UnzoomTab {
            tab_id: Some("tab_main".to_string()),
        })
        .expect("unzoom succeeds");
    assert_eq!(
        unzoomed
            .state
            .tab("tab_main")
            .unwrap()
            .zoomed_pane_id
            .as_deref(),
        None
    );
    assert_eq!(unzoomed.state.focused_pane_id.as_deref(), Some("pane_2"));

    let pruned = zoomed
        .state
        .reduce(ReducerOperation::ClosePane {
            pane_slot_id: "pane_2".to_string(),
        })
        .expect("closing zoomed pane succeeds");
    assert_eq!(
        pruned
            .state
            .tab("tab_main")
            .unwrap()
            .zoomed_pane_id
            .as_deref(),
        None
    );
}

#[test]
fn terminal_metadata_updates_content_runtime_fields_and_respects_user_title_lock() {
    let updated = base_state()
        .reduce(ReducerOperation::UpdateTerminalMetadata {
            pane_slot_id: "pane_1".to_string(),
            title: Some("cargo test".to_string()),
            cwd: Some("/repo/app".to_string()),
            active_task_state: Some(ShellTabActiveTaskState::ForegroundCommand),
            activity: None,
        })
        .expect("metadata update succeeds");
    let content = updated
        .state
        .contents
        .iter()
        .find(|content| content.content_id == "content_pane_1")
        .unwrap();
    let metadata = content.terminal_metadata.as_ref().unwrap();
    assert_eq!(metadata.title.as_deref(), Some("cargo test"));
    assert_eq!(metadata.cwd.as_deref(), Some("/repo/app"));
    assert_eq!(
        metadata.active_task_state,
        ShellTabActiveTaskState::ForegroundCommand
    );
    assert_eq!(
        updated.state.tab("tab_main").unwrap().title.as_deref(),
        Some("cargo test")
    );
    assert_eq!(
        updated.changed_ids.updated_content_ids,
        vec!["content_pane_1"]
    );

    let locked = base_state()
        .reduce(ReducerOperation::RenameTab {
            tab_id: "tab_main".to_string(),
            title: "Locked".to_string(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::UpdateTerminalMetadata {
            pane_slot_id: "pane_1".to_string(),
            title: Some("ignored runtime title".to_string()),
            cwd: None,
            active_task_state: None,
            activity: None,
        })
        .expect("metadata update on locked tab succeeds");
    assert_eq!(
        locked.state.tab("tab_main").unwrap().title.as_deref(),
        Some("Locked")
    );
}

#[test]
fn clear_inactive_temporary_tabs_uses_terminal_active_task_metadata() {
    let state = base_state()
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Idle".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Active".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::UpdateTerminalMetadata {
            pane_slot_id: "pane_3".to_string(),
            title: None,
            cwd: None,
            active_task_state: Some(ShellTabActiveTaskState::ForegroundCommand),
            activity: None,
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::FocusPane {
            pane_slot_id: "pane_1".to_string(),
        })
        .unwrap()
        .state;

    let cleared = state
        .reduce(ReducerOperation::ClearInactiveTemporaryTabs {
            space_id: "space_main".to_string(),
            protected_tab_ids: Vec::new(),
        })
        .expect("clear inactive tabs succeeds");

    assert_eq!(
        cleared.state.spaces[0]
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tab_main", "tab_3"]
    );
    assert_eq!(cleared.changed_ids.removed_tab_ids, vec!["tab_2"]);
}

#[test]
fn agent_activity_update_projects_activity_and_working_directory_to_terminal_content() {
    let activity = sample_activity();
    let updated = base_state()
        .reduce(ReducerOperation::ApplyAgentActivity {
            pane_slot_id: "pane_1".to_string(),
            activity: activity.clone(),
            working_directory: Some("/repo/app".to_string()),
        })
        .expect("agent activity update succeeds");

    let metadata = updated.state.contents[0]
        .terminal_metadata
        .as_ref()
        .unwrap();
    assert_eq!(metadata.cwd.as_deref(), Some("/repo/app"));
    assert_eq!(metadata.activity.as_ref(), Some(&activity));
    assert_eq!(
        updated.changed_ids.updated_content_ids,
        vec!["content_pane_1"]
    );
}

#[test]
fn reducer_reports_stable_errors_for_invalid_moves() {
    let single_pane_error = base_state()
        .reduce(ReducerOperation::MovePaneToNewTab {
            pane_slot_id: "pane_1".to_string(),
            title: None,
        })
        .unwrap_err();
    assert_eq!(single_pane_error.code, ReducerErrorCode::LastPane);

    let invalid_spatial_move = base_state()
        .reduce(ReducerOperation::MovePaneWithinTab {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Left,
        })
        .unwrap_err();
    assert_eq!(
        invalid_spatial_move.code,
        ReducerErrorCode::InvalidMoveTarget
    );

    let invalid_tab_move = base_state()
        .reduce(ReducerOperation::MoveTab {
            tab_id: "tab_main".to_string(),
            section_offset: 1,
        })
        .unwrap_err();
    assert_eq!(
        invalid_tab_move.code,
        ReducerErrorCode::InvalidTabOrganizationTarget
    );

    let invalid_tab_organization = base_state()
        .reduce(ReducerOperation::OrganizeTab {
            tab_id: "tab_main".to_string(),
            target_space_id: Some("space_main".to_string()),
            section: TabOrganizationSection::Unpinned,
            index: Some(2),
        })
        .unwrap_err();
    assert_eq!(
        invalid_tab_organization.code,
        ReducerErrorCode::InvalidTabOrganizationTarget
    );

    let invalid_zoom = base_state()
        .reduce(ReducerOperation::ZoomPane {
            pane_slot_id: "pane_1".to_string(),
        })
        .unwrap_err();
    assert_eq!(invalid_zoom.code, ReducerErrorCode::InvalidMoveTarget);

    let invalid_unzoom = base_state()
        .reduce(ReducerOperation::UnzoomTab {
            tab_id: Some("tab_main".to_string()),
        })
        .unwrap_err();
    assert_eq!(invalid_unzoom.code, ReducerErrorCode::InvalidMoveTarget);
}

#[test]
fn split_reducer_rejects_unsupported_non_terminal_content() {
    let mut state = base_state();
    state.contents[0] = ContentInstance {
        content_id: "content_pane_1".to_string(),
        kind: ContentKind::Markdown,
        title: "Notes".to_string(),
        icon_name: None,
        capabilities: ContentKind::Markdown.default_capabilities(),
        payload: ShellContentPayload::default(),
        terminal_metadata: None,
        lifecycle: ContentLifecycleState::Active,
        renderer_state: Default::default(),
    };

    let error = state
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: None,
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap_err();

    assert_eq!(error.code, ReducerErrorCode::UnsupportedContent);
}

#[test]
fn agent_renderer_progress_updates_only_the_mounted_content_payload() {
    let mut state = base_state();
    state.contents[0] = ContentInstance {
        content_id: "content_pane_1".to_string(),
        kind: ContentKind::Agent,
        title: "Agent 7".to_string(),
        icon_name: None,
        capabilities: ContentKind::Agent.default_capabilities(),
        payload: ShellContentPayload {
            agent: Some(AgentAttachment {
                process: AgentProcessReference {
                    boot_id: "boot-a".to_string(),
                    pid: 7,
                },
                offsets: AgentStreamOffsets::default(),
                presentation: AgentContentPresentation::default(),
            }),
            ..ShellContentPayload::default()
        },
        terminal_metadata: None,
        lifecycle: ContentLifecycleState::Active,
        renderer_state: Default::default(),
    };
    let offsets = AgentStreamOffsets {
        output: 42,
        requests: 3,
        actions: 5,
        ui: 8,
    };
    let presentation = AgentContentPresentation {
        follows_output: true,
    };

    let result = state
        .reduce(ReducerOperation::UpdateAgentRendererState {
            pane_slot_id: "pane_1".to_string(),
            offsets: offsets.clone(),
            presentation: presentation.clone(),
        })
        .expect("Agent renderer progress update succeeds");
    let attachment = result.state.contents[0]
        .payload
        .agent
        .as_ref()
        .expect("Agent attachment remains present");

    assert_eq!(attachment.process.boot_id, "boot-a");
    assert_eq!(attachment.process.pid, 7);
    assert_eq!(attachment.offsets, offsets);
    assert_eq!(attachment.presentation, presentation);
    assert_eq!(result.changed_ids.updated_content_ids, ["content_pane_1"]);
}

fn sample_activity() -> TerminalActivitySnapshot {
    TerminalActivitySnapshot {
        source: TerminalActivitySource {
            kind: TerminalActivitySourceKind::Codex,
            label: Some("Codex".to_string()),
        },
        status: TerminalActivityStatus::Running,
        priority: TerminalActivityPriority::Active,
        agent: Some(TerminalActivityAgentMetadata {
            kind: TerminalActivitySourceKind::Codex,
            safe_session_label: None,
            project_label: Some("alan".to_string()),
            working_directory: Some("/repo/app".to_string()),
        }),
        display: TerminalActivityDisplay {
            source_label: Some("Codex".to_string()),
            state_label: "Running".to_string(),
            detail_label: None,
            pane_hint: None,
        },
        freshness: TerminalActivityFreshness {
            updated_at: "2026-05-17T09:00:00Z".to_string(),
            stale_at: Some("2026-05-17T09:01:30Z".to_string()),
            expires_at: None,
        },
    }
}

fn content_title<'a>(state: &'a WorkspaceState, content_id: &str) -> Option<&'a str> {
    state
        .contents
        .iter()
        .find(|content| content.content_id == content_id)
        .map(|content| content.title.as_str())
}

fn base_state() -> WorkspaceState {
    WorkspaceState {
        contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        focused_space_id: Some("space_main".to_string()),
        focused_tab_id: Some("tab_main".to_string()),
        focused_pane_id: Some("pane_1".to_string()),
        spaces: vec![Space {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            attention: ShellAttentionState::Active,
            tabs: vec![Tab {
                tab_id: "tab_main".to_string(),
                kind: TabKind::Terminal,
                title: Some("Shell".to_string()),
                pane_tree: PaneTreeNode::pane("node_pane_1", "pane_1"),
                zoomed_pane_id: None,
                is_pinned: false,
                is_title_user_locked: false,
            }],
            selected_tab_id: Some("tab_main".to_string()),
            terminal_profile_id: Some("profile-main".to_string()),
            presentation_icon: None,
        }],
        pane_slots: vec![PaneSlot {
            pane_slot_id: "pane_1".to_string(),
            tab_id: "tab_main".to_string(),
            space_id: "space_main".to_string(),
            content_id: "content_pane_1".to_string(),
            attention: ShellAttentionState::Active,
        }],
        contents: vec![ContentInstance {
            content_id: "content_pane_1".to_string(),
            kind: ContentKind::Terminal,
            title: "Shell".to_string(),
            icon_name: None,
            capabilities: ContentKind::Terminal.default_capabilities(),
            payload: ShellContentPayload::terminal(ShellLaunchTarget::Shell, None, Some("Shell")),
            terminal_metadata: None,
            lifecycle: ContentLifecycleState::Active,
            renderer_state: Default::default(),
        }],
    }
}
