import Foundation

/// Threading seam for shell persistence. The encode + atomic disk writes for both
/// the workspace manifest and the control-plane shell-state file run on a serial
/// background executor; callers choose synchronous durability (structural
/// mutations) or fire-and-forget (debounced terminal-callback churn) so the
/// terminal callback path never blocks the main thread on disk.
protocol ShellPersistenceWriting: AnyObject {
    /// Blocks the caller until the manifest is written (structural mutations).
    /// Returns `true` when the write succeeded so callers can advance their
    /// last-saved state and surface failures.
    @discardableResult
    func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool
    /// Enqueues the manifest write without blocking the caller (debounced content).
    /// Failures are reported through the writer's error sink, not the caller.
    func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest)
    /// Blocks the caller until the shell-state file is written (structural).
    func writeShellStateSync(_ state: ShellStateSnapshot)
    /// Enqueues the shell-state file write without blocking the caller (debounced).
    func writeShellStateAsync(_ state: ShellStateSnapshot)
}

final class ShellPersistenceWriter: ShellPersistenceWriting {
    private let manifestStore: ShellWorkspaceManifestStore?
    private let stateStore: ShellStatePersistenceStore
    private let queue: DispatchQueue
    /// Reports async-write failures. Set once after construction (before any write
    /// is enqueued) so the owner can route failures to its diagnostics surface.
    var onError: (String) -> Void

    init(
        manifestStore: ShellWorkspaceManifestStore?,
        stateStore: ShellStatePersistenceStore,
        queue: DispatchQueue = DispatchQueue(label: "app.alan.shell.persistence", qos: .utility),
        onError: @escaping (String) -> Void = { NSLog("%@", $0) }
    ) {
        self.manifestStore = manifestStore
        self.stateStore = stateStore
        self.queue = queue
        self.onError = onError
    }

    @discardableResult
    func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool {
        queue.sync { self.trySaveManifest(manifest) }
    }

    func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest) {
        queue.async {
            if !self.trySaveManifest(manifest) {
                self.onError("workspace manifest async save failed")
            }
        }
    }

    func writeShellStateSync(_ state: ShellStateSnapshot) {
        queue.sync { self.stateStore.save(state) }
    }

    func writeShellStateAsync(_ state: ShellStateSnapshot) {
        queue.async { self.stateStore.save(state) }
    }

    /// Returns `true` on success (or when there is no manifest store to write to).
    private func trySaveManifest(_ manifest: ShellContentWorkspaceManifest) -> Bool {
        guard let manifestStore else { return true }
        do {
            try manifestStore.save(manifest)
            return true
        } catch {
            return false
        }
    }
}
