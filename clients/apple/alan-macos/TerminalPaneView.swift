import Foundation
import SwiftUI

#if os(macOS)
struct TerminalPaneView: View {
    @ObservedObject var host: ShellHostController
    let tab: ShellTab?
    let spaceID: String?
    let selectedPaneID: String?
    let zoomedPaneID: String?
    let terminalSurfaceInsets: EdgeInsets
    let onClosePane: ((ShellPane) -> Void)?

    init(
        host: ShellHostController,
        tab: ShellTab? = nil,
        spaceID: String? = nil,
        selectedPaneID: String? = nil,
        zoomedPaneID: String? = nil,
        terminalSurfaceInsets: EdgeInsets,
        onClosePane: ((ShellPane) -> Void)? = nil
    ) {
        self.host = host
        self.tab = tab
        self.spaceID = spaceID
        self.selectedPaneID = selectedPaneID
        self.zoomedPaneID = zoomedPaneID
        self.terminalSurfaceInsets = terminalSurfaceInsets
        self.onClosePane = onClosePane
    }

    var body: some View {
        paneCanvas
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(terminalSurfaceInsets)
    }

    private var paneCanvas: some View {
        Group {
            if let paneTree = displayPaneTree {
                ShellPaneTreeLayoutView(
                    node: paneTree,
                    host: host,
                    selectedPaneID: displaySelectedPaneID,
                    onClosePane: onClosePane
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            } else {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Empty Space")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(ShellPalette.ink)
                    Text("Start a terminal in this space.")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(ShellPalette.mutedInk)
                    Button {
                        _ = host.performShellAutomationCommand(
                            .createTab(
                                ShellAutomationCreateTabRequest(
                                    launchTarget: .shell,
                                    spaceID: displaySpaceID,
                                    title: nil,
                                    workingDirectory: nil
                                )
                            )
                        )
                    } label: {
                        Label("New Tab", systemImage: "plus")
                            .font(.system(size: 12, weight: .semibold))
                            .padding(.horizontal, 11)
                            .frame(height: 28)
                    }
                    .buttonStyle(.plain)
                    .background {
                        ShellMaterialShape(
                            role: .controlGlassHover,
                            shape: RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                        )
                    }
                    .help("Create a tab in this space")
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                .padding(28)
            }
        }
        .modifier(ShellTerminalSurfaceFrame())
    }

    private var displayTab: ShellTab? {
        tab ?? host.selectedTab
    }

    private var displaySpaceID: String? {
        spaceID ?? host.selectedSpace?.spaceID
    }

    private var displaySelectedPaneID: String? {
        selectedPaneID ?? host.selectedPane?.paneID
    }

    private var displayPaneTree: ShellPaneTreeNode? {
        guard let tab = displayTab else { return nil }
        guard let zoomedPaneID,
              let zoomedLeaf = tab.paneTree.leafNode(containingPaneID: zoomedPaneID)
        else {
            return tab.paneTree
        }
        return zoomedLeaf
    }

    private var runtimeCard: some View {
        let runtime = host.selectedPaneRuntime
        return TerminalInfoCard(title: "Host Runtime", accent: ShellPalette.accent) {
            TerminalInfoRow(label: "Stage", value: runtime.stageLabel)
            TerminalInfoRow(label: "Focus", value: runtime.isFocused ? "focused" : "background")
            TerminalInfoRow(label: "Renderer", value: rendererKindLabel(for: runtime.renderer.kind))
            TerminalInfoRow(label: "Phase", value: runtime.renderer.phaseLabel)
            TerminalInfoRow(
                label: "Logical",
                value: sizeLabel(for: runtime.logicalSize)
            )
            TerminalInfoRow(
                label: "Backing",
                value: sizeLabel(for: runtime.backingSize)
            )
            TerminalInfoRow(
                label: "Display",
                value: runtime.displayName ?? "not attached"
            )
            TerminalInfoRow(
                label: "Display ID",
                value: host.selectedPane?.context?.displayID ?? runtime.displayID ?? "pending"
            )
            TerminalInfoRow(
                label: "Window",
                value: host.selectedPane?.context?.windowTitle ?? runtime.attachedWindowTitle ?? "pending"
            )
            TerminalInfoRow(
                label: "Status",
                value: runtime.renderer.summary
            )
            TerminalInfoRow(
                label: "Surface",
                value: surfaceReadinessLabel(runtime.surfaceState.readiness)
            )
            TerminalInfoRow(
                label: "Input",
                value: runtime.surfaceState.inputReady ? "ready" : "not ready"
            )
            TerminalInfoRow(
                label: "Mode",
                value: runtime.surfaceState.terminalMode.rawValue.replacingOccurrences(of: "_", with: " ")
            )
            TerminalInfoRow(
                label: "Renderer Health",
                value: runtime.surfaceState.rendererHealth
            )
            TerminalInfoRow(
                label: "cwd",
                value: runtime.paneMetadata.workingDirectory ?? "pending"
            )
            TerminalInfoRow(
                label: "Title",
                value: runtime.paneMetadata.title ?? "pending"
            )
            TerminalInfoRow(
                label: "Attention",
                value: runtime.paneMetadata.attention.rawValue
            )
            TerminalInfoRow(
                label: "Process",
                value: runtime.paneMetadata.processExited ? "exited" : "running"
            )
            TerminalInfoRow(
                label: "Branch",
                value: host.selectedPane?.context?.gitBranch ?? "pending"
            )
            TerminalInfoRow(
                label: "Process State",
                value: host.selectedPane?.context?.processState ?? "pending"
            )
            TerminalInfoRow(
                label: "Exit",
                value: host.selectedPane?.context?.lastCommandExitCode.map(String.init) ?? "pending"
            )
            TerminalInfoRow(
                label: "Metadata",
                value: host.selectedPane?.context?.lastMetadataAt ?? "pending"
            )

            if let failureReason = runtime.renderer.failureReason,
               !failureReason.isEmpty {
                Divider()
                    .overlay(ShellPalette.line.opacity(0.22))

                TerminalInfoRow(label: "Failure", value: failureReason)
            }

            if !runtime.renderer.recentEvents.isEmpty {
                Divider()
                    .overlay(ShellPalette.line.opacity(0.22))

                VStack(alignment: .leading, spacing: 8) {
                    Text("Recent Events")
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .textCase(.uppercase)
                        .foregroundStyle(ShellPalette.mutedInk)

                    ForEach(Array(runtime.renderer.recentEvents.enumerated()), id: \.offset) { entry in
                        TerminalMonoLine(text: entry.element)
                    }
                }
            }
        }
    }

    private var bootCard: some View {
        TerminalInfoCard(title: "Boot Contract", accent: ShellPalette.ink) {
            TerminalInfoRow(
                label: "Target",
                value: host.selectedPane?.resolvedLaunchTarget.rawValue ?? "pending"
            )
            TerminalInfoRow(
                label: "Strategy",
                value: host.selectedPaneBootProfile?.command.strategy.rawValue ?? "pending"
            )
            TerminalInfoRow(
                label: "Command",
                value: host.selectedPaneBootProfile?.launchCommandString ?? "pending"
            )
            TerminalInfoRow(
                label: "Resolved",
                value: host.selectedPaneBootProfile?.command.detail ?? "PATH lookup"
            )
            TerminalInfoRow(
                label: "cwd",
                value: host.selectedPaneBootProfile?.workingDirectory ?? "pending"
            )
            TerminalInfoRow(
                label: "Control",
                value: host.selectedPaneBootProfile?.environment["ALAN_SHELL_CONTROL_DIR"] ?? "pending"
            )
            TerminalInfoRow(
                label: "Socket",
                value: host.selectedPaneBootProfile?.environment["ALAN_SHELL_SOCKET"] ?? "pending"
            )
            TerminalInfoRow(
                label: "Binding",
                value: host.selectedPaneBootProfile?.environment["ALAN_SHELL_BINDING_FILE"] ?? "pending"
            )
            TerminalInfoRow(
                label: "Integration",
                value: host.selectedPane?.context?.shellIntegrationSource ?? "pending"
            )

            if let bootProfile = host.selectedPaneBootProfile {
                Divider()
                    .overlay(ShellPalette.line.opacity(0.22))

                VStack(alignment: .leading, spacing: 8) {
                    Text("Environment")
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .textCase(.uppercase)
                        .foregroundStyle(ShellPalette.mutedInk)

                    ForEach(Array(bootProfile.environmentPreview.prefix(4)), id: \.key) { entry in
                        TerminalMonoLine(text: "\(entry.key)=\(entry.value)")
                    }
                }

                Divider()
                    .overlay(ShellPalette.line.opacity(0.22))

                VStack(alignment: .leading, spacing: 8) {
                    Text("Command Discovery")
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .textCase(.uppercase)
                        .foregroundStyle(ShellPalette.mutedInk)

                    ForEach(Array(bootProfile.command.candidates.prefix(4))) { candidate in
                        HStack(alignment: .top, spacing: 8) {
                            Circle()
                                .fill(candidate.isPresent ? Color.green.opacity(0.9) : Color.orange.opacity(0.82))
                                .frame(width: 8, height: 8)
                                .padding(.top, 4)

                            VStack(alignment: .leading, spacing: 3) {
                                Text(candidate.label)
                                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                                    .foregroundStyle(ShellPalette.ink)
                                Text(candidate.path)
                                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                                    .foregroundStyle(ShellPalette.mutedInk)
                                    .lineLimit(2)
                            }
                        }
                    }
                }
            }
        }
    }

    private var ghosttyCard: some View {
        TerminalInfoCard(title: "Ghostty", accent: ShellPalette.accent) {
            TerminalInfoRow(
                label: "Status",
                value: host.selectedPaneBootProfile?.ghostty.summary ?? "pending"
            )
            TerminalInfoRow(
                label: "Setup",
                value: host.selectedPaneBootProfile?.ghostty.setupCommand ?? "pending"
            )

            if let candidates = host.selectedPaneBootProfile?.ghostty.candidates {
                Divider()
                    .overlay(ShellPalette.line.opacity(0.22))

                VStack(alignment: .leading, spacing: 8) {
                    Text("Discovery")
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .textCase(.uppercase)
                        .foregroundStyle(ShellPalette.mutedInk)

                    ForEach(Array(candidates.prefix(3))) { candidate in
                        HStack(alignment: .top, spacing: 8) {
                            Circle()
                                .fill(candidate.isPresent ? Color.green.opacity(0.9) : Color.orange.opacity(0.82))
                                .frame(width: 8, height: 8)
                                .padding(.top, 4)

                            VStack(alignment: .leading, spacing: 3) {
                                Text(candidate.label)
                                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                                    .foregroundStyle(ShellPalette.ink)
                                Text(candidate.path)
                                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                                    .foregroundStyle(ShellPalette.mutedInk)
                                    .lineLimit(2)
                            }
                        }
                    }
                }
            }
        }
    }

    private var alanBindingCard: some View {
        TerminalInfoCard(title: "alan binding", accent: ShellPalette.ink) {
            if let binding = host.selectedPane?.alanBinding {
                TerminalInfoRow(label: "Session", value: binding.sessionID)
                TerminalInfoRow(label: "Run", value: binding.runStatus)
                TerminalInfoRow(label: "Yield", value: binding.pendingYield ? "pending" : "none")
                TerminalInfoRow(label: "Source", value: binding.source ?? "binding file")
                TerminalInfoRow(label: "Projected", value: binding.lastProjectedAt ?? "pending")
            } else {
                Text("This pane is shell-addressable even when no alan session is projected onto it.")
                    .font(.system(size: 13, weight: .medium, design: .rounded))
                    .foregroundStyle(ShellPalette.mutedInk)
            }
        }
    }

    private func sizeLabel(for size: CGSize) -> String {
        guard size != .zero else { return "pending" }
        return "\(Int(size.width)) × \(Int(size.height))"
    }

    private func rendererKindLabel(for kind: TerminalRendererKind) -> String {
        kind.rawValue.replacingOccurrences(of: "_", with: " ")
    }

    private func surfaceReadinessLabel(_ readiness: AlanTerminalSurfaceReadiness) -> String {
        switch readiness {
        case .ready:
            return "ready"
        case .unready(let reason):
            return reason.rawValue.replacingOccurrences(of: "_", with: " ")
        }
    }
}

struct ShellQuickTerminalPeakView: View {
    @ObservedObject var host: ShellHostController

    var body: some View {
        ZStack {
            ShellMaterialBackgroundView(.windowBackdrop)
                .ignoresSafeArea()

            if let pane = host.quickTerminalPane {
                TerminalPaneView(
                    host: host,
                    tab: ShellQuickTerminalPeakModel.tab(for: pane),
                    spaceID: ShellQuickTerminalSlot.globalSpaceID,
                    selectedPaneID: pane.paneID,
                    terminalSurfaceInsets: EdgeInsets(top: 10, leading: 10, bottom: 10, trailing: 10),
                    onClosePane: { _ in
                        _ = host.requestCloseQuickTerminal()
                    }
                )
            }
        }
        .frame(minWidth: 520, minHeight: 280)
    }
}

private struct ShellTerminalSurfaceFrame: ViewModifier {
    @Environment(\.colorScheme) private var colorScheme
    private let shape = RoundedRectangle(cornerRadius: ShellRadii.terminalSurface, style: .continuous)

    func body(content: Content) -> some View {
        content
            .clipShape(shape)
            .background {
                ShellMaterialShape(
                    role: .terminalSurround,
                    shape: shape
                )
                .shellShadow(ShellShadows.terminalSurfaceRim)
                .shellShadow(ShellShadows.terminalSurface)
            }
            .overlay {
                terminalSurfaceRim
            }
    }

    private var terminalSurfaceRim: some View {
        ZStack {
            shape
                .strokeBorder(
                    ShellPalette.line.opacity(colorScheme == .light ? 0.30 : 0.26),
                    lineWidth: 0.85
                )

            shape
                .strokeBorder(
                    LinearGradient(
                        colors: [
                            Color.white.opacity(colorScheme == .light ? 0.18 : 0.07),
                            Color.white.opacity(0.015),
                            Color.black.opacity(colorScheme == .light ? 0.14 : 0.32),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ),
                    lineWidth: 0.65
                )

            shape
                .inset(by: 1)
                .strokeBorder(
                    Color.white.opacity(colorScheme == .light ? 0.06 : 0.03),
                    lineWidth: 0.4
                )
        }
        .allowsHitTesting(false)
    }
}

private struct ShellPaneTreeLayoutView: View {
    let node: ShellPaneTreeNode
    @ObservedObject var host: ShellHostController
    let selectedPaneID: String?
    let onClosePane: ((ShellPane) -> Void)?

    var body: some View {
        switch node.kind {
        case .pane:
            if let paneID = node.paneID {
                contentLeaf(for: paneID)
            }
        case .split:
            ShellSplitLayoutView(
                node: node,
                host: host,
                selectedPaneID: selectedPaneID,
                onClosePane: onClosePane
            )
        }
    }

    @ViewBuilder
    private func contentLeaf(for paneSlotID: String) -> some View {
        let contentState = host.shellState.contentStateProjection()
        let pane = host.shellState.pane(paneID: paneSlotID)
        let descriptor = ShellContentRenderingRegistry.descriptor(
            forPaneSlotID: paneSlotID,
            in: contentState,
            fallbackPane: pane
        )

        switch descriptor.renderKind {
        case .terminal:
            if let pane {
                terminalLeaf(for: pane)
            } else {
                boundedContentLeaf(descriptor: descriptor, paneSlotID: paneSlotID, backingPane: nil)
            }
        case .markdown, .settings, .unavailable:
            boundedContentLeaf(descriptor: descriptor, paneSlotID: paneSlotID, backingPane: pane)
        }
    }

    private func terminalLeaf(for pane: ShellPane) -> some View {
        ShellTerminalLeafView(
            pane: pane,
            bootProfile: host.bootProfile(for: pane),
            isSelected: selectedPaneID == pane.paneID,
            renderPriority: host.terminalRenderPriority(for: pane),
            isZoomed: host.isPaneZoomed(pane.paneID),
            canZoom: host.canZoomPane(pane.paneID),
            canMovePane: { placement in
                host.shellActionAvailability(
                    paneMoveActionID(for: placement),
                    target: .contextPane(pane.paneID)
                ).isAvailable
            },
            canCopyTerminalSelection: host.canCopyTerminalSelection(
                source: .contextMenu,
                target: .contextPane(pane.paneID)
            ),
            canPasteIntoTerminal: host.canPasteIntoTerminal(
                source: .contextMenu,
                target: .contextPane(pane.paneID)
            ),
            canOpenTerminalSearch: host.canOpenTerminalSearch(
                source: .contextMenu,
                target: .contextPane(pane.paneID)
            ),
            runtimeRegistry: host.terminalRuntimeRegistry,
            activationDelegate: host,
            onShellAction: { actionID, target in
                host.performShellAction(actionID, target: target, source: .terminalHost)
            },
            onToggleZoom: {
                if host.isPaneZoomed(pane.paneID) {
                    _ = host.unzoomSelectedTab()
                } else {
                    _ = host.zoomPane(paneID: pane.paneID)
                }
            },
            onMovePane: { placement in
                host.performShellAction(
                    paneMoveActionID(for: placement),
                    target: .contextPane(pane.paneID)
                )
            },
            onCopyTerminalSelection: {
                host.copyTerminalSelection(
                    source: .contextMenu,
                    target: .contextPane(pane.paneID)
                )
            },
            onPasteIntoTerminal: {
                host.pasteIntoTerminalFromPasteboard(
                    source: .contextMenu,
                    target: .contextPane(pane.paneID)
                )
            },
            onOpenTerminalSearch: {
                host.openTerminalSearch(
                    source: .contextMenu,
                    target: .contextPane(pane.paneID)
                )
            },
            onClosePane: {
                if let onClosePane {
                    onClosePane(pane)
                } else {
                    host.closePaneByID(pane.paneID)
                }
            },
            onRuntimeUpdate: host.updateTerminalRuntime,
            onMetadataUpdate: { metadata in
                host.updateTerminalMetadata(metadata, for: pane.paneID)
            }
        )
    }

    private func boundedContentLeaf(
        descriptor: ShellContentRenderDescriptor,
        paneSlotID: String,
        backingPane: ShellPane?
    ) -> some View {
        ShellBoundedContentLeafView(
            descriptor: descriptor,
            paneSlotID: paneSlotID,
            isSelected: selectedPaneID == paneSlotID,
            isZoomed: host.isPaneZoomed(paneSlotID),
            canZoom: host.canZoomPane(paneSlotID),
            canMovePane: { placement in
                host.shellActionAvailability(
                    paneMoveActionID(for: placement),
                    target: .contextPane(paneSlotID)
                ).isAvailable
            },
            onFocusPane: {
                _ = host.performShellAutomationCommand(.focusPane(paneID: paneSlotID))
            },
            onToggleZoom: {
                if host.isPaneZoomed(paneSlotID) {
                    _ = host.unzoomSelectedTab()
                } else {
                    _ = host.zoomPane(paneID: paneSlotID)
                }
            },
            onMovePane: { placement in
                host.performShellAction(
                    paneMoveActionID(for: placement),
                    target: .contextPane(paneSlotID)
                )
            },
            onClosePane: {
                if let backingPane,
                   let onClosePane
                {
                    onClosePane(backingPane)
                } else {
                    host.closePaneByID(paneSlotID)
                }
            }
        )
    }
}

private struct ShellSplitLayoutView: View {
    let node: ShellPaneTreeNode
    @ObservedObject var host: ShellHostController
    let selectedPaneID: String?
    let onClosePane: ((ShellPane) -> Void)?
    @State private var dragStartRatio: Double?
    @State private var dragPreviewRatio: Double?

    private var children: [ShellPaneTreeNode] {
        node.children ?? []
    }

    var body: some View {
        if children.count == 2 {
            GeometryReader { proxy in
                if node.direction == .vertical {
                    HStack(spacing: 0) {
                        ShellPaneTreeLayoutView(
                            node: children[0],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(width: primaryLength(total: proxy.size.width))
                        ShellSplitDividerView(direction: .vertical)
                            .gesture(resizeGesture(totalLength: proxy.size.width))
                        ShellPaneTreeLayoutView(
                            node: children[1],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(width: secondaryLength(total: proxy.size.width))
                    }
                } else {
                    VStack(spacing: 0) {
                        ShellPaneTreeLayoutView(
                            node: children[0],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(height: primaryLength(total: proxy.size.height))
                        ShellSplitDividerView(direction: .horizontal)
                            .gesture(resizeGesture(totalLength: proxy.size.height))
                        ShellPaneTreeLayoutView(
                            node: children[1],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(height: secondaryLength(total: proxy.size.height))
                    }
                }
            }
        } else if node.direction == .vertical {
            HStack(spacing: 0) {
                indexedChildrenWithDividers
            }
        } else {
            VStack(spacing: 0) {
                indexedChildrenWithDividers
            }
        }
    }

    @ViewBuilder
    private var indexedChildrenWithDividers: some View {
        ForEach(Array(children.enumerated()), id: \.element.id) { index, child in
            if index > 0 {
                ShellSplitDividerView(direction: node.direction ?? .vertical)
            }
            ShellPaneTreeLayoutView(
                node: child,
                host: host,
                selectedPaneID: selectedPaneID,
                onClosePane: onClosePane
            )
        }
    }

    private var dividerThickness: CGFloat { ShellSplitDividerMetrics.thickness }

    private func primaryLength(total: CGFloat) -> CGFloat {
        max((total - dividerThickness) * node.splitRatio, 0)
    }

    private func secondaryLength(total: CGFloat) -> CGFloat {
        max(total - dividerThickness - primaryLength(total: total), 0)
    }

    private func resizeGesture(totalLength: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                if dragStartRatio == nil {
                    dragStartRatio = node.splitRatio
                }
                let delta = node.direction == .vertical
                    ? value.translation.width
                    : value.translation.height
                let usableLength = max(totalLength - dividerThickness, 1)
                let nextRatio = (dragStartRatio ?? node.splitRatio) + Double(delta / usableLength)
                if host.resizeSplit(splitNodeID: node.nodeID, ratio: nextRatio, persist: false) {
                    dragPreviewRatio = nextRatio
                }
            }
            .onEnded { _ in
                if let finalRatio = dragPreviewRatio {
                    _ = host.resizeSplit(splitNodeID: node.nodeID, ratio: finalRatio, persist: true)
                }
                dragStartRatio = nil
                dragPreviewRatio = nil
            }
    }
}

private struct ShellSplitDividerView: View {
    @State private var isHovered = false
    let direction: ShellSplitDirection

    var body: some View {
        seam
            .frame(
                width: direction == .vertical ? ShellSplitDividerMetrics.thickness : nil,
                height: direction == .horizontal ? ShellSplitDividerMetrics.thickness : nil
            )
            .contentShape(Rectangle())
            .onHover { isHovered = $0 }
            .help("Resize split")
    }

    @ViewBuilder
    private var seam: some View {
        if direction == .vertical {
            HStack(spacing: 0) {
                Rectangle().fill(ShellSplitDividerTint.shadow(isHovered: isHovered))
                Rectangle().fill(ShellSplitDividerTint.highlight(isHovered: isHovered))
            }
        } else {
            VStack(spacing: 0) {
                Rectangle().fill(ShellSplitDividerTint.shadow(isHovered: isHovered))
                Rectangle().fill(ShellSplitDividerTint.highlight(isHovered: isHovered))
            }
        }
    }
}

private enum ShellSplitDividerMetrics {
    static let thickness: CGFloat = 2
}

private enum ShellSplitDividerTint {
    static func shadow(isHovered: Bool) -> Color {
        Color.black.opacity(isHovered ? 0.22 : 0.14)
    }

    static func highlight(isHovered: Bool) -> Color {
        ShellPalette.terminalSoft.opacity(isHovered ? 0.48 : 0.34)
    }
}

private struct ShellTerminalLeafView: View {
    @AppStorage("alanShellDimsInactiveSplitPanes") private var dimsInactiveSplitPanes = true

    let pane: ShellPane
    let bootProfile: AlanShellBootProfile?
    let isSelected: Bool
    let renderPriority: TerminalRuntimeRenderPriority
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let canCopyTerminalSelection: Bool
    let canPasteIntoTerminal: Bool
    let canOpenTerminalSearch: Bool
    let runtimeRegistry: TerminalRuntimeRegistry
    let activationDelegate: TerminalHostActivationDelegate?
    let onShellAction: (ShellActionID, ShellActionTarget) -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onCopyTerminalSelection: () -> Void
    let onPasteIntoTerminal: () -> Void
    let onOpenTerminalSearch: () -> Void
    let onClosePane: () -> Void
    let onRuntimeUpdate: (TerminalHostRuntimeSnapshot) -> Void
    let onMetadataUpdate: (TerminalPaneMetadataSnapshot) -> Void

    var body: some View {
        VStack(spacing: 0) {
            ShellPaneTitleBarView(
                title: shellPaneTitleBarTitle(for: pane),
                pane: pane,
                isSelected: isSelected,
                isZoomed: isZoomed,
                canZoom: canZoom,
                canMovePane: canMovePane,
                canCopyTerminalSelection: canCopyTerminalSelection,
                canPasteIntoTerminal: canPasteIntoTerminal,
                canOpenTerminalSearch: canOpenTerminalSearch,
                onFocusPane: {
                    activationDelegate?.terminalHostDidRequestActivation(paneID: pane.paneID)
                },
                onToggleZoom: onToggleZoom,
                onMovePane: onMovePane,
                onCopyTerminalSelection: onCopyTerminalSelection,
                onPasteIntoTerminal: onPasteIntoTerminal,
                onOpenTerminalSearch: onOpenTerminalSearch,
                onClosePane: onClosePane
            )

            ZStack(alignment: .topTrailing) {
                TerminalHostView(
                    pane: pane,
                    terminalContentMount: TerminalContentMount(pane: pane),
                    bootProfile: bootProfile,
                    isSelected: isSelected,
                    renderPriority: renderPriority,
                    runtimeRegistry: runtimeRegistry,
                    activationDelegate: activationDelegate,
                    onShellAction: onShellAction,
                    onCloseRequest: { requiresConfirmation in
                        guard !requiresConfirmation else { return }
                        onClosePane()
                    },
                    onRuntimeUpdate: onRuntimeUpdate,
                    onMetadataUpdate: onMetadataUpdate
                )
                .id(pane.paneID)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(ShellPalette.terminal)
                .overlay {
                    ShellInactivePaneDim(
                        isSelected: isSelected,
                        isEnabled: dimsInactiveSplitPanes
                    )
                }

                if isSelected,
                   let searchState = runtimeRegistry.snapshot(for: pane.paneID).surfaceState.search,
                   searchState.isActive
                {
                    ShellFindBarView(
                        searchState: searchState,
                        onQueryChange: { query in
                            _ = runtimeRegistry.updateFindQuery(for: pane.paneID, query: query)
                        },
                        onNext: {
                            runtimeRegistry.selectNextFindMatch(for: pane.paneID)
                        },
                        onPrevious: {
                            runtimeRegistry.selectPreviousFindMatch(for: pane.paneID)
                        },
                        onClose: {
                            runtimeRegistry.dismissFindInteraction(for: pane.paneID)
                        }
                    )
                    .padding(10)
                }
            }
        }
    }
}

private func paneMoveActionID(for placement: ShellPaneSplitDirection) -> ShellActionID {
    switch placement {
    case .left:
        return .paneMoveLeft
    case .right:
        return .paneMoveRight
    case .up:
        return .paneMoveUp
    case .down:
        return .paneMoveDown
    }
}

private enum ShellPaneTitleTypography {
    static let titleSize: CGFloat = 11
    static let accessorySize: CGFloat = 10
    static let closeSize: CGFloat = 9

    static func titleWeight(isSelected: Bool) -> Font.Weight {
        isSelected ? .medium : .regular
    }

    static let accessoryWeight: Font.Weight = .regular
    static let emphasizedAccessoryWeight: Font.Weight = .medium
    static let iconWeight: Font.Weight = .medium
    static let closeWeight: Font.Weight = .semibold
}

private enum ShellPaneTitleBarMetrics {
    static let height: CGFloat = 28
    static let minimumTitleWidth: CGFloat = 56
    static let horizontalLeadingPadding: CGFloat = 10
    static let horizontalTrailingPadding: CGFloat = 6
    static let itemSpacing: CGFloat = 8
    static let accessorySpacing: CGFloat = 8
    static let accessoryInternalSpacing: CGFloat = 4
    static let closeButtonSize: CGFloat = 22
}

private enum ShellPaneTitleBarPresentation {
    case full
    case compact
    case minimal
}

private enum ShellPaneTitleBarAccessoryMode: Equatable {
    case textAndIcon
    case iconOnly
}

private struct ShellBoundedContentLeafView: View {
    let descriptor: ShellContentRenderDescriptor
    let paneSlotID: String
    let isSelected: Bool
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let onFocusPane: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onClosePane: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            ShellContentPaneTitleBarView(
                descriptor: descriptor,
                isSelected: isSelected,
                isZoomed: isZoomed,
                canZoom: canZoom,
                canMovePane: canMovePane,
                onFocusPane: onFocusPane,
                onToggleZoom: onToggleZoom,
                onMovePane: onMovePane,
                onClosePane: onClosePane
            )

            switch descriptor.renderKind {
            case .markdown:
                ShellMarkdownContentView(descriptor: descriptor)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onFocusPane)
            case .settings:
                ShellSettingsContentView(descriptor: descriptor)
                    .contentShape(Rectangle())
                    .onTapGesture(perform: onFocusPane)
            case .terminal, .unavailable:
                boundedPlaceholder
            }
        }
    }

    private var boundedPlaceholder: some View {
        ZStack {
            ShellPalette.workspace

            VStack(spacing: 10) {
                Image(systemName: descriptor.iconName)
                    .font(.system(size: 22, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .frame(width: 32, height: 32)

                Text(descriptor.title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(ShellPalette.ink)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 260)

                Text(contentKindLabel)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(ShellPalette.mutedInk)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .onTapGesture(perform: onFocusPane)
        }
    }

    private var contentKindLabel: String {
        switch descriptor.renderKind {
        case .terminal:
            return "Terminal"
        case .markdown:
            return "Document"
        case .settings:
            return "Settings"
        case .unavailable:
            return "Unavailable"
        }
    }
}

private struct ShellMarkdownContentView: View {
    let descriptor: ShellContentRenderDescriptor
    @State private var renderedContent = AttributedString("")
    @State private var loadError: String?
    @State private var isLoading = false

    var body: some View {
        ZStack {
            ShellPalette.workspace

            ScrollView {
                if isLoading {
                    ProgressView()
                        .controlSize(.small)
                        .frame(maxWidth: .infinity, minHeight: 180)
                        .padding(24)
                } else if let loadError {
                    VStack(spacing: 8) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 20, weight: .medium))
                            .foregroundStyle(ShellPalette.mutedInk)
                        Text(loadError)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(ShellPalette.ink)
                    }
                    .frame(maxWidth: .infinity, minHeight: 180)
                    .padding(24)
                } else {
                    Text(renderedContent)
                        .font(.system(size: 13))
                        .foregroundStyle(ShellPalette.ink)
                        .lineSpacing(3)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 24)
                        .padding(.vertical, 20)
                }
            }
        }
        .task(id: markdownSource) {
            await loadMarkdown()
        }
    }

    @MainActor
    private func loadMarkdown() async {
        guard let fileURL else {
            renderedContent = AttributedString("")
            loadError = "Unable to open this document."
            isLoading = false
            return
        }

        isLoading = true
        loadError = nil
        renderedContent = AttributedString("")

        let result = await Task.detached(priority: .userInitiated) {
            ShellMarkdownContentLoader.load(fileURL: fileURL)
        }.value
        guard !Task.isCancelled else { return }

        isLoading = false
        switch result {
        case .success(let content):
            renderedContent = content
            loadError = nil
        case .failure:
            renderedContent = AttributedString("")
            loadError = "Unable to read this document."
        }
    }

    private var markdownSource: String {
        descriptor.payload?.markdown?.fileURL ?? ""
    }

    private var fileURL: URL? {
        ShellMarkdownContentLoader.fileURL(from: descriptor.payload?.markdown?.fileURL)
    }
}

private enum ShellMarkdownContentLoader {
    static func fileURL(from rawValue: String?) -> URL? {
        guard let rawValue else { return nil }
        let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return nil }

        if let url = URL(string: value),
           url.scheme != nil
        {
            return url.isFileURL ? url.standardizedFileURL : url
        }

        return URL(fileURLWithPath: NSString(string: value).expandingTildeInPath)
            .standardizedFileURL
    }

    static func load(fileURL: URL) -> ShellMarkdownContentLoadResult {
        do {
            let markdown = try String(contentsOf: fileURL, encoding: .utf8)
            let content = (try? AttributedString(markdown: markdown)) ?? AttributedString(markdown)
            return .success(content)
        } catch {
            return .failure
        }
    }
}

private enum ShellMarkdownContentLoadResult {
    case success(AttributedString)
    case failure
}

private struct ShellSettingsContentView: View {
    let descriptor: ShellContentRenderDescriptor

    @AppStorage("alanShellAppearanceMode") private var appearanceMode = ShellAppearanceMode.system
    @AppStorage("alanShellSidebarCollapsed") private var isSidebarCollapsed = false
    @AppStorage("alanShellDimsInactiveSplitPanes") private var dimsInactiveSplitPanes = true
    @State private var localSummary = ShellSettingsLocalSummary.current()
    @State private var remoteSnapshot = ShellSettingsRemoteSnapshot.unavailable(
        reason: "Daemon unavailable"
    )

    private var snapshot: ShellSettingsSurfaceSnapshot {
        ShellSettingsSurfaceSnapshot.make(remote: remoteSnapshot, local: localSummary)
    }

    var body: some View {
        ZStack {
            ShellPalette.workspace

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    ForEach(snapshot.sections) { section in
                        ShellSettingsSectionView(
                            section: section,
                            appearanceMode: $appearanceMode,
                            sidebarVisible: sidebarVisible,
                            dimsInactiveSplitPanes: $dimsInactiveSplitPanes
                        )
                    }
                }
                .frame(maxWidth: 720, alignment: .leading)
                .padding(.horizontal, 28)
                .padding(.vertical, 24)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
        }
        .task(id: descriptor.contentID ?? descriptor.title) {
            await refreshSettingsSummaries()
        }
    }

    private var sidebarVisible: Binding<Bool> {
        Binding(
            get: { !isSidebarCollapsed },
            set: { isSidebarCollapsed = !$0 }
        )
    }

    @MainActor
    private func refreshSettingsSummaries() async {
        let local = ShellSettingsLocalSummary.current()
        localSummary = local

        do {
            let client = try AlanAPIClient(baseURLString: local.daemonURL)
            async let catalogResponse = client.connectionCatalog()
            async let profilesResponse = client.listConnectionProfiles()
            async let currentResponse = client.currentConnection()
            async let skillsResponse = client.skillCatalog()

            let (catalog, profiles, current, skills) = try await (
                catalogResponse,
                profilesResponse,
                currentResponse,
                skillsResponse
            )
            remoteSnapshot = ShellSettingsRemoteSnapshot(
                accounts: ShellSettingsAccountsSummary(
                    current: ShellSettingsConnectionSelection(
                        defaultProfile: current.defaultProfile ?? profiles.defaultProfile,
                        effectiveProfile: current.effectiveProfile,
                        effectiveSource: current.effectiveSource
                    ),
                    profiles: profiles.profiles.map { profile in
                        ShellSettingsConnectionProfile(
                            profileID: profile.profileID,
                            label: profile.label,
                            provider: profile.provider,
                            credentialStatus: profile.credentialStatus,
                            settings: profile.settings,
                            isDefault: profile.isDefault
                        )
                    },
                    providers: catalog.providers.map { provider in
                        ShellSettingsConnectionProvider(
                            providerID: provider.providerID,
                            displayName: provider.displayName,
                            supportsBrowserLogin: provider.supportsBrowserLogin,
                            supportsDeviceLogin: provider.supportsDeviceLogin,
                            supportsSecretEntry: provider.supportsSecretEntry,
                            supportsLogout: provider.supportsLogout,
                            supportsTest: provider.supportsTest
                        )
                    },
                    unavailableReason: nil
                ),
                capabilities: ShellSettingsCapabilitiesSummary(
                    skills: skills.skills.map { skill in
                        ShellSettingsSkillSummary(
                            id: skill.id,
                            name: skill.name,
                            enabled: skill.enabled,
                            allowImplicitInvocation: skill.allowImplicitInvocation,
                            available: skill.available
                        )
                    },
                    unavailableReason: nil
                )
            )
        } catch {
            remoteSnapshot = .unavailable(reason: "Daemon unavailable")
        }
    }
}

private struct ShellSettingsSectionView: View {
    let section: ShellSettingsSectionModel
    @Binding var appearanceMode: ShellAppearanceMode
    let sidebarVisible: Binding<Bool>
    @Binding var dimsInactiveSplitPanes: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(section.title)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(ShellPalette.mutedInk)
                .textCase(.uppercase)

            VStack(spacing: 0) {
                ForEach(Array(section.rows.enumerated()), id: \.element.id) { index, row in
                    if index > 0 {
                        ShellSettingsDivider()
                    }

                    rowView(row)
                }
            }
            .background {
                RoundedRectangle(cornerRadius: ShellRadii.row, style: .continuous)
                    .fill(ShellPalette.panel.opacity(0.72))
            }
            .overlay {
                RoundedRectangle(cornerRadius: ShellRadii.row, style: .continuous)
                    .stroke(ShellPalette.line.opacity(0.26), lineWidth: 0.8)
            }
        }
    }

    @ViewBuilder
    private func rowView(_ row: ShellSettingsRowModel) -> some View {
        switch row.id {
        case "appearance":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Picker("Appearance", selection: $appearanceMode) {
                    ForEach(ShellAppearanceMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                .labelsHidden()
                .pickerStyle(.segmented)
                .frame(maxWidth: 240)
            }
        case "sidebar":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle("Show", isOn: sidebarVisible)
                    .toggleStyle(.switch)
            }
        case "inactiveSplitDimming":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle("Enabled", isOn: $dimsInactiveSplitPanes)
                    .toggleStyle(.switch)
            }
        default:
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                ShellSettingsValueLabel(value: row.value, mutability: row.mutability)
            }
        }
    }
}

private struct ShellSettingsRow<Accessory: View>: View {
    let systemName: String
    let title: String
    let detail: String?
    @ViewBuilder let accessory: () -> Accessory

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: systemName)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(ShellPalette.mutedInk)
                .frame(width: 18, height: 18)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(ShellPalette.ink)
                    .lineLimit(1)

                if let detail,
                   !detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                {
                    Text(detail)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(ShellPalette.mutedInk)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            Spacer(minLength: 16)

            accessory()
                .font(.system(size: 12, weight: .medium))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .frame(minHeight: 48)
    }
}

private struct ShellSettingsValueLabel: View {
    let value: String?
    let mutability: ShellSettingsRowMutability

    var body: some View {
        HStack(spacing: 6) {
            if mutability == .actionOnly {
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(ShellPalette.mutedInk)
            }

            Text(value ?? "Unavailable")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(valueStyle)
                .lineLimit(2)
                .multilineTextAlignment(.trailing)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: 230, alignment: .trailing)
    }

    private var valueStyle: some ShapeStyle {
        if value == "Unavailable" {
            return AnyShapeStyle(ShellPalette.mutedInk)
        }
        return AnyShapeStyle(ShellPalette.ink)
    }
}

private struct ShellSettingsDivider: View {
    var body: some View {
        Rectangle()
            .fill(ShellPalette.line.opacity(0.22))
            .frame(height: 0.8)
            .padding(.leading, 44)
    }
}

private struct ShellContentPaneTitleBarView: View {
    let descriptor: ShellContentRenderDescriptor
    let isSelected: Bool
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let onFocusPane: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onClosePane: () -> Void

    var body: some View {
        HStack(spacing: ShellPaneTitleBarMetrics.itemSpacing) {
            Image(systemName: descriptor.iconName)
                .font(.system(size: ShellPaneTitleTypography.accessorySize, weight: .medium))
                .foregroundStyle(ShellPalette.mutedInk)
                .frame(width: 14, height: 14)

            Text(descriptor.title)
                .font(
                    .system(
                        size: ShellPaneTitleTypography.titleSize,
                        weight: ShellPaneTitleTypography.titleWeight(isSelected: isSelected)
                    )
                )
                .foregroundStyle(ShellPalette.ink.opacity(isSelected ? 0.96 : 0.72))
                .lineLimit(1)
                .truncationMode(.middle)
                .layoutPriority(2)

            Spacer(minLength: 0)

            if canZoom {
                zoomButton
            }

            closeButton
        }
        .padding(.leading, ShellPaneTitleBarMetrics.horizontalLeadingPadding)
        .padding(.trailing, ShellPaneTitleBarMetrics.horizontalTrailingPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: ShellPaneTitleBarMetrics.height)
        .background(ShellPalette.workspace)
        .contentShape(Rectangle())
        .onTapGesture(perform: onFocusPane)
        .contextMenu {
            Button("Move Pane Left") {
                onMovePane(.left)
            }
            .disabled(!canMovePane(.left))

            Button("Move Pane Right") {
                onMovePane(.right)
            }
            .disabled(!canMovePane(.right))

            Button("Move Pane Up") {
                onMovePane(.up)
            }
            .disabled(!canMovePane(.up))

            Button("Move Pane Down") {
                onMovePane(.down)
            }
            .disabled(!canMovePane(.down))
        }
    }

    private var closeButton: some View {
        Button(action: onClosePane) {
            Image(systemName: "xmark")
                .font(
                    .system(
                        size: ShellPaneTitleTypography.closeSize,
                        weight: ShellPaneTitleTypography.closeWeight
                    )
                )
                .foregroundStyle(ShellPalette.mutedInk.opacity(isSelected ? 0.78 : 0.58))
                .frame(
                    width: ShellPaneTitleBarMetrics.closeButtonSize,
                    height: ShellPaneTitleBarMetrics.closeButtonSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: true)
        .help("Close pane")
        .accessibilityLabel("Close pane")
    }

    private var zoomButton: some View {
        Button(action: onToggleZoom) {
            Image(systemName: isZoomed ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                .font(
                    .system(
                        size: ShellPaneTitleTypography.closeSize,
                        weight: ShellPaneTitleTypography.closeWeight
                    )
                )
                .foregroundStyle(ShellPalette.mutedInk.opacity(isSelected ? 0.82 : 0.62))
                .frame(
                    width: ShellPaneTitleBarMetrics.closeButtonSize,
                    height: ShellPaneTitleBarMetrics.closeButtonSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: true)
        .help(isZoomed ? "Unzoom pane" : "Zoom pane")
        .accessibilityLabel(isZoomed ? "Unzoom pane" : "Zoom pane")
    }
}

private struct ShellPaneTitleBarView: View {
    let title: String
    let pane: ShellPane
    let isSelected: Bool
    let isZoomed: Bool
    let canZoom: Bool
    let canMovePane: (ShellPaneSplitDirection) -> Bool
    let canCopyTerminalSelection: Bool
    let canPasteIntoTerminal: Bool
    let canOpenTerminalSearch: Bool
    let onFocusPane: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onCopyTerminalSelection: () -> Void
    let onPasteIntoTerminal: () -> Void
    let onOpenTerminalSearch: () -> Void
    let onClosePane: () -> Void
    @State private var activityFreshnessNow = Date()

    var body: some View {
        ViewThatFits(in: .horizontal) {
            titleBarContent(presentation: .full)
            titleBarContent(presentation: .compact)
            titleBarContent(presentation: .minimal)
        }
        .padding(.leading, ShellPaneTitleBarMetrics.horizontalLeadingPadding)
        .padding(.trailing, ShellPaneTitleBarMetrics.horizontalTrailingPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: ShellPaneTitleBarMetrics.height)
        .background(ShellPalette.terminal)
        .contentShape(Rectangle())
        .onTapGesture(perform: onFocusPane)
        .contextMenu {
            Button("Move Pane Left") {
                onMovePane(.left)
            }
            .disabled(!canMovePane(.left))

            Button("Move Pane Right") {
                onMovePane(.right)
            }
            .disabled(!canMovePane(.right))

            Button("Move Pane Up") {
                onMovePane(.up)
            }
            .disabled(!canMovePane(.up))

            Button("Move Pane Down") {
                onMovePane(.down)
            }
            .disabled(!canMovePane(.down))

            Divider()

            Button("Copy") {
                onCopyTerminalSelection()
            }
            .disabled(!canCopyTerminalSelection)

            Button("Paste") {
                onPasteIntoTerminal()
            }
            .disabled(!canPasteIntoTerminal)

            Button("Find") {
                onOpenTerminalSearch()
            }
            .disabled(!canOpenTerminalSearch)
        }
        .task(id: activityFreshnessRefreshID) {
            await scheduleActivityFreshnessRefresh()
        }
    }

    private func titleBarContent(presentation: ShellPaneTitleBarPresentation) -> some View {
        HStack(spacing: ShellPaneTitleBarMetrics.itemSpacing) {
            titleView

            let visibleAccessories = accessories(for: presentation)
            if !visibleAccessories.isEmpty {
                HStack(spacing: ShellPaneTitleBarMetrics.accessorySpacing) {
                    ForEach(visibleAccessories) { accessory in
                        ShellPaneTitleBarAccessoryView(
                            accessory: accessory,
                            isSelected: isSelected,
                            mode: accessoryMode(for: accessory, presentation: presentation)
                        )
                    }
                }
                .fixedSize(horizontal: true, vertical: true)
            }

            Spacer(minLength: 0)

            if canZoom {
                zoomButton
            }

            closeButton
        }
    }

    private var titleView: some View {
        Text(title)
            .font(
                .system(
                    size: ShellPaneTitleTypography.titleSize,
                    weight: ShellPaneTitleTypography.titleWeight(isSelected: isSelected)
                )
            )
            .foregroundStyle(Color.white.opacity(isSelected ? 0.94 : 0.78))
            .lineLimit(1)
            .truncationMode(.middle)
            .layoutPriority(2)
            .frame(
                minWidth: ShellPaneTitleBarMetrics.minimumTitleWidth,
                alignment: .leading
            )
    }

    private var closeButton: some View {
        Button(action: onClosePane) {
            Image(systemName: "xmark")
                .font(
                    .system(
                        size: ShellPaneTitleTypography.closeSize,
                        weight: ShellPaneTitleTypography.closeWeight
                    )
                )
                .foregroundStyle(Color.white.opacity(isSelected ? 0.68 : 0.52))
                .frame(
                    width: ShellPaneTitleBarMetrics.closeButtonSize,
                    height: ShellPaneTitleBarMetrics.closeButtonSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: true)
        .help("Close pane")
        .accessibilityLabel("Close pane")
    }

    private var zoomButton: some View {
        Button(action: onToggleZoom) {
            Image(systemName: isZoomed ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right")
                .font(
                    .system(
                        size: ShellPaneTitleTypography.closeSize,
                        weight: ShellPaneTitleTypography.closeWeight
                    )
                )
                .foregroundStyle(Color.white.opacity(isSelected ? 0.70 : 0.54))
                .frame(
                    width: ShellPaneTitleBarMetrics.closeButtonSize,
                    height: ShellPaneTitleBarMetrics.closeButtonSize
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .fixedSize(horizontal: true, vertical: true)
        .help(isZoomed ? "Unzoom pane" : "Zoom pane")
        .accessibilityLabel(isZoomed ? "Unzoom pane" : "Zoom pane")
    }

    private var activityFreshnessRefreshID: String {
        nextActivityFreshnessExpiry(after: activityFreshnessNow)
            .map { "\($0.timeIntervalSince1970)" } ?? "none"
    }

    private func scheduleActivityFreshnessRefresh() async {
        guard let deadline = nextActivityFreshnessExpiry(after: activityFreshnessNow) else {
            return
        }

        let delay = min(max(deadline.timeIntervalSinceNow, 0), 86_400)
        if delay > 0 {
            let nanoseconds = UInt64(delay * 1_000_000_000)
            try? await Task.sleep(nanoseconds: nanoseconds)
        }

        guard !Task.isCancelled else { return }
        await MainActor.run {
            activityFreshnessNow = Date()
        }
    }

    private func nextActivityFreshnessExpiry(after now: Date) -> Date? {
        guard let activity = pane.activity else { return nil }

        return [
            activity.freshness.staleAt,
            activity.freshness.expiresAt,
        ]
        .compactMap { value in
            value.flatMap(Self.activityFreshnessFormatter.date(from:))
        }
        .filter { $0 > now }
        .min()
    }

    private static let activityFreshnessFormatter = ISO8601DateFormatter()

    private var accessories: [ShellPaneTitleBarAccessory] {
        shellPaneTitleBarDetailProjection(
            for: pane,
            title: title,
            now: activityFreshnessNow
        ).map { projection in
            ShellPaneTitleBarAccessory(
                id: projection.id,
                icon: accessoryIcon(for: projection.id),
                title: projection.title,
                help: projection.help,
                tint: accessoryTint(for: projection.id),
                isEmphasized: accessoryIsEmphasized(projection.id)
            )
        }
    }

    private func accessories(
        for presentation: ShellPaneTitleBarPresentation
    ) -> [ShellPaneTitleBarAccessory] {
        switch presentation {
        case .full, .compact:
            return accessories
        case .minimal:
            return accessories.filter { $0.isPrimary || $0.isEmphasized }
        }
    }

    private func accessoryMode(
        for accessory: ShellPaneTitleBarAccessory,
        presentation: ShellPaneTitleBarPresentation
    ) -> ShellPaneTitleBarAccessoryMode {
        switch presentation {
        case .full:
            return .textAndIcon
        case .compact:
            return accessory.isPrimary ? .textAndIcon : .iconOnly
        case .minimal:
            return .iconOnly
        }
    }

    private var activityIcon: String {
        switch pane.activity?.status {
        case .needsInput:
            return "person.crop.circle.badge.exclamationmark"
        case .failed:
            return "exclamationmark.triangle"
        case .paused:
            return "pause.circle"
        case .progress:
            return "progress.indicator"
        case .running:
            return "play.circle"
        case .bell:
            return "bell"
        case .exited:
            return "rectangle.portrait.and.arrow.right"
        case .done:
            return "checkmark.circle"
        case .idle, .stale, nil:
            return "info.circle"
        }
    }

    private var activityTint: Color {
        switch pane.activity?.priority {
        case .awaitingUser, .notable:
            return ShellPalette.attention
        case .active:
            return ShellPalette.accent
        case .passive, nil:
            return Color.white
        }
    }

    private var statusIcon: String {
        if pane.context?.processState == "exited"
            || pane.context?.surfaceReadiness == "child_exited"
        {
            return "checkmark.circle"
        }
        if pane.context?.rendererHealth == "failed"
            || pane.context?.rendererPhase == "failed"
            || pane.context?.surfaceReadiness == "renderer_failed"
        {
            return "exclamationmark.triangle"
        }
        let attention = shellEffectiveAttention(for: pane, now: activityFreshnessNow)
        if attention == .awaitingUser || attention == .notable {
            return "bell.badge"
        }
        return "info.circle"
    }

    private var statusTint: Color {
        if pane.context?.rendererHealth == "failed"
            || pane.context?.rendererPhase == "failed"
            || pane.context?.surfaceReadiness == "renderer_failed"
            || shellEffectiveAttention(for: pane, now: activityFreshnessNow) == .awaitingUser
        {
            return ShellPalette.attention
        }
        return Color.white
    }

    private func accessoryIcon(for id: String) -> String {
        switch id {
        case "activity":
            return activityIcon
        case "status":
            return statusIcon
        case "worktree", "cwd":
            return "folder"
        case "branch":
            return "point.topleft.down.curvedto.point.bottomright.up"
        case "process":
            return "terminal"
        case "alan":
            return "sparkles"
        default:
            return "info.circle"
        }
    }

    private func accessoryTint(for id: String) -> Color {
        switch id {
        case "activity":
            return activityTint
        case "status":
            return statusTint
        case "alan":
            return ShellPalette.accent
        default:
            return Color.white
        }
    }

    private func accessoryIsEmphasized(_ id: String) -> Bool {
        switch id {
        case "activity":
            return pane.activity?.priority == .awaitingUser || pane.activity?.priority == .notable
        case "status":
            return shellEffectiveAttention(for: pane, now: activityFreshnessNow) == .awaitingUser
                || shellEffectiveAttention(for: pane, now: activityFreshnessNow) == .notable
        case "alan":
            return pane.alanBinding?.pendingYield == true
        default:
            return false
        }
    }

}

private struct ShellPaneTitleBarAccessory: Identifiable {
    let id: String
    let icon: String
    let title: String?
    let help: String
    let tint: Color
    let isEmphasized: Bool

    var isPrimary: Bool {
        id == "activity" || id == "status"
    }
}

private struct ShellPaneTitleBarAccessoryView: View {
    let accessory: ShellPaneTitleBarAccessory
    let isSelected: Bool
    let mode: ShellPaneTitleBarAccessoryMode

    var body: some View {
        HStack(spacing: ShellPaneTitleBarMetrics.accessoryInternalSpacing) {
            Image(systemName: accessory.icon)
                .font(
                    .system(
                        size: ShellPaneTitleTypography.accessorySize,
                        weight: ShellPaneTitleTypography.iconWeight
                    )
                )

            if mode == .textAndIcon,
               let title = accessory.title {
                Text(title)
                    .font(
                        .system(
                            size: ShellPaneTitleTypography.accessorySize,
                            weight: accessory.isEmphasized
                                ? ShellPaneTitleTypography.emphasizedAccessoryWeight
                                : ShellPaneTitleTypography.accessoryWeight
                        )
                    )
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .fixedSize(horizontal: true, vertical: false)
            }
        }
        .foregroundStyle(accessory.tint.opacity(accessoryOpacity))
        .fixedSize(horizontal: true, vertical: true)
        .help(accessory.help)
        .accessibilityLabel(accessory.help)
    }

    private var accessoryOpacity: Double {
        if accessory.isEmphasized {
            return isSelected ? 0.96 : 0.82
        }
        return isSelected ? 0.78 : 0.62
    }
}

private struct ShellFindBarView: View {
    let searchState: AlanTerminalSearchState
    let onQueryChange: (String) -> Void
    let onNext: () -> Void
    let onPrevious: () -> Void
    let onClose: () -> Void

    @State private var query: String
    @FocusState private var isFocused: Bool

    init(
        searchState: AlanTerminalSearchState,
        onQueryChange: @escaping (String) -> Void,
        onNext: @escaping () -> Void,
        onPrevious: @escaping () -> Void,
        onClose: @escaping () -> Void
    ) {
        self.searchState = searchState
        self.onQueryChange = onQueryChange
        self.onNext = onNext
        self.onPrevious = onPrevious
        self.onClose = onClose
        _query = State(initialValue: searchState.query)
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(ShellPalette.mutedInk)

            TextField("Find", text: $query)
                .textFieldStyle(.plain)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(ShellPalette.ink)
                .focused($isFocused)
                .frame(width: 180)
                .onChange(of: query) { _, nextQuery in
                    onQueryChange(nextQuery)
                }
                .onChange(of: searchState.query) { _, nextQuery in
                    guard nextQuery != query else { return }
                    query = nextQuery
                }
                .onChange(of: searchState.focusRequestID) { _, _ in
                    isFocused = true
                }
                .onSubmit {
                    onNext()
                }

            Text(resultLabel)
                .font(.system(size: 11, weight: .semibold, design: .monospaced))
                .foregroundStyle(ShellPalette.mutedInk)
                .frame(minWidth: 48, alignment: .trailing)

            Button(action: onPrevious) {
                Image(systemName: "chevron.up")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Previous match")
            .keyboardShortcut("g", modifiers: [.command, .shift])

            Button(action: onNext) {
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Next match")
            .keyboardShortcut("g", modifiers: [.command])

            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .bold))
                    .frame(width: 22, height: 22)
            }
            .buttonStyle(.plain)
            .help("Close Find")
            .keyboardShortcut(.escape, modifiers: [])
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(
            ShellMaterialShape(
                role: .floatingInput,
                shape: RoundedRectangle(cornerRadius: ShellRadii.surface, style: .continuous)
            )
        )
        .overlay {
            RoundedRectangle(cornerRadius: ShellRadii.surface, style: .continuous)
                .stroke(ShellPalette.line.opacity(0.35), lineWidth: 1)
        }
        .shellShadow(ShellShadows.floatingInput)
        .onAppear {
            query = searchState.query
            isFocused = true
        }
        .onExitCommand {
            onClose()
        }
    }

    private var resultLabel: String {
        if let total = searchState.totalMatches,
           let selected = searchState.selectedIndex
        {
            guard total > 0 else { return "0" }
            return "\(selected + 1)/\(total)"
        }
        return query.isEmpty ? "" : "..."
    }
}

private struct ShellInactivePaneDim: View {
    let isSelected: Bool
    let isEnabled: Bool

    var body: some View {
        Rectangle()
            .fill(Color.black.opacity(isSelected || !isEnabled ? 0 : 0.14))
            .allowsHitTesting(false)
    }
}

private struct TerminalActionButton: View {
    let icon: String
    let title: String
    var isDestructive = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            TerminalActionLabel(
                icon: icon,
                title: title,
                foreground: isDestructive ? Color.red.opacity(0.8) : ShellPalette.ink
            )
        }
        .buttonStyle(.plain)
    }
}

private struct TerminalActionLabel: View {
    let icon: String
    let title: String
    var foreground: Color = ShellPalette.ink

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
            Text(title)
        }
        .font(.system(size: 12, weight: .semibold))
        .foregroundStyle(foreground)
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            ShellMaterialShape(
                role: .controlGlass,
                shape: RoundedRectangle(cornerRadius: ShellRadii.row, style: .continuous)
            )
        )
    }
}

private struct TerminalPaneChip: View {
    let icon: String
    let title: String
    let foreground: Color
    let background: Color

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
            Text(title)
        }
        .font(.system(size: 11, weight: .semibold))
        .foregroundStyle(foreground)
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: ShellRadii.row, style: .continuous)
                .fill(background)
        )
    }
}

private struct TerminalInfoCard<Content: View>: View {
    let title: String
    let accent: Color
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title)
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .foregroundStyle(ShellPalette.ink)

            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            ShellMaterialShape(
                role: .panel,
                shape: RoundedRectangle(cornerRadius: ShellRadii.overlay, style: .continuous)
            )
        )
        .overlay {
            RoundedRectangle(cornerRadius: ShellRadii.overlay, style: .continuous)
                .stroke(accent.opacity(0.16), lineWidth: 1)
        }
    }
}

private struct TerminalInfoRow: View {
    let label: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(size: 11, weight: .semibold, design: .rounded))
                .textCase(.uppercase)
                .foregroundStyle(ShellPalette.mutedInk)
            Text(value)
                .font(.system(size: 13, weight: .medium, design: .rounded))
                .foregroundStyle(ShellPalette.ink)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct TerminalMonoLine: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .medium, design: .monospaced))
            .foregroundStyle(ShellPalette.mutedInk)
            .lineLimit(2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(
                ShellMaterialShape(
                    role: .panelSoft,
                    shape: RoundedRectangle(cornerRadius: ShellRadii.surface, style: .continuous)
                )
            )
    }
}

#endif
