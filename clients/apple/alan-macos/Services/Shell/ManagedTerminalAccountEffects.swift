import Foundation

struct ManagedTerminalAccountApplyResult: Equatable {
    let completedSteps: [ManagedTerminalAccountPlanStepKind]
    let failedStep: ManagedTerminalAccountPlanStepKind?
    let cancelled: Bool
    let visibleDiagnostics: [String]

    static func cancelled(before steps: [ManagedTerminalAccountPlanStepKind]) -> ManagedTerminalAccountApplyResult {
        ManagedTerminalAccountApplyResult(
            completedSteps: [],
            failedStep: steps.first,
            cancelled: true,
            visibleDiagnostics: ["Provisioning cancelled before privileged changes."]
        )
    }
}

protocol ManagedTerminalAccountPrivilegedExecuting {
    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult
}

struct ManagedTerminalAccountLocalEffectResult: Equatable {
    let succeeded: Bool
    let redactedMessage: String

    static func succeeded(_ redactedMessage: String) -> ManagedTerminalAccountLocalEffectResult {
        ManagedTerminalAccountLocalEffectResult(succeeded: true, redactedMessage: redactedMessage)
    }

    static func failed(_ redactedMessage: String) -> ManagedTerminalAccountLocalEffectResult {
        ManagedTerminalAccountLocalEffectResult(succeeded: false, redactedMessage: redactedMessage)
    }
}

protocol ManagedTerminalAccountLocalEffectExecuting {
    func apply(
        _ step: ManagedTerminalAccountPlanStepKind,
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult?
}

struct ManagedTerminalAccountTerminalProfileEffectExecutor: ManagedTerminalAccountLocalEffectExecuting {
    let store: TerminalProfileStore

    init(
        store: TerminalProfileStore = .defaultStore()
    ) {
        self.store = store
    }

    func apply(
        _ step: ManagedTerminalAccountPlanStepKind,
        request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult? {
        switch step {
        case .createOrUpdateTerminalProfile:
            return createOrUpdateTerminalProfile(for: request)
        case .removeManagedTerminalProfile:
            return removeManagedTerminalProfile(for: request)
        case .createStandardAccount, .repairAccountType, .repairHomeDirectory, .repairShell,
                .hideAccount, .deleteAccount, .deleteHomeDirectory, .helperStep:
            return nil
        }
    }

    private func createOrUpdateTerminalProfile(
        for request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult {
        let document = store.load().document
        let draft = TerminalProfileEditorDraft(
            id: request.terminalProfileID,
            title: request.fullName ?? request.accountName,
            launchKind: .managedUser,
            unixUser: request.accountName,
            customCommand: "",
            defaultWorkingDirectory: request.homeDirectory,
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: nil
            ),
            managedTerminalAccountID: request.accountName
        )
        let editorResult = TerminalProfileEditor.upserting(draft: draft, into: document)
        guard let nextDocument = editorResult.document else {
            return .failed("Terminal Profile handoff failed. Credentials redacted.")
        }
        return save(
            nextDocument,
            successMessage: "Terminal Profile handoff completed. Credentials redacted.",
            failureMessage: "Terminal Profile handoff failed. Credentials redacted."
        )
    }

    private func removeManagedTerminalProfile(
        for request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountLocalEffectResult {
        let document = store.load().document
        let remainingProfiles = document.profiles.filter {
            $0.managedTerminalAccountID != request.accountName
        }

        guard remainingProfiles.count != document.profiles.count else {
            return .succeeded("Managed Terminal Profile was already absent. Credentials redacted.")
        }

        let nextDocument: TerminalProfileDocument
        if remainingProfiles.isEmpty {
            nextDocument = .fallback
        } else {
            let defaultProfileID = remainingProfiles.contains { $0.id == document.defaultProfileID }
                ? document.defaultProfileID
                : remainingProfiles[0].id
            nextDocument = TerminalProfileDocument(
                defaultProfileID: defaultProfileID,
                profiles: remainingProfiles
            )
        }

        return save(
            nextDocument,
            successMessage: "Managed Terminal Profile removal completed. Credentials redacted.",
            failureMessage: "Managed Terminal Profile removal failed. Credentials redacted."
        )
    }

    private func save(
        _ document: TerminalProfileDocument,
        successMessage: String,
        failureMessage: String
    ) -> ManagedTerminalAccountLocalEffectResult {
        do {
            try store.save(document)
            return .succeeded(successMessage)
        } catch {
            return .failed(failureMessage)
        }
    }
}

struct ManagedTerminalAccountHelperExecutor: ManagedTerminalAccountPrivilegedExecuting {
    let channel: AlanInstallChannel
    let helperClient: AlanPrivilegedHelperClienting
    let localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting

    init(
        channel: AlanInstallChannel = .current(),
        helperClient: AlanPrivilegedHelperClienting,
        localEffectExecutor: ManagedTerminalAccountLocalEffectExecuting =
            ManagedTerminalAccountTerminalProfileEffectExecutor()
    ) {
        self.channel = channel
        self.helperClient = helperClient
        self.localEffectExecutor = localEffectExecutor
    }

    func apply(_ plan: ManagedTerminalAccountPlan) -> ManagedTerminalAccountApplyResult {
        if let rejectedStep = plan.steps.first(where: rejectsUnscopedPrivilegedStep) {
            return ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: rejectedStep.kind,
                cancelled: false,
                visibleDiagnostics: [
                    "Helper-backed Managed User plan rejected an unscoped privileged step. Credentials redacted.",
                ]
            )
        }

        let helperPlan = AlanManagedUserHelperPlan(
            operationID: UUID().uuidString,
            channelID: channel.installChannelID,
            request: plan.request,
            steps: plan.steps.compactMap(helperPlanStep)
        )

        var completed: [ManagedTerminalAccountPlanStepKind] = []
        if !helperPlan.steps.isEmpty {
            let helperResult = helperClient.applyManagedUserPlan(helperPlan)
            completed.append(contentsOf: helperResult.completedSteps)
            if helperResult.failedStep != nil || helperResult.cancelled {
                return helperResult
            }
        }

        for step in plan.steps where !step.requiresPrivilege {
            guard let result = localEffectExecutor.apply(step.kind, request: plan.request) else {
                continue
            }
            guard result.succeeded else {
                return ManagedTerminalAccountApplyResult(
                    completedSteps: completed,
                    failedStep: step.kind,
                    cancelled: false,
                    visibleDiagnostics: [result.redactedMessage]
                )
            }
            completed.append(step.kind)
        }

        return ManagedTerminalAccountApplyResult(
            completedSteps: completed,
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Helper-backed Managed User plan applied. Credentials redacted."]
        )
    }

    private func rejectsUnscopedPrivilegedStep(_ step: ManagedTerminalAccountPlanStep) -> Bool {
        guard step.requiresPrivilege else { return false }
        if case .helperStep = step.kind {
            return false
        }
        return true
    }

    private func helperPlanStep(
        _ step: ManagedTerminalAccountPlanStep
    ) -> AlanManagedUserHelperPlanStep? {
        guard case .helperStep(let kind) = step.kind else { return nil }
        return AlanManagedUserHelperPlanStep(
            kind: kind,
            summary: step.summary,
            requiresDestructiveConfirmation: kind == .deleteAccount || kind == .deleteHomeDirectory
        )
    }
}
