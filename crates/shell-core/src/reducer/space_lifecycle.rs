use super::{
    DomainEvent, ReducerError, ReducerErrorCode, RuntimeIntent, WorkspaceReducer, next_id,
    terminal_content, terminal_content_id,
};
use crate::{PaneSlot, PaneTreeNode, ShellAttentionState, Space, Tab, TabKind};

impl WorkspaceReducer {
    pub(super) fn set_terminal_profile(
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

    pub(super) fn set_presentation_icon(
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

    pub(super) fn delete_space(
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

    pub(super) fn create_terminal_space(
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

    pub(super) fn open_terminal_tab(
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
