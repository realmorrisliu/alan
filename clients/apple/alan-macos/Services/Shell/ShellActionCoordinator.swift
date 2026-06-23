import Foundation

@MainActor
struct ShellActionEffectHandlers {
    let selectedTabID: () -> String?
    let selectedPaneID: () -> String?
    let performWorkspaceCommand: (ShellWorkspaceCommand) -> Bool
    let openTab: (ShellLaunchTarget, String?) -> Bool
    let requestCloseTab: (String) -> Bool
    let duplicateTab: (String) -> Bool
    let openTabInSplitView: (String) -> Bool
    let requestClosePane: (String) -> Bool
    let selectAdjacentTab: (Int) -> Bool
    let selectAdjacentSpace: (Int) -> Bool
    let selectSpaceAt: (Int) -> Bool
    let pinTab: (String?) -> Bool
    let unpinTab: (String?) -> Bool
    let updatePinnedTab: (String?) -> Bool
    let moveTab: (String?, Int) -> Bool
    let moveTabToSpace: (String, String) -> Bool
    let movePaneWithinTab: (String, ShellPaneSplitDirection) -> Bool
    let clearTerminal: (String?) -> Bool

    func perform(_ effect: ShellActionEffect) -> Bool {
        switch effect {
        case .workspaceCommand(let command):
            return performWorkspaceCommand(command)
        case .openTab(let launchTarget, let spaceID):
            return openTab(launchTarget, spaceID)
        case .closeTab(let tabID):
            guard let tabID = tabID ?? selectedTabID() else { return false }
            return requestCloseTab(tabID)
        case .renameTab:
            return false
        case .duplicateTab(let tabID):
            guard let tabID else { return false }
            return duplicateTab(tabID)
        case .openTabInSplitView(let tabID):
            guard let tabID else { return false }
            return openTabInSplitView(tabID)
        case .closePane(let paneID):
            guard let paneID = paneID ?? selectedPaneID() else { return false }
            return requestClosePane(paneID)
        case .selectAdjacentTab(let offset):
            return selectAdjacentTab(offset)
        case .selectAdjacentSpace(let offset):
            return selectAdjacentSpace(offset)
        case .selectSpaceAt(let index):
            return selectSpaceAt(index)
        case .pinTab(let tabID):
            return pinTab(tabID)
        case .unpinTab(let tabID):
            return unpinTab(tabID)
        case .updatePinnedTab(let tabID):
            return updatePinnedTab(tabID)
        case .moveTab(let tabID, let offset):
            return moveTab(tabID, offset)
        case .moveTabToSpace(let tabID, let spaceID):
            guard let tabID, let spaceID else { return false }
            return moveTabToSpace(tabID, spaceID)
        case .movePaneInTab(let paneID, let placement):
            guard let paneID else { return false }
            return movePaneWithinTab(paneID, placement)
        case .terminalClear(let paneID):
            return clearTerminal(paneID)
        case .disabledPlaceholder:
            return false
        }
    }
}

@MainActor
struct ShellActionCoordinator {
    func title(_ id: ShellActionID) -> String {
        (try? ShellCoreFFIAdapter.shared.actionTitle(id)) ?? id.rawValue
    }

    func availability(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ShellActionAvailability {
        do {
            return try ShellCoreFFIAdapter.shared.actionAvailability(id, target: target, state: state)
        } catch {
            return .unavailable(reason: "shell-core action availability unavailable")
        }
    }

    func shortcut(
        _ id: ShellActionID,
        target: ShellActionTarget
    ) -> ShellActionShortcut? {
        try? ShellCoreFFIAdapter.shared.defaultActionShortcut(id, target: target)
    }

    func keyboardAction(for shortcut: ShellActionShortcut) -> ShellKeyboardAction? {
        try? ShellCoreFFIAdapter.shared.keyboardAction(shortcut)
    }

    func perform(
        _ id: ShellActionID,
        target: ShellActionTarget,
        source: ShellTerminalCommandSource,
        state: ShellStateSnapshot,
        isModalFlowActive: Bool,
        openSearch: (ShellTerminalCommandSource, ShellActionTarget) -> Bool,
        effectHandlers: ShellActionEffectHandlers
    ) -> ShellActionExecutionResult {
        // The Space creation form is a modal flow over a display-only draft.
        // Suppress shell actions so they cannot mutate the hidden underlying
        // Space while it is open.
        if isModalFlowActive {
            return .failed(reason: "Space creation in progress")
        }

        if id == .findOpen {
            return openSearch(source, target)
                ? .executed
                : .failed(reason: "Terminal search target is unavailable")
        }

        do {
            return try ShellCoreFFIAdapter.shared.executeAction(
                id,
                target: target,
                state: state
            ) { effect in
                effectHandlers.perform(effect)
            }
        } catch {
            return .failed(reason: "shell-core action dispatch failed: \(error)")
        }
    }
}

enum ShellActionAvailabilityResolver {
    static func availability(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ShellActionAvailability {
        switch id {
        case .newTerminalTab, .paneEqualizeSplits:
            return .available
        case .paneSplitLeft,
             .paneSplitRight,
             .paneSplitUp,
             .paneSplitDown,
             .paneFocusLeft,
             .paneFocusRight,
             .paneFocusUp,
             .paneFocusDown,
             .paneClose:
            return focusedPaneAvailability(state: state, target: target)
        case .terminalClear, .findOpen:
            return terminalContentAvailability(state: state, target: target)
        case .paneZoomToggle:
            return splitPaneAvailability(state: state, target: target)
        case .paneMoveLeft:
            return paneMovementAvailability(.left, state: state, target: target)
        case .paneMoveRight:
            return paneMovementAvailability(.right, state: state, target: target)
        case .paneMoveUp:
            return paneMovementAvailability(.up, state: state, target: target)
        case .paneMoveDown:
            return paneMovementAvailability(.down, state: state, target: target)
        case .tabClose, .tabRename:
            return selectedTabAvailability(state: state, target: target)
        case .tabDuplicate:
            return duplicateTabAvailability(state: state, target: target)
        case .tabOpenInSplitView:
            return openTabInSplitViewAvailability(state: state, target: target)
        case .tabSelectPrevious, .tabSelectNext:
            return multipleTabsAvailability(state: state)
        case .spaceSelectPrevious, .spaceSelectNext:
            return multipleSpacesAvailability(state: state)
        case .spaceSelectByIndex:
            return indexedSpaceAvailability(state: state, target: target)
        case .tabPin:
            return pinTabAvailability(state: state, target: target)
        case .tabUnpin, .tabUpdatePin:
            return unpinTabAvailability(state: state, target: target)
        case .tabMoveLeft:
            return moveTabAvailability(offset: -1, state: state, target: target)
        case .tabMoveRight:
            return moveTabAvailability(offset: 1, state: state, target: target)
        case .tabMoveToSpace:
            return moveTabToSpaceAvailability(state: state, target: target)
        }
    }

    private static func focusedPaneAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        switch target {
        case .contextPane(let paneID):
            return state.paneExists(paneID)
                ? .available
                : .unavailable(reason: "Pane is not available")
        default:
            return state.focusedPaneID != nil
                ? .available
                : .unavailable(reason: "No focused pane")
        }
    }

    private static func terminalContentAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let paneID = targetPaneID(state: state, target: target) else {
            return .unavailable(reason: "No focused pane")
        }
        guard state.paneExists(paneID) else {
            return .unavailable(reason: "Pane is not available")
        }
        guard terminalContentIDIfAvailable(state: state, paneID: paneID) != nil else {
            return .unavailable(reason: "Focused content is not a terminal")
        }
        return .available
    }

    private static func splitPaneAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let paneID = targetPaneID(state: state, target: target),
              let tab = state.tab(containingPaneID: paneID)
        else {
            return .unavailable(reason: "No focused pane")
        }
        return tab.paneTree.paneIDs.count > 1
            ? .available
            : .unavailable(reason: "Pane zoom requires a split tab")
    }

    private static func paneMovementAvailability(
        _ placement: ShellPaneSplitDirection,
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let paneID = targetPaneID(state: state, target: target),
              let tab = state.tab(containingPaneID: paneID)
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

    private static func selectedTabAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        switch target {
        case .contextTab(let tabID):
            return state.tab(tabID: tabID) != nil
                ? .available
                : .unavailable(reason: "Tab is not available")
        default:
            return state.focusedTabID != nil
                ? .available
                : .unavailable(reason: "No selected tab")
        }
    }

    private static func duplicateTabAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let tab = targetedTab(state: state, target: target) else {
            return .unavailable(reason: "Tab is not available")
        }
        guard tab.paneTree.paneIDs.first.flatMap({
            terminalContentIDIfAvailable(state: state, paneID: $0)
        }) != nil else {
            return .unavailable(reason: "Tab is not a terminal")
        }
        return .available
    }

    private static func openTabInSplitViewAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let tab = targetedTab(state: state, target: target) else {
            return .unavailable(reason: "Tab is not available")
        }
        let paneID: String?
        if let focusedPaneID = state.focusedPaneID,
           tab.paneTree.contains(paneID: focusedPaneID)
        {
            paneID = focusedPaneID
        } else {
            paneID = tab.paneTree.paneIDs.first
        }
        guard let paneID,
              terminalContentIDIfAvailable(state: state, paneID: paneID) != nil
        else {
            return .unavailable(reason: "Tab cannot be split")
        }
        return .available
    }

    private static func multipleTabsAvailability(state: ShellStateSnapshot) -> ShellActionAvailability {
        guard let spaceID = state.focusedSpaceID,
              let space = state.space(spaceID: spaceID)
        else {
            return .unavailable(reason: "No adjacent tab")
        }
        guard space.tabs.count > 1 else {
            return .unavailable(reason: "No adjacent tab")
        }
        guard state.focusedTabID != nil else {
            return .unavailable(reason: "No selected tab")
        }
        return .available
    }

    private static func multipleSpacesAvailability(state: ShellStateSnapshot) -> ShellActionAvailability {
        state.spaces.count > 1
            ? .available
            : .unavailable(reason: "No adjacent space")
    }

    private static func indexedSpaceAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard case .spaceIndex(let index) = target else {
            return .unavailable(reason: "Space index is required")
        }
        return index < state.spaces.count
            ? .available
            : .unavailable(reason: "Space is not available")
    }

    private static func pinTabAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let tab = targetedTab(state: state, target: target) else {
            return .unavailable(reason: "Tab is not available")
        }
        return tab.isPinned
            ? .unavailable(reason: "Tab is already pinned")
            : .available
    }

    private static func unpinTabAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let tab = targetedTab(state: state, target: target) else {
            return .unavailable(reason: "Tab is not available")
        }
        return tab.isPinned
            ? .available
            : .unavailable(reason: "Tab is not pinned")
    }

    private static func moveTabAvailability(
        offset: Int,
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard let tab = targetedTab(state: state, target: target),
              let location = state.tabLocation(tabID: tab.tabID),
              let space = state.space(spaceID: location.spaceID)
        else {
            return .unavailable(reason: "Tab is not available")
        }
        let sectionTabs = space.tabs.filter { $0.isPinned == location.isPinned }
        let nextIndex = location.index + offset
        return sectionTabs.indices.contains(nextIndex)
            ? .available
            : .unavailable(reason: "No adjacent tab in section")
    }

    private static func moveTabToSpaceAvailability(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellActionAvailability {
        guard case .tabToSpace(let tabID, let spaceID) = target else {
            return .unavailable(reason: "Move target is required")
        }
        guard let location = state.tabLocation(tabID: tabID) else {
            return .unavailable(reason: "Tab is not available")
        }
        guard state.space(spaceID: spaceID) != nil else {
            return .unavailable(reason: "Space is not available")
        }
        return location.spaceID == spaceID
            ? .unavailable(reason: "Tab is already in that space")
            : .available
    }

    private static func targetPaneID(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> String? {
        switch target {
        case .contextPane(let paneID):
            return paneID
        default:
            return state.focusedPaneID
        }
    }

    private static func targetedTab(
        state: ShellStateSnapshot,
        target: ShellActionTarget
    ) -> ShellTab? {
        switch target {
        case .contextTab(let tabID):
            return state.tab(tabID: tabID)
        default:
            return state.focusedTabID.flatMap { state.tab(tabID: $0) }
        }
    }

    private static func terminalContentIDIfAvailable(
        state: ShellStateSnapshot,
        paneID: String
    ) -> String? {
        guard let pane = state.pane(paneID: paneID) else {
            return nil
        }
        if let mountedContent = state.explicitContentMounted(in: paneID) {
            return mountedContent.kind == .terminal ? mountedContent.contentID : nil
        }
        return state.isTerminalBackedPane(pane) ? pane.terminalContentID : nil
    }
}

private struct ShellActionTabLocation {
    let spaceID: String
    let isPinned: Bool
    let index: Int
}

private extension ShellStateSnapshot {
    func paneExists(_ paneID: String) -> Bool {
        spaces.lazy
            .flatMap(\.tabs)
            .contains { $0.paneTree.contains(paneID: paneID) }
    }

    func tab(containingPaneID paneID: String) -> ShellTab? {
        spaces.lazy
            .flatMap(\.tabs)
            .first { $0.paneTree.contains(paneID: paneID) }
    }

    func tabLocation(tabID: String) -> ShellActionTabLocation? {
        for space in spaces {
            guard let tab = space.tabs.first(where: { $0.tabID == tabID }) else {
                continue
            }
            let sectionTabs = space.tabs.filter { $0.isPinned == tab.isPinned }
            guard let index = sectionTabs.firstIndex(where: { $0.tabID == tabID }) else {
                return nil
            }
            return ShellActionTabLocation(
                spaceID: space.spaceID,
                isPinned: tab.isPinned,
                index: index
            )
        }
        return nil
    }
}
