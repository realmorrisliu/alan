import SwiftUI

#if os(macOS)
private enum ShellSidebarRowVisualState: Equatable {
    case normal
    case hover
    case selected

    var cornerRadius: CGFloat {
        switch self {
        case .normal:
            return ShellRadii.row
        case .hover:
            return ShellRadii.control
        case .selected:
            return ShellRadii.row
        }
    }

    var fill: Color? {
        switch self {
        case .normal:
            return nil
        case .hover:
            return ShellPalette.sidebarRowHover
        case .selected:
            return ShellPalette.sidebarRowSelected
        }
    }

    var stroke: Color {
        switch self {
        case .normal:
            return .clear
        case .hover:
            return ShellPalette.line.opacity(0.08)
        case .selected:
            return ShellPalette.line.opacity(0.12)
        }
    }

    var shadow: ShellShadowStyle {
        switch self {
        case .normal, .hover:
            return ShellShadows.none
        case .selected:
            return ShellShadows.navigationSelection
        }
    }
}

private struct ShellSidebarRowBackground: View {
    @Environment(\.colorScheme) private var colorScheme
    let state: ShellSidebarRowVisualState

    var body: some View {
        if let fill = state.fill {
            let shape = RoundedRectangle(cornerRadius: state.cornerRadius, style: .continuous)
            shape
                .fill(fill)
                .overlay {
                    shape.stroke(state.stroke, lineWidth: 0.5)
                }
                .overlay {
                    if colorScheme == .light && state == .selected {
                        shape
                            .stroke(Color.white.opacity(0.34), lineWidth: 0.55)
                            .mask {
                                shape.fill(
                                    LinearGradient(
                                        colors: [
                                            Color.white,
                                            Color.white.opacity(0),
                                        ],
                                        startPoint: .top,
                                        endPoint: .bottom
                                    )
                                )
                            }
                    }
                }
                .shellShadow(state.shadow)
        }
    }
}

private enum ShellSidebarTypography {
    static let titleSize: CGFloat = 14
    static let markerSize: CGFloat = 9
    static let closeSize: CGFloat = 9.5

    static func titleWeight(isSelected: Bool) -> Font.Weight {
        isSelected ? .medium : .regular
    }

    static let secondaryWeight: Font.Weight = .regular
    static let secondaryEmphasisWeight: Font.Weight = .medium
    static let iconWeight: Font.Weight = .medium
    static let markerWeight: Font.Weight = .semibold
}

enum ShellSidebarRowMetrics {
    static let height: CGFloat = 36
    static let horizontalInset: CGFloat = 8
    static let leadingSlot: CGFloat = 24
    static let trailingSlot: CGFloat = 20
    static let dragMidpoint: CGFloat = height / 2
}

enum ShellSidebarTabListMetrics {
    static let itemSpacing: CGFloat = 6
    static let sliderToListLift: CGFloat = 12
}

enum ShellSidebarTabDragState {
    static let dragThreshold: CGFloat = 7
}

private enum ShellSidebarTabControlMetrics {
    static let hitHeight: CGFloat = 16
    static let horizontalInset: CGFloat = ShellSidebarRowMetrics.horizontalInset
}

struct ShellSidebarTabControlRow: View {
    @State private var isControlHovered = false
    let showsDivider: Bool
    let showsClear: Bool
    let isClearEnabled: Bool
    let clearAction: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            if showsDivider {
                Rectangle()
                    .fill(ShellPalette.sidebarDivider)
                    .frame(maxWidth: .infinity, minHeight: 1, maxHeight: 1)
            }

            if showsClear && isControlHovered {
                Button(action: clearAction) {
                    Label("Clear", systemImage: "arrow.down.to.line.compact")
                        .labelStyle(.titleAndIcon)
                        .font(.system(size: 11, weight: .semibold))
                }
                .buttonStyle(.plain)
                .disabled(!isClearEnabled)
                .foregroundStyle(ShellPalette.sidebarMutedInk.opacity(isClearEnabled ? 0.72 : 0.44))
                .help("Clear inactive tabs")
                .fixedSize()
            }
        }
        .padding(.horizontal, ShellSidebarTabControlMetrics.horizontalInset)
        .frame(
            maxWidth: .infinity,
            minHeight: ShellSidebarTabControlMetrics.hitHeight,
            maxHeight: ShellSidebarTabControlMetrics.hitHeight,
            alignment: .center
        )
        .contentShape(Rectangle())
        .onHover { isControlHovered = $0 }
        .animation(.easeInOut(duration: 0.12), value: isControlHovered)
    }
}

struct ShellTabSidebarRow: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var isKeyboardFocused: Bool
    @State private var isCloseHovered = false
    let title: String
    let subtitle: String?
    let isActivitySubtitle: Bool
    let secondaryIsMachineFact: Bool
    let progress: TerminalActivityProgress?
    let stateAccessory: ShellSidebarTabStateAccessory?
    let attention: ShellAttentionState?
    let showsAlanMarker: Bool
    let paneSummary: ShellTabPaneSummary?
    let isPinned: Bool
    let isSelected: Bool
    let isHovered: Bool
    let showsCloseAffordance: Bool
    let onFocusSplitPane: (String) -> Void
    let onFocusNextSplitPane: (ShellTabPaneSummary) -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            leadingSlot
                .frame(width: ShellSidebarRowMetrics.leadingSlot, height: ShellSidebarRowMetrics.leadingSlot, alignment: .center)

            VStack(alignment: .leading, spacing: subtitle == nil ? 0 : 1) {
                HStack(spacing: 6) {
                    Text(title)
                        .font(
                            .system(
                                size: ShellSidebarTypography.titleSize,
                                weight: ShellSidebarTypography.titleWeight(isSelected: isSelected)
                            )
                        )
                        .foregroundStyle(titleForeground)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    if showsAlanMarker {
                        Image(systemName: "sparkles")
                            .font(
                                .system(
                                    size: ShellSidebarTypography.markerSize,
                                    weight: ShellSidebarTypography.markerWeight
                                )
                            )
                            .foregroundStyle(ShellPalette.accent)
                    }

                }

                if let subtitle {
                    subtitleText(subtitle)
                        .foregroundStyle(subtitleForeground)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }

                if let progress {
                    ShellSidebarActivityProgressRail(progress: progress, isSelected: isSelected)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            closeButtonSlot
        }
        .padding(.horizontal, ShellSidebarRowMetrics.horizontalInset)
        .frame(minHeight: ShellSidebarRowMetrics.height)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            ShellSidebarRowBackground(state: visualState)
        )
        .contentShape(RoundedRectangle(cornerRadius: visualState.cornerRadius, style: .continuous))
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: visualState)
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: showsCloseButton)
        .focusable()
        .focused($isKeyboardFocused)
        .focusEffectDisabled()
        .accessibilityLabel(accessibilityLabel)
        .help("Select tab")
    }

    private var visualState: ShellSidebarRowVisualState {
        if isSelected {
            return .selected
        }

        if isHovered || isKeyboardFocused {
            return .hover
        }

        return .normal
    }

    private var isInteractionActive: Bool {
        isHovered || showsCloseAffordance || isKeyboardFocused
    }

    private var showsCloseButton: Bool {
        isSelected || isInteractionActive
    }

    @ViewBuilder
    private var closeButtonSlot: some View {
        ZStack {
            if showsCloseButton {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(
                            .system(
                                size: ShellSidebarTypography.closeSize,
                                weight: ShellSidebarTypography.markerWeight
                            )
                        )
                        .foregroundStyle(closeForeground)
                        .frame(width: ShellSidebarRowMetrics.trailingSlot, height: ShellSidebarRowMetrics.trailingSlot)
                        .contentShape(Circle())
                        .background {
                            if isCloseHovered || isKeyboardFocused {
                                Circle()
                                    .fill(ShellPalette.sidebarInk.opacity(isSelected ? 0.05 : 0.035))
                            }
                        }
                }
                .buttonStyle(.plain)
                .help("Close tab")
                .accessibilityLabel("Close tab")
                .accessibilityHidden(!showsCloseButton)
                .onHover { isHovering in
                    isCloseHovered = isHovering
                }
            } else if let stateAccessory {
                Image(systemName: stateAccessory.systemImageName)
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(ShellPalette.sidebarMutedInk.opacity(0.70))
                    .frame(width: ShellSidebarRowMetrics.trailingSlot, height: ShellSidebarRowMetrics.trailingSlot)
                    .help(stateAccessory.accessibilityLabel)
                    .accessibilityLabel(stateAccessory.accessibilityLabel)
            } else {
                Color.clear
                    .frame(width: ShellSidebarRowMetrics.trailingSlot, height: ShellSidebarRowMetrics.trailingSlot)
                    .accessibilityHidden(true)
            }
        }
    }

    @ViewBuilder
    private var leadingSlot: some View {
        if let paneSummary {
            ShellPaneTopologyIndicator(
                summary: paneSummary,
                isSelected: isSelected,
                onFocusSplitPane: onFocusSplitPane,
                onFocusNextSplitPane: onFocusNextSplitPane
            )
        } else {
            ShellPaneTopologyIndicator.placeholder(isSelected: isSelected)
        }
    }

    private func subtitleText(_ subtitle: String) -> Text {
        let parts = subtitle.components(separatedBy: " · ")
        guard isActivitySubtitle, !parts.isEmpty else {
            // Only machine facts (cwd/branch/process context) render in the mono
            // accent track; human-language status summaries and content-type
            // hints stay in SF Pro. See docs/design/design-language.md,
            // principle 4.
            if secondaryIsMachineFact {
                return Text(subtitle)
                    .font(ShellType.mono(ShellType.monoCaption, weight: ShellSidebarTypography.secondaryWeight))
            }
            return Text(subtitle)
                .font(ShellType.pro(ShellType.caption, weight: ShellSidebarTypography.secondaryWeight))
        }

        let emphasizedIndex = emphasizedSubtitleIndex(for: parts)
        var attributedSubtitle = AttributedString()
        for element in parts.enumerated() {
            let (index, part) = element
            let prefix = index == 0 ? "" : " · "
            let weight = index == emphasizedIndex
                ? ShellSidebarTypography.secondaryEmphasisWeight
                : ShellSidebarTypography.secondaryWeight
            var fragment = AttributedString(prefix + part)
            fragment.font = ShellType.pro(ShellType.caption, weight: weight)
            attributedSubtitle += fragment
        }
        return Text(attributedSubtitle)
    }

    private func emphasizedSubtitleIndex(for parts: [String]) -> Int {
        if parts.count >= 3,
           parts[0].hasPrefix("Pane ")
        {
            return 1
        }
        return 0
    }

    private var titleForeground: Color {
        isSelected ? ShellPalette.sidebarInk : ShellPalette.sidebarInk.opacity(0.88)
    }

    private var subtitleForeground: Color {
        isSelected ? ShellPalette.sidebarMutedInk.opacity(0.90) : ShellPalette.sidebarMutedInk.opacity(0.68)
    }

    private var closeForeground: Color {
        if isCloseHovered {
            return ShellPalette.sidebarInk.opacity(isSelected ? 0.68 : 0.76)
        }
        return isSelected ? ShellPalette.sidebarInk.opacity(0.46) : ShellPalette.sidebarMutedInk.opacity(0.62)
    }

    private var accessibilityLabel: String {
        var parts = [title]
        if let subtitle {
            parts.append(subtitle)
        }
        if isSelected {
            parts.append("selected")
        }
        if attention != nil {
            parts.append("needs attention")
        }
        if let paneSummary {
            parts.append(paneSummary.paneCount == 1 ? "1 pane" : "\(paneSummary.paneCount) panes")
        }
        if isPinned {
            parts.append("pinned")
        }
        return parts.joined(separator: ", ")
    }
}

struct ShellCompactEmptyAction: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var isKeyboardFocused: Bool
    @State private var isHovered = false
    let title: String
    let systemImage: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: systemImage)
                    .font(.system(size: 15, weight: .regular))
                    .frame(width: ShellSidebarRowMetrics.leadingSlot, height: ShellSidebarRowMetrics.leadingSlot)
                Text(title)
                    .font(.system(size: ShellSidebarTypography.titleSize, weight: .regular))
                Spacer(minLength: 0)
            }
            .foregroundStyle(foreground)
            .padding(.horizontal, ShellSidebarRowMetrics.horizontalInset)
            .frame(minHeight: ShellSidebarRowMetrics.height)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                ShellSidebarRowBackground(state: visualState)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusable()
        .focused($isKeyboardFocused)
        .focusEffectDisabled()
        .onHover { isHovered = $0 }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.14), value: visualState)
        .accessibilityLabel(title)
    }

    private var visualState: ShellSidebarRowVisualState {
        isHovered || isKeyboardFocused ? .hover : .normal
    }

    private var foreground: Color {
        isHovered || isKeyboardFocused
            ? ShellPalette.sidebarMutedInk.opacity(0.86)
            : ShellPalette.sidebarMutedInk.opacity(0.58)
    }
}
#endif
