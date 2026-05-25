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
        homebrewPrefixes: [String]? = nil,
        fileManager: FileManager = .default
    ) -> Bool {
        let appPath = appBundleURL.standardizedFileURL.path
        let prefixes = resolvedHomebrewPrefixes(
            fileManager: fileManager,
            homebrewPrefixes: homebrewPrefixes
        )

        if prefixes.contains(where: { prefix in
            isHomebrewManagedAppBundlePath(appPath, prefix: prefix)
        }) {
            return true
        }

        let resolvedAppPath = appBundleURL
            .resolvingSymlinksInPath()
            .standardizedFileURL
            .path
        if resolvedAppPath != appPath,
           prefixes.contains(where: { prefix in
               isHomebrewManagedAppBundlePath(resolvedAppPath, prefix: prefix)
           })
        {
            return true
        }

        return false
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
