use crate::{Space, SplitPlacement, Tab, WorkspaceState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Stable shell action id shared across shell clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ShellActionId {
    /// Open a new terminal tab.
    #[serde(rename = "shell.tab.new_terminal")]
    NewTerminalTab,
    /// Close a tab.
    #[serde(rename = "shell.tab.close")]
    TabClose,
    /// Rename a tab.
    #[serde(rename = "shell.tab.rename")]
    TabRename,
    /// Duplicate a tab.
    #[serde(rename = "shell.tab.duplicate")]
    TabDuplicate,
    /// Open a tab in split view.
    #[serde(rename = "shell.tab.open_in_split_view")]
    TabOpenInSplitView,
    /// Select previous tab.
    #[serde(rename = "shell.tab.select_previous")]
    TabSelectPrevious,
    /// Select next tab.
    #[serde(rename = "shell.tab.select_next")]
    TabSelectNext,
    /// Pin tab.
    #[serde(rename = "shell.tab.pin")]
    TabPin,
    /// Unpin tab.
    #[serde(rename = "shell.tab.unpin")]
    TabUnpin,
    /// Update pinned tab snapshot.
    #[serde(rename = "shell.tab.update_pin")]
    TabUpdatePin,
    /// Move tab left.
    #[serde(rename = "shell.tab.move_left")]
    TabMoveLeft,
    /// Move tab right.
    #[serde(rename = "shell.tab.move_right")]
    TabMoveRight,
    /// Move tab to another Space.
    #[serde(rename = "shell.tab.move_to_space")]
    TabMoveToSpace,
    /// Split pane left.
    #[serde(rename = "shell.pane.split_left")]
    PaneSplitLeft,
    /// Split pane right.
    #[serde(rename = "shell.pane.split_right")]
    PaneSplitRight,
    /// Split pane up.
    #[serde(rename = "shell.pane.split_up")]
    PaneSplitUp,
    /// Split pane down.
    #[serde(rename = "shell.pane.split_down")]
    PaneSplitDown,
    /// Focus pane left.
    #[serde(rename = "shell.pane.focus_left")]
    PaneFocusLeft,
    /// Focus pane right.
    #[serde(rename = "shell.pane.focus_right")]
    PaneFocusRight,
    /// Focus pane up.
    #[serde(rename = "shell.pane.focus_up")]
    PaneFocusUp,
    /// Focus pane down.
    #[serde(rename = "shell.pane.focus_down")]
    PaneFocusDown,
    /// Equalize splits.
    #[serde(rename = "shell.pane.equalize_splits")]
    PaneEqualizeSplits,
    /// Toggle pane zoom.
    #[serde(rename = "shell.pane.zoom_toggle")]
    PaneZoomToggle,
    /// Move pane left.
    #[serde(rename = "shell.pane.move_left")]
    PaneMoveLeft,
    /// Move pane right.
    #[serde(rename = "shell.pane.move_right")]
    PaneMoveRight,
    /// Move pane up.
    #[serde(rename = "shell.pane.move_up")]
    PaneMoveUp,
    /// Move pane down.
    #[serde(rename = "shell.pane.move_down")]
    PaneMoveDown,
    /// Close pane.
    #[serde(rename = "shell.pane.close")]
    PaneClose,
    /// Clear terminal.
    #[serde(rename = "shell.terminal.clear")]
    TerminalClear,
    /// Open find.
    #[serde(rename = "shell.find.open")]
    FindOpen,
    /// Select previous Space.
    #[serde(rename = "shell.space.select_previous")]
    SpaceSelectPrevious,
    /// Select next Space.
    #[serde(rename = "shell.space.select_next")]
    SpaceSelectNext,
    /// Select Space by index.
    #[serde(rename = "shell.space.select_by_index")]
    SpaceSelectByIndex,
}

impl ShellActionId {
    /// Stable raw action id.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NewTerminalTab => "shell.tab.new_terminal",
            Self::TabClose => "shell.tab.close",
            Self::TabRename => "shell.tab.rename",
            Self::TabDuplicate => "shell.tab.duplicate",
            Self::TabOpenInSplitView => "shell.tab.open_in_split_view",
            Self::TabSelectPrevious => "shell.tab.select_previous",
            Self::TabSelectNext => "shell.tab.select_next",
            Self::TabPin => "shell.tab.pin",
            Self::TabUnpin => "shell.tab.unpin",
            Self::TabUpdatePin => "shell.tab.update_pin",
            Self::TabMoveLeft => "shell.tab.move_left",
            Self::TabMoveRight => "shell.tab.move_right",
            Self::TabMoveToSpace => "shell.tab.move_to_space",
            Self::PaneSplitLeft => "shell.pane.split_left",
            Self::PaneSplitRight => "shell.pane.split_right",
            Self::PaneSplitUp => "shell.pane.split_up",
            Self::PaneSplitDown => "shell.pane.split_down",
            Self::PaneFocusLeft => "shell.pane.focus_left",
            Self::PaneFocusRight => "shell.pane.focus_right",
            Self::PaneFocusUp => "shell.pane.focus_up",
            Self::PaneFocusDown => "shell.pane.focus_down",
            Self::PaneEqualizeSplits => "shell.pane.equalize_splits",
            Self::PaneZoomToggle => "shell.pane.zoom_toggle",
            Self::PaneMoveLeft => "shell.pane.move_left",
            Self::PaneMoveRight => "shell.pane.move_right",
            Self::PaneMoveUp => "shell.pane.move_up",
            Self::PaneMoveDown => "shell.pane.move_down",
            Self::PaneClose => "shell.pane.close",
            Self::TerminalClear => "shell.terminal.clear",
            Self::FindOpen => "shell.find.open",
            Self::SpaceSelectPrevious => "shell.space.select_previous",
            Self::SpaceSelectNext => "shell.space.select_next",
            Self::SpaceSelectByIndex => "shell.space.select_by_index",
        }
    }
}

/// Action target kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellActionTargetKind {
    /// Current selection.
    CurrentSelection,
    /// Tab target.
    Tab,
    /// Pane target.
    Pane,
    /// Space target.
    Space,
    /// Destination Space target.
    DestinationSpace,
}

/// Action target supplied by UI or keyboard dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellActionTarget {
    /// Current selection.
    CurrentSelection,
    /// Context tab id.
    ContextTab { tab_id: String },
    /// Context pane id.
    ContextPane { pane_id: String },
    /// Context Space id.
    ContextSpace { space_id: String },
    /// Zero-based Space index.
    SpaceIndex { index: usize },
    /// Move a tab to a Space.
    TabToSpace { tab_id: String, space_id: String },
}

/// Resolved action target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellResolvedActionTarget {
    /// Resolved current selection.
    Selection {
        /// Space id.
        space_id: Option<String>,
        /// Tab id.
        tab_id: Option<String>,
        /// Pane id.
        pane_id: Option<String>,
    },
    /// Resolved tab id.
    Tab { tab_id: String },
    /// Resolved pane id.
    Pane { pane_id: String },
    /// Resolved Space id.
    Space { space_id: String },
    /// Resolved Space index.
    SpaceIndex { index: usize },
    /// Resolved tab-to-Space move target.
    TabToSpace { tab_id: String, space_id: String },
    /// Unresolved target.
    Unresolved,
}

/// Keyboard shortcut modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellActionModifier {
    /// Command modifier.
    Command,
    /// Control modifier.
    Control,
    /// Option modifier.
    Option,
    /// Shift modifier.
    Shift,
}

/// Keyboard shortcut context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellActionShortcutContext {
    /// Shell-wide shortcut context.
    Shell,
    /// Terminal find context.
    TerminalFind,
}

/// Keyboard shortcut metadata.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ShellActionShortcut {
    /// Key identifier.
    pub key: String,
    /// Modifier set in stable order.
    pub modifiers: Vec<ShellActionModifier>,
    /// Shortcut context.
    pub context: ShellActionShortcutContext,
}

impl ShellActionShortcut {
    /// Creates a normalized shortcut.
    pub fn new(
        key: impl Into<String>,
        modifiers: Vec<ShellActionModifier>,
        context: ShellActionShortcutContext,
    ) -> Self {
        let mut modifiers = modifiers;
        modifiers.sort();
        modifiers.dedup();
        Self {
            key: key.into(),
            modifiers,
            context,
        }
    }

    /// Creates a dynamic Space-selection shortcut.
    pub fn space_selection(index: usize) -> Option<Self> {
        (index < 9).then(|| {
            Self::new(
                (index + 1).to_string(),
                vec![ShellActionModifier::Command, ShellActionModifier::Option],
                ShellActionShortcutContext::Shell,
            )
        })
    }

    fn normalized(&self) -> Self {
        Self::new(self.key.clone(), self.modifiers.clone(), self.context)
    }
}

/// Action availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShellActionAvailability {
    /// Action is available.
    Available,
    /// Action is unavailable.
    Unavailable {
        /// Stable unavailable reason.
        reason: String,
    },
}

impl ShellActionAvailability {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    fn is_available(&self) -> bool {
        matches!(self, ShellActionAvailability::Available)
    }
}

/// Portable workspace command ids used by reusable action effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellWorkspaceCommand {
    /// New terminal tab command.
    NewTerminalTab,
    /// Split left.
    SplitLeft,
    /// Split right.
    SplitRight,
    /// Split up.
    SplitUp,
    /// Split down.
    SplitDown,
    /// Focus left.
    FocusLeft,
    /// Focus right.
    FocusRight,
    /// Focus up.
    FocusUp,
    /// Focus down.
    FocusDown,
    /// Equalize splits.
    EqualizeSplits,
    /// Toggle pane zoom.
    TogglePaneZoom,
}

/// Portable action effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellActionEffect {
    /// Dispatch a workspace command.
    WorkspaceCommand { command: ShellWorkspaceCommand },
    /// Open a tab.
    OpenTab {
        /// Launch target.
        launch_target: crate::ShellLaunchTarget,
        /// Optional target Space id.
        space_id: Option<String>,
    },
    /// Close tab.
    CloseTab { tab_id: Option<String> },
    /// Rename tab.
    RenameTab { tab_id: Option<String> },
    /// Duplicate tab.
    DuplicateTab { tab_id: Option<String> },
    /// Open tab in split view.
    OpenTabInSplitView { tab_id: Option<String> },
    /// Close pane.
    ClosePane { pane_id: Option<String> },
    /// Select adjacent tab.
    SelectAdjacentTab { offset: i32 },
    /// Select adjacent Space.
    SelectAdjacentSpace { offset: i32 },
    /// Select Space by index.
    SelectSpaceAt { index: usize },
    /// Pin tab.
    PinTab { tab_id: Option<String> },
    /// Unpin tab.
    UnpinTab { tab_id: Option<String> },
    /// Update pinned tab.
    UpdatePinnedTab { tab_id: Option<String> },
    /// Move tab.
    MoveTab {
        /// Tab id.
        tab_id: Option<String>,
        /// Offset.
        offset: i32,
    },
    /// Move tab to Space.
    MoveTabToSpace {
        /// Tab id.
        tab_id: Option<String>,
        /// Space id.
        space_id: Option<String>,
    },
    /// Move pane inside its tab.
    MovePaneInTab {
        /// Pane id.
        pane_id: Option<String>,
        /// Placement.
        placement: SplitPlacement,
    },
    /// Clear terminal.
    TerminalClear { pane_id: Option<String> },
    /// Disabled placeholder.
    DisabledPlaceholder,
}

/// Action execution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShellActionExecutionResult {
    /// Executed.
    Executed {
        /// Effect passed to the handler.
        effect: ShellActionEffect,
    },
    /// Failed.
    Failed {
        /// Failure reason.
        reason: String,
    },
    /// Unavailable.
    Unavailable {
        /// Unavailable reason.
        reason: String,
    },
}

/// Resolved action metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellResolvedAction {
    /// Action descriptor.
    pub descriptor: Option<ShellActionDescriptor>,
    /// Resolved target.
    pub resolved_target: ShellResolvedActionTarget,
    /// Availability.
    pub availability: ShellActionAvailability,
}

/// Keyboard action lookup result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellKeyboardAction {
    /// Action id.
    pub id: ShellActionId,
    /// Action target.
    pub target: ShellActionTarget,
}

/// Action descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellActionDescriptor {
    /// Action id.
    pub id: ShellActionId,
    /// User-facing title.
    pub title: String,
    /// Target kind.
    pub target_kind: ShellActionTargetKind,
    /// Default shortcut.
    pub default_shortcut: Option<ShellActionShortcut>,
    /// Base effect.
    pub effect: ShellActionEffect,
    #[serde(skip)]
    availability: AvailabilityKind,
}

impl ShellActionDescriptor {
    /// Creates a descriptor with always-available behavior.
    pub fn always_available(
        id: ShellActionId,
        title: impl Into<String>,
        target_kind: ShellActionTargetKind,
        default_shortcut: Option<ShellActionShortcut>,
        effect: ShellActionEffect,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            target_kind,
            default_shortcut,
            effect,
            availability: AvailabilityKind::Always,
        }
    }

    fn new(
        id: ShellActionId,
        title: &str,
        target_kind: ShellActionTargetKind,
        default_shortcut: Option<ShellActionShortcut>,
        effect: ShellActionEffect,
        availability: AvailabilityKind,
    ) -> Self {
        Self {
            id,
            title: title.to_string(),
            target_kind,
            default_shortcut,
            effect,
            availability,
        }
    }

    /// Evaluates action availability.
    pub fn availability(
        &self,
        state: &WorkspaceState,
        target: &ShellActionTarget,
    ) -> ShellActionAvailability {
        self.availability.evaluate(state, target)
    }
}

/// Action registry construction error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShellActionRegistryError {
    /// Duplicate action id.
    #[error("duplicate action id {}", .0.as_str())]
    DuplicateActionId(ShellActionId),
    /// Duplicate shortcut.
    #[error("duplicate shortcut {shortcut:?} for action ids {action_ids:?}")]
    DuplicateShortcut {
        /// Shortcut.
        shortcut: ShellActionShortcut,
        /// Conflicting action ids.
        action_ids: Vec<ShellActionId>,
    },
}

/// Shared shell action registry.
#[derive(Debug, Clone)]
pub struct ShellActionRegistry {
    actions: Vec<ShellActionDescriptor>,
}

impl ShellActionRegistry {
    /// Creates a registry from action descriptors.
    pub fn new(actions: Vec<ShellActionDescriptor>) -> Result<Self, ShellActionRegistryError> {
        let mut action_ids = BTreeMap::new();
        for action in &actions {
            if action_ids.insert(action.id, ()).is_some() {
                return Err(ShellActionRegistryError::DuplicateActionId(action.id));
            }
        }

        let has_dynamic_space_selection = actions
            .iter()
            .any(|action| action.id == ShellActionId::SpaceSelectByIndex);
        let mut shortcut_entries = actions
            .iter()
            .filter_map(|action| {
                action
                    .default_shortcut
                    .as_ref()
                    .map(|shortcut| (shortcut.normalized(), action.id))
            })
            .collect::<Vec<_>>();
        if has_dynamic_space_selection {
            shortcut_entries.extend(dynamic_shortcut_entries());
        }
        let mut shortcuts: BTreeMap<ShellActionShortcut, Vec<ShellActionId>> = BTreeMap::new();
        for (shortcut, id) in shortcut_entries {
            shortcuts.entry(shortcut).or_default().push(id);
        }
        for (shortcut, action_ids) in shortcuts {
            if action_ids.len() > 1 {
                return Err(ShellActionRegistryError::DuplicateShortcut {
                    shortcut,
                    action_ids,
                });
            }
        }

        Ok(Self { actions })
    }

    /// Returns the standard shared shell action registry.
    pub fn standard() -> Self {
        Self::new(standard_actions()).expect("standard shell action registry is valid")
    }

    /// Returns all actions.
    pub fn actions(&self) -> &[ShellActionDescriptor] {
        &self.actions
    }

    /// Looks up an action by id.
    pub fn action(&self, id: ShellActionId) -> Option<&ShellActionDescriptor> {
        self.actions.iter().find(|action| action.id == id)
    }

    /// Returns the default shortcut for an action id and target.
    pub fn default_shortcut(
        &self,
        id: ShellActionId,
        target: &ShellActionTarget,
    ) -> Option<ShellActionShortcut> {
        if id == ShellActionId::SpaceSelectByIndex
            && let ShellActionTarget::SpaceIndex { index } = target
        {
            return ShellActionShortcut::space_selection(*index);
        }
        self.action(id)
            .and_then(|action| action.default_shortcut.clone())
    }

    /// Resolves a keyboard action.
    pub fn keyboard_action(&self, shortcut: &ShellActionShortcut) -> Option<ShellKeyboardAction> {
        let shortcut = shortcut.normalized();
        if let Some(action) = self
            .actions
            .iter()
            .find(|action| action.default_shortcut.as_ref() == Some(&shortcut))
        {
            return Some(ShellKeyboardAction {
                id: action.id,
                target: ShellActionTarget::CurrentSelection,
            });
        }

        if shortcut.context == ShellActionShortcutContext::Shell
            && shortcut.modifiers == vec![ShellActionModifier::Command, ShellActionModifier::Option]
            && self.action(ShellActionId::SpaceSelectByIndex).is_some()
            && let Ok(value) = shortcut.key.parse::<usize>()
            && (1..=9).contains(&value)
        {
            return Some(ShellKeyboardAction {
                id: ShellActionId::SpaceSelectByIndex,
                target: ShellActionTarget::SpaceIndex { index: value - 1 },
            });
        }
        None
    }

    /// Resolves action target and availability.
    pub fn resolve(
        &self,
        id: ShellActionId,
        target: &ShellActionTarget,
        state: &WorkspaceState,
    ) -> ShellResolvedAction {
        let Some(descriptor) = self.action(id) else {
            return ShellResolvedAction {
                descriptor: None,
                resolved_target: ShellResolvedActionTarget::Unresolved,
                availability: ShellActionAvailability::unavailable("Action is not registered"),
            };
        };
        let resolved_target = resolve_target(descriptor.target_kind, target, state);
        let availability = descriptor.availability(state, target);
        ShellResolvedAction {
            descriptor: Some(descriptor.clone()),
            resolved_target,
            availability,
        }
    }

    /// Resolves and returns the effect that should be dispatched.
    pub fn execute(
        &self,
        id: ShellActionId,
        target: &ShellActionTarget,
        state: &WorkspaceState,
    ) -> ShellActionExecutionResult {
        let resolved = self.resolve(id, target, state);
        let Some(descriptor) = resolved.descriptor else {
            return ShellActionExecutionResult::Unavailable {
                reason: "Action is not registered".to_string(),
            };
        };
        if !resolved.availability.is_available() {
            let ShellActionAvailability::Unavailable { reason } = resolved.availability else {
                unreachable!("availability checked");
            };
            return ShellActionExecutionResult::Unavailable { reason };
        }

        ShellActionExecutionResult::Executed {
            effect: effect(descriptor.effect, &resolved.resolved_target),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum AvailabilityKind {
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
    fn evaluate(
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

fn standard_actions() -> Vec<ShellActionDescriptor> {
    use ShellActionId::*;
    use ShellActionModifier::*;
    use ShellActionShortcutContext::Shell;
    use ShellActionTargetKind::*;
    use ShellWorkspaceCommand as Cmd;

    vec![
        action(
            NewTerminalTab,
            "New Terminal Tab",
            Space,
            shortcut("t", vec![Command], Shell),
            ShellActionEffect::OpenTab {
                launch_target: crate::ShellLaunchTarget::Shell,
                space_id: None,
            },
            AvailabilityKind::Always,
        ),
        action(
            PaneSplitRight,
            "Split Right",
            Pane,
            shortcut("d", vec![Command], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::SplitRight,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneSplitDown,
            "Split Down",
            Pane,
            shortcut("d", vec![Command, Shift], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::SplitDown,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneSplitLeft,
            "Split Left",
            Pane,
            shortcut("d", vec![Command, Option], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::SplitLeft,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneSplitUp,
            "Split Up",
            Pane,
            shortcut("d", vec![Command, Option, Shift], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::SplitUp,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneEqualizeSplits,
            "Equalize Splits",
            CurrentSelection,
            shortcut("=", vec![Command, Option], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::EqualizeSplits,
            },
            AvailabilityKind::Always,
        ),
        action(
            PaneZoomToggle,
            "Zoom / Unzoom Pane",
            Pane,
            shortcut("return", vec![Command, Shift], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::TogglePaneZoom,
            },
            AvailabilityKind::SplitPane,
        ),
        action(
            PaneMoveLeft,
            "Move Pane Left",
            Pane,
            shortcut("leftArrow", vec![Command, Control, Shift], Shell),
            ShellActionEffect::MovePaneInTab {
                pane_id: None,
                placement: SplitPlacement::Left,
            },
            AvailabilityKind::PaneMovement(SplitPlacement::Left),
        ),
        action(
            PaneMoveRight,
            "Move Pane Right",
            Pane,
            shortcut("rightArrow", vec![Command, Control, Shift], Shell),
            ShellActionEffect::MovePaneInTab {
                pane_id: None,
                placement: SplitPlacement::Right,
            },
            AvailabilityKind::PaneMovement(SplitPlacement::Right),
        ),
        action(
            PaneMoveUp,
            "Move Pane Up",
            Pane,
            shortcut("upArrow", vec![Command, Control, Shift], Shell),
            ShellActionEffect::MovePaneInTab {
                pane_id: None,
                placement: SplitPlacement::Up,
            },
            AvailabilityKind::PaneMovement(SplitPlacement::Up),
        ),
        action(
            PaneMoveDown,
            "Move Pane Down",
            Pane,
            shortcut("downArrow", vec![Command, Control, Shift], Shell),
            ShellActionEffect::MovePaneInTab {
                pane_id: None,
                placement: SplitPlacement::Down,
            },
            AvailabilityKind::PaneMovement(SplitPlacement::Down),
        ),
        action(
            PaneFocusLeft,
            "Focus Pane Left",
            Pane,
            shortcut("leftArrow", vec![Command, Control], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::FocusLeft,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneFocusRight,
            "Focus Pane Right",
            Pane,
            shortcut("rightArrow", vec![Command, Control], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::FocusRight,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneFocusUp,
            "Focus Pane Up",
            Pane,
            shortcut("upArrow", vec![Command, Control], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::FocusUp,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneFocusDown,
            "Focus Pane Down",
            Pane,
            shortcut("downArrow", vec![Command, Control], Shell),
            ShellActionEffect::WorkspaceCommand {
                command: Cmd::FocusDown,
            },
            AvailabilityKind::FocusedPane,
        ),
        action(
            PaneClose,
            "Close Pane",
            Pane,
            shortcut("w", vec![Command, Shift], Shell),
            ShellActionEffect::ClosePane { pane_id: None },
            AvailabilityKind::FocusedPane,
        ),
        action(
            TerminalClear,
            "Clear Terminal",
            Pane,
            shortcut("k", vec![Command], Shell),
            ShellActionEffect::TerminalClear { pane_id: None },
            AvailabilityKind::TerminalContent,
        ),
        action(
            TabClose,
            "Close Tab",
            Tab,
            shortcut("w", vec![Command], Shell),
            ShellActionEffect::CloseTab { tab_id: None },
            AvailabilityKind::SelectedTab,
        ),
        action(
            TabRename,
            "Rename...",
            Tab,
            None,
            ShellActionEffect::RenameTab { tab_id: None },
            AvailabilityKind::SelectedTab,
        ),
        action(
            TabDuplicate,
            "Duplicate Tab",
            Tab,
            None,
            ShellActionEffect::DuplicateTab { tab_id: None },
            AvailabilityKind::DuplicateTab,
        ),
        action(
            TabOpenInSplitView,
            "Open in Split View",
            Tab,
            None,
            ShellActionEffect::OpenTabInSplitView { tab_id: None },
            AvailabilityKind::OpenTabInSplitView,
        ),
        action(
            TabSelectPrevious,
            "Previous Tab",
            CurrentSelection,
            shortcut("[", vec![Command, Shift], Shell),
            ShellActionEffect::SelectAdjacentTab { offset: -1 },
            AvailabilityKind::MultipleTabs,
        ),
        action(
            TabSelectNext,
            "Next Tab",
            CurrentSelection,
            shortcut("]", vec![Command, Shift], Shell),
            ShellActionEffect::SelectAdjacentTab { offset: 1 },
            AvailabilityKind::MultipleTabs,
        ),
        action(
            FindOpen,
            "Find",
            Pane,
            shortcut("f", vec![Command], Shell),
            ShellActionEffect::DisabledPlaceholder,
            AvailabilityKind::TerminalContent,
        ),
        action(
            SpaceSelectPrevious,
            "Previous Space",
            Space,
            shortcut("leftArrow", vec![Command, Option], Shell),
            ShellActionEffect::SelectAdjacentSpace { offset: -1 },
            AvailabilityKind::MultipleSpaces,
        ),
        action(
            SpaceSelectNext,
            "Next Space",
            Space,
            shortcut("rightArrow", vec![Command, Option], Shell),
            ShellActionEffect::SelectAdjacentSpace { offset: 1 },
            AvailabilityKind::MultipleSpaces,
        ),
        action(
            SpaceSelectByIndex,
            "Select Space",
            Space,
            None,
            ShellActionEffect::SelectSpaceAt { index: 0 },
            AvailabilityKind::IndexedSpace,
        ),
        action(
            TabPin,
            "Pin Tab",
            Tab,
            None,
            ShellActionEffect::PinTab { tab_id: None },
            AvailabilityKind::PinTab,
        ),
        action(
            TabUnpin,
            "Unpin Tab",
            Tab,
            None,
            ShellActionEffect::UnpinTab { tab_id: None },
            AvailabilityKind::UnpinTab,
        ),
        action(
            TabUpdatePin,
            "Update Pin",
            Tab,
            None,
            ShellActionEffect::UpdatePinnedTab { tab_id: None },
            AvailabilityKind::UpdatePinnedTab,
        ),
        action(
            TabMoveLeft,
            "Move Tab Left",
            Tab,
            shortcut("leftArrow", vec![Command, Option, Shift], Shell),
            ShellActionEffect::MoveTab {
                tab_id: None,
                offset: -1,
            },
            AvailabilityKind::MoveTab(-1),
        ),
        action(
            TabMoveRight,
            "Move Tab Right",
            Tab,
            shortcut("rightArrow", vec![Command, Option, Shift], Shell),
            ShellActionEffect::MoveTab {
                tab_id: None,
                offset: 1,
            },
            AvailabilityKind::MoveTab(1),
        ),
        action(
            TabMoveToSpace,
            "Move Tab to Space...",
            DestinationSpace,
            None,
            ShellActionEffect::MoveTabToSpace {
                tab_id: None,
                space_id: None,
            },
            AvailabilityKind::MoveTabToSpace,
        ),
    ]
}

fn action(
    id: ShellActionId,
    title: &str,
    target_kind: ShellActionTargetKind,
    default_shortcut: Option<ShellActionShortcut>,
    effect: ShellActionEffect,
    availability: AvailabilityKind,
) -> ShellActionDescriptor {
    ShellActionDescriptor::new(
        id,
        title,
        target_kind,
        default_shortcut,
        effect,
        availability,
    )
}

fn shortcut(
    key: &str,
    modifiers: Vec<ShellActionModifier>,
    context: ShellActionShortcutContext,
) -> Option<ShellActionShortcut> {
    Some(ShellActionShortcut::new(key, modifiers, context))
}

fn dynamic_shortcut_entries() -> Vec<(ShellActionShortcut, ShellActionId)> {
    (0..9)
        .filter_map(|index| {
            ShellActionShortcut::space_selection(index)
                .map(|shortcut| (shortcut, ShellActionId::SpaceSelectByIndex))
        })
        .collect()
}

fn resolve_target(
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

fn effect(
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
