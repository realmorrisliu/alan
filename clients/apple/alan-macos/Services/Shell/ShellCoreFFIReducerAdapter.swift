import Foundation

extension ShellCoreFFIAdapter {
    func applyReducer(
        state: ShellStateSnapshot,
        operation: ShellCoreReducerOperation
    ) throws -> ShellStateMutationResult {
        let response: ShellCoreReducerApplyResponse = try send(
            operation: "reducer.apply",
            payload: ShellCoreReducerApplyPayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                operation: operation
            )
        )

        guard response.status == "ok",
              let result = response.result
        else {
            let code = response.errorCode ?? "unknown_reducer_error"
            if let mutationError = ShellStateMutationError(rawValue: code) {
                throw mutationError
            }
            throw ShellCoreFFIAdapterError.reducerError(
                code: code,
                message: response.errorMessage ?? "shell-core reducer returned an error"
            )
        }

        return ShellStateMutationResult(
            state: try result.state.materializedShellState()
                .preservingPlatformPaneFields(from: state),
            spaceID: result.focus.spaceID,
            tabID: result.focus.tabID,
            paneID: result.focus.paneSlotID
        )
    }

}

enum ShellCoreReducerOperation: Encodable {
    case focusPane(paneSlotID: String)
    case focusAdjacentPane(direction: ShellSpatialFocusDirection)
    case selectSpace(spaceID: String)
    case selectTab(tabID: String)
    case setTerminalProfile(spaceID: String, terminalProfileID: String?)
    case setPresentationIcon(spaceID: String, presentationIcon: String?)
    case deleteSpace(spaceID: String, defaultWorkingDirectory: String?)
    case createTerminalSpace(
        title: String?,
        tabTitle: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        presentationIcon: String?,
        reservedPaneSlotIDs: [String]
    )
    case openTerminalTab(
        spaceID: String?,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        reservedPaneSlotIDs: [String]
    )
    case openContentTab(
        spaceID: String?,
        kind: ShellContentKind,
        title: String,
        payload: ShellContentPayload,
        reservedPaneSlotIDs: [String]
    )
    case duplicateTab(tabID: String, reservedPaneSlotIDs: [String])
    case moveTab(tabID: String, sectionOffset: Int)
    case moveTabToSpace(tabID: String, targetSpaceID: String)
    case organizeTab(
        tabID: String,
        targetSpaceID: String?,
        section: ShellTabOrganizationSection,
        index: Int?
    )
    case pinTab(tabID: String)
    case unpinTab(tabID: String)
    case renameTab(tabID: String, title: String)
    case closeTab(tabID: String)
    case closePane(paneSlotID: String)
    case clearInactiveTemporaryTabs(spaceID: String, protectedTabIDs: [String])
    case splitPane(
        paneSlotID: String,
        placement: ShellPaneSplitDirection,
        title: String?,
        workingDirectory: String?,
        terminalProfileID: String?,
        reservedPaneSlotIDs: [String]
    )
    case splitContentPane(
        paneSlotID: String,
        placement: ShellPaneSplitDirection,
        kind: ShellContentKind,
        title: String,
        payload: ShellContentPayload,
        reservedPaneSlotIDs: [String]
    )
    case resizeSplit(splitNodeID: String, ratio: Double)
    case equalizeSplits(tabID: String?)
    case movePaneWithinTab(paneSlotID: String, placement: ShellPaneSplitDirection)
    case movePaneToNewTab(paneSlotID: String, title: String?)
    case movePaneToTab(paneSlotID: String, targetTabID: String, direction: ShellSplitDirection)
    case setAttention(paneSlotID: String, attention: ShellAttentionState)
    case updateAgentRendererState(
        paneSlotID: String,
        offsets: AlanAgentStreamOffsets,
        presentation: AlanAgentContentPresentation
    )

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case direction
        case spaceID = "space_id"
        case tabID = "tab_id"
        case targetTabID = "target_tab_id"
        case sectionOffset = "section_offset"
        case targetSpaceID = "target_space_id"
        case section
        case index
        case title
        case tabTitle = "tab_title"
        case workingDirectory = "working_directory"
        case defaultWorkingDirectory = "default_working_directory"
        case terminalProfileID = "terminal_profile_id"
        case presentationIcon = "presentation_icon"
        case reservedPaneSlotIDs = "reserved_pane_slot_ids"
        case protectedTabIDs = "protected_tab_ids"
        case attention
        case splitNodeID = "split_node_id"
        case ratio
        case placement
        case kind
        case payload
        case offsets
        case presentation
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .focusPane(let paneSlotID):
            try container.encode("focus_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
        case .focusAdjacentPane(let direction):
            try container.encode("focus_adjacent_pane", forKey: .type)
            try container.encode(direction, forKey: .direction)
        case .selectSpace(let spaceID):
            try container.encode("select_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
        case .selectTab(let tabID):
            try container.encode("select_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .setTerminalProfile(let spaceID, let terminalProfileID):
            try container.encode("set_terminal_profile", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
        case .setPresentationIcon(let spaceID, let presentationIcon):
            try container.encode("set_presentation_icon", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(presentationIcon, forKey: .presentationIcon)
        case .deleteSpace(let spaceID, let defaultWorkingDirectory):
            try container.encode("delete_space", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(
                defaultWorkingDirectory,
                forKey: .defaultWorkingDirectory
            )
        case .createTerminalSpace(
            let title,
            let tabTitle,
            let workingDirectory,
            let terminalProfileID,
            let presentationIcon,
            let reservedPaneSlotIDs
        ):
            try container.encode("create_terminal_space", forKey: .type)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(tabTitle, forKey: .tabTitle)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encodeIfPresent(presentationIcon, forKey: .presentationIcon)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .openTerminalTab(
            let spaceID,
            let title,
            let workingDirectory,
            let terminalProfileID,
            let reservedPaneSlotIDs
        ):
            try container.encode("open_terminal_tab", forKey: .type)
            try container.encodeIfPresent(spaceID, forKey: .spaceID)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .openContentTab(
            let spaceID,
            let kind,
            let title,
            let payload,
            let reservedPaneSlotIDs
        ):
            try container.encode("open_content_tab", forKey: .type)
            try container.encodeIfPresent(spaceID, forKey: .spaceID)
            try container.encode(kind, forKey: .kind)
            try container.encode(title, forKey: .title)
            try container.encode(payload, forKey: .payload)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .duplicateTab(let tabID, let reservedPaneSlotIDs):
            try container.encode("duplicate_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .moveTab(let tabID, let sectionOffset):
            try container.encode("move_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(sectionOffset, forKey: .sectionOffset)
        case .moveTabToSpace(let tabID, let targetSpaceID):
            try container.encode("move_tab_to_space", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(targetSpaceID, forKey: .targetSpaceID)
        case .organizeTab(let tabID, let targetSpaceID, let section, let index):
            try container.encode("organize_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encodeIfPresent(targetSpaceID, forKey: .targetSpaceID)
            try container.encode(section, forKey: .section)
            try container.encodeIfPresent(index, forKey: .index)
        case .pinTab(let tabID):
            try container.encode("pin_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .unpinTab(let tabID):
            try container.encode("unpin_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .renameTab(let tabID, let title):
            try container.encode("rename_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
            try container.encode(title, forKey: .title)
        case .closeTab(let tabID):
            try container.encode("close_tab", forKey: .type)
            try container.encode(tabID, forKey: .tabID)
        case .closePane(let paneSlotID):
            try container.encode("close_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
        case .clearInactiveTemporaryTabs(let spaceID, let protectedTabIDs):
            try container.encode("clear_inactive_temporary_tabs", forKey: .type)
            try container.encode(spaceID, forKey: .spaceID)
            try container.encode(protectedTabIDs, forKey: .protectedTabIDs)
        case .splitPane(
            let paneSlotID,
            let placement,
            let title,
            let workingDirectory,
            let terminalProfileID,
            let reservedPaneSlotIDs
        ):
            try container.encode("split_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
            try container.encodeIfPresent(title, forKey: .title)
            try container.encodeIfPresent(workingDirectory, forKey: .workingDirectory)
            try container.encodeIfPresent(terminalProfileID, forKey: .terminalProfileID)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .splitContentPane(
            let paneSlotID,
            let placement,
            let kind,
            let title,
            let payload,
            let reservedPaneSlotIDs
        ):
            try container.encode("split_content_pane", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
            try container.encode(kind, forKey: .kind)
            try container.encode(title, forKey: .title)
            try container.encode(payload, forKey: .payload)
            try container.encode(reservedPaneSlotIDs, forKey: .reservedPaneSlotIDs)
        case .resizeSplit(let splitNodeID, let ratio):
            try container.encode("resize_split", forKey: .type)
            try container.encode(splitNodeID, forKey: .splitNodeID)
            try container.encode(ratio, forKey: .ratio)
        case .equalizeSplits(let tabID):
            try container.encode("equalize_splits", forKey: .type)
            try container.encodeIfPresent(tabID, forKey: .tabID)
        case .movePaneWithinTab(let paneSlotID, let placement):
            try container.encode("move_pane_within_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(placement, forKey: .placement)
        case .movePaneToNewTab(let paneSlotID, let title):
            try container.encode("move_pane_to_new_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encodeIfPresent(title, forKey: .title)
        case .movePaneToTab(let paneSlotID, let targetTabID, let direction):
            try container.encode("move_pane_to_tab", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(targetTabID, forKey: .targetTabID)
            try container.encode(direction, forKey: .direction)
        case .setAttention(let paneSlotID, let attention):
            try container.encode("set_attention", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(attention, forKey: .attention)
        case .updateAgentRendererState(let paneSlotID, let offsets, let presentation):
            try container.encode("update_agent_renderer_state", forKey: .type)
            try container.encode(paneSlotID, forKey: .paneSlotID)
            try container.encode(offsets, forKey: .offsets)
            try container.encode(presentation, forKey: .presentation)
        }
    }
}

private struct ShellCoreReducerApplyPayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let operation: ShellCoreReducerOperation
}

private struct ShellCoreReducerApplyResponse: Decodable {
    let status: String
    let result: ShellCoreReducerResult?
    let errorCode: String?
    let errorMessage: String?
    let state: ShellCorePortableWorkspaceState?

    private enum CodingKeys: String, CodingKey {
        case status
        case result
        case errorCode = "error_code"
        case errorMessage = "error_message"
        case state
    }
}

private struct ShellCoreReducerResult: Decodable {
    let state: ShellCorePortableWorkspaceState
    let focus: ShellCoreReducerFocus
}

private struct ShellCoreReducerFocus: Decodable {
    let spaceID: String?
    let tabID: String?
    let paneSlotID: String?

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneSlotID = "pane_slot_id"
    }
}
