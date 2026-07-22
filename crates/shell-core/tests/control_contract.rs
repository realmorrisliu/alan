use alan_shell_core::{
    ContentInstance, ContentKind, ContentLifecycleState, PaneSlot, PaneTreeNode,
    ShellAttentionState, ShellContentPayload, ShellControlCommand, ShellControlCommandKind,
    ShellControlExecutionContext, ShellControlRuntimeIntent, ShellLaunchTarget, Space,
    SplitDirection, Tab, TabKind, TabOrganizationSection, TerminalControlKey, WorkspaceState,
};

#[test]
fn state_command_projects_snapshot_lists_and_contract_version() {
    let state = base_state();

    let result = state.reduce_control(command("req-state", ShellControlCommandKind::State));

    assert_eq!(result.response.request_id, "req-state");
    assert_eq!(result.response.contract_version, "0.2");
    assert_eq!(result.response.applied, Some(true));
    assert_eq!(
        result.response.state.as_ref().unwrap().window_id,
        "window_main"
    );
    assert_eq!(result.response.pane_slots.as_ref().unwrap().len(), 1);
    assert_eq!(result.response.contents.as_ref().unwrap().len(), 1);
    assert!(result.updated_state.is_none());
    assert!(result.runtime_intents.is_empty());
}

#[test]
fn creation_commands_respect_runtime_reserved_pane_slot_ids() {
    let context = ShellControlExecutionContext {
        reserved_pane_slot_ids: vec!["pane_2".to_string()],
    };

    let created_space = base_state()
        .reduce_control_with_context(
            command("req-space", ShellControlCommandKind::SpaceCreate),
            context.clone(),
        )
        .updated_state
        .expect("space creation updates state");
    assert!(
        created_space
            .pane_slots
            .iter()
            .any(|pane| pane.pane_slot_id == "pane_3")
    );

    let opened_tab = base_state()
        .reduce_control_with_context(
            command("req-tab", ShellControlCommandKind::TabOpen),
            context.clone(),
        )
        .updated_state
        .expect("tab creation updates state");
    assert!(
        opened_tab
            .pane_slots
            .iter()
            .any(|pane| pane.pane_slot_id == "pane_3")
    );

    let mut split = command("req-split", ShellControlCommandKind::PaneSplit);
    split.pane_id = Some("pane_1".to_string());
    split.direction = Some(SplitDirection::Vertical);
    let split_state = base_state()
        .reduce_control_with_context(split, context)
        .updated_state
        .expect("pane split updates state");
    assert!(
        split_state
            .pane_slots
            .iter()
            .any(|pane| pane.pane_slot_id == "pane_3")
    );
}

#[test]
fn missing_tab_close_uses_stable_validation_code() {
    let state = base_state();

    let result = state.reduce_control(command("req-close", ShellControlCommandKind::TabClose));

    assert_eq!(result.response.applied, Some(false));
    assert_eq!(result.response.error_code.as_deref(), Some("tab_required"));
    assert!(result.updated_state.is_none());
    assert!(result.runtime_intents.is_empty());
}

#[test]
fn background_tab_close_preserves_the_closed_tab_subject() {
    let state = pinned_and_unpinned_state()
        .reduce_control(command(
            "req-create-foreground-space",
            ShellControlCommandKind::SpaceCreate,
        ))
        .updated_state
        .expect("Space creation updates focus");
    let foreground_pane_id = state.focused_pane_id.clone();
    let mut request = command("req-close-background", ShellControlCommandKind::TabClose);
    request.tab_id = Some("tab_pinned".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(true));
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_pinned"));
    assert_eq!(result.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(
        result.response.current_focused_pane_slot_id,
        foreground_pane_id
    );
    assert!(
        result
            .updated_state
            .as_ref()
            .is_some_and(|state| state.tab("tab_pinned").is_none())
    );
}

#[test]
fn tab_open_applies_reducer_and_wraps_terminal_start_intent() {
    let state = base_state();
    let mut request = command("req-open", ShellControlCommandKind::TabOpen);
    request.space_id = Some("space_main".to_string());
    request.title = Some("Worker".to_string());
    request.cwd = Some("/tmp/project".to_string());
    request.terminal_profile_id = Some("profile-main".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(true));
    assert_eq!(result.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_2"));
    assert_eq!(result.response.pane_slot_id.as_deref(), Some("pane_2"));
    assert_eq!(
        result.response.content_id.as_deref(),
        Some("content_pane_2")
    );
    assert_eq!(
        result
            .updated_state
            .as_ref()
            .unwrap()
            .focused_pane_id
            .as_deref(),
        Some("pane_2")
    );
    let created_payload = result
        .updated_state
        .as_ref()
        .unwrap()
        .contents
        .iter()
        .find(|content| content.content_id == "content_pane_2")
        .and_then(|content| content.payload.terminal.as_ref())
        .expect("tab.open terminal content carries launch payload");
    assert_eq!(created_payload.cwd.as_deref(), Some("/tmp/project"));
    assert_eq!(
        created_payload.terminal_profile_id.as_deref(),
        Some("profile-main")
    );
    assert_eq!(created_payload.title.as_deref(), Some("Worker"));
    assert!(matches!(
        result.runtime_intents.as_slice(),
        [ShellControlRuntimeIntent::Reducer {
            intent: alan_shell_core::RuntimeIntent::StartTerminal {
                pane_slot_id,
                content_id,
                working_directory,
                terminal_profile_id,
                title,
            }
        }] if pane_slot_id == "pane_2"
            && content_id == "content_pane_2"
            && working_directory.as_deref() == Some("/tmp/project")
            && terminal_profile_id.as_deref() == Some("profile-main")
            && title == "Worker"
    ));
}

#[test]
fn pane_split_missing_direction_uses_stable_validation_code() {
    let state = base_state();
    let mut request = command("req-split", ShellControlCommandKind::PaneSplit);
    request.pane_id = Some("pane_1".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(false));
    assert_eq!(
        result.response.error_code.as_deref(),
        Some("direction_required")
    );
    assert!(result.updated_state.is_none());
}

#[test]
fn equalize_reports_changed_splits_and_rejects_unchanged_state() {
    let state = split_state();
    let split_node_id = state.spaces[0].tabs[0]
        .pane_tree
        .split_ratios_by_node_id()
        .into_keys()
        .next()
        .expect("split fixture exposes a split node");
    let mut resize = command("req-resize", ShellControlCommandKind::PaneResizeSplit);
    resize.split_node_id = Some(split_node_id.clone());
    resize.ratio = Some(0.72);
    let resized = state.reduce_control(resize);
    let resized_state = resized.updated_state.expect("resize updates state");

    let mut equalize = command("req-equalize", ShellControlCommandKind::PaneEqualizeSplits);
    equalize.tab_id = Some("tab_main".to_string());
    let equalized = resized_state.reduce_control(equalize);

    assert_eq!(equalized.response.applied, Some(true));
    assert_eq!(equalized.response.ratio, Some(0.5));
    assert_eq!(
        equalized.response.changed_split_ids.as_deref(),
        Some([split_node_id].as_slice())
    );

    let unchanged = equalized
        .updated_state
        .expect("equalize updates state")
        .reduce_control(command(
            "req-equalize-implicit",
            ShellControlCommandKind::PaneEqualizeSplits,
        ));
    assert_eq!(unchanged.response.applied, Some(false));
    assert_eq!(
        unchanged.response.error_code.as_deref(),
        Some("unchanged_state")
    );
    assert_eq!(unchanged.response.tab_id.as_deref(), Some("tab_main"));
    assert!(unchanged.updated_state.is_none());
}

#[test]
fn split_mutations_project_the_background_target_space_and_tab() {
    let target_state = split_state();
    let target_split_id = target_state.spaces[0].tabs[0]
        .pane_tree
        .split_ratios_by_node_id()
        .into_keys()
        .next()
        .expect("target tab exposes a split node");
    let focused_other_space = target_state
        .reduce_control(command(
            "req-create-foreground-space",
            ShellControlCommandKind::SpaceCreate,
        ))
        .updated_state
        .expect("Space creation updates focus");
    assert_ne!(
        focused_other_space.focused_space_id.as_deref(),
        Some("space_main")
    );

    let mut resize = command(
        "req-resize-background",
        ShellControlCommandKind::PaneResizeSplit,
    );
    resize.split_node_id = Some(target_split_id);
    resize.ratio = Some(0.7);
    let resized = focused_other_space.reduce_control(resize);
    assert_eq!(resized.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(resized.response.tab_id.as_deref(), Some("tab_main"));

    let mut equalize = command(
        "req-equalize-background",
        ShellControlCommandKind::PaneEqualizeSplits,
    );
    equalize.tab_id = Some("tab_main".to_string());
    let equalized = resized
        .updated_state
        .expect("resize updates target tab")
        .reduce_control(equalize);
    assert_eq!(equalized.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(equalized.response.tab_id.as_deref(), Some("tab_main"));
}

#[test]
fn unzoom_validates_explicit_pane_before_mutating_tab() {
    let state = split_state();
    let mut zoom = command("req-zoom", ShellControlCommandKind::PaneZoom);
    zoom.pane_id = Some("pane_2".to_string());
    let zoomed = state.reduce_control(zoom);
    let zoomed_state = zoomed.updated_state.expect("zoom updates state");

    let mut missing = command("req-unzoom-missing", ShellControlCommandKind::PaneUnzoom);
    missing.pane_id = Some("pane_missing".to_string());
    let rejected = zoomed_state.reduce_control(missing);
    assert_eq!(rejected.response.applied, Some(false));
    assert_eq!(
        rejected.response.error_code.as_deref(),
        Some("pane_not_found")
    );
    assert!(rejected.updated_state.is_none());

    let mut unzoom = command("req-unzoom", ShellControlCommandKind::PaneUnzoom);
    unzoom.pane_id = Some("pane_2".to_string());
    let applied = zoomed_state.reduce_control(unzoom);
    assert_eq!(applied.response.applied, Some(true));
    assert_eq!(applied.response.zoomed_pane_id, None);
    assert_eq!(
        applied.updated_state.unwrap().spaces[0].tabs[0].zoomed_pane_id,
        None
    );
}

#[test]
fn unzoom_projects_the_target_tab_instead_of_the_focused_zoom_state() {
    let mut zoom_target = command("req-zoom-target", ShellControlCommandKind::PaneZoom);
    zoom_target.pane_id = Some("pane_2".to_string());
    let target_zoomed = split_state().reduce_control(zoom_target);
    let target_zoomed_pane_id = target_zoomed
        .response
        .zoomed_pane_id
        .clone()
        .expect("target tab is zoomed");

    let opened = target_zoomed
        .updated_state
        .expect("zoom updates target tab")
        .reduce_control(command(
            "req-open-foreground",
            ShellControlCommandKind::TabOpen,
        ));
    let opened_state = opened.updated_state.expect("tab open updates state");
    let foreground_tab_id = opened_state
        .focused_tab_id
        .clone()
        .expect("opened tab is focused");
    let foreground_pane_id = opened_state
        .focused_pane_id
        .clone()
        .expect("opened tab has a focused pane");

    let mut split_foreground = command("req-split-foreground", ShellControlCommandKind::PaneSplit);
    split_foreground.pane_id = Some(foreground_pane_id);
    split_foreground.direction = Some(SplitDirection::Vertical);
    let split_foreground = opened_state.reduce_control(split_foreground);
    let foreground_zoom_pane_id = split_foreground
        .response
        .pane_id
        .clone()
        .expect("split reports its new pane");
    let mut zoom_foreground = command("req-zoom-foreground", ShellControlCommandKind::PaneZoom);
    zoom_foreground.pane_id = Some(foreground_zoom_pane_id.clone());
    let foreground_zoomed = split_foreground
        .updated_state
        .expect("split updates foreground tab")
        .reduce_control(zoom_foreground);
    let foreground_zoomed_state = foreground_zoomed
        .updated_state
        .expect("zoom updates foreground tab");

    let mut unzoom_target = command("req-unzoom-target", ShellControlCommandKind::PaneUnzoom);
    unzoom_target.tab_id = Some("tab_main".to_string());
    let unzoomed = foreground_zoomed_state.reduce_control(unzoom_target);

    assert_eq!(unzoomed.response.applied, Some(true));
    assert_eq!(unzoomed.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(unzoomed.response.tab_id.as_deref(), Some("tab_main"));
    assert_eq!(
        unzoomed.response.pane_id.as_deref(),
        Some(target_zoomed_pane_id.as_str())
    );
    assert_eq!(unzoomed.response.zoomed_pane_id, None);
    let updated = unzoomed.updated_state.expect("unzoom updates target tab");
    assert_eq!(updated.tab("tab_main").unwrap().zoomed_pane_id, None);
    assert_eq!(
        updated
            .tab(&foreground_tab_id)
            .unwrap()
            .zoomed_pane_id
            .as_deref(),
        Some(foreground_zoom_pane_id.as_str())
    );
}

#[test]
fn terminal_send_text_returns_runtime_intent_without_state_update() {
    let state = base_state();
    let mut request = command("req-text", ShellControlCommandKind::TerminalSendText);
    request.pane_id = Some("pane_1".to_string());
    request.text = Some("echo ready\n".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, None);
    assert_eq!(result.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_main"));
    assert_eq!(result.response.pane_slot_id.as_deref(), Some("pane_1"));
    assert_eq!(
        result.response.content_id.as_deref(),
        Some("content_pane_1")
    );
    assert_eq!(result.response.content_kind, Some(ContentKind::Terminal));
    assert!(result.updated_state.is_none());
    assert!(matches!(
        result.runtime_intents.as_slice(),
        [ShellControlRuntimeIntent::SendTerminalText {
            pane_slot_id,
            content_id,
            text,
        }] if pane_slot_id == "pane_1"
            && content_id == "content_pane_1"
            && text == "echo ready\n"
    ));
}

#[test]
fn terminal_send_key_rejects_unsupported_keys() {
    let state = base_state();
    let mut request = command("req-key", ShellControlCommandKind::TerminalSendKey);
    request.pane_id = Some("pane_1".to_string());
    request.key = Some("escape".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(false));
    assert_eq!(
        result.response.error_code.as_deref(),
        Some("terminal_key_unsupported")
    );
    assert!(result.runtime_intents.is_empty());
}

#[test]
fn terminal_send_key_accepts_return_and_defers_to_runtime() {
    let state = base_state();
    let mut request = command("req-return", ShellControlCommandKind::TerminalSendKey);
    request.pane_id = Some("pane_1".to_string());
    request.key = Some("return".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, None);
    assert!(result.updated_state.is_none());
    assert!(matches!(
        result.runtime_intents.as_slice(),
        [ShellControlRuntimeIntent::SendTerminalKey {
            pane_slot_id,
            content_id,
            key: TerminalControlKey::Return,
        }] if pane_slot_id == "pane_1" && content_id == "content_pane_1"
    ));
}

#[test]
fn tab_reorder_moves_tab_into_requested_section() {
    let state = pinned_and_unpinned_state();
    let mut request = command("req-reorder", ShellControlCommandKind::TabReorder);
    request.tab_id = Some("tab_unpinned".to_string());
    request.section = Some(TabOrganizationSection::Pinned);
    request.index = Some(0);

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(true));
    let updated = result
        .updated_state
        .as_ref()
        .expect("reorder updates state");
    let reordered = updated.spaces[0]
        .tabs
        .iter()
        .find(|tab| tab.tab_id == "tab_unpinned")
        .expect("reordered tab survives");
    assert!(
        reordered.is_pinned,
        "tab.reorder must honor the requested pinned section"
    );
    // The response subject must be the reordered tab, not the focused one, and report where it
    // landed so automation can confirm the organization without a follow-up state read.
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_unpinned"));
    assert_eq!(result.response.space_id.as_deref(), Some("space_main"));
    assert_eq!(
        result.response.section,
        Some(TabOrganizationSection::Pinned)
    );
    assert_eq!(result.response.index, Some(0));
}

#[test]
fn tab_reorder_honors_explicit_target_space_aliases() {
    for use_target_space_id in [true, false] {
        let mut state = pinned_and_unpinned_state();
        state.spaces.push(Space {
            space_id: "space_target".to_string(),
            title: "Target".to_string(),
            attention: ShellAttentionState::Idle,
            tabs: Vec::new(),
            selected_tab_id: None,
            terminal_profile_id: None,
            presentation_icon: None,
        });
        let mut request = command("req-reorder-space", ShellControlCommandKind::TabReorder);
        request.tab_id = Some("tab_unpinned".to_string());
        request.section = Some(TabOrganizationSection::Pinned);
        request.index = Some(0);
        if use_target_space_id {
            request.target_space_id = Some("space_target".to_string());
        } else {
            request.space_id = Some("space_target".to_string());
        }

        let result = state.reduce_control(request);

        assert_eq!(result.response.applied, Some(true));
        assert_eq!(result.response.space_id.as_deref(), Some("space_target"));
        assert_eq!(
            result.response.target_space_id.as_deref(),
            Some("space_target")
        );
        assert_eq!(result.response.tab_id.as_deref(), Some("tab_unpinned"));
        let updated = result
            .updated_state
            .expect("cross-Space reorder updates state");
        assert!(
            updated.spaces[0]
                .tabs
                .iter()
                .all(|tab| tab.tab_id != "tab_unpinned"),
            "cross-Space reorder must remove the tab from its source Space"
        );
        let target_tab = updated.spaces[1]
            .tabs
            .first()
            .expect("cross-Space reorder inserts the tab into its target Space");
        assert_eq!(target_tab.tab_id, "tab_unpinned");
        assert!(target_tab.is_pinned);
        assert_eq!(
            updated
                .pane_slots
                .iter()
                .find(|slot| slot.pane_slot_id == "pane_unpinned")
                .map(|slot| slot.space_id.as_str()),
            Some("space_target")
        );
    }
}

#[test]
fn tab_unpin_reports_resulting_section_and_index() {
    let state = pinned_and_unpinned_state();
    let mut request = command("req-unpin", ShellControlCommandKind::TabUnpin);
    request.tab_id = Some("tab_pinned".to_string());

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(true));
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_pinned"));
    assert_eq!(
        result.response.section,
        Some(TabOrganizationSection::Unpinned),
        "tab.unpin must report the resulting unpinned section, not nil"
    );
    assert!(
        result.response.index.is_some(),
        "tab.unpin must report the resulting index within the section"
    );
}

#[test]
fn attention_set_reports_target_pane_not_focused_pane() {
    // Focus is on pane_unpinned; set attention on the background pane_pinned.
    let state = pinned_and_unpinned_state();
    let mut request = command("req-attention", ShellControlCommandKind::AttentionSet);
    request.pane_id = Some("pane_pinned".to_string());
    request.attention = Some(ShellAttentionState::Notable);

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(true));
    assert_eq!(
        result.response.pane_id.as_deref(),
        Some("pane_pinned"),
        "attention.set response must report the mutated pane, not the focused pane"
    );
    assert_eq!(result.response.pane_slot_id.as_deref(), Some("pane_pinned"));
    assert_eq!(result.response.tab_id.as_deref(), Some("tab_pinned"));
    // Focus is unchanged and reported separately.
    assert_eq!(
        result.response.current_focused_pane_slot_id.as_deref(),
        Some("pane_unpinned")
    );
}

#[test]
fn tab_reorder_requires_section() {
    let state = pinned_and_unpinned_state();
    let mut request = command("req-reorder", ShellControlCommandKind::TabReorder);
    request.tab_id = Some("tab_unpinned".to_string());
    request.index = Some(0);

    let result = state.reduce_control(request);

    assert_eq!(result.response.applied, Some(false));
    assert_eq!(
        result.response.error_code.as_deref(),
        Some("tab_reorder_target_required")
    );
    assert!(result.updated_state.is_none());
}

fn command(request_id: &str, kind: ShellControlCommandKind) -> ShellControlCommand {
    ShellControlCommand {
        request_id: request_id.to_string(),
        command: kind,
        space_id: None,
        target_space_id: None,
        tab_id: None,
        pane_id: None,
        pane_slot_id: None,
        content_id: None,
        split_node_id: None,
        ratio: None,
        index: None,
        section: None,
        direction: None,
        spatial_direction: None,
        placement: None,
        title: None,
        cwd: None,
        text: None,
        key: None,
        attention: None,
        terminal_profile_id: None,
    }
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

fn split_state() -> WorkspaceState {
    let mut split = command("fixture-split", ShellControlCommandKind::PaneSplit);
    split.pane_id = Some("pane_1".to_string());
    split.direction = Some(SplitDirection::Vertical);
    base_state()
        .reduce_control(split)
        .updated_state
        .expect("split fixture updates state")
}

fn pinned_and_unpinned_state() -> WorkspaceState {
    fn tab(id: &str, node: &str, pane: &str, is_pinned: bool) -> Tab {
        Tab {
            tab_id: id.to_string(),
            kind: TabKind::Terminal,
            title: Some(id.to_string()),
            pane_tree: PaneTreeNode::pane(node, pane),
            zoomed_pane_id: None,
            is_pinned,
            is_title_user_locked: false,
        }
    }
    fn pane_slot(pane: &str, content: &str, tab: &str) -> PaneSlot {
        PaneSlot {
            pane_slot_id: pane.to_string(),
            tab_id: tab.to_string(),
            space_id: "space_main".to_string(),
            content_id: content.to_string(),
            attention: ShellAttentionState::Active,
        }
    }
    fn content(id: &str) -> ContentInstance {
        ContentInstance {
            content_id: id.to_string(),
            kind: ContentKind::Terminal,
            title: "Shell".to_string(),
            icon_name: None,
            capabilities: ContentKind::Terminal.default_capabilities(),
            payload: ShellContentPayload::terminal(ShellLaunchTarget::Shell, None, Some("Shell")),
            terminal_metadata: None,
            lifecycle: ContentLifecycleState::Active,
            renderer_state: Default::default(),
        }
    }

    WorkspaceState {
        contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        focused_space_id: Some("space_main".to_string()),
        focused_tab_id: Some("tab_unpinned".to_string()),
        focused_pane_id: Some("pane_unpinned".to_string()),
        spaces: vec![Space {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            attention: ShellAttentionState::Active,
            tabs: vec![
                tab("tab_pinned", "node_pinned", "pane_pinned", true),
                tab("tab_unpinned", "node_unpinned", "pane_unpinned", false),
            ],
            selected_tab_id: Some("tab_unpinned".to_string()),
            terminal_profile_id: Some("profile-main".to_string()),
            presentation_icon: None,
        }],
        pane_slots: vec![
            pane_slot("pane_pinned", "content_pinned", "tab_pinned"),
            pane_slot("pane_unpinned", "content_unpinned", "tab_unpinned"),
        ],
        contents: vec![content("content_pinned"), content("content_unpinned")],
    }
}
