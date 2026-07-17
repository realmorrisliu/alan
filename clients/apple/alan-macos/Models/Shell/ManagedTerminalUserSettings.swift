import Foundation

#if os(macOS)
enum ManagedTerminalUserReadinessState: String, Equatable {
    case ready
    case repairable
    case readyToApply
    case invalid
    case helperUnavailable
    case accountNotAlanManaged
    case ptySpawnFailed
    case destructiveConfirmation
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
        case .ptySpawnFailed:
            readinessState = .ptySpawnFailed
            repairState = "\(plan.request.accountName) failed helper-managed PTY verification."
            conflictState = nil
        case .requiresDestructiveConfirmation:
            readinessState = .destructiveConfirmation
            repairState = nil
            conflictState = nil
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

    var request: ManagedTerminalAccountRequest {
        ManagedTerminalAccountRequest(
            accountName: unixUserName.trimmingCharacters(in: .whitespacesAndNewlines),
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
        case .createOrUpdateTerminalProfile:
            return "Terminal Profile \(request.terminalProfileID)"
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
#endif
