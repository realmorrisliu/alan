import Foundation

#if os(macOS)
struct ManagedTerminalAccountCatalogEntry: Codable, Equatable {
    let accountName: String
    let displayLabel: String
}

struct ManagedTerminalAccountCatalog: Codable, Equatable {
    let entries: [ManagedTerminalAccountCatalogEntry]

    static let empty = ManagedTerminalAccountCatalog(entries: [])

    var normalized: ManagedTerminalAccountCatalog {
        var entriesByAccount: [String: ManagedTerminalAccountCatalogEntry] = [:]
        for entry in entries {
            let accountName = entry.accountName.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !accountName.isEmpty else { continue }
            let label = entry.displayLabel.trimmingCharacters(in: .whitespacesAndNewlines)
            entriesByAccount[accountName] = ManagedTerminalAccountCatalogEntry(
                accountName: accountName,
                displayLabel: label.isEmpty ? accountName : label
            )
        }
        return ManagedTerminalAccountCatalog(
            entries: entriesByAccount.values.sorted { $0.accountName < $1.accountName }
        )
    }
}

struct ManagedTerminalAccountCatalogStore {
    let fileManager: FileManager
    let storeURL: URL

    init(fileManager: FileManager = .default, storeURL: URL) {
        self.fileManager = fileManager
        self.storeURL = storeURL
    }

    static func defaultStore(
        channelApplicationSupportDirectoryName: String =
            TerminalProfileStore.currentChannelApplicationSupportDirectoryName(),
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> ManagedTerminalAccountCatalogStore {
        let profileStore = TerminalProfileStore.defaultStore(
            channelApplicationSupportDirectoryName: channelApplicationSupportDirectoryName,
            fileManager: fileManager,
            environment: environment
        )
        return ManagedTerminalAccountCatalogStore(
            fileManager: fileManager,
            storeURL: profileStore.storeURL
                .deletingLastPathComponent()
                .appendingPathComponent("managed-terminal-users.json", isDirectory: false)
        )
    }

    func load() -> ManagedTerminalAccountCatalog {
        guard fileManager.fileExists(atPath: storeURL.path),
              let data = try? Data(contentsOf: storeURL),
              let catalog = try? JSONDecoder().decode(ManagedTerminalAccountCatalog.self, from: data)
        else {
            return .empty
        }
        return catalog.normalized
    }

    func upsert(_ entry: ManagedTerminalAccountCatalogEntry) throws {
        var entries = load().entries.filter { $0.accountName != entry.accountName }
        entries.append(entry)
        try save(ManagedTerminalAccountCatalog(entries: entries).normalized)
    }

    func remove(accountName: String) throws {
        let entries = load().entries.filter { $0.accountName != accountName }
        try save(ManagedTerminalAccountCatalog(entries: entries))
    }

    private func save(_ catalog: ManagedTerminalAccountCatalog) throws {
        try fileManager.createDirectory(
            at: storeURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(catalog.normalized)
        try data.write(to: storeURL, options: .atomic)
    }
}
#endif
