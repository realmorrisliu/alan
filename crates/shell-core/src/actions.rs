mod catalog;
mod evaluation;

use crate::{SplitPlacement, WorkspaceState};
use catalog::{dynamic_shortcut_entries, standard_actions};
use evaluation::{AvailabilityKind, effect, resolve_target};
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
