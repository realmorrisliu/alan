use super::{
    DomainEvent, ReducerError, ReducerErrorCode, RuntimeIntent, WorkspaceReducer, next_id,
    terminal_content, terminal_content_id,
};
use crate::{
    ContentKind, PaneSlot, PaneTreeNode, PaneTreeNodeResizeOutcome, ShellAttentionState,
    SpatialFocusDirection, SplitDirection, SplitPlacement, Tab,
};

impl WorkspaceReducer {
    pub(super) fn split_pane(
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

    pub(super) fn resize_split(
        &mut self,
        split_node_id: &str,
        ratio: f64,
    ) -> Result<(), ReducerError> {
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

    pub(super) fn equalize_splits(&mut self, tab_id: Option<String>) -> Result<(), ReducerError> {
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

    pub(super) fn zoom_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
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

    pub(super) fn unzoom_tab(&mut self, tab_id: Option<String>) -> Result<(), ReducerError> {
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

    pub(super) fn close_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
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

    pub(super) fn move_pane_to_new_tab(
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

    pub(super) fn move_pane_to_tab(
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

    pub(super) fn move_pane_within_tab(
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
}

fn spatial_direction_for_placement(placement: SplitPlacement) -> SpatialFocusDirection {
    match placement {
        SplitPlacement::Left => SpatialFocusDirection::Left,
        SplitPlacement::Right => SpatialFocusDirection::Right,
        SplitPlacement::Up => SpatialFocusDirection::Up,
        SplitPlacement::Down => SpatialFocusDirection::Down,
    }
}
