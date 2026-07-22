import Foundation

#if os(macOS)
extension ShellHostController {
    var spaces: [ShellSpace] {
        shellState.spaces
    }

    var selectedSpace: ShellSpace? {
        if let focusedSpaceID = shellState.focusedSpaceID,
           let focusedSpace = shellState.spaces.first(where: { $0.spaceID == focusedSpaceID })
        {
            return focusedSpace
        }
        return shellState.spaces.first
    }

    var selectedSpaceID: String? {
        selectedSpace?.spaceID
    }

    var selectedTab: ShellTab? {
        guard let selectedSpace else { return nil }
        if let focusedTabID = shellState.focusedTabID,
           let focusedTab = selectedSpace.tabs.first(where: { $0.tabID == focusedTabID })
        {
            return focusedTab
        }
        guard let selectedTabID = selectedSpace.resolvedSelectedTabID else { return nil }
        return selectedSpace.tabs.first { $0.tabID == selectedTabID }
    }

    var selectedTabID: String? {
        selectedTab?.tabID
    }

    var selectedTabPaneTree: ShellPaneTreeNode? {
        selectedTab?.paneTree
    }

    var selectedTabZoomedPaneID: String? {
        guard let selectedTab else { return nil }
        return zoomedPaneID(in: selectedTab)
    }

    var panesForSelectedTab: [ShellPane] {
        guard let tabID = selectedTab?.tabID else { return [] }
        return shellState.panes.filter { $0.tabID == tabID }
    }

    var selectedPane: ShellPane? {
        if let focusedPane, focusedPane.tabID == selectedTab?.tabID {
            return focusedPane
        }
        return panesForSelectedTab.first
    }

    var focusedPane: ShellPane? {
        guard let focusedPaneID = shellState.focusedPaneID else { return nil }
        return pane(paneID: focusedPaneID)
    }

    var selectedPaneBootProfile: AlanShellBootProfile? {
        bootProfile(for: selectedPane)
    }

    var selectedPaneRuntime: TerminalHostRuntimeSnapshot {
        runtime(for: selectedPane?.paneID)
    }

    var focusedContentSupportsTerminalCommands: Bool {
        guard let focusedPaneID = shellState.focusedPaneID,
              let pane = pane(paneID: focusedPaneID)
        else {
            return false
        }
        return paneSupportsTerminalCommands(pane, in: shellState.contentStateProjection())
    }

    var attentionItems: [ShellAttentionItem] {
        let now = Date()
        return shellState.panes
            .compactMap { pane in
                let attention = shellEffectiveAttention(for: pane, now: now)
                guard attention != .idle else { return nil }
                return ShellAttentionItem(
                    paneID: pane.paneID,
                    spaceID: pane.spaceID,
                    tabID: pane.tabID,
                    title: pane.viewport?.title ?? pane.process?.program ?? "Pane",
                    summary: pane.viewport?.summary ?? "Activity detected",
                    attention: attention
                )
            }
            .sorted {
                Self.attentionRank(for: $0.attention) == Self.attentionRank(for: $1.attention)
                    ? $0.paneID < $1.paneID
                    : Self.attentionRank(for: $0.attention) > Self.attentionRank(for: $1.attention)
            }
    }

    var routingCandidates: [AlanShellRoutingCandidate] {
        routingCandidates(preferredPaneID: selectedPane?.paneID)
    }

    var moveDestinationTabs: [ShellTab] {
        guard let selectedPane else { return [] }
        return shellState.spaces
            .flatMap(\.tabs)
            .filter { $0.tabID != selectedPane.tabID }
            .sorted {
                if $0.tabID == $1.tabID {
                    return ($0.title ?? "") < ($1.title ?? "")
                }
                return $0.tabID < $1.tabID
            }
    }

    var awaitingAttentionCount: Int {
        attentionItems.filter { $0.attention == .awaitingUser }.count
    }

    var snapshotJSON: String {
        shellState.prettyPrintedJSON
    }

    func bootProfile(for pane: ShellPane?) -> AlanShellBootProfile? {
        guard let pane else { return nil }
        seedRestoredTranscriptSnapshotIfNeeded(for: pane)
        return bootProfileCache.profile(for: pane, shellState: shellState)
    }

    func restoredTranscriptSnapshot(for pane: ShellPane?) -> TerminalTranscriptSnapshot? {
        guard let pane,
              let content = terminalContentInstance(mountedIn: pane)
        else {
            return nil
        }
        return content.payload.terminal?.transcriptSnapshot?.boundedForManifest()
    }

    @discardableResult
    func clearRestoredTranscriptSnapshot(for pane: ShellPane?) -> Bool {
        guard let pane,
              let contentID = terminalContentID(mountedIn: pane)
        else {
            return false
        }
        return clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
    }

    @discardableResult
    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) -> Bool {
        terminalRuntimeRegistry.clearRestoredTranscriptSnapshot(forTerminalContentID: contentID)
        let stateResult = shellState.clearingRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
        if stateResult.removed {
            adoptStateFromControlPlane(stateResult.state, publish: false)
            publishControlPlaneState()
        }
        let manifestRemoved = persistenceCoordinator.clearRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
        return stateResult.removed || manifestRemoved
    }

    private func seedRestoredTranscriptSnapshotIfNeeded(for pane: ShellPane) {
        guard let content = terminalContentInstance(mountedIn: pane),
              let transcriptSnapshot = content.payload.terminal?.transcriptSnapshot
        else {
            return
        }

        terminalRuntimeRegistry.seedRestoredTranscriptSnapshot(
            transcriptSnapshot,
            forTerminalContentID: content.contentID
        )
    }

    private func terminalContentInstance(mountedIn pane: ShellPane) -> ShellContentInstance? {
        let contentID =
            shellState.paneSlots?
                .first { $0.paneSlotID == pane.paneID }?
                .contentID
            ?? pane.terminalContentID
        return shellState.contents?.first { $0.contentID == contentID }
    }

    func terminalContentID(mountedIn pane: ShellPane) -> String? {
        if let mountedContent = shellState.contentStateProjection().contentMounted(in: pane.paneID) {
            return mountedContent.kind == .terminal ? mountedContent.contentID : nil
        }
        return pane.terminalContentID
    }

    func runtime(for paneID: String?) -> TerminalHostRuntimeSnapshot {
        terminalRuntimeRegistry.snapshot(for: paneID)
    }

    func terminalRenderPriority(for pane: ShellPane) -> TerminalRuntimeRenderPriority {
        let visiblePaneIDs = Set(displayPaneTree(for: selectedTab)?.paneIDs ?? [])
        return terminalRuntimeRenderPriority(
            paneID: pane.paneID,
            paneSpaceID: pane.spaceID,
            paneTabID: pane.tabID,
            selectedSpaceID: selectedSpaceID,
            selectedTabID: selectedTabID,
            focusedPaneID: shellState.focusedPaneID,
            visiblePaneIDs: visiblePaneIDs,
            windowIsVisible: shellWindowIsVisibleForRendering
        )
    }

    func updateShellWindowVisibilityForRendering(_ isVisible: Bool) {
        guard shellWindowIsVisibleForRendering != isVisible else { return }
        shellWindowIsVisibleForRendering = isVisible
        synchronizeTerminalRenderPriorities()
    }

    func displayPaneTree(for tab: ShellTab?) -> ShellPaneTreeNode? {
        guard let tab else { return nil }
        guard let zoomedPaneID = zoomedPaneID(in: tab) else {
            return tab.paneTree
        }
        return tab.paneTree.leafNode(containingPaneID: zoomedPaneID) ?? tab.paneTree
    }

    func isPaneZoomed(_ paneID: String) -> Bool {
        guard let tab = tab(containingPaneID: paneID) else { return false }
        return zoomedPaneIDByTabID[tab.tabID] == paneID
    }

    func canZoomPane(_ paneID: String) -> Bool {
        guard let tab = tab(containingPaneID: paneID) else { return false }
        return tab.paneTree.paneIDs.count > 1
    }

    @discardableResult
    func toggleSelectedPaneZoom() -> Bool {
        guard let paneID = selectedPane?.paneID else { return false }
        if isPaneZoomed(paneID) {
            return unzoomTab(containingPaneID: paneID)
        }
        return zoomPane(paneID: paneID)
    }

    @discardableResult
    func zoomPane(paneID: String) -> Bool {
        guard canZoomPane(paneID),
              let tab = tab(containingPaneID: paneID)
        else {
            return false
        }
        guard zoomedPaneIDByTabID[tab.tabID] != paneID else {
            return false
        }
        if shellState.focusedPaneID != paneID {
            focus(paneID: paneID)
        }
        zoomedPaneIDByTabID[tab.tabID] = paneID
        shellState.zoomedPaneIDByTabID[tab.tabID] = paneID
        controlPlane.recordZoomStateChanged(
            requestID: nil,
            spaceID: shellState.contentStateProjection().paneSlot(paneSlotID: paneID)?.spaceID,
            tabID: tab.tabID,
            paneID: paneID,
            zoomedPaneID: paneID
        )
        synchronizeTerminalRenderPriorities()
        return true
    }

    @discardableResult
    func unzoomSelectedTab() -> Bool {
        guard let tabID = selectedTab?.tabID else { return false }
        return unzoomTab(tabID: tabID)
    }

    @discardableResult
    private func unzoomTab(containingPaneID paneID: String) -> Bool {
        guard let pane = pane(paneID: paneID) else { return false }
        return unzoomTab(tabID: pane.tabID)
    }

    @discardableResult
    func unzoomTab(tabID: String) -> Bool {
        guard let zoomedPaneID = zoomedPaneIDByTabID[tabID] else { return false }
        let pane = pane(paneID: zoomedPaneID)
        zoomedPaneIDByTabID.removeValue(forKey: tabID)
        shellState.zoomedPaneIDByTabID.removeValue(forKey: tabID)
        controlPlane.recordZoomStateChanged(
            requestID: nil,
            spaceID: pane?.spaceID,
            tabID: tabID,
            paneID: zoomedPaneID,
            zoomedPaneID: nil
        )
        synchronizeTerminalRenderPriorities()
        return true
    }

    func select(spaceID: String) {
        guard let paneID = targetPaneID(forSpaceID: spaceID) else {
            guard shellState.space(spaceID: spaceID) != nil else { return }
            let spaces = shellState.spaces.map { space in
                guard space.spaceID == spaceID else { return space }
                return ShellSpace(
                    spaceID: space.spaceID,
                    title: space.title,
                    attention: space.attention,
                    tabs: space.tabs,
                    selectedTabID: nil,
                    terminalProfileID: space.terminalProfileID,
                    presentationIconSystemName: space.presentationIconSystemName
                )
            }
            shellState = ShellStateSnapshot(
                contractVersion: shellState.contractVersion,
                windowID: shellState.windowID,
                focusedSpaceID: spaceID,
                focusedTabID: nil,
                focusedPaneID: nil,
                spaces: spaces,
                panes: shellState.panes,
                paneSlots: shellState.paneSlots,
                contents: shellState.contents,
                zoomedPaneIDByTabID: shellState.zoomedPaneIDByTabID
            )
            refreshSelectionRuntimeProjection()
            publishControlPlaneState()
            return
        }
        focus(paneID: paneID, requestTerminalFocus: true)
    }

    func select(tabID: String) {
        guard let paneID = targetPaneID(forTabID: tabID, in: selectedSpace) else { return }
        focus(paneID: paneID, requestTerminalFocus: true)
    }

    @discardableResult
    func selectSpace(at index: Int) -> Bool {
        guard spaces.indices.contains(index) else { return false }
        select(spaceID: spaces[index].spaceID)
        return true
    }

    @discardableResult
    func selectAdjacentSpace(offset: Int) -> Bool {
        guard spaces.count > 1 else { return false }
        guard let selectedSpaceID,
              let currentIndex = spaces.firstIndex(where: { $0.spaceID == selectedSpaceID })
        else {
            select(spaceID: spaces[0].spaceID)
            return true
        }

        let nextIndex = (currentIndex + offset + spaces.count) % spaces.count
        select(spaceID: spaces[nextIndex].spaceID)
        return true
    }

    @discardableResult
    func selectAdjacentTab(offset: Int) -> Bool {
        guard let selectedSpace,
              !selectedSpace.tabs.isEmpty
        else {
            return false
        }
        guard selectedSpace.tabs.count > 1 else { return false }
        let currentTabID = selectedTab?.tabID ?? selectedSpace.tabs.first?.tabID
        guard let currentTabID,
              let currentIndex = selectedSpace.tabs.firstIndex(where: { $0.tabID == currentTabID })
        else {
            return false
        }

        let nextIndex = (currentIndex + offset + selectedSpace.tabs.count) % selectedSpace.tabs.count
        select(tabID: selectedSpace.tabs[nextIndex].tabID)
        return true
    }

    func focusAttentionItem(_ item: ShellAttentionItem) {
        focus(paneID: item.paneID, requestTerminalFocus: true)
    }

    func focus(paneID: String) {
        focus(paneID: paneID, requestTerminalFocus: false)
    }

    func focus(paneID: String, requestTerminalFocus: Bool) {
        let focusStartedAt = performanceDiagnosticsStartTime()
        let result: ShellStateMutationResult
        do {
            let rustResult = try reducerAdapter.apply(
                state: shellState,
                operation: .focusPane(paneSlotID: paneID)
            )
            // Rust owns workspace focus. Swift keeps this narrow post-pass
            // for platform terminal activity acknowledgement until activity
            // signals are fully domain-owned by shell-core.
            let acknowledgedState = rustResult.tabID.map { tabID in
                rustResult.state.acknowledgingCommandFailureActivities(
                    in: tabID,
                    focusedPaneID: paneID
                )
            } ?? rustResult.state
            result = ShellStateMutationResult(
                state: acknowledgedState,
                spaceID: rustResult.spaceID,
                tabID: rustResult.tabID,
                paneID: rustResult.paneID
            )
        } catch {
            recordControlPlaneDiagnostic("shell-core focus pane failed: \(error)")
            return
        }
        applyMutationResult(result)
        if let focusStartedAt {
            let focusedPane = pane(paneID: paneID)
            recordPerformanceDiagnostic(
                .shellFocusChange,
                durationMs: performanceDurationMs(since: focusStartedAt),
                runtime: runtime(for: paneID),
                fallbackPaneID: paneID,
                fallbackContentID: focusedPane?.terminalContentID,
                fallbackPriority: focusedPane.map { terminalRenderPriority(for: $0) }
            )
        }
        if requestTerminalFocus && canRequestTerminalFocus(for: paneID) {
            terminalRuntimeRegistry.requestFocus(for: paneID)
        }
    }

    private func targetPaneID(forSpaceID spaceID: String) -> String? {
        guard let space = shellState.spaces.first(where: { $0.spaceID == spaceID }) else {
            return nil
        }
        let targetTab =
            space.selectedTabID.flatMap { selectedTabID in
                space.tabs.first { $0.tabID == selectedTabID }
            }
            ?? space.tabs.first { tab in
                guard let focusedPaneID = shellState.focusedPaneID else { return false }
                return tab.contains(paneID: focusedPaneID)
            }
            ?? space.tabs.first
        return targetTab.flatMap(targetPaneID)
    }

    private func targetPaneID(
        forTabID tabID: String,
        in space: ShellSpace?
    ) -> String? {
        guard let tab = space?.tabs.first(where: { $0.tabID == tabID }) else {
            return nil
        }
        return targetPaneID(for: tab)
    }

    private func targetPaneID(for tab: ShellTab) -> String? {
        if let focusedPaneID = shellState.focusedPaneID,
           tab.contains(paneID: focusedPaneID)
        {
            return focusedPaneID
        }
        let contentState = shellState.contentStateProjection()
        return tab.paneTree.paneIDs.first { paneID in
            contentState.paneSlot(paneSlotID: paneID)?.tabID == tab.tabID
        } ?? tab.paneTree.paneIDs.first
    }

    func refocusSelectedTerminalPane() {
        guard let paneID = selectedPane?.paneID else { return }
        guard canRequestTerminalFocus(for: paneID) else { return }
        terminalRuntimeRegistry.requestFocus(for: paneID)
    }

    private func canRequestTerminalFocus(for paneID: String) -> Bool {
        guard let pane = pane(paneID: paneID) else { return false }
        return paneHasTerminalContent(pane, in: shellState.contentStateProjection())
    }

    @discardableResult
    func requestCloseWindow() -> Bool {
        requestCloseShellSurface(scope: .window)
    }

    @discardableResult
    func requestTerminateApp() -> Bool {
        requestCloseShellSurface(scope: .app)
    }

    private func requestCloseShellSurface(scope: ShellCloseGuardScope) -> Bool {
        // Flush any debounced restore content before tearing down so a clean exit
        // never loses the most recent transcript.
        persistenceCoordinator.flushWorkspacePersistence()
        if let impact = closeGuardImpact(for: scope) {
            return confirmAndApplyClose(impact)
        }
        shutdownTerminalRuntimes()
        return true
    }

    func terminalHostDidRequestActivation(paneID: String) {
        focus(paneID: paneID)
    }

}
#endif
