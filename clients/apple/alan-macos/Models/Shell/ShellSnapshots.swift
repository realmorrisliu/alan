import Foundation

struct ShellViewportSnapshot: Codable, Equatable {
    let title: String?
    let summary: String?
    let visibleExcerpt: String?
    let lastActivityAt: String?

    private enum CodingKeys: String, CodingKey {
        case title
        case summary
        case visibleExcerpt = "visible_excerpt"
        case lastActivityAt = "last_activity_at"
    }
}

struct ShellAlanBinding: Codable, Equatable {
    let sessionID: String
    let runStatus: String
    let pendingYield: Bool
    let source: String?
    let lastProjectedAt: String?

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id"
        case runStatus = "run_status"
        case pendingYield = "pending_yield"
        case source
        case lastProjectedAt = "last_projected_at"
    }
}

enum ShellQuickTerminalPresentation: String, Codable, Equatable {
    case visible
    case hidden
}

struct ShellQuickTerminalSlot: Codable, Equatable {
    static let globalPaneID = "quick_terminal_pane"
    static let globalTabID = "quick_terminal_tab"
    static let globalSpaceID = "quick_terminal_space"

    let paneID: String
    let presentation: ShellQuickTerminalPresentation
    let lastWorkingDirectory: String?

    init(
        paneID: String = Self.globalPaneID,
        presentation: ShellQuickTerminalPresentation,
        lastWorkingDirectory: String?
    ) {
        self.paneID = paneID
        self.presentation = presentation
        self.lastWorkingDirectory = lastWorkingDirectory
    }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
    }
}

struct ShellPane: Identifiable, Codable, Equatable {
    let paneID: String
    let tabID: String
    let spaceID: String
    let launchTarget: ShellLaunchTarget?
    let cwd: String?
    let process: ShellProcessBinding?
    let attention: ShellAttentionState
    let context: ShellContextSnapshot?
    let viewport: ShellViewportSnapshot?
    let activity: TerminalActivitySnapshot?
    let alanBinding: ShellAlanBinding?
    let terminalProfileID: String?

    var id: String { paneID }

    init(
        paneID: String,
        tabID: String,
        spaceID: String,
        launchTarget: ShellLaunchTarget?,
        cwd: String?,
        process: ShellProcessBinding?,
        attention: ShellAttentionState,
        context: ShellContextSnapshot?,
        viewport: ShellViewportSnapshot?,
        activity: TerminalActivitySnapshot? = nil,
        alanBinding: ShellAlanBinding?,
        terminalProfileID: String? = nil
    ) {
        self.paneID = paneID
        self.tabID = tabID
        self.spaceID = spaceID
        self.launchTarget = launchTarget
        self.cwd = cwd
        self.process = process
        self.attention = attention
        self.context = context
        self.viewport = viewport
        self.activity = activity
        self.alanBinding = alanBinding
        self.terminalProfileID = terminalProfileID
    }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case tabID = "tab_id"
        case spaceID = "space_id"
        case launchTarget = "launch_target"
        case cwd
        case process
        case attention
        case context
        case viewport
        case activity
        case alanBinding = "alan_binding"
        case terminalProfileID = "terminal_profile_id"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            paneID: try container.decode(String.self, forKey: .paneID),
            tabID: try container.decode(String.self, forKey: .tabID),
            spaceID: try container.decode(String.self, forKey: .spaceID),
            launchTarget: try container.decodeIfPresent(ShellLaunchTarget.self, forKey: .launchTarget),
            cwd: try container.decodeIfPresent(String.self, forKey: .cwd),
            process: try container.decodeIfPresent(ShellProcessBinding.self, forKey: .process),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            context: try container.decodeIfPresent(ShellContextSnapshot.self, forKey: .context),
            viewport: try container.decodeIfPresent(ShellViewportSnapshot.self, forKey: .viewport),
            activity: try container.decodeIfPresent(TerminalActivitySnapshot.self, forKey: .activity),
            alanBinding: try container.decodeIfPresent(ShellAlanBinding.self, forKey: .alanBinding),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID)
        )
    }
}

extension ShellPane {
    var isQuickTerminalPane: Bool {
        paneID == ShellQuickTerminalSlot.globalPaneID
            && tabID == ShellQuickTerminalSlot.globalTabID
            && spaceID == ShellQuickTerminalSlot.globalSpaceID
    }

    var terminalContentID: String {
        ShellContentInstance.terminalContentID(forPaneID: paneID)
    }
}

extension ShellPane {
    var resolvedLaunchTarget: ShellLaunchTarget {
        launchTarget ?? .shell
    }
}

enum ShellContentKind: String, Codable, CaseIterable {
    case terminal
    case markdown
    case settings
}

enum ShellContentIntent {
    case terminal(launchTarget: ShellLaunchTarget, title: String?, workingDirectory: String?)
    case markdown(fileURL: URL, title: String?)
    case settings(title: String?)
}

enum ShellContentCapability: String, Codable, CaseIterable {
    case terminalInput = "terminal_input"
    case terminalSearch = "terminal_search"
    case terminalPaste = "terminal_paste"
    case terminalRuntimeMetadata = "terminal_runtime_metadata"
    case markdownReadOnlyViewer = "markdown_read_only_viewer"
    case settingsSurface = "settings_surface"
}

enum ShellContentLifecycleState: String, Codable, CaseIterable {
    case active
    case closing
    case closed
    case failed
}

struct ShellContentRendererState: Codable, Equatable {
    let phase: String
    let detail: String?

    static let placeholder = ShellContentRendererState(phase: "placeholder", detail: nil)

    private enum CodingKeys: String, CodingKey {
        case phase
        case detail
    }
}

struct TerminalTranscriptDimensions: Codable, Equatable {
    let columns: Int
    let rows: Int

    private enum CodingKeys: String, CodingKey {
        case columns
        case rows
    }
}

struct TerminalTranscriptViewport: Codable, Equatable {
    let firstVisibleRow: Int?
    let cursorRow: Int?

    private enum CodingKeys: String, CodingKey {
        case firstVisibleRow = "first_visible_row"
        case cursorRow = "cursor_row"
    }
}

struct TerminalTranscriptProcessSummary: Codable, Equatable {
    let processState: String?
    let program: String?
    let argvPreview: [String]?
    let lastCommandExitCode: Int?

    private enum CodingKeys: String, CodingKey {
        case processState = "process_state"
        case program
        case argvPreview = "argv_preview"
        case lastCommandExitCode = "last_command_exit_code"
    }
}

struct TerminalTranscriptTruncationMetadata: Codable, Equatable {
    let originalRowCount: Int
    let storedRowCount: Int
    let rowLimit: Int
    let encodedByteLimit: Int
    let encodedByteCount: Int
    let truncatedHead: Bool
    let truncatedBytes: Bool

    private enum CodingKeys: String, CodingKey {
        case originalRowCount = "original_row_count"
        case storedRowCount = "stored_row_count"
        case rowLimit = "row_limit"
        case encodedByteLimit = "encoded_byte_limit"
        case encodedByteCount = "encoded_byte_count"
        case truncatedHead = "truncated_head"
        case truncatedBytes = "truncated_bytes"
    }
}

struct TerminalTranscriptSnapshot: Codable, Equatable {
    static let defaultMaxRows = 500
    static let defaultEncodedByteLimit = 64 * 1024

    let contentID: String
    let cwd: String?
    let title: String?
    let dimensions: TerminalTranscriptDimensions?
    let viewport: TerminalTranscriptViewport?
    let transcriptLines: [String]
    let processSummary: TerminalTranscriptProcessSummary?
    let capturedAt: Date
    let truncation: TerminalTranscriptTruncationMetadata
    let alternateScreen: Bool

    init(
        contentID: String,
        cwd: String?,
        title: String?,
        dimensions: TerminalTranscriptDimensions?,
        viewport: TerminalTranscriptViewport?,
        transcriptLines: [String],
        processSummary: TerminalTranscriptProcessSummary?,
        capturedAt: Date,
        truncation: TerminalTranscriptTruncationMetadata? = nil,
        alternateScreen: Bool
    ) {
        self.contentID = contentID
        self.cwd = cwd
        self.title = title
        self.dimensions = dimensions
        self.viewport = viewport
        self.transcriptLines = transcriptLines
        self.processSummary = processSummary
        self.capturedAt = capturedAt
        self.truncation = truncation ?? TerminalTranscriptTruncationMetadata(
            originalRowCount: transcriptLines.count,
            storedRowCount: transcriptLines.count,
            rowLimit: Self.defaultMaxRows,
            encodedByteLimit: Self.defaultEncodedByteLimit,
            encodedByteCount: Self.encodedByteCount(for: transcriptLines),
            truncatedHead: false,
            truncatedBytes: false
        )
        self.alternateScreen = alternateScreen
    }

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case cwd
        case title
        case dimensions
        case viewport
        case transcriptLines = "transcript_lines"
        case processSummary = "process_summary"
        case capturedAt = "captured_at"
        case truncation
        case alternateScreen = "alternate_screen"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        contentID = try container.decode(String.self, forKey: .contentID)
        cwd = try container.decodeIfPresent(String.self, forKey: .cwd)
        title = try container.decodeIfPresent(String.self, forKey: .title)
        dimensions = try container.decodeIfPresent(TerminalTranscriptDimensions.self, forKey: .dimensions)
        viewport = try container.decodeIfPresent(TerminalTranscriptViewport.self, forKey: .viewport)
        transcriptLines = try container.decodeIfPresent([String].self, forKey: .transcriptLines) ?? []
        processSummary = try container.decodeIfPresent(
            TerminalTranscriptProcessSummary.self,
            forKey: .processSummary
        )
        capturedAt = try container.decode(Date.self, forKey: .capturedAt)
        truncation = try container.decodeIfPresent(
            TerminalTranscriptTruncationMetadata.self,
            forKey: .truncation
        ) ?? TerminalTranscriptTruncationMetadata(
            originalRowCount: transcriptLines.count,
            storedRowCount: transcriptLines.count,
            rowLimit: Self.defaultMaxRows,
            encodedByteLimit: Self.defaultEncodedByteLimit,
            encodedByteCount: Self.encodedByteCount(for: transcriptLines),
            truncatedHead: false,
            truncatedBytes: false
        )
        alternateScreen = try container.decodeIfPresent(Bool.self, forKey: .alternateScreen) ?? false
    }

    func encode(to encoder: Encoder) throws {
        let bounded = boundedForManifest()
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(bounded.contentID, forKey: .contentID)
        try container.encodeIfPresent(bounded.cwd, forKey: .cwd)
        try container.encodeIfPresent(bounded.title, forKey: .title)
        try container.encodeIfPresent(bounded.dimensions, forKey: .dimensions)
        try container.encodeIfPresent(bounded.viewport, forKey: .viewport)
        try container.encode(bounded.transcriptLines, forKey: .transcriptLines)
        try container.encodeIfPresent(bounded.processSummary, forKey: .processSummary)
        try container.encode(bounded.capturedAt, forKey: .capturedAt)
        try container.encode(bounded.truncation, forKey: .truncation)
        try container.encode(bounded.alternateScreen, forKey: .alternateScreen)
    }

    func boundedForManifest(
        maxRows: Int = TerminalTranscriptSnapshot.defaultMaxRows,
        maxEncodedBytes: Int = TerminalTranscriptSnapshot.defaultEncodedByteLimit
    ) -> TerminalTranscriptSnapshot {
        let rowLimit = max(0, maxRows)
        let byteLimit = max(0, maxEncodedBytes)
        let originalRowCount = transcriptLines.count
        var lines = Array(transcriptLines.suffix(rowLimit))
        var truncatedHead = originalRowCount > lines.count || truncation.truncatedHead
        var truncatedBytes = truncation.truncatedBytes

        while Self.encodedByteCount(for: lines) > byteLimit, !lines.isEmpty {
            if lines.count == 1 {
                let trimmed = Self.utf8Suffix(lines[0], byteLimit: byteLimit)
                truncatedBytes = trimmed != lines[0] || truncatedBytes
                lines = trimmed.isEmpty ? [] : [trimmed]
                break
            }
            lines.removeFirst()
            truncatedHead = true
            truncatedBytes = true
        }

        return TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: cwd,
            title: title,
            dimensions: dimensions,
            viewport: viewport,
            transcriptLines: lines,
            processSummary: processSummary,
            capturedAt: capturedAt,
            truncation: TerminalTranscriptTruncationMetadata(
                originalRowCount: originalRowCount,
                storedRowCount: lines.count,
                rowLimit: rowLimit,
                encodedByteLimit: byteLimit,
                encodedByteCount: Self.encodedByteCount(for: lines),
                truncatedHead: truncatedHead,
                truncatedBytes: truncatedBytes
            ),
            alternateScreen: alternateScreen
        )
    }

    private static func encodedByteCount(for lines: [String]) -> Int {
        lines.joined(separator: "\n").utf8.count
    }

    private static func utf8Suffix(_ text: String, byteLimit: Int) -> String {
        guard byteLimit > 0 else { return "" }
        guard text.utf8.count > byteLimit else { return text }
        return String(decoding: Array(text.utf8.suffix(byteLimit)), as: UTF8.self)
    }
}

struct ShellTerminalContentPayload: Codable, Equatable {
    let launchTarget: ShellLaunchTarget
    let cwd: String?
    let title: String?
    let transcriptSnapshot: TerminalTranscriptSnapshot?
    let terminalProfileID: String?

    init(
        launchTarget: ShellLaunchTarget,
        cwd: String?,
        title: String?,
        transcriptSnapshot: TerminalTranscriptSnapshot? = nil,
        terminalProfileID: String? = nil
    ) {
        self.launchTarget = launchTarget
        self.cwd = cwd
        self.title = title
        self.transcriptSnapshot = transcriptSnapshot
        self.terminalProfileID = terminalProfileID
    }

    private enum CodingKeys: String, CodingKey {
        case launchTarget = "launch_target"
        case cwd
        case title
        case transcriptSnapshot = "transcript_snapshot"
        case terminalProfileID = "terminal_profile_id"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            launchTarget: try container.decode(ShellLaunchTarget.self, forKey: .launchTarget),
            cwd: try container.decodeIfPresent(String.self, forKey: .cwd),
            title: try container.decodeIfPresent(String.self, forKey: .title),
            transcriptSnapshot: try container.decodeIfPresent(
                TerminalTranscriptSnapshot.self,
                forKey: .transcriptSnapshot
            ),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID)
        )
    }
}

struct ShellMarkdownContentPayload: Codable, Equatable {
    let fileURL: String
    let title: String?

    private enum CodingKeys: String, CodingKey {
        case fileURL = "file_url"
        case title
    }
}

struct ShellSettingsContentPayload: Codable, Equatable {
    let surfaceID: String
    let title: String?

    private enum CodingKeys: String, CodingKey {
        case surfaceID = "surface_id"
        case title
    }
}

struct ShellContentPayload: Codable, Equatable {
    let terminal: ShellTerminalContentPayload?
    let markdown: ShellMarkdownContentPayload?
    let settings: ShellSettingsContentPayload?

    private enum CodingKeys: String, CodingKey {
        case terminal
        case markdown
        case settings
    }

    static func terminal(_ payload: ShellTerminalContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: payload, markdown: nil, settings: nil)
    }

    static func markdown(_ payload: ShellMarkdownContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: nil, markdown: payload, settings: nil)
    }

    static func settings(_ payload: ShellSettingsContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: nil, markdown: nil, settings: payload)
    }
}

struct ShellContentInstance: Identifiable, Codable, Equatable {
    let contentID: String
    let kind: ShellContentKind
    let title: String
    let iconName: String?
    let capabilities: [ShellContentCapability]
    let payload: ShellContentPayload
    let lifecycle: ShellContentLifecycleState
    let rendererState: ShellContentRendererState

    var id: String { contentID }

    init(
        contentID: String,
        kind: ShellContentKind,
        title: String,
        iconName: String? = nil,
        capabilities: [ShellContentCapability]? = nil,
        payload: ShellContentPayload,
        lifecycle: ShellContentLifecycleState = .active,
        rendererState: ShellContentRendererState = .placeholder
    ) {
        self.contentID = contentID
        self.kind = kind
        self.title = title
        self.iconName = iconName
        self.capabilities = capabilities ?? Self.defaultCapabilities(for: kind)
        self.payload = payload
        self.lifecycle = lifecycle
        self.rendererState = rendererState
    }

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case iconName = "icon_name"
        case capabilities
        case payload
        case lifecycle
        case rendererState = "renderer_state"
    }

    static func defaultCapabilities(for kind: ShellContentKind) -> [ShellContentCapability] {
        switch kind {
        case .terminal:
            return [
                .terminalInput,
                .terminalSearch,
                .terminalPaste,
                .terminalRuntimeMetadata,
            ]
        case .markdown:
            return [.markdownReadOnlyViewer]
        case .settings:
            return [.settingsSurface]
        }
    }
}

struct ShellPaneSlot: Identifiable, Codable, Equatable {
    let paneSlotID: String
    let tabID: String
    let spaceID: String
    let contentID: String
    let attention: ShellAttentionState

    var id: String { paneSlotID }

    private enum CodingKeys: String, CodingKey {
        case paneSlotID = "pane_slot_id"
        case tabID = "tab_id"
        case spaceID = "space_id"
        case contentID = "content_id"
        case attention
    }
}

struct ShellPaneTreeNode: Identifiable, Codable, Equatable {
    static let minimumSplitRatio = 0.15
    static let maximumSplitRatio = 0.85

    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneID: String?
    let children: [ShellPaneTreeNode]?

    var id: String { nodeID }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneID = "pane_id"
        case children
    }

    init(
        nodeID: String,
        kind: ShellPaneTreeKind,
        direction: ShellSplitDirection?,
        ratio: Double? = nil,
        paneID: String?,
        children: [ShellPaneTreeNode]?
    ) {
        self.nodeID = nodeID
        self.kind = kind
        self.direction = direction
        self.ratio = kind == .split
            ? Self.clampedSplitRatio(ratio ?? 0.5)
            : nil
        self.paneID = paneID
        self.children = children
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(ShellPaneTreeKind.self, forKey: .kind)
        let decodedRatio = kind == .split ? try container.decode(Double.self, forKey: .ratio) : nil

        self.init(
            nodeID: try container.decode(String.self, forKey: .nodeID),
            kind: kind,
            direction: try container.decodeIfPresent(ShellSplitDirection.self, forKey: .direction),
            ratio: decodedRatio,
            paneID: try container.decodeIfPresent(String.self, forKey: .paneID),
            children: try container.decodeIfPresent([ShellPaneTreeNode].self, forKey: .children)
        )
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(nodeID, forKey: .nodeID)
        try container.encode(kind, forKey: .kind)
        try container.encodeIfPresent(direction, forKey: .direction)
        if kind == .split {
            try container.encode(ratio ?? 0.5, forKey: .ratio)
        }
        try container.encodeIfPresent(paneID, forKey: .paneID)
        try container.encodeIfPresent(children, forKey: .children)
    }

    static func clampedSplitRatio(_ ratio: Double) -> Double {
        guard ratio.isFinite else { return 0.5 }
        return min(max(ratio, minimumSplitRatio), maximumSplitRatio)
    }

    var splitRatio: Double {
        Self.clampedSplitRatio(ratio ?? 0.5)
    }
}

extension ShellPaneTreeNode {
    var splitNodes: [ShellPaneTreeNode] {
        switch kind {
        case .pane:
            return []
        case .split:
            return [self] + (children ?? []).flatMap(\.splitNodes)
        }
    }

    var splitRatiosByNodeID: [String: Double] {
        Dictionary(uniqueKeysWithValues: splitNodes.map { ($0.nodeID, $0.splitRatio) })
    }

    func splitNodeIDsWithChangedRatios(comparedTo previous: ShellPaneTreeNode) -> [String] {
        let previousRatios = previous.splitRatiosByNodeID
        return splitRatiosByNodeID.keys
            .filter { nodeID in
                guard let previousRatio = previousRatios[nodeID],
                      let currentRatio = splitRatiosByNodeID[nodeID]
                else {
                    return false
                }
                return previousRatio != currentRatio
            }
            .sorted()
    }

    var nodeIDs: [String] {
        [nodeID] + (children ?? []).flatMap(\.nodeIDs)
    }

    var paneIDs: [String] {
        switch kind {
        case .pane:
            return paneID.map { [$0] } ?? []
        case .split:
            return (children ?? []).flatMap(\.paneIDs)
        }
    }

    func contains(paneID targetPaneID: String) -> Bool {
        switch kind {
        case .pane:
            return paneID == targetPaneID
        case .split:
            return (children ?? []).contains { $0.contains(paneID: targetPaneID) }
        }
    }

    func contains(nodeID targetNodeID: String) -> Bool {
        if nodeID == targetNodeID { return true }
        return (children ?? []).contains { $0.contains(nodeID: targetNodeID) }
    }

    func node(nodeID targetNodeID: String) -> ShellPaneTreeNode? {
        if nodeID == targetNodeID { return self }
        return (children ?? []).lazy.compactMap { $0.node(nodeID: targetNodeID) }.first
    }

    func adjacentPaneID(
        from targetPaneID: String,
        direction: ShellSpatialFocusDirection
    ) -> String? {
        let frames = leafFrames(in: .unit)
        guard let targetFrame = frames.first(where: { $0.paneID == targetPaneID }) else {
            return nil
        }

        return frames
            .filter { $0.paneID != targetPaneID && targetFrame.isAdjacentCandidate($0, direction: direction) }
            .min { lhs, rhs in
                targetFrame.sortsBefore(lhs, rhs, direction: direction)
            }?
            .paneID
    }

    private struct PaneFrame {
        static let unit = PaneFrame(
            paneID: "",
            minX: 0,
            maxX: 1,
            minY: 0,
            maxY: 1
        )

        let paneID: String
        let minX: Double
        let maxX: Double
        let minY: Double
        let maxY: Double

        var width: Double { max(maxX - minX, 0) }
        var height: Double { max(maxY - minY, 0) }
        var midX: Double { (minX + maxX) / 2 }
        var midY: Double { (minY + maxY) / 2 }

        func replacingPaneID(_ paneID: String) -> PaneFrame {
            PaneFrame(
                paneID: paneID,
                minX: minX,
                maxX: maxX,
                minY: minY,
                maxY: maxY
            )
        }

        func isAdjacentCandidate(
            _ candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Bool {
            let epsilon = 0.000_001
            guard perpendicularOverlap(with: candidate, direction: direction) > epsilon else {
                return false
            }

            switch direction {
            case .left:
                return candidate.maxX <= minX + epsilon
            case .right:
                return candidate.minX >= maxX - epsilon
            case .up:
                return candidate.maxY <= minY + epsilon
            case .down:
                return candidate.minY >= maxY - epsilon
            }
        }

        func sortsBefore(
            _ lhs: PaneFrame,
            _ rhs: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Bool {
            let epsilon = 0.000_001
            let lhsDistance = primaryDistance(to: lhs, direction: direction)
            let rhsDistance = primaryDistance(to: rhs, direction: direction)
            if abs(lhsDistance - rhsDistance) > epsilon {
                return lhsDistance < rhsDistance
            }

            let lhsOverlap = perpendicularOverlap(with: lhs, direction: direction)
            let rhsOverlap = perpendicularOverlap(with: rhs, direction: direction)
            if abs(lhsOverlap - rhsOverlap) > epsilon {
                return lhsOverlap > rhsOverlap
            }

            let lhsCenterDistance = perpendicularCenterDistance(to: lhs, direction: direction)
            let rhsCenterDistance = perpendicularCenterDistance(to: rhs, direction: direction)
            if abs(lhsCenterDistance - rhsCenterDistance) > epsilon {
                return lhsCenterDistance < rhsCenterDistance
            }

            return lhs.paneID < rhs.paneID
        }

        private func primaryDistance(
            to candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left:
                return max(minX - candidate.maxX, 0)
            case .right:
                return max(candidate.minX - maxX, 0)
            case .up:
                return max(minY - candidate.maxY, 0)
            case .down:
                return max(candidate.minY - maxY, 0)
            }
        }

        private func perpendicularOverlap(
            with candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left, .right:
                return max(0, min(maxY, candidate.maxY) - max(minY, candidate.minY))
            case .up, .down:
                return max(0, min(maxX, candidate.maxX) - max(minX, candidate.minX))
            }
        }

        private func perpendicularCenterDistance(
            to candidate: PaneFrame,
            direction: ShellSpatialFocusDirection
        ) -> Double {
            switch direction {
            case .left, .right:
                return abs(midY - candidate.midY)
            case .up, .down:
                return abs(midX - candidate.midX)
            }
        }
    }

    private func leafFrames(in frame: PaneFrame) -> [PaneFrame] {
        switch kind {
        case .pane:
            guard let paneID else { return [] }
            return [frame.replacingPaneID(paneID)]
        case .split:
            let childNodes = children ?? []
            guard !childNodes.isEmpty else { return [] }

            if childNodes.count == 2 {
                let ratio = splitRatio
                switch direction ?? .horizontal {
                case .vertical:
                    let splitX = frame.minX + frame.width * ratio
                    return childNodes[0].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: splitX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    ) + childNodes[1].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: splitX,
                            maxX: frame.maxX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    )
                case .horizontal:
                    let splitY = frame.minY + frame.height * ratio
                    return childNodes[0].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: frame.minY,
                            maxY: splitY
                        )
                    ) + childNodes[1].leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: splitY,
                            maxY: frame.maxY
                        )
                    )
                }
            }

            let childCount = Double(childNodes.count)
            return childNodes.enumerated().flatMap { index, child in
                let start = Double(index) / childCount
                let end = Double(index + 1) / childCount
                switch direction ?? .horizontal {
                case .vertical:
                    let minX = frame.minX + frame.width * start
                    let maxX = frame.minX + frame.width * end
                    return child.leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: minX,
                            maxX: maxX,
                            minY: frame.minY,
                            maxY: frame.maxY
                        )
                    )
                case .horizontal:
                    let minY = frame.minY + frame.height * start
                    let maxY = frame.minY + frame.height * end
                    return child.leafFrames(
                        in: PaneFrame(
                            paneID: "",
                            minX: frame.minX,
                            maxX: frame.maxX,
                            minY: minY,
                            maxY: maxY
                        )
                    )
                }
            }
        }
    }
}

struct ShellPaneSlotTreeNode: Identifiable, Codable, Equatable {
    let nodeID: String
    let kind: ShellPaneTreeKind
    let direction: ShellSplitDirection?
    let ratio: Double?
    let paneSlotID: String?
    let children: [ShellPaneSlotTreeNode]?

    var id: String { nodeID }

    private enum CodingKeys: String, CodingKey {
        case nodeID = "node_id"
        case kind
        case direction
        case ratio
        case paneSlotID = "pane_slot_id"
        case children
    }

    init(
        nodeID: String,
        kind: ShellPaneTreeKind,
        direction: ShellSplitDirection?,
        ratio: Double? = nil,
        paneSlotID: String?,
        children: [ShellPaneSlotTreeNode]?
    ) {
        self.nodeID = nodeID
        self.kind = kind
        self.direction = direction
        self.ratio = kind == .split
            ? ShellPaneTreeNode.clampedSplitRatio(ratio ?? 0.5)
            : nil
        self.paneSlotID = paneSlotID
        self.children = children
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(ShellPaneTreeKind.self, forKey: .kind)
        let decodedRatio = kind == .split ? try container.decode(Double.self, forKey: .ratio) : nil

        self.init(
            nodeID: try container.decode(String.self, forKey: .nodeID),
            kind: kind,
            direction: try container.decodeIfPresent(ShellSplitDirection.self, forKey: .direction),
            ratio: decodedRatio,
            paneSlotID: try container.decodeIfPresent(String.self, forKey: .paneSlotID),
            children: try container.decodeIfPresent([ShellPaneSlotTreeNode].self, forKey: .children)
        )
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(nodeID, forKey: .nodeID)
        try container.encode(kind, forKey: .kind)
        try container.encodeIfPresent(direction, forKey: .direction)
        if kind == .split {
            try container.encode(ratio ?? 0.5, forKey: .ratio)
        }
        try container.encodeIfPresent(paneSlotID, forKey: .paneSlotID)
        try container.encodeIfPresent(children, forKey: .children)
    }

    static func migrating(
        paneTree: ShellPaneTreeNode,
        paneIDToSlotID: (String) -> String = { $0 }
    ) -> ShellPaneSlotTreeNode {
        ShellPaneSlotTreeNode(
            nodeID: paneTree.nodeID,
            kind: paneTree.kind,
            direction: paneTree.direction,
            ratio: paneTree.ratio,
            paneSlotID: paneTree.paneID.map(paneIDToSlotID),
            children: paneTree.children?.map {
                ShellPaneSlotTreeNode.migrating(paneTree: $0, paneIDToSlotID: paneIDToSlotID)
            }
        )
    }

    var paneSlotIDs: [String] {
        switch kind {
        case .pane:
            return paneSlotID.map { [$0] } ?? []
        case .split:
            return (children ?? []).flatMap(\.paneSlotIDs)
        }
    }

    func restoringPaneTree() -> ShellPaneTreeNode {
        ShellPaneTreeNode(
            nodeID: nodeID,
            kind: kind,
            direction: direction,
            ratio: ratio,
            paneID: paneSlotID,
            children: children?.map { $0.restoringPaneTree() }
        )
    }
}

struct ShellTab: Identifiable, Codable, Equatable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneTreeNode
    let isPinned: Bool

    var id: String { tabID }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
    }

    init(
        tabID: String,
        kind: ShellTabKind,
        title: String?,
        paneTree: ShellPaneTreeNode,
        isPinned: Bool = false
    ) {
        self.tabID = tabID
        self.kind = kind
        self.title = title
        self.paneTree = paneTree
        self.isPinned = isPinned
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            tabID: try container.decode(String.self, forKey: .tabID),
            kind: try container.decode(ShellTabKind.self, forKey: .kind),
            title: try container.decodeIfPresent(String.self, forKey: .title),
            paneTree: try container.decode(ShellPaneTreeNode.self, forKey: .paneTree),
            isPinned: try container.decodeIfPresent(Bool.self, forKey: .isPinned) ?? false
        )
    }
}

struct ShellContentTab: Identifiable, Codable, Equatable {
    let tabID: String
    let kind: ShellTabKind
    let title: String?
    let paneTree: ShellPaneSlotTreeNode
    let isPinned: Bool

    var id: String { tabID }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case kind
        case title
        case paneTree = "pane_tree"
        case isPinned = "is_pinned"
    }
}

struct ShellSpace: Identifiable, Codable, Equatable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellTab]
    let terminalProfileID: String?

    var id: String { spaceID }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case terminalProfileID = "terminal_profile_id"
    }

    init(
        spaceID: String,
        title: String,
        attention: ShellAttentionState,
        tabs: [ShellTab],
        terminalProfileID: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.attention = attention
        self.tabs = tabs
        self.terminalProfileID = terminalProfileID
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            spaceID: try container.decode(String.self, forKey: .spaceID),
            title: try container.decode(String.self, forKey: .title),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            tabs: try container.decode([ShellTab].self, forKey: .tabs),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID)
        )
    }
}

struct ShellContentSpace: Identifiable, Codable, Equatable {
    let spaceID: String
    let title: String
    let attention: ShellAttentionState
    let tabs: [ShellContentTab]
    let terminalProfileID: String?

    var id: String { spaceID }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case attention
        case tabs
        case terminalProfileID = "terminal_profile_id"
    }

    init(
        spaceID: String,
        title: String,
        attention: ShellAttentionState,
        tabs: [ShellContentTab],
        terminalProfileID: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.attention = attention
        self.tabs = tabs
        self.terminalProfileID = terminalProfileID
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            spaceID: try container.decode(String.self, forKey: .spaceID),
            title: try container.decode(String.self, forKey: .title),
            attention: try container.decode(ShellAttentionState.self, forKey: .attention),
            tabs: try container.decode([ShellContentTab].self, forKey: .tabs),
            terminalProfileID: try container.decodeIfPresent(String.self, forKey: .terminalProfileID)
        )
    }
}

struct ShellStateSnapshot: Codable, Equatable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [ShellSpace]
    let panes: [ShellPane]
    var paneSlots: [ShellPaneSlot]? = nil
    var contents: [ShellContentInstance]? = nil
    var quickTerminal: ShellQuickTerminalSlot? = nil

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case panes
        case paneSlots = "pane_slots"
        case contents
        case quickTerminal = "quick_terminal"
    }

    var prettyPrintedJSON: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{\n  \"error\": \"failed to encode shell snapshot\"\n}"
        }

        return string
    }
}

struct ShellContentStateSnapshot: Codable, Equatable {
    static let currentContractVersion = "0.2"

    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneSlotID: String?
    let spaces: [ShellContentSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [ShellContentInstance]

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
    }

    var prettyPrintedJSON: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{\n  \"error\": \"failed to encode shell content snapshot\"\n}"
        }

        return string
    }
}

extension ShellTab {
    func contains(paneID: String) -> Bool {
        paneTree.contains(paneID: paneID)
    }

    var organizationSection: ShellTabOrganizationSection {
        isPinned ? .pinned : .unpinned
    }
}

extension ShellSpace {
    var pinnedTabs: [ShellTab] {
        tabs.filter(\.isPinned)
    }

    var unpinnedTabs: [ShellTab] {
        tabs.filter { !$0.isPinned }
    }

    func tabs(in section: ShellTabOrganizationSection) -> [ShellTab] {
        switch section {
        case .pinned:
            return pinnedTabs
        case .unpinned:
            return unpinnedTabs
        }
    }
}

extension ShellStateSnapshot {
    var totalTabCount: Int {
        spaces.reduce(into: 0) { partialResult, space in
            partialResult += space.tabs.count
        }
    }

    func space(spaceID: String) -> ShellSpace? {
        spaces.first { $0.spaceID == spaceID }
    }

    func tab(tabID: String) -> ShellTab? {
        spaces.lazy.flatMap(\.tabs).first { $0.tabID == tabID }
    }

    func pane(paneID: String) -> ShellPane? {
        panes.first { $0.paneID == paneID }
    }

    func tabs(in spaceID: String?) -> [ShellTab] {
        guard let spaceID else {
            return spaces.flatMap(\.tabs)
        }
        return space(spaceID: spaceID)?.tabs ?? []
    }

    func panes(in tabID: String?) -> [ShellPane] {
        guard let tabID else {
            return panes
        }
        return panes.filter { $0.tabID == tabID }
    }

    func tabOrganizationLocation(tabID: String) -> ShellTabOrganizationLocation? {
        for space in spaces {
            if let pinnedIndex = space.pinnedTabs.firstIndex(where: { $0.tabID == tabID }) {
                return ShellTabOrganizationLocation(
                    spaceID: space.spaceID,
                    section: .pinned,
                    index: pinnedIndex
                )
            }
            if let unpinnedIndex = space.unpinnedTabs.firstIndex(where: { $0.tabID == tabID }) {
                return ShellTabOrganizationLocation(
                    spaceID: space.spaceID,
                    section: .unpinned,
                    index: unpinnedIndex
                )
            }
        }
        return nil
    }

    func contentStateProjection() -> ShellContentStateSnapshot {
        ShellContentStateSnapshot.projecting(self)
    }
}

extension ShellContentStateSnapshot {
    static func projecting(_ shellState: ShellStateSnapshot) -> ShellContentStateSnapshot {
        let layoutPaneIDs = Set(shellState.spaces.flatMap(\.tabs).flatMap(\.paneTree.paneIDs))
        let projectedPanes = shellState.panes.filter { layoutPaneIDs.contains($0.paneID) }
        let paneSlotLocations = paneSlotLocations(in: shellState.spaces)
        let projectedPanesByID = projectedPanes.reduce(into: [String: ShellPane]()) { panesByID, pane in
            panesByID[pane.paneID] = pane
        }
        let explicitPaneSlots = (shellState.paneSlots ?? []).compactMap { paneSlot -> ShellPaneSlot? in
            guard layoutPaneIDs.contains(paneSlot.paneSlotID),
                  let location = paneSlotLocations[paneSlot.paneSlotID]
            else {
                return nil
            }

            return ShellPaneSlot(
                paneSlotID: paneSlot.paneSlotID,
                tabID: location.tabID,
                spaceID: location.spaceID,
                contentID: paneSlot.contentID,
                attention: projectedPanesByID[paneSlot.paneSlotID]?.attention ?? paneSlot.attention
            )
        }
        let explicitPaneSlotIDs = Set(explicitPaneSlots.map(\.paneSlotID))
        let explicitContentIDs = Set(explicitPaneSlots.map(\.contentID))
        let explicitPaneSlotsByContentID = explicitPaneSlots.reduce(into: [String: ShellPaneSlot]()) {
            slotsByContentID, slot in
            slotsByContentID[slot.contentID] = slot
        }
        let explicitContents = (shellState.contents ?? []).filter {
            explicitContentIDs.contains($0.contentID)
        }.map { content in
            guard content.kind == .terminal,
                  let paneSlot = explicitPaneSlotsByContentID[content.contentID],
                  let pane = projectedPanesByID[paneSlot.paneSlotID]
            else {
                return content
            }

            let projected = ShellContentInstance.projectingTerminalPane(
                pane,
                contentID: content.contentID
            )
            guard let transcriptSnapshot = content.payload.terminal?.transcriptSnapshot,
                  let terminalPayload = projected.payload.terminal
            else {
                return projected
            }
            return ShellContentInstance(
                contentID: projected.contentID,
                kind: projected.kind,
                title: projected.title,
                iconName: projected.iconName,
                capabilities: projected.capabilities,
                payload: .terminal(
                    ShellTerminalContentPayload(
                        launchTarget: terminalPayload.launchTarget,
                        cwd: terminalPayload.cwd,
                        title: terminalPayload.title,
                        transcriptSnapshot: transcriptSnapshot,
                        terminalProfileID: terminalPayload.terminalProfileID
                    )
                ),
                lifecycle: projected.lifecycle,
                rendererState: projected.rendererState
            )
        }
        let terminalPanes = projectedPanes.filter { !explicitPaneSlotIDs.contains($0.paneID) }
        let paneSlots = explicitPaneSlots + terminalPanes.map(ShellPaneSlot.projectingTerminalPane)
        let contents = explicitContents + terminalPanes.map(ShellContentInstance.projectingTerminalPane)
        let validPaneSlotIDs = Set(paneSlots.map(\.paneSlotID))
        let focusedPaneSlotID =
            shellState.focusedPaneID.flatMap { validPaneSlotIDs.contains($0) ? $0 : nil }
            ?? paneSlots.first?.paneSlotID
        let spaces = shellState.spaces.map { space in
            ShellContentSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: Self.strongestAttention(
                    in: paneSlots.filter { $0.spaceID == space.spaceID }
                ),
                tabs: space.tabs.map { tab in
                    ShellContentTab(
                        tabID: tab.tabID,
                        kind: tab.kind,
                        title: tab.title,
                        paneTree: ShellPaneSlotTreeNode.migrating(paneTree: tab.paneTree),
                        isPinned: tab.isPinned
                    )
                },
                terminalProfileID: space.terminalProfileID
            )
        }

        return ShellContentStateSnapshot(
            contractVersion: currentContractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: shellState.focusedSpaceID,
            focusedTabID: shellState.focusedTabID,
            focusedPaneSlotID: focusedPaneSlotID,
            spaces: spaces,
            paneSlots: paneSlots,
            contents: contents
        )
    }

    func space(spaceID: String) -> ShellContentSpace? {
        spaces.first { $0.spaceID == spaceID }
    }

    func tab(tabID: String) -> ShellContentTab? {
        spaces.lazy.flatMap(\.tabs).first { $0.tabID == tabID }
    }

    func paneSlot(paneSlotID: String) -> ShellPaneSlot? {
        paneSlots.first { $0.paneSlotID == paneSlotID }
    }

    func content(contentID: String) -> ShellContentInstance? {
        contents.first { $0.contentID == contentID }
    }

    var focusedPaneSlot: ShellPaneSlot? {
        focusedPaneSlotID.flatMap { paneSlot(paneSlotID: $0) }
    }

    var focusedContent: ShellContentInstance? {
        focusedPaneSlot.flatMap { content(contentID: $0.contentID) }
    }

    func contentMounted(in paneSlotID: String) -> ShellContentInstance? {
        paneSlot(paneSlotID: paneSlotID).flatMap { content(contentID: $0.contentID) }
    }

    func primaryContent(in tabID: String) -> ShellContentInstance? {
        guard let tab = tab(tabID: tabID) else { return nil }
        return tab.paneTree.paneSlotIDs.lazy.compactMap { contentMounted(in: $0) }.first
    }

    func userFacingTitle(for tab: ShellContentTab) -> String? {
        tab.title
            ?? tab.paneTree.paneSlotIDs.lazy.compactMap { contentMounted(in: $0)?.title }.first
    }

    func materializingShellState() -> ShellStateSnapshot? {
        guard contractVersion == Self.currentContractVersion else { return nil }

        let sourceTabCount = spaces.reduce(0) { count, space in
            count + space.tabs.count
        }
        let paneSlotsByID = paneSlots.reduce(into: [String: ShellPaneSlot]()) { slotsByID, slot in
            slotsByID[slot.paneSlotID] = slot
        }
        let contentsByID = contents.reduce(into: [String: ShellContentInstance]()) { contentsByID, content in
            contentsByID[content.contentID] = content
        }
        var materializedPanes: [ShellPane] = []
        var materializedPaneSlots: [ShellPaneSlot] = []
        var materializedContents: [ShellContentInstance] = []

        let materializedSpaces = spaces.map { space -> ShellSpace in
            let tabs = space.tabs.compactMap { tab -> ShellTab? in
                let paneSlotIDs = tab.paneTree.paneSlotIDs
                guard !paneSlotIDs.isEmpty else { return nil }

                var tabPaneSlots: [ShellPaneSlot] = []
                var tabContents: [ShellContentInstance] = []
                for paneSlotID in paneSlotIDs {
                    guard let paneSlot = paneSlotsByID[paneSlotID],
                          let content = contentsByID[paneSlot.contentID]
                    else {
                        return nil
                    }
                    tabPaneSlots.append(paneSlot)
                    tabContents.append(content)
                }

                materializedPanes.append(
                    contentsOf: zip(tabPaneSlots, tabContents).map {
                        ShellPane.restoringContent($1, mountedIn: $0)
                    }
                )
                materializedPaneSlots.append(contentsOf: tabPaneSlots)
                materializedContents.append(contentsOf: tabContents)

                return ShellTab(
                    tabID: tab.tabID,
                    kind: tab.kind,
                    title: tab.title,
                    paneTree: tab.paneTree.restoringPaneTree(),
                    isPinned: tab.isPinned
                )
            }

            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: Self.strongestAttention(
                    in: materializedPaneSlots.filter { $0.spaceID == space.spaceID }
                ),
                tabs: tabs,
                terminalProfileID: space.terminalProfileID
            )
        }

        if sourceTabCount > 0 && materializedPanes.isEmpty {
            return nil
        }

        let existingSpaceIDs = Set(materializedSpaces.map(\.spaceID))
        let focusableSpaces = materializedSpaces.filter { !$0.tabs.isEmpty }
        let resolvedFocusedSpaceID = focusedSpaceID.flatMap {
            existingSpaceIDs.contains($0) ? $0 : nil
        } ?? focusableSpaces.first?.spaceID ?? materializedSpaces.first?.spaceID
        let focusedSpace = resolvedFocusedSpaceID.flatMap { spaceID in
            materializedSpaces.first { $0.spaceID == spaceID }
        }
        let resolvedFocusedTabID = focusedTabID.flatMap { tabID in
            focusedSpace?.tabs.contains { $0.tabID == tabID } == true ? tabID : nil
        } ?? focusedSpace?.tabs.first?.tabID
        let focusedTab = resolvedFocusedTabID.flatMap { tabID in
            focusedSpace?.tabs.first { $0.tabID == tabID }
        }
        let focusedTabPaneIDs = Set(focusedTab?.paneTree.paneIDs ?? [])
        let resolvedFocusedPaneID = focusedPaneSlotID.flatMap {
            focusedTabPaneIDs.contains($0) ? $0 : nil
        } ?? focusedTab?.paneTree.paneIDs.first

        return ShellStateSnapshot(
            contractVersion: Self.currentContractVersion,
            windowID: windowID,
            focusedSpaceID: resolvedFocusedSpaceID,
            focusedTabID: resolvedFocusedTabID,
            focusedPaneID: resolvedFocusedPaneID,
            spaces: materializedSpaces,
            panes: materializedPanes,
            paneSlots: materializedPaneSlots,
            contents: materializedContents
        )
    }

    private static func strongestAttention(in paneSlots: [ShellPaneSlot]) -> ShellAttentionState {
        paneSlots
            .map(\.attention)
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private static func paneSlotLocations(
        in spaces: [ShellSpace]
    ) -> [String: (spaceID: String, tabID: String)] {
        spaces.reduce(into: [String: (spaceID: String, tabID: String)]()) { locationsByID, space in
            for tab in space.tabs {
                for paneSlotID in tab.paneTree.paneIDs {
                    locationsByID[paneSlotID] = (spaceID: space.spaceID, tabID: tab.tabID)
                }
            }
        }
    }

    private static func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .idle:
            return 0
        case .active:
            return 1
        case .notable:
            return 2
        case .awaitingUser:
            return 3
        }
    }
}

private extension ShellPane {
    static func restoringContent(
        _ content: ShellContentInstance,
        mountedIn paneSlot: ShellPaneSlot
    ) -> ShellPane {
        let terminalPayload = content.payload.terminal
        return ShellPane(
            paneID: paneSlot.paneSlotID,
            tabID: paneSlot.tabID,
            spaceID: paneSlot.spaceID,
            launchTarget: terminalPayload?.launchTarget,
            cwd: terminalPayload?.cwd,
            process: nil,
            attention: paneSlot.attention,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: content.title,
                summary: restoredSummary(for: content.kind),
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            alanBinding: nil,
            terminalProfileID: terminalPayload?.terminalProfileID
        )
    }

    static func restoredSummary(for kind: ShellContentKind) -> String? {
        switch kind {
        case .terminal:
            return nil
        case .markdown:
            return "markdown viewer ready"
        case .settings:
            return "settings surface ready"
        }
    }
}

extension ShellPaneSlot {
    static func projectingTerminalPane(_ pane: ShellPane) -> ShellPaneSlot {
        ShellPaneSlot(
            paneSlotID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            contentID: ShellContentInstance.terminalContentID(forPaneID: pane.paneID),
            attention: pane.attention
        )
    }
}

extension ShellContentInstance {
    static func projectingTerminalPane(_ pane: ShellPane) -> ShellContentInstance {
        projectingTerminalPane(pane, contentID: terminalContentID(forPaneID: pane.paneID))
    }

    static func projectingTerminalPane(_ pane: ShellPane, contentID: String) -> ShellContentInstance {
        let title = terminalTitle(for: pane)
        return ShellContentInstance(
            contentID: contentID,
            kind: .terminal,
            title: title,
            payload: .terminal(
                ShellTerminalContentPayload(
                    launchTarget: pane.resolvedLaunchTarget,
                    cwd: pane.cwd,
                    title: title,
                    terminalProfileID: pane.terminalProfileID
                )
            ),
            rendererState: terminalRendererState(for: pane)
        )
    }

    static func terminalContentID(forPaneID paneID: String) -> String {
        "content_\(paneID)"
    }

    static func markdownContentID(forPaneSlotID paneSlotID: String) -> String {
        "content_markdown_\(paneSlotID)"
    }

    static let settingsSurfaceID = "settings_main"
    static let settingsContentID = "content_settings_main"

    private static func terminalTitle(for pane: ShellPane) -> String {
        if let title = pane.viewport?.title?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        switch pane.resolvedLaunchTarget {
        case .shell:
            return "Shell"
        }
    }

    private static func terminalRendererState(for pane: ShellPane) -> ShellContentRendererState {
        let phase = pane.context?.rendererPhase
            ?? pane.context?.rendererHealth
            ?? "placeholder"
        let detail = pane.context?.surfaceReadiness ?? pane.viewport?.summary
        return ShellContentRendererState(phase: phase, detail: detail)
    }
}
