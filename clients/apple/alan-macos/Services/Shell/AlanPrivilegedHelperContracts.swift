import Foundation
import Security

enum AlanPrivilegedHelperRegistrationAPI: String, Codable, Equatable {
    case smAppServiceDaemon = "SMAppService.daemon(plistName:)"
}

struct AlanPrivilegedHelperIdentity: Codable, Equatable {
    let channelID: String
    let registrationAPI: AlanPrivilegedHelperRegistrationAPI
    let appBundleIdentifier: String
    let helperBundleIdentifier: String
    let launchdServiceLabel: String
    let machServiceName: String
    let plistName: String
    let dataRootPath: String
    let expectedClientRequirement: String
}

enum AlanCodeSigningRequirement {
    private static let unsignedTeamIdentifier = "ALAN_UNSIGNED_HELPER_DENY"

    static func clientRequirement(
        bundleIdentifier: String,
        signingTeamIdentifier: String?
    ) -> String {
        let teamIdentifier = normalized(signingTeamIdentifier) ?? unsignedTeamIdentifier
        return "anchor apple generic and identifier \"\(requirementStringLiteral(bundleIdentifier))\" and certificate leaf[subject.OU] = \"\(requirementStringLiteral(teamIdentifier))\""
    }

    static func currentTeamIdentifier() -> String? {
        var code: SecCode?
        guard SecCodeCopySelf(SecCSFlags(), &code) == errSecSuccess, let code else {
            return nil
        }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
              let staticCode
        else {
            return nil
        }

        var information: CFDictionary?
        let flags = SecCSFlags(rawValue: kSecCSSigningInformation)
        guard SecCodeCopySigningInformation(staticCode, flags, &information) == errSecSuccess,
              let dictionary = information as? [String: Any]
        else {
            return nil
        }
        return normalized(dictionary[kSecCodeInfoTeamIdentifier as String] as? String)
    }

    private static func normalized(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty
        else {
            return nil
        }
        return value
    }

    private static func requirementStringLiteral(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
    }
}

extension AlanInstallChannel {
    var privilegedHelperIdentity: AlanPrivilegedHelperIdentity {
        privilegedHelperIdentity(signingTeamIdentifier: AlanCodeSigningRequirement.currentTeamIdentifier())
    }

    func privilegedHelperIdentity(signingTeamIdentifier: String?) -> AlanPrivilegedHelperIdentity {
        let helperBundleID = "\(bundleIdentifier).privileged-helper"
        return AlanPrivilegedHelperIdentity(
            channelID: installChannelID,
            registrationAPI: .smAppServiceDaemon,
            appBundleIdentifier: bundleIdentifier,
            helperBundleIdentifier: helperBundleID,
            launchdServiceLabel: helperBundleID,
            machServiceName: "\(helperBundleID).xpc",
            plistName: "\(helperBundleID).plist",
            dataRootPath: "/Library/Application Support/\(applicationSupportDirectoryName)/privileged-helper",
            expectedClientRequirement: AlanCodeSigningRequirement.clientRequirement(
                bundleIdentifier: bundleIdentifier,
                signingTeamIdentifier: signingTeamIdentifier
            )
        )
    }
}

enum AlanPrivilegedHelperOperation: String, Codable, Equatable, CaseIterable {
    case helperStatus
    case diagnoseManagedUser
    case applyManagedUserPlan
    case startManagedUserPTY
    case readManagedUserPTY
    case writeManagedUserPTY
    case resizeManagedUserPTY
    case closeManagedUserPTYInput
    case signalManagedUserPTY
    case observeManagedUserPTYExit
    case terminatePTY
    case removeManagedUserIntegration
    case deleteManagedUser
}

enum AlanPrivilegedHelperStatusState: String, Codable, Equatable, CaseIterable {
    case notInstalled = "not_installed"
    case outdated
    case invalidSignature = "invalid_signature"
    case installing
    case updating
    case healthy
    case unavailable
    case uninstallable
}

struct AlanPrivilegedHelperStatus: Codable, Equatable {
    let state: AlanPrivilegedHelperStatusState
    let identity: AlanPrivilegedHelperIdentity
    let installedVersion: String?
    let expectedVersion: String?
    let sanitizedMessage: String?

    var isHealthy: Bool {
        state == .healthy
    }
}

enum AlanPrivilegedHelperErrorCode: String, Codable, Equatable {
    case helperUnavailable = "helper_unavailable"
    case helperOutdated = "helper_outdated"
    case helperSignatureInvalid = "helper_signature_invalid"
    case clientRequirementFailed = "client_requirement_failed"
    case channelMismatch = "channel_mismatch"
    case invalidAccountIdentifier = "invalid_account_identifier"
    case invalidHomePath = "invalid_home_path"
    case shellNotAllowed = "shell_not_allowed"
    case unsupportedOperation = "unsupported_operation"
    case accountNotAlanManaged = "account_not_alan_managed"
    case rawCommandRejected = "raw_command_rejected"
    case rawSudoersRejected = "raw_sudoers_rejected"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

struct AlanPrivilegedHelperDiagnostic: Error, Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String?
    let operation: AlanPrivilegedHelperOperation
    let code: AlanPrivilegedHelperErrorCode?
    let sanitizedMessage: String
}

enum AlanManagedUserOwnershipState: String, Codable, Equatable {
    case missing
    case alanManaged = "alan_managed"
    case notAlanManaged = "not_alan_managed"
}

enum AlanManagedUserReadinessState: String, Codable, Equatable {
    case accountMissing = "account_missing"
    case repairable
    case ready
    case accountNotAlanManaged = "account_not_alan_managed"
    case helperUnavailable = "helper_unavailable"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

struct AlanManagedUserDiagnosis: Codable, Equatable {
    let request: ManagedTerminalAccountRequest
    let ownershipState: AlanManagedUserOwnershipState
    let readinessState: AlanManagedUserReadinessState
    let accountExists: Bool
    let isAdmin: Bool
    let homeDirectoryExists: Bool
    let homeDirectoryMatches: Bool
    let shellMatches: Bool
    let hiddenFromLoginWindow: Bool
    let terminalProfileID: String?
    let ptySmokeVerified: Bool
    let diagnostic: AlanPrivilegedHelperDiagnostic?
}

extension AlanManagedUserDiagnosis {
    static func helperUnavailable(
        request: ManagedTerminalAccountRequest,
        status: AlanPrivilegedHelperStatus
    ) -> AlanManagedUserDiagnosis {
        AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: .helperUnavailable,
            accountExists: false,
            isAdmin: false,
            homeDirectoryExists: false,
            homeDirectoryMatches: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: status.identity.channelID,
                accountName: request.accountName,
                operation: .diagnoseManagedUser,
                code: status.unavailableErrorCode,
                sanitizedMessage: status.sanitizedMessage ?? "Privileged helper is unavailable."
            )
        )
    }
}

private extension AlanPrivilegedHelperStatus {
    var unavailableErrorCode: AlanPrivilegedHelperErrorCode {
        switch state {
        case .outdated:
            return .helperOutdated
        case .invalidSignature:
            return .helperSignatureInvalid
        case .notInstalled, .installing, .updating, .healthy, .unavailable, .uninstallable:
            return .helperUnavailable
        }
    }
}

enum AlanManagedUserHelperPlanStepKind: String, Codable, Equatable, CaseIterable {
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

struct AlanManagedUserHelperPlanStep: Codable, Equatable {
    let kind: AlanManagedUserHelperPlanStepKind
    let summary: String
    let requiresDestructiveConfirmation: Bool
}

struct AlanManagedUserHelperPlan: Codable, Equatable {
    let operationID: String
    let channelID: String
    let request: ManagedTerminalAccountRequest
    let steps: [AlanManagedUserHelperPlanStep]
}

struct AlanManagedUserPTYStartRequest: Codable, Equatable {
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

struct AlanManagedUserPTYSession: Codable, Equatable {
    let sessionID: String
    let accountName: String
    let contentID: String
    let helperOwnsChildProcess: Bool
    let sanitizedMessage: String
}

struct AlanManagedUserPTYInputRequest: Codable, Equatable {
    let sessionID: String
    let data: Data

    init(sessionID: String, data: Data) {
        self.sessionID = sessionID
        self.data = data
    }

    init(sessionID: String, text: String) {
        self.init(sessionID: sessionID, data: Data(text.utf8))
    }

    var text: String {
        String(decoding: data, as: UTF8.self)
    }
}

struct AlanManagedUserPTYReadRequest: Codable, Equatable {
    let sessionID: String
    let maxBytes: Int
}

struct AlanManagedUserPTYOutputChunk: Codable, Equatable {
    let sessionID: String
    let data: Data
    let final: Bool
    let sanitizedMessage: String?
}

struct AlanManagedUserPTYResizeRequest: Codable, Equatable {
    let sessionID: String
    let columns: Int
    let rows: Int
}

enum AlanManagedUserPTYSignal: String, Codable, Equatable {
    case interrupt
    case terminate
    case kill
}

struct AlanManagedUserPTYSignalRequest: Codable, Equatable {
    let sessionID: String
    let signal: AlanManagedUserPTYSignal
}

struct AlanManagedUserPTYExitObservation: Codable, Equatable {
    let sessionID: String
    let final: Bool
    let exitCode: Int32?
    let terminatingSignal: Int32?
    let sanitizedMessage: String?
}

struct AlanManagedUserPTYControlResult: Codable, Equatable {
    let accepted: Bool
    let diagnostic: AlanPrivilegedHelperDiagnostic

    static func accepted(
        operation: AlanPrivilegedHelperOperation,
        channelID: String,
        accountName: String?,
        message: String
    ) -> AlanManagedUserPTYControlResult {
        AlanManagedUserPTYControlResult(
            accepted: true,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: channelID,
                accountName: accountName,
                operation: operation,
                code: nil,
                sanitizedMessage: message
            )
        )
    }

    static func rejected(
        operation: AlanPrivilegedHelperOperation,
        channelID: String,
        accountName: String?,
        code: AlanPrivilegedHelperErrorCode,
        message: String
    ) -> AlanManagedUserPTYControlResult {
        AlanManagedUserPTYControlResult(
            accepted: false,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: channelID,
                accountName: accountName,
                operation: operation,
                code: code,
                sanitizedMessage: message
            )
        )
    }
}

enum AlanPrivilegedHelperRequestValidator {
    static let allowedShells: Set<String> = ["/bin/zsh"]

    static func validate(
        request: ManagedTerminalAccountRequest,
        channel: AlanInstallChannel
    ) -> [AlanPrivilegedHelperErrorCode] {
        var errors: [AlanPrivilegedHelperErrorCode] = []
        if !ManagedTerminalAccountIdentifierValidator.validate(request).isEmpty {
            errors.append(.invalidAccountIdentifier)
        }
        if request.homeDirectory != ManagedTerminalAccountRequest.canonicalHomeDirectory(
            for: request.accountName
        ) {
            errors.append(.invalidHomePath)
        }
        if !allowedShells.contains(request.shell) {
            errors.append(.shellNotAllowed)
        }
        if channel.privilegedHelperIdentity.channelID != channel.installChannelID {
            errors.append(.channelMismatch)
        }
        return errors
    }

    static func rejectsRawPrivilegedPayload(_ payload: String) -> AlanPrivilegedHelperErrorCode? {
        let lowered = payload.lowercased()
        if lowered.contains("do shell script") || lowered.contains("#!/bin/sh") || lowered.contains("sudo ") {
            return .rawCommandRejected
        }
        if lowered.contains("/etc/sudoers") || lowered.contains("nopasswd") {
            return .rawSudoersRejected
        }
        return nil
    }
}

protocol AlanPrivilegedHelperClienting {
    func status() -> AlanPrivilegedHelperStatus
    func diagnoseManagedUser(_ request: ManagedTerminalAccountRequest) -> AlanManagedUserDiagnosis
    func applyManagedUserPlan(_ plan: AlanManagedUserHelperPlan) -> ManagedTerminalAccountApplyResult
    func startManagedUserPTY(_ request: AlanManagedUserPTYStartRequest) -> Result<AlanManagedUserPTYSession, AlanPrivilegedHelperDiagnostic>
    func readManagedUserPTY(_ request: AlanManagedUserPTYReadRequest) -> Result<AlanManagedUserPTYOutputChunk, AlanPrivilegedHelperDiagnostic>
    func writeManagedUserPTY(_ request: AlanManagedUserPTYInputRequest) -> AlanManagedUserPTYControlResult
    func resizeManagedUserPTY(_ request: AlanManagedUserPTYResizeRequest) -> AlanManagedUserPTYControlResult
    func closeManagedUserPTYInput(sessionID: String) -> AlanManagedUserPTYControlResult
    func signalManagedUserPTY(_ request: AlanManagedUserPTYSignalRequest) -> AlanManagedUserPTYControlResult
    func observeManagedUserPTYExit(sessionID: String) -> AlanManagedUserPTYExitObservation?
    func terminatePTY(sessionID: String) -> AlanPrivilegedHelperDiagnostic
    func removeManagedUserIntegration(_ request: ManagedTerminalAccountRequest) -> ManagedTerminalAccountApplyResult
}
