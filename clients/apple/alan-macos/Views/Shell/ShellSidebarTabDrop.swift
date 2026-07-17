import SwiftUI
import UniformTypeIdentifiers

#if os(macOS)
struct ShellSidebarTabListOffsetPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

struct ShellSidebarTabDropTarget: Equatable {
    let spaceID: String
    let section: ShellTabOrganizationSection
    let index: Int
}

struct ShellSidebarTabDropDelegate: DropDelegate {
    let target: ShellSidebarTabDropTarget
    @Binding var activeDrag: ShellSidebarTabDragSource?
    @Binding var preview: ShellSidebarTabDropTarget?
    let host: ShellHostController

    func dropEntered(info: DropInfo) {
        preview = resolvedTarget(for: info)
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        preview = resolvedTarget(for: info)
        return DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        if preview == target {
            preview = nil
        }
    }

    func performDrop(info: DropInfo) -> Bool {
        let insertionTarget = resolvedTarget(for: info)
        preview = nil

        if let activeDrag {
            self.activeDrag = nil
            return performDrop(source: activeDrag, insertionTarget: insertionTarget)
        }

        self.activeDrag = nil
        guard let provider = info.itemProviders(for: [.plainText]).first else {
            return false
        }
        provider.loadObject(ofClass: NSString.self) { object, _ in
            guard let payload = object as? String,
                  let source = try? ShellSidebarTabDragSource.decodedPlainTextPayload(payload)
            else {
                return
            }
            DispatchQueue.main.async {
                _ = performDrop(source: source, insertionTarget: insertionTarget)
            }
        }
        return true
    }

    private func performDrop(
        source: ShellSidebarTabDragSource,
        insertionTarget: ShellSidebarTabDropTarget
    ) -> Bool {
        let activeDrag = source
        let mutationIndex = mutationIndex(for: insertionTarget, activeDrag: activeDrag)

        if source.sourceSpaceID == insertionTarget.spaceID,
           source.sourceSection == insertionTarget.section,
           source.sourceIndex == mutationIndex
        {
            return true
        }

        return host.reorderTab(
            tabID: source.tabID,
            targetSpaceID: insertionTarget.spaceID,
            section: insertionTarget.section,
            index: mutationIndex
        )
    }

    private func mutationIndex(
        for insertionTarget: ShellSidebarTabDropTarget,
        activeDrag: ShellSidebarTabDragSource
    ) -> Int {
        ShellSidebarTabDropModel.mutationIndex(
            for: ShellSidebarTabInsertionTarget(
                spaceID: insertionTarget.spaceID,
                section: insertionTarget.section,
                index: insertionTarget.index
            ),
            source: activeDrag
        )
    }

    private func resolvedTarget(for info: DropInfo) -> ShellSidebarTabDropTarget {
        let rowMidpoint = ShellSidebarRowMetrics.dragMidpoint
        let sectionCount = host.shellState
            .space(spaceID: target.spaceID)?
            .tabs(in: target.section)
            .count ?? target.index
        let adjustedIndex = info.location.y > rowMidpoint
            ? target.index + 1
            : target.index
        return ShellSidebarTabDropTarget(
            spaceID: target.spaceID,
            section: target.section,
            index: min(max(adjustedIndex, 0), sectionCount)
        )
    }
}

struct ShellSidebarTabInsertionLine: View {
    let isVisible: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: ShellRadii.micro, style: .continuous)
            .fill(ShellPalette.accent.opacity(isVisible ? 0.72 : 0))
            .frame(height: isVisible ? 2 : 0)
            .padding(.horizontal, ShellSidebarMetrics.rowInset + 2)
            .padding(.vertical, isVisible ? 3 : 0)
            .animation(.easeOut(duration: 0.10), value: isVisible)
            .accessibilityHidden(true)
    }
}

struct ShellSidebarScrollBoundary: View {
    let progress: CGFloat

    var body: some View {
        VStack(spacing: 0) {
            Rectangle()
                .fill(ShellPalette.line.opacity(0.36))
                .frame(height: 0.5)

            LinearGradient(
                colors: [
                    ShellPalette.sidebarInk.opacity(0.10),
                    ShellPalette.sidebarInk.opacity(0.035),
                    ShellPalette.sidebarInk.opacity(0),
                ],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: 14)
        }
        .frame(maxWidth: .infinity, alignment: .top)
        .opacity(progress)
        .allowsHitTesting(false)
    }
}
#endif
