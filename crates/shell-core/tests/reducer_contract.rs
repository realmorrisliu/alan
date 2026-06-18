use alan_shell_core::{
    ContentInstance, ContentKind, ContentLifecycleState, ManifestSyncHint, PaneSlot, PaneTreeNode,
    ReducerErrorCode, ReducerOperation, RuntimeIntent, ShellAttentionState, ShellContentPayload,
    ShellLaunchTarget, ShellTabActiveTaskState, Space, SplitDirection, SplitPlacement, Tab,
    TabKind, TabOrganizationSection, TerminalActivityAgentMetadata, TerminalActivityDisplay,
    TerminalActivityFreshness, TerminalActivityPriority, TerminalActivitySnapshot,
    TerminalActivitySource, TerminalActivitySourceKind, TerminalActivityStatus, WorkspaceState,
};
use serde_json::json;

#[test]
fn split_pane_reducer_returns_created_ids_runtime_intent_and_next_state() {
    let state = base_state();

    let result = state
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: Some("Worker".to_string()),
            working_directory: Some("/tmp/project".to_string()),
            terminal_profile_id: Some("profile-main".to_string()),
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("split succeeds");

    assert_eq!(result.state.focused_pane_id.as_deref(), Some("pane_2"));
    assert_eq!(result.focus.pane_slot_id.as_deref(), Some("pane_2"));
    assert_eq!(result.changed_ids.created_pane_slot_ids, vec!["pane_2"]);
    assert_eq!(
        result.changed_ids.created_content_ids,
        vec!["content_pane_2"]
    );
    assert_eq!(result.changed_ids.updated_tab_ids, vec!["tab_main"]);
    assert_eq!(result.manifest_sync, ManifestSyncHint::SyncWorkspaceState);
    assert_eq!(
        result.state.spaces[0].tabs[0].pane_tree.pane_ids(),
        vec!["pane_1".to_string(), "pane_2".to_string()]
    );
    let created_payload = result
        .state
        .contents
        .iter()
        .find(|content| content.content_id == "content_pane_2")
        .and_then(|content| content.payload.terminal.as_ref())
        .expect("split terminal content carries payload");
    assert_eq!(created_payload.cwd.as_deref(), Some("/tmp/project"));
    assert_eq!(
        created_payload.terminal_profile_id.as_deref(),
        Some("profile-main")
    );
    assert_eq!(created_payload.title.as_deref(), Some("Worker"));
    assert_eq!(
        content_title(&result.state, "content_pane_2"),
        Some("Worker")
    );
    assert!(matches!(
        result.runtime_intents.as_slice(),
        [RuntimeIntent::StartTerminal {
            pane_slot_id,
            content_id,
            working_directory,
            terminal_profile_id,
            title,
        }] if pane_slot_id == "pane_2"
            && content_id == "content_pane_2"
            && working_directory.as_deref() == Some("/tmp/project")
            && terminal_profile_id.as_deref() == Some("profile-main")
            && title == "Worker"
    ));
}

#[test]
fn terminal_creation_reducers_skip_reserved_platform_pane_slot_ids() {
    let reserved = vec!["pane_2".to_string()];

    let opened = base_state()
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Opened".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: reserved.clone(),
        })
        .expect("open terminal tab succeeds");
    assert_eq!(opened.changed_ids.created_pane_slot_ids, vec!["pane_3"]);
    assert_eq!(
        content_title(&opened.state, "content_pane_3"),
        Some("Opened")
    );

    let split = base_state()
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            title: Some("Split Worker".to_string()),
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: reserved.clone(),
        })
        .expect("split succeeds");
    assert_eq!(split.changed_ids.created_pane_slot_ids, vec!["pane_3"]);
    assert_eq!(
        content_title(&split.state, "content_pane_3"),
        Some("Split Worker")
    );

    let duplicated = base_state()
        .reduce(ReducerOperation::DuplicateTab {
            tab_id: "tab_main".to_string(),
            reserved_pane_slot_ids: reserved.clone(),
        })
        .expect("duplicate tab succeeds");
    assert_eq!(duplicated.changed_ids.created_pane_slot_ids, vec!["pane_3"]);

    let created_space = base_state()
        .reduce(ReducerOperation::CreateTerminalSpace {
            title: None,
            tab_title: Some("Other Shell".to_string()),
            working_directory: Some("/repo/generated/.git".to_string()),
            terminal_profile_id: None,
            presentation_icon: Some("folder.fill".to_string()),
            reserved_pane_slot_ids: reserved,
        })
        .expect("create terminal space succeeds");
    assert_eq!(
        created_space.changed_ids.created_pane_slot_ids,
        vec!["pane_3"]
    );
    assert_eq!(
        content_title(&created_space.state, "content_pane_3"),
        Some("Other Shell")
    );
    assert_eq!(created_space.state.spaces[1].title, "generated");
    assert_eq!(
        created_space.state.spaces[1].presentation_icon.as_deref(),
        Some("folder.fill")
    );
}

#[test]
fn space_metadata_reducers_update_terminal_profile_and_presentation_icon() {
    let profiled = base_state()
        .reduce(ReducerOperation::SetTerminalProfile {
            space_id: "space_main".to_string(),
            terminal_profile_id: Some("profile-alt".to_string()),
        })
        .expect("terminal profile update succeeds");
    assert_eq!(
        profiled.state.spaces[0].terminal_profile_id.as_deref(),
        Some("profile-alt")
    );
    assert_eq!(profiled.changed_ids.updated_space_ids, vec!["space_main"]);

    let icon = profiled
        .state
        .reduce(ReducerOperation::SetPresentationIcon {
            space_id: "space_main".to_string(),
            presentation_icon: Some("folder.fill".to_string()),
        })
        .expect("presentation icon update succeeds");
    assert_eq!(
        icon.state.spaces[0].presentation_icon.as_deref(),
        Some("folder.fill")
    );

    let cleared_invalid = icon
        .state
        .reduce(ReducerOperation::SetPresentationIcon {
            space_id: "space_main".to_string(),
            presentation_icon: Some("not a symbol!!".to_string()),
        })
        .expect("invalid presentation icon clears to default");
    assert_eq!(cleared_invalid.state.spaces[0].presentation_icon, None);

    let missing = base_state()
        .reduce(ReducerOperation::SetTerminalProfile {
            space_id: "missing".to_string(),
            terminal_profile_id: None,
        })
        .unwrap_err();
    assert_eq!(missing.code, ReducerErrorCode::SpaceNotFound);
}

#[test]
fn delete_space_removes_space_content_and_bootstraps_default_when_empty() {
    let state = base_state()
        .reduce(ReducerOperation::CreateTerminalSpace {
            title: Some("Other".to_string()),
            tab_title: Some("Other Shell".to_string()),
            working_directory: Some("/tmp/other".to_string()),
            terminal_profile_id: None,
            presentation_icon: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;

    let deleted_main = state
        .reduce(ReducerOperation::DeleteSpace {
            space_id: "space_main".to_string(),
            default_working_directory: Some("/fallback".to_string()),
        })
        .expect("delete space succeeds");
    assert_eq!(
        deleted_main.changed_ids.removed_space_ids,
        vec!["space_main"]
    );
    assert!(
        !deleted_main
            .state
            .spaces
            .iter()
            .any(|space| space.space_id == "space_main")
    );
    assert_eq!(
        deleted_main.state.focused_space_id.as_deref(),
        Some("space_2")
    );
    assert_eq!(
        deleted_main.state.focused_pane_id.as_deref(),
        Some("pane_2")
    );
    assert!(
        !deleted_main
            .state
            .pane_slots
            .iter()
            .any(|pane| pane.pane_slot_id == "pane_1")
    );
    assert!(
        deleted_main
            .state
            .contents
            .iter()
            .all(|content| content.content_id != "content_pane_1")
    );

    let bootstrapped = base_state()
        .reduce(ReducerOperation::DeleteSpace {
            space_id: "space_main".to_string(),
            default_working_directory: Some("/fallback".to_string()),
        })
        .expect("deleting final space bootstraps default workspace");
    assert_eq!(bootstrapped.state.spaces.len(), 1);
    assert_eq!(
        bootstrapped.state.focused_space_id.as_deref(),
        Some("space_main")
    );
    assert_eq!(
        bootstrapped
            .state
            .contents
            .iter()
            .find(|content| content.content_id == "content_pane_1")
            .and_then(|content| content.payload.terminal.as_ref())
            .and_then(|payload| payload.cwd.as_deref()),
        Some("/fallback")
    );
}

#[test]
fn invalid_reducer_operation_returns_stable_error_and_leaves_input_available() {
    let state = base_state();
    let error = state
        .reduce(ReducerOperation::SplitPane {
            pane_slot_id: "missing".to_string(),
            placement: SplitPlacement::Right,
            title: None,
            working_directory: None,
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap_err();

    assert_eq!(error.code, ReducerErrorCode::PaneNotFound);
    assert_eq!(state.focused_pane_id.as_deref(), Some("pane_1"));
    assert_eq!(state.pane_slots.len(), 1);
}

#[test]
fn closing_selected_pane_removes_content_and_repairs_focus() {
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

    let result = state
        .reduce(ReducerOperation::ClosePane {
            pane_slot_id: "pane_2".to_string(),
        })
        .expect("close pane succeeds");

    assert_eq!(result.state.focused_pane_id.as_deref(), Some("pane_1"));
    assert_eq!(result.changed_ids.removed_pane_slot_ids, vec!["pane_2"]);
    assert_eq!(
        result.changed_ids.removed_content_ids,
        vec!["content_pane_2"]
    );
    assert_eq!(result.state.pane_slots.len(), 1);
    assert_eq!(
        result.state.spaces[0].tabs[0].pane_tree.pane_ids(),
        vec!["pane_1".to_string()]
    );
    assert!(matches!(
        result.runtime_intents.as_slice(),
        [RuntimeIntent::CloseTerminal { pane_slot_id, content_id }]
            if pane_slot_id == "pane_2" && content_id == "content_pane_2"
    ));
}

#[test]
fn pure_tab_and_attention_reducers_report_changed_ids() {
    let state = base_state();

    let renamed = state
        .reduce(ReducerOperation::RenameTab {
            tab_id: "tab_main".to_string(),
            title: "  Focused   Work  ".to_string(),
        })
        .expect("rename succeeds");
    assert_eq!(
        renamed.state.spaces[0].tabs[0].title.as_deref(),
        Some("Focused Work")
    );
    assert!(renamed.state.spaces[0].tabs[0].is_title_user_locked);
    assert_eq!(renamed.changed_ids.updated_tab_ids, vec!["tab_main"]);

    let pinned = renamed
        .state
        .reduce(ReducerOperation::PinTab {
            tab_id: "tab_main".to_string(),
        })
        .expect("pin succeeds");
    assert!(pinned.state.spaces[0].tabs[0].is_pinned);

    let attention = pinned
        .state
        .reduce(ReducerOperation::SetAttention {
            pane_slot_id: "pane_1".to_string(),
            attention: ShellAttentionState::AwaitingUser,
        })
        .expect("attention succeeds");
    assert_eq!(
        attention.state.pane_slots[0].attention,
        ShellAttentionState::AwaitingUser
    );
    assert_eq!(
        attention.state.spaces[0].attention,
        ShellAttentionState::AwaitingUser
    );
    assert_eq!(attention.changed_ids.updated_pane_slot_ids, vec!["pane_1"]);
}

#[test]
fn tab_reducers_duplicate_move_reorder_and_clear_inactive_temporary_tabs() {
    let state = base_state()
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Second".to_string()),
            working_directory: Some("/tmp/second".to_string()),
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::OpenTerminalTab {
            space_id: Some("space_main".to_string()),
            title: Some("Third".to_string()),
            working_directory: Some("/tmp/third".to_string()),
            terminal_profile_id: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state
        .reduce(ReducerOperation::CreateTerminalSpace {
            title: Some("Other".to_string()),
            tab_title: Some("Other Shell".to_string()),
            working_directory: Some("/tmp/other".to_string()),
            terminal_profile_id: None,
            presentation_icon: None,
            reserved_pane_slot_ids: Vec::new(),
        })
        .unwrap()
        .state;

    let duplicated = state
        .reduce(ReducerOperation::DuplicateTab {
            tab_id: "tab_2".to_string(),
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("duplicate tab succeeds");
    assert_eq!(
        duplicated.changed_ids.created_tab_ids,
        vec!["tab_5".to_string()]
    );
    assert_eq!(duplicated.state.spaces[0].tabs[2].tab_id, "tab_5");
    assert_eq!(
        duplicated.state.spaces[0].tabs[2].title.as_deref(),
        Some("Second")
    );
    assert_eq!(
        duplicated.runtime_intents,
        vec![RuntimeIntent::StartTerminal {
            pane_slot_id: "pane_5".to_string(),
            content_id: "content_pane_5".to_string(),
            working_directory: Some("/tmp/second".to_string()),
            terminal_profile_id: Some("profile-main".to_string()),
            title: "Second".to_string(),
        }]
    );
    let duplicated_payload = duplicated
        .state
        .contents
        .iter()
        .find(|content| content.content_id == "content_pane_5")
        .and_then(|content| content.payload.terminal.as_ref())
        .expect("duplicated terminal content carries payload");
    assert_eq!(
        duplicated_payload.terminal_profile_id.as_deref(),
        Some("profile-main")
    );
    assert_eq!(duplicated_payload.cwd.as_deref(), Some("/tmp/second"));
    assert_eq!(duplicated_payload.title.as_deref(), Some("Second"));

    let moved_to_space = duplicated
        .state
        .reduce(ReducerOperation::MoveTabToSpace {
            tab_id: "tab_5".to_string(),
            target_space_id: "space_2".to_string(),
        })
        .expect("move tab to space succeeds");
    assert_eq!(
        moved_to_space.state.spaces[1]
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tab_4", "tab_5"]
    );
    assert_eq!(
        moved_to_space
            .state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == "pane_5")
            .unwrap()
            .space_id,
        "space_2"
    );

    let reordered = moved_to_space
        .state
        .reduce(ReducerOperation::MoveTab {
            tab_id: "tab_3".to_string(),
            section_offset: -1,
        })
        .expect("move tab within section succeeds");
    assert_eq!(
        reordered.state.spaces[0]
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tab_main", "tab_3", "tab_2"]
    );

    let focused_for_organization = reordered
        .state
        .reduce(ReducerOperation::FocusPane {
            pane_slot_id: "pane_1".to_string(),
        })
        .expect("focus main tab before non-focused organization")
        .state;
    let organized = focused_for_organization
        .reduce(ReducerOperation::OrganizeTab {
            tab_id: "tab_2".to_string(),
            target_space_id: Some("space_2".to_string()),
            section: TabOrganizationSection::Pinned,
            index: Some(0),
        })
        .expect("organize tab into target section succeeds");
    assert_eq!(
        organized.state.spaces[1]
            .tabs
            .iter()
            .map(|tab| (tab.tab_id.as_str(), tab.is_pinned))
            .collect::<Vec<_>>(),
        vec![("tab_2", true), ("tab_4", false), ("tab_5", false)]
    );
    assert_eq!(
        organized
            .state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == "pane_2")
            .unwrap()
            .space_id,
        "space_2"
    );
    assert_eq!(
        organized.state.focused_pane_id.as_deref(),
        Some("pane_1"),
        "organizing a non-focused tab preserves focus"
    );
    assert_eq!(
        organized.state.spaces[1].selected_tab_id.as_deref(),
        Some("tab_5"),
        "organizing a tab does not select it in the target space"
    );
    assert_eq!(
        organized.changed_ids.updated_tab_ids,
        vec!["tab_2".to_string()]
    );
    assert_eq!(
        organized.changed_ids.updated_pane_slot_ids,
        vec!["pane_2".to_string()]
    );

    let selected_for_clear = reordered
        .state
        .reduce(ReducerOperation::FocusPane {
            pane_slot_id: "pane_2".to_string(),
        })
        .expect("focus protected tab")
        .state;
    let cleared = selected_for_clear
        .reduce(ReducerOperation::ClearInactiveTemporaryTabs {
            space_id: "space_main".to_string(),
            protected_tab_ids: vec!["tab_2".to_string()],
        })
        .expect("clear inactive tabs succeeds");
    assert_eq!(
        cleared.state.spaces[0]
            .tabs
            .iter()
            .map(|tab| tab.tab_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tab_2"]
    );
    assert_eq!(cleared.state.focused_tab_id.as_deref(), Some("tab_2"));
    assert_eq!(cleared.state.focused_pane_id.as_deref(), Some("pane_2"));
    assert!(
        cleared
            .changed_ids
            .removed_tab_ids
            .contains(&"tab_3".to_string())
    );
    assert!(
        cleared
            .changed_ids
            .removed_tab_ids
            .contains(&"tab_main".to_string())
    );
}

#[test]
fn content_reducers_open_split_and_focus_existing_settings_content() {
    let opened = base_state()
        .reduce(ReducerOperation::OpenContentTab {
            space_id: Some("space_main".to_string()),
            kind: ContentKind::Markdown,
            title: "Guide.md".to_string(),
            payload: ShellContentPayload {
                terminal: None,
                markdown: Some(json!({
                    "file_url": "file:///repo/Guide.md",
                    "title": "Guide.md"
                })),
                settings: None,
            },
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("open markdown content succeeds");
    assert_eq!(opened.changed_ids.created_tab_ids, vec!["tab_2"]);
    assert_eq!(opened.changed_ids.created_pane_slot_ids, vec!["pane_2"]);
    assert_eq!(
        opened.changed_ids.created_content_ids,
        vec!["content_markdown_pane_2"]
    );
    assert_eq!(opened.state.focused_pane_id.as_deref(), Some("pane_2"));
    let markdown = opened
        .state
        .contents
        .iter()
        .find(|content| content.content_id == "content_markdown_pane_2")
        .expect("markdown content created");
    assert_eq!(markdown.kind, ContentKind::Markdown);
    assert_eq!(
        markdown.payload.markdown.as_ref(),
        Some(&json!({
            "file_url": "file:///repo/Guide.md",
            "title": "Guide.md"
        }))
    );

    let settings = opened
        .state
        .reduce(ReducerOperation::OpenContentTab {
            space_id: Some("space_main".to_string()),
            kind: ContentKind::Settings,
            title: "Settings".to_string(),
            payload: ShellContentPayload {
                terminal: None,
                markdown: None,
                settings: Some(json!({
                    "surface_id": "settings_main",
                    "title": "Settings"
                })),
            },
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("open settings content succeeds");
    assert_eq!(
        settings.changed_ids.created_content_ids,
        vec!["content_settings_main"]
    );
    let focused_existing_settings = settings
        .state
        .reduce(ReducerOperation::OpenContentTab {
            space_id: Some("space_main".to_string()),
            kind: ContentKind::Settings,
            title: "Settings".to_string(),
            payload: ShellContentPayload {
                terminal: None,
                markdown: None,
                settings: Some(json!({
                    "surface_id": "settings_main",
                    "title": "Settings"
                })),
            },
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("opening settings focuses existing settings content");
    assert!(
        focused_existing_settings
            .changed_ids
            .created_tab_ids
            .is_empty()
    );
    assert_eq!(
        focused_existing_settings.state.focused_pane_id.as_deref(),
        settings.state.focused_pane_id.as_deref()
    );

    let split = settings
        .state
        .reduce(ReducerOperation::SplitContentPane {
            pane_slot_id: "pane_1".to_string(),
            placement: SplitPlacement::Right,
            kind: ContentKind::Markdown,
            title: "Split Notes".to_string(),
            payload: ShellContentPayload {
                terminal: None,
                markdown: Some(json!({
                    "file_url": "file:///repo/Split.md",
                    "title": "Split Notes"
                })),
                settings: None,
            },
            reserved_pane_slot_ids: Vec::new(),
        })
        .expect("split markdown content succeeds");
    assert_eq!(split.changed_ids.created_pane_slot_ids, vec!["pane_4"]);
    assert_eq!(
        split.changed_ids.created_content_ids,
        vec!["content_markdown_pane_4"]
    );
    assert_eq!(split.state.focused_pane_id.as_deref(), Some("pane_4"));
    assert!(
        split.state.spaces[0].tabs[0]
            .pane_tree
            .contains_pane_id("pane_4")
    );
}

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
