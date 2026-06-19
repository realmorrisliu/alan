use crate::{
    ContentInstance, ContentKind, PaneSlot, ReducerError, ReducerErrorCode, ReducerOperation,
    RuntimeIntent, ShellAttentionState, Space, SpatialFocusDirection, SplitDirection,
    SplitPlacement, Tab, TabOrganizationSection, WorkspaceState,
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
        ShellControlReducer::new(self.clone()).apply(command)
    }
}

struct ShellControlReducer {
    state: WorkspaceState,
}

impl ShellControlReducer {
    fn new(state: WorkspaceState) -> Self {
        Self { state }
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
                    working_directory: self.state.cwd_from_command(&command),
                    terminal_profile_id: command.terminal_profile_id.clone(),
                    presentation_icon: None,
                    reserved_pane_slot_ids: Vec::new(),
                },
                ResponseProjection::Current,
            ),
            ShellControlCommandKind::TabOpen => self.apply_reducer(
                command.clone(),
                ReducerOperation::OpenTerminalTab {
                    space_id: command.space_id.clone(),
                    title: command.title.clone(),
                    working_directory: command.cwd.clone(),
                    terminal_profile_id: command.terminal_profile_id.clone(),
                    reserved_pane_slot_ids: Vec::new(),
                },
                ResponseProjection::Current,
            ),
            ShellControlCommandKind::TabClose => {
                let Some(tab_id) = command.tab_id.clone() else {
                    return self.validation_error(command, "tab_required", "tab_id is required.");
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::CloseTab { tab_id },
                    ResponseProjection::Current,
                )
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
                self.apply_reducer(
                    command,
                    ReducerOperation::ClosePane { pane_slot_id },
                    ResponseProjection::Current,
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
                        title: command.title.clone(),
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
            ShellControlCommandKind::PaneEqualizeSplits => self.apply_reducer(
                command.clone(),
                ReducerOperation::EqualizeSplits {
                    tab_id: command.tab_id.clone(),
                },
                ResponseProjection::Current,
            ),
            ShellControlCommandKind::PaneZoom => {
                let Some(pane_slot_id) = pane_slot_id_from_command(&command) else {
                    return self.validation_error(command, "pane_required", "pane_id is required.");
                };
                self.apply_reducer(
                    command,
                    ReducerOperation::ZoomPane { pane_slot_id },
                    ResponseProjection::Zoom,
                )
            }
            ShellControlCommandKind::PaneUnzoom => self.apply_reducer(
                command.clone(),
                ReducerOperation::UnzoomTab {
                    tab_id: command.tab_id.clone(),
                },
                ResponseProjection::Zoom,
            ),
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
        // Honor the requested target section so a tab can move between the pinned and unpinned
        // sections, not only within its current one. `OrganizeTab` treats `index` as the
        // absolute position inside the target section.
        self.apply_reducer(
            command,
            ReducerOperation::OrganizeTab {
                tab_id: tab_id.clone(),
                target_space_id: None,
                section,
                index: Some(index),
            },
            ResponseProjection::TargetTab(tab_id),
        )
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
        // Echo the requested destination Space so automation can confirm the move target.
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
                terminal_profile_id: command.terminal_profile_id.clone(),
                reserved_pane_slot_ids: Vec::new(),
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
        response.content_id = command.content_id.clone();
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

#[derive(Debug, Clone)]
enum ResponseProjection {
    Current,
    Snapshot,
    Focus,
    ResizeSplit,
    Zoom,
    MovePaneWithinTab(SplitPlacement),
    /// Report the named pane as the response subject (not the workspace focus), for commands
    /// like `attention.set` that mutate a specific, possibly unfocused, pane.
    TargetPane(String),
    /// Report the named tab as the response subject, for commands like `tab.pin`/`tab.unpin`/
    /// `tab.reorder`/`tab.move_to_space` that mutate a specific, possibly unfocused, tab.
    TargetTab(String),
}

struct TerminalTarget {
    pane_slot: PaneSlot,
    content: ContentInstance,
}

trait ControlWorkspaceExt {
    fn cwd_from_command(&self, command: &ShellControlCommand) -> Option<String>;
}

impl ControlWorkspaceExt for WorkspaceState {
    fn cwd_from_command(&self, command: &ShellControlCommand) -> Option<String> {
        command.cwd.clone()
    }
}

fn project_success_response(
    response: &mut ShellControlResponse,
    command: &ShellControlCommand,
    state: &WorkspaceState,
    projection: &ResponseProjection,
) {
    response.state = matches!(projection, ResponseProjection::Snapshot).then(|| state.clone());
    response.focused_pane_slot_id = state.focused_pane_id.clone();
    response.space_id = state.focused_space_id.clone();
    response.tab_id = state.focused_tab_id.clone();
    response.pane_id = state.focused_pane_id.clone();
    response.pane_slot_id = state.focused_pane_id.clone();
    response.current_focused_pane_slot_id = state.focused_pane_id.clone();
    response.previous_focused_pane_slot_id =
        command.pane_slot_id.clone().or(command.pane_id.clone());

    match projection {
        ResponseProjection::Snapshot => {}
        ResponseProjection::Current | ResponseProjection::Focus => {}
        ResponseProjection::TargetPane(pane_slot_id) => {
            // Echo the mutated pane (and its tab/Space) as the subject so automation sees the
            // object it acted on, not whichever pane happens to hold focus.
            response.pane_id = Some(pane_slot_id.clone());
            response.pane_slot_id = Some(pane_slot_id.clone());
            if let Some(slot) = state
                .pane_slots
                .iter()
                .find(|slot| &slot.pane_slot_id == pane_slot_id)
            {
                response.tab_id = Some(slot.tab_id.clone());
                response.space_id = Some(slot.space_id.clone());
            }
        }
        ResponseProjection::TargetTab(tab_id) => {
            response.tab_id = Some(tab_id.clone());
            if let Some(space) = state
                .spaces
                .iter()
                .find(|space| space.tabs.iter().any(|tab| &tab.tab_id == tab_id))
            {
                response.space_id = Some(space.space_id.clone());
            }
            // Report where the tab landed so automation can confirm an accepted organization
            // mutation (pin/unpin/reorder/move) without a follow-up state read.
            if let Some((section, index)) = tab_section_and_index(state, tab_id) {
                response.section = Some(section);
                response.index = Some(index);
            }
        }
        ResponseProjection::ResizeSplit => {
            response.split_node_id = command.split_node_id.clone();
            response.ratio = command.ratio;
            response.changed_split_ids = command.split_node_id.clone().map(|id| vec![id]);
        }
        ResponseProjection::Zoom => {
            response.zoomed_pane_id = state
                .focused_tab_id
                .as_deref()
                .and_then(|tab_id| state.tab(tab_id))
                .and_then(|tab| tab.zoomed_pane_id.clone());
        }
        ResponseProjection::MovePaneWithinTab(placement) => {
            response.placement = Some(*placement);
        }
    }
}

fn project_runtime_intent_response(
    response: &mut ShellControlResponse,
    runtime_intent: &RuntimeIntent,
) {
    match runtime_intent {
        RuntimeIntent::StartTerminal {
            pane_slot_id,
            content_id,
            ..
        }
        | RuntimeIntent::CloseTerminal {
            pane_slot_id,
            content_id,
        } => {
            response.pane_id = Some(pane_slot_id.clone());
            response.pane_slot_id = Some(pane_slot_id.clone());
            response.content_id = Some(content_id.clone());
        }
    }
}

fn fill_content_projection(response: &mut ShellControlResponse, state: &WorkspaceState) {
    if response.pane_slot_id.is_none()
        && let Some(content_id) = response.content_id.as_deref()
    {
        response.pane_slot_id = state
            .pane_slots
            .iter()
            .find(|slot| slot.content_id == content_id)
            .map(|slot| slot.pane_slot_id.clone());
    }

    if response.content_id.is_none()
        && let Some(pane_slot_id) = response.pane_slot_id.as_deref()
    {
        response.content_id = state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == pane_slot_id)
            .map(|slot| slot.content_id.clone());
    }

    if response.content_kind.is_none()
        && let Some(content_id) = response.content_id.as_deref()
    {
        response.content_kind = state
            .contents
            .iter()
            .find(|content| content.content_id == content_id)
            .map(|content| content.kind);
    }
}

fn pane_slot_id_from_command(command: &ShellControlCommand) -> Option<String> {
    command
        .pane_slot_id
        .clone()
        .or_else(|| command.pane_id.clone())
}

fn tabs_in_space(state: &WorkspaceState, space_id: Option<&str>) -> Vec<Tab> {
    match space_id {
        Some(space_id) => state
            .spaces
            .iter()
            .find(|space| space.space_id == space_id)
            .map(|space| space.tabs.clone())
            .unwrap_or_default(),
        None => state
            .spaces
            .iter()
            .flat_map(|space| space.tabs.clone())
            .collect(),
    }
}

/// Resolves a tab's pinned/unpinned section and its index within that section.
fn tab_section_and_index(
    state: &WorkspaceState,
    tab_id: &str,
) -> Option<(TabOrganizationSection, usize)> {
    for space in &state.spaces {
        if let Some(tab) = space
            .tabs
            .iter()
            .find(|candidate| candidate.tab_id == tab_id)
        {
            let section = if tab.is_pinned {
                TabOrganizationSection::Pinned
            } else {
                TabOrganizationSection::Unpinned
            };
            let index = space
                .tabs
                .iter()
                .filter(|candidate| candidate.is_pinned == tab.is_pinned)
                .position(|candidate| candidate.tab_id == tab_id)?;
            return Some((section, index));
        }
    }
    None
}

fn pane_slots_in_tab(state: &WorkspaceState, tab_id: Option<&str>) -> Vec<PaneSlot> {
    match tab_id {
        Some(tab_id) => state
            .pane_slots
            .iter()
            .filter(|slot| slot.tab_id == tab_id)
            .cloned()
            .collect(),
        None => state.pane_slots.clone(),
    }
}

fn contents_in_tab(state: &WorkspaceState, tab_id: Option<&str>) -> Vec<ContentInstance> {
    let content_ids = pane_slots_in_tab(state, tab_id)
        .into_iter()
        .map(|slot| slot.content_id)
        .collect::<std::collections::BTreeSet<_>>();
    state
        .contents
        .iter()
        .filter(|content| content_ids.contains(&content.content_id))
        .cloned()
        .collect()
}

fn placement_for_split_direction(direction: SplitDirection) -> SplitPlacement {
    match direction {
        SplitDirection::Horizontal => SplitPlacement::Down,
        SplitDirection::Vertical => SplitPlacement::Right,
    }
}

fn terminal_target(
    state: &WorkspaceState,
    command: &ShellControlCommand,
) -> Result<TerminalTarget, (&'static str, &'static str)> {
    let requested_pane_slot_id = pane_slot_id_from_command(command);
    let (pane_slot, content) = if let Some(content_id) = &command.content_id {
        let Some(content) = state
            .contents
            .iter()
            .find(|content| content.content_id == *content_id)
            .cloned()
        else {
            return Err((
                "content_not_found",
                "terminal command requires an existing terminal content target.",
            ));
        };
        let Some(pane_slot) = state
            .pane_slots
            .iter()
            .find(|slot| slot.content_id == *content_id)
            .cloned()
        else {
            return Err((
                "content_not_found",
                "terminal command requires an existing terminal content target.",
            ));
        };
        (pane_slot, content)
    } else if let Some(pane_slot_id) = requested_pane_slot_id {
        let Some(pane_slot) = state
            .pane_slots
            .iter()
            .find(|slot| slot.pane_slot_id == pane_slot_id)
            .cloned()
        else {
            return Err((
                "pane_not_found",
                "terminal command requires an existing terminal content target.",
            ));
        };
        let Some(content) = state
            .contents
            .iter()
            .find(|content| content.content_id == pane_slot.content_id)
            .cloned()
        else {
            return Err((
                "content_not_found",
                "terminal command requires an existing terminal content target.",
            ));
        };
        (pane_slot, content)
    } else {
        return Err((
            "terminal_target_required",
            "terminal command requires an existing terminal content target.",
        ));
    };

    if content.kind != ContentKind::Terminal {
        return Err((
            "unsupported_content",
            "terminal command requires terminal content.",
        ));
    }

    Ok(TerminalTarget { pane_slot, content })
}

fn reducer_error_projection(error: &ReducerError) -> (&'static str, &'static str) {
    match error.code {
        ReducerErrorCode::SpaceNotFound => {
            ("space_not_found", "The requested space does not exist.")
        }
        ReducerErrorCode::TabNotFound => ("tab_not_found", "The requested tab does not exist."),
        ReducerErrorCode::PaneNotFound => ("pane_not_found", "The requested pane does not exist."),
        ReducerErrorCode::UnsupportedContent => (
            "unsupported_content",
            "This action requires terminal content.",
        ),
        ReducerErrorCode::SplitNotFound => {
            ("split_not_found", "The requested split does not exist.")
        }
        ReducerErrorCode::SpatialFocusTargetNotFound => (
            "spatial_focus_target_not_found",
            "There is no pane in that direction.",
        ),
        ReducerErrorCode::LastPane => (
            "last_pane",
            "This action requires the pane to have at least one sibling.",
        ),
        ReducerErrorCode::InvalidMoveTarget => (
            "invalid_move_target",
            "The pane cannot be moved onto its current tab.",
        ),
        ReducerErrorCode::InvalidTabOrganizationTarget => (
            "invalid_tab_organization_target",
            "The requested tab organization target is not available.",
        ),
    }
}
