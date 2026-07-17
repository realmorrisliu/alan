import Foundation

extension ShellCoreFFIAdapter {
    func handleControlCommand(
        _ command: AlanShellControlCommand,
        state: ShellStateSnapshot
    ) throws -> ShellCoreControlCommandResult {
        let response: ShellCoreControlHandleResponse = try send(
            operation: "control.handle",
            payload: ShellCoreControlHandlePayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                command: command
            )
        )
        return try response.result.shellCommandResult(fallbackState: state)
    }

}

enum ShellCoreControlSideEffect: Equatable {
    case sendText(paneID: String, text: String)
}

struct ShellCoreControlCommandResult {
    let response: AlanShellControlResponse
    let updatedState: ShellStateSnapshot?
    let sideEffect: ShellCoreControlSideEffect?
}

private struct ShellCoreControlHandlePayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let command: AlanShellControlCommand
}

private struct ShellCoreControlHandleResponse: Decodable {
    let result: ShellCoreControlResult
}

private struct ShellCoreControlResult: Decodable {
    let response: ShellCoreControlResponse
    let updatedState: ShellCorePortableWorkspaceState?
    let runtimeIntents: [ShellCoreControlRuntimeIntent]

    private enum CodingKeys: String, CodingKey {
        case response
        case updatedState = "updated_state"
        case runtimeIntents = "runtime_intents"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        response = try container.decode(ShellCoreControlResponse.self, forKey: .response)
        updatedState = try container.decodeIfPresent(
            ShellCorePortableWorkspaceState.self,
            forKey: .updatedState
        )
        runtimeIntents = try container.decodeIfPresent(
            [ShellCoreControlRuntimeIntent].self,
            forKey: .runtimeIntents
        ) ?? []
    }

    func shellCommandResult(fallbackState: ShellStateSnapshot) throws -> ShellCoreControlCommandResult {
        // shell-core returns portable state that does not carry Swift-only pane fields
        // (live cwd/process/activity/viewport/alanBinding). Merge them back from the live
        // fallback state, matching `ShellCoreReducerAdapter.apply`, so adopted updates and
        // local control responses don't drop platform data until the next metadata callback.
        let materializedUpdatedState = try updatedState?.materializedShellState()
            .preservingPlatformPaneFields(from: fallbackState)
        let materializedResponseState = try response.state?.materializedShellState()
            .preservingPlatformPaneFields(from: fallbackState)
        let projectionState = materializedResponseState ?? materializedUpdatedState ?? fallbackState
        return ShellCoreControlCommandResult(
            response: try response.shellResponse(
                fallbackState: fallbackState,
                projectionState: projectionState,
                materializedResponseState: materializedResponseState
            ),
            updatedState: materializedUpdatedState,
            sideEffect: runtimeIntents.compactMap(\.sideEffect).first
        )
    }
}

private struct ShellCoreControlResponse: Decodable {
    let requestID: String
    let contractVersion: String
    let applied: Bool?
    let state: ShellCorePortableWorkspaceState?
    let spaces: [ShellCorePortableSpace]?
    let tabs: [ShellCorePortableTab]?
    let paneSlots: [ShellPaneSlot]?
    let contents: [ShellCorePortableContentInstance]?
    let focusedPaneSlotID: String?
    let spaceID: String?
    let targetSpaceID: String?
    let tabID: String?
    let paneID: String?
    let paneSlotID: String?
    let contentID: String?
    let contentKind: ShellContentKind?
    let splitNodeID: String?
    let ratio: Double?
    let changedSplitIDs: [String]?
    let zoomedPaneID: String?
    let previousFocusedPaneSlotID: String?
    let currentFocusedPaneSlotID: String?
    let placement: ShellPaneSplitDirection?
    let section: ShellTabOrganizationSection?
    let index: Int?
    let errorCode: String?
    let errorMessage: String?

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case contractVersion = "contract_version"
        case applied
        case state
        case spaces
        case tabs
        case paneSlots = "pane_slots"
        case contents
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaceID = "space_id"
        case targetSpaceID = "target_space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case contentKind = "content_kind"
        case splitNodeID = "split_node_id"
        case ratio
        case changedSplitIDs = "changed_split_ids"
        case zoomedPaneID = "zoomed_pane_id"
        case previousFocusedPaneSlotID = "previous_focused_pane_slot_id"
        case currentFocusedPaneSlotID = "current_focused_pane_slot_id"
        case placement
        case section
        case index
        case errorCode = "error_code"
        case errorMessage = "error_message"
    }

    func shellResponse(
        fallbackState: ShellStateSnapshot,
        projectionState: ShellStateSnapshot,
        materializedResponseState: ShellStateSnapshot?
    ) throws -> AlanShellControlResponse {
        let projectedContentState = projectionState.contentStateProjection()
        let contentProjection = projectedContentState.controlPlaneContentProjection(
            paneSlotID: paneSlotID ?? paneID,
            contentID: contentID
        )
        let responseState = materializedResponseState
        return AlanShellControlResponse(
            requestID: requestID,
            contractVersion: contractVersion,
            applied: applied,
            state: responseState,
            spaces: spaces.map { portableSpaces in
                materializedSpaces(
                    portableSpaces,
                    from: projectedContentState
                ) ?? projectionState.spaces
            },
            tabs: tabs?.map(\.shellTab),
            // The legacy `panes` list is dropped by shell-core's portable projection, but the
            // `pane.list` response (pane_slots present, no full state snapshot) still backs the
            // CLI's `alan shell pane list`, which requires `panes`. Re-project it from the
            // response `paneSlots` so the scope matches exactly — an unscoped `pane.list` returns
            // every tab's panes even though shell-core defaults `tab_id` to the focused tab.
            panes: state == nil
                ? paneSlots?.compactMap { projectionState.pane(paneID: $0.paneSlotID) }
                : nil,
            paneSlots: paneSlots,
            contents: contents?.map(\.contentInstance),
            pane: nil,
            items: nil,
            candidates: nil,
            events: nil,
            focusedPaneID: projectionState.focusedPaneID ?? fallbackState.focusedPaneID,
            focusedPaneSlotID: focusedPaneSlotID ?? projectedContentState.focusedPaneSlotID,
            spaceID: spaceID,
            sourceSpaceID: nil,
            targetSpaceID: targetSpaceID,
            tabID: tabID,
            paneID: paneID,
            paneSlotID: paneSlotID ?? contentProjection.paneSlotID,
            contentID: contentID ?? contentProjection.contentID,
            contentKind: contentKind ?? contentProjection.kind,
            contentTitle: contentProjection.title,
            contentCapabilities: contentProjection.capabilities,
            section: section,
            index: index,
            acceptedBytes: nil,
            deliveryCode: nil,
            runtimePhase: nil,
            terminalRenderMetrics: nil,
            latestEventID: nil,
            splitNodeID: splitNodeID,
            ratio: ratio,
            changedSplitIDs: changedSplitIDs,
            affectedPaneIDs: nil,
            zoomedPaneID: zoomedPaneID,
            sourceTabID: nil,
            targetTabID: nil,
            previousFocusedPaneID: previousFocusedPaneSlotID,
            currentFocusedPaneID: currentFocusedPaneSlotID,
            previousFocusedPaneSlotID: previousFocusedPaneSlotID,
            currentFocusedPaneSlotID: currentFocusedPaneSlotID,
            splitDirection: nil,
            spatialDirection: nil,
            placement: placement,
            mountedContentInstanceID: nil,
            diagnosticsEnabled: nil,
            diagnosticsRetainedEventCount: nil,
            diagnosticsStutterMarkerCount: nil,
            diagnosticsBundlePath: nil,
            errorCode: errorCode,
            errorMessage: errorMessage
        )
    }

    private func materializedSpaces(
        _ portableSpaces: [ShellCorePortableSpace],
        from contentState: ShellContentStateSnapshot
    ) -> [ShellSpace]? {
        ShellContentStateSnapshot(
            contractVersion: contentState.contractVersion,
            windowID: contentState.windowID,
            focusedSpaceID: contentState.focusedSpaceID,
            focusedTabID: contentState.focusedTabID,
            focusedPaneSlotID: contentState.focusedPaneSlotID,
            spaces: portableSpaces.map(\.contentSpace),
            paneSlots: contentState.paneSlots,
            contents: contentState.contents
        )
        .materializingShellState()?
        .spaces
    }
}

private enum ShellCoreControlRuntimeIntent: Decodable {
    case sendTerminalText(paneSlotID: String, contentID: String, text: String)
    case sendTerminalKey(paneSlotID: String, contentID: String, key: TerminalRuntimeControlKey)
    case reducer
    case unsupported

    private enum CodingKeys: String, CodingKey {
        case type
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case text
        case key
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "send_terminal_text":
            self = .sendTerminalText(
                paneSlotID: try container.decode(String.self, forKey: .paneSlotID),
                contentID: try container.decode(String.self, forKey: .contentID),
                text: try container.decode(String.self, forKey: .text)
            )
        case "send_terminal_key":
            self = .sendTerminalKey(
                paneSlotID: try container.decode(String.self, forKey: .paneSlotID),
                contentID: try container.decode(String.self, forKey: .contentID),
                key: try container.decode(TerminalRuntimeControlKey.self, forKey: .key)
            )
        case "reducer":
            self = .reducer
        default:
            self = .unsupported
        }
    }

    var sideEffect: ShellCoreControlSideEffect? {
        switch self {
        case .sendTerminalText(let paneSlotID, _, let text):
            return .sendText(paneID: paneSlotID, text: text)
        case .sendTerminalKey(let paneSlotID, _, .returnKey):
            return .sendText(paneID: paneSlotID, text: "\r")
        case .sendTerminalKey:
            return nil
        case .reducer, .unsupported:
            return nil
        }
    }
}
