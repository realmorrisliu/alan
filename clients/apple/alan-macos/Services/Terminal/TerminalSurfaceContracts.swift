#if os(macOS)
import AppKit
import Foundation
#if canImport(GhosttyKit)
import GhosttyKit
#endif

enum TerminalTranscriptCaptureFailureCode: String, Equatable {
    case missingRuntime = "missing_runtime"
    case emptyTranscript = "empty_transcript"
}

struct TerminalTranscriptCaptureFailure: Equatable {
    let contentID: String
    let code: TerminalTranscriptCaptureFailureCode
    let message: String
}

enum TerminalTranscriptCaptureResult: Equatable {
    case captured(TerminalTranscriptSnapshot)
    case failed(TerminalTranscriptCaptureFailure)
}

enum TerminalRuntimeGracefulShutdownReason: String, Codable, Equatable {
    case paneClose = "pane_close"
    case tabClose = "tab_close"
    case windowClose = "window_close"
    case appQuit = "app_quit"
}

enum TerminalRuntimeGracefulShutdownRequestCode: String, Codable, Equatable {
    case requested
    case alreadyExited = "already_exited"
    case missingRuntime = "missing_runtime"
    case unavailable
    case rejected
}

struct TerminalRuntimeGracefulShutdownRequestResult: Codable, Equatable {
    let contentID: String
    let reason: TerminalRuntimeGracefulShutdownReason
    let code: TerminalRuntimeGracefulShutdownRequestCode
    let delivery: TerminalRuntimeDeliveryResult?
    let message: String?

    var wasRequested: Bool {
        code == .requested
    }
}

enum AlanTerminalSurfaceLifecyclePhase: String, Equatable {
    case pending
    case bootstrapping
    case attachable
    case attached
    case closing
    case closed
    case failed
}

enum AlanTerminalSurfaceTeardownStatus: String, Equatable {
    case notStarted = "not_started"
    case closing
    case completed
    case interrupted
}

struct AlanTerminalSurfaceSnapshot: Equatable {
    let contentID: String
    let paneID: String
    let lifecyclePhase: AlanTerminalSurfaceLifecyclePhase
    let renderer: TerminalRendererSnapshot
    let metadata: TerminalPaneMetadataSnapshot
    let lastDelivery: TerminalRuntimeDeliveryResult?
    let teardownStatus: AlanTerminalSurfaceTeardownStatus
    let attachedViewCount: Int
    let lastUpdatedAt: Date

    var runtimePhase: String {
        renderer.phase.rawValue
    }

    static func pending(contentID: String, paneID: String) -> AlanTerminalSurfaceSnapshot {
        AlanTerminalSurfaceSnapshot(
            contentID: contentID,
            paneID: paneID,
            lifecyclePhase: .pending,
            renderer: .placeholder,
            metadata: .placeholder,
            lastDelivery: nil,
            teardownStatus: .notStarted,
            attachedViewCount: 0,
            lastUpdatedAt: .now
        )
    }
}

@MainActor
protocol AlanTerminalSurfaceHandle: AnyObject {
    var contentID: String { get }
    var paneID: String { get }
    var snapshot: AlanTerminalSurfaceSnapshot { get }
    var isSurfaceReady: Bool { get }
    var renderPriority: TerminalRuntimeRenderPriority { get }
    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? { get }
    var fallbackTranscriptLines: [String] { get }
    var terminalDimensions: AlanTerminalPtyDimensions? { get }
    var seededTranscriptSnapshot: TerminalTranscriptSnapshot? { get }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?)
    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    )
    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    )
    func detach()
    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot)
    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String?
    func seedRestoredTranscriptSnapshot(_ snapshot: TerminalTranscriptSnapshot)
    func clearRestoredTranscriptSnapshot()
    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult
    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult
    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult
    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus
}

#if canImport(GhosttyKit)
@MainActor
protocol AlanGhosttyEventSurfaceHandle:
    AlanTerminalSurfaceHandle,
    AlanTerminalSearchEngine,
    AlanTerminalScrollbackEngine,
    AlanTerminalSelectionEngine,
    AlanTerminalCommandBufferEngine
{
    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e
    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool
    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool
    func sendProgrammaticText(_ text: String)
    func sendPreedit(_ text: String?)
    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e)
    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool
    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t)
    func sendMousePressure(stage: UInt32, pressure: Double)
    func readSelectionText() -> String?
    func hasSelection() -> Bool
    func readText(in range: AlanTerminalBufferRange) -> String?
    func imeRect(in view: NSView) -> NSRect?
}
#endif

@MainActor
protocol AlanTerminalRuntimeService: AnyObject {
    var diagnostics: AlanGhosttyBootstrapDiagnostics { get }
    var registeredContentIDs: Set<String> { get }
    var registeredPaneIDs: Set<String> { get }
    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? { get }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics
    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle
    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle?
    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot?
    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult
    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult
    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    )
    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String)
    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult
    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult
    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus
    func finalizeTerminalContents(excluding activeContentIDs: Set<String>)
}

extension AlanTerminalRuntimeService {
    func surfaceHandle(
        for paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        surfaceHandle(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            mountedAtPaneID: paneID,
            bootProfile: bootProfile
        )
    }

    func existingSurfaceHandle(for paneID: String) -> AlanTerminalSurfaceHandle? {
        existingSurfaceHandle(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID)
        )
    }

    func snapshot(for paneID: String) -> AlanTerminalSurfaceSnapshot? {
        snapshot(forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID))
    }

    func sendText(to paneID: String, text: String) -> TerminalRuntimeDeliveryResult {
        sendText(
            toTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            text: text
        )
    }

    func sendKey(to paneID: String, key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        sendKey(
            toTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            key: key
        )
    }

    func requestGracefulShutdown(
        for paneID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        requestGracefulShutdown(
            forTerminalContentID: ShellContentInstance.terminalContentID(forPaneID: paneID),
            reason: reason
        )
    }

    @discardableResult
    func finalizePane(_ paneID: String) -> AlanTerminalSurfaceTeardownStatus {
        finalizeTerminalContent(ShellContentInstance.terminalContentID(forPaneID: paneID))
    }

    func finalizePanes(excluding activePaneIDs: Set<String>) {
        finalizeTerminalContents(
            excluding: Set(activePaneIDs.map { ShellContentInstance.terminalContentID(forPaneID: $0) })
        )
    }
}

#endif
