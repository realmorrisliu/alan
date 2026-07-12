import Foundation

struct ShellWorkspaceManifestLoadResult: Equatable {
    var manifest: ShellContentWorkspaceManifest
    var recovery: ShellWorkspaceManifestRecovery
}

enum ShellWorkspaceManifestRecovery: Equatable {
    case loadedExisting
    case createdDefault
    case quarantinedCorruptFile(URL)
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
    let validateManifest: (Data) throws -> Bool

    init(
        fileManager: FileManager = .default,
        manifestURL: URL,
        validateManifest: @escaping (Data) throws -> Bool = {
            try ShellCoreFFIAdapter.shared.validateContentWorkspaceManifest(data: $0)
        }
    ) {
        self.fileManager = fileManager
        self.manifestURL = manifestURL
        self.validateManifest = validateManifest
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
            let manifest = try ShellCoreFFIAdapter.shared.defaultContentWorkspaceManifest(
                windowID: windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
            try save(manifest)
            return ShellWorkspaceManifestLoadResult(manifest: manifest, recovery: .createdDefault)
        }

        let data: Data
        do {
            data = try Data(contentsOf: manifestURL)
        } catch {
            return try quarantineCorruptManifest(
                windowID: windowID,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
        }

        if try validateManifest(data),
           let manifest = try? Self.decoder.decode(ShellContentWorkspaceManifest.self, from: data)
        {
            guard manifest.schemaVersion == ShellContentWorkspaceManifest.currentSchemaVersion,
                  manifest.contentContractVersion == ShellContentWorkspaceManifest.currentContentContractVersion
            else {
                return try quarantineCorruptManifest(
                    windowID: windowID,
                    defaultWorkingDirectory: defaultWorkingDirectory,
                    now: now
                )
            }
            return ShellWorkspaceManifestLoadResult(manifest: manifest, recovery: .loadedExisting)
        }

        return try quarantineCorruptManifest(
            windowID: windowID,
            defaultWorkingDirectory: defaultWorkingDirectory,
            now: now
        )
    }

    private func quarantineCorruptManifest(
        windowID: String,
        defaultWorkingDirectory: String,
        now: Date
    ) throws -> ShellWorkspaceManifestLoadResult {
        let corruptURL = quarantineURL(now: now)
        if fileManager.fileExists(atPath: corruptURL.path) {
            try fileManager.removeItem(at: corruptURL)
        }
        try fileManager.moveItem(at: manifestURL, to: corruptURL)

        let manifest = try ShellCoreFFIAdapter.shared.defaultContentWorkspaceManifest(
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

    func save(_ manifest: ShellContentWorkspaceManifest) throws {
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
