import Foundation

#if os(macOS)
extension ShellHostController {
    /// Forces pending debounced persistence to disk synchronously. Wired to app
    /// background/resign-active and quit so a clean exit never loses pending
    /// restore content; also a deterministic flush point for tests.
    func flushWorkspacePersistence() {
        persistenceCoordinator.flushWorkspacePersistence(
            state: shellState,
            controlPlane: controlPlane,
            makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                self?.makeWorkspaceManifestFromShellState(
                    now: now,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
            },
            makePinnedSnapshot: { [weak self] tabID in
                self?.makePinnedTabSnapshot(tabID: tabID)
            }
        )
    }

    func clearRestoredTranscriptSnapshotFromWorkspaceManifest(
        forTerminalContentID contentID: String
    ) -> Bool {
        persistenceCoordinator.clearRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
    }

    func makePinnedTabSnapshot(tabID: String) -> ShellContentTabRestoreSnapshot? {
        shellState.tab(tabID: tabID).map(makeRestoreSnapshot)
    }

    func updateWorkspaceManifestTab(
        tabID: String,
        mutate: (inout ShellContentWorkspaceTabRecord, ShellContentTabRestoreSnapshot) -> Void,
        diagnostic: (String) -> String
    ) -> Bool {
        let updated = persistenceCoordinator.updateManifestTab(
            tabID: tabID,
            makeManifest: { [weak self] now, transcriptSnapshotOverrides in
                self?.makeWorkspaceManifestFromShellState(
                    now: now,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
            },
            makePinnedSnapshot: { [weak self] tabID in
                self?.makePinnedTabSnapshot(tabID: tabID)
            },
            mutate: mutate,
            diagnostic: diagnostic
        )
        if updated {
            objectWillChange.send()
        }
        return updated
    }

    private func makeWorkspaceManifestFromShellState(now: Date) -> ShellContentWorkspaceManifest {
        makeWorkspaceManifestFromShellState(now: now, transcriptSnapshotOverrides: [:])
    }

    func makeWorkspaceManifestFromShellState(
        now: Date,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentWorkspaceManifest {
        let existingSpaces = Dictionary(
            uniqueKeysWithValues: (workspaceManifest?.spaces ?? []).map { ($0.spaceID, $0) }
        )
        let existingTabs = Dictionary(
            uniqueKeysWithValues: (workspaceManifest?.spaces ?? [])
                .flatMap(\.tabs)
                .map { ($0.tabID, $0) }
        )
        let contentState = shellState.contentStateProjection()

        let spaces = shellState.spaces.enumerated().map { index, space -> ShellContentWorkspaceSpaceRecord in
            let existingSpace = existingSpaces[space.spaceID]
            let tabRecords = space.tabs.map { tab -> ShellContentWorkspaceTabRecord in
                let existingTab = existingTabs[tab.tabID]
                let panes = shellState.panes(in: tab.tabID)
                let snapshot = makeRestoreSnapshot(
                    for: tab,
                    contentState: contentState,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides
                )
                let paneActivityAt = panes.compactMap { paneActivityDate($0) }.max()
                let lastActivatedAt = tab.tabID == shellState.focusedTabID
                    ? now
                    : (existingTab?.lastActivatedAt ?? now)
                let lastActivityAt = max(
                    existingTab?.lastActivityAt ?? now,
                    paneActivityAt ?? existingTab?.lastActivityAt ?? now
                )

                return ShellContentWorkspaceTabRecord(
                    tabID: tab.tabID,
                    title: tab.title,
                    kind: tab.kind,
                    createdAt: existingTab?.createdAt ?? now,
                    lastActivatedAt: lastActivatedAt,
                    lastActivityAt: lastActivityAt,
                    isPinned: tab.isPinned,
                    isTitleUserLocked: tab.isTitleUserLocked,
                    pinSnapshot: tab.isPinned ? existingTab?.pinSnapshot : nil,
                    liveSnapshot: snapshot,
                    activeTask: projectedActiveTask(for: tab, panes: panes)
                )
            }

            return ShellContentWorkspaceSpaceRecord(
                spaceID: space.spaceID,
                title: space.title,
                order: existingSpace?.order ?? index,
                createdAt: existingSpace?.createdAt ?? now,
                updatedAt: now,
                selectedTabID: space.resolvedSelectedTabID,
                tabs: tabRecords,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }

        var manifest = ShellContentWorkspaceManifest(
            schemaVersion: ShellContentWorkspaceManifest.currentSchemaVersion,
            contentContractVersion: ShellContentWorkspaceManifest.currentContentContractVersion,
            windowID: shellState.windowID,
            selectedSpaceID: shellState.focusedSpaceID ?? selectedSpaceID,
            selectedTabID: shellState.focusedTabID,
            spaces: spaces
        )
        manifest.repairSelection()
        return manifest
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab
    ) -> ShellContentTabRestoreSnapshot {
        makeRestoreSnapshot(for: tab, contentState: shellState.contentStateProjection())
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab,
        contentState: ShellContentStateSnapshot
    ) -> ShellContentTabRestoreSnapshot {
        makeRestoreSnapshot(
            for: tab,
            contentState: contentState,
            transcriptSnapshotOverrides: [:]
        )
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab,
        contentState: ShellContentStateSnapshot,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentTabRestoreSnapshot {
        let snapshot = ShellContentTabRestoreSnapshot.projecting(tab: tab, contentState: contentState)
        var capturedTranscripts = capturedTerminalTranscriptSnapshots(for: snapshot)
        capturedTranscripts.merge(transcriptSnapshotOverrides) { _, override in override }
        return snapshot.overlayingTerminalTranscriptSnapshots(capturedTranscripts)
    }

    private func capturedTerminalTranscriptSnapshots(
        for snapshot: ShellContentTabRestoreSnapshot
    ) -> [String: TerminalTranscriptSnapshot] {
        var capturedByContentID: [String: TerminalTranscriptSnapshot] = [:]
        for content in snapshot.contents where content.kind == .terminal {
            if let transcript = capturedTerminalTranscriptSnapshot(forContentID: content.contentID) {
                capturedByContentID[content.contentID] = transcript
            }
        }
        return capturedByContentID
    }

    private func capturedTerminalTranscriptSnapshot(
        forContentID contentID: String
    ) -> TerminalTranscriptSnapshot? {
        switch terminalRuntimeRegistry.captureTranscriptSnapshot(forTerminalContentID: contentID) {
        case .captured(let transcript):
            return transcript
        case .failed(let failure):
            recordControlPlaneDiagnostic(
                "terminal transcript capture failed for \(contentID): \(failure.code.rawValue)"
            )
            return nil
        }
    }

    @discardableResult
    func captureTerminalTranscriptSnapshots(
        for impact: ShellCloseGuardImpact
    ) -> [String: TerminalTranscriptSnapshot] {
        var capturedByContentID: [String: TerminalTranscriptSnapshot] = [:]
        for contentID in impact.affectedTerminalContentIDs {
            switch terminalRuntimeRegistry.captureTranscriptSnapshot(forTerminalContentID: contentID) {
            case .captured(let transcript):
                capturedByContentID[contentID] = transcript
            case .failed(let failure):
                recordControlPlaneDiagnostic(
                    "terminal transcript capture failed for \(contentID): \(failure.code.rawValue)"
                )
            }
        }
        return capturedByContentID
    }

    private func paneActivityDate(_ pane: ShellPane) -> Date? {
        if let lastActivityAt = pane.viewport?.lastActivityAt,
           let date = Self.iso8601Formatter.date(from: lastActivityAt)
        {
            return date
        }

        if let lastMetadataAt = pane.context?.lastMetadataAt,
           let date = Self.iso8601Formatter.date(from: lastMetadataAt)
        {
            return date
        }

        return nil
    }

    private func projectedActiveTask(
        for tab: ShellTab,
        panes: [ShellPane]
    ) -> ShellTabActiveTaskState {
        if let terminalActiveTask = strongestTerminalActiveTask(
            in: panes.filter { tab.contains(paneID: $0.paneID) }
        ),
           terminalActiveTask.protectsFromPruning
        {
            return terminalActiveTask
        }

        for pane in panes where tab.contains(paneID: pane.paneID) {
            if pane.alanBinding?.pendingRequest == true {
                return .alanPendingYield
            }

            if let machineState = pane.alanBinding?.machineState,
               !Self.inactiveAlanMachineStates.contains(machineState.lowercased())
            {
                return .alanRunning
            }

            if pane.context?.processState == "foreground_command" {
                return .foregroundCommand
            }
        }

        return .inactive
    }

    func activeTaskByTabID() -> [String: ShellTabActiveTaskState] {
        shellState.spaces
            .flatMap(\.tabs)
            .reduce(into: [String: ShellTabActiveTaskState]()) { result, tab in
                result[tab.tabID] = projectedActiveTask(for: tab, panes: shellState.panes)
            }
    }

    @discardableResult
    func recordTerminalActiveTask(
        _ activeTaskState: ShellTabActiveTaskState?,
        processExited: Bool,
        for paneID: String
    ) -> Bool {
        let nextState: ShellTabActiveTaskState?
        if processExited {
            nextState = .inactive
        } else {
            nextState = activeTaskState
        }

        guard let nextState else { return false }
        guard terminalActiveTasksByPaneID[paneID] != nextState else { return false }
        terminalActiveTasksByPaneID[paneID] = nextState
        return true
    }

    private func strongestTerminalActiveTask(in panes: [ShellPane]) -> ShellTabActiveTaskState? {
        panes
            .compactMap { terminalActiveTasksByPaneID[$0.paneID] }
            .max { activeTaskRank($0) < activeTaskRank($1) }
    }

    private func activeTaskRank(_ state: ShellTabActiveTaskState) -> Int {
        switch state {
        case .inactive:
            return 0
        case .unknown:
            return 1
        case .foregroundCommand:
            return 2
        case .alanRunning:
            return 3
        case .alanProcess:
            return 4
        case .alanPendingYield:
            return 5
        }
    }

    private static let inactiveAlanMachineStates: Set<String> = [
        "completed",
        "failed",
        "cancelled",
        "canceled",
        "exited",
        "idle",
    ]

    func recordWorkspaceManifestRecovery(_ recovery: ShellWorkspaceManifestRecovery) {
        switch recovery {
        case .loadedExisting:
            return
        case .createdDefault:
            recordControlPlaneDiagnostic("workspace manifest created default")
        case .quarantinedCorruptFile(let url):
            recordControlPlaneDiagnostic("workspace manifest corrupt file quarantined: \(url.path)")
        }
    }

}
#endif
