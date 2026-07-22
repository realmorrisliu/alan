mod projection;

use crate::{
    ContentInstance, ContentKind, PaneSlot, ReducerError, ReducerOperation, RuntimeIntent,
    ShellAttentionState, Space, SpatialFocusDirection, SplitDirection, SplitPlacement, Tab,
    TabOrganizationSection, WorkspaceState,
};
use projection::{
    ResponseProjection, contents_in_tab, fill_content_projection, pane_slot_id_from_command,
    pane_slots_in_tab, placement_for_split_direction, project_runtime_intent_response,
    project_success_response, reducer_error_projection, tabs_in_space, terminal_target,
};
use serde::{Deserialize, Serialize};

/// Stable shell control command names accepted by the portable control reducer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellControlCommandKind {
    /// Read the current state.
    #[serde(rename = "state")]
    State,
    /// List Spaces.
    #[serde(rename = "space.list")]
    SpaceList,
    /// Create a Space.
    #[serde(rename = "space.create")]
    SpaceCreate,
    /// List tabs.
    #[serde(rename = "tab.list")]
    TabList,
    /// Open a terminal tab.
    #[serde(rename = "tab.open")]
    TabOpen,
    /// Close a tab.
    #[serde(rename = "tab.close")]
    TabClose,
    /// Pin a tab.
    #[serde(rename = "tab.pin")]
    TabPin,
    /// Unpin a tab.
    #[serde(rename = "tab.unpin")]
    TabUnpin,
    /// Move a tab within its organization section.
    #[serde(rename = "tab.reorder")]
    TabReorder,
    /// Move a tab to another Space.
    #[serde(rename = "tab.move_to_space")]
    TabMoveToSpace,
    /// List pane slots and content.
    #[serde(rename = "pane.list")]
    PaneList,
    /// Split a pane.
    #[serde(rename = "pane.split")]
    PaneSplit,
    /// Close a pane.
    #[serde(rename = "pane.close")]
    PaneClose,
    /// Move pane to a new tab.
    #[serde(rename = "pane.lift")]
    PaneLift,
    /// Move pane to a target tab.
    #[serde(rename = "pane.move")]
    PaneMove,
    /// Move pane within its current tab.
    #[serde(rename = "pane.move_within_tab")]
    PaneMoveWithinTab,
    /// Focus a pane.
    #[serde(rename = "pane.focus")]
    PaneFocus,
    /// Focus spatially.
    #[serde(rename = "pane.spatial_focus")]
    PaneSpatialFocus,
    /// Resize split.
    #[serde(rename = "pane.resize_split")]
    PaneResizeSplit,
    /// Equalize splits.
    #[serde(rename = "pane.equalize_splits")]
    PaneEqualizeSplits,
    /// Zoom pane.
    #[serde(rename = "pane.zoom")]
    PaneZoom,
    /// Unzoom tab.
    #[serde(rename = "pane.unzoom")]
    PaneUnzoom,
    /// Send terminal text.
    #[serde(rename = "terminal.send_text")]
    TerminalSendText,
    /// Send terminal key.
    #[serde(rename = "terminal.send_key")]
    TerminalSendKey,
    /// Set pane attention.
    #[serde(rename = "attention.set")]
    AttentionSet,
}

/// Portable shell control command DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellControlCommand {
    /// Request id.
    pub request_id: String,
    /// Command kind.
    pub command: ShellControlCommandKind,
    /// Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    /// Target Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_space_id: Option<String>,
    /// Tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    /// Pane slot id, compatible with current pane ids.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    /// Explicit pane slot id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_slot_id: Option<String>,
    /// Content id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// Split node id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_node_id: Option<String>,
    /// Split ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Reorder index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// Target pinned/unpinned section for tab organization (e.g. `tab.reorder`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<TabOrganizationSection>,
    /// Split direction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<SplitDirection>,
    /// Spatial focus direction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spatial_direction: Option<SpatialFocusDirection>,
    /// Pane move placement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<SplitPlacement>,
    /// Title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Text for terminal.send_text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Key for terminal.send_key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Attention state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention: Option<ShellAttentionState>,
    /// Terminal Profile id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_profile_id: Option<String>,
}

/// Runtime collision inputs supplied by a shell host without transferring command ownership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellControlExecutionContext {
    /// Pane slot ids that remain reserved by live platform runtimes during state reconciliation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reserved_pane_slot_ids: Vec<String>,
}

/// Portable shell control result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellControlResult {
    /// Response projected for clients.
    pub response: ShellControlResponse,
    /// Updated state when command changed state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_state: Option<WorkspaceState>,
    /// Core runtime intents that a platform adapter must execute.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_intents: Vec<ShellControlRuntimeIntent>,
}

/// Portable shell control response projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellControlResponse {
    /// Request id.
    pub request_id: String,
    /// Workspace contract version.
    pub contract_version: String,
    /// Whether the domain command applied immediately.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<bool>,
    /// Optional state snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<WorkspaceState>,
    /// Space list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spaces: Option<Vec<Space>>,
    /// Tab list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tabs: Option<Vec<Tab>>,
    /// Pane slot list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_slots: Option<Vec<PaneSlot>>,
    /// Content list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<Vec<ContentInstance>>,
    /// Focused pane slot id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_pane_slot_id: Option<String>,
    /// Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    /// Target Space id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_space_id: Option<String>,
    /// Tab id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    /// Pane id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    /// Pane slot id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_slot_id: Option<String>,
    /// Content id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,
    /// Content kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<ContentKind>,
    /// Split node id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_node_id: Option<String>,
    /// Split ratio.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Changed split ids.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_split_ids: Option<Vec<String>>,
    /// Zoomed pane id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoomed_pane_id: Option<String>,
    /// Previous focused pane slot id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_focused_pane_slot_id: Option<String>,
    /// Current focused pane slot id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_focused_pane_slot_id: Option<String>,
    /// Placement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<SplitPlacement>,
    /// Resulting pinned/unpinned section after a tab organization mutation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<TabOrganizationSection>,
    /// Resulting index within the target section after a tab organization mutation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// Stable error code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Stable error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Runtime-dependent control intent for platform adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellControlRuntimeIntent {
    /// Send terminal text.
    SendTerminalText {
        /// Pane slot id.
        pane_slot_id: String,
        /// Content id.
        content_id: String,
        /// Text.
        text: String,
    },
    /// Send terminal key.
    SendTerminalKey {
        /// Pane slot id.
        pane_slot_id: String,
        /// Content id.
        content_id: String,
        /// Key.
        key: TerminalControlKey,
    },
    /// Runtime intent emitted by workspace reducer.
    Reducer {
        /// Pure reducer intent.
        intent: RuntimeIntent,
    },
}

/// Supported terminal control keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalControlKey {
    /// Return key.
    Return,
}

impl WorkspaceState {
    /// Applies a shell control command to this workspace state.
    pub fn reduce_control(&self, command: ShellControlCommand) -> ShellControlResult {
        self.reduce_control_with_context(command, ShellControlExecutionContext::default())
    }

    /// Applies a shell control command with runtime collision inputs supplied by its host.
    pub fn reduce_control_with_context(
        &self,
        command: ShellControlCommand,
        context: ShellControlExecutionContext,
    ) -> ShellControlResult {
        ShellControlReducer::new(self.clone(), context).apply(command)
    }
}

struct ShellControlReducer {
    state: WorkspaceState,
    context: ShellControlExecutionContext,
}

impl ShellControlReducer {
    fn new(state: WorkspaceState, context: ShellControlExecutionContext) -> Self {
        Self { state, context }
    }

    fn apply(&self, command: ShellControlCommand) -> ShellControlResult {
        match command.command {
            ShellControlCommandKind::State => self.read_state(command),
            ShellControlCommandKind::SpaceList => self.space_list(command),
            ShellControlCommandKind::TabList => self.tab_list(command),
            ShellControlCommandKind::PaneList => self.pane_list(command),
            ShellControlCommandKind::SpaceCreate => self.apply_reducer(
                command.clone(),
                ReducerOperation::CreateTerminalSpace {
                    title: command.title.clone(),
                    tab_title: None,
                    working_directory: command.cwd.clone(),
                    terminal_profile_id: command.terminal_profile_id,
                    presentation_icon: None,
                    reserved_pane_slot_ids: self.context.reserved_pane_slot_ids.clone(),
                },
                ResponseProjection::Current,
            ),
            ShellControlCommandKind::TabOpen => self.apply_reducer(
                command.clone(),
                ReducerOperation::OpenTerminalTab {
                    space_id: command.space_id.clone(),
                    title: command.title.clone(),
                    working_directory: command.cwd.clone(),
                    terminal_profile_id: command.terminal_profile_id,
                    reserved_pane_slot_ids: self.context.reserved_pane_slot_ids.clone(),
                },
                ResponseProjection::Current,
            ),
            ShellControlCommandKind::TabClose => {
                let Some(tab_id) = command.tab_id.clone() else {
                    return self.validation_error(command, "tab_required", "tab_id is required.");
                };
                let projection = ResponseProjection::removed_tab(&self.state, &tab_id);
                self.apply_reducer(command, ReducerOperation::CloseTab { tab_id }, projection)
            }
            ShellControlCommandKind::TabPin => {
                let Some(tab_id) = command
                    .tab_id
                    .clone()
                    .or_else(|| self.state.focused_tab_id.clone())
                else {
                    return self.validation_error(command, "tab_required", "tab_id is required.");
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::PinTab {
                        tab_id: tab_id.clone(),
                    },
                    ResponseProjection::TargetTab(tab_id),
                )
            }
            ShellControlCommandKind::TabUnpin => {
                let Some(tab_id) = command
                    .tab_id
                    .clone()
                    .or_else(|| self.state.focused_tab_id.clone())
                else {
                    return self.validation_error(command, "tab_required", "tab_id is required.");
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::UnpinTab {
                        tab_id: tab_id.clone(),
                    },
                    ResponseProjection::TargetTab(tab_id),
                )
            }
            ShellControlCommandKind::TabReorder => self.tab_reorder(command),
            ShellControlCommandKind::TabMoveToSpace => self.tab_move_to_space(command),
            ShellControlCommandKind::PaneSplit => self.pane_split(command),
            ShellControlCommandKind::PaneClose => {
                let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
                    return self.validation_error(command, "pane_required", "pane_id is required.");
                };
                let projection = ResponseProjection::removed_pane(&self.state, &pane_slot_id);
                self.apply_reducer(
                    command,
                    ReducerOperation::ClosePane { pane_slot_id },
                    projection,
                )
            }
            ShellControlCommandKind::PaneLift => {
                let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
                    return self.validation_error(command, "pane_required", "pane_id is required.");
                };
                self.apply_reducer(
                    command.clone(),
                    ReducerOperation::MovePaneToNewTab {
                        pane_slot_id,
                        title: command.title,
                    },
                    ResponseProjection::Current,
                )
            }
            ShellControlCommandKind::PaneMove => self.pane_move(command),
            ShellControlCommandKind::PaneMoveWithinTab => self.pane_move_within_tab(command),
            ShellControlCommandKind::PaneFocus => {
                let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
                    return self.validation_error(command, "pane_required", "pane_id is required.");
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::FocusPane { pane_slot_id },
                    ResponseProjection::Focus,
                )
            }
            ShellControlCommandKind::PaneSpatialFocus => {
                let Some(direction) = command.spatial_direction else {
                    return self.validation_error(
                        command,
                        "spatial_direction_required",
                        "spatial_direction is required.",
                    );
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::FocusAdjacentPane { direction },
                    ResponseProjection::Focus,
                )
            }
            ShellControlCommandKind::PaneResizeSplit => self.pane_resize_split(command),
            ShellControlCommandKind::PaneEqualizeSplits => self.pane_equalize_splits(command),
            ShellControlCommandKind::PaneZoom => self.pane_zoom(command),
            ShellControlCommandKind::PaneUnzoom => self.pane_unzoom(command),
            ShellControlCommandKind::TerminalSendText => self.terminal_send_text(command),
            ShellControlCommandKind::TerminalSendKey => self.terminal_send_key(command),
            ShellControlCommandKind::AttentionSet => {
                let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
                    return self.validation_error(
                        command,
                        "attention_target_required",
                        "pane_id and attention are required.",
                    );
                };
                let Some(attention) = command.attention else {
                    return self.validation_error(
                        command,
                        "attention_target_required",
                        "pane_id and attention are required.",
                    );
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::SetAttention {
                        pane_slot_id: pane_slot_id.clone(),
                        attention,
                    },
                    ResponseProjection::TargetPane(pane_slot_id),
                )
            }
        }
    }

    fn read_state(&self, command: ShellControlCommand) -> ShellControlResult {
        let mut response = self.response(&command, Some(true));
        response.state = Some(self.state.clone());
        response.pane_slots = Some(self.state.pane_slots.clone());
        response.contents = Some(self.state.contents.clone());
        response.space_id = self.state.focused_space_id.clone();
        response.tab_id = self.state.focused_tab_id.clone();
        response.pane_id = self.state.focused_pane_id.clone();
        response.pane_slot_id = self.state.focused_pane_id.clone();
        fill_content_projection(&mut response, &self.state);
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: Vec::new(),
        }
    }

    fn space_list(&self, command: ShellControlCommand) -> ShellControlResult {
        let mut response = self.response(&command, Some(true));
        response.spaces = Some(self.state.spaces.clone());
        response.space_id = command
            .space_id
            .or_else(|| self.state.focused_space_id.clone());
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: Vec::new(),
        }
    }

    fn tab_list(&self, command: ShellControlCommand) -> ShellControlResult {
        let mut response = self.response(&command, Some(true));
        response.tabs = Some(tabs_in_space(&self.state, command.space_id.as_deref()));
        response.space_id = command
            .space_id
            .or_else(|| self.state.focused_space_id.clone());
        response.tab_id = self.state.focused_tab_id.clone();
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: Vec::new(),
        }
    }

    fn pane_list(&self, command: ShellControlCommand) -> ShellControlResult {
        let mut response = self.response(&command, Some(true));
        response.pane_slots = Some(pane_slots_in_tab(&self.state, command.tab_id.as_deref()));
        response.contents = Some(contents_in_tab(&self.state, command.tab_id.as_deref()));
        response.tab_id = command.tab_id.or_else(|| self.state.focused_tab_id.clone());
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: Vec::new(),
        }
    }

    fn tab_reorder(&self, command: ShellControlCommand) -> ShellControlResult {
        let (Some(tab_id), Some(section), Some(index)) =
            (command.tab_id.clone(), command.section, command.index)
        else {
            return self.validation_error(
                command,
                "tab_reorder_target_required",
                "tab_id, section, and index are required.",
            );
        };
        let target_space_id = command
            .target_space_id
            .clone()
            .or_else(|| command.space_id.clone());
        // `OrganizeTab` uses an absolute index within the requested target Space/section.
        let mut result = self.apply_reducer(
            command,
            ReducerOperation::OrganizeTab {
                tab_id: tab_id.clone(),
                target_space_id: target_space_id.clone(),
                section,
                index: Some(index),
            },
            ResponseProjection::TargetTab(tab_id),
        );
        result.response.target_space_id = target_space_id;
        result
    }

    fn tab_move_to_space(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(tab_id) = command.tab_id.clone() else {
            return self.validation_error(
                command,
                "tab_move_target_required",
                "tab_id and target_space_id are required.",
            );
        };
        let Some(target_space_id) = command
            .target_space_id
            .clone()
            .or_else(|| command.space_id.clone())
        else {
            return self.validation_error(
                command,
                "tab_move_target_required",
                "tab_id and target_space_id are required.",
            );
        };
        let mut result = self.apply_reducer(
            command,
            ReducerOperation::MoveTabToSpace {
                tab_id: tab_id.clone(),
                target_space_id: target_space_id.clone(),
            },
            ResponseProjection::TargetTab(tab_id),
        );
        result.response.target_space_id = Some(target_space_id);
        result
    }

    fn pane_split(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
            return self.validation_error(command, "pane_required", "pane_id is required.");
        };
        let Some(direction) = command.direction else {
            return self.validation_error(
                command,
                "direction_required",
                "direction is required for pane.split.",
            );
        };
        self.apply_reducer(
            command.clone(),
            ReducerOperation::SplitPane {
                pane_slot_id,
                placement: placement_for_split_direction(direction),
                title: command.title.clone(),
                working_directory: command.cwd.clone(),
                terminal_profile_id: command.terminal_profile_id,
                reserved_pane_slot_ids: self.context.reserved_pane_slot_ids.clone(),
            },
            ResponseProjection::Snapshot,
        )
    }

    fn pane_move(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
            return self.validation_error(
                command,
                "pane_move_target_required",
                "pane_id and tab_id are required.",
            );
        };
        let Some(target_tab_id) = command.tab_id.clone() else {
            return self.validation_error(
                command,
                "pane_move_target_required",
                "pane_id and tab_id are required.",
            );
        };
        self.apply_reducer(
            command.clone(),
            ReducerOperation::MovePaneToTab {
                pane_slot_id,
                target_tab_id,
                direction: command.direction.unwrap_or(SplitDirection::Vertical),
            },
            ResponseProjection::Current,
        )
    }

    fn pane_move_within_tab(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
            return self.validation_error(command, "pane_required", "pane_id is required.");
        };
        let Some(placement) = command.placement else {
            return self.validation_error(
                command,
                "placement_required",
                "placement is required for pane.move_within_tab.",
            );
        };
        self.apply_reducer(
            command,
            ReducerOperation::MovePaneWithinTab {
                pane_slot_id,
                placement,
            },
            ResponseProjection::MovePaneWithinTab(placement),
        )
    }

    fn pane_resize_split(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(split_node_id) = command.split_node_id.clone() else {
            return self.validation_error(
                command,
                "split_node_required",
                "split_node_id is required.",
            );
        };
        let Some(ratio) = command.ratio else {
            return self.validation_error(command, "ratio_required", "ratio is required.");
        };
        self.apply_reducer(
            command,
            ReducerOperation::ResizeSplit {
                split_node_id,
                ratio,
            },
            ResponseProjection::ResizeSplit,
        )
    }

    fn pane_equalize_splits(&self, mut command: ShellControlCommand) -> ShellControlResult {
        let Some(tab_id) = command
            .tab_id
            .clone()
            .or_else(|| self.state.focused_tab_id.clone())
        else {
            return self.validation_error(command, "tab_required", "tab_id is required.");
        };
        command.tab_id = Some(tab_id.clone());
        let Some(tab) = self.state.tab(&tab_id) else {
            return self.validation_error(
                command,
                "tab_not_found",
                "The requested tab does not exist.",
            );
        };
        if tab.pane_tree.split_ratios_by_node_id().is_empty() {
            return self.validation_error(
                command,
                "no_split_branches",
                "The requested tab does not have split branches.",
            );
        }
        let equalized_tree = tab.pane_tree.equalized_splits();
        let changed_split_ids = equalized_tree.split_node_ids_with_changed_ratios(&tab.pane_tree);
        if changed_split_ids.is_empty() {
            return self.validation_error(
                command,
                "unchanged_state",
                "The requested split ratios are already equalized.",
            );
        }

        let mut result = self.apply_reducer(
            command,
            ReducerOperation::EqualizeSplits {
                tab_id: Some(tab_id.clone()),
            },
            ResponseProjection::TabSubject(tab_id),
        );
        result.response.ratio = Some(0.5);
        result.response.changed_split_ids = Some(changed_split_ids);
        result
    }

    fn pane_zoom(&self, command: ShellControlCommand) -> ShellControlResult {
        let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
            return self.validation_error(command, "pane_required", "pane_id is required.");
        };
        let Some(pane_slot) = self
            .state
            .pane_slots
            .iter()
            .find(|candidate| candidate.pane_slot_id == pane_slot_id)
        else {
            return self.validation_error(
                command,
                "pane_not_found",
                "The requested pane does not exist.",
            );
        };
        let Some(tab) = self.state.tab(&pane_slot.tab_id) else {
            return self.validation_error(
                command,
                "tab_not_found",
                "The requested tab does not exist.",
            );
        };
        if tab.pane_tree.pane_ids().len() <= 1 {
            return self.validation_error(
                command,
                "split_tab_required",
                "Pane zoom requires a split tab.",
            );
        }
        if tab.zoomed_pane_id.as_deref() == Some(pane_slot_id.as_str()) {
            return self.validation_error(
                command,
                "unchanged_state",
                "The requested pane is already zoomed.",
            );
        }
        let tab_id = pane_slot.tab_id.clone();
        self.apply_reducer(
            command,
            ReducerOperation::ZoomPane {
                pane_slot_id: pane_slot_id.clone(),
            },
            ResponseProjection::Zoom {
                tab_id,
                pane_slot_id,
            },
        )
    }

    fn pane_unzoom(&self, command: ShellControlCommand) -> ShellControlResult {
        let requested_pane_slot_id = pane_slot_id_from_command(&command);
        let requested_pane_slot = requested_pane_slot_id.as_ref().and_then(|pane_slot_id| {
            self.state
                .pane_slots
                .iter()
                .find(|candidate| &candidate.pane_slot_id == pane_slot_id)
        });
        if requested_pane_slot_id.is_some() && requested_pane_slot.is_none() {
            return self.validation_error(
                command,
                "pane_not_found",
                "The requested pane does not exist.",
            );
        }
        let Some(tab_id) = command
            .tab_id
            .clone()
            .or_else(|| requested_pane_slot.map(|pane_slot| pane_slot.tab_id.clone()))
            .or_else(|| self.state.focused_tab_id.clone())
        else {
            return self.validation_error(command, "tab_required", "tab_id is required.");
        };
        let Some(tab) = self.state.tab(&tab_id) else {
            return self.validation_error(
                command,
                "tab_not_found",
                "The requested tab does not exist.",
            );
        };
        let Some(previous_zoomed_pane_id) = tab.zoomed_pane_id.clone() else {
            return self.validation_error(
                command,
                "unchanged_state",
                "The requested tab is not zoomed.",
            );
        };
        self.apply_reducer(
            command,
            ReducerOperation::UnzoomTab {
                tab_id: Some(tab_id.clone()),
            },
            ResponseProjection::Zoom {
                tab_id,
                pane_slot_id: previous_zoomed_pane_id,
            },
        )
    }

    fn terminal_send_text(&self, command: ShellControlCommand) -> ShellControlResult {
        let target = match terminal_target(&self.state, &command) {
            Ok(target) => target,
            Err((code, message)) => return self.validation_error(command, code, message),
        };
        let mut response = self.response(&command, None);
        response.space_id = Some(target.pane_slot.space_id.clone());
        response.tab_id = Some(target.pane_slot.tab_id.clone());
        response.pane_id = Some(target.pane_slot.pane_slot_id.clone());
        response.pane_slot_id = Some(target.pane_slot.pane_slot_id.clone());
        response.content_id = Some(target.content.content_id.clone());
        response.content_kind = Some(target.content.kind);
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: vec![ShellControlRuntimeIntent::SendTerminalText {
                pane_slot_id: target.pane_slot.pane_slot_id,
                content_id: target.content.content_id,
                text: command.text.unwrap_or_default(),
            }],
        }
    }

    fn terminal_send_key(&self, command: ShellControlCommand) -> ShellControlResult {
        let target = match terminal_target(&self.state, &command) {
            Ok(target) => target,
            Err((code, message)) => return self.validation_error(command, code, message),
        };
        let key = match command
            .key
            .as_deref()
            .map(str::trim)
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("return" | "enter") => TerminalControlKey::Return,
            _ => {
                return self.validation_error(
                    command,
                    "terminal_key_unsupported",
                    "terminal.send_key currently supports return.",
                );
            }
        };
        let mut response = self.response(&command, None);
        response.space_id = Some(target.pane_slot.space_id.clone());
        response.tab_id = Some(target.pane_slot.tab_id.clone());
        response.pane_id = Some(target.pane_slot.pane_slot_id.clone());
        response.pane_slot_id = Some(target.pane_slot.pane_slot_id.clone());
        response.content_id = Some(target.content.content_id.clone());
        response.content_kind = Some(target.content.kind);
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: vec![ShellControlRuntimeIntent::SendTerminalKey {
                pane_slot_id: target.pane_slot.pane_slot_id,
                content_id: target.content.content_id,
                key,
            }],
        }
    }

    fn apply_reducer(
        &self,
        command: ShellControlCommand,
        operation: ReducerOperation,
        projection: ResponseProjection,
    ) -> ShellControlResult {
        match self.state.reduce(operation) {
            Ok(result) => {
                let mut response = self.response(&command, Some(true));
                project_success_response(&mut response, &command, &result.state, &projection);
                for runtime_intent in &result.runtime_intents {
                    project_runtime_intent_response(&mut response, runtime_intent);
                }
                fill_content_projection(&mut response, &result.state);
                ShellControlResult {
                    response,
                    updated_state: Some(result.state),
                    runtime_intents: result
                        .runtime_intents
                        .into_iter()
                        .map(|intent| ShellControlRuntimeIntent::Reducer { intent })
                        .collect(),
                }
            }
            Err(error) => self.reducer_error(command, error),
        }
    }

    fn validation_error(
        &self,
        command: ShellControlCommand,
        code: &str,
        message: &str,
    ) -> ShellControlResult {
        let mut response = self.response(&command, Some(false));
        response.error_code = Some(code.to_string());
        response.error_message = Some(message.to_string());
        response.space_id = command.space_id.clone();
        response.target_space_id = command.target_space_id.clone();
        response.tab_id = command.tab_id.clone();
        response.pane_id = command.pane_id.clone();
        response.pane_slot_id = command
            .pane_slot_id
            .clone()
            .or_else(|| command.pane_id.clone());
        response.content_id = command.content_id;
        fill_content_projection(&mut response, &self.state);
        ShellControlResult {
            response,
            updated_state: None,
            runtime_intents: Vec::new(),
        }
    }

    fn reducer_error(
        &self,
        command: ShellControlCommand,
        error: ReducerError,
    ) -> ShellControlResult {
        let (code, message) = reducer_error_projection(&error);
        self.validation_error(command, code, message)
    }

    fn response(
        &self,
        command: &ShellControlCommand,
        applied: Option<bool>,
    ) -> ShellControlResponse {
        ShellControlResponse {
            request_id: command.request_id.clone(),
            contract_version: self.state.contract_version.clone(),
            applied,
            state: None,
            spaces: None,
            tabs: None,
            pane_slots: None,
            contents: None,
            focused_pane_slot_id: self.state.focused_pane_id.clone(),
            space_id: None,
            target_space_id: None,
            tab_id: None,
            pane_id: None,
            pane_slot_id: None,
            content_id: None,
            content_kind: None,
            split_node_id: None,
            ratio: None,
            changed_split_ids: None,
            zoomed_pane_id: None,
            previous_focused_pane_slot_id: None,
            current_focused_pane_slot_id: None,
            placement: None,
            section: None,
            index: None,
            error_code: None,
            error_message: None,
        }
    }
}
