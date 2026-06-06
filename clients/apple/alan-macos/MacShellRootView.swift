import SwiftUI

#if os(macOS)
struct MacShellRootView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @ObservedObject private var host: ShellHostController
    @Binding private var appearanceMode: ShellAppearanceMode
    @Binding private var isSidebarCollapsed: Bool
    @State private var isSidebarPanelRevealed = false
    @State private var areFloatingSidebarTrafficLightsVisible = false
    @State private var sidebarRevealToken = 0
    @State private var floatingSidebarTrafficLightRevealToken = 0
    @State private var sidebarPinMorphToken = 0
    @State private var isSidebarPinMorphActive = false
    @State private var pinnedSidebarPresentationProgress: CGFloat
    @State private var sidebarPinMorphProgress: CGFloat = 1
    @State private var windowChromeMetrics = ShellWindowChromeMetrics()
    @State private var systemColorScheme = ShellAppearanceMode.currentSystemColorScheme
    @State private var isCollapsedSidebarPointerRetained = false
    private let sidebarWidth: CGFloat = 264
    private let floatingSidebarInset: CGFloat = 6
    private let floatingSidebarTrafficLightRevealDelay: TimeInterval = 0.08
    private let sidebarPinMorphDuration: TimeInterval = 0.18

    init(
        host: ShellHostController,
        appearanceMode: Binding<ShellAppearanceMode> = .constant(.system),
        isSidebarCollapsed: Binding<Bool> = .constant(false)
    ) {
        self.host = host
        _appearanceMode = appearanceMode
        _isSidebarCollapsed = isSidebarCollapsed
        _pinnedSidebarPresentationProgress = State(
            initialValue: isSidebarCollapsed.wrappedValue ? 0 : 1
        )
    }

    private var isSidebarSurfaceVisible: Bool {
        sidebarPresentation.isSurfaceVisible
    }

    private var isPinnedSidebarFullyCollapsed: Bool {
        pinnedSidebarPresentationProgress <= ShellSidebarPresentationSnapshot.visibilityEpsilon
            && !isSidebarPinMorphActive
    }

    private var sidebarPinnedVisibleWidth: CGFloat {
        sidebarPresentation.layoutWidth
    }

    private var sidebarPinnedContentOffsetX: CGFloat {
        sidebarPresentation.contentOffsetX
    }

    private var sidebarPinnedContentOpacity: Double {
        sidebarPresentation.contentOpacity
    }

    private var clampedPinnedSidebarPresentationProgress: CGFloat {
        sidebarPresentation.layoutProgress
    }

    private var sidebarChromeSurfaceOrigin: CGPoint {
        sidebarPresentation.surfaceOrigin
    }

    private var canCreateTerminalSpace: Bool {
        host.spaces.count < ShellSidebarSpaceSliderLayout.maximumVisibleSpaces
    }

    private var sidebarPresentationConfiguration: ShellSidebarPresentationConfiguration {
        ShellSidebarPresentationConfiguration(
            sidebarWidth: sidebarWidth,
            floatingSidebarInset: floatingSidebarInset,
            floatingCornerRadius: ShellRadii.floatingSidebarPanel
        )
    }

    private var sidebarPresentationPhase: ShellSidebarPresentationPhase {
        ShellSidebarPresentationPhase.resolved(
            isSidebarCollapsed: isSidebarCollapsed,
            pinnedProgress: pinnedSidebarPresentationProgress,
            isFloatingPanelRevealed: isSidebarPanelRevealed,
            showsFloatingTrafficLights: areFloatingSidebarTrafficLightsVisible,
            isFloatingToPinnedMorphActive: isSidebarPinMorphActive,
            floatingToPinnedMorphProgress: sidebarPinMorphProgress
        )
    }

    private var sidebarPresentation: ShellSidebarPresentationSnapshot {
        ShellSidebarPresentationSnapshot(
            phase: sidebarPresentationPhase,
            configuration: sidebarPresentationConfiguration
        )
    }

    private var isCollapsedSidebarPointerRetentionActive: Bool {
        isSidebarCollapsed
            && isPinnedSidebarFullyCollapsed
            && isSidebarPanelRevealed
            && !isSidebarPinMorphActive
    }

    private func revealCollapsedSidebarPanel() {
        guard isSidebarCollapsed, isPinnedSidebarFullyCollapsed else { return }
        sidebarRevealToken += 1
        guard !isSidebarPanelRevealed else { return }

        areFloatingSidebarTrafficLightsVisible = false
        floatingSidebarTrafficLightRevealToken += 1
        let token = floatingSidebarTrafficLightRevealToken
        withAnimation(sidebarPanelRevealAnimation) {
            isSidebarPanelRevealed = true
        }
        scheduleFloatingSidebarTrafficLightReveal(token: token)
    }

    private func scheduleCollapsedSidebarHide() {
        guard isSidebarCollapsed else { return }
        sidebarRevealToken += 1
        let token = sidebarRevealToken
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16) {
            guard sidebarRevealToken == token, isSidebarCollapsed else { return }
            guard !isCollapsedSidebarPointerRetained else { return }
            areFloatingSidebarTrafficLightsVisible = false
            floatingSidebarTrafficLightRevealToken += 1
            withAnimation(sidebarPanelHideAnimation) {
                isSidebarPanelRevealed = false
            }
        }
    }

    private func scheduleFloatingSidebarTrafficLightReveal(token: Int) {
        let delay = reduceMotion ? 0 : floatingSidebarTrafficLightRevealDelay
        let reveal = {
            guard floatingSidebarTrafficLightRevealToken == token,
                  isSidebarCollapsed,
                  isSidebarPanelRevealed
            else {
                return
            }
            areFloatingSidebarTrafficLightsVisible = true
        }

        if delay <= 0 {
            DispatchQueue.main.async(execute: reveal)
        } else {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: reveal)
        }
    }

    private func handleCollapsedSidebarHover(_ hovering: Bool) {
        hovering ? revealCollapsedSidebarPanel() : scheduleCollapsedSidebarHide()
    }

    private func handleCollapsedSidebarToolbarHover(_ hovering: Bool) {
        guard isSidebarCollapsed, isPinnedSidebarFullyCollapsed else { return }
        handleCollapsedSidebarHover(hovering)
    }

    private func handleCollapsedSidebarPointerRetention(_ retained: Bool) {
        guard isCollapsedSidebarPointerRetentionActive else { return }
        if retained {
            sidebarRevealToken += 1
        } else {
            scheduleCollapsedSidebarHide()
        }
    }

    private var sidebarPanelRevealAnimation: Animation? {
        reduceMotion
            ? nil
            : .interactiveSpring(response: 0.28, dampingFraction: 0.86, blendDuration: 0.02)
    }

    private var sidebarPanelHideAnimation: Animation? {
        reduceMotion ? nil : .easeInOut(duration: 0.18)
    }

    private var sidebarPinnedStateAnimation: Animation? {
        reduceMotion ? nil : .easeOut(duration: 0.16)
    }

    private var sidebarPinMorphAnimation: Animation? {
        reduceMotion ? nil : .easeOut(duration: sidebarPinMorphDuration)
    }

    private var resolvedAppearanceColorScheme: ColorScheme {
        appearanceMode.resolvedColorScheme(systemColorScheme: systemColorScheme)
    }

    private func updateSidebarCollapsed(_ collapsed: Bool) {
        if collapsed {
            collapsePinnedSidebar()
        } else if shouldMorphFloatingSidebarToPinned {
            morphFloatingSidebarToPinned()
        } else {
            expandPinnedSidebar()
        }
    }

    private var shouldMorphFloatingSidebarToPinned: Bool {
        isSidebarCollapsed && isPinnedSidebarFullyCollapsed && isSidebarPanelRevealed
    }

    private func collapsePinnedSidebar() {
        sidebarPinMorphToken += 1
        isSidebarPinMorphActive = false
        sidebarPinMorphProgress = 1
        withAnimation(sidebarPinnedStateAnimation) {
            isSidebarCollapsed = true
            pinnedSidebarPresentationProgress = 0
            isSidebarPanelRevealed = false
            areFloatingSidebarTrafficLightsVisible = false
            floatingSidebarTrafficLightRevealToken += 1
        }
    }

    private func expandPinnedSidebar() {
        sidebarPinMorphToken += 1
        isSidebarPinMorphActive = false
        sidebarPinMorphProgress = 1
        withAnimation(sidebarPinnedStateAnimation) {
            isSidebarCollapsed = false
            pinnedSidebarPresentationProgress = 1
            isSidebarPanelRevealed = false
            areFloatingSidebarTrafficLightsVisible = false
            floatingSidebarTrafficLightRevealToken += 1
        }
    }

    private func morphFloatingSidebarToPinned() {
        sidebarRevealToken += 1
        floatingSidebarTrafficLightRevealToken += 1
        sidebarPinMorphToken += 1
        let token = sidebarPinMorphToken

        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            isSidebarCollapsed = false
            isSidebarPinMorphActive = true
            sidebarPinMorphProgress = 0
            pinnedSidebarPresentationProgress = 0
            areFloatingSidebarTrafficLightsVisible = true
        }

        withAnimation(sidebarPinMorphAnimation) {
            sidebarPinMorphProgress = 1
            pinnedSidebarPresentationProgress = 1
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + sidebarPinMorphDuration) {
            guard sidebarPinMorphToken == token, isSidebarPinMorphActive else { return }
            var completionTransaction = Transaction()
            completionTransaction.disablesAnimations = true
            withTransaction(completionTransaction) {
                isSidebarPanelRevealed = false
                isSidebarPinMorphActive = false
                sidebarPinMorphProgress = 1
                pinnedSidebarPresentationProgress = 1
                areFloatingSidebarTrafficLightsVisible = false
            }
        }
    }

    var body: some View {
        ZStack {
            ShellSpaceKeyboardShortcuts(host: host)

            ShellPalette.rootBacking
                .ignoresSafeArea()

            HStack(spacing: 0) {
                pinnedSidebarSurface()

                ShellWorkspaceView(
                    host: host,
                    expandedSidebarProgress: clampedPinnedSidebarPresentationProgress
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .ignoresSafeArea(edges: .top)
            }
            .frame(
                minWidth: ShellWindowSizing.minimumSize.width,
                minHeight: ShellWindowSizing.minimumSize.height
            )

            if isSidebarCollapsed && isPinnedSidebarFullyCollapsed {
                collapsedSidebarRevealZone
            }

            if sidebarPresentation.showsOverlaySurface {
                sidebarOverlaySurface
                    .transition(floatingSidebarPanelTransition)
            }

            if isSidebarSurfaceVisible {
                sidebarChromeControls
                    .transition(.opacity)
            }

        }
        .animation(sidebarPinnedStateAnimation, value: pinnedSidebarPresentationProgress)
        .environment(\.colorScheme, resolvedAppearanceColorScheme)
        .onAppear {
            pinnedSidebarPresentationProgress = isSidebarCollapsed ? 0 : 1
            sidebarPinMorphProgress = 1
            isSidebarPinMorphActive = false
        }
        .onChange(of: isSidebarCollapsed) { _, collapsed in
            guard !isSidebarPinMorphActive else { return }
            synchronizePinnedSidebarPresentation(collapsed: collapsed)
        }
        .onChange(of: isCollapsedSidebarPointerRetained) { _, retained in
            handleCollapsedSidebarPointerRetention(retained)
        }
        .background(
            ShellWindowPlacementAnimationSyncView(
                metrics: $windowChromeMetrics,
                appearanceMode: appearanceMode,
                pinnedSidebarPresentationProgress: pinnedSidebarPresentationProgress,
                sidebarPinMorphProgress: sidebarPinMorphProgress,
                isSidebarCollapsed: isSidebarCollapsed,
                isSidebarPanelRevealed: isSidebarPanelRevealed,
                areFloatingSidebarTrafficLightsVisible: areFloatingSidebarTrafficLightsVisible,
                isSidebarPinMorphActive: isSidebarPinMorphActive,
                sidebarWidth: sidebarWidth,
                floatingSidebarInset: floatingSidebarInset,
                floatingCornerRadius: ShellRadii.floatingSidebarPanel,
                systemColorScheme: $systemColorScheme,
                collapsedSidebarPointerRetentionEnabled: isCollapsedSidebarPointerRetentionActive,
                collapsedSidebarPointerRetained: $isCollapsedSidebarPointerRetained,
                windowVisibilityHandler: host.updateShellWindowVisibilityForRendering
            )
        )
    }

    @ViewBuilder
    private func pinnedSidebarSurface() -> some View {
        if sidebarPresentation.showsPinnedSurfaceContent {
            sidebarContent(isSwipeEnabled: true)
                .frame(width: sidebarWidth)
                .offset(x: sidebarPinnedContentOffsetX)
                .opacity(sidebarPinnedContentOpacity)
                .allowsHitTesting(!isSidebarCollapsed)
                .frame(width: sidebarPinnedVisibleWidth, alignment: .leading)
                .clipped()
                .ignoresSafeArea(edges: .top)
        } else {
            Color.clear
                .frame(width: sidebarPinnedVisibleWidth)
                .allowsHitTesting(false)
                .ignoresSafeArea(edges: .top)
        }
    }

    private func synchronizePinnedSidebarPresentation(collapsed: Bool) {
        withAnimation(sidebarPinnedStateAnimation) {
            pinnedSidebarPresentationProgress = collapsed ? 0 : 1
            if collapsed {
                isSidebarPanelRevealed = false
                areFloatingSidebarTrafficLightsVisible = false
                floatingSidebarTrafficLightRevealToken += 1
            } else {
                isSidebarPanelRevealed = false
            }
        }
    }

    private func sidebarContent(isSwipeEnabled: Bool = true) -> some View {
        ShellSidebarView(
            host: host,
            chromeMetrics: windowChromeMetrics,
            displaySpaceID: nil,
            isSwipeEnabled: isSwipeEnabled
        )
    }

    private var collapsedSidebarRevealZone: some View {
        Color.clear
            .frame(width: ShellSidebarMetrics.collapsedRevealEdgeWidth)
            .frame(maxHeight: .infinity, alignment: .topLeading)
            .contentShape(Rectangle())
            .onHover(perform: handleCollapsedSidebarHover)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .ignoresSafeArea()
            .zIndex(10)
    }

    private var sidebarOverlaySurface: some View {
        GeometryReader { proxy in
            let presentation = sidebarPresentation
            let surfaceFrame = presentation.visibleSurfaceFrame(in: proxy.size)
            let shape = RoundedRectangle(
                cornerRadius: presentation.cornerRadius,
                style: .continuous
            )

            ZStack(alignment: .topLeading) {
                shape
                    .fill(.clear)
                    .background {
                        ShellMaterialBackgroundView(.sidebarGlass)
                            .clipShape(shape)
                    }
                    .overlay {
                        shape
                            .stroke(ShellPalette.line.opacity(0.22), lineWidth: 0.8)
                            .opacity(Double(presentation.floatingTreatmentProgress))
                    }

                sidebarContent(isSwipeEnabled: true)
                    .clipShape(shape)
            }
            .frame(width: surfaceFrame.width, height: surfaceFrame.height, alignment: .topLeading)
            .offset(x: surfaceFrame.minX, y: surfaceFrame.minY)
            .shellShadow(
                presentation.showsFloatingShadow ? ShellShadows.floatingPanel : ShellShadows.none
            )
            .onHover(perform: handleCollapsedSidebarHover)
            .allowsHitTesting(presentation.hitTestingRole != .morphingFloatingToPinned)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .ignoresSafeArea(edges: [.top, .bottom])
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .zIndex(20)
    }

    private var floatingSidebarPanelTransition: AnyTransition {
        .asymmetric(
            insertion: .move(edge: .leading).combined(with: .opacity),
            removal: .move(edge: .leading)
        )
    }

    private var sidebarChromeControls: some View {
        GeometryReader { _ in
            HStack(spacing: 0) {
                HStack(spacing: ShellSidebarMetrics.titlebarToolSpacing) {
                    ShellSidebarCollapseControl(isCollapsed: isSidebarCollapsed) {
                        updateSidebarCollapsed(!isSidebarCollapsed)
                    }

                    ShellAppearanceModeControl(mode: $appearanceMode)
                }
                .contentShape(Rectangle())
                .onHover(perform: handleCollapsedSidebarToolbarHover)

                Spacer(minLength: ShellSidebarMetrics.titlebarToolSpacing)

                ShellSidebarNewSpaceControl {
                    _ = host.createTerminalSpace()
                }
                .disabled(!canCreateTerminalSpace)
                .opacity(canCreateTerminalSpace ? 1 : 0.45)
                .contentShape(Rectangle())
                .onHover(perform: handleCollapsedSidebarToolbarHover)
            }
            .padding(.leading, windowChromeMetrics.titlebarToolLeadingInset)
            .padding(.trailing, ShellSidebarMetrics.edgeInset)
            .padding(.top, windowChromeMetrics.titlebarToolTopInset)
            .frame(width: sidebarPresentation.surfaceWidth, alignment: .topLeading)
            .offset(x: sidebarChromeSurfaceOrigin.x, y: sidebarChromeSurfaceOrigin.y)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .ignoresSafeArea(edges: .top)
        .zIndex(30)
    }

}

private struct ShellWindowPlacementAnimationSyncView: View, Animatable {
    @Binding var metrics: ShellWindowChromeMetrics
    let appearanceMode: ShellAppearanceMode
    var pinnedSidebarPresentationProgress: CGFloat
    var sidebarPinMorphProgress: CGFloat
    let isSidebarCollapsed: Bool
    let isSidebarPanelRevealed: Bool
    let areFloatingSidebarTrafficLightsVisible: Bool
    let isSidebarPinMorphActive: Bool
    let sidebarWidth: CGFloat
    let floatingSidebarInset: CGFloat
    let floatingCornerRadius: CGFloat
    @Binding var systemColorScheme: ColorScheme
    let collapsedSidebarPointerRetentionEnabled: Bool
    @Binding var collapsedSidebarPointerRetained: Bool
    let windowVisibilityHandler: (Bool) -> Void

    var animatableData: AnimatablePair<CGFloat, CGFloat> {
        get {
            AnimatablePair(pinnedSidebarPresentationProgress, sidebarPinMorphProgress)
        }
        set {
            pinnedSidebarPresentationProgress = newValue.first
            sidebarPinMorphProgress = newValue.second
        }
    }

    var body: some View {
        ShellWindowPlacementView(
            metrics: $metrics,
            appearanceMode: appearanceMode,
            chromeSurface: windowChromeSurface,
            systemColorScheme: $systemColorScheme,
            collapsedSidebarPointerRetentionEnabled: collapsedSidebarPointerRetentionEnabled,
            collapsedSidebarPointerRetained: $collapsedSidebarPointerRetained,
            windowVisibilityHandler: windowVisibilityHandler
        )
    }

    private var windowChromeSurface: ShellWindowChromeSurface {
        sidebarPresentation.chromeSurface
    }

    private var sidebarPresentation: ShellSidebarPresentationSnapshot {
        ShellSidebarPresentationSnapshot(
            phase: sidebarPresentationPhase,
            configuration: ShellSidebarPresentationConfiguration(
                sidebarWidth: sidebarWidth,
                floatingSidebarInset: floatingSidebarInset,
                floatingCornerRadius: floatingCornerRadius
            )
        )
    }

    private var sidebarPresentationPhase: ShellSidebarPresentationPhase {
        ShellSidebarPresentationPhase.resolved(
            isSidebarCollapsed: isSidebarCollapsed,
            pinnedProgress: pinnedSidebarPresentationProgress,
            isFloatingPanelRevealed: isSidebarPanelRevealed,
            showsFloatingTrafficLights: areFloatingSidebarTrafficLightsVisible,
            isFloatingToPinnedMorphActive: isSidebarPinMorphActive,
            floatingToPinnedMorphProgress: sidebarPinMorphProgress
        )
    }
}

private struct ShellSidebarCollapseControl: View {
    let isCollapsed: Bool
    let action: () -> Void

    var body: some View {
        ShellGhostChromeButton(
            systemName: "sidebar.left",
            help: isCollapsed ? "Pin Sidebar" : "Hide Sidebar",
            accessibilityLabel: isCollapsed ? "Pin Sidebar" : "Hide Sidebar",
            action: action
        )
    }
}

private struct ShellAppearanceModeControl: View {
    @Binding var mode: ShellAppearanceMode

    var body: some View {
        ShellGhostChromeButton(
            systemName: mode.symbolName,
            help: "Appearance: \(mode.label). Click for \(mode.next.label).",
            accessibilityLabel: "Appearance",
            accessibilityValue: mode.label
        ) {
            mode = mode.next
        }
    }
}

private struct ShellSidebarNewSpaceControl: View {
    let action: () -> Void

    var body: some View {
        ShellGhostChromeButton(
            systemName: "plus",
            help: "Create Space",
            accessibilityLabel: "Create Space",
            action: action
        )
    }
}

private struct ShellGhostChromeButton: View {
    let systemName: String
    let help: String
    let accessibilityLabel: String
    var accessibilityValue: String?
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 14, weight: .semibold))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(iconForeground)
                .frame(
                    width: ShellSidebarMetrics.titlebarToolWidth,
                    height: ShellSidebarMetrics.titlebarToolHeight
                )
                .background {
                    if isHovered {
                        RoundedRectangle(cornerRadius: ShellRadii.titlebarTool, style: .continuous)
                            .fill(ShellPalette.titlebarToolGlassTint.opacity(0.20))
                            .overlay {
                                RoundedRectangle(cornerRadius: ShellRadii.titlebarTool, style: .continuous)
                                    .stroke(ShellPalette.line.opacity(0.18), lineWidth: 0.6)
                            }
                    }
                }
                .contentShape(
                    RoundedRectangle(cornerRadius: ShellRadii.titlebarTool, style: .continuous)
                )
        }
        .buttonStyle(.plain)
        .controlSize(.regular)
        .onHover { hovering in
            isHovered = hovering
        }
        .help(help)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityValue(accessibilityValue ?? "")
    }

    private var iconForeground: Color {
        isHovered
            ? ShellPalette.sidebarInk.opacity(0.84)
            : ShellPalette.sidebarMutedInk.opacity(0.76)
    }
}
#endif
