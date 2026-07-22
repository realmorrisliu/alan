import Foundation

@MainActor
final class ShellWorkspacePersistenceCoordinator {
    private struct PersistenceContext {
        let state: ShellStateSnapshot
        let terminalRuntimeRegistry: TerminalRuntimeRegistry
        let controlPlane: AlanShellControlPlane
    }

    private let manifestStore: ShellWorkspaceManifestStore?
    private let persistenceWriter: ShellPersistenceWriting
    private let manifestFlushScheduler: ManifestFlushScheduling
    private let manifestProjector = ShellWorkspaceManifestProjector()
    private var workspaceManifest: ShellContentWorkspaceManifest?
    private var latestContext: PersistenceContext?
    private var contentFlushScheduled = false
    private var contentFlushPending = false

    var onDiagnostic: (String) -> Void = { _ in }

    init(
        manifestStore: ShellWorkspaceManifestStore?,
        workspaceManifest: ShellContentWorkspaceManifest?,
        persistenceWriter: ShellPersistenceWriting? = nil,
        manifestFlushScheduler: ManifestFlushScheduling? = nil
    ) {
        self.manifestStore = manifestStore
        self.persistenceWriter =
            persistenceWriter
            ?? ShellPersistenceWriter(
                manifestStore: manifestStore
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

    func adoptPersistenceContext(
        state: ShellStateSnapshot,
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        controlPlane: AlanShellControlPlane
    ) {
        latestContext = PersistenceContext(
            state: state,
            terminalRuntimeRegistry: terminalRuntimeRegistry,
            controlPlane: controlPlane
        )
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
        terminalRuntimeRegistry: TerminalRuntimeRegistry,
        controlPlane: AlanShellControlPlane,
        pinSnapshotTabIDs: Set<String> = [],
        coalesced: Bool = false
    ) {
        adoptPersistenceContext(
            state: state,
            terminalRuntimeRegistry: terminalRuntimeRegistry,
            controlPlane: controlPlane
        )

        // The high-frequency terminal callback path keeps the in-memory
        // control-plane state fresh (so IPC clients never read stale pane state)
        // but defers disk work: manifest + control-plane event log + temporary
        // state.json mirror. Structural mutations persist synchronously.
        if coalesced {
            controlPlane.publishInMemory(state: state)
            contentFlushPending = true
            scheduleContentFlush()
        } else {
            contentFlushPending = false
            syncManifestFromShellState(pinSnapshotTabIDs: pinSnapshotTabIDs)
            controlPlane.publish(state: state)
        }
    }

    /// Forces the latest adopted state and any pending debounced content to disk.
    /// A queued scheduler callback becomes a no-op unless later content arrives.
    func flushWorkspacePersistence() {
        guard let context = latestContext else { return }
        contentFlushPending = false
        syncManifestFromShellState()
        context.controlPlane.publish(state: context.state)
        context.controlPlane.flushStateFile()
    }

    func persistCurrentManifest(
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot] = [:]
    ) {
        contentFlushPending = false
        syncManifestFromShellState(
            transcriptSnapshotOverrides: transcriptSnapshotOverrides
        )
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
        mutate: (inout ShellContentWorkspaceTabRecord, ShellContentTabRestoreSnapshot) -> Void,
        diagnostic: (String) -> String
    ) -> Bool {
        guard manifestPersistenceEnabled,
              let context = latestContext,
              let snapshot = manifestProjector.makePinnedSnapshot(
                  tabID: tabID,
                  state: context.state,
                  terminalRuntimeRegistry: context.terminalRuntimeRegistry,
                  onDiagnostic: onDiagnostic
              )
        else {
            return false
        }

        var manifest = manifestProjector.makeManifest(
            from: context.state,
            previousManifest: workspaceManifest,
            terminalRuntimeRegistry: context.terminalRuntimeRegistry,
            now: .now,
            transcriptSnapshotOverrides: [:],
            onDiagnostic: onDiagnostic
        )
        var didUpdate = false

        for spaceIndex in manifest.spaces.indices {
            guard let tabIndex = manifest.spaces[spaceIndex].tabs.firstIndex(where: {
                $0.tabID == tabID
            }) else {
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

    private func scheduleContentFlush() {
        guard !contentFlushScheduled else { return }
        contentFlushScheduled = true
        manifestFlushScheduler.schedule { [weak self] in
            self?.flushPendingPersistence()
        }
    }

    private func flushPendingPersistence() {
        contentFlushScheduled = false
        guard contentFlushPending, let context = latestContext else { return }
        contentFlushPending = false
        syncManifestFromShellState(coalesced: true)
        context.controlPlane.persistPublished()
    }

    private func syncManifestFromShellState(
        now: Date = .now,
        pinSnapshotTabIDs: Set<String> = [],
        transcriptSnapshotOverrides: [String: TerminalTranscriptSnapshot] = [:],
        coalesced: Bool = false
    ) {
        guard manifestPersistenceEnabled, let context = latestContext else { return }

        var manifestToSave = manifestProjector.makeManifest(
            from: context.state,
            previousManifest: workspaceManifest,
            terminalRuntimeRegistry: context.terminalRuntimeRegistry,
            now: now,
            transcriptSnapshotOverrides: transcriptSnapshotOverrides,
            onDiagnostic: onDiagnostic
        )
        if !pinSnapshotTabIDs.isEmpty {
            applyPinSnapshotOverrides(
                to: &manifestToSave,
                tabIDs: pinSnapshotTabIDs,
                context: context
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

    private func applyPinSnapshotOverrides(
        to manifest: inout ShellContentWorkspaceManifest,
        tabIDs: Set<String>,
        context: PersistenceContext
    ) {
        for spaceIndex in manifest.spaces.indices {
            for tabIndex in manifest.spaces[spaceIndex].tabs.indices {
                let tabID = manifest.spaces[spaceIndex].tabs[tabIndex].tabID
                guard tabIDs.contains(tabID),
                      let snapshot = manifestProjector.makePinnedSnapshot(
                          tabID: tabID,
                          state: context.state,
                          terminalRuntimeRegistry: context.terminalRuntimeRegistry,
                          onDiagnostic: onDiagnostic
                      )
                else { continue }

                manifest.spaces[spaceIndex].tabs[tabIndex].isPinned = true
                manifest.spaces[spaceIndex].tabs[tabIndex].pinSnapshot = snapshot
                manifest.spaces[spaceIndex].tabs[tabIndex].liveSnapshot = snapshot
            }
        }
    }
}
