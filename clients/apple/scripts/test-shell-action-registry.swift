import Foundation

@main
struct ShellActionRegistryTestRunner {
    static func main() throws {
        try ShellActionRegistryTests.run()
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
