import Foundation

@main
struct ShellActionRegistryTestRunner {
    static func main() throws {
        try ShellActionRegistryTests.run()
        try ShellActionRegistryFixtureExporter.exportIfRequested()
    }
}

private enum ShellActionRegistryTests {
    static func run() throws {
        try verifiesActionIDsAreUniqueAndStable()
        try verifiesStandardShortcutDefaults()
        try verifiesKeyboardActionLookup()
        try verifiesShortcutConflictsAreRejected()
        try verifiesDynamicSpaceShortcutConflictsAreRejected()
        try verifiesContextTabTargetDoesNotSelectTabFirst()
        try verifiesMoveToSpaceRequiresExplicitTarget()
        try verifiesUnavailableShortcutActionDoesNotExecuteHandler()
        try verifiesMoveTabShortcutRoutesHandler()
        try verifiesRemovedAlanActionsRemainOutOfRegistry()
        try verifiesQuickTerminalActionsRouteThroughSharedRegistry()
        try verifiesQuickTerminalPromoteRequiresExplicitDestination()
        try verifiesPaneZoomRoutesThroughSharedRegistry()
        try verifiesPaneMovementRoutesThroughSharedRegistry()
        print("Shell action registry tests passed.")
    }

    private static func verifiesActionIDsAreUniqueAndStable() throws {
        let registry = ShellActionRegistry.standard
        let ids = registry.actions.map(\.id.rawValue)

        expect(Set(ids).count == ids.count, "shell action ids must be unique")
        expect(
            registry.action(for: .tabPin)?.id.rawValue == "shell.tab.pin",
            "pin-tab action id must stay stable"
        )
        expect(
            registry.action(for: .paneSplitRight)?.title == "Split Right",
            "registered actions must expose user-facing labels"
        )
    }

    private static func verifiesStandardShortcutDefaults() throws {
        let registry = ShellActionRegistry.standard

        let expectedShortcuts: [(ShellActionID, ShellActionShortcut)] = [
            (.newTerminalTab, ShellActionShortcut(key: "t", modifiers: [.command], context: .shell)),
            (.tabClose, ShellActionShortcut(key: "w", modifiers: [.command], context: .shell)),
            (.tabSelectPrevious, ShellActionShortcut(key: "[", modifiers: [.command, .shift], context: .shell)),
            (.tabSelectNext, ShellActionShortcut(key: "]", modifiers: [.command, .shift], context: .shell)),
            (
                .tabMoveLeft,
                ShellActionShortcut(
                    key: "leftArrow",
                    modifiers: [.command, .option, .shift],
                    context: .shell
                )
            ),
            (
                .tabMoveRight,
                ShellActionShortcut(
                    key: "rightArrow",
                    modifiers: [.command, .option, .shift],
                    context: .shell
                )
            ),
            (.paneSplitRight, ShellActionShortcut(key: "d", modifiers: [.command], context: .shell)),
            (.paneSplitDown, ShellActionShortcut(key: "d", modifiers: [.command, .shift], context: .shell)),
            (.paneSplitLeft, ShellActionShortcut(key: "d", modifiers: [.command, .option], context: .shell)),
            (
                .paneSplitUp,
                ShellActionShortcut(key: "d", modifiers: [.command, .option, .shift], context: .shell)
            ),
            (.paneEqualizeSplits, ShellActionShortcut(key: "=", modifiers: [.command, .option], context: .shell)),
            (.paneZoomToggle, ShellActionShortcut(key: "return", modifiers: [.command, .shift], context: .shell)),
            (
                .paneMoveLeft,
                ShellActionShortcut(
                    key: "leftArrow",
                    modifiers: [.command, .control, .shift],
                    context: .shell
                )
            ),
            (
                .paneMoveRight,
                ShellActionShortcut(
                    key: "rightArrow",
                    modifiers: [.command, .control, .shift],
                    context: .shell
                )
            ),
            (
                .paneMoveUp,
                ShellActionShortcut(
                    key: "upArrow",
                    modifiers: [.command, .control, .shift],
                    context: .shell
                )
            ),
            (
                .paneMoveDown,
                ShellActionShortcut(
                    key: "downArrow",
                    modifiers: [.command, .control, .shift],
                    context: .shell
                )
            ),
            (
                .paneFocusRight,
                ShellActionShortcut(key: "rightArrow", modifiers: [.command, .control], context: .shell)
            ),
            (.paneClose, ShellActionShortcut(key: "w", modifiers: [.command, .shift], context: .shell)),
            (.findOpen, ShellActionShortcut(key: "f", modifiers: [.command], context: .shell)),
            (
                .spaceSelectPrevious,
                ShellActionShortcut(key: "leftArrow", modifiers: [.command, .option], context: .shell)
            ),
            (
                .spaceSelectNext,
                ShellActionShortcut(key: "rightArrow", modifiers: [.command, .option], context: .shell)
            ),
        ]

        for (actionID, shortcut) in expectedShortcuts {
            expect(
                registry.defaultShortcut(for: actionID) == shortcut,
                "\(actionID.rawValue) must keep its expected default shortcut"
            )
        }

        expect(
            registry.defaultShortcut(for: .spaceSelectByIndex, target: .spaceIndex(1))
                == ShellActionShortcut(key: "2", modifiers: [.command, .option], context: .shell),
            "space numeric shortcuts must be derived dynamically from the target index"
        )
        expect(
            registry.defaultShortcut(for: .tabPin) == nil,
            "pin tab must not receive a shortcut before tab organization owns that action"
        )
        expect(
            registry.defaultShortcut(for: .tabMoveToSpace) == nil,
            "move tab to space must stay action-only in this phase"
        )
    }

    private static func verifiesKeyboardActionLookup() throws {
        let registry = ShellActionRegistry.standard

        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "t", modifiers: [.command], context: .shell)
            ) == ShellKeyboardAction(id: .newTerminalTab, target: .currentSelection),
            "command-t must resolve to new-terminal-tab through the registry"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "]", modifiers: [.command, .shift], context: .shell)
            ) == ShellKeyboardAction(id: .tabSelectNext, target: .currentSelection),
            "command-shift-] must resolve to next-tab through the registry"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "2", modifiers: [.command, .option], context: .shell)
            ) == ShellKeyboardAction(id: .spaceSelectByIndex, target: .spaceIndex(1)),
            "command-option-2 must resolve to dynamic second-space selection"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "return", modifiers: [.command, .shift], context: .shell)
            ) == ShellKeyboardAction(id: .paneZoomToggle, target: .currentSelection),
            "command-shift-return must resolve to pane zoom toggle"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(
                    key: "leftArrow",
                    modifiers: [.command, .control, .shift],
                    context: .shell
                )
            ) == ShellKeyboardAction(id: .paneMoveLeft, target: .currentSelection),
            "command-control-shift-left must resolve to pane movement"
        )
    }

    private static func verifiesShortcutConflictsAreRejected() throws {
        let duplicateShortcut = ShellActionShortcut(
            key: "t",
            modifiers: [.command],
            context: .shell
        )
        let first = ShellActionDescriptor(
            id: .newTerminalTab,
            title: "New Terminal Tab",
            targetKind: .currentSelection,
            defaultShortcut: duplicateShortcut,
            effect: .workspaceCommand(.newTerminalTab)
        )
        let second = ShellActionDescriptor(
            id: .tabClose,
            title: "Conflicting Close Tab",
            targetKind: .currentSelection,
            defaultShortcut: duplicateShortcut,
            effect: .closeTab(nil)
        )

        do {
            _ = try ShellActionRegistry(actions: [first, second])
            expect(false, "duplicate shortcuts in the same context must be rejected")
        } catch ShellActionRegistryError.duplicateShortcut(let shortcut, let actionIDs) {
            expect(shortcut == duplicateShortcut, "duplicate shortcut error must include the shortcut")
            expect(
                actionIDs == [.newTerminalTab, .tabClose],
                "duplicate shortcut error must name both conflicting actions"
            )
        }
    }

    private static func verifiesDynamicSpaceShortcutConflictsAreRejected() throws {
        let conflictingSpaceShortcut = ShellActionShortcut(
            key: "1",
            modifiers: [.command, .option],
            context: .shell
        )
        let custom = ShellActionDescriptor(
            id: .newTerminalTab,
            title: "Conflicting Dynamic Shortcut",
            targetKind: .currentSelection,
            defaultShortcut: conflictingSpaceShortcut,
            effect: .workspaceCommand(.newTerminalTab)
        )
        let dynamicSpaceSelection = ShellActionDescriptor(
            id: .spaceSelectByIndex,
            title: "Select Space",
            targetKind: .space,
            effect: .selectSpaceAt(0)
        )

        do {
            _ = try ShellActionRegistry(actions: [custom, dynamicSpaceSelection])
            expect(false, "dynamic numeric space shortcuts must participate in conflict detection")
        } catch ShellActionRegistryError.duplicateShortcut(let shortcut, let actionIDs) {
            expect(shortcut == conflictingSpaceShortcut, "duplicate shortcut error must include the dynamic shortcut")
            expect(
                Set(actionIDs) == Set([.newTerminalTab, .spaceSelectByIndex]),
                "dynamic space shortcut conflicts must name both action ids"
            )
        }
    }

    private static func verifiesContextTabTargetDoesNotSelectTabFirst() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTab(
            launchTarget: .shell,
            in: "space_main",
            title: "Second",
            workingDirectory: "/tmp"
        ).state
        let selectedTabBefore = state.focusedTabID
        guard let contextTab = state.spaces.first?.tabs.first(where: { $0.tabID != selectedTabBefore }) else {
            throw TestFailure("expected a second tab")
        }

        let resolved = ShellActionRegistry.standard.resolve(
            .tabClose,
            target: .contextTab(contextTab.tabID),
            state: state
        )
        var handledEffects: [ShellActionEffect] = []
        let execution = ShellActionRegistry.standard.execute(
            .tabClose,
            target: .contextTab(contextTab.tabID),
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(resolved.resolvedTarget == .tab(contextTab.tabID), "context menu must preserve clicked tab")
        expect(state.focusedTabID == selectedTabBefore, "resolving context target must not select the tab first")
        expect(execution == .executed, "context tab close must execute when the tab exists")
        expect(
            handledEffects == [.closeTab(contextTab.tabID)],
            "context tab close must route the clicked tab id to the handler"
        )
    }

    private static func verifiesMoveToSpaceRequiresExplicitTarget() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        var handledEffects: [ShellActionEffect] = []

        let result = ShellActionRegistry.standard.execute(
            .tabMoveToSpace,
            target: .currentSelection,
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(
            result == .unavailable(reason: "Move target is required"),
            "move-tab-to-space must require an explicit tab and destination space target"
        )
        expect(handledEffects.isEmpty, "unavailable actions must not execute handlers")
    }

    private static func verifiesUnavailableShortcutActionDoesNotExecuteHandler() throws {
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        var handledEffects: [ShellActionEffect] = []

        expect(
            ShellActionRegistry.standard.defaultShortcut(for: .tabMoveLeft) != nil,
            "disabled move-tab actions must still expose menu shortcut hints"
        )
        let result = ShellActionRegistry.standard.execute(
            .tabMoveLeft,
            target: .currentSelection,
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(
            result == .unavailable(reason: "No adjacent tab in section"),
            "move-tab shortcuts must report a stable unavailable reason at section edges"
        )
        expect(handledEffects.isEmpty, "disabled move-tab shortcuts must not mutate state")
    }

    private static func verifiesMoveTabShortcutRoutesHandler() throws {
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.openingTab(
            launchTarget: .shell,
            in: "space_main",
            title: "Second",
            workingDirectory: "/tmp"
        ).state

        var handledEffects: [ShellActionEffect] = []
        let result = ShellActionRegistry.standard.execute(
            .tabMoveLeft,
            target: .currentSelection,
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(result == .executed, "move-tab-left must execute when an adjacent tab exists")
        expect(
            handledEffects == [.moveTab(state.focusedTabID, offset: -1)],
            "move-tab-left must route the selected tab and offset to the handler"
        )
    }

    private static func verifiesRemovedAlanActionsRemainOutOfRegistry() throws {
        let registry = ShellActionRegistry.standard
        let actionRawIDs = Set(registry.actions.map(\.id.rawValue))
        let titles = registry.actions.map { $0.title.lowercased() }

        expect(!actionRawIDs.contains("shell.tab.new_alan"), "new alan tab action must stay out of registry")
        expect(!actionRawIDs.contains("shell.command_input.open"), "Ask alan command input must stay out of registry")
        expect(!titles.contains { $0.contains("ask alan") || $0.contains("new alan tab") }, "removed alan actions must not have descriptors")
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "p", modifiers: [.command], context: .shell)
            ) == nil,
            "Command-P must not resolve to an Ask alan command input action"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "t", modifiers: [.command, .option], context: .shell)
            ) == nil,
            "Command-Option-T must not resolve to first-party alan tab creation"
        )
        expect(
            registry.action(for: .tabMoveToSpace)?.availability(
                state: ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp"),
                target: .currentSelection
            ) == .unavailable(reason: "Move target is required"),
            "move-tab-to-space must stay explicit and avoid implicit current-space targets"
        )
    }

    private static func verifiesQuickTerminalActionsRouteThroughSharedRegistry() throws {
        let registry = ShellActionRegistry.standard
        let state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")

        expect(
            registry.defaultShortcut(for: .quickTerminalToggle)
                == ShellActionShortcut(key: "space", modifiers: [.option], context: .shell),
            "quick terminal toggle must advertise the draft option-space shortcut"
        )
        expect(
            registry.keyboardAction(
                for: ShellActionShortcut(key: "space", modifiers: [.option], context: .shell)
            ) == ShellKeyboardAction(id: .quickTerminalToggle, target: .currentSelection),
            "option-space must resolve to the shared quick-terminal toggle action"
        )

        let routedEffects: [(ShellActionID, ShellActionEffect)] = [
            (.quickTerminalToggle, .workspaceCommand(.quickTerminalToggle)),
            (.quickTerminalShow, .workspaceCommand(.quickTerminalShow)),
            (.quickTerminalHide, .workspaceCommand(.quickTerminalHide)),
            (.quickTerminalFocus, .workspaceCommand(.quickTerminalFocus)),
            (.quickTerminalClose, .workspaceCommand(.quickTerminalClose)),
        ]

        for (actionID, expectedEffect) in routedEffects {
            var handledEffects: [ShellActionEffect] = []
            let result = registry.execute(actionID, target: .currentSelection, state: state) { effect in
                handledEffects.append(effect)
                return true
            }

            expect(result == .executed, "\(actionID.rawValue) must execute through the registry")
            expect(handledEffects == [expectedEffect], "\(actionID.rawValue) must route the shared command effect")
        }
    }

    private static func verifiesQuickTerminalPromoteRequiresExplicitDestination() throws {
        let registry = ShellActionRegistry.standard
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = state.creatingTerminalSpace(title: "Second", workingDirectory: "/tmp").state
        state = state.showingQuickTerminal(workingDirectory: "/tmp").state

        var handledEffects: [ShellActionEffect] = []
        let missingTarget = registry.execute(
            .quickTerminalPromote,
            target: .currentSelection,
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }
        let explicitTarget = registry.execute(
            .quickTerminalPromote,
            target: .contextSpace("space_2"),
            state: state
        ) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(
            missingTarget == .unavailable(reason: "Quick terminal destination is required"),
            "quick terminal promotion must require an explicit destination"
        )
        expect(explicitTarget == .executed, "quick terminal promotion must execute for an explicit space")
        expect(
            handledEffects == [.promoteQuickTerminal(spaceID: "space_2")],
            "quick terminal promotion must route the selected destination to the handler"
        )
    }

    private static func verifiesPaneZoomRoutesThroughSharedRegistry() throws {
        let registry = ShellActionRegistry.standard
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        let unavailable = registry.execute(.paneZoomToggle, target: .currentSelection, state: state) { _ in
            true
        }
        expect(
            unavailable == .unavailable(reason: "Pane zoom requires a split tab"),
            "pane zoom must require a split tab"
        )

        state = try state.splittingPane("pane_1", placement: .right).state
        var handledEffects: [ShellActionEffect] = []
        let result = registry.execute(.paneZoomToggle, target: .currentSelection, state: state) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(result == .executed, "pane zoom must execute when a split pane is focused")
        expect(
            handledEffects == [.workspaceCommand(.togglePaneZoom)],
            "pane zoom must route through the shared workspace command path"
        )
    }

    private static func verifiesPaneMovementRoutesThroughSharedRegistry() throws {
        let registry = ShellActionRegistry.standard
        var state = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        state = try state.splittingPane("pane_1", placement: .right).state

        let unavailable = registry.execute(.paneMoveRight, target: .currentSelection, state: state) { _ in
            true
        }
        expect(
            unavailable == .unavailable(reason: "No adjacent pane in that direction"),
            "pane movement must require an adjacent in-tab destination"
        )

        var handledEffects: [ShellActionEffect] = []
        let result = registry.execute(.paneMoveLeft, target: .currentSelection, state: state) { effect in
            handledEffects.append(effect)
            return true
        }

        expect(result == .executed, "pane movement must execute when an adjacent pane exists")
        expect(
            handledEffects == [.movePaneInTab("pane_2", placement: .left)],
            "pane movement must route the selected pane and placement to the shared handler"
        )
    }
}

private enum ShellActionRegistryFixtureExporter {
    static func exportIfRequested() throws {
        guard let rootPath = ProcessInfo.processInfo.environment["ALAN_SHELL_ACTION_FIXTURE_DIR"],
              !rootPath.isEmpty
        else {
            return
        }

        let rootURL = URL(fileURLWithPath: rootPath)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        for fixture in try fixtures() {
            let fixtureURL = rootURL
                .appendingPathComponent(fixture.id)
                .appendingPathExtension("json")
            try FileManager.default.createDirectory(
                at: fixtureURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try encoder.encode(fixture).write(to: fixtureURL, options: .atomic)
        }
        print("Shell action registry fixtures exported to \(rootPath).")
    }

    private static func fixtures() throws -> [ShellCoreFixtureCase] {
        let registry = ShellActionRegistry.standard
        let shortcutRequests = [
            ActionShortcutRequest(id: .newTerminalTab, target: .currentSelection),
            ActionShortcutRequest(id: .tabClose, target: .currentSelection),
            ActionShortcutRequest(id: .paneZoomToggle, target: .currentSelection),
            ActionShortcutRequest(id: .quickTerminalToggle, target: .currentSelection),
            ActionShortcutRequest(id: .spaceSelectByIndex, target: .spaceIndex(1)),
        ]
        let shortcutResults = shortcutRequests.map { request in
            ActionShortcutResult(
                id: request.id,
                shortcut: registry.defaultShortcut(for: request.id, target: request.target.swiftTarget)
            )
        }

        var contextTabState = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        contextTabState = try contextTabState.openingTab(
            launchTarget: .shell,
            in: "space_main",
            title: "Second",
            workingDirectory: "/tmp"
        ).state
        let contextTabExecution = execute(
            .tabClose,
            target: .contextTab("tab_other"),
            state: contextTabState
        )

        var quickTerminalState = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        quickTerminalState = quickTerminalState.creatingTerminalSpace(
            title: "Second",
            workingDirectory: "/tmp"
        ).state
        quickTerminalState = quickTerminalState.showingQuickTerminal(workingDirectory: "/tmp").state
        let quickTerminalExecution = execute(
            .quickTerminalPromote,
            target: .contextSpace("space_2"),
            state: quickTerminalState
        )

        var splitState = ShellStateSnapshot.bootstrapDefault(workingDirectory: "/tmp")
        splitState = try splitState.splittingPane("pane_1", placement: .right).state
        let paneMoveExecution = execute(
            .paneMoveLeft,
            target: .currentSelection,
            state: splitState
        )

        return [
            ShellCoreFixtureCase(
                id: "actions/standard-shortcuts",
                kind: "action_registry",
                description: "Standard action shortcuts preserve stable keyboard metadata.",
                input: EmptyFixtureInput(),
                operation: StandardShortcutsOperation(requests: shortcutRequests),
                expected: StandardShortcutsExpectation(shortcuts: shortcutResults)
            ),
            ShellCoreFixtureCase(
                id: "actions/keyboard-pane-zoom",
                kind: "action_registry",
                description: "Command-shift-return resolves to the shared pane zoom action.",
                input: EmptyFixtureInput(),
                operation: KeyboardActionOperation(
                    shortcut: ActionFixtureShortcut(
                        key: "return",
                        modifiers: [.command, .shift],
                        context: .shell
                    )
                ),
                expected: KeyboardActionExpectation(
                    keyboardAction: ActionFixtureKeyboardAction(
                        id: .paneZoomToggle,
                        target: .currentSelection
                    )
                )
            ),
            ShellCoreFixtureCase(
                id: "actions/context-tab-close",
                kind: "action_registry",
                description: "Context tab close preserves the clicked tab target.",
                input: ActionFixtureWorkspaceState(contextTabState),
                operation: ExecuteActionOperation(id: .tabClose, target: .contextTab("tab_other")),
                expected: ExecuteActionExpectation(result: contextTabExecution)
            ),
            ShellCoreFixtureCase(
                id: "actions/quick-terminal-promote",
                kind: "action_registry",
                description: "Quick terminal promote requires and routes an explicit destination Space.",
                input: ActionFixtureWorkspaceState(quickTerminalState),
                operation: ExecuteActionOperation(
                    id: .quickTerminalPromote,
                    target: .contextSpace("space_2")
                ),
                expected: ExecuteActionExpectation(result: quickTerminalExecution)
            ),
            ShellCoreFixtureCase(
                id: "actions/pane-move-left",
                kind: "action_registry",
                description: "Pane movement routes the focused split pane and placement.",
                input: ActionFixtureWorkspaceState(splitState),
                operation: ExecuteActionOperation(id: .paneMoveLeft, target: .currentSelection),
                expected: ExecuteActionExpectation(result: paneMoveExecution)
            ),
        ]
    }

    private static func execute(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ActionFixtureExecutionResult {
        var handledEffect: ShellActionEffect?
        let result = ShellActionRegistry.standard.execute(id, target: target, state: state) { effect in
            handledEffect = effect
            return true
        }
        switch result {
        case .executed:
            return .executed(ActionFixtureEffect(handledEffect))
        case .failed(let reason):
            return .failed(reason)
        case .unavailable(let reason):
            return .unavailable(reason)
        }
    }
}

private struct ShellCoreFixtureCase: Encodable {
    let id: String
    let kind: String
    let source = "swift"
    let description: String
    let input: AnyEncodable
    let operation: AnyEncodable
    let expected: AnyEncodable

    init<Input: Encodable, Operation: Encodable, Expected: Encodable>(
        id: String,
        kind: String,
        description: String,
        input: Input,
        operation: Operation,
        expected: Expected
    ) {
        self.id = id
        self.kind = kind
        self.description = description
        self.input = AnyEncodable(input)
        self.operation = AnyEncodable(operation)
        self.expected = AnyEncodable(expected)
    }
}

private struct AnyEncodable: Encodable {
    private let encodeValue: (Encoder) throws -> Void

    init<Value: Encodable>(_ value: Value) {
        encodeValue = value.encode(to:)
    }

    func encode(to encoder: Encoder) throws {
        try encodeValue(encoder)
    }
}

private struct EmptyFixtureInput: Encodable {}

private struct StandardShortcutsOperation: Encodable {
    let type = "standard_shortcuts"
    let requests: [ActionShortcutRequest]
}

private struct KeyboardActionOperation: Encodable {
    let type = "keyboard_action"
    let shortcut: ActionFixtureShortcut
}

private struct ExecuteActionOperation: Encodable {
    let type = "execute"
    let id: ShellActionID
    let target: ActionFixtureTarget

    init(id: ShellActionID, target: ActionFixtureTarget) {
        self.id = id
        self.target = target
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case id
        case target
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(type, forKey: .type)
        try container.encode(id.rawValue, forKey: .id)
        try container.encode(target, forKey: .target)
    }
}

private struct StandardShortcutsExpectation: Encodable {
    let status = "ok"
    let shortcuts: [ActionShortcutResult]
}

private struct KeyboardActionExpectation: Encodable {
    let status = "ok"
    let keyboardAction: ActionFixtureKeyboardAction?

    private enum CodingKeys: String, CodingKey {
        case status
        case keyboardAction = "keyboard_action"
    }
}

private struct ExecuteActionExpectation: Encodable {
    let status = "ok"
    let result: ActionFixtureExecutionResult
}

private struct ActionShortcutRequest: Encodable {
    let id: ShellActionID
    let target: ActionFixtureTarget

    private enum CodingKeys: String, CodingKey {
        case id
        case target
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id.rawValue, forKey: .id)
        try container.encode(target, forKey: .target)
    }
}

private struct ActionShortcutResult: Encodable {
    let id: ShellActionID
    let shortcut: ActionFixtureShortcut?

    init(id: ShellActionID, shortcut: ShellActionShortcut?) {
        self.id = id
        self.shortcut = shortcut.map(ActionFixtureShortcut.init)
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case shortcut
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id.rawValue, forKey: .id)
        try container.encode(shortcut, forKey: .shortcut)
    }
}

private struct ActionFixtureKeyboardAction: Encodable {
    let id: ShellActionID
    let target: ActionFixtureTarget

    private enum CodingKeys: String, CodingKey {
        case id
        case target
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id.rawValue, forKey: .id)
        try container.encode(target, forKey: .target)
    }
}

private enum ActionFixtureTarget: Encodable {
    case currentSelection
    case contextTab(String)
    case contextPane(String)
    case contextSpace(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)

    var swiftTarget: ShellActionTarget {
        switch self {
        case .currentSelection:
            return .currentSelection
        case .contextTab(let tabID):
            return .contextTab(tabID)
        case .contextPane(let paneID):
            return .contextPane(paneID)
        case .contextSpace(let spaceID):
            return .contextSpace(spaceID)
        case .spaceIndex(let index):
            return .spaceIndex(index)
        case .tabToSpace(let tabID, let spaceID):
            return .tabToSpace(tabID: tabID, spaceID: spaceID)
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case paneID = "pane_id"
        case spaceID = "space_id"
        case index
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .currentSelection:
            try container.encode("current_selection", forKey: .type)
        case .contextTab(let tabID):
            try container.encode("context_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .contextPane(let paneID):
            try container.encode("context_pane", forKey: .type)
            try container.encode(paneID, forKey: .paneID)
        case .contextSpace(let spaceID):
            try container.encode("context_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
        case .spaceIndex(let index):
            try container.encode("space_index", forKey: .type)
            try container.encode(index, forKey: .index)
        case .tabToSpace(let tabID, let spaceID):
            try container.encode("tab_to_space", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(spaceID, forKey: .spaceID)
        }
    }
}

private struct ActionFixtureShortcut: Encodable {
    let key: String
    let modifiers: [ShellActionModifier]
    let context: ShellActionShortcutContext

    init(key: String, modifiers: [ShellActionModifier], context: ShellActionShortcutContext) {
        self.key = key
        self.modifiers = modifiers.sorted()
        self.context = context
    }

    init(_ shortcut: ShellActionShortcut) {
        self.init(
            key: shortcut.key,
            modifiers: Array(shortcut.modifiers),
            context: shortcut.context
        )
    }

    private enum CodingKeys: String, CodingKey {
        case key
        case modifiers
        case context
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(key, forKey: .key)
        try container.encode(modifiers.map(\.rawValue), forKey: .modifiers)
        try container.encode(context.rawValue, forKey: .context)
    }
}

private enum ActionFixtureExecutionResult: Encodable {
    case executed(ActionFixtureEffect)
    case failed(String)
    case unavailable(String)

    private enum CodingKeys: String, CodingKey {
        case status
        case effect
        case reason
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .executed(let effect):
            try container.encode("executed", forKey: .status)
            try container.encode(effect, forKey: .effect)
        case .failed(let reason):
            try container.encode("failed", forKey: .status)
            try container.encode(reason, forKey: .reason)
        case .unavailable(let reason):
            try container.encode("unavailable", forKey: .status)
            try container.encode(reason, forKey: .reason)
        }
    }
}

private enum ActionFixtureEffect: Encodable {
    case workspaceCommand(ShellWorkspaceCommand)
    case closeTab(String?)
    case promoteQuickTerminal(String?)
    case movePaneInTab(String?, ShellPaneSplitDirection)

    init(_ effect: ShellActionEffect?) {
        switch effect {
        case .workspaceCommand(let command):
            self = .workspaceCommand(command)
        case .closeTab(let tabID):
            self = .closeTab(tabID)
        case .promoteQuickTerminal(let spaceID):
            self = .promoteQuickTerminal(spaceID)
        case .movePaneInTab(let paneID, let placement):
            self = .movePaneInTab(paneID, placement)
        default:
            self = .workspaceCommand(.newTerminalTab)
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type
        case command
        case tabID = "tab_id"
        case spaceID = "space_id"
        case paneID = "pane_id"
        case placement
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .workspaceCommand(let command):
            try container.encode("workspace_command", forKey: .type)
            try container.encode(command.rawValue, forKey: .command)
        case .closeTab(let tabID):
            try container.encode("close_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .promoteQuickTerminal(let spaceID):
            try container.encode("promote_quick_terminal", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
        case .movePaneInTab(let paneID, let placement):
            try container.encode("move_pane_in_tab", forKey: .type)
            try container.encode(paneID, forKey: .paneID)
            try container.encode(placement, forKey: .placement)
        }
    }
}

private struct ActionFixtureWorkspaceState: Encodable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [ActionFixtureSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [ActionFixtureContentInstance]
    let quickTerminal: ActionFixtureQuickTerminalState?

    init(_ state: ShellStateSnapshot) {
        let contentState = state.contentStateProjection()
        contractVersion = contentState.contractVersion
        windowID = contentState.windowID
        focusedSpaceID = state.focusedSpaceID
        focusedTabID = state.focusedTabID
        focusedPaneID = state.focusedPaneID
        spaces = contentState.spaces.map(ActionFixtureSpace.init)
        paneSlots = contentState.paneSlots
        contents = contentState.contents.map(ActionFixtureContentInstance.init)
        quickTerminal = ActionFixtureQuickTerminalState(state)
    }

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
        case quickTerminal = "quick_terminal"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(contractVersion, forKey: .contractVersion)
        try container.encode(windowID, forKey: .windowID)
        try container.encode(focusedSpaceID, forKey: .focusedSpaceID)
        try container.encode(focusedTabID, forKey: .focusedTabID)
        try container.encode(focusedPaneID, forKey: .focusedPaneID)
        try container.encode(spaces, forKey: .spaces)
        try container.encode(paneSlots, forKey: .paneSlots)
        try container.encode(contents, forKey: .contents)
        try container.encodeIfPresent(quickTerminal, forKey: .quickTerminal)
    }
}

private struct ActionFixtureSpace: Encodable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ActionFixtureTab]
    let selectedTabID: String?
    let terminalProfileID: String?
    let presentationIcon: String?

    init(_ space: ShellContentSpace) {
        spaceID = space.spaceID
        title = space.title
        attention = space.attention
        tabs = space.tabs.map(ActionFixtureTab.init)
        selectedTabID = space.selectedTabID
        terminalProfileID = space.terminalProfileID
        presentationIcon = space.presentationIconSystemName
    }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case selectedTabID = "selected_tab_id"
        case terminalProfileID = "terminal_profile_id"
        case presentationIcon = "presentation_icon"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(spaceID, forKey: .spaceID)
        try container.encode(title, forKey: .title)
        try container.encode(attention, forKey: .attention)
        try container.encode(tabs, forKey: .tabs)
        try container.encode(selectedTabID, forKey: .selectedTabID)
        try container.encode(terminalProfileID, forKey: .terminalProfileID)
        try container.encode(presentationIcon, forKey: .presentationIcon)
    }
}

private struct ActionFixtureTab: Encodable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ActionFixturePaneTreeNode
    let isPinned: Bool
    let isTitleUserLocked: Bool

    init(_ tab: ShellContentTab) {
        tabID = tab.tabID
        kind = tab.kind
        title = tab.title
        paneTree = ActionFixturePaneTreeNode(tab.paneTree)
        isPinned = tab.isPinned
        isTitleUserLocked = tab.isTitleUserLocked
    }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
    }
}

private struct ActionFixturePaneTreeNode: Encodable {
    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneID: String?
    let children: [ActionFixturePaneTreeNode]?

    init(_ node: ShellPaneSlotTreeNode) {
        nodeID = node.nodeID
        kind = node.kind
        direction = node.direction
        ratio = node.ratio
        paneID = node.paneSlotID
        children = node.children?.map(ActionFixturePaneTreeNode.init)
    }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneID = "pane_id"
        case children
    }
}

private struct ActionFixtureContentInstance: Encodable {
    let contentID: String
    let kind: ShellContentKind
    let title: String
    let iconName: String?
    let capabilities: [ShellContentCapability]
    let terminalMetadata: ActionFixtureTerminalRuntimeMetadata?
    let lifecycle: ShellContentLifecycleState

    init(_ content: ShellContentInstance) {
        contentID = content.contentID
        kind = content.kind
        title = content.title
        iconName = content.iconName
        capabilities = content.capabilities
        terminalMetadata = content.payload.terminal.map(ActionFixtureTerminalRuntimeMetadata.init)
        lifecycle = content.lifecycle
    }

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case iconName = "icon_name"
        case capabilities
        case terminalMetadata = "terminal_metadata"
        case lifecycle
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(contentID, forKey: .contentID)
        try container.encode(kind, forKey: .kind)
        try container.encode(title, forKey: .title)
        try container.encode(iconName, forKey: .iconName)
        try container.encode(capabilities, forKey: .capabilities)
        try container.encode(terminalMetadata, forKey: .terminalMetadata)
        try container.encode(lifecycle, forKey: .lifecycle)
    }
}

private struct ActionFixtureTerminalRuntimeMetadata: Encodable {
    let title: String?
    let cwd: String?
    let activeTaskState: ShellTabActiveTaskState = .inactive
    let activity: TerminalActivitySnapshot? = nil

    init(_ payload: ShellTerminalContentPayload) {
        title = payload.title
        cwd = payload.cwd
    }

    init(title: String?, cwd: String?) {
        self.title = title
        self.cwd = cwd
    }

    private enum CodingKeys: String, CodingKey {
        case title
        case cwd
        case activeTaskState = "active_task_state"
        case activity
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(title, forKey: .title)
        try container.encode(cwd, forKey: .cwd)
        try container.encode(activeTaskState, forKey: .activeTaskState)
        try container.encode(activity, forKey: .activity)
    }
}

private struct ActionFixtureQuickTerminalState: Encodable {
    let paneID: String
    let presentation: ShellQuickTerminalPresentation
    let lastWorkingDirectory: String?
    let contentID: String
    let terminalMetadata: ActionFixtureTerminalRuntimeMetadata?
    let attention: ShellAttentionState

    init?(_ state: ShellStateSnapshot) {
        guard let slot = state.quickTerminal,
              let pane = state.pane(paneID: slot.paneID)
        else {
            return nil
        }

        let contentID = ShellContentInstance.terminalContentID(forPaneID: slot.paneID)
        let content = state.contents?.first { $0.contentID == contentID }
        paneID = slot.paneID
        presentation = slot.presentation
        lastWorkingDirectory = slot.lastWorkingDirectory
        self.contentID = contentID
        terminalMetadata = content?.payload.terminal.map(ActionFixtureTerminalRuntimeMetadata.init)
            ?? ActionFixtureTerminalRuntimeMetadata(title: pane.viewport?.title, cwd: pane.cwd)
        attention = pane.attention
    }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
        case contentID = "content_id"
        case terminalMetadata = "terminal_metadata"
        case attention
    }
}

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    if !condition() {
        fatalError(message)
    }
}

private struct TestFailure: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
