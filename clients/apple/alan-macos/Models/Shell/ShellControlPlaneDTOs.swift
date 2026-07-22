import Foundation

#if os(macOS)
enum AlanShellControlCommandKind: String, Codable {
    case state
    case spaceList = "space.list"
    case spaceCreate = "space.create"
    case spaceSetTerminalProfile = "space.set_terminal_profile"
    case tabList = "tab.list"
    case tabOpen = "tab.open"
    case tabClose = "tab.close"
    case tabReorder = "tab.reorder"
    case tabPin = "tab.pin"
    case tabUnpin = "tab.unpin"
    case tabMoveToSpace = "tab.move_to_space"
    case paneList = "pane.list"
    case paneSnapshot = "pane.snapshot"
    case paneSplit = "pane.split"
    case paneClose = "pane.close"
    case paneLift = "pane.lift"
    case paneMove = "pane.move"
    case paneMoveWithinTab = "pane.move_within_tab"
    case paneFocus = "pane.focus"
    case paneSpatialFocus = "pane.spatial_focus"
    case paneResizeSplit = "pane.resize_split"
    case paneEqualizeSplits = "pane.equalize_splits"
    case paneZoom = "pane.zoom"
    case paneUnzoom = "pane.unzoom"
    case terminalSendText = "terminal.send_text"
    case terminalSendKey = "terminal.send_key"
    case terminalRenderMetrics = "terminal.render_metrics"
    case agentActivity = "agent.activity"
    case attentionInbox = "attention.inbox"
    case attentionSet = "attention.set"
    case routingCandidates = "routing.candidates"
    case eventsRead = "events.read"
    case performanceDiagnosticsSetEnabled = "performance_diagnostics.set_enabled"
    case performanceDiagnosticsExportRecent = "performance_diagnostics.export_recent"
    case performanceDiagnosticsRecordChildPressure =
        "performance_diagnostics.record_child_pressure"
}

struct AlanShellControlCommand: Codable {
    let requestID: String
    let command: AlanShellControlCommandKind
    let spaceID: String?
    let targetSpaceID: String?
    let tabID: String?
    let paneID: String?
    let paneSlotID: String?
    let contentID: String?
    let splitNodeID: String?
    let ratio: Double?
    let section: ShellTabOrganizationSection?
    let index: Int?
    let direction: ShellSplitDirection?
    let spatialDirection: ShellSpatialFocusDirection?
    let placement: ShellPaneSplitDirection?
    let title: String?
    let cwd: String?
    let text: String?
    let key: String?
    let attention: ShellAttentionState?
    let agentKind: String?
    let agentStatus: String?
    let sessionLabel: String?
    let projectLabel: String?
    let workingDirectory: String?
    let terminalProfileID: String?
    let detail: String?
    let updatedAt: String?
    let afterEventID: String?
    let limit: Int?
    let enabled: Bool?
    let exportDirectory: String?
    let childProcessRole: String?
    let childCPUPercent: Double?
    let childMemoryBytes: UInt64?
    let childThreadCount: Int?

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case command
        case spaceID = "space_id"
        case targetSpaceID = "target_space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case splitNodeID = "split_node_id"
        case ratio
        case section
        case index
        case direction
        case spatialDirection = "spatial_direction"
        case placement
        case title
        case cwd
        case text
        case key
        case attention
        case agentKind = "agent_kind"
        case agentStatus = "agent_status"
        case sessionLabel = "session_label"
        case projectLabel = "project_label"
        case workingDirectory = "working_directory"
        case terminalProfileID = "terminal_profile_id"
        case detail
        case updatedAt = "updated_at"
        case afterEventID = "after_event_id"
        case limit
        case enabled
        case exportDirectory = "export_directory"
        case childProcessRole = "child_process_role"
        case childCPUPercent = "child_cpu_percent"
        case childMemoryBytes = "child_memory_bytes"
        case childThreadCount = "child_thread_count"
    }
}

extension AlanShellControlCommand {
    var agentActivityEvent: TerminalAgentActivityEvent? {
        guard let agentKind, let agentStatus else { return nil }
        return TerminalAgentActivityEvent(
            agentKind: agentKind,
            status: agentStatus,
            sessionLabel: sessionLabel,
            projectLabel: projectLabel,
            workingDirectory: workingDirectory ?? cwd,
            detail: detail,
            updatedAt: updatedAt
        )
    }
}

struct AlanShellControlContentProjection {
    let paneSlotID: String?
    let contentID: String?
    let kind: ShellContentKind?
    let title: String?
    let capabilities: [ShellContentCapability]?
}

extension ShellContentStateSnapshot {
    func controlPlanePaneSlots(in tabID: String?) -> [ShellPaneSlot] {
        guard let tabID else { return paneSlots }
        return paneSlots.filter { $0.tabID == tabID }
    }

    func controlPlaneContents(in tabID: String?) -> [ShellContentInstance] {
        let mountedContentIDs = Set(controlPlanePaneSlots(in: tabID).map(\.contentID))
        return contents.filter { mountedContentIDs.contains($0.contentID) }
    }

    func controlPlaneContentProjection(
        paneSlotID: String?,
        contentID: String?
    ) -> AlanShellControlContentProjection {
        if let contentID {
            guard let content = content(contentID: contentID) else {
                return AlanShellControlContentProjection(
                    paneSlotID: nil,
                    contentID: contentID,
                    kind: nil,
                    title: nil,
                    capabilities: nil
                )
            }

            let paneSlot = paneSlots.first { $0.contentID == contentID }
            return AlanShellControlContentProjection(
                paneSlotID: paneSlot?.paneSlotID,
                contentID: content.contentID,
                kind: content.kind,
                title: content.title,
                capabilities: content.capabilities
            )
        }

        if let paneSlotID,
           let paneSlot = paneSlot(paneSlotID: paneSlotID),
           let content = content(contentID: paneSlot.contentID)
        {
            return AlanShellControlContentProjection(
                paneSlotID: paneSlot.paneSlotID,
                contentID: content.contentID,
                kind: content.kind,
                title: content.title,
                capabilities: content.capabilities
            )
        }

        return AlanShellControlContentProjection(
            paneSlotID: paneSlotID,
            contentID: contentID,
            kind: nil,
            title: nil,
            capabilities: nil
        )
    }
}

struct AlanShellControlResponse: Codable {
    let requestID: String
    let contractVersion: String
    let applied: Bool?
    let state: ShellStateSnapshot?
    let spaces: [ShellSpace]?
    let tabs: [ShellTab]?
    let panes: [ShellPane]?
    let paneSlots: [ShellPaneSlot]?
    let contents: [ShellContentInstance]?
    let pane: ShellPane?
    let items: [AlanShellAttentionInboxItem]?
    let candidates: [AlanShellRoutingCandidate]?
    let events: [AlanShellEventEnvelope]?
    let focusedPaneID: String?
    let focusedPaneSlotID: String?
    let spaceID: String?
    let sourceSpaceID: String?
    let targetSpaceID: String?
    let tabID: String?
    let paneID: String?
    let paneSlotID: String?
    let contentID: String?
    let contentKind: ShellContentKind?
    let contentTitle: String?
    let contentCapabilities: [ShellContentCapability]?
    let section: ShellTabOrganizationSection?
    let index: Int?
    let acceptedBytes: Int?
    let deliveryCode: String?
    let runtimePhase: String?
    var terminalRenderMetrics: TerminalRenderCoordinatorMetrics?
    var latestEventID: String?
    let splitNodeID: String?
    let ratio: Double?
    let changedSplitIDs: [String]?
    let affectedPaneIDs: [String]?
    let zoomedPaneID: String?
    let sourceTabID: String?
    let targetTabID: String?
    let previousFocusedPaneID: String?
    let currentFocusedPaneID: String?
    let previousFocusedPaneSlotID: String?
    let currentFocusedPaneSlotID: String?
    let splitDirection: ShellSplitDirection?
    let spatialDirection: ShellSpatialFocusDirection?
    let placement: ShellPaneSplitDirection?
    let mountedContentInstanceID: String?
    let diagnosticsEnabled: Bool?
    let diagnosticsRetainedEventCount: Int?
    let diagnosticsStutterMarkerCount: Int?
    let diagnosticsBundlePath: String?
    let errorCode: String?
    let errorMessage: String?

    init(
        requestID: String,
        contractVersion: String,
        applied: Bool?,
        state: ShellStateSnapshot? = nil,
        spaces: [ShellSpace]? = nil,
        tabs: [ShellTab]? = nil,
        panes: [ShellPane]? = nil,
        paneSlots: [ShellPaneSlot]? = nil,
        contents: [ShellContentInstance]? = nil,
        pane: ShellPane? = nil,
        items: [AlanShellAttentionInboxItem]? = nil,
        candidates: [AlanShellRoutingCandidate]? = nil,
        events: [AlanShellEventEnvelope]? = nil,
        focusedPaneID: String? = nil,
        focusedPaneSlotID: String? = nil,
        spaceID: String? = nil,
        sourceSpaceID: String? = nil,
        targetSpaceID: String? = nil,
        tabID: String? = nil,
        paneID: String? = nil,
        paneSlotID: String? = nil,
        contentID: String? = nil,
        contentKind: ShellContentKind? = nil,
        contentTitle: String? = nil,
        contentCapabilities: [ShellContentCapability]? = nil,
        section: ShellTabOrganizationSection? = nil,
        index: Int? = nil,
        acceptedBytes: Int? = nil,
        deliveryCode: String? = nil,
        runtimePhase: String? = nil,
        terminalRenderMetrics: TerminalRenderCoordinatorMetrics? = nil,
        latestEventID: String? = nil,
        splitNodeID: String? = nil,
        ratio: Double? = nil,
        changedSplitIDs: [String]? = nil,
        affectedPaneIDs: [String]? = nil,
        zoomedPaneID: String? = nil,
        sourceTabID: String? = nil,
        targetTabID: String? = nil,
        previousFocusedPaneID: String? = nil,
        currentFocusedPaneID: String? = nil,
        previousFocusedPaneSlotID: String? = nil,
        currentFocusedPaneSlotID: String? = nil,
        splitDirection: ShellSplitDirection? = nil,
        spatialDirection: ShellSpatialFocusDirection? = nil,
        placement: ShellPaneSplitDirection? = nil,
        mountedContentInstanceID: String? = nil,
        diagnosticsEnabled: Bool? = nil,
        diagnosticsRetainedEventCount: Int? = nil,
        diagnosticsStutterMarkerCount: Int? = nil,
        diagnosticsBundlePath: String? = nil,
        errorCode: String? = nil,
        errorMessage: String? = nil
    ) {
        self.requestID = requestID
        self.contractVersion = contractVersion
        self.applied = applied
        self.state = state
        self.spaces = spaces
        self.tabs = tabs
        self.panes = panes
        self.paneSlots = paneSlots
        self.contents = contents
        self.pane = pane
        self.items = items
        self.candidates = candidates
        self.events = events
        self.focusedPaneID = focusedPaneID
        self.focusedPaneSlotID = focusedPaneSlotID
        self.spaceID = spaceID
        self.sourceSpaceID = sourceSpaceID
        self.targetSpaceID = targetSpaceID
        self.tabID = tabID
        self.paneID = paneID
        self.paneSlotID = paneSlotID
        self.contentID = contentID
        self.contentKind = contentKind
        self.contentTitle = contentTitle
        self.contentCapabilities = contentCapabilities
        self.section = section
        self.index = index
        self.acceptedBytes = acceptedBytes
        self.deliveryCode = deliveryCode
        self.runtimePhase = runtimePhase
        self.terminalRenderMetrics = terminalRenderMetrics
        self.latestEventID = latestEventID
        self.splitNodeID = splitNodeID
        self.ratio = ratio
        self.changedSplitIDs = changedSplitIDs
        self.affectedPaneIDs = affectedPaneIDs
        self.zoomedPaneID = zoomedPaneID
        self.sourceTabID = sourceTabID
        self.targetTabID = targetTabID
        self.previousFocusedPaneID = previousFocusedPaneID
        self.currentFocusedPaneID = currentFocusedPaneID
        self.previousFocusedPaneSlotID = previousFocusedPaneSlotID
        self.currentFocusedPaneSlotID = currentFocusedPaneSlotID
        self.splitDirection = splitDirection
        self.spatialDirection = spatialDirection
        self.placement = placement
        self.mountedContentInstanceID = mountedContentInstanceID
        self.diagnosticsEnabled = diagnosticsEnabled
        self.diagnosticsRetainedEventCount = diagnosticsRetainedEventCount
        self.diagnosticsStutterMarkerCount = diagnosticsStutterMarkerCount
        self.diagnosticsBundlePath = diagnosticsBundlePath
        self.errorCode = errorCode
        self.errorMessage = errorMessage
    }

    private enum CodingKeys: String, CodingKey {
        case requestID = "request_id"
        case contractVersion = "contract_version"
        case applied
        case state
        case spaces
        case tabs
        case panes
        case paneSlots = "pane_slots"
        case contents
        case pane
        case items
        case candidates
        case events
        case focusedPaneID = "focused_pane_id"
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaceID = "space_id"
        case sourceSpaceID = "source_space_id"
        case targetSpaceID = "target_space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
        case contentKind = "content_kind"
        case contentTitle = "content_title"
        case contentCapabilities = "content_capabilities"
        case section
        case index
        case acceptedBytes = "accepted_bytes"
        case deliveryCode = "delivery_code"
        case runtimePhase = "runtime_phase"
        case terminalRenderMetrics = "terminal_render_metrics"
        case latestEventID = "latest_event_id"
        case splitNodeID = "split_node_id"
        case ratio
        case changedSplitIDs = "changed_split_ids"
        case affectedPaneIDs = "affected_pane_ids"
        case zoomedPaneID = "zoomed_pane_id"
        case sourceTabID = "source_tab_id"
        case targetTabID = "target_tab_id"
        case previousFocusedPaneID = "previous_focused_pane_id"
        case currentFocusedPaneID = "current_focused_pane_id"
        case previousFocusedPaneSlotID = "previous_focused_pane_slot_id"
        case currentFocusedPaneSlotID = "current_focused_pane_slot_id"
        case splitDirection = "split_direction"
        case spatialDirection = "spatial_direction"
        case placement
        case mountedContentInstanceID = "mounted_content_instance_id"
        case diagnosticsEnabled = "diagnostics_enabled"
        case diagnosticsRetainedEventCount = "diagnostics_retained_event_count"
        case diagnosticsStutterMarkerCount = "diagnostics_stutter_marker_count"
        case diagnosticsBundlePath = "diagnostics_bundle_path"
        case errorCode = "error_code"
        case errorMessage = "error_message"
    }
}

struct AlanShellAttentionInboxItem: Codable, Equatable, Identifiable {
    let itemID: String
    let spaceID: String
    let tabID: String
    let paneID: String
    let attention: ShellAttentionState
    let summary: String

    var id: String { itemID }

    private enum CodingKeys: String, CodingKey {
        case itemID = "item_id"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case attention
        case summary
    }
}

struct AlanShellRoutingCandidate: Codable, Equatable, Identifiable {
    let paneID: String
    let score: Double
    let reasons: [String]

    var id: String { paneID }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case score
        case reasons
    }
}

enum AlanShellJSONValue: Codable, Equatable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case array([AlanShellJSONValue])
    case object([String: AlanShellJSONValue])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([AlanShellJSONValue].self) {
            self = .array(value)
        } else {
            self = .object(try container.decode([String: AlanShellJSONValue].self))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .string(value):
            try container.encode(value)
        case let .number(value):
            try container.encode(value)
        case let .bool(value):
            try container.encode(value)
        case let .array(value):
            try container.encode(value)
        case let .object(value):
            try container.encode(value)
        case .null:
            try container.encodeNil()
        }
    }
}

struct AlanShellEventEnvelope: Codable, Equatable, Identifiable {
    let eventID: String
    let type: String
    let timestamp: String
    let windowID: String
    let spaceID: String?
    let tabID: String?
    let paneID: String?
    let payload: [String: AlanShellJSONValue]

    var id: String { eventID }

    private enum CodingKeys: String, CodingKey {
        case eventID = "event_id"
        case type
        case timestamp
        case windowID = "window_id"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
        case payload
    }
}

struct AlanShellBindingProjection: Codable, Equatable {
    let processPath: String
    let machineState: String
    let pendingRequest: Bool
    let source: String?
    let lastProjectedAt: String?
    let windowID: String?
    let spaceID: String?
    let tabID: String?
    let paneID: String?

    private enum CodingKeys: String, CodingKey {
        case processPath = "process_path"
        case machineState = "machine_state"
        case pendingRequest = "pending_request"
        case source
        case lastProjectedAt = "last_projected_at"
        case windowID = "window_id"
        case spaceID = "space_id"
        case tabID = "tab_id"
        case paneID = "pane_id"
    }

    var shellBinding: ShellAlanBinding {
        ShellAlanBinding(
            processPath: processPath,
            machineState: machineState,
            pendingRequest: pendingRequest,
            source: source ?? "pane_binding_file",
            lastProjectedAt: lastProjectedAt
        )
    }
}
#endif
