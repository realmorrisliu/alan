import Foundation
import SwiftUI

struct ShellPaneTreeLayoutView: View {
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
        case .markdown, .settings, .agent, .unavailable:
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
        return ShellBoundedContentLeafView(
            descriptor: descriptor,
            paneSlotID: paneSlotID,
            onAgentRendererStateUpdate: { offsets, presentation in
                host.updateAgentRendererState(
                    paneID: paneSlotID,
                    offsets: offsets,
                    presentation: presentation
                )
            },
            onOpenAgentView: { attachment in
                _ = host.openAgentTab(attachment: attachment)
            },
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
