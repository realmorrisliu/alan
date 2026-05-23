import Foundation

enum ShellActionID: String, CaseIterable, Identifiable, Hashable {
    case quickTerminalToggle = "shell.quick_terminal.toggle"
    case quickTerminalShow = "shell.quick_terminal.show"
    case quickTerminalHide = "shell.quick_terminal.hide"
    case quickTerminalFocus = "shell.quick_terminal.focus"
    case quickTerminalClose = "shell.quick_terminal.close"
    case quickTerminalPromote = "shell.quick_terminal.promote"
    case newTerminalTab = "shell.tab.new_terminal"
    case newAlanTab = "shell.tab.new_alan"
    case tabClose = "shell.tab.close"
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
    case findOpen = "shell.find.open"
    case spaceSelectPrevious = "shell.space.select_previous"
    case spaceSelectNext = "shell.space.select_next"
    case spaceSelectByIndex = "shell.space.select_by_index"
    case commandInputOpen = "shell.command_input.open"

    var id: String { rawValue }
}

enum ShellActionTargetKind: Equatable {
    case currentSelection
    case tab
    case pane
    case space
    case destinationSpace
}

enum ShellActionTarget: Equatable {
    case currentSelection
    case contextTab(String)
    case contextPane(String)
    case contextSpace(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)
}

enum ShellResolvedActionTarget: Equatable {
    case selection(spaceID: String?, tabID: String?, paneID: String?)
    case tab(String)
    case pane(String)
    case space(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)
    case unresolved
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
        commandInputActive: Bool,
        runtimeState: (String) -> ShellTerminalCommandRuntimeState
    ) -> ShellTerminalCommandResolution {
        if commandInputActive,
           source == .menuBar || source == .keyboardShortcut
        {
            return .shell(reason: "shell_text_input_active")
        }

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

enum ShellActionModifier: String, CaseIterable, Hashable, Comparable {
    case command
    case option
    case shift
    case control

    static func < (lhs: ShellActionModifier, rhs: ShellActionModifier) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

enum ShellActionShortcutContext: String, Hashable {
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
    case promoteQuickTerminal(spaceID: String?)
    case disabledPlaceholder
}

enum ShellActionExecutionResult: Equatable {
    case executed
    case failed(reason: String)
    case unavailable(reason: String)
}

enum ShellActionRegistryError: Error, Equatable {
    case duplicateActionID(ShellActionID)
    case duplicateShortcut(ShellActionShortcut, [ShellActionID])
}

struct ShellResolvedAction {
    let descriptor: ShellActionDescriptor?
    let resolvedTarget: ShellResolvedActionTarget
    let availability: ShellActionAvailability
}

struct ShellKeyboardAction: Equatable {
    let id: ShellActionID
    let target: ShellActionTarget
}

struct ShellActionDescriptor {
    let id: ShellActionID
    let title: String
    let targetKind: ShellActionTargetKind
    let defaultShortcut: ShellActionShortcut?
    let effect: ShellActionEffect
    private let availabilityEvaluator: (ShellStateSnapshot, ShellActionTarget) -> ShellActionAvailability

    init(
        id: ShellActionID,
        title: String,
        targetKind: ShellActionTargetKind,
        defaultShortcut: ShellActionShortcut? = nil,
        effect: ShellActionEffect,
        availability: @escaping (ShellStateSnapshot, ShellActionTarget) -> ShellActionAvailability = {
            _, _ in .available
        }
    ) {
        self.id = id
        self.title = title
        self.targetKind = targetKind
        self.defaultShortcut = defaultShortcut
        self.effect = effect
        availabilityEvaluator = availability
    }

    func availability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        availabilityEvaluator(state, target)
    }
}

final class ShellActionRegistry {
    let actions: [ShellActionDescriptor]

    static let standard: ShellActionRegistry = {
        do {
            return try ShellActionRegistry(actions: standardActions)
        } catch {
            preconditionFailure("Invalid shell action registry: \(error)")
        }
    }()

    init(actions: [ShellActionDescriptor]) throws {
        var actionIDs = Set<ShellActionID>()
        for action in actions {
            guard actionIDs.insert(action.id).inserted else {
                throw ShellActionRegistryError.duplicateActionID(action.id)
            }
        }

        let hasDynamicSpaceSelection = actions.contains { $0.id == .spaceSelectByIndex }
        let shortcutEntries =
            actions.compactMap { action -> (ShellActionShortcut, ShellActionID)? in
                guard let shortcut = action.defaultShortcut else { return nil }
                return (shortcut, action.id)
            }
            + (hasDynamicSpaceSelection ? Self.dynamicShortcutEntries() : [])
        let shortcuts = Dictionary(grouping: shortcutEntries, by: { $0.0 })

        for (shortcut, entries) in shortcuts where entries.count > 1 {
            throw ShellActionRegistryError.duplicateShortcut(shortcut, entries.map(\.1))
        }

        self.actions = actions
    }

    func action(for id: ShellActionID) -> ShellActionDescriptor? {
        actions.first { $0.id == id }
    }

    func defaultShortcut(
        for id: ShellActionID,
        target: ShellActionTarget = .currentSelection
    ) -> ShellActionShortcut? {
        if id == .spaceSelectByIndex,
           case .spaceIndex(let index) = target
        {
            return ShellActionShortcut.spaceSelection(index: index)
        }
        return action(for: id)?.defaultShortcut
    }

    func keyboardAction(for shortcut: ShellActionShortcut) -> ShellKeyboardAction? {
        if let action = actions.first(where: { $0.defaultShortcut == shortcut }) {
            return ShellKeyboardAction(id: action.id, target: .currentSelection)
        }
        if shortcut.context == .shell,
           shortcut.modifiers == [.command, .option],
           action(for: .spaceSelectByIndex) != nil,
           let value = Int(shortcut.key),
           (1...9).contains(value)
        {
            return ShellKeyboardAction(id: .spaceSelectByIndex, target: .spaceIndex(value - 1))
        }
        return nil
    }

    func resolve(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ShellResolvedAction {
        guard let descriptor = action(for: id) else {
            return ShellResolvedAction(
                descriptor: nil,
                resolvedTarget: .unresolved,
                availability: .unavailable(reason: "Action is not registered")
            )
        }

        let resolvedTarget = Self.resolveTarget(
            descriptor.targetKind,
            target: target,
            state: state
        )
        let availability = descriptor.availability(state: state, target: target)
        return ShellResolvedAction(
            descriptor: descriptor,
            resolvedTarget: resolvedTarget,
            availability: availability
        )
    }

    func execute(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot,
        handler: (ShellActionEffect) -> Bool
    ) -> ShellActionExecutionResult {
        let resolved = resolve(id, target: target, state: state)
        guard let descriptor = resolved.descriptor else {
            return .unavailable(reason: "Action is not registered")
        }
        guard case .available = resolved.availability else {
            if case .unavailable(let reason) = resolved.availability {
                return .unavailable(reason: reason)
            }
            return .unavailable(reason: "Action is unavailable")
        }

        let effect = Self.effect(descriptor.effect, resolvedTarget: resolved.resolvedTarget)
        guard handler(effect) else {
            return .failed(reason: "Action handler failed")
        }
        return .executed
    }

    private static func resolveTarget(
        _ targetKind: ShellActionTargetKind,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ShellResolvedActionTarget {
        switch targetKind {
        case .currentSelection:
            return .selection(
                spaceID: state.focusedSpaceID,
                tabID: state.focusedTabID,
                paneID: state.focusedPaneID
            )
        case .tab:
            if case .contextTab(let tabID) = target {
                return .tab(tabID)
            }
            return state.focusedTabID.map(ShellResolvedActionTarget.tab) ?? .unresolved
        case .pane:
            if case .contextPane(let paneID) = target {
                return .pane(paneID)
            }
            return state.focusedPaneID.map(ShellResolvedActionTarget.pane) ?? .unresolved
        case .space:
            switch target {
            case .contextSpace(let spaceID):
                return .space(spaceID)
            case .spaceIndex(let index):
                return .spaceIndex(index)
            default:
                return state.focusedSpaceID.map(ShellResolvedActionTarget.space) ?? .unresolved
            }
        case .destinationSpace:
            guard case .tabToSpace(let tabID, let spaceID) = target else {
                return .unresolved
            }
            return .tabToSpace(tabID: tabID, spaceID: spaceID)
        }
    }

    private static func effect(
        _ baseEffect: ShellActionEffect,
        resolvedTarget: ShellResolvedActionTarget
    ) -> ShellActionEffect {
        switch baseEffect {
        case .pinTab:
            if case .tab(let tabID) = resolvedTarget {
                return .pinTab(tabID)
            }
            return .pinTab(nil)
        case .unpinTab:
            if case .tab(let tabID) = resolvedTarget {
                return .unpinTab(tabID)
            }
            return .unpinTab(nil)
        case .updatePinnedTab:
            if case .tab(let tabID) = resolvedTarget {
                return .updatePinnedTab(tabID)
            }
            return .updatePinnedTab(nil)
        case .moveTab(_, let offset):
            if case .tab(let tabID) = resolvedTarget {
                return .moveTab(tabID, offset: offset)
            }
            return .moveTab(nil, offset: offset)
        case .moveTabToSpace:
            if case .tabToSpace(let tabID, let spaceID) = resolvedTarget {
                return .moveTabToSpace(tabID: tabID, spaceID: spaceID)
            }
            return .moveTabToSpace(tabID: nil, spaceID: nil)
        case .movePaneInTab(_, let placement):
            if case .pane(let paneID) = resolvedTarget {
                return .movePaneInTab(paneID, placement: placement)
            }
            return .movePaneInTab(nil, placement: placement)
        case .promoteQuickTerminal:
            if case .space(let spaceID) = resolvedTarget {
                return .promoteQuickTerminal(spaceID: spaceID)
            }
            return .promoteQuickTerminal(spaceID: nil)
        case .closeTab:
            if case .tab(let tabID) = resolvedTarget {
                return .closeTab(tabID)
            }
            return .closeTab(nil)
        case .closePane:
            if case .pane(let paneID) = resolvedTarget {
                return .closePane(paneID)
            }
            return .closePane(nil)
        case .openTab(let launchTarget, _):
            if case .space(let spaceID) = resolvedTarget {
                return .openTab(launchTarget, spaceID: spaceID)
            }
            return baseEffect
        case .selectSpaceAt:
            if case .spaceIndex(let index) = resolvedTarget {
                return .selectSpaceAt(index)
            }
            return baseEffect
        default:
            return baseEffect
        }
    }

    private static func dynamicShortcutEntries() -> [(ShellActionShortcut, ShellActionID)] {
        (0..<9).compactMap { index in
            ShellActionShortcut.spaceSelection(index: index).map {
                ($0, .spaceSelectByIndex)
            }
        }
    }
}

private let standardActions: [ShellActionDescriptor] = [
    ShellActionDescriptor(
        id: .quickTerminalToggle,
        title: "Toggle Quick Terminal",
        targetKind: .currentSelection,
        defaultShortcut: ShellActionShortcut(key: "space", modifiers: [.option], context: .shell),
        effect: .workspaceCommand(.quickTerminalToggle)
    ),
    ShellActionDescriptor(
        id: .quickTerminalShow,
        title: "Show Quick Terminal",
        targetKind: .currentSelection,
        effect: .workspaceCommand(.quickTerminalShow)
    ),
    ShellActionDescriptor(
        id: .quickTerminalHide,
        title: "Hide Quick Terminal",
        targetKind: .currentSelection,
        effect: .workspaceCommand(.quickTerminalHide)
    ),
    ShellActionDescriptor(
        id: .quickTerminalFocus,
        title: "Focus Quick Terminal",
        targetKind: .currentSelection,
        effect: .workspaceCommand(.quickTerminalFocus)
    ),
    ShellActionDescriptor(
        id: .quickTerminalClose,
        title: "Close Quick Terminal",
        targetKind: .currentSelection,
        effect: .workspaceCommand(.quickTerminalClose)
    ),
    ShellActionDescriptor(
        id: .quickTerminalPromote,
        title: "Open Quick Terminal in Space...",
        targetKind: .space,
        effect: .promoteQuickTerminal(spaceID: nil),
        availability: quickTerminalPromoteAvailability
    ),
    ShellActionDescriptor(
        id: .newTerminalTab,
        title: "New Terminal Tab",
        targetKind: .space,
        defaultShortcut: ShellActionShortcut(key: "t", modifiers: [.command], context: .shell),
        effect: .openTab(.shell, spaceID: nil)
    ),
    ShellActionDescriptor(
        id: .newAlanTab,
        title: "New alan tab",
        targetKind: .space,
        defaultShortcut: ShellActionShortcut(key: "t", modifiers: [.command, .option], context: .shell),
        effect: .openTab(.alan, spaceID: nil)
    ),
    ShellActionDescriptor(
        id: .paneSplitRight,
        title: "Split Right",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "d", modifiers: [.command], context: .shell),
        effect: .workspaceCommand(.splitRight),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneSplitDown,
        title: "Split Down",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "d", modifiers: [.command, .shift], context: .shell),
        effect: .workspaceCommand(.splitDown),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneSplitLeft,
        title: "Split Left",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "d", modifiers: [.command, .option], context: .shell),
        effect: .workspaceCommand(.splitLeft),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneSplitUp,
        title: "Split Up",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "d", modifiers: [.command, .option, .shift], context: .shell),
        effect: .workspaceCommand(.splitUp),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneEqualizeSplits,
        title: "Equalize Splits",
        targetKind: .currentSelection,
        defaultShortcut: ShellActionShortcut(key: "=", modifiers: [.command, .option], context: .shell),
        effect: .workspaceCommand(.equalizeSplits)
    ),
    ShellActionDescriptor(
        id: .paneZoomToggle,
        title: "Zoom / Unzoom Pane",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "return", modifiers: [.command, .shift], context: .shell),
        effect: .workspaceCommand(.togglePaneZoom),
        availability: splitPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneMoveLeft,
        title: "Move Pane Left",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(
            key: "leftArrow",
            modifiers: [.command, .control, .shift],
            context: .shell
        ),
        effect: .movePaneInTab(nil, placement: .left),
        availability: paneMovementAvailability(.left)
    ),
    ShellActionDescriptor(
        id: .paneMoveRight,
        title: "Move Pane Right",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(
            key: "rightArrow",
            modifiers: [.command, .control, .shift],
            context: .shell
        ),
        effect: .movePaneInTab(nil, placement: .right),
        availability: paneMovementAvailability(.right)
    ),
    ShellActionDescriptor(
        id: .paneMoveUp,
        title: "Move Pane Up",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(
            key: "upArrow",
            modifiers: [.command, .control, .shift],
            context: .shell
        ),
        effect: .movePaneInTab(nil, placement: .up),
        availability: paneMovementAvailability(.up)
    ),
    ShellActionDescriptor(
        id: .paneMoveDown,
        title: "Move Pane Down",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(
            key: "downArrow",
            modifiers: [.command, .control, .shift],
            context: .shell
        ),
        effect: .movePaneInTab(nil, placement: .down),
        availability: paneMovementAvailability(.down)
    ),
    ShellActionDescriptor(
        id: .paneFocusLeft,
        title: "Focus Pane Left",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "leftArrow", modifiers: [.command, .control], context: .shell),
        effect: .workspaceCommand(.focusLeft),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneFocusRight,
        title: "Focus Pane Right",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "rightArrow", modifiers: [.command, .control], context: .shell),
        effect: .workspaceCommand(.focusRight),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneFocusUp,
        title: "Focus Pane Up",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "upArrow", modifiers: [.command, .control], context: .shell),
        effect: .workspaceCommand(.focusUp),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneFocusDown,
        title: "Focus Pane Down",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "downArrow", modifiers: [.command, .control], context: .shell),
        effect: .workspaceCommand(.focusDown),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .paneClose,
        title: "Close Pane",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "w", modifiers: [.command, .shift], context: .shell),
        effect: .closePane(nil),
        availability: focusedPaneAvailability
    ),
    ShellActionDescriptor(
        id: .tabClose,
        title: "Close Tab",
        targetKind: .tab,
        defaultShortcut: ShellActionShortcut(key: "w", modifiers: [.command], context: .shell),
        effect: .closeTab(nil),
        availability: selectedTabAvailability
    ),
    ShellActionDescriptor(
        id: .tabSelectPrevious,
        title: "Previous Tab",
        targetKind: .currentSelection,
        defaultShortcut: ShellActionShortcut(key: "[", modifiers: [.command, .shift], context: .shell),
        effect: .selectAdjacentTab(-1),
        availability: multipleTabsAvailability
    ),
    ShellActionDescriptor(
        id: .tabSelectNext,
        title: "Next Tab",
        targetKind: .currentSelection,
        defaultShortcut: ShellActionShortcut(key: "]", modifiers: [.command, .shift], context: .shell),
        effect: .selectAdjacentTab(1),
        availability: multipleTabsAvailability
    ),
    ShellActionDescriptor(
        id: .findOpen,
        title: "Find",
        targetKind: .pane,
        defaultShortcut: ShellActionShortcut(key: "f", modifiers: [.command], context: .shell),
        effect: .disabledPlaceholder,
        availability: terminalContentAvailability
    ),
    ShellActionDescriptor(
        id: .spaceSelectPrevious,
        title: "Previous Space",
        targetKind: .space,
        defaultShortcut: ShellActionShortcut(key: "leftArrow", modifiers: [.command, .option], context: .shell),
        effect: .selectAdjacentSpace(-1),
        availability: multipleSpacesAvailability
    ),
    ShellActionDescriptor(
        id: .spaceSelectNext,
        title: "Next Space",
        targetKind: .space,
        defaultShortcut: ShellActionShortcut(key: "rightArrow", modifiers: [.command, .option], context: .shell),
        effect: .selectAdjacentSpace(1),
        availability: multipleSpacesAvailability
    ),
    ShellActionDescriptor(
        id: .spaceSelectByIndex,
        title: "Select Space",
        targetKind: .space,
        effect: .selectSpaceAt(0),
        availability: indexedSpaceAvailability
    ),
    ShellActionDescriptor(
        id: .tabPin,
        title: "Pin Tab",
        targetKind: .tab,
        effect: .pinTab(nil),
        availability: pinTabAvailability
    ),
    ShellActionDescriptor(
        id: .tabUnpin,
        title: "Unpin Tab",
        targetKind: .tab,
        effect: .unpinTab(nil),
        availability: unpinTabAvailability
    ),
    ShellActionDescriptor(
        id: .tabUpdatePin,
        title: "Update Pin",
        targetKind: .tab,
        effect: .updatePinnedTab(nil),
        availability: updatePinnedTabAvailability
    ),
    ShellActionDescriptor(
        id: .tabMoveLeft,
        title: "Move Tab Left",
        targetKind: .tab,
        defaultShortcut: ShellActionShortcut(
            key: "leftArrow",
            modifiers: [.command, .option, .shift],
            context: .shell
        ),
        effect: .moveTab(nil, offset: -1),
        availability: moveTabAvailability(offset: -1)
    ),
    ShellActionDescriptor(
        id: .tabMoveRight,
        title: "Move Tab Right",
        targetKind: .tab,
        defaultShortcut: ShellActionShortcut(
            key: "rightArrow",
            modifiers: [.command, .option, .shift],
            context: .shell
        ),
        effect: .moveTab(nil, offset: 1),
        availability: moveTabAvailability(offset: 1)
    ),
    ShellActionDescriptor(
        id: .tabMoveToSpace,
        title: "Move Tab to Space...",
        targetKind: .destinationSpace,
        effect: .moveTabToSpace(tabID: nil, spaceID: nil),
        availability: moveTabToSpaceAvailability
    ),
]

private func focusedPaneAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    switch target {
    case .contextPane(let paneID):
        return state.pane(paneID: paneID) == nil
            ? .unavailable(reason: "Pane is not available")
            : .available
    default:
        return state.focusedPaneID == nil
            ? .unavailable(reason: "No focused pane")
            : .available
    }
}

private func terminalContentAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    let paneID: String?
    if case .contextPane(let contextPaneID) = target {
        paneID = contextPaneID
    } else {
        paneID = state.focusedPaneID
    }

    guard let paneID else {
        return .unavailable(reason: "No focused pane")
    }
    guard let pane = state.pane(paneID: paneID) else {
        return .unavailable(reason: "Pane is not available")
    }
    guard terminalContentIDIfAvailable(for: pane, in: state) != nil else {
        return .unavailable(reason: "Focused content is not a terminal")
    }
    return .available
}

private func terminalContentIDIfAvailable(
    for pane: ShellPane,
    in state: ShellStateSnapshot
) -> String? {
    if let mountedContent = state.contentStateProjection().contentMounted(in: pane.paneID) {
        return mountedContent.kind == .terminal ? mountedContent.contentID : nil
    }

    if pane.isQuickTerminalPane,
       state.quickTerminal?.paneID == pane.paneID
    {
        return pane.terminalContentID
    }

    return nil
}

private func splitPaneAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    let paneID: String?
    if case .contextPane(let contextPaneID) = target {
        paneID = contextPaneID
    } else {
        paneID = state.focusedPaneID
    }

    guard let paneID,
          let pane = state.pane(paneID: paneID),
          let tab = state.tab(tabID: pane.tabID)
    else {
        return .unavailable(reason: "No focused pane")
    }

    return tab.paneTree.paneIDs.count > 1
        ? .available
        : .unavailable(reason: "Pane zoom requires a split tab")
}

private func paneMovementAvailability(
    _ placement: ShellPaneSplitDirection
) -> (ShellStateSnapshot, ShellActionTarget) -> ShellActionAvailability {
    { state, target in
        let paneID: String?
        if case .contextPane(let contextPaneID) = target {
            paneID = contextPaneID
        } else {
            paneID = state.focusedPaneID
        }

        guard let paneID,
              let pane = state.pane(paneID: paneID),
              let tab = state.tab(tabID: pane.tabID)
        else {
            return .unavailable(reason: "No focused pane")
        }

        guard tab.paneTree.paneIDs.count > 1 else {
            return .unavailable(reason: "Pane movement requires a split tab")
        }

        guard tab.paneTree.adjacentPaneID(
            from: paneID,
            direction: placement.spatialFocusDirection
        ) != nil else {
            return .unavailable(reason: "No adjacent pane in that direction")
        }

        return .available
    }
}

private func selectedTabAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    switch target {
    case .contextTab(let tabID):
        return state.tab(tabID: tabID) == nil
            ? .unavailable(reason: "Tab is not available")
            : .available
    default:
        return state.focusedTabID == nil
            ? .unavailable(reason: "No selected tab")
            : .available
    }
}

private func targetedTab(
    in state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellTab? {
    switch target {
    case .contextTab(let tabID):
        return state.tab(tabID: tabID)
    default:
        return state.focusedTabID.flatMap { state.tab(tabID: $0) }
    }
}

private func pinTabAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard let tab = targetedTab(in: state, target: target) else {
        return .unavailable(reason: "Tab is not available")
    }
    return tab.isPinned
        ? .unavailable(reason: "Tab is already pinned")
        : .available
}

private func unpinTabAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard let tab = targetedTab(in: state, target: target) else {
        return .unavailable(reason: "Tab is not available")
    }
    return tab.isPinned
        ? .available
        : .unavailable(reason: "Tab is not pinned")
}

private func updatePinnedTabAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard let tab = targetedTab(in: state, target: target) else {
        return .unavailable(reason: "Tab is not available")
    }
    return tab.isPinned
        ? .available
        : .unavailable(reason: "Tab is not pinned")
}

private func moveTabAvailability(
    offset: Int
) -> (ShellStateSnapshot, ShellActionTarget) -> ShellActionAvailability {
    { state, target in
        guard let tab = targetedTab(in: state, target: target),
              let location = state.tabOrganizationLocation(tabID: tab.tabID),
              let space = state.space(spaceID: location.spaceID)
        else {
            return .unavailable(reason: "Tab is not available")
        }

        let sectionCount = space.tabs(in: location.section).count
        let nextIndex = location.index + offset
        return (0..<sectionCount).contains(nextIndex)
            ? .available
            : .unavailable(reason: "No adjacent tab in section")
    }
}

private func moveTabToSpaceAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard case .tabToSpace(let tabID, let spaceID) = target else {
        return .unavailable(reason: "Move target is required")
    }
    guard let location = state.tabOrganizationLocation(tabID: tabID) else {
        return .unavailable(reason: "Tab is not available")
    }
    guard state.space(spaceID: spaceID) != nil else {
        return .unavailable(reason: "Space is not available")
    }
    return location.spaceID == spaceID
        ? .unavailable(reason: "Tab is already in that space")
        : .available
}

private func multipleTabsAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard let selectedSpaceID = state.focusedSpaceID,
          let selectedSpace = state.space(spaceID: selectedSpaceID),
          selectedSpace.tabs.count > 1
    else {
        return .unavailable(reason: "No adjacent tab")
    }
    return state.focusedTabID == nil
        ? .unavailable(reason: "No selected tab")
        : .available
}

private func multipleSpacesAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    state.spaces.count > 1
        ? .available
        : .unavailable(reason: "No adjacent space")
}

private func indexedSpaceAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard case .spaceIndex(let index) = target else {
        return .unavailable(reason: "Space index is required")
    }
    return state.spaces.indices.contains(index)
        ? .available
        : .unavailable(reason: "Space is not available")
}

private func quickTerminalPromoteAvailability(
    state: ShellStateSnapshot,
    target: ShellActionTarget
) -> ShellActionAvailability {
    guard state.quickTerminal != nil else {
        return .unavailable(reason: "Quick terminal is not available")
    }
    guard case .contextSpace(let spaceID) = target else {
        return .unavailable(reason: "Quick terminal destination is required")
    }
    return state.space(spaceID: spaceID) == nil
        ? .unavailable(reason: "Space is not available")
        : .available
}

private func disabledPlaceholder(
    _ reason: String
) -> (ShellStateSnapshot, ShellActionTarget) -> ShellActionAvailability {
    { _, _ in .unavailable(reason: reason) }
}
