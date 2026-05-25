import Foundation

#if os(macOS)
enum AlanMacUpdateInstallation: Equatable {
    case direct
    case homebrewManaged
    case unsupportedChannel
}

struct AlanMacUpdateDecision: Equatable {
    let installation: AlanMacUpdateInstallation
    let allowsSparkleUpdates: Bool
    let menuTitle: String
    let userMessage: String
}

enum AlanMacUpdatePolicy {
    static func decision(
        appBundleURL: URL = Bundle.main.bundleURL,
        channel: AlanInstallChannel = .current(),
        homebrewPrefixes: [String]? = nil,
        fileManager: FileManager = .default
    ) -> AlanMacUpdateDecision {
        if channel != .stable {
            return AlanMacUpdateDecision(
                installation: .unsupportedChannel,
                allowsSparkleUpdates: false,
                menuTitle: "Check for Updates...",
                userMessage: "This local dev build does not use Sparkle updates."
            )
        }

        if isHomebrewManaged(
            appBundleURL: appBundleURL,
            channel: channel,
            homebrewPrefixes: homebrewPrefixes,
            fileManager: fileManager
        ) {
            return AlanMacUpdateDecision(
                installation: .homebrewManaged,
                allowsSparkleUpdates: false,
                menuTitle: "Check for Updates...",
                userMessage: "This Alan.app is managed by Homebrew. Update it with brew upgrade --cask alan."
            )
        }

        return AlanMacUpdateDecision(
            installation: .direct,
            allowsSparkleUpdates: true,
            menuTitle: "Check for Updates...",
            userMessage: ""
        )
    }

    static func isHomebrewManaged(
        appBundleURL: URL,
        channel: AlanInstallChannel = .stable,
        homebrewPrefixes: [String]? = nil,
        fileManager: FileManager = .default
    ) -> Bool {
        let appPath = appBundleURL.standardizedFileURL.path
        let prefixes = resolvedHomebrewPrefixes(
            fileManager: fileManager,
            homebrewPrefixes: homebrewPrefixes
        )

        if prefixes.contains(where: { prefix in
            appPath.hasPrefix(prefix + "/Caskroom/alan/")
                || appPath.hasPrefix(prefix + "/Cellar/alan/")
        }) {
            return true
        }

        let embeddedToolPaths = channel.toolNames.map { tool in
            appBundleURL
                .appendingPathComponent("Contents", isDirectory: true)
                .appendingPathComponent("Resources", isDirectory: true)
                .appendingPathComponent("bin", isDirectory: true)
                .appendingPathComponent(tool)
                .standardizedFileURL
                .path
        }

        for prefix in prefixes {
            let binDirectory = URL(fileURLWithPath: prefix, isDirectory: true)
                .appendingPathComponent("bin", isDirectory: true)
            for tool in channel.toolNames {
                let link = binDirectory.appendingPathComponent(tool)
                guard let destination = try? fileManager.destinationOfSymbolicLink(atPath: link.path)
                else {
                    continue
                }

                let resolvedDestination = URL(fileURLWithPath: destination, relativeTo: binDirectory)
                    .standardizedFileURL
                    .path
                if embeddedToolPaths.contains(resolvedDestination) {
                    return true
                }
            }
        }

        return false
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
