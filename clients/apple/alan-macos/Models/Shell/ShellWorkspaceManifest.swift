import Foundation

struct ShellWorkspaceManifest: Codable, Equatable {
    static let currentSchemaVersion = 1

    var schemaVersion: Int
    var windowID: String
    var selectedSpaceID: String?
    var selectedTabID: String?
    var spaces: [ShellWorkspaceSpaceRecord]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case windowID = "window_id"
        case selectedSpaceID = "selected_space_id"
        case selectedTabID = "selected_tab_id"
        case spaces
    }

    static func defaultManifest(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellWorkspaceManifest {
        let spaceID = "space_main"
        let tabID = "tab_main"
        let paneID = "pane_1"
        let snapshot = ShellTabRestoreSnapshot(
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            ),
            panes: [
                ShellPaneRestoreRecord(
                    paneID: paneID,
                    launchTarget: .shell,
                    cwd: defaultWorkingDirectory,
                    title: "Shell"
                )
            ]
        )

        return ShellWorkspaceManifest(
            schemaVersion: currentSchemaVersion,
            windowID: windowID,
            selectedSpaceID: spaceID,
            selectedTabID: tabID,
            spaces: [
                ShellWorkspaceSpaceRecord(
                    spaceID: spaceID,
                    title: "Terminal",
                    order: 0,
                    createdAt: now,
                    updatedAt: now,
                    selectedTabID: tabID,
                    tabs: [
                        ShellWorkspaceTabRecord(
                            tabID: tabID,
                            title: "Shell",
                            kind: .terminal,
                            createdAt: now,
                            lastActivatedAt: now,
                            lastActivityAt: now,
                            isPinned: false,
                            pinSnapshot: nil,
                            liveSnapshot: snapshot,
                            activeTask: .inactive
                        )
                    ]
                )
            ]
        )
    }
}

struct ShellWorkspaceSpaceRecord: Codable, Equatable, Identifiable {
    var spaceID: String
    var title: String
    var order: Int
    var createdAt: Date
    var updatedAt: Date
    var selectedTabID: String? = nil
    var tabs: [ShellWorkspaceTabRecord]
    var terminalProfileID: String? = nil

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
    }

    init(
        spaceID: String,
        title: String,
        order: Int,
        createdAt: Date,
        updatedAt: Date,
        selectedTabID: String? = nil,
        tabs: [ShellWorkspaceTabRecord],
        terminalProfileID: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.order = order
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.selectedTabID = selectedTabID
        self.tabs = tabs
        self.terminalProfileID = terminalProfileID
    }
}

struct ShellWorkspaceTabRecord: Codable, Equatable, Identifiable {
    var tabID: String
    var title: String?
    var kind: ShellTabKind
    var createdAt: Date
    var lastActivatedAt: Date
    var lastActivityAt: Date
    var isPinned: Bool
    var pinSnapshot: ShellTabRestoreSnapshot?
    var liveSnapshot: ShellTabRestoreSnapshot?
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
        case pinSnapshot = "pin_snapshot"
        case liveSnapshot = "live_snapshot"
        case activeTask = "active_task"
    }

    func restoreSnapshot(defaultWorkingDirectory: String) -> ShellTabRestoreSnapshot {
        if isPinned, let pinSnapshot {
            return pinSnapshot
        }

        if let liveSnapshot {
            return liveSnapshot
        }

        let paneID = "pane_\(tabID)"
        return ShellTabRestoreSnapshot(
            paneTree: ShellPaneTreeNode(
                nodeID: "node_\(paneID)",
                kind: .pane,
                direction: nil,
                paneID: paneID,
                children: nil
            ),
            panes: [
                ShellPaneRestoreRecord(
                    paneID: paneID,
                    launchTarget: .shell,
                    cwd: defaultWorkingDirectory,
                    title: title
                )
            ]
        )
    }

    func shouldRetain(now: Date, ttl: TimeInterval) -> Bool {
        if isPinned {
            return true
        }

        if activeTask.protectsFromPruning {
            return true
        }

        return now.timeIntervalSince(max(lastActivatedAt, lastActivityAt)) <= ttl
    }
}

struct ShellTabRestoreSnapshot: Codable, Equatable {
    var paneTree: ShellPaneTreeNode
    var panes: [ShellPaneRestoreRecord]

    private enum CodingKeys: String, CodingKey {
        case paneTree = "pane_tree"
        case panes
    }
}

struct ShellPaneRestoreRecord: Codable, Equatable, Identifiable {
    var paneID: String
    var launchTarget: ShellLaunchTarget
    var cwd: String?
    var title: String?
    var terminalProfileID: String? = nil

    var id: String { paneID }

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case launchTarget = "launch_target"
        case cwd
        case title
        case terminalProfileID = "terminal_profile_id"
    }
}

struct ShellContentWorkspaceManifest: Codable, Equatable {
    static let currentContentContractVersion = ShellContentStateSnapshot.currentContractVersion

    var schemaVersion: Int
    var contentContractVersion: String
    var windowID: String
    var selectedSpaceID: String?
    var selectedTabID: String?
    var spaces: [ShellContentWorkspaceSpaceRecord]
    var quickTerminal: ShellQuickTerminalRestoreRecord? = nil

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case contentContractVersion = "content_contract_version"
        case windowID = "window_id"
        case selectedSpaceID = "selected_space_id"
        case selectedTabID = "selected_tab_id"
        case spaces
        case quickTerminal = "quick_terminal"
    }
}

extension ShellContentWorkspaceManifest {
    static func defaultManifest(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellContentWorkspaceManifest {
        ShellWorkspaceManifest.defaultManifest(
            windowID: windowID,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
        .migratingTerminalRestoreSnapshotsToContentContainers()
    }

    func pruningExpiredTabs(now: Date, ttl: TimeInterval) -> ShellContentWorkspaceManifest {
        var pruned = self
        pruned.spaces = spaces.map { space in
            var space = space
            space.tabs = space.tabs.filter { $0.shouldRetain(now: now, ttl: ttl) }
            space.updatedAt = now
            return space
        }
        pruned.repairSelection()
        return pruned
    }

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
    }

    init(
        spaceID: String,
        title: String,
        order: Int,
        createdAt: Date,
        updatedAt: Date,
        selectedTabID: String? = nil,
        tabs: [ShellContentWorkspaceTabRecord],
        terminalProfileID: String? = nil
    ) {
        self.spaceID = spaceID
        self.title = title
        self.order = order
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.selectedTabID = selectedTabID
        self.tabs = tabs
        self.terminalProfileID = terminalProfileID
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
        case pinSnapshot = "pin_snapshot"
        case liveSnapshot = "live_snapshot"
        case activeTask = "active_task"
    }

    func restoreSnapshot(defaultWorkingDirectory: String) -> ShellContentTabRestoreSnapshot {
        if isPinned, let pinSnapshot {
            return pinSnapshot.overlayingTerminalTranscriptSnapshots(from: liveSnapshot)
        }

        if let liveSnapshot {
            return liveSnapshot
        }

        let paneSlotID = "pane_\(tabID)"
        let contentID = ShellContentInstance.terminalContentID(forPaneID: paneSlotID)
        let title = title ?? ShellWorkspaceMaterializer.defaultViewportTitleForMigration(
            launchTarget: .shell
        )
        return ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode(
                nodeID: "node_\(paneSlotID)",
                kind: .pane,
                direction: nil,
                paneSlotID: paneSlotID,
                children: nil
            ),
            paneSlots: [
                ShellPaneSlotRestoreRecord(
                    paneSlotID: paneSlotID,
                    contentID: contentID
                )
            ],
            contents: [
                ShellContentRestoreRecord(
                    contentID: contentID,
                    kind: .terminal,
                    title: title,
                    payload: .terminal(
                        ShellTerminalContentPayload(
                            launchTarget: .shell,
                            cwd: defaultWorkingDirectory,
                            title: title
                        )
                    )
                )
            ]
        )
    }

    func shouldRetain(now: Date, ttl: TimeInterval) -> Bool {
        if isPinned {
            return true
        }

        if activeTask.protectsFromPruning {
            return true
        }

        return now.timeIntervalSince(max(lastActivatedAt, lastActivityAt)) <= ttl
    }
}

struct ShellQuickTerminalRestoreRecord: Codable, Equatable {
    var paneID: String
    var presentation: ShellQuickTerminalPresentation
    var lastWorkingDirectory: String?
    var liveSnapshot: ShellContentTabRestoreSnapshot?
    var activeTask: ShellTabActiveTaskState

    private enum CodingKeys: String, CodingKey {
        case paneID = "pane_id"
        case presentation
        case lastWorkingDirectory = "last_working_directory"
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

extension ShellWorkspaceManifest {
    func migratingTerminalRestoreSnapshotsToContentContainers() -> ShellContentWorkspaceManifest {
        ShellContentWorkspaceManifest(
            schemaVersion: schemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: windowID,
            selectedSpaceID: selectedSpaceID,
            selectedTabID: selectedTabID,
            spaces: spaces.map { space in
                ShellContentWorkspaceSpaceRecord(
                    spaceID: space.spaceID,
                    title: space.title,
                    order: space.order,
                    createdAt: space.createdAt,
                    updatedAt: space.updatedAt,
                    selectedTabID: space.selectedTabID,
                    tabs: space.tabs.map { tab in
                        ShellContentWorkspaceTabRecord(
                            tabID: tab.tabID,
                            title: tab.title,
                            kind: tab.kind,
                            createdAt: tab.createdAt,
                            lastActivatedAt: tab.lastActivatedAt,
                            lastActivityAt: tab.lastActivityAt,
                            isPinned: tab.isPinned,
                            pinSnapshot: tab.pinSnapshot?.migratingTerminalPanesToContentContainers(),
                            liveSnapshot: tab.liveSnapshot?.migratingTerminalPanesToContentContainers(),
                            activeTask: tab.activeTask
                        )
                    },
                    terminalProfileID: space.terminalProfileID
                )
            }
        )
    }

    func pruningExpiredTabs(now: Date, ttl: TimeInterval) -> ShellWorkspaceManifest {
        var pruned = self
        pruned.spaces = spaces.map { space in
            var space = space
            space.tabs = space.tabs.filter { $0.shouldRetain(now: now, ttl: ttl) }
            space.updatedAt = now
            return space
        }
        pruned.repairSelection()
        return pruned
    }

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
}

extension ShellTabRestoreSnapshot {
    func migratingTerminalPanesToContentContainers() -> ShellContentTabRestoreSnapshot {
        let paneSlots = panes.map { pane in
            ShellPaneSlotRestoreRecord(
                paneSlotID: pane.paneID,
                contentID: Self.contentID(forPaneID: pane.paneID)
            )
        }
        let contents = panes.map { pane in
            let title = pane.title ?? ShellWorkspaceMaterializer.defaultViewportTitleForMigration(
                launchTarget: pane.launchTarget
            )
            return ShellContentRestoreRecord(
                contentID: Self.contentID(forPaneID: pane.paneID),
                kind: .terminal,
                title: title,
                payload: .terminal(
                    ShellTerminalContentPayload(
                        launchTarget: pane.launchTarget,
                        cwd: pane.cwd,
                        title: title,
                        terminalProfileID: pane.terminalProfileID
                    )
                )
            )
        }

        return ShellContentTabRestoreSnapshot(
            paneTree: ShellPaneSlotTreeNode.migrating(paneTree: paneTree),
            paneSlots: paneSlots,
            contents: contents
        )
    }

    private static func contentID(forPaneID paneID: String) -> String {
        "content_\(paneID)"
    }
}

struct ShellWorkspaceMaterializer {
    static func materialize(
        manifest: ShellContentWorkspaceManifest,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellStateSnapshot {
        var repairedManifest = manifest
        repairedManifest.repairSelection()

        let sourceManifest = repairedManifest.spaces.isEmpty
            ? ShellContentWorkspaceManifest.defaultManifest(
                windowID: manifest.windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
            : repairedManifest

        if let state = materializeContentManifest(
            sourceManifest,
            defaultWorkingDirectory: defaultWorkingDirectory
        ) {
            return state
        }

        let fallbackManifest = ShellContentWorkspaceManifest.defaultManifest(
            windowID: manifest.windowID,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
        return materializeContentManifest(
            fallbackManifest,
            defaultWorkingDirectory: defaultWorkingDirectory
        ) ?? ShellStateSnapshot.bootstrapDefault(
            windowID: manifest.windowID,
            workingDirectory: defaultWorkingDirectory
        )
    }

    private static func materializeContentManifest(
        _ sourceManifest: ShellContentWorkspaceManifest,
        defaultWorkingDirectory: String
    ) -> ShellStateSnapshot? {
        let spaces = sourceManifest.spaces.sorted { lhs, rhs in
            if lhs.order == rhs.order {
                return lhs.spaceID < rhs.spaceID
            }
            return lhs.order < rhs.order
        }

        var paneSlots: [ShellPaneSlot] = []
        var contents: [ShellContentInstance] = []
        let contentSpaces = spaces.map { space -> ShellContentSpace in
            let contentTabs = organizedTabs(space.tabs).compactMap { tabRecord -> ShellContentTab? in
                let restoreSnapshot = tabRecord.restoreSnapshot(
                    defaultWorkingDirectory: defaultWorkingDirectory
                )
                let validContentIDs = Set(restoreSnapshot.contents.map(\.contentID))
                let snapshotPaneSlots = restoreSnapshot.paneSlots.compactMap { paneSlot -> ShellPaneSlot? in
                    guard validContentIDs.contains(paneSlot.contentID) else { return nil }
                    return ShellPaneSlot(
                        paneSlotID: paneSlot.paneSlotID,
                        tabID: tabRecord.tabID,
                        spaceID: space.spaceID,
                        contentID: paneSlot.contentID,
                        attention: tabRecord.tabID == sourceManifest.selectedTabID ? .active : .idle
                    )
                }
                guard !snapshotPaneSlots.isEmpty else { return nil }

                paneSlots.append(contentsOf: snapshotPaneSlots)
                contents.append(
                    contentsOf: restoreSnapshot.contents.map { content in
                        restoredContentInstance(
                            content,
                            defaultWorkingDirectory: defaultWorkingDirectory
                        )
                    }
                )

                return ShellContentTab(
                    tabID: tabRecord.tabID,
                    kind: tabRecord.kind,
                    title: tabRecord.title,
                    paneTree: restoreSnapshot.paneTree,
                    isPinned: tabRecord.isPinned
                )
            }

            return ShellContentSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(
                    in: paneSlots.filter { $0.spaceID == space.spaceID }
                ),
                tabs: contentTabs,
                selectedTabID: space.selectedTabID,
                terminalProfileID: space.terminalProfileID
            )
        }

        let contentState = ShellContentStateSnapshot(
            contractVersion: ShellContentStateSnapshot.currentContractVersion,
            windowID: sourceManifest.windowID,
            focusedSpaceID: sourceManifest.selectedSpaceID,
            focusedTabID: sourceManifest.selectedTabID,
            focusedPaneSlotID: sourceManifest.selectedTabID
                .flatMap { selectedTabID in
                    contentSpaces
                        .flatMap(\.tabs)
                        .first { $0.tabID == selectedTabID }?
                        .paneTree
                        .paneSlotIDs
                        .first
                },
            spaces: contentSpaces,
            paneSlots: paneSlots,
            contents: contents
        )

        guard var state = contentState.materializingShellState() else {
            return nil
        }
        if let quickTerminal = sourceManifest.quickTerminal,
           let restoredQuickTerminal = materializeQuickTerminal(
            quickTerminal,
            defaultWorkingDirectory: defaultWorkingDirectory
           ),
           !state.panes.contains(where: { $0.paneID == restoredQuickTerminal.pane.paneID })
        {
            var contents = state.contents ?? []
            if !contents.contains(where: { $0.contentID == restoredQuickTerminal.content.contentID }) {
                contents.append(restoredQuickTerminal.content)
            }
            state = ShellStateSnapshot(
                contractVersion: state.contractVersion,
                windowID: state.windowID,
                focusedSpaceID: state.focusedSpaceID,
                focusedTabID: state.focusedTabID,
                focusedPaneID: state.focusedPaneID,
                spaces: state.spaces,
                panes: state.panes + [restoredQuickTerminal.pane],
                paneSlots: state.paneSlots,
                contents: contents.isEmpty ? nil : contents,
                quickTerminal: restoredQuickTerminal.slot
            )
        }
        return state
    }

    private static func materializeQuickTerminal(
        _ record: ShellQuickTerminalRestoreRecord,
        defaultWorkingDirectory: String
    ) -> (
        slot: ShellQuickTerminalSlot,
        pane: ShellPane,
        content: ShellContentInstance
    )? {
        guard let snapshot = record.liveSnapshot,
              let paneSlotRecord = snapshot.paneSlots.first(where: { $0.paneSlotID == record.paneID })
                ?? snapshot.paneSlots.first,
              let contentRecord = snapshot.contents.first(where: {
                $0.contentID == paneSlotRecord.contentID
              }),
              contentRecord.kind == .terminal
        else {
            return nil
        }

        let content = restoredContentInstance(contentRecord, defaultWorkingDirectory: defaultWorkingDirectory)
        guard let terminalPayload = content.payload.terminal else {
            return nil
        }
        let lastWorkingDirectory = record.lastWorkingDirectory ?? terminalPayload.cwd
        let pane = ShellPane(
            paneID: record.paneID,
            tabID: ShellQuickTerminalSlot.globalTabID,
            spaceID: ShellQuickTerminalSlot.globalSpaceID,
            launchTarget: terminalPayload.launchTarget,
            cwd: terminalPayload.cwd,
            process: nil,
            attention: record.activeTask.protectsFromPruning ? .active : .idle,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: content.title,
                summary: nil,
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            alanBinding: nil,
            terminalProfileID: terminalPayload.terminalProfileID
        )
        let slot = ShellQuickTerminalSlot(
            paneID: record.paneID,
            // The detached Peak panel is transient UI; restore its runtime and content
            // without presenting the panel during app launch.
            presentation: .hidden,
            lastWorkingDirectory: lastWorkingDirectory
        )
        return (slot, pane, content)
    }

    private static func restoredContentInstance(
        _ record: ShellContentRestoreRecord,
        defaultWorkingDirectory: String
    ) -> ShellContentInstance {
        let payload: ShellContentPayload
        if record.kind == .terminal,
           let terminalPayload = record.payload.terminal
        {
            payload = .terminal(
                ShellTerminalContentPayload(
                    launchTarget: terminalPayload.launchTarget,
                    cwd: restoredWorkingDirectory(
                        terminalPayload.cwd,
                        terminalProfileID: terminalPayload.terminalProfileID,
                        defaultWorkingDirectory: defaultWorkingDirectory
                    ),
                    title: terminalPayload.title,
                    transcriptSnapshot: terminalPayload.transcriptSnapshot,
                    terminalProfileID: terminalPayload.terminalProfileID
                )
            )
        } else {
            payload = record.payload
        }

        return ShellContentInstance(
            contentID: record.contentID,
            kind: record.kind,
            title: record.title,
            payload: payload
        )
    }

    static func materialize(
        manifest: ShellWorkspaceManifest,
        defaultWorkingDirectory: String,
        now: Date
    ) -> ShellStateSnapshot {
        var repairedManifest = manifest
        repairedManifest.repairSelection()

        let sourceManifest = repairedManifest.spaces.isEmpty
            ? ShellWorkspaceManifest.defaultManifest(
                windowID: manifest.windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
            : repairedManifest

        let spaces = sourceManifest.spaces.sorted { lhs, rhs in
            if lhs.order == rhs.order {
                return lhs.spaceID < rhs.spaceID
            }
            return lhs.order < rhs.order
        }

        var shellSpaces: [ShellSpace] = []
        var panes: [ShellPane] = []

        for space in spaces {
            let shellTabs = organizedTabs(space.tabs).map { tabRecord -> ShellTab in
                let restoreSnapshot = tabRecord.restoreSnapshot(
                    defaultWorkingDirectory: defaultWorkingDirectory
                )

                panes.append(
                    contentsOf: restoreSnapshot.panes.map { paneRecord in
                        makePane(
                            record: paneRecord,
                            tabID: tabRecord.tabID,
                            spaceID: space.spaceID,
                            selectedTabID: sourceManifest.selectedTabID,
                            defaultWorkingDirectory: defaultWorkingDirectory
                        )
                    }
                )

                return ShellTab(
                    tabID: tabRecord.tabID,
                    kind: tabRecord.kind,
                    title: tabRecord.title,
                    paneTree: restoreSnapshot.paneTree,
                    isPinned: tabRecord.isPinned
                )
            }

            shellSpaces.append(
                ShellSpace(
                    spaceID: space.spaceID,
                    title: space.title,
                    attention: strongestAttention(for: shellTabs, panes: panes),
                    tabs: shellTabs,
                    selectedTabID: space.selectedTabID,
                    terminalProfileID: space.terminalProfileID
                )
            )
        }

        let focusedSpaceID = sourceManifest.selectedSpaceID
        let focusedTabID = focusedSpaceID.flatMap { spaceID in
            let selectedSpace = shellSpaces.first { $0.spaceID == spaceID }
            if let selectedTabID = sourceManifest.selectedTabID,
               selectedSpace?.tabs.contains(where: { $0.tabID == selectedTabID }) == true
            {
                return selectedTabID
            }
            return selectedSpace?.tabs.first?.tabID
        }
        let focusedPaneID = focusedTabID
            .flatMap { tabID in shellSpaces.lazy.flatMap(\.tabs).first { $0.tabID == tabID } }
            .flatMap { $0.paneTree.paneIDs.first }

        return ShellStateSnapshot(
            contractVersion: "0.1",
            windowID: sourceManifest.windowID,
            focusedSpaceID: focusedSpaceID,
            focusedTabID: focusedTabID,
            focusedPaneID: focusedPaneID,
            spaces: shellSpaces,
            panes: panes
        )
    }

    private static func makePane(
        record: ShellPaneRestoreRecord,
        tabID: String,
        spaceID: String,
        selectedTabID: String?,
        defaultWorkingDirectory: String
    ) -> ShellPane {
        let launchTarget = record.launchTarget
        let title = record.title ?? defaultViewportTitle(for: launchTarget)
        return ShellPane(
            paneID: record.paneID,
            tabID: tabID,
            spaceID: spaceID,
            launchTarget: launchTarget,
            cwd: restoredWorkingDirectory(
                record.cwd,
                terminalProfileID: record.terminalProfileID,
                defaultWorkingDirectory: defaultWorkingDirectory
            ),
            process: nil,
            attention: tabID == selectedTabID ? .active : .idle,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: title,
                summary: nil,
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            alanBinding: nil,
            terminalProfileID: record.terminalProfileID
        )
    }

    private static func organizedTabs(
        _ tabs: [ShellWorkspaceTabRecord]
    ) -> [ShellWorkspaceTabRecord] {
        tabs.filter(\.isPinned) + tabs.filter { !$0.isPinned }
    }

    private static func restoredWorkingDirectory(
        _ workingDirectory: String?,
        terminalProfileID: String?,
        defaultWorkingDirectory: String
    ) -> String? {
        if terminalProfileID != nil {
            return workingDirectory
        }
        return workingDirectory ?? defaultWorkingDirectory
    }

    private static func organizedTabs(
        _ tabs: [ShellContentWorkspaceTabRecord]
    ) -> [ShellContentWorkspaceTabRecord] {
        tabs.filter(\.isPinned) + tabs.filter { !$0.isPinned }
    }

    private static func strongestAttention(
        for tabs: [ShellTab],
        panes: [ShellPane]
    ) -> ShellAttentionState {
        let paneIDs = Set(tabs.flatMap(\.paneTree.paneIDs))
        return panes
            .filter { paneIDs.contains($0.paneID) }
            .map(\.attention)
            .max { attentionRank(for: $0) < attentionRank(for: $1) }
            ?? .idle
    }

    private static func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .idle:
            return 0
        case .active:
            return 1
        case .notable:
            return 2
        case .awaitingUser:
            return 3
        }
    }

    private static func strongestAttention(in paneSlots: [ShellPaneSlot]) -> ShellAttentionState {
        paneSlots
            .map(\.attention)
            .max { attentionRank(for: $0) < attentionRank(for: $1) }
            ?? .idle
    }

    private static func defaultViewportTitle(for launchTarget: ShellLaunchTarget) -> String {
        switch launchTarget {
        case .shell:
            return "Shell"
        }
    }

    static func defaultViewportTitleForMigration(launchTarget: ShellLaunchTarget) -> String {
        defaultViewportTitle(for: launchTarget)
    }
}
