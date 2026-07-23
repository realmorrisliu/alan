struct TerminalContentMount: Equatable {
    let contentID: String
    let paneSlotID: String
    let tabID: String
    let spaceID: String

    init(contentID: String, paneSlotID: String, tabID: String, spaceID: String) {
        self.contentID = contentID
        self.paneSlotID = paneSlotID
        self.tabID = tabID
        self.spaceID = spaceID
    }

    init(pane: ShellPane) {
        self.init(
            contentID: pane.terminalContentID,
            paneSlotID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID
        )
    }
}

extension ShellContentStateSnapshot {
    var activeTerminalMounts: [TerminalContentMount] {
        var contentsByID: [String: ShellContentInstance] = [:]
        contents.forEach { content in
            contentsByID[content.contentID] = content
        }
        let mountedPaneSlotIDs = Set(
            spaces
                .flatMap(\.tabs)
                .flatMap(\.paneTree.paneSlotIDs)
        )
        return paneSlots.compactMap { slot -> TerminalContentMount? in
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
