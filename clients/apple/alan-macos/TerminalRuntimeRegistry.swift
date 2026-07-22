import SwiftUI

#if os(macOS)
@MainActor
protocol TerminalRuntimeHandle: AnyObject {
    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult
    func teardownTerminalRuntime()
}

@MainActor
final class MockTerminalRuntimeHandle: TerminalRuntimeHandle {
    private(set) var attachedCount = 0
    private(set) var detachedCount = 0
    private(set) var teardownCount = 0
    private(set) var deliveredText: [String] = []
    var deliveryResult: TerminalRuntimeDeliveryResult?

    func attach() {
        attachedCount += 1
    }

    func detach() {
        detachedCount += 1
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        deliveredText.append(text)
        return deliveryResult ?? .accepted(byteCount: text.lengthOfBytes(using: .utf8))
    }

    func teardownTerminalRuntime() {
        teardownCount += 1
    }
}

struct TerminalContentMount: Equatable {
    let contentID: String
    let paneSlotID: String
    let tabID: String
    let spaceID: String

    init(contentID: String, paneSlotID: String, tabID: String, spaceID: String) {
        self.contentID = contentID
        self.paneSlotID = paneSlotID
        self.tabID = tabID
        self.spaceID = spaceID
    }

    init(pane: ShellPane) {
        self.init(
            contentID: pane.terminalContentID,
            paneSlotID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID
        )
    }
}

enum TerminalHostAttachmentPolicy: Equatable {
    case immediate
    case deferUntilWindowAttached
}

@MainActor
final class TerminalRuntimeRegistry: ObservableObject {
    typealias MockDeliveryHandler = (String, String) -> TerminalRuntimeDeliveryResult

    private struct PendingShellProjection {
        let snapshot: TerminalHostRuntimeSnapshot
        let observer: (TerminalHostRuntimeSnapshot) -> Void
    }

    private var hostViewsByContentID: [String: AlanTerminalHostNSView] = [:]
    private var snapshotsByContentID: [String: TerminalHostRuntimeSnapshot] = [:]
    private var activeTasksByContentID: [String: ShellTabActiveTaskState] = [:]
    private var paneSlotIDByContentID: [String: String] = [:]
    private var contentIDByPaneSlotID: [String: String] = [:]
    private var pendingFocusPaneSlotIDs: Set<String> = []
    private var pendingShellProjectionsByContentID: [String: PendingShellProjection] = [:]
    private var shellProjectionFlushScheduled = false
    private let runtimeService: AlanTerminalRuntimeService
    private let mockDeliveryHandler: MockDeliveryHandler?
    private let performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?

    init(
        runtimeService: AlanTerminalRuntimeService? = nil,
        mockDeliveryHandler: MockDeliveryHandler? = nil,
        performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil
    ) {
        self.runtimeService = runtimeService ?? AlanWindowTerminalRuntimeService()
        self.mockDeliveryHandler = mockDeliveryHandler
        self.performanceDiagnosticsRecorder = performanceDiagnosticsRecorder
    }

    func hostView(
        for pane: ShellPane?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        renderPriority: TerminalRuntimeRenderPriority = .foregroundInteractive,
        activationDelegate: TerminalHostActivationDelegate?,
        attachmentPolicy: TerminalHostAttachmentPolicy = .immediate,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onClearRestoredTranscript: (() -> Void)? = nil,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) -> AlanTerminalHostNSView {
        hostView(
            forTerminalContent: pane.map(TerminalContentMount.init(pane:)),
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            renderPriority: renderPriority,
            activationDelegate: activationDelegate,
            attachmentPolicy: attachmentPolicy,
            onShellAction: onShellAction,
            onClearRestoredTranscript: onClearRestoredTranscript,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }

    func hostView(
        forTerminalContent mount: TerminalContentMount?,
        pane: ShellPane?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        renderPriority: TerminalRuntimeRenderPriority = .foregroundInteractive,
        activationDelegate: TerminalHostActivationDelegate?,
        attachmentPolicy: TerminalHostAttachmentPolicy = .immediate,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onClearRestoredTranscript: (() -> Void)? = nil,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) -> AlanTerminalHostNSView {
        let hostView = mount.flatMap { hostViewsByContentID[$0.contentID] }
            ?? AlanTerminalHostNSView()

        configureHostView(
            hostView,
            forTerminalContent: mount,
            pane: pane,
            bootProfile: bootProfile,
            isSelected: isSelected,
            renderPriority: renderPriority,
            activationDelegate: activationDelegate,
            attachmentPolicy: attachmentPolicy,
            onShellAction: onShellAction,
            onClearRestoredTranscript: onClearRestoredTranscript,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
        return hostView
    }

    func configureHostView(
        _ hostView: AlanTerminalHostNSView,
        forTerminalContent mount: TerminalContentMount?,
        pane: ShellPane?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        renderPriority: TerminalRuntimeRenderPriority = .foregroundInteractive,
        activationDelegate: TerminalHostActivationDelegate?,
        attachmentPolicy: TerminalHostAttachmentPolicy = .immediate,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onClearRestoredTranscript: (() -> Void)? = nil,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) {
        let surfaceHandle: AlanTerminalSurfaceHandle?
        if let mount {
            registerHostView(hostView, contentID: mount.contentID, paneSlotID: mount.paneSlotID)
            surfaceHandle = runtimeService.surfaceHandle(
                forTerminalContentID: mount.contentID,
                mountedAtPaneID: mount.paneSlotID,
                bootProfile: bootProfile
            )
        } else {
            unregisterHostView(hostView)
            surfaceHandle = nil
        }

        hostView.configure(
            pane: pane,
            terminalContentID: mount?.contentID,
            bootProfile: bootProfile,
            isSelected: isSelected,
            renderPriority: renderPriority,
            surfaceHandle: surfaceHandle,
            activationDelegate: activationDelegate,
            attachmentPolicy: attachmentPolicy,
            onShellAction: onShellAction,
            onClearRestoredTranscript: onClearRestoredTranscript,
            onCloseRequest: onCloseRequest,
            onRuntimeUpdate: onRuntimeUpdate,
            onMetadataUpdate: onMetadataUpdate
        )
    }

    func surfaceHandle(
        for pane: ShellPane?,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle? {
        guard let pane else { return nil }
        return surfaceHandle(
            forTerminalContent: TerminalContentMount(pane: pane),
            bootProfile: bootProfile
        )
    }

    func surfaceHandle(
        forTerminalContent mount: TerminalContentMount,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalSurfaceHandle {
        recordMount(contentID: mount.contentID, paneSlotID: mount.paneSlotID)
        return runtimeService.surfaceHandle(
            forTerminalContentID: mount.contentID,
            mountedAtPaneID: mount.paneSlotID,
            bootProfile: bootProfile
        )
    }

    /// Records the runtime snapshot and reports whether its shell-facing
    /// projection changed. The registry remains authoritative even when the
    /// shell projection is suppressed as timestamp-only churn.
    @discardableResult
    func updateSnapshot(_ snapshot: TerminalHostRuntimeSnapshot) -> Bool {
        guard let contentID = snapshot.contentID ?? snapshot.paneID.map(terminalContentID(forPaneID:)) else {
            return false
        }
        let previous = snapshotsByContentID[contentID]
            ?? runtimeSnapshot(from: runtimeService.snapshot(forTerminalContentID: contentID))
        if let paneID = snapshot.paneID {
            recordMount(contentID: contentID, paneSlotID: paneID)
        }
        snapshotsByContentID[contentID] = snapshot
        runtimeService
            .existingSurfaceHandle(forTerminalContentID: contentID)?
            .updateHostRuntimeSnapshot(snapshot)
        return TerminalRuntimePublicationPolicy.shouldProjectToShell(
            previous: previous,
            next: snapshot
        )
    }

    /// Publishes immediately for foreground/hidden runtimes and coalesces
    /// visible-background churn by terminal content identity for one frame.
    func publishShellProjection(
        _ snapshot: TerminalHostRuntimeSnapshot,
        observer: @escaping (TerminalHostRuntimeSnapshot) -> Void
    ) {
        guard snapshot.renderPriority == .visibleBackground,
              let contentID = snapshot.contentID
                  ?? snapshot.paneID.map(terminalContentID(mountedAtPaneID:))
        else {
            observer(snapshot)
            return
        }

        pendingShellProjectionsByContentID[contentID] = PendingShellProjection(
            snapshot: snapshot,
            observer: observer
        )
        guard !shellProjectionFlushScheduled else { return }
        shellProjectionFlushScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(16)) { [weak self] in
            self?.flushShellProjections()
        }
    }

    func flushShellProjections() {
        let pending = pendingShellProjectionsByContentID
        pendingShellProjectionsByContentID.removeAll()
        shellProjectionFlushScheduled = false
        for projection in pending.values.sorted(
            by: { ($0.snapshot.paneID ?? "") < ($1.snapshot.paneID ?? "") }
        ) {
            projection.observer(projection.snapshot)
        }
    }

    func updateRenderPriorities(
        _ prioritiesByContentID: [String: TerminalRuntimeRenderPriority]
    ) {
        let knownContentIDs = registeredContentIDs
            .union(paneSlotIDByContentID.keys)
            .union(hostViewsByContentID.keys)
        knownContentIDs.forEach { contentID in
            let priority = prioritiesByContentID[contentID] ?? .hiddenBackground
            let handle = runtimeService.existingSurfaceHandle(forTerminalContentID: contentID)
            let previousPriority = handle?.renderPriority ?? .hiddenBackground
            handle?.updateRenderPriority(
                priority,
                forceCatchUp: previousPriority == .hiddenBackground && priority.isVisible
            )
            guard priority != previousPriority else { return }
            recordPerformanceDiagnostic(
                .runtimePriorityChange,
                contentID: contentID,
                priority: priority
            )
            if priority.isVisible != previousPriority.isVisible {
                recordPerformanceDiagnostic(
                    .runtimeVisibilityChange,
                    contentID: contentID,
                    priority: priority
                )
            }
        }
    }

    private func recordPerformanceDiagnostic(
        _ kind: AlanPerformanceDiagnosticEventKind,
        contentID: String,
        priority: TerminalRuntimeRenderPriority
    ) {
        if let performanceDiagnosticsRecorder {
            guard performanceDiagnosticsRecorder.isEnabled else { return }
        } else {
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else { return }
        }
        let paneID = paneSlotIDByContentID[contentID]
        let event = AlanPerformanceDiagnosticEvent(
            kind: kind,
            durationMs: 0,
            paneID: paneID,
            contentID: contentID,
            priority: priority.diagnosticsValue,
            visibility: priority.diagnosticsVisibility,
            thread: Thread.isMainThread ? "main" : "background"
        )
        if let performanceDiagnosticsRecorder {
            performanceDiagnosticsRecorder.record(event)
        } else {
            AlanPerformanceDiagnosticsController.shared.record(
                kind,
                paneID: event.paneID,
                contentID: event.contentID,
                priority: event.priority,
                visibility: event.visibility,
                thread: event.thread
            )
        }
    }

    func snapshot(for paneID: String?) -> TerminalHostRuntimeSnapshot {
        guard let paneID else { return .placeholder }
        return snapshot(forTerminalContentID: terminalContentID(mountedAtPaneID: paneID))
    }

    func snapshot(forTerminalContentID contentID: String?) -> TerminalHostRuntimeSnapshot {
        guard let contentID else { return .placeholder }
        return snapshotsByContentID[contentID]
            ?? runtimeSnapshot(from: runtimeService.snapshot(forTerminalContentID: contentID))
    }

    @discardableResult
    func recordActiveTask(
        _ activeTaskState: ShellTabActiveTaskState?,
        processExited: Bool,
        forPaneID paneID: String
    ) -> Bool {
        let nextState: ShellTabActiveTaskState?
        if processExited {
            nextState = .inactive
        } else {
            nextState = activeTaskState
        }

        guard let nextState else { return false }
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        guard activeTasksByContentID[contentID] != nextState else { return false }
        activeTasksByContentID[contentID] = nextState
        return true
    }

    func strongestActiveTask(forPaneIDs paneIDs: [String]) -> ShellTabActiveTaskState? {
        paneIDs
            .compactMap { activeTasksByContentID[terminalContentID(mountedAtPaneID: $0)] }
            .max { Self.activeTaskRank($0) < Self.activeTaskRank($1) }
    }

    func captureTranscriptSnapshot(
        forTerminalContentID contentID: String
    ) -> TerminalTranscriptCaptureResult {
        runtimeService.captureTranscriptSnapshot(forTerminalContentID: contentID)
    }

    func requestGracefulShutdown(
        forTerminalContentID contentID: String,
        reason: TerminalRuntimeGracefulShutdownReason
    ) -> TerminalRuntimeGracefulShutdownRequestResult {
        runtimeService.requestGracefulShutdown(
            forTerminalContentID: contentID,
            reason: reason
        )
    }

    func seedRestoredTranscriptSnapshot(
        _ snapshot: TerminalTranscriptSnapshot,
        forTerminalContentID contentID: String
    ) {
        runtimeService.seedRestoredTranscriptSnapshot(snapshot, forTerminalContentID: contentID)
    }

    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) {
        runtimeService.clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
    }

    func releaseRuntimes(excluding activePaneIDs: Set<String>) {
        let activeContentIDs = Set(activePaneIDs.map { terminalContentID(mountedAtPaneID: $0) })
        releaseRuntimes(excludingTerminalContentIDs: activeContentIDs)
    }

    func releaseRuntimes(excluding activeMounts: [TerminalContentMount]) {
        activeMounts.forEach { mount in
            recordMount(contentID: mount.contentID, paneSlotID: mount.paneSlotID)
        }
        let activeContentIDs = Set(activeMounts.map(\.contentID))
        releaseRuntimes(excludingTerminalContentIDs: activeContentIDs)
    }

    private func releaseRuntimes(excludingTerminalContentIDs activeContentIDs: Set<String>) {
        let trackedContentIDs = registeredContentIDs.union(paneSlotIDByContentID.keys)
        let staleContentIDs = trackedContentIDs.subtracting(activeContentIDs)
        staleContentIDs.forEach { releaseTerminalContent($0) }
    }

    func releaseRuntime(for paneID: String) {
        releaseTerminalContent(terminalContentID(mountedAtPaneID: paneID))
    }

    func releaseAllRuntimes() {
        registeredContentIDs.forEach { releaseTerminalContent($0) }
    }

    func sendText(to paneID: String, text: String) -> TerminalRuntimeDeliveryResult {
        sendText(toTerminalContentID: terminalContentID(mountedAtPaneID: paneID), text: text)
    }

    func sendText(
        toTerminalContentID contentID: String,
        text: String
    ) -> TerminalRuntimeDeliveryResult {
        if let mockDeliveryHandler {
            return mockDeliveryHandler(contentID, text)
        }

        return runtimeService.sendText(toTerminalContentID: contentID, text: text)
    }

    func sendKey(to paneID: String, key: TerminalRuntimeControlKey) -> TerminalRuntimeDeliveryResult {
        sendKey(toTerminalContentID: terminalContentID(mountedAtPaneID: paneID), key: key)
    }

    func sendKey(
        toTerminalContentID contentID: String,
        key: TerminalRuntimeControlKey
    ) -> TerminalRuntimeDeliveryResult {
        runtimeService.sendKey(toTerminalContentID: contentID, key: key)
    }

    func terminalCommandRuntimeState(for paneID: String) -> ShellTerminalCommandRuntimeState {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.terminalCommandRuntimeState
        }

        let surfaceHandle = runtimeService.existingSurfaceHandle(forTerminalContentID: contentID)
        let selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine
        let searchEngine = surfaceHandle as? AlanTerminalSearchEngine
        let snapshot = snapshotsByContentID[contentID]
        return ShellTerminalCommandRuntimeState(
            paneID: paneID,
            hasSelection: selectionEngine?.hasSelection() ?? false,
            inputReady: surfaceHandle?.isSurfaceReady ?? snapshot?.surfaceState.inputReady ?? false,
            searchAvailable: searchEngine != nil,
            hasReliableSemanticCommands: snapshot?.surfaceState.semanticCommands.hasReliableCommandBoundaries ?? false
        )
    }

    @discardableResult
    func copySelection(for paneID: String) -> Bool {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.copySelection()
        }
        return copySelection(
            for: paneID,
            to: AlanTerminalSystemPasteboardWriter(pasteboard: .general)
        )
    }

    @discardableResult
    func copySelection(for paneID: String, to writer: AlanTerminalPasteboardWriting) -> Bool {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.copySelection(to: writer)
        }
        guard let selectionEngine = runtimeService
            .existingSurfaceHandle(forTerminalContentID: contentID) as? AlanTerminalSelectionEngine,
              let selectedText = selectionEngine.readSelectionText(),
              !selectedText.isEmpty
        else {
            return false
        }
        return writer.writeString(selectedText)
    }

    @discardableResult
    func pasteText(_ text: String, to paneID: String) -> TerminalRuntimeDeliveryResult {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            return hostView.pasteText(text)
        }
        return sendText(to: paneID, text: text)
    }

    @discardableResult
    func beginFindInteraction(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .beginFindInteraction() ?? false
    }

    @discardableResult
    func beginLastCommandOutputSearch(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .beginLastCommandOutputSearch() ?? false
    }

    @discardableResult
    func navigateSemanticPrompt(
        for paneID: String,
        direction: AlanTerminalPromptNavigationDirection
    ) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .navigateSemanticPrompt(direction) ?? false
    }

    @discardableResult
    func copyLastCommandOutput(for paneID: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .copyLastCommandOutput() ?? false
    }

    @discardableResult
    func updateFindQuery(for paneID: String, query: String) -> Bool {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .updateFindQuery(query) ?? false
    }

    func selectNextFindMatch(for paneID: String) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?.selectNextFindMatch()
    }

    func selectPreviousFindMatch(for paneID: String) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?.selectPreviousFindMatch()
    }

    func dismissFindInteraction(for paneID: String, refocusTerminal: Bool = true) {
        hostViewsByContentID[terminalContentID(mountedAtPaneID: paneID)]?
            .dismissFindInteraction(refocusTerminal: refocusTerminal)
    }

    func requestFocus(for paneID: String, retryBudget: Int = 0) {
        let contentID = terminalContentID(mountedAtPaneID: paneID)
        if let hostView = hostViewsByContentID[contentID] {
            hostView.focusTerminal()
            return
        }
        guard retryBudget > 0 else { return }
        pendingFocusPaneSlotIDs.insert(paneID)
    }

    var registeredContentIDs: Set<String> {
        Set(hostViewsByContentID.keys)
            .union(snapshotsByContentID.keys)
            .union(runtimeService.registeredContentIDs)
    }

    var registeredPaneIDs: Set<String> {
        Set(paneSlotIDByContentID.values)
            .union(snapshotsByContentID.values.compactMap(\.paneID))
            .union(runtimeService.registeredPaneIDs)
    }

    var renderCoordinatorMetrics: TerminalRenderCoordinatorMetrics? {
        runtimeService.renderCoordinatorMetrics
    }

    private func releaseTerminalContent(_ contentID: String) {
        let paneSlotID = paneSlotIDByContentID[contentID]
            ?? snapshotsByContentID[contentID]?.paneID
        if let hostView = hostViewsByContentID.removeValue(forKey: contentID) {
            hostView.teardownTerminalRuntime()
        }
        runtimeService.finalizeTerminalContent(contentID)
        snapshotsByContentID.removeValue(forKey: contentID)
        activeTasksByContentID.removeValue(forKey: contentID)
        pendingShellProjectionsByContentID.removeValue(forKey: contentID)
        if let paneSlotID {
            pendingFocusPaneSlotIDs.remove(paneSlotID)
        }
        if let paneSlotID = paneSlotIDByContentID.removeValue(forKey: contentID),
           contentIDByPaneSlotID[paneSlotID] == contentID
        {
            contentIDByPaneSlotID.removeValue(forKey: paneSlotID)
        }
    }

    private func terminalContentID(forPaneID paneID: String) -> String {
        ShellContentInstance.terminalContentID(forPaneID: paneID)
    }

    private func terminalContentID(mountedAtPaneID paneID: String) -> String {
        contentIDByPaneSlotID[paneID] ?? terminalContentID(forPaneID: paneID)
    }

    private func registerHostView(
        _ hostView: AlanTerminalHostNSView,
        contentID: String,
        paneSlotID: String
    ) {
        unregisterHostView(hostView, excludingContentID: contentID)
        recordMount(contentID: contentID, paneSlotID: paneSlotID)
        hostViewsByContentID[contentID] = hostView
        if pendingFocusPaneSlotIDs.remove(paneSlotID) != nil {
            hostView.focusTerminal()
        }
    }

    private func unregisterHostView(
        _ hostView: AlanTerminalHostNSView,
        excludingContentID retainedContentID: String? = nil
    ) {
        let staleContentIDs = hostViewsByContentID.compactMap { contentID, registeredHostView in
            registeredHostView === hostView && contentID != retainedContentID ? contentID : nil
        }
        staleContentIDs.forEach { hostViewsByContentID.removeValue(forKey: $0) }
    }

    private func recordMount(contentID: String, paneSlotID: String) {
        if let previousPaneSlotID = paneSlotIDByContentID[contentID],
           previousPaneSlotID != paneSlotID,
           contentIDByPaneSlotID[previousPaneSlotID] == contentID
        {
            contentIDByPaneSlotID.removeValue(forKey: previousPaneSlotID)
        }
        if let previousContentID = contentIDByPaneSlotID[paneSlotID],
           previousContentID != contentID
        {
            paneSlotIDByContentID.removeValue(forKey: previousContentID)
        }
        paneSlotIDByContentID[contentID] = paneSlotID
        contentIDByPaneSlotID[paneSlotID] = contentID
    }

    private static func activeTaskRank(_ state: ShellTabActiveTaskState) -> Int {
        switch state {
        case .inactive:
            return 0
        case .unknown:
            return 1
        case .foregroundCommand:
            return 2
        case .alanRunning:
            return 3
        case .alanProcess:
            return 4
        case .alanPendingYield:
            return 5
        }
    }

    private func runtimeSnapshot(
        from surfaceSnapshot: AlanTerminalSurfaceSnapshot?
    ) -> TerminalHostRuntimeSnapshot {
        guard let surfaceSnapshot else { return .placeholder }
        return TerminalHostRuntimeSnapshot(
            stage: .scaffold,
            contentID: surfaceSnapshot.contentID,
            paneID: surfaceSnapshot.paneID,
            tabID: nil,
            renderPriority: .hiddenBackground,
            logicalSize: .zero,
            backingSize: .zero,
            displayName: nil,
            displayID: nil,
            attachedWindowTitle: nil,
            isFocused: false,
            renderer: surfaceSnapshot.renderer,
            paneMetadata: surfaceSnapshot.metadata,
            surfaceState: .placeholder,
            lastUpdatedAt: surfaceSnapshot.lastUpdatedAt
        )
    }
}
#endif
