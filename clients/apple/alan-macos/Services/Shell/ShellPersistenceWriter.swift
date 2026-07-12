import Foundation

/// Threading seam for durable shell persistence. Workspace-manifest encoding and
/// atomic writes run on a serial background executor; the control plane owns its
/// separate temporary IPC projection.
protocol ShellPersistenceWriting: AnyObject {
    /// Blocks the caller until the manifest is written (structural mutations).
    /// Returns `true` when the write succeeded so callers can advance their
    /// last-saved state and surface failures.
    @discardableResult
    func writeManifestSync(_ manifest: ShellContentWorkspaceManifest) -> Bool
    /// Enqueues the manifest write without blocking the caller (debounced content).
    /// Failures are reported through the writer's error sink, not the caller.
    func writeManifestAsync(_ manifest: ShellContentWorkspaceManifest)
}

final class ShellPersistenceWriter: ShellPersistenceWriting {
    private let manifestStore: ShellWorkspaceManifestStore?
    private let queue: DispatchQueue
    /// Reports async-write failures. Set once after construction (before any write
    /// is enqueued) so the owner can route failures to its diagnostics surface.
    var onError: (String) -> Void

    init(
        manifestStore: ShellWorkspaceManifestStore?,
        queue: DispatchQueue = DispatchQueue(label: "app.alan.shell.persistence", qos: .utility),
        onError: @escaping (String) -> Void = { NSLog("%@", $0) }
    ) {
        self.manifestStore = manifestStore
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
