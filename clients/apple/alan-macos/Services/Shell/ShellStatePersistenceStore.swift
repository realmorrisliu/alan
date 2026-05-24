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

    static func defaultPersistenceURL(
        windowID: String,
        fileManager: FileManager,
        channel: AlanInstallChannel = .current()
    ) -> URL {
        let sanitizedWindowID = windowID
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: ":", with: "_")
        return persistenceDirectory(fileManager: fileManager, channel: channel)
            .appendingPathComponent("\(persistenceFilePrefix)\(sanitizedWindowID)\(persistenceFileExtension)")
    }

    @MainActor
    static func restoredWindowContext(
        fileManager: FileManager,
        restorePrevious: Bool,
        channel: AlanInstallChannel = .current()
    ) -> ShellWindowContext? {
        guard restorePrevious else { return nil }

        let directories = [persistenceDirectory(fileManager: fileManager, channel: channel)]
            + legacyPersistenceDirectories(fileManager: fileManager, channel: channel)

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
                      let windowID = restorePersistedWindowID(
                        fileManager: fileManager,
                        persistenceURL: url,
                        channel: channel
                      )
                else {
                    return nil
                }

                let values = try? url.resourceValues(forKeys: [.contentModificationDateKey])
                let modifiedAt = values?.contentModificationDate ?? .distantPast
                let canonicalURL = defaultPersistenceURL(
                    windowID: windowID,
                    fileManager: fileManager,
                    channel: channel
                )
                return (
                    modifiedAt,
                    ShellWindowContext(
                        windowID: windowID,
                        persistenceURL: canonicalURL,
                        installChannel: channel,
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
        restorePrevious: Bool,
        channel: AlanInstallChannel = .current()
    ) -> ShellWindowContext {
        if restorePrevious {
            return ShellWindowContext.make(
                fileManager: fileManager,
                windowID: defaultRestorationWindowID,
                installChannel: channel
            )
        }

        return ShellWindowContext.make(fileManager: fileManager, installChannel: channel)
    }

    static func restoreShellState(
        fileManager: FileManager,
        persistenceURL: URL,
        channel: AlanInstallChannel = .current()
    ) -> ShellStateSnapshot? {
        let restoreURL = readablePersistenceURL(
            fileManager: fileManager,
            canonicalURL: persistenceURL,
            channel: channel
        )
        guard let restoreURL,
              let data = try? Data(contentsOf: restoreURL)
        else {
            return nil
        }

        if let contentState = try? JSONDecoder().decode(ShellContentStateSnapshot.self, from: data),
           let state = contentState.materializingShellState(),
           !state.spaces.isEmpty,
           !state.panes.isEmpty
        {
            return state
        }

        guard let state = try? JSONDecoder().decode(ShellStateSnapshot.self, from: data),
              !state.spaces.isEmpty,
              !state.panes.isEmpty
        else {
            return nil
        }
        return state
    }

    private static func restorePersistedWindowID(
        fileManager: FileManager,
        persistenceURL: URL,
        channel: AlanInstallChannel
    ) -> String? {
        let restoreURL = readablePersistenceURL(
            fileManager: fileManager,
            canonicalURL: persistenceURL,
            channel: channel
        )
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

    private static func persistenceDirectory(
        fileManager: FileManager,
        channel: AlanInstallChannel
    ) -> URL {
        let appSupportURL = alanMacApplicationSupportDirectory(fileManager: fileManager)
        return appSupportURL.appendingPathComponent(
            channel.applicationSupportDirectoryName,
            isDirectory: true
        )
    }

    private static func legacyPersistenceDirectories(
        fileManager: FileManager,
        channel: AlanInstallChannel
    ) -> [URL] {
        guard channel == .stable else {
            return []
        }

        let appSupportURL = alanMacApplicationSupportDirectory(fileManager: fileManager)
        return [
            appSupportURL.appendingPathComponent("AlanNative", isDirectory: true),
        ]
    }

    private static func readablePersistenceURL(
        fileManager: FileManager,
        canonicalURL: URL,
        channel: AlanInstallChannel
    ) -> URL? {
        if fileManager.fileExists(atPath: canonicalURL.path) {
            return canonicalURL
        }

        return legacyPersistenceDirectories(fileManager: fileManager, channel: channel)
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
