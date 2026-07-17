import Foundation
import Security

struct AlanPrivilegedHelperXPCIdentity: Codable, Equatable {
    let channelID: String
    let helperBundleIdentifier: String
    let machServiceName: String
    let expectedClientRequirement: String

    static func current(
        bundleIdentifier: String? = Bundle.main.bundleIdentifier,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        executablePath: String? = Bundle.main.executablePath,
        signingTeamIdentifier: String? = currentSigningTeamIdentifier()
    ) -> AlanPrivilegedHelperXPCIdentity {
        let helperBundleIdentifier = helperBundleIdentifier(
            bundleIdentifier: bundleIdentifier,
            environment: environment,
            executablePath: executablePath
        )
        let channelID = helperBundleIdentifier.contains(".dev.") ? "dev" : "stable"
        let appBundleIdentifier = channelID == "dev" ? "app.alanworks.macos.dev" : "app.alanworks.macos"
        return AlanPrivilegedHelperXPCIdentity(
            channelID: channelID,
            helperBundleIdentifier: helperBundleIdentifier,
            machServiceName: "\(helperBundleIdentifier).xpc",
            expectedClientRequirement: clientRequirement(
                bundleIdentifier: appBundleIdentifier,
                signingTeamIdentifier: signingTeamIdentifier
            )
        )
    }

    private static func helperBundleIdentifier(
        bundleIdentifier: String?,
        environment: [String: String],
        executablePath: String?
    ) -> String {
        if let serviceName = normalized(environment["XPC_SERVICE_NAME"]),
           serviceName.hasPrefix("app.alanworks.macos."),
           serviceName.contains("privileged-helper")
        {
            return serviceName.hasSuffix(".xpc")
                ? String(serviceName.dropLast(4))
                : serviceName
        }
        if let bundleIdentifier = normalized(bundleIdentifier),
           bundleIdentifier.hasPrefix("app.alanworks.macos."),
           bundleIdentifier.contains("privileged-helper")
        {
            return bundleIdentifier
        }
        if let executablePath = normalized(executablePath) {
            let executableName = URL(fileURLWithPath: executablePath).lastPathComponent
            if executableName.hasPrefix("app.alanworks.macos."),
               executableName.contains("privileged-helper")
            {
                return executableName
            }
        }
        return "app.alanworks.macos.privileged-helper"
    }

    private static func normalized(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty
        else {
            return nil
        }
        return value
    }

    private static func clientRequirement(
        bundleIdentifier: String,
        signingTeamIdentifier: String?
    ) -> String {
        let teamIdentifier = normalized(signingTeamIdentifier) ?? "ALAN_UNSIGNED_HELPER_DENY"
        return "anchor apple generic and identifier \"\(requirementStringLiteral(bundleIdentifier))\" and certificate leaf[subject.OU] = \"\(requirementStringLiteral(teamIdentifier))\""
    }

    private static func currentSigningTeamIdentifier() -> String? {
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

    private static func requirementStringLiteral(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
    }

    var dataRootPath: String {
        let applicationSupportName = channelID == "dev" ? "alan-macos-dev" : "alan-macos"
        return "/Library/Application Support/\(applicationSupportName)/privileged-helper"
    }
}

enum AlanPrivilegedHelperXPCOperation: String, Codable, Equatable {
    case helperStatus = "helper_status"
    case diagnoseManagedUser = "diagnose_managed_user"
    case applyManagedUserPlan = "apply_managed_user_plan"
    case startManagedUserPTY = "start_managed_user_pty"
    case readManagedUserPTY = "read_managed_user_pty"
    case writeManagedUserPTY = "write_managed_user_pty"
    case resizeManagedUserPTY = "resize_managed_user_pty"
    case closeManagedUserPTYInput = "close_managed_user_pty_input"
    case signalManagedUserPTY = "signal_managed_user_pty"
    case observeManagedUserPTYExit = "observe_managed_user_pty_exit"
    case terminatePTY = "terminate_pty"
    case removeManagedUserIntegration = "remove_managed_user_integration"

    var diagnosticOperationName: String {
        switch self {
        case .helperStatus:
            return "helperStatus"
        case .diagnoseManagedUser:
            return "diagnoseManagedUser"
        case .applyManagedUserPlan:
            return "applyManagedUserPlan"
        case .startManagedUserPTY:
            return "startManagedUserPTY"
        case .readManagedUserPTY:
            return "readManagedUserPTY"
        case .writeManagedUserPTY:
            return "writeManagedUserPTY"
        case .resizeManagedUserPTY:
            return "resizeManagedUserPTY"
        case .closeManagedUserPTYInput:
            return "closeManagedUserPTYInput"
        case .signalManagedUserPTY:
            return "signalManagedUserPTY"
        case .observeManagedUserPTYExit:
            return "observeManagedUserPTYExit"
        case .terminatePTY:
            return "terminatePTY"
        case .removeManagedUserIntegration:
            return "removeManagedUserIntegration"
        }
    }
}

struct AlanPrivilegedHelperXPCRequest: Codable, Equatable {
    let operationID: String
    let operation: AlanPrivilegedHelperXPCOperation
    let channelID: String
    let helperBundleIdentifier: String
    let machServiceName: String
    let expectedClientRequirement: String
    let payload: Data?

    static func helperStatus(
        identity: AlanPrivilegedHelperXPCIdentity,
        operationID: String = UUID().uuidString
    ) -> AlanPrivilegedHelperXPCRequest {
        AlanPrivilegedHelperXPCRequest(
            operationID: operationID,
            operation: .helperStatus,
            channelID: identity.channelID,
            helperBundleIdentifier: identity.helperBundleIdentifier,
            machServiceName: identity.machServiceName,
            expectedClientRequirement: identity.expectedClientRequirement,
            payload: nil
        )
    }

    static func operation(
        _ operation: AlanPrivilegedHelperXPCOperation,
        identity: AlanPrivilegedHelperXPCIdentity,
        payload: Data? = nil,
        operationID: String = UUID().uuidString
    ) -> AlanPrivilegedHelperXPCRequest {
        AlanPrivilegedHelperXPCRequest(
            operationID: operationID,
            operation: operation,
            channelID: identity.channelID,
            helperBundleIdentifier: identity.helperBundleIdentifier,
            machServiceName: identity.machServiceName,
            expectedClientRequirement: identity.expectedClientRequirement,
            payload: payload
        )
    }
}

enum AlanPrivilegedHelperXPCErrorCode: String, Error, Codable, Equatable {
    case invalidRequest = "invalid_request"
    case unsupportedOperation = "unsupported_operation"
    case channelMismatch = "channel_mismatch"
    case clientRequirementFailed = "client_requirement_failed"
    case connectionFailed = "connection_failed"
    case helperUnavailable = "helper_unavailable"
    case invalidAccountIdentifier = "invalid_account_identifier"
    case invalidHomePath = "invalid_home_path"
    case shellNotAllowed = "shell_not_allowed"
    case accountNotAlanManaged = "account_not_alan_managed"
    case ptySpawnFailed = "pty_spawn_failed"
    case timeout
}

struct AlanPrivilegedHelperXPCResponse: Codable, Equatable {
    let operationID: String?
    let operation: AlanPrivilegedHelperXPCOperation?
    let accepted: Bool
    let channelID: String
    let helperBundleIdentifier: String
    let errorCode: AlanPrivilegedHelperXPCErrorCode?
    let sanitizedMessage: String
    let payload: Data?

    static func accepted(
        request: AlanPrivilegedHelperXPCRequest,
        identity: AlanPrivilegedHelperXPCIdentity,
        message: String,
        payload: Data? = nil
    ) -> AlanPrivilegedHelperXPCResponse {
        AlanPrivilegedHelperXPCResponse(
            operationID: request.operationID,
            operation: request.operation,
            accepted: true,
            channelID: identity.channelID,
            helperBundleIdentifier: identity.helperBundleIdentifier,
            errorCode: nil,
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message),
            payload: payload
        )
    }

    static func rejected(
        request: AlanPrivilegedHelperXPCRequest?,
        identity: AlanPrivilegedHelperXPCIdentity,
        code: AlanPrivilegedHelperXPCErrorCode,
        message: String,
        payload: Data? = nil
    ) -> AlanPrivilegedHelperXPCResponse {
        AlanPrivilegedHelperXPCResponse(
            operationID: request?.operationID,
            operation: request?.operation,
            accepted: false,
            channelID: identity.channelID,
            helperBundleIdentifier: identity.helperBundleIdentifier,
            errorCode: code,
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message),
            payload: payload
        )
    }
}

struct AlanPrivilegedHelperXPCApplyResultPayload: Codable, Equatable {
    let completedHelperSteps: [String]
    let failedHelperStep: String?
    let cancelled: Bool
    let visibleDiagnostics: [String]
}

struct AlanPrivilegedHelperProtocolStatus: Codable, Equatable {
    static let currentVersion = 2

    let protocolVersion: Int
}

struct AlanPrivilegedHelperSanitizedEvent: Codable, Equatable {
    let operationID: String?
    let channelID: String
    let helperBundleIdentifier: String
    let operation: AlanPrivilegedHelperXPCOperation?
    let errorCode: AlanPrivilegedHelperXPCErrorCode?
    let sanitizedMessage: String

    init(response: AlanPrivilegedHelperXPCResponse) {
        self.operationID = response.operationID
        self.channelID = response.channelID
        self.helperBundleIdentifier = response.helperBundleIdentifier
        self.operation = response.operation
        self.errorCode = response.errorCode
        self.sanitizedMessage = AlanPrivilegedHelperSanitizer.sanitizedMessage(
            response.sanitizedMessage
        )
    }
}

enum AlanPrivilegedHelperSanitizer {
    private static let redactedTerms = [
        "do shell script",
        "#!/bin/sh",
        "#!/bin/bash",
        "/etc/sudoers",
        "NOPASSWD",
        "sudo -n -iu",
        "password",
        "transcript",
    ]

    static func sanitizedMessage(_ message: String) -> String {
        var sanitized = message
        for term in redactedTerms {
            sanitized = sanitized.replacingOccurrences(
                of: term,
                with: "[redacted]",
                options: [.caseInsensitive]
            )
        }
        if sanitized.count > 500 {
            sanitized = String(sanitized.prefix(500)) + "..."
        }
        return sanitized
    }
}

enum AlanPrivilegedHelperXPCCodec {
    static func encode(_ response: AlanPrivilegedHelperXPCResponse) -> NSData {
        let data = (try? JSONEncoder().encode(response)) ?? Data()
        return data as NSData
    }

    static func encode(_ request: AlanPrivilegedHelperXPCRequest) -> NSData {
        let data = (try? JSONEncoder().encode(request)) ?? Data()
        return data as NSData
    }

    static func decodeRequest(_ data: NSData) throws -> AlanPrivilegedHelperXPCRequest {
        try JSONDecoder().decode(AlanPrivilegedHelperXPCRequest.self, from: data as Data)
    }

    static func decodeResponse(_ data: NSData) throws -> AlanPrivilegedHelperXPCResponse {
        try JSONDecoder().decode(AlanPrivilegedHelperXPCResponse.self, from: data as Data)
    }
}

@objc(AlanPrivilegedHelperXPCProtocol)
protocol AlanPrivilegedHelperXPCProtocol {
    func helperStatus(
        _ requestData: NSData,
        withReply reply: @escaping (NSData) -> Void
    )

    func performRequest(
        _ requestData: NSData,
        withReply reply: @escaping (NSData) -> Void
    )
}
