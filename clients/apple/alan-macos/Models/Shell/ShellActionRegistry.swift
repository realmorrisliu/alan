import Foundation

enum ShellActionID: String, Codable, CaseIterable, Identifiable, Hashable {
    case newTerminalTab = "shell.tab.new_terminal"
    case tabClose = "shell.tab.close"
    case tabRename = "shell.tab.rename"
    case tabDuplicate = "shell.tab.duplicate"
    case tabOpenInSplitView = "shell.tab.open_in_split_view"
    case tabSelectPrevious = "shell.tab.select_previous"
    case tabSelectNext = "shell.tab.select_next"
    case tabPin = "shell.tab.pin"
    case tabUnpin = "shell.tab.unpin"
    case tabUpdatePin = "shell.tab.update_pin"
    case tabMoveLeft = "shell.tab.move_left"
    case tabMoveRight = "shell.tab.move_right"
    case tabMoveToSpace = "shell.tab.move_to_space"
    case paneSplitLeft = "shell.pane.split_left"
    case paneSplitRight = "shell.pane.split_right"
    case paneSplitUp = "shell.pane.split_up"
    case paneSplitDown = "shell.pane.split_down"
    case paneFocusLeft = "shell.pane.focus_left"
    case paneFocusRight = "shell.pane.focus_right"
    case paneFocusUp = "shell.pane.focus_up"
    case paneFocusDown = "shell.pane.focus_down"
    case paneEqualizeSplits = "shell.pane.equalize_splits"
    case paneZoomToggle = "shell.pane.zoom_toggle"
    case paneMoveLeft = "shell.pane.move_left"
    case paneMoveRight = "shell.pane.move_right"
    case paneMoveUp = "shell.pane.move_up"
    case paneMoveDown = "shell.pane.move_down"
    case paneClose = "shell.pane.close"
    case terminalClear = "shell.terminal.clear"
    case findOpen = "shell.find.open"
    case spaceSelectPrevious = "shell.space.select_previous"
    case spaceSelectNext = "shell.space.select_next"
    case spaceSelectByIndex = "shell.space.select_by_index"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .newTerminalTab:
            return "New Terminal Tab"
        case .tabClose:
            return "Close Tab"
        case .tabRename:
            return "Rename..."
        case .tabDuplicate:
            return "Duplicate Tab"
        case .tabOpenInSplitView:
            return "Open in Split View"
        case .tabSelectPrevious:
            return "Previous Tab"
        case .tabSelectNext:
            return "Next Tab"
        case .tabPin:
            return "Pin Tab"
        case .tabUnpin:
            return "Unpin Tab"
        case .tabUpdatePin:
            return "Update Pin"
        case .tabMoveLeft:
            return "Move Tab Left"
        case .tabMoveRight:
            return "Move Tab Right"
        case .tabMoveToSpace:
            return "Move Tab to Space..."
        case .paneSplitLeft:
            return "Split Left"
        case .paneSplitRight:
            return "Split Right"
        case .paneSplitUp:
            return "Split Up"
        case .paneSplitDown:
            return "Split Down"
        case .paneFocusLeft:
            return "Focus Pane Left"
        case .paneFocusRight:
            return "Focus Pane Right"
        case .paneFocusUp:
            return "Focus Pane Up"
        case .paneFocusDown:
            return "Focus Pane Down"
        case .paneEqualizeSplits:
            return "Equalize Splits"
        case .paneZoomToggle:
            return "Zoom / Unzoom Pane"
        case .paneMoveLeft:
            return "Move Pane Left"
        case .paneMoveRight:
            return "Move Pane Right"
        case .paneMoveUp:
            return "Move Pane Up"
        case .paneMoveDown:
            return "Move Pane Down"
        case .paneClose:
            return "Close Pane"
        case .terminalClear:
            return "Clear Terminal"
        case .findOpen:
            return "Find"
        case .spaceSelectPrevious:
            return "Previous Space"
        case .spaceSelectNext:
            return "Next Space"
        case .spaceSelectByIndex:
            return "Select Space"
        }
    }
}

enum ShellActionTarget: Equatable {
    case currentSelection
    case contextTab(String)
    case contextPane(String)
    case contextSpace(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)
}

enum ShellActionSurface: String, Equatable {
    case menuBar = "menu_bar"
    case contextMenu = "context_menu"
    case keyboard
}

enum ShellTerminalCommand: String, Equatable {
    case copySelection = "copy_selection"
    case paste
    case search
    case copyLastCommandOutput = "copy_last_command_output"
    case searchLastCommandOutput = "search_last_command_output"
}

enum ShellTerminalCommandSource: String, Equatable {
    case menuBar = "menu_bar"
    case keyboardShortcut = "keyboard_shortcut"
    case commandUI = "command_ui"
    case contextMenu = "context_menu"
    case terminalHost = "terminal_host"
}

struct ShellTerminalCommandRuntimeState: Equatable {
    let paneID: String
    let hasSelection: Bool
    let inputReady: Bool
    let searchAvailable: Bool
    let hasReliableSemanticCommands: Bool
}

struct ShellTerminalCommandTarget: Equatable {
    let paneID: String
    let tabID: String
    let spaceID: String
    let mountedContentID: String
}

enum ShellTerminalCommandResolution: Equatable {
    case terminal(ShellTerminalCommandTarget)
    case shell(reason: String)

    var terminalTarget: ShellTerminalCommandTarget? {
        guard case .terminal(let target) = self else { return nil }
        return target
    }
}

enum ShellCommandTargetResolver {
    static func resolveTerminalCommand(
        _ command: ShellTerminalCommand,
        source: ShellTerminalCommandSource,
        target: ShellActionTarget,
        state: ShellStateSnapshot,
        runtimeState: (String) -> ShellTerminalCommandRuntimeState
    ) -> ShellTerminalCommandResolution {
        let paneID: String?
        if case .contextPane(let contextPaneID) = target {
            paneID = contextPaneID
        } else {
            paneID = state.focusedPaneID
        }

        guard let paneID,
              let pane = state.pane(paneID: paneID)
        else {
            return .shell(reason: "terminal_pane_unavailable")
        }
        guard let mountedContentID = terminalContentIDIfAvailable(
            for: pane,
            in: state
        )
        else {
            return .shell(reason: "terminal_content_unavailable")
        }

        let runtime = runtimeState(paneID)
        switch command {
        case .copySelection:
            guard runtime.hasSelection else {
                return .shell(reason: "terminal_selection_unavailable")
            }
        case .paste:
            guard runtime.inputReady else {
                return .shell(reason: "terminal_input_unavailable")
            }
        case .search:
            guard runtime.searchAvailable else {
                return .shell(reason: "terminal_search_unavailable")
            }
        case .copyLastCommandOutput, .searchLastCommandOutput:
            guard runtime.hasReliableSemanticCommands else {
                return .shell(reason: "terminal_semantic_commands_unavailable")
            }
        }

        return .terminal(
            ShellTerminalCommandTarget(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                mountedContentID: mountedContentID
            )
        )
    }
}

func terminalContentIDIfAvailable(
    for pane: ShellPane,
    in state: ShellStateSnapshot
) -> String? {
    if let mountedContent = state.explicitContentMounted(in: pane.paneID) {
        return mountedContent.kind == .terminal ? mountedContent.contentID : nil
    }

    if state.isTerminalBackedPane(pane) {
        return pane.terminalContentID
    }

    return nil
}

enum ShellActionModifier: String, Codable, CaseIterable, Hashable, Comparable {
    case command
    case option
    case shift
    case control

    static func < (lhs: ShellActionModifier, rhs: ShellActionModifier) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

enum ShellActionShortcutContext: String, Codable, Hashable {
    case shell
    case terminalFind = "terminal_find"
}

struct ShellActionShortcut: Hashable {
    let key: String
    let modifiers: Set<ShellActionModifier>
    let context: ShellActionShortcutContext

    static func spaceSelection(index: Int) -> ShellActionShortcut? {
        guard (0..<9).contains(index) else { return nil }
        return ShellActionShortcut(
            key: String(index + 1),
            modifiers: [.command, .option],
            context: .shell
        )
    }
}

enum ShellActionMetadataCatalog {
    static func keyboardAction(for shortcut: ShellActionShortcut) -> ShellKeyboardAction? {
        let normalized = normalize(shortcut)
        if normalized.context == .shell,
           normalized.modifiers == [.command, .option],
           let value = Int(normalized.key),
           (1...9).contains(value)
        {
            return ShellKeyboardAction(
                id: .spaceSelectByIndex,
                target: .spaceIndex(value - 1)
            )
        }

        for id in ShellActionID.allCases {
            guard let defaultShortcut = self.shortcut(id),
                  normalize(defaultShortcut) == normalized
            else {
                continue
            }
            return ShellKeyboardAction(id: id, target: .currentSelection)
        }
        return nil
    }

    static func shortcut(
        _ id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionShortcut? {
        if case .spaceSelectByIndex = id,
           case .spaceIndex(let index) = target
        {
            return ShellActionShortcut.spaceSelection(index: index)
        }

        switch id {
        case .newTerminalTab:
            return shortcut("t", [.command])
        case .tabClose:
            return shortcut("w", [.command])
        case .tabSelectPrevious:
            return shortcut("[", [.command, .shift])
        case .tabSelectNext:
            return shortcut("]", [.command, .shift])
        case .tabMoveLeft:
            return shortcut("leftArrow", [.command, .option, .shift])
        case .tabMoveRight:
            return shortcut("rightArrow", [.command, .option, .shift])
        case .paneSplitLeft:
            return shortcut("d", [.command, .option])
        case .paneSplitRight:
            return shortcut("d", [.command])
        case .paneSplitUp:
            return shortcut("d", [.command, .option, .shift])
        case .paneSplitDown:
            return shortcut("d", [.command, .shift])
        case .paneFocusLeft:
            return shortcut("leftArrow", [.command, .control])
        case .paneFocusRight:
            return shortcut("rightArrow", [.command, .control])
        case .paneFocusUp:
            return shortcut("upArrow", [.command, .control])
        case .paneFocusDown:
            return shortcut("downArrow", [.command, .control])
        case .paneEqualizeSplits:
            return shortcut("=", [.command, .option])
        case .paneZoomToggle:
            return shortcut("return", [.command, .shift])
        case .paneMoveLeft:
            return shortcut("leftArrow", [.command, .control, .shift])
        case .paneMoveRight:
            return shortcut("rightArrow", [.command, .control, .shift])
        case .paneMoveUp:
            return shortcut("upArrow", [.command, .control, .shift])
        case .paneMoveDown:
            return shortcut("downArrow", [.command, .control, .shift])
        case .paneClose:
            return shortcut("w", [.command, .shift])
        case .terminalClear:
            return shortcut("k", [.command])
        case .findOpen:
            return shortcut("f", [.command])
        case .spaceSelectPrevious:
            return shortcut("leftArrow", [.command, .option])
        case .spaceSelectNext:
            return shortcut("rightArrow", [.command, .option])
        case .tabRename,
             .tabDuplicate,
             .tabOpenInSplitView,
             .tabPin,
             .tabUnpin,
             .tabUpdatePin,
             .tabMoveToSpace,
             .spaceSelectByIndex:
            return nil
        }
    }

    private static func shortcut(
        _ key: String,
        _ modifiers: Set<ShellActionModifier>,
        context: ShellActionShortcutContext = .shell
    ) -> ShellActionShortcut {
        ShellActionShortcut(key: key, modifiers: modifiers, context: context)
    }

    private static func normalize(_ shortcut: ShellActionShortcut) -> ShellActionShortcut {
        ShellActionShortcut(
            key: shortcut.key,
            modifiers: Set(shortcut.modifiers),
            context: shortcut.context
        )
    }
}

enum ShellActionAvailability: Equatable {
    case available
    case unavailable(reason: String)

    var isAvailable: Bool {
        self == .available
    }
}

enum ShellActionEffect: Equatable {
    case workspaceCommand(ShellWorkspaceCommand)
    case openTab(ShellLaunchTarget, spaceID: String?)
    case closeTab(String?)
    case renameTab(String?)
    case duplicateTab(String?)
    case openTabInSplitView(String?)
    case closePane(String?)
    case selectAdjacentTab(Int)
    case selectAdjacentSpace(Int)
    case selectSpaceAt(Int)
    case pinTab(String?)
    case unpinTab(String?)
    case updatePinnedTab(String?)
    case moveTab(String?, offset: Int)
    case moveTabToSpace(tabID: String?, spaceID: String?)
    case movePaneInTab(String?, placement: ShellPaneSplitDirection)
    case terminalClear(String?)
    case disabledPlaceholder
}

enum ShellActionExecutionResult: Equatable {
    case executed
    case failed(reason: String)
    case unavailable(reason: String)
}

struct ShellKeyboardAction: Equatable {
    let id: ShellActionID
    let target: ShellActionTarget
}
