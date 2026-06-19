use alan_shell_core::{
    ContentInstance, ContentKind, ContentLifecycleState, PaneSlot, PaneTreeNode, ShellActionEffect,
    ShellActionExecutionResult, ShellActionId, ShellActionModifier, ShellActionRegistry,
    ShellActionRegistryError, ShellActionShortcut, ShellActionShortcutContext, ShellActionTarget,
    ShellAttentionState, ShellContentPayload, ShellLaunchTarget, ShellQuickTerminalPresentation,
    ShellQuickTerminalState, ShellWorkspaceCommand, Space, SplitDirection, SplitPlacement, Tab,
    TabKind, TerminalRuntimeMetadata, WorkspaceState,
};

#[test]
fn standard_action_ids_shortcuts_and_keyboard_lookup_are_stable() {
    let registry = ShellActionRegistry::standard();
    let ids = registry
        .actions()
        .iter()
        .map(|action| action.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids.iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        ids.len()
    );
    assert_eq!(
        registry.action(ShellActionId::TabPin).unwrap().id.as_str(),
        "shell.tab.pin"
    );
    assert_eq!(
        registry
            .action(ShellActionId::PaneSplitRight)
            .unwrap()
            .title,
        "Split Right"
    );
    assert_eq!(
        registry.default_shortcut(
            ShellActionId::NewTerminalTab,
            &ShellActionTarget::CurrentSelection
        ),
        Some(ShellActionShortcut::new(
            "t",
            vec![ShellActionModifier::Command],
            ShellActionShortcutContext::Shell,
        ))
    );
    assert_eq!(
        registry.default_shortcut(
            ShellActionId::SpaceSelectByIndex,
            &ShellActionTarget::SpaceIndex { index: 1 }
        ),
        Some(ShellActionShortcut::new(
            "2",
            vec![ShellActionModifier::Command, ShellActionModifier::Option],
            ShellActionShortcutContext::Shell,
        ))
    );
    assert_eq!(
        registry
            .keyboard_action(&ShellActionShortcut::new(
                "return",
                vec![ShellActionModifier::Command, ShellActionModifier::Shift],
                ShellActionShortcutContext::Shell,
            ))
            .map(|action| action.id),
        Some(ShellActionId::PaneZoomToggle)
    );
}

#[test]
fn shortcut_conflicts_include_dynamic_space_shortcuts() {
    let conflicting = alan_shell_core::ShellActionDescriptor::always_available(
        ShellActionId::NewTerminalTab,
        "Conflicting Dynamic Shortcut",
        alan_shell_core::ShellActionTargetKind::CurrentSelection,
        Some(ShellActionShortcut::new(
            "1",
            vec![ShellActionModifier::Command, ShellActionModifier::Option],
            ShellActionShortcutContext::Shell,
        )),
        ShellActionEffect::WorkspaceCommand {
            command: ShellWorkspaceCommand::NewTerminalTab,
        },
    );
    let dynamic_space = alan_shell_core::ShellActionDescriptor::always_available(
        ShellActionId::SpaceSelectByIndex,
        "Select Space",
        alan_shell_core::ShellActionTargetKind::Space,
        None,
        ShellActionEffect::SelectSpaceAt { index: 0 },
    );

    let error = ShellActionRegistry::new(vec![conflicting, dynamic_space]).unwrap_err();

    assert!(matches!(
        error,
        ShellActionRegistryError::DuplicateShortcut { .. }
    ));
}

#[test]
fn context_tab_target_routes_effect_without_changing_selection() {
    let registry = ShellActionRegistry::standard();
    let state = workspace_with_two_tabs();

    let resolved = registry.resolve(
        ShellActionId::TabClose,
        &ShellActionTarget::ContextTab {
            tab_id: "tab_other".to_string(),
        },
        &state,
    );
    let result = registry.execute(
        ShellActionId::TabClose,
        &ShellActionTarget::ContextTab {
            tab_id: "tab_other".to_string(),
        },
        &state,
    );

    assert_eq!(state.focused_tab_id.as_deref(), Some("tab_main"));
    assert!(matches!(
        resolved.resolved_target,
        alan_shell_core::ShellResolvedActionTarget::Tab { ref tab_id }
            if tab_id == "tab_other"
    ));
    assert_eq!(
        result,
        ShellActionExecutionResult::Executed {
            effect: ShellActionEffect::CloseTab {
                tab_id: Some("tab_other".to_string()),
            },
        }
    );
}

#[test]
fn move_tab_to_space_requires_explicit_target_and_move_shortcut_stays_unavailable_at_edge() {
    let registry = ShellActionRegistry::standard();
    let state = single_tab_workspace();

    assert_eq!(
        registry.execute(
            ShellActionId::TabMoveToSpace,
            &ShellActionTarget::CurrentSelection,
            &state,
        ),
        ShellActionExecutionResult::Unavailable {
            reason: "Move target is required".to_string(),
        }
    );
    assert_eq!(
        registry.execute(
            ShellActionId::TabMoveLeft,
            &ShellActionTarget::CurrentSelection,
            &state,
        ),
        ShellActionExecutionResult::Unavailable {
            reason: "No adjacent tab in section".to_string(),
        }
    );
}

#[test]
fn quick_terminal_promote_requires_destination_and_routes_space_id() {
    let registry = ShellActionRegistry::standard();
    let mut state = workspace_with_two_spaces();
    state.quick_terminal = Some(ShellQuickTerminalState {
        pane_id: "quick_terminal_pane".to_string(),
        presentation: ShellQuickTerminalPresentation::Hidden,
        last_working_directory: Some("/tmp".to_string()),
        content_id: "content_quick_terminal_pane".to_string(),
        terminal_payload: Some(alan_shell_core::ShellTerminalContentPayload {
            launch_target: ShellLaunchTarget::Shell,
            cwd: Some("/tmp".to_string()),
            title: Some("Shell".to_string()),
            transcript_snapshot: None,
            terminal_profile_id: None,
        }),
        terminal_metadata: Some(TerminalRuntimeMetadata {
            title: Some("Shell".to_string()),
            cwd: Some("/tmp".to_string()),
            active_task_state: Default::default(),
            activity: None,
        }),
        attention: ShellAttentionState::Idle,
    });

    assert_eq!(
        registry.execute(
            ShellActionId::QuickTerminalPromote,
            &ShellActionTarget::CurrentSelection,
            &state,
        ),
        ShellActionExecutionResult::Unavailable {
            reason: "Quick terminal destination is required".to_string(),
        }
    );
    assert_eq!(
        registry.execute(
            ShellActionId::QuickTerminalPromote,
            &ShellActionTarget::ContextSpace {
                space_id: "space_2".to_string(),
            },
            &state,
        ),
        ShellActionExecutionResult::Executed {
            effect: ShellActionEffect::PromoteQuickTerminal {
                space_id: Some("space_2".to_string()),
            },
        }
    );
}

#[test]
fn pane_zoom_and_movement_follow_split_tree_availability() {
    let registry = ShellActionRegistry::standard();
    let single = single_tab_workspace();
    let split = split_workspace();

    assert_eq!(
        registry.execute(
            ShellActionId::PaneZoomToggle,
            &ShellActionTarget::CurrentSelection,
            &single,
        ),
        ShellActionExecutionResult::Unavailable {
            reason: "Pane zoom requires a split tab".to_string(),
        }
    );
    assert_eq!(
        registry.execute(
            ShellActionId::PaneMoveRight,
            &ShellActionTarget::CurrentSelection,
            &split,
        ),
        ShellActionExecutionResult::Unavailable {
            reason: "No adjacent pane in that direction".to_string(),
        }
    );
    assert_eq!(
        registry.execute(
            ShellActionId::PaneMoveLeft,
            &ShellActionTarget::CurrentSelection,
            &split,
        ),
        ShellActionExecutionResult::Executed {
            effect: ShellActionEffect::MovePaneInTab {
                pane_id: Some("pane_2".to_string()),
                placement: SplitPlacement::Left,
            },
        }
    );
}

fn single_tab_workspace() -> WorkspaceState {
    workspace(vec![tab("tab_main", "pane_1", false)], vec!["pane_1"], None)
}

fn workspace_with_two_tabs() -> WorkspaceState {
    workspace(
        vec![
            tab("tab_main", "pane_1", false),
            tab("tab_other", "pane_2", false),
        ],
        vec!["pane_1", "pane_2"],
        None,
    )
}

fn workspace_with_two_spaces() -> WorkspaceState {
    let mut state = single_tab_workspace();
    state.spaces.push(Space {
        space_id: "space_2".to_string(),
        title: "Second".to_string(),
        attention: ShellAttentionState::Idle,
        tabs: Vec::new(),
        selected_tab_id: None,
        terminal_profile_id: None,
        presentation_icon: None,
    });
    state
}

fn split_workspace() -> WorkspaceState {
    let mut state = workspace(
        vec![Tab {
            tab_id: "tab_main".to_string(),
            kind: TabKind::Terminal,
            title: Some("Shell".to_string()),
            pane_tree: PaneTreeNode::split(
                "node_split",
                SplitDirection::Vertical,
                vec![
                    PaneTreeNode::pane("node_pane_1", "pane_1"),
                    PaneTreeNode::pane("node_pane_2", "pane_2"),
                ],
            ),
            zoomed_pane_id: None,
            is_pinned: false,
            is_title_user_locked: false,
        }],
        vec!["pane_1", "pane_2"],
        Some("pane_2"),
    );
    state.focused_pane_id = Some("pane_2".to_string());
    state
}

fn workspace(tabs: Vec<Tab>, pane_ids: Vec<&str>, focused_pane_id: Option<&str>) -> WorkspaceState {
    let focused_pane_id = focused_pane_id.unwrap_or("pane_1");
    WorkspaceState {
        contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        focused_space_id: Some("space_main".to_string()),
        focused_tab_id: Some("tab_main".to_string()),
        focused_pane_id: Some(focused_pane_id.to_string()),
        spaces: vec![Space {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            attention: ShellAttentionState::Active,
            tabs,
            selected_tab_id: Some("tab_main".to_string()),
            terminal_profile_id: None,
            presentation_icon: None,
        }],
        pane_slots: pane_ids
            .iter()
            .map(|pane_id| PaneSlot {
                pane_slot_id: (*pane_id).to_string(),
                tab_id: if *pane_id == "pane_2" {
                    "tab_other".to_string()
                } else {
                    "tab_main".to_string()
                },
                space_id: "space_main".to_string(),
                content_id: format!("content_{pane_id}"),
                attention: if *pane_id == focused_pane_id {
                    ShellAttentionState::Active
                } else {
                    ShellAttentionState::Idle
                },
            })
            .collect(),
        contents: pane_ids
            .iter()
            .map(|pane_id| ContentInstance {
                content_id: format!("content_{pane_id}"),
                kind: ContentKind::Terminal,
                title: "Shell".to_string(),
                icon_name: None,
                capabilities: ContentKind::Terminal.default_capabilities(),
                payload: ShellContentPayload::terminal(
                    ShellLaunchTarget::Shell,
                    None,
                    Some("Shell"),
                ),
                terminal_metadata: None,
                lifecycle: ContentLifecycleState::Active,
            })
            .collect(),
        quick_terminal: None,
    }
}

fn tab(tab_id: &str, pane_id: &str, is_pinned: bool) -> Tab {
    Tab {
        tab_id: tab_id.to_string(),
        kind: TabKind::Terminal,
        title: Some("Shell".to_string()),
        pane_tree: PaneTreeNode::pane(format!("node_{pane_id}"), pane_id),
        zoomed_pane_id: None,
        is_pinned,
        is_title_user_locked: false,
    }
}
