import Foundation

enum ManagedTerminalAccountPlanStepKind: Equatable {
    case createStandardAccount
    case repairAccountType
    case repairHomeDirectory
    case repairShell
    case hideAccount
    case createOrUpdateTerminalProfile
    case removeManagedTerminalProfile
    case deleteAccount
    case deleteHomeDirectory
    case helperStep(AlanManagedUserHelperPlanStepKind)
}

struct ManagedTerminalAccountPlanStep: Equatable {
    let kind: ManagedTerminalAccountPlanStepKind
    let summary: String
    let requiresPrivilege: Bool
}

enum ManagedTerminalAccountPlanStatus: Equatable {
    case readyToApply
    case alreadyReady
    case repair
    case invalid([ManagedTerminalAccountValidationError])
    case helperUnavailable
    case accountNotAlanManaged
    case ptySpawnFailed
    case requiresDestructiveConfirmation
    case terminalProfileConflict(profileID: String)
}

struct ManagedTerminalAccountPlan: Equatable {
    let request: ManagedTerminalAccountRequest
    let status: ManagedTerminalAccountPlanStatus
    let steps: [ManagedTerminalAccountPlanStep]
}

enum ManagedTerminalAccountRollbackScope: Equatable {
    case alanIntegrationOnly
    case deleteAccountAndHome(confirmation: String?)
}

enum ManagedTerminalAccountPlanner {
    static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis
    ) -> ManagedTerminalAccountPlan {
        plan(request: request, diagnosis: diagnosis, terminalProfiles: nil)
    }

    static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfiles: TerminalProfileDocument
    ) -> ManagedTerminalAccountPlan {
        plan(request: request, diagnosis: diagnosis, terminalProfiles: Optional(terminalProfiles))
    }

    static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope
    ) -> ManagedTerminalAccountPlan {
        rollbackPlan(
            request: request,
            diagnosis: diagnosis,
            scope: scope,
            terminalProfiles: nil
        )
    }

    static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope,
        terminalProfiles: TerminalProfileDocument
    ) -> ManagedTerminalAccountPlan {
        rollbackPlan(
            request: request,
            diagnosis: diagnosis,
            scope: scope,
            terminalProfiles: Optional(terminalProfiles)
        )
    }

    private static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfiles: TerminalProfileDocument?
    ) -> ManagedTerminalAccountPlan {
        do {
            return try ShellCoreManagedTerminalAccountAdapter().managedTerminalAccountPlan(
                request: request,
                diagnosis: diagnosis,
                terminalProfiles: terminalProfiles
            )
        } catch {
            return coreUnavailablePlan(request: request, error: error)
        }
    }

    private static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope,
        terminalProfiles: TerminalProfileDocument?
    ) -> ManagedTerminalAccountPlan {
        do {
            return try ShellCoreManagedTerminalAccountAdapter().managedTerminalAccountRollbackPlan(
                request: request,
                diagnosis: diagnosis,
                scope: scope,
                terminalProfiles: terminalProfiles
            )
        } catch {
            return coreUnavailablePlan(request: request, error: error)
        }
    }

    private static func coreUnavailablePlan(
        request: ManagedTerminalAccountRequest,
        error: Error
    ) -> ManagedTerminalAccountPlan {
        ManagedTerminalAccountPlan(
            request: request,
            status: .invalid([.coreUnavailable(String(describing: error))]),
            steps: []
        )
    }
}
