import Foundation

struct TerminalProfileStore {
    let fileManager: FileManager
    let storeURL: URL

    init(fileManager: FileManager = .default, storeURL: URL) {
        self.fileManager = fileManager
        self.storeURL = storeURL
    }

    static func defaultStore(
        channelApplicationSupportDirectoryName: String = currentChannelApplicationSupportDirectoryName(),
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> TerminalProfileStore {
        let supportRoot = localApplicationSupportDirectory(
            fileManager: fileManager,
            environment: environment
        )
        let storeURL = supportRoot
            .appendingPathComponent(channelApplicationSupportDirectoryName, isDirectory: true)
            .appendingPathComponent("terminal-profiles.json", isDirectory: false)
        return TerminalProfileStore(fileManager: fileManager, storeURL: storeURL)
    }

    static func currentChannelApplicationSupportDirectoryName(
        bundleIdentifier: String? = Bundle.main.bundleIdentifier,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> String {
        if bundleIdentifier == "app.alanworks.macos.dev" || environment["ALAN_INSTALL_CHANNEL"] == "dev" {
            return "alan-macos-dev"
        }
        return "alan-macos"
    }

    private static func localApplicationSupportDirectory(
        fileManager: FileManager,
        environment: [String: String]
    ) -> URL {
        if let override = environment["ALAN_MACOS_APPLICATION_SUPPORT_DIR"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !override.isEmpty
        {
            return URL(fileURLWithPath: NSString(string: override).expandingTildeInPath, isDirectory: true)
        }

        return fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? fileManager.temporaryDirectory
    }
}

func terminalProfileIDForGlobalDefaultPaneCapture(
    channelApplicationSupportDirectoryName: String = TerminalProfileStore
        .currentChannelApplicationSupportDirectoryName(),
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> String? {
    let document = TerminalProfileStore.defaultStore(
        channelApplicationSupportDirectoryName: channelApplicationSupportDirectoryName,
        fileManager: fileManager,
        environment: environment
    ).load().document
    guard let profile = document.defaultProfile else {
        return nil
    }
    do {
        return try ShellCoreFFIAdapter.shared.shouldCaptureGlobalDefaultTerminalProfile(profile)
            ? profile.id
            : nil
    } catch {
        return nil
    }
}

enum TerminalProfileValidator {
    static func validate(
        _ document: TerminalProfileDocument,
        fileManager: FileManager? = nil
    ) -> TerminalProfileValidationResult {
        do {
            return try ShellCoreFFIAdapter.shared.validateTerminalProfileDocument(
                document,
                executablePaths: executablePaths(from: fileManager),
                enforceExecutableAvailability: fileManager != nil
            )
        } catch {
            return TerminalProfileValidationResult(errors: [.coreUnavailable("\(error)")])
        }
    }

    private static func executablePaths(from fileManager: FileManager?) -> Set<String> {
        guard let fileManager else { return [] }
        return Set(
            ["/usr/bin/sudo", "/bin/zsh"].filter { fileManager.isExecutableFile(atPath: $0) }
        )
    }
}

enum TerminalProfileEditor {
    static func makeDefinition(from draft: TerminalProfileEditorDraft) -> TerminalProfileEditorResult {
        do {
            return try ShellCoreFFIAdapter.shared.makeTerminalProfileDefinition(from: draft)
        } catch {
            return TerminalProfileEditorResult(
                definition: nil,
                errors: [.coreUnavailable("\(error)")]
            )
        }
    }

    static func upserting(
        draft: TerminalProfileEditorDraft,
        into document: TerminalProfileDocument
    ) -> TerminalProfileDocumentEditorResult {
        do {
            return try ShellCoreFFIAdapter.shared.upsertTerminalProfileDraft(draft, into: document)
        } catch {
            return TerminalProfileDocumentEditorResult(
                document: nil,
                errors: [.coreUnavailable("\(error)")]
            )
        }
    }
}

extension TerminalProfileStore {
    func load() -> TerminalProfileLoadResult {
        guard fileManager.fileExists(atPath: storeURL.path) else {
            return TerminalProfileLoadResult(document: .fallback, recovery: nil)
        }

        do {
            let data = try Data(contentsOf: storeURL)
            let document = try JSONDecoder().decode(TerminalProfileDocument.self, from: data)
            let validation = TerminalProfileValidator.validate(document)
            guard validation.isValid else {
                return TerminalProfileLoadResult(document: .fallback, recovery: nil)
            }
            return TerminalProfileLoadResult(document: document, recovery: nil)
        } catch {
            let evidenceURL = quarantineCorruptStore()
            return TerminalProfileLoadResult(
                document: .fallback,
                recovery: TerminalProfileStoreRecovery(
                    kind: .corruptStoreQuarantined,
                    evidenceURL: evidenceURL
                )
            )
        }
    }

    func save(_ document: TerminalProfileDocument) throws {
        let validation = TerminalProfileValidator.validate(document)
        guard validation.isValid else {
            throw TerminalProfileStoreError.invalidDocument(validation.errors)
        }
        try fileManager.createDirectory(
            at: storeURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(document)
        try data.write(to: storeURL, options: .atomic)
    }

    private func quarantineCorruptStore() -> URL {
        let stamp = ISO8601DateFormatter()
            .string(from: Date())
            .replacingOccurrences(of: ":", with: "-")
        let evidenceURL = storeURL
            .deletingLastPathComponent()
            .appendingPathComponent("terminal-profiles.corrupt-\(stamp).json")
        do {
            if fileManager.fileExists(atPath: evidenceURL.path) {
                try fileManager.removeItem(at: evidenceURL)
            }
            try fileManager.moveItem(at: storeURL, to: evidenceURL)
        } catch {
            try? fileManager.copyItem(at: storeURL, to: evidenceURL)
        }
        return evidenceURL
    }
}
