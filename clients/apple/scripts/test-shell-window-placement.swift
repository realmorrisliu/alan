import AppKit
import Foundation

@main
struct ShellWindowPlacementTestRunner {
    static func main() async throws {
        try await MainActor.run {
            try ShellWindowPlacementTests.run()
        }
    }
}

@MainActor
private enum ShellWindowPlacementTests {
    static func run() throws {
        try verifiesDefaultFrameUsesVisibleRegionRatios()
        try verifiesDefaultFrameFitsSmallVisibleRegions()
        try verifiesZoomFrameUsesEntireVisibleRegion()
        try verifiesApproximateZoomFrameMatching()
        try verifiesWorkspaceInsetFollowsPinnedSidebarProgress()
        try verifiesTitlebarToolsMoveToLeadingEdgeWhenTrafficLightsAreHidden()
        try verifiesFullscreenChromeMetricsHideTrafficLightReservation()
        try verifiesHiddenSidebarSurfaceHidesTrafficLights()
        try verifiesFloatingSidebarSurfaceRevealsTrafficLightsAtSurfaceOrigin()
        try verifiesPinnedSidebarMotionMovesTrafficLightsWithSurfaceOrigin()
        try verifiesPendingFloatingSidebarRevealHidesTrafficLightsButReservesLayout()
        try verifiesFloatingSidebarRevealAfterPendingHiddenStateUsesSurfaceOrigin()
        try verifiesCollapsedSidebarRetentionIncludesLeftResizeFrame()
        try verifiesPrimaryShellWindowDisablesGlobalBackgroundDragging()
        try verifiesTitlebarPointOutsideContentViewCanTriggerDoubleClickZoom()
        try verifiesTerminalSurfaceTitleBarDoesNotTriggerDoubleClickZoom()
        try verifiesTrafficLightControlsDoNotTriggerDoubleClickZoom()
        try verifiesTrafficLightGapsCanTriggerDoubleClickZoom()
        try verifiesSidebarChromeBlankAreaCanTriggerDoubleClickZoom()
        try verifiesSidebarTabListDoesNotTriggerWindowDrag()
        try verifiesSidebarSpaceSliderDoesNotTriggerWindowDrag()
        try verifiesSidebarToolbarButtonsDoNotTriggerDoubleClickZoom()
        try verifiesTitlebarOverlayAcceptsTopBlankHit()
        try verifiesTitlebarOverlayAcceptsSidebarChromeBlankHit()
        try verifiesTitlebarOverlayRejectsTerminalSurfaceTitleBarHit()
        try verifiesTitlebarOverlayRejectsTrafficLightHit()
        try verifiesAppearanceModeAppliesToAttachedWindowImmediately()
        try verifiesSystemModeClearsExplicitWindowAppearanceImmediately()
        try verifiesCloseGuardDefersSideEffectsUntilPreviousDelegateApproves()
        try verifiesCloseGuardRunsAfterPreviousDelegateApproval()
        print("Shell window placement tests passed.")
    }

    private static func verifiesDefaultFrameUsesVisibleRegionRatios() throws {
        let visibleFrame = CGRect(x: 10, y: 40, width: 1600, height: 1000)

        let frame = ShellWindowSizing.defaultFrame(in: visibleFrame)

        expect(frame.width == 1440, "default window width must use 90% of visible width")
        expect(frame.height == 800, "default window height must use 80% of visible height")
        expect(frame.midX == visibleFrame.midX, "default window frame must be horizontally centered")
        expect(frame.midY == visibleFrame.midY, "default window frame must be vertically centered")
    }

    private static func verifiesDefaultFrameFitsSmallVisibleRegions() throws {
        let visibleFrame = CGRect(x: 0, y: 80, width: 1000, height: 700)

        let frame = ShellWindowSizing.defaultFrame(in: visibleFrame)

        expect(
            visibleFrame.contains(frame),
            "default window frame must stay inside the visible screen region"
        )
        expect(frame.width == 968, "default window width must clamp to visible width with margins")
        expect(frame.height == 668, "default window height must clamp to visible height with margins")
    }

    private static func verifiesZoomFrameUsesEntireVisibleRegion() throws {
        let visibleFrame = CGRect(x: -120, y: 60, width: 1920, height: 1080)

        let frame = ShellWindowSizing.zoomFrame(in: visibleFrame)

        expect(frame == visibleFrame, "zoom frame must match the screen visible region exactly")
    }

    private static func verifiesApproximateZoomFrameMatching() throws {
        let visibleFrame = CGRect(x: 0, y: 60, width: 1512, height: 889)
        let nearlyZoomed = CGRect(x: 0.5, y: 59.5, width: 1511.5, height: 889.5)
        let notZoomed = CGRect(x: 20, y: 80, width: 1200, height: 760)

        expect(
            ShellWindowSizing.frame(nearlyZoomed, approximatelyMatches: visibleFrame),
            "zoom frame matching must allow AppKit rounding differences"
        )
        expect(
            !ShellWindowSizing.frame(notZoomed, approximatelyMatches: visibleFrame),
            "zoom frame matching must reject clearly different frames"
        )
    }

    private static func verifiesWorkspaceInsetFollowsPinnedSidebarProgress() throws {
        let expandedInsets = ShellWorkspaceMetrics.terminalSurfaceInsets(
            expandedSidebarProgress: 1
        )
        let midTransitionInsets = ShellWorkspaceMetrics.terminalSurfaceInsets(
            expandedSidebarProgress: 0.5
        )
        let collapsedInsets = ShellWorkspaceMetrics.terminalSurfaceInsets(
            expandedSidebarProgress: 0
        )

        expect(expandedInsets.leading == 0, "expanded sidebar must remove terminal leading inset")
        expect(
            midTransitionInsets.leading == ShellWorkspaceMetrics.terminalSurfaceInset / 2,
            "workspace leading inset must move continuously with pinned sidebar progress"
        )
        expect(
            collapsedInsets.leading == ShellWorkspaceMetrics.terminalSurfaceInset,
            "collapsed pinned sidebar must restore terminal leading inset"
        )
    }

    private static func verifiesTitlebarToolsMoveToLeadingEdgeWhenTrafficLightsAreHidden() throws {
        var metrics = ShellWindowChromeMetrics()
        metrics.standardTrafficLightsVisible = false

        expect(
            metrics.titlebarToolLeadingInset == ShellSidebarMetrics.edgeInset,
            "titlebar tools must move to the leading edge when native fullscreen hides traffic lights"
        )
    }

    private static func verifiesFullscreenChromeMetricsHideTrafficLightReservation() throws {
        let window = makeTestWindow()

        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            nativeFullScreenOverride: true
        )

        expect(
            !metrics.standardTrafficLightsVisible,
            "native fullscreen metrics must stop reserving leading space for traffic lights"
        )
        expect(
            metrics.titlebarToolLeadingInset == ShellSidebarMetrics.edgeInset,
            "native fullscreen titlebar tools must align to the leading edge"
        )
    }

    private static func verifiesHiddenSidebarSurfaceHidesTrafficLights() throws {
        let window = makeTestWindow()

        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            chromeSurface: ShellWindowChromeSurface(isVisible: false)
        )

        expect(
            !metrics.standardTrafficLightsVisible,
            "hidden sidebar surfaces must stop reserving titlebar space for traffic lights"
        )
        expect(
            standardWindowButtons(in: window).allSatisfy(\.isHidden),
            "hidden sidebar surfaces must hide standard traffic-light controls"
        )
    }

    private static func verifiesFloatingSidebarSurfaceRevealsTrafficLightsAtSurfaceOrigin() throws {
        let window = makeTestWindow()
        let surfaceOrigin = CGPoint(x: 6, y: 6)

        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            chromeSurface: ShellWindowChromeSurface(isVisible: true, origin: surfaceOrigin)
        )
        let actualFrame = actualTrafficLightFrame(in: window)

        expect(
            standardWindowButtons(in: window).allSatisfy { !$0.isHidden },
            "visible floating sidebar surfaces must show standard traffic-light controls"
        )
        expect(
            actualFrame.minX.isApproximately(
                ShellSidebarMetrics.trafficLightLeadingInset + surfaceOrigin.x
            ),
            "floating sidebar traffic lights must move to the visible surface origin; actual \(actualFrame.minX), expected \(ShellSidebarMetrics.trafficLightLeadingInset + surfaceOrigin.x)"
        )
        expect(
            actualFrame.minY.isApproximately(
                ShellSidebarMetrics.trafficLightTopInset + surfaceOrigin.y
            ),
            "floating sidebar traffic lights must move down with the visible surface origin; actual \(actualFrame.minY), expected \(ShellSidebarMetrics.trafficLightTopInset + surfaceOrigin.y)"
        )
        expect(
            metrics.trafficLightGroupFrame.minX.isApproximately(
                ShellSidebarMetrics.trafficLightLeadingInset
            ),
            "published chrome metrics must remain local to the sidebar surface"
        )
        expect(
            metrics.trafficLightGroupFrame.minY.isApproximately(
                ShellSidebarMetrics.trafficLightTopInset
            ),
            "published chrome metrics must not include the floating surface offset; actual \(metrics.trafficLightGroupFrame.minY), expected \(ShellSidebarMetrics.trafficLightTopInset)"
        )
    }

    private static func verifiesPinnedSidebarMotionMovesTrafficLightsWithSurfaceOrigin() throws {
        let window = makeTestWindow()
        let surfaceOrigin = CGPoint(x: -132, y: 0)

        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            chromeSurface: ShellWindowChromeSurface(
                isVisible: true,
                origin: surfaceOrigin,
                width: 264
            )
        )
        let actualFrame = actualTrafficLightFrame(in: window)

        expect(
            standardWindowButtons(in: window).allSatisfy { !$0.isHidden },
            "pinned sidebar motion must keep native traffic lights visible until the surface is hidden"
        )
        expect(
            actualFrame.minX.isApproximately(
                ShellSidebarMetrics.trafficLightLeadingInset + surfaceOrigin.x
            ),
            "pinned sidebar traffic lights must move with the surface origin"
        )
        expect(
            metrics.trafficLightGroupFrame.minX.isApproximately(
                ShellSidebarMetrics.trafficLightLeadingInset
            ),
            "published pinned chrome metrics must remain local to the moving sidebar surface"
        )
    }

    private static func verifiesPendingFloatingSidebarRevealHidesTrafficLightsButReservesLayout() throws {
        let window = makeTestWindow()
        let surfaceOrigin = CGPoint(x: 6, y: 6)

        let metrics = AlanShellWindowPlacement.synchronizeChrome(
            for: window,
            chromeSurface: ShellWindowChromeSurface(
                isVisible: true,
                origin: surfaceOrigin,
                showsStandardTrafficLights: false
            )
        )

        expect(
            standardWindowButtons(in: window).allSatisfy(\.isHidden),
            "pending floating sidebar reveal must keep standard traffic lights hidden"
        )
        expect(
            metrics.standardTrafficLightsVisible,
            "pending floating sidebar reveal must reserve the traffic-light layout space"
        )
        expect(
            metrics.titlebarToolLeadingInset
                > ShellSidebarMetrics.edgeInset + ShellSidebarMetrics.titlebarToolWidth,
            "titlebar tools must not jump to the leading edge while traffic lights wait for panel reveal timing"
        )
    }

    private static func verifiesFloatingSidebarRevealAfterPendingHiddenStateUsesSurfaceOrigin() throws {
        let window = makeTestWindow()
        let surfaceOrigin = CGPoint(x: 6, y: 6)
        let pendingSurface = ShellWindowChromeSurface(
            isVisible: true,
            origin: surfaceOrigin,
            showsStandardTrafficLights: false
        )
        let revealedSurface = ShellWindowChromeSurface(
            isVisible: true,
            origin: surfaceOrigin,
            showsStandardTrafficLights: true
        )

        _ = AlanShellWindowPlacement.synchronizeChrome(for: window, chromeSurface: pendingSurface)
        _ = AlanShellWindowPlacement.synchronizeChrome(for: window, chromeSurface: revealedSurface)

        let actualFrame = actualTrafficLightFrame(in: window)
        expect(
            standardWindowButtons(in: window).allSatisfy { !$0.isHidden },
            "floating sidebar traffic lights must become visible after pending reveal"
        )
        expect(
            actualFrame.minX.isApproximately(
                ShellSidebarMetrics.trafficLightLeadingInset + surfaceOrigin.x
            ),
            "floating sidebar traffic lights must not flash at the non-floating x position after pending reveal; actual \(actualFrame.minX), expected \(ShellSidebarMetrics.trafficLightLeadingInset + surfaceOrigin.x)"
        )
        expect(
            actualFrame.minY.isApproximately(
                ShellSidebarMetrics.trafficLightTopInset + surfaceOrigin.y
            ),
            "floating sidebar traffic lights must not flash at the non-floating y position after pending reveal; actual \(actualFrame.minY), expected \(ShellSidebarMetrics.trafficLightTopInset + surfaceOrigin.y)"
        )
    }

    private static func verifiesCollapsedSidebarRetentionIncludesLeftResizeFrame() throws {
        let windowSize = CGSize(width: 1512, height: 889)
        let chromeSurface = ShellWindowChromeSurface(
            isVisible: true,
            origin: CGPoint(x: 6, y: 6),
            width: 264
        )

        expect(
            ShellCollapsedSidebarPointerRetention.contains(
                locationInWindow: CGPoint(x: -3, y: 420),
                windowSize: windowSize,
                chromeSurface: chromeSurface
            ),
            "collapsed sidebar retention must include the adjacent left resize frame"
        )
        expect(
            ShellCollapsedSidebarPointerRetention.contains(
                locationInWindow: CGPoint(x: 12, y: 420),
                windowSize: windowSize,
                chromeSurface: chromeSurface
            ),
            "collapsed sidebar retention must include the narrow edge reveal neighborhood"
        )
        expect(
            !ShellCollapsedSidebarPointerRetention.contains(
                locationInWindow: CGPoint(x: 420, y: 420),
                windowSize: windowSize,
                chromeSurface: chromeSurface
            ),
            "collapsed sidebar retention must end outside the related sidebar and resize regions"
        )
    }

    private static func verifiesTitlebarPointOutsideContentViewCanTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )

        let location = CGPoint(x: 420, y: window.frame.height - 4)

        expect(
            ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window
            ),
            "top titlebar blank points must trigger double-click visible-frame zoom"
        )
    }

    private static func verifiesPrimaryShellWindowDisablesGlobalBackgroundDragging() throws {
        let window = makeTestWindow()
        window.isMovableByWindowBackground = true

        AlanShellWindowPlacement.configure(window, appearanceMode: .system)

        expect(
            !window.isMovableByWindowBackground,
            "primary shell window must not use global background dragging"
        )
    }

    private static func verifiesTerminalSurfaceTitleBarDoesNotTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(
            x: 760,
            y: window.frame.height - ShellWorkspaceMetrics.terminalSurfaceInset - 12
        )

        expect(
            !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "terminal surface title-bar controls must receive clicks instead of the window zoom overlay"
        )
    }

    private static func verifiesTrafficLightControlsDoNotTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(x: 42, y: window.frame.height - 18)

        expect(
            !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "traffic light region must keep its standard button behavior"
        )
    }

    private static func verifiesTrafficLightGapsCanTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(x: 32, y: window.frame.height - 18)

        expect(
            ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "blank gaps in the traffic-light titlebar area must trigger double-click visible-frame zoom"
        )
    }

    private static func verifiesSidebarChromeBlankAreaCanTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(x: 190, y: window.frame.height - 20)

        expect(
            ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "blank sidebar chrome beside the traffic lights and toolbar controls must trigger double-click visible-frame zoom"
        )
    }

    private static func verifiesSidebarTabListDoesNotTriggerWindowDrag() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(x: 128, y: window.frame.height - 128)

        expect(
            !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "sidebar tab-list rows must not be treated as window drag or zoom candidates"
        )
    }

    private static func verifiesSidebarSpaceSliderDoesNotTriggerWindowDrag() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)

        let location = CGPoint(x: 128, y: 56)

        expect(
            !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                locationInWindow: location,
                in: window,
                chromeSurface: chromeSurface
            ),
            "sidebar Space slider controls must not be treated as window drag or zoom candidates"
        )
    }

    private static func verifiesSidebarToolbarButtonsDoNotTriggerDoubleClickZoom() throws {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let chromeSurface = ShellWindowChromeSurface(width: 264)
        let controlY = window.frame.height - 20

        let leadingControlLocation = CGPoint(x: 98, y: controlY)
        let newSpaceControlLocation = CGPoint(
            x: 264
                - ShellSidebarMetrics.edgeInset
                - (ShellSidebarMetrics.titlebarToolWidth / 2),
            y: controlY
        )

        for location in [leadingControlLocation, newSpaceControlLocation] {
            expect(
                !ShellWindowDoubleClickZoomHitTesting.isWindowTopChromeZoomCandidate(
                    locationInWindow: location,
                    in: window,
                    chromeSurface: chromeSurface
                ),
                "sidebar titlebar toolbar buttons must receive clicks instead of the window zoom overlay"
            )
        }
    }

    private static func verifiesTitlebarOverlayAcceptsTopBlankHit() throws {
        let window = makeTestWindow()
        let overlay = ShellWindowDoubleClickZoomOverlayView(
            frame: CGRect(origin: .zero, size: window.frame.size)
        )
        window.contentView?.addSubview(overlay)

        let windowPoint = CGPoint(x: 420, y: window.frame.height - 4)
        let overlayPoint = overlay.convert(windowPoint, from: nil)

        expect(
            overlay.hitTest(overlayPoint) === overlay,
            "titlebar overlay must accept top blank titlebar hits"
        )
    }

    private static func verifiesTitlebarOverlayAcceptsSidebarChromeBlankHit() throws {
        let window = makeTestWindow()
        let overlay = ShellWindowDoubleClickZoomOverlayView(
            frame: CGRect(origin: .zero, size: window.frame.size)
        )
        overlay.chromeSurface = ShellWindowChromeSurface(width: 264)
        window.contentView?.addSubview(overlay)

        let windowPoint = CGPoint(x: 190, y: window.frame.height - 20)
        let overlayPoint = overlay.convert(windowPoint, from: nil)

        expect(
            overlay.hitTest(overlayPoint) === overlay,
            "titlebar overlay must accept blank sidebar chrome hits below the top content inset"
        )
    }

    private static func verifiesTitlebarOverlayRejectsTerminalSurfaceTitleBarHit() throws {
        let window = makeTestWindow()
        let overlay = ShellWindowDoubleClickZoomOverlayView(
            frame: CGRect(origin: .zero, size: window.frame.size)
        )
        overlay.chromeSurface = ShellWindowChromeSurface(width: 264)
        window.contentView?.addSubview(overlay)

        let windowPoint = CGPoint(
            x: window.frame.width - 40,
            y: window.frame.height - ShellWorkspaceMetrics.terminalSurfaceInset - 12
        )
        let overlayPoint = overlay.convert(windowPoint, from: nil)

        expect(
            overlay.hitTest(overlayPoint) == nil,
            "titlebar overlay must leave terminal surface title-bar controls clickable"
        )
    }

    private static func verifiesTitlebarOverlayRejectsTrafficLightHit() throws {
        let window = makeTestWindow()
        let overlay = ShellWindowDoubleClickZoomOverlayView(
            frame: CGRect(origin: .zero, size: window.frame.size)
        )
        overlay.chromeSurface = ShellWindowChromeSurface(width: 264)
        window.contentView?.addSubview(overlay)

        let windowPoint = CGPoint(x: 42, y: window.frame.height - 18)
        let overlayPoint = overlay.convert(windowPoint, from: nil)

        expect(
            overlay.hitTest(overlayPoint) == nil,
            "titlebar overlay must leave traffic light hits to standard window buttons"
        )
    }

    private static func verifiesAppearanceModeAppliesToAttachedWindowImmediately() throws {
        let (window, placementView) = makeAttachedPlacementView()

        placementView.updateAppearanceMode(.dark)

        expect(
            window.appearance?.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua,
            "dark appearance mode must apply to the attached window immediately"
        )
    }

    private static func verifiesSystemModeClearsExplicitWindowAppearanceImmediately() throws {
        let (window, placementView) = makeAttachedPlacementView()
        placementView.updateAppearanceMode(.dark)

        placementView.updateAppearanceMode(.system)

        expect(
            window.appearance == nil,
            "system appearance mode must clear the explicit window appearance immediately"
        )
    }

    private static func verifiesCloseGuardDefersSideEffectsUntilPreviousDelegateApproves() throws {
        let window = makeTestWindow()
        let previousDelegate = FakeWindowCloseDelegate(responses: [false])
        var shouldCloseCalls = 0
        let coordinator = ShellWindowCloseGuardView.Coordinator {
            shouldCloseCalls += 1
            return true
        }
        coordinator.previousDelegate = previousDelegate

        let shouldClose = coordinator.windowShouldClose(window)

        expect(!shouldClose, "close guard must honor a previous delegate veto")
        expect(previousDelegate.callCount == 1, "close guard must consult the previous delegate")
        expect(
            shouldCloseCalls == 0,
            "close guard must not run close side effects when the previous delegate vetoes"
        )
    }

    private static func verifiesCloseGuardRunsAfterPreviousDelegateApproval() throws {
        let window = makeTestWindow()
        let previousDelegate = FakeWindowCloseDelegate(responses: [true])
        var shouldCloseCalls = 0
        let coordinator = ShellWindowCloseGuardView.Coordinator {
            shouldCloseCalls += 1
            return true
        }
        coordinator.previousDelegate = previousDelegate

        let shouldClose = coordinator.windowShouldClose(window)

        expect(shouldClose, "close guard must allow close after both delegates approve")
        expect(previousDelegate.callCount == 1, "close guard must keep previous delegate chaining")
        expect(shouldCloseCalls == 1, "close guard must run its own close check after approval")
    }

    private static func makeAttachedPlacementView() -> (
        window: NSWindow,
        placementView: ShellWindowPlacementNSView
    ) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 240),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        let contentView = NSView(frame: window.contentView?.bounds ?? .zero)
        let placementView = ShellWindowPlacementNSView(appearanceMode: .system) { _ in }

        contentView.addSubview(placementView)
        window.contentView = contentView

        return (window, placementView)
    }

    private static func makeTestWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 520),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 520))
        return window
    }

    private static func standardWindowButtons(in window: NSWindow) -> [NSButton] {
        let buttonTypes: [NSWindow.ButtonType] = [.closeButton, .miniaturizeButton, .zoomButton]
        let buttons = buttonTypes.compactMap { window.standardWindowButton($0) }

        expect(
            buttons.count == buttonTypes.count,
            "test window must expose all standard window buttons"
        )

        return buttons
    }

    private static func actualTrafficLightFrame(in window: NSWindow) -> CGRect {
        let buttons = standardWindowButtons(in: window)
        guard let superview = buttons.first?.superview,
              buttons.allSatisfy({ $0.superview === superview })
        else {
            fatalError("test window standard buttons must share a superview")
        }

        let buttonFrame = buttons
            .map(\.frame)
            .reduce(NSRect.null) { $0.union($1) }
        let windowFrame = superview.convert(buttonFrame, to: nil)
        let topInset = max(0, window.frame.height - windowFrame.maxY)

        return CGRect(
            x: windowFrame.minX,
            y: topInset,
            width: windowFrame.width,
            height: windowFrame.height
        )
    }

    private static func expect(
        _ condition: @autoclosure () -> Bool,
        _ message: @autoclosure () -> String
    ) {
        guard condition() else {
            fatalError(message())
        }
    }

    private final class FakeWindowCloseDelegate: NSObject, NSWindowDelegate {
        private var responses: [Bool]
        private(set) var callCount = 0

        init(responses: [Bool]) {
            self.responses = responses
        }

        func windowShouldClose(_ sender: NSWindow) -> Bool {
            callCount += 1
            return responses.isEmpty ? true : responses.removeFirst()
        }
    }
}

private extension CGFloat {
    func isApproximately(_ other: CGFloat, tolerance: CGFloat = 1) -> Bool {
        abs(self - other) <= tolerance
    }
}
