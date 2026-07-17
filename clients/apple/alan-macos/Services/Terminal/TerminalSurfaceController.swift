#if os(macOS)
import AppKit
import Foundation

#if canImport(GhosttyKit)
import GhosttyKit
#endif

@MainActor
final class AlanTerminalSurfaceController {
    let inputRouter = AlanTerminalInputRouter()
    let scrollbackAdapter = AlanTerminalScrollbackAdapter()
    let nativeScrollViewAdapter = AlanTerminalNativeScrollViewAdapter()
    let metadataAdapter = AlanTerminalMetadataAdapter()
    var onSearchStateChange: (() -> Void)?
    var onSurfaceStateChange: (() -> Void)?

    private(set) var searchAdapter: AlanTerminalSearchAdapter?
    private(set) var clipboardAdapter = AlanTerminalSelectionClipboardAdapter(surfaceHandle: nil)
    private weak var surfaceHandle: AlanTerminalSurfaceHandle?
    private weak var searchEngine: AlanTerminalSearchEngine?
    private weak var scrollbackEngine: AlanTerminalScrollbackEngine?
    private weak var commandBufferEngine: AlanTerminalCommandBufferEngine?
    private weak var selectionEngine: AlanTerminalSelectionEngine?
    private let performanceDiagnosticsRecorder: AlanPerformanceDiagnosticsRecorder?
    private var semanticCommandState = AlanTerminalSemanticCommandState.placeholder
    private var latestRenderer = TerminalRendererSnapshot.placeholder
    private var latestMetadata = TerminalPaneMetadataSnapshot.placeholder
    private var readonly = false
    private var secureInput = false
    private var nativeScrollRowHeight: CGFloat = 1

    init(diagnosticsRecorder: AlanPerformanceDiagnosticsRecorder? = nil) {
        self.performanceDiagnosticsRecorder = diagnosticsRecorder
        nativeScrollViewAdapter.onVisibleRowChange = { [weak self] row in
            self?.scrollToNativeRow(row)
        }
    }

    var isSurfaceReady: Bool {
        surfaceReadiness == .ready
    }

    var surfaceStateSnapshot: AlanTerminalSurfaceStateSnapshot {
        AlanTerminalSurfaceStateSnapshot(
            readiness: surfaceReadiness,
            terminalMode: scrollbackAdapter.state.metrics.mode,
            scrollback: scrollbackAdapter.state,
            search: searchAdapter?.state,
            semanticCommands: semanticCommandState,
            readonly: readonly,
            secureInput: secureInput,
            inputReady: isSurfaceReady,
            rendererHealth: latestRenderer.phase == .failed ? "failed" : latestRenderer.phase.rawValue,
            childExited: latestMetadata.processExited,
            lastUpdatedAt: .now
        )
    }

    func bind(surfaceHandle: AlanTerminalSurfaceHandle?, paneID: String?) {
        let surfaceChanged = self.surfaceHandle !== surfaceHandle
        if surfaceChanged {
            self.surfaceHandle?.detach()
            self.surfaceHandle = surfaceHandle
        }
        let nextSearchEngine = surfaceHandle as? AlanTerminalSearchEngine
        if searchEngine !== nextSearchEngine {
            searchEngine?.setSearchUpdateHandler(nil)
            searchEngine = nextSearchEngine
            searchEngine?.setSearchUpdateHandler { [weak self] update in
                self?.applySearchEngineUpdate(update)
            }
        }
        let nextScrollbackEngine = surfaceHandle as? AlanTerminalScrollbackEngine
        if scrollbackEngine !== nextScrollbackEngine {
            scrollbackEngine?.setScrollbackUpdateHandler(nil)
            scrollbackEngine = nextScrollbackEngine
            resetPerSurfaceStateForSurfaceChange()
            scrollbackEngine?.setScrollbackUpdateHandler { [weak self] metrics in
                self?.applyScrollbackMetrics(metrics)
            }
        } else if surfaceChanged {
            resetPerSurfaceStateForSurfaceChange()
        }
        commandBufferEngine = surfaceHandle as? AlanTerminalCommandBufferEngine
        selectionEngine = surfaceHandle as? AlanTerminalSelectionEngine
        clipboardAdapter.updateSurfaceHandle(surfaceHandle)
        if let paneID, searchAdapter?.state.paneID != paneID {
            searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
            semanticCommandState = .unavailable(
                paneID: paneID,
                reason: "Semantic command boundary signals are not available for this pane."
            )
        } else if paneID == nil {
            searchAdapter = nil
            semanticCommandState = .placeholder
        }
    }

    func attach(
        to canvasView: NSView,
        bootProfile: AlanShellBootProfile?,
        focused: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        onDiagnosticsChange: @escaping (TerminalRendererSnapshot) -> Void,
        onMetadataChange: @escaping (TerminalPaneMetadataSnapshot) -> Void,
        onCloseRequest: @escaping (Bool) -> Void
    ) {
        guard let surfaceHandle else { return }
        let attachStartedAt = performanceDiagnosticsStartTime()
        surfaceHandle.configure(mountedAtPaneID: surfaceHandle.paneID, bootProfile: bootProfile)
        surfaceHandle.updateRenderPriority(renderPriority, forceCatchUp: false)
        surfaceHandle.attach(
            to: canvasView,
            focused: focused,
            renderPriority: renderPriority,
            onDiagnosticsChange: { [weak self] snapshot in
                guard let self else { return }
                latestRenderer = snapshot
                if performanceDiagnosticsStartTime() != nil {
                    recordPerformanceDiagnostic(
                        .terminalRendererUpdate,
                        durationMs: 0,
                        counts: AlanPerformanceDiagnosticCounts(events: snapshot.recentEvents.count)
                    )
                }
                onDiagnosticsChange(snapshot)
            },
            onMetadataChange: { [weak self] metadata in
                guard let self else { return }
                latestMetadata = metadata
                onMetadataChange(metadata)
            },
            onCloseRequest: onCloseRequest
        )
        if let attachStartedAt {
            recordPerformanceDiagnostic(
                .terminalSurfaceAttach,
                durationMs: performanceDurationMs(since: attachStartedAt),
                priority: renderPriority
            )
        }
    }

    func detach() {
        surfaceHandle?.detach()
        surfaceHandle = nil
        searchEngine?.setSearchUpdateHandler(nil)
        searchEngine = nil
        scrollbackEngine?.setScrollbackUpdateHandler(nil)
        scrollbackEngine = nil
        commandBufferEngine = nil
        selectionEngine = nil
        semanticCommandState = .placeholder
        resetPerSurfaceStateForSurfaceChange()
        clipboardAdapter.updateSurfaceHandle(nil)
    }

    func updateRenderer(_ renderer: TerminalRendererSnapshot) {
        let updateStartedAt = performanceDiagnosticsStartTime()
        latestRenderer = renderer
        if let updateStartedAt {
            recordPerformanceDiagnostic(
                .terminalRendererUpdate,
                durationMs: performanceDurationMs(since: updateStartedAt),
                counts: AlanPerformanceDiagnosticCounts(events: renderer.recentEvents.count)
            )
        }
    }

    func updateMetadata(_ metadata: TerminalPaneMetadataSnapshot) {
        latestMetadata = metadata
    }

    func overlayState(
        renderer: TerminalRendererSnapshot,
        metadata: TerminalPaneMetadataSnapshot,
        bootProfile: AlanShellBootProfile?
    ) -> AlanTerminalOverlayState? {
        let readiness: AlanTerminalSurfaceReadiness
        if bootProfile == nil || surfaceHandle == nil {
            readiness = .unready(reason: .missingSurface)
        } else {
            readiness = surfaceReadiness
        }
        return metadataAdapter.overlayState(renderer: renderer, metadata: metadata, surface: readiness)
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return .accepted(byteCount: 0, runtimePhase: surfaceHandle?.snapshot.runtimePhase)
        }
        guard let surfaceHandle else {
            return .rejected(
                errorCode: "terminal_runtime_unavailable",
                errorMessage: "No service-owned terminal surface is attached to this host."
            )
        }
        guard !surfaceHandle.snapshot.metadata.processExited else {
            return .rejected(
                errorCode: "terminal_child_exited",
                errorMessage: "The terminal process has exited.",
                runtimePhase: surfaceHandle.snapshot.runtimePhase
            )
        }
        guard surfaceHandle.isSurfaceReady else {
            return .rejected(
                errorCode: "terminal_runtime_unavailable",
                errorMessage: "The requested pane is not ready to receive terminal input.",
                runtimePhase: surfaceHandle.snapshot.runtimePhase
            )
        }
        return surfaceHandle.sendControlText(text)
    }

    func syncNativeScrollView(viewportSize: CGSize) {
        let visibleRows = max(scrollbackAdapter.state.metrics.visibleRows, 1)
        let rowHeight = viewportSize.height / CGFloat(visibleRows)
        nativeScrollRowHeight = max(rowHeight, 1)
        nativeScrollViewAdapter.sync(
            state: scrollbackAdapter.state,
            viewportSize: viewportSize,
            rowHeight: nativeScrollRowHeight
        )
    }

    func routeScroll(_ input: AlanTerminalScrollInput) -> AlanTerminalScrollRoutingDecision {
        guard isSurfaceReady else { return .ignored }
        guard !scrollbackAdapter.shouldForwardScrollToTerminal() else { return .terminalScroll }
        guard scrollbackAdapter.shouldConsumeNativeScrollInput(input) else {
            scrollbackAdapter.resetPreciseScrollAccumulator()
            return .terminalScroll
        }
        guard let row = scrollbackAdapter.targetFirstVisibleRow(
            for: input,
            rowHeight: nativeScrollRowHeight
        ) else { return .ignored }
        guard scrollToNativeRow(row) else { return .terminalScroll }
        return .nativeScroll(row: row)
    }

    func routePointer(_ input: AlanTerminalPointerInput) -> AlanTerminalPointerRoutingDecision {
        inputRouter.routePointer(
            input,
            terminalMode: scrollbackAdapter.state.metrics.mode,
            surfaceReady: isSurfaceReady
        )
    }

    func routeKeyboard(
        _ input: AlanTerminalKeyInput,
        hasMarkedText: Bool
    ) -> AlanTerminalKeyboardRoutingDecision {
        inputRouter.routeKeyboard(input, hasMarkedText: hasMarkedText)
    }

    func routeLeftMouseDown(
        hitOwnsTerminal: Bool,
        commandSurfaceVisible: Bool,
        isFirstResponder: Bool,
        appIsActive: Bool,
        windowIsKey: Bool
    ) -> AlanTerminalLeftMouseDownRoutingDecision {
        inputRouter.routeLeftMouseDown(
            hitOwnsTerminal: hitOwnsTerminal,
            commandSurfaceVisible: commandSurfaceVisible,
            isFirstResponder: isFirstResponder,
            appIsActive: appIsActive,
            windowIsKey: windowIsKey
        )
    }

    func resetInputRouting() {
        inputRouter.reset()
    }

    @discardableResult
    private func scrollToNativeRow(_ row: Int) -> Bool {
        guard scrollbackEngine?.scrollTo(row: row) == true else { return false }
        scrollbackAdapter.scrollTo(firstVisibleRow: row)
        notifySurfaceStateChanged()
        return true
    }

    func copySelection(to pasteboard: NSPasteboard = .general) -> Bool {
        clipboardAdapter.writeSelectionToPasteboard(selectionEngine?.readSelectionText(), pasteboard: pasteboard)
    }

    func copySelection(to writer: AlanTerminalPasteboardWriting) -> Bool {
        clipboardAdapter.writeSelection(selectionEngine?.readSelectionText(), to: writer)
    }

    func paste(_ text: String) -> TerminalRuntimeDeliveryResult {
        clipboardAdapter.paste(text)
    }

    func readSelectionText() -> String? {
        selectionEngine?.readSelectionText()
    }

    func hasSelection() -> Bool {
        selectionEngine?.hasSelection() ?? false
    }

    func updateSemanticCommands(_ state: AlanTerminalSemanticCommandState) {
        semanticCommandState = state
        notifySurfaceStateChanged()
    }

    func invalidateSemanticCommands(reason: String) {
        semanticCommandState = AlanTerminalSemanticCommandState(
            paneID: semanticCommandState.paneID ?? searchAdapter?.state.paneID ?? surfaceHandle?.paneID,
            boundaryState: .stale(reason: reason),
            segments: semanticCommandState.segments,
            lastUpdatedAt: .now
        )
        notifySurfaceStateChanged()
    }

    var hasReliableSemanticCommandActions: Bool {
        scrollbackAdapter.state.metrics.mode == .normalBuffer
            && semanticCommandState.hasReliableCommandBoundaries
    }

    @discardableResult
    func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              semanticCommandState.hasReliablePromptMarks
        else {
            return false
        }

        let promptRows = semanticCommandState
            .reliableSegments
            .compactMap { $0.promptRange?.lowerBound }
            .sorted()
        guard !promptRows.isEmpty else { return false }

        let currentRow = scrollbackAdapter.state.metrics.firstVisibleRow
        let targetRow: Int?
        switch direction {
        case .previous:
            targetRow = promptRows.reversed().first { $0 < currentRow } ?? promptRows.last
        case .next:
            targetRow = promptRows.first { $0 > currentRow } ?? promptRows.first
        }

        guard let targetRow else { return false }
        return scrollToNativeRow(targetRow)
    }

    @discardableResult
    func copyLastCommandOutput(to writer: AlanTerminalPasteboardWriting) -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              let outputRange = semanticCommandState.lastReliableOutputRange
        else {
            return copySelection(to: writer)
        }
        if outputRange.isEmpty {
            return writer.writeString("")
        }
        guard let output = commandBufferEngine?.readText(in: outputRange) else {
            return copySelection(to: writer)
        }

        return writer.writeString(output)
    }

    @discardableResult
    func copyLastCommandOutput(to pasteboard: NSPasteboard = .general) -> Bool {
        copyLastCommandOutput(to: AlanTerminalSystemPasteboardWriter(pasteboard: pasteboard))
    }

    @discardableResult
    func beginLastCommandOutputSearch() -> Bool {
        guard scrollbackAdapter.state.metrics.mode == .normalBuffer,
              let outputRange = semanticCommandState.lastReliableOutputRange
        else {
            return beginSearch()
        }

        return beginSearch(scope: .commandOutput(outputRange))
    }

    @discardableResult
    func beginSearch(scope: AlanTerminalSearchScope = .scrollback) -> Bool {
        guard let paneID = searchAdapter?.state.paneID ?? surfaceHandle?.paneID else { return false }
        if searchAdapter == nil {
            searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
        }
        if searchAdapter?.state.isActive == true,
           searchAdapter?.state.scope == scope
        {
            searchAdapter?.requestFocus(scope: scope)
            notifySearchStateChanged()
            return true
        }
        guard searchEngine?.startSearch() == true else { return false }
        searchAdapter?.requestFocus(scope: scope)
        searchAdapter?.updateQuery(searchAdapter?.state.query ?? "")
        return true
    }

    @discardableResult
    func updateSearchQuery(_ query: String) -> Bool {
        let scope = searchAdapter?.state.isActive == true
            ? (searchAdapter?.state.scope ?? .scrollback)
            : .scrollback
        guard beginSearch(scope: scope) else { return false }
        guard searchEngine?.updateSearchQuery(query) == true else { return false }
        searchAdapter?.updateQuery(query)
        return true
    }

    func nextSearchMatch() {
        if searchEngine?.navigateSearch(.next) != true {
            searchAdapter?.next()
        }
    }

    func previousSearchMatch() {
        if searchEngine?.navigateSearch(.previous) != true {
            searchAdapter?.previous()
        }
    }

    func dismissSearch() {
        _ = searchEngine?.endSearch()
        searchAdapter?.dismiss()
    }

    private func applySearchEngineUpdate(_ update: AlanTerminalSearchEngineUpdate) {
        switch update {
        case .started(let query):
            guard let paneID = searchAdapter?.state.paneID ?? surfaceHandle?.paneID else { return }
            if searchAdapter == nil {
                searchAdapter = AlanTerminalSearchAdapter(paneID: paneID)
            }
            searchAdapter?.updateQuery(query)
        case .ended:
            searchAdapter?.dismiss()
        case .matches(let total):
            searchAdapter?.updateMatches(
                total: total,
                selectedIndex: searchAdapter?.state.selectedIndex
            )
        case .selected(let index):
            searchAdapter?.updateMatches(
                total: searchAdapter?.state.totalMatches,
                selectedIndex: index
            )
        }
        notifySearchStateChanged()
    }

    private func applyScrollbackMetrics(_ metrics: AlanTerminalScrollbackMetrics) {
        let updateStartedAt = performanceDiagnosticsStartTime()
        scrollbackAdapter.updateMetrics(metrics)
        notifySurfaceStateChanged()
        if let updateStartedAt {
            recordPerformanceDiagnostic(
                .terminalScrollbackUpdate,
                durationMs: performanceDurationMs(since: updateStartedAt),
                counts: AlanPerformanceDiagnosticCounts(lines: metrics.totalRows)
            )
        }
    }

    private func recordPerformanceDiagnostic(
        _ kind: AlanPerformanceDiagnosticEventKind,
        durationMs: Double,
        priority: TerminalRuntimeRenderPriority? = nil,
        counts: AlanPerformanceDiagnosticCounts? = nil
    ) {
        if let performanceDiagnosticsRecorder {
            guard performanceDiagnosticsRecorder.isEnabled else { return }
        } else {
            guard AlanPerformanceDiagnosticsController.shared.isEnabled else { return }
        }
        let resolvedPriority = priority ?? surfaceHandle?.renderPriority
        let event = AlanPerformanceDiagnosticEvent(
            kind: kind,
            durationMs: durationMs,
            paneID: surfaceHandle?.paneID,
            contentID: surfaceHandle?.contentID,
            priority: resolvedPriority?.diagnosticsValue,
            visibility: resolvedPriority?.diagnosticsVisibility,
            thread: Thread.isMainThread ? "main" : "background",
            counts: counts
        )
        if let performanceDiagnosticsRecorder {
            performanceDiagnosticsRecorder.record(event)
        } else {
            AlanPerformanceDiagnosticsController.shared.record(
                kind,
                durationMs: durationMs,
                paneID: event.paneID,
                contentID: event.contentID,
                priority: event.priority,
                visibility: event.visibility,
                thread: event.thread,
                counts: event.counts
            )
        }
    }

    private func performanceDurationMs(since start: DispatchTime) -> Double {
        let end = DispatchTime.now()
        let nanos = end.uptimeNanoseconds >= start.uptimeNanoseconds
            ? end.uptimeNanoseconds - start.uptimeNanoseconds
            : 0
        return Double(nanos) / 1_000_000
    }

    private func performanceDiagnosticsStartTime() -> DispatchTime? {
        if let performanceDiagnosticsRecorder {
            return performanceDiagnosticsRecorder.isEnabled ? DispatchTime.now() : nil
        }
        return AlanPerformanceDiagnosticsController.shared.isEnabled ? DispatchTime.now() : nil
    }

    private func resetPerSurfaceStateForSurfaceChange() {
        let previousState = scrollbackAdapter.state
        let previousRenderer = latestRenderer
        let previousMetadata = latestMetadata
        let previousSemanticCommandState = semanticCommandState
        let previousReadonly = readonly
        let previousSecureInput = secureInput
        scrollbackAdapter.reset()
        inputRouter.reset()
        if let paneID = surfaceHandle?.paneID {
            semanticCommandState = .unavailable(
                paneID: paneID,
                reason: "Terminal surface changed; command boundary ranges were invalidated."
            )
        } else {
            semanticCommandState = .placeholder
        }
        nativeScrollRowHeight = 1
        latestRenderer = .placeholder
        latestMetadata = .placeholder
        readonly = false
        secureInput = false
        if previousState != .empty
            || previousRenderer != .placeholder
            || previousMetadata != .placeholder
            || (
                previousSemanticCommandState.hasReliableCommandBoundaries
                    && previousSemanticCommandState != semanticCommandState
            )
            || previousReadonly
            || previousSecureInput
        {
            notifySurfaceStateChanged()
        }
    }

    private func notifySearchStateChanged() {
        onSearchStateChange?()
        notifySurfaceStateChanged()
    }

    private func notifySurfaceStateChanged() {
        onSurfaceStateChange?()
    }

    private var surfaceReadiness: AlanTerminalSurfaceReadiness {
        guard let surfaceHandle else { return .unready(reason: .missingSurface) }
        if latestMetadata.processExited || surfaceHandle.snapshot.metadata.processExited {
            return .unready(reason: .childExited)
        }
        if latestRenderer.phase == .failed || surfaceHandle.snapshot.renderer.phase == .failed {
            return .unready(reason: .rendererFailed)
        }
        if readonly {
            return .unready(reason: .readonly)
        }
        guard surfaceHandle.isSurfaceReady else {
            return .unready(reason: .inputNotReady)
        }
        return .ready
    }
}

#if canImport(GhosttyKit)
extension AlanTerminalSurfaceController {
    var ghosttySurfaceHandle: AlanGhosttyEventSurfaceHandle? {
        surfaceHandle as? AlanGhosttyEventSurfaceHandle
    }

    func keyTranslationMods(for mods: ghostty_input_mods_e) -> ghostty_input_mods_e {
        ghosttySurfaceHandle?.keyTranslationMods(for: mods) ?? mods
    }

    func sendKey(_ keyEvent: ghostty_input_key_s) -> Bool {
        ghosttySurfaceHandle?.sendKey(keyEvent) ?? false
    }

    func keyIsBinding(
        _ keyEvent: ghostty_input_key_s,
        flags: UnsafeMutablePointer<ghostty_binding_flags_e>?
    ) -> Bool {
        ghosttySurfaceHandle?.keyIsBinding(keyEvent, flags: flags) ?? false
    }

    func sendProgrammaticText(_ text: String) {
        ghosttySurfaceHandle?.sendProgrammaticText(text)
    }

    func sendPreedit(_ text: String?) {
        ghosttySurfaceHandle?.sendPreedit(text)
    }

    func sendMousePosition(x: Double, y: Double, mods: ghostty_input_mods_e) {
        ghosttySurfaceHandle?.sendMousePosition(x: x, y: y, mods: mods)
    }

    func sendMouseButton(
        state: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e,
        mods: ghostty_input_mods_e
    ) -> Bool {
        ghosttySurfaceHandle?.sendMouseButton(state: state, button: button, mods: mods) ?? false
    }

    func sendMouseScroll(x: Double, y: Double, mods: ghostty_input_scroll_mods_t) {
        ghosttySurfaceHandle?.sendMouseScroll(x: x, y: y, mods: mods)
    }

    func sendMousePressure(stage: UInt32, pressure: Double) {
        ghosttySurfaceHandle?.sendMousePressure(stage: stage, pressure: pressure)
    }

    func imeRect(in view: NSView) -> NSRect? {
        ghosttySurfaceHandle?.imeRect(in: view)
    }
}
#endif

#endif
