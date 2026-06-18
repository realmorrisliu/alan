import Foundation

@MainActor
final class ShellWorkspacePersistenceCoordinator {
    typealias ManifestBuilder = (
        _ now: Date,
        _ transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot]
    ) -> ShellContentWorkspaceManifest?
    typealias PinnedSnapshotProvider = (_ tabID: String) -> ShellContentTabRestoreSnapshot?

    private let manifestStore: ShellWorkspaceManifestStore?
    private let persistenceWriter: ShellPersistenceWriting
    private let manifestFlushScheduler: ManifestFlushScheduling
    private var pendingContentFlushScheduled = false
    private var workspaceManifest: ShellContentWorkspaceManifest?

    var onDiagnostic: (String) -> Void = { _ in }

    init(
        manifestStore: ShellWorkspaceManifestStore?,
        stateStore: ShellStatePersistenceStore,
        workspaceManifest: ShellContentWorkspaceManifest?,
        persistenceWriter: ShellPersistenceWriting? = nil,
        manifestFlushScheduler: ManifestFlushScheduling? = nil
    ) {
        self.manifestStore = manifestStore
        self.persistenceWriter =
            persistenceWriter
            ?? ShellPersistenceWriter(
                manifestStore: manifestStore,
                stateStore: stateStore
            )
        self.manifestFlushScheduler = manifestFlushScheduler ?? DebouncedManifestFlushScheduler()
        self.workspaceManifest = workspaceManifest

        if let writer = self.persistenceWriter as? ShellPersistenceWriter {
            writer.onError = { [weak self] message in
                if Thread.isMainThread {
                    self?.onDiagnostic(message)
                } else {
                    DispatchQueue.main.async { self?.onDiagnostic(message) }
                }
            }
        }
    }

    var manifestPersistenceEnabled: Bool {
        manifestStore != nil
    }

    func currentManifest() -> ShellContentWorkspaceManifest? {
        workspaceManifest
    }

    func isTabPinned(tabID: String, in state: ShellStateSnapshot) -> Bool {
        if let tab = state.tab(tabID: tabID) {
            return tab.isPinned
        }
        return workspaceManifest?
            .spaces
            .flatMap(\.tabs)
            .first { $0.tabID == tabID }?
            .isPinned == true
    }

    func publishControlPlaneState(
        state: ShellStateSnapshot,
        controlPlane: AlanShellControlPlane,
        pinSnapshotTabIDs: Set<String> = [],
        coalesced: Bool = false,
        latestState: @escaping () -> ShellStateSnapshot?,
        makeManifest: @escaping ManifestBuilder,
        makePinnedSnapshot: @escaping PinnedSnapshotProvider
    ) {
        // The high-frequency terminal callback path keeps the in-memory
        // control-plane state fresh (so IPC clients never read stale pane state)
        // but defers all disk work: manifest + shell-state file + control-plane
        // event log + state.json mirror. Structural mutations persist
        // synchronously for prompt durability.
        if coalesced {
            controlPlane.publishInMemory(state: state)
            scheduleContentFlush(
                controlPlane: controlPlane,
                latestState: latestState,
                makeManifest: makeManifest,
                makePinnedSnapshot: makePinnedSnapshot
            )
        } else {
            syncManifestFromShellState(
                pinSnapshotTabIDs: pinSnapshotTabIDs,
                makeManifest: makeManifest,
                makePinnedSnapshot: makePinnedSnapshot
            )
            persistShellState(state)
            controlPlane.publish(state: state)
        }
    }

    func flushWorkspacePersistence(
        state: ShellStateSnapshot,
        controlPlane: AlanShellControlPlane,
        makeManifest: ManifestBuilder,
        makePinnedSnapshot: PinnedSnapshotProvider
    ) {
        pendingContentFlushScheduled = false
        syncManifestFromShellState(
            makeManifest: makeManifest,
            makePinnedSnapshot: makePinnedSnapshot
        )
        persistShellState(state)
        controlPlane.publish(state: state)
        controlPlane.flushStateFile()
    }

    @discardableResult
    func clearRestoredTranscriptSnapshot(forTerminalContentID contentID: String) -> Bool {
        guard manifestPersistenceEnabled, let workspaceManifest else { return false }

        let result = workspaceManifest.clearingRestoredTranscriptSnapshot(
            forTerminalContentID: contentID
        )
        guard result.removed else { return false }
        guard persistenceWriter.writeManifestSync(result.manifest) else {
            onDiagnostic("workspace manifest clear transcript save failed")
            return false
        }
        self.workspaceManifest = result.manifest
        return true
    }

    @discardableResult
    func updateManifestTab(
        tabID: String,
        makeManifest: ManifestBuilder,
        makePinnedSnapshot: PinnedSnapshotProvider,
        mutate: (inout ShellContentWorkspaceTabRecord, ShellContentTabRestoreSnapshot) -> Void,
        diagnostic: (String) -> String
    ) -> Bool {
        guard manifestPersistenceEnabled,
              let snapshot = makePinnedSnapshot(tabID)
        else {
            return false
        }

        guard var manifest = makeManifest(.now, [:]) else { return false }
        var didUpdate = false

        for spaceIndex in manifest.spaces.indices {
            guard let tabIndex = manifest.spaces[spaceIndex].tabs.firstIndex(where: { $0.tabID == tabID }) else {
                continue
            }
            mutate(&manifest.spaces[spaceIndex].tabs[tabIndex], snapshot)
            didUpdate = true
            break
        }

        guard didUpdate else { return false }

        guard persistenceWriter.writeManifestSync(manifest) else {
            onDiagnostic("workspace manifest save failed")
            return false
        }
        workspaceManifest = manifest
        onDiagnostic(diagnostic(tabID))
        return true
    }

    private func scheduleContentFlush(
        controlPlane: AlanShellControlPlane,
        latestState: @escaping () -> ShellStateSnapshot?,
        makeManifest: @escaping ManifestBuilder,
        makePinnedSnapshot: @escaping PinnedSnapshotProvider
    ) {
        guard !pendingContentFlushScheduled else { return }
        pendingContentFlushScheduled = true
        manifestFlushScheduler.schedule { [weak self] in
            self?.flushPendingPersistence(
                controlPlane: controlPlane,
                latestState: latestState,
                makeManifest: makeManifest,
                makePinnedSnapshot: makePinnedSnapshot
            )
        }
    }

    private func flushPendingPersistence(
        controlPlane: AlanShellControlPlane,
        latestState: () -> ShellStateSnapshot?,
        makeManifest: ManifestBuilder,
        makePinnedSnapshot: PinnedSnapshotProvider
    ) {
        pendingContentFlushScheduled = false
        guard let state = latestState() else { return }
        syncManifestFromShellState(
            coalesced: true,
            makeManifest: makeManifest,
            makePinnedSnapshot: makePinnedSnapshot
        )
        persistShellState(state, coalesced: true)
        controlPlane.persistPublished()
    }

    func syncManifestFromShellState(
        now: Date = .now,
        pinSnapshotTabIDs: Set<String> = [],
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot] = [:],
        coalesced: Bool = false,
        makeManifest: ManifestBuilder,
        makePinnedSnapshot: PinnedSnapshotProvider
    ) {
        guard manifestPersistenceEnabled else { return }

        guard var manifestToSave = makeManifest(now, transcriptSnapshotOverrides) else { return }
        if !pinSnapshotTabIDs.isEmpty {
            applyPinSnapshotOverrides(
                to: &manifestToSave,
                tabIDs: pinSnapshotTabIDs,
                makePinnedSnapshot: makePinnedSnapshot
            )
        }
        if coalesced {
            // Debounced restore content: advance the intended last-saved manifest
            // optimistically. A failed write self-heals on the next flush, which
            // rebuilds from current state; the writer surfaces async failures.
            workspaceManifest = manifestToSave
            persistenceWriter.writeManifestAsync(manifestToSave)
        } else if persistenceWriter.writeManifestSync(manifestToSave) {
            workspaceManifest = manifestToSave
        } else {
            onDiagnostic("workspace manifest save failed")
        }
    }

    func persistShellState(
        _ state: ShellStateSnapshot,
        coalesced: Bool = false
    ) {
        if coalesced {
            persistenceWriter.writeShellStateAsync(state)
        } else {
            persistenceWriter.writeShellStateSync(state)
        }
    }

    private func applyPinSnapshotOverrides(
        to manifest: inout ShellContentWorkspaceManifest,
        tabIDs: Set<String>,
        makePinnedSnapshot: PinnedSnapshotProvider
    ) {
        for spaceIndex in manifest.spaces.indices {
            for tabIndex in manifest.spaces[spaceIndex].tabs.indices {
                let tabID = manifest.spaces[spaceIndex].tabs[tabIndex].tabID
                guard tabIDs.contains(tabID),
                      let snapshot = makePinnedSnapshot(tabID)
                else { continue }

                manifest.spaces[spaceIndex].tabs[tabIndex].isPinned = true
                manifest.spaces[spaceIndex].tabs[tabIndex].pinSnapshot = snapshot
                manifest.spaces[spaceIndex].tabs[tabIndex].liveSnapshot = snapshot
            }
        }
    }
}
