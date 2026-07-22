use super::{ShellControlCommand, ShellControlResponse};
use crate::{
    ContentInstance, ContentKind, PaneSlot, ReducerError, ReducerErrorCode, RuntimeIntent,
    SplitDirection, SplitPlacement, Tab, TabOrganizationSection, WorkspaceState,
};

#[derive(Debug, Clone)]
pub(super) enum ResponseProjection {
    Current,
    Snapshot,
    Focus,
    TabSubject(String),
    ResizeSplit,
    Zoom {
        tab_id: String,
        pane_slot_id: String,
    },
    MovePaneWithinTab(SplitPlacement),
    /// Report the named pane as the response subject (not the workspace focus), for commands
    /// like `attention.set` that mutate a specific, possibly unfocused, pane.
    TargetPane(String),
    /// Report the named tab as the response subject, for commands like `tab.pin`/`tab.unpin`/
    /// `tab.reorder`/`tab.move_to_space` that mutate a specific, possibly unfocused, tab.
    TargetTab(String),
}

pub(super) struct TerminalTarget {
    pub(super) pane_slot: PaneSlot,
    pub(super) content: ContentInstance,
}

pub(super) fn project_success_response(
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
        ResponseProjection::TabSubject(tab_id) => {
            project_tab_subject(response, state, tab_id);
        }
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
            project_tab_subject(response, state, tab_id);
            // Report where the tab landed so automation can confirm an accepted organization
            // mutation (pin/unpin/reorder/move) without a follow-up state read.
            if let Some((section, index)) = tab_section_and_index(state, tab_id) {
                response.section = Some(section);
                response.index = Some(index);
            }
        }
        ResponseProjection::ResizeSplit => {
            if let Some(tab_id) = command
                .split_node_id
                .as_deref()
                .and_then(|node_id| tab_id_containing_node(state, node_id))
            {
                project_tab_subject(response, state, &tab_id);
            }
            response.split_node_id = command.split_node_id.clone();
            response.ratio = command.ratio;
            response.changed_split_ids = command.split_node_id.clone().map(|id| vec![id]);
        }
        ResponseProjection::Zoom {
            tab_id,
            pane_slot_id,
        } => {
            project_tab_subject(response, state, tab_id);
            response.pane_id = Some(pane_slot_id.clone());
            response.pane_slot_id = Some(pane_slot_id.clone());
            response.zoomed_pane_id = state.tab(tab_id).and_then(|tab| tab.zoomed_pane_id.clone());
        }
        ResponseProjection::MovePaneWithinTab(placement) => {
            response.placement = Some(*placement);
        }
    }
}

fn project_tab_subject(response: &mut ShellControlResponse, state: &WorkspaceState, tab_id: &str) {
    response.tab_id = Some(tab_id.to_string());
    if let Some(space) = state
        .spaces
        .iter()
        .find(|space| space.tabs.iter().any(|tab| tab.tab_id == tab_id))
    {
        response.space_id = Some(space.space_id.clone());
    }
}

fn tab_id_containing_node(state: &WorkspaceState, node_id: &str) -> Option<String> {
    state
        .spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .find(|tab| tab.pane_tree.contains_node_id(node_id))
        .map(|tab| tab.tab_id.clone())
}

pub(super) fn project_runtime_intent_response(
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

pub(super) fn fill_content_projection(response: &mut ShellControlResponse, state: &WorkspaceState) {
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

pub(super) fn pane_slot_id_from_command(command: &ShellControlCommand) -> Option<String> {
    command
        .pane_slot_id
        .clone()
        .or_else(|| command.pane_id.clone())
}

pub(super) fn tabs_in_space(state: &WorkspaceState, space_id: Option<&str>) -> Vec<Tab> {
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

pub(super) fn pane_slots_in_tab(state: &WorkspaceState, tab_id: Option<&str>) -> Vec<PaneSlot> {
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

pub(super) fn contents_in_tab(
    state: &WorkspaceState,
    tab_id: Option<&str>,
) -> Vec<ContentInstance> {
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

pub(super) fn placement_for_split_direction(direction: SplitDirection) -> SplitPlacement {
    match direction {
        SplitDirection::Horizontal => SplitPlacement::Down,
        SplitDirection::Vertical => SplitPlacement::Right,
    }
}

pub(super) fn terminal_target(
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

pub(super) fn reducer_error_projection(error: &ReducerError) -> (&'static str, &'static str) {
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
