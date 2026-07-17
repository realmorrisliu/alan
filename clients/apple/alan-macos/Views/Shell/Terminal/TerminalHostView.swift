#if os(macOS)
import AppKit
#if canImport(QuartzCore)
import QuartzCore
#endif
#if canImport(GhosttyKit)
import GhosttyKit
#endif

final class AlanTerminalHostNSView: NSView, TerminalRuntimeHandle {
    static let inputTrace = AlanTerminalInputTrace()

    private let canvasView = makeCanvasView()
    private let overlayPresenter = TerminalHostOverlayPresenter()
    let surfaceController = AlanTerminalSurfaceController()
    let keyEquivalentAdapter = AlanTerminalKeyEquivalentAdapter()
    private let runtimeReporter = TerminalHostRuntimeReporter()
    private let windowObserver = TerminalHostWindowObserver()

    var pane: ShellPane?
    private var terminalContentID: String?
    private var bootProfile: AlanShellBootProfile?
    var isSelected = false
    private var attachmentPolicy: TerminalHostAttachmentPolicy = .immediate
    private var renderPriority: TerminalRuntimeRenderPriority = .hiddenBackground
    weak var activationDelegate: TerminalHostActivationDelegate?
    var shellActionHandler: ((ShellActionID, ShellActionTarget) -> Void)?
    var clearRestoredTranscriptHandler: (() -> Void)?
    private var closeRequestHandler: ((Bool) -> Void)?
    private var runtimeObserver: ((TerminalHostRuntimeSnapshot) -> Void)?
    private var metadataObserver: ((TerminalPaneMetadataSnapshot) -> Void)?
    private var rendererSnapshot: TerminalRendererSnapshot = .placeholder
    private var paneMetadata: TerminalPaneMetadataSnapshot = .placeholder
    private var lastReportedMetadata: TerminalPaneMetadataSnapshot?
    private var trackingArea: NSTrackingArea?
    private var eventMonitor: Any?
    var markedText = NSMutableAttributedString()
    var keyTextAccumulator: [String]?
    var clearCommandTracker = AlanTerminalClearCommandTracker()
    var previousPressureStage = 0
    private var hasTornDownRuntime = false
    var pendingFocusRequest = false
    var needsWindowAttachmentFocus = false
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

    func publishRuntimeSnapshot() {
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

    func synchronizeLiveHost() {
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

    func syncNativeScrollback() {
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

    func syncOverlayVisibility() {
        let overlayState = surfaceController.overlayState(
            renderer: rendererSnapshot,
            metadata: paneMetadata,
            bootProfile: bootProfile
        )
        overlayPresenter.syncOverlay(overlayState: overlayState, bootProfile: bootProfile)
    }

    var isFocused: Bool {
        window?.firstResponder === self
    }

    var terminalInputIsActive: Bool {
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
}
#endif
