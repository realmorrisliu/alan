import Foundation

@MainActor
struct ShellWorkspaceManifestProjector {
    typealias DiagnosticHandler = (String) -> Void
    private static let iso8601Formatter = ISO8601DateFormatter()

    func makeManifest(
        from state: ShellStateSnapshot,
        previousManifest: ShellContentWorkspaceManifest?,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        now: Date,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot],
        onDiagnostic: DiagnosticHandler
    ) -> ShellContentWorkspaceManifest {
        let existingSpaces = Dictionary(
            uniqueKeysWithValues: (previousManifest?.spaces ?? []).map { ($0.spaceID, $0) }
        )
        let existingTabs = Dictionary(
            uniqueKeysWithValues: (previousManifest?.spaces ?? [])
                .flatMap(\.tabs)
                .map { ($0.tabID, $0) }
        )
        let contentState = state.contentStateProjection()

        let spaces = state.spaces.enumerated().map { index, space in
            let existingSpace = existingSpaces[space.spaceID]
            let tabRecords = space.tabs.map { tab in
                let existingTab = existingTabs[tab.tabID]
                let panes = state.panes(in: tab.tabID)
                let snapshot = makeRestoreSnapshot(
                    for: tab,
                    contentState: contentState,
                    terminalRuntimeRegistry: terminalRuntimeRegistry,
                    transcriptSnapshotOverrides: transcriptSnapshotOverrides,
                    onDiagnostic: onDiagnostic
                )
                let paneActivityAt = panes.compactMap(paneActivityDate).max()
                let lastActivatedAt = tab.tabID == state.focusedTabID
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
                    activeTask: terminalRuntimeRegistry.strongestActiveTask(
                        for: tab,
                        in: state
                    )
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
            windowID: state.windowID,
            selectedSpaceID: state.focusedSpaceID,
            selectedTabID: state.focusedTabID,
            spaces: spaces
        )
        manifest.repairSelection()
        return manifest
    }

    func makePinnedSnapshot(
        tabID: String,
        state: ShellStateSnapshot,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        onDiagnostic: DiagnosticHandler
    ) -> ShellContentTabRestoreSnapshot? {
        state.tab(tabID: tabID).map {
            makeRestoreSnapshot(
                for: $0,
                contentState: state.contentStateProjection(),
                terminalRuntimeRegistry: terminalRuntimeRegistry,
                transcriptSnapshotOverrides: [:],
                onDiagnostic: onDiagnostic
            )
        }
    }

    private func makeRestoreSnapshot(
        for tab: ShellTab,
        contentState: ShellContentStateSnapshot,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot],
        onDiagnostic: DiagnosticHandler
    ) -> ShellContentTabRestoreSnapshot {
        let snapshot = ShellContentTabRestoreSnapshot.projecting(tab: tab, contentState: contentState)
        var capturedTranscripts = capturedTerminalTranscriptSnapshots(
            for: snapshot,
            terminalRuntimeRegistry: terminalRuntimeRegistry,
            onDiagnostic: onDiagnostic
        )
        capturedTranscripts.merge(transcriptSnapshotOverrides) { _, override in override }
        return snapshot.overlayingTerminalTranscriptSnapshots(capturedTranscripts)
    }

    private func capturedTerminalTranscriptSnapshots(
        for snapshot: ShellContentTabRestoreSnapshot,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        onDiagnostic: DiagnosticHandler
    ) -> [String: TerminalTranscriptSnapshot] {
        snapshot.contents.reduce(into: [:]) { capturedByContentID, content in
            guard content.kind == .terminal else { return }
            switch terminalRuntimeRegistry.captureTranscriptSnapshot(
                forTerminalContentID: content.contentID
            ) {
            case .captured(let transcript):
                capturedByContentID[content.contentID] = transcript
            case .failed(let failure):
                onDiagnostic(
                    "terminal transcript capture failed for \(content.contentID): "
                        + failure.code.rawValue
                )
            }
        }
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
}
