mod content;

use crate::{
    AgentContentPresentation, AgentStreamOffsets, ContentInstance, ContentKind,
    ContentLifecycleState, PaneSlot, PaneTreeNode, PaneTreeNodeResizeOutcome, ShellAttentionState,
    ShellContentPayload, ShellLaunchTarget, ShellTabActiveTaskState, Space, SpatialFocusDirection,
    SplitDirection, SplitPlacement, Tab, TabKind, TabOrganizationSection, TerminalActivitySnapshot,
    WorkspaceState,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Stable reducer operation accepted by the platform-neutral workspace core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReducerOperation {
    /// Focus a pane slot directly.
    FocusPane {
        /// Target pane slot id.
        pane_slot_id: String,
    },
    /// Move focus spatially from the currently focused pane.
    FocusAdjacentPane {
        /// Spatial direction.
        direction: SpatialFocusDirection,
    },
    /// Select a Space and focus its selected or first pane.
    SelectSpace {
        /// Target Space id.
        space_id: String,
    },
    /// Select a Tab and focus its current or first pane.
    SelectTab {
        /// Target Tab id.
        tab_id: String,
    },
    /// Set or clear the default Terminal Profile for a Space.
    SetTerminalProfile {
        /// Target Space id.
        space_id: String,
        /// Optional Terminal Profile id.
        terminal_profile_id: Option<String>,
    },
    /// Set, clear, or validate the presentation icon for a Space.
    SetPresentationIcon {
        /// Target Space id.
        space_id: String,
        /// Optional presentation icon system name.
        presentation_icon: Option<String>,
    },
    /// Delete a Space and repair focus.
    DeleteSpace {
        /// Target Space id.
        space_id: String,
        /// Working directory used when deleting the final Space bootstraps the default shell.
        default_working_directory: Option<String>,
    },
    /// Create a new Space containing one terminal tab.
    CreateTerminalSpace {
        /// Optional Space title.
        title: Option<String>,
        /// Optional initial tab title.
        tab_title: Option<String>,
        /// Optional working directory for the terminal runtime.
        working_directory: Option<String>,
        /// Optional Terminal Profile id.
        terminal_profile_id: Option<String>,
        /// Optional presentation icon system name.
        presentation_icon: Option<String>,
        /// Pane slot ids already reserved by the platform terminal runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Open a terminal tab in a Space.
    OpenTerminalTab {
        /// Target Space id. Defaults to focused or first Space.
        space_id: Option<String>,
        /// Optional tab/content title.
        title: Option<String>,
        /// Optional working directory for the terminal runtime.
        working_directory: Option<String>,
        /// Optional Terminal Profile id.
        terminal_profile_id: Option<String>,
        /// Pane slot ids already reserved by the platform terminal runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Open a non-terminal content tab in a Space.
    OpenContentTab {
        /// Target Space id. Defaults to focused or first Space.
        space_id: Option<String>,
        /// Mounted content kind.
        kind: ContentKind,
        /// User-facing tab and content title.
        title: String,
        /// Portable content restore payload.
        payload: ShellContentPayload,
        /// Pane slot ids already reserved by the platform runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Duplicate a terminal-backed tab next to the source tab.
    DuplicateTab {
        /// Source Tab id.
        tab_id: String,
        /// Pane slot ids already reserved by the platform terminal runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Move a tab within its pinned or unpinned section by an offset.
    MoveTab {
        /// Tab id.
        tab_id: String,
        /// Relative section offset.
        section_offset: isize,
    },
    /// Move a tab to another Space, preserving its pinned/unpinned section.
    MoveTabToSpace {
        /// Tab id.
        tab_id: String,
        /// Target Space id.
        target_space_id: String,
    },
    /// Organize a tab into a target Space section at an absolute section index.
    OrganizeTab {
        /// Tab id.
        tab_id: String,
        /// Target Space id. Defaults to the source Space.
        target_space_id: Option<String>,
        /// Target pinned or unpinned section.
        section: TabOrganizationSection,
        /// Absolute index in the target section. Defaults to the section tail.
        index: Option<usize>,
    },
    /// Clear inactive unpinned tabs in a Space, preserving selected and protected tabs.
    ClearInactiveTemporaryTabs {
        /// Target Space id.
        space_id: String,
        /// Tab ids protected by active task state.
        #[serde(default)]
        protected_tab_ids: Vec<String>,
    },
    /// Split a pane slot with a new terminal-backed pane slot.
    SplitPane {
        /// Existing pane slot id.
        pane_slot_id: String,
        /// Requested placement.
        placement: SplitPlacement,
        /// Optional terminal title.
        title: Option<String>,
        /// Optional working directory for the terminal runtime.
        working_directory: Option<String>,
        /// Optional Terminal Profile id.
        terminal_profile_id: Option<String>,
        /// Pane slot ids already reserved by the platform terminal runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Split a pane slot with a new non-terminal content pane slot.
    SplitContentPane {
        /// Existing pane slot id.
        pane_slot_id: String,
        /// Requested placement.
        placement: SplitPlacement,
        /// Mounted content kind.
        kind: ContentKind,
        /// User-facing content title.
        title: String,
        /// Portable content restore payload.
        payload: ShellContentPayload,
        /// Pane slot ids already reserved by the platform runtime.
        #[serde(default)]
        reserved_pane_slot_ids: Vec<String>,
    },
    /// Resize a split tree node.
    ResizeSplit {
        /// Split node id.
        split_node_id: String,
        /// Requested split ratio.
        ratio: f64,
    },
    /// Equalize every split ratio in a tab.
    EqualizeSplits {
        /// Target tab id. Defaults to focused tab.
        tab_id: Option<String>,
    },
    /// Zoom a pane within its split tab.
    ZoomPane {
        /// Target pane slot id.
        pane_slot_id: String,
    },
    /// Clear zoom for a tab.
    UnzoomTab {
        /// Target tab id. Defaults to focused tab.
        tab_id: Option<String>,
    },
    /// Close a pane slot.
    ClosePane {
        /// Pane slot id.
        pane_slot_id: String,
    },
    /// Move a pane from a split tab into a new adjacent tab.
    MovePaneToNewTab {
        /// Pane slot id.
        pane_slot_id: String,
        /// Optional title for the new tab.
        title: Option<String>,
    },
    /// Move a pane into another tab as a split.
    MovePaneToTab {
        /// Pane slot id.
        pane_slot_id: String,
        /// Target Tab id.
        target_tab_id: String,
        /// Split direction used to attach the moved pane.
        direction: SplitDirection,
    },
    /// Move a pane within its current tab.
    MovePaneWithinTab {
        /// Pane slot id.
        pane_slot_id: String,
        /// Requested placement relative to its adjacent target.
        placement: SplitPlacement,
    },
    /// Close a tab.
    CloseTab {
        /// Tab id.
        tab_id: String,
    },
    /// Pin a tab.
    PinTab {
        /// Tab id.
        tab_id: String,
    },
    /// Unpin a tab.
    UnpinTab {
        /// Tab id.
        tab_id: String,
    },
    /// Rename a tab and lock its title.
    RenameTab {
        /// Tab id.
        tab_id: String,
        /// Raw requested title.
        title: String,
    },
    /// Set an automatic title unless the user locked the title.
    SetAutomaticTabTitle {
        /// Tab id.
        tab_id: String,
        /// Optional automatic title.
        title: Option<String>,
    },
    /// Update terminal runtime metadata for a pane's mounted terminal content.
    UpdateTerminalMetadata {
        /// Target pane slot id.
        pane_slot_id: String,
        /// Optional runtime title.
        title: Option<String>,
        /// Optional current working directory.
        cwd: Option<String>,
        /// Optional active task state.
        active_task_state: Option<ShellTabActiveTaskState>,
        /// Optional activity snapshot.
        activity: Option<TerminalActivitySnapshot>,
    },
    /// Persist renderer-owned progress for one mounted Agent ContentInstance.
    UpdateAgentRendererState {
        /// Pane slot mounting the Agent ContentInstance.
        pane_slot_id: String,
        /// Caller-owned AgentFS stream offsets.
        offsets: AgentStreamOffsets,
        /// Host presentation only; never Agent Machine state.
        presentation: AgentContentPresentation,
    },
    /// Apply an agent activity snapshot to a pane's mounted terminal content.
    ApplyAgentActivity {
        /// Target pane slot id.
        pane_slot_id: String,
        /// Activity snapshot.
        activity: TerminalActivitySnapshot,
        /// Optional working directory from the agent activity event.
        working_directory: Option<String>,
    },
    /// Set pane attention.
    SetAttention {
        /// Pane slot id.
        pane_slot_id: String,
        /// Next attention state.
        attention: ShellAttentionState,
    },
}

/// Reducer output from a successful state transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReducerResult {
    /// Updated workspace state.
    pub state: WorkspaceState,
    /// Focus projection after the operation.
    pub focus: ReducerFocus,
    /// Stable ids created, updated, or removed by the operation.
    pub changed_ids: ReducerChangedIds,
    /// Domain events emitted by the reducer.
    pub domain_events: Vec<DomainEvent>,
    /// Runtime intents to be executed by the platform adapter.
    pub runtime_intents: Vec<RuntimeIntent>,
    /// Whether the platform should sync the workspace manifest.
    pub manifest_sync: ManifestSyncHint,
}

/// Focus projection after a reducer operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReducerFocus {
    /// Focused Space id.
    pub space_id: Option<String>,
    /// Focused Tab id.
    pub tab_id: Option<String>,
    /// Focused pane slot id.
    pub pane_slot_id: Option<String>,
}

/// Stable changed-id projection for platform adapters and tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReducerChangedIds {
    /// Created Space ids.
    #[serde(default)]
    pub created_space_ids: Vec<String>,
    /// Created Tab ids.
    #[serde(default)]
    pub created_tab_ids: Vec<String>,
    /// Created pane slot ids.
    #[serde(default)]
    pub created_pane_slot_ids: Vec<String>,
    /// Created content ids.
    #[serde(default)]
    pub created_content_ids: Vec<String>,
    /// Updated content ids.
    #[serde(default)]
    pub updated_content_ids: Vec<String>,
    /// Updated Space ids.
    #[serde(default)]
    pub updated_space_ids: Vec<String>,
    /// Removed Space ids.
    #[serde(default)]
    pub removed_space_ids: Vec<String>,
    /// Updated Tab ids.
    #[serde(default)]
    pub updated_tab_ids: Vec<String>,
    /// Updated pane slot ids.
    #[serde(default)]
    pub updated_pane_slot_ids: Vec<String>,
    /// Removed Tab ids.
    #[serde(default)]
    pub removed_tab_ids: Vec<String>,
    /// Removed pane slot ids.
    #[serde(default)]
    pub removed_pane_slot_ids: Vec<String>,
    /// Removed content ids.
    #[serde(default)]
    pub removed_content_ids: Vec<String>,
}

impl ReducerChangedIds {
    fn normalize(&mut self) {
        self.created_space_ids = sorted_unique(&self.created_space_ids);
        self.created_tab_ids = sorted_unique(&self.created_tab_ids);
        self.created_pane_slot_ids = sorted_unique(&self.created_pane_slot_ids);
        self.created_content_ids = sorted_unique(&self.created_content_ids);
        self.updated_content_ids = sorted_unique(&self.updated_content_ids);
        self.updated_space_ids = sorted_unique(&self.updated_space_ids);
        self.removed_space_ids = sorted_unique(&self.removed_space_ids);
        self.updated_tab_ids = sorted_unique(&self.updated_tab_ids);
        self.updated_pane_slot_ids = sorted_unique(&self.updated_pane_slot_ids);
        self.removed_tab_ids = sorted_unique(&self.removed_tab_ids);
        self.removed_pane_slot_ids = sorted_unique(&self.removed_pane_slot_ids);
        self.removed_content_ids = sorted_unique(&self.removed_content_ids);
    }

    fn is_empty(&self) -> bool {
        self.created_space_ids.is_empty()
            && self.created_tab_ids.is_empty()
            && self.created_pane_slot_ids.is_empty()
            && self.created_content_ids.is_empty()
            && self.updated_content_ids.is_empty()
            && self.updated_space_ids.is_empty()
            && self.removed_space_ids.is_empty()
            && self.updated_tab_ids.is_empty()
            && self.updated_pane_slot_ids.is_empty()
            && self.removed_tab_ids.is_empty()
            && self.removed_pane_slot_ids.is_empty()
            && self.removed_content_ids.is_empty()
    }
}

/// Reducer domain event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    /// Focus changed.
    FocusChanged {
        /// Focused pane slot id.
        pane_slot_id: Option<String>,
    },
    /// A Space was created.
    SpaceCreated {
        /// Created Space id.
        space_id: String,
    },
    /// A tab was opened.
    TabOpened {
        /// Created tab id.
        tab_id: String,
        /// Created pane slot id.
        pane_slot_id: String,
    },
    /// A tab moved.
    TabMoved {
        /// Moved tab id.
        tab_id: String,
        /// Target Space id.
        space_id: String,
    },
    /// Temporary tabs were cleared.
    TemporaryTabsCleared {
        /// Target Space id.
        space_id: String,
        /// Removed Tab ids.
        removed_tab_ids: Vec<String>,
    },
    /// A pane was split.
    PaneSplit {
        /// Target pane slot id.
        target_pane_slot_id: String,
        /// Created pane slot id.
        created_pane_slot_id: String,
    },
    /// A split was resized.
    SplitResized {
        /// Split node id.
        split_node_id: String,
    },
    /// Splits were equalized.
    SplitsEqualized {
        /// Tab id.
        tab_id: String,
    },
    /// Tab-scoped pane zoom changed.
    PaneZoomChanged {
        /// Target Tab id.
        tab_id: String,
        /// Zoomed pane slot id, or `None` after unzoom.
        pane_slot_id: Option<String>,
    },
    /// A pane was closed.
    PaneClosed {
        /// Removed pane slot id.
        pane_slot_id: String,
    },
    /// A pane moved.
    PaneMoved {
        /// Moved pane slot id.
        pane_slot_id: String,
        /// Target Tab id.
        tab_id: String,
    },
    /// A tab was closed.
    TabClosed {
        /// Removed tab id.
        tab_id: String,
    },
    /// A tab was pinned or unpinned.
    TabPinChanged {
        /// Tab id.
        tab_id: String,
        /// Whether the tab is now pinned.
        is_pinned: bool,
    },
    /// A tab title changed.
    TabTitleChanged {
        /// Tab id.
        tab_id: String,
    },
    /// Pane attention changed.
    AttentionChanged {
        /// Pane slot id.
        pane_slot_id: String,
    },
    /// Terminal runtime metadata changed.
    TerminalMetadataUpdated {
        /// Pane slot id.
        pane_slot_id: String,
        /// Content id.
        content_id: String,
    },
    /// Agent activity metadata changed.
    AgentActivityUpdated {
        /// Pane slot id.
        pane_slot_id: String,
        /// Content id.
        content_id: String,
    },
    /// An Agent renderer advanced its caller-owned offsets or presentation.
    AgentRendererStateUpdated {
        /// Pane slot id.
        pane_slot_id: String,
        /// Content id.
        content_id: String,
    },
}

/// Runtime intent emitted by shell core for platform adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeIntent {
    /// Start a terminal runtime for a terminal content instance.
    StartTerminal {
        /// Target pane slot id.
        pane_slot_id: String,
        /// Target content id.
        content_id: String,
        /// Optional working directory.
        working_directory: Option<String>,
        /// Optional Terminal Profile id.
        terminal_profile_id: Option<String>,
        /// User-facing title.
        title: String,
    },
    /// Close a terminal runtime.
    CloseTerminal {
        /// Target pane slot id.
        pane_slot_id: String,
        /// Target content id.
        content_id: String,
    },
}

/// Manifest synchronization hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestSyncHint {
    /// No manifest sync needed.
    Unchanged,
    /// Workspace manifest should be synced from the returned state.
    SyncWorkspaceState,
}

/// Stable reducer error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReducerErrorCode {
    /// Target Space was not found.
    SpaceNotFound,
    /// Target Tab was not found.
    TabNotFound,
    /// Target pane slot was not found.
    PaneNotFound,
    /// Target content kind cannot perform the requested operation.
    UnsupportedContent,
    /// Target split node was not found.
    SplitNotFound,
    /// Spatial focus target was not found.
    SpatialFocusTargetNotFound,
    /// Last pane cannot be moved out of its tab.
    LastPane,
    /// Move target is invalid.
    InvalidMoveTarget,
    /// Tab organization target is invalid.
    InvalidTabOrganizationTarget,
}

/// Reducer failure with stable code.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[error("{code:?}: {message}")]
pub struct ReducerError {
    /// Stable error code.
    pub code: ReducerErrorCode,
    /// Human-readable diagnostic.
    pub message: String,
}

impl ReducerError {
    fn new(code: ReducerErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl WorkspaceState {
    /// Applies a pure reducer operation to this workspace state.
    pub fn reduce(&self, operation: ReducerOperation) -> Result<ReducerResult, ReducerError> {
        WorkspaceReducer::new(self.clone()).apply(operation)
    }
}

struct WorkspaceReducer {
    state: WorkspaceState,
    changed_ids: ReducerChangedIds,
    domain_events: Vec<DomainEvent>,
    runtime_intents: Vec<RuntimeIntent>,
}

impl WorkspaceReducer {
    fn new(state: WorkspaceState) -> Self {
        Self {
            state,
            changed_ids: ReducerChangedIds::default(),
            domain_events: Vec::new(),
            runtime_intents: Vec::new(),
        }
    }

    fn apply(mut self, operation: ReducerOperation) -> Result<ReducerResult, ReducerError> {
        match operation {
            ReducerOperation::FocusPane { pane_slot_id } => self.focus_pane(&pane_slot_id)?,
            ReducerOperation::FocusAdjacentPane { direction } => self.focus_adjacent(direction)?,
            ReducerOperation::SelectSpace { space_id } => self.select_space(&space_id)?,
            ReducerOperation::SelectTab { tab_id } => self.select_tab(&tab_id)?,
            ReducerOperation::SetTerminalProfile {
                space_id,
                terminal_profile_id,
            } => self.set_terminal_profile(&space_id, terminal_profile_id)?,
            ReducerOperation::SetPresentationIcon {
                space_id,
                presentation_icon,
            } => self.set_presentation_icon(&space_id, presentation_icon)?,
            ReducerOperation::DeleteSpace {
                space_id,
                default_working_directory,
            } => self.delete_space(&space_id, default_working_directory)?,
            ReducerOperation::CreateTerminalSpace {
                title,
                tab_title,
                working_directory,
                terminal_profile_id,
                presentation_icon,
                reserved_pane_slot_ids,
            } => self.create_terminal_space(
                title,
                tab_title,
                working_directory,
                terminal_profile_id,
                presentation_icon,
                reserved_pane_slot_ids,
            ),
            ReducerOperation::OpenTerminalTab {
                space_id,
                title,
                working_directory,
                terminal_profile_id,
                reserved_pane_slot_ids,
            } => self.open_terminal_tab(
                space_id,
                title,
                working_directory,
                terminal_profile_id,
                reserved_pane_slot_ids,
            )?,
            ReducerOperation::OpenContentTab {
                space_id,
                kind,
                title,
                payload,
                reserved_pane_slot_ids,
            } => self.open_content_tab(space_id, kind, title, payload, reserved_pane_slot_ids)?,
            ReducerOperation::DuplicateTab {
                tab_id,
                reserved_pane_slot_ids,
            } => self.duplicate_tab(&tab_id, reserved_pane_slot_ids)?,
            ReducerOperation::MoveTab {
                tab_id,
                section_offset,
            } => self.move_tab(&tab_id, section_offset)?,
            ReducerOperation::MoveTabToSpace {
                tab_id,
                target_space_id,
            } => self.move_tab_to_space(&tab_id, &target_space_id)?,
            ReducerOperation::OrganizeTab {
                tab_id,
                target_space_id,
                section,
                index,
            } => self.organize_tab(&tab_id, target_space_id, section, index)?,
            ReducerOperation::ClearInactiveTemporaryTabs {
                space_id,
                protected_tab_ids,
            } => self.clear_inactive_temporary_tabs(&space_id, protected_tab_ids)?,
            ReducerOperation::SplitPane {
                pane_slot_id,
                placement,
                title,
                working_directory,
                terminal_profile_id,
                reserved_pane_slot_ids,
            } => self.split_pane(
                &pane_slot_id,
                placement,
                title,
                working_directory,
                terminal_profile_id,
                reserved_pane_slot_ids,
            )?,
            ReducerOperation::SplitContentPane {
                pane_slot_id,
                placement,
                kind,
                title,
                payload,
                reserved_pane_slot_ids,
            } => self.split_content_pane(
                &pane_slot_id,
                placement,
                kind,
                title,
                payload,
                reserved_pane_slot_ids,
            )?,
            ReducerOperation::ResizeSplit {
                split_node_id,
                ratio,
            } => self.resize_split(&split_node_id, ratio)?,
            ReducerOperation::EqualizeSplits { tab_id } => self.equalize_splits(tab_id)?,
            ReducerOperation::ZoomPane { pane_slot_id } => self.zoom_pane(&pane_slot_id)?,
            ReducerOperation::UnzoomTab { tab_id } => self.unzoom_tab(tab_id)?,
            ReducerOperation::ClosePane { pane_slot_id } => self.close_pane(&pane_slot_id)?,
            ReducerOperation::MovePaneToNewTab {
                pane_slot_id,
                title,
            } => self.move_pane_to_new_tab(&pane_slot_id, title)?,
            ReducerOperation::MovePaneToTab {
                pane_slot_id,
                target_tab_id,
                direction,
            } => self.move_pane_to_tab(&pane_slot_id, &target_tab_id, direction)?,
            ReducerOperation::MovePaneWithinTab {
                pane_slot_id,
                placement,
            } => self.move_pane_within_tab(&pane_slot_id, placement)?,
            ReducerOperation::CloseTab { tab_id } => self.close_tab(&tab_id)?,
            ReducerOperation::PinTab { tab_id } => self.set_tab_pinned(&tab_id, true)?,
            ReducerOperation::UnpinTab { tab_id } => self.set_tab_pinned(&tab_id, false)?,
            ReducerOperation::RenameTab { tab_id, title } => {
                self.set_tab_title(&tab_id, visible_tab_title(Some(&title)), true, false)?
            }
            ReducerOperation::SetAutomaticTabTitle { tab_id, title } => {
                self.set_tab_title(&tab_id, visible_tab_title(title.as_deref()), false, true)?
            }
            ReducerOperation::UpdateTerminalMetadata {
                pane_slot_id,
                title,
                cwd,
                active_task_state,
                activity,
            } => self.update_terminal_metadata(
                &pane_slot_id,
                title,
                cwd,
                active_task_state,
                activity,
            )?,
            ReducerOperation::UpdateAgentRendererState {
                pane_slot_id,
                offsets,
                presentation,
            } => self.update_agent_renderer_state(&pane_slot_id, offsets, presentation)?,
            ReducerOperation::ApplyAgentActivity {
                pane_slot_id,
                activity,
                working_directory,
            } => self.apply_agent_activity(&pane_slot_id, activity, working_directory)?,
            ReducerOperation::SetAttention {
                pane_slot_id,
                attention,
            } => self.set_attention(&pane_slot_id, attention)?,
        }

        Ok(self.finish())
    }

    fn finish(mut self) -> ReducerResult {
        self.reconcile_zoom_state();
        self.changed_ids.normalize();
        ReducerResult {
            focus: self.state.focus(),
            state: self.state,
            manifest_sync: if self.changed_ids.is_empty() {
                ManifestSyncHint::Unchanged
            } else {
                ManifestSyncHint::SyncWorkspaceState
            },
            changed_ids: self.changed_ids,
            domain_events: self.domain_events,
            runtime_intents: self.runtime_intents,
        }
    }

    fn focus_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
        self.require_pane_slot(pane_slot_id)?;
        self.repair_focus(Some(pane_slot_id.to_string()));
        self.domain_events.push(DomainEvent::FocusChanged {
            pane_slot_id: self.state.focused_pane_id.clone(),
        });
        Ok(())
    }

    fn focus_adjacent(&mut self, direction: SpatialFocusDirection) -> Result<(), ReducerError> {
        let focused_pane_id =
            self.state.focused_pane_id.clone().ok_or_else(|| {
                ReducerError::new(ReducerErrorCode::PaneNotFound, "no focused pane")
            })?;
        let focused_slot = self.require_pane_slot(&focused_pane_id)?.clone();
        let (space_index, tab_index) = self.tab_location(&focused_slot.tab_id)?;
        let tab = &self.state.spaces[space_index].tabs[tab_index];
        let target_pane_id = tab
            .pane_tree
            .adjacent_pane_id(&focused_pane_id, direction)
            .ok_or_else(|| {
                ReducerError::new(
                    ReducerErrorCode::SpatialFocusTargetNotFound,
                    "spatial focus target not found",
                )
            })?;
        self.focus_pane(&target_pane_id)
    }

    fn select_space(&mut self, space_id: &str) -> Result<(), ReducerError> {
        let space_index = self.space_index(space_id)?;
        let target_tab = self.state.spaces[space_index]
            .selected_tab_id
            .clone()
            .and_then(|selected_tab_id| {
                self.state.spaces[space_index]
                    .tabs
                    .iter()
                    .find(|tab| tab.tab_id == selected_tab_id)
                    .cloned()
            })
            .or_else(|| {
                self.state
                    .focused_pane_id
                    .as_ref()
                    .and_then(|focused_pane_id| {
                        self.state.spaces[space_index]
                            .tabs
                            .iter()
                            .find(|tab| tab.pane_tree.contains_pane_id(focused_pane_id))
                            .cloned()
                    })
            })
            .or_else(|| self.state.spaces[space_index].tabs.first().cloned());

        let Some(target_tab) = target_tab else {
            self.state.focused_space_id = Some(space_id.to_string());
            self.state.focused_tab_id = None;
            self.state.focused_pane_id = None;
            self.state.spaces[space_index].selected_tab_id = None;
            self.domain_events
                .push(DomainEvent::FocusChanged { pane_slot_id: None });
            return Ok(());
        };
        let pane_id = self.target_pane_id_for_tab(&target_tab)?;
        self.focus_pane(&pane_id)
    }

    fn select_tab(&mut self, tab_id: &str) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let tab = self.state.spaces[space_index].tabs[tab_index].clone();
        let pane_id = self.target_pane_id_for_tab(&tab)?;
        self.focus_pane(&pane_id)
    }

    fn set_terminal_profile(
        &mut self,
        space_id: &str,
        terminal_profile_id: Option<String>,
    ) -> Result<(), ReducerError> {
        let space_index = self.space_index(space_id)?;
        self.state.spaces[space_index].terminal_profile_id = terminal_profile_id;
        self.changed_ids
            .updated_space_ids
            .push(space_id.to_string());
        Ok(())
    }

    fn set_presentation_icon(
        &mut self,
        space_id: &str,
        presentation_icon: Option<String>,
    ) -> Result<(), ReducerError> {
        let space_index = self.space_index(space_id)?;
        self.state.spaces[space_index].presentation_icon =
            supported_presentation_icon(presentation_icon);
        self.changed_ids
            .updated_space_ids
            .push(space_id.to_string());
        Ok(())
    }

    fn delete_space(
        &mut self,
        space_id: &str,
        default_working_directory: Option<String>,
    ) -> Result<(), ReducerError> {
        let space_index = self.space_index(space_id)?;
        let target_space = self.state.spaces.remove(space_index);
        let removed_tab_ids = target_space
            .tabs
            .iter()
            .map(|tab| tab.tab_id.clone())
            .collect::<Vec<_>>();
        let removed_pane_slot_ids = target_space
            .tabs
            .iter()
            .flat_map(|tab| tab.pane_tree.pane_ids())
            .collect::<Vec<_>>();

        self.changed_ids
            .removed_space_ids
            .push(space_id.to_string());
        self.changed_ids.removed_tab_ids.extend(removed_tab_ids);
        self.remove_pane_slots_and_contents(&removed_pane_slot_ids);

        if self.state.spaces.is_empty() {
            self.bootstrap_default_workspace(default_working_directory);
        } else {
            self.repair_focus(self.state.focused_pane_id.clone());
        }
        Ok(())
    }

    fn create_terminal_space(
        &mut self,
        title: Option<String>,
        tab_title: Option<String>,
        working_directory: Option<String>,
        terminal_profile_id: Option<String>,
        presentation_icon: Option<String>,
        reserved_pane_slot_ids: Vec<String>,
    ) {
        let space_id = next_id(
            "space",
            self.state.spaces.iter().map(|space| &space.space_id),
        );
        let tab_id = next_id(
            "tab",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .map(|tab| &tab.tab_id),
        );
        let pane_slot_ids = self.pane_slot_id_sources(reserved_pane_slot_ids);
        let pane_slot_id = next_id("pane", pane_slot_ids.iter());
        let content_id = terminal_content_id(&pane_slot_id);
        let locks_tab_title = tab_title
            .as_deref()
            .map(str::trim)
            .is_some_and(|title| !title.is_empty());
        let resolved_tab_title = tab_title.unwrap_or_else(|| "Shell".to_string());
        let resolved_space_title = title.unwrap_or_else(|| {
            default_space_title_from_working_directory(
                working_directory.as_deref(),
                self.state.spaces.len() + 1,
            )
        });

        let tab = Tab {
            tab_id: tab_id.clone(),
            kind: TabKind::Terminal,
            title: Some(resolved_tab_title.clone()),
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            zoomed_pane_id: None,
            is_pinned: false,
            is_title_user_locked: locks_tab_title,
        };
        self.state.spaces.push(Space {
            space_id: space_id.clone(),
            title: resolved_space_title,
            attention: ShellAttentionState::Active,
            tabs: vec![tab],
            selected_tab_id: Some(tab_id.clone()),
            terminal_profile_id: terminal_profile_id.clone(),
            presentation_icon: supported_presentation_icon(presentation_icon),
        });
        self.state.pane_slots.push(PaneSlot {
            pane_slot_id: pane_slot_id.clone(),
            tab_id: tab_id.clone(),
            space_id: space_id.clone(),
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        });
        self.state.contents.push(terminal_content(
            &content_id,
            Some(&resolved_tab_title),
            working_directory.as_deref(),
            terminal_profile_id.as_deref(),
        ));
        self.changed_ids.created_space_ids.push(space_id.clone());
        self.changed_ids.created_tab_ids.push(tab_id.clone());
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id.clone());
        self.changed_ids
            .created_content_ids
            .push(content_id.clone());
        self.domain_events
            .push(DomainEvent::SpaceCreated { space_id });
        self.domain_events.push(DomainEvent::TabOpened {
            tab_id,
            pane_slot_id: pane_slot_id.clone(),
        });
        self.runtime_intents.push(RuntimeIntent::StartTerminal {
            pane_slot_id: pane_slot_id.clone(),
            content_id,
            working_directory,
            terminal_profile_id,
            title: resolved_tab_title,
        });
        self.repair_focus(Some(pane_slot_id));
    }

    fn open_terminal_tab(
        &mut self,
        space_id: Option<String>,
        title: Option<String>,
        working_directory: Option<String>,
        terminal_profile_id: Option<String>,
        reserved_pane_slot_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        let target_space_id = space_id
            .or_else(|| self.state.focused_space_id.clone())
            .or_else(|| {
                self.state
                    .spaces
                    .first()
                    .map(|space| space.space_id.clone())
            })
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::SpaceNotFound, "no target space"))?;
        let space_index = self.space_index(&target_space_id)?;
        let tab_id = next_id(
            "tab",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .map(|tab| &tab.tab_id),
        );
        let pane_slot_ids = self.pane_slot_id_sources(reserved_pane_slot_ids);
        let pane_slot_id = next_id("pane", pane_slot_ids.iter());
        let content_id = terminal_content_id(&pane_slot_id);
        let locks_tab_title = title
            .as_deref()
            .map(str::trim)
            .is_some_and(|title| !title.is_empty());
        let resolved_title = title.unwrap_or_else(|| {
            let next_tab_count = self.state.spaces[space_index].tabs.len() + 1;
            format!("Shell {next_tab_count}")
        });
        let resolved_profile = terminal_profile_id.or_else(|| {
            self.state.spaces[space_index]
                .terminal_profile_id
                .as_ref()
                .cloned()
        });

        self.state.spaces[space_index].tabs.push(Tab {
            tab_id: tab_id.clone(),
            kind: TabKind::Terminal,
            title: Some(resolved_title.clone()),
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            zoomed_pane_id: None,
            is_pinned: false,
            is_title_user_locked: locks_tab_title,
        });
        self.state.spaces[space_index].selected_tab_id = Some(tab_id.clone());
        self.state.pane_slots.push(PaneSlot {
            pane_slot_id: pane_slot_id.clone(),
            tab_id: tab_id.clone(),
            space_id: target_space_id,
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        });
        self.state.contents.push(terminal_content(
            &content_id,
            Some(&resolved_title),
            working_directory.as_deref(),
            resolved_profile.as_deref(),
        ));
        self.changed_ids.created_tab_ids.push(tab_id.clone());
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id.clone());
        self.changed_ids
            .created_content_ids
            .push(content_id.clone());
        self.domain_events.push(DomainEvent::TabOpened {
            tab_id,
            pane_slot_id: pane_slot_id.clone(),
        });
        self.runtime_intents.push(RuntimeIntent::StartTerminal {
            pane_slot_id: pane_slot_id.clone(),
            content_id,
            working_directory,
            terminal_profile_id: resolved_profile,
            title: resolved_title,
        });
        self.repair_focus(Some(pane_slot_id));
        Ok(())
    }

    fn split_pane(
        &mut self,
        pane_slot_id: &str,
        placement: SplitPlacement,
        title: Option<String>,
        working_directory: Option<String>,
        terminal_profile_id: Option<String>,
        reserved_pane_slot_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        let source_slot = self.require_pane_slot(pane_slot_id)?.clone();
        if self.content_kind(&source_slot.content_id) != Some(ContentKind::Terminal) {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "only terminal content can be split without an explicit content mount",
            ));
        }

        let (space_index, tab_index) = self.tab_location(&source_slot.tab_id)?;
        let pane_slot_ids = self.pane_slot_id_sources(reserved_pane_slot_ids);
        let pane_slot_id_new = next_id("pane", pane_slot_ids.iter());
        let split_node_id = next_id(
            "node",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .flat_map(|tab| tab.pane_tree.node_ids()),
        );
        let content_id = terminal_content_id(&pane_slot_id_new);
        let resolved_title = title.unwrap_or_else(|| "Shell".to_string());

        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        tab.pane_tree = tab.pane_tree.split_pane(
            pane_slot_id,
            placement,
            split_node_id,
            format!("node_{pane_slot_id_new}"),
            pane_slot_id_new.clone(),
        );
        self.state.pane_slots.push(PaneSlot {
            pane_slot_id: pane_slot_id_new.clone(),
            tab_id: source_slot.tab_id.clone(),
            space_id: source_slot.space_id.clone(),
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        });
        self.state.contents.push(terminal_content(
            &content_id,
            Some(&resolved_title),
            working_directory.as_deref(),
            terminal_profile_id.as_deref(),
        ));

        self.changed_ids.updated_tab_ids.push(source_slot.tab_id);
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id_new.clone());
        self.changed_ids
            .created_content_ids
            .push(content_id.clone());
        self.domain_events.push(DomainEvent::PaneSplit {
            target_pane_slot_id: pane_slot_id.to_string(),
            created_pane_slot_id: pane_slot_id_new.clone(),
        });
        self.runtime_intents.push(RuntimeIntent::StartTerminal {
            pane_slot_id: pane_slot_id_new.clone(),
            content_id,
            working_directory,
            terminal_profile_id,
            title: resolved_title,
        });
        self.repair_focus(Some(pane_slot_id_new));
        Ok(())
    }

    fn duplicate_tab(
        &mut self,
        tab_id: &str,
        reserved_pane_slot_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        let (source_space_index, source_tab_index) = self.tab_location(tab_id)?;
        let source_tab = self.state.spaces[source_space_index].tabs[source_tab_index].clone();
        let primary_pane_slot_id = source_tab
            .pane_tree
            .pane_ids()
            .first()
            .cloned()
            .ok_or_else(|| {
                ReducerError::new(ReducerErrorCode::UnsupportedContent, "tab has no pane")
            })?;
        let primary_slot = self.require_pane_slot(&primary_pane_slot_id)?.clone();
        if self.content_kind(&primary_slot.content_id) != Some(ContentKind::Terminal) {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "only terminal-backed tabs can be duplicated",
            ));
        }

        let tab_id_new = next_id(
            "tab",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .map(|tab| &tab.tab_id),
        );
        let pane_slot_ids = self.pane_slot_id_sources(reserved_pane_slot_ids);
        let pane_slot_id = next_id("pane", pane_slot_ids.iter());
        let content_id = terminal_content_id(&pane_slot_id);
        let title = source_tab
            .title
            .clone()
            .unwrap_or_else(|| "Shell".to_string());
        let duplicate = Tab {
            tab_id: tab_id_new.clone(),
            kind: source_tab.kind,
            title: source_tab.title.clone(),
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            zoomed_pane_id: None,
            is_pinned: source_tab.is_pinned,
            is_title_user_locked: source_tab.is_title_user_locked,
        };
        self.state.spaces[source_space_index]
            .tabs
            .insert(source_tab_index + 1, duplicate);
        self.state.spaces[source_space_index].selected_tab_id = Some(tab_id_new.clone());
        self.state.pane_slots.push(PaneSlot {
            pane_slot_id: pane_slot_id.clone(),
            tab_id: tab_id_new.clone(),
            space_id: self.state.spaces[source_space_index].space_id.clone(),
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        });
        let source_terminal_payload = self.terminal_payload(&primary_slot.content_id).cloned();
        let working_directory = source_terminal_payload
            .as_ref()
            .and_then(|payload| payload.cwd.clone());
        let terminal_profile_id = source_terminal_payload
            .as_ref()
            .and_then(|payload| payload.terminal_profile_id.clone())
            .or_else(|| {
                self.state.spaces[source_space_index]
                    .terminal_profile_id
                    .clone()
            });
        self.state.contents.push(terminal_content(
            &content_id,
            Some(&title),
            working_directory.as_deref(),
            terminal_profile_id.as_deref(),
        ));
        self.changed_ids.created_tab_ids.push(tab_id_new.clone());
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id.clone());
        self.changed_ids
            .created_content_ids
            .push(content_id.clone());
        self.domain_events.push(DomainEvent::TabOpened {
            tab_id: tab_id_new,
            pane_slot_id: pane_slot_id.clone(),
        });
        self.runtime_intents.push(RuntimeIntent::StartTerminal {
            pane_slot_id: pane_slot_id.clone(),
            content_id,
            working_directory,
            terminal_profile_id,
            title,
        });
        self.repair_focus(Some(pane_slot_id));
        Ok(())
    }

    fn move_tab(&mut self, tab_id: &str, section_offset: isize) -> Result<(), ReducerError> {
        if section_offset == 0 {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidTabOrganizationTarget,
                "tab move offset must be non-zero",
            ));
        }
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let is_pinned = self.state.spaces[space_index].tabs[tab_index].is_pinned;
        let section_indices: Vec<_> = self.state.spaces[space_index]
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| (tab.is_pinned == is_pinned).then_some(index))
            .collect();
        let section_index = section_indices
            .iter()
            .position(|index| *index == tab_index)
            .ok_or_else(|| {
                ReducerError::new(
                    ReducerErrorCode::InvalidTabOrganizationTarget,
                    "tab section not found",
                )
            })?;
        let next_section_index = section_index as isize + section_offset;
        if !(0..section_indices.len() as isize).contains(&next_section_index) {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidTabOrganizationTarget,
                "tab move target out of bounds",
            ));
        }
        let mut tab = self.state.spaces[space_index].tabs.remove(tab_index);
        tab.is_pinned = is_pinned;
        let mut next_tabs = self.state.spaces[space_index].tabs.clone();
        let insertion_index =
            insertion_index_for_section(&next_tabs, is_pinned, next_section_index as usize);
        next_tabs.insert(insertion_index, tab);
        self.state.spaces[space_index].tabs = next_tabs;
        self.changed_ids.updated_tab_ids.push(tab_id.to_string());
        self.domain_events.push(DomainEvent::TabMoved {
            tab_id: tab_id.to_string(),
            space_id: self.state.spaces[space_index].space_id.clone(),
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn move_tab_to_space(
        &mut self,
        tab_id: &str,
        target_space_id: &str,
    ) -> Result<(), ReducerError> {
        let (source_space_index, source_tab_index) = self.tab_location(tab_id)?;
        let source_space_id = self.state.spaces[source_space_index].space_id.clone();
        if source_space_id == target_space_id {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "tab is already in target space",
            ));
        }
        self.space_index(target_space_id)?;
        let moved_tab = self.state.spaces[source_space_index]
            .tabs
            .remove(source_tab_index);
        let target_space_index = self.space_index(target_space_id)?;
        let insertion_index = insertion_index_for_section(
            &self.state.spaces[target_space_index].tabs,
            moved_tab.is_pinned,
            self.state.spaces[target_space_index]
                .tabs
                .iter()
                .filter(|tab| tab.is_pinned == moved_tab.is_pinned)
                .count(),
        );
        self.state.spaces[target_space_index]
            .tabs
            .insert(insertion_index, moved_tab);
        self.state.spaces[target_space_index].selected_tab_id = Some(tab_id.to_string());
        for slot in &mut self.state.pane_slots {
            if slot.tab_id == tab_id {
                slot.space_id = target_space_id.to_string();
                self.changed_ids
                    .updated_pane_slot_ids
                    .push(slot.pane_slot_id.clone());
            }
        }
        self.changed_ids.updated_tab_ids.push(tab_id.to_string());
        self.domain_events.push(DomainEvent::TabMoved {
            tab_id: tab_id.to_string(),
            space_id: target_space_id.to_string(),
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn organize_tab(
        &mut self,
        tab_id: &str,
        target_space_id: Option<String>,
        section: TabOrganizationSection,
        index: Option<usize>,
    ) -> Result<(), ReducerError> {
        let (source_space_index, source_tab_index) = self.tab_location(tab_id)?;
        let source_space_id = self.state.spaces[source_space_index].space_id.clone();
        let target_space_id = target_space_id.unwrap_or(source_space_id.clone());
        let target_space_index = self.space_index(&target_space_id)?;
        let mut tab = self.state.spaces[source_space_index]
            .tabs
            .remove(source_tab_index);
        tab.is_pinned = section.is_pinned();

        let section_count = self.state.spaces[target_space_index]
            .tabs
            .iter()
            .filter(|candidate| candidate.is_pinned == tab.is_pinned)
            .count();
        let section_index = index.unwrap_or(section_count);
        if section_index > section_count {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidTabOrganizationTarget,
                "tab organization target out of bounds",
            ));
        }

        let insertion_index = insertion_index_for_section(
            &self.state.spaces[target_space_index].tabs,
            tab.is_pinned,
            section_index,
        );
        self.state.spaces[target_space_index]
            .tabs
            .insert(insertion_index, tab);

        if source_space_id != target_space_id {
            for slot in &mut self.state.pane_slots {
                if slot.tab_id == tab_id {
                    slot.space_id = target_space_id.clone();
                    self.changed_ids
                        .updated_pane_slot_ids
                        .push(slot.pane_slot_id.clone());
                }
            }
        }

        self.changed_ids.updated_tab_ids.push(tab_id.to_string());
        self.domain_events.push(DomainEvent::TabMoved {
            tab_id: tab_id.to_string(),
            space_id: target_space_id,
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn clear_inactive_temporary_tabs(
        &mut self,
        space_id: &str,
        protected_tab_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        let space_index = self.space_index(space_id)?;
        let selected_tab_id = self.state.spaces[space_index]
            .selected_tab_id
            .clone()
            .filter(|selected| {
                self.state.spaces[space_index]
                    .tabs
                    .iter()
                    .any(|tab| tab.tab_id == *selected)
            })
            .or_else(|| {
                self.state.spaces[space_index]
                    .tabs
                    .first()
                    .map(|tab| tab.tab_id.clone())
            });
        let mut protected: BTreeSet<_> = protected_tab_ids.into_iter().collect();
        protected.extend(self.active_task_protected_tab_ids(space_id));
        let clearable_tab_ids: BTreeSet<_> = self.state.spaces[space_index]
            .tabs
            .iter()
            .filter(|tab| !tab.is_pinned)
            .filter(|tab| Some(&tab.tab_id) != selected_tab_id.as_ref())
            .filter(|tab| !protected.contains(&tab.tab_id))
            .map(|tab| tab.tab_id.clone())
            .collect();
        if clearable_tab_ids.is_empty() {
            return Ok(());
        }
        let removed_pane_ids: Vec<_> = self.state.spaces[space_index]
            .tabs
            .iter()
            .filter(|tab| clearable_tab_ids.contains(&tab.tab_id))
            .flat_map(|tab| tab.pane_tree.pane_ids())
            .collect();
        self.state.spaces[space_index]
            .tabs
            .retain(|tab| !clearable_tab_ids.contains(&tab.tab_id));
        self.changed_ids
            .removed_tab_ids
            .extend(clearable_tab_ids.iter().cloned());
        self.remove_pane_slots_and_contents(&removed_pane_ids);
        self.domain_events.push(DomainEvent::TemporaryTabsCleared {
            space_id: space_id.to_string(),
            removed_tab_ids: clearable_tab_ids.into_iter().collect(),
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn resize_split(&mut self, split_node_id: &str, ratio: f64) -> Result<(), ReducerError> {
        let Some((space_index, tab_index)) = self.find_tab_containing_node(split_node_id) else {
            return Err(ReducerError::new(
                ReducerErrorCode::SplitNotFound,
                "split node not found",
            ));
        };
        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        let result = tab.pane_tree.resize_split(split_node_id, ratio);
        if result.outcome == PaneTreeNodeResizeOutcome::Unchanged {
            return Err(ReducerError::new(
                ReducerErrorCode::SplitNotFound,
                "split node not found",
            ));
        }
        tab.pane_tree = result.node;
        self.changed_ids.updated_tab_ids.push(tab.tab_id.clone());
        self.domain_events.push(DomainEvent::SplitResized {
            split_node_id: split_node_id.to_string(),
        });
        Ok(())
    }

    fn equalize_splits(&mut self, tab_id: Option<String>) -> Result<(), ReducerError> {
        let target_tab_id = tab_id
            .or_else(|| self.state.focused_tab_id.clone())
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::TabNotFound, "no focused tab"))?;
        let (space_index, tab_index) = self.tab_location(&target_tab_id)?;
        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        tab.pane_tree = tab.pane_tree.equalized_splits();
        self.changed_ids.updated_tab_ids.push(tab.tab_id.clone());
        self.domain_events.push(DomainEvent::SplitsEqualized {
            tab_id: tab.tab_id.clone(),
        });
        Ok(())
    }

    fn zoom_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let (space_index, tab_index) = self.tab_location(&pane_slot.tab_id)?;
        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        if !tab.pane_tree.contains_pane_id(pane_slot_id) {
            return Err(ReducerError::new(
                ReducerErrorCode::PaneNotFound,
                "pane not found in tab",
            ));
        }
        if tab.pane_tree.pane_ids().len() <= 1 {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "pane zoom requires a split tab",
            ));
        }
        if tab.zoomed_pane_id.as_deref() == Some(pane_slot_id) {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "pane is already zoomed",
            ));
        }
        tab.zoomed_pane_id = Some(pane_slot_id.to_string());
        self.changed_ids.updated_tab_ids.push(tab.tab_id.clone());
        self.domain_events.push(DomainEvent::PaneZoomChanged {
            tab_id: tab.tab_id.clone(),
            pane_slot_id: Some(pane_slot_id.to_string()),
        });
        self.repair_focus(Some(pane_slot_id.to_string()));
        Ok(())
    }

    fn unzoom_tab(&mut self, tab_id: Option<String>) -> Result<(), ReducerError> {
        let target_tab_id = tab_id
            .or_else(|| self.state.focused_tab_id.clone())
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::TabNotFound, "no focused tab"))?;
        let (space_index, tab_index) = self.tab_location(&target_tab_id)?;
        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        if tab.zoomed_pane_id.is_none() {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "tab is not zoomed",
            ));
        }
        tab.zoomed_pane_id = None;
        self.changed_ids.updated_tab_ids.push(tab.tab_id.clone());
        self.domain_events.push(DomainEvent::PaneZoomChanged {
            tab_id: tab.tab_id.clone(),
            pane_slot_id: None,
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn close_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let (space_index, tab_index) = self.tab_location(&pane_slot.tab_id)?;
        let pane_count = self.state.spaces[space_index].tabs[tab_index]
            .pane_tree
            .pane_ids()
            .len();
        if pane_count == 1 {
            return self.close_tab(&pane_slot.tab_id);
        }

        let updated_pane_ids = {
            let tab = &mut self.state.spaces[space_index].tabs[tab_index];
            tab.pane_tree = tab.pane_tree.remove_pane(pane_slot_id).ok_or_else(|| {
                ReducerError::new(ReducerErrorCode::PaneNotFound, "pane not found")
            })?;
            self.changed_ids.updated_tab_ids.push(tab.tab_id.clone());
            tab.pane_tree.pane_ids()
        };
        self.remove_pane_slots_and_contents(&[pane_slot_id.to_string()]);
        self.domain_events.push(DomainEvent::PaneClosed {
            pane_slot_id: pane_slot_id.to_string(),
        });

        let preferred_focus = if self.state.focused_pane_id.as_deref() == Some(pane_slot_id) {
            updated_pane_ids.first().cloned()
        } else {
            self.state.focused_pane_id.clone()
        };
        self.repair_focus(preferred_focus);
        Ok(())
    }

    fn move_pane_to_new_tab(
        &mut self,
        pane_slot_id: &str,
        title: Option<String>,
    ) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let (space_index, source_tab_index) = self.tab_location(&pane_slot.tab_id)?;
        let source_tab = self.state.spaces[space_index].tabs[source_tab_index].clone();
        if source_tab.pane_tree.pane_ids().len() <= 1 {
            return Err(ReducerError::new(
                ReducerErrorCode::LastPane,
                "cannot move the last pane to a new tab",
            ));
        }
        let source_tree = source_tab
            .pane_tree
            .remove_pane(pane_slot_id)
            .ok_or_else(|| {
                ReducerError::new(
                    ReducerErrorCode::PaneNotFound,
                    "pane not found in source tab",
                )
            })?;
        let new_tab_id = next_id(
            "tab",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .map(|tab| &tab.tab_id),
        );
        // When no explicit title is given (e.g. socket/control-file `pane.lift`), inherit the
        // moved pane's current content title so titled terminals keep their name, matching the
        // host UI path; only fall back to the generic label when no content title is available.
        let resolved_title = title
            .or_else(|| {
                self.state
                    .contents
                    .iter()
                    .find(|content| content.content_id == pane_slot.content_id)
                    .map(|content| content.title.clone())
            })
            .or_else(|| Some("Lifted Pane".to_string()));
        self.state.spaces[space_index].tabs[source_tab_index].pane_tree = source_tree;
        let moved_tab = Tab {
            tab_id: new_tab_id.clone(),
            kind: source_tab.kind,
            title: resolved_title,
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.to_string()),
            zoomed_pane_id: None,
            is_pinned: false,
            is_title_user_locked: false,
        };
        self.state.spaces[space_index]
            .tabs
            .insert(source_tab_index + 1, moved_tab);
        for slot in &mut self.state.pane_slots {
            if slot.pane_slot_id == pane_slot_id {
                slot.tab_id = new_tab_id.clone();
                self.changed_ids
                    .updated_pane_slot_ids
                    .push(slot.pane_slot_id.clone());
            }
        }
        self.changed_ids
            .updated_tab_ids
            .extend([source_tab.tab_id, new_tab_id.clone()]);
        self.domain_events.push(DomainEvent::PaneMoved {
            pane_slot_id: pane_slot_id.to_string(),
            tab_id: new_tab_id,
        });
        self.repair_focus(Some(pane_slot_id.to_string()));
        Ok(())
    }

    fn move_pane_to_tab(
        &mut self,
        pane_slot_id: &str,
        target_tab_id: &str,
        direction: SplitDirection,
    ) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let source_tab_id = pane_slot.tab_id;
        if source_tab_id == target_tab_id {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "pane is already in target tab",
            ));
        }
        self.tab_location(target_tab_id)?;
        let (source_space_index, source_tab_index) = self.tab_location(&source_tab_id)?;
        let new_split_node_id = next_id(
            "node",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .flat_map(|tab| tab.pane_tree.node_ids()),
        );
        let source_tree = self.state.spaces[source_space_index].tabs[source_tab_index]
            .pane_tree
            .remove_pane(pane_slot_id);
        match source_tree {
            Some(tree) => {
                self.state.spaces[source_space_index].tabs[source_tab_index].pane_tree = tree;
            }
            None => {
                self.state.spaces[source_space_index]
                    .tabs
                    .remove(source_tab_index);
                self.changed_ids.removed_tab_ids.push(source_tab_id.clone());
            }
        }

        let (target_space_index, target_tab_index) = self.tab_location(target_tab_id)?;
        let target_tab = &mut self.state.spaces[target_space_index].tabs[target_tab_index];
        target_tab.pane_tree = target_tab.pane_tree.attach_pane(
            pane_slot_id.to_string(),
            direction,
            new_split_node_id,
            format!("node_{pane_slot_id}_moved"),
        );
        for slot in &mut self.state.pane_slots {
            if slot.pane_slot_id == pane_slot_id {
                slot.tab_id = target_tab_id.to_string();
                slot.space_id = self.state.spaces[target_space_index].space_id.clone();
                self.changed_ids
                    .updated_pane_slot_ids
                    .push(slot.pane_slot_id.clone());
            }
        }
        self.changed_ids
            .updated_tab_ids
            .extend([source_tab_id, target_tab_id.to_string()]);
        self.domain_events.push(DomainEvent::PaneMoved {
            pane_slot_id: pane_slot_id.to_string(),
            tab_id: target_tab_id.to_string(),
        });
        self.repair_focus(Some(pane_slot_id.to_string()));
        Ok(())
    }

    fn move_pane_within_tab(
        &mut self,
        pane_slot_id: &str,
        placement: SplitPlacement,
    ) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let (space_index, tab_index) = self.tab_location(&pane_slot.tab_id)?;
        let tab = self.state.spaces[space_index].tabs[tab_index].clone();
        if tab.pane_tree.pane_ids().len() <= 1 {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "cannot move a pane in a single-pane tab",
            ));
        }
        let target_pane_id = tab
            .pane_tree
            .adjacent_pane_id(pane_slot_id, spatial_direction_for_placement(placement))
            .filter(|target| target != pane_slot_id)
            .ok_or_else(|| {
                ReducerError::new(
                    ReducerErrorCode::InvalidMoveTarget,
                    "no adjacent pane target for move",
                )
            })?;
        let tree_without_moved = tab.pane_tree.remove_pane(pane_slot_id).ok_or_else(|| {
            ReducerError::new(ReducerErrorCode::InvalidMoveTarget, "pane removal failed")
        })?;
        if !tree_without_moved.contains_pane_id(&target_pane_id) {
            return Err(ReducerError::new(
                ReducerErrorCode::InvalidMoveTarget,
                "move target disappeared after removing pane",
            ));
        }
        let new_split_node_id = next_id(
            "node",
            self.state
                .spaces
                .iter()
                .flat_map(|space| &space.tabs)
                .flat_map(|tab| tab.pane_tree.node_ids()),
        );
        self.state.spaces[space_index].tabs[tab_index].pane_tree = tree_without_moved.split_pane(
            &target_pane_id,
            placement,
            new_split_node_id,
            format!("node_{pane_slot_id}_moved_in_tab"),
            pane_slot_id.to_string(),
        );
        self.changed_ids
            .updated_tab_ids
            .push(pane_slot.tab_id.clone());
        self.domain_events.push(DomainEvent::PaneMoved {
            pane_slot_id: pane_slot_id.to_string(),
            tab_id: pane_slot.tab_id,
        });
        self.repair_focus(Some(pane_slot_id.to_string()));
        Ok(())
    }

    fn close_tab(&mut self, tab_id: &str) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let removed_pane_ids = self.state.spaces[space_index].tabs[tab_index]
            .pane_tree
            .pane_ids();
        self.state.spaces[space_index].tabs.remove(tab_index);
        self.changed_ids.removed_tab_ids.push(tab_id.to_string());
        self.remove_pane_slots_and_contents(&removed_pane_ids);
        self.domain_events.push(DomainEvent::TabClosed {
            tab_id: tab_id.to_string(),
        });

        let preferred_focus = self
            .state
            .focused_pane_id
            .clone()
            .filter(|pane_id| {
                self.state
                    .pane_slots
                    .iter()
                    .any(|pane| pane.pane_slot_id == *pane_id)
            })
            .or_else(|| {
                self.state.spaces[space_index]
                    .tabs
                    .iter()
                    .flat_map(|tab| tab.pane_tree.pane_ids())
                    .find(|pane_id| {
                        self.state
                            .pane_slots
                            .iter()
                            .any(|pane| pane.pane_slot_id == *pane_id)
                    })
            })
            .or_else(|| {
                self.state
                    .pane_slots
                    .first()
                    .map(|pane| pane.pane_slot_id.clone())
            });
        self.repair_focus(preferred_focus);
        Ok(())
    }

    fn set_tab_pinned(&mut self, tab_id: &str, is_pinned: bool) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let mut tab = self.state.spaces[space_index].tabs.remove(tab_index);
        tab.is_pinned = is_pinned;

        let insertion_index = if is_pinned {
            self.state.spaces[space_index]
                .tabs
                .iter()
                .take_while(|tab| tab.is_pinned)
                .count()
        } else {
            self.state.spaces[space_index].tabs.len()
        };
        self.state.spaces[space_index]
            .tabs
            .insert(insertion_index, tab);
        self.changed_ids.updated_tab_ids.push(tab_id.to_string());
        self.domain_events.push(DomainEvent::TabPinChanged {
            tab_id: tab_id.to_string(),
            is_pinned,
        });
        self.repair_focus(self.state.focused_pane_id.clone());
        Ok(())
    }

    fn set_tab_title(
        &mut self,
        tab_id: &str,
        title: Option<String>,
        is_title_user_locked: bool,
        respects_user_title_lock: bool,
    ) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let tab = &mut self.state.spaces[space_index].tabs[tab_index];
        if respects_user_title_lock && tab.is_title_user_locked {
            return Ok(());
        }
        tab.title = title;
        tab.is_title_user_locked = is_title_user_locked;
        self.changed_ids.updated_tab_ids.push(tab_id.to_string());
        self.domain_events.push(DomainEvent::TabTitleChanged {
            tab_id: tab_id.to_string(),
        });
        Ok(())
    }

    fn set_attention(
        &mut self,
        pane_slot_id: &str,
        attention: ShellAttentionState,
    ) -> Result<(), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        for slot in &mut self.state.pane_slots {
            if slot.pane_slot_id == pane_slot_id {
                slot.attention = attention;
            }
        }
        self.changed_ids
            .updated_pane_slot_ids
            .push(pane_slot_id.to_string());
        self.domain_events.push(DomainEvent::AttentionChanged {
            pane_slot_id: pane_slot_id.to_string(),
        });
        self.recompute_space_attention();
        if self.state.focused_pane_id.is_none() {
            self.repair_focus(Some(pane_slot.pane_slot_id));
        }
        Ok(())
    }

    fn require_pane_slot(&self, pane_slot_id: &str) -> Result<&PaneSlot, ReducerError> {
        self.state
            .pane_slots
            .iter()
            .find(|pane| pane.pane_slot_id == pane_slot_id)
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::PaneNotFound, "pane not found"))
    }

    fn space_index(&self, space_id: &str) -> Result<usize, ReducerError> {
        self.state
            .spaces
            .iter()
            .position(|space| space.space_id == space_id)
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::SpaceNotFound, "space not found"))
    }

    fn tab_location(&self, tab_id: &str) -> Result<(usize, usize), ReducerError> {
        for (space_index, space) in self.state.spaces.iter().enumerate() {
            if let Some(tab_index) = space.tabs.iter().position(|tab| tab.tab_id == tab_id) {
                return Ok((space_index, tab_index));
            }
        }
        Err(ReducerError::new(
            ReducerErrorCode::TabNotFound,
            "tab not found",
        ))
    }

    fn find_tab_containing_node(&self, node_id: &str) -> Option<(usize, usize)> {
        for (space_index, space) in self.state.spaces.iter().enumerate() {
            for (tab_index, tab) in space.tabs.iter().enumerate() {
                if tab.pane_tree.contains_node_id(node_id) {
                    return Some((space_index, tab_index));
                }
            }
        }
        None
    }

    fn content_kind(&self, content_id: &str) -> Option<ContentKind> {
        self.state
            .contents
            .iter()
            .find(|content| content.content_id == content_id)
            .map(|content| content.kind)
    }

    fn terminal_payload(
        &self,
        content_id: &str,
    ) -> Option<&crate::model::ShellTerminalContentPayload> {
        self.state
            .contents
            .iter()
            .find(|content| content.content_id == content_id)
            .and_then(|content| content.payload.terminal.as_ref())
    }

    fn pane_slot_id_sources(&self, reserved_pane_slot_ids: Vec<String>) -> Vec<String> {
        let mut ids = self
            .state
            .pane_slots
            .iter()
            .map(|pane| pane.pane_slot_id.clone())
            .collect::<Vec<_>>();
        ids.extend(reserved_pane_slot_ids);
        ids
    }

    fn bootstrap_default_workspace(&mut self, default_working_directory: Option<String>) {
        let space_id = "space_main".to_string();
        let tab_id = "tab_main".to_string();
        let pane_slot_id = "pane_1".to_string();
        let content_id = terminal_content_id(&pane_slot_id);
        let working_directory = default_working_directory.filter(|cwd| !cwd.trim().is_empty());

        self.state.focused_space_id = Some(space_id.clone());
        self.state.focused_tab_id = Some(tab_id.clone());
        self.state.focused_pane_id = Some(pane_slot_id.clone());
        self.state.spaces = vec![Space {
            space_id: space_id.clone(),
            title: "Terminal".to_string(),
            attention: ShellAttentionState::Active,
            tabs: vec![Tab {
                tab_id: tab_id.clone(),
                kind: TabKind::Terminal,
                title: Some("Shell".to_string()),
                pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
                zoomed_pane_id: None,
                is_pinned: false,
                is_title_user_locked: false,
            }],
            selected_tab_id: Some(tab_id.clone()),
            terminal_profile_id: None,
            presentation_icon: None,
        }];
        self.state.pane_slots = vec![PaneSlot {
            pane_slot_id: pane_slot_id.clone(),
            tab_id: tab_id.clone(),
            space_id,
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        }];
        self.state.contents = vec![terminal_content(
            &content_id,
            Some("Shell"),
            working_directory.as_deref(),
            None,
        )];
        self.changed_ids
            .created_space_ids
            .push("space_main".to_string());
        self.changed_ids.created_tab_ids.push(tab_id);
        self.changed_ids.created_pane_slot_ids.push(pane_slot_id);
        self.changed_ids.created_content_ids.push(content_id);
    }

    fn active_task_protected_tab_ids(&self, space_id: &str) -> Vec<String> {
        self.state
            .pane_slots
            .iter()
            .filter(|slot| slot.space_id == space_id)
            .filter_map(|slot| {
                self.state
                    .contents
                    .iter()
                    .find(|content| content.content_id == slot.content_id)
                    .and_then(|content| content.terminal_metadata.as_ref())
                    .is_some_and(|metadata| metadata.active_task_state.protects_from_pruning())
                    .then_some(slot.tab_id.clone())
            })
            .collect()
    }

    fn target_pane_id_for_tab(&self, tab: &Tab) -> Result<String, ReducerError> {
        if let Some(focused_pane_id) = &self.state.focused_pane_id
            && tab.pane_tree.contains_pane_id(focused_pane_id)
        {
            return Ok(focused_pane_id.clone());
        }
        tab.pane_tree
            .pane_ids()
            .into_iter()
            .find(|pane_id| {
                self.state
                    .pane_slots
                    .iter()
                    .any(|slot| slot.pane_slot_id == *pane_id && slot.tab_id == tab.tab_id)
            })
            .or_else(|| tab.pane_tree.pane_ids().into_iter().next())
            .ok_or_else(|| ReducerError::new(ReducerErrorCode::PaneNotFound, "tab has no pane"))
    }

    fn remove_pane_slots_and_contents(&mut self, pane_slot_ids: &[String]) {
        let pane_slot_ids: BTreeSet<_> = pane_slot_ids.iter().cloned().collect();
        let removed_slots: Vec<_> = self
            .state
            .pane_slots
            .iter()
            .filter(|slot| pane_slot_ids.contains(&slot.pane_slot_id))
            .cloned()
            .collect();
        let removed_content_ids: BTreeSet<_> = removed_slots
            .iter()
            .map(|slot| slot.content_id.clone())
            .collect();
        let removed_terminal_content: Vec<_> = self
            .state
            .contents
            .iter()
            .filter(|content| removed_content_ids.contains(&content.content_id))
            .filter(|content| content.kind == ContentKind::Terminal)
            .map(|content| content.content_id.clone())
            .collect();

        self.state
            .pane_slots
            .retain(|slot| !pane_slot_ids.contains(&slot.pane_slot_id));
        self.state
            .contents
            .retain(|content| !removed_content_ids.contains(&content.content_id));

        for slot in &removed_slots {
            self.changed_ids
                .removed_pane_slot_ids
                .push(slot.pane_slot_id.clone());
            self.changed_ids
                .removed_content_ids
                .push(slot.content_id.clone());
            if removed_terminal_content.contains(&slot.content_id) {
                self.runtime_intents.push(RuntimeIntent::CloseTerminal {
                    pane_slot_id: slot.pane_slot_id.clone(),
                    content_id: slot.content_id.clone(),
                });
            }
        }
    }

    fn repair_focus(&mut self, preferred_pane_slot_id: Option<String>) {
        let resolved_pane_slot_id = preferred_pane_slot_id
            .filter(|pane_slot_id| {
                self.state
                    .pane_slots
                    .iter()
                    .any(|slot| slot.pane_slot_id == *pane_slot_id)
            })
            .or_else(|| {
                self.state
                    .pane_slots
                    .first()
                    .map(|slot| slot.pane_slot_id.clone())
            });

        let focused_slot = resolved_pane_slot_id.as_ref().and_then(|pane_slot_id| {
            self.state
                .pane_slots
                .iter()
                .find(|slot| slot.pane_slot_id == *pane_slot_id)
        });
        self.state.focused_space_id =
            focused_slot.map(|slot| slot.space_id.clone()).or_else(|| {
                self.state
                    .spaces
                    .first()
                    .map(|space| space.space_id.clone())
            });
        self.state.focused_tab_id = focused_slot.map(|slot| slot.tab_id.clone()).or_else(|| {
            self.state
                .spaces
                .first()
                .and_then(|space| space.tabs.first())
                .map(|tab| tab.tab_id.clone())
        });
        self.state.focused_pane_id = resolved_pane_slot_id;

        for space in &mut self.state.spaces {
            let preferred_tab_id =
                if self.state.focused_space_id.as_deref() == Some(&space.space_id) {
                    self.state.focused_tab_id.clone()
                } else {
                    None
                };
            space.selected_tab_id = preferred_tab_id
                .filter(|tab_id| space.tabs.iter().any(|tab| tab.tab_id == *tab_id))
                .or_else(|| {
                    space
                        .selected_tab_id
                        .clone()
                        .filter(|tab_id| space.tabs.iter().any(|tab| tab.tab_id == *tab_id))
                })
                .or_else(|| space.tabs.first().map(|tab| tab.tab_id.clone()));
        }
        self.recompute_space_attention();
    }

    fn reconcile_zoom_state(&mut self) {
        let pane_tab_by_id: BTreeMap<_, _> = self
            .state
            .pane_slots
            .iter()
            .map(|slot| (slot.pane_slot_id.clone(), slot.tab_id.clone()))
            .collect();
        let focused_pane_id = self.state.focused_pane_id.clone();
        let focused_tab_id = focused_pane_id
            .as_ref()
            .and_then(|pane_id| pane_tab_by_id.get(pane_id))
            .cloned();
        let mut zoom_changes = Vec::new();

        for space in &mut self.state.spaces {
            for tab in &mut space.tabs {
                let original = tab.zoomed_pane_id.clone();
                let mut next = original.clone().filter(|pane_id| {
                    tab.pane_tree.pane_ids().len() > 1
                        && tab.pane_tree.contains_pane_id(pane_id)
                        && pane_tab_by_id
                            .get(pane_id)
                            .is_some_and(|slot_tab_id| slot_tab_id == &tab.tab_id)
                });

                if next.is_some()
                    && focused_tab_id.as_deref() == Some(&tab.tab_id)
                    && let Some(focused_pane_id) = &focused_pane_id
                    && tab.pane_tree.contains_pane_id(focused_pane_id)
                {
                    next = Some(focused_pane_id.clone());
                }

                if next != original {
                    tab.zoomed_pane_id = next.clone();
                    zoom_changes.push((tab.tab_id.clone(), next));
                }
            }
        }

        for (tab_id, pane_slot_id) in zoom_changes {
            self.changed_ids.updated_tab_ids.push(tab_id.clone());
            self.domain_events.push(DomainEvent::PaneZoomChanged {
                tab_id,
                pane_slot_id,
            });
        }
    }

    fn recompute_space_attention(&mut self) {
        for space in &mut self.state.spaces {
            space.attention = self
                .state
                .pane_slots
                .iter()
                .filter(|pane| pane.space_id == space.space_id)
                .map(|pane| pane.attention)
                .max_by_key(|attention| attention_rank(*attention))
                .unwrap_or(ShellAttentionState::Idle);
            self.changed_ids
                .updated_space_ids
                .push(space.space_id.clone());
        }
    }
}

impl WorkspaceState {
    fn focus(&self) -> ReducerFocus {
        ReducerFocus {
            space_id: self.focused_space_id.clone(),
            tab_id: self.focused_tab_id.clone(),
            pane_slot_id: self.focused_pane_id.clone(),
        }
    }
}

fn terminal_content_id(pane_slot_id: &str) -> String {
    format!("content_{pane_slot_id}")
}

fn insertion_index_for_section(tabs: &[Tab], is_pinned: bool, section_index: usize) -> usize {
    if is_pinned {
        return section_index.min(tabs.iter().filter(|tab| tab.is_pinned).count());
    }

    let pinned_count = tabs.iter().filter(|tab| tab.is_pinned).count();
    let unpinned_count = tabs.len().saturating_sub(pinned_count);
    pinned_count + section_index.min(unpinned_count)
}

fn spatial_direction_for_placement(placement: SplitPlacement) -> SpatialFocusDirection {
    match placement {
        SplitPlacement::Left => SpatialFocusDirection::Left,
        SplitPlacement::Right => SpatialFocusDirection::Right,
        SplitPlacement::Up => SpatialFocusDirection::Up,
        SplitPlacement::Down => SpatialFocusDirection::Down,
    }
}

fn terminal_content(
    content_id: &str,
    title: Option<&str>,
    cwd: Option<&str>,
    terminal_profile_id: Option<&str>,
) -> ContentInstance {
    let title = title.unwrap_or("Shell");
    let payload = if cwd.is_some() || terminal_profile_id.is_some() {
        ShellContentPayload::terminal_with_profile(
            ShellLaunchTarget::Shell,
            cwd,
            Some(title),
            terminal_profile_id,
        )
    } else {
        ShellContentPayload::default()
    };
    ContentInstance {
        content_id: content_id.to_string(),
        kind: ContentKind::Terminal,
        title: title.to_string(),
        icon_name: None,
        capabilities: ContentKind::Terminal.default_capabilities(),
        payload,
        terminal_metadata: None,
        lifecycle: ContentLifecycleState::Active,
        renderer_state: Default::default(),
    }
}

fn default_space_title_from_working_directory(
    working_directory: Option<&str>,
    space_index: usize,
) -> String {
    let derived = working_directory
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let mut components = path
                .split('/')
                .filter(|component| !component.is_empty())
                .collect::<Vec<_>>();
            if components.last() == Some(&".git") {
                components.pop();
            }
            components.last().copied().unwrap_or_default().to_string()
        })
        .unwrap_or_default();
    if derived.is_empty() {
        format!("Space {space_index}")
    } else {
        derived
    }
}

fn supported_presentation_icon(system_name: Option<String>) -> Option<String> {
    let trimmed = system_name?.trim().to_string();
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    trimmed
        .chars()
        .all(|character| character.is_alphanumeric() || matches!(character, '.' | '-' | '_'))
        .then_some(trimmed)
}

fn visible_tab_title(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped = collapsed.chars().take(80).collect::<String>();
    let title = clipped.trim().to_string();
    (!title.is_empty()).then_some(title)
}

fn sorted_unique(values: &[String]) -> Vec<String> {
    values
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn next_id<S>(prefix: &str, existing: impl Iterator<Item = S>) -> String
where
    S: AsRef<str>,
{
    let existing: Vec<_> = existing.collect();
    let next_ordinal = existing
        .iter()
        .filter_map(|identifier| identifier.as_ref().rsplit('_').next())
        .filter_map(|ordinal| ordinal.parse::<u64>().ok())
        .max()
        .map(|ordinal| ordinal + 1)
        .unwrap_or_else(|| {
            if existing.is_empty() {
                1
            } else {
                existing.len() as u64 + 1
            }
        });
    format!("{prefix}_{next_ordinal}")
}

fn attention_rank(attention: ShellAttentionState) -> u8 {
    match attention {
        ShellAttentionState::Idle => 0,
        ShellAttentionState::Active => 1,
        ShellAttentionState::Notable => 2,
        ShellAttentionState::AwaitingUser => 3,
    }
}
