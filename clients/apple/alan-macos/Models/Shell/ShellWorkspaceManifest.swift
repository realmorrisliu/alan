import Foundation

struct ShellContentWorkspaceManifest: Codable, Equatable {
    static let currentSchemaVersion = 1
    static let currentContentContractVersion = ShellContentStateSnapshot.currentContractVersion

    var schemaVersion: Int
    var contentContractVersion: String
    var windowID: String
    var selectedSpaceID: String?
    var selectedTabID: String?
    var spaces: [ShellContentWorkspaceSpaceRecord]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case contentContractVersion = "content_contract_version"
        case windowID = "window_id"
        case selectedSpaceID = "selected_space_id"
        case selectedTabID = "selected_tab_id"
        case spaces
    }
}

extension ShellContentWorkspaceManifest {
    mutating func repairSelection() {
        guard !spaces.isEmpty else {
            selectedSpaceID = nil
            selectedTabID = nil
            return
        }

        if selectedSpaceID == nil || !spaces.contains(where: { $0.spaceID == selectedSpaceID }) {
            selectedSpaceID = spaces.first?.spaceID
        }

        for index in spaces.indices {
            let legacySelectedTabID =
                spaces[index].spaceID == selectedSpaceID ? selectedTabID : nil
            spaces[index].selectedTabID = repairedSelectedTabID(
                spaces[index].selectedTabID,
                legacySelectedTabID: legacySelectedTabID,
                tabs: spaces[index].tabs.map(\.tabID)
            )
        }

        guard let selectedSpaceID,
              let selectedSpace = spaces.first(where: { $0.spaceID == selectedSpaceID })
        else {
            selectedTabID = nil
            return
        }

        selectedTabID = selectedSpace.selectedTabID
    }

    private func repairedSelectedTabID(
        _ selectedTabID: String?,
        legacySelectedTabID: String?,
        tabs: [String]
    ) -> String? {
        if let selectedTabID, tabs.contains(selectedTabID) {
            return selectedTabID
        }
        if let legacySelectedTabID, tabs.contains(legacySelectedTabID) {
            return legacySelectedTabID
        }
        return tabs.first
    }

    func clearingRestoredTranscriptSnapshot(
        forTerminalContentID contentID: String
    ) -> (manifest: ShellContentWorkspaceManifest, removed: Bool) {
        var manifest = self
        var removed = false
        for spaceIndex in manifest.spaces.indices {
            for tabIndex in manifest.spaces[spaceIndex].tabs.indices {
                if let pinSnapshot = manifest.spaces[spaceIndex].tabs[tabIndex].pinSnapshot {
                    let result = pinSnapshot.clearingRestoredTranscriptSnapshot(
                        forTerminalContentID: contentID
                    )
                    manifest.spaces[spaceIndex].tabs[tabIndex].pinSnapshot = result.snapshot
                    removed = removed || result.removed
                }
                if let liveSnapshot = manifest.spaces[spaceIndex].tabs[tabIndex].liveSnapshot {
                    let result = liveSnapshot.clearingRestoredTranscriptSnapshot(
                        forTerminalContentID: contentID
                    )
                    manifest.spaces[spaceIndex].tabs[tabIndex].liveSnapshot = result.snapshot
                    removed = removed || result.removed
                }
            }
        }
        return (manifest, removed)
    }
}

struct ShellContentWorkspaceSpaceRecord: Codable, Equatable, Identifiable {
    var spaceID: String
    var title: String
    var order: Int
    var createdAt: Date
    var updatedAt: Date
    var selectedTabID: String? = nil
    var tabs: [ShellContentWorkspaceTabRecord]
    var terminalProfileID: String? = nil
    var presentationIconSystemName: String? = nil

    var id: String { spaceID }

    private enum CodingKeys: String, CodingKey {
        case spaceID = "space_id"
        case title
        case order
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case selectedTabID = "selected_tab_id"
        case tabs
        case terminalProfileID = "terminal_profile_id"
        case presentationIconSystemName = "presentation_icon"
    }

    init(
        spaceID: String,
        title: String,
        order: Int,
        createdAt: Date,
        updatedAt: Date,
        selectedTabID: String? = nil,
        tabs: [ShellContentWorkspaceTabRecord],
        terminalProfileID: String? = nil,
        presentationIconSystemName: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.order = order
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.selectedTabID = selectedTabID
        self.tabs = tabs
        self.terminalProfileID = terminalProfileID
        self.presentationIconSystemName = presentationIconSystemName
    }
}

struct ShellContentWorkspaceTabRecord: Codable, Equatable, Identifiable {
    var tabID: String
    var title: String?
    var kind: ShellTabKind
    var createdAt: Date
    var lastActivatedAt: Date
    var lastActivityAt: Date
    var isPinned: Bool
    var isTitleUserLocked: Bool?
    var pinSnapshot: ShellContentTabRestoreSnapshot?
    var liveSnapshot: ShellContentTabRestoreSnapshot?
    var activeTask: ShellTabActiveTaskState

    var id: String { tabID }

    private enum CodingKeys: String, CodingKey {
        case tabID = "tab_id"
        case title
        case kind
        case createdAt = "created_at"
        case lastActivatedAt = "last_activated_at"
        case lastActivityAt = "last_activity_at"
        case isPinned = "is_pinned"
        case isTitleUserLocked = "is_title_user_locked"
        case pinSnapshot = "pin_snapshot"
        case liveSnapshot = "live_snapshot"
        case activeTask = "active_task"
    }

}

struct ShellContentTabRestoreSnapshot: Codable, Equatable {
    var paneTree: ShellPaneSlotTreeNode
    var paneSlots: [ShellPaneSlotRestoreRecord]
    var contents: [ShellContentRestoreRecord]

    private enum CodingKeys: String, CodingKey {
        case paneTree = "pane_tree"
        case paneSlots = "pane_slots"
        case contents
    }
}

extension ShellContentTabRestoreSnapshot {
    func overlayingTerminalTranscriptSnapshots(
        from liveSnapshot: ShellContentTabRestoreSnapshot?
    ) -> ShellContentTabRestoreSnapshot {
        guard let liveSnapshot else { return self }
        let liveTranscriptsByContentID = Dictionary(
            uniqueKeysWithValues: liveSnapshot.contents.compactMap { content in
                content.payload.terminal?.transcriptSnapshot.map { (content.contentID, $0) }
            }
        )
        guard !liveTranscriptsByContentID.isEmpty else { return self }

        return overlayingTerminalTranscriptSnapshots(liveTranscriptsByContentID)
    }

    func overlayingTerminalTranscriptSnapshots(
        _ transcriptsByContentID: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentTabRestoreSnapshot {
        guard !transcriptsByContentID.isEmpty else { return self }

        var restored = self
        restored.contents = contents.map { content in
            guard let terminalPayload = content.payload.terminal,
                  let transcriptSnapshot = transcriptsByContentID[content.contentID]
            else {
                return content
            }
            return ShellContentRestoreRecord(
                contentID: content.contentID,
                kind: content.kind,
                title: content.title,
                payload: .terminal(
                    ShellTerminalContentPayload(
                        launchTarget: terminalPayload.launchTarget,
                        cwd: terminalPayload.cwd,
                        title: terminalPayload.title,
                        transcriptSnapshot: transcriptSnapshot,
                        terminalProfileID: terminalPayload.terminalProfileID
                    )
                )
            )
        }
        return restored
    }

    func clearingRestoredTranscriptSnapshot(
        forTerminalContentID contentID: String
    ) -> (snapshot: ShellContentTabRestoreSnapshot, removed: Bool) {
        var removed = false
        var snapshot = self
        snapshot.contents = contents.map { content in
            guard content.contentID == contentID,
                  let terminalPayload = content.payload.terminal,
                  terminalPayload.transcriptSnapshot != nil
            else {
                return content
            }

            removed = true
            return ShellContentRestoreRecord(
                contentID: content.contentID,
                kind: content.kind,
                title: content.title,
                payload: .terminal(terminalPayload.clearingRestoredTranscriptSnapshot())
            )
        }
        return (snapshot, removed)
    }
}

struct ShellPaneSlotRestoreRecord: Codable, Equatable, Identifiable {
    var paneSlotID: String
    var contentID: String

    var id: String { paneSlotID }

    private enum CodingKeys: String, CodingKey {
        case paneSlotID = "pane_slot_id"
        case contentID = "content_id"
    }
}

struct ShellContentRestoreRecord: Codable, Equatable, Identifiable {
    var contentID: String
    var kind: ShellContentKind
    var title: String
    var payload: ShellContentPayload

    var id: String { contentID }

    private enum CodingKeys: String, CodingKey {
        case contentID = "content_id"
        case kind
        case title
        case payload
    }
}

extension ShellContentTabRestoreSnapshot {
    static func projecting(
        tab: ShellTab,
        contentState: ShellContentStateSnapshot
    ) -> ShellContentTabRestoreSnapshot {
        let paneSlotIDs = tab.paneTree.paneIDs
        let paneSlots = paneSlotIDs.compactMap { paneSlotID in
            contentState.paneSlot(paneSlotID: paneSlotID)
        }
        let mountedContentIDs = Set(paneSlots.map(\.contentID))
        let contents = contentState.contents.filter { mountedContentIDs.contains($0.contentID) }

        return ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode.migrating(paneTree: tab.paneTree),
            paneSlots: paneSlots.map { paneSlot in
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlot.paneSlotID,
                    contentID: paneSlot.contentID
                )
            },
            contents: contents.map { content in
                ShellContentRestoreRecord(
                    contentID: content.contentID,
                    kind: content.kind,
                    title: content.title,
                    payload: content.payload
                )
            }
        )
    }
}
