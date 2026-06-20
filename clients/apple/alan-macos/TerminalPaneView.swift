import Foundation
import OSLog
import SwiftUI

struct TerminalPaneView: View {
    @ObservedObject var host: ShellHostController
    let tab: ShellTab?
    let spaceID: String?
    let selectedPaneID: String?
    let zoomedPaneID: String?
    let workspacePanelInsets: EdgeInsets
    let onClosePane: ((ShellPane) -> Void)?

    init(
        host: ShellHostController,
        tab: ShellTab? = nil,
        spaceID: String? = nil,
        selectedPaneID: String? = nil,
        zoomedPaneID: String? = nil,
        workspacePanelInsets: EdgeInsets,
        onClosePane: ((ShellPane) -> Void)? = nil
    ) {
        self.host = host
        self.tab = tab
        self.spaceID = spaceID
        self.selectedPaneID = selectedPaneID
        self.zoomedPaneID = zoomedPaneID
        self.workspacePanelInsets = workspacePanelInsets
        self.onClosePane = onClosePane
    }

    var body: some View {
        paneCanvas
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(workspacePanelInsets)
    }

    private var paneCanvas: some View {
        Group {
            if host.isPresentingSpaceCreation {
                ShellEmptyWorkspacePlaceholder(
                    spaceTitle: creationDraftTitle
                ) {
                    // No-op during creation — the form drives the Space creation flow.
                }
            } else if let paneTree = displayPaneTree {
                workspaceContentTree(for: paneTree)
            } else {
                ShellEmptyWorkspacePlaceholder(
                    spaceTitle: displaySpaceTitle
                ) {
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
                }
            }
        }
        .shellWorkspacePanelFrame()
    }

    /// Title for the workspace placeholder shown while the Space creation form is open.
    /// Returns the live draft name the user is typing, or "New Space" when blank.
    private var creationDraftTitle: String {
        let trimmed = host.spaceDraftName.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "New Space" : trimmed
    }

    @ViewBuilder
    private func workspaceContentTree(for tree: ShellPaneTreeNode) -> some View {
        ShellPaneTreeLayoutView(
            node: tree,
            host: host,
            selectedPaneID: displaySelectedPaneID,
            onClosePane: onClosePane
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var displayTab: ShellTab? {
        tab ?? host.selectedTab
    }

    private var displaySpaceID: String? {
        spaceID ?? host.selectedSpace?.spaceID
    }

    private var displaySpaceTitle: String? {
        if let id = displaySpaceID {
            return host.spaces.first { $0.spaceID == id }?.title ?? host.selectedSpace?.title
        }
        return host.selectedSpace?.title
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

private struct ShellEmptyWorkspacePlaceholder: View {
    var spaceTitle: String? = nil
    let onCreateTerminalTab: () -> Void

    @State private var isHoveringButton = false

    private var resolvedTitle: String {
        spaceTitle ?? "Empty Space"
    }

    var body: some View {
        VStack(spacing: ShellSpacing.section) {
            // Heading: Space title or fallback
            Text(resolvedTitle)
                .font(ShellType.pro(ShellType.display, weight: .semibold))
                .foregroundStyle(ShellPalette.ink)
                .accessibilityAddTraits(.isHeader)

            // Secondary line
            Text("Start a terminal in this space.")
                .font(ShellType.pro(ShellType.row))
                .foregroundStyle(ShellPalette.mutedInk)

            // Primary action: bordered quiet control
            Button(action: onCreateTerminalTab) {
                Label("New Tab", systemImage: "plus")
                    .font(ShellType.pro(ShellType.row, weight: .semibold))
                    .padding(.horizontal, ShellSpacing.row)
                    .padding(.vertical, ShellSpacing.control)
            }
            .buttonStyle(.plain)
            .background {
                ShellMaterialShape(
                    role: isHoveringButton ? .controlGlassHover : .controlGlass,
                    shape: RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous),
                    showsStroke: true
                )
            }
            .onHover { hovering in
                isHoveringButton = hovering
            }
            .help("Create a tab in this space")

            // Key hint: mono chord + pro caption
            HStack(spacing: ShellSpacing.tight) {
                Text("⌘T")
                    .font(ShellType.mono(ShellType.monoCaption))
                    .foregroundStyle(ShellPalette.mutedInk.opacity(0.7))
                Text("opens a new tab")
                    .font(ShellType.pro(ShellType.caption))
                    .foregroundStyle(ShellPalette.mutedInk.opacity(0.7))
            }
            .accessibilityElement(children: .combine)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
    }
}

private struct ShellWorkspacePanelFrame: ViewModifier {
    @Environment(\.colorScheme) private var colorScheme
    private let shape = RoundedRectangle(cornerRadius: ShellRadii.workspacePanel, style: .continuous)

    func body(content: Content) -> some View {
        content
            .clipShape(shape)
            .background {
                shape
                    .fill(ShellPalette.rootBacking)
                    .shellShadow(ShellShadows.workspacePanelRim)
                    .shellShadow(ShellShadows.workspacePanel)
            }
            .overlay {
                workspacePanelRim
            }
    }

    private var workspacePanelRim: some View {
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
                            ShellInk.rimHighlight,
                            Color.white.opacity(0.015),
                            ShellInk.rimShadowLine,
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

private extension View {
    func shellWorkspacePanelFrame() -> some View {
        modifier(ShellWorkspacePanelFrame())
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
            restoredTranscriptSnapshot: host.restoredTranscriptSnapshot(for: pane),
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
            onClearRestoredTranscript: {
                _ = host.clearRestoredTranscriptSnapshot(for: pane)
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
            onTerminalRuntimeExit: {
                _ = host.closePaneAfterTerminalRuntimeExit(paneID: pane.paneID)
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
        let settingsWorkspaceContext =
            descriptor.renderKind == .settings
            ? host.settingsWorkspaceContext(forPaneSlotID: paneSlotID)
            : .none
        return ShellBoundedContentLeafView(
            descriptor: descriptor,
            paneSlotID: paneSlotID,
            settingsWorkspaceContext: settingsWorkspaceContext,
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
                            .frame(maxHeight: .infinity, alignment: .topLeading)
                        ShellSplitDividerView(direction: .vertical)
                            .gesture(resizeGesture(totalLength: proxy.size.width))
                        ShellPaneTreeLayoutView(
                            node: children[1],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(width: secondaryLength(total: proxy.size.width))
                            .frame(maxHeight: .infinity, alignment: .topLeading)
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
                            .frame(maxWidth: .infinity, alignment: .topLeading)
                        ShellSplitDividerView(direction: .horizontal)
                            .gesture(resizeGesture(totalLength: proxy.size.height))
                        ShellPaneTreeLayoutView(
                            node: children[1],
                            host: host,
                            selectedPaneID: selectedPaneID,
                            onClosePane: onClosePane
                        )
                            .frame(height: secondaryLength(total: proxy.size.height))
                            .frame(maxWidth: .infinity, alignment: .topLeading)
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
    let restoredTranscriptSnapshot: TerminalTranscriptSnapshot?
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
    let onClearRestoredTranscript: () -> Void
    let onToggleZoom: () -> Void
    let onMovePane: (ShellPaneSplitDirection) -> Void
    let onCopyTerminalSelection: () -> Void
    let onPasteIntoTerminal: () -> Void
    let onOpenTerminalSearch: () -> Void
    let onClosePane: () -> Void
    let onTerminalRuntimeExit: () -> Void
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
                VStack(spacing: 0) {
                    if let restoredTranscriptSnapshot {
                        RestoredTerminalTranscriptView(snapshot: restoredTranscriptSnapshot)
                    }

                    TerminalHostView(
                        pane: pane,
                        terminalContentMount: TerminalContentMount(pane: pane),
                        bootProfile: bootProfile,
                        isSelected: isSelected,
                        renderPriority: renderPriority,
                        runtimeRegistry: runtimeRegistry,
                        activationDelegate: activationDelegate,
                        onShellAction: onShellAction,
                        onClearRestoredTranscript: onClearRestoredTranscript,
                        onCloseRequest: { requiresConfirmation in
                            guard !requiresConfirmation else { return }
                            onTerminalRuntimeExit()
                        },
                        onRuntimeUpdate: onRuntimeUpdate,
                        onMetadataUpdate: onMetadataUpdate
                    )
                    .id(pane.paneID)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
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
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct RestoredTerminalTranscriptView: View {
    let snapshot: TerminalTranscriptSnapshot

    private var lines: [String] {
        let boundedLines = snapshot.boundedForManifest().transcriptLines
        guard boundedLines.contains(where: { !$0.trimmingCharacters(in: .whitespaces).isEmpty })
        else {
            return []
        }
        return boundedLines
    }

    private var presentation: RestoredTerminalTranscriptPanelPresentation {
        RestoredTerminalTranscriptPanelPresentation(snapshot: snapshot)
    }

    var body: some View {
        if !lines.isEmpty {
            GeometryReader { proxy in
                ScrollView([.vertical, .horizontal]) {
                    HStack(alignment: .top, spacing: 0) {
                        Text(presentation.transcriptText)
                            .font(
                                .system(
                                    size: presentation.fontSize,
                                    weight: .regular,
                                    design: .monospaced
                                )
                            )
                            .foregroundStyle(Color.white.opacity(0.72))
                            .textSelection(.enabled)
                            .fixedSize(horizontal: true, vertical: false)
                        Spacer(minLength: 0)
                    }
                    .padding(.leading, presentation.leadingInset)
                    .padding(.trailing, presentation.trailingInset)
                    .padding(.vertical, presentation.verticalInset)
                    .frame(minWidth: proxy.size.width, alignment: .topLeading)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .frame(
                maxWidth: .infinity,
                minHeight: presentation.height,
                maxHeight: presentation.height,
                alignment: .topLeading
            )
            .background(ShellPalette.terminal)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(ShellPalette.line.opacity(0.20))
                    .frame(height: 0.6)
            }
            .accessibilityIdentifier("restored-terminal-transcript")
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
    let settingsWorkspaceContext: ShellSettingsWorkspaceContext
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
                ShellSettingsContentView(
                    descriptor: descriptor,
                    workspaceContext: settingsWorkspaceContext
                )
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
        if Task.isCancelled {
            isLoading = false
            return
        }

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
    let workspaceContext: ShellSettingsWorkspaceContext

    @AppStorage("alanShellAppearanceMode") private var appearanceMode = ShellAppearanceMode.system
    @AppStorage("alanShellSidebarCollapsed") private var isSidebarCollapsed = false
    @AppStorage("alanShellDimsInactiveSplitPanes") private var dimsInactiveSplitPanes = true
    @AppStorage(AlanPerformanceDiagnosticsController.preferenceKey)
    private var performanceDiagnosticsEnabled = false
    @State private var localSummary = ShellSettingsLocalSummary.current()
    @State private var remoteSnapshot = ShellSettingsRemoteSnapshot.unavailable(
        reason: "Daemon unavailable"
    )
    @State private var terminalProfilesSummary = TerminalProfileSettingsSummary.current()
    @State private var privilegedHelperSummary = PrivilegedHelperSettingsSummary.current()
    @State private var managedTerminalAccountsSummary = ManagedTerminalAccountSettingsSummary.empty
    @State private var lastDiagnosticsExportURL: URL?
    @State private var selectedGroup = ShellSettingsNavigationGroup.general
    @State private var isManagedUserCreationPresented = false
    @State private var managedUserCreationDraft = ManagedTerminalUserCreationDraft(
        unixUserName: "",
        displayLabel: "",
        guiUserName: NSUserName()
    )
    @State private var managedUserCreationPreviewResult: ManagedTerminalUserCreationPreviewResult?
    @State private var managedUserActionSheet: ShellManagedUserActionSheetState?
    @State private var managedUserApplyDiagnostics: [String] = []
    @State private var managedUserApplyInFlight = false

    nonisolated private static let managedUserApplyLogger = Logger(
        subsystem: Bundle.main.bundleIdentifier ?? "app.alanworks.macos",
        category: "ManagedUsers"
    )
    nonisolated private static let managedUserApplyTimeoutNanoseconds: UInt64 =
        10 * 60 * 1_000_000_000

    private var snapshot: ShellSettingsSurfaceSnapshot {
        ShellSettingsSurfaceSnapshot.make(
            remote: remoteSnapshot,
            local: localSummary,
            terminalProfiles: terminalProfilesSummary,
            privilegedHelper: privilegedHelperSummary,
            managedTerminalAccounts: managedTerminalAccountsSummary,
            diagnostics: diagnosticsSummary
        )
    }

    private var settingsGroups: [ShellSettingsNavigationGroupModel] {
        snapshot.navigationGroups
    }

    private var selectedGroupModel: ShellSettingsNavigationGroupModel {
        settingsGroups.first { $0.id == selectedGroup }
            ?? settingsGroups.first
            ?? ShellSettingsNavigationGroupModel(id: .general, sections: [])
    }

    var body: some View {
        ZStack {
            ShellSettingsBackdrop()

            HStack(alignment: .top, spacing: 0) {
                ShellSettingsNavigationView(
                    groups: settingsGroups,
                    selectedGroup: $selectedGroup
                )
                .frame(width: ShellSettingsMetrics.navigationWidth, alignment: .topLeading)
                .padding(.leading, ShellSettingsMetrics.navigationLeadingPadding)
                .padding(.trailing, ShellSettingsMetrics.navigationTrailingPadding)
                .padding(.top, ShellSettingsMetrics.navigationTopPadding)
                .frame(maxHeight: .infinity, alignment: .topLeading)
                .background {
                    ShellSettingsNavigationRailBackground()
                }

                ZStack(alignment: .topLeading) {
                    ShellSettingsDetailBackground()

                    ScrollView {
                        ShellSettingsGroupView(
                            group: selectedGroupModel,
                            appearanceMode: $appearanceMode,
                            sidebarVisible: sidebarVisible,
                            dimsInactiveSplitPanes: $dimsInactiveSplitPanes,
                            performanceDiagnosticsEnabled: performanceDiagnosticsBinding,
                            onExportPerformanceDiagnostics: exportPerformanceDiagnostics,
                            onRowAction: handleSettingsRowAction
                        )
                        .frame(maxWidth: ShellSettingsMetrics.contentWidth, alignment: .leading)
                        .padding(.leading, ShellSettingsMetrics.detailContentLeadingPadding)
                        .padding(.trailing, ShellSettingsMetrics.detailContentTrailingPadding)
                        .padding(.top, ShellSettingsMetrics.detailContentTopPadding)
                        .padding(.bottom, ShellSettingsMetrics.detailContentBottomPadding)
                        .frame(maxWidth: .infinity, alignment: .topLeading)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
        .task(id: refreshTaskID) {
            await refreshSettingsSummaries()
        }
        .sheet(isPresented: $isManagedUserCreationPresented) {
            ShellManagedUserCreationSheet(
                draft: $managedUserCreationDraft,
                previewResult: managedUserCreationPreviewResult,
                diagnostics: managedUserApplyDiagnostics,
                isApplying: managedUserApplyInFlight,
                onDraftChanged: resetManagedUserCreationPreview,
                onPreview: reviewManagedUserCreationDraft,
                onApply: applyManagedUserCreationPreview,
                onCancel: {
                    isManagedUserCreationPresented = false
                }
            )
        }
        .sheet(item: $managedUserActionSheet) { sheet in
            ShellManagedUserPlanSheet(
                sheet: sheet,
                diagnostics: managedUserApplyDiagnostics,
                isApplying: managedUserApplyInFlight,
                onApply: {
                    applyManagedUserActionSheet(sheet)
                },
                onCancel: {
                    managedUserActionSheet = nil
                }
            )
        }
    }

    private var refreshTaskID: String {
        [
            descriptor.contentID ?? descriptor.title,
            workspaceContext.connectionWorkspaceDir,
            workspaceContext.skillCatalogWorkspaceDir,
            workspaceContext.skillCatalogUnavailableReason,
            workspaceContext.agentName,
        ]
        .compactMap { $0 }
        .joined(separator: "|")
    }

    private var sidebarVisible: Binding<Bool> {
        Binding(
            get: { !isSidebarCollapsed },
            set: { isSidebarCollapsed = !$0 }
        )
    }

    private var performanceDiagnosticsBinding: Binding<Bool> {
        Binding(
            get: { performanceDiagnosticsEnabled },
            set: { nextValue in
                performanceDiagnosticsEnabled = nextValue
                AlanPerformanceDiagnosticsController.shared.setEnabled(nextValue)
            }
        )
    }

    private var diagnosticsSummary: ShellSettingsDiagnosticsSummary {
        let summary = AlanPerformanceDiagnosticsController.shared.summarySnapshot()
        return ShellSettingsDiagnosticsSummary(
            isEnabled: performanceDiagnosticsEnabled,
            retainedEventCount: AlanPerformanceDiagnosticsController.shared.eventsSnapshot().count,
            stutterMarkerCount: summary.stutterMarkerCount,
            lastExportURL: lastDiagnosticsExportURL
        )
    }

    private func exportPerformanceDiagnostics() {
        lastDiagnosticsExportURL = AlanPerformanceDiagnosticsExportPresenter.exportRecentDiagnostics(
            installChannel: localSummary.channelLabel
        )
    }

    @discardableResult
    @MainActor
    private func refreshLocalTerminalIdentitySummaries() -> ManagedTerminalAccountSettingsSummary {
        let profiles = TerminalProfileSettingsSummary.current()
        let accounts = ManagedTerminalAccountSettingsSummary.current(
            terminalProfiles: profiles,
            helperClient: AlanPrivilegedHelperAppClient(channel: .current())
        )
        terminalProfilesSummary = profiles
        managedTerminalAccountsSummary = accounts
        return accounts
    }

    @MainActor
    private func handleSettingsRowAction(
        row: ShellSettingsRowModel,
        action: ShellSettingsRowActionKind
    ) {
        managedUserApplyDiagnostics = []
        switch action {
        case .create:
            managedUserCreationDraft = ManagedTerminalUserCreationDraft(
                unixUserName: "",
                displayLabel: "",
                guiUserName: NSUserName()
            )
            managedUserCreationPreviewResult = nil
            isManagedUserCreationPresented = true
        case .review, .repair, .verify, .remove:
            handleExistingManagedUserAction(row: row, action: action)
        case .installHelper, .updateHelper, .uninstallHelper:
            applyPrivilegedHelperLifecycleAction(action)
        }
    }

    @MainActor
    private func applyPrivilegedHelperLifecycleAction(_ action: ShellSettingsRowActionKind) {
        let manager = AlanPrivilegedHelperAppServiceManager()
        let result: AlanPrivilegedHelperLifecycleResult
        switch action {
        case .installHelper, .updateHelper:
            result = manager.installOrUpdate()
        case .uninstallHelper:
            result = manager.uninstall()
        case .create, .review, .repair, .verify, .remove:
            return
        }
        privilegedHelperSummary = PrivilegedHelperSettingsSummary(status: result.status)
        managedUserApplyDiagnostics = result.diagnostic.map {
            [$0.sanitizedMessage, "Credentials redacted."]
        } ?? ["Privileged helper \(result.action.rawValue) completed. Credentials redacted."]
    }

    @MainActor
    private func handleExistingManagedUserAction(
        row: ShellSettingsRowModel,
        action: ShellSettingsRowActionKind
    ) {
        guard let plan = managedUserPlan(forRowID: row.id) else { return }
        switch action {
        case .create, .installHelper, .updateHelper, .uninstallHelper:
            return
        case .review:
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: plan,
                allowsApply: false
            )
        case .repair:
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: plan,
                allowsApply: true
            )
        case .verify:
            let refreshedSummary = refreshLocalTerminalIdentitySummaries()
            let refreshedPlan = refreshedSummary.plans.first {
                $0.request.accountName == plan.request.accountName
            } ?? plan
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: .review,
                plan: refreshedPlan,
                allowsApply: false
            )
        case .remove:
            let helperClient = AlanPrivilegedHelperAppClient(channel: .current())
            let status = helperClient.status()
            let diagnosis = status.isHealthy
                ? helperClient.diagnoseManagedUser(plan.request)
                : AlanManagedUserDiagnosis.helperUnavailable(request: plan.request, status: status)
            let rollbackPlan = ManagedTerminalAccountPlanner.rollbackPlan(
                request: plan.request,
                diagnosis: diagnosis,
                scope: .alanIntegrationOnly
            )
            managedUserActionSheet = ShellManagedUserActionSheetState(
                action: action,
                plan: rollbackPlan,
                allowsApply: true
            )
        }
    }

    private func managedUserPlan(forRowID rowID: String) -> ManagedTerminalAccountPlan? {
        let prefix = "terminalAccount."
        guard rowID.hasPrefix(prefix) else { return nil }
        let accountName = String(rowID.dropFirst(prefix.count))
        return managedTerminalAccountsSummary.plans.first {
            $0.request.accountName == accountName
        }
    }

    @MainActor
    private func resetManagedUserCreationPreview() {
        managedUserCreationPreviewResult = nil
        managedUserApplyDiagnostics = []
    }

    @MainActor
    private func reviewManagedUserCreationDraft() {
        let request = managedUserCreationDraft.request
        let helperClient = AlanPrivilegedHelperAppClient(channel: .current())
        let status = helperClient.status()
        let diagnosis = status.isHealthy
            ? helperClient.diagnoseManagedUser(request)
            : AlanManagedUserDiagnosis.helperUnavailable(request: request, status: status)
        managedUserCreationPreviewResult = ManagedTerminalUserCreationPreviewBuilder.make(
            draft: managedUserCreationDraft,
            existingUsers: managedTerminalAccountsSummary.users,
            terminalProfiles: terminalProfilesSummary,
            diagnosis: diagnosis
        )
    }

    @MainActor
    private func applyManagedUserCreationPreview(_ preview: ManagedTerminalUserCreationPreview) {
        guard !managedUserApplyInFlight else { return }
        managedUserApplyInFlight = true
        managedUserApplyDiagnostics = ["Applying managed user changes. Credentials redacted."]

        Task {
            defer { managedUserApplyInFlight = false }
            let result = await Self.applyManagedUserPlanInBackground(
                plan: preview.plan,
                request: preview.request
            )
            if Task.isCancelled { return }

            terminalProfilesSummary = result.terminalProfiles
            managedTerminalAccountsSummary = result.managedTerminalAccounts
            managedUserApplyDiagnostics = result.applyResult.visibleDiagnostics
            if !result.applyResult.cancelled && result.applyResult.failedStep == nil {
                isManagedUserCreationPresented = false
                managedUserCreationPreviewResult = nil
            }
        }
    }

    @MainActor
    private func applyManagedUserActionSheet(_ sheet: ShellManagedUserActionSheetState) {
        guard !managedUserApplyInFlight else { return }
        managedUserApplyInFlight = true
        managedUserApplyDiagnostics = ["Applying managed user changes. Credentials redacted."]

        Task {
            defer { managedUserApplyInFlight = false }
            let result = await Self.applyManagedUserPlanInBackground(
                plan: sheet.plan,
                request: sheet.plan.request
            )
            if Task.isCancelled { return }

            terminalProfilesSummary = result.terminalProfiles
            managedTerminalAccountsSummary = result.managedTerminalAccounts
            managedUserApplyDiagnostics = result.applyResult.visibleDiagnostics
            if !result.applyResult.cancelled && result.applyResult.failedStep == nil {
                managedUserActionSheet = nil
            }
        }
    }

    nonisolated private static func applyManagedUserPlanInBackground(
        plan: ManagedTerminalAccountPlan,
        request: ManagedTerminalAccountRequest
    ) async -> ShellManagedUserApplyBackgroundResult {
        await withCheckedContinuation { continuation in
            let continuationBox = ShellManagedUserApplyContinuationBox(continuation: continuation)
            let work = Task.detached(priority: .userInitiated) {
                let result = runManagedUserPlanInBackground(plan: plan, request: request)
                continuationBox.resume(returning: result)
            }
            Task.detached(priority: .userInitiated) {
                try? await Task.sleep(nanoseconds: managedUserApplyTimeoutNanoseconds)
                guard !work.isCancelled else { return }
                work.cancel()
                managedUserApplyLogger.error("Managed User apply timed out.")
                continuationBox.resume(returning: timeoutManagedUserApplyResult(plan: plan))
            }
        }
    }

    nonisolated private static func runManagedUserPlanInBackground(
        plan: ManagedTerminalAccountPlan,
        request: ManagedTerminalAccountRequest
    ) -> ShellManagedUserApplyBackgroundResult {
        managedUserApplyLogger.info("Managed User apply started.")
        let catalogStore = ManagedTerminalAccountCatalogStore.defaultStore()
        let isRemovalPlan = plan.steps.contains {
            switch $0.kind {
            case .removeSudoersDropIn, .removeManagedTerminalProfile, .deleteAccount, .deleteHomeDirectory:
                return true
            case .helperStep(let helperKind):
                return helperKind == .removeManagedUserIntegration
                    || helperKind == .deleteAccount
                    || helperKind == .deleteHomeDirectory
            default:
                return false
            }
        } && !plan.steps.contains {
            switch $0.kind {
            case .createStandardAccount,
                 .repairAccountType,
                 .repairHomeDirectory,
                 .repairShell,
                 .hideAccount,
                 .createOrUpdateTerminalProfile,
                 .bindCurrentSpace:
                return true
            case .helperStep(let helperKind):
                switch helperKind {
                case .removeManagedUserIntegration, .deleteAccount, .deleteHomeDirectory:
                    return false
                case .createStandardAccount,
                     .repairAccountType,
                     .repairHomeDirectory,
                     .repairShell,
                     .hideAccount,
                     .writeOwnershipMarker,
                     .verifyAccount,
                     .cleanupLegacySudoers,
                     .verifyManagedUserPTY:
                    return true
                }
            case .removeSudoersDropIn, .removeManagedTerminalProfile, .deleteAccount, .deleteHomeDirectory:
                return false
            default:
                return false
            }
        }
        if !isRemovalPlan {
            try? catalogStore.upsert(
                ManagedTerminalAccountCatalogEntry(
                    accountName: request.accountName,
                    displayLabel: request.fullName ?? request.accountName
                )
            )
        }
        let channel = AlanInstallChannel.current()
        let helperClient = AlanPrivilegedHelperAppClient(channel: channel)
        let executor = ManagedTerminalAccountHelperExecutor(
            channel: channel,
            helperClient: helperClient
        )
        let applyResult = executor.apply(plan)
        if isRemovalPlan && !applyResult.cancelled && applyResult.failedStep == nil {
            try? catalogStore.remove(accountName: request.accountName)
        }
        let terminalProfiles = TerminalProfileSettingsSummary.current()
        let managedTerminalAccounts = ManagedTerminalAccountSettingsSummary.current(
            terminalProfiles: terminalProfiles,
            helperClient: helperClient
        )
        managedUserApplyLogger.info("Managed User apply finished.")
        return ShellManagedUserApplyBackgroundResult(
            applyResult: applyResult,
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: managedTerminalAccounts
        )
    }

    nonisolated private static func timeoutManagedUserApplyResult(
        plan: ManagedTerminalAccountPlan
    ) -> ShellManagedUserApplyBackgroundResult {
        let terminalProfiles = TerminalProfileSettingsSummary.current()
        return ShellManagedUserApplyBackgroundResult(
            applyResult: ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: plan.steps.first?.kind,
                cancelled: false,
                visibleDiagnostics: [
                    "Managed User apply timed out. Credentials redacted.",
                ]
            ),
            terminalProfiles: terminalProfiles,
            managedTerminalAccounts: ManagedTerminalAccountSettingsSummary.current(
                terminalProfiles: terminalProfiles,
                helperClient: AlanPrivilegedHelperAppClient(channel: .current())
            )
        )
    }

    private struct ShellManagedUserApplyBackgroundResult {
        let applyResult: ManagedTerminalAccountApplyResult
        let terminalProfiles: TerminalProfileSettingsSummary
        let managedTerminalAccounts: ManagedTerminalAccountSettingsSummary
    }

    private final class ShellManagedUserApplyContinuationBox: @unchecked Sendable {
        private let lock = NSLock()
        private var hasResumed = false
        private let continuation: CheckedContinuation<ShellManagedUserApplyBackgroundResult, Never>

        init(continuation: CheckedContinuation<ShellManagedUserApplyBackgroundResult, Never>) {
            self.continuation = continuation
        }

        func resume(returning result: ShellManagedUserApplyBackgroundResult) {
            lock.lock()
            defer { lock.unlock() }
            guard !hasResumed else { return }
            hasResumed = true
            continuation.resume(returning: result)
        }
    }

    @MainActor
    private func refreshSettingsSummaries() async {
        let local = ShellSettingsLocalSummary.current()
        localSummary = local
        privilegedHelperSummary = PrivilegedHelperSettingsSummary.current()
        refreshLocalTerminalIdentitySummaries()

        do {
            let client = try AlanAPIClient(baseURLString: local.daemonURL)
            async let catalogResponse = client.connectionCatalog()
            async let profilesResponse = client.listConnectionProfiles()
            async let currentResponse = client.currentConnection(
                workspaceDir: workspaceContext.connectionWorkspaceDir
            )

            let (catalog, profiles, current) = try await (
                catalogResponse,
                profilesResponse,
                currentResponse
            )
            let capabilitiesSummary: ShellSettingsCapabilitiesSummary
            if let reason = workspaceContext.skillCatalogUnavailableReason {
                capabilitiesSummary = .unavailable(reason: reason)
            } else {
                let skills = try await client.skillCatalog(
                    workspaceDir: workspaceContext.skillCatalogWorkspaceDir,
                    agentName: workspaceContext.agentName
                )
                capabilitiesSummary = ShellSettingsCapabilitiesSummary(
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
            }
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
                capabilities: capabilitiesSummary
            )
        } catch {
            remoteSnapshot = .unavailable(reason: "Daemon unavailable")
        }
    }
}

private struct ShellSettingsBackdrop: View {
    var body: some View {
        ZStack {
            ShellPalette.settingsPane
            ShellPalette.windowBackdropTint.opacity(0.025)
        }
    }
}

private struct ShellSettingsNavigationRailBackground: View {
    var body: some View {
        Color.clear
    }
}

private struct ShellSettingsDetailBackground: View {
    var body: some View {
        Color.clear
    }
}

private struct ShellSettingsNavigationView: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hoveredGroup: ShellSettingsNavigationGroup?

    let groups: [ShellSettingsNavigationGroupModel]
    @Binding var selectedGroup: ShellSettingsNavigationGroup

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSettingsMetrics.navigationRowSpacing) {
            ForEach(groups) { group in
                Button {
                    selectedGroup = group.id
                } label: {
                    HStack(spacing: ShellSettingsMetrics.navigationRowContentSpacing) {
                        Image(systemName: group.systemName)
                            .font(ShellSettingsTypography.navigationIcon)
                            .foregroundStyle(iconStyle(for: group))
                            .frame(width: ShellSettingsMetrics.navigationIconSlotWidth, height: 16)

                        Text(group.title)
                            .font(ShellSettingsTypography.navigationLabel(selected: group.id == selectedGroup))
                            .foregroundStyle(textStyle(for: group))
                            .lineLimit(1)

                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, ShellSettingsMetrics.navigationRowHorizontalPadding)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: ShellSettingsMetrics.navigationRowHeight)
                    .contentShape(Rectangle())
                    .background {
                        ShellSettingsNavigationRowBackground(
                            state: rowVisualState(for: group)
                        )
                    }
                }
                .buttonStyle(.plain)
                .help(group.title)
                .accessibilityLabel(Text(group.title))
                .onHover { isHovered in
                    if isHovered {
                        hoveredGroup = group.id
                    } else if hoveredGroup == group.id {
                        hoveredGroup = nil
                    }
                }
            }
        }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hoveredGroup)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: selectedGroup)
    }

    private func iconStyle(for group: ShellSettingsNavigationGroupModel) -> some ShapeStyle {
        group.id == selectedGroup
            ? AnyShapeStyle(ShellPalette.settingsPrimaryInk)
            : AnyShapeStyle(ShellPalette.settingsSecondaryInk)
    }

    private func textStyle(for group: ShellSettingsNavigationGroupModel) -> some ShapeStyle {
        group.id == selectedGroup
            ? AnyShapeStyle(ShellPalette.settingsPrimaryInk)
            : AnyShapeStyle(ShellPalette.settingsSecondaryInk)
    }

    private func rowVisualState(
        for group: ShellSettingsNavigationGroupModel
    ) -> ShellSettingsNavigationRowVisualState {
        if group.id == selectedGroup {
            return .selected
        }

        if group.id == hoveredGroup {
            return .hover
        }

        return .normal
    }
}

private enum ShellSettingsNavigationRowVisualState: Equatable {
    case normal
    case hover
    case selected

    var fill: Color? {
        switch self {
        case .normal:
            return nil
        case .hover:
            return ShellPalette.settingsNavigationHover
        case .selected:
            return ShellPalette.settingsNavigationSelection
        }
    }

    var stroke: Color {
        switch self {
        case .normal:
            return .clear
        case .hover:
            return ShellPalette.line.opacity(0.07)
        case .selected:
            return ShellPalette.line.opacity(0.08)
        }
    }
}

private struct ShellSettingsNavigationRowBackground: View {
    let state: ShellSettingsNavigationRowVisualState

    var body: some View {
        let shape = RoundedRectangle(
            cornerRadius: ShellSettingsMetrics.navigationSelectionCornerRadius,
            style: .continuous
        )

        ZStack(alignment: .leading) {
            if let fill = state.fill {
                shape
                    .fill(fill)
                    .overlay {
                        shape.stroke(state.stroke, lineWidth: 0.5)
                    }
            }
        }
    }
}

private struct ShellSettingsGroupView: View {
    let group: ShellSettingsNavigationGroupModel
    @Binding var appearanceMode: ShellAppearanceMode
    let sidebarVisible: Binding<Bool>
    @Binding var dimsInactiveSplitPanes: Bool
    let performanceDiagnosticsEnabled: Binding<Bool>
    let onExportPerformanceDiagnostics: () -> Void
    let onRowAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ShellSettingsMetrics.pageTitleToSectionsSpacing) {
            Text(group.title)
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            VStack(alignment: .leading, spacing: ShellSettingsMetrics.sectionSpacing) {
                ForEach(group.sections) { section in
                    ShellSettingsSectionView(
                        section: section,
                        appearanceMode: $appearanceMode,
                        sidebarVisible: sidebarVisible,
                        dimsInactiveSplitPanes: $dimsInactiveSplitPanes,
                        performanceDiagnosticsEnabled: performanceDiagnosticsEnabled,
                        onExportPerformanceDiagnostics: onExportPerformanceDiagnostics,
                        onRowAction: onRowAction
                    )
                }
            }
        }
    }
}

private struct ShellSettingsVerticalDivider: View {
    var body: some View {
        Rectangle()
            .fill(ShellPalette.line.opacity(0.13))
            .frame(width: 0.8)
    }
}

private struct ShellSettingsSectionView: View {
    let section: ShellSettingsGroupSectionModel
    @Binding var appearanceMode: ShellAppearanceMode
    let sidebarVisible: Binding<Bool>
    @Binding var dimsInactiveSplitPanes: Bool
    let performanceDiagnosticsEnabled: Binding<Bool>
    let onExportPerformanceDiagnostics: () -> Void
    let onRowAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                Text(section.title.uppercased())
                    .font(ShellSettingsTypography.sectionTitle)
                    .foregroundStyle(ShellPalette.settingsSecondaryInk)
                    .tracking(0.4)
                    .lineLimit(1)

                Rectangle()
                    .fill(ShellPalette.line.opacity(0.20))
                    .frame(height: 0.8)
            }
            .padding(.bottom, ShellSettingsMetrics.sectionTitleBottomPadding)

            VStack(spacing: 0) {
                ForEach(Array(section.rows.enumerated()), id: \.element.id) { index, row in
                    if index > 0 {
                        ShellSettingsDivider()
                    }

                    rowView(row)
                }
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
                .controlSize(.small)
                .frame(width: ShellSettingsMetrics.segmentedControlWidth)
            }
        case "sidebar":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: sidebarVisible)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "inactiveSplitDimming":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: $dimsInactiveSplitPanes)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "performanceDiagnostics":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Toggle(row.title, isOn: performanceDiagnosticsEnabled)
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .controlSize(.small)
            }
        case "performanceDiagnosticsExport":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.detail
            ) {
                Button("Export", action: onExportPerformanceDiagnostics)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(!performanceDiagnosticsEnabled.wrappedValue)
                    .opacity(
                        performanceDiagnosticsEnabled.wrappedValue
                            ? 1
                            : ShellSettingsMetrics.disabledButtonOpacity
                    )
            }
        case "agentSelector":
            ShellSettingsAgentSummaryRow()
        case "daemonEndpoint":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.value
            ) {
                ShellSettingsInlineValueAction(
                    isEnabled: row.value != nil,
                    buttonSystemName: "doc.on.doc",
                    buttonHelp: "Copy daemon endpoint"
                ) {
                    shellSettingsCopyToPasteboard(row.value)
                }
            }
        case "applicationSupport", "dataRoot", "publicSkills":
            ShellSettingsRow(
                systemName: row.systemName,
                title: row.title,
                detail: row.value
            ) {
                ShellSettingsPathAction(value: row.value)
            }
        default:
            if !row.actions.isEmpty {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: row.detail
                ) {
                    ShellSettingsRowActionAccessory(row: row, onAction: onRowAction)
                }
            } else if let detail = row.detail, row.value != nil {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: detail
                ) {
                    ShellSettingsValueLabel(
                        value: row.value,
                        mutability: row.mutability
                    )
                }
            } else {
                ShellSettingsRow(
                    systemName: row.systemName,
                    title: row.title,
                    detail: row.detail ?? row.value
                )
            }
        }
    }
}

private struct ShellSettingsRowHoveredKey: EnvironmentKey {
    static let defaultValue = false
}

private extension EnvironmentValues {
    var shellSettingsRowHovered: Bool {
        get { self[ShellSettingsRowHoveredKey.self] }
        set { self[ShellSettingsRowHoveredKey.self] = newValue }
    }
}

private struct ShellSettingsRow<Accessory: View>: View {
    @State private var isHovered = false

    let systemName: String
    let title: String
    let detail: String?
    @ViewBuilder let accessory: () -> Accessory

    init(
        systemName: String,
        title: String,
        detail: String?,
        @ViewBuilder accessory: @escaping () -> Accessory
    ) {
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.accessory = accessory
    }

    var body: some View {
        HStack(alignment: .center, spacing: ShellSettingsMetrics.rowColumnSpacing) {
            VStack(alignment: .leading, spacing: ShellSettingsMetrics.rowTextSpacing) {
                Text(title)
                    .font(ShellSettingsTypography.rowTitle)
                    .foregroundStyle(ShellPalette.settingsPrimaryInk)
                    .lineLimit(1)

                if let detail,
                   !detail.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                {
                    Text(detail)
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                        .lineSpacing(1)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .layoutPriority(1)

            accessoryView
                .environment(\.shellSettingsRowHovered, isHovered)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, ShellSettingsMetrics.rowVerticalPadding)
        .frame(minHeight: rowMinHeight)
        .onHover { isHovered = $0 }
    }

    private var rowMinHeight: CGFloat {
        let hasDetail = detail?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
        return hasDetail ? ShellSettingsMetrics.rowMinHeightWithDetail : ShellSettingsMetrics.rowMinHeight
    }

    @ViewBuilder
    private var accessoryView: some View {
        accessory()
            .font(ShellSettingsTypography.accessory)
            .frame(width: ShellSettingsMetrics.accessoryColumnWidth, alignment: .trailing)
    }
}

private extension ShellSettingsRow where Accessory == EmptyView {
    init(
        systemName: String,
        title: String,
        detail: String?
    ) {
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.accessory = { EmptyView() }
    }
}

private struct ShellSettingsAgentSummaryRow: View {
    var body: some View {
        HStack(alignment: .center, spacing: ShellSettingsMetrics.rowColumnSpacing) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Alan")
                    .font(ShellSettingsTypography.agentName)
                    .foregroundStyle(ShellPalette.settingsPrimaryInk)

                Text("Current agent")
                    .font(ShellSettingsTypography.rowDetail)
                    .foregroundStyle(ShellPalette.settingsSecondaryInk)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            Text("Configurable")
                .font(ShellSettingsTypography.badge)
                .foregroundStyle(ShellPalette.settingsValueInk)
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(
                    RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                        .fill(ShellPalette.panel.opacity(0.56))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: ShellRadii.control, style: .continuous)
                        .stroke(ShellPalette.line.opacity(0.18), lineWidth: 0.7)
                )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, ShellSettingsMetrics.rowVerticalPadding)
        .frame(minHeight: ShellSettingsMetrics.agentSummaryRowMinHeight)
        .accessibilityLabel(Text("Alan, current configurable agent"))
    }
}

private struct ShellSettingsValueLabel: View {
    let value: String?
    let mutability: ShellSettingsRowMutability

    var body: some View {
        HStack(spacing: 6) {
            Text(value ?? "Unavailable")
                .font(ShellSettingsTypography.value)
                .foregroundStyle(valueStyle)
                .lineLimit(1)
                .truncationMode(.middle)
                .multilineTextAlignment(.trailing)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: ShellSettingsMetrics.valueColumnWidth, alignment: .trailing)
        .help(value ?? "Unavailable")
    }

    private var valueStyle: some ShapeStyle {
        if value == "Unavailable" {
            return AnyShapeStyle(ShellPalette.settingsDisabledInk)
        }
        if mutability == .actionOnly {
            return AnyShapeStyle(ShellPalette.settingsValueInk)
        }
        return AnyShapeStyle(ShellPalette.settingsValueInk)
    }
}

private struct ShellSettingsInlineValueAction: View {
    @Environment(\.shellSettingsRowHovered) private var isRowHovered

    let isEnabled: Bool
    let buttonSystemName: String
    let buttonHelp: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Label("Copy", systemImage: buttonSystemName)
                .labelStyle(.titleAndIcon)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(!isEnabled)
        .opacity(isRowHovered || isEnabled ? 1 : ShellSettingsMetrics.disabledButtonOpacity)
        .help(buttonHelp)
    }
}

private struct ShellSettingsPathAction: View {
    let value: String?

    var body: some View {
        Button("Show…") {
            shellSettingsOpenFolder(value)
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(!ShellLocalFolderOpener.canOpenFolder(displayPath: value))
        .help(value ?? "Folder unavailable")
    }
}

private struct ShellSettingsRowActionAccessory: View {
    let row: ShellSettingsRowModel
    let onAction: (ShellSettingsRowModel, ShellSettingsRowActionKind) -> Void

    var body: some View {
        HStack(spacing: ShellSettingsMetrics.inlineActionSpacing) {
            if let value = row.value,
               !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text(value)
                    .font(ShellSettingsTypography.value)
                    .foregroundStyle(ShellPalette.settingsValueInk)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: 96, alignment: .trailing)
            }

            if row.actions.count == 1,
               let action = row.actions.first {
                Button {
                    onAction(row, action.id)
                } label: {
                    Label(action.title, systemImage: action.systemName)
                        .labelStyle(.iconOnly)
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help(action.title)
            } else {
                Menu {
                    ForEach(row.actions) { action in
                        Button {
                            onAction(row, action.id)
                        } label: {
                            Label(action.title, systemImage: action.systemName)
                        }
                    }
                } label: {
                    Label("Actions", systemImage: "ellipsis.circle")
                        .labelStyle(.iconOnly)
                }
                .menuStyle(.button)
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help("Actions")
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

private struct ShellManagedUserActionSheetState: Identifiable, Equatable {
    let action: ShellSettingsRowActionKind
    let plan: ManagedTerminalAccountPlan
    let allowsApply: Bool

    var id: String {
        "\(action.rawValue)-\(plan.request.accountName)-\(plan.steps.map(\.kind).count)"
    }

    var title: String {
        switch action {
        case .create:
            return "Create Managed User"
        case .review:
            return "Review Managed User"
        case .repair:
            return "Repair Managed User"
        case .verify:
            return "Verify Managed User"
        case .remove:
            return "Remove Managed User"
        case .installHelper, .updateHelper, .uninstallHelper:
            return "Managed User Helper"
        }
    }

    var applyTitle: String {
        switch action {
        case .remove:
            return "Remove"
        case .repair:
            return "Apply Repair"
        case .create:
            return "Create"
        case .review, .verify:
            return "Apply"
        case .installHelper, .updateHelper, .uninstallHelper:
            return "Apply"
        }
    }
}

private struct ShellManagedUserCreationSheet: View {
    @Binding var draft: ManagedTerminalUserCreationDraft
    let previewResult: ManagedTerminalUserCreationPreviewResult?
    let diagnostics: [String]
    let isApplying: Bool
    let onDraftChanged: () -> Void
    let onPreview: () -> Void
    let onApply: (ManagedTerminalUserCreationPreview) -> Void
    let onCancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Create Managed User")
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            VStack(alignment: .leading, spacing: 10) {
                TextField("Unix user", text: $draft.unixUserName)
                TextField("Display label", text: $draft.displayLabel)
            }
            .textFieldStyle(.roundedBorder)
            .disabled(isApplying)

            previewContent

            HStack {
                Button("Cancel", role: .cancel, action: onCancel)
                    .disabled(isApplying)
                Spacer()
                if isApplying {
                    ProgressView()
                        .controlSize(.small)
                }
                Button("Review Plan", action: onPreview)
                    .disabled(isApplying)
                Button("Apply") {
                    if let preview = previewResult?.preview {
                        onApply(preview)
                    }
                }
                .disabled(isApplying || (previewResult?.preview?.plan.steps.isEmpty ?? true))
            }
        }
        .padding(24)
        .frame(width: 460, alignment: .leading)
        .onChange(of: draft) { _, _ in
            onDraftChanged()
        }
    }

    @ViewBuilder
    private var previewContent: some View {
        if let result = previewResult,
           !result.errors.isEmpty {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(result.errors.map(errorMessage), id: \.self) { message in
                    Label(message, systemImage: "exclamationmark.triangle")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        } else if let preview = previewResult?.preview {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(preview.visiblePlanRows, id: \.self) { row in
                    Label(row, systemImage: "checkmark")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        }

        if !diagnostics.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(diagnostics, id: \.self) { diagnostic in
                    Text(diagnostic)
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }
        }
    }

    private func errorMessage(_ error: ManagedTerminalUserCreationPreviewError) -> String {
        switch error {
        case .missingUnixUserName:
            return "Unix user is required."
        case .missingDisplayLabel:
            return "Display label is required."
        case .duplicateUnixUser(let user):
            return "\(user) already exists."
        case .terminalProfileConflict(let profileID):
            return "Terminal Profile \(profileID) already exists."
        case .validation:
            return "Use a valid local Unix user name."
        }
    }
}

private struct ShellManagedUserPlanSheet: View {
    let sheet: ShellManagedUserActionSheetState
    let diagnostics: [String]
    let isApplying: Bool
    let onApply: () -> Void
    let onCancel: () -> Void

    private var preview: ManagedTerminalUserCreationPreview {
        ManagedTerminalUserCreationPreview(request: sheet.plan.request, plan: sheet.plan)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(sheet.title)
                .font(ShellSettingsTypography.pageTitle)
                .foregroundStyle(ShellPalette.settingsPrimaryInk)

            Text("\(sheet.plan.request.fullName ?? sheet.plan.request.accountName) · \(planStatusText)")
                .font(ShellSettingsTypography.rowDetail)
                .foregroundStyle(ShellPalette.settingsSecondaryInk)

            VStack(alignment: .leading, spacing: 6) {
                ForEach(preview.visiblePlanRows, id: \.self) { row in
                    Label(row, systemImage: "checkmark")
                        .font(ShellSettingsTypography.rowDetail)
                        .foregroundStyle(ShellPalette.settingsSecondaryInk)
                }
            }

            if !diagnostics.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(diagnostics, id: \.self) { diagnostic in
                        Text(diagnostic)
                            .font(ShellSettingsTypography.rowDetail)
                            .foregroundStyle(ShellPalette.settingsSecondaryInk)
                    }
                }
            }

            HStack {
                Button("Close", role: .cancel, action: onCancel)
                    .disabled(isApplying)
                Spacer()
                if isApplying {
                    ProgressView()
                        .controlSize(.small)
                }
                if sheet.allowsApply {
                    Button(sheet.applyTitle, action: onApply)
                        .disabled(isApplying || sheet.plan.steps.isEmpty)
                }
            }
        }
        .padding(24)
        .frame(width: 460, alignment: .leading)
    }

    private var planStatusText: String {
        switch sheet.plan.status {
        case .alreadyReady:
            return "Ready"
        case .readyToApply:
            return "Ready to apply"
        case .repair:
            return "Repairable"
        case .helperUnavailable:
            return "Helper unavailable"
        case .accountNotAlanManaged:
            return "Not managed"
        case .legacySudoersPresent:
            return "Legacy sudoers"
        case .ptySpawnFailed:
            return "PTY failed"
        case .invalid:
            return "Invalid"
        case .requiresDestructiveConfirmation:
            return "Needs confirmation"
        case .sudoersConflict:
            return "Sudoers conflict"
        case .terminalProfileConflict:
            return "Terminal Profile conflict"
        }
    }
}

@MainActor
private func shellSettingsCopyToPasteboard(_ value: String?) {
    guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
          !value.isEmpty
    else {
        return
    }
    ShellClipboardWriter().writeString(value)
}

@MainActor
private func shellSettingsOpenFolder(_ value: String?) {
    ShellLocalFolderOpener.openFolder(displayPath: value)
}

private struct ShellSettingsDivider: View {
    var body: some View {
        Divider()
            .opacity(0.45)
            .padding(.leading, ShellSettingsMetrics.rowDividerLeadingPadding)
    }
}

private enum ShellSettingsMetrics {
    static let navigationWidth: CGFloat = 188
    static let navigationLeadingPadding: CGFloat = 12
    static let navigationTrailingPadding: CGFloat = 8
    static let navigationTopPadding: CGFloat = 24
    static let navigationRowHeight: CGFloat = 30
    static let navigationRowSpacing: CGFloat = 2
    static let navigationRowHorizontalPadding: CGFloat = 8
    static let navigationRowContentSpacing: CGFloat = 12
    static let navigationIconSlotWidth: CGFloat = 18
    static let navigationSelectionCornerRadius: CGFloat = 7
    static let contentWidth: CGFloat = 760
    static let detailContentLeadingPadding: CGFloat = 48
    static let detailContentTrailingPadding: CGFloat = 48
    static let detailContentTopPadding: CGFloat = 42
    static let detailContentBottomPadding: CGFloat = 40
    static let pageTitleToSectionsSpacing: CGFloat = 26
    static let sectionSpacing: CGFloat = 28
    static let sectionTitleBottomPadding: CGFloat = 10
    static let rowVerticalPadding: CGFloat = 8
    static let rowMinHeight: CGFloat = 48
    static let rowMinHeightWithDetail: CGFloat = 56
    static let agentSummaryRowMinHeight: CGFloat = 58
    static let rowTextSpacing: CGFloat = 1
    static let rowColumnSpacing: CGFloat = 20
    static let rowDividerLeadingPadding: CGFloat = 0
    static let accessoryColumnWidth: CGFloat = 188
    static let valueColumnWidth: CGFloat = 220
    static let inlineActionSpacing: CGFloat = 8
    static let inlineIconButtonSize: CGFloat = 22
    static let segmentedControlWidth: CGFloat = 196
    static let disabledButtonOpacity: CGFloat = 0.55
}

private enum ShellSettingsTypography {
    static let navigationIcon = Font.system(size: 13, weight: .regular)

    static func navigationLabel(selected: Bool) -> Font {
        .system(size: 13, weight: selected ? .semibold : .regular)
    }

    static let pageTitle = Font.system(size: 22, weight: .semibold)
    static let sectionTitle = Font.system(size: 11, weight: .semibold)
    static let rowTitle = Font.system(size: 13, weight: .semibold)
    static let rowDetail = Font.system(size: 12, weight: .regular)
    static let accessory = Font.system(size: 13, weight: .regular)
    static let value = Font.system(size: 13, weight: .medium)
    static let agentName = Font.system(size: 15, weight: .semibold)
    static let badge = Font.system(size: 11.5, weight: .medium)
    static let valueActionIcon = Font.system(size: 9.5, weight: .semibold)
    static let inlineActionIcon = Font.system(size: 10.5, weight: .medium)
    static let actionButton = Font.system(size: 12.3, weight: .medium)
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
                .font(ShellType.pro(ShellPaneTitleTypography.accessorySize, weight: .medium))
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
        .background(titleBarBackground)
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

    private var titleBarBackground: Color {
        descriptor.renderKind == .settings ? ShellPalette.settingsPane : ShellPalette.workspace
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

        if Task.isCancelled { return }
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
            return ShellSignal.action
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
            return ShellSignal.action
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

    // Machine facts (paths, branches, process names) render in the mono accent
    // track; human-language accessories (activity/status/alan) stay in pro.
    // See docs/design/design-language.md, principle 4.
    var isMachineFact: Bool {
        id == "worktree" || id == "cwd" || id == "branch" || id == "process"
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
                    ShellType.pro(
                        ShellPaneTitleTypography.accessorySize,
                        weight: ShellPaneTitleTypography.iconWeight
                    )
                )

            if mode == .textAndIcon,
               let title = accessory.title {
                // Only machine-fact accessories (worktree/cwd/branch/process)
                // render in the mono accent track; human-language accessories
                // (activity/status/alan) stay in the pro track at the same
                // size. See docs/design/design-language.md, principle 4.
                Text(title)
                    .font(accessoryFont)
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

    private var accessoryFont: Font {
        let weight = accessory.isEmphasized
            ? ShellPaneTitleTypography.emphasizedAccessoryWeight
            : ShellPaneTitleTypography.accessoryWeight
        if accessory.isMachineFact {
            return ShellType.mono(ShellType.monoCaption, weight: weight)
        }
        return ShellType.pro(ShellPaneTitleTypography.accessorySize, weight: weight)
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
