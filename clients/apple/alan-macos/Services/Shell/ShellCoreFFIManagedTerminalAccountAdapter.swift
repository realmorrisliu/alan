import Foundation

extension ShellCoreFFIAdapter {
    func validateManagedTerminalAccountRequest(
        _ request: ManagedTerminalAccountRequest
    ) throws -> [ManagedTerminalAccountValidationError] {
        let response: ShellCoreManagedAccountValidationResponse = try send(
            operation: "managed_terminal_account.validate_request",
            payload: ShellCoreManagedAccountRequestPayload(request)
        )
        return response.errors.map(\.swiftError)
    }

    func managedTerminalAccountPlan(
        request: ManagedTerminalAccountRequest,
        state: ManagedTerminalAccountState
    ) throws -> ManagedTerminalAccountPlan {
        let response: ShellCoreManagedAccountPlanResponse = try send(
            operation: "managed_terminal_account.plan",
            payload: ShellCoreManagedAccountPlanPayload(request: request, state: state)
        )
        return response.plan.swiftPlan
    }
}

private struct ShellCoreManagedAccountValidationResponse: Decodable {
    let errors: [ShellCoreManagedAccountValidationError]
}

private struct ShellCoreManagedAccountPlanResponse: Decodable {
    let plan: ShellCoreManagedAccountPlanValue
}

private struct ShellCoreManagedAccountPlanPayload: Encodable {
    let request: ShellCoreManagedAccountRequestPayload
    let state: ShellCoreManagedAccountStatePayload

    init(request: ManagedTerminalAccountRequest, state: ManagedTerminalAccountState) {
        self.request = ShellCoreManagedAccountRequestPayload(request)
        self.state = ShellCoreManagedAccountStatePayload(state)
    }
}

private struct ShellCoreManagedAccountRequestPayload: Codable {
    let accountName: String
    let guiUserName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool
    let bindCurrentSpaceAfterSuccess: Bool

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case guiUserName = "gui_user_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
        case bindCurrentSpaceAfterSuccess = "bind_current_space_after_success"
    }

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        guiUserName = request.guiUserName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
        bindCurrentSpaceAfterSuccess = request.bindCurrentSpaceAfterSuccess
    }

    var swiftRequest: ManagedTerminalAccountRequest {
        ManagedTerminalAccountRequest(
            accountName: accountName,
            guiUserName: guiUserName,
            fullName: fullName,
            shell: shell,
            homeDirectory: homeDirectory,
            hideFromLoginWindow: hideFromLoginWindow,
            bindCurrentSpaceAfterSuccess: bindCurrentSpaceAfterSuccess
        )
    }
}

private struct ShellCoreManagedAccountStatePayload: Encodable {
    let account: ShellCoreManagedAccountRecordPayload
    let sudoers: ShellCoreManagedAccountSudoersStatePayload
    let terminalProfile: ShellCoreManagedAccountProfileStatePayload
    let verification: ShellCoreManagedAccountVerificationStatusPayload
    let homeDirectoryExists: Bool

    private enum CodingKeys: String, CodingKey {
        case account
        case sudoers
        case terminalProfile = "terminal_profile"
        case verification
        case homeDirectoryExists = "home_directory_exists"
    }

    init(_ state: ManagedTerminalAccountState) {
        account = ShellCoreManagedAccountRecordPayload(state.account)
        sudoers = ShellCoreManagedAccountSudoersStatePayload(state.sudoers)
        terminalProfile = ShellCoreManagedAccountProfileStatePayload(state.terminalProfile)
        verification = ShellCoreManagedAccountVerificationStatusPayload(state.verification)
        homeDirectoryExists = state.homeDirectoryExists
    }
}

private struct ShellCoreManagedAccountRecordPayload: Encodable {
    let state: String
    let homeDirectory: String?
    let shell: String?
    let hidden: Bool?
    let reason: String?

    private enum CodingKeys: String, CodingKey {
        case state
        case homeDirectory = "home_directory"
        case shell
        case hidden
        case reason
    }

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

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(homeDirectory, forKey: .homeDirectory)
        try container.encodeIfPresent(shell, forKey: .shell)
        try container.encodeIfPresent(hidden, forKey: .hidden)
        try container.encodeIfPresent(reason, forKey: .reason)
    }
}

private struct ShellCoreManagedAccountSudoersStatePayload: Encodable {
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

    func encode(to encoder: Encoder) throws {
        enum CodingKeys: String, CodingKey {
            case state
            case path
            case message
        }

        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(path, forKey: .path)
        try container.encodeIfPresent(message, forKey: .message)
    }
}

private struct ShellCoreManagedAccountProfileStatePayload: Encodable {
    let state: String
    let profileID: String?

    private enum CodingKeys: String, CodingKey {
        case state
        case profileID = "profile_id"
    }

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

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(state, forKey: .state)
        try container.encodeIfPresent(profileID, forKey: .profileID)
    }
}

private struct ShellCoreManagedAccountVerificationStatusPayload: Encodable {
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
            self.step = step.rawValue
            self.message = message
        }
    }

    func encode(to encoder: Encoder) throws {
        enum CodingKeys: String, CodingKey {
            case status
            case step
            case message
        }

        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(status, forKey: .status)
        try container.encodeIfPresent(step, forKey: .step)
        try container.encodeIfPresent(message, forKey: .message)
    }
}

private struct ShellCoreManagedAccountPlanValue: Decodable {
    let request: ShellCoreManagedAccountRequestPayload
    let status: ShellCoreManagedAccountPlanStatus
    let steps: [ShellCoreManagedAccountPlanStep]

    var swiftPlan: ManagedTerminalAccountPlan {
        ManagedTerminalAccountPlan(
            request: request.swiftRequest,
            status: status.swiftStatus,
            steps: steps.map(\.swiftStep)
        )
    }
}

private enum ShellCoreManagedAccountPlanStatus: Decodable {
    case readyToApply
    case alreadyReady
    case repair
    case invalid([ShellCoreManagedAccountValidationError])
    case requiresDestructiveConfirmation
    case sudoersConflict(path: String)
    case terminalProfileConflict(profileID: String)

    private enum CodingKeys: String, CodingKey {
        case type
        case errors
        case path
        case profileID = "profile_id"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .type) {
        case "ready_to_apply":
            self = .readyToApply
        case "already_ready":
            self = .alreadyReady
        case "repair":
            self = .repair
        case "invalid":
            self = .invalid(
                try container.decode([ShellCoreManagedAccountValidationError].self, forKey: .errors)
            )
        case "requires_destructive_confirmation":
            self = .requiresDestructiveConfirmation
        case "sudoers_conflict":
            self = .sudoersConflict(path: try container.decode(String.self, forKey: .path))
        case "terminal_profile_conflict":
            self = .terminalProfileConflict(
                profileID: try container.decode(String.self, forKey: .profileID)
            )
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported managed terminal account plan status"
                )
            )
        }
    }

    var swiftStatus: ManagedTerminalAccountPlanStatus {
        switch self {
        case .readyToApply:
            return .readyToApply
        case .alreadyReady:
            return .alreadyReady
        case .repair:
            return .repair
        case let .invalid(errors):
            return .invalid(errors.map(\.swiftError))
        case .requiresDestructiveConfirmation:
            return .requiresDestructiveConfirmation
        case let .sudoersConflict(path):
            return .sudoersConflict(path: path)
        case let .terminalProfileConflict(profileID):
            return .terminalProfileConflict(profileID: profileID)
        }
    }
}

private enum ShellCoreManagedAccountValidationError: Decodable {
    case invalidAccountName(String)
    case invalidGUIUserName(String)
    case reservedAccountName(String)
    case invalidShell(String)

    private enum CodingKeys: String, CodingKey {
        case type
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let value = try container.decode(String.self, forKey: .value)
        switch try container.decode(String.self, forKey: .type) {
        case "invalid_account_name":
            self = .invalidAccountName(value)
        case "invalid_gui_user_name":
            self = .invalidGUIUserName(value)
        case "reserved_account_name":
            self = .reservedAccountName(value)
        case "invalid_shell":
            self = .invalidShell(value)
        default:
            throw DecodingError.dataCorrupted(
                DecodingError.Context(
                    codingPath: decoder.codingPath,
                    debugDescription: "Unsupported managed terminal account validation error"
                )
            )
        }
    }

    var swiftError: ManagedTerminalAccountValidationError {
        switch self {
        case let .invalidAccountName(value):
            return .invalidAccountName(value)
        case let .invalidGUIUserName(value):
            return .invalidGUIUserName(value)
        case let .reservedAccountName(value):
            return .reservedAccountName(value)
        case let .invalidShell(value):
            return .invalidShell(value)
        }
    }
}

private struct ShellCoreManagedAccountPlanStep: Decodable {
    let kind: ShellCoreManagedAccountPlanStepKind
    let summary: String
    let requiresPrivilege: Bool

    private enum CodingKeys: String, CodingKey {
        case kind
        case summary
        case requiresPrivilege = "requires_privilege"
    }

    var swiftStep: ManagedTerminalAccountPlanStep {
        ManagedTerminalAccountPlanStep(
            kind: kind.swiftKind,
            summary: summary,
            requiresPrivilege: requiresPrivilege
        )
    }
}

private enum ShellCoreManagedAccountPlanStepKind: String, Decodable {
    case createStandardAccount = "create_standard_account"
    case repairAccountType = "repair_account_type"
    case repairHomeDirectory = "repair_home_directory"
    case repairShell = "repair_shell"
    case hideAccount = "hide_account"
    case writeSudoersDropIn = "write_sudoers_drop_in"
    case validateSudoers = "validate_sudoers"
    case verifyTerminalEntry = "verify_terminal_entry"
    case createOrUpdateTerminalProfile = "create_or_update_terminal_profile"
    case bindCurrentSpace = "bind_current_space"
    case removeSudoersDropIn = "remove_sudoers_drop_in"
    case removeManagedTerminalProfile = "remove_managed_terminal_profile"
    case deleteAccount = "delete_account"
    case deleteHomeDirectory = "delete_home_directory"

    var swiftKind: ManagedTerminalAccountPlanStepKind {
        switch self {
        case .createStandardAccount:
            return .createStandardAccount
        case .repairAccountType:
            return .repairAccountType
        case .repairHomeDirectory:
            return .repairHomeDirectory
        case .repairShell:
            return .repairShell
        case .hideAccount:
            return .hideAccount
        case .writeSudoersDropIn:
            return .writeSudoersDropIn
        case .validateSudoers:
            return .validateSudoers
        case .verifyTerminalEntry:
            return .verifyTerminalEntry
        case .createOrUpdateTerminalProfile:
            return .createOrUpdateTerminalProfile
        case .bindCurrentSpace:
            return .bindCurrentSpace
        case .removeSudoersDropIn:
            return .removeSudoersDropIn
        case .removeManagedTerminalProfile:
            return .removeManagedTerminalProfile
        case .deleteAccount:
            return .deleteAccount
        case .deleteHomeDirectory:
            return .deleteHomeDirectory
        }
    }
}
