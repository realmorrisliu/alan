use alan_shell_core::{
    AgentAttachment, AgentContentPresentation, AgentProcessReference, AgentStreamOffsets,
    ContentKind, PaneTreeNode, ShellContentPayload, ShellContentRestoreRecord,
    ShellContentTabRestoreSnapshot, ShellContentWorkspaceManifest,
    ShellContentWorkspaceSpaceRecord, ShellContentWorkspaceTabRecord, ShellLaunchTarget,
    ShellPaneSlotRestoreRecord, ShellTabActiveTaskState, TabKind,
};
use chrono::{DateTime, Utc};
use serde_json::json;

const REFERENCE_TIME: &str = "2027-01-15T08:00:00Z";

#[test]
fn agent_attachment_persists_only_reference_offsets_and_presentation() {
    let payload = ShellContentPayload {
        agent: Some(AgentAttachment {
            process: AgentProcessReference {
                boot_id: "boot-a".to_string(),
                pid: 42,
            },
            offsets: AgentStreamOffsets {
                output: 10,
                requests: 20,
                actions: 30,
                ui: 40,
            },
            presentation: AgentContentPresentation {
                follows_output: true,
            },
        }),
        ..Default::default()
    };

    let encoded = serde_json::to_value(&payload).unwrap();
    assert_eq!(
        encoded,
        json!({
            "agent": {
                "process": { "boot_id": "boot-a", "pid": 42 },
                "offsets": { "output": 10, "requests": 20, "actions": 30, "ui": 40 },
                "presentation": { "follows_output": true }
            }
        })
    );
    let text = encoded.to_string();
    for forbidden in [
        "tape",
        "machine",
        "provider",
        "tool",
        "socket",
        "host_path",
        "secret",
    ] {
        assert!(
            !text.contains(forbidden),
            "manifest leaked forbidden Agent state: {forbidden}"
        );
    }

    let forbidden_agent_state = json!({
        "agent": {
            "process": { "boot_id": "boot-a", "pid": 42 },
            "offsets": { "output": 0, "requests": 0, "actions": 0, "ui": 0 },
            "presentation": { "follows_output": true },
            "tape": ["must not persist"]
        }
    });
    assert!(serde_json::from_value::<ShellContentPayload>(forbidden_agent_state).is_err());
    assert!(
        serde_json::from_value::<ShellContentPayload>(json!({
            "agent": null,
            "socket": "/tmp/authority.sock"
        }))
        .is_err()
    );
}

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

    let only_empty_manifest = ShellContentWorkspaceManifest {
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_empty".to_string(),
            title: "Empty".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: None,
            tabs: Vec::new(),
            terminal_profile_id: None,
            presentation_icon: None,
        }],
        ..manifest
    };
    let only_empty_state = only_empty_manifest.materialize("/fallback", REFERENCE_TIME);
    assert_eq!(only_empty_state.spaces.len(), 1);
    assert_eq!(
        only_empty_state.focused_space_id.as_deref(),
        Some("space_empty")
    );
    assert_eq!(only_empty_state.focused_tab_id, None);
    assert_eq!(only_empty_state.focused_pane_id, None);
    assert!(only_empty_state.pane_slots.is_empty());
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
fn unknown_active_task_decodes_conservatively_and_remains_pruning_protected() {
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_future_task".to_string()),
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_future_task".to_string()),
            tabs: vec![content_tab("tab_future_task", "Future Task", "/future")],
            terminal_profile_id: None,
            presentation_icon: None,
        }],
    };
    let mut encoded = serde_json::to_value(manifest).expect("encode manifest");
    encoded["spaces"][0]["tabs"][0]["active_task"] =
        serde_json::Value::String("future_agent_state".to_string());

    let decoded: ShellContentWorkspaceManifest =
        serde_json::from_value(encoded).expect("decode unknown active task");

    assert_eq!(
        decoded.spaces[0].tabs[0].active_task,
        ShellTabActiveTaskState::Unknown
    );
    assert_eq!(
        decoded
            .materialize("/fallback", REFERENCE_TIME)
            .spaces
            .len(),
        1
    );
    assert_eq!(
        decoded
            .pruning_expired_tabs("2027-01-16T08:00:00Z", 60)
            .spaces[0]
            .tabs[0]
            .tab_id,
        "tab_future_task"
    );
    assert_eq!(
        serde_json::to_value(decoded).expect("re-encode manifest")["spaces"][0]["tabs"][0]["active_task"],
        "unknown"
    );
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
fn current_manifest_decoder_rejects_retired_and_unknown_shapes() {
    let current =
        ShellContentWorkspaceManifest::default_manifest("window_main", "/repo/app", REFERENCE_TIME);
    let mut quick_terminal = serde_json::to_value(&current).expect("encode current manifest");
    quick_terminal["quick_terminal"] = serde_json::json!({"presentation": "hidden"});
    assert!(
        serde_json::from_value::<ShellContentWorkspaceManifest>(quick_terminal).is_err(),
        "retired quick-terminal content must be rejected"
    );

    let terminal_only = serde_json::json!({
        "schema_version": 1,
        "window_id": "window_main",
        "selected_space_id": null,
        "selected_tab_id": null,
        "spaces": []
    });
    assert!(
        serde_json::from_value::<ShellContentWorkspaceManifest>(terminal_only).is_err(),
        "terminal-only manifests must be rejected"
    );

    let mut unknown = serde_json::to_value(current).expect("encode current manifest");
    unknown["unknown_restore_surface"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<ShellContentWorkspaceManifest>(unknown).is_err(),
        "unknown current-manifest fields must fail closed"
    );
}

#[test]
fn materialize_recovers_default_terminal_when_no_panes_survive() {
    // A tab whose pane slot references content that is not present materializes to no panes.
    // With it as the only tab, the whole workspace would otherwise open empty.
    let corrupt_tab = ShellContentWorkspaceTabRecord {
        live_snapshot: Some(ShellContentTabRestoreSnapshot {
            pane_tree: PaneTreeNode::pane("node_pane_corrupt", "pane_corrupt"),
            pane_slots: vec![ShellPaneSlotRestoreRecord {
                pane_slot_id: "pane_corrupt".to_string(),
                content_id: "content_missing".to_string(),
            }],
            contents: Vec::new(),
        }),
        ..content_tab("tab_corrupt", "Corrupt", "/corrupt")
    };
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_corrupt".to_string()),
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_corrupt".to_string()),
            tabs: vec![corrupt_tab],
            terminal_profile_id: None,
            presentation_icon: None,
        }],
    };

    let state = manifest.materialize("/fallback", REFERENCE_TIME);

    assert!(
        !state.pane_slots.is_empty(),
        "a manifest that materializes no panes must recover a default terminal"
    );
    assert!(
        state.focused_pane_id.is_some(),
        "recovered workspace must focus a pane"
    );
}

#[test]
fn materialize_repairs_focus_when_selected_tab_is_filtered_out() {
    // The selected tab's snapshot references missing content, so it is filtered out during
    // materialization while a sibling tab survives.
    let dropped_selected_tab = ShellContentWorkspaceTabRecord {
        live_snapshot: Some(ShellContentTabRestoreSnapshot {
            pane_tree: PaneTreeNode::pane("node_pane_dropped", "pane_dropped"),
            pane_slots: vec![ShellPaneSlotRestoreRecord {
                pane_slot_id: "pane_dropped".to_string(),
                content_id: "content_missing".to_string(),
            }],
            contents: Vec::new(),
        }),
        ..content_tab("tab_selected", "Selected", "/selected")
    };
    let manifest = ShellContentWorkspaceManifest {
        schema_version: 1,
        content_contract_version: "0.2".to_string(),
        window_id: "window_main".to_string(),
        selected_space_id: Some("space_main".to_string()),
        selected_tab_id: Some("tab_selected".to_string()),
        spaces: vec![ShellContentWorkspaceSpaceRecord {
            space_id: "space_main".to_string(),
            title: "Main".to_string(),
            order: 0,
            created_at: reference_time(),
            updated_at: reference_time(),
            selected_tab_id: Some("tab_selected".to_string()),
            tabs: vec![
                dropped_selected_tab,
                content_tab("tab_valid", "Valid", "/valid"),
            ],
            terminal_profile_id: None,
            presentation_icon: None,
        }],
    };

    let state = manifest.materialize("/fallback", REFERENCE_TIME);

    assert!(
        state.spaces[0]
            .tabs
            .iter()
            .all(|tab| tab.tab_id != "tab_selected"),
        "the invalid selected tab must be filtered out"
    );
    assert_eq!(
        state.focused_tab_id.as_deref(),
        Some("tab_valid"),
        "focus must repair to the surviving sibling tab, not point at the dropped tab"
    );
    assert!(
        state
            .focused_pane_id
            .as_deref()
            .map(|pane_id| state
                .pane_slots
                .iter()
                .any(|slot| slot.pane_slot_id == pane_id))
            .unwrap_or(false),
        "repaired focus must resolve a usable focused pane present in the workspace"
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
                pane_slot_id,
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
