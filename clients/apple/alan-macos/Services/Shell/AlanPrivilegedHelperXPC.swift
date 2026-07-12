import Darwin
import Foundation
import Security

@_silgen_name("alan_darwin_pty_spawn_as_user")
private func alanDarwinPtySpawnAsUser(
    _ executablePath: UnsafePointer<CChar>,
    _ argv: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ envp: UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>,
    _ workingDirectory: UnsafePointer<CChar>,
    _ accountName: UnsafePointer<CChar>,
    _ uid: uid_t,
    _ gid: gid_t,
    _ rows: UInt16,
    _ columns: UInt16,
    _ masterFileDescriptor: UnsafeMutablePointer<Int32>,
    _ processID: UnsafeMutablePointer<pid_t>
) -> Int32

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

protocol AlanPrivilegedHelperClientRequirementChecking {
    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode>
}

struct AlanPrivilegedHelperSecCodeRequirementChecker: AlanPrivilegedHelperClientRequirementChecking {
    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode> {
        var guestCode: SecCode?
        let attributes = [kSecGuestAttributePid as String: processIdentifier] as CFDictionary
        let copyStatus = SecCodeCopyGuestWithAttributes(nil, attributes, SecCSFlags(), &guestCode)
        guard copyStatus == errSecSuccess, let guestCode else {
            return .failure(.clientRequirementFailed)
        }

        var requirement: SecRequirement?
        let requirementStatus = SecRequirementCreateWithString(
            expectedRequirement as CFString,
            SecCSFlags(),
            &requirement
        )
        guard requirementStatus == errSecSuccess, let requirement else {
            return .failure(.clientRequirementFailed)
        }

        let checkStatus = SecCodeCheckValidity(guestCode, SecCSFlags(), requirement)
        return checkStatus == errSecSuccess ? .success(()) : .failure(.clientRequirementFailed)
    }
}

struct AlanPrivilegedHelperFakeRequirementChecker: AlanPrivilegedHelperClientRequirementChecking {
    var acceptedProcessIdentifiers: Set<pid_t>

    func validateClient(
        processIdentifier: pid_t,
        expectedRequirement: String
    ) -> Result<Void, AlanPrivilegedHelperXPCErrorCode> {
        guard !expectedRequirement.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              acceptedProcessIdentifiers.contains(processIdentifier)
        else {
            return .failure(.clientRequirementFailed)
        }
        return .success(())
    }
}

private protocol AlanPrivilegedHelperConnectionCleanup {
    func cleanupConnectionSessions()
}

final class AlanPrivilegedHelperXPCService: NSObject, AlanPrivilegedHelperXPCProtocol,
    AlanPrivilegedHelperConnectionCleanup
{
    let identity: AlanPrivilegedHelperXPCIdentity
    private let managedUserService: AlanPrivilegedHelperManagedUserService
    private let ptySessions: AlanPrivilegedHelperPTYSessionStore

    init(identity: AlanPrivilegedHelperXPCIdentity = .current()) {
        self.identity = identity
        self.ptySessions = AlanPrivilegedHelperPTYSessionStore(identity: identity)
        self.managedUserService = AlanPrivilegedHelperManagedUserService(identity: identity)
    }

    func helperStatus(
        _ requestData: NSData,
        withReply reply: @escaping (NSData) -> Void
    ) {
        performRequest(requestData, withReply: reply)
    }

    func performRequest(
        _ requestData: NSData,
        withReply reply: @escaping (NSData) -> Void
    ) {
        let response: AlanPrivilegedHelperXPCResponse
        do {
            let request = try AlanPrivilegedHelperXPCCodec.decodeRequest(requestData)
            response = handle(request)
        } catch {
            response = .rejected(
                request: nil,
                identity: identity,
                code: .invalidRequest,
                message: "Privileged helper rejected an invalid XPC request."
            )
        }
        reply(AlanPrivilegedHelperXPCCodec.encode(response))
    }

    func cleanupConnectionSessions() {
        ptySessions.terminateAll()
    }

    private func handle(_ request: AlanPrivilegedHelperXPCRequest) -> AlanPrivilegedHelperXPCResponse {
        guard request.channelID == identity.channelID,
              request.helperBundleIdentifier == identity.helperBundleIdentifier,
              request.machServiceName == identity.machServiceName
        else {
            return .rejected(
                request: request,
                identity: identity,
                code: .channelMismatch,
                message: "Privileged helper rejected a channel-mismatched request."
            )
        }

        switch request.operation {
        case .helperStatus:
            return .accepted(
                request: request,
                identity: identity,
                message: "Privileged helper XPC boundary is available.",
                payload: encodePayload(
                    AlanPrivilegedHelperProtocolStatus(
                        protocolVersion: AlanPrivilegedHelperProtocolStatus.currentVersion
                    )
                )
            )
        case .diagnoseManagedUser:
            return withDecodedPayload(request, as: AlanXPCManagedTerminalAccountRequest.self) {
                let diagnosis = managedUserService.diagnose(request: $0, verifyPTY: true)
                return .accepted(
                    request: request,
                    identity: identity,
                    message: "Privileged helper diagnosed Managed User state.",
                    payload: encodePayload(diagnosis)
                )
            }
        case .applyManagedUserPlan:
            return withDecodedPayload(request, as: AlanXPCManagedUserHelperPlan.self) {
                let result = managedUserService.apply(plan: $0, ptySessions: ptySessions)
                let accepted = result.failedHelperStep == nil && !result.cancelled
                if accepted {
                    return .accepted(
                        request: request,
                        identity: identity,
                        message: "Privileged helper applied Managed User plan.",
                        payload: encodePayload(result)
                    )
                }
                return .rejected(
                    request: request,
                    identity: identity,
                    code: .helperUnavailable,
                    message: result.visibleDiagnostics.first
                        ?? "Privileged helper failed to apply Managed User plan.",
                    payload: encodePayload(result)
                )
            }
        case .startManagedUserPTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYStartRequest.self) {
                switch ptySessions.start(request: $0, managedUserService: managedUserService) {
                case .success(let session):
                    return .accepted(
                        request: request,
                        identity: identity,
                        message: "Privileged helper started Managed User PTY.",
                        payload: encodePayload(session)
                    )
                case .failure(let diagnostic):
                    return .rejected(
                        request: request,
                        identity: identity,
                        code: .ptySpawnFailed,
                        message: diagnostic.sanitizedMessage,
                        payload: encodePayload(diagnostic)
                    )
                }
            }
        case .readManagedUserPTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYReadRequest.self) {
                switch ptySessions.read($0) {
                case .success(let chunk):
                    return .accepted(
                        request: request,
                        identity: identity,
                        message: chunk.sanitizedMessage ?? "Privileged helper read Managed User PTY output.",
                        payload: encodePayload(chunk)
                    )
                case .failure(let diagnostic):
                    return .rejected(
                        request: request,
                        identity: identity,
                        code: .helperUnavailable,
                        message: diagnostic.sanitizedMessage,
                        payload: encodePayload(diagnostic)
                    )
                }
            }
        case .writeManagedUserPTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYInputRequest.self) {
                let result = ptySessions.write($0)
                return controlResponse(request: request, result: result)
            }
        case .resizeManagedUserPTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYResizeRequest.self) {
                let result = ptySessions.resize($0)
                return controlResponse(request: request, result: result)
            }
        case .closeManagedUserPTYInput:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYSessionRequest.self) {
                let result = ptySessions.closeInput(sessionID: $0.sessionID)
                return controlResponse(request: request, result: result)
            }
        case .signalManagedUserPTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYSignalRequest.self) {
                let result = ptySessions.signal($0)
                return controlResponse(request: request, result: result)
            }
        case .observeManagedUserPTYExit:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYSessionRequest.self) {
                let observation = ptySessions.observeExit(sessionID: $0.sessionID)
                return .accepted(
                    request: request,
                    identity: identity,
                    message: "Privileged helper observed Managed User PTY state.",
                    payload: observation.flatMap(encodePayload)
                )
            }
        case .terminatePTY:
            return withDecodedPayload(request, as: AlanXPCManagedUserPTYSessionRequest.self) {
                let diagnostic = ptySessions.terminate(sessionID: $0.sessionID)
                let accepted = diagnostic.code == nil
                if accepted {
                    return .accepted(
                        request: request,
                        identity: identity,
                        message: diagnostic.sanitizedMessage,
                        payload: encodePayload(diagnostic)
                    )
                }
                return .rejected(
                    request: request,
                    identity: identity,
                    code: .helperUnavailable,
                    message: diagnostic.sanitizedMessage,
                    payload: encodePayload(diagnostic)
                )
            }
        case .removeManagedUserIntegration:
            return withDecodedPayload(request, as: AlanXPCManagedTerminalAccountRequest.self) {
                let result = managedUserService.removeIntegration(request: $0)
                return .accepted(
                    request: request,
                    identity: identity,
                    message: "Privileged helper removed Managed User integration.",
                    payload: encodePayload(result)
                )
            }
        }
    }

    private func controlResponse(
        request: AlanPrivilegedHelperXPCRequest,
        result: AlanXPCManagedUserPTYControlResult
    ) -> AlanPrivilegedHelperXPCResponse {
        if result.accepted {
            return .accepted(
                request: request,
                identity: identity,
                message: result.diagnostic.sanitizedMessage,
                payload: encodePayload(result)
            )
        }
        return .rejected(
            request: request,
            identity: identity,
            code: .helperUnavailable,
            message: result.diagnostic.sanitizedMessage,
            payload: encodePayload(result)
        )
    }

    private func withDecodedPayload<T: Decodable>(
        _ request: AlanPrivilegedHelperXPCRequest,
        as type: T.Type,
        perform: (T) -> AlanPrivilegedHelperXPCResponse
    ) -> AlanPrivilegedHelperXPCResponse {
        guard let payload = request.payload,
              let decoded = try? JSONDecoder().decode(type, from: payload)
        else {
            return .rejected(
                request: request,
                identity: identity,
                code: .invalidRequest,
                message: "Privileged helper rejected an invalid typed payload."
            )
        }
        return perform(decoded)
    }
}

final class AlanPrivilegedHelperXPCListenerDelegate: NSObject, NSXPCListenerDelegate {
    let identity: AlanPrivilegedHelperXPCIdentity
    let requirementChecker: AlanPrivilegedHelperClientRequirementChecking
    let serviceFactory: () -> AlanPrivilegedHelperXPCProtocol

    init(
        identity: AlanPrivilegedHelperXPCIdentity = .current(),
        requirementChecker: AlanPrivilegedHelperClientRequirementChecking =
            AlanPrivilegedHelperSecCodeRequirementChecker(),
        serviceFactory: @escaping () -> AlanPrivilegedHelperXPCProtocol
    ) {
        self.identity = identity
        self.requirementChecker = requirementChecker
        self.serviceFactory = serviceFactory
    }

    func listener(
        _ listener: NSXPCListener,
        shouldAcceptNewConnection newConnection: NSXPCConnection
    ) -> Bool {
        switch requirementChecker.validateClient(
            processIdentifier: newConnection.processIdentifier,
            expectedRequirement: identity.expectedClientRequirement
        ) {
        case .success:
            break
        case .failure:
            return false
        }
        let service = serviceFactory()
        newConnection.exportedInterface = NSXPCInterface(with: AlanPrivilegedHelperXPCProtocol.self)
        newConnection.exportedObject = service
        newConnection.invalidationHandler = { [service] in
            (service as? AlanPrivilegedHelperConnectionCleanup)?.cleanupConnectionSessions()
        }
        newConnection.interruptionHandler = { [service] in
            (service as? AlanPrivilegedHelperConnectionCleanup)?.cleanupConnectionSessions()
        }
        newConnection.resume()
        return true
    }
}

final class AlanPrivilegedHelperXPCClient {
    private static let managedUserApplyTimeoutSeconds: TimeInterval = 600

    let identity: AlanPrivilegedHelperXPCIdentity
    let timeoutSeconds: TimeInterval
    private var connection: NSXPCConnection?

    init(
        identity: AlanPrivilegedHelperXPCIdentity,
        timeoutSeconds: TimeInterval = 5
    ) {
        self.identity = identity
        self.timeoutSeconds = timeoutSeconds
    }

    deinit {
        connection?.invalidate()
    }

    func helperStatus() -> AlanPrivilegedHelperXPCResponse {
        perform(operation: .helperStatus)
    }

    func perform(
        operation: AlanPrivilegedHelperXPCOperation,
        payload: Data? = nil
    ) -> AlanPrivilegedHelperXPCResponse {
        let request = AlanPrivilegedHelperXPCRequest.operation(
            operation,
            identity: identity,
            payload: payload
        )
        return perform(request)
    }

    private func perform(_ request: AlanPrivilegedHelperXPCRequest) -> AlanPrivilegedHelperXPCResponse {
        let connection = liveConnection()
        let requestData = AlanPrivilegedHelperXPCCodec.encode(request)
        let semaphore = DispatchSemaphore(value: 0)
        var response: AlanPrivilegedHelperXPCResponse?

        let proxy = connection.remoteObjectProxyWithErrorHandler { _ in
            response = .rejected(
                request: request,
                identity: self.identity,
                code: .connectionFailed,
                message: "Privileged helper XPC connection failed."
            )
            semaphore.signal()
        } as? AlanPrivilegedHelperXPCProtocol

        if request.operation == .helperStatus {
            proxy?.helperStatus(requestData) { replyData in
                response = try? AlanPrivilegedHelperXPCCodec.decodeResponse(replyData)
                semaphore.signal()
            }
        } else {
            proxy?.performRequest(requestData) { replyData in
                response = try? AlanPrivilegedHelperXPCCodec.decodeResponse(replyData)
                semaphore.signal()
            }
        }

        let deadline = DispatchTime.now() + timeout(for: request.operation)
        if semaphore.wait(timeout: deadline) == .timedOut {
            return .rejected(
                request: request,
                identity: identity,
                code: .timeout,
                message: "Privileged helper XPC request timed out."
            )
        }
        return response
            ?? .rejected(
                request: request,
                identity: identity,
                code: .invalidRequest,
                message: "Privileged helper returned an invalid XPC response."
            )
    }

    private func timeout(for operation: AlanPrivilegedHelperXPCOperation?) -> TimeInterval {
        switch operation {
        case .applyManagedUserPlan:
            return max(timeoutSeconds, Self.managedUserApplyTimeoutSeconds)
        case .helperStatus, .diagnoseManagedUser, .startManagedUserPTY, .readManagedUserPTY,
                .writeManagedUserPTY, .resizeManagedUserPTY, .closeManagedUserPTYInput,
                .signalManagedUserPTY, .observeManagedUserPTYExit, .terminatePTY,
                .removeManagedUserIntegration, .none:
            return timeoutSeconds
        }
    }

    private func liveConnection() -> NSXPCConnection {
        if let connection {
            return connection
        }
        let nextConnection = NSXPCConnection(
            machServiceName: identity.machServiceName,
            options: .privileged
        )
        nextConnection.remoteObjectInterface = NSXPCInterface(
            with: AlanPrivilegedHelperXPCProtocol.self
        )
        nextConnection.resume()
        connection = nextConnection
        return nextConnection
    }
}

private struct AlanXPCManagedTerminalAccountRequest: Codable, Equatable {
    let accountName: String
    let fullName: String?
    let shell: String
    let homeDirectory: String
    let hideFromLoginWindow: Bool

    var terminalProfileID: String { accountName }
    var canonicalHomeDirectory: String { "/Users/\(accountName)" }
}

private enum AlanXPCManagedUserOwnershipState: String, Codable, Equatable {
    case missing
    case alanManaged = "alan_managed"
    case notAlanManaged = "not_alan_managed"
}

private enum AlanXPCManagedUserReadinessState: String, Codable, Equatable {
    case accountMissing = "account_missing"
    case repairable
    case ready
    case accountNotAlanManaged = "account_not_alan_managed"
    case helperUnavailable = "helper_unavailable"
    case ptySpawnFailed = "pty_spawn_failed"
    case destructiveConfirmationRequired = "destructive_confirmation_required"
}

private enum AlanXPCManagedUserHelperPlanStepKind: String, Codable, Equatable, CaseIterable {
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

private struct AlanXPCManagedUserHelperPlanStep: Codable, Equatable {
    let kind: AlanXPCManagedUserHelperPlanStepKind
    let summary: String
    let requiresDestructiveConfirmation: Bool
}

private struct AlanXPCManagedUserHelperPlan: Codable, Equatable {
    let operationID: String
    let channelID: String
    let request: AlanXPCManagedTerminalAccountRequest
    let steps: [AlanXPCManagedUserHelperPlanStep]
}

struct AlanPrivilegedHelperXPCApplyResultPayload: Codable, Equatable {
    let completedHelperSteps: [String]
    let failedHelperStep: String?
    let cancelled: Bool
    let visibleDiagnostics: [String]
}

private struct AlanXPCPrivilegedHelperDiagnostic: Error, Codable, Equatable {
    let operationID: String
    let channelID: String
    let accountName: String?
    let operation: String
    let code: String?
    let sanitizedMessage: String
}

private struct AlanXPCManagedUserDiagnosis: Codable, Equatable {
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

private struct AlanXPCManagedUserPTYStartRequest: Codable, Equatable {
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

private struct AlanXPCManagedUserPTYSession: Codable, Equatable {
    let sessionID: String
    let accountName: String
    let contentID: String
    let helperOwnsChildProcess: Bool
    let sanitizedMessage: String
}

private struct AlanXPCManagedUserPTYInputRequest: Codable, Equatable {
    let sessionID: String
    let data: Data
}

private struct AlanXPCManagedUserPTYReadRequest: Codable, Equatable {
    let sessionID: String
    let maxBytes: Int
}

private struct AlanXPCManagedUserPTYOutputChunk: Codable, Equatable {
    let sessionID: String
    let data: Data
    let final: Bool
    let sanitizedMessage: String?
}

private struct AlanXPCManagedUserPTYResizeRequest: Codable, Equatable {
    let sessionID: String
    let columns: Int
    let rows: Int
}

private enum AlanXPCManagedUserPTYSignal: String, Codable, Equatable {
    case interrupt
    case terminate
    case kill
}

private struct AlanXPCManagedUserPTYSignalRequest: Codable, Equatable {
    let sessionID: String
    let signal: AlanXPCManagedUserPTYSignal
}

private struct AlanXPCManagedUserPTYSessionRequest: Codable, Equatable {
    let sessionID: String
}

private struct AlanXPCManagedUserPTYExitObservation: Codable, Equatable {
    let sessionID: String
    let final: Bool
    let exitCode: Int32?
    let terminatingSignal: Int32?
    let sanitizedMessage: String?
}

private struct AlanXPCManagedUserPTYControlResult: Codable, Equatable {
    let accepted: Bool
    let diagnostic: AlanXPCPrivilegedHelperDiagnostic
}

private final class AlanPrivilegedHelperManagedUserService {
    private let identity: AlanPrivilegedHelperXPCIdentity
    private let fileManager: FileManager

    init(identity: AlanPrivilegedHelperXPCIdentity, fileManager: FileManager = .default) {
        self.identity = identity
        self.fileManager = fileManager
    }

    func diagnose(
        request: AlanXPCManagedTerminalAccountRequest,
        verifyPTY: Bool
    ) -> AlanXPCManagedUserDiagnosis {
        guard validate(request).isEmpty else {
            return diagnosis(
                request: request,
                ownershipState: .missing,
                readinessState: .repairable,
                account: nil,
                ptySmokeVerified: false,
                diagnostic: diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .invalidAccountIdentifier,
                    message: "Managed User request is invalid."
                )
            )
        }

        let account = accountRecord(for: request.accountName)
        guard let account else {
            return diagnosis(
                request: request,
                ownershipState: .missing,
                readinessState: .accountMissing,
                account: nil,
                ptySmokeVerified: false,
                diagnostic: nil
            )
        }

        let markerExists = fileManager.fileExists(atPath: ownershipMarkerPath(for: request))
        let ownershipState: AlanXPCManagedUserOwnershipState
        if markerExists {
            ownershipState = .alanManaged
        } else {
            ownershipState = .notAlanManaged
        }

        guard ownershipState == .alanManaged else {
            return diagnosis(
                request: request,
                ownershipState: ownershipState,
                readinessState: .accountNotAlanManaged,
                account: account,
                ptySmokeVerified: false,
                diagnostic: diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .accountNotAlanManaged,
                    message: "Existing account is not Alan managed."
                )
            )
        }

        let repairable = account.isAdmin
            || account.homeDirectory != request.homeDirectory
            || !fileManager.fileExists(atPath: request.homeDirectory)
            || account.shell != request.shell
            || (request.hideFromLoginWindow && !account.hidden)
        guard !repairable else {
            return diagnosis(
                request: request,
                ownershipState: ownershipState,
                readinessState: .repairable,
                account: account,
                ptySmokeVerified: false,
                diagnostic: nil
            )
        }

        let ptySmokeVerified = verifyPTY ? verifyManagedUserPTY(request: request) : true
        return diagnosis(
            request: request,
            ownershipState: ownershipState,
            readinessState: ptySmokeVerified ? .ready : .ptySpawnFailed,
            account: account,
            ptySmokeVerified: ptySmokeVerified,
            diagnostic: ptySmokeVerified
                ? nil
                : diagnostic(
                    operation: .diagnoseManagedUser,
                    accountName: request.accountName,
                    code: .ptySpawnFailed,
                    message: "Managed User PTY smoke verification failed."
                )
        )
    }

    func apply(
        plan: AlanXPCManagedUserHelperPlan,
        ptySessions: AlanPrivilegedHelperPTYSessionStore
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        guard plan.channelID == identity.channelID else {
            return failedApply(
                completed: [],
                failed: plan.steps.first?.kind,
                message: "Privileged helper rejected a channel-mismatched Managed User plan."
            )
        }
        let validationErrors = validate(plan.request)
        guard validationErrors.isEmpty else {
            return failedApply(
                completed: [],
                failed: plan.steps.first?.kind,
                message: "Privileged helper rejected an invalid Managed User plan."
            )
        }

        var completed: [AlanXPCManagedUserHelperPlanStepKind] = []
        var destructiveAccountRecord: AlanManagedUserAccountRecord?
        for step in plan.steps {
            switch step.kind {
            case .deleteAccount:
                let revalidation = managedAccountRecordForDestructiveDeletion(plan.request)
                guard let account = revalidation.account else {
                    return failedApply(
                        completed: completed,
                        failed: step.kind,
                        message: revalidation.message
                    )
                }
                destructiveAccountRecord = account
            case .deleteHomeDirectory:
                let revalidation = validateHomeDeletionStillManaged(
                    plan.request,
                    originalAccount: destructiveAccountRecord
                )
                guard revalidation.succeeded else {
                    return failedApply(
                        completed: completed,
                        failed: step.kind,
                        message: revalidation.message
                    )
                }
            case .createStandardAccount, .repairAccountType, .repairHomeDirectory, .repairShell,
                    .hideAccount, .writeOwnershipMarker, .verifyAccount,
                    .verifyManagedUserPTY, .removeManagedUserIntegration:
                break
            }

            let result = apply(step: step, request: plan.request, ptySessions: ptySessions)
            guard result.succeeded else {
                return failedApply(
                    completed: completed,
                    failed: step.kind,
                    message: result.message
                )
            }
            completed.append(step.kind)
        }
        return AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: completed.map(\.rawValue),
            failedHelperStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper applied the Managed User plan. Credentials redacted."]
        )
    }

    private func managedAccountRecordForDestructiveDeletion(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (account: AlanManagedUserAccountRecord?, message: String) {
        guard let account = accountRecord(for: request.accountName) else {
            return (nil, "Privileged helper refused destructive deletion for a missing Managed User.")
        }
        guard destructiveOwnershipEvidenceExists(for: request) else {
            return (
                nil,
                "Privileged helper refused destructive deletion because Alan ownership could not be revalidated."
            )
        }
        return (account, "Managed User destructive ownership revalidated.")
    }

    private func validateHomeDeletionStillManaged(
        _ request: AlanXPCManagedTerminalAccountRequest,
        originalAccount: AlanManagedUserAccountRecord?
    ) -> (succeeded: Bool, message: String) {
        guard destructiveOwnershipEvidenceExists(for: request) else {
            return (
                false,
                "Privileged helper refused home deletion because Alan ownership could not be revalidated."
            )
        }
        if let currentAccount = accountRecord(for: request.accountName),
           let originalAccount,
           currentAccount.uid != originalAccount.uid
        {
            return (
                false,
                "Privileged helper refused home deletion because the Managed User identity changed."
            )
        }
        return (true, "Managed User home deletion ownership revalidated.")
    }

    private func destructiveOwnershipEvidenceExists(
        for request: AlanXPCManagedTerminalAccountRequest
    ) -> Bool {
        fileManager.fileExists(atPath: ownershipMarkerPath(for: request))
    }

    func removeIntegration(
        request: AlanXPCManagedTerminalAccountRequest
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        let markerPath = ownershipMarkerPath(for: request)
        if fileManager.fileExists(atPath: markerPath) {
            try? fileManager.removeItem(atPath: markerPath)
        }
        return AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: [AlanXPCManagedUserHelperPlanStepKind.removeManagedUserIntegration.rawValue],
            failedHelperStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper removed Managed User integration. Credentials redacted."]
        )
    }

    func accountReadyForPTY(
        accountName: String,
        homeDirectory: String,
        shell: String,
        channelID: String
    ) -> Result<AlanManagedUserAccountRecord, AlanXPCPrivilegedHelperDiagnostic> {
        let request = AlanXPCManagedTerminalAccountRequest(
            accountName: accountName,
            fullName: nil,
            shell: shell,
            homeDirectory: homeDirectory,
            hideFromLoginWindow: true
        )
        guard channelID == identity.channelID else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: accountName,
                    code: .channelMismatch,
                    message: "Privileged helper rejected a channel-mismatched PTY request."
                )
            )
        }
        guard validate(request).isEmpty, let account = accountRecord(for: accountName) else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: accountName,
                    code: .invalidAccountIdentifier,
                    message: "Privileged helper rejected an invalid PTY request."
                )
            )
        }
        let diagnosis = diagnose(request: request, verifyPTY: false)
        guard diagnosis.readinessState == .ready else {
            return .failure(
                diagnosis.diagnostic
                    ?? diagnostic(
                        operation: .startManagedUserPTY,
                        accountName: accountName,
                        code: .accountNotAlanManaged,
                        message: "Managed User is not ready for PTY launch."
                    )
            )
        }
        return .success(account)
    }

    private func apply(
        step: AlanXPCManagedUserHelperPlanStep,
        request: AlanXPCManagedTerminalAccountRequest,
        ptySessions: AlanPrivilegedHelperPTYSessionStore
    ) -> (succeeded: Bool, message: String) {
        switch step.kind {
        case .createStandardAccount:
            return createStandardAccount(request)
        case .repairAccountType:
            return removeAdminMembership(request.accountName)
        case .repairHomeDirectory:
            return repairHomeDirectory(request)
        case .repairShell:
            return runFixed("/usr/bin/dscl", [".", "-create", "/Users/\(request.accountName)", "UserShell", request.shell])
        case .hideAccount:
            return runFixed("/usr/bin/dscl", [".", "-create", "/Users/\(request.accountName)", "IsHidden", "1"])
        case .writeOwnershipMarker:
            return writeOwnershipMarker(request)
        case .verifyAccount:
            let diagnosis = diagnose(request: request, verifyPTY: false)
            return diagnosis.readinessState == .ready || diagnosis.readinessState == .ptySpawnFailed
                ? (true, "Managed User account state verified. Credentials redacted.")
                : (false, diagnosis.diagnostic?.sanitizedMessage ?? "Managed User verification failed.")
        case .verifyManagedUserPTY:
            return verifyManagedUserPTY(request: request)
                ? (true, "Managed User PTY smoke verified. Credentials redacted.")
                : (false, "Managed User PTY smoke verification failed. Credentials redacted.")
        case .removeManagedUserIntegration:
            _ = removeIntegration(request: request)
            return (true, "Managed User integration removed. Credentials redacted.")
        case .deleteAccount:
            return runFixed("/usr/bin/dscl", [".", "-delete", "/Users/\(request.accountName)"])
        case .deleteHomeDirectory:
            guard request.homeDirectory == request.canonicalHomeDirectory else {
                return (false, "Privileged helper refused non-canonical home deletion.")
            }
            do {
                if fileManager.fileExists(atPath: request.homeDirectory) {
                    try fileManager.removeItem(atPath: request.homeDirectory)
                }
                return (true, "Managed User home directory deleted. Credentials redacted.")
            } catch {
                return (false, "Managed User home directory deletion failed. Credentials redacted.")
            }
        }
    }

    private func createStandardAccount(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        if accountRecord(for: request.accountName) != nil {
            return (true, "Managed User already exists. Credentials redacted.")
        }
        let password = UUID().uuidString + UUID().uuidString
        let fullName = request.fullName?.trimmingCharacters(in: .whitespacesAndNewlines)
        let result = runFixed(
            "/usr/sbin/sysadminctl",
            [
                "-addUser",
                request.accountName,
                "-fullName",
                fullName?.isEmpty == false ? fullName! : request.accountName,
                "-home",
                request.homeDirectory,
                "-shell",
                request.shell,
                "-password",
                password,
            ]
        )
        guard result.succeeded else { return result }
        return repairHomeDirectory(request)
    }

    private func removeAdminMembership(_ accountName: String) -> (succeeded: Bool, message: String) {
        runFixed("/usr/sbin/dseditgroup", ["-o", "edit", "-d", accountName, "-t", "user", "admin"])
    }

    private func repairHomeDirectory(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        guard let account = accountRecord(for: request.accountName) else {
            return (false, "Managed User account is missing. Credentials redacted.")
        }
        if account.homeDirectory != request.homeDirectory {
            let result = runFixed(
                "/usr/bin/dscl",
                [
                    ".",
                    "-create",
                    "/Users/\(request.accountName)",
                    "NFSHomeDirectory",
                    request.homeDirectory,
                ]
            )
            guard result.succeeded else {
                return result
            }
        }
        do {
            try fileManager.createDirectory(
                atPath: request.homeDirectory,
                withIntermediateDirectories: true
            )
            _ = chown(request.homeDirectory, account.uid, account.gid)
            return (true, "Managed User home directory repaired. Credentials redacted.")
        } catch {
            return (false, "Managed User home directory repair failed. Credentials redacted.")
        }
    }

    private func writeOwnershipMarker(
        _ request: AlanXPCManagedTerminalAccountRequest
    ) -> (succeeded: Bool, message: String) {
        let markerPath = ownershipMarkerPath(for: request)
        let markerURL = URL(fileURLWithPath: markerPath)
        let marker: [String: String] = [
            "managed_by": "alan",
            "channel_id": identity.channelID,
            "account_name": request.accountName,
            "home_directory": request.homeDirectory,
            "shell": request.shell,
        ]
        do {
            try fileManager.createDirectory(
                at: markerURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let data = try JSONSerialization.data(withJSONObject: marker, options: [.sortedKeys])
            try data.write(to: markerURL, options: .atomic)
            return (true, "Managed User ownership marker written. Credentials redacted.")
        } catch {
            return (false, "Managed User ownership marker write failed. Credentials redacted.")
        }
    }

    private func verifyManagedUserPTY(request: AlanXPCManagedTerminalAccountRequest) -> Bool {
        guard let account = accountRecord(for: request.accountName) else { return false }
        var master: Int32 = -1
        var pid: pid_t = 0
        let argvValues = [request.shell, "-lc", "exit 0"]
        let envValues = helperEnvironment(accountName: request.accountName, home: request.homeDirectory, shell: request.shell)
        let result = request.shell.withCString { executable in
            request.homeDirectory.withCString { workingDirectory in
                request.accountName.withCString { accountName in
                    withCStringArray(argvValues) { argv in
                        withCStringArray(envValues) { envp in
                            alanDarwinPtySpawnAsUser(
                                executable,
                                argv,
                                envp,
                                workingDirectory,
                                accountName,
                                account.uid,
                                account.gid,
                                24,
                                80,
                                &master,
                                &pid
                            )
                        }
                    }
                }
            }
        }
        defer {
            if master >= 0 {
                close(master)
            }
        }
        guard result == 0 else { return false }
        var status: Int32 = 0
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline {
            let waitResult = waitpid(pid, &status, WNOHANG)
            if waitResult == pid {
                return waitStatusExited(status) && waitStatusExitCode(status) == 0
            }
            Thread.sleep(forTimeInterval: 0.05)
        }
        _ = kill(pid, SIGTERM)
        _ = waitpid(pid, &status, 0)
        return false
    }

    private func validate(_ request: AlanXPCManagedTerminalAccountRequest) -> [AlanPrivilegedHelperXPCErrorCode] {
        var errors: [AlanPrivilegedHelperXPCErrorCode] = []
        let pattern = #"^[A-Za-z_][A-Za-z0-9_-]{0,31}$"#
        if request.accountName.range(of: pattern, options: .regularExpression) == nil
            || ["root", "daemon", "nobody"].contains(request.accountName.lowercased())
        {
            errors.append(.invalidAccountIdentifier)
        }
        if request.homeDirectory != request.canonicalHomeDirectory {
            errors.append(.invalidHomePath)
        }
        if request.shell != "/bin/zsh" {
            errors.append(.shellNotAllowed)
        }
        return errors
    }

    private func diagnosis(
        request: AlanXPCManagedTerminalAccountRequest,
        ownershipState: AlanXPCManagedUserOwnershipState,
        readinessState: AlanXPCManagedUserReadinessState,
        account: AlanManagedUserAccountRecord?,
        ptySmokeVerified: Bool,
        diagnostic: AlanXPCPrivilegedHelperDiagnostic?
    ) -> AlanXPCManagedUserDiagnosis {
        AlanXPCManagedUserDiagnosis(
            request: request,
            ownershipState: ownershipState,
            readinessState: readinessState,
            accountExists: account != nil,
            isAdmin: account?.isAdmin == true,
            homeDirectoryExists: fileManager.fileExists(atPath: request.homeDirectory),
            shellMatches: account?.shell == request.shell,
            hiddenFromLoginWindow: account?.hidden == true,
            terminalProfileID: nil,
            ptySmokeVerified: ptySmokeVerified,
            diagnostic: diagnostic
        )
    }

    private func accountRecord(for accountName: String) -> AlanManagedUserAccountRecord? {
        let result = runCommand(
            "/usr/bin/dscl",
            [
                ".",
                "-read",
                "/Users/\(accountName)",
                "UniqueID",
                "PrimaryGroupID",
                "NFSHomeDirectory",
                "UserShell",
                "IsHidden",
            ]
        )
        guard result.succeeded,
              let uidString = propertyValue("UniqueID", in: result.stdout),
              let gidString = propertyValue("PrimaryGroupID", in: result.stdout),
              let uid = uid_t(uidString),
              let gid = gid_t(gidString)
        else {
            return nil
        }
        let home = propertyValue("NFSHomeDirectory", in: result.stdout) ?? "/Users/\(accountName)"
        let shell = propertyValue("UserShell", in: result.stdout) ?? "/bin/zsh"
        let hidden = propertyValue("IsHidden", in: result.stdout) == "1"
        return AlanManagedUserAccountRecord(
            name: accountName,
            uid: uid,
            gid: gid,
            homeDirectory: home,
            shell: shell,
            hidden: hidden,
            isAdmin: isAdmin(accountName)
        )
    }

    private func isAdmin(_ accountName: String) -> Bool {
        let result = runCommand("/usr/sbin/dseditgroup", ["-o", "checkmember", "-m", accountName, "admin"])
        guard result.succeeded else { return false }
        let output = (result.stdout + "\n" + result.stderr).lowercased()
        return output.contains("yes") || (output.contains("is a member") && !output.contains("not a member"))
    }

    private func propertyValue(_ key: String, in output: String) -> String? {
        output
            .split(separator: "\n", omittingEmptySubsequences: false)
            .compactMap { line -> String? in
                let prefixes = ["\(key):", "dsAttrTypeNative:\(key):"]
                guard let prefix = prefixes.first(where: { line.hasPrefix($0) }) else {
                    return nil
                }
                let value = line.dropFirst(prefix.count)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                return value.isEmpty ? nil : value
            }
            .first
    }

    private func ownershipMarkerPath(for request: AlanXPCManagedTerminalAccountRequest) -> String {
        "\(identity.dataRootPath)/managed-users/\(request.accountName)/ownership.json"
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperXPCOperation,
        accountName: String?,
        code: AlanPrivilegedHelperXPCErrorCode?,
        message: String
    ) -> AlanXPCPrivilegedHelperDiagnostic {
        AlanXPCPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: identity.channelID,
            accountName: accountName,
            operation: operation.diagnosticOperationName,
            code: mappedAppErrorCode(code),
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message)
        )
    }

    private func failedApply(
        completed: [AlanXPCManagedUserHelperPlanStepKind],
        failed: AlanXPCManagedUserHelperPlanStepKind?,
        message: String
    ) -> AlanPrivilegedHelperXPCApplyResultPayload {
        AlanPrivilegedHelperXPCApplyResultPayload(
            completedHelperSteps: completed.map(\.rawValue),
            failedHelperStep: failed?.rawValue,
            cancelled: false,
            visibleDiagnostics: [AlanPrivilegedHelperSanitizer.sanitizedMessage(message)]
        )
    }

    private func runFixed(
        _ executable: String,
        _ arguments: [String]
    ) -> (succeeded: Bool, message: String) {
        let result = runCommand(executable, arguments)
        return result.succeeded
            ? (true, "Privileged helper operation completed. Credentials redacted.")
            : (false, "Privileged helper operation failed. Credentials redacted.")
    }

    private func runCommand(
        _ executable: String,
        _ arguments: [String]
    ) -> (succeeded: Bool, stdout: String, stderr: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return (false, "", "\(error)")
        }
        let stdout = String(data: outputPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let stderr = String(data: errorPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        return (process.terminationStatus == 0, stdout, stderr)
    }
}

private struct AlanManagedUserAccountRecord {
    let name: String
    let uid: uid_t
    let gid: gid_t
    let homeDirectory: String
    let shell: String
    let hidden: Bool
    let isAdmin: Bool
}

private final class AlanPrivilegedHelperPTYSessionStore {
    private static let maxPendingInputBytes = 4 * 1024 * 1024
    private let identity: AlanPrivilegedHelperXPCIdentity
    private var sessions: [String: AlanPrivilegedHelperPTYSession] = [:]

    init(identity: AlanPrivilegedHelperXPCIdentity) {
        self.identity = identity
    }

    func start(
        request: AlanXPCManagedUserPTYStartRequest,
        managedUserService: AlanPrivilegedHelperManagedUserService
    ) -> Result<AlanXPCManagedUserPTYSession, AlanXPCPrivilegedHelperDiagnostic> {
        switch managedUserService.accountReadyForPTY(
            accountName: request.accountName,
            homeDirectory: request.homeDirectory,
            shell: request.shell,
            channelID: request.channelID
        ) {
        case .failure(let diagnostic):
            return .failure(diagnostic)
        case .success(let account):
            var master: Int32 = -1
            var pid: pid_t = 0
            let shellName = URL(fileURLWithPath: request.shell).lastPathComponent
            let argvValues = ["-\(shellName)"]
            let envValues = helperEnvironment(accountName: request.accountName, home: request.homeDirectory, shell: request.shell)
            let workingDirectory = request.workingDirectory.isEmpty
                ? request.homeDirectory
                : request.workingDirectory
            let spawnResult = request.shell.withCString { executable in
                workingDirectory.withCString { workingDirectory in
                    request.accountName.withCString { accountName in
                        withCStringArray(argvValues) { argv in
                            withCStringArray(envValues) { envp in
                                alanDarwinPtySpawnAsUser(
                                    executable,
                                    argv,
                                    envp,
                                    workingDirectory,
                                    accountName,
                                    account.uid,
                                    account.gid,
                                    UInt16(max(1, min(request.rows, Int(UInt16.max)))),
                                    UInt16(max(1, min(request.columns, Int(UInt16.max)))),
                                    &master,
                                    &pid
                                )
                            }
                        }
                    }
                }
            }
            guard spawnResult == 0, master >= 0 else {
                if master >= 0 {
                    close(master)
                }
                return .failure(
                    diagnostic(
                        operation: .startManagedUserPTY,
                        accountName: request.accountName,
                        code: .ptySpawnFailed,
                        message: "Privileged helper could not start the managed-user PTY."
                    )
                )
            }
            setNonBlocking(master)
            let sessionID = UUID().uuidString
            sessions[sessionID] = AlanPrivilegedHelperPTYSession(
                sessionID: sessionID,
                accountName: request.accountName,
                contentID: request.contentID,
                masterFileDescriptor: master,
                processID: pid
            )
            return .success(
                AlanXPCManagedUserPTYSession(
                    sessionID: sessionID,
                    accountName: request.accountName,
                    contentID: request.contentID,
                    helperOwnsChildProcess: true,
                    sanitizedMessage: "Privileged helper PTY session started."
                )
            )
        }
    }

    func read(
        _ request: AlanXPCManagedUserPTYReadRequest
    ) -> Result<AlanXPCManagedUserPTYOutputChunk, AlanXPCPrivilegedHelperDiagnostic> {
        guard let session = sessions[request.sessionID] else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: nil,
                    code: .helperUnavailable,
                    message: "Managed User PTY session is missing."
                )
            )
        }
        guard session.masterFileDescriptor >= 0 else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: session.accountName,
                    code: .helperUnavailable,
                    message: "Managed User PTY output stream is closed."
                )
            )
        }
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return .failure(diagnostic)
        }

        let maxBytes = max(1, min(request.maxBytes, 64 * 1024))
        var buffer = [UInt8](repeating: 0, count: maxBytes)
        let count = Darwin.read(session.masterFileDescriptor, &buffer, maxBytes)
        if count > 0 {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(buffer.prefix(count)),
                    final: false,
                    sanitizedMessage: "Privileged helper read Managed User PTY output."
                )
            )
        }
        if count == 0 {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(),
                    final: true,
                    sanitizedMessage: "Managed User PTY output stream ended."
                )
            )
        }
        if errno == EAGAIN || errno == EWOULDBLOCK {
            return .success(
                AlanXPCManagedUserPTYOutputChunk(
                    sessionID: request.sessionID,
                    data: Data(),
                    final: false,
                    sanitizedMessage: nil
                )
            )
        }
        return .failure(
            diagnostic(
                operation: .readManagedUserPTY,
                accountName: session.accountName,
                code: .helperUnavailable,
                message: "Managed User PTY output read failed."
            )
        )
    }

    func write(_ request: AlanXPCManagedUserPTYInputRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        let data = request.data
        guard data.count <= Self.maxPendingInputBytes,
              session.pendingInput.count <= Self.maxPendingInputBytes - data.count
        else {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: "Managed User PTY input queue is full.")
        }
        session.pendingInput.append(data)
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return rejected(.writeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: diagnostic.sanitizedMessage)
        }
        return accepted(.writeManagedUserPTY, session: session, message: "Privileged helper accepted PTY input.")
    }

    func resize(_ request: AlanXPCManagedUserPTYResizeRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.resizeManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        var size = winsize(
            ws_row: UInt16(max(1, min(request.rows, Int(UInt16.max)))),
            ws_col: UInt16(max(1, min(request.columns, Int(UInt16.max)))),
            ws_xpixel: 0,
            ws_ypixel: 0
        )
        guard ioctl(session.masterFileDescriptor, TIOCSWINSZ, &size) == 0 else {
            return rejected(.resizeManagedUserPTY, sessionID: request.sessionID, accountName: session.accountName, message: "Managed User PTY resize failed.")
        }
        return accepted(.resizeManagedUserPTY, session: session, message: "Privileged helper resized PTY session.")
    }

    func closeInput(sessionID: String) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[sessionID] else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, message: "Managed User PTY session is missing.")
        }
        guard session.masterFileDescriptor >= 0 else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: "Managed User PTY input stream is closed.")
        }
        let eof = Data([UInt8(4)])
        guard session.pendingInput.count <= Self.maxPendingInputBytes - eof.count else {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: "Managed User PTY input queue is full.")
        }
        session.pendingInput.append(eof)
        if case .failure(let diagnostic) = drainPendingInput(session) {
            return rejected(.closeManagedUserPTYInput, sessionID: sessionID, accountName: session.accountName, message: diagnostic.sanitizedMessage)
        }
        return accepted(.closeManagedUserPTYInput, session: session, message: "Privileged helper sent PTY EOF.")
    }

    func signal(_ request: AlanXPCManagedUserPTYSignalRequest) -> AlanXPCManagedUserPTYControlResult {
        guard let session = sessions[request.sessionID] else {
            return rejected(.signalManagedUserPTY, sessionID: request.sessionID, message: "Managed User PTY session is missing.")
        }
        let signalNumber: Int32
        switch request.signal {
        case .interrupt:
            signalNumber = SIGINT
        case .terminate:
            signalNumber = SIGTERM
        case .kill:
            signalNumber = SIGKILL
        }
        if kill(-session.processID, signalNumber) != 0 {
            _ = kill(session.processID, signalNumber)
        }
        return accepted(.signalManagedUserPTY, session: session, message: "Privileged helper signaled PTY session.")
    }

    func observeExit(sessionID: String) -> AlanXPCManagedUserPTYExitObservation? {
        guard let session = sessions[sessionID] else { return nil }
        if let observation = session.finalObservation {
            return observation
        }
        var status: Int32 = 0
        let result = waitpid(session.processID, &status, WNOHANG)
        guard result == session.processID else { return nil }
        let observation = exitObservation(sessionID: sessionID, status: status)
        session.finalObservation = observation
        closeIfNeeded(session)
        sessions.removeValue(forKey: sessionID)
        return observation
    }

    func terminate(sessionID: String) -> AlanXPCPrivilegedHelperDiagnostic {
        guard let session = sessions[sessionID] else {
            return diagnostic(
                operation: .terminatePTY,
                accountName: nil,
                code: nil,
                message: "Managed User PTY session was already absent."
            )
        }
        _ = kill(-session.processID, SIGTERM)
        _ = kill(session.processID, SIGTERM)
        closeIfNeeded(session)
        var status: Int32 = 0
        _ = waitpid(session.processID, &status, WNOHANG)
        session.finalObservation = AlanXPCManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Privileged helper terminated PTY session."
        )
        sessions.removeValue(forKey: sessionID)
        return diagnostic(
            operation: .terminatePTY,
            accountName: session.accountName,
            code: nil,
            message: "Privileged helper terminated PTY session."
        )
    }

    func terminateAll() {
        for sessionID in Array(sessions.keys) {
            _ = terminate(sessionID: sessionID)
        }
    }

    private func closeIfNeeded(_ session: AlanPrivilegedHelperPTYSession) {
        if session.masterFileDescriptor >= 0 {
            close(session.masterFileDescriptor)
            session.masterFileDescriptor = -1
        }
    }

    private func setNonBlocking(_ fileDescriptor: Int32) {
        let flags = fcntl(fileDescriptor, F_GETFL)
        guard flags >= 0 else { return }
        _ = fcntl(fileDescriptor, F_SETFL, flags | O_NONBLOCK)
    }

    private func drainPendingInput(
        _ session: AlanPrivilegedHelperPTYSession
    ) -> Result<Void, AlanXPCPrivilegedHelperDiagnostic> {
        while !session.pendingInput.isEmpty {
            let written = session.pendingInput.withUnsafeBytes { buffer -> Int in
                guard let base = buffer.baseAddress else { return 0 }
                return Darwin.write(session.masterFileDescriptor, base, buffer.count)
            }
            if written > 0 {
                session.pendingInput.removeFirst(written)
                continue
            }
            if written == 0 {
                return .success(())
            }
            if errno == EINTR {
                continue
            }
            if errno == EAGAIN || errno == EWOULDBLOCK {
                return .success(())
            }
            return .failure(
                diagnostic(
                    operation: .writeManagedUserPTY,
                    accountName: session.accountName,
                    code: .helperUnavailable,
                    message: "Managed User PTY input failed."
                )
            )
        }
        return .success(())
    }

    private func accepted(
        _ operation: AlanPrivilegedHelperXPCOperation,
        session: AlanPrivilegedHelperPTYSession,
        message: String
    ) -> AlanXPCManagedUserPTYControlResult {
        AlanXPCManagedUserPTYControlResult(
            accepted: true,
            diagnostic: diagnostic(
                operation: operation,
                accountName: session.accountName,
                code: nil,
                message: message
            )
        )
    }

    private func rejected(
        _ operation: AlanPrivilegedHelperXPCOperation,
        sessionID: String,
        accountName: String? = nil,
        message: String
    ) -> AlanXPCManagedUserPTYControlResult {
        AlanXPCManagedUserPTYControlResult(
            accepted: false,
            diagnostic: diagnostic(
                operation: operation,
                accountName: accountName,
                code: .helperUnavailable,
                message: message
            )
        )
    }

    private func exitObservation(
        sessionID: String,
        status: Int32
    ) -> AlanXPCManagedUserPTYExitObservation {
        if waitStatusExited(status) {
            return AlanXPCManagedUserPTYExitObservation(
                sessionID: sessionID,
                final: true,
                exitCode: waitStatusExitCode(status),
                terminatingSignal: nil,
                sanitizedMessage: "Managed User PTY child exited."
            )
        }
        if waitStatusSignaled(status) {
            return AlanXPCManagedUserPTYExitObservation(
                sessionID: sessionID,
                final: true,
                exitCode: nil,
                terminatingSignal: waitStatusTermSignal(status),
                sanitizedMessage: "Managed User PTY child exited after signal."
            )
        }
        return AlanXPCManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Managed User PTY child exited."
        )
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperXPCOperation,
        accountName: String?,
        code: AlanPrivilegedHelperXPCErrorCode?,
        message: String
    ) -> AlanXPCPrivilegedHelperDiagnostic {
        AlanXPCPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: identity.channelID,
            accountName: accountName,
            operation: operation.diagnosticOperationName,
            code: mappedAppErrorCode(code),
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message)
        )
    }
}

private final class AlanPrivilegedHelperPTYSession {
    let sessionID: String
    let accountName: String
    let contentID: String
    var masterFileDescriptor: Int32
    let processID: pid_t
    var finalObservation: AlanXPCManagedUserPTYExitObservation?
    var pendingInput = Data()

    init(
        sessionID: String,
        accountName: String,
        contentID: String,
        masterFileDescriptor: Int32,
        processID: pid_t
    ) {
        self.sessionID = sessionID
        self.accountName = accountName
        self.contentID = contentID
        self.masterFileDescriptor = masterFileDescriptor
        self.processID = processID
    }

    deinit {
        if masterFileDescriptor >= 0 {
            close(masterFileDescriptor)
        }
    }
}

private func helperEnvironment(
    accountName: String,
    home: String,
    shell: String
) -> [String] {
    [
        "HOME=\(home)",
        "USER=\(accountName)",
        "LOGNAME=\(accountName)",
        "SHELL=\(shell)",
        "TERM=xterm-256color",
        "PATH=/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin",
    ]
}

private func mappedAppErrorCode(_ code: AlanPrivilegedHelperXPCErrorCode?) -> String? {
    guard let code else { return nil }
    switch code {
    case .invalidRequest, .invalidAccountIdentifier:
        return "invalid_account_identifier"
    case .unsupportedOperation:
        return "unsupported_operation"
    case .channelMismatch:
        return "channel_mismatch"
    case .clientRequirementFailed:
        return "client_requirement_failed"
    case .connectionFailed, .helperUnavailable, .timeout:
        return "helper_unavailable"
    case .invalidHomePath:
        return "invalid_home_path"
    case .shellNotAllowed:
        return "shell_not_allowed"
    case .accountNotAlanManaged:
        return "account_not_alan_managed"
    case .ptySpawnFailed:
        return "pty_spawn_failed"
    }
}

private func encodePayload<T: Encodable>(_ payload: T) -> Data? {
    try? JSONEncoder().encode(payload)
}

private func withCStringArray<Result>(
    _ strings: [String],
    _ body: (UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> Result
) -> Result {
    let cStrings = strings.map { strdup($0) }
    let argv = UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>.allocate(capacity: cStrings.count + 1)
    for (index, value) in cStrings.enumerated() {
        argv[index] = value
    }
    argv[cStrings.count] = nil
    defer {
        for value in cStrings {
            free(value)
        }
        argv.deallocate()
    }
    return body(argv)
}

private func waitStatusTermSignal(_ status: Int32) -> Int32 {
    status & 0x7f
}

private func waitStatusExited(_ status: Int32) -> Bool {
    waitStatusTermSignal(status) == 0
}

private func waitStatusSignaled(_ status: Int32) -> Bool {
    let signal = waitStatusTermSignal(status)
    return signal != 0 && signal != 0x7f
}

private func waitStatusExitCode(_ status: Int32) -> Int32 {
    (status >> 8) & 0xff
}
