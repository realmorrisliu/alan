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
