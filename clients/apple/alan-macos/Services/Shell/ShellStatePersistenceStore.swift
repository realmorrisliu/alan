import Foundation

#if os(macOS)
struct ShellStatePersistenceStore {
    private static let persistenceFilePrefix = "shell-state-"
    private static let persistenceFileExtension = ".json"
    private static let defaultRestorationWindowID = "window_main"

    private let fileManager: FileManager
    private let persistenceURL: URL

    init(fileManager: FileManager = .default, persistenceURL: URL) {
        self.fileManager = fileManager
        self.persistenceURL = persistenceURL
    }

    func save(_ shellState: ShellStateSnapshot) {
        let parentURL = persistenceURL.deletingLastPathComponent()
        try? fileManager.createDirectory(at: parentURL, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(shellState.contentStateProjection()) else { return }
        try? data.write(to: persistenceURL, options: .atomic)
    }

    static func defaultPersistenceURL(windowID: String, fileManager: FileManager) -> URL {
        let sanitizedWindowID = windowID
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: ":", with: "_")
        return persistenceDirectory(fileManager: fileManager)
            .appendingPathComponent("\(persistenceFilePrefix)\(sanitizedWindowID)\(persistenceFileExtension)")
    }

    @MainActor
    static func restoredWindowContext(
        fileManager: FileManager,
        restorePrevious: Bool
    ) -> ShellWindowContext? {
        guard restorePrevious else { return nil }

        let directories = [persistenceDirectory(fileManager: fileManager)]
            + legacyPersistenceDirectories(fileManager: fileManager)

        let candidates = directories.flatMap { directory -> [(Date, ShellWindowContext)] in
            guard let urls = try? fileManager.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: [.contentModificationDateKey],
                options: [.skipsHiddenFiles]
            ) else {
                return []
            }

            return urls.compactMap { url -> (Date, ShellWindowContext)? in
                guard isShellStatePersistenceURL(url),
                      let windowID = restorePersistedWindowID(fileManager: fileManager, persistenceURL: url)
                else {
                    return nil
                }

                let values = try? url.resourceValues(forKeys: [.contentModificationDateKey])
                let modifiedAt = values?.contentModificationDate ?? .distantPast
                let canonicalURL = defaultPersistenceURL(
                    windowID: windowID,
                    fileManager: fileManager
                )
                return (
                    modifiedAt,
                    ShellWindowContext(
                        windowID: windowID,
                        persistenceURL: canonicalURL,
                        terminalRuntimeRegistry: TerminalRuntimeRegistry()
                    )
                )
            }
        }

        return candidates.max { lhs, rhs in lhs.0 < rhs.0 }?.1
    }

    @MainActor
    static func defaultWindowContext(
        fileManager: FileManager,
        restorePrevious: Bool
    ) -> ShellWindowContext {
        if restorePrevious {
            return ShellWindowContext.make(
                fileManager: fileManager,
                windowID: defaultRestorationWindowID
            )
        }

        return ShellWindowContext.make(fileManager: fileManager)
    }

    static func restoreShellState(
        fileManager: FileManager,
        persistenceURL: URL
    ) -> ShellStateSnapshot? {
        let restoreURL = readablePersistenceURL(fileManager: fileManager, canonicalURL: persistenceURL)
        guard let restoreURL,
              let data = try? Data(contentsOf: restoreURL),
              let state = try? JSONDecoder().decode(ShellStateSnapshot.self, from: data),
              !state.spaces.isEmpty,
              !state.panes.isEmpty
        else {
            return nil
        }
        return state
    }

    private static func restorePersistedWindowID(
        fileManager: FileManager,
        persistenceURL: URL
    ) -> String? {
        let restoreURL = readablePersistenceURL(fileManager: fileManager, canonicalURL: persistenceURL)
        guard let restoreURL,
              let data = try? Data(contentsOf: restoreURL)
        else {
            return nil
        }

        if let contentState = try? JSONDecoder().decode(ShellContentStateSnapshot.self, from: data),
           contentState.contractVersion == ShellContentStateSnapshot.currentContractVersion,
           !contentState.windowID.isEmpty,
           !contentState.spaces.isEmpty
        {
            return contentState.windowID
        }

        if let state = try? JSONDecoder().decode(ShellStateSnapshot.self, from: data),
           !state.spaces.isEmpty,
           !state.panes.isEmpty
        {
            return state.windowID
        }

        return nil
    }

    private static func persistenceDirectory(fileManager: FileManager) -> URL {
        let appSupportURL =
            fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fileManager.temporaryDirectory
        return appSupportURL.appendingPathComponent("alan-macos", isDirectory: true)
    }

    private static func legacyPersistenceDirectories(fileManager: FileManager) -> [URL] {
        let appSupportURL =
            fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fileManager.temporaryDirectory
        return [
            appSupportURL.appendingPathComponent("AlanNative", isDirectory: true),
        ]
    }

    private static func readablePersistenceURL(
        fileManager: FileManager,
        canonicalURL: URL
    ) -> URL? {
        if fileManager.fileExists(atPath: canonicalURL.path) {
            return canonicalURL
        }

        return legacyPersistenceDirectories(fileManager: fileManager)
            .map { $0.appendingPathComponent(canonicalURL.lastPathComponent) }
            .first { fileManager.fileExists(atPath: $0.path) }
    }

    private static func isShellStatePersistenceURL(_ url: URL) -> Bool {
        let fileName = url.lastPathComponent
        return fileName.hasPrefix(persistenceFilePrefix)
            && fileName.hasSuffix(persistenceFileExtension)
            && fileName != "shell-state-v0.1.json"
    }
}
#endif
