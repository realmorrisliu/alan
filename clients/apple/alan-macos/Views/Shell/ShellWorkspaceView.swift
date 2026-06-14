import SwiftUI

#if os(macOS)
struct ShellWorkspaceView: View {
    @ObservedObject var host: ShellHostController
    let expandedSidebarProgress: CGFloat

    var body: some View {
        let contentState = host.shellState.contentStateProjection()
        TerminalPaneView(
            host: host,
            tab: host.selectedTab,
            spaceID: host.selectedSpace?.spaceID,
            selectedPaneID: contentState.focusedPaneSlotID,
            zoomedPaneID: host.selectedTabZoomedPaneID,
            workspacePanelInsets: ShellWorkspaceMetrics.workspacePanelInsets(
                expandedSidebarProgress: expandedSidebarProgress
            )
        )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct ShellSpaceKeyboardShortcuts: View {
    @ObservedObject var host: ShellHostController

    var body: some View {
        VStack(spacing: 0) {
            // While the Space creation form is open the slider shows a
            // display-only draft target; space-switching shortcuts must stay
            // inert so the draft and the prior selection are preserved.
            if !host.isPresentingSpaceCreation {
                Button("") {
                    host.performShellAction(.spaceSelectPrevious)
                }
                .shellActionKeyboardShortcut(host.shellActionShortcut(.spaceSelectPrevious))

                Button("") {
                    host.performShellAction(.spaceSelectNext)
                }
                .shellActionKeyboardShortcut(host.shellActionShortcut(.spaceSelectNext))

                ForEach(
                    Array(host.spaces.enumerated()),
                    id: \.element.spaceID
                ) { index, _ in
                    let target = ShellActionTarget.spaceIndex(index)
                    Button("") {
                        host.performShellAction(.spaceSelectByIndex, target: target)
                    }
                    .shellActionKeyboardShortcut(host.shellActionShortcut(.spaceSelectByIndex, target: target))
                }
            }
        }
        .labelsHidden()
        .buttonStyle(.plain)
        .frame(width: 0, height: 0)
        .opacity(0.001)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}
#endif
