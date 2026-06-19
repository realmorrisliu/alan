// Script/test support only. Production managed-account semantics are owned by shell-core.
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
            launch: .sudoUser(unixUser: request.accountName),
            defaultWorkingDirectory: request.homeDirectory,
            presentation: TerminalProfilePresentation(
                symbolName: "person.crop.circle",
                colorName: nil
            ),
            managedTerminalAccountID: request.accountName
        )
    }
}
