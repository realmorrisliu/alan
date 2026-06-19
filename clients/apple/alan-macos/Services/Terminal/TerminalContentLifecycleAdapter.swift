import Foundation

#if os(macOS)
@MainActor
struct TerminalContentLifecycleAdapter {
    func reconcileRuntimes(
        afterAdopting state: ShellStateSnapshot,
        registry: TerminalRuntimeRegistry
    ) {
        registry.releaseRuntimes(excluding: activeTerminalMounts(in: state))
    }

    func finalizeAllRuntimes(registry: TerminalRuntimeRegistry) {
        registry.releaseAllRuntimes()
    }

    func activeTerminalMounts(in state: ShellStateSnapshot) -> [TerminalContentMount] {
        let contentState = state.contentStateProjection()
        var contentsByID: [String: ShellContentInstance] = [:]
        contentState.contents.forEach { content in
            contentsByID[content.contentID] = content
        }
        let mountedPaneSlotIDs = Set(
            contentState.spaces
                .flatMap(\.tabs)
                .flatMap(\.paneTree.paneSlotIDs)
        )
        return contentState.paneSlots.compactMap { slot -> TerminalContentMount? in
            guard mountedPaneSlotIDs.contains(slot.paneSlotID),
                  let content = contentsByID[slot.contentID],
                  content.kind == .terminal,
                  content.lifecycle == .active
            else {
                return nil
            }

            return TerminalContentMount(
                contentID: content.contentID,
                paneSlotID: slot.paneSlotID,
                tabID: slot.tabID,
                spaceID: slot.spaceID
            )
        }
    }
}
#endif
