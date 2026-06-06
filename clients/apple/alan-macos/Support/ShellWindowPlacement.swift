import SwiftUI

#if os(macOS)
import AppKit

struct ShellWindowPlacementView: NSViewRepresentable {
    @Binding private var metrics: ShellWindowChromeMetrics
    let appearanceMode: ShellAppearanceMode
    var chromeSurface: ShellWindowChromeSurface = .visible
    @Binding private var systemColorScheme: ColorScheme
    var collapsedSidebarPointerRetentionEnabled = false
    @Binding private var collapsedSidebarPointerRetained: Bool
    var windowVisibilityHandler: (Bool) -> Void = { _ in }

    init(
        metrics: Binding<ShellWindowChromeMetrics>,
        appearanceMode: ShellAppearanceMode,
        chromeSurface: ShellWindowChromeSurface = .visible,
        systemColorScheme: Binding<ColorScheme>,
        collapsedSidebarPointerRetentionEnabled: Bool = false,
        collapsedSidebarPointerRetained: Binding<Bool> = .constant(false),
        windowVisibilityHandler: @escaping (Bool) -> Void = { _ in }
    ) {
        _metrics = metrics
        self.appearanceMode = appearanceMode
        self.chromeSurface = chromeSurface
        _systemColorScheme = systemColorScheme
        self.collapsedSidebarPointerRetentionEnabled = collapsedSidebarPointerRetentionEnabled
        _collapsedSidebarPointerRetained = collapsedSidebarPointerRetained
        self.windowVisibilityHandler = windowVisibilityHandler
    }

    func makeNSView(context: Context) -> ShellWindowPlacementNSView {
        let metricsBinding = _metrics
        let systemColorSchemeBinding = _systemColorScheme
        let pointerRetentionBinding = _collapsedSidebarPointerRetained
        return ShellWindowPlacementNSView(
            appearanceMode: appearanceMode,
            chromeSurface: chromeSurface,
            metricsHandler: metricsHandler(metricsBinding),
            systemAppearanceHandler: systemAppearanceHandler(systemColorSchemeBinding),
            collapsedSidebarPointerRetentionEnabled: collapsedSidebarPointerRetentionEnabled,
            collapsedSidebarPointerRetentionHandler: pointerRetentionHandler(pointerRetentionBinding),
            windowVisibilityHandler: windowVisibilityHandler
        )
    }

    func updateNSView(_ nsView: ShellWindowPlacementNSView, context: Context) {
        let metricsBinding = _metrics
        let systemColorSchemeBinding = _systemColorScheme
        let pointerRetentionBinding = _collapsedSidebarPointerRetained
        nsView.updateAppearanceMode(appearanceMode)
        nsView.updateMetricsHandler(metricsHandler(metricsBinding))
        nsView.updateSystemAppearanceHandler(systemAppearanceHandler(systemColorSchemeBinding))
        nsView.updateCollapsedSidebarPointerRetentionHandler(
            pointerRetentionHandler(pointerRetentionBinding)
        )
        nsView.updateWindowVisibilityHandler(windowVisibilityHandler)
        nsView.updateChromeSurface(chromeSurface)
        nsView.updateCollapsedSidebarPointerRetention(
            enabled: collapsedSidebarPointerRetentionEnabled
        )
        nsView.resolveWindowIfNeeded()
    }

    private func metricsHandler(
        _ metricsBinding: Binding<ShellWindowChromeMetrics>
    ) -> (ShellWindowChromeMetrics) -> Void {
        { metrics in
            DispatchQueue.main.async {
                guard metricsBinding.wrappedValue != metrics else { return }
                metricsBinding.wrappedValue = metrics
            }
        }
    }

    private func systemAppearanceHandler(
        _ systemColorSchemeBinding: Binding<ColorScheme>
    ) -> (ColorScheme) -> Void {
        { colorScheme in
            DispatchQueue.main.async {
                guard systemColorSchemeBinding.wrappedValue != colorScheme else { return }
                systemColorSchemeBinding.wrappedValue = colorScheme
            }
        }
    }

    private func pointerRetentionHandler(
        _ pointerRetentionBinding: Binding<Bool>
    ) -> (Bool) -> Void {
        { retained in
            DispatchQueue.main.async {
                guard pointerRetentionBinding.wrappedValue != retained else { return }
                pointerRetentionBinding.wrappedValue = retained
            }
        }
    }
}

enum ShellWindowRenderVisibility {
    static func isVisible(_ window: NSWindow?) -> Bool {
        guard let window else { return false }
        return window.isVisible
            && !window.isMiniaturized
            && window.occlusionState.contains(.visible)
    }
}

struct ShellWindowChromeSurface: Equatable {
    var isVisible = true
    var origin: CGPoint = .zero
    var width: CGFloat?
    var showsStandardTrafficLights = true

    static let visible = ShellWindowChromeSurface()
}

struct ShellWindowChromeMetrics: Equatable {
    var standardTrafficLightsVisible = true
    var trafficLightGroupFrame = CGRect(
        x: ShellSidebarMetrics.trafficLightLeadingInset,
        y: ShellSidebarMetrics.trafficLightTopInset,
        width: ShellSidebarMetrics.trafficLightFallbackGroupWidth,
        height: ShellSidebarMetrics.trafficLightFallbackButtonHeight
    )

    var titlebarToolLeadingInset: CGFloat {
        guard standardTrafficLightsVisible else {
            return ShellSidebarMetrics.edgeInset
        }

        return trafficLightGroupFrame.maxX + ShellSidebarMetrics.titlebarToolGapAfterTrafficLights
    }

    var titlebarToolTopInset: CGFloat {
        max(
            0,
            trafficLightGroupFrame.midY - (ShellSidebarMetrics.titlebarToolHeight / 2)
        )
    }

    var commandLauncherTopInset: CGFloat {
        trafficLightGroupFrame.maxY + ShellSidebarMetrics.commandLauncherGapBelowTrafficLights
    }

    var collapsedRevealHeaderHeight: CGFloat {
        commandLauncherTopInset + ShellSidebarMetrics.commandLauncherHeight + 8
    }
}

enum ShellCollapsedSidebarPointerRetention {
    static let leftResizeFrameWidth: CGFloat = 16

    static func contains(
        locationInWindow point: CGPoint,
        windowSize: CGSize,
        chromeSurface: ShellWindowChromeSurface
    ) -> Bool {
        contains(
            locationInWindow: point,
            windowSize: windowSize,
            chromeSurface: chromeSurface,
            edgeWidth: ShellSidebarMetrics.collapsedRevealEdgeWidth,
            leftResizeFrameWidth: leftResizeFrameWidth
        )
    }

    static func contains(
        locationInWindow point: CGPoint,
        windowSize: CGSize,
        chromeSurface: ShellWindowChromeSurface,
        edgeWidth: CGFloat,
        leftResizeFrameWidth: CGFloat
    ) -> Bool {
        guard windowSize.width > 0, windowSize.height > 0 else { return false }

        let verticalBounds = ClosedRange(
            uncheckedBounds: (
                lower: -leftResizeFrameWidth,
                upper: windowSize.height + leftResizeFrameWidth
            )
        )
        guard verticalBounds.contains(point.y) else { return false }

        if point.x >= -leftResizeFrameWidth
            && point.x <= max(edgeWidth, leftResizeFrameWidth) {
            return true
        }

        guard chromeSurface.isVisible, let surfaceWidth = chromeSurface.width else {
            return false
        }

        let surfaceMinX = chromeSurface.origin.x
        let surfaceMaxX = surfaceMinX + surfaceWidth
        if point.x >= surfaceMinX && point.x <= surfaceMaxX {
            return true
        }

        return ShellWindowDoubleClickZoomHitTesting.isPointInSidebarChromeControls(
            point,
            windowSize: windowSize,
            chromeSurface: chromeSurface
        )
    }
}

enum ShellWindowSizing {
    static let defaultWidthRatio: CGFloat = 0.90
    static let defaultHeightRatio: CGFloat = 0.80
    static let visibleFrameMargin: CGFloat = 16
    static let zoomFrameTolerance: CGFloat = 1
    static let minimumSize = CGSize(width: 1180, height: 760)

    static func defaultFrame(in visibleFrame: CGRect) -> CGRect {
        let size = defaultSize(in: visibleFrame)

        return CGRect(
            x: visibleFrame.midX - (size.width / 2),
            y: visibleFrame.midY - (size.height / 2),
            width: size.width,
            height: size.height
        )
    }

    static func defaultSize(in visibleFrame: CGRect) -> CGSize {
        let maxWidth = max(visibleFrame.width - (visibleFrameMargin * 2), 1)
        let maxHeight = max(visibleFrame.height - (visibleFrameMargin * 2), 1)

        return CGSize(
            width: min(max(visibleFrame.width * defaultWidthRatio, minimumSize.width), maxWidth),
            height: min(max(visibleFrame.height * defaultHeightRatio, minimumSize.height), maxHeight)
        )
    }

    static func zoomFrame(in visibleFrame: CGRect) -> CGRect {
        visibleFrame
    }

    static func frame(
        _ lhs: CGRect,
        approximatelyMatches rhs: CGRect,
        tolerance: CGFloat = zoomFrameTolerance
    ) -> Bool {
        abs(lhs.origin.x - rhs.origin.x) <= tolerance
            && abs(lhs.origin.y - rhs.origin.y) <= tolerance
            && abs(lhs.width - rhs.width) <= tolerance
            && abs(lhs.height - rhs.height) <= tolerance
    }
}

enum ShellWindowDoubleClickZoomHitTesting {
    static let topChromeBandHeight: CGFloat = 36
    static let contentInteractionTopInset: CGFloat = ShellWorkspaceMetrics.workspacePanelInset

    static func isWindowTopChromeZoomCandidate(
        locationInWindow point: CGPoint,
        in window: NSWindow,
        chromeSurface: ShellWindowChromeSurface = .visible
    ) -> Bool {
        let windowSize = window.frame.size
        let windowBounds = CGRect(origin: .zero, size: windowSize)
        guard windowBounds.contains(point) else { return false }
        guard isPointInWindowTopChromeBand(point, windowSize: windowSize) else { return false }
        if isPointAboveContentInteractionBand(point, windowSize: windowSize) {
            return true
        }

        guard isPointInSidebarChromeBand(point, windowSize: windowSize, chromeSurface: chromeSurface)
        else {
            return false
        }
        return !isPointInSidebarChromeControls(
            point,
            windowSize: windowSize,
            chromeSurface: chromeSurface
        )
    }

    static func isPointInWindowTopChromeBand(
        _ point: CGPoint,
        windowSize: CGSize
    ) -> Bool {
        let visualTopInset = windowSize.height - point.y
        return visualTopInset >= 0 && visualTopInset <= topChromeBandHeight
    }

    static func isPointAboveContentInteractionBand(
        _ point: CGPoint,
        windowSize: CGSize
    ) -> Bool {
        let visualTopInset = windowSize.height - point.y
        return visualTopInset >= 0 && visualTopInset <= contentInteractionTopInset
    }

    static func isPointInSidebarChromeBand(
        _ point: CGPoint,
        windowSize: CGSize,
        chromeSurface: ShellWindowChromeSurface
    ) -> Bool {
        guard chromeSurface.isVisible, let chromeSurfaceWidth = chromeSurface.width else {
            return false
        }

        let localPoint = localTopLeadingPoint(
            point,
            windowSize: windowSize,
            chromeSurface: chromeSurface
        )
        return localPoint.x >= 0
            && localPoint.x <= chromeSurfaceWidth
            && localPoint.y >= 0
            && localPoint.y <= topChromeBandHeight
    }

    static func isPointInSidebarChromeControls(
        _ point: CGPoint,
        windowSize: CGSize,
        chromeSurface: ShellWindowChromeSurface
    ) -> Bool {
        let localPoint = localTopLeadingPoint(
            point,
            windowSize: windowSize,
            chromeSurface: chromeSurface
        )
        let hitsTrailingTitlebarControl =
            chromeSurface.width.map {
                sidebarTrailingTitlebarToolControlFrame(surfaceWidth: $0).contains(localPoint)
            } ?? false

        return standardTrafficLightControlFrames.contains { $0.contains(localPoint) }
            || sidebarLeadingTitlebarToolControlFrame.contains(localPoint)
            || hitsTrailingTitlebarControl
    }

    private static var standardTrafficLightGroupFrame: CGRect {
        CGRect(
            x: ShellSidebarMetrics.trafficLightLeadingInset,
            y: ShellSidebarMetrics.trafficLightTopInset,
            width: ShellSidebarMetrics.trafficLightFallbackGroupWidth,
            height: ShellSidebarMetrics.trafficLightFallbackButtonHeight
        )
    }

    private static var standardTrafficLightControlFrames: [CGRect] {
        let groupFrame = standardTrafficLightGroupFrame
        let buttonWidth = ShellSidebarMetrics.trafficLightFallbackButtonHeight
        let gap = max((groupFrame.width - (buttonWidth * 3)) / 2, 0)

        return (0..<3).map { index in
            CGRect(
                x: groupFrame.minX + (CGFloat(index) * (buttonWidth + gap)),
                y: groupFrame.minY,
                width: buttonWidth,
                height: buttonWidth
            )
        }
    }

    private static var sidebarLeadingTitlebarToolControlFrame: CGRect {
        let trafficLights = standardTrafficLightGroupFrame
        let firstButtonX = trafficLights.maxX + ShellSidebarMetrics.titlebarToolGapAfterTrafficLights
        let buttonCount: CGFloat = 2
        let totalButtonWidth =
            (buttonCount * ShellSidebarMetrics.titlebarToolWidth)
            + ((buttonCount - 1) * ShellSidebarMetrics.titlebarToolSpacing)

        return CGRect(
            x: firstButtonX,
            y: max(0, trafficLights.midY - (ShellSidebarMetrics.titlebarToolHeight / 2)),
            width: totalButtonWidth,
            height: ShellSidebarMetrics.titlebarToolHeight
        )
    }

    private static func sidebarTrailingTitlebarToolControlFrame(surfaceWidth: CGFloat) -> CGRect {
        CGRect(
            x: surfaceWidth
                - ShellSidebarMetrics.edgeInset
                - ShellSidebarMetrics.titlebarToolWidth,
            y: titlebarToolY,
            width: ShellSidebarMetrics.titlebarToolWidth,
            height: ShellSidebarMetrics.titlebarToolHeight
        )
    }

    private static var titlebarToolY: CGFloat {
        let trafficLights = standardTrafficLightGroupFrame
        return max(0, trafficLights.midY - (ShellSidebarMetrics.titlebarToolHeight / 2))
    }

    private static func localTopLeadingPoint(
        _ point: CGPoint,
        windowSize: CGSize,
        chromeSurface: ShellWindowChromeSurface
    ) -> CGPoint {
        let visualTopInset = windowSize.height - point.y
        return CGPoint(
            x: point.x - chromeSurface.origin.x,
            y: visualTopInset - chromeSurface.origin.y
        )
    }
}

struct ShellWindowCloseGuardView: NSViewRepresentable {
    let shouldClose: @MainActor () -> Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(shouldClose: shouldClose)
    }

    func makeNSView(context: Context) -> ShellWindowCloseGuardNSView {
        ShellWindowCloseGuardNSView(coordinator: context.coordinator)
    }

    func updateNSView(_ nsView: ShellWindowCloseGuardNSView, context: Context) {
        context.coordinator.shouldClose = shouldClose
        nsView.resolveWindowIfNeeded()
    }

    @MainActor
    final class Coordinator: NSObject, NSWindowDelegate {
        var shouldClose: @MainActor () -> Bool
        weak var previousDelegate: NSWindowDelegate?

        init(shouldClose: @escaping @MainActor () -> Bool) {
            self.shouldClose = shouldClose
        }

        func windowShouldClose(_ sender: NSWindow) -> Bool {
            if let previousDelegate,
               previousDelegate !== self,
               previousDelegate.responds(to: #selector(NSWindowDelegate.windowShouldClose(_:)))
            {
                guard previousDelegate.windowShouldClose?(sender) ?? true else {
                    return false
                }
            }
            return shouldClose()
        }
    }
}

final class ShellWindowCloseGuardNSView: NSView {
    private let coordinator: ShellWindowCloseGuardView.Coordinator
    private weak var observedWindow: NSWindow?

    init(coordinator: ShellWindowCloseGuardView.Coordinator) {
        self.coordinator = coordinator
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        resolveWindowIfNeeded()
    }

    func resolveWindowIfNeeded() {
        DispatchQueue.main.async { [weak self] in
            guard let self, let window = self.window else { return }
            guard self.observedWindow !== window else { return }
            self.observedWindow = window
            self.coordinator.previousDelegate = window.delegate
            window.delegate = self.coordinator
        }
    }
}

final class ShellWindowPlacementNSView: NSView {
    private var appearanceMode: ShellAppearanceMode
    private var chromeSurface: ShellWindowChromeSurface
    private var metricsHandler: (ShellWindowChromeMetrics) -> Void
    private var systemAppearanceHandler: (ColorScheme) -> Void
    private var configuredWindowNumber: Int?
    private var lastPublishedMetrics: ShellWindowChromeMetrics?
    private var doubleClickZoomOverlay: ShellWindowDoubleClickZoomOverlayView?
    private weak var observedWindow: NSWindow?
    private weak var observedTitlebarView: NSView?
    private var windowObservers: [NSObjectProtocol] = []
    private var titlebarObservers: [NSObjectProtocol] = []
    private var chromeSyncScheduled = false
    private var isSynchronizingChrome = false
    private var liveResizeChromeSyncTimer: Timer?
    private var nativeFullScreenOverride: Bool?
    private var collapsedSidebarPointerRetentionEnabled = false
    private var collapsedSidebarPointerRetained = false
    private var collapsedSidebarPointerRetentionHandler: (Bool) -> Void
    private var windowVisibilityHandler: (Bool) -> Void
    private var lastPublishedWindowVisibility: Bool?
    private var pointerRetentionEventMonitor: Any?

    init(
        appearanceMode: ShellAppearanceMode,
        chromeSurface: ShellWindowChromeSurface = .visible,
        metricsHandler: @escaping (ShellWindowChromeMetrics) -> Void,
        systemAppearanceHandler: @escaping (ColorScheme) -> Void = { _ in },
        collapsedSidebarPointerRetentionEnabled: Bool = false,
        collapsedSidebarPointerRetentionHandler: @escaping (Bool) -> Void = { _ in },
        windowVisibilityHandler: @escaping (Bool) -> Void = { _ in }
    ) {
        self.appearanceMode = appearanceMode
        self.chromeSurface = chromeSurface
        self.metricsHandler = metricsHandler
        self.systemAppearanceHandler = systemAppearanceHandler
        self.collapsedSidebarPointerRetentionEnabled = collapsedSidebarPointerRetentionEnabled
        self.collapsedSidebarPointerRetentionHandler = collapsedSidebarPointerRetentionHandler
        self.windowVisibilityHandler = windowVisibilityHandler
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        stopLiveResizeChromeSync()
        removePointerRetentionEventMonitor()
        removeWindowObservers()
        removeTitlebarObservers()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        resolveWindowIfNeeded()
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        resolveWindowIfNeeded()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        publishEffectiveSystemColorScheme()
    }

    override func layout() {
        super.layout()
        guard window?.inLiveResize == true else { return }
        synchronizeChromeIfAttached()
    }

    func updateMetricsHandler(_ handler: @escaping (ShellWindowChromeMetrics) -> Void) {
        metricsHandler = handler
    }

    func updateSystemAppearanceHandler(_ handler: @escaping (ColorScheme) -> Void) {
        systemAppearanceHandler = handler
        publishEffectiveSystemColorScheme()
    }

    func updateWindowVisibilityHandler(_ handler: @escaping (Bool) -> Void) {
        windowVisibilityHandler = handler
        publishCurrentWindowVisibility()
    }

    func updateAppearanceMode(_ mode: ShellAppearanceMode) {
        let didChange = appearanceMode != mode
        appearanceMode = mode
        guard didChange else { return }
        applyAppearanceToAttachedWindow()
        publishEffectiveSystemColorScheme()
    }

    func updateChromeSurface(_ surface: ShellWindowChromeSurface) {
        let didChange = chromeSurface != surface
        chromeSurface = surface
        updateCollapsedSidebarPointerRetentionState()
        guard didChange else { return }
        synchronizeChromeIfAttached()
    }

    func updateCollapsedSidebarPointerRetentionHandler(_ handler: @escaping (Bool) -> Void) {
        collapsedSidebarPointerRetentionHandler = handler
        collapsedSidebarPointerRetentionHandler(collapsedSidebarPointerRetained)
    }

    func updateCollapsedSidebarPointerRetention(enabled: Bool) {
        let didChange = collapsedSidebarPointerRetentionEnabled != enabled
        collapsedSidebarPointerRetentionEnabled = enabled

        if enabled {
            installPointerRetentionEventMonitorIfNeeded()
            updateCollapsedSidebarPointerRetentionState()
        } else {
            removePointerRetentionEventMonitor()
            publishCollapsedSidebarPointerRetained(false)
        }

        guard didChange else { return }
        updateCollapsedSidebarPointerRetentionState()
    }

    func resolveWindowIfNeeded() {
        DispatchQueue.main.async { [weak self] in
            guard let self, let window = self.window else { return }
            if self.configuredWindowNumber != window.windowNumber {
                AlanShellWindowPlacement.configure(window, appearanceMode: self.appearanceMode)
                self.installWindowObservers(for: window)
                self.configuredWindowNumber = window.windowNumber
            }

            self.synchronizeChrome(for: window)
            self.publishWindowVisibility(for: window)
        }
    }

    private func applyAppearanceToAttachedWindow() {
        guard let window else { return }
        AlanShellWindowPlacement.applyAppearance(to: window, appearanceMode: appearanceMode)
    }

    private func publishEffectiveSystemColorScheme() {
        let appearance = window?.effectiveAppearance ?? effectiveAppearance
        systemAppearanceHandler(ShellAppearanceMode.colorScheme(for: appearance))
    }

    private func installWindowObservers(for window: NSWindow) {
        guard observedWindow !== window else { return }

        removeWindowObservers()
        observedWindow = window
        if collapsedSidebarPointerRetentionEnabled {
            installPointerRetentionEventMonitorIfNeeded()
            updateCollapsedSidebarPointerRetentionState()
        }

        let center = NotificationCenter.default
        windowObservers = [
            center.addObserver(
                forName: NSWindow.willStartLiveResizeNotification,
                object: window,
                queue: nil
            ) { [weak self] _ in
                self?.startLiveResizeChromeSync()
            },
            center.addObserver(
                forName: NSWindow.didResizeNotification,
                object: window,
                queue: nil
            ) { [weak self] _ in
                self?.synchronizeChromeIfAttached()
            },
            center.addObserver(
                forName: NSWindow.didEndLiveResizeNotification,
                object: window,
                queue: nil
            ) { [weak self] _ in
                self?.stopLiveResizeChromeSync()
                self?.synchronizeChromeIfAttached()
                self?.scheduleChromeSync(after: 0.02)
            },
            center.addObserver(
                forName: NSWindow.didChangeScreenNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.scheduleChromeSync()
                self?.publishCurrentWindowVisibility()
            },
            center.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.publishCurrentWindowVisibility()
            },
            center.addObserver(
                forName: NSWindow.didMiniaturizeNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.publishCurrentWindowVisibility()
            },
            center.addObserver(
                forName: NSWindow.didDeminiaturizeNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.publishCurrentWindowVisibility()
            },
            center.addObserver(
                forName: NSWindow.willEnterFullScreenNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.nativeFullScreenOverride = true
                self?.scheduleChromeSync()
            },
            center.addObserver(
                forName: NSWindow.didEnterFullScreenNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.nativeFullScreenOverride = nil
                self?.scheduleChromeSync()
            },
            center.addObserver(
                forName: NSWindow.willExitFullScreenNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.nativeFullScreenOverride = true
                self?.scheduleChromeSync()
            },
            center.addObserver(
                forName: NSWindow.didExitFullScreenNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                self?.nativeFullScreenOverride = nil
                self?.scheduleChromeSync()
                self?.scheduleChromeSync(after: 0.08)
            },
        ]
    }

    private func removeWindowObservers() {
        stopLiveResizeChromeSync()
        removePointerRetentionEventMonitor()
        windowObservers.forEach(NotificationCenter.default.removeObserver)
        windowObservers.removeAll()
        observedWindow = nil
        publishWindowVisibility(for: nil)
    }

    private func publishCurrentWindowVisibility() {
        publishWindowVisibility(for: observedWindow ?? window)
    }

    private func publishWindowVisibility(for targetWindow: NSWindow?) {
        let isVisible = ShellWindowRenderVisibility.isVisible(targetWindow)
        guard lastPublishedWindowVisibility != isVisible else { return }
        lastPublishedWindowVisibility = isVisible
        windowVisibilityHandler(isVisible)
    }

    private func installTitlebarObservers(for titlebarView: NSView?) {
        guard observedTitlebarView !== titlebarView else { return }

        removeTitlebarObservers()
        guard let titlebarView else { return }

        observedTitlebarView = titlebarView
        titlebarView.postsFrameChangedNotifications = true
        titlebarView.postsBoundsChangedNotifications = true

        let center = NotificationCenter.default
        titlebarObservers = [
            center.addObserver(
                forName: NSView.frameDidChangeNotification,
                object: titlebarView,
                queue: nil
            ) { [weak self] _ in
                self?.synchronizeChromeIfAttached()
            },
            center.addObserver(
                forName: NSView.boundsDidChangeNotification,
                object: titlebarView,
                queue: nil
            ) { [weak self] _ in
                self?.synchronizeChromeIfAttached()
            },
        ]
    }

    private func removeTitlebarObservers() {
        titlebarObservers.forEach(NotificationCenter.default.removeObserver)
        titlebarObservers.removeAll()
        observedTitlebarView = nil
    }

    private func scheduleChromeSync(after delay: TimeInterval = 0) {
        let shouldCoalesce = delay <= 0
        if shouldCoalesce {
            guard !chromeSyncScheduled else { return }
            chromeSyncScheduled = true
        }

        let work = { [weak self] in
            guard let self else { return }
            if shouldCoalesce {
                self.chromeSyncScheduled = false
            }
            guard let window = self.window else { return }
            self.synchronizeChrome(for: window)
        }

        if delay > 0 {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
        } else {
            DispatchQueue.main.async(execute: work)
        }
    }

    private func startLiveResizeChromeSync() {
        stopLiveResizeChromeSync()
        synchronizeChromeIfAttached()

        let timer = Timer(timeInterval: 1.0 / 60.0, repeats: true) { [weak self] timer in
            guard let self else {
                timer.invalidate()
                return
            }

            guard self.window?.inLiveResize == true else {
                timer.invalidate()
                self.liveResizeChromeSyncTimer = nil
                self.synchronizeChromeIfAttached()
                return
            }

            self.synchronizeChromeIfAttached()
        }
        timer.tolerance = 1.0 / 120.0
        RunLoop.main.add(timer, forMode: .eventTracking)
        RunLoop.main.add(timer, forMode: .common)
        liveResizeChromeSyncTimer = timer
        timer.fire()
    }

    private func stopLiveResizeChromeSync() {
        liveResizeChromeSyncTimer?.invalidate()
        liveResizeChromeSyncTimer = nil
    }

    private func synchronizeChromeIfAttached() {
        guard let window else { return }
        synchronizeChrome(for: window)
    }

    private func installPointerRetentionEventMonitorIfNeeded() {
        guard pointerRetentionEventMonitor == nil else { return }
        let mask: NSEvent.EventTypeMask = [
            .mouseMoved,
            .leftMouseDragged,
            .rightMouseDragged,
            .otherMouseDragged,
        ]
        pointerRetentionEventMonitor = NSEvent.addLocalMonitorForEvents(matching: mask) {
            [weak self] event in
            self?.handlePointerRetentionEvent(event)
            return event
        }
    }

    private func removePointerRetentionEventMonitor() {
        guard let pointerRetentionEventMonitor else { return }
        NSEvent.removeMonitor(pointerRetentionEventMonitor)
        self.pointerRetentionEventMonitor = nil
    }

    private func handlePointerRetentionEvent(_ event: NSEvent) {
        guard collapsedSidebarPointerRetentionEnabled else { return }
        guard let window else {
            publishCollapsedSidebarPointerRetained(false)
            return
        }

        if let eventWindow = event.window, eventWindow !== window {
            publishCollapsedSidebarPointerRetained(false)
            return
        }

        updateCollapsedSidebarPointerRetentionState()
    }

    private func updateCollapsedSidebarPointerRetentionState() {
        guard collapsedSidebarPointerRetentionEnabled, let window else {
            publishCollapsedSidebarPointerRetained(false)
            return
        }

        let retained = ShellCollapsedSidebarPointerRetention.contains(
            locationInWindow: window.mouseLocationOutsideOfEventStream,
            windowSize: window.frame.size,
            chromeSurface: chromeSurface
        )
        publishCollapsedSidebarPointerRetained(retained)
    }

    private func publishCollapsedSidebarPointerRetained(_ retained: Bool) {
        guard collapsedSidebarPointerRetained != retained else { return }
        collapsedSidebarPointerRetained = retained
        collapsedSidebarPointerRetentionHandler(retained)
    }

    private func synchronizeChrome(for window: NSWindow) {
        guard !isSynchronizingChrome else { return }
        isSynchronizingChrome = true
        defer {
            isSynchronizingChrome = false
        }

        let titlebarView = AlanShellWindowPlacement.titlebarControlContainer(for: window)
        installTitlebarObservers(for: titlebarView)
        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            chromeSurface: chromeSurface,
            nativeFullScreenOverride: nativeFullScreenOverride
        )
        installDoubleClickZoomOverlayIfNeeded(in: titlebarView)
        publishEffectiveSystemColorScheme()
        updateCollapsedSidebarPointerRetentionState()
        guard lastPublishedMetrics != metrics else { return }
        lastPublishedMetrics = metrics
        metricsHandler(metrics)
    }

    private func installDoubleClickZoomOverlayIfNeeded(in titlebarView: NSView?) {
        guard let titlebarView else {
            doubleClickZoomOverlay?.removeFromSuperview()
            doubleClickZoomOverlay = nil
            return
        }

        if let doubleClickZoomOverlay,
           doubleClickZoomOverlay.superview === titlebarView {
            doubleClickZoomOverlay.frame = titlebarView.bounds
            doubleClickZoomOverlay.chromeSurface = chromeSurface
            return
        }

        doubleClickZoomOverlay?.removeFromSuperview()
        let overlay = ShellWindowDoubleClickZoomOverlayView(frame: titlebarView.bounds)
        overlay.chromeSurface = chromeSurface
        overlay.autoresizingMask = [.width, .height]
        titlebarView.addSubview(overlay, positioned: .below, relativeTo: titlebarView.subviews.first)
        doubleClickZoomOverlay = overlay
    }
}

final class ShellWindowDoubleClickZoomOverlayView: NSView {
    var chromeSurface = ShellWindowChromeSurface.visible
    private var restoreFrame: NSRect?

    override var mouseDownCanMoveWindow: Bool {
        true
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let window else { return nil }
        guard bounds.contains(point) else { return nil }
        let windowPoint = convert(point, to: nil)
        guard ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
            locationInWindow: windowPoint,
            in: window,
            chromeSurface: chromeSurface
        ) else {
            return nil
        }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        guard event.clickCount == 2, let window else {
            window?.performDrag(with: event)
            return
        }
        toggleVisibleFrameZoom(for: window)
    }

    private func toggleVisibleFrameZoom(for window: NSWindow) {
        let visibleFrame =
            window.screen?.visibleFrame
            ?? NSScreen.main?.visibleFrame
            ?? window.frame
        let zoomFrame = ShellWindowSizing.zoomFrame(in: visibleFrame)

        if ShellWindowSizing.frame(window.frame, approximatelyMatches: zoomFrame),
           let restoreFrame,
           !ShellWindowSizing.frame(restoreFrame, approximatelyMatches: zoomFrame) {
            window.setFrame(restoreFrame, display: true, animate: true)
            self.restoreFrame = nil
        } else {
            restoreFrame = window.frame
            window.setFrame(zoomFrame, display: true, animate: true)
        }
    }
}

enum AlanShellWindowPlacement {
    private struct StandardWindowButtonGroup {
        let buttons: [NSButton]
        let superview: NSView
    }

    static func configure(_ window: NSWindow, appearanceMode: ShellAppearanceMode) {
        window.title = "alan"
        applyAppearance(to: window, appearanceMode: appearanceMode)
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        window.styleMask.insert(.fullSizeContentView)
        window.isMovableByWindowBackground = false
        window.minSize = ShellWindowSizing.minimumSize
        window.tabbingMode = .disallowed

        if !window.isVisible {
            window.makeKeyAndOrderFront(nil)
        }

        NSApp.activate(ignoringOtherApps: true)
    }

    static func applyAppearance(to window: NSWindow, appearanceMode: ShellAppearanceMode) {
        let appearance = appearanceMode.nsAppearanceName.flatMap(NSAppearance.init(named:))
        window.appearance = appearance
        window.contentView?.appearance = appearance
        window.contentView?.needsDisplay = true
        window.displayIfNeeded()
    }

    static func synchronizeChrome(
        for window: NSWindow,
        chromeSurface: ShellWindowChromeSurface = .visible,
        nativeFullScreenOverride: Bool? = nil
    ) -> ShellWindowChromeMetrics {
        var metrics = ShellWindowChromeMetrics()
        let isNativeFullScreen = nativeFullScreenOverride ?? window.styleMask.contains(.fullScreen)

        guard chromeSurface.isVisible, !isNativeFullScreen else {
            setStandardWindowButtons(in: window, hidden: true)
            metrics.standardTrafficLightsVisible = false
            return metrics
        }

        let shouldPrimeInvisibleTrafficLights =
            chromeSurface.showsStandardTrafficLights && standardWindowButtonsAreHidden(in: window)
        if shouldPrimeInvisibleTrafficLights {
            setStandardWindowButtons(in: window, hidden: false, alphaValue: 0)
        }

        if let trafficLightGroupFrame = repositionStandardWindowButtons(
            in: window,
            chromeSurfaceOrigin: chromeSurface.origin
        ) {
            metrics.trafficLightGroupFrame = localTrafficLightGroupFrame(
                for: trafficLightGroupFrame
            )
        }
        setStandardWindowButtons(
            in: window,
            hidden: !chromeSurface.showsStandardTrafficLights,
            alphaValue: 1
        )
        if chromeSurface.showsStandardTrafficLights,
           let trafficLightGroupFrame = repositionStandardWindowButtons(
               in: window,
               chromeSurfaceOrigin: chromeSurface.origin
           )
        {
            metrics.trafficLightGroupFrame = localTrafficLightGroupFrame(
                for: trafficLightGroupFrame
            )
        }

        return metrics
    }

    static func titlebarControlContainer(for window: NSWindow) -> NSView? {
        window.standardWindowButton(.closeButton)?.superview
    }

    private static func setStandardWindowButtons(
        in window: NSWindow,
        hidden: Bool,
        alphaValue: CGFloat? = nil
    ) {
        guard let group = standardWindowButtonGroup(in: window) else { return }
        group.buttons.forEach { button in
            if let alphaValue, button.alphaValue != alphaValue {
                button.alphaValue = alphaValue
            }
            if button.isHidden != hidden {
                button.isHidden = hidden
            }
        }
    }

    private static func standardWindowButtonsAreHidden(in window: NSWindow) -> Bool {
        guard let group = standardWindowButtonGroup(in: window) else { return false }
        return group.buttons.allSatisfy(\.isHidden)
    }

    private static func localTrafficLightGroupFrame(for frame: CGRect) -> CGRect {
        CGRect(
            x: ShellSidebarMetrics.trafficLightLeadingInset,
            y: ShellSidebarMetrics.trafficLightTopInset,
            width: frame.width,
            height: frame.height
        )
    }

    private static func standardWindowButtonGroup(in window: NSWindow) -> StandardWindowButtonGroup? {
        let buttonTypes: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        let buttons = buttonTypes.compactMap { window.standardWindowButton($0) }

        guard buttons.count == buttonTypes.count,
              let superview = buttons.first?.superview,
              buttons.allSatisfy({ $0.superview === superview })
        else {
            return nil
        }

        return StandardWindowButtonGroup(buttons: buttons, superview: superview)
    }

    private static func repositionStandardWindowButtons(
        in window: NSWindow,
        chromeSurfaceOrigin: CGPoint
    ) -> CGRect? {
        guard let buttonGroup = standardWindowButtonGroup(in: window) else { return nil }
        let buttons = buttonGroup.buttons
        let superview = buttonGroup.superview

        let currentGroupFrame = buttons
            .map(\.frame)
            .reduce(NSRect.null) { $0.union($1) }
        guard !currentGroupFrame.isNull else { return nil }

        let currentVisualTopInset = visualTopInset(of: currentGroupFrame, in: superview)
        let targetLeadingInset =
            ShellSidebarMetrics.trafficLightLeadingInset + chromeSurfaceOrigin.x
        let targetTopInset =
            ShellSidebarMetrics.trafficLightTopInset + chromeSurfaceOrigin.y
        let deltaX = targetLeadingInset - currentGroupFrame.minX
        let deltaTop = targetTopInset - currentVisualTopInset
        let deltaY = superview.isFlipped ? deltaTop : -deltaTop

        if abs(deltaX) > 0.5 || abs(deltaY) > 0.5 {
            for button in buttons {
                var frame = button.frame
                frame.origin.x += deltaX
                frame.origin.y += deltaY
                button.setFrameOrigin(frame.origin)
            }
        }

        let movedGroupFrame = buttons
            .map(\.frame)
            .reduce(NSRect.null) { $0.union($1) }
        guard !movedGroupFrame.isNull else { return nil }

        return topLeadingFrame(for: movedGroupFrame, in: superview, window: window)
    }

    private static func visualTopInset(of frame: NSRect, in view: NSView) -> CGFloat {
        if view.isFlipped {
            return frame.minY
        }

        return max(0, view.bounds.height - frame.maxY)
    }

    private static func topLeadingFrame(
        for frame: NSRect,
        in view: NSView,
        window: NSWindow
    ) -> CGRect? {
        guard let contentView = window.contentView else { return nil }
        let windowFrame = view.convert(frame, to: nil)
        let contentFrame = contentView.convert(windowFrame, from: nil)
        let topInset = visualTopInset(of: contentFrame, in: contentView)

        return CGRect(
            x: contentFrame.minX,
            y: topInset,
            width: contentFrame.width,
            height: contentFrame.height
        )
    }
}
#endif
