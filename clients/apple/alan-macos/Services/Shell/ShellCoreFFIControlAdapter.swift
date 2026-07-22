import Foundation

struct AlanShellLocalCommandExecutionContext {
    let reservedPaneSlotIDs: [String]

    init(reservedPaneSlotIDs: [String] = []) {
        self.reservedPaneSlotIDs = reservedPaneSlotIDs
    }
}

extension ShellCoreFFIAdapter {
    func handleControlCommand(
        _ command: AlanShellControlCommand,
        state: ShellStateSnapshot,
        context: AlanShellLocalCommandExecutionContext = .init()
    ) throws -> ShellCoreControlCommandResult {
        let response: ShellCoreControlHandleResponse = try send(
            operation: "control.handle",
            payload: ShellCoreControlHandlePayload(
                state: ShellCorePortableWorkspaceState(projecting: state),
                command: command,
                context: ShellCoreControlExecutionContext(
                    reservedPaneSlotIDs: context.reservedPaneSlotIDs
                )
            )
        )
        return try response.result.shellCommandResult(command: command, fallbackState: state)
    }

}

enum ShellCoreControlSideEffect: Equatable {
    case sendText(paneSlotID: String, contentID: String, text: String)
    case sendKey(
        paneSlotID: String,
        contentID: String,
        key: TerminalRuntimeControlKey
    )
}

struct ShellCoreControlCommandResult {
    let response: AlanShellControlResponse
    let updatedState: ShellStateSnapshot?
    let sideEffect: ShellCoreControlSideEffect?
}

private struct ShellCoreControlHandlePayload: Encodable {
    let state: ShellCorePortableWorkspaceState
    let command: AlanShellControlCommand
    let context: ShellCoreControlExecutionContext
}

private struct ShellCoreControlExecutionContext: Encodable {
    let reservedPaneSlotIDs: [String]

    private enum CodingKeys: String, CodingKey {
        case reservedPaneSlotIDs = "reserved_pane_slot_ids"
    }
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

    func shellCommandResult(
        command: AlanShellControlCommand,
        fallbackState: ShellStateSnapshot
    ) throws -> ShellCoreControlCommandResult {
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
                command: command,
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
        command: AlanShellControlCommand,
        fallbackState: ShellStateSnapshot,
        projectionState: ShellStateSnapshot,
        materializedResponseState: ShellStateSnapshot?
    ) throws -> AlanShellControlResponse {
        let projectedContentState = projectionState.contentStateProjection()
        let responseState = materializedResponseState
        let commandPaneSlotID = command.paneSlotID ?? command.paneID
        let previousPane = commandPaneSlotID.flatMap(fallbackState.pane(paneID:))
        let targetZoomTabID = command.tabID ?? previousPane?.tabID ?? fallbackState.focusedTabID
        let previousZoomedPaneID = targetZoomTabID.flatMap {
            fallbackState.zoomedPaneIDByTabID[$0]
        }
        let equalizedTabID = command.tabID ?? projectionState.focusedTabID
        let equalizedTab = equalizedTabID.flatMap(projectionState.tab(tabID:))
        let previousEqualizedTab = equalizedTabID.flatMap(fallbackState.tab(tabID:))
        let derivedChangedSplitIDs: [String]? = {
            guard command.command == .paneEqualizeSplits,
                  applied == true || errorCode == "unchanged_state",
                  let equalizedTab,
                  let previousEqualizedTab
            else {
                return changedSplitIDs
            }
            return equalizedTab.paneTree.splitNodeIDsWithChangedRatios(
                comparedTo: previousEqualizedTab.paneTree
            )
        }()
        let derivedAffectedPaneIDs: [String]? = {
            switch command.command {
            case .paneResizeSplit:
                guard applied == true else { return nil }
                return splitNodeID.flatMap { splitNodeID in
                    projectionState.spaces
                        .flatMap(\.tabs)
                        .compactMap { $0.paneTree.node(nodeID: splitNodeID)?.paneIDs }
                        .first
                }
            case .paneEqualizeSplits:
                return applied == true || errorCode == "unchanged_state"
                    ? equalizedTab?.paneTree.paneIDs
                    : nil
            default:
                return nil
            }
        }()
        let sourceSpaceID: String? = {
            switch command.command {
            case .tabPin, .tabUnpin, .tabReorder, .tabMoveToSpace:
                return command.tabID.flatMap {
                    fallbackState.tabOrganizationLocation(tabID: $0)?.spaceID
                }
            default:
                return nil
            }
        }()
        let sourceTabID: String? = {
            switch command.command {
            case .paneMove, .paneMoveWithinTab:
                return previousPane?.tabID
            default:
                return nil
            }
        }()
        let targetTabID: String? = {
            switch command.command {
            case .paneMove:
                return command.tabID
            case .paneMoveWithinTab:
                return previousPane?.tabID
            default:
                return nil
            }
        }()
        let mountedContentInstanceID: String? = {
            switch command.command {
            case .paneMove, .paneMoveWithinTab:
                return commandPaneSlotID.flatMap {
                    projectedContentState.contentMounted(in: $0)?.contentID
                }
            case .paneZoom:
                return commandPaneSlotID.flatMap {
                    projectedContentState.contentMounted(in: $0)?.contentID
                }
            case .paneUnzoom:
                return previousZoomedPaneID.flatMap {
                    fallbackState.contentStateProjection().contentMounted(in: $0)?.contentID
                }
            default:
                return nil
            }
        }()
        let responsePaneID: String? = {
            switch command.command {
            case .paneResizeSplit, .paneEqualizeSplits:
                return applied == true ? derivedAffectedPaneIDs?.first ?? paneID : paneID
            case .paneZoom:
                return commandPaneSlotID ?? paneID
            case .paneUnzoom:
                if applied == true {
                    return previousZoomedPaneID
                }
                return errorCode == "pane_not_found" ? commandPaneSlotID ?? paneID : nil
            case .paneSpatialFocus:
                return applied == true ? paneID : fallbackState.focusedPaneID
            case .paneMoveWithinTab:
                return commandPaneSlotID ?? paneID
            default:
                return paneID
            }
        }()
        let responseSpaceID: String? = {
            switch command.command {
            case .paneZoom:
                return spaceID ?? previousPane?.spaceID
            case .paneSpatialFocus:
                return applied == true ? spaceID : fallbackState.focusedSpaceID
            case .paneMoveWithinTab:
                return spaceID ?? previousPane?.spaceID
            default:
                return spaceID
            }
        }()
        let responseTabID: String? = {
            switch command.command {
            case .paneZoom:
                return tabID ?? previousPane?.tabID
            case .paneSpatialFocus:
                return applied == true ? tabID : fallbackState.focusedTabID
            case .paneMoveWithinTab:
                return previousPane?.tabID ?? tabID
            default:
                return tabID
            }
        }()
        let responseZoomedPaneID = command.command == .paneZoom
            && errorCode == "unchanged_state"
            ? previousZoomedPaneID
            : zoomedPaneID
        let explicitContentTargetWasMissing = command.contentID != nil
            && errorCode == "content_not_found"
        let suppressesUnzoomTargetProjection = command.command == .paneUnzoom
            && applied != true
            && errorCode != "pane_not_found"
        let responsePaneSlotID: String? = if explicitContentTargetWasMissing {
            command.paneSlotID
        } else if suppressesUnzoomTargetProjection {
            nil
        } else {
            paneSlotID ?? responsePaneID
        }
        let contentProjection = projectedContentState.controlPlaneContentProjection(
            paneSlotID: responsePaneSlotID,
            contentID: contentID
        )
        let reportsFocusTransition = switch command.command {
        case .paneSpatialFocus, .paneZoom, .paneUnzoom:
            true
        default:
            false
        }
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
            spaceID: responseSpaceID,
            sourceSpaceID: sourceSpaceID,
            targetSpaceID: targetSpaceID,
            tabID: responseTabID,
            paneID: responsePaneID,
            paneSlotID: explicitContentTargetWasMissing
                ? nil
                : responsePaneSlotID ?? contentProjection.paneSlotID,
            contentID: suppressesUnzoomTargetProjection
                ? nil
                : contentID ?? contentProjection.contentID,
            contentKind: explicitContentTargetWasMissing || suppressesUnzoomTargetProjection
                ? nil
                : contentKind ?? contentProjection.kind,
            contentTitle: explicitContentTargetWasMissing || suppressesUnzoomTargetProjection
                ? nil
                : contentProjection.title,
            contentCapabilities: explicitContentTargetWasMissing || suppressesUnzoomTargetProjection
                ? nil
                : contentProjection.capabilities,
            section: section,
            index: index,
            acceptedBytes: nil,
            deliveryCode: nil,
            runtimePhase: nil,
            terminalRenderMetrics: nil,
            latestEventID: nil,
            splitNodeID: splitNodeID,
            ratio: command.command == .paneEqualizeSplits
                && (applied == true || errorCode == "unchanged_state")
                ? 0.5
                : ratio,
            changedSplitIDs: derivedChangedSplitIDs,
            affectedPaneIDs: derivedAffectedPaneIDs,
            zoomedPaneID: responseZoomedPaneID,
            sourceTabID: sourceTabID,
            targetTabID: targetTabID,
            previousFocusedPaneID: reportsFocusTransition
                ? fallbackState.focusedPaneID
                : previousFocusedPaneSlotID,
            currentFocusedPaneID: reportsFocusTransition
                ? projectionState.focusedPaneID
                : currentFocusedPaneSlotID,
            previousFocusedPaneSlotID: reportsFocusTransition
                ? fallbackState.focusedPaneID
                : previousFocusedPaneSlotID,
            currentFocusedPaneSlotID: reportsFocusTransition
                ? projectionState.focusedPaneID
                : currentFocusedPaneSlotID,
            splitDirection: command.command == .paneMove
                ? command.direction ?? .vertical
                : nil,
            spatialDirection: command.command == .paneSpatialFocus
                ? command.spatialDirection
                : nil,
            placement: command.command == .paneMoveWithinTab
                ? command.placement
                : placement,
            mountedContentInstanceID: explicitContentTargetWasMissing
                || suppressesUnzoomTargetProjection
                ? nil
                : contentProjection.contentID ?? mountedContentInstanceID,
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
        case .sendTerminalText(let paneSlotID, let contentID, let text):
            return .sendText(
                paneSlotID: paneSlotID,
                contentID: contentID,
                text: text
            )
        case .sendTerminalKey(let paneSlotID, let contentID, let key):
            return .sendKey(
                paneSlotID: paneSlotID,
                contentID: contentID,
                key: key
            )
        case .reducer, .unsupported:
            return nil
        }
    }
}
