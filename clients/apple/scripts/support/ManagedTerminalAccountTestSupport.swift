// Script/test support only. Production managed-account semantics are owned by shell-core.
enum ManagedTerminalAccountRecord: Equatable {
    case missing
    case standard(homeDirectory: String, shell: String, hidden: Bool)
    case admin(homeDirectory: String, shell: String, hidden: Bool)
    case invalid(reason: String)
}

enum ManagedTerminalAccountOwnershipEvidence: Equatable {
    case helperMarker(path: String)
}

enum ManagedTerminalAccountOwnershipState: Equatable {
    case missing
    case alanManaged(ManagedTerminalAccountOwnershipEvidence)
    case notAlanManaged(reason: String)
}

enum ManagedTerminalAccountProfileState: Equatable {
    case missing
    case existingManaged(profileID: String)
    case existingManagedOutdated(profileID: String)
    case existingUnmanaged(profileID: String)
}

enum ManagedTerminalAccountVerificationStep: String, Equatable {
    case accountLookup = "account_lookup"
    case nonAdminAccount = "non_admin_account"
    case homeDirectory = "home_directory"
    case shell
    case ownership
    case managedUserPTY = "managed_user_pty"
}

enum ManagedTerminalAccountVerificationStatus: Equatable {
    case notRun
    case passed
    case failed(step: ManagedTerminalAccountVerificationStep, message: String)
}

struct ManagedTerminalAccountState: Equatable {
    let account: ManagedTerminalAccountRecord
    let ownership: ManagedTerminalAccountOwnershipState
    let terminalProfile: ManagedTerminalAccountProfileState
    let verification: ManagedTerminalAccountVerificationStatus
    let homeDirectoryExists: Bool

    init(
        account: ManagedTerminalAccountRecord,
        ownership: ManagedTerminalAccountOwnershipState = .missing,
        terminalProfile: ManagedTerminalAccountProfileState,
        verification: ManagedTerminalAccountVerificationStatus,
        homeDirectoryExists: Bool = true
    ) {
        self.account = account
        self.ownership = ownership
        self.terminalProfile = terminalProfile
        self.verification = verification
        self.homeDirectoryExists = homeDirectoryExists
    }
}

final class ManagedTerminalAccountFakeExecutor: ManagedTerminalAccountPrivilegedExecuting {
    var failAt: ManagedTerminalAccountPlanStepKind?
    var cancelBeforeApply = false

    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult {
        let stepKinds = plan.steps.map(\.kind)
        if cancelBeforeApply {
            return .cancelled(before: stepKinds)
        }

        var completed: [ManagedTerminalAccountPlanStepKind] = []
        for step in plan.steps {
            if step.kind == failAt {
                return ManagedTerminalAccountApplyResult(
                    completedSteps: completed,
                    failedStep: step.kind,
                    cancelled: false,
                    visibleDiagnostics: ["Step failed: \(step.summary). Credentials redacted."]
                )
            }
            completed.append(step.kind)
        }
        return ManagedTerminalAccountApplyResult(
            completedSteps: completed,
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Provisioning plan applied. Credentials redacted."]
        )
    }
}

enum ManagedTerminalAccountProfileHandoff {
    static func profileDefinition(
        for request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState
    ) -> TerminalProfileDefinition? {
        guard state.verification == .passed else { return nil }
        return TerminalProfileDefinition(
            id: request.terminalProfileID,
            title: request.fullName ?? request.accountName,
            launch: .managedUser(unixUser: request.accountName),
            defaultWorkingDirectory: request.homeDirectory,
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: nil
            ),
            managedTerminalAccountID: request.accountName
        )
    }
}
