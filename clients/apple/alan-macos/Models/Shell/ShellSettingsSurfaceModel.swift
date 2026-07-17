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
        case .repairable, .readyToApply, .ptySpawnFailed:
            kinds = [.review, .repair]
        case .invalid,
             .helperUnavailable,
             .accountNotAlanManaged,
             .destructiveConfirmation,
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
#endif
