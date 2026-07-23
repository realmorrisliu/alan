#if os(macOS)
import AppKit
import Foundation
#if canImport(GhosttyKit)
import GhosttyKit
#endif

private extension AlanTerminalPtyDimensions {
    var terminalGridDimensions: TerminalGridDimensions {
        TerminalGridDimensions(columns: columns, rows: rows)
    }
}

@MainActor
final class AlanGhosttySurfaceHandle: AlanTerminalSurfaceHandle {
    let contentID: String
    private(set) var paneID: String
    private(set) var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground

    private let bootstrap: AlanGhosttyProcessBootstrap
    private let ptyRuntime: AlanTerminalPtyRuntime
    private var ptyHandle: AlanTerminalPtyHandle?
    private var bootProfile: AlanShellBootProfile?
    private var currentSnapshot: AlanTerminalSurfaceSnapshot
    private var latestHostRuntime: TerminalHostRuntimeSnapshot?
    private var lastAppliedPtyGrid: AlanTerminalPtyDimensions?
    private var transcriptRingBufferLines: [String] = []
    private(set) var seededTranscriptSnapshot: TerminalTranscriptSnapshot?
#if canImport(GhosttyKit)
    private let liveHost = AlanGhosttyLiveHost()
#endif

    init(
        contentID: String,
        paneID: String,
        bootstrap: AlanGhosttyProcessBootstrap,
        ptyRuntime: AlanTerminalPtyRuntime,
        renderCoordinator: TerminalRenderCoordinator? = nil
    ) {
        self.contentID = contentID
        self.paneID = paneID
        self.bootstrap = bootstrap
        self.ptyRuntime = ptyRuntime
        self.currentSnapshot = .pending(contentID: contentID, paneID: paneID)
#if canImport(GhosttyKit)
        self.liveHost.renderCoordinator = renderCoordinator
#endif
    }

    var snapshot: AlanTerminalSurfaceSnapshot {
        currentSnapshot
    }

    var isSurfaceReady: Bool {
#if canImport(GhosttyKit)
        return currentSnapshot.teardownStatus != .completed && liveHost.isSurfaceReady
#else
        return false
#endif
    }

    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? {
        latestHostRuntime
    }

    private var ptyRuntimePhase: String? {
        ptyHandle?.snapshot.phase.rawValue ?? currentSnapshot.runtimePhase
    }

    var fallbackTranscriptLines: [String] {
        if let ptyLines = ptyHandle?.snapshot.transcriptLines, !ptyLines.isEmpty {
            return ptyLines
        }
        return transcriptRingBufferLines
    }

    var terminalDimensions: AlanTerminalPtyDimensions? {
        ptyHandle?.snapshot.dimensions
    }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?) {
        self.paneID = paneID
        self.bootProfile = bootProfile
        if let bootProfile {
            ptyHandle = ptyRuntime.handle(
                forTerminalContentID: contentID,
                bootRequest: bootProfile.bootRequest
            )
            lastAppliedPtyGrid = nil
        }
        guard currentSnapshot.teardownStatus != .completed else { return }
        updateSnapshot(
            lifecyclePhase: bootProfile == nil ? .pending : .attachable,
            metadata: metadataWithBootProfile(bootProfile)
        )
    }

    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    ) {
        let previousPriority = renderPriority
        renderPriority = priority
#if canImport(GhosttyKit)
        liveHost.updateRenderPriority(priority)
        if forceCatchUp || (previousPriority == .hiddenBackground && priority.isVisible) {
            liveHost.requestRenderCatchUp()
        }
#endif
    }

    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        guard currentSnapshot.teardownStatus != .completed else {
            onDiagnosticsChange(currentSnapshot.renderer)
            onMetadataChange(currentSnapshot.metadata)
            return
        }

        updateSnapshot(lifecyclePhase: .bootstrapping, attachedViewCount: 1)
        let diagnostics = bootstrap.ensureReady()
        guard diagnostics.isReady else {
            let renderer = TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .failed,
                summary: diagnostics.summary,
                detail: diagnostics.detail,
                failureReason: diagnostics.failureReason,
                recentEvents: currentSnapshot.renderer.recentEvents
            )
            updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
            onDiagnosticsChange(renderer)
            onMetadataChange(currentSnapshot.metadata)
            return
        }

#if canImport(GhosttyKit)
        guard let canvasView = canvasView as? AlanGhosttyCanvasView else {
            let renderer = TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .failed,
                summary: "Ghostty surface cannot attach to this canvas.",
                detail: nil,
                failureReason: "Expected AlanGhosttyCanvasView.",
                recentEvents: currentSnapshot.renderer.recentEvents
            )
            updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
            onDiagnosticsChange(renderer)
            return
        }

        liveHost.onDiagnosticsChange = { [weak self] snapshot in
            guard let self else { return }
            updateSnapshot(
                lifecyclePhase: snapshot.phase == .failed ? .failed : .attached,
                renderer: snapshot
            )
            onDiagnosticsChange(snapshot)
        }
        liveHost.onMetadataChange = { [weak self] metadata in
            guard let self else { return }
            updateSnapshot(metadata: metadata)
            onMetadataChange(metadata)
        }
        liveHost.onCloseRequest = { requiresConfirmation in
            onCloseRequest(requiresConfirmation)
        }
        updateRenderPriority(renderPriority, forceCatchUp: false)
        liveHost.attach(
            to: canvasView,
            bootProfile: bootProfile,
            ptyAttachmentProvider: { [weak self] in
                guard let ptyHandle = self?.ptyHandle else {
                    return .rejected(
                        .rejected(
                            "terminal_pty_runtime_missing",
                            message: "Alan-owned PTY runtime is required before renderer attachment."
                        )
                    )
                }
                return ptyHandle.makeRendererAttachment()
            },
            focused: focused,
            renderPriority: renderPriority
        )
        resizePtyToRendererGridIfAvailable()
        updateSnapshot(
            lifecyclePhase: liveHost.isSurfaceReady ? .attached : .attachable,
            metadata: liveHost.latestMetadata
        )
#else
        let renderer = TerminalRendererSnapshot(
            kind: .scaffold,
            phase: .failed,
            summary: "GhosttyKit is not linked into this build.",
            detail: nil,
            failureReason: "GhosttyKit framework is unavailable at compile time.",
            recentEvents: currentSnapshot.renderer.recentEvents
        )
        updateSnapshot(lifecyclePhase: .failed, renderer: renderer)
        onDiagnosticsChange(renderer)
#endif
    }

    func detach() {
        updateSnapshot(attachedViewCount: 0)
    }

    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) {
        latestHostRuntime = snapshot
        resizePtyToRendererGridIfAvailable()
    }

    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String? {
#if canImport(GhosttyKit)
        liveHost.readText(in: range)
#else
        nil
#endif
    }

    func seedRestoredTranscriptSnapshot(_ snapshot: TerminalTranscriptSnapshot) {
        let bounded = snapshot.boundedForManifest()
        seededTranscriptSnapshot = bounded
        transcriptRingBufferLines = bounded.transcriptLines
    }

    func clearRestoredTranscriptSnapshot() {
        seededTranscriptSnapshot = nil
        transcriptRingBufferLines = []
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return recordDelivery(.accepted(byteCount: 0, runtimePhase: ptyRuntimePhase))
        }
        guard currentSnapshot.teardownStatus != .completed else {
            return recordDelivery(
                .rejected(
                    errorCode: "terminal_runtime_closed",
                    errorMessage: "The requested pane runtime has already closed.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }
        guard let ptyHandle else {
            return recordDelivery(
                .unavailable(
                    errorMessage: "The requested pane does not have an Alan-owned PTY runtime.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }

        return recordInputDelivery(ptyHandle.writeInput(text), text: text)
    }

    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        guard currentSnapshot.teardownStatus != .completed else {
            return recordDelivery(
                .rejected(
                    errorCode: "terminal_runtime_closed",
                    errorMessage: "The requested pane runtime has already closed.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }
        guard let ptyHandle else {
            return recordDelivery(
                .unavailable(
                    errorMessage: "The requested pane does not have an Alan-owned PTY runtime.",
                    runtimePhase: ptyRuntimePhase
                )
            )
        }

        if key == .endOfTransmission {
            let eof = ptyHandle.closeInput()
            let delivery: TerminalRuntimeDeliveryResult = eof.accepted
                ? .accepted(byteCount: 0, runtimePhase: ptyHandle.snapshot.phase.rawValue)
                : .rejected(
                    errorCode: eof.code,
                    errorMessage: eof.message ?? "Alan-owned PTY EOF delivery failed.",
                    runtimePhase: ptyHandle.snapshot.phase.rawValue
                )
            return recordDelivery(delivery)
        }

        let text: String
        switch key {
        case .interrupt:
            text = "\u{3}"
        case .endOfTransmission:
            text = ""
        case .returnKey:
            text = "\r"
        }
        return recordInputDelivery(ptyHandle.writeInput(text), text: text)
    }

    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        let ptySnapshot = ptyHandle?.snapshot
        if currentSnapshot.metadata.processExited || ptySnapshot?.exitStatus != nil {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .alreadyExited,
                delivery: nil,
                message: "The terminal process has already exited."
            )
        }
        if currentSnapshot.teardownStatus == .completed {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .unavailable,
                delivery: nil,
                message: "The terminal runtime has already closed."
            )
        }
        guard let ptyHandle else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No Alan-owned PTY runtime is registered for this content."
            )
        }

        let signal = ptyHandle.sendSignal(.interrupt)
        let delivery: TerminalRuntimeDeliveryResult = signal.accepted
            ? .accepted(byteCount: 0, runtimePhase: ptyHandle.snapshot.phase.rawValue)
            : .rejected(
                errorCode: signal.code,
                errorMessage: signal.message ?? "Alan-owned PTY signal delivery failed.",
                runtimePhase: ptyHandle.snapshot.phase.rawValue
            )
        let code: TerminalRuntimeGracefulShutdownRequestCode = signal.accepted ? .requested : .rejected
        _ = recordDelivery(delivery)
        return TerminalRuntimeGracefulShutdownRequestResult(
            contentID: contentID,
            reason: reason,
            code: code,
            delivery: delivery,
            message: delivery.errorMessage
        )
    }

    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus {
        guard currentSnapshot.teardownStatus != .completed else { return .completed }
        updateSnapshot(lifecyclePhase: .closing, teardownStatus: .closing)
#if canImport(GhosttyKit)
        liveHost.teardown()
#endif
        _ = ptyHandle?.terminateForCleanup()
        ptyHandle = nil
        updateSnapshot(
            lifecyclePhase: .closed,
            metadata: .placeholder,
            teardownStatus: .completed,
            attachedViewCount: 0
        )
        return .completed
    }

    private func metadataWithBootProfile(
        _ bootProfile: AlanShellBootProfile?
    ) -> TerminalPaneMetadataSnapshot {
        guard let bootProfile else { return currentSnapshot.metadata }
        return TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: bootProfile.workingDirectory,
            summary: currentSnapshot.metadata.summary,
            attention: currentSnapshot.metadata.attention,
            processExited: currentSnapshot.metadata.processExited,
            lastCommandExitCode: currentSnapshot.metadata.lastCommandExitCode,
            lastUpdatedAt: currentSnapshot.metadata.lastUpdatedAt,
            activeTaskState: currentSnapshot.metadata.activeTaskState
        )
    }

    private func recordDelivery(
        _ delivery: TerminalRuntimeDeliveryResult
    ) -> TerminalRuntimeDeliveryResult {
        updateSnapshot(lastDelivery: delivery)
        return delivery
    }

    private func recordInputDelivery(
        _ delivery: TerminalRuntimeDeliveryResult,
        text: String
    ) -> TerminalRuntimeDeliveryResult {
#if canImport(GhosttyKit)
        if delivery.applied {
            liveHost.recordProgrammaticCommandSubmission(in: text)
        }
#endif
        return recordDelivery(delivery)
    }

    private func resizePtyToRendererGridIfAvailable() {
        guard let rendererGrid = rendererTerminalGridForPtyResize else { return }
        guard rendererGrid.isUsable else { return }
        let dimensions = AlanTerminalPtyDimensions(
            columns: rendererGrid.columns,
            rows: rendererGrid.rows
        )
        guard dimensions != lastAppliedPtyGrid else { return }
        guard let ptyHandle else { return }
        let result = ptyHandle.resize(columns: dimensions.columns, rows: dimensions.rows)
        if result.accepted {
            lastAppliedPtyGrid = dimensions
        }
    }

    private var rendererTerminalGridForPtyResize: TerminalGridDimensions? {
#if canImport(GhosttyKit)
        if let rendererGrid = liveHost.terminalGridDimensions?.terminalGridDimensions {
            return rendererGrid
        }
#endif
        return nil
    }

    private func updateSnapshot(
        lifecyclePhase: AlanTerminalSurfaceLifecyclePhase? = nil,
        renderer: TerminalRendererSnapshot? = nil,
        metadata: TerminalPaneMetadataSnapshot? = nil,
        lastDelivery: TerminalRuntimeDeliveryResult? = nil,
        teardownStatus: AlanTerminalSurfaceTeardownStatus? = nil,
        attachedViewCount: Int? = nil
    ) {
        currentSnapshot = AlanTerminalSurfaceSnapshot(
            contentID: contentID,
            paneID: paneID,
            lifecyclePhase: lifecyclePhase ?? currentSnapshot.lifecyclePhase,
            renderer: renderer ?? currentSnapshot.renderer,
            metadata: metadata ?? currentSnapshot.metadata,
            lastDelivery: lastDelivery ?? currentSnapshot.lastDelivery,
            teardownStatus: teardownStatus ?? currentSnapshot.teardownStatus,
            attachedViewCount: attachedViewCount ?? currentSnapshot.attachedViewCount,
            lastUpdatedAt: .now
        )
    }
}

#if canImport(GhosttyKit)
extension AlanGhosttySurfaceHandle: AlanGhosttyEventSurfaceHandle {
    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e {
        liveHost.keyTranslationMods(for: mods)
    }

    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        liveHost.sendKey(keyEvent)
    }

    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool {
        liveHost.keyIsBinding(keyEvent, flags: flags)
    }

    func sendProgrammaticText(_ text: String) {
        liveHost.sendProgrammaticText(text)
    }

    func sendPreedit(_ text: String?) {
        liveHost.sendPreedit(text)
    }

    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e) {
        liveHost.sendMousePosition(x: x, y: y, mods: mods)
    }

    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool {
        liveHost.sendMouseButton(state: state, button: button, mods: mods)
    }

    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t) {
        liveHost.sendMouseScroll(x: x, y: y, mods: mods)
    }

    func sendMousePressure(stage: UInt32, pressure: Double) {
        liveHost.sendMousePressure(stage: stage, pressure: pressure)
    }

    func readSelectionText() -> String? {
        liveHost.readSelectionText()
    }

    func hasSelection() -> Bool {
        liveHost.hasSelection()
    }

    func readText(in range: AlanTerminalBufferRange) -> String? {
        liveHost.readText(in: range)
    }

    func imeRect(in view: NSView) -> NSRect? {
        liveHost.imeRect(in: view)
    }

    func setSearchUpdateHandler(_ handler: ((AlanTerminalSearchEngineUpdate) -> Void)?) {
        liveHost.onSearchUpdate = handler
    }

    func setScrollbackUpdateHandler(_ handler: ((AlanTerminalScrollbackMetrics) -> Void)?) {
        liveHost.onScrollbackUpdate = handler
    }

    func startSearch() -> Bool {
        liveHost.performBindingAction("start_search")
    }

    func updateSearchQuery(_ query: String) -> Bool {
        liveHost.performBindingAction("search:\(query)")
    }

    func navigateSearch(_ direction: AlanTerminalSearchNavigationDirection) -> Bool {
        switch direction {
        case .next:
            return liveHost.performBindingAction("navigate_search:next")
        case .previous:
            return liveHost.performBindingAction("navigate_search:previous")
        }
    }

    func endSearch() -> Bool {
        liveHost.performBindingAction("end_search")
    }

    func scrollTo(row: Int) -> Bool {
        liveHost.performBindingAction("scroll_to_row:\(row)")
    }
}
#endif

#endif
