use alan_shell_core::{
    AgentAttachment, AgentContentPresentation, AgentProcessReference, AgentStreamOffsets,
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
                agent: None,
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
                agent: None,
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
                agent: None,
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
                agent: None,
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
