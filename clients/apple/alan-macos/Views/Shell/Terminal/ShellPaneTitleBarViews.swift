import Foundation
import SwiftUI

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
