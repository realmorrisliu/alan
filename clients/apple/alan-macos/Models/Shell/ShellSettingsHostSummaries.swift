import Foundation

#if os(macOS)
struct ShellSettingsLocalSummary: Equatable {
    let channel: AlanInstallChannel
    let appDisplayName: String
    let appBundleName: String
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let updateSummary: String
    let updateDetail: String
    let systemStoreDisplayPath: String
    let applicationSupportDisplayPath: String
    let shellControlNamespace: String

    static func current(
        channel: AlanInstallChannel = .current(),
        updateDecision: AlanMacUpdateDecision = AlanMacUpdatePolicy.decision(),
        homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> ShellSettingsLocalSummary {
        return ShellSettingsLocalSummary(
            channel: channel,
            appDisplayName: channel.appDisplayName,
            appBundleName: channel.appBundleName,
            bundleIdentifier: channel.bundleIdentifier,
            channelLabel: channel.settingsChannelLabel,
            cliToolName: channel.cliToolName,
            updateSummary: updateSummary(for: updateDecision),
            updateDetail: updateDetail(for: updateDecision),
            systemStoreDisplayPath: channel.systemStoreDisplayPath(homeDirectory: homeDirectory),
            applicationSupportDisplayPath: channel.applicationSupportDisplayPath(
                homeDirectory: homeDirectory
            ),
            shellControlNamespace: channel.shellControlNamespace
        )
    }

    private static func updateSummary(for decision: AlanMacUpdateDecision) -> String {
        switch decision.installation {
        case .direct:
            return decision.allowsSparkleUpdates ? "Sparkle updates available" : "Manual updates"
        case .homebrewManaged:
            return "Homebrew managed"
        case .unsupportedChannel:
            return "Manual local build"
        }
    }

    private static func updateDetail(for decision: AlanMacUpdateDecision) -> String {
        let trimmed = decision.userMessage.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return trimmed
        }
        return "Use \(decision.menuTitle) for this install."
    }
}

struct ShellSettingsDiagnosticsSummary: Equatable {
    let isEnabled: Bool
    let retainedEventCount: Int
    let stutterMarkerCount: Int
    let lastExportURL: URL?

    static let disabled = ShellSettingsDiagnosticsSummary(
        isEnabled: false,
        retainedEventCount: 0,
        stutterMarkerCount: 0,
        lastExportURL: nil
    )

    var exportDetail: String {
        if retainedEventCount == 0 {
            return isEnabled
                ? "Exports the retained local trace after activity is captured."
                : "Enable diagnostics to retain recent local performance events."
        }

        let markerLabel = stutterMarkerCount == 1 ? "marker" : "markers"
        return "\(retainedEventCount) retained events, \(stutterMarkerCount) stutter \(markerLabel)."
    }
}

private extension AlanInstallChannel {
    var appDisplayName: String {
        switch self {
        case .stable:
            return "Alan"
        case .dev:
            return "Alan Dev"
        }
    }

    var appBundleName: String {
        switch self {
        case .stable:
            return "Alan.app"
        case .dev:
            return "Alan Dev.app"
        }
    }

    var settingsChannelLabel: String {
        switch self {
        case .stable:
            return "Stable"
        case .dev:
            return "Dev"
        }
    }

    func systemStoreDisplayPath(homeDirectory: URL) -> String {
        let suffix = "Library/Application Support/Alan/System Store/\(installChannelID)"
        let homePath = homeDirectory.standardizedFileURL.path
        if homePath == "/" {
            return "/\(suffix)"
        }
        return "~/" + suffix
    }

    func applicationSupportDisplayPath(homeDirectory: URL) -> String {
        let suffix = "Library/Application Support/\(applicationSupportDirectoryName)"
        let homePath = homeDirectory.standardizedFileURL.path
        if homePath == "/" {
            return "/\(suffix)"
        }
        return "~/" + suffix
    }

}
#endif
