import CoreGraphics
import Foundation

#if os(macOS)
enum TerminalHostStage: String, Equatable {
    case scaffold
    case viewAttached = "view_attached"
    case windowAttached = "window_attached"
    case focused
}

enum TerminalRendererKind: String, Equatable {
    case scaffold
    case ghosttyLive = "ghostty_live"
}

enum TerminalRendererPhase: String, Equatable {
    case pending
    case libraryReady = "library_ready"
    case appReady = "app_ready"
    case surfaceReady = "surface_ready"
    case firstRefresh = "first_refresh"
    case failed
}

struct TerminalRendererSnapshot: Equatable {
    let kind: TerminalRendererKind
    let phase: TerminalRendererPhase
    let summary: String
    let detail: String?
    let failureReason: String?
    let recentEvents: [String]

    var phaseLabel: String {
        phase.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    static let placeholder = TerminalRendererSnapshot(
        kind: .scaffold,
        phase: .pending,
        summary: "AppKit terminal scaffold is active.",
        detail: nil,
        failureReason: nil,
        recentEvents: []
    )
}

struct TerminalPaneMetadataSnapshot: Equatable {
    let title: String?
    let workingDirectory: String?
    let summary: String?
    let attention: ShellAttentionState
    let processExited: Bool
    let lastCommandExitCode: Int?
    let lastUpdatedAt: Date?
    let activeTaskState: ShellTabActiveTaskState?
    let activity: TerminalActivitySnapshot?
    let clearsActivity: Bool

    init(
        title: String?,
        workingDirectory: String?,
        summary: String?,
        attention: ShellAttentionState,
        processExited: Bool,
        lastCommandExitCode: Int?,
        lastUpdatedAt: Date?,
        activeTaskState: ShellTabActiveTaskState? = .inactive,
        activity: TerminalActivitySnapshot? = nil,
        clearsActivity: Bool = false
    ) {
        self.title = title
        self.workingDirectory = workingDirectory
        self.summary = summary
        self.attention = attention
        self.processExited = processExited
        self.lastCommandExitCode = lastCommandExitCode
        self.lastUpdatedAt = lastUpdatedAt
        self.activeTaskState = activeTaskState
        self.activity = activity
        self.clearsActivity = clearsActivity
    }

    static let placeholder = TerminalPaneMetadataSnapshot(
        title: nil,
        workingDirectory: nil,
        summary: nil,
        attention: .idle,
        processExited: false,
        lastCommandExitCode: nil,
        lastUpdatedAt: nil,
        activeTaskState: .inactive,
        activity: nil,
        clearsActivity: false
    )
}

struct TerminalHostRuntimeSnapshot: Equatable {
    let stage: TerminalHostStage
    let contentID: String?
    let paneID: String?
    let tabID: String?
    let renderPriority: TerminalRuntimeRenderPriority
    let logicalSize: CGSize
    let backingSize: CGSize
    let displayName: String?
    let displayID: String?
    let attachedWindowTitle: String?
    let isFocused: Bool
    let renderer: TerminalRendererSnapshot
    let paneMetadata: TerminalPaneMetadataSnapshot
    let surfaceState: AlanTerminalSurfaceStateSnapshot
    let lastUpdatedAt: Date

    init(
        stage: TerminalHostStage,
        contentID: String?,
        paneID: String?,
        tabID: String?,
        renderPriority: TerminalRuntimeRenderPriority = .foregroundInteractive,
        logicalSize: CGSize,
        backingSize: CGSize,
        displayName: String?,
        displayID: String?,
        attachedWindowTitle: String?,
        isFocused: Bool,
        renderer: TerminalRendererSnapshot,
        paneMetadata: TerminalPaneMetadataSnapshot,
        surfaceState: AlanTerminalSurfaceStateSnapshot,
        lastUpdatedAt: Date
    ) {
        self.stage = stage
        self.contentID = contentID
        self.paneID = paneID
        self.tabID = tabID
        self.renderPriority = renderPriority
        self.logicalSize = logicalSize
        self.backingSize = backingSize
        self.displayName = displayName
        self.displayID = displayID
        self.attachedWindowTitle = attachedWindowTitle
        self.isFocused = isFocused
        self.renderer = renderer
        self.paneMetadata = paneMetadata
        self.surfaceState = surfaceState
        self.lastUpdatedAt = lastUpdatedAt
    }

    var stageLabel: String {
        stage.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    /// Equality that ignores the volatile publish timestamps. Two snapshots that
    /// only differ in `lastUpdatedAt` represent the same observable shell state,
    /// so callers can suppress redundant `@Published` mutations (and the
    /// tree-wide SwiftUI invalidation they trigger).
    func equalsIgnoringTimestamp(_ other: TerminalHostRuntimeSnapshot) -> Bool {
        stage == other.stage
            && contentID == other.contentID
            && paneID == other.paneID
            && tabID == other.tabID
            && renderPriority == other.renderPriority
            && logicalSize == other.logicalSize
            && backingSize == other.backingSize
            && displayName == other.displayName
            && displayID == other.displayID
            && attachedWindowTitle == other.attachedWindowTitle
            && isFocused == other.isFocused
            && renderer == other.renderer
            && paneMetadata == other.paneMetadata
            && surfaceState.equalsIgnoringTimestamp(other.surfaceState)
    }

    static let placeholder = TerminalHostRuntimeSnapshot(
        stage: .scaffold,
        contentID: nil,
        paneID: nil,
        tabID: nil,
        renderPriority: .hiddenBackground,
        logicalSize: .zero,
        backingSize: .zero,
        displayName: nil,
        displayID: nil,
        attachedWindowTitle: nil,
        isFocused: false,
        renderer: .placeholder,
        paneMetadata: .placeholder,
        surfaceState: .placeholder,
        lastUpdatedAt: .now
    )
}
#endif
