import Foundation

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

    var isAlanManaged: Bool {
        if case .alanManaged = self {
            return true
        }
        return false
    }
}

enum ManagedTerminalAccountProfileState: Equatable {
    case missing
    case existingManaged(profileID: String)
    case existingManagedOutdated(profileID: String)
    case existingUnmanaged(profileID: String)
}

private extension ManagedTerminalAccountProfileState {
    var managedProfileID: String? {
        switch self {
        case .existingManaged(let profileID):
            return profileID
        case .missing, .existingManagedOutdated, .existingUnmanaged:
            return nil
        }
    }

    var unmanagedProfileID: String? {
        switch self {
        case .existingUnmanaged(let profileID):
            return profileID
        case .missing, .existingManaged, .existingManagedOutdated:
            return nil
        }
    }
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
        plan(request: request, diagnosis: diagnosis, terminalProfile: nil)
    }

    static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfiles: TerminalProfileDocument
    ) -> ManagedTerminalAccountPlan {
        plan(
            request: request,
            diagnosis: diagnosis,
            terminalProfile: terminalProfileState(for: request, document: terminalProfiles)
        )
    }

    private static func plan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfile: ManagedTerminalAccountProfileState?
    ) -> ManagedTerminalAccountPlan {
        let validationErrors = ManagedTerminalAccountIdentifierValidator.validate(request)
        guard validationErrors.isEmpty else {
            return ManagedTerminalAccountPlan(request: request, status: .invalid(validationErrors), steps: [])
        }

        switch diagnosis.readinessState {
        case .helperUnavailable:
            return ManagedTerminalAccountPlan(request: request, status: .helperUnavailable, steps: [])
        case .accountNotAlanManaged:
            return ManagedTerminalAccountPlan(request: request, status: .accountNotAlanManaged, steps: [])
        case _ where terminalProfile?.unmanagedProfileID != nil:
            return ManagedTerminalAccountPlan(
                request: request,
                status: .terminalProfileConflict(
                    profileID: terminalProfile?.unmanagedProfileID ?? request.terminalProfileID
                ),
                steps: []
            )
        case .destructiveConfirmationRequired:
            return ManagedTerminalAccountPlan(
                request: request,
                status: .requiresDestructiveConfirmation,
                steps: helperBackedSteps(
                    request: request,
                    diagnosis: diagnosis,
                    terminalProfile: terminalProfile
                )
            )
        case .ready:
            let steps = terminalProfileHandoffSteps(
                request: request,
                diagnosis: diagnosis,
                terminalProfile: terminalProfile
            )
            return ManagedTerminalAccountPlan(
                request: request,
                status: steps.isEmpty ? .alreadyReady : .readyToApply,
                steps: steps
            )
        case .ptySpawnFailed:
            return ManagedTerminalAccountPlan(
                request: request,
                status: .ptySpawnFailed,
                steps: helperBackedSteps(
                    request: request,
                    diagnosis: diagnosis,
                    terminalProfile: terminalProfile
                )
            )
        case .accountMissing, .repairable:
            let steps = helperBackedSteps(
                request: request,
                diagnosis: diagnosis,
                terminalProfile: terminalProfile
            )
            return ManagedTerminalAccountPlan(
                request: request,
                status: diagnosis.accountExists ? .repair : .readyToApply,
                steps: steps
            )
        }
    }

    static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope
    ) -> ManagedTerminalAccountPlan {
        rollbackPlan(request: request, diagnosis: diagnosis, scope: scope, terminalProfile: nil)
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
            terminalProfile: terminalProfileState(for: request, document: terminalProfiles)
        )
    }

    private static func rollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope,
        terminalProfile: ManagedTerminalAccountProfileState?
    ) -> ManagedTerminalAccountPlan {
        if diagnosis.readinessState == .helperUnavailable {
            return ManagedTerminalAccountPlan(request: request, status: .helperUnavailable, steps: [])
        }
        if diagnosis.readinessState == .accountNotAlanManaged
            || diagnosis.ownershipState == .notAlanManaged
        {
            return ManagedTerminalAccountPlan(request: request, status: .accountNotAlanManaged, steps: [])
        }

        var steps: [ManagedTerminalAccountPlanStep] = []
        if terminalProfile?.managedProfileID == request.terminalProfileID
            || diagnosis.terminalProfileID == request.terminalProfileID
        {
            steps.append(step(.removeManagedTerminalProfile, "Remove managed Terminal Profile", false))
        }

        switch scope {
        case .alanIntegrationOnly:
            steps.append(
                helperStep(.removeManagedUserIntegration, "Remove helper-managed account integration")
            )
            return ManagedTerminalAccountPlan(request: request, status: .readyToApply, steps: steps)
        case .deleteAccountAndHome(let confirmation):
            guard diagnosis.ownershipState == .alanManaged else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .accountNotAlanManaged,
                    steps: steps
                )
            }
            guard confirmation == request.accountName else {
                return ManagedTerminalAccountPlan(
                    request: request,
                    status: .requiresDestructiveConfirmation,
                    steps: steps
                )
            }
            var destructiveSteps: [ManagedTerminalAccountPlanStep] = []
            if diagnosis.accountExists {
                destructiveSteps.append(helperStep(.deleteAccount, "Delete terminal account"))
            }
            if diagnosis.homeDirectoryExists
                && request.homeDirectory == ManagedTerminalAccountRequest.canonicalHomeDirectory(
                    for: request.accountName
                )
            {
                destructiveSteps.append(
                    helperStep(.deleteHomeDirectory, "Delete terminal account home directory")
                )
            }
            destructiveSteps.append(
                helperStep(.removeManagedUserIntegration, "Remove helper-managed account integration")
            )
            return ManagedTerminalAccountPlan(
                request: request,
                status: .readyToApply,
                steps: steps + destructiveSteps
            )
        }
    }

    private static func step(
        _ kind: ManagedTerminalAccountPlanStepKind,
        _ summary: String,
        _ requiresPrivilege: Bool
    ) -> ManagedTerminalAccountPlanStep {
        ManagedTerminalAccountPlanStep(
            kind: kind,
            summary: summary,
            requiresPrivilege: requiresPrivilege
        )
    }

    private static func helperStep(
        _ kind: AlanManagedUserHelperPlanStepKind,
        _ summary: String
    ) -> ManagedTerminalAccountPlanStep {
        step(.helperStep(kind), summary, true)
    }

    private static func helperBackedSteps(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfile: ManagedTerminalAccountProfileState?
    ) -> [ManagedTerminalAccountPlanStep] {
        var steps: [ManagedTerminalAccountPlanStep] = []

        if !diagnosis.accountExists {
            steps.append(helperStep(.createStandardAccount, "Create standard local terminal account"))
        } else {
            if diagnosis.isAdmin {
                steps.append(helperStep(.repairAccountType, "Repair terminal account type"))
            }
            if !diagnosis.homeDirectoryExists {
                steps.append(helperStep(.repairHomeDirectory, "Repair terminal account home directory"))
            }
            if !diagnosis.shellMatches {
                steps.append(helperStep(.repairShell, "Repair terminal account shell"))
            }
        }

        if request.hideFromLoginWindow && !diagnosis.hiddenFromLoginWindow {
            steps.append(helperStep(.hideAccount, "Hide terminal account from login window lists"))
        }
        if diagnosis.ownershipState != .alanManaged {
            steps.append(helperStep(.writeOwnershipMarker, "Write Alan-managed ownership marker"))
        }
        steps.append(helperStep(.verifyAccount, "Verify helper-managed account state"))
        if !diagnosis.ptySmokeVerified {
            steps.append(helperStep(.verifyManagedUserPTY, "Verify helper-managed PTY startup"))
        }
        steps.append(
            contentsOf: terminalProfileHandoffSteps(
                request: request,
                diagnosis: diagnosis,
                terminalProfile: terminalProfile
            )
        )
        return steps
    }

    private static func terminalProfileHandoffSteps(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfile: ManagedTerminalAccountProfileState?
    ) -> [ManagedTerminalAccountPlanStep] {
        if let terminalProfile {
            switch terminalProfile {
            case .existingManaged:
                return []
            case .missing, .existingManagedOutdated:
                return [
                    step(.createOrUpdateTerminalProfile, "Create matching Terminal Profile", false),
                ]
            case .existingUnmanaged:
                return []
            }
        }
        guard diagnosis.terminalProfileID == request.terminalProfileID else {
            return [
                step(.createOrUpdateTerminalProfile, "Create matching Terminal Profile", false),
            ]
        }
        return []
    }

    private static func terminalProfileState(
        for request: ManagedTerminalAccountRequest,
        document: TerminalProfileDocument
    ) -> ManagedTerminalAccountProfileState {
        guard let profile = document.profile(id: request.terminalProfileID) else {
            return .missing
        }
        guard profile.managedTerminalAccountID == request.accountName else {
            return .existingUnmanaged(profileID: profile.id)
        }
        guard profile.launch == .managedUser(unixUser: request.accountName),
            profile.defaultWorkingDirectory == request.homeDirectory
        else {
            return .existingManagedOutdated(profileID: profile.id)
        }
        return .existingManaged(profileID: profile.id)
    }

}
