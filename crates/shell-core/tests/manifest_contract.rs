use alan_shell_core::{
    ContentKind, PaneTreeNode, ShellContentPayload, ShellContentRestoreRecord,
    ShellContentTabRestoreSnapshot, ShellContentWorkspaceManifest,
    ShellContentWorkspaceSpaceRecord, ShellContentWorkspaceTabRecord, ShellLaunchTarget,
    ShellPaneRestoreRecord, ShellPaneSlotRestoreRecord, ShellQuickTerminalPresentation,
    ShellQuickTerminalRestoreRecord, ShellTabActiveTaskState, ShellTabRestoreSnapshot,
    ShellWorkspaceManifest, ShellWorkspaceSpaceRecord, ShellWorkspaceTabRecord, TabKind,
};
use chrono::{DateTime, Utc};

const REFERENCE_TIME: &str = "2027-01-15T08:00:00Z";

#[test]
fn default_manifest_materializes_single_terminal_workspace() {
    let manifest =
        ShellContentWorkspaceManifest::default_manifest("window_main", "/repo/app", REFERENCE_TIME);

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.content_contract_version, "0.2");
    assert_eq!(manifest.selected_space_id.as_deref(), Some("space_main"));
    assert_eq!(manifest.selected_tab_id.as_deref(), Some("tab_main"));

    let state = manifest.materialize("/fallback", REFERENCE_TIME);
    assert_eq!(state.window_id, "window_main");
    assert_eq!(state.focused_space_id.as_deref(), Some("space_main"));
    assert_eq!(state.focused_tab_id.as_deref(), Some("tab_main"));
    assert_eq!(state.focused_pane_id.as_deref(), Some("pane_1"));
    assert_eq!(state.spaces[0].tabs[0].tab_id, "tab_main");
    assert_eq!(state.pane_slots[0].content_id, "content_pane_1");
    assert_eq!(state.contents[0].kind, ContentKind::Terminal);
    assert_eq!(
        state.contents[0]
            .terminal_metadata
            .as_ref()
            .and_then(|metadata| metadata.cwd.as_deref()),
        Some("/repo/app")
    );
}

#[test]
fn materialize_preserves_empty_selected_space_and_inactive_space_selection() {
    let mut manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_empty".to_string()),
        selected_tab_id: None,
        spaces: vec![
            ShellContentWorkspaceSpaceRecord {
                space_id: "space_empty".to_string(),
                title: "Empty".to_string(),
                order: 0,
                created_at: reference_time(),
                updated_at: reference_time(),
                selected_tab_id: None,
                tabs: Vec::new(),
                terminal_profile_id: None,
                presentation_icon: None,
            },
            ShellContentWorkspaceSpaceRecord {
                space_id: "space_other".to_string(),
                title: "Other".to_string(),
                order: 1,
                created_at: reference_time(),
                updated_at: reference_time(),
                selected_tab_id: Some("tab_other".to_string()),
                tabs: vec![content_tab("tab_other", "Other", "/other")],
                terminal_profile_id: None,
                presentation_icon: Some("rectangle.stack.fill".to_string()),
            },
        ],
        quick_terminal: None,
    };
    manifest.repair_selection();

    let state = manifest.materialize("/fallback", REFERENCE_TIME);

    assert_eq!(state.spaces.len(), 2);
    assert_eq!(state.focused_space_id.as_deref(), Some("space_empty"));
    assert_eq!(state.focused_tab_id, None);
    assert_eq!(state.focused_pane_id, None);
    assert_eq!(
        state.spaces[1].selected_tab_id.as_deref(),
        Some("tab_other")
    );
    assert_eq!(
        state.spaces[1].presentation_icon.as_deref(),
        Some("rectangle.stack.fill")
    );
}

#[test]
fn pruning_retains_pinned_active_task_and_empty_spaces_while_repairing_selection() {
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_expired".to_string()),
        spaces: vec![
            ShellContentWorkspaceSpaceRecord {
                space_id: "space_main".to_string(),
                title: "Main".to_string(),
                order: 0,
                created_at: reference_time(),
                updated_at: reference_time(),
                selected_tab_id: Some("tab_expired".to_string()),
                tabs: vec![
                    content_tab("tab_expired", "Expired", "/expired"),
                    ShellContentWorkspaceTabRecord {
                        is_pinned: true,
                        ..content_tab("tab_pinned", "Pinned", "/pinned")
                    },
                    ShellContentWorkspaceTabRecord {
                        active_task: ShellTabActiveTaskState::ForegroundCommand,
                        ..content_tab("tab_active", "Active", "/active")
                    },
                ],
                terminal_profile_id: None,
                presentation_icon: None,
            },
            ShellContentWorkspaceSpaceRecord {
                space_id: "space_empty".to_string(),
                title: "Empty".to_string(),
                order: 1,
                created_at: reference_time(),
                updated_at: reference_time(),
                selected_tab_id: None,
                tabs: Vec::new(),
                terminal_profile_id: None,
                presentation_icon: None,
            },
        ],
        quick_terminal: None,
    };

    let pruned = manifest.pruning_expired_tabs("2027-01-16T08:00:00Z", 60);

    assert_eq!(
        pruned.spaces[0]
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tab_pinned", "tab_active"]
    );
    assert_eq!(
        pruned.spaces[0].selected_tab_id.as_deref(),
        Some("tab_pinned")
    );
    assert_eq!(pruned.selected_tab_id.as_deref(), Some("tab_pinned"));
    assert_eq!(pruned.spaces[1].tabs.len(), 0);
}

#[test]
fn materialize_uses_pin_snapshot_for_pinned_tabs() {
    let live = content_tab("tab_pinned", "Pinned", "/live/project")
        .live_snapshot
        .expect("live snapshot");
    let pin = content_tab("tab_pinned", "Pinned", "/pinned/project")
        .live_snapshot
        .expect("pin snapshot");
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_pinned".to_string()),
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_pinned".to_string()),
            tabs: vec![ShellContentWorkspaceTabRecord {
                is_pinned: true,
                pin_snapshot: Some(pin),
                live_snapshot: Some(live),
                ..content_tab("tab_pinned", "Pinned", "/ignored")
            }],
            terminal_profile_id: None,
            presentation_icon: None,
        }],
        quick_terminal: None,
    };

    let state = manifest.materialize("/fallback", REFERENCE_TIME);

    assert_eq!(
        state.contents[0]
            .terminal_metadata
            .as_ref()
            .and_then(|metadata| metadata.cwd.as_deref()),
        Some("/pinned/project")
    );
}

#[test]
fn legacy_terminal_manifest_migrates_to_content_container_shape() {
    let legacy = ShellWorkspaceManifest {
        schema_version: 1,
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_main".to_string()),
        spaces: vec![ShellWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_main".to_string()),
            tabs: vec![ShellWorkspaceTabRecord {
                tab_id: "tab_main".to_string(),
                title: Some("Pinned".to_string()),
                kind: TabKind::Terminal,
                created_at: reference_time(),
                last_activated_at: reference_time(),
                last_activity_at: reference_time(),
                is_pinned: true,
                is_title_user_locked: Some(true),
                pin_snapshot: Some(ShellTabRestoreSnapshot {
                    pane_tree: PaneTreeNode::pane("node_pane_1", "pane_1"),
                    panes: vec![ShellPaneRestoreRecord {
                        pane_id: "pane_1".to_string(),
                        launch_target: ShellLaunchTarget::Shell,
                        cwd: Some("/pinned".to_string()),
                        title: Some("Pinned".to_string()),
                        terminal_profile_id: Some("profile-main".to_string()),
                    }],
                }),
                live_snapshot: None,
                active_task: ShellTabActiveTaskState::Inactive,
            }],
            terminal_profile_id: Some("profile-main".to_string()),
            presentation_icon: Some("rectangle.stack.fill".to_string()),
        }],
    };

    let migrated = legacy.migrating_terminal_restore_snapshots_to_content_containers();
    let tab = &migrated.spaces[0].tabs[0];
    let pin_snapshot = tab.pin_snapshot.as_ref().expect("pin snapshot");

    assert_eq!(migrated.content_contract_version, "0.2");
    assert_eq!(pin_snapshot.pane_slots[0].pane_slot_id, "pane_1");
    assert_eq!(pin_snapshot.pane_slots[0].content_id, "content_pane_1");
    assert_eq!(pin_snapshot.contents[0].content_id, "content_pane_1");
    assert_eq!(
        pin_snapshot.contents[0]
            .payload
            .terminal
            .as_ref()
            .and_then(|payload| payload.cwd.as_deref()),
        Some("/pinned")
    );
    assert_eq!(
        pin_snapshot.contents[0]
            .payload
            .terminal
            .as_ref()
            .and_then(|payload| payload.terminal_profile_id.as_deref()),
        Some("profile-main")
    );

    let state = migrated.materialize("/fallback", REFERENCE_TIME);
    assert_eq!(
        state.contents[0]
            .terminal_metadata
            .as_ref()
            .and_then(|metadata| metadata.cwd.as_deref()),
        Some("/pinned")
    );
    assert_eq!(
        state.spaces[0].presentation_icon.as_deref(),
        Some("rectangle.stack.fill")
    );
}

#[test]
fn materialize_restores_quick_terminal_hidden_with_runtime_metadata() {
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_main".to_string()),
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_main".to_string()),
            tabs: vec![content_tab("tab_main", "Main", "/main")],
            terminal_profile_id: None,
            presentation_icon: None,
        }],
        quick_terminal: Some(ShellQuickTerminalRestoreRecord {
            pane_id: "quick_terminal_pane".to_string(),
            presentation: ShellQuickTerminalPresentation::Visible,
            last_working_directory: None,
            live_snapshot: Some(ShellContentTabRestoreSnapshot {
                pane_tree: PaneTreeNode::pane("node_quick_terminal_pane", "quick_terminal_pane"),
                pane_slots: vec![ShellPaneSlotRestoreRecord {
                    pane_slot_id: "quick_terminal_pane".to_string(),
                    content_id: "content_quick_terminal_pane".to_string(),
                }],
                contents: vec![ShellContentRestoreRecord {
                    content_id: "content_quick_terminal_pane".to_string(),
                    kind: ContentKind::Terminal,
                    title: "python server".to_string(),
                    payload: ShellContentPayload::terminal(
                        ShellLaunchTarget::Shell,
                        Some("/repo/quick"),
                        Some("python server"),
                    ),
                }],
            }),
            active_task: ShellTabActiveTaskState::ForegroundCommand,
        }),
    };

    let state = manifest.materialize("/fallback", REFERENCE_TIME);
    let quick_terminal = state.quick_terminal.expect("quick terminal");

    assert_eq!(
        quick_terminal.presentation,
        ShellQuickTerminalPresentation::Hidden
    );
    assert_eq!(
        quick_terminal.last_working_directory.as_deref(),
        Some("/repo/quick")
    );
    assert_eq!(quick_terminal.content_id, "content_quick_terminal_pane");
    assert_eq!(
        quick_terminal
            .terminal_metadata
            .as_ref()
            .and_then(|metadata| metadata.cwd.as_deref()),
        Some("/repo/quick")
    );
}

fn content_tab(tab_id: &str, title: &str, cwd: &str) -> ShellContentWorkspaceTabRecord {
    let pane_slot_id = format!("pane_{tab_id}");
    let content_id = format!("content_{pane_slot_id}");
    ShellContentWorkspaceTabRecord {
        tab_id: tab_id.to_string(),
        title: Some(title.to_string()),
        kind: TabKind::Terminal,
        created_at: reference_time(),
        last_activated_at: reference_time(),
        last_activity_at: reference_time(),
        is_pinned: false,
        is_title_user_locked: None,
        pin_snapshot: None,
        live_snapshot: Some(ShellContentTabRestoreSnapshot {
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            pane_slots: vec![ShellPaneSlotRestoreRecord {
                pane_slot_id: pane_slot_id.clone(),
                content_id: content_id.clone(),
            }],
            contents: vec![ShellContentRestoreRecord {
                content_id,
                kind: ContentKind::Terminal,
                title: title.to_string(),
                payload: ShellContentPayload::terminal(
                    ShellLaunchTarget::Shell,
                    Some(cwd),
                    Some(title),
                ),
            }],
        }),
        active_task: ShellTabActiveTaskState::Inactive,
    }
}

fn reference_time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(REFERENCE_TIME)
        .unwrap()
        .with_timezone(&Utc)
}
