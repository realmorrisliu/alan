use std::collections::BTreeMap;

use alan_agent::AgentWorkspaceModel;
use alan_kernel::{ActorId, CommandDescriptor, CommandInvocation};
use crossterm::event::{
    Event as TerminalEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind,
};
use serde_json::json;

/// Host adapter that translates Crossterm events before they cross runtime boundaries.
#[derive(Clone, Debug)]
pub struct AgentWorkspaceInputAdapter {
    actor_id: ActorId,
    commands_by_name: BTreeMap<String, CommandDescriptor>,
}

impl AgentWorkspaceInputAdapter {
    /// Builds an input adapter from the current Agent Workspace command model.
    #[must_use]
    pub fn new(actor_id: ActorId, model: &AgentWorkspaceModel) -> Self {
        Self {
            actor_id,
            commands_by_name: model
                .commands
                .iter()
                .cloned()
                .map(|command| (command.name.clone(), command))
                .collect(),
        }
    }

    /// Translates one raw terminal event into host-local layout, semantic input,
    /// view-local input, or a Kernel command invocation.
    #[must_use]
    pub fn translate(&self, event: &TerminalEvent) -> WorkspaceHostIntent {
        match event {
            TerminalEvent::Key(key) => self.translate_key(key),
            TerminalEvent::Paste(text) => {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::InsertText(text.clone()))
            }
            TerminalEvent::Resize(width, height) => {
                WorkspaceHostIntent::HostLayout(HostLayoutChange {
                    width: *width,
                    height: *height,
                })
            }
            TerminalEvent::Mouse(mouse) => WorkspaceHostIntent::ViewLocalInput(
                ViewLocalInput::Mouse(mouse_event_summary(mouse)),
            ),
            _ => WorkspaceHostIntent::Noop,
        }
    }

    fn translate_key(&self, key: &KeyEvent) -> WorkspaceHostIntent {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('q')) {
            return WorkspaceHostIntent::SemanticInput(SemanticInputIntent::QuitHost);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r')) {
            return WorkspaceHostIntent::ViewLocalInput(ViewLocalInput::ToggleSemanticMode {
                mode: "thinking_expanded".to_string(),
            });
        }

        match key.code {
            KeyCode::Esc => self
                .command_invocation("agent.interrupt", json!({"source": "terminal.escape"}))
                .map(|invocation| WorkspaceHostIntent::CommandInvocation(Box::new(invocation)))
                .unwrap_or(WorkspaceHostIntent::Noop),
            KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::SubmitFocusedInput)
            }
            KeyCode::Enter => {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::InsertText("\n".into()))
            }
            KeyCode::Tab => {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::AcceptOrFocusNext)
            }
            KeyCode::BackTab => {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::FocusPrevious)
            }
            KeyCode::Up => WorkspaceHostIntent::SemanticInput(SemanticInputIntent::NavigateUp),
            KeyCode::Down => WorkspaceHostIntent::SemanticInput(SemanticInputIntent::NavigateDown),
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                WorkspaceHostIntent::SemanticInput(SemanticInputIntent::InsertText(ch.to_string()))
            }
            _ => WorkspaceHostIntent::Noop,
        }
    }

    fn command_invocation(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Option<CommandInvocation> {
        let descriptor = self.commands_by_name.get(name)?;
        Some(CommandInvocation::from_descriptor(
            descriptor,
            self.actor_id,
            arguments,
        ))
    }
}

/// Result of translating a host event.
#[derive(Clone, Debug, PartialEq)]
pub enum WorkspaceHostIntent {
    /// Invoke a Kernel command.
    CommandInvocation(Box<CommandInvocation>),
    /// Semantic input that can be routed to the active view or app composer.
    SemanticInput(SemanticInputIntent),
    /// Host layout changed; this remains renderer-local.
    HostLayout(HostLayoutChange),
    /// View-local state changed without becoming Kernel semantic state directly.
    ViewLocalInput(ViewLocalInput),
    /// Event has no semantic effect.
    Noop,
}

/// Renderer-independent user input intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SemanticInputIntent {
    /// Insert text into the focused semantic input surface.
    InsertText(String),
    /// Submit the currently focused input surface.
    SubmitFocusedInput,
    /// Accept completion or move focus forward.
    AcceptOrFocusNext,
    /// Move focus backward.
    FocusPrevious,
    /// Navigate up in the active semantic surface.
    NavigateUp,
    /// Navigate down in the active semantic surface.
    NavigateDown,
    /// Quit the host frame.
    QuitHost,
}

/// Host-owned layout change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostLayoutChange {
    /// Terminal width in cells.
    pub width: u16,
    /// Terminal height in cells.
    pub height: u16,
}

/// Input that affects host-local view presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewLocalInput {
    /// Toggle a host-local semantic render mode.
    ToggleSemanticMode { mode: String },
    /// Mouse event summarized without storing Crossterm types in Kernel state.
    Mouse(MouseEventSummary),
}

/// Bounded mouse event summary owned by the terminal host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MouseEventSummary {
    /// Event kind label.
    pub kind: String,
    /// Column in terminal cells.
    pub column: u16,
    /// Row in terminal cells.
    pub row: u16,
}

fn mouse_event_summary(mouse: &MouseEvent) -> MouseEventSummary {
    MouseEventSummary {
        kind: mouse_kind_label(&mouse.kind).to_string(),
        column: mouse.column,
        row: mouse.row,
    }
}

fn mouse_kind_label(kind: &MouseEventKind) -> &'static str {
    match kind {
        MouseEventKind::Down(_) => "down",
        MouseEventKind::Up(_) => "up",
        MouseEventKind::Drag(_) => "drag",
        MouseEventKind::Moved => "moved",
        MouseEventKind::ScrollDown => "scroll_down",
        MouseEventKind::ScrollUp => "scroll_up",
        MouseEventKind::ScrollLeft => "scroll_left",
        MouseEventKind::ScrollRight => "scroll_right",
    }
}

#[cfg(test)]
mod tests {
    use alan_agent::{AgentWorkspaceProjector, AgentWorkspaceSessionMetadata};
    use crossterm::event::{KeyEvent, MouseButton};

    use super::*;

    fn adapter() -> AgentWorkspaceInputAdapter {
        let projector =
            AgentWorkspaceProjector::new(AgentWorkspaceSessionMetadata::new("session-1"));
        AgentWorkspaceInputAdapter::new(projector.ids().user_actor, &projector.model())
    }

    #[test]
    fn escape_translates_to_interrupt_command_invocation() {
        let intent = adapter().translate(&TerminalEvent::Key(KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));

        let WorkspaceHostIntent::CommandInvocation(invocation) = intent else {
            panic!("expected command invocation");
        };
        assert_eq!(invocation.arguments["source"], "terminal.escape");
        assert!(matches!(
            invocation.target,
            alan_kernel::CommandTarget::Object { .. }
        ));
    }

    #[test]
    fn text_paste_resize_and_mouse_stay_in_host_or_semantic_input_space() {
        let adapter = adapter();

        assert_eq!(
            adapter.translate(&TerminalEvent::Paste("hello".to_string())),
            WorkspaceHostIntent::SemanticInput(SemanticInputIntent::InsertText(
                "hello".to_string()
            ))
        );
        assert_eq!(
            adapter.translate(&TerminalEvent::Resize(120, 32)),
            WorkspaceHostIntent::HostLayout(HostLayoutChange {
                width: 120,
                height: 32,
            })
        );
        assert_eq!(
            adapter.translate(&TerminalEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 4,
                row: 5,
                modifiers: KeyModifiers::NONE,
            })),
            WorkspaceHostIntent::ViewLocalInput(ViewLocalInput::Mouse(MouseEventSummary {
                kind: "down".to_string(),
                column: 4,
                row: 5,
            }))
        );
    }

    #[test]
    fn host_shortcuts_do_not_become_kernel_state() {
        assert_eq!(
            adapter().translate(&TerminalEvent::Key(KeyEvent::new(
                KeyCode::Char('r'),
                KeyModifiers::CONTROL,
            ))),
            WorkspaceHostIntent::ViewLocalInput(ViewLocalInput::ToggleSemanticMode {
                mode: "thinking_expanded".to_string(),
            })
        );
        assert_eq!(
            adapter().translate(&TerminalEvent::Key(KeyEvent::new(
                KeyCode::Char('q'),
                KeyModifiers::CONTROL,
            ))),
            WorkspaceHostIntent::SemanticInput(SemanticInputIntent::QuitHost)
        );
    }
}
