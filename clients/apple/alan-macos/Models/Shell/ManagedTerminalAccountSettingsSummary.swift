import Foundation

#if os(macOS)
struct ManagedTerminalAccountSettingsSummary: Equatable {
    let plans: [ManagedTerminalAccountPlan]

    static let empty = ManagedTerminalAccountSettingsSummary(plans: [])

    static func current(
        terminalProfiles: TerminalProfileSettingsSummary,
        helperClient: AlanPrivilegedHelperClienting,
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
                    fullName: entry.displayLabel
                )
            )
        }

        for profile in terminalProfiles.profiles {
            guard let accountID = profile.managedTerminalAccountID else { continue }
            upsertRequest(
                ManagedTerminalAccountRequest(
                    accountName: accountID,
                    fullName: profile.title
                )
            )
        }

        let plans = orderedAccountNames.compactMap { accountName -> ManagedTerminalAccountPlan? in
            guard let request = requestsByAccount[accountName] else { return nil }
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
        return ManagedTerminalAccountSettingsSummary(plans: plans)
    }

    var users: [ManagedTerminalUserSummary] {
        plans.map(ManagedTerminalUserSummary.init(plan:))
    }
}
#endif
