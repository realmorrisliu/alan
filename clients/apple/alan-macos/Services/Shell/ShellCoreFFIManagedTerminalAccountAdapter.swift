import Foundation

struct ShellCoreManagedTerminalAccountAdapter {
    private let adapter: ShellCoreFFIAdapter?

    init(adapter: ShellCoreFFIAdapter? = nil) {
        self.adapter = adapter
    }

    func validateManagedTerminalAccountRequest(
        _ request: ManagedTerminalAccountRequest
    ) throws -> [ManagedTerminalAccountValidationError] {
        let ffi = try adapter ?? ShellCoreFFIAdapter.shared
        let response: ShellCoreManagedAccountValidationResponse = try ffi.send(
            operation: "managed_terminal_account.validate_request",
            payload: ShellCoreManagedAccountRequestPayload(request)
        )
        return response.errors.map(\.swiftError)
    }

    func managedTerminalAccountPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        terminalProfiles: TerminalProfileDocument?
    ) throws -> ManagedTerminalAccountPlan {
        try sendManagedTerminalAccountPlan(
            ShellCoreManagedAccountPlanPayload(
                type: "provision",
                request: request,
                diagnosis: diagnosis,
                scope: nil,
                terminalProfiles: terminalProfiles
            )
        )
    }

    func managedTerminalAccountRollbackPlan(
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ManagedTerminalAccountRollbackScope,
        terminalProfiles: TerminalProfileDocument?
    ) throws -> ManagedTerminalAccountPlan {
        try sendManagedTerminalAccountPlan(
            ShellCoreManagedAccountPlanPayload(
                type: "rollback",
                request: request,
                diagnosis: diagnosis,
                scope: ShellCoreManagedAccountRollbackScopePayload(scope),
                terminalProfiles: terminalProfiles
            )
        )
    }

    private func sendManagedTerminalAccountPlan(
        _ payload: ShellCoreManagedAccountPlanPayload
    ) throws -> ManagedTerminalAccountPlan {
        let ffi = try adapter ?? ShellCoreFFIAdapter.shared
        let response: ShellCoreManagedAccountPlanResponse = try ffi.send(
            operation: "managed_terminal_account.plan",
            payload: payload
        )
        return try response.plan.swiftPlan
    }
}

private struct ShellCoreManagedAccountValidationResponse: Decodable {
    let errors: [ShellCoreManagedAccountValidationError]
}

private struct ShellCoreManagedAccountPlanResponse: Decodable {
    let plan: ShellCoreManagedAccountPlan
}

private struct ShellCoreManagedAccountPlan: Decodable {
    let request: ShellCoreManagedAccountRequestPayload
    let status: ShellCoreManagedAccountPlanStatus
    let steps: [ShellCoreManagedAccountPlanStep]

    var swiftPlan: ManagedTerminalAccountPlan {
        get throws {
            ManagedTerminalAccountPlan(
                request: request.swiftRequest,
                status: status.swiftStatus,
                steps: try steps.map { try $0.swiftStep }
            )
        }
    }
}

private struct ShellCoreManagedAccountPlanPayload: Encodable {
    let type: String
    let request: ShellCoreManagedAccountRequestPayload
    let diagnosis: ShellCoreManagedAccountDiagnosisPayload
    let scope: ShellCoreManagedAccountRollbackScopePayload?
    let terminalProfiles: TerminalProfileDocument?

    private enum CodingKeys: String, CodingKey {
        case type
        case request
        case diagnosis
        case scope
        case terminalProfiles = "terminal_profiles"
    }

    init(
        type: String,
        request: ManagedTerminalAccountRequest,
        diagnosis: AlanManagedUserDiagnosis,
        scope: ShellCoreManagedAccountRollbackScopePayload?,
        terminalProfiles: TerminalProfileDocument?
    ) {
        self.type = type
        self.request = ShellCoreManagedAccountRequestPayload(request)
        self.diagnosis = ShellCoreManagedAccountDiagnosisPayload(diagnosis)
        self.scope = scope
        self.terminalProfiles = terminalProfiles
    }
}

private struct ShellCoreManagedAccountRequestPayload: Codable {
    let accountName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    private enum CodingKeys: String, CodingKey {
        case accountName = "account_name"
        case fullName = "full_name"
        case shell
        case homeDirectory = "home_directory"
        case hideFromLoginWindow = "hide_from_login_window"
    }

    init(_ request: ManagedTerminalAccountRequest) {
        accountName = request.accountName
        fullName = request.fullName
        shell = request.shell
        homeDirectory = request.homeDirectory
        hideFromLoginWindow = request.hideFromLoginWindow
    }

    var swiftRequest: ManagedTerminalAccountRequest {
        ManagedTerminalAccountRequest(
            accountName: accountName,
            fullName: fullName,
            shell: shell,
            homeDirectory: homeDirectory,
            hideFromLoginWindow: hideFromLoginWindow
        )
    }
}

private struct ShellCoreManagedAccountDiagnosisPayload: Encodable {
    let ownershipState: String
    let readinessState: String
    let accountExists: Bool
    let isAdmin: Bool
    let homeDirectoryExists: Bool
    let homeDirectoryMatches: Bool
    let shellMatches: Bool
    let hiddenFromLoginWindow: Bool
    let terminalProfileID: String?
    let ptySmokeVerified: Bool

    private enum CodingKeys: String, CodingKey {
        case ownershipState = "ownership_state"
        case readinessState = "readiness_state"
        case accountExists = "account_exists"
        case isAdmin = "is_admin"
        case homeDirectoryExists = "home_directory_exists"
        case homeDirectoryMatches = "home_directory_matches"
        case shellMatches = "shell_matches"
        case hiddenFromLoginWindow = "hidden_from_login_window"
        case terminalProfileID = "terminal_profile_id"
        case ptySmokeVerified = "pty_smoke_verified"
    }

    init(_ diagnosis: AlanManagedUserDiagnosis) {
        ownershipState = diagnosis.ownershipState.rawValue
        readinessState = diagnosis.readinessState.rawValue
        accountExists = diagnosis.accountExists
        isAdmin = diagnosis.isAdmin
        homeDirectoryExists = diagnosis.homeDirectoryExists
        homeDirectoryMatches = diagnosis.homeDirectoryMatches
        shellMatches = diagnosis.shellMatches
        hiddenFromLoginWindow = diagnosis.hiddenFromLoginWindow
        terminalProfileID = diagnosis.terminalProfileID
        ptySmokeVerified = diagnosis.ptySmokeVerified
    }
}

private struct ShellCoreManagedAccountRollbackScopePayload: Encodable {
    let type: String
    let confirmation: String?

    init(_ scope: ManagedTerminalAccountRollbackScope) {
        switch scope {
        case .alanIntegrationOnly:
            type = "alan_integration_only"
            confirmation = nil
        case let .deleteAccountAndHome(value):
            type = "delete_account_and_home"
            confirmation = value
        }
    }
}

private enum ShellCoreManagedAccountPlanStatus: Decodable {
    case readyToApply
    case alreadyReady
    case repair
    case invalid([ShellCoreManagedAccountValidationError])
    case requiresDestructiveConfirmation
    case terminalProfileConflict(String)
    case helperUnavailable
    case accountNotAlanManaged
    case ptySpawnFailed

    private enum CodingKeys: String, CodingKey {
        case type
        case errors
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
                try container.decode(
                    [ShellCoreManagedAccountValidationError].self,
                    forKey: .errors
                )
            )
        case "requires_destructive_confirmation":
            self = .requiresDestructiveConfirmation
        case "terminal_profile_conflict":
            self = .terminalProfileConflict(
                try container.decode(String.self, forKey: .profileID)
            )
        case "helper_unavailable":
            self = .helperUnavailable
        case "account_not_alan_managed":
            self = .accountNotAlanManaged
        case "pty_spawn_failed":
            self = .ptySpawnFailed
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type,
                in: container,
                debugDescription: "Unsupported managed terminal account plan status"
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
        case let .terminalProfileConflict(profileID):
            return .terminalProfileConflict(profileID: profileID)
        case .helperUnavailable:
            return .helperUnavailable
        case .accountNotAlanManaged:
            return .accountNotAlanManaged
        case .ptySpawnFailed:
            return .ptySpawnFailed
        }
    }
}

private struct ShellCoreManagedAccountPlanStep: Decodable {
    let kind: String
    let summary: String
    let requiresPrivilege: Bool

    private enum CodingKeys: String, CodingKey {
        case kind
        case summary
        case requiresPrivilege = "requires_privilege"
    }

    var swiftStep: ManagedTerminalAccountPlanStep {
        get throws {
            ManagedTerminalAccountPlanStep(
                kind: try swiftKind,
                summary: summary,
                requiresPrivilege: requiresPrivilege
            )
        }
    }

    private var swiftKind: ManagedTerminalAccountPlanStepKind {
        get throws {
            switch kind {
            case "create_or_update_terminal_profile":
                return .createOrUpdateTerminalProfile
            case "remove_managed_terminal_profile":
                return .removeManagedTerminalProfile
            case "create_standard_account":
                return .helperStep(.createStandardAccount)
            case "repair_account_type":
                return .helperStep(.repairAccountType)
            case "repair_home_directory":
                return .helperStep(.repairHomeDirectory)
            case "repair_shell":
                return .helperStep(.repairShell)
            case "hide_account":
                return .helperStep(.hideAccount)
            case "write_ownership_marker":
                return .helperStep(.writeOwnershipMarker)
            case "verify_account":
                return .helperStep(.verifyAccount)
            case "verify_managed_user_pty":
                return .helperStep(.verifyManagedUserPTY)
            case "remove_managed_user_integration":
                return .helperStep(.removeManagedUserIntegration)
            case "delete_account":
                return .helperStep(.deleteAccount)
            case "delete_home_directory":
                return .helperStep(.deleteHomeDirectory)
            default:
                throw DecodingError.dataCorrupted(
                    DecodingError.Context(
                        codingPath: [],
                        debugDescription: "Unsupported managed terminal account plan step \(kind)"
                    )
                )
            }
        }
    }
}

private enum ShellCoreManagedAccountValidationError: Decodable {
    case invalidAccountName(String)
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
        case let .reservedAccountName(value):
            return .reservedAccountName(value)
        case let .invalidShell(value):
            return .invalidShell(value)
        }
    }
}
