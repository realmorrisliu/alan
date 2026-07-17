import SwiftUI

#if os(macOS)
struct ShellSidebarActivityProgressRail: View {
    let progress: TerminalActivityProgress
    let isSelected: Bool

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.16 : 0.12))

                Capsule()
                    .fill(fillColor)
                    .frame(width: fillWidth(in: proxy.size.width))
            }
        }
        .frame(height: 2)
        .accessibilityHidden(true)
    }

    private var fillColor: Color {
        switch progress.kind {
        case .failed:
            return ShellSignal.action.opacity(isSelected ? 0.86 : 0.72)
        case .paused:
            return ShellPalette.sidebarMutedInk.opacity(isSelected ? 0.62 : 0.48)
        case .percent, .indeterminate:
            return ShellPalette.accent.opacity(isSelected ? 0.82 : 0.68)
        }
    }

    private func fillWidth(in width: CGFloat) -> CGFloat {
        switch progress.kind {
        case .percent:
            return width * CGFloat(progress.percent ?? 0) / 100
        case .indeterminate:
            return max(width * 0.36, 18)
        case .paused, .failed:
            return width
        }
    }
}
#endif
