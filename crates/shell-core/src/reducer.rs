mod content;
mod focus_state;
mod pane_geometry;
mod space_lifecycle;
mod tab_lifecycle;

use crate::{
    AgentContentPresentation, AgentStreamOffsets, ContentInstance, ContentKind,
    ContentLifecycleState, PaneSlot, ShellAttentionState, ShellContentPayload, ShellLaunchTarget,
    ShellTabActiveTaskState, SpatialFocusDirection, SplitDirection, SplitPlacement,
    TabOrganizationSection, TerminalActivitySnapshot, WorkspaceState,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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

    fn content_kind(&self, content_id: &str) -> Option<ContentKind> {
        self.state
            .contents
            .iter()
            .find(|content| content.content_id == content_id)
            .map(|content| content.kind)
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
}

fn terminal_content_id(pane_slot_id: &str) -> String {
    format!("content_{pane_slot_id}")
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
