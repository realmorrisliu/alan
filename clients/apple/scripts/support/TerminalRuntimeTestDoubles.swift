#if os(macOS)
import AppKit
import Foundation

@MainActor
final class FakeAlanGhosttyProcessBootstrap: AlanGhosttyProcessBootstrap {
    private(set) var ensureCallCount = 0
    var nextDiagnostics: AlanGhosttyBootstrapDiagnostics

    init(
        nextDiagnostics: AlanGhosttyBootstrapDiagnostics = AlanGhosttyBootstrapDiagnostics(
            phase: .ready,
            summary: "Fake Ghostty bootstrap ready.",
            detail: nil,
            failureReason: nil,
            dependencies: GhosttyIntegrationStatus.discover(),
            lastUpdatedAt: .now
        )
    ) {
        self.nextDiagnostics = nextDiagnostics
        self.cachedDiagnostics = .pending(dependencies: nextDiagnostics.dependencies)
    }

    private var cachedDiagnostics: AlanGhosttyBootstrapDiagnostics

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        cachedDiagnostics
    }

    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        if cachedDiagnostics.phase == .ready || cachedDiagnostics.phase == .failed {
            return cachedDiagnostics
        }
        ensureCallCount += 1
        cachedDiagnostics = nextDiagnostics
        return cachedDiagnostics
    }
}

@MainActor
final class FakeAlanTerminalPtyRuntime: AlanTerminalPtyRuntime {
    private var handlesByContentID: [String: FakeAlanTerminalPtyHandle] = [:]

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        if let existing = handlesByContentID[contentID] {
            return existing
        }
        let handle = FakeAlanTerminalPtyHandle(
            contentID: contentID,
            bootRequest: bootRequest
        )
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingHandle(forTerminalContentID contentID: String) -> AlanTerminalPtyHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalPtyRuntimeSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func unregisterHandle(forTerminalContentID contentID: String) {
        handlesByContentID.removeValue(forKey: contentID)
    }
}

@MainActor
final class FakeAlanTerminalPtyHandle: AlanTerminalPtyHandle {
    let contentID: String
    let bootRequest: AlanTerminalBootRequest
    private(set) var deliveredText: [String] = []
    private(set) var resizeRequests: [AlanTerminalPtyDimensions] = []
    private(set) var signalRequests: [AlanTerminalPtySignal] = []
    private(set) var phase: AlanTerminalPtyRuntimePhase = .running
    private(set) var inputClosed = false
    private(set) var exitStatus: AlanTerminalProcessExitStatus?
    private var transcriptRingBufferLines: [String] = []

    init(contentID: String, bootRequest: AlanTerminalBootRequest) {
        self.contentID = contentID
        self.bootRequest = bootRequest
    }

    var snapshot: AlanTerminalPtyRuntimeSnapshot {
        AlanTerminalPtyRuntimeSnapshot(
            contentID: contentID,
            bootRequest: bootRequest,
            phase: phase,
            dimensions: resizeRequests.last,
            acceptedInputBytes: deliveredText.reduce(0) {
                $0 + $1.lengthOfBytes(using: .utf8)
            },
            inputClosed: inputClosed,
            lastSignal: signalRequests.last,
            exitStatus: exitStatus,
            transcriptLines: transcriptRingBufferLines
        )
    }

    var isInputReady: Bool {
        exitStatus == nil && !inputClosed
    }

    func writeInput(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard exitStatus == nil else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: phase.rawValue
            )
        }
        guard !inputClosed else {
            return .rejected(
                errorCode: "terminal_pty_input_closed",
                errorMessage: "The terminal PTY input stream is closed.",
                runtimePhase: phase.rawValue
            )
        }
        deliveredText.append(text)
        return .accepted(
            byteCount: text.lengthOfBytes(using: .utf8),
            runtimePhase: phase.rawValue
        )
    }

    func resize(columns: Int, rows: Int) -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        let dimensions = AlanTerminalPtyDimensions(
            columns: max(0, columns),
            rows: max(0, rows)
        )
        resizeRequests.append(dimensions)
        return .accepted("resized")
    }

    func closeInput() -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        inputClosed = true
        phase = .inputClosed
        return .accepted("input_closed")
    }

    func sendSignal(_ signal: AlanTerminalPtySignal) -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else {
            return .rejected(
                "terminal_child_exited",
                message: "The terminal process has exited."
            )
        }
        signalRequests.append(signal)
        return .accepted(signal.rawValue)
    }

    func makeRendererAttachment() -> AlanTerminalPtyRendererAttachmentResult {
        .rejected(
            .rejected(
                "terminal_renderer_attachment_unsupported",
                message: "The fake PTY runtime does not expose renderer file descriptors."
            )
        )
    }

    func terminateForCleanup() -> AlanTerminalPtyOperationResult {
        guard exitStatus == nil else { return .accepted("already_exited") }
        inputClosed = true
        phase = .exited
        exitStatus = .unknown
        signalRequests.append(.terminate)
        return .accepted("terminated")
    }

    func recordTranscriptOutput(_ text: String) {
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    func markExited(_ status: AlanTerminalProcessExitStatus) {
        exitStatus = status
        phase = .exited
    }
}

@MainActor
final class FakeAlanTerminalSurfaceHandle: AlanTerminalSurfaceHandle {
    let contentID: String
    private(set) var paneID: String
    private(set) var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    private(set) var configureCount = 0
    private(set) var attachCount = 0
    private(set) var detachCount = 0
    private(set) var teardownCount = 0
    private(set) var renderCatchUpRequestCount = 0
    private(set) var deliveredText: [String] = []
    private(set) var deliveredKeys: [TerminalRuntimeControlKey] = []
    private(set) var gracefulShutdownRequests: [TerminalRuntimeGracefulShutdownReason] = []
    private(set) var searchActions: [String] = []
    private(set) var scrollActions: [String] = []
    var deliveryResult: TerminalRuntimeDeliveryResult?
    var onGracefulShutdownRequest: ((TerminalRuntimeGracefulShutdownReason) -> Void)?
    var searchActionsShouldSucceed = true
    var scrollActionsShouldSucceed = true
    var terminalDimensionsOverride: AlanTerminalPtyDimensions?
    var commandOutputTextByRange: [AlanTerminalBufferRange: String] = [:]
    private(set) var captureTranscriptTextRanges: [AlanTerminalBufferRange] = []
    var selectedText: String?
    var ready = true
    private(set) var seededTranscriptSnapshot: TerminalTranscriptSnapshot?
    private(set) var transcriptRingBufferLines: [String] = []
    private var latestHostRuntime: TerminalHostRuntimeSnapshot?
    private var diagnosticsChangeHandler: ((TerminalRendererSnapshot) -> Void)?
    private var searchUpdateHandler: ((AlanTerminalSearchEngineUpdate) -> Void)?
    private var scrollbackUpdateHandler: ((AlanTerminalScrollbackMetrics) -> Void)?
    private var closeRequestHandler: ((Bool) -> Void)?
    private var currentSnapshot: AlanTerminalSurfaceSnapshot

    init(contentID: String, paneID: String) {
        self.contentID = contentID
        self.paneID = paneID
        self.currentSnapshot = .pending(contentID: contentID, paneID: paneID)
    }

    convenience init(paneID: String) {
        self.init(contentID: ShellContentInstance.terminalContentID(forPaneID: paneID), paneID: paneID)
    }

    var snapshot: AlanTerminalSurfaceSnapshot {
        currentSnapshot
    }

    var isSurfaceReady: Bool {
        ready && currentSnapshot.teardownStatus != .completed
    }

    var latestHostRuntimeSnapshot: TerminalHostRuntimeSnapshot? {
        latestHostRuntime
    }

    var fallbackTranscriptLines: [String] {
        transcriptRingBufferLines
    }

    var terminalDimensions: AlanTerminalPtyDimensions? {
        terminalDimensionsOverride
    }

    func configure(mountedAtPaneID paneID: String, bootProfile: AlanShellBootProfile?) {
        self.paneID = paneID
        configureCount += 1
        updateSnapshot(lifecyclePhase: bootProfile == nil ? .pending : .attachable)
    }

    func updateRenderPriority(
        _ priority: TerminalRuntimeRenderPriority,
        forceCatchUp: Bool
    ) {
        renderPriority = priority
        if forceCatchUp {
            renderCatchUpRequestCount += 1
        }
    }

    func attach(
        to canvasView: NSView,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        updateRenderPriority(renderPriority, forceCatchUp: false)
        attachCount += 1
        diagnosticsChangeHandler = onDiagnosticsChange
        closeRequestHandler = onCloseRequest
        updateSnapshot(lifecyclePhase: .attached, attachedViewCount: 1)
        onDiagnosticsChange(currentSnapshot.renderer)
        onMetadataChange(currentSnapshot.metadata)
    }

    func detach() {
        detachCount += 1
        diagnosticsChangeHandler = nil
        closeRequestHandler = nil
        updateSnapshot(attachedViewCount: 0)
    }

    func emitDiagnosticsSnapshot(_ snapshot: TerminalRendererSnapshot) {
        updateSnapshot(renderer: snapshot)
        diagnosticsChangeHandler?(snapshot)
    }

    func updateHostRuntimeSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) {
        latestHostRuntime = snapshot
    }

    func captureTranscriptText(in range: AlanTerminalBufferRange) -> String? {
        captureTranscriptTextRanges.append(range)
        return commandOutputTextByRange[range]
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

    func recordTranscriptOutput(_ text: String) {
        transcriptRingBufferLines.append(contentsOf: transcriptLines(from: text))
        if transcriptRingBufferLines.count > TerminalTranscriptSnapshot.defaultMaxRows {
            transcriptRingBufferLines = Array(
                transcriptRingBufferLines.suffix(TerminalTranscriptSnapshot.defaultMaxRows)
            )
        }
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !currentSnapshot.metadata.processExited else {
            let result = TerminalRuntimeDeliveryResult.rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        guard isSurfaceReady else {
            let result = TerminalRuntimeDeliveryResult.unavailable(
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        deliveredText.append(text)
        let result = deliveryResult
            ?? .accepted(
                byteCount: text.lengthOfBytes(using: .utf8),
                runtimePhase: currentSnapshot.runtimePhase
            )
        updateSnapshot(lastDelivery: result)
        return result
    }

    func sendControlKey(_ key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        guard !currentSnapshot.metadata.processExited else {
            let result = TerminalRuntimeDeliveryResult.rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        guard isSurfaceReady else {
            let result = TerminalRuntimeDeliveryResult.unavailable(
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: currentSnapshot.runtimePhase
            )
            updateSnapshot(lastDelivery: result)
            return result
        }
        deliveredKeys.append(key)
        let result = deliveryResult
            ?? .accepted(byteCount: 0, runtimePhase: currentSnapshot.runtimePhase)
        updateSnapshot(lastDelivery: result)
        return result
    }

    func requestGracefulShutdown(
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        if currentSnapshot.metadata.processExited {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .alreadyExited,
                delivery: nil,
                message: "The terminal process has already exited."
            )
        }

        gracefulShutdownRequests.append(reason)
        let delivery = sendControlKey(.interrupt)
        let code: TerminalRuntimeGracefulShutdownRequestCode
        switch delivery.code {
        case .accepted, .queued:
            code = .requested
            onGracefulShutdownRequest?(reason)
        case .missingTarget:
            code = .missingRuntime
        case .unavailableRuntime, .timeout:
            code = .unavailable
        case .rejected:
            code = .rejected
        }
        return TerminalRuntimeGracefulShutdownRequestResult(
            contentID: contentID,
            reason: reason,
            code: code,
            delivery: delivery,
            message: delivery.errorMessage
        )
    }

    func markActiveTaskState(_ activeTaskState: ShellTabActiveTaskState?) {
        let metadata = TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: currentSnapshot.metadata.workingDirectory,
            summary: currentSnapshot.metadata.summary,
            attention: activeTaskState?.protectsFromPruning == true ? .active : .idle,
            processExited: currentSnapshot.metadata.processExited,
            lastCommandExitCode: currentSnapshot.metadata.lastCommandExitCode,
            lastUpdatedAt: .now,
            activeTaskState: activeTaskState
        )
        updateSnapshot(metadata: metadata)
    }

    func markProcessExited(exitCode: Int) {
        let metadata = TerminalPaneMetadataSnapshot(
            title: currentSnapshot.metadata.title,
            workingDirectory: currentSnapshot.metadata.workingDirectory,
            summary: "process exited",
            attention: .awaitingUser,
            processExited: true,
            lastCommandExitCode: exitCode,
            lastUpdatedAt: .now,
            activeTaskState: .inactive
        )
        updateSnapshot(metadata: metadata)
    }

    func requestClose(requiresConfirmation: Bool) {
        closeRequestHandler?(requiresConfirmation)
    }

    @discardableResult
    func teardown() -> AlanTerminalSurfaceTeardownStatus {
        guard currentSnapshot.teardownStatus != .completed else { return .completed }
        teardownCount += 1
        updateSnapshot(
            lifecyclePhase: .closed,
            teardownStatus: .completed,
            attachedViewCount: 0
        )
        return .completed
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

extension FakeAlanTerminalSurfaceHandle: AlanTerminalSearchEngine {
    func setSearchUpdateHandler(_ handler: ((AlanTerminalSearchEngineUpdate) -> Void)?) {
        searchUpdateHandler = handler
    }

    func startSearch() -> Bool {
        recordSearchAction("start_search")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.started(query: ""))
        return true
    }

    func updateSearchQuery(_ query: String) -> Bool {
        recordSearchAction("search:\(query)")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.started(query: query))
        return true
    }

    func navigateSearch(_ direction: AlanTerminalSearchNavigationDirection) -> Bool {
        switch direction {
        case .next:
            recordSearchAction("navigate_search:next")
        case .previous:
            recordSearchAction("navigate_search:previous")
        }
        return searchActionsShouldSucceed
    }

    func endSearch() -> Bool {
        recordSearchAction("end_search")
        guard searchActionsShouldSucceed else { return false }
        searchUpdateHandler?(.ended)
        return true
    }

    func emitSearchUpdate(_ update: AlanTerminalSearchEngineUpdate) {
        searchUpdateHandler?(update)
    }

    private func recordSearchAction(_ action: String) {
        searchActions.append(action)
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalScrollbackEngine {
    func setScrollbackUpdateHandler(_ handler: ((AlanTerminalScrollbackMetrics) -> Void)?) {
        scrollbackUpdateHandler = handler
    }

    func scrollTo(row: Int) -> Bool {
        scrollActions.append("scroll_to_row:\(row)")
        return scrollActionsShouldSucceed
    }

    func emitScrollbackUpdate(_ metrics: AlanTerminalScrollbackMetrics) {
        scrollbackUpdateHandler?(metrics)
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalSelectionEngine {
    func readSelectionText() -> String? {
        selectedText
    }

    func hasSelection() -> Bool {
        selectedText?.isEmpty == false
    }
}

extension FakeAlanTerminalSurfaceHandle: AlanTerminalCommandBufferEngine {
    func readText(in range: AlanTerminalBufferRange) -> String? {
        commandOutputTextByRange[range]
    }
}

@MainActor
final class FakeAlanTerminalRuntimeService: AlanTerminalRuntimeService {
    let bootstrap: FakeAlanGhosttyProcessBootstrap
    private(set) var handlesByContentID: [String: FakeAlanTerminalSurfaceHandle] = [:]
    private var restoredTranscriptSnapshotsByContentID: [String: TerminalTranscriptSnapshot] = [:]
    var renderCoordinatorMetricsOverride: TerminalRenderCoordinatorMetrics?

    init() {
        self.bootstrap = FakeAlanGhosttyProcessBootstrap()
    }

    init(bootstrap: FakeAlanGhosttyProcessBootstrap) {
        self.bootstrap = bootstrap
    }

    var diagnostics: AlanGhosttyBootstrapDiagnostics {
        bootstrap.diagnostics
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    var registeredPaneIDs: Set<String> {
        Set(handlesByContentID.values.map(\.paneID))
    }

    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? {
        renderCoordinatorMetricsOverride
    }

    @discardableResult
    func ensureReady() -> AlanGhosttyBootstrapDiagnostics {
        bootstrap.ensureReady()
    }

    func surfaceHandle(
        forTerminalContentID contentID: String,
        mountedAtPaneID paneID: String,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        ensureReady()
        if let handle = handlesByContentID[contentID] {
            handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
            if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
                handle.seedRestoredTranscriptSnapshot(restored)
            }
            return handle
        }
        let handle = FakeAlanTerminalSurfaceHandle(contentID: contentID, paneID: paneID)
        handle.configure(mountedAtPaneID: paneID, bootProfile: bootProfile)
        if let restored = restoredTranscriptSnapshotsByContentID[contentID] {
            handle.seedRestoredTranscriptSnapshot(restored)
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingSurfaceHandle(forTerminalContentID contentID: String) -> AlanTerminalSurfaceHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalSurfaceSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func captureTranscriptSnapshot(forTerminalContentID contentID: String) -> TerminalTranscriptCaptureResult {
        guard let handle = handlesByContentID[contentID] else {
            return .failed(
                TerminalTranscriptCaptureFailure(
                    contentID: contentID,
                    code: .missingRuntime,
                    message: "No fake terminal runtime is registered for this content."
                )
            )
        }
        return buildTerminalTranscriptCapture(for: handle)
    }

    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        guard let handle = handlesByContentID[contentID] else {
            return TerminalRuntimeGracefulShutdownRequestResult(
                contentID: contentID,
                reason: reason,
                code: .missingRuntime,
                delivery: nil,
                message: "No fake terminal runtime is registered for this content."
            )
        }
        return handle.requestGracefulShutdown(reason: reason)
    }

    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    ) {
        let bounded = snapshot.boundedForManifest()
        restoredTranscriptSnapshotsByContentID[contentID] = bounded
        handlesByContentID[contentID]?.seedRestoredTranscriptSnapshot(bounded)
    }

    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        handlesByContentID[contentID]?.clearRestoredTranscriptSnapshot()
    }

    func sendText(toTerminalContentID contentID: String, text: String) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a fake terminal runtime."
            )
        }
        return handle.sendControlText(text)
    }

    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult {
        guard let handle = handlesByContentID[contentID] else {
            return .missingTarget(
                errorMessage: "The requested terminal content does not have a fake terminal runtime."
            )
        }
        return handle.sendControlKey(key)
    }

    @discardableResult
    func finalizeTerminalContent(_ contentID: String) -> AlanTerminalSurfaceTeardownStatus {
        restoredTranscriptSnapshotsByContentID.removeValue(forKey: contentID)
        guard let handle = handlesByContentID.removeValue(forKey: contentID) else {
            return .notStarted
        }
        return handle.teardown()
    }

    func finalizeTerminalContents(excluding activeContentIDs: Set<String>) {
        let staleContentIDs = Set(handlesByContentID.keys).subtracting(activeContentIDs)
        staleContentIDs.forEach { finalizeTerminalContent($0) }
    }
}

#endif
