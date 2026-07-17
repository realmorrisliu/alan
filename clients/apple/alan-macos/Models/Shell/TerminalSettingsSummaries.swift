import Foundation

#if os(macOS)
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
#endif
