import Foundation

private func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    guard condition() else {
        fputs("terminal-account-dev-dry-run-smoke: \(message)\n", stderr)
        exit(1)
    }
}

@main
private enum TerminalAccountDevDryRunSmoke {
    static func main() throws {
        let fileManager = FileManager.default
        let root = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("alan-terminal-account-dev-dry-run-\(UUID().uuidString)", isDirectory: true)
        let appSupport = root.appendingPathComponent("app-support", isDirectory: true)
        let devProfileStore = appSupport
            .appendingPathComponent("alan-macos-dev", isDirectory: true)
            .appendingPathComponent("terminal-profiles.json", isDirectory: false)
        let stableProfileStore = appSupport
            .appendingPathComponent("alan-macos", isDirectory: true)
            .appendingPathComponent("terminal-profiles.json", isDirectory: false)

        try fileManager.createDirectory(at: appSupport, withIntermediateDirectories: true)

        let request = ManagedTerminalAccountRequest(
            accountName: "alan_smoke",
            guiUserName: "morris",
            fullName: "Alan Smoke",
            shell: "/bin/zsh",
            homeDirectory: "/Users/alan_smoke",
            hideFromLoginWindow: true,
            bindCurrentSpaceAfterSuccess: true
        )
        let missingState = ManagedTerminalAccountState(
            account: .missing,
            sudoers: .missing,
            terminalProfile: .missing,
            verification: .notRun
        )
        let plan = ManagedTerminalAccountPlanner.plan(request: request, state: missingState)
        let planKinds = plan.steps.map(\.kind)

        expect(plan.status == .readyToApply, "missing account dry run must be ready to apply")
        expect(planKinds.contains(.createStandardAccount), "dry run must include account creation")
        expect(planKinds.contains(.hideAccount), "dry run must include login-window hiding")
        expect(planKinds.contains(.writeSudoersDropIn), "dry run must include sudoers write")
        expect(planKinds.contains(.validateSudoers), "dry run must include sudoers validation")
        expect(planKinds.contains(.verifyTerminalEntry), "dry run must include terminal entry verification")
        expect(planKinds.contains(.createOrUpdateTerminalProfile), "dry run must include profile handoff")
        expect(planKinds.contains(.bindCurrentSpace), "dry run must include explicit Space binding step")

        let rule = ManagedTerminalAccountSudoersRule(request: request)
        expect(
            rule.filePath == "/etc/sudoers.d/alan-terminal-morris-to-alan_smoke",
            "sudoers path must be deterministic and Alan-owned"
        )
        expect(
            rule.contents.contains("morris ALL=(alan_smoke) NOPASSWD: ALL"),
            "sudoers rule must target only the managed account"
        )
        expect(!rule.contents.contains("ALL=(ALL)"), "sudoers rule must not grant passwordless root")
        expect(
            !rule.contents.contains("morris ALL=(root)"),
            "sudoers rule must not grant direct root entry"
        )

        let cancelledExecutor = ManagedTerminalAccountFakeExecutor()
        cancelledExecutor.cancelBeforeApply = true
        let cancelled = cancelledExecutor.apply(plan)
        expect(cancelled.cancelled, "cancelled preview must not apply privileged changes")
        expect(cancelled.completedSteps.isEmpty, "cancelled preview must not complete steps")
        expect(
            !fileManager.fileExists(atPath: devProfileStore.path),
            "cancelled dry run must not create a dev profile store"
        )
        expect(
            !fileManager.fileExists(atPath: stableProfileStore.path),
            "cancelled dry run must not create a stable profile store"
        )

        let readyState = ManagedTerminalAccountState(
            account: .standard(homeDirectory: "/Users/alan_smoke", shell: "/bin/zsh", hidden: true),
            sudoers: .alanOwnedValid(path: rule.filePath),
            terminalProfile: .missing,
            verification: .passed
        )
        guard let handoff = ManagedTerminalAccountProfileHandoff.profileDefinition(
            for: request,
            state: readyState
        ) else {
            fputs("terminal-account-dev-dry-run-smoke: ready state did not produce handoff profile\n", stderr)
            exit(1)
        }

        let store = TerminalProfileStore.defaultStore(
            channelApplicationSupportDirectoryName: "alan-macos-dev",
            fileManager: fileManager,
            environment: [
                "ALAN_INSTALL_CHANNEL": "dev",
                "ALAN_MACOS_APPLICATION_SUPPORT_DIR": appSupport.path,
            ]
        )
        try store.save(TerminalProfileDocument(defaultProfileID: handoff.id, profiles: [handoff]))
        let loaded = store.load().document.profile(id: "alan_smoke")

        expect(fileManager.fileExists(atPath: devProfileStore.path), "handoff must write dev profile store")
        expect(
            !fileManager.fileExists(atPath: stableProfileStore.path),
            "dev-channel handoff must not create stable profile store"
        )
        expect(loaded?.managedTerminalAccountID == "alan_smoke", "profile must link managed account")
        expect(loaded?.launch == .sudoUser(unixUser: "alan_smoke"), "profile must use sudo_user launch")

        try TerminalAccountFixtureExporter.exportIfRequested(
            request: request,
            missingState: missingState,
            readyState: readyState,
            plan: plan,
            rule: rule,
            cancelled: cancelled,
            handoff: handoff
        )

        print("terminal account dev dry-run smoke passed")
        print("tmp_root=\(root.path)")
        print("dev_profile_store=\(devProfileStore.path)")
    }
}

private enum TerminalAccountFixtureExporter {
    static func exportIfRequested(
        request: ManagedTerminalAccountRequest,
        missingState: ManagedTerminalAccountState,
        readyState: ManagedTerminalAccountState,
        plan: ManagedTerminalAccountPlan,
        rule: ManagedTerminalAccountSudoersRule,
        cancelled: ManagedTerminalAccountApplyResult,
        handoff: TerminalProfileDefinition
    ) throws {
        guard let rootPath = ProcessInfo.processInfo.environment["ALAN_TERMINAL_ACCOUNT_FIXTURE_DIR"],
              !rootPath.isEmpty
        else {
            return
        }

        let fixture = ShellCoreFixtureCase(
            id: "terminal-profile/managed-account-dev-dry-run",
            kind: "terminal_profile",
            description: "Managed terminal account dry-run plans privileged steps and profile handoff without applying local effects.",
            input: ManagedTerminalAccountDryRunFixtureInput(
                request: request,
                missingState: missingState,
                readyState: readyState,
                cancelBeforeApply: true
            ),
            operation: ManagedAccountDryRunOperation(),
            expected: ManagedTerminalAccountDryRunExpectation(
                plan: plan,
                rule: rule,
                cancelled: cancelled,
                handoff: handoff
            )
        )

        let fixtureURL = URL(fileURLWithPath: rootPath)
            .appendingPathComponent(fixture.id)
            .appendingPathExtension("json")
        try FileManager.default.createDirectory(
            at: fixtureURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(fixture).write(to: fixtureURL, options: .atomic)
        print("Terminal account fixtures exported to \(rootPath).")
    }
}

private struct ShellCoreFixtureCase: Encodable {
    let id: String
    let kind: String
    let source = "swift"
    let description: String
    let input: AnyEncodable
    let operation: AnyEncodable
    let expected: AnyEncodable

    init<Input: Encodable, Operation: Encodable, Expected: Encodable>(
        id: String,
        kind: String,
        description: String,
        input: Input,
        operation: Operation,
        expected: Expected
    ) {
        self.id = id
        self.kind = kind
        self.description = description
        self.input = AnyEncodable(input)
        self.operation = AnyEncodable(operation)
        self.expected = AnyEncodable(expected)
    }
}

private struct AnyEncodable: Encodable {
    private let encodeValue: (Encoder) throws -> Void

    init<Value: Encodable>(_ value: Value) {
        encodeValue = value.encode(to:)
    }

    func encode(to encoder: Encoder) throws {
        try encodeValue(encoder)
    }
}

private struct ManagedAccountDryRunOperation: Encodable {
    let type = "managed_account_dry_run"
}

private struct ManagedTerminalAccountDryRunFixtureInput: Encodable {
    let request: PortableManagedTerminalAccountRequest
    let missingState: PortableManagedTerminalAccountState
    let readyState: PortableManagedTerminalAccountState
    let cancelBeforeApply: Bool

    init(
        request: ManagedTerminalAccountRequest,
        missingState: ManagedTerminalAccountState,
        readyState: ManagedTerminalAccountState,
        cancelBeforeApply: Bool
    ) {
        self.request = PortableManagedTerminalAccountRequest(request)
        self.missingState = PortableManagedTerminalAccountState(missingState)
        self.readyState = PortableManagedTerminalAccountState(readyState)
        self.cancelBeforeApply = cancelBeforeApply
    }

    private enum CodingKeys: String, CodingKey {
        case request
        case missingState = "missing_state"
        case readyState = "ready_state"
        case cancelBeforeApply = "cancel_before_apply"
    }
}

private struct ManagedTerminalAccountDryRunExpectation: Encodable {
    let plan: PortableManagedTerminalAccountPlan
    let sudoersRule: PortableManagedTerminalAccountSudoersRule
    let cancelledApplyResult: PortableManagedTerminalAccountApplyResult
    let profileHandoff: TerminalProfileDefinition

    init(
        plan: ManagedTerminalAccountPlan,
        rule: ManagedTerminalAccountSudoersRule,
        cancelled: ManagedTerminalAccountApplyResult,
        handoff: TerminalProfileDefinition
    ) {
        self.plan = PortableManagedTerminalAccountPlan(plan)
        sudoersRule = PortableManagedTerminalAccountSudoersRule(rule)
        cancelledApplyResult = PortableManagedTerminalAccountApplyResult(cancelled)
        profileHandoff = handoff
    }

    private enum CodingKeys: String, CodingKey {
        case plan
        case sudoersRule = "sudoers_rule"
        case cancelledApplyResult = "cancelled_apply_result"
        case profileHandoff = "profile_handoff"
    }
}

private struct PortableManagedTerminalAccountRequest: Encodable {
    let accountName: String
    let guiUserName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool
    let bindCurrentSpaceAfterSuccess: Bool

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        guiUserName = request.guiUserName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
        bindCurrentSpaceAfterSuccess = request.bindCurrentSpaceAfterSuccess
    }

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case guiUserName = "gui_user_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
        case bindCurrentSpaceAfterSuccess = "bind_current_space_after_success"
    }
}

private struct PortableManagedTerminalAccountState: Encodable {
    let account: PortableManagedTerminalAccountRecord
    let sudoers: PortableManagedTerminalAccountSudoersState
    let terminalProfile: PortableManagedTerminalAccountProfileState
    let verification: PortableManagedTerminalAccountVerificationStatus

    init(_ state: ManagedTerminalAccountState) {
        account = PortableManagedTerminalAccountRecord(state.account)
        sudoers = PortableManagedTerminalAccountSudoersState(state.sudoers)
        terminalProfile = PortableManagedTerminalAccountProfileState(state.terminalProfile)
        verification = PortableManagedTerminalAccountVerificationStatus(state.verification)
    }

    private enum CodingKeys: String, CodingKey {
        case account
        case sudoers
        case terminalProfile = "terminal_profile"
        case verification
    }
}

private struct PortableManagedTerminalAccountRecord: Encodable {
    let state: String
    let homeDirectory: String?
    let shell: String?
    let hidden: Bool?
    let reason: String?

    init(_ record: ManagedTerminalAccountRecord) {
        switch record {
        case .missing:
            state = "missing"
            homeDirectory = nil
            shell = nil
            hidden = nil
            reason = nil
        case let .standard(homeDirectory, shell, hidden):
            state = "standard"
            self.homeDirectory = homeDirectory
            self.shell = shell
            self.hidden = hidden
            reason = nil
        case let .admin(homeDirectory, shell, hidden):
            state = "admin"
            self.homeDirectory = homeDirectory
            self.shell = shell
            self.hidden = hidden
            reason = nil
        case let .invalid(reason):
            state = "invalid"
            homeDirectory = nil
            shell = nil
            hidden = nil
            self.reason = reason
        }
    }

    private enum CodingKeys: String, CodingKey {
        case state
        case homeDirectory = "home_directory"
        case shell
        case hidden
        case reason
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(homeDirectory, forKey: .homeDirectory)
        try container.encodeIfPresent(shell, forKey: .shell)
        try container.encodeIfPresent(hidden, forKey: .hidden)
        try container.encodeIfPresent(reason, forKey: .reason)
    }
}

private struct PortableManagedTerminalAccountSudoersState: Encodable {
    let state: String
    let path: String?
    let message: String?

    init(_ sudoers: ManagedTerminalAccountSudoersState) {
        switch sudoers {
        case .missing:
            state = "missing"
            path = nil
            message = nil
        case let .alanOwnedValid(path):
            state = "alan_owned_valid"
            self.path = path
            message = nil
        case let .alanOwnedInvalid(path, message):
            state = "alan_owned_invalid"
            self.path = path
            self.message = message
        case let .unmanaged(path):
            state = "unmanaged"
            self.path = path
            message = nil
        case let .existingUnreadable(path):
            state = "existing_unreadable"
            self.path = path
            message = nil
        }
    }

    private enum CodingKeys: String, CodingKey {
        case state
        case path
        case message
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(path, forKey: .path)
        try container.encodeIfPresent(message, forKey: .message)
    }
}

private struct PortableManagedTerminalAccountProfileState: Encodable {
    let state: String
    let profileID: String?

    init(_ profile: ManagedTerminalAccountProfileState) {
        switch profile {
        case .missing:
            state = "missing"
            profileID = nil
        case let .existingManaged(profileID):
            state = "existing_managed"
            self.profileID = profileID
        case let .existingManagedOutdated(profileID):
            state = "existing_managed_outdated"
            self.profileID = profileID
        case let .existingUnmanaged(profileID):
            state = "existing_unmanaged"
            self.profileID = profileID
        }
    }

    private enum CodingKeys: String, CodingKey {
        case state
        case profileID = "profile_id"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(profileID, forKey: .profileID)
    }
}

private struct PortableManagedTerminalAccountVerificationStatus: Encodable {
    let status: String
    let step: String?
    let message: String?

    init(_ verification: ManagedTerminalAccountVerificationStatus) {
        switch verification {
        case .notRun:
            status = "not_run"
            step = nil
            message = nil
        case .passed:
            status = "passed"
            step = nil
            message = nil
        case let .failed(step, message):
            status = "failed"
            self.step = stepID(step)
            self.message = message
        }
    }

    private enum CodingKeys: String, CodingKey {
        case status
        case step
        case message
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(status, forKey: .status)
        try container.encodeIfPresent(step, forKey: .step)
        try container.encodeIfPresent(message, forKey: .message)
    }
}

private struct PortableManagedTerminalAccountPlan: Encodable {
    let status: String
    let steps: [PortableManagedTerminalAccountPlanStep]

    init(_ plan: ManagedTerminalAccountPlan) {
        status = planStatusID(plan.status)
        steps = plan.steps.map(PortableManagedTerminalAccountPlanStep.init)
    }
}

private struct PortableManagedTerminalAccountPlanStep: Encodable {
    let kind: String
    let summary: String
    let requiresPrivilege: Bool

    init(_ step: ManagedTerminalAccountPlanStep) {
        kind = stepKindID(step.kind)
        summary = step.summary
        requiresPrivilege = step.requiresPrivilege
    }

    private enum CodingKeys: String, CodingKey {
        case kind
        case summary
        case requiresPrivilege = "requires_privilege"
    }
}

private struct PortableManagedTerminalAccountSudoersRule: Encodable {
    let fileName: String
    let filePath: String
    let contents: String

    init(_ rule: ManagedTerminalAccountSudoersRule) {
        fileName = rule.fileName
        filePath = rule.filePath
        contents = rule.contents
    }

    private enum CodingKeys: String, CodingKey {
        case fileName = "file_name"
        case filePath = "file_path"
        case contents
    }
}

private struct PortableManagedTerminalAccountApplyResult: Encodable {
    let completedSteps: [String]
    let failedStep: String?
    let cancelled: Bool
    let visibleDiagnostics: [String]

    init(_ result: ManagedTerminalAccountApplyResult) {
        completedSteps = result.completedSteps.map(stepKindID)
        failedStep = result.failedStep.map(stepKindID)
        cancelled = result.cancelled
        visibleDiagnostics = result.visibleDiagnostics
    }

    private enum CodingKeys: String, CodingKey {
        case completedSteps = "completed_steps"
        case failedStep = "failed_step"
        case cancelled
        case visibleDiagnostics = "visible_diagnostics"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(completedSteps, forKey: .completedSteps)
        try container.encodeIfPresent(failedStep, forKey: .failedStep)
        try container.encode(cancelled, forKey: .cancelled)
        try container.encode(visibleDiagnostics, forKey: .visibleDiagnostics)
    }
}

private func planStatusID(_ status: ManagedTerminalAccountPlanStatus) -> String {
    switch status {
    case .readyToApply:
        return "ready_to_apply"
    case .alreadyReady:
        return "already_ready"
    case .repair:
        return "repair"
    case .invalid:
        return "invalid"
    case .requiresDestructiveConfirmation:
        return "requires_destructive_confirmation"
    case .sudoersConflict:
        return "sudoers_conflict"
    case .terminalProfileConflict:
        return "terminal_profile_conflict"
    }
}

private func stepKindID(_ kind: ManagedTerminalAccountPlanStepKind) -> String {
    switch kind {
    case .createStandardAccount:
        return "create_standard_account"
    case .repairAccountType:
        return "repair_account_type"
    case .repairHomeDirectory:
        return "repair_home_directory"
    case .repairShell:
        return "repair_shell"
    case .hideAccount:
        return "hide_account"
    case .writeSudoersDropIn:
        return "write_sudoers_drop_in"
    case .validateSudoers:
        return "validate_sudoers"
    case .verifyTerminalEntry:
        return "verify_terminal_entry"
    case .createOrUpdateTerminalProfile:
        return "create_or_update_terminal_profile"
    case .bindCurrentSpace:
        return "bind_current_space"
    case .removeSudoersDropIn:
        return "remove_sudoers_drop_in"
    case .removeManagedTerminalProfile:
        return "remove_managed_terminal_profile"
    case .deleteAccount:
        return "delete_account"
    case .deleteHomeDirectory:
        return "delete_home_directory"
    }
}

private func stepID(_ step: ManagedTerminalAccountVerificationStep) -> String {
    switch step {
    case .accountLookup:
        return "account_lookup"
    case .nonAdminAccount:
        return "non_admin_account"
    case .homeDirectory:
        return "home_directory"
    case .shell:
        return "shell"
    case .sudoersValidation:
        return "sudoers_validation"
    case .nonInteractiveSudo:
        return "non_interactive_sudo"
    }
}
