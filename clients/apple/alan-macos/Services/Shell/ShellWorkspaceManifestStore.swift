import Foundation

struct ShellWorkspaceManifestLoadResult: Equatable {
    var manifest: ShellContentWorkspaceManifest
    var recovery: ShellWorkspaceManifestRecovery
}

enum ShellWorkspaceManifestRecovery: Equatable {
    case loadedExisting
    case migratedLegacyTerminalManifest
    case createdDefault
    case quarantinedCorruptFile(URL)
}

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

/// Debounce seam for coalescing high-frequency restore-content flush requests.
/// Injected so tests can fire the pending flush deterministically.
protocol ManifestFlushScheduling: AnyObject {
    /// Schedule `work` to run after the debounce window. Implementations run the
    /// most recently scheduled `work` once per window.
    func schedule(_ work: @escaping () -> Void)
}

final class DebouncedManifestFlushScheduler: ManifestFlushScheduling {
    private let window: DispatchTimeInterval
    private let queue: DispatchQueue
    private var pending: DispatchWorkItem?

    init(
        window: DispatchTimeInterval = .milliseconds(500),
        queue: DispatchQueue = .main
    ) {
        self.window = window
        self.queue = queue
    }

    func schedule(_ work: @escaping () -> Void) {
        pending?.cancel()
        let item = DispatchWorkItem(block: work)
        pending = item
        queue.asyncAfter(deadline: .now() + window, execute: item)
    }
}

struct ShellWorkspaceManifestStore {
    let fileManager: FileManager
    let manifestURL: URL

    init(
        fileManager: FileManager = .default,
        manifestURL: URL
    ) {
        self.fileManager = fileManager
        self.manifestURL = manifestURL
    }

    init(
        fileManager: FileManager = .default,
        windowID: String,
        channel: AlanInstallChannel = .current()
    ) {
        self.init(
            fileManager: fileManager,
            manifestURL: Self.defaultManifestURL(
                windowID: windowID,
                fileManager: fileManager,
                channel: channel
            )
        )
    }

    func loadOrCreateDefault(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellWorkspaceManifestLoadResult {
        if !fileManager.fileExists(atPath: manifestURL.path) {
            let manifest = ShellContentWorkspaceManifest.defaultManifest(
                windowID: windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
            try save(manifest)
            return ShellWorkspaceManifestLoadResult(manifest: manifest, recovery: .createdDefault)
        }

        do {
            let data = try Data(contentsOf: manifestURL)
            if let manifest = try? Self.decoder.decode(ShellContentWorkspaceManifest.self, from: data) {
                guard manifest.schemaVersion == ShellWorkspaceManifest.currentSchemaVersion,
                      manifest.contentContractVersion == ShellContentWorkspaceManifest.currentContentContractVersion
                else {
                    throw DecodingError.dataCorrupted(
                        DecodingError.Context(
                            codingPath: [],
                            debugDescription: "Unsupported shell workspace manifest schema"
                        )
                    )
                }
                return ShellWorkspaceManifestLoadResult(manifest: manifest, recovery: .loadedExisting)
            }

            let legacyManifest = try Self.decoder.decode(ShellWorkspaceManifest.self, from: data)
            guard legacyManifest.schemaVersion == ShellWorkspaceManifest.currentSchemaVersion else {
                throw DecodingError.dataCorrupted(
                    DecodingError.Context(
                        codingPath: [],
                        debugDescription: "Unsupported legacy shell workspace manifest schema"
                    )
                )
            }
            let migratedManifest = legacyManifest.migratingTerminalRestoreSnapshotsToContentContainers()
            try save(migratedManifest)
            return ShellWorkspaceManifestLoadResult(
                manifest: migratedManifest,
                recovery: .migratedLegacyTerminalManifest
            )
        } catch {
            let corruptURL = quarantineURL(now: now)
            if fileManager.fileExists(atPath: corruptURL.path) {
                try fileManager.removeItem(at: corruptURL)
            }
            try fileManager.moveItem(at: manifestURL, to: corruptURL)

            let manifest = ShellContentWorkspaceManifest.defaultManifest(
                windowID: windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
            try save(manifest)
            return ShellWorkspaceManifestLoadResult(
                manifest: manifest,
                recovery: .quarantinedCorruptFile(corruptURL)
            )
        }
    }

    func save(_ manifest: ShellContentWorkspaceManifest) throws {
        let directoryURL = manifestURL.deletingLastPathComponent()
        try fileManager.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true
        )
        let data = try Self.encoder.encode(manifest)
        try data.write(to: manifestURL, options: .atomic)
    }

    func saveLegacyTerminalManifest(_ manifest: ShellWorkspaceManifest) throws {
        let directoryURL = manifestURL.deletingLastPathComponent()
        try fileManager.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true
        )
        let data = try Self.encoder.encode(manifest)
        try data.write(to: manifestURL, options: .atomic)
    }

    static func defaultManifestURL(
        windowID: String,
        fileManager: FileManager = .default,
        channel: AlanInstallChannel = .current()
    ) -> URL {
        let applicationSupportURL = alanMacApplicationSupportDirectory(fileManager: fileManager)
        return applicationSupportURL
            .appendingPathComponent(channel.applicationSupportDirectoryName, isDirectory: true)
            .appendingPathComponent("shell-workspace-\(sanitizedWindowID(windowID)).json")
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    private static func sanitizedWindowID(_ windowID: String) -> String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_-"))
        let scalars = windowID.unicodeScalars.map { scalar in
            allowed.contains(scalar) ? Character(scalar) : "_"
        }
        let sanitized = String(scalars)
        return sanitized.isEmpty ? "window_main" : sanitized
    }

    private func quarantineURL(now: Date) -> URL {
        let basename = manifestURL.deletingPathExtension().lastPathComponent
        let pathExtension = manifestURL.pathExtension.isEmpty ? "json" : manifestURL.pathExtension
        let stamp = ISO8601DateFormatter()
            .string(from: now)
            .replacingOccurrences(of: ":", with: "")
        return manifestURL
            .deletingLastPathComponent()
            .appendingPathComponent("\(basename).corrupt-\(stamp).\(pathExtension)")
    }
}
