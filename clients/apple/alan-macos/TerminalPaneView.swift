import Foundation
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
                TerminalInfoRow(label: "Process", value: binding.processPath)
                TerminalInfoRow(label: "Machine", value: binding.machineState)
                TerminalInfoRow(label: "Request", value: binding.pendingRequest ? "pending" : "none")
                TerminalInfoRow(label: "Source", value: binding.source ?? "binding file")
                TerminalInfoRow(label: "Projected", value: binding.lastProjectedAt ?? "pending")
            } else {
                Text("This pane is shell-addressable even when no Alan Process is projected onto it.")
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
                    .fill(ShellPaper.root)
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

struct ShellContentPaneTitleBarView: View {
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

struct ShellPaneTitleBarView: View {
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
            return pane.alanBinding?.pendingRequest == true
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

struct ShellFindBarView: View {
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

struct ShellInactivePaneDim: View {
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
