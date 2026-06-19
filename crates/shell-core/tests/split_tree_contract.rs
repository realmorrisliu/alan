use alan_shell_core::{
    ContentInstance, ContentKind, ContentLifecycleState, PaneSlot, PaneTreeKind, PaneTreeNode,
    PaneTreeNodeResizeOutcome, ShellAttentionState, ShellContentPayload, ShellLaunchTarget, Space,
    SpatialFocusDirection, SplitDirection, SplitPlacement, Tab, TabKind, WorkspaceState,
};

fn leaf(pane_id: &str) -> PaneTreeNode {
    PaneTreeNode::pane(format!("node_{pane_id}"), pane_id)
}

fn split(direction: SplitDirection, children: Vec<PaneTreeNode>) -> PaneTreeNode {
    let slug = children
        .iter()
        .flat_map(PaneTreeNode::pane_ids)
        .collect::<Vec<_>>()
        .join("_");
    PaneTreeNode::split(
        format!("node_split_{}_{}", direction.as_str(), slug),
        direction,
        children,
    )
}

#[test]
fn pane_split_placement_matches_swift_child_order_and_axis() {
    let base = leaf("pane_1");

    let right = base.split_pane(
        "pane_1",
        SplitPlacement::Right,
        "node_split",
        "node_pane_2",
        "pane_2",
    );
    assert_eq!(right.kind, PaneTreeKind::Split);
    assert_eq!(right.direction, Some(SplitDirection::Vertical));
    assert_eq!(right.ratio, Some(0.5));
    assert_eq!(right.pane_ids(), vec!["pane_1", "pane_2"]);

    let left = base.split_pane(
        "pane_1",
        SplitPlacement::Left,
        "node_split",
        "node_pane_2",
        "pane_2",
    );
    assert_eq!(left.direction, Some(SplitDirection::Vertical));
    assert_eq!(left.pane_ids(), vec!["pane_2", "pane_1"]);

    let down = base.split_pane(
        "pane_1",
        SplitPlacement::Down,
        "node_split",
        "node_pane_2",
        "pane_2",
    );
    assert_eq!(down.direction, Some(SplitDirection::Horizontal));
    assert_eq!(down.pane_ids(), vec!["pane_1", "pane_2"]);

    let up = base.split_pane(
        "pane_1",
        SplitPlacement::Up,
        "node_split",
        "node_pane_2",
        "pane_2",
    );
    assert_eq!(up.direction, Some(SplitDirection::Horizontal));
    assert_eq!(up.pane_ids(), vec!["pane_2", "pane_1"]);
}

#[test]
fn split_ratio_resize_clamps_and_equalize_restores_nested_ratios() {
    let tree = split(
        SplitDirection::Vertical,
        vec![
            leaf("pane_1"),
            split(
                SplitDirection::Horizontal,
                vec![leaf("pane_2"), leaf("pane_3")],
            ),
        ],
    );
    let root_id = tree.node_id.clone();
    let nested_id = tree.children.as_ref().unwrap()[1].node_id.clone();

    let resized = tree.resize_split(&root_id, 0.99);
    assert_eq!(resized.outcome, PaneTreeNodeResizeOutcome::Changed);
    assert_eq!(resized.node.ratio, Some(PaneTreeNode::MAXIMUM_SPLIT_RATIO));

    let resized_nested = resized.node.resize_split(&nested_id, 0.01).node;
    assert_eq!(
        resized_nested.children.as_ref().unwrap()[1].ratio,
        Some(PaneTreeNode::MINIMUM_SPLIT_RATIO)
    );

    let equalized = resized_nested.equalized_splits();
    assert_eq!(equalized.ratio, Some(0.5));
    assert_eq!(equalized.children.as_ref().unwrap()[1].ratio, Some(0.5));
}

#[test]
fn split_ratio_diff_and_zoom_projection_preserve_stable_tree_identity() {
    let tree = split(
        SplitDirection::Vertical,
        vec![leaf("pane_1"), leaf("pane_2")],
    );
    let root_id = tree.node_id.clone();
    let zoomed_leaf = tree.leaf_node("pane_2").unwrap();

    assert_eq!(zoomed_leaf.kind, PaneTreeKind::Pane);
    assert_eq!(zoomed_leaf.pane_id.as_deref(), Some("pane_2"));
    assert_eq!(tree.pane_ids(), vec!["pane_1", "pane_2"]);

    let resized = tree.resize_split(&root_id, 0.72).node;

    assert_eq!(
        resized.split_node_ids_with_changed_ratios(&tree),
        vec![root_id.clone()]
    );
    assert_eq!(
        resized.split_ratios_by_node_id().get(&root_id).copied(),
        Some(0.72)
    );
    assert!(resized.node_ids().contains(&"node_pane_2".to_string()));
}

#[test]
fn spatial_focus_preserves_perpendicular_position() {
    let tree = split(
        SplitDirection::Vertical,
        vec![
            split(
                SplitDirection::Horizontal,
                vec![leaf("pane_1"), leaf("pane_3")],
            ),
            split(
                SplitDirection::Horizontal,
                vec![leaf("pane_2"), leaf("pane_4")],
            ),
        ],
    );

    assert_eq!(
        tree.adjacent_pane_id("pane_3", SpatialFocusDirection::Right)
            .as_deref(),
        Some("pane_4")
    );
    assert_eq!(
        tree.adjacent_pane_id("pane_4", SpatialFocusDirection::Left)
            .as_deref(),
        Some("pane_3")
    );
    assert_eq!(
        tree.adjacent_pane_id("pane_1", SpatialFocusDirection::Left),
        None
    );
}

#[test]
fn removing_and_attaching_panes_preserves_binary_tree_shape() {
    let tree = split(
        SplitDirection::Vertical,
        vec![leaf("pane_1"), leaf("pane_2")],
    );

    let attached = tree.attach_pane(
        "pane_3",
        SplitDirection::Vertical,
        "node_nested_split",
        "node_pane_3",
    );
    assert_eq!(attached.children.as_ref().unwrap().len(), 2);
    assert_eq!(attached.pane_ids(), vec!["pane_1", "pane_2", "pane_3"]);
    assert_eq!(
        attached.children.as_ref().unwrap()[1].kind,
        PaneTreeKind::Split
    );

    let removed = attached.remove_pane("pane_2").unwrap();
    assert_eq!(removed.pane_ids(), vec!["pane_1", "pane_3"]);
    assert_eq!(removed.children.as_ref().unwrap().len(), 2);
}

#[test]
fn split_tree_decode_requires_persisted_split_ratio() {
    let missing_ratio = serde_json::json!({
        "node_id": "node_split",
        "kind": "split",
        "direction": "vertical",
        "children": [
            {"node_id": "node_pane_1", "kind": "pane", "pane_id": "pane_1"},
            {"node_id": "node_pane_2", "kind": "pane", "pane_id": "pane_2"}
        ]
    });

    let error = serde_json::from_value::<PaneTreeNode>(missing_ratio).unwrap_err();

    assert!(error.to_string().contains("ratio"));
}

#[test]
fn workspace_model_serializes_platform_neutral_identity_fields() {
    let tree = leaf("pane_1");
    let tab = Tab {
        tab_id: "tab_main".to_string(),
        kind: TabKind::Terminal,
        title: Some("Terminal".to_string()),
        pane_tree: tree,
        zoomed_pane_id: None,
        is_pinned: false,
        is_title_user_locked: false,
    };
    let state = WorkspaceState {
        contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        focused_space_id: Some("space_main".to_string()),
        focused_tab_id: Some("tab_main".to_string()),
        focused_pane_id: Some("pane_1".to_string()),
        spaces: vec![Space {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            attention: ShellAttentionState::Active,
            tabs: vec![tab],
            selected_tab_id: Some("tab_main".to_string()),
            terminal_profile_id: Some("profile-main".to_string()),
            presentation_icon: None,
        }],
        pane_slots: vec![PaneSlot {
            pane_slot_id: "pane_1".to_string(),
            tab_id: "tab_main".to_string(),
            space_id: "space_main".to_string(),
            content_id: "terminal:pane_1".to_string(),
            attention: ShellAttentionState::Active,
        }],
        contents: vec![ContentInstance {
            content_id: "terminal:pane_1".to_string(),
            kind: ContentKind::Terminal,
            title: "Terminal".to_string(),
            icon_name: None,
            capabilities: ContentKind::Terminal.default_capabilities(),
            payload: ShellContentPayload::terminal(
                ShellLaunchTarget::Shell,
                None,
                Some("Terminal"),
            ),
            terminal_metadata: None,
            lifecycle: ContentLifecycleState::Active,
        }],
    };

    let encoded = serde_json::to_value(&state).unwrap();

    assert_eq!(encoded["window_id"], "window_main");
    assert_eq!(encoded["focused_pane_id"], "pane_1");
    assert_eq!(encoded["spaces"][0]["terminal_profile_id"], "profile-main");
    assert_eq!(encoded["pane_slots"][0]["content_id"], "terminal:pane_1");
    assert!(encoded["contents"][0].get("renderer_state").is_none());
}
