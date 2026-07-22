import Foundation

struct ShellStateSnapshot: Codable, Equatable {
    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneID: String?
    let spaces: [ShellSpace]
    let panes: [ShellPane]
    var paneSlots: [ShellPaneSlot]? = nil
    var contents: [ShellContentInstance]? = nil
    /// Portable pane-zoom state. Omitted from persisted/control snapshot encoding while the
    /// renderer projection migrates away from its duplicate published map.
    var zoomedPaneIDByTabID: [String: String] = [:]

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneID = "focused_pane_id"
        case spaces
        case panes
        case paneSlots = "pane_slots"
        case contents
    }

    var prettyPrintedJSON: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{\n  \"error\": \"failed to encode shell snapshot\"\n}"
        }

        return string
    }
}

struct ShellContentStateSnapshot: Codable, Equatable {
    static let currentContractVersion = "0.2"

    let contractVersion: String
    let windowID: String
    let focusedSpaceID: String?
    let focusedTabID: String?
    let focusedPaneSlotID: String?
    let spaces: [ShellContentSpace]
    let paneSlots: [ShellPaneSlot]
    let contents: [ShellContentInstance]

    private enum CodingKeys: String, CodingKey {
        case contractVersion = "contract_version"
        case windowID = "window_id"
        case focusedSpaceID = "focused_space_id"
        case focusedTabID = "focused_tab_id"
        case focusedPaneSlotID = "focused_pane_slot_id"
        case spaces
        case paneSlots = "pane_slots"
        case contents
    }

    var prettyPrintedJSON: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        guard let data = try? encoder.encode(self),
              let string = String(data: data, encoding: .utf8)
        else {
            return "{\n  \"error\": \"failed to encode shell content snapshot\"\n}"
        }

        return string
    }
}

extension ShellTab {
    func contains(paneID: String) -> Bool {
        paneTree.contains(paneID: paneID)
    }

    var organizationSection: ShellTabOrganizationSection {
        isPinned ? .pinned : .unpinned
    }
}

extension ShellSpace {
    var resolvedSelectedTabID: String? {
        if let selectedTabID,
           tabs.contains(where: { $0.tabID == selectedTabID })
        {
            return selectedTabID
        }
        return tabs.first?.tabID
    }

    func repairingSelectedTabID(preferredTabID: String? = nil) -> ShellSpace {
        let resolvedPreferred = preferredTabID.flatMap { candidate in
            tabs.contains(where: { $0.tabID == candidate }) ? candidate : nil
        }
        let resolvedExisting = selectedTabID.flatMap { candidate in
            tabs.contains(where: { $0.tabID == candidate }) ? candidate : nil
        }
        return ShellSpace(
            spaceID: spaceID,
            title: title,
            attention: attention,
            tabs: tabs,
            selectedTabID: resolvedPreferred ?? resolvedExisting ?? tabs.first?.tabID,
            terminalProfileID: terminalProfileID,
            presentationIconSystemName: presentationIconSystemName
        )
    }

    var pinnedTabs: [ShellTab] {
        tabs.filter(\.isPinned)
    }

    var unpinnedTabs: [ShellTab] {
        tabs.filter { !$0.isPinned }
    }

    func tabs(in section: ShellTabOrganizationSection) -> [ShellTab] {
        switch section {
        case .pinned:
            return pinnedTabs
        case .unpinned:
            return unpinnedTabs
        }
    }
}

extension ShellContentSpace {
    var resolvedSelectedTabID: String? {
        if let selectedTabID,
           tabs.contains(where: { $0.tabID == selectedTabID })
        {
            return selectedTabID
        }
        return tabs.first?.tabID
    }

    func repairingSelectedTabID(preferredTabID: String? = nil) -> ShellContentSpace {
        let resolvedPreferred = preferredTabID.flatMap { candidate in
            tabs.contains(where: { $0.tabID == candidate }) ? candidate : nil
        }
        let resolvedExisting = selectedTabID.flatMap { candidate in
            tabs.contains(where: { $0.tabID == candidate }) ? candidate : nil
        }
        return ShellContentSpace(
            spaceID: spaceID,
            title: title,
            attention: attention,
            tabs: tabs,
            selectedTabID: resolvedPreferred ?? resolvedExisting ?? tabs.first?.tabID,
            terminalProfileID: terminalProfileID
        )
    }
}

extension ShellStateSnapshot {
    var totalTabCount: Int {
        spaces.reduce(into: 0) { partialResult, space in
            partialResult += space.tabs.count
        }
    }

    func space(spaceID: String) -> ShellSpace? {
        spaces.first { $0.spaceID == spaceID }
    }

    func tab(tabID: String) -> ShellTab? {
        spaces.lazy.flatMap(\.tabs).first { $0.tabID == tabID }
    }

    func pane(paneID: String) -> ShellPane? {
        panes.first { $0.paneID == paneID }
    }

    func explicitContentMounted(in paneID: String) -> ShellContentInstance? {
        guard let paneSlot = paneSlots?.first(where: { $0.paneSlotID == paneID }) else {
            return nil
        }
        return contents?.first { $0.contentID == paneSlot.contentID }
    }

    func isTerminalBackedPane(_ pane: ShellPane) -> Bool {
        if let mountedContent = explicitContentMounted(in: pane.paneID) {
            return mountedContent.kind == .terminal
        }
        return pane.launchTarget != nil
    }

    func terminalBackedPane(paneID: String) -> ShellPane? {
        guard let pane = pane(paneID: paneID),
              isTerminalBackedPane(pane)
        else {
            return nil
        }
        return pane
    }

    func tabs(in spaceID: String?) -> [ShellTab] {
        guard let spaceID else {
            return spaces.flatMap(\.tabs)
        }
        return space(spaceID: spaceID)?.tabs ?? []
    }

    func panes(in tabID: String?) -> [ShellPane] {
        guard let tabID else {
            return panes
        }
        return panes.filter { $0.tabID == tabID }
    }

    func tabOrganizationLocation(tabID: String) -> ShellTabOrganizationLocation? {
        for space in spaces {
            if let pinnedIndex = space.pinnedTabs.firstIndex(where: { $0.tabID == tabID }) {
                return ShellTabOrganizationLocation(
                    spaceID: space.spaceID,
                    section: .pinned,
                    index: pinnedIndex
                )
            }
            if let unpinnedIndex = space.unpinnedTabs.firstIndex(where: { $0.tabID == tabID }) {
                return ShellTabOrganizationLocation(
                    spaceID: space.spaceID,
                    section: .unpinned,
                    index: unpinnedIndex
                )
            }
        }
        return nil
    }

    func contentStateProjection() -> ShellContentStateSnapshot {
        ShellContentStateSnapshot.projecting(self)
    }

    func clearingRestoredTranscriptSnapshot(
        forTerminalContentID contentID: String
    ) -> (state: ShellStateSnapshot, removed: Bool) {
        guard let contents else { return (self, false) }

        var removed = false
        let nextContents = contents.map { content -> ShellContentInstance in
            guard content.contentID == contentID else { return content }
            let result = content.clearingRestoredTranscriptSnapshot()
            removed = removed || result.removed
            return result.content
        }

        guard removed else { return (self, false) }
        return (
            ShellStateSnapshot(
                contractVersion: contractVersion,
                windowID: windowID,
                focusedSpaceID: focusedSpaceID,
                focusedTabID: focusedTabID,
                focusedPaneID: focusedPaneID,
                spaces: spaces,
                panes: panes,
                paneSlots: paneSlots,
                contents: nextContents,
                zoomedPaneIDByTabID: zoomedPaneIDByTabID
            ),
            true
        )
    }
}

extension ShellContentStateSnapshot {
    static func projecting(_ shellState: ShellStateSnapshot) -> ShellContentStateSnapshot {
        let layoutPaneIDs = Set(shellState.spaces.flatMap(\.tabs).flatMap(\.paneTree.paneIDs))
        let projectedPanes = shellState.panes.filter { layoutPaneIDs.contains($0.paneID) }
        let paneSlotLocations = paneSlotLocations(in: shellState.spaces)
        let projectedPanesByID = projectedPanes.reduce(into: [String: ShellPane]()) { panesByID, pane in
            panesByID[pane.paneID] = pane
        }
        let explicitPaneSlots = (shellState.paneSlots ?? []).compactMap { paneSlot -> ShellPaneSlot? in
            guard layoutPaneIDs.contains(paneSlot.paneSlotID),
                  let location = paneSlotLocations[paneSlot.paneSlotID]
            else {
                return nil
            }

            return ShellPaneSlot(
                paneSlotID: paneSlot.paneSlotID,
                tabID: location.tabID,
                spaceID: location.spaceID,
                contentID: paneSlot.contentID,
                attention: projectedPanesByID[paneSlot.paneSlotID]?.attention ?? paneSlot.attention
            )
        }
        let explicitPaneSlotIDs = Set(explicitPaneSlots.map(\.paneSlotID))
        let explicitContentIDs = Set(explicitPaneSlots.map(\.contentID))
        let explicitPaneSlotsByContentID = explicitPaneSlots.reduce(into: [String: ShellPaneSlot]()) {
            slotsByContentID, slot in
            slotsByContentID[slot.contentID] = slot
        }
        let explicitContents = (shellState.contents ?? []).filter {
            explicitContentIDs.contains($0.contentID)
        }.map { content in
            guard content.kind == .terminal,
                  let paneSlot = explicitPaneSlotsByContentID[content.contentID],
                  let pane = projectedPanesByID[paneSlot.paneSlotID]
            else {
                return content
            }

            let projected = ShellContentInstance.projectingTerminalPane(
                pane,
                contentID: content.contentID
            )
            guard let transcriptSnapshot = content.payload.terminal?.transcriptSnapshot,
                  let terminalPayload = projected.payload.terminal
            else {
                return projected
            }
            return ShellContentInstance(
                contentID: projected.contentID,
                kind: projected.kind,
                title: projected.title,
                iconName: projected.iconName,
                capabilities: projected.capabilities,
                payload: .terminal(
                    ShellTerminalContentPayload(
                        launchTarget: terminalPayload.launchTarget,
                        cwd: terminalPayload.cwd,
                        title: terminalPayload.title,
                        transcriptSnapshot: transcriptSnapshot,
                        terminalProfileID: terminalPayload.terminalProfileID
                    )
                ),
                lifecycle: projected.lifecycle,
                rendererState: projected.rendererState
            )
        }
        let terminalPanes = projectedPanes.filter { !explicitPaneSlotIDs.contains($0.paneID) }
        let paneSlots = explicitPaneSlots + terminalPanes.map(ShellPaneSlot.projectingTerminalPane)
        let contents = explicitContents + terminalPanes.map(ShellContentInstance.projectingTerminalPane)
        let validPaneSlotIDs = Set(paneSlots.map(\.paneSlotID))
        let focusedPaneSlotID =
            shellState.focusedPaneID.flatMap { validPaneSlotIDs.contains($0) ? $0 : nil }
            ?? paneSlots.first?.paneSlotID
        let spaces = shellState.spaces.map { space in
            ShellContentSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: Self.strongestAttention(
                    in: paneSlots.filter { $0.spaceID == space.spaceID }
                ),
                tabs: space.tabs.map { tab in
                    ShellContentTab(
                        tabID: tab.tabID,
                        kind: tab.kind,
                        title: tab.title,
                        paneTree: ShellPaneSlotTreeNode.migrating(paneTree: tab.paneTree),
                        isPinned: tab.isPinned,
                        isTitleUserLocked: tab.isTitleUserLocked
                    )
                },
                selectedTabID: space.resolvedSelectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }

        return ShellContentStateSnapshot(
            contractVersion: currentContractVersion,
            windowID: shellState.windowID,
            focusedSpaceID: shellState.focusedSpaceID,
            focusedTabID: shellState.focusedTabID,
            focusedPaneSlotID: focusedPaneSlotID,
            spaces: spaces,
            paneSlots: paneSlots,
            contents: contents
        )
    }

    func space(spaceID: String) -> ShellContentSpace? {
        spaces.first { $0.spaceID == spaceID }
    }

    func tab(tabID: String) -> ShellContentTab? {
        spaces.lazy.flatMap(\.tabs).first { $0.tabID == tabID }
    }

    func paneSlot(paneSlotID: String) -> ShellPaneSlot? {
        paneSlots.first { $0.paneSlotID == paneSlotID }
    }

    func content(contentID: String) -> ShellContentInstance? {
        contents.first { $0.contentID == contentID }
    }

    var focusedPaneSlot: ShellPaneSlot? {
        focusedPaneSlotID.flatMap { paneSlot(paneSlotID: $0) }
    }

    var focusedContent: ShellContentInstance? {
        focusedPaneSlot.flatMap { content(contentID: $0.contentID) }
    }

    func contentMounted(in paneSlotID: String) -> ShellContentInstance? {
        paneSlot(paneSlotID: paneSlotID).flatMap { content(contentID: $0.contentID) }
    }

    func primaryContent(in tabID: String) -> ShellContentInstance? {
        guard let tab = tab(tabID: tabID) else { return nil }
        return tab.paneTree.paneSlotIDs.lazy.compactMap { contentMounted(in: $0) }.first
    }

    func userFacingTitle(for tab: ShellContentTab) -> String? {
        tab.title
            ?? tab.paneTree.paneSlotIDs.lazy.compactMap { contentMounted(in: $0)?.title }.first
    }

    func materializingShellState() -> ShellStateSnapshot? {
        guard contractVersion == Self.currentContractVersion else { return nil }

        let sourceTabCount = spaces.reduce(0) { count, space in
            count + space.tabs.count
        }
        let paneSlotsByID = paneSlots.reduce(into: [String: ShellPaneSlot]()) { slotsByID, slot in
            slotsByID[slot.paneSlotID] = slot
        }
        let contentsByID = contents.reduce(into: [String: ShellContentInstance]()) { contentsByID, content in
            contentsByID[content.contentID] = content
        }
        var materializedPanes: [ShellPane] = []
        var materializedPaneSlots: [ShellPaneSlot] = []
        var materializedContents: [ShellContentInstance] = []

        let materializedSpaces = spaces.map { space -> ShellSpace in
            let tabs = space.tabs.compactMap { tab -> ShellTab? in
                let paneSlotIDs = tab.paneTree.paneSlotIDs
                guard !paneSlotIDs.isEmpty else { return nil }

                var tabPaneSlots: [ShellPaneSlot] = []
                var tabContents: [ShellContentInstance] = []
                for paneSlotID in paneSlotIDs {
                    guard let paneSlot = paneSlotsByID[paneSlotID],
                          let content = contentsByID[paneSlot.contentID]
                    else {
                        return nil
                    }
                    tabPaneSlots.append(paneSlot)
                    tabContents.append(content)
                }

                materializedPanes.append(
                    contentsOf: zip(tabPaneSlots, tabContents).map {
                        ShellPane.restoringContent($1, mountedIn: $0)
                    }
                )
                materializedPaneSlots.append(contentsOf: tabPaneSlots)
                materializedContents.append(contentsOf: tabContents)

                return ShellTab(
                    tabID: tab.tabID,
                    kind: tab.kind,
                    title: tab.title,
                    paneTree: tab.paneTree.restoringPaneTree(),
                    isPinned: tab.isPinned,
                    isTitleUserLocked: tab.isTitleUserLocked
                )
            }

            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: Self.strongestAttention(
                    in: materializedPaneSlots.filter { $0.spaceID == space.spaceID }
                ),
                tabs: tabs,
                selectedTabID: space.resolvedSelectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }

        if sourceTabCount > 0 && materializedPanes.isEmpty {
            return nil
        }

        let existingSpaceIDs = Set(materializedSpaces.map(\.spaceID))
        let focusableSpaces = materializedSpaces.filter { !$0.tabs.isEmpty }
        let resolvedFocusedSpaceID = focusedSpaceID.flatMap {
            existingSpaceIDs.contains($0) ? $0 : nil
        } ?? focusableSpaces.first?.spaceID ?? materializedSpaces.first?.spaceID
        let focusedSpace = resolvedFocusedSpaceID.flatMap { spaceID in
            materializedSpaces.first { $0.spaceID == spaceID }
        }
        let resolvedFocusedTabID = focusedTabID.flatMap { tabID in
            focusedSpace?.tabs.contains { $0.tabID == tabID } == true ? tabID : nil
        } ?? focusedSpace?.tabs.first?.tabID
        let focusedTab = resolvedFocusedTabID.flatMap { tabID in
            focusedSpace?.tabs.first { $0.tabID == tabID }
        }
        let focusedTabPaneIDs = Set(focusedTab?.paneTree.paneIDs ?? [])
        let resolvedFocusedPaneID = focusedPaneSlotID.flatMap {
            focusedTabPaneIDs.contains($0) ? $0 : nil
        } ?? focusedTab?.paneTree.paneIDs.first
        let repairedSpaces = materializedSpaces.map { space in
            let preferredTabID = space.spaceID == resolvedFocusedSpaceID ? resolvedFocusedTabID : nil
            return space.repairingSelectedTabID(preferredTabID: preferredTabID)
        }

        return ShellStateSnapshot(
            contractVersion: Self.currentContractVersion,
            windowID: windowID,
            focusedSpaceID: resolvedFocusedSpaceID,
            focusedTabID: resolvedFocusedTabID,
            focusedPaneID: resolvedFocusedPaneID,
            spaces: repairedSpaces,
            panes: materializedPanes,
            paneSlots: materializedPaneSlots,
            contents: materializedContents
        )
    }

    private static func strongestAttention(in paneSlots: [ShellPaneSlot]) -> ShellAttentionState {
        paneSlots
            .map(\.attention)
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private static func paneSlotLocations(
        in spaces: [ShellSpace]
    ) -> [String: (spaceID: String, tabID: String)] {
        spaces.reduce(into: [String: (spaceID: String, tabID: String)]()) { locationsByID, space in
            for tab in space.tabs {
                for paneSlotID in tab.paneTree.paneIDs {
                    locationsByID[paneSlotID] = (spaceID: space.spaceID, tabID: tab.tabID)
                }
            }
        }
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
}

private extension ShellPane {
    static func restoringContent(
        _ content: ShellContentInstance,
        mountedIn paneSlot: ShellPaneSlot
    ) -> ShellPane {
        let terminalPayload = content.payload.terminal
        return ShellPane(
            paneID: paneSlot.paneSlotID,
            tabID: paneSlot.tabID,
            spaceID: paneSlot.spaceID,
            launchTarget: terminalPayload?.launchTarget,
            cwd: terminalPayload?.cwd,
            process: nil,
            attention: paneSlot.attention,
            context: nil,
            viewport: ShellViewportSnapshot(
                title: content.title,
                summary: restoredSummary(for: content.kind),
                visibleExcerpt: nil,
                lastActivityAt: nil
            ),
            alanBinding: nil,
            terminalProfileID: terminalPayload?.terminalProfileID
        )
    }

    static func restoredSummary(for kind: ShellContentKind) -> String? {
        switch kind {
        case .terminal:
            return nil
        case .markdown:
            return "markdown viewer ready"
        case .settings:
            return "settings surface ready"
        case .agent:
            return "Agent attachment ready"
        }
    }
}

extension ShellPaneSlot {
    static func projectingTerminalPane(_ pane: ShellPane) -> ShellPaneSlot {
        ShellPaneSlot(
            paneSlotID: pane.paneID,
            tabID: pane.tabID,
            spaceID: pane.spaceID,
            contentID: ShellContentInstance.terminalContentID(forPaneID: pane.paneID),
            attention: pane.attention
        )
    }
}

extension ShellContentInstance {
    static func projectingTerminalPane(_ pane: ShellPane) -> ShellContentInstance {
        projectingTerminalPane(pane, contentID: terminalContentID(forPaneID: pane.paneID))
    }

    static func projectingTerminalPane(_ pane: ShellPane, contentID: String) -> ShellContentInstance {
        let title = terminalTitle(for: pane)
        return ShellContentInstance(
            contentID: contentID,
            kind: .terminal,
            title: title,
            payload: .terminal(
                ShellTerminalContentPayload(
                    launchTarget: pane.resolvedLaunchTarget,
                    cwd: pane.cwd,
                    title: title,
                    terminalProfileID: pane.terminalProfileID
                )
            ),
            rendererState: terminalRendererState(for: pane)
        )
    }

    static func terminalContentID(forPaneID paneID: String) -> String {
        "content_\(paneID)"
    }

    static func markdownContentID(forPaneSlotID paneSlotID: String) -> String {
        "content_markdown_\(paneSlotID)"
    }

    static let settingsSurfaceID = "settings_main"
    static let settingsContentID = "content_settings_main"

    private static func terminalTitle(for pane: ShellPane) -> String {
        if let title = pane.viewport?.title?.trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty
        {
            return title
        }

        switch pane.resolvedLaunchTarget {
        case .shell:
            return "Shell"
        }
    }

    private static func terminalRendererState(for pane: ShellPane) -> ShellContentRendererState {
        let phase = pane.context?.rendererPhase
            ?? pane.context?.rendererHealth
            ?? "placeholder"
        let detail = pane.context?.surfaceReadiness ?? pane.viewport?.summary
        return ShellContentRendererState(phase: phase, detail: detail)
    }
}
