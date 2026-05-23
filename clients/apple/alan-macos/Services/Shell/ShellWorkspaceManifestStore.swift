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
        windowID: String
    ) {
        self.init(
            fileManager: fileManager,
            manifestURL: Self.defaultManifestURL(windowID: windowID, fileManager: fileManager)
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
        fileManager: FileManager = .default
    ) -> URL {
        let applicationSupportURL = fileManager.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? fileManager.temporaryDirectory
        return applicationSupportURL
            .appendingPathComponent("alan-macos", isDirectory: true)
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
