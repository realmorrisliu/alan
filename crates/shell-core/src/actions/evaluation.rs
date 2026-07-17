use super::{
    ShellActionAvailability, ShellActionEffect, ShellActionTarget, ShellActionTargetKind,
    ShellResolvedActionTarget,
};
use crate::{Space, SplitPlacement, Tab, WorkspaceState};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum AvailabilityKind {
    #[default]
    Always,
    FocusedPane,
    TerminalContent,
    SplitPane,
    PaneMovement(SplitPlacement),
    SelectedTab,
    DuplicateTab,
    OpenTabInSplitView,
    MultipleTabs,
    MultipleSpaces,
    IndexedSpace,
    PinTab,
    UnpinTab,
    UpdatePinnedTab,
    MoveTab(i32),
    MoveTabToSpace,
}

impl AvailabilityKind {
    pub(super) fn evaluate(
        self,
        state: &WorkspaceState,
        target: &ShellActionTarget,
    ) -> ShellActionAvailability {
        match self {
            Self::Always => ShellActionAvailability::Available,
            Self::FocusedPane => focused_pane_availability(state, target),
            Self::TerminalContent => terminal_content_availability(state, target),
            Self::SplitPane => split_pane_availability(state, target),
            Self::PaneMovement(placement) => pane_movement_availability(placement, state, target),
            Self::SelectedTab => selected_tab_availability(state, target),
            Self::DuplicateTab => duplicate_tab_availability(state, target),
            Self::OpenTabInSplitView => open_tab_in_split_view_availability(state, target),
            Self::MultipleTabs => multiple_tabs_availability(state),
            Self::MultipleSpaces => multiple_spaces_availability(state),
            Self::IndexedSpace => indexed_space_availability(state, target),
            Self::PinTab => pin_tab_availability(state, target),
            Self::UnpinTab => unpin_tab_availability(state, target),
            Self::UpdatePinnedTab => update_pinned_tab_availability(state, target),
            Self::MoveTab(offset) => move_tab_availability(offset, state, target),
            Self::MoveTabToSpace => move_tab_to_space_availability(state, target),
        }
    }
}

pub(super) fn resolve_target(
    target_kind: ShellActionTargetKind,
    target: &ShellActionTarget,
    state: &WorkspaceState,
) -> ShellResolvedActionTarget {
    match target_kind {
        ShellActionTargetKind::CurrentSelection => ShellResolvedActionTarget::Selection {
            space_id: state.focused_space_id.clone(),
            tab_id: state.focused_tab_id.clone(),
            pane_id: state.focused_pane_id.clone(),
        },
        ShellActionTargetKind::Tab => match target {
            ShellActionTarget::ContextTab { tab_id } => ShellResolvedActionTarget::Tab {
                tab_id: tab_id.clone(),
            },
            _ => state
                .focused_tab_id
                .clone()
                .map(|tab_id| ShellResolvedActionTarget::Tab { tab_id })
                .unwrap_or(ShellResolvedActionTarget::Unresolved),
        },
        ShellActionTargetKind::Pane => match target {
            ShellActionTarget::ContextPane { pane_id } => ShellResolvedActionTarget::Pane {
                pane_id: pane_id.clone(),
            },
            _ => state
                .focused_pane_id
                .clone()
                .map(|pane_id| ShellResolvedActionTarget::Pane { pane_id })
                .unwrap_or(ShellResolvedActionTarget::Unresolved),
        },
        ShellActionTargetKind::Space => match target {
            ShellActionTarget::ContextSpace { space_id } => ShellResolvedActionTarget::Space {
                space_id: space_id.clone(),
            },
            ShellActionTarget::SpaceIndex { index } => {
                ShellResolvedActionTarget::SpaceIndex { index: *index }
            }
            _ => state
                .focused_space_id
                .clone()
                .map(|space_id| ShellResolvedActionTarget::Space { space_id })
                .unwrap_or(ShellResolvedActionTarget::Unresolved),
        },
        ShellActionTargetKind::DestinationSpace => match target {
            ShellActionTarget::TabToSpace { tab_id, space_id } => {
                ShellResolvedActionTarget::TabToSpace {
                    tab_id: tab_id.clone(),
                    space_id: space_id.clone(),
                }
            }
            _ => ShellResolvedActionTarget::Unresolved,
        },
    }
}

pub(super) fn effect(
    base_effect: ShellActionEffect,
    resolved_target: &ShellResolvedActionTarget,
) -> ShellActionEffect {
    match base_effect {
        ShellActionEffect::PinTab { .. } => ShellActionEffect::PinTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::UnpinTab { .. } => ShellActionEffect::UnpinTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::UpdatePinnedTab { .. } => ShellActionEffect::UpdatePinnedTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::MoveTab { offset, .. } => ShellActionEffect::MoveTab {
            tab_id: resolved_tab_id(resolved_target),
            offset,
        },
        ShellActionEffect::MoveTabToSpace { .. } => {
            if let ShellResolvedActionTarget::TabToSpace { tab_id, space_id } = resolved_target {
                ShellActionEffect::MoveTabToSpace {
                    tab_id: Some(tab_id.clone()),
                    space_id: Some(space_id.clone()),
                }
            } else {
                ShellActionEffect::MoveTabToSpace {
                    tab_id: None,
                    space_id: None,
                }
            }
        }
        ShellActionEffect::MovePaneInTab { placement, .. } => ShellActionEffect::MovePaneInTab {
            pane_id: resolved_pane_id(resolved_target),
            placement,
        },
        ShellActionEffect::TerminalClear { .. } => ShellActionEffect::TerminalClear {
            pane_id: resolved_pane_id(resolved_target),
        },
        ShellActionEffect::CloseTab { .. } => ShellActionEffect::CloseTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::RenameTab { .. } => ShellActionEffect::RenameTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::DuplicateTab { .. } => ShellActionEffect::DuplicateTab {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::OpenTabInSplitView { .. } => ShellActionEffect::OpenTabInSplitView {
            tab_id: resolved_tab_id(resolved_target),
        },
        ShellActionEffect::ClosePane { .. } => ShellActionEffect::ClosePane {
            pane_id: resolved_pane_id(resolved_target),
        },
        ShellActionEffect::OpenTab {
            launch_target,
            space_id,
        } => {
            if let Some(resolved_space_id) = resolved_space_id(resolved_target) {
                ShellActionEffect::OpenTab {
                    launch_target,
                    space_id: Some(resolved_space_id),
                }
            } else {
                ShellActionEffect::OpenTab {
                    launch_target,
                    space_id,
                }
            }
        }
        ShellActionEffect::SelectSpaceAt { index } => {
            if let ShellResolvedActionTarget::SpaceIndex { index } = resolved_target {
                ShellActionEffect::SelectSpaceAt { index: *index }
            } else {
                ShellActionEffect::SelectSpaceAt { index }
            }
        }
        other => other,
    }
}

fn resolved_tab_id(target: &ShellResolvedActionTarget) -> Option<String> {
    match target {
        ShellResolvedActionTarget::Tab { tab_id } => Some(tab_id.clone()),
        _ => None,
    }
}

fn resolved_pane_id(target: &ShellResolvedActionTarget) -> Option<String> {
    match target {
        ShellResolvedActionTarget::Pane { pane_id } => Some(pane_id.clone()),
        _ => None,
    }
}

fn resolved_space_id(target: &ShellResolvedActionTarget) -> Option<String> {
    match target {
        ShellResolvedActionTarget::Space { space_id } => Some(space_id.clone()),
        _ => None,
    }
}

fn focused_pane_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    match target {
        ShellActionTarget::ContextPane { pane_id } => {
            if state.pane_exists(pane_id) {
                ShellActionAvailability::Available
            } else {
                ShellActionAvailability::unavailable("Pane is not available")
            }
        }
        _ => {
            if state.focused_pane_id.is_some() {
                ShellActionAvailability::Available
            } else {
                ShellActionAvailability::unavailable("No focused pane")
            }
        }
    }
}

fn terminal_content_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let pane_id = target_pane_id(state, target);
    let Some(pane_id) = pane_id else {
        return ShellActionAvailability::unavailable("No focused pane");
    };
    if !state.pane_exists(&pane_id) {
        return ShellActionAvailability::unavailable("Pane is not available");
    }
    if terminal_content_id_if_available(state, &pane_id).is_none() {
        return ShellActionAvailability::unavailable("Focused content is not a terminal");
    }
    ShellActionAvailability::Available
}

fn split_pane_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(pane_id) = target_pane_id(state, target) else {
        return ShellActionAvailability::unavailable("No focused pane");
    };
    let Some(tab) = state.tab_for_pane(&pane_id) else {
        return ShellActionAvailability::unavailable("No focused pane");
    };
    if tab.pane_tree.pane_ids().len() > 1 {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("Pane zoom requires a split tab")
    }
}

fn pane_movement_availability(
    placement: SplitPlacement,
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(pane_id) = target_pane_id(state, target) else {
        return ShellActionAvailability::unavailable("No focused pane");
    };
    let Some(tab) = state.tab_for_pane(&pane_id) else {
        return ShellActionAvailability::unavailable("No focused pane");
    };
    if tab.pane_tree.pane_ids().len() <= 1 {
        return ShellActionAvailability::unavailable("Pane movement requires a split tab");
    }
    if tab
        .pane_tree
        .adjacent_pane_id(&pane_id, placement.spatial_focus_direction())
        .is_none()
    {
        return ShellActionAvailability::unavailable("No adjacent pane in that direction");
    }
    ShellActionAvailability::Available
}

fn selected_tab_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    match target {
        ShellActionTarget::ContextTab { tab_id } => {
            if state.tab(tab_id).is_some() {
                ShellActionAvailability::Available
            } else {
                ShellActionAvailability::unavailable("Tab is not available")
            }
        }
        _ => {
            if state.focused_tab_id.is_some() {
                ShellActionAvailability::Available
            } else {
                ShellActionAvailability::unavailable("No selected tab")
            }
        }
    }
}

fn duplicate_tab_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(tab) = targeted_tab(state, target) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    if tab
        .pane_tree
        .pane_ids()
        .first()
        .and_then(|pane_id| terminal_content_id_if_available(state, pane_id))
        .is_some()
    {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("Tab is not a terminal")
    }
}

fn open_tab_in_split_view_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(tab) = targeted_tab(state, target) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    let pane_id = if tab
        .pane_tree
        .contains_pane_id(state.focused_pane_id.as_deref().unwrap_or_default())
    {
        state.focused_pane_id.clone()
    } else {
        tab.pane_tree.pane_ids().first().cloned()
    };
    if pane_id
        .as_deref()
        .and_then(|pane_id| terminal_content_id_if_available(state, pane_id))
        .is_some()
    {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("Tab cannot be split")
    }
}

fn multiple_tabs_availability(state: &WorkspaceState) -> ShellActionAvailability {
    let Some(space_id) = &state.focused_space_id else {
        return ShellActionAvailability::unavailable("No adjacent tab");
    };
    let Some(space) = state.space(space_id) else {
        return ShellActionAvailability::unavailable("No adjacent tab");
    };
    if space.tabs.len() <= 1 {
        return ShellActionAvailability::unavailable("No adjacent tab");
    }
    if state.focused_tab_id.is_none() {
        return ShellActionAvailability::unavailable("No selected tab");
    }
    ShellActionAvailability::Available
}

fn multiple_spaces_availability(state: &WorkspaceState) -> ShellActionAvailability {
    if state.spaces.len() > 1 {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("No adjacent space")
    }
}

fn indexed_space_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let ShellActionTarget::SpaceIndex { index } = target else {
        return ShellActionAvailability::unavailable("Space index is required");
    };
    if *index < state.spaces.len() {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("Space is not available")
    }
}

fn pin_tab_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(tab) = targeted_tab(state, target) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    if tab.is_pinned {
        ShellActionAvailability::unavailable("Tab is already pinned")
    } else {
        ShellActionAvailability::Available
    }
}

fn unpin_tab_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(tab) = targeted_tab(state, target) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    if tab.is_pinned {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("Tab is not pinned")
    }
}

fn update_pinned_tab_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    unpin_tab_availability(state, target)
}

fn move_tab_availability(
    offset: i32,
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let Some(tab) = targeted_tab(state, target) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    let Some(location) = state.tab_location(&tab.tab_id) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    let Some(space) = state.space(&location.space_id) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    let section_count = space
        .tabs
        .iter()
        .filter(|candidate| candidate.is_pinned == location.is_pinned)
        .count();
    let next_index = location.index as i32 + offset;
    if (0..section_count as i32).contains(&next_index) {
        ShellActionAvailability::Available
    } else {
        ShellActionAvailability::unavailable("No adjacent tab in section")
    }
}

fn move_tab_to_space_availability(
    state: &WorkspaceState,
    target: &ShellActionTarget,
) -> ShellActionAvailability {
    let ShellActionTarget::TabToSpace { tab_id, space_id } = target else {
        return ShellActionAvailability::unavailable("Move target is required");
    };
    let Some(location) = state.tab_location(tab_id) else {
        return ShellActionAvailability::unavailable("Tab is not available");
    };
    if state.space(space_id).is_none() {
        return ShellActionAvailability::unavailable("Space is not available");
    }
    if location.space_id == *space_id {
        ShellActionAvailability::unavailable("Tab is already in that space")
    } else {
        ShellActionAvailability::Available
    }
}

fn target_pane_id(state: &WorkspaceState, target: &ShellActionTarget) -> Option<String> {
    match target {
        ShellActionTarget::ContextPane { pane_id } => Some(pane_id.clone()),
        _ => state.focused_pane_id.clone(),
    }
}

fn targeted_tab<'a>(state: &'a WorkspaceState, target: &ShellActionTarget) -> Option<&'a Tab> {
    match target {
        ShellActionTarget::ContextTab { tab_id } => state.tab(tab_id),
        _ => state
            .focused_tab_id
            .as_deref()
            .and_then(|tab_id| state.tab(tab_id)),
    }
}

fn terminal_content_id_if_available(state: &WorkspaceState, pane_id: &str) -> Option<String> {
    let slot = state
        .pane_slots
        .iter()
        .find(|slot| slot.pane_slot_id == pane_id)?;
    let content = state
        .contents
        .iter()
        .find(|content| content.content_id == slot.content_id)?;
    (content.kind == crate::ContentKind::Terminal).then(|| content.content_id.clone())
}

struct TabLocation {
    space_id: String,
    is_pinned: bool,
    index: usize,
}

trait WorkspaceStateActionExt {
    fn space(&self, space_id: &str) -> Option<&Space>;
    fn pane_exists(&self, pane_id: &str) -> bool;
    fn tab_for_pane(&self, pane_id: &str) -> Option<&Tab>;
    fn tab_location(&self, tab_id: &str) -> Option<TabLocation>;
}

impl WorkspaceStateActionExt for WorkspaceState {
    fn space(&self, space_id: &str) -> Option<&Space> {
        self.spaces.iter().find(|space| space.space_id == space_id)
    }

    fn pane_exists(&self, pane_id: &str) -> bool {
        self.spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .any(|tab| tab.pane_tree.contains_pane_id(pane_id))
    }

    fn tab_for_pane(&self, pane_id: &str) -> Option<&Tab> {
        self.spaces
            .iter()
            .flat_map(|space| &space.tabs)
            .find(|tab| tab.pane_tree.contains_pane_id(pane_id))
    }

    fn tab_location(&self, tab_id: &str) -> Option<TabLocation> {
        for space in &self.spaces {
            let Some(tab) = space.tabs.iter().find(|tab| tab.tab_id == tab_id) else {
                continue;
            };
            let index = space
                .tabs
                .iter()
                .filter(|candidate| candidate.is_pinned == tab.is_pinned)
                .position(|candidate| candidate.tab_id == tab_id)?;
            return Some(TabLocation {
                space_id: space.space_id.clone(),
                is_pinned: tab.is_pinned,
                index,
            });
        }
        None
    }
}

trait SplitPlacementActionExt {
    fn spatial_focus_direction(self) -> crate::SpatialFocusDirection;
}

impl SplitPlacementActionExt for SplitPlacement {
    fn spatial_focus_direction(self) -> crate::SpatialFocusDirection {
        match self {
            SplitPlacement::Left => crate::SpatialFocusDirection::Left,
            SplitPlacement::Right => crate::SpatialFocusDirection::Right,
            SplitPlacement::Up => crate::SpatialFocusDirection::Up,
            SplitPlacement::Down => crate::SpatialFocusDirection::Down,
        }
    }
}
