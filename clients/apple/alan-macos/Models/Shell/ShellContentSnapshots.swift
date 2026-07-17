import CoreGraphics
import Foundation

enum ShellContentKind: String, Codable, CaseIterable {
    case terminal
    case markdown
    case settings
    case agent
}

enum ShellContentIntent {
    case terminal(launchTarget: ShellLaunchTarget, title: String?, workingDirectory: String?)
    case markdown(fileURL: URL, title: String?)
    case settings(title: String?)
    case agent(attachment: AlanAgentAttachment, title: String?)
}

enum ShellContentCapability: String, Codable, CaseIterable {
    case terminalInput = "terminal_input"
    case terminalSearch = "terminal_search"
    case terminalPaste = "terminal_paste"
    case terminalRuntimeMetadata = "terminal_runtime_metadata"
    case markdownReadOnlyViewer = "markdown_read_only_viewer"
    case settingsSurface = "settings_surface"
    case agentInput = "agent_input"
    case agentRequestResponse = "agent_request_response"
    case agentMachineControl = "agent_machine_control"
    case agentStopProcess = "agent_stop_process"
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

struct TerminalGridDimensions: Codable, Equatable {
    let columns: Int
    let rows: Int

    var isUsable: Bool {
        columns > 0 && rows > 0
    }

    var transcriptDimensions: TerminalTranscriptDimensions {
        TerminalTranscriptDimensions(columns: columns, rows: rows)
    }

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

    func replacingDimensions(_ dimensions: TerminalTranscriptDimensions?) -> TerminalTranscriptSnapshot {
        TerminalTranscriptSnapshot(
            contentID: contentID,
            cwd: cwd,
            title: title,
            dimensions: dimensions,
            viewport: viewport,
            transcriptLines: transcriptLines,
            processSummary: processSummary,
            capturedAt: capturedAt,
            truncation: truncation,
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

struct RestoredTerminalTranscriptPanelPresentation: Equatable {
    static let maxVisibleRows = 12
    static let fontSize: CGFloat = 13
    static let rowHeight: CGFloat = 18
    static let verticalInset: CGFloat = 8
    static let leadingInset: CGFloat = 0
    static let trailingInset: CGFloat = 10

    let transcriptText: String
    let visibleRows: Int
    let height: CGFloat
    let fontSize: CGFloat
    let rowHeight: CGFloat
    let verticalInset: CGFloat
    let leadingInset: CGFloat
    let trailingInset: CGFloat

    init(snapshot: TerminalTranscriptSnapshot) {
        let lines = snapshot.boundedForManifest().transcriptLines
        transcriptText = lines.joined(separator: "\n")
        visibleRows = min(max(lines.count, 1), Self.maxVisibleRows)
        fontSize = Self.fontSize
        rowHeight = Self.rowHeight
        verticalInset = Self.verticalInset
        leadingInset = Self.leadingInset
        trailingInset = Self.trailingInset
        height = CGFloat(visibleRows) * Self.rowHeight + Self.verticalInset * 2
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
            terminalProfileID: try container.decodeIfPresent(
                String.self,
                forKey: .terminalProfileID
            )
        )
    }
}

extension ShellTerminalContentPayload {
    func clearingRestoredTranscriptSnapshot() -> ShellTerminalContentPayload {
        ShellTerminalContentPayload(
            launchTarget: launchTarget,
            cwd: cwd,
            title: title,
            transcriptSnapshot: nil,
            terminalProfileID: terminalProfileID
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

struct AlanOSProcessReference: Codable, Equatable, Hashable {
    let bootID: String
    let pid: UInt64

    private enum CodingKeys: String, CodingKey {
        case bootID = "boot_id"
        case pid
    }
}

struct AlanAgentStreamOffsets: Codable, Equatable {
    var output: UInt64
    var requests: UInt64
    var actions: UInt64
    var ui: UInt64

    static let zero = AlanAgentStreamOffsets(output: 0, requests: 0, actions: 0, ui: 0)
}

struct AlanAgentContentPresentation: Codable, Equatable {
    var followsOutput: Bool

    static let `default` = AlanAgentContentPresentation(followsOutput: true)

    private enum CodingKeys: String, CodingKey {
        case followsOutput = "follows_output"
    }
}

/// The only Agent state shell persistence may own.
struct AlanAgentAttachment: Codable, Equatable {
    let process: AlanOSProcessReference
    var offsets: AlanAgentStreamOffsets
    var presentation: AlanAgentContentPresentation
}

struct ShellContentPayload: Codable, Equatable {
    let terminal: ShellTerminalContentPayload?
    let markdown: ShellMarkdownContentPayload?
    let settings: ShellSettingsContentPayload?
    let agent: AlanAgentAttachment?

    private enum CodingKeys: String, CodingKey {
        case terminal
        case markdown
        case settings
        case agent
    }

    static func terminal(_ payload: ShellTerminalContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: payload, markdown: nil, settings: nil, agent: nil)
    }

    static func markdown(_ payload: ShellMarkdownContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: nil, markdown: payload, settings: nil, agent: nil)
    }

    static func settings(_ payload: ShellSettingsContentPayload) -> ShellContentPayload {
        ShellContentPayload(terminal: nil, markdown: nil, settings: payload, agent: nil)
    }

    static func agent(_ attachment: AlanAgentAttachment) -> ShellContentPayload {
        ShellContentPayload(terminal: nil, markdown: nil, settings: nil, agent: attachment)
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
        case .agent:
            return [.agentInput, .agentRequestResponse, .agentMachineControl, .agentStopProcess]
        }
    }
}

extension ShellContentInstance {
    func clearingRestoredTranscriptSnapshot() -> (content: ShellContentInstance, removed: Bool) {
        guard let terminalPayload = payload.terminal,
              terminalPayload.transcriptSnapshot != nil
        else {
            return (self, false)
        }

        return (
            ShellContentInstance(
                contentID: contentID,
                kind: kind,
                title: title,
                iconName: iconName,
                capabilities: capabilities,
                payload: .terminal(terminalPayload.clearingRestoredTranscriptSnapshot()),
                lifecycle: lifecycle,
                rendererState: rendererState
            ),
            true
        )
    }
}
