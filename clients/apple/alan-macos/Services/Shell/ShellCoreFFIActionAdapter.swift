import Foundation

extension ShellCoreFFIAdapter {
    func executeAction(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot,
        handler: (ShellActionEffect) -> Bool
    ) throws -> ShellActionExecutionResult {
        let result = try coreActionExecutionResult(id, target: target, state: state)
        switch result.status {
        case .executed:
            guard let effect = result.effect?.shellActionEffect else {
                return .failed(reason: "Action effect is unavailable")
            }
            return handler(effect) ? .executed : .failed(reason: "Action handler failed")
        case .failed:
            return .failed(reason: result.reason ?? "Action failed")
        case .unavailable:
            return .unavailable(reason: result.reason ?? "Action is unavailable")
        }
    }

    private func coreActionExecutionResult(
        _ id: ShellActionID,
        target: ShellActionTarget,
        state: ShellStateSnapshot
    ) throws -> ShellCoreActionExecutionResult {
        let response: ShellCoreActionExecuteResponse = try send(
            operation: "actions.execute",
            payload: ShellCoreActionExecutePayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                id: id,
                target: ShellCoreActionTarget(target)
            )
        )
        return response.result
    }

}

private struct ShellCoreActionExecutePayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let id: ShellActionID
    let target: ShellCoreActionTarget
}

private struct ShellCoreActionExecuteResponse: Decodable {
    let result: ShellCoreActionExecutionResult
}

private struct ShellCoreActionExecutionResult: Decodable {
    let status: ShellCoreActionExecutionStatus
    let effect: ShellCoreActionEffect?
    let reason: String?
}

private enum ShellCoreActionExecutionStatus: String, Decodable {
    case executed
    case failed
    case unavailable
}

private enum ShellCoreActionTarget: Codable {
    case currentSelection
    case contextTab(String)
    case contextPane(String)
    case contextSpace(String)
    case spaceIndex(Int)
    case tabToSpace(tabID: String, spaceID: String)
    case unresolved

    private enum CodingKeys: String, CodingKey {
        case type
        case tabID = "tab_id"
        case paneID = "pane_id"
        case spaceID = "space_id"
        case index
    }

    init(_ target: ShellActionTarget) {
        switch target {
        case .currentSelection:
            self = .currentSelection
        case .contextTab(let tabID):
            self = .contextTab(tabID)
        case .contextPane(let paneID):
            self = .contextPane(paneID)
        case .contextSpace(let spaceID):
            self = .contextSpace(spaceID)
        case .spaceIndex(let index):
            self = .spaceIndex(index)
        case .tabToSpace(let tabID, let spaceID):
            self = .tabToSpace(tabID: tabID, spaceID: spaceID)
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "current_selection":
            self = .currentSelection
        case "context_tab":
            self = .contextTab(try container.decode(String.self, forKey: .tabID))
        case "context_pane":
            self = .contextPane(try container.decode(String.self, forKey: .paneID))
        case "context_space":
            self = .contextSpace(try container.decode(String.self, forKey: .spaceID))
        case "space_index":
            self = .spaceIndex(try container.decode(Int.self, forKey: .index))
        case "tab_to_space":
            self = .tabToSpace(
                tabID: try container.decode(String.self, forKey: .tabID),
                spaceID: try container.decode(String.self, forKey: .spaceID)
            )
        default:
            self = .unresolved
        }
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
        case .unresolved:
            try container.encode("unresolved", forKey: .type)
        }
    }

    var shellTarget: ShellActionTarget {
        switch self {
        case .currentSelection, .unresolved:
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
}

private enum ShellCoreActionEffect: Decodable {
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

    private enum CodingKeys: String, CodingKey {
        case type
        case command
        case launchTarget = "launch_target"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case offset
        case index
        case placement
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "workspace_command":
            self = .workspaceCommand(try container.decode(ShellWorkspaceCommand.self, forKey: .command))
        case "open_tab":
            self = .openTab(
                try container.decode(ShellLaunchTarget.self, forKey: .launchTarget),
                spaceID: try container.decodeIfPresent(String.self, forKey: .spaceID)
            )
        case "close_tab":
            self = .closeTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "rename_tab":
            self = .renameTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "duplicate_tab":
            self = .duplicateTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "open_tab_in_split_view":
            self = .openTabInSplitView(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "close_pane":
            self = .closePane(try container.decodeIfPresent(String.self, forKey: .paneID))
        case "select_adjacent_tab":
            self = .selectAdjacentTab(try container.decode(Int.self, forKey: .offset))
        case "select_adjacent_space":
            self = .selectAdjacentSpace(try container.decode(Int.self, forKey: .offset))
        case "select_space_at":
            self = .selectSpaceAt(try container.decode(Int.self, forKey: .index))
        case "pin_tab":
            self = .pinTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "unpin_tab":
            self = .unpinTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "update_pinned_tab":
            self = .updatePinnedTab(try container.decodeIfPresent(String.self, forKey: .tabID))
        case "move_tab":
            self = .moveTab(
                try container.decodeIfPresent(String.self, forKey: .tabID),
                offset: try container.decode(Int.self, forKey: .offset)
            )
        case "move_tab_to_space":
            self = .moveTabToSpace(
                tabID: try container.decodeIfPresent(String.self, forKey: .tabID),
                spaceID: try container.decodeIfPresent(String.self, forKey: .spaceID)
            )
        case "move_pane_in_tab":
            self = .movePaneInTab(
                try container.decodeIfPresent(String.self, forKey: .paneID),
                placement: try container.decode(ShellPaneSplitDirection.self, forKey: .placement)
            )
        case "terminal_clear":
            self = .terminalClear(try container.decodeIfPresent(String.self, forKey: .paneID))
        case "disabled_placeholder":
            self = .disabledPlaceholder
        default:
            self = .disabledPlaceholder
        }
    }

    var shellActionEffect: ShellActionEffect {
        switch self {
        case .workspaceCommand(let command):
            return .workspaceCommand(command)
        case .openTab(let launchTarget, let spaceID):
            return .openTab(launchTarget, spaceID: spaceID)
        case .closeTab(let tabID):
            return .closeTab(tabID)
        case .renameTab(let tabID):
            return .renameTab(tabID)
        case .duplicateTab(let tabID):
            return .duplicateTab(tabID)
        case .openTabInSplitView(let tabID):
            return .openTabInSplitView(tabID)
        case .closePane(let paneID):
            return .closePane(paneID)
        case .selectAdjacentTab(let offset):
            return .selectAdjacentTab(offset)
        case .selectAdjacentSpace(let offset):
            return .selectAdjacentSpace(offset)
        case .selectSpaceAt(let index):
            return .selectSpaceAt(index)
        case .pinTab(let tabID):
            return .pinTab(tabID)
        case .unpinTab(let tabID):
            return .unpinTab(tabID)
        case .updatePinnedTab(let tabID):
            return .updatePinnedTab(tabID)
        case .moveTab(let tabID, let offset):
            return .moveTab(tabID, offset: offset)
        case .moveTabToSpace(let tabID, let spaceID):
            return .moveTabToSpace(tabID: tabID, spaceID: spaceID)
        case .movePaneInTab(let paneID, let placement):
            return .movePaneInTab(paneID, placement: placement)
        case .terminalClear(let paneID):
            return .terminalClear(paneID)
        case .disabledPlaceholder:
            return .disabledPlaceholder
        }
    }
}
