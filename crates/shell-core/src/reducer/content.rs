use super::{
    DomainEvent, ReducerError, ReducerErrorCode, WorkspaceReducer, next_id, terminal_content_id,
    visible_tab_title,
};
use crate::{
    AgentContentPresentation, AgentStreamOffsets, ContentInstance, ContentKind,
    ContentLifecycleState, PaneSlot, PaneTreeNode, ShellAttentionState, ShellContentPayload,
    ShellTabActiveTaskState, SplitPlacement, Tab, TabKind, TerminalActivitySnapshot,
};
use std::collections::BTreeSet;

const SETTINGS_CONTENT_ID: &str = "content_settings_main";

impl WorkspaceReducer {
    pub(super) fn open_content_tab(
        &mut self,
        space_id: Option<String>,
        kind: ContentKind,
        title: String,
        payload: ShellContentPayload,
        reserved_pane_slot_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        if kind == ContentKind::Settings
            && let Some(pane_slot_id) = self.pane_slot_id_for_content_kind(ContentKind::Settings)
        {
            return self.focus_pane(&pane_slot_id);
        }
        if kind == ContentKind::Terminal {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "terminal content requires open_terminal_tab",
            ));
        }

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
        let content_id = content_id_for_mount(kind, &pane_slot_id);

        self.state.spaces[space_index].tabs.push(Tab {
            tab_id: tab_id.clone(),
            kind: TabKind::Terminal,
            title: Some(title.clone()),
            pane_tree: PaneTreeNode::pane(format!("node_{pane_slot_id}"), pane_slot_id.clone()),
            zoomed_pane_id: None,
            is_pinned: false,
            is_title_user_locked: false,
        });
        self.state.spaces[space_index].selected_tab_id = Some(tab_id.clone());
        self.state.pane_slots.push(PaneSlot {
            pane_slot_id: pane_slot_id.clone(),
            tab_id: tab_id.clone(),
            space_id: target_space_id,
            content_id: content_id.clone(),
            attention: ShellAttentionState::Active,
        });
        self.state
            .contents
            .push(content_instance(&content_id, kind, title, payload));
        self.changed_ids.created_tab_ids.push(tab_id.clone());
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id.clone());
        self.changed_ids.created_content_ids.push(content_id);
        self.domain_events.push(DomainEvent::TabOpened {
            tab_id,
            pane_slot_id: pane_slot_id.clone(),
        });
        self.repair_focus(Some(pane_slot_id));
        Ok(())
    }

    pub(super) fn split_content_pane(
        &mut self,
        pane_slot_id: &str,
        placement: SplitPlacement,
        kind: ContentKind,
        title: String,
        payload: ShellContentPayload,
        reserved_pane_slot_ids: Vec<String>,
    ) -> Result<(), ReducerError> {
        if kind == ContentKind::Settings
            && let Some(existing_pane_slot_id) =
                self.pane_slot_id_for_content_kind(ContentKind::Settings)
        {
            return self.focus_pane(&existing_pane_slot_id);
        }
        if kind == ContentKind::Terminal {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "terminal content requires split_pane",
            ));
        }

        let source_slot = self.require_pane_slot(pane_slot_id)?.clone();
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
        let content_id = content_id_for_mount(kind, &pane_slot_id_new);

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
        self.state
            .contents
            .push(content_instance(&content_id, kind, title, payload));

        self.changed_ids.updated_tab_ids.push(source_slot.tab_id);
        self.changed_ids
            .created_pane_slot_ids
            .push(pane_slot_id_new.clone());
        self.changed_ids.created_content_ids.push(content_id);
        self.domain_events.push(DomainEvent::PaneSplit {
            target_pane_slot_id: pane_slot_id.to_string(),
            created_pane_slot_id: pane_slot_id_new.clone(),
        });
        self.repair_focus(Some(pane_slot_id_new));
        Ok(())
    }

    pub(super) fn update_terminal_metadata(
        &mut self,
        pane_slot_id: &str,
        title: Option<String>,
        cwd: Option<String>,
        active_task_state: Option<ShellTabActiveTaskState>,
        activity: Option<TerminalActivitySnapshot>,
    ) -> Result<(), ReducerError> {
        let (content_id, tab_id) = self.apply_terminal_metadata_fields(
            pane_slot_id,
            title.clone(),
            cwd,
            active_task_state,
            activity,
        )?;
        self.domain_events
            .push(DomainEvent::TerminalMetadataUpdated {
                pane_slot_id: pane_slot_id.to_string(),
                content_id,
            });
        if let Some(title) = title {
            self.set_tab_title(&tab_id, visible_tab_title(Some(&title)), false, true)?;
        }
        Ok(())
    }

    pub(super) fn update_agent_renderer_state(
        &mut self,
        pane_slot_id: &str,
        offsets: AgentStreamOffsets,
        presentation: AgentContentPresentation,
    ) -> Result<(), ReducerError> {
        let slot = self.require_pane_slot(pane_slot_id)?.clone();
        let content = self
            .state
            .contents
            .iter_mut()
            .find(|content| content.content_id == slot.content_id)
            .ok_or_else(|| {
                ReducerError::new(
                    ReducerErrorCode::UnsupportedContent,
                    "pane does not mount an Agent ContentInstance",
                )
            })?;
        if content.kind != ContentKind::Agent {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "renderer offsets belong only to Agent content",
            ));
        }
        let attachment = content.payload.agent.as_mut().ok_or_else(|| {
            ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "Agent content has no Process Reference",
            )
        })?;
        if attachment.offsets == offsets && attachment.presentation == presentation {
            return Ok(());
        }
        attachment.offsets = offsets;
        attachment.presentation = presentation;
        self.changed_ids
            .updated_content_ids
            .push(content.content_id.clone());
        self.domain_events
            .push(DomainEvent::AgentRendererStateUpdated {
                pane_slot_id: pane_slot_id.to_string(),
                content_id: content.content_id.clone(),
            });
        Ok(())
    }

    pub(super) fn apply_agent_activity(
        &mut self,
        pane_slot_id: &str,
        activity: TerminalActivitySnapshot,
        working_directory: Option<String>,
    ) -> Result<(), ReducerError> {
        let (content_id, _tab_id) = self.apply_terminal_metadata_fields(
            pane_slot_id,
            None,
            working_directory,
            None,
            Some(activity),
        )?;
        self.domain_events.push(DomainEvent::AgentActivityUpdated {
            pane_slot_id: pane_slot_id.to_string(),
            content_id,
        });
        Ok(())
    }

    fn apply_terminal_metadata_fields(
        &mut self,
        pane_slot_id: &str,
        title: Option<String>,
        cwd: Option<String>,
        active_task_state: Option<ShellTabActiveTaskState>,
        activity: Option<TerminalActivitySnapshot>,
    ) -> Result<(String, String), ReducerError> {
        let pane_slot = self.require_pane_slot(pane_slot_id)?.clone();
        let content_index = self
            .state
            .contents
            .iter()
            .position(|content| content.content_id == pane_slot.content_id)
            .ok_or_else(|| {
                ReducerError::new(ReducerErrorCode::UnsupportedContent, "content not found")
            })?;
        if self.state.contents[content_index].kind != ContentKind::Terminal {
            return Err(ReducerError::new(
                ReducerErrorCode::UnsupportedContent,
                "terminal metadata requires terminal content",
            ));
        }

        let content = &mut self.state.contents[content_index];
        let mut metadata = content.terminal_metadata.clone().unwrap_or_default();
        if let Some(title) = title
            && let Some(visible_title) = visible_tab_title(Some(&title))
        {
            metadata.title = Some(visible_title);
        }
        if let Some(cwd) = cwd {
            metadata.cwd = Some(cwd);
        }
        if let Some(active_task_state) = active_task_state {
            metadata.active_task_state = active_task_state;
        }
        if let Some(activity) = activity {
            metadata.activity = Some(activity);
        }
        content.terminal_metadata = Some(metadata);
        self.changed_ids
            .updated_content_ids
            .push(content.content_id.clone());
        Ok((content.content_id.clone(), pane_slot.tab_id))
    }

    fn pane_slot_id_for_content_kind(&self, kind: ContentKind) -> Option<String> {
        let content_ids = self
            .state
            .contents
            .iter()
            .filter_map(|content| (content.kind == kind).then_some(&content.content_id))
            .collect::<BTreeSet<_>>();
        self.state
            .pane_slots
            .iter()
            .find(|slot| content_ids.contains(&slot.content_id))
            .map(|slot| slot.pane_slot_id.clone())
    }
}

fn content_id_for_mount(kind: ContentKind, pane_slot_id: &str) -> String {
    match kind {
        ContentKind::Terminal => terminal_content_id(pane_slot_id),
        ContentKind::Markdown => format!("content_markdown_{pane_slot_id}"),
        ContentKind::Settings => SETTINGS_CONTENT_ID.to_string(),
        ContentKind::Agent => format!("content_agent_{pane_slot_id}"),
    }
}

fn content_instance(
    content_id: &str,
    kind: ContentKind,
    title: String,
    payload: ShellContentPayload,
) -> ContentInstance {
    ContentInstance {
        content_id: content_id.to_string(),
        kind,
        title,
        icon_name: None,
        capabilities: kind.default_capabilities(),
        payload,
        terminal_metadata: None,
        lifecycle: ContentLifecycleState::Active,
        renderer_state: Default::default(),
    }
}
