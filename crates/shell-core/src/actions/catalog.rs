use super::{
    AvailabilityKind, ShellActionDescriptor, ShellActionEffect, ShellActionId, ShellActionModifier,
    ShellActionShortcut, ShellActionShortcutContext, ShellActionTargetKind, ShellWorkspaceCommand,
};
use crate::SplitPlacement;

pub(super) fn standard_actions() -> Vec<ShellActionDescriptor> {
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

pub(super) fn dynamic_shortcut_entries() -> Vec<(ShellActionShortcut, ShellActionId)> {
    (0..9)
        .filter_map(|index| {
            ShellActionShortcut::space_selection(index)
                .map(|shortcut| (shortcut, ShellActionId::SpaceSelectByIndex))
        })
        .collect()
}
