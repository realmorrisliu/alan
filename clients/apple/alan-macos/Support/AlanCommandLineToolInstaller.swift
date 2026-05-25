import Foundation

#if os(macOS)
func alanMacApplicationSupportDirectory(
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment
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

enum AlanInstallChannel: Equatable {
    case stable
    case dev

    static func current(
        bundleIdentifier: String? = Bundle.main.bundleIdentifier,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> AlanInstallChannel {
        if let channel = fromBundleIdentifier(bundleIdentifier) {
            return channel
        }
        if environment["ALAN_INSTALL_CHANNEL"] == "dev" {
            return .dev
        }
        return .stable
    }

    static func fromBundleIdentifier(_ bundleIdentifier: String?) -> AlanInstallChannel? {
        switch bundleIdentifier {
        case "app.alanworks.macos":
            return .stable
        case "app.alanworks.macos.dev":
            return .dev
        default:
            return nil
        }
    }

    var cliToolName: String {
        switch self {
        case .stable:
            return "alan"
        case .dev:
            return "alan-dev"
        }
    }

    var installChannelID: String {
        switch self {
        case .stable:
            return "stable"
        case .dev:
            return "dev"
        }
    }

    var bundleIdentifier: String {
        switch self {
        case .stable:
            return "app.alanworks.macos"
        case .dev:
            return "app.alanworks.macos.dev"
        }
    }

    var applicationSupportDirectoryName: String {
        switch self {
        case .stable:
            return "alan-macos"
        case .dev:
            return "alan-macos-dev"
        }
    }

    var shellControlNamespace: String {
        switch self {
        case .stable:
            return "alan-shell-control"
        case .dev:
            return "alan-dev-shell-control"
        }
    }

    var logSubsystem: String {
        bundleIdentifier
    }

    var toolNames: [String] {
        [cliToolName]
    }

    var ownedAppBundleNames: [String] {
        switch self {
        case .stable:
            return ["Alan.app", "alan.app"]
        case .dev:
            return ["Alan Dev.app"]
        }
    }
}

struct AlanCommandLineToolInstallRecord: Equatable {
    enum Status: Equatable {
        case installed
        case skipped(String)
    }

    let tool: String
    let sourcePath: String
    let targetPath: String
    let status: Status
}

enum AlanCommandLineToolInstaller {
    static let defaultInstallDirectory = URL(fileURLWithPath: "/usr/local/bin", isDirectory: true)
    static let toolNames = AlanInstallChannel.stable.toolNames

    static func embeddedBinDirectory(resourceURL: URL? = Bundle.main.resourceURL) -> URL? {
        resourceURL?.appendingPathComponent("bin", isDirectory: true)
    }

    static func install(
        targetDirectory: URL = defaultInstallDirectory,
        resourceURL: URL? = Bundle.main.resourceURL,
        fileManager: FileManager = .default,
        homebrewPrefixes: [String]? = nil,
        channel: AlanInstallChannel = .current()
    ) throws -> [AlanCommandLineToolInstallRecord] {
        guard let embeddedBinDirectory = embeddedBinDirectory(resourceURL: resourceURL) else {
            throw CocoaError(.fileNoSuchFile)
        }
        let toolNames = channel.toolNames

        let standardizedTargetPath = targetDirectory.standardizedFileURL.path
        if standardizedTargetPath.contains("/.alan/bin")
            || standardizedTargetPath.contains("/.alan-dev/bin")
        {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        if isHomebrewPrefixTarget(
            targetDirectory,
            fileManager: fileManager,
            homebrewPrefixes: homebrewPrefixes
        ) {
            throw CocoaError(.fileWriteNoPermission)
        }

        let existingHomebrewLinks = homebrewManagedCommandLinks(
            fileManager: fileManager,
            homebrewPrefixes: homebrewPrefixes,
            channel: channel
        )
        if !existingHomebrewLinks.isEmpty {
            return toolNames.map { tool in
                let source = embeddedBinDirectory.appendingPathComponent(tool)
                let target = existingHomebrewLinks[tool]
                    ?? targetDirectory.appendingPathComponent(tool).path
                return AlanCommandLineToolInstallRecord(
                    tool: tool,
                    sourcePath: source.path,
                    targetPath: target,
                    status: .skipped("Homebrew already manages alan command-line links.")
                )
            }
        }

        try fileManager.createDirectory(
            at: targetDirectory,
            withIntermediateDirectories: true
        )

        return try toolNames.map { tool in
            let source = embeddedBinDirectory.appendingPathComponent(tool)
            let target = targetDirectory.appendingPathComponent(tool)

            guard fileManager.isExecutableFile(atPath: source.path) else {
                throw CocoaError(.fileNoSuchFile)
            }

            if fileManager.fileExists(atPath: target.path) || isSymbolicLink(target, fileManager: fileManager) {
                guard isAlanOwnedLink(target, tool: tool, channel: channel, fileManager: fileManager) else {
                    return AlanCommandLineToolInstallRecord(
                        tool: tool,
                        sourcePath: source.path,
                        targetPath: target.path,
                        status: .skipped("Existing file is not an Alan.app command-line link.")
                    )
                }
                try fileManager.removeItem(at: target)
            }

            try fileManager.createSymbolicLink(
                at: target,
                withDestinationURL: source
            )

            return AlanCommandLineToolInstallRecord(
                tool: tool,
                sourcePath: source.path,
                targetPath: target.path,
                status: .installed
            )
        }
    }

    private static func isSymbolicLink(_ url: URL, fileManager: FileManager) -> Bool {
        guard let attributes = try? fileManager.attributesOfItem(atPath: url.path),
              let fileType = attributes[.type] as? FileAttributeType
        else {
            return false
        }
        return fileType == .typeSymbolicLink
    }

    private static func isAlanOwnedLink(
        _ url: URL,
        tool: String,
        channel: AlanInstallChannel,
        fileManager: FileManager
    ) -> Bool {
        guard isSymbolicLink(url, fileManager: fileManager),
              let destination = try? fileManager.destinationOfSymbolicLink(atPath: url.path)
        else {
            return false
        }

        return channel.ownedAppBundleNames.contains { bundleName in
            destination.hasSuffix("/\(bundleName)/Contents/Resources/bin/\(tool)")
        }
    }

    private static func isHomebrewPrefixTarget(
        _ targetDirectory: URL,
        fileManager: FileManager,
        homebrewPrefixes: [String]? = nil
    ) -> Bool {
        let targetPath = targetDirectory.standardizedFileURL.path + "/"
        let prefixes = homebrewPrefixes ?? [
            "/opt/homebrew",
            "/usr/local",
        ].filter { prefix in
            fileManager.fileExists(atPath: "\(prefix)/Homebrew")
                || fileManager.fileExists(atPath: "\(prefix)/bin/brew")
        }

        return prefixes.contains { prefix in
            targetPath.hasPrefix(prefix + "/")
        }
    }

    private static func homebrewManagedCommandLinks(
        fileManager: FileManager,
        homebrewPrefixes: [String]? = nil,
        channel: AlanInstallChannel
    ) -> [String: String] {
        let prefixes = resolvedHomebrewPrefixes(
            fileManager: fileManager,
            homebrewPrefixes: homebrewPrefixes
        )
        var links: [String: String] = [:]

        for prefix in prefixes {
            let binDirectory = URL(fileURLWithPath: prefix, isDirectory: true)
                .appendingPathComponent("bin", isDirectory: true)
            for tool in channel.toolNames {
                let link = binDirectory.appendingPathComponent(tool)
                if isHomebrewManagedCommandLink(
                    link,
                    tool: tool,
                    channel: channel,
                    homebrewPrefixes: prefixes,
                    fileManager: fileManager
                ) {
                    links[tool] = link.path
                }
            }
        }

        return links
    }

    private static func isHomebrewManagedCommandLink(
        _ url: URL,
        tool: String,
        channel: AlanInstallChannel,
        homebrewPrefixes: [String],
        fileManager: FileManager
    ) -> Bool {
        guard isSymbolicLink(url, fileManager: fileManager),
              let destination = try? fileManager.destinationOfSymbolicLink(atPath: url.path)
        else {
            return false
        }

        let destinationURL = URL(
            fileURLWithPath: destination,
            relativeTo: url.deletingLastPathComponent()
        )
        .resolvingSymlinksInPath()
        .standardizedFileURL
        let destinationPath = destinationURL.path

        guard channel.ownedAppBundleNames.contains(where: { bundleName in
            destinationPath.hasSuffix("/\(bundleName)/Contents/Resources/bin/\(tool)")
        }) else {
            return false
        }

        let appBundleURL = destinationURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return homebrewPrefixes.contains { prefix in
            isHomebrewManagedAppBundlePath(appBundleURL.path, prefix: prefix)
        }
    }

    private static func isHomebrewManagedAppBundlePath(_ path: String, prefix: String) -> Bool {
        path.hasPrefix(prefix + "/Caskroom/alan/")
            || path.hasPrefix(prefix + "/Cellar/alan/")
    }

    private static func resolvedHomebrewPrefixes(
        fileManager: FileManager,
        homebrewPrefixes: [String]? = nil
    ) -> [String] {
        if let homebrewPrefixes {
            return homebrewPrefixes.map { URL(fileURLWithPath: $0).standardizedFileURL.path }
        }

        return [
            "/opt/homebrew",
            "/usr/local",
        ].filter { prefix in
            fileManager.fileExists(atPath: "\(prefix)/Homebrew")
                || fileManager.fileExists(atPath: "\(prefix)/bin/brew")
        }
    }
}
#endif
