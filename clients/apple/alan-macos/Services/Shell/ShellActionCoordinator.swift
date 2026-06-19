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
        do {
            return try ShellCoreFFIAdapter.shared.actionTitle(for: id) ?? "Unavailable"
        } catch {
            return "Unavailable"
        }
    }

    func availability(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) -> ShellActionAvailability {
        do {
            return try ShellCoreFFIAdapter.shared.actionAvailability(
                id,
                target: target,
                state: state
            )
        } catch {
            return .unavailable(reason: "shell-core action availability failed: \(error)")
        }
    }

    func shortcut(
        _ id: ShellActionID,
        target: ShellActionTarget
    ) -> ShellActionShortcut? {
        do {
            return try ShellCoreFFIAdapter.shared.defaultActionShortcut(for: id, target: target)
        } catch {
            return nil
        }
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
