use crate::{
    ContentCapability, ContentInstance, ContentKind, ContentLifecycleState, PaneSlot, PaneTreeKind,
    PaneTreeNode, ShellAttentionState, ShellContentPayload, ShellLaunchTarget,
    ShellTabActiveTaskState, Space, SplitDirection, Tab, TabKind, TerminalRuntimeMetadata,
    WorkspaceState,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Legacy terminal-only workspace manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellWorkspaceManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Window id.
    pub window_id: String,
    /// Selected Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_space_id: Option<String>,
    /// Selected Tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_tab_id: Option<String>,
    /// Spaces.
    pub spaces: Vec<ShellWorkspaceSpaceRecord>,
}

impl ShellWorkspaceManifest {
    /// Migrates legacy terminal pane restore snapshots into content-container snapshots.
    pub fn migrating_terminal_restore_snapshots_to_content_containers(
        &self,
    ) -> ShellContentWorkspaceManifest {
        ShellContentWorkspaceManifest {
            schema_version: self.schema_version,
            content_contract_version:
                ShellContentWorkspaceManifest::CURRENT_CONTENT_CONTRACT_VERSION.to_string(),
            window_id: self.window_id.clone(),
            selected_space_id: self.selected_space_id.clone(),
            selected_tab_id: self.selected_tab_id.clone(),
            spaces: self
                .spaces
                .iter()
                .map(ShellContentWorkspaceSpaceRecord::from_legacy)
                .collect(),
            legacy_quick_terminal: None,
        }
    }
}

/// Legacy terminal-only Space manifest record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellWorkspaceSpaceRecord {
    /// Space id.
    pub space_id: String,
    /// Space title.
    pub title: String,
    /// Sort order.
    pub order: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
    /// Space-local selected tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_tab_id: Option<String>,
    /// Tab records.
    pub tabs: Vec<ShellWorkspaceTabRecord>,
    /// Terminal Profile id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_profile_id: Option<String>,
    /// Optional presentation icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_icon: Option<String>,
}

/// Legacy terminal-only tab manifest record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellWorkspaceTabRecord {
    /// Tab id.
    pub tab_id: String,
    /// Optional title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Tab kind.
    pub kind: TabKind,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last activation time.
    pub last_activated_at: DateTime<Utc>,
    /// Last activity time.
    pub last_activity_at: DateTime<Utc>,
    /// Whether tab is pinned.
    pub is_pinned: bool,
    /// Whether the title is user locked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_title_user_locked: Option<bool>,
    /// Pinned restore snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_snapshot: Option<ShellTabRestoreSnapshot>,
    /// Live restore snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_snapshot: Option<ShellTabRestoreSnapshot>,
    /// Active task state.
    #[serde(default)]
    pub active_task: ShellTabActiveTaskState,
}

/// Legacy terminal-only tab restore snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellTabRestoreSnapshot {
    /// Legacy pane tree.
    pub pane_tree: PaneTreeNode,
    /// Legacy terminal pane restore records.
    pub panes: Vec<ShellPaneRestoreRecord>,
}

impl ShellTabRestoreSnapshot {
    fn migrating_terminal_panes_to_content_containers(&self) -> ShellContentTabRestoreSnapshot {
        ShellContentTabRestoreSnapshot {
            pane_tree: self.pane_tree.clone(),
            pane_slots: self
                .panes
                .iter()
                .map(|pane| ShellPaneSlotRestoreRecord {
                    pane_slot_id: pane.pane_id.clone(),
                    content_id: content_id_for_pane_id(&pane.pane_id),
                })
                .collect(),
            contents: self
                .panes
                .iter()
                .map(|pane| {
                    let title = pane.title.clone().unwrap_or_else(|| "Shell".to_string());
                    ShellContentRestoreRecord {
                        content_id: content_id_for_pane_id(&pane.pane_id),
                        kind: ContentKind::Terminal,
                        title: title.clone(),
                        payload: ShellContentPayload::terminal_with_profile(
                            pane.launch_target,
                            pane.cwd.as_deref(),
                            Some(&title),
                            pane.terminal_profile_id.as_deref(),
                        ),
                    }
                })
                .collect(),
        }
    }
}

/// Legacy terminal pane restore record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellPaneRestoreRecord {
    /// Pane id.
    pub pane_id: String,
    /// Launch target.
    pub launch_target: ShellLaunchTarget,
    /// Restored current working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Restored terminal title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Terminal Profile id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_profile_id: Option<String>,
}

/// Content record stored in a restore snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellContentRestoreRecord {
    /// Content id.
    pub content_id: String,
    /// Content kind.
    pub kind: ContentKind,
    /// Display title.
    pub title: String,
    /// Restore payload.
    pub payload: ShellContentPayload,
}

/// Pane slot restore record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellPaneSlotRestoreRecord {
    /// Pane slot id.
    pub pane_slot_id: String,
    /// Mounted content id.
    pub content_id: String,
}

/// Tab restore snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellContentTabRestoreSnapshot {
    /// Pane tree.
    #[serde(with = "manifest_pane_tree")]
    pub pane_tree: PaneTreeNode,
    /// Pane slot restore records.
    pub pane_slots: Vec<ShellPaneSlotRestoreRecord>,
    /// Content restore records.
    pub contents: Vec<ShellContentRestoreRecord>,
}

impl ShellContentTabRestoreSnapshot {
    fn overlaying_terminal_transcript_snapshots(
        &self,
        live_snapshot: Option<&ShellContentTabRestoreSnapshot>,
    ) -> Self {
        let Some(live_snapshot) = live_snapshot else {
            return self.clone();
        };
        let transcripts_by_content_id: BTreeMap<_, _> = live_snapshot
            .contents
            .iter()
            .filter_map(|content| {
                let transcript = content
                    .payload
                    .terminal
                    .as_ref()?
                    .transcript_snapshot
                    .clone()?;
                Some((content.content_id.clone(), transcript))
            })
            .collect();
        if transcripts_by_content_id.is_empty() {
            return self.clone();
        }

        let mut restored = self.clone();
        for content in &mut restored.contents {
            let Some(transcript) = transcripts_by_content_id.get(&content.content_id) else {
                continue;
            };
            let Some(terminal) = &mut content.payload.terminal else {
                continue;
            };
            terminal.transcript_snapshot = Some(transcript.clone());
        }
        restored
    }
}

/// Workspace tab manifest record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellContentWorkspaceTabRecord {
    /// Tab id.
    pub tab_id: String,
    /// Optional title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Tab kind.
    pub kind: TabKind,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last activation time.
    pub last_activated_at: DateTime<Utc>,
    /// Last activity time.
    pub last_activity_at: DateTime<Utc>,
    /// Whether tab is pinned.
    pub is_pinned: bool,
    /// Whether the title is user locked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_title_user_locked: Option<bool>,
    /// Pinned restore snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_snapshot: Option<ShellContentTabRestoreSnapshot>,
    /// Live restore snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_snapshot: Option<ShellContentTabRestoreSnapshot>,
    /// Active task state.
    #[serde(default)]
    pub active_task: ShellTabActiveTaskState,
}

impl ShellContentWorkspaceTabRecord {
    fn restore_snapshot(&self, default_working_directory: &str) -> ShellContentTabRestoreSnapshot {
        if self.is_pinned
            && let Some(snapshot) = &self.pin_snapshot
        {
            return snapshot.overlaying_terminal_transcript_snapshots(self.live_snapshot.as_ref());
        }
        if let Some(snapshot) = &self.live_snapshot {
            return snapshot.clone();
        }

        let pane_slot_id = format!("pane_{}", self.tab_id);
        let content_id = format!("content_{pane_slot_id}");
        let title = self.title.clone().unwrap_or_else(|| "Shell".to_string());
        ShellContentTabRestoreSnapshot {
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            pane_slots: vec![ShellPaneSlotRestoreRecord {
                pane_slot_id,
                content_id: content_id.clone(),
            }],
            contents: vec![ShellContentRestoreRecord {
                content_id,
                kind: ContentKind::Terminal,
                title: title.clone(),
                payload: ShellContentPayload::terminal(
                    ShellLaunchTarget::Shell,
                    Some(default_working_directory),
                    Some(&title),
                ),
            }],
        }
    }

    fn should_retain(&self, now: DateTime<Utc>, ttl: Duration) -> bool {
        self.is_pinned
            || self.active_task.protects_from_pruning()
            || now - self.last_activated_at.max(self.last_activity_at) <= ttl
    }
}

/// Workspace space manifest record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellContentWorkspaceSpaceRecord {
    /// Space id.
    pub space_id: String,
    /// Space title.
    pub title: String,
    /// Sort order.
    pub order: i32,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last update time.
    pub updated_at: DateTime<Utc>,
    /// Space-local selected tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_tab_id: Option<String>,
    /// Tab records.
    pub tabs: Vec<ShellContentWorkspaceTabRecord>,
    /// Terminal Profile id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_profile_id: Option<String>,
    /// Optional presentation icon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_icon: Option<String>,
}

impl ShellContentWorkspaceSpaceRecord {
    fn from_legacy(space: &ShellWorkspaceSpaceRecord) -> Self {
        Self {
            space_id: space.space_id.clone(),
            title: space.title.clone(),
            order: space.order,
            created_at: space.created_at,
            updated_at: space.updated_at,
            selected_tab_id: space.selected_tab_id.clone(),
            tabs: space
                .tabs
                .iter()
                .map(ShellContentWorkspaceTabRecord::from_legacy)
                .collect(),
            terminal_profile_id: space.terminal_profile_id.clone(),
            presentation_icon: space.presentation_icon.clone(),
        }
    }
}

impl ShellContentWorkspaceTabRecord {
    fn from_legacy(tab: &ShellWorkspaceTabRecord) -> Self {
        Self {
            tab_id: tab.tab_id.clone(),
            title: tab.title.clone(),
            kind: tab.kind,
            created_at: tab.created_at,
            last_activated_at: tab.last_activated_at,
            last_activity_at: tab.last_activity_at,
            is_pinned: tab.is_pinned,
            is_title_user_locked: tab.is_title_user_locked,
            pin_snapshot: tab
                .pin_snapshot
                .as_ref()
                .map(ShellTabRestoreSnapshot::migrating_terminal_panes_to_content_containers),
            live_snapshot: tab
                .live_snapshot
                .as_ref()
                .map(ShellTabRestoreSnapshot::migrating_terminal_panes_to_content_containers),
            active_task: tab.active_task,
        }
    }
}

/// Legacy quick-terminal presentation state decoded only to discard old manifest data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyQuickTerminalPresentation {
    /// Legacy quick terminal was visible.
    Visible,
    /// Legacy quick terminal was hidden.
    Hidden,
}

/// Legacy quick terminal restore record decoded only to discard old manifest data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyQuickTerminalRestoreRecord {
    /// Pane id.
    pub pane_id: String,
    /// Persisted presentation state.
    #[serde(default = "default_legacy_quick_terminal_presentation")]
    pub presentation: LegacyQuickTerminalPresentation,
    /// Last working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_working_directory: Option<String>,
    /// Live restore snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_snapshot: Option<ShellContentTabRestoreSnapshot>,
    /// Active task state.
    #[serde(default)]
    pub active_task: ShellTabActiveTaskState,
}

/// Content-container workspace manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellContentWorkspaceManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Content contract version.
    pub content_contract_version: String,
    /// Window id.
    pub window_id: String,
    /// Selected Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_space_id: Option<String>,
    /// Selected Tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_tab_id: Option<String>,
    /// Spaces.
    pub spaces: Vec<ShellContentWorkspaceSpaceRecord>,
    /// Legacy quick-terminal restore record. Decoded for load tolerance and omitted on write.
    #[serde(default, rename = "quick_terminal", skip_serializing)]
    pub legacy_quick_terminal: Option<LegacyQuickTerminalRestoreRecord>,
}

impl ShellContentWorkspaceManifest {
    /// Current manifest schema version.
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;
    /// Current shell content contract version.
    pub const CURRENT_CONTENT_CONTRACT_VERSION: &'static str = "0.2";

    /// Creates a default content-container manifest.
    pub fn default_manifest(window_id: &str, default_working_directory: &str, now: &str) -> Self {
        let now = parse_manifest_time(now);
        let pane_slot_id = "pane_1".to_string();
        let content_id = "content_pane_1".to_string();
        let snapshot = ShellContentTabRestoreSnapshot {
            pane_tree: PaneTreeNode::pane("node_pane_1", &pane_slot_id),
            pane_slots: vec![ShellPaneSlotRestoreRecord {
                pane_slot_id: pane_slot_id.clone(),
                content_id: content_id.clone(),
            }],
            contents: vec![ShellContentRestoreRecord {
                content_id,
                kind: ContentKind::Terminal,
                title: "Shell".to_string(),
                payload: ShellContentPayload::terminal(
                    ShellLaunchTarget::Shell,
                    Some(default_working_directory),
                    Some("Shell"),
                ),
            }],
        };

        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION,
            content_contract_version: Self::CURRENT_CONTENT_CONTRACT_VERSION.to_string(),
            window_id: window_id.to_string(),
            selected_space_id: Some("space_main".to_string()),
            selected_tab_id: Some("tab_main".to_string()),
            spaces: vec![ShellContentWorkspaceSpaceRecord {
                space_id: "space_main".to_string(),
                title: "Terminal".to_string(),
                order: 0,
                created_at: now,
                updated_at: now,
                selected_tab_id: Some("tab_main".to_string()),
                tabs: vec![ShellContentWorkspaceTabRecord {
                    tab_id: "tab_main".to_string(),
                    title: Some("Shell".to_string()),
                    kind: TabKind::Terminal,
                    created_at: now,
                    last_activated_at: now,
                    last_activity_at: now,
                    is_pinned: false,
                    is_title_user_locked: Some(false),
                    pin_snapshot: None,
                    live_snapshot: Some(snapshot),
                    active_task: ShellTabActiveTaskState::Inactive,
                }],
                terminal_profile_id: None,
                presentation_icon: None,
            }],
            legacy_quick_terminal: None,
        }
    }

    /// Repairs global and Space-local selection.
    pub fn repair_selection(&mut self) {
        if self.spaces.is_empty() {
            self.selected_space_id = None;
            self.selected_tab_id = None;
            return;
        }

        if self
            .selected_space_id
            .as_ref()
            .is_none_or(|selected| !self.spaces.iter().any(|space| space.space_id == *selected))
        {
            self.selected_space_id = self.spaces.first().map(|space| space.space_id.clone());
        }

        for space in &mut self.spaces {
            let legacy_selected_tab_id = (Some(&space.space_id) == self.selected_space_id.as_ref())
                .then(|| self.selected_tab_id.clone())
                .flatten();
            space.selected_tab_id = repaired_selected_tab_id(
                space.selected_tab_id.clone(),
                legacy_selected_tab_id,
                &space.tabs,
            );
        }

        self.selected_tab_id = self
            .selected_space_id
            .as_ref()
            .and_then(|space_id| self.spaces.iter().find(|space| space.space_id == *space_id))
            .and_then(|space| space.selected_tab_id.clone());
    }

    /// Returns a manifest with expired unpinned inactive tabs pruned.
    pub fn pruning_expired_tabs(&self, now: &str, ttl_seconds: i64) -> Self {
        let now = parse_manifest_time(now);
        let ttl = Duration::seconds(ttl_seconds.max(0));
        let mut pruned = self.clone();
        for space in &mut pruned.spaces {
            space.tabs.retain(|tab| tab.should_retain(now, ttl));
            space.updated_at = now;
        }
        pruned.repair_selection();
        pruned
    }

    /// Materializes this manifest into portable workspace state.
    pub fn materialize(&self, default_working_directory: &str, now: &str) -> WorkspaceState {
        let mut manifest = self.clone();
        manifest.repair_selection();
        if manifest.spaces.is_empty() {
            manifest = Self::default_manifest(&self.window_id, default_working_directory, now);
        }
        let source_tab_count = manifest
            .spaces
            .iter()
            .map(|space| space.tabs.len())
            .sum::<usize>();

        let state = Self::materialize_resolved_manifest(manifest, default_working_directory);
        if !state.pane_slots.is_empty() || source_tab_count == 0 {
            return state;
        }

        // A non-empty manifest whose tabs all reference missing content materializes into a
        // workspace with no terminal panes. Recover with a default terminal instead of opening
        // empty, matching the pre-shell-core Swift materializer's fallback behavior.
        let fallback = Self::default_manifest(&self.window_id, default_working_directory, now);
        Self::materialize_resolved_manifest(fallback, default_working_directory)
    }

    fn materialize_resolved_manifest(
        manifest: Self,
        default_working_directory: &str,
    ) -> WorkspaceState {
        let mut spaces = manifest.spaces.clone();
        spaces.sort_by(|lhs, rhs| {
            lhs.order
                .cmp(&rhs.order)
                .then(lhs.space_id.cmp(&rhs.space_id))
        });

        let mut pane_slots = Vec::new();
        let mut contents = Vec::new();
        let materialized_spaces = spaces
            .into_iter()
            .map(|space| {
                let mut tabs = organized_tabs(space.tabs)
                    .into_iter()
                    .filter_map(|tab_record| {
                        let snapshot = tab_record.restore_snapshot(default_working_directory);
                        let valid_content_ids = snapshot
                            .contents
                            .iter()
                            .map(|content| content.content_id.clone())
                            .collect::<BTreeSet<_>>();
                        let snapshot_pane_slots = snapshot
                            .pane_slots
                            .into_iter()
                            .filter(|slot| valid_content_ids.contains(&slot.content_id))
                            .map(|slot| PaneSlot {
                                pane_slot_id: slot.pane_slot_id,
                                tab_id: tab_record.tab_id.clone(),
                                space_id: space.space_id.clone(),
                                content_id: slot.content_id,
                                attention: if Some(&tab_record.tab_id)
                                    == manifest.selected_tab_id.as_ref()
                                {
                                    ShellAttentionState::Active
                                } else {
                                    ShellAttentionState::Idle
                                },
                            })
                            .collect::<Vec<_>>();
                        if snapshot_pane_slots.is_empty() {
                            return None;
                        }
                        pane_slots.extend(snapshot_pane_slots);
                        contents.extend(snapshot.contents.into_iter().map(|content| {
                            restored_content_instance(content, default_working_directory)
                        }));
                        Some(Tab {
                            tab_id: tab_record.tab_id,
                            kind: tab_record.kind,
                            title: tab_record.title,
                            pane_tree: snapshot.pane_tree,
                            zoomed_pane_id: None,
                            is_pinned: tab_record.is_pinned,
                            is_title_user_locked: tab_record.is_title_user_locked == Some(true),
                        })
                    })
                    .collect::<Vec<_>>();

                tabs = organized_runtime_tabs(tabs);
                let attention = strongest_attention(&pane_slots, &space.space_id);
                Space {
                    space_id: space.space_id,
                    title: space.title,
                    attention,
                    tabs,
                    selected_tab_id: space.selected_tab_id,
                    terminal_profile_id: space.terminal_profile_id,
                    presentation_icon: space.presentation_icon,
                }
            })
            .collect::<Vec<_>>();

        // Repair focus so it never points at a tab dropped during materialization. The selected
        // tab may have been filtered out above for having no valid pane slots; in that case fall
        // back to the selected Space's surviving selected/first tab, keeping focus within that
        // Space (a legitimately empty selected Space still resolves to no focused tab/pane).
        let focused_space = manifest.selected_space_id.as_ref().and_then(|space_id| {
            materialized_spaces
                .iter()
                .find(|space| &space.space_id == space_id)
        });
        let focused_tab_id = manifest
            .selected_tab_id
            .clone()
            .filter(|selected| materialized_tab_exists(&materialized_spaces, selected))
            .or_else(|| {
                focused_space.and_then(|space| {
                    space
                        .selected_tab_id
                        .clone()
                        .filter(|selected| space.tabs.iter().any(|tab| &tab.tab_id == selected))
                        .or_else(|| space.tabs.first().map(|tab| tab.tab_id.clone()))
                })
            });
        let focused_pane_id = focused_tab_id.as_ref().and_then(|tab_id| {
            materialized_spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .find(|tab| &tab.tab_id == tab_id)
                .and_then(|tab| tab.pane_tree.pane_ids().first().cloned())
        });
        WorkspaceState {
            contract_version: manifest.content_contract_version,
            window_id: manifest.window_id,
            focused_space_id: manifest.selected_space_id,
            focused_tab_id,
            focused_pane_id,
            spaces: materialized_spaces,
            pane_slots,
            contents,
        }
    }
}

fn materialized_tab_exists(spaces: &[Space], tab_id: &str) -> bool {
    spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .any(|tab| tab.tab_id == tab_id)
}

fn repaired_selected_tab_id(
    selected_tab_id: Option<String>,
    legacy_selected_tab_id: Option<String>,
    tabs: &[ShellContentWorkspaceTabRecord],
) -> Option<String> {
    selected_tab_id
        .filter(|candidate| tabs.iter().any(|tab| tab.tab_id == *candidate))
        .or_else(|| {
            legacy_selected_tab_id
                .filter(|candidate| tabs.iter().any(|tab| tab.tab_id == *candidate))
        })
        .or_else(|| tabs.first().map(|tab| tab.tab_id.clone()))
}

fn organized_tabs(
    tabs: Vec<ShellContentWorkspaceTabRecord>,
) -> Vec<ShellContentWorkspaceTabRecord> {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();
    for tab in tabs {
        if tab.is_pinned {
            pinned.push(tab);
        } else {
            unpinned.push(tab);
        }
    }
    pinned.extend(unpinned);
    pinned
}

fn organized_runtime_tabs(tabs: Vec<Tab>) -> Vec<Tab> {
    let mut pinned = Vec::new();
    let mut unpinned = Vec::new();
    for tab in tabs {
        if tab.is_pinned {
            pinned.push(tab);
        } else {
            unpinned.push(tab);
        }
    }
    pinned.extend(unpinned);
    pinned
}

fn restored_content_instance(
    record: ShellContentRestoreRecord,
    default_working_directory: &str,
) -> ContentInstance {
    let mut payload = record.payload;
    if record.kind == ContentKind::Terminal
        && let Some(terminal_payload) = &mut payload.terminal
        && terminal_payload.cwd.is_none()
        && terminal_payload.terminal_profile_id.is_none()
    {
        terminal_payload.cwd = Some(default_working_directory.to_string());
    }
    let terminal_metadata = payload
        .terminal
        .as_ref()
        .map(|payload| TerminalRuntimeMetadata {
            title: payload.title.clone(),
            cwd: payload.cwd.clone(),
            active_task_state: ShellTabActiveTaskState::Inactive,
            activity: None,
        });
    ContentInstance {
        content_id: record.content_id,
        kind: record.kind,
        title: record.title,
        icon_name: None,
        capabilities: match record.kind {
            ContentKind::Terminal => ContentKind::Terminal.default_capabilities(),
            ContentKind::Markdown => vec![ContentCapability::MarkdownReadOnlyViewer],
            ContentKind::Settings => vec![ContentCapability::SettingsSurface],
        },
        payload,
        terminal_metadata,
        lifecycle: ContentLifecycleState::Active,
    }
}

fn content_id_for_pane_id(pane_id: &str) -> String {
    format!("content_{pane_id}")
}

fn default_legacy_quick_terminal_presentation() -> LegacyQuickTerminalPresentation {
    LegacyQuickTerminalPresentation::Hidden
}

fn strongest_attention(pane_slots: &[PaneSlot], space_id: &str) -> ShellAttentionState {
    pane_slots
        .iter()
        .filter(|slot| slot.space_id == space_id)
        .map(|slot| slot.attention)
        .max_by_key(|attention| match attention {
            ShellAttentionState::Idle => 0,
            ShellAttentionState::Active => 1,
            ShellAttentionState::Notable => 2,
            ShellAttentionState::AwaitingUser => 3,
        })
        .unwrap_or(ShellAttentionState::Idle)
}

fn parse_manifest_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_space_attention_keeps_awaiting_user_above_notable() {
        let pane_slots = vec![
            pane_slot(
                "pane_awaiting",
                "space_main",
                ShellAttentionState::AwaitingUser,
            ),
            pane_slot("pane_notable", "space_main", ShellAttentionState::Notable),
            pane_slot("pane_other", "space_other", ShellAttentionState::Notable),
        ];

        assert_eq!(
            strongest_attention(&pane_slots, "space_main"),
            ShellAttentionState::AwaitingUser
        );
        assert_eq!(
            strongest_attention(&pane_slots, "space_other"),
            ShellAttentionState::Notable
        );
    }

    fn pane_slot(pane_slot_id: &str, space_id: &str, attention: ShellAttentionState) -> PaneSlot {
        PaneSlot {
            pane_slot_id: pane_slot_id.to_string(),
            tab_id: "tab_main".to_string(),
            space_id: space_id.to_string(),
            content_id: format!("content_{pane_slot_id}"),
            attention,
        }
    }
}

mod manifest_pane_tree {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(tree: &PaneTreeNode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ManifestPaneTreeNode::from(tree).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PaneTreeNode, D::Error>
    where
        D: Deserializer<'de>,
    {
        ManifestPaneTreeNode::deserialize(deserializer).map(Into::into)
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct ManifestPaneTreeNode {
        node_id: String,
        kind: PaneTreeKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        direction: Option<SplitDirection>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ratio: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pane_slot_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        children: Option<Vec<ManifestPaneTreeNode>>,
    }

    impl From<&PaneTreeNode> for ManifestPaneTreeNode {
        fn from(tree: &PaneTreeNode) -> Self {
            Self {
                node_id: tree.node_id.clone(),
                kind: tree.kind,
                direction: tree.direction,
                ratio: (tree.kind == PaneTreeKind::Split).then(|| tree.split_ratio()),
                pane_slot_id: tree.pane_id.clone(),
                children: tree
                    .children
                    .as_ref()
                    .map(|children| children.iter().map(Self::from).collect()),
            }
        }
    }

    impl From<ManifestPaneTreeNode> for PaneTreeNode {
        fn from(tree: ManifestPaneTreeNode) -> Self {
            match tree.kind {
                PaneTreeKind::Pane => {
                    PaneTreeNode::pane(tree.node_id, tree.pane_slot_id.unwrap_or_default())
                }
                PaneTreeKind::Split => PaneTreeNode::split_with_ratio(
                    tree.node_id,
                    tree.direction.unwrap_or(SplitDirection::Horizontal),
                    tree.ratio.unwrap_or(0.5),
                    tree.children
                        .unwrap_or_default()
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                ),
            }
        }
    }
}
