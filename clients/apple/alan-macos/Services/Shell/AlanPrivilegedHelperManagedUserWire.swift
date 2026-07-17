import Foundation

struct AlanXPCManagedTerminalAccountRequest: Codable, Equatable {
    let accountName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    var terminalProfileID: String { accountName }
    var canonicalHomeDirectory: String { "/Users/\(accountName)" }
}

enum AlanXPCManagedUserOwnershipState: String, Codable, Equatable {
    case missing
    case alanManaged = "alan_managed"
    case notAlanManaged = "not_alan_managed"
}

enum AlanXPCManagedUserReadinessState: String, Codable, Equatable {
    case accountMissing = "account_missing"
    case repairable
    case ready
    case accountNotAlanManaged = "account_not_alan_managed"
    case helperUnavailable = "helper_unavailable"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

enum AlanXPCManagedUserHelperPlanStepKind: String, Codable, Equatable, CaseIterable {
    case createStandardAccount = "create_standard_account"
    case repairAccountType = "repair_account_type"
    case repairHomeDirectory = "repair_home_directory"
    case repairShell = "repair_shell"
    case hideAccount = "hide_account"
    case writeOwnershipMarker = "write_ownership_marker"
    case verifyAccount = "verify_account"
    case verifyManagedUserPTY = "verify_managed_user_pty"
    case removeManagedUserIntegration = "remove_managed_user_integration"
    case deleteAccount = "delete_account"
    case deleteHomeDirectory = "delete_home_directory"
}

struct AlanXPCManagedUserHelperPlanStep: Codable, Equatable {
    let kind: AlanXPCManagedUserHelperPlanStepKind
    let summary: String
    let requiresDestructiveConfirmation: Bool
}

struct AlanXPCManagedUserHelperPlan: Codable, Equatable {
    let operationID: String
    let channelID: String
    let request: AlanXPCManagedTerminalAccountRequest
    let steps: [AlanXPCManagedUserHelperPlanStep]
}

struct AlanXPCPrivilegedHelperDiagnostic: Error, Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String?
    let operation: String
    let code: String?
    let sanitizedMessage: String
}

struct AlanXPCManagedUserDiagnosis: Codable, Equatable {
    let request: AlanXPCManagedTerminalAccountRequest
    let ownershipState: AlanXPCManagedUserOwnershipState
    let readinessState: AlanXPCManagedUserReadinessState
    let accountExists: Bool
    let isAdmin: Bool
    let homeDirectoryExists: Bool
    let shellMatches: Bool
    let hiddenFromLoginWindow: Bool
    let terminalProfileID: String?
    let ptySmokeVerified: Bool
    let diagnostic: AlanXPCPrivilegedHelperDiagnostic?
}

struct AlanXPCManagedUserPTYStartRequest: Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String
    let homeDirectory: String
    let workingDirectory: String
    let shell: String
    let contentID: String
    let columns: Int
    let rows: Int
}

struct AlanXPCManagedUserPTYSession: Codable, Equatable {
    let sessionID: String
    let accountName: String
    let contentID: String
    let helperOwnsChildProcess: Bool
    let sanitizedMessage: String
}

struct AlanXPCManagedUserPTYInputRequest: Codable, Equatable {
    let sessionID: String
    let data: Data
}

struct AlanXPCManagedUserPTYReadRequest: Codable, Equatable {
    let sessionID: String
    let maxBytes: Int
}

struct AlanXPCManagedUserPTYOutputChunk: Codable, Equatable {
    let sessionID: String
    let data: Data
    let final: Bool
    let sanitizedMessage: String?
}

struct AlanXPCManagedUserPTYResizeRequest: Codable, Equatable {
    let sessionID: String
    let columns: Int
    let rows: Int
}

enum AlanXPCManagedUserPTYSignal: String, Codable, Equatable {
    case interrupt
    case terminate
    case kill
}

struct AlanXPCManagedUserPTYSignalRequest: Codable, Equatable {
    let sessionID: String
    let signal: AlanXPCManagedUserPTYSignal
}

struct AlanXPCManagedUserPTYSessionRequest: Codable, Equatable {
    let sessionID: String
}

struct AlanXPCManagedUserPTYExitObservation: Codable, Equatable {
    let sessionID: String
    let final: Bool
    let exitCode: Int32?
    let terminatingSignal: Int32?
    let sanitizedMessage: String?
}

struct AlanXPCManagedUserPTYControlResult: Codable, Equatable {
    let accepted: Bool
    let diagnostic: AlanXPCPrivilegedHelperDiagnostic
}
