use super::{
    DomainEvent, ReducerError, ReducerErrorCode, RuntimeIntent, WorkspaceReducer, next_id,
    terminal_content, terminal_content_id,
};
use crate::{
    ContentKind, PaneSlot, PaneTreeNode, ShellAttentionState, Tab, TabOrganizationSection,
};
use std::collections::BTreeSet;

impl WorkspaceReducer {
    pub(super) fn duplicate_tab(
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

    pub(super) fn move_tab(
        &mut self,
        tab_id: &str,
        section_offset: isize,
    ) -> Result<(), ReducerError> {
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

    pub(super) fn move_tab_to_space(
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

    pub(super) fn organize_tab(
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

    pub(super) fn clear_inactive_temporary_tabs(
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

    pub(super) fn close_tab(&mut self, tab_id: &str) -> Result<(), ReducerError> {
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

    pub(super) fn set_tab_pinned(
        &mut self,
        tab_id: &str,
        is_pinned: bool,
    ) -> Result<(), ReducerError> {
        let (space_index, tab_index) = self.tab_location(tab_id)?;
        if self.state.spaces[space_index].tabs[tab_index].is_pinned == is_pinned {
            return Ok(());
        }
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

    pub(super) fn set_tab_title(
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
}

fn insertion_index_for_section(tabs: &[Tab], is_pinned: bool, section_index: usize) -> usize {
    if is_pinned {
        return section_index.min(tabs.iter().filter(|tab| tab.is_pinned).count());
    }

    let pinned_count = tabs.iter().filter(|tab| tab.is_pinned).count();
    let unpinned_count = tabs.len().saturating_sub(pinned_count);
    pinned_count + section_index.min(unpinned_count)
}
