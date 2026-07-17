use super::{DomainEvent, ReducerError, ReducerErrorCode, ReducerFocus, WorkspaceReducer};
use crate::{ShellAttentionState, SpatialFocusDirection, Tab, WorkspaceState};
use std::collections::BTreeMap;

impl WorkspaceReducer {
    pub(super) fn focus_pane(&mut self, pane_slot_id: &str) -> Result<(), ReducerError> {
        self.require_pane_slot(pane_slot_id)?;
        self.repair_focus(Some(pane_slot_id.to_string()));
        self.domain_events.push(DomainEvent::FocusChanged {
            pane_slot_id: self.state.focused_pane_id.clone(),
        });
        Ok(())
    }

    pub(super) fn focus_adjacent(
        &mut self,
        direction: SpatialFocusDirection,
    ) -> Result<(), ReducerError> {
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

    pub(super) fn select_space(&mut self, space_id: &str) -> Result<(), ReducerError> {
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

    pub(super) fn select_tab(&mut self, tab_id: &str) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        let tab = self.state.spaces[space_index].tabs[tab_index].clone();
        let pane_id = self.target_pane_id_for_tab(&tab)?;
        self.focus_pane(&pane_id)
    }

    pub(super) fn set_attention(
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

    pub(super) fn repair_focus(&mut self, preferred_pane_slot_id: Option<String>) {
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

    pub(super) fn reconcile_zoom_state(&mut self) {
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
    pub(super) fn focus(&self) -> ReducerFocus {
        ReducerFocus {
            space_id: self.focused_space_id.clone(),
            tab_id: self.focused_tab_id.clone(),
            pane_slot_id: self.focused_pane_id.clone(),
        }
    }
}

fn attention_rank(attention: ShellAttentionState) -> u8 {
    match attention {
        ShellAttentionState::Idle => 0,
        ShellAttentionState::Active => 1,
        ShellAttentionState::Notable => 2,
        ShellAttentionState::AwaitingUser => 3,
    }
}
