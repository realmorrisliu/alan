import Foundation

#if os(macOS)
enum ShellSettingsSectionID: String, CaseIterable, Equatable {
    case interface
    case terminalProfiles
    case terminalAccounts
    case accounts
    case sessions
    case capabilities
    case local

    static let defaultOrder: [ShellSettingsSectionID] = [
        .interface,
        .terminalProfiles,
        .terminalAccounts,
        .accounts,
        .sessions,
        .capabilities,
        .local,
    ]

    var title: String {
        switch self {
        case .interface:
            return "Interface"
        case .terminalProfiles:
            return "Terminal Profiles"
        case .terminalAccounts:
            return "Terminal Accounts"
        case .accounts:
            return "Accounts"
        case .sessions:
            return "Sessions"
        case .capabilities:
            return "Capabilities"
        case .local:
            return "Local"
        }
    }
}

enum ShellSettingsRowMutability: Equatable {
    case editable
    case readOnly
    case actionOnly
    case deferred
}

struct ShellSettingsRowModel: Identifiable, Equatable {
    let id: String
    let systemName: String
    let title: String
    let detail: String?
    let value: String?
    let mutability: ShellSettingsRowMutability
    let offersFreeformEditing: Bool

    init(
        id: String,
        systemName: String,
        title: String,
        detail: String? = nil,
        value: String? = nil,
        mutability: ShellSettingsRowMutability = .readOnly,
        offersFreeformEditing: Bool = false
    ) {
        self.id = id
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.value = value
        self.mutability = mutability
        self.offersFreeformEditing = offersFreeformEditing
    }

    var visibleText: [String] {
        [title, value, detail].compactMap { text in
            let trimmed = text?.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmed?.isEmpty == false ? trimmed : nil
        }
    }
}

struct ShellSettingsSectionModel: Identifiable, Equatable {
    let id: ShellSettingsSectionID
    let rows: [ShellSettingsRowModel]

    var title: String { id.title }

    var visibleText: [String] {
        [title] + rows.flatMap(\.visibleText)
    }
}

struct ShellSettingsSurfaceSnapshot: Equatable {
    let sections: [ShellSettingsSectionModel]

    static func make(
        remote: ShellSettingsRemoteSnapshot,
        local: ShellSettingsLocalSummary,
        terminalProfiles: TerminalProfileSettingsSummary = .current(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary = .empty
    ) -> ShellSettingsSurfaceSnapshot {
        ShellSettingsSurfaceSnapshot(
            sections: [
                ShellSettingsSectionModel(id: .interface, rows: interfaceRows()),
                ShellSettingsSectionModel(
                    id: .terminalProfiles,
                    rows: terminalProfileRows(terminalProfiles)
                ),
                ShellSettingsSectionModel(
                    id: .terminalAccounts,
                    rows: managedTerminalAccountRows(managedTerminalAccounts)
                ),
                ShellSettingsSectionModel(id: .accounts, rows: accountRows(remote.accounts)),
                ShellSettingsSectionModel(id: .sessions, rows: sessionRows()),
                ShellSettingsSectionModel(id: .capabilities, rows: capabilityRows(remote.capabilities)),
                ShellSettingsSectionModel(id: .local, rows: localRows(local)),
            ]
        )
    }

    var visibleText: [String] {
        sections.flatMap(\.visibleText)
    }

    private static func interfaceRows() -> [ShellSettingsRowModel] {
        [
            ShellSettingsRowModel(
                id: "appearance",
                systemName: "circle.lefthalf.filled",
                title: "Appearance",
                mutability: .editable
            ),
            ShellSettingsRowModel(
                id: "sidebar",
                systemName: "sidebar.left",
                title: "Sidebar",
                mutability: .editable
            ),
            ShellSettingsRowModel(
                id: "inactiveSplitDimming",
                systemName: "rectangle.split.2x1",
                title: "Inactive split dimming",
                mutability: .editable
            ),
        ]
    }

    private static func terminalProfileRows(
        _ summary: TerminalProfileSettingsSummary
    ) -> [ShellSettingsRowModel] {
        var rows: [ShellSettingsRowModel] = [
            ShellSettingsRowModel(
                id: "terminalProfilesDefault",
                systemName: "terminal",
                title: "Default Terminal Profile",
                detail: "Local startup identity for new terminal content.",
                value: summary.defaultProfileTitle ?? "Login shell",
                mutability: .editable
            ),
            ShellSettingsRowModel(
                id: "terminalProfilesCreate",
                systemName: "plus.circle",
                title: "New Terminal Profile",
                detail: "Structured launch mode with local validation.",
                value: "Create",
                mutability: .actionOnly
            )
        ]

        if let recoveryMessage = summary.recoveryMessage {
            rows.append(
                ShellSettingsRowModel(
                    id: "terminalProfilesRecovery",
                    systemName: "exclamationmark.triangle",
                    title: "Profile store recovery",
                    detail: recoveryMessage,
                    value: "Fallback active"
                )
            )
        }

        rows.append(
            contentsOf: summary.profiles.map { profile in
                ShellSettingsRowModel(
                    id: "terminalProfile.\(profile.id)",
                    systemName: terminalProfileSystemName(profile),
                    title: profile.title,
                    detail: profile.redactedDisplayDetail,
                    value: profile.id == summary.defaultProfileID ? "Default" : profile.launch.kind.rawValue,
                    mutability: .editable
                )
            }
        )
        rows.append(
            ShellSettingsRowModel(
                id: "terminalProfilesSudoGuidance",
                systemName: "lock.shield",
                title: "Sudo behavior",
                detail: "Prompts and passwordless sudo are controlled by macOS sudo policy.",
                value: "System managed"
            )
        )
        return rows
    }

    private static func managedTerminalAccountRows(
        _ summary: ManagedTerminalAccountSettingsSummary
    ) -> [ShellSettingsRowModel] {
        guard !summary.plans.isEmpty else {
            return [
                ShellSettingsRowModel(
                    id: "terminalAccountProvision",
                    systemName: "person.crop.circle.badge.plus",
                    title: "Managed terminal account",
                    detail: "Create a terminal-only local user for passwordless terminal entry.",
                    value: "Preview plan",
                    mutability: .actionOnly
                ),
                ShellSettingsRowModel(
                    id: "terminalAccountLoginBoundary",
                    systemName: "macwindow.badge.plus",
                    title: "Mac login session",
                    detail: "This flow leaves the Mac login session setting unchanged.",
                    value: "Not changed"
                ),
            ]
        }

        return summary.plans.map { plan in
            ShellSettingsRowModel(
                id: "terminalAccount.\(plan.request.accountName)",
                systemName: terminalAccountSystemName(plan),
                title: "Managed terminal account",
                detail: terminalAccountDetail(plan),
                value: terminalAccountStatusLabel(plan),
                mutability: .actionOnly
            )
        }
    }

    private static func terminalProfileSystemName(_ profile: TerminalProfileDefinition) -> String {
        switch profile.launch {
        case .loginShell:
            return "terminal"
        case .sudoUser:
            return "person.crop.circle"
        case .sudoRoot:
            return "exclamationmark.triangle"
        case .customCommand:
            return "chevron.left.forwardslash.chevron.right"
        }
    }

    private static func terminalAccountSystemName(_ plan: ManagedTerminalAccountPlan) -> String {
        switch plan.status {
        case .alreadyReady:
            return "checkmark.seal"
        case .repair:
            return "wrench.and.screwdriver"
        case .requiresDestructiveConfirmation, .invalid, .sudoersConflict, .terminalProfileConflict:
            return "exclamationmark.triangle"
        case .readyToApply:
            return "person.crop.circle.badge.plus"
        }
    }

    private static func terminalAccountStatusLabel(_ plan: ManagedTerminalAccountPlan) -> String {
        switch plan.status {
        case .alreadyReady:
            return "Ready"
        case .repair:
            return "Repairable"
        case .invalid:
            return "Invalid"
        case .requiresDestructiveConfirmation:
            return "Confirm"
        case .sudoersConflict, .terminalProfileConflict:
            return "Conflict"
        case .readyToApply:
            return "Preview"
        }
    }

    private static func terminalAccountDetail(_ plan: ManagedTerminalAccountPlan) -> String {
        let target = plan.request.accountName
        switch plan.status {
        case .alreadyReady:
            return "\(target) is ready for terminal entry and linked to its Terminal Profile."
        case .repair:
            return "\(target) needs repair before terminal entry is ready."
        case .invalid:
            return "\(target) needs a valid local account identifier."
        case .requiresDestructiveConfirmation:
            return "\(target) rollback needs separate destructive confirmation."
        case .sudoersConflict(let path):
            return "\(target) has an existing non-Alan sudoers file at \(path)."
        case .terminalProfileConflict(let profileID):
            return "\(target) has an existing non-Alan Terminal Profile named \(profileID)."
        case .readyToApply:
            return "\(target) terminal entry plan is ready for explicit confirmation."
        }
    }

    private static func accountRows(
        _ summary: ShellSettingsAccountsSummary
    ) -> [ShellSettingsRowModel] {
        if let reason = summary.compactUnavailableReason {
            return [
                unavailableRow(
                    id: "accountsUnavailable",
                    systemName: "person.crop.circle.badge.exclamationmark",
                    title: "Connection profile",
                    reason: reason
                ),
            ]
        }

        guard let profile = summary.effectiveProfile else {
            return [
                ShellSettingsRowModel(
                    id: "selectedProfile",
                    systemName: "person.crop.circle",
                    title: "Selected profile",
                    detail: "Add one with alan connection add.",
                    value: "Not configured"
                ),
                ShellSettingsRowModel(
                    id: "accountActions",
                    systemName: "arrowshape.turn.up.right",
                    title: "Account actions",
                    detail: "Login, key entry, and connection tests run through alan connection.",
                    value: "Command line"
                ),
            ]
        }

        let provider = summary.provider(for: profile.provider)
        return [
            ShellSettingsRowModel(
                id: "selectedProfile",
                systemName: "person.crop.circle",
                title: "Selected profile",
                detail: "Profile ID: \(profile.profileID)",
                value: profile.displayName
            ),
            ShellSettingsRowModel(
                id: "provider",
                systemName: "network",
                title: "Provider",
                detail: profile.provider,
                value: provider?.displayName ?? displayProviderName(profile.provider)
            ),
            ShellSettingsRowModel(
                id: "model",
                systemName: "cpu",
                title: "Model",
                detail: "Resolved by the connection profile.",
                value: profile.modelDisplayValue
            ),
            ShellSettingsRowModel(
                id: "credential",
                systemName: "key",
                title: "Credential",
                detail: "Secret values stay in the host credential store.",
                value: displayCredentialStatus(profile.credentialStatus)
            ),
            ShellSettingsRowModel(
                id: "accountActions",
                systemName: "arrowshape.turn.up.right",
                title: "Account actions",
                detail: "Login, key entry, and connection tests run through alan connection.",
                value: provider?.supportedActionLabel ?? "Command line",
                mutability: .actionOnly
            ),
        ]
    }

    private static func sessionRows() -> [ShellSettingsRowModel] {
        [
            ShellSettingsRowModel(
                id: "governance",
                systemName: "checkmark.shield",
                title: "Governance",
                detail: "Default new-session policy.",
                value: "Conservative"
            ),
            ShellSettingsRowModel(
                id: "reasoningEffort",
                systemName: "brain.head.profile",
                title: "Reasoning effort",
                detail: "Uses model defaults when no session override is selected.",
                value: "Model default"
            ),
            ShellSettingsRowModel(
                id: "streamingMode",
                systemName: "dot.radiowaves.left.and.right",
                title: "Streaming",
                detail: "New sessions follow the daemon default.",
                value: "Auto"
            ),
            ShellSettingsRowModel(
                id: "recoveryMode",
                systemName: "arrow.clockwise",
                title: "Stream recovery",
                detail: "Partial stream recovery continues once by default.",
                value: "Continue once"
            ),
        ]
    }

    private static func capabilityRows(
        _ summary: ShellSettingsCapabilitiesSummary
    ) -> [ShellSettingsRowModel] {
        if let reason = summary.compactUnavailableReason {
            return [
                unavailableRow(
                    id: "capabilitiesUnavailable",
                    systemName: "puzzlepiece.extension",
                    title: "Skill catalog",
                    reason: reason
                ),
            ]
        }

        let total = summary.skills.count
        let enabled = summary.skills.filter(\.enabled).count
        let implicit = summary.skills.filter(\.allowImplicitInvocation).count
        let unavailable = summary.skills.filter { !$0.available }.count

        return [
            ShellSettingsRowModel(
                id: "enabledSkills",
                systemName: "puzzlepiece.extension",
                title: "Enabled skills",
                detail: "Resolved from the skill catalog.",
                value: total == 0 ? "No skills" : "\(enabled) of \(total)"
            ),
            ShellSettingsRowModel(
                id: "implicitInvocation",
                systemName: "sparkle.magnifyingglass",
                title: "Implicit invocation",
                detail: "Shows how many skills may be invoked without an explicit mention.",
                value: "\(implicit) allowed"
            ),
            ShellSettingsRowModel(
                id: "unavailableSkills",
                systemName: "exclamationmark.triangle",
                title: "Unavailable skills",
                detail: "Unavailable packages stay visible without blocking Settings.",
                value: unavailable == 0 ? "None" : "\(unavailable)"
            ),
        ]
    }

    private static func localRows(_ local: ShellSettingsLocalSummary) -> [ShellSettingsRowModel] {
        [
            ShellSettingsRowModel(
                id: "appIdentity",
                systemName: "app",
                title: "App",
                detail: local.bundleIdentifier,
                value: local.appDisplayName
            ),
            ShellSettingsRowModel(
                id: "installChannel",
                systemName: "shippingbox",
                title: "Install channel",
                detail: local.appBundleName,
                value: local.channelLabel
            ),
            ShellSettingsRowModel(
                id: "cliTool",
                systemName: "terminal",
                title: "Command line tool",
                detail: "Installed into /usr/local/bin when requested.",
                value: local.cliToolName
            ),
            ShellSettingsRowModel(
                id: "daemonEndpoint",
                systemName: "server.rack",
                title: "Daemon endpoint",
                detail: "Bind \(local.daemonBindAddress)",
                value: local.daemonURL
            ),
            ShellSettingsRowModel(
                id: "updates",
                systemName: "arrow.down.circle",
                title: "Updates",
                detail: local.updateDetail,
                value: local.updateSummary
            ),
            ShellSettingsRowModel(
                id: "dataRoot",
                systemName: "folder",
                title: "Alan home",
                detail: "Credentials and auth are stored separately; values are not shown.",
                value: local.alanHomeDisplayPath
            ),
            ShellSettingsRowModel(
                id: "publicSkills",
                systemName: "folder.badge.gearshape",
                title: "Public skills",
                detail: "Package install root for this channel.",
                value: local.globalSkillsDisplayPath
            ),
            ShellSettingsRowModel(
                id: "applicationSupport",
                systemName: "externaldrive",
                title: "Shell state",
                detail: "Workspace manifests and local shell state.",
                value: local.applicationSupportDisplayPath
            ),
            ShellSettingsRowModel(
                id: "shellControl",
                systemName: "point.3.connected.trianglepath.dotted",
                title: "Shell control",
                detail: "Local control namespace.",
                value: local.shellControlNamespace
            ),
        ]
    }

    private static func unavailableRow(
        id: String,
        systemName: String,
        title: String,
        reason: String
    ) -> ShellSettingsRowModel {
        ShellSettingsRowModel(
            id: id,
            systemName: systemName,
            title: title,
            detail: sanitizedUnavailableReason(reason),
            value: "Unavailable"
        )
    }

    private static func sanitizedUnavailableReason(_ reason: String) -> String {
        let trimmed = reason.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return "The daemon did not return this summary."
        }
        if trimmed.contains("\n") || trimmed.contains("{") || trimmed.contains("}") {
            return "The daemon did not return this summary."
        }
        return trimmed
    }

    private static func displayCredentialStatus(_ status: String) -> String {
        switch status {
        case "available":
            return "Available"
        case "missing":
            return "Missing"
        case "pending":
            return "Pending"
        case "expired":
            return "Expired"
        case "error":
            return "Needs attention"
        default:
            return "Unknown"
        }
    }

    private static func displayProviderName(_ provider: String) -> String {
        provider
            .split(separator: "_")
            .map { part in
                part.prefix(1).uppercased() + part.dropFirst()
            }
            .joined(separator: " ")
    }
}

struct ShellSettingsRemoteSnapshot: Equatable {
    let accounts: ShellSettingsAccountsSummary
    let capabilities: ShellSettingsCapabilitiesSummary

    static func unavailable(reason: String) -> ShellSettingsRemoteSnapshot {
        ShellSettingsRemoteSnapshot(
            accounts: .unavailable(reason: reason),
            capabilities: .unavailable(reason: reason)
        )
    }
}

struct TerminalProfileSettingsSummary: Equatable {
    let profiles: [TerminalProfileDefinition]
    let defaultProfileID: String
    let recoveryMessage: String?

    static func current(
        store: TerminalProfileStore = .defaultStore()
    ) -> TerminalProfileSettingsSummary {
        let load = store.load()
        return TerminalProfileSettingsSummary(
            profiles: load.profiles,
            defaultProfileID: load.document.defaultProfileID,
            recoveryMessage: load.recovery.map { _ in
                "The local Terminal Profile store was unreadable and has been preserved."
            }
        )
    }

    var defaultProfileTitle: String? {
        profiles.first { $0.id == defaultProfileID }?.title
    }
}

struct ManagedTerminalAccountSettingsSummary: Equatable {
    let plans: [ManagedTerminalAccountPlan]

    static let empty = ManagedTerminalAccountSettingsSummary(plans: [])
}

struct ShellSettingsAccountsSummary: Equatable {
    let current: ShellSettingsConnectionSelection?
    let profiles: [ShellSettingsConnectionProfile]
    let providers: [ShellSettingsConnectionProvider]
    let unavailableReason: String?

    static func unavailable(reason: String) -> ShellSettingsAccountsSummary {
        ShellSettingsAccountsSummary(
            current: nil,
            profiles: [],
            providers: [],
            unavailableReason: reason
        )
    }

    var compactUnavailableReason: String? {
        unavailableReason
    }

    var effectiveProfile: ShellSettingsConnectionProfile? {
        if let effectiveProfile = current?.effectiveProfile {
            return profiles.first { $0.profileID == effectiveProfile }
        }
        if let defaultProfile = current?.defaultProfile {
            return profiles.first { $0.profileID == defaultProfile }
        }
        return profiles.first { $0.isDefault } ?? profiles.first
    }

    func provider(for providerID: String) -> ShellSettingsConnectionProvider? {
        providers.first { $0.providerID == providerID }
    }
}

struct ShellSettingsConnectionSelection: Equatable {
    let defaultProfile: String?
    let effectiveProfile: String?
    let effectiveSource: String
}

struct ShellSettingsConnectionProfile: Equatable {
    let profileID: String
    let label: String?
    let provider: String
    let credentialStatus: String
    let settings: [String: String]
    let isDefault: Bool

    var displayName: String {
        let trimmedLabel = label?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let trimmedLabel, !trimmedLabel.isEmpty {
            return trimmedLabel
        }
        return profileID
    }

    var modelDisplayValue: String {
        sanitizedSettingValue(for: "model") ?? "Model default"
    }

    private func sanitizedSettingValue(for key: String) -> String? {
        guard !Self.isSensitiveSettingKey(key),
              let value = settings[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty
        else {
            return nil
        }
        return value
    }

    private static func isSensitiveSettingKey(_ key: String) -> Bool {
        let normalized = key.lowercased()
        return normalized.contains("key")
            || normalized.contains("token")
            || normalized.contains("secret")
            || normalized.contains("credential")
            || normalized.contains("authorization")
    }
}

struct ShellSettingsConnectionProvider: Equatable {
    let providerID: String
    let displayName: String
    let supportsBrowserLogin: Bool
    let supportsDeviceLogin: Bool
    let supportsSecretEntry: Bool
    let supportsLogout: Bool
    let supportsTest: Bool

    var supportedActionLabel: String {
        var actions: [String] = []
        if supportsBrowserLogin || supportsDeviceLogin {
            actions.append("Login")
        }
        if supportsSecretEntry {
            actions.append("Set Key")
        }
        if supportsTest {
            actions.append("Test")
        }
        if supportsLogout {
            actions.append("Logout")
        }
        return actions.isEmpty ? "Command line" : actions.joined(separator: ", ")
    }
}

struct ShellSettingsCapabilitiesSummary: Equatable {
    let skills: [ShellSettingsSkillSummary]
    let unavailableReason: String?

    static func unavailable(reason: String) -> ShellSettingsCapabilitiesSummary {
        ShellSettingsCapabilitiesSummary(skills: [], unavailableReason: reason)
    }

    var compactUnavailableReason: String? {
        unavailableReason
    }
}

struct ShellSettingsSkillSummary: Equatable {
    let id: String
    let name: String
    let enabled: Bool
    let allowImplicitInvocation: Bool
    let available: Bool
}

struct ShellSettingsLocalSummary: Equatable {
    let channel: AlanInstallChannel
    let appDisplayName: String
    let appBundleName: String
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let daemonURL: String
    let daemonBindAddress: String
    let updateSummary: String
    let updateDetail: String
    let alanHomeDisplayPath: String
    let applicationSupportDisplayPath: String
    let globalSkillsDisplayPath: String
    let shellControlNamespace: String

    static func current(
        channel: AlanInstallChannel = .current(),
        environment: [String: String] = ProcessInfo.processInfo.environment,
        updateDecision: AlanMacUpdateDecision = AlanMacUpdatePolicy.decision(),
        homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> ShellSettingsLocalSummary {
        let hostConfig = ShellSettingsHostConfig.resolve(
            channel: channel,
            environment: environment,
            homeDirectory: homeDirectory
        )
        return ShellSettingsLocalSummary(
            channel: channel,
            appDisplayName: channel.appDisplayName,
            appBundleName: channel.appBundleName,
            bundleIdentifier: channel.bundleIdentifier,
            channelLabel: channel.settingsChannelLabel,
            cliToolName: channel.cliToolName,
            daemonURL: hostConfig.daemonURL,
            daemonBindAddress: hostConfig.bindAddress,
            updateSummary: updateSummary(for: updateDecision),
            updateDetail: updateDetail(for: updateDecision),
            alanHomeDisplayPath: channel.alanHomeDisplayPath,
            applicationSupportDisplayPath: channel.applicationSupportDisplayPath(
                homeDirectory: homeDirectory
            ),
            globalSkillsDisplayPath: channel.globalSkillsDisplayPath,
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

struct ShellSettingsWorkspaceContext: Equatable {
    let connectionWorkspaceDir: String?
    let skillCatalogWorkspaceDir: String?
    let skillCatalogUnavailableReason: String?
    let agentName: String?

    static let none = ShellSettingsWorkspaceContext(
        connectionWorkspaceDir: nil,
        skillCatalogWorkspaceDir: nil,
        skillCatalogUnavailableReason: nil,
        agentName: nil
    )

    static func resolve(
        activeWorkingDirectory: String?,
        channel: AlanInstallChannel = .current(),
        agentName: String? = nil,
        homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser,
        fileManager: FileManager = .default
    ) -> ShellSettingsWorkspaceContext {
        guard let activeDirectory = normalizedPath(activeWorkingDirectory) else {
            return ShellSettingsWorkspaceContext(
                connectionWorkspaceDir: nil,
                skillCatalogWorkspaceDir: nil,
                skillCatalogUnavailableReason: nil,
                agentName: normalizedNonEmpty(agentName)
            )
        }

        if let registered = ShellSettingsWorkspaceRegistry.load(
            channel: channel,
            homeDirectory: homeDirectory,
            fileManager: fileManager
        )
        .mostSpecificEntry(containing: activeDirectory) {
            return ShellSettingsWorkspaceContext(
                connectionWorkspaceDir: registered.path,
                skillCatalogWorkspaceDir: registered.catalogIdentifier,
                skillCatalogUnavailableReason: nil,
                agentName: normalizedNonEmpty(agentName)
            )
        }

        let discoveredWorkspaceRoot = workspaceRootContainingAlanDirectory(
            activeDirectory,
            fileManager: fileManager
        )
        return ShellSettingsWorkspaceContext(
            connectionWorkspaceDir: discoveredWorkspaceRoot ?? activeDirectory,
            skillCatalogWorkspaceDir: nil,
            skillCatalogUnavailableReason: discoveredWorkspaceRoot.map { _ in
                "Register this workspace to show workspace skills."
            },
            agentName: normalizedNonEmpty(agentName)
        )
    }

    private static func normalizedNonEmpty(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed?.isEmpty == false ? trimmed : nil
    }

    private static func normalizedPath(_ path: String?) -> String? {
        guard let trimmed = normalizedNonEmpty(path) else {
            return nil
        }
        return URL(fileURLWithPath: trimmed, isDirectory: true)
            .standardizedFileURL
            .path
    }

    private static func workspaceRootContainingAlanDirectory(
        _ path: String,
        fileManager: FileManager
    ) -> String? {
        var url = URL(fileURLWithPath: path, isDirectory: true).standardizedFileURL
        while url.path != "/" {
            let alanDirectory = url.appendingPathComponent(".alan", isDirectory: true).path
            var isDirectory: ObjCBool = false
            if fileManager.fileExists(atPath: alanDirectory, isDirectory: &isDirectory),
               isDirectory.boolValue
            {
                return url.path
            }
            url.deleteLastPathComponent()
        }
        return nil
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

    var alanHomeDisplayPath: String {
        switch self {
        case .stable:
            return "~/.alan"
        case .dev:
            return "~/.alan-dev"
        }
    }

    var alanHomeDirectoryName: String {
        switch self {
        case .stable:
            return ".alan"
        case .dev:
            return ".alan-dev"
        }
    }

    var globalSkillsDisplayPath: String {
        switch self {
        case .stable:
            return "~/.agents/skills"
        case .dev:
            return "~/.agents-dev/skills"
        }
    }

    var defaultDaemonBindAddress: String {
        switch self {
        case .stable:
            return "0.0.0.0:8090"
        case .dev:
            return "127.0.0.1:8091"
        }
    }

    var defaultDaemonURL: String {
        switch self {
        case .stable:
            return "http://127.0.0.1:8090"
        case .dev:
            return "http://127.0.0.1:8091"
        }
    }

    func applicationSupportDisplayPath(homeDirectory: URL) -> String {
        let suffix = "Library/Application Support/\(applicationSupportDirectoryName)"
        let homePath = homeDirectory.standardizedFileURL.path
        if homePath == "/" {
            return "/\(suffix)"
        }
        return "~/" + suffix
    }

    static func localDaemonURL(for bindAddress: String) -> String {
        let port = bindAddress.split(separator: ":").last.flatMap { UInt16($0) } ?? 8090
        return "http://127.0.0.1:\(port)"
    }
}

private struct ShellSettingsHostConfig {
    let bindAddress: String
    let daemonURL: String

    static func resolve(
        channel: AlanInstallChannel,
        environment: [String: String],
        homeDirectory: URL,
        fileManager: FileManager = .default
    ) -> ShellSettingsHostConfig {
        let fileConfig = load(channel: channel, homeDirectory: homeDirectory, fileManager: fileManager)
        let bindAddress =
            nonEmpty(environment["BIND_ADDRESS"])
            ?? fileConfig?.bindAddress
            ?? channel.defaultDaemonBindAddress
        let daemonURL =
            nonEmpty(environment["ALAN_AGENTD_URL"])
            ?? fileConfig?.daemonURL
            ?? channel.defaultDaemonURL
        return ShellSettingsHostConfig(bindAddress: bindAddress, daemonURL: daemonURL)
    }

    private static func load(
        channel: AlanInstallChannel,
        homeDirectory: URL,
        fileManager: FileManager
    ) -> ShellSettingsHostConfig? {
        let path = homeDirectory
            .appendingPathComponent(channel.alanHomeDirectoryName, isDirectory: true)
            .appendingPathComponent("host.toml", isDirectory: false)
            .path
        guard fileManager.fileExists(atPath: path),
              let content = try? String(contentsOfFile: path, encoding: .utf8)
        else {
            return nil
        }

        let values = ShellSettingsTopLevelTOMLParser.parse(content)
        let bindAddress = nonEmpty(values["bind_address"]) ?? channel.defaultDaemonBindAddress
        let daemonURL = nonEmpty(values["daemon_url"]) ?? AlanInstallChannel.localDaemonURL(
            for: bindAddress
        )
        return ShellSettingsHostConfig(bindAddress: bindAddress, daemonURL: daemonURL)
    }

    private static func nonEmpty(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed?.isEmpty == false ? trimmed : nil
    }
}

private enum ShellSettingsTopLevelTOMLParser {
    static func parse(_ content: String) -> [String: String] {
        var values: [String: String] = [:]
        for line in content.components(separatedBy: .newlines) {
            let stripped = stripComment(from: line).trimmingCharacters(in: .whitespacesAndNewlines)
            guard !stripped.isEmpty,
                  !stripped.hasPrefix("["),
                  let separator = stripped.firstIndex(of: "=")
            else {
                continue
            }

            let key = String(stripped[..<separator]).trimmingCharacters(in: .whitespacesAndNewlines)
            let rawValue = String(stripped[stripped.index(after: separator)...])
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !key.isEmpty,
               let value = parsedScalar(rawValue)
            {
                values[key] = value
            }
        }
        return values
    }

    private static func parsedScalar(_ rawValue: String) -> String? {
        if rawValue.count >= 2,
           let first = rawValue.first,
           let last = rawValue.last,
           (first == "\"" && last == "\"") || (first == "'" && last == "'")
        {
            return String(rawValue.dropFirst().dropLast())
        }
        return rawValue.isEmpty ? nil : rawValue
    }

    private static func stripComment(from line: String) -> String {
        var isInSingleQuotedString = false
        var isInDoubleQuotedString = false
        var previousCharacter: Character?

        for index in line.indices {
            let character = line[index]
            if character == "\"",
               !isInSingleQuotedString,
               previousCharacter != "\\"
            {
                isInDoubleQuotedString.toggle()
            } else if character == "'",
                      !isInDoubleQuotedString
            {
                isInSingleQuotedString.toggle()
            } else if character == "#",
                      !isInSingleQuotedString,
                      !isInDoubleQuotedString
            {
                return String(line[..<index])
            }
            previousCharacter = character
        }
        return line
    }
}

private struct ShellSettingsWorkspaceRegistry {
    struct Entry: Decodable, Equatable {
        let id: String
        let path: String
        let alias: String

        var catalogIdentifier: String {
            let trimmedAlias = alias.trimmingCharacters(in: .whitespacesAndNewlines)
            return trimmedAlias.isEmpty ? id : trimmedAlias
        }

        private enum CodingKeys: String, CodingKey {
            case id
            case path
            case alias
        }
    }

    let entries: [Entry]

    static func load(
        channel: AlanInstallChannel,
        homeDirectory: URL,
        fileManager: FileManager = .default
    ) -> ShellSettingsWorkspaceRegistry {
        let path = homeDirectory
            .appendingPathComponent(channel.alanHomeDirectoryName, isDirectory: true)
            .appendingPathComponent("registry.json", isDirectory: false)
            .path
        guard fileManager.fileExists(atPath: path),
              let data = try? Data(contentsOf: URL(fileURLWithPath: path)),
              let file = try? JSONDecoder().decode(File.self, from: data)
        else {
            return ShellSettingsWorkspaceRegistry(entries: [])
        }
        let entries = file.workspaces.map { entry in
            Entry(
                id: entry.id,
                path: normalizedPath(entry.path) ?? entry.path,
                alias: entry.alias
            )
        }
        return ShellSettingsWorkspaceRegistry(entries: entries)
    }

    func mostSpecificEntry(containing activeDirectory: String) -> Entry? {
        entries
            .filter { contains(path: activeDirectory, inWorkspaceRoot: $0.path) }
            .sorted { lhs, rhs in
                lhs.path.count > rhs.path.count
            }
            .first
    }

    private static func normalizedPath(_ path: String) -> String? {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            return nil
        }
        return URL(fileURLWithPath: trimmed, isDirectory: true).standardizedFileURL.path
    }

    private func contains(path: String, inWorkspaceRoot root: String) -> Bool {
        guard !root.isEmpty else {
            return false
        }
        if path == root {
            return true
        }
        let normalizedRoot = root.hasSuffix("/") ? root : root + "/"
        return path.hasPrefix(normalizedRoot)
    }

    private struct File: Decodable {
        let workspaces: [Entry]
    }
}
#endif
