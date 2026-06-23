#if os(macOS)
import AppKit
import Carbon
#if canImport(QuartzCore)
import QuartzCore
#endif
#if canImport(GhosttyKit)
import GhosttyKit
#endif

final class AlanTerminalHostNSView: NSView, NSTextInputClient, TerminalRuntimeHandle {
    private static let inputTrace = AlanTerminalInputTrace()

    private let canvasView = makeCanvasView()
    private let overlayPresenter = TerminalHostOverlayPresenter()
    private let surfaceController = AlanTerminalSurfaceController()
    private let keyEquivalentAdapter = AlanTerminalKeyEquivalentAdapter()
    private let runtimeReporter = TerminalHostRuntimeReporter()
    private let windowObserver = TerminalHostWindowObserver()

    private var pane: ShellPane?
    private var terminalContentID: String?
    private var bootProfile: AlanShellBootProfile?
    private var isSelected = false
    private var attachmentPolicy: TerminalHostAttachmentPolicy = .immediate
    private var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    private weak var activationDelegate: TerminalHostActivationDelegate?
    private var shellActionHandler: ((ShellActionID, ShellActionTarget) -> Void)?
    private var clearRestoredTranscriptHandler: (() -> Void)?
    private var closeRequestHandler: ((Bool) -> Void)?
    private var runtimeObserver: ((TerminalHostRuntimeSnapshot) -> Void)?
    private var metadataObserver: ((TerminalPaneMetadataSnapshot) -> Void)?
    private var rendererSnapshot: TerminalRendererSnapshot = .placeholder
    private var paneMetadata: TerminalPaneMetadataSnapshot = .placeholder
    private var lastReportedMetadata: TerminalPaneMetadataSnapshot?
    private var trackingArea: NSTrackingArea?
    private var eventMonitor: Any?
    private var markedText = NSMutableAttributedString()
    private var keyTextAccumulator: [String]?
    private var clearCommandTracker = AlanTerminalClearCommandTracker()
    private var previousPressureStage = 0
    private var hasTornDownRuntime = false
    private var pendingFocusRequest = false
    private var needsWindowAttachmentFocus = false
    private var needsDeferredSurfaceAttachment = false

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        surfaceController.onSurfaceStateChange = { [weak self] in
            self?.syncNativeScrollback()
            self?.syncOverlayVisibility()
            self?.publishRuntimeSnapshot()
        }
        configureView()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    deinit {
        removeLocalEventMonitor()
        teardownRuntimeIfNeeded()
    }

    override var acceptsFirstResponder: Bool {
        true
    }

    override var mouseDownCanMoveWindow: Bool { false }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }

        let trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.activeInActiveApp, .inVisibleRect, .mouseEnteredAndExited, .mouseMoved],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea)
        self.trackingArea = trackingArea
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        publishRuntimeSnapshot()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        installWindowObservers()
        window?.acceptsMouseMovedEvents = true
        if window != nil, needsDeferredSurfaceAttachment {
            scheduleDeferredSurfaceAttachment()
        } else if window == nil {
            needsDeferredSurfaceAttachment = false
        }
        if window != nil, needsWindowAttachmentFocus {
            needsWindowAttachmentFocus = false
            focusTerminalSoon()
        } else if window == nil {
            needsWindowAttachmentFocus = false
            pendingFocusRequest = false
        }
        publishRuntimeSnapshot()
    }

    override func becomeFirstResponder() -> Bool {
        let result = super.becomeFirstResponder()
        if result {
            synchronizeLiveHost()
            publishRuntimeSnapshot()
        }
        return result
    }

    override func resignFirstResponder() -> Bool {
        let result = super.resignFirstResponder()
        if result {
            surfaceController.resetInputRouting()
            synchronizeLiveHost()
            publishRuntimeSnapshot()
        }
        return result
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        publishRuntimeSnapshot()
    }

    override func layout() {
        super.layout()
        synchronizeLiveHost()
        syncNativeScrollback()
        publishRuntimeSnapshot()
    }

    func configure(
        pane: ShellPane?,
        terminalContentID: String?,
        bootProfile: AlanShellBootProfile?,
        isSelected: Bool,
        renderPriority: TerminalRuntimeRenderPriority,
        surfaceHandle: AlanTerminalSurfaceHandle?,
        activationDelegate: TerminalHostActivationDelegate?,
        attachmentPolicy: TerminalHostAttachmentPolicy,
        onShellAction: ((ShellActionID, ShellActionTarget) -> Void)?,
        onClearRestoredTranscript: (() -> Void)?,
        onCloseRequest: ((Bool) -> Void)?,
        onRuntimeUpdate: @escaping (TerminalHostRuntimeSnapshot) -> Void,
        onMetadataUpdate: @escaping (TerminalPaneMetadataSnapshot) -> Void
    ) {
        let previousPaneID = self.pane?.paneID
        let previousContentID = self.terminalContentID
        let wasSelected = self.isSelected

        self.pane = pane
        self.terminalContentID = terminalContentID
        self.bootProfile = bootProfile
        self.isSelected = isSelected
        self.attachmentPolicy = attachmentPolicy
        self.renderPriority = renderPriority
        surfaceController.bind(surfaceHandle: surfaceHandle, paneID: pane?.paneID)
        self.activationDelegate = activationDelegate
        shellActionHandler = onShellAction
        clearRestoredTranscriptHandler = onClearRestoredTranscript
        closeRequestHandler = onCloseRequest
        runtimeObserver = onRuntimeUpdate
        metadataObserver = onMetadataUpdate
        if previousContentID != terminalContentID {
            clearCommandTracker.reset()
        }

        overlayPresenter.configure(pane: pane, bootProfile: bootProfile)

        synchronizeRendererSnapshot(with: bootProfile)
        syncStatusBadge()
        syncOverlayVisibility()
        synchronizeLiveHost()
        syncNativeScrollback()
        if terminalHostShouldAutoFocusAfterConfigure(
            isSelected: isSelected,
            previousPaneID: previousContentID == terminalContentID ? previousPaneID : nil,
            paneID: pane?.paneID,
            wasSelected: wasSelected
        ) {
            focusTerminalSoon()
        }
        reportMetadataIfNeeded(paneMetadata)
        publishRuntimeSnapshot()
    }

    private func reportMetadataIfNeeded(_ snapshot: TerminalPaneMetadataSnapshot) {
        guard lastReportedMetadata != snapshot else { return }
        lastReportedMetadata = snapshot
        guard let metadataObserver else { return }
        DispatchQueue.main.async { [weak self] in
            guard let self, self.lastReportedMetadata == snapshot else { return }
            metadataObserver(snapshot)
        }
    }

    private func configureView() {
        wantsLayer = true
        layer?.backgroundColor = NSColor(calibratedRed: 0.06, green: 0.08, blue: 0.10, alpha: 1).cgColor
        layer?.masksToBounds = true
        layer?.borderWidth = 0
        layer?.shadowColor = NSColor.black.cgColor
        layer?.shadowOpacity = 0
        layer?.shadowRadius = 0
        layer?.shadowOffset = .zero

        translatesAutoresizingMaskIntoConstraints = false

        let nativeScrollView = surfaceController.nativeScrollViewAdapter.scrollView
        surfaceController.nativeScrollViewAdapter.onScrollWheel = { [weak self] event in
            self?.routeScrollWheel(event) ?? false
        }
        surfaceController.nativeScrollViewAdapter.onMouseEvent = { [weak self] routedEvent, event in
            self?.routeWrappedMouseEvent(routedEvent, event) ?? false
        }
        surfaceController.nativeScrollViewAdapter.attachCanvasView(canvasView)
        addSubview(nativeScrollView)
        overlayPresenter.install(in: self)
        installLocalEventMonitor()

        NSLayoutConstraint.activate([
            nativeScrollView.topAnchor.constraint(equalTo: topAnchor),
            nativeScrollView.leadingAnchor.constraint(equalTo: leadingAnchor),
            nativeScrollView.trailingAnchor.constraint(equalTo: trailingAnchor),
            nativeScrollView.bottomAnchor.constraint(equalTo: bottomAnchor),

        ])
    }

    private func installLocalEventMonitor() {
        guard eventMonitor == nil else { return }
        eventMonitor = NSEvent.addLocalMonitorForEvents(
            matching: [.keyUp, .leftMouseDown]
        ) { [weak self] event in
            self?.localEventHandler(event) ?? event
        }
    }

    private func removeLocalEventMonitor() {
        if let eventMonitor {
            NSEvent.removeMonitor(eventMonitor)
            self.eventMonitor = nil
        }
    }

    private func localEventHandler(_ event: NSEvent) -> NSEvent? {
        switch event.type {
        case .keyUp:
            return localEventKeyUp(event)
        case .leftMouseDown:
            return localEventLeftMouseDown(event)
        default:
            return event
        }
    }

    private func localEventKeyUp(_ event: NSEvent) -> NSEvent? {
        guard event.modifierFlags.contains(.command) else { return event }
        guard terminalInputIsActive else { return event }
        keyUp(with: event)
        return nil
    }

    private func localEventLeftMouseDown(_ event: NSEvent) -> NSEvent? {
        guard let window,
              event.window != nil,
              event.window == window,
              let contentView = window.contentView
        else {
            traceTerminalInput(
                "local-leftMouseDown ignored",
                event: event,
                details: "reason=window_mismatch"
            )
            return event
        }

        let location = contentView.convert(event.locationInWindow, from: nil)
        let hitView = contentView.hitTest(location)
        let hitOwnsTerminal = terminalHostOwnsHitTestView(hitView)
        let decision = surfaceController.routeLeftMouseDown(
            hitOwnsTerminal: hitOwnsTerminal,
            commandSurfaceVisible: false,
            isFirstResponder: terminalInputIsActive,
            appIsActive: NSApp.isActive,
            windowIsKey: window.isKeyWindow
        )
        traceTerminalInput(
            "local-leftMouseDown",
            event: event,
            details: "hitOwnsTerminal=\(hitOwnsTerminal) hitView=\(traceViewName(hitView)) decision=\(decision)"
        )

        switch decision {
        case .ignored, .deliverToTerminal:
            return event
        case .focusOnly:
            activateTerminalHostForMouseEvent()
            return nil
        case .focusAndDeliver:
            activateTerminalHostForMouseEvent()
            return event
        }
    }

    private func terminalHostOwnsHitTestView(_ view: NSView?) -> Bool {
        var current = view
        while let candidate = current {
            if candidate === self {
                return true
            }
            current = candidate.superview
        }
        return false
    }

    func sendControlText(_ text: String) -> TerminalRuntimeDeliveryResult {
        guard !text.isEmpty else {
            return .accepted(byteCount: 0)
        }

        guard terminalContentID != nil else {
            return .rejected(
                errorCode: "terminal_runtime_unavailable",
                errorMessage: "No terminal content is attached to this host."
            )
        }

        return surfaceController.sendControlText(text)
    }

    func teardownTerminalRuntime() {
        teardownRuntimeIfNeeded()
    }

    private func teardownRuntimeIfNeeded() {
        guard !hasTornDownRuntime else { return }
        hasTornDownRuntime = true
        windowObserver.remove()
        surfaceController.detach()
    }

    private func installWindowObservers() {
        windowObserver.install(
            for: window,
            onRuntimeEnvironmentChange: { [weak self] in
                self?.publishRuntimeSnapshot()
            },
            onSurfaceEnvironmentChange: { [weak self] in
                self?.synchronizeLiveHost()
            }
        )
    }

    private func publishRuntimeSnapshot() {
        guard let runtimeObserver else { return }

        let logicalSize = bounds.size
        let backingRect = convertToBacking(bounds)
        let screen = window?.screen
        let stage: TerminalHostStage = {
            guard superview != nil else { return .scaffold }
            guard window != nil else { return .viewAttached }
            return terminalInputIsActive ? .focused : .windowAttached
        }()

        let displayID = (screen?.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)
            .map { "\($0.uint32Value)" }
        let surfaceState = surfaceController.surfaceStateSnapshot

        let snapshot = TerminalHostRuntimeSnapshot(
            stage: stage,
            contentID: terminalContentID,
            paneID: pane?.paneID,
            tabID: pane?.tabID,
            renderPriority: effectiveRenderPriority,
            logicalSize: logicalSize,
            backingSize: backingRect.size,
            displayName: screen?.localizedName,
            displayID: displayID,
            attachedWindowTitle: window?.title,
            isFocused: terminalInputIsActive,
            renderer: rendererSnapshot,
            paneMetadata: paneMetadata,
            surfaceState: surfaceState,
            lastUpdatedAt: .now
        )
        runtimeReporter.publish(snapshot, observer: runtimeObserver)
    }

    private func synchronizeLiveHost() {
        guard canSynchronizeLiveHost else {
            needsDeferredSurfaceAttachment = true
            publishRuntimeSnapshot()
            return
        }
        needsDeferredSurfaceAttachment = false
#if canImport(GhosttyKit)
        guard let canvasView = canvasView as? AlanGhosttyCanvasView else { return }
        surfaceController.attach(
            to: canvasView,
            bootProfile: bootProfile,
            focused: terminalInputIsActive,
            renderPriority: effectiveRenderPriority,
            onDiagnosticsChange: { [weak self] snapshot in
                guard let self else { return }
                rendererSnapshot = snapshot
                surfaceController.updateRenderer(snapshot)
                syncStatusBadge()
                syncOverlayVisibility()
                publishRuntimeSnapshot()
            },
            onMetadataChange: { [weak self] snapshot in
                guard let self else { return }
                paneMetadata = snapshot
                surfaceController.updateMetadata(snapshot)
                overlayPresenter.updateSubtitle(snapshot.summary)
                reportMetadataIfNeeded(snapshot)
                syncOverlayVisibility()
                publishRuntimeSnapshot()
            },
            onCloseRequest: { [weak self] requiresConfirmation in
                self?.reportCloseRequest(requiresConfirmation: requiresConfirmation)
            }
        )
#endif
    }

    private var canSynchronizeLiveHost: Bool {
    #if canImport(GhosttyKit)
        terminalSurfaceAttachmentBlocker == nil
    #else
        attachmentPolicy == .immediate || window != nil
    #endif
    }

#if canImport(GhosttyKit)
    private var terminalSurfaceAttachmentBlocker: String? {
        guard let window else {
            return "window"
        }
        guard window.screen != nil || NSScreen.main != nil else {
            return "screen"
        }

        let logicalSize = canvasView.bounds.size
        guard logicalSize.width > 0, logicalSize.height > 0 else {
            return "logical_size"
        }

        let backingSize = canvasView.convertToBacking(
            NSRect(origin: .zero, size: logicalSize)
        ).size
        guard backingSize.width > 0, backingSize.height > 0 else {
            return "backing_size"
        }

        return nil
    }
#endif

    private func scheduleDeferredSurfaceAttachment() {
        guard window != nil else {
            needsDeferredSurfaceAttachment = true
            return
        }
        needsDeferredSurfaceAttachment = false
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            guard window != nil else {
                needsDeferredSurfaceAttachment = true
                return
            }
            synchronizeLiveHost()
        }
    }

    private func reportCloseRequest(requiresConfirmation: Bool) {
        guard let closeRequestHandler else { return }
        let paneID = pane?.paneID
        let contentID = terminalContentID
        DispatchQueue.main.async { [weak self] in
            guard let self,
                  self.pane?.paneID == paneID,
                  self.terminalContentID == contentID
            else {
                return
            }
            closeRequestHandler(requiresConfirmation)
        }
    }

    private func syncNativeScrollback() {
        surfaceController.syncNativeScrollView(viewportSize: bounds.size)
    }

    private func synchronizeRendererSnapshot(with bootProfile: AlanShellBootProfile?) {
#if canImport(GhosttyKit)
        if rendererSnapshot.kind == .scaffold {
            rendererSnapshot = TerminalRendererSnapshot(
                kind: .ghosttyLive,
                phase: .pending,
                summary: bootProfile?.ghostty.isReady == true
                    ? "GhosttyKit is linked and waiting for the live host handshake."
                    : "GhosttyKit has not been linked into the local repo yet.",
                detail: bootProfile?.command.summary,
                failureReason: nil,
                recentEvents: rendererSnapshot.recentEvents
            )
        }
#else
        rendererSnapshot = TerminalRendererSnapshot(
            kind: .scaffold,
            phase: .pending,
            summary: bootProfile?.ghostty.isReady == true
                ? "GhosttyKit is available on disk but this build does not import it."
                : "GhosttyKit has not been linked into the local repo yet.",
            detail: bootProfile?.command.summary,
            failureReason: nil,
            recentEvents: []
        )
#endif
    }

    private func syncStatusBadge() {
        overlayPresenter.syncStatusBadge(bootProfile: bootProfile, renderer: rendererSnapshot)
    }

    private func syncOverlayVisibility() {
        let overlayState = surfaceController.overlayState(
            renderer: rendererSnapshot,
            metadata: paneMetadata,
            bootProfile: bootProfile
        )
        overlayPresenter.syncOverlay(overlayState: overlayState, bootProfile: bootProfile)
    }

    private var isFocused: Bool {
        window?.firstResponder === self
    }

    private var terminalInputIsActive: Bool {
        isSelected && isFocused
    }

    private var effectiveRenderPriority: TerminalRuntimeRenderPriority {
        guard renderPriority.isVisible,
              window?.occlusionState.contains(.visible) == true
        else {
            return .hiddenBackground
        }
        guard terminalInputIsActive else {
            return renderPriority == .foregroundInteractive
                ? .visibleBackground
                : renderPriority
        }
        return renderPriority
    }

    private func traceTerminalInput(
        _ eventName: String,
        event: NSEvent? = nil,
        details: @autoclosure () -> String = ""
    ) {
        guard Self.inputTrace.isEnabled else { return }
        let paneID = pane?.paneID ?? "nil"
        let firstResponder = traceResponderName(window?.firstResponder)
        let windowKey = window?.isKeyWindow == true
        let timestamp = event.map { String(format: "%.6f", $0.timestamp) } ?? "nil"
        let point = event.map { tracePoint(localPoint(for: $0)) } ?? "nil"
        let detailText = details()
        let suffix = detailText.isEmpty ? "" : " \(detailText)"
        Self.inputTrace.log(
            "\(eventName) pane=\(paneID) selected=\(isSelected) firstResponderSelf=\(isFocused) active=\(terminalInputIsActive) appActive=\(NSApp.isActive) windowKey=\(windowKey) firstResponder=\(firstResponder) ts=\(timestamp) point=\(point)\(suffix)"
        )
    }

    private func terminalInputTraceStart() -> DispatchTime? {
        Self.inputTrace.isEnabled ? DispatchTime.now() : nil
    }

    private func traceTerminalInputDuration(
        _ eventName: String,
        event: NSEvent? = nil,
        startedAt: DispatchTime?,
        details: @autoclosure () -> String = ""
    ) {
        guard let startedAt else { return }
        let now = DispatchTime.now()
        let nanos = now.uptimeNanoseconds >= startedAt.uptimeNanoseconds
            ? now.uptimeNanoseconds - startedAt.uptimeNanoseconds
            : 0
        let elapsedMs = Double(nanos) / 1_000_000
        let detailText = details()
        let suffix = detailText.isEmpty ? "" : " \(detailText)"
        traceTerminalInput(
            eventName,
            event: event,
            details: "elapsed_ms=\(String(format: "%.3f", elapsedMs))\(suffix)"
        )
    }

    private func tracePointerInput(
        _ eventName: String,
        input: AlanTerminalPointerInput,
        decision: AlanTerminalPointerRoutingDecision,
        handled: Bool
    ) {
        guard Self.inputTrace.isEnabled else { return }
        switch input.phase {
        case .buttonDown, .buttonUp, .drag:
            break
        case .entered, .moved, .exited, .pressure:
            return
        }

        let paneID = pane?.paneID ?? "nil"
        Self.inputTrace.log(
            "\(eventName) pane=\(paneID) selected=\(isSelected) firstResponderSelf=\(isFocused) active=\(terminalInputIsActive) phase=\(input.phase) button=\(input.normalizedButton.map { String(describing: $0) } ?? "nil") x=\(String(format: "%.1f", input.x)) y=\(String(format: "%.1f", input.y)) decision=\(decision) handled=\(handled)"
        )
    }

    private func tracePoint(_ point: CGPoint) -> String {
        "(\(String(format: "%.1f", point.x)),\(String(format: "%.1f", point.y)))"
    }

    private func traceResponderName(_ responder: NSResponder?) -> String {
        guard let responder else { return "nil" }
        return String(describing: type(of: responder))
    }

    private func traceViewName(_ view: NSView?) -> String {
        guard let view else { return "nil" }
        return String(describing: type(of: view))
    }

    private func focusTerminalSoon() {
        guard isSelected, pane != nil else { return }
        guard window != nil else {
            needsWindowAttachmentFocus = true
            return
        }
        guard !pendingFocusRequest else { return }
        pendingFocusRequest = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            pendingFocusRequest = false
            guard isSelected, pane != nil, window != nil else { return }
            requestTerminalFocus()
        }
    }

    private func requestTerminalFocus() {
        guard !terminalInputIsActive else { return }
        if window?.makeFirstResponder(self) == true {
            return
        }
        synchronizeLiveHost()
        publishRuntimeSnapshot()
    }

    func focusTerminal() {
        focusTerminalSoon()
    }

    private func activateTerminalHostForMouseEvent() {
        if let paneID = pane?.paneID {
            activationDelegate?.terminalHostDidRequestActivation(paneID: paneID)
        }
        requestTerminalFocus()
    }

    private func localPoint(for event: NSEvent) -> CGPoint {
        convert(event.locationInWindow, from: nil)
    }

    private func ghosttyPoint(for event: NSEvent) -> CGPoint {
        let point = localPoint(for: event)
        return CGPoint(x: point.x, y: bounds.height - point.y)
    }

    private func terminalPointerInput(
        for event: NSEvent,
        phase: AlanTerminalPointerPhase,
        button: AlanTerminalPointerButton? = nil
    ) -> AlanTerminalPointerInput {
        let point = ghosttyPoint(for: event)
        return AlanTerminalPointerInput(
            phase: phase,
            button: button,
            buttonNumber: event.buttonNumber,
            x: point.x,
            y: point.y,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            pressureStage: nil,
            pressure: nil
        )
    }

    private func terminalPointerPressureInput(for event: NSEvent) -> AlanTerminalPointerInput {
        AlanTerminalPointerInput(
            phase: .pressure,
            button: nil,
            buttonNumber: nil,
            x: 0,
            y: 0,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            pressureStage: event.stage,
            pressure: Double(event.pressure)
        )
    }

    @discardableResult
    private func routePointer(_ input: AlanTerminalPointerInput) -> Bool {
        let decision = pointerDecision(for: input)
        let handled = executePointerDecision(decision)
        tracePointerInput("pointer-route", input: input, decision: decision, handled: handled)
        return handled
    }

    private func pointerDecision(
        for input: AlanTerminalPointerInput
    ) -> AlanTerminalPointerRoutingDecision {
#if canImport(GhosttyKit)
        return surfaceController.routePointer(input)
#else
        return .ignored
#endif
    }

    @discardableResult
    private func executePointerDecision(_ decision: AlanTerminalPointerRoutingDecision) -> Bool {
#if canImport(GhosttyKit)
        return deliverPointerDecision(decision)
#else
        return false
#endif
    }

    override func mouseDown(with event: NSEvent) {
        traceTerminalInput("raw-leftMouseDown", event: event)
        activateTerminalHostForMouseEvent()
        routePointer(terminalPointerInput(for: event, phase: .buttonDown, button: .primary))
    }

    override func mouseUp(with event: NSEvent) {
        let buttonUpDecision = pointerDecision(
            for: terminalPointerInput(for: event, phase: .buttonUp, button: .primary)
        )
        traceTerminalInput(
            "raw-leftMouseUp",
            event: event,
            details: "decision=\(buttonUpDecision)"
        )
        if buttonUpDecision == .consumed {
            return
        }
        previousPressureStage = 0
        executePointerDecision(buttonUpDecision)
        routePointer(
            AlanTerminalPointerInput(
                phase: .pressure,
                button: nil,
                buttonNumber: nil,
                x: 0,
                y: 0,
                modifiers: terminalKeyModifiers(from: event.modifierFlags),
                pressureStage: 0,
                pressure: 0
            )
        )
    }

    override func rightMouseDown(with event: NSEvent) {
        activateTerminalHostForMouseEvent()
        let consumed = routePointer(
            terminalPointerInput(for: event, phase: .buttonDown, button: .secondary)
        )
        if !consumed {
            super.rightMouseDown(with: event)
        }
    }

    override func rightMouseUp(with event: NSEvent) {
        let consumed = routePointer(
            terminalPointerInput(for: event, phase: .buttonUp, button: .secondary)
        )
        if !consumed {
            super.rightMouseUp(with: event)
        }
    }

    override func otherMouseDown(with event: NSEvent) {
        activateTerminalHostForMouseEvent()
        let consumed = routePointer(
            terminalPointerInput(
                for: event,
                phase: .buttonDown,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
        if !consumed {
            super.otherMouseDown(with: event)
        }
    }

    override func otherMouseUp(with event: NSEvent) {
        let consumed = routePointer(
            terminalPointerInput(
                for: event,
                phase: .buttonUp,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
        if !consumed {
            super.otherMouseUp(with: event)
        }
    }

    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        routePointer(terminalPointerInput(for: event, phase: .entered))
    }

    override func mouseMoved(with event: NSEvent) {
        routePointer(terminalPointerInput(for: event, phase: .moved))
    }

    override func mouseDragged(with event: NSEvent) {
        traceTerminalInput("raw-leftMouseDragged", event: event)
        routePointer(terminalPointerInput(for: event, phase: .drag, button: .primary))
    }

    override func rightMouseDragged(with event: NSEvent) {
        routePointer(terminalPointerInput(for: event, phase: .drag, button: .secondary))
    }

    override func otherMouseDragged(with event: NSEvent) {
        routePointer(
            terminalPointerInput(
                for: event,
                phase: .drag,
                button: AlanTerminalPointerButton.fromAppKitButtonNumber(event.buttonNumber)
            )
        )
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        routePointer(terminalPointerInput(for: event, phase: .exited))
    }

    @discardableResult
    private func routeWrappedMouseEvent(_ routedEvent: AlanTerminalRoutedMouseEvent, _ event: NSEvent) -> Bool {
        traceTerminalInput(
            "wrapped-\(routedEvent)",
            event: event,
            details: "source=nativeScrollView"
        )
        switch routedEvent {
        case .mouseDown:
            mouseDown(with: event)
        case .mouseUp:
            mouseUp(with: event)
        case .rightMouseDown:
            rightMouseDown(with: event)
        case .rightMouseUp:
            rightMouseUp(with: event)
        case .otherMouseDown:
            otherMouseDown(with: event)
        case .otherMouseUp:
            otherMouseUp(with: event)
        case .mouseEntered:
            mouseEntered(with: event)
        case .mouseMoved:
            mouseMoved(with: event)
        case .mouseDragged:
            mouseDragged(with: event)
        case .rightMouseDragged:
            rightMouseDragged(with: event)
        case .otherMouseDragged:
            otherMouseDragged(with: event)
        case .mouseExited:
            mouseExited(with: event)
        case .pressureChange:
            pressureChange(with: event)
        }
        return true
    }

    override func scrollWheel(with event: NSEvent) {
        if routeScrollWheel(event) {
            return
        }
        super.scrollWheel(with: event)
    }

    private func routeScrollWheel(_ event: NSEvent) -> Bool {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return false }

        let scrollRoute = surfaceController.routeScroll(
            AlanTerminalScrollInput(
                deltaX: event.scrollingDeltaX,
                deltaY: event.scrollingDeltaY,
                precise: event.hasPreciseScrollingDeltas
            )
        )
        switch scrollRoute {
        case .nativeScroll:
            syncNativeScrollback()
            publishRuntimeSnapshot()
            return true
        case .ignored:
            return true
        case .terminalScroll:
            break
        }

        var x = event.scrollingDeltaX
        var y = event.scrollingDeltaY
        let precision = event.hasPreciseScrollingDeltas
        if precision {
            x *= 2
            y *= 2
        }

        var scrollMods: Int32 = 0
        if precision {
            scrollMods |= 0b0000_0001
        }

        let momentum: Int32
        switch event.momentumPhase {
        case .began:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_BEGAN.rawValue)
        case .stationary:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_STATIONARY.rawValue)
        case .changed:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_CHANGED.rawValue)
        case .ended:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_ENDED.rawValue)
        case .cancelled:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_CANCELLED.rawValue)
        case .mayBegin:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_MAY_BEGIN.rawValue)
        default:
            momentum = Int32(GHOSTTY_MOUSE_MOMENTUM_NONE.rawValue)
        }
        scrollMods |= momentum << 1

        surfaceController.sendMouseScroll(x: x, y: y, mods: ghostty_input_scroll_mods_t(scrollMods))
        return true
#else
        return false
#endif
    }

    override func pressureChange(with event: NSEvent) {
        super.pressureChange(with: event)
        guard routePointer(terminalPointerPressureInput(for: event)) else { return }
        previousPressureStage = event.stage
    }

#if canImport(GhosttyKit)
    @discardableResult
    private func deliverPointerDecision(_ decision: AlanTerminalPointerRoutingDecision) -> Bool {
        switch decision {
        case .terminalMouse(let operation),
             .terminalSelection(let operation),
             .terminalHover(let operation):
            return deliverPointerOperation(operation)
        case .consumed:
            return true
        case .ignored:
            return false
        }
    }

    @discardableResult
    private func deliverPointerOperation(_ operation: AlanTerminalPointerOperation) -> Bool {
        switch operation {
        case .position(let x, let y, let modifiers):
            surfaceController.sendMousePosition(
                x: x,
                y: y,
                mods: ghosttyMods(from: modifiers)
            )
            return true
        case .button(let state, let button, let x, let y, let modifiers):
            let mods = ghosttyMods(from: modifiers)
            surfaceController.sendMousePosition(x: x, y: y, mods: mods)
            return surfaceController.sendMouseButton(
                state: ghosttyMouseState(from: state),
                button: ghosttyMouseButton(from: button),
                mods: mods
            )
        case .pressure(let stage, let pressure):
            surfaceController.sendMousePressure(stage: UInt32(max(stage, 0)), pressure: pressure)
            return true
        }
    }

    private func ghosttyMouseState(
        from state: AlanTerminalPointerButtonState
    ) -> ghostty_input_mouse_state_e {
        switch state {
        case .press:
            GHOSTTY_MOUSE_PRESS
        case .release:
            GHOSTTY_MOUSE_RELEASE
        }
    }

    private func ghosttyMouseButton(
        from button: AlanTerminalPointerButton
    ) -> ghostty_input_mouse_button_e {
        switch button {
        case .unknown:
            GHOSTTY_MOUSE_UNKNOWN
        case .primary:
            GHOSTTY_MOUSE_LEFT
        case .secondary:
            GHOSTTY_MOUSE_RIGHT
        case .middle:
            GHOSTTY_MOUSE_MIDDLE
        case .four:
            GHOSTTY_MOUSE_FOUR
        case .five:
            GHOSTTY_MOUSE_FIVE
        case .six:
            GHOSTTY_MOUSE_SIX
        case .seven:
            GHOSTTY_MOUSE_SEVEN
        case .eight:
            GHOSTTY_MOUSE_EIGHT
        case .nine:
            GHOSTTY_MOUSE_NINE
        case .ten:
            GHOSTTY_MOUSE_TEN
        case .eleven:
            GHOSTTY_MOUSE_ELEVEN
        }
    }

    private func ghosttyMods(from modifiers: AlanTerminalKeyModifiers) -> ghostty_input_mods_e {
        var mods = GHOSTTY_MODS_NONE.rawValue
        if modifiers.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if modifiers.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if modifiers.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
        if modifiers.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
        return ghostty_input_mods_e(rawValue: mods)
    }
#endif

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if routeShellActionKeyIfNeeded(event) {
            return true
        }
        if routeNativeKeyCommandIfNeeded(event) {
            return true
        }

#if canImport(GhosttyKit)
        guard !isApplicationReservedKeyEquivalent(event) else { return false }
        guard event.type == .keyDown,
              terminalInputIsActive,
              surfaceController.isSurfaceReady == true else { return false }

        var keyEvent = ghosttyKeyEvent(for: event, action: GHOSTTY_ACTION_PRESS)
        var flags = ghostty_binding_flags_e(0)
        let text = event.characters ?? ""
        let isBinding = text.withCString { cString in
            keyEvent.text = cString
            return surfaceController.keyIsBinding(keyEvent, flags: &flags)
        }

        switch keyEquivalentAdapter.routeKeyEquivalent(
            terminalKeyEquivalentInput(for: event),
            isFocused: terminalInputIsActive,
            isTerminalBinding: isBinding
        ) {
        case .sendOriginal:
            keyDown(with: event)
            return true
        case .sendEquivalent(let equivalent):
            keyDown(with: terminalKeyEquivalentEvent(equivalent: equivalent, basedOn: event) ?? event)
            return true
        case .deferToResponder:
            return false
        }
#else
        return false
#endif
    }

    override func keyDown(with event: NSEvent) {
        let traceStartedAt = terminalInputTraceStart()
        traceTerminalInput(
            "raw-keyDown-begin",
            event: event,
            details: "keyCode=\(event.keyCode) chars_len=\((event.characters ?? "").count) surfaceReady=\(surfaceController.isSurfaceReady == true)"
        )
        defer {
            traceTerminalInputDuration(
                "raw-keyDown-end",
                event: event,
                startedAt: traceStartedAt,
                details: "keyCode=\(event.keyCode) surfaceReady=\(surfaceController.isSurfaceReady == true)"
            )
        }

        if routeShellActionKeyIfNeeded(event) {
            return
        }
        if routeNativeKeyCommandIfNeeded(event) {
            return
        }

#if canImport(GhosttyKit)
        if isApplicationReservedKeyEquivalent(event) {
            NSApp.terminate(nil)
            return
        }

        guard surfaceController.isSurfaceReady == true else {
            interpretKeyEvents([event])
            return
        }

        requestTerminalFocus()
        keyEquivalentAdapter.clearPendingRedispatch()
        let keyInput = terminalKeyInput(for: event)
        let keyboardDecision = surfaceController.routeKeyboard(
            keyInput,
            hasMarkedText: markedText.length > 0
        )
        if AlanTerminalClearIntent.isControlL(keyInput) {
            observeTerminalClearIntent()
        }

        switch keyboardDecision {
        case .shellAction(let actionID, let target):
            if actionID == .findOpen {
                _ = routeFindCommandToShellHost(target: target)
            } else {
                shellActionHandler?(actionID, target)
            }
            return
        case .nativeCommand("find"):
            _ = routeFindCommandToShellHost()
            return
        case .nativeCommand("quit"):
            NSApp.terminate(nil)
            return
        case .shellActionLookupFailed:
            return
        case .nativeCommand, .drop:
            return
        case .interpretTextInput, .terminalKey:
            break
        }

        let translationModsGhostty =
            surfaceController.keyTranslationMods(for: modsFromEvent(event))
        var translationMods = event.modifierFlags
        for flag in [NSEvent.ModifierFlags.shift, .control, .option, .command] {
            let shouldInclude: Bool
            switch flag {
            case .shift:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_SHIFT.rawValue) != 0
            case .control:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_CTRL.rawValue) != 0
            case .option:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_ALT.rawValue) != 0
            case .command:
                shouldInclude = (translationModsGhostty.rawValue & GHOSTTY_MODS_SUPER.rawValue) != 0
            default:
                shouldInclude = translationMods.contains(flag)
            }

            if shouldInclude {
                translationMods.insert(flag)
            } else {
                translationMods.remove(flag)
            }
        }

        let translationEvent: NSEvent
        if translationMods == event.modifierFlags {
            translationEvent = event
        } else {
            translationEvent = NSEvent.keyEvent(
                with: event.type,
                location: event.locationInWindow,
                modifierFlags: translationMods,
                timestamp: event.timestamp,
                windowNumber: event.windowNumber,
                context: nil,
                characters: event.characters(byApplyingModifiers: translationMods) ?? "",
                charactersIgnoringModifiers: event.charactersIgnoringModifiers ?? "",
                isARepeat: event.isARepeat,
                keyCode: event.keyCode
            ) ?? event
        }

        keyTextAccumulator = []
        defer { keyTextAccumulator = nil }

        let markedTextBefore = markedText.length > 0
        let shouldInterpretText = keyboardDecision == .interpretTextInput
        let keyboardIDBefore = !markedTextBefore && shouldInterpretText
            ? AlanKeyboardLayout.currentID
            : nil
        if shouldInterpretText {
            interpretKeyEvents([translationEvent])
            if let keyboardIDBefore, keyboardIDBefore != AlanKeyboardLayout.currentID {
                return
            }
            syncPreedit(clearIfNeeded: markedTextBefore)
        }
        let composing = markedText.length > 0 || markedTextBefore
        let action = event.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS

        if shouldInterpretText,
           let keyTextAccumulator,
           !keyTextAccumulator.isEmpty
        {
            for text in keyTextAccumulator {
                guard !AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
                    text,
                    composing: composing
                ) else {
                    continue
                }

                observeCommittedTerminalText(text)
                if markedTextBefore {
                    sendCommittedPreeditText(text, action: action)
                } else {
                    sendGhosttyKeyEvent(
                        for: event,
                        action: action,
                        translationMods: translationMods,
                        textOverride: text,
                        composing: false
                    )
                }
            }

            if markedTextBefore, shouldReplayCommittedPreeditKey(translationEvent) {
                sendGhosttyKeyEvent(
                    for: event,
                    action: action,
                    translationMods: translationMods,
                    composing: false
                )
            }
            return
        }

        if AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
            event.characters,
            composing: composing
        ) {
            return
        }

        if !composing {
            observeCommittedTerminalText(translationEvent.characters ?? event.characters)
        }
        sendGhosttyKeyEvent(
            for: event,
            action: action,
            translationMods: translationMods,
            textEvent: translationEvent,
            composing: composing
        )
#else
        super.keyDown(with: event)
#endif
    }

    private func observeTerminalClearIntent() {
        clearCommandTracker.reset()
        clearRestoredTranscriptHandler?()
    }

    private func observeCommittedTerminalText(_ text: String?) {
        if clearCommandTracker.observeCommittedText(text) {
            clearRestoredTranscriptHandler?()
        }
    }

    override func keyUp(with event: NSEvent) {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return super.keyUp(with: event) }
        let traceStartedAt = terminalInputTraceStart()
        let keyEvent = ghosttyKeyEvent(for: event, action: GHOSTTY_ACTION_RELEASE)
        let handled = surfaceController.sendKey(keyEvent)
        traceTerminalInputDuration(
            "keyUp-sendKey-end",
            event: event,
            startedAt: traceStartedAt,
            details: "keyCode=\(event.keyCode) handled=\(handled)"
        )
#else
        super.keyUp(with: event)
#endif
    }

    override func flagsChanged(with event: NSEvent) {
#if canImport(GhosttyKit)
        guard surfaceController.isSurfaceReady == true else { return super.flagsChanged(with: event) }
        guard !hasMarkedText() else { return }

        let modifier: UInt32
        switch event.keyCode {
        case 0x39: modifier = GHOSTTY_MODS_CAPS.rawValue
        case 0x38, 0x3C: modifier = GHOSTTY_MODS_SHIFT.rawValue
        case 0x3B, 0x3E: modifier = GHOSTTY_MODS_CTRL.rawValue
        case 0x3A, 0x3D: modifier = GHOSTTY_MODS_ALT.rawValue
        case 0x37, 0x36: modifier = GHOSTTY_MODS_SUPER.rawValue
        default: return
        }

        let mods = modsFromEvent(event)
        var action = GHOSTTY_ACTION_RELEASE
        if mods.rawValue & modifier != 0 {
            let sidePressed: Bool
            switch event.keyCode {
            case 0x3C:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERSHIFTKEYMASK) != 0
            case 0x3E:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCTLKEYMASK) != 0
            case 0x3D:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERALTKEYMASK) != 0
            case 0x36:
                sidePressed = event.modifierFlags.rawValue & UInt(NX_DEVICERCMDKEYMASK) != 0
            default:
                sidePressed = true
            }
            if sidePressed {
                action = GHOSTTY_ACTION_PRESS
            }
        }

        let keyEvent = ghosttyKeyEvent(for: event, action: action)
        _ = surfaceController.sendKey(keyEvent)
        routePointer(terminalPointerInput(for: event, phase: .moved))
#else
        super.flagsChanged(with: event)
#endif
    }

    @objc func copy(_ sender: Any?) {
        _ = copySelection()
    }

    @objc func cut(_ sender: Any?) {
        copy(sender)
    }

    @objc func paste(_ sender: Any?) {
        guard let text = NSPasteboard.general.string(forType: .string), !text.isEmpty else { return }
        _ = pasteText(text)
    }

    var terminalCommandRuntimeState: ShellTerminalCommandRuntimeState {
        ShellTerminalCommandRuntimeState(
            paneID: pane?.paneID ?? "",
            hasSelection: surfaceController.hasSelection(),
            inputReady: surfaceController.surfaceStateSnapshot.inputReady,
            searchAvailable: pane?.paneID != nil,
            hasReliableSemanticCommands: surfaceController.hasReliableSemanticCommandActions
        )
    }

    @discardableResult
    func copySelection() -> Bool {
        surfaceController.copySelection(to: .general)
    }

    @discardableResult
    func copySelection(to writer: AlanTerminalPasteboardWriting) -> Bool {
        surfaceController.copySelection(to: writer)
    }

    @discardableResult
    func pasteText(_ text: String) -> TerminalRuntimeDeliveryResult {
        let result = surfaceController.paste(text)
        publishRuntimeSnapshot()
        return result
    }

    private func modsFromEvent(_ event: NSEvent) -> ghostty_input_mods_e {
        ghosttyMods(from: event.modifierFlags)
    }

    private func consumedModsFromFlags(_ flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        ghosttyMods(from: flags)
    }

    private func ghosttyMods(from flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        var mods = GHOSTTY_MODS_NONE.rawValue
        if flags.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
        if flags.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
        if flags.contains(.capsLock) { mods |= GHOSTTY_MODS_CAPS.rawValue }

        let rawFlags = flags.rawValue
        if rawFlags & UInt(NX_DEVICERSHIFTKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_SHIFT_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERCTLKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_CTRL_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERALTKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_ALT_RIGHT.rawValue
        }
        if rawFlags & UInt(NX_DEVICERCMDKEYMASK) != 0 {
            mods |= GHOSTTY_MODS_SUPER_RIGHT.rawValue
        }

        return ghostty_input_mods_e(rawValue: mods)
    }

    private func ghosttyKeyEvent(
        for event: NSEvent,
        action: ghostty_input_action_e,
        translationMods: NSEvent.ModifierFlags? = nil
    ) -> ghostty_input_key_s {
        var keyEvent = ghostty_input_key_s()
        keyEvent.action = action
        keyEvent.keycode = UInt32(event.keyCode)
        keyEvent.mods = modsFromEvent(event)
        keyEvent.consumed_mods = consumedModsFromFlags(
            (translationMods ?? event.modifierFlags).subtracting([.control, .command])
        )
        keyEvent.text = nil
        keyEvent.composing = false
        keyEvent.unshifted_codepoint = unshiftedCodepointFromEvent(event)
        return keyEvent
    }

    @discardableResult
    private func sendGhosttyKeyEvent(
        for event: NSEvent,
        action: ghostty_input_action_e,
        translationMods: NSEvent.ModifierFlags,
        textEvent: NSEvent? = nil,
        textOverride: String? = nil,
        composing: Bool
    ) -> Bool {
        let traceStartedAt = terminalInputTraceStart()
        var keyEvent = ghosttyKeyEvent(
            for: event,
            action: action,
            translationMods: translationMods
        )
        keyEvent.composing = composing

        let text = textOverride ?? textEvent.flatMap { textForKeyEvent($0) }
        let handled: Bool
        if let text, shouldSendText(text) {
            handled = text.withCString { cString in
                keyEvent.text = cString
                return surfaceController.sendKey(keyEvent)
            }
        } else {
            handled = surfaceController.sendKey(keyEvent)
        }

        traceTerminalInputDuration(
            "sendGhosttyKeyEvent-end",
            event: event,
            startedAt: traceStartedAt,
            details: "action=\(action.rawValue) text_len=\((text ?? "").count) composing=\(composing) handled=\(handled)"
        )
        return handled
    }

    private func sendCommittedPreeditText(
        _ text: String,
        action: ghostty_input_action_e
    ) {
        var keyEvent = ghostty_input_key_s()
        keyEvent.action = action
        keyEvent.keycode = 0
        keyEvent.mods = GHOSTTY_MODS_NONE
        keyEvent.consumed_mods = GHOSTTY_MODS_NONE
        keyEvent.text = nil
        keyEvent.composing = false
        keyEvent.unshifted_codepoint = 0

        text.withCString { cString in
            keyEvent.text = cString
            _ = surfaceController.sendKey(keyEvent)
        }
    }

    private func shouldReplayCommittedPreeditKey(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 0x7C, 0x7D, 0x7E:
            return true
        case 0x7B:
            return !event.modifierFlags.isDisjoint(with: [.shift, .control, .option, .command])
        default:
            return false
        }
    }

    private func unshiftedCodepointFromEvent(_ event: NSEvent) -> UInt32 {
        guard event.type != .flagsChanged else {
            return 0
        }
        guard let chars = event.characters(byApplyingModifiers: []) ?? event.charactersIgnoringModifiers ?? event.characters,
              let scalar = chars.unicodeScalars.first else {
            return 0
        }
        return scalar.value
    }

    private func textForKeyEvent(_ event: NSEvent) -> String? {
        guard let chars = event.characters, !chars.isEmpty else { return nil }

        if chars.count == 1, let scalar = chars.unicodeScalars.first {
            if isControlCharacter(scalar) {
                return event.characters(byApplyingModifiers: event.modifierFlags.subtracting(.control))
            }

            if scalar.value >= 0xF700 && scalar.value <= 0xF8FF {
                return nil
            }
        }

        return chars
    }

    private func isControlCharacter(_ scalar: UnicodeScalar) -> Bool {
        scalar.value < 0x20 || scalar.value == 0x7F
    }

    private func shouldSendText(_ text: String) -> Bool {
        guard !text.isEmpty else { return false }
        if text.count == 1, let scalar = text.unicodeScalars.first {
            return !isControlCharacter(scalar)
        }
        return true
    }

    private func routeNativeKeyCommandIfNeeded(_ event: NSEvent) -> Bool {
        switch surfaceController.routeKeyboard(
            terminalKeyInput(for: event),
            hasMarkedText: markedText.length > 0
        ) {
        case .nativeCommand("find"):
            return routeFindCommandToShellHost()
        case .nativeCommand("quit"):
            return false
        case .nativeCommand,
             .shellAction,
             .shellActionLookupFailed,
             .terminalKey,
             .interpretTextInput,
             .drop:
            return false
        }
    }

    private func routeShellActionKeyIfNeeded(_ event: NSEvent) -> Bool {
        let decision = surfaceController.routeKeyboard(
            terminalKeyInput(for: event),
            hasMarkedText: markedText.length > 0
        )
        guard case .shellAction(let actionID, let target) = decision else {
            if case .shellActionLookupFailed = decision {
                return true
            }
            return false
        }
        if actionID == .findOpen {
            return routeFindCommandToShellHost(target: target)
        }
        shellActionHandler?(actionID, target)
        return true
    }

    private func routeFindCommandToShellHost(target: ShellActionTarget = .currentSelection) -> Bool {
        guard let shellActionHandler else {
            return beginFindInteraction()
        }
        let resolvedTarget: ShellActionTarget
        if case .currentSelection = target,
           let paneID = pane?.paneID
        {
            resolvedTarget = .contextPane(paneID)
        } else {
            resolvedTarget = target
        }
        shellActionHandler(.findOpen, resolvedTarget)
        return true
    }

    private func terminalKeyInput(for event: NSEvent) -> AlanTerminalKeyInput {
        let phase: AlanTerminalKeyPhase
        switch event.type {
        case .keyDown:
            phase = .down
        case .keyUp:
            phase = .up
        case .flagsChanged:
            phase = .flagsChanged
        default:
            phase = .down
        }
        return AlanTerminalKeyInput(
            characters: event.charactersIgnoringModifiers ?? event.characters,
            keyCode: event.keyCode,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            phase: phase,
            isRepeat: event.isARepeat
        )
    }

    private func terminalKeyModifiers(from flags: NSEvent.ModifierFlags) -> AlanTerminalKeyModifiers {
        var modifiers: AlanTerminalKeyModifiers = []
        if flags.contains(.shift) { modifiers.insert(.shift) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.command) { modifiers.insert(.command) }
        return modifiers
    }

    private func isApplicationReservedKeyEquivalent(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return false }

        let flags = event.modifierFlags
            .intersection(.deviceIndependentFlagsMask)
            .subtracting([.capsLock, .numericPad, .function])
        guard flags == .command else { return false }

        return event.charactersIgnoringModifiers?.lowercased() == "q"
    }

    private func terminalKeyEquivalentInput(for event: NSEvent) -> AlanTerminalKeyEquivalentInput {
        AlanTerminalKeyEquivalentInput(
            characters: event.characters,
            charactersIgnoringModifiers: event.charactersIgnoringModifiers,
            modifiers: terminalKeyModifiers(from: event.modifierFlags),
            keyCode: event.keyCode,
            timestamp: event.timestamp,
            isRepeat: event.isARepeat
        )
    }

    private func terminalKeyEquivalentEvent(
        equivalent: String,
        basedOn event: NSEvent
    ) -> NSEvent? {
        NSEvent.keyEvent(
            with: .keyDown,
            location: event.locationInWindow,
            modifierFlags: event.modifierFlags,
            timestamp: event.timestamp,
            windowNumber: event.windowNumber,
            context: nil,
            characters: equivalent,
            charactersIgnoringModifiers: equivalent,
            isARepeat: event.isARepeat,
            keyCode: event.keyCode
        )
    }

    private func syncPreedit(clearIfNeeded: Bool = true) {
#if canImport(GhosttyKit)
        if markedText.length > 0 {
            surfaceController.sendPreedit(markedText.string)
        } else if clearIfNeeded {
            surfaceController.sendPreedit(nil)
        }
#endif
    }

    // MARK: NSTextInputClient

    func insertText(_ string: Any, replacementRange: NSRange) {
        let characters: String
        switch string {
        case let value as NSAttributedString:
            characters = value.string
        case let value as String:
            characters = value
        default:
            return
        }

        let wasComposing = markedText.length > 0
        unmarkText()
        guard !AlanTerminalTextCompositionPolicy.shouldSuppressComposingControlInput(
            characters,
            composing: wasComposing
        ) else { return }

        if var keyTextAccumulator {
            keyTextAccumulator.append(characters)
            self.keyTextAccumulator = keyTextAccumulator
        } else {
#if canImport(GhosttyKit)
            surfaceController.sendProgrammaticText(characters)
#endif
        }
    }

    override func insertText(_ insertString: Any) {
        insertText(insertString, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    override func doCommand(by selector: Selector) {
#if canImport(GhosttyKit)
        if let current = NSApp.currentEvent,
           keyEquivalentAdapter.shouldRedispatchDoCommand(currentEventTimestamp: current.timestamp)
        {
            NSApp.sendEvent(current)
        }
#endif
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        switch string {
        case let value as NSAttributedString:
            markedText = NSMutableAttributedString(attributedString: value)
        case let value as String:
            markedText = NSMutableAttributedString(string: value)
        default:
            return
        }

        if keyTextAccumulator == nil {
            syncPreedit()
        }
    }

    func unmarkText() {
        guard markedText.length > 0 else { return }
        markedText.mutableString.setString("")
        syncPreedit()
    }

    func selectedRange() -> NSRange {
#if canImport(GhosttyKit)
        if let selection = surfaceController.readSelectionText() {
            return NSRange(location: 0, length: selection.utf16.count)
        }
#endif
        return NSRange(location: NSNotFound, length: 0)
    }

    func markedRange() -> NSRange {
        markedText.length > 0
            ? NSRange(location: 0, length: markedText.length)
            : NSRange(location: NSNotFound, length: 0)
    }

    func hasMarkedText() -> Bool {
        markedText.length > 0
    }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
#if canImport(GhosttyKit)
        guard let selection = surfaceController.readSelectionText(), !selection.isEmpty else { return nil }
        actualRange?.pointee = NSRange(location: 0, length: selection.utf16.count)
        return NSAttributedString(string: selection)
#else
        return nil
#endif
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
#if canImport(GhosttyKit)
        if let imeRect = surfaceController.imeRect(in: self) {
            return imeRect
        }
#endif
        guard let window else { return frame }
        return window.convertToScreen(convert(bounds, to: nil))
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }

    @discardableResult
    func beginFindInteraction() -> Bool {
        guard surfaceController.beginSearch() else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func beginLastCommandOutputSearch() -> Bool {
        guard surfaceController.beginLastCommandOutputSearch() else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func navigateSemanticPrompt(_ direction: AlanTerminalPromptNavigationDirection) -> Bool {
        guard surfaceController.navigateSemanticPrompt(direction) else { return false }
        syncNativeScrollback()
        publishRuntimeSnapshot()
        return true
    }

    func copyLastCommandOutput() -> Bool {
        surfaceController.copyLastCommandOutput(to: .general)
    }

    @discardableResult
    func updateFindQuery(_ query: String) -> Bool {
        guard surfaceController.updateSearchQuery(query) else { return false }
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        return true
    }

    func selectNextFindMatch() {
        surfaceController.nextSearchMatch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
    }

    func selectPreviousFindMatch() {
        surfaceController.previousSearchMatch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
    }

    func dismissFindInteraction(refocusTerminal: Bool) {
        surfaceController.dismissSearch()
        syncOverlayVisibility()
        publishRuntimeSnapshot()
        if refocusTerminal {
            requestTerminalFocus()
        }
    }
}

private struct AlanTerminalInputTraceConfig {
    let isEnabled: Bool
    let fileURL: URL?
}

private final class AlanTerminalInputTrace {
    private let environment: [String: String]
    private let defaults: UserDefaults
    private let fileManager: FileManager
    private let lock = NSLock()
    private let timestampFormatter: ISO8601DateFormatter
    private let configRefreshInterval: TimeInterval = 0.5
    private var cachedConfig: AlanTerminalInputTraceConfig?
    private var lastConfigRefresh = Date.distantPast

    var isEnabled: Bool {
        currentConfig().isEnabled
    }

    init(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        defaults: UserDefaults = .standard,
        fileManager: FileManager = .default
    ) {
        self.environment = environment
        self.defaults = defaults
        self.fileManager = fileManager
        timestampFormatter = ISO8601DateFormatter()
        timestampFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    }

    func log(_ message: @autoclosure () -> String) {
        let config = currentConfig()
        guard config.isEnabled, let fileURL = config.fileURL else { return }

        let line = "\(timestampFormatter.string(from: Date())) \(message())\n"
        guard let data = line.data(using: .utf8) else { return }

        lock.lock()
        defer { lock.unlock() }

        do {
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            if !FileManager.default.fileExists(atPath: fileURL.path) {
                FileManager.default.createFile(atPath: fileURL.path, contents: nil)
            }
            let handle = try FileHandle(forWritingTo: fileURL)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
        } catch {
            // Tracing is diagnostic-only and must never affect terminal input delivery.
        }
    }

    private func currentConfig() -> AlanTerminalInputTraceConfig {
        let now = Date()

        lock.lock()
        defer { lock.unlock() }

        if let cachedConfig,
           now.timeIntervalSince(lastConfigRefresh) < configRefreshInterval
        {
            return cachedConfig
        }

        defaults.synchronize()
        let config = Self.resolveConfig(
            environment: environment,
            defaults: defaults,
            fileManager: fileManager
        )
        cachedConfig = config
        lastConfigRefresh = now
        return config
    }

    private static func resolveConfig(
        environment: [String: String],
        defaults: UserDefaults,
        fileManager: FileManager
    ) -> AlanTerminalInputTraceConfig {
        AlanTerminalInputTraceConfig(
            isEnabled: isTruthy(environment["ALAN_TERMINAL_INPUT_TRACE"])
                || defaults.bool(forKey: "AlanTerminalInputTraceEnabled"),
            fileURL: resolveFileURL(
                environment: environment,
                defaults: defaults,
                fileManager: fileManager
            )
        )
    }

    private static func resolveFileURL(
        environment: [String: String],
        defaults: UserDefaults,
        fileManager: FileManager
    ) -> URL? {
        if let override = environment["ALAN_TERMINAL_INPUT_TRACE_PATH"],
           !override.isEmpty {
            return URL(fileURLWithPath: expandingHomeDirectory(in: override))
        }
        if let override = defaults.string(forKey: "AlanTerminalInputTracePath"),
           !override.isEmpty {
            return URL(fileURLWithPath: expandingHomeDirectory(in: override))
        }

        guard let libraryURL = fileManager.urls(for: .libraryDirectory, in: .userDomainMask).first else {
            return nil
        }
        return libraryURL
            .appendingPathComponent("Logs", isDirectory: true)
            .appendingPathComponent("Alan", isDirectory: true)
            .appendingPathComponent("terminal-input-trace.log", isDirectory: false)
    }

    private static func expandingHomeDirectory(in path: String) -> String {
        guard path == "~" || path.hasPrefix("~/") else { return path }
        return NSHomeDirectory() + String(path.dropFirst())
    }

    private static func isTruthy(_ value: String?) -> Bool {
        guard let value = value?.lowercased() else { return false }
        return value == "1" || value == "true" || value == "yes" || value == "on"
    }
}

private enum AlanKeyboardLayout {
    static var currentID: String? {
        guard let source = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
              let sourceIDPointer = TISGetInputSourceProperty(source, kTISPropertyInputSourceID)
        else {
            return nil
        }

        let sourceID = unsafeBitCast(sourceIDPointer, to: CFString.self)
        return sourceID as String
    }
}

#endif
