import SwiftUI

#if os(macOS)
struct ShellPaneTopologyIndicator: View {
    let summary: ShellTabPaneSummary
    let isSelected: Bool
    let onFocusSplitPane: (String) -> Void
    let onFocusNextSplitPane: (ShellTabPaneSummary) -> Void

    @ViewBuilder
    var body: some View {
        switch summary.topology.kind {
        case .single:
            singlePaneIndicator
        case .columns(let count):
            columnsIndicator(paneIDs: Array(summary.paneIDs.prefix(count)))
        case .rows(let count):
            rowsIndicator(paneIDs: Array(summary.paneIDs.prefix(count)))
        case .mainLeftWithRightStack:
            mainLeftWithRightStackIndicator
        case .mainRightWithLeftStack:
            mainRightWithLeftStackIndicator
        case .mainTopWithBottomSplit:
            mainTopWithBottomSplitIndicator
        case .mainBottomWithTopSplit:
            mainBottomWithTopSplitIndicator
        case .grid2x2(let rootDirection):
            gridIndicator(rootDirection: rootDirection)
        case .complex:
            complexButton
        }
    }

    private var singlePaneIndicator: some View {
        indicatorFrame {
            RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
                .fill(singlePaneFill)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityLabel("Single pane")
    }

    private func columnsIndicator(paneIDs: [String]) -> some View {
        indicatorFrame {
            HStack(spacing: 2) {
                ForEach(paneIDs, id: \.self) { paneID in
                    segmentButton(paneID: paneID)
                }
            }
        }
        .help("Focus split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    private func rowsIndicator(paneIDs: [String]) -> some View {
        indicatorFrame {
            VStack(spacing: 2) {
                ForEach(paneIDs, id: \.self) { paneID in
                    segmentButton(paneID: paneID)
                }
            }
        }
        .help("Focus split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    @ViewBuilder
    private var mainLeftWithRightStackIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                HStack(spacing: 2) {
                    segmentButton(paneID: paneIDs[0])
                    VStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[1])
                        segmentButton(paneID: paneIDs[2])
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainRightWithLeftStackIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                HStack(spacing: 2) {
                    VStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[0])
                        segmentButton(paneID: paneIDs[1])
                    }
                    segmentButton(paneID: paneIDs[2])
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainTopWithBottomSplitIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                VStack(spacing: 2) {
                    segmentButton(paneID: paneIDs[0])
                    HStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[1])
                        segmentButton(paneID: paneIDs[2])
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private var mainBottomWithTopSplitIndicator: some View {
        let paneIDs = Array(summary.paneIDs.prefix(3))
        if paneIDs.count == 3 {
            indicatorFrame {
                VStack(spacing: 2) {
                    HStack(spacing: 2) {
                        segmentButton(paneID: paneIDs[0])
                        segmentButton(paneID: paneIDs[1])
                    }
                    segmentButton(paneID: paneIDs[2])
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    @ViewBuilder
    private func gridIndicator(rootDirection: ShellSplitDirection) -> some View {
        let paneIDs = Array(summary.paneIDs.prefix(4))
        if paneIDs.count == 4 {
            indicatorFrame {
                if rootDirection == .vertical {
                    HStack(spacing: 2) {
                        VStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[0])
                            segmentButton(paneID: paneIDs[1])
                        }
                        VStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[2])
                            segmentButton(paneID: paneIDs[3])
                        }
                    }
                } else {
                    VStack(spacing: 2) {
                        HStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[0])
                            segmentButton(paneID: paneIDs[1])
                        }
                        HStack(spacing: 2) {
                            segmentButton(paneID: paneIDs[2])
                            segmentButton(paneID: paneIDs[3])
                        }
                    }
                }
            }
            .help("Focus split pane")
            .accessibilityLabel(splitAccessibilityLabel)
        } else {
            complexButton
        }
    }

    private func indicatorFrame<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: ShellRadii.badge, style: .continuous)

        return content()
            .padding(3)
            .frame(width: 22, height: 18)
            .background {
                shape
                    .fill(containerFill)
                    .overlay {
                        shape.stroke(ShellPalette.line.opacity(isSelected ? 0.20 : 0.15), lineWidth: 0.5)
                    }
            }
    }

    static func placeholder(isSelected: Bool) -> some View {
        let shape = RoundedRectangle(cornerRadius: ShellRadii.badge, style: .continuous)
        return shape
            .fill(ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.20 : 0.13))
            .frame(width: 22, height: 18)
            .overlay {
                shape.stroke(ShellPalette.line.opacity(isSelected ? 0.20 : 0.15), lineWidth: 0.5)
            }
            .accessibilityLabel("Pane")
    }

    private var containerFill: Color {
        ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.105 : 0.075)
    }

    private var primaryPaneFill: Color {
        isSelected ? ShellPalette.accent.opacity(0.82) : ShellPalette.sidebarMutedInk.opacity(0.38)
    }

    /// Single-pane topology has no focus-within-split to mark, so selection is
    /// conveyed by the row surface — not focus indigo. A neutral ink, a touch
    /// stronger when selected, keeps the action/focus accents scarce
    /// (design-language.md principle 3).
    private var singlePaneFill: Color {
        ShellPalette.sidebarInk.opacity(isSelected ? 0.55 : 0.38)
    }

    private func paneFill(isFocused: Bool) -> Color {
        if isFocused {
            return ShellPalette.accent.opacity(isSelected ? 0.88 : 0.78)
        }
        return ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.40 : 0.32)
    }

    private var complexButton: some View {
        Button {
            onFocusNextSplitPane(summary)
        } label: {
            complexCountOverlay
        }
        .buttonStyle(.plain)
        .help("Focus next split pane")
        .accessibilityLabel(splitAccessibilityLabel)
    }

    private var complexCountOverlay: some View {
        indicatorFrame {
            ZStack {
                RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
                    .fill(primaryPaneFill)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                Text("\(summary.paneCount)")
                    .font(.system(size: 8.5, weight: .bold, design: .monospaced))
                    .foregroundStyle(complexCountForeground)
            }
        }
    }

    private var complexCountForeground: Color {
        isSelected ? Color.white.opacity(0.92) : ShellPalette.sidebarInk.opacity(0.76)
    }

    private func segmentButton(paneID: String) -> some View {
        Button {
            onFocusSplitPane(paneID)
        } label: {
            segmentView(paneID: paneID)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(summary.focusedPaneID == paneID ? "Focused split pane" : "Focus split pane")
        .accessibilityLabel(summary.focusedPaneID == paneID ? "Focused split pane" : "Focus split pane")
    }

    private func segmentView(paneID: String) -> some View {
        let isFocused = summary.focusedPaneID == paneID

        return RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
            .fill(paneFill(isFocused: isFocused))
    }

    private var splitAccessibilityLabel: String {
        "Split tab, \(summary.accessibilityTopologyLabel)"
    }
}
#endif
