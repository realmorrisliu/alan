import Foundation

#if os(macOS)
enum ShellSettingsSectionID: String, CaseIterable, Equatable {
    case interface
    case terminalProfiles
    case terminalAccounts
    case local

    static let defaultOrder: [ShellSettingsSectionID] = [
        .interface,
        .terminalProfiles,
        .terminalAccounts,
        .local,
    ]

    var title: String {
        switch self {
        case .interface:
            return "Interface"
        case .terminalProfiles:
            return "Terminal Profiles"
        case .terminalAccounts:
            return "Managed Users"
        case .local:
            return "Local"
        }
    }
}

enum ShellSettingsNavigationGroup: String, CaseIterable, Equatable, Identifiable {
    case general
    case terminal
    case system

    static let defaultOrder: [ShellSettingsNavigationGroup] = [
        .general,
        .terminal,
        .system,
    ]

    var id: ShellSettingsNavigationGroup { self }

    var title: String {
        switch self {
        case .general:
            return "General"
        case .terminal:
            return "Terminal"
        case .system:
            return "System"
        }
    }

    var systemName: String {
        switch self {
        case .general:
            return "slider.horizontal.3"
        case .terminal:
            return "terminal"
        case .system:
            return "gearshape.2"
        }
    }
}

enum ShellSettingsGroupSectionID: String, Equatable, Identifiable {
    case interface
    case profiles
    case localIdentity
    case app
    case localRuntime
    case storage
    case diagnostics

    var id: ShellSettingsGroupSectionID { self }

    var title: String {
        switch self {
        case .interface:
            return "Interface"
        case .profiles:
            return "Profiles"
        case .localIdentity:
            return "Managed Users"
        case .app:
            return "Application"
        case .localRuntime:
            return "Runtime"
        case .storage:
            return "Storage"
        case .diagnostics:
            return "Diagnostics"
        }
    }
}

enum ShellSettingsRowMutability: Equatable {
    case editable
    case readOnly
    case actionOnly
    case deferred
}

enum ShellSettingsRowActionKind: String, Equatable {
    case create
    case review
    case repair
    case verify
    case remove
    case installHelper = "install_helper"
    case updateHelper = "update_helper"
    case uninstallHelper = "uninstall_helper"
}

struct ShellSettingsRowActionModel: Identifiable, Equatable {
    let id: ShellSettingsRowActionKind
    let title: String
    let systemName: String

    static func make(_ kind: ShellSettingsRowActionKind) -> ShellSettingsRowActionModel {
        switch kind {
        case .create:
            return ShellSettingsRowActionModel(id: kind, title: "Create", systemName: "plus")
        case .review:
            return ShellSettingsRowActionModel(id: kind, title: "Review", systemName: "doc.text")
        case .repair:
            return ShellSettingsRowActionModel(
                id: kind,
                title: "Repair",
                systemName: "wrench.and.screwdriver"
            )
        case .verify:
            return ShellSettingsRowActionModel(
                id: kind,
                title: "Verify",
                systemName: "checkmark.seal"
            )
        case .remove:
            return ShellSettingsRowActionModel(id: kind, title: "Remove", systemName: "trash")
        case .installHelper:
            return ShellSettingsRowActionModel(id: kind, title: "Install", systemName: "arrow.down.circle")
        case .updateHelper:
            return ShellSettingsRowActionModel(id: kind, title: "Update", systemName: "arrow.triangle.2.circlepath")
        case .uninstallHelper:
            return ShellSettingsRowActionModel(id: kind, title: "Uninstall", systemName: "trash")
        }
    }
}

struct ShellSettingsRowModel: Identifiable, Equatable {
    let id: String
    let systemName: String
    let title: String
    let detail: String?
    let value: String?
    let mutability: ShellSettingsRowMutability
    let offersFreeformEditing: Bool
    let actions: [ShellSettingsRowActionModel]

    init(
        id: String,
        systemName: String,
        title: String,
        detail: String? = nil,
        value: String? = nil,
        mutability: ShellSettingsRowMutability = .readOnly,
        offersFreeformEditing: Bool = false,
        actions: [ShellSettingsRowActionModel] = []
    ) {
        self.id = id
        self.systemName = systemName
        self.title = title
        self.detail = detail
        self.value = value
        self.mutability = mutability
        self.offersFreeformEditing = offersFreeformEditing
        self.actions = actions
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

struct ShellSettingsNavigationGroupModel: Identifiable, Equatable {
    let id: ShellSettingsNavigationGroup
    let sections: [ShellSettingsGroupSectionModel]

    var title: String { id.title }
    var systemName: String { id.systemName }

    var rows: [ShellSettingsRowModel] {
        sections.flatMap(\.rows)
    }

    var visibleText: [String] {
        [title] + sections.flatMap(\.visibleText)
    }
}

struct ShellSettingsGroupSectionModel: Identifiable, Equatable {
    let id: ShellSettingsGroupSectionID
    let rows: [ShellSettingsRowModel]

    var title: String { id.title }

    var visibleText: [String] {
        [title] + rows.flatMap(\.visibleText)
    }
}

struct ShellSettingsSurfaceSnapshot: Equatable {
    let sections: [ShellSettingsSectionModel]

    static func make(
        local: ShellSettingsLocalSummary,
        terminalProfiles: TerminalProfileSettingsSummary = .current(),
        privilegedHelper: PrivilegedHelperSettingsSummary = .current(),
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary = .empty,
        diagnostics: ShellSettingsDiagnosticsSummary = .disabled
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
                    rows: managedTerminalAccountRows(
                        managedTerminalAccounts,
                        privilegedHelper: privilegedHelper
                    )
                ),
                ShellSettingsSectionModel(id: .local, rows: localRows(local, diagnostics: diagnostics)),
            ]
        )
    }

    var visibleText: [String] {
        sections.flatMap(\.visibleText)
    }

    var navigationGroups: [ShellSettingsNavigationGroupModel] {
        return ShellSettingsNavigationGroup.defaultOrder.map { group in
            ShellSettingsNavigationGroupModel(
                id: group,
                sections: groupSections(for: group)
            )
        }
    }

    private var allRows: [ShellSettingsRowModel] {
        sections.flatMap(\.rows)
    }

    private var rowsByID: [String: ShellSettingsRowModel] {
        Dictionary(uniqueKeysWithValues: allRows.map { ($0.id, $0) })
    }

    private func groupSections(
        for group: ShellSettingsNavigationGroup
    ) -> [ShellSettingsGroupSectionModel] {
        let rowLookup = rowsByID
        switch group {
        case .general:
            return [
                section(
                    .interface,
                    rowIDs: ["appearance", "sidebar", "inactiveSplitDimming"],
                    rowsByID: rowLookup
                ),
            ].compactMap { $0 }
        case .terminal:
            return [
                section(
                    .profiles,
                    rows: rows(
                        rowIDs: [
                            "terminalProfilesDefault",
                            "terminalProfilesCreate",
                            "terminalProfilesRecovery",
                        ],
                        matchingPrefix: "terminalProfile.",
                        rowsByID: rowLookup
                    )
                ),
                section(
                    .localIdentity,
                    rows: rows(
                        rowIDs: [
                            "terminalAccountProvision",
                            "terminalAccountLoginBoundary",
                            "terminalProfilesSudoGuidance",
                        ],
                        matchingPrefix: "terminalAccount.",
                        rowsByID: rowLookup
                    )
                ),
            ].compactMap { $0 }
        case .system:
            return [
                section(
                    .app,
                    rowIDs: ["appIdentity", "installChannel", "updates", "cliTool"],
                    rowsByID: rowLookup
                ),
                section(
                    .localRuntime,
                    rowIDs: ["applicationSupport", "shellControl"],
                    rowsByID: rowLookup
                ),
                section(.storage, rowIDs: ["dataRoot"], rowsByID: rowLookup),
                section(
                    .diagnostics,
                    rowIDs: ["performanceDiagnostics", "performanceDiagnosticsExport"],
                    rowsByID: rowLookup
                ),
            ].compactMap { $0 }
        }
    }

    private func section(
        _ id: ShellSettingsGroupSectionID,
        rowIDs: [String],
        rowsByID: [String: ShellSettingsRowModel]
    ) -> ShellSettingsGroupSectionModel? {
        section(id, rows: rowIDs.compactMap { rowsByID[$0] })
    }

    private func section(
        _ id: ShellSettingsGroupSectionID,
        rows: [ShellSettingsRowModel]
    ) -> ShellSettingsGroupSectionModel? {
        guard !rows.isEmpty else { return nil }
        return ShellSettingsGroupSectionModel(id: id, rows: rows)
    }

    private func rows(
        rowIDs: [String],
        rowsByID: [String: ShellSettingsRowModel]
    ) -> [ShellSettingsRowModel] {
        rowIDs.compactMap { rowsByID[$0] }
    }

    private func rows(
        rowIDs: [String],
        matchingPrefix prefix: String,
        rowsByID: [String: ShellSettingsRowModel]
    ) -> [ShellSettingsRowModel] {
        let explicitRows = rowIDs.compactMap { rowsByID[$0] }
        let dynamicRows = allRows.filter { $0.id.hasPrefix(prefix) }
        return explicitRows + dynamicRows
    }

    private static func interfaceRows() -> [ShellSettingsRowModel] {
        [
            ShellSettingsRowModel(
                id: "appearance",
                systemName: "circle.lefthalf.filled",
                title: "Appearance",
                detail: "Use system appearance or choose a fixed theme.",
                mutability: .editable
            ),
            ShellSettingsRowModel(
                id: "sidebar",
                systemName: "sidebar.left",
                title: "Sidebar",
                detail: "Show workspace sidebar in terminal windows.",
                mutability: .editable
            ),
            ShellSettingsRowModel(
                id: "inactiveSplitDimming",
                systemName: "rectangle.split.2x1",
                title: "Inactive split dimming",
                detail: "Dim inactive terminal panes.",
                mutability: .editable
            ),
        ]
    }

    private static func terminalProfileRows(
        _ summary: TerminalProfileSettingsSummary
    ) -> [ShellSettingsRowModel] {
        do {
            let rows = try ShellCoreFFIAdapter.shared.terminalProfileRows(summary)
            return rows
        } catch {
            return [
                unavailableRow(
                    id: "terminalProfilesUnavailable",
                    systemName: "terminal",
                    title: "Terminal Profiles"
                ),
            ]
        }
    }

    private static func managedTerminalAccountRows(
        _ summary: ManagedTerminalAccountSettingsSummary,
        privilegedHelper: PrivilegedHelperSettingsSummary
    ) -> [ShellSettingsRowModel] {
        let users = summary.users
        let helperRow = privilegedHelper.row
        let createRows = [
            ShellSettingsRowModel(
                id: "terminalAccountProvision",
                systemName: "person.crop.circle.badge.plus",
                title: "Create Managed User",
                detail: "Create a terminal-only local user for passwordless terminal entry.",
                value: "Preview…",
                mutability: .actionOnly,
                actions: [.make(.create)]
            )
        ]
        let userRows = managedTerminalAccountUserRows(summary: summary, users: users)
        let boundaryRows = [
            ShellSettingsRowModel(
                id: "terminalAccountLoginBoundary",
                systemName: "macwindow.badge.plus",
                title: "Mac login session",
                detail: "This flow leaves the Mac login session setting unchanged.",
                value: "Not changed"
            )
        ]
        return [helperRow] + createRows + userRows + boundaryRows
    }

    private static func managedTerminalAccountUserRows(
        summary: ManagedTerminalAccountSettingsSummary,
        users: [ManagedTerminalUserSummary]
    ) -> [ShellSettingsRowModel] {
        guard !users.isEmpty else { return [] }
        do {
            let coreRows = try ShellCoreFFIAdapter.shared.managedTerminalAccountRows(summary)
            let rowsByID = Dictionary(uniqueKeysWithValues: coreRows.map { ($0.id, $0) })
            return users.map { user in
                let id = "terminalAccount.\(user.unixUserName)"
                let coreRow = rowsByID[id]
                return ShellSettingsRowModel(
                    id: id,
                    systemName: coreRow?.systemName ?? "exclamationmark.triangle",
                    title: user.displayLabel,
                    detail: coreRow?.detail,
                    value: coreRow?.value ?? "Unavailable",
                    mutability: .actionOnly,
                    actions: terminalAccountActions(user)
                )
            }
        } catch {
            return users.map { user in
                ShellSettingsRowModel(
                    id: "terminalAccount.\(user.unixUserName)",
                    systemName: "exclamationmark.triangle",
                    title: user.displayLabel,
                    value: "Unavailable",
                    mutability: .actionOnly,
                    actions: [.make(.review)]
                )
            }
        }
    }

    private static func terminalAccountActions(
        _ user: ManagedTerminalUserSummary
    ) -> [ShellSettingsRowActionModel] {
        let kinds: [ShellSettingsRowActionKind]
        switch user.readinessState {
        case .ready:
            kinds = [.review, .verify, .remove]
        case .repairable, .readyToApply, .legacySudoersPresent, .ptySpawnFailed:
            kinds = [.review, .repair]
        case .invalid,
             .helperUnavailable,
             .accountNotAlanManaged,
             .destructiveConfirmation,
             .sudoersConflict,
             .terminalProfileConflict:
            kinds = [.review]
        }
        return kinds.map(ShellSettingsRowActionModel.make)
    }

    private static func localRows(
        _ local: ShellSettingsLocalSummary,
        diagnostics: ShellSettingsDiagnosticsSummary
    ) -> [ShellSettingsRowModel] {
        do {
            let rows = try ShellCoreFFIAdapter.shared.localRows(local, diagnostics: diagnostics)
            return rows
        } catch {
            return [
                unavailableRow(
                    id: "localStateUnavailable",
                    systemName: "app",
                    title: "Local state"
                ),
            ]
        }
    }

    private static func unavailableRow(
        id: String,
        systemName: String,
        title: String
    ) -> ShellSettingsRowModel {
        ShellSettingsRowModel(
            id: id,
            systemName: systemName,
            title: title,
            value: "Unavailable"
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

    var containsManagedUserProfile: Bool {
        profiles.contains { profile in
            if case .managedUser = profile.launch {
                return true
            }
            return false
        }
    }

    var document: TerminalProfileDocument {
        TerminalProfileDocument(defaultProfileID: defaultProfileID, profiles: profiles)
    }
}

struct PrivilegedHelperSettingsSummary: Equatable {
    let status: AlanPrivilegedHelperStatus

    static func current(
        manager: AlanPrivilegedHelperLifecycleManaging = AlanPrivilegedHelperAppServiceManager()
    ) -> PrivilegedHelperSettingsSummary {
        PrivilegedHelperSettingsSummary(status: manager.status())
    }

    var row: ShellSettingsRowModel {
        ShellSettingsRowModel(
            id: "terminalPrivilegedHelper",
            systemName: systemName,
            title: "Privileged helper",
            detail: detail,
            value: value,
            mutability: actions.isEmpty ? .readOnly : .actionOnly,
            actions: actions.map(ShellSettingsRowActionModel.make)
        )
    }

    private var systemName: String {
        switch status.state {
        case .healthy:
            return "checkmark.shield"
        case .installing, .updating:
            return "hourglass"
        case .notInstalled, .outdated, .invalidSignature, .unavailable, .uninstallable:
            return "exclamationmark.shield"
        }
    }

    private var value: String {
        switch status.state {
        case .notInstalled:
            return "Not installed"
        case .outdated:
            return "Outdated"
        case .invalidSignature:
            return "Invalid signature"
        case .installing:
            return "Installing"
        case .updating:
            return "Updating"
        case .healthy:
            return "Healthy"
        case .unavailable:
            return "Unavailable"
        case .uninstallable:
            return "Uninstallable"
        }
    }

    private var detail: String {
        if let message = status.sanitizedMessage,
           !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            return message
        }
        switch status.state {
        case .notInstalled:
            return "Install the helper before Managed Users can be created or repaired."
        case .outdated:
            return "Update the helper before using helper-backed Managed Users."
        case .invalidSignature:
            return "Reinstall the helper because its signature does not match this Alan build."
        case .installing:
            return "Helper installation is in progress."
        case .updating:
            return "Helper update is in progress."
        case .healthy:
            return "Managed User create, repair, and terminal launch can use the helper."
        case .unavailable:
            return "Helper status is unavailable; Managed User privileged operations are disabled."
        case .uninstallable:
            return "The helper can be removed from this Mac."
        }
    }

    private var actions: [ShellSettingsRowActionKind] {
        switch status.state {
        case .notInstalled, .unavailable:
            return [.installHelper]
        case .outdated, .invalidSignature:
            return [.updateHelper]
        case .uninstallable:
            return [.uninstallHelper]
        case .installing, .updating, .healthy:
            return []
        }
    }
}

struct ManagedTerminalAccountSettingsSummary: Equatable {
    let plans: [ManagedTerminalAccountPlan]

    static let empty = ManagedTerminalAccountSettingsSummary(plans: [])

    static func current(
        terminalProfiles: TerminalProfileSettingsSummary,
        guiUserName: String = NSUserName(),
        discoverer: ManagedTerminalAccountLocalStateDiscoverer = ManagedTerminalAccountLocalStateDiscoverer(),
        entryVerifier: ManagedTerminalAccountEntryVerifying = ManagedTerminalAccountSudoEntryVerifier(),
        helperClient: AlanPrivilegedHelperClienting? = nil,
        catalog: ManagedTerminalAccountCatalog? = nil
    ) -> ManagedTerminalAccountSettingsSummary {
        let storedCatalog = catalog ?? ManagedTerminalAccountCatalogStore.defaultStore().load()
        var requestsByAccount: [String: ManagedTerminalAccountRequest] = [:]
        var orderedAccountNames: [String] = []

        func upsertRequest(_ request: ManagedTerminalAccountRequest) {
            if requestsByAccount[request.accountName] == nil {
                orderedAccountNames.append(request.accountName)
            }
            requestsByAccount[request.accountName] = request
        }

        for entry in storedCatalog.entries {
            upsertRequest(
                ManagedTerminalAccountRequest(
                    accountName: entry.accountName,
                    guiUserName: guiUserName,
                    fullName: entry.displayLabel
                )
            )
        }

        for profile in terminalProfiles.profiles {
            guard let accountID = profile.managedTerminalAccountID else { continue }
            upsertRequest(
                ManagedTerminalAccountRequest(
                    accountName: accountID,
                    guiUserName: guiUserName,
                    fullName: profile.title
                )
            )
        }

        let plans = orderedAccountNames.compactMap { accountName -> ManagedTerminalAccountPlan? in
            guard let request = requestsByAccount[accountName] else { return nil }
            if let helperClient {
                let status = helperClient.status()
                let diagnosis = status.isHealthy
                    ? helperClient.diagnoseManagedUser(request)
                    : AlanManagedUserDiagnosis.helperUnavailable(request: request, status: status)
                return ManagedTerminalAccountPlanner.plan(
                    request: request,
                    diagnosis: diagnosis,
                    terminalProfiles: terminalProfiles.document
                )
            }
            let discoveredState = discoverer.discover(
                request: request,
                terminalProfiles: terminalProfiles.document
            )
            let verification = ManagedTerminalAccountReadinessVerifier.verify(
                request: request,
                state: discoveredState,
                entryVerifier: entryVerifier
            )
            let verifiedState = ManagedTerminalAccountState(
                account: discoveredState.account,
                sudoers: discoveredState.sudoers,
                ownership: discoveredState.ownership,
                terminalProfile: discoveredState.terminalProfile,
                verification: verification,
                homeDirectoryExists: discoveredState.homeDirectoryExists
            )
            return ManagedTerminalAccountPlanner.plan(request: request, state: verifiedState)
        }
        return ManagedTerminalAccountSettingsSummary(plans: plans)
    }

    var users: [ManagedTerminalUserSummary] {
        plans.map(ManagedTerminalUserSummary.init(plan:))
    }
}

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

enum TerminalProfileSpaceIdentityFilter {
    static func selectableProfiles(
        terminalProfiles: TerminalProfileSettingsSummary,
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary
    ) -> [TerminalProfileDefinition] {
        terminalProfiles.profiles.filter { profile in
            guard profile.id != TerminalProfileDefinition.loginShellFallback.id else { return false }
            guard let managedAccountID = profile.managedTerminalAccountID else { return true }
            return managedTerminalAccounts.users.first {
                $0.unixUserName == managedAccountID && $0.readinessState == .ready
            } != nil
        }
    }

    static func repairGuidance(
        profileID: String,
        terminalProfiles: TerminalProfileSettingsSummary,
        managedTerminalAccounts: ManagedTerminalAccountSettingsSummary
    ) -> String? {
        guard let profile = terminalProfiles.profiles.first(where: { $0.id == profileID }),
              let managedAccountID = profile.managedTerminalAccountID
        else {
            return nil
        }
        guard let user = managedTerminalAccounts.users.first(where: { $0.unixUserName == managedAccountID })
        else {
            return "Repair this Managed User in Settings before using it for a Space."
        }
        guard user.readinessState != .ready else { return nil }
        if let repairState = user.repairState {
            return "Repair required: \(repairState)"
        }
        return user.conflictState ?? "Repair this Managed User in Settings before using it for a Space."
    }
}

enum ManagedTerminalUserReadinessState: String, Equatable {
    case ready
    case repairable
    case readyToApply
    case invalid
    case helperUnavailable
    case accountNotAlanManaged
    case legacySudoersPresent
    case ptySpawnFailed
    case destructiveConfirmation
    case sudoersConflict
    case terminalProfileConflict
}

struct ManagedTerminalUserSummary: Equatable, Identifiable {
    let unixUserName: String
    let displayLabel: String
    let readinessState: ManagedTerminalUserReadinessState
    let repairState: String?
    let conflictState: String?
    let managedTerminalProfileID: String

    var id: String { unixUserName }

    init(plan: ManagedTerminalAccountPlan) {
        unixUserName = plan.request.accountName
        let trimmedLabel = plan.request.fullName?.trimmingCharacters(in: .whitespacesAndNewlines)
        if let trimmedLabel, !trimmedLabel.isEmpty {
            displayLabel = trimmedLabel
        } else {
            displayLabel = plan.request.accountName
        }
        managedTerminalProfileID = plan.request.terminalProfileID

        switch plan.status {
        case .alreadyReady:
            readinessState = .ready
            repairState = nil
            conflictState = nil
        case .repair:
            readinessState = .repairable
            repairState = "\(plan.request.accountName) needs repair before terminal entry is ready."
            conflictState = nil
        case .readyToApply:
            readinessState = .readyToApply
            repairState = nil
            conflictState = nil
        case .invalid:
            readinessState = .invalid
            repairState = nil
            conflictState = nil
        case .helperUnavailable:
            readinessState = .helperUnavailable
            repairState = nil
            conflictState = "Privileged helper is unavailable for \(plan.request.accountName)."
        case .accountNotAlanManaged:
            readinessState = .accountNotAlanManaged
            repairState = nil
            conflictState = "\(plan.request.accountName) is an existing local account outside Alan management."
        case .legacySudoersPresent(let path):
            readinessState = .legacySudoersPresent
            repairState = path.map {
                "\(plan.request.accountName) has legacy Alan sudoers state at \($0)."
            } ?? "\(plan.request.accountName) has legacy Alan sudoers state."
            conflictState = nil
        case .ptySpawnFailed:
            readinessState = .ptySpawnFailed
            repairState = "\(plan.request.accountName) failed helper-managed PTY verification."
            conflictState = nil
        case .requiresDestructiveConfirmation:
            readinessState = .destructiveConfirmation
            repairState = nil
            conflictState = nil
        case .sudoersConflict(let path):
            readinessState = .sudoersConflict
            repairState = nil
            conflictState = "\(plan.request.accountName) has an existing non-Alan sudoers file at \(path)."
        case .terminalProfileConflict(let profileID):
            readinessState = .terminalProfileConflict
            repairState = nil
            conflictState =
                "\(plan.request.accountName) has an existing non-Alan Terminal Profile named \(profileID)."
        }
    }
}

struct ManagedTerminalUserCreationDraft: Equatable {
    var unixUserName: String
    var displayLabel: String
    var guiUserName: String

    var request: ManagedTerminalAccountRequest {
        ManagedTerminalAccountRequest(
            accountName: unixUserName.trimmingCharacters(in: .whitespacesAndNewlines),
            guiUserName: guiUserName.trimmingCharacters(in: .whitespacesAndNewlines),
            fullName: normalizedDisplayLabel
        )
    }

    private var normalizedDisplayLabel: String? {
        let trimmed = displayLabel.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}

enum ManagedTerminalUserCreationPreviewError: Equatable {
    case missingUnixUserName
    case missingDisplayLabel
    case duplicateUnixUser(String)
    case terminalProfileConflict(String)
    case validation([ManagedTerminalAccountValidationError])
}

struct ManagedTerminalUserCreationPreview: Equatable {
    let request: ManagedTerminalAccountRequest
    let plan: ManagedTerminalAccountPlan

    var visiblePlanRows: [String] {
        var rows = [
            "Account \(request.accountName)",
            "Home \(request.homeDirectory)",
            "Shell \(request.shell)",
        ]
        if request.hideFromLoginWindow {
            rows.append("Hidden from login window")
        }
        rows.append("Privileged helper managed")
        rows.append(contentsOf: plan.steps.map(visiblePlanRow(for:)))
        return rows
    }

    private func visiblePlanRow(for step: ManagedTerminalAccountPlanStep) -> String {
        switch step.kind {
        case .createStandardAccount:
            return "Create standard account"
        case .repairAccountType:
            return "Repair account type"
        case .repairHomeDirectory:
            return "Repair home directory"
        case .repairShell:
            return "Repair shell"
        case .hideAccount:
            return "Hide from login window"
        case .writeSudoersDropIn:
            return "Prepare helper-managed account entry"
        case .validateSudoers:
            return "Validate helper plan"
        case .verifyTerminalEntry:
            return "Verify terminal entry"
        case .createOrUpdateTerminalProfile:
            return "Terminal Profile \(request.terminalProfileID)"
        case .bindCurrentSpace:
            return "Bind current Space"
        case .removeSudoersDropIn:
            return "Remove Alan-owned sudoers drop-in"
        case .removeManagedTerminalProfile:
            return "Remove managed Terminal Profile"
        case .deleteAccount:
            return "Delete terminal account"
        case .deleteHomeDirectory:
            return "Delete terminal account home directory"
        case .helperStep:
            return step.summary
        }
    }
}

struct ManagedTerminalUserCreationPreviewResult: Equatable {
    let preview: ManagedTerminalUserCreationPreview?
    let errors: [ManagedTerminalUserCreationPreviewError]

    var isValid: Bool {
        preview != nil && errors.isEmpty
    }
}

enum ManagedTerminalUserCreationPreviewBuilder {
    static func make(
        draft: ManagedTerminalUserCreationDraft,
        existingUsers: [ManagedTerminalUserSummary],
        terminalProfiles: TerminalProfileSettingsSummary,
        diagnosis: AlanManagedUserDiagnosis
    ) -> ManagedTerminalUserCreationPreviewResult {
        make(
            draft: draft,
            existingUsers: existingUsers,
            terminalProfiles: terminalProfiles,
            accountIsUnavailable: diagnosis.accountExists
                && diagnosis.ownershipState != .alanManaged,
            plan: ManagedTerminalAccountPlanner.plan(
                request: draft.request,
                diagnosis: diagnosis,
                terminalProfiles: terminalProfiles.document
            )
        )
    }

    static func make(
        draft: ManagedTerminalUserCreationDraft,
        existingUsers: [ManagedTerminalUserSummary],
        terminalProfiles: TerminalProfileSettingsSummary,
        state: ManagedTerminalAccountState
    ) -> ManagedTerminalUserCreationPreviewResult {
        make(
            draft: draft,
            existingUsers: existingUsers,
            terminalProfiles: terminalProfiles,
            accountIsUnavailable: accountIsUnavailableForCreation(state.account),
            plan: ManagedTerminalAccountPlanner.plan(request: draft.request, state: state)
        )
    }

    private static func make(
        draft: ManagedTerminalUserCreationDraft,
        existingUsers: [ManagedTerminalUserSummary],
        terminalProfiles: TerminalProfileSettingsSummary,
        accountIsUnavailable: Bool,
        plan: ManagedTerminalAccountPlan
    ) -> ManagedTerminalUserCreationPreviewResult {
        let request = draft.request
        var errors: [ManagedTerminalUserCreationPreviewError] = []
        if request.accountName.isEmpty {
            errors.append(.missingUnixUserName)
        }
        if request.fullName == nil {
            errors.append(.missingDisplayLabel)
        }
        let duplicatesManagedUser = existingUsers.contains { $0.unixUserName == request.accountName }
        if duplicatesManagedUser {
            errors.append(.duplicateUnixUser(request.accountName))
        }
        if !duplicatesManagedUser && accountIsUnavailable {
            errors.append(.duplicateUnixUser(request.accountName))
        }
        if let conflictingProfile = terminalProfiles.profiles.first(where: {
            $0.id == request.terminalProfileID
                && $0.managedTerminalAccountID != request.accountName
        }) {
            errors.append(.terminalProfileConflict(conflictingProfile.id))
        }

        let validationErrors = ManagedTerminalAccountIdentifierValidator.validate(request)
        if !validationErrors.isEmpty {
            errors.append(.validation(validationErrors))
        }
        guard errors.isEmpty else {
            return ManagedTerminalUserCreationPreviewResult(preview: nil, errors: errors)
        }

        return ManagedTerminalUserCreationPreviewResult(
            preview: ManagedTerminalUserCreationPreview(request: request, plan: plan),
            errors: []
        )
    }

    private static func accountIsUnavailableForCreation(
        _ account: ManagedTerminalAccountRecord
    ) -> Bool {
        switch account {
        case .missing:
            return false
        case .invalid(let reason) where reason.localizedCaseInsensitiveContains("incomplete"):
            return false
        case .invalid, .standard, .admin:
            return true
        }
    }
}

struct ManagedTerminalUserProvisioningApplyResult: Equatable {
    let applyResult: ManagedTerminalAccountApplyResult
    let refreshedSummary: ManagedTerminalAccountSettingsSummary
}

enum ManagedTerminalUserProvisioningFlow {
    static func applyApproved<Executor: ManagedTerminalAccountPrivilegedExecuting>(
        plan: ManagedTerminalAccountPlan,
        executor: Executor,
        refresh: () -> ManagedTerminalAccountSettingsSummary
    ) -> ManagedTerminalUserProvisioningApplyResult {
        let applyResult = executor.apply(plan)
        return ManagedTerminalUserProvisioningApplyResult(
            applyResult: applyResult,
            refreshedSummary: refresh()
        )
    }
}

struct ShellSettingsLocalSummary: Equatable {
    let channel: AlanInstallChannel
    let appDisplayName: String
    let appBundleName: String
    let bundleIdentifier: String
    let channelLabel: String
    let cliToolName: String
    let updateSummary: String
    let updateDetail: String
    let alanHomeDisplayPath: String
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
            alanHomeDisplayPath: channel.alanHomeDisplayPath,
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

    var alanHomeDisplayPath: String {
        switch self {
        case .stable:
            return "~/.alan"
        case .dev:
            return "~/.alan-dev"
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

}
#endif
