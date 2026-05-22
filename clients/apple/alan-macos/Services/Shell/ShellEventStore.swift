import Foundation

#if os(macOS)
@MainActor
final class AlanShellEventStore {
    private let windowID: String
    private let fileManager: FileManager
    private let eventsFileURL: URL
    private let encoder: JSONEncoder
    private let diagnosticHandler: @MainActor (String) -> Void
    private var events: [AlanShellEventEnvelope] = []
    private var nextEventOrdinal = 1

    init(
        windowID: String,
        fileManager: FileManager,
        eventsFileURL: URL,
        encoder: JSONEncoder,
        diagnosticHandler: @escaping @MainActor (String) -> Void
    ) {
        self.windowID = windowID
        self.fileManager = fileManager
        self.eventsFileURL = eventsFileURL
        self.encoder = encoder
        self.diagnosticHandler = diagnosticHandler
    }

    var latestEventID: String? {
        events.last?.eventID
    }

    func read(afterEventID: String?, limit: Int?) -> [AlanShellEventEnvelope] {
        let startIndex: Int
        if let afterEventID,
           let index = events.firstIndex(where: { $0.eventID == afterEventID }) {
            startIndex = events.index(after: index)
        } else {
            startIndex = 0
        }

        let slice = events.dropFirst(startIndex)
        let capped = limit.map { max(0, $0) } ?? 50
        return Array(slice.prefix(capped))
    }

    func recordTextDelivery(
        requestID: String,
        spaceID: String?,
        tabID: String?,
        paneID: String,
        contentID: String,
        delivery: TerminalRuntimeDeliveryResult
    ) {
        var payload: [String: AlanShellJSONValue] = [
            "request_id": .string(requestID),
            "content_id": .string(contentID),
            "delivery_code": .string(delivery.code.rawValue),
            "accepted_bytes": .number(Double(delivery.acceptedBytes))
        ]
        if let errorCode = delivery.errorCode {
            payload["error_code"] = .string(errorCode)
        }
        if let errorMessage = delivery.errorMessage {
            payload["error_message"] = .string(errorMessage)
        }
        if let runtimePhase = delivery.runtimePhase {
            payload["runtime_phase"] = .string(runtimePhase)
        }

        append(
            type: "terminal.text_delivery",
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            payload: payload
        )
    }

    func recordSplitEqualized(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        changedSplitIDs: [String],
        affectedPaneIDs: [String]
    ) {
        var payload: [String: AlanShellJSONValue] = [
            "tab_id": .string(tabID),
            "changed_split_ids": .array(changedSplitIDs.map(AlanShellJSONValue.string)),
            "affected_pane_ids": .array(affectedPaneIDs.map(AlanShellJSONValue.string)),
            "ratio": .number(0.5)
        ]
        if let requestID {
            payload["request_id"] = .string(requestID)
        }

        append(
            type: "split.equalized",
            spaceID: spaceID,
            tabID: tabID,
            paneID: affectedPaneIDs.first,
            payload: payload
        )
    }

    func recordZoomStateChanged(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        paneID: String?,
        zoomedPaneID: String?
    ) {
        var payload: [String: AlanShellJSONValue] = [
            "tab_id": .string(tabID),
            "zoomed": .bool(zoomedPaneID != nil),
            "zoomed_pane_id": zoomedPaneID.map(AlanShellJSONValue.string) ?? .null
        ]
        if let requestID {
            payload["request_id"] = .string(requestID)
        }

        append(
            type: "pane.zoom_changed",
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID ?? zoomedPaneID,
            payload: payload
        )
    }

    func recordSpatialFocus(
        requestID: String?,
        spaceID: String?,
        tabID: String?,
        previousPaneID: String?,
        currentPaneID: String?,
        direction: ShellSpatialFocusDirection,
        applied: Bool
    ) {
        var payload: [String: AlanShellJSONValue] = [
            "spatial_direction": .string(direction.rawValue),
            "applied": .bool(applied),
            "previous_focused_pane_id": previousPaneID.map(AlanShellJSONValue.string) ?? .null,
            "current_focused_pane_id": currentPaneID.map(AlanShellJSONValue.string) ?? .null
        ]
        if let requestID {
            payload["request_id"] = .string(requestID)
        }

        append(
            type: "pane.spatial_focus",
            spaceID: spaceID,
            tabID: tabID,
            paneID: currentPaneID ?? previousPaneID,
            payload: payload
        )
    }

    func recordPaneMovedInTab(
        requestID: String?,
        spaceID: String?,
        tabID: String,
        paneID: String,
        placement: ShellPaneSplitDirection,
        mountedContentInstanceID: String
    ) {
        var payload: [String: AlanShellJSONValue] = [
            "pane_id": .string(paneID),
            "source_tab_id": .string(tabID),
            "target_tab_id": .string(tabID),
            "placement": .string(placement.rawValue),
            "mounted_content_instance_id": .string(mountedContentInstanceID),
            "preserved_mounted_content_instance": .bool(true)
        ]
        if let requestID {
            payload["request_id"] = .string(requestID)
        }

        append(
            type: "pane.moved_in_tab",
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            payload: payload
        )
    }

    func recordChanges(from previousState: ShellStateSnapshot?, to currentState: ShellStateSnapshot) {
        guard let previousState else { return }

        let previousPanesByID = Dictionary(uniqueKeysWithValues: previousState.panes.map { ($0.paneID, $0) })
        let currentPanesByID = Dictionary(uniqueKeysWithValues: currentState.panes.map { ($0.paneID, $0) })

        if previousState.focusedPaneID != currentState.focusedPaneID {
            append(
                type: "focus.changed",
                spaceID: currentState.focusedSpaceID,
                tabID: currentState.focusedTabID,
                paneID: currentState.focusedPaneID,
                payload: [
                    "previous_pane_id": .string(previousState.focusedPaneID ?? ""),
                    "current_pane_id": .string(currentState.focusedPaneID ?? "")
                ]
            )
        }

        let previousTabs = Set(previousState.spaces.flatMap(\.tabs).map(\.tabID))
        let currentTabs = Set(currentState.spaces.flatMap(\.tabs).map(\.tabID))
        for createdTabID in currentTabs.subtracting(previousTabs).sorted() {
            if let tab = currentState.tab(tabID: createdTabID),
               let paneID = tab.paneTree.paneIDs.first,
               let pane = currentPanesByID[paneID] {
                append(
                    type: "tab.created",
                    spaceID: pane.spaceID,
                    tabID: tab.tabID,
                    paneID: paneID,
                    payload: [
                        "tab_id": .string(tab.tabID),
                        "kind": .string(tab.kind.rawValue)
                    ]
                )
            }
        }
        for closedTabID in previousTabs.subtracting(currentTabs).sorted() {
            let pane = previousState.panes.first { $0.tabID == closedTabID }
            append(
                type: "tab.closed",
                spaceID: pane?.spaceID,
                tabID: closedTabID,
                paneID: pane?.paneID,
                payload: ["tab_id": .string(closedTabID)]
            )
        }

        recordTabOrganizationChanges(
            previousState: previousState,
            currentState: currentState,
            commonTabIDs: previousTabs.intersection(currentTabs)
        )
        recordSplitRatioChanges(
            previousState: previousState,
            currentState: currentState,
            commonTabIDs: previousTabs.intersection(currentTabs)
        )

        let allPaneIDs = Set(previousPanesByID.keys).union(currentPanesByID.keys)
        for paneID in allPaneIDs.sorted() {
            let previousPane = previousPanesByID[paneID]
            let currentPane = currentPanesByID[paneID]

            if let previousPane, let currentPane {
                recordExistingPaneChanges(previousPane: previousPane, currentPane: currentPane)
            } else if let currentPane {
                append(
                    type: "pane.created",
                    spaceID: currentPane.spaceID,
                    tabID: currentPane.tabID,
                    paneID: currentPane.paneID,
                    payload: [
                        "pane_id": .string(currentPane.paneID),
                        "tab_id": .string(currentPane.tabID)
                    ]
                )
            } else if let previousPane {
                append(
                    type: "pane.closed",
                    spaceID: previousPane.spaceID,
                    tabID: previousPane.tabID,
                    paneID: previousPane.paneID,
                    payload: [
                        "pane_id": .string(previousPane.paneID)
                    ]
                )
            }
        }
    }

    private func recordTabOrganizationChanges(
        previousState: ShellStateSnapshot,
        currentState: ShellStateSnapshot,
        commonTabIDs: Set<String>
    ) {
        for tabID in commonTabIDs.sorted() {
            guard let previousLocation = previousState.tabOrganizationLocation(tabID: tabID),
                  let currentLocation = currentState.tabOrganizationLocation(tabID: tabID),
                  previousLocation != currentLocation
            else {
                continue
            }

            let eventType: String
            if previousLocation.spaceID != currentLocation.spaceID {
                eventType = "tab.moved_to_space"
            } else if previousLocation.section != currentLocation.section {
                eventType = "tab.pin_changed"
            } else {
                eventType = "tab.reordered"
            }

            append(
                type: eventType,
                spaceID: currentLocation.spaceID,
                tabID: tabID,
                paneID: currentState.tab(tabID: tabID)?.paneTree.paneIDs.first,
                payload: [
                    "tab_id": .string(tabID),
                    "previous_space_id": .string(previousLocation.spaceID),
                    "current_space_id": .string(currentLocation.spaceID),
                    "previous_section": .string(previousLocation.section.rawValue),
                    "current_section": .string(currentLocation.section.rawValue),
                    "previous_index": .number(Double(previousLocation.index)),
                    "current_index": .number(Double(currentLocation.index)),
                    "previous_pinned": .bool(previousLocation.section == .pinned),
                    "current_pinned": .bool(currentLocation.section == .pinned),
                    "focused_space_id": .string(currentState.focusedSpaceID ?? ""),
                    "focused_tab_id": .string(currentState.focusedTabID ?? "")
                ]
            )
        }
    }

    private func recordSplitRatioChanges(
        previousState: ShellStateSnapshot,
        currentState: ShellStateSnapshot,
        commonTabIDs: Set<String>
    ) {
        for tabID in commonTabIDs.sorted() {
            guard let previousTab = previousState.tab(tabID: tabID),
                  let currentTab = currentState.tab(tabID: tabID)
            else {
                continue
            }

            let previousRatios = previousTab.paneTree.splitRatiosByNodeID
            let currentRatios = currentTab.paneTree.splitRatiosByNodeID
            for splitNodeID in Set(previousRatios.keys).intersection(currentRatios.keys).sorted() {
                guard let previousRatio = previousRatios[splitNodeID],
                      let currentRatio = currentRatios[splitNodeID],
                      previousRatio != currentRatio,
                      let splitNode = currentTab.paneTree.node(nodeID: splitNodeID)
                else {
                    continue
                }

                let affectedPaneIDs = splitNode.paneIDs
                append(
                    type: "split.ratio_changed",
                    spaceID: currentState.panes.first { $0.tabID == tabID }?.spaceID,
                    tabID: tabID,
                    paneID: affectedPaneIDs.first,
                    payload: [
                        "split_node_id": .string(splitNodeID),
                        "previous_ratio": .number(previousRatio),
                        "ratio": .number(currentRatio),
                        "affected_pane_ids": .array(affectedPaneIDs.map(AlanShellJSONValue.string))
                    ]
                )
            }
        }
    }

    private func recordExistingPaneChanges(previousPane: ShellPane, currentPane: ShellPane) {
        if previousPane.tabID != currentPane.tabID || previousPane.spaceID != currentPane.spaceID {
            append(
                type: "pane.moved",
                spaceID: currentPane.spaceID,
                tabID: currentPane.tabID,
                paneID: currentPane.paneID,
                payload: [
                    "previous_space_id": .string(previousPane.spaceID),
                    "current_space_id": .string(currentPane.spaceID),
                    "previous_tab_id": .string(previousPane.tabID),
                    "current_tab_id": .string(currentPane.tabID),
                    "mounted_content_instance_id": .string(currentPane.paneID),
                    "preserved_mounted_content_instance": .bool(true)
                ]
            )
        }

        let changedFields = changedMetadataFields(previousPane: previousPane, currentPane: currentPane)
        if !changedFields.isEmpty {
            append(
                type: "pane.metadata_changed",
                spaceID: currentPane.spaceID,
                tabID: currentPane.tabID,
                paneID: currentPane.paneID,
                payload: [
                    "changed_fields": .array(changedFields.map(AlanShellJSONValue.string))
                ]
            )
        }

        if previousPane.attention != currentPane.attention {
            append(
                type: "attention.changed",
                spaceID: currentPane.spaceID,
                tabID: currentPane.tabID,
                paneID: currentPane.paneID,
                payload: [
                    "previous": .string(previousPane.attention.rawValue),
                    "current": .string(currentPane.attention.rawValue)
                ]
            )
        }

        if previousPane.alanBinding != currentPane.alanBinding {
            append(
                type: "AlanBinding.changed",
                spaceID: currentPane.spaceID,
                tabID: currentPane.tabID,
                paneID: currentPane.paneID,
                payload: [
                    "session_id": .string(currentPane.alanBinding?.sessionID ?? ""),
                    "run_status": .string(currentPane.alanBinding?.runStatus ?? ""),
                    "pending_yield": .bool(currentPane.alanBinding?.pendingYield ?? false)
                ]
            )
        }
    }

    private func changedMetadataFields(
        previousPane: ShellPane,
        currentPane: ShellPane
    ) -> [String] {
        var changedFields: [String] = []
        if previousPane.cwd != currentPane.cwd {
            changedFields.append("cwd")
        }
        if previousPane.viewport?.title != currentPane.viewport?.title {
            changedFields.append("viewport.title")
        }
        if previousPane.viewport?.summary != currentPane.viewport?.summary {
            changedFields.append("viewport.summary")
        }
        if previousPane.context?.gitBranch != currentPane.context?.gitBranch {
            changedFields.append("context.git_branch")
        }
        if previousPane.context?.lastCommandExitCode != currentPane.context?.lastCommandExitCode {
            changedFields.append("context.last_command_exit_code")
        }
        if previousPane.context?.rendererPhase != currentPane.context?.rendererPhase {
            changedFields.append("context.renderer_phase")
        }
        if previousPane.context?.displayName != currentPane.context?.displayName {
            changedFields.append("context.display_name")
        }
        if previousPane.context?.displayID != currentPane.context?.displayID {
            changedFields.append("context.display_id")
        }
        if previousPane.context?.windowTitle != currentPane.context?.windowTitle {
            changedFields.append("context.window_title")
        }
        if previousPane.context?.socketPath != currentPane.context?.socketPath {
            changedFields.append("context.socket_path")
        }
        if previousPane.context?.launchCommand != currentPane.context?.launchCommand {
            changedFields.append("context.launch_command")
        }
        return changedFields
    }

    private func append(
        type: String,
        spaceID: String?,
        tabID: String?,
        paneID: String?,
        payload: [String: AlanShellJSONValue]
    ) {
        let event = AlanShellEventEnvelope(
            eventID: "ev_\(nextEventOrdinal)",
            type: type,
            timestamp: ISO8601DateFormatter().string(from: .now),
            windowID: windowID,
            spaceID: spaceID,
            tabID: tabID,
            paneID: paneID,
            payload: payload
        )
        nextEventOrdinal += 1
        events.append(event)
        if events.count > 200 {
            events.removeFirst(events.count - 200)
        }
        persist(event)
    }

    private func persist(_ event: AlanShellEventEnvelope) {
        if let data = try? encoder.encode(event),
           let line = String(data: data, encoding: .utf8) {
            do {
                if fileManager.fileExists(atPath: eventsFileURL.path) {
                    let handle = try FileHandle(forWritingTo: eventsFileURL)
                    defer { try? handle.close() }
                    _ = try handle.seekToEnd()
                    try handle.write(contentsOf: Data("\(line)\n".utf8))
                } else {
                    try Data("\(line)\n".utf8).write(to: eventsFileURL, options: .atomic)
                }
            } catch {
                diagnosticHandler("Failed to persist shell event log: \(error.localizedDescription)")
            }
        }
    }
}
#endif
