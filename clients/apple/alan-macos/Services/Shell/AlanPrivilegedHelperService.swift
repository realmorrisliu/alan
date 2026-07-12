import Foundation
#if os(macOS)
import ServiceManagement
#endif

enum AlanPrivilegedHelperLifecycleAction: String, Codable, Equatable, Hashable {
    case install
    case update
    case uninstall
    case validateSignature = "validate_signature"
}

extension AlanPrivilegedHelperIdentity {
    var xpcIdentity: AlanPrivilegedHelperXPCIdentity {
        AlanPrivilegedHelperXPCIdentity(
            channelID: channelID,
            helperBundleIdentifier: helperBundleIdentifier,
            machServiceName: machServiceName,
            expectedClientRequirement: expectedClientRequirement
        )
    }
}

extension AlanPrivilegedHelperStatus {
    static func fromXPCStatus(
        _ response: AlanPrivilegedHelperXPCResponse,
        identity: AlanPrivilegedHelperIdentity
    ) -> AlanPrivilegedHelperStatus {
        guard response.accepted else {
            return AlanPrivilegedHelperStatus(
                state: .unavailable,
                identity: identity,
                installedVersion: nil,
                expectedVersion: String(AlanPrivilegedHelperProtocolStatus.currentVersion),
                sanitizedMessage: response.sanitizedMessage
            )
        }
        let protocolStatus = response.payload.flatMap {
            try? JSONDecoder().decode(AlanPrivilegedHelperProtocolStatus.self, from: $0)
        }
        let isCurrent = protocolStatus?.protocolVersion
            == AlanPrivilegedHelperProtocolStatus.currentVersion
        return AlanPrivilegedHelperStatus(
            state: isCurrent ? .healthy : .outdated,
            identity: identity,
            installedVersion: protocolStatus.map { String($0.protocolVersion) },
            expectedVersion: String(AlanPrivilegedHelperProtocolStatus.currentVersion),
            sanitizedMessage: isCurrent ? nil : "Privileged helper update required."
        )
    }
}

struct AlanPrivilegedHelperLifecycleResult: Codable, Equatable {
    let action: AlanPrivilegedHelperLifecycleAction
    let status: AlanPrivilegedHelperStatus
    let diagnostic: AlanPrivilegedHelperDiagnostic?

    var succeeded: Bool {
        diagnostic == nil
    }
}

protocol AlanPrivilegedHelperLifecycleManaging {
    var identity: AlanPrivilegedHelperIdentity { get }

    func status() -> AlanPrivilegedHelperStatus
    func installOrUpdate() -> AlanPrivilegedHelperLifecycleResult
    func uninstall() -> AlanPrivilegedHelperLifecycleResult
    func validateSignature() -> AlanPrivilegedHelperStatus
}

#if os(macOS)
final class AlanPrivilegedHelperAppServiceManager: AlanPrivilegedHelperLifecycleManaging {
    let identity: AlanPrivilegedHelperIdentity

    init(channel: AlanInstallChannel = .current()) {
        self.identity = channel.privilegedHelperIdentity
    }

    func status() -> AlanPrivilegedHelperStatus {
        let registrationStatus = mapStatus(service.status, message: nil)
        guard registrationStatus.state == .healthy else { return registrationStatus }
        return AlanPrivilegedHelperStatus.fromXPCStatus(
            AlanPrivilegedHelperXPCClient(identity: identity.xpcIdentity).helperStatus(),
            identity: identity
        )
    }

    func installOrUpdate() -> AlanPrivilegedHelperLifecycleResult {
        let currentState = status().state
        let action: AlanPrivilegedHelperLifecycleAction = currentState == .healthy
            || currentState == .outdated
            ? .update
            : .install
        if action == .update {
            do {
                try unregisterForUpdate()
            } catch {
                return failure(
                    action: action,
                    code: .helperUnavailable,
                    message: sanitizedLifecycleFailure(
                        prefix: "Privileged helper update preparation failed",
                        error: error
                    )
                )
            }
        }
        return register(action: action)
    }

    func uninstall() -> AlanPrivilegedHelperLifecycleResult {
        do {
            try service.unregister()
            return AlanPrivilegedHelperLifecycleResult(
                action: .uninstall,
                status: mapStatus(service.status, message: nil),
                diagnostic: nil
            )
        } catch {
            return failure(
                action: .uninstall,
                code: .helperUnavailable,
                message: sanitizedLifecycleFailure(
                    prefix: "Privileged helper unregister failed",
                    error: error
                )
            )
        }
    }

    func validateSignature() -> AlanPrivilegedHelperStatus {
        mapStatus(service.status, message: nil)
    }

    private var service: SMAppService {
        SMAppService.daemon(plistName: identity.plistName)
    }

    private func register(
        action: AlanPrivilegedHelperLifecycleAction
    ) -> AlanPrivilegedHelperLifecycleResult {
        var lastError: Error?
        for attempt in 0..<5 {
            do {
                try service.register()
                return AlanPrivilegedHelperLifecycleResult(
                    action: action,
                    status: status(),
                    diagnostic: nil
                )
            } catch {
                lastError = error
                if attempt == 4 || !shouldRetryRegistration(error) {
                    break
                }
                Thread.sleep(forTimeInterval: 0.75)
            }
        }

        return failure(
            action: action,
            code: .helperUnavailable,
            message: sanitizedLifecycleFailure(
                prefix: "Privileged helper registration failed",
                error: lastError ?? CocoaError(.featureUnsupported)
            )
        )
    }

    private func unregisterForUpdate() throws {
        try service.unregister()
        waitForUnregistration()
    }

    private func waitForUnregistration() {
        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            switch service.status {
            case .notRegistered, .notFound:
                return
            case .enabled, .requiresApproval:
                Thread.sleep(forTimeInterval: 0.25)
            @unknown default:
                return
            }
        }
    }

    private func shouldRetryRegistration(_ error: Error) -> Bool {
        let nsError = error as NSError
        if nsError.domain == "SMAppServiceErrorDomain" && nsError.code == 1 {
            return true
        }
        return nsError.localizedDescription.localizedCaseInsensitiveContains("Operation not permitted")
    }

    private func sanitizedLifecycleFailure(prefix: String, error: Error) -> String {
        let detail = "\(error)"
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !detail.isEmpty else {
            return "\(prefix)."
        }
        return AlanPrivilegedHelperSanitizer.sanitizedMessage("\(prefix): \(detail)")
    }

    private func mapStatus(
        _ status: SMAppService.Status,
        message: String?
    ) -> AlanPrivilegedHelperStatus {
        let state: AlanPrivilegedHelperStatusState
        let sanitizedMessage: String?
        switch status {
        case .notRegistered, .notFound:
            state = .notInstalled
            sanitizedMessage = message
        case .enabled:
            state = .healthy
            sanitizedMessage = message
        case .requiresApproval:
            state = .unavailable
            sanitizedMessage = message ?? "Privileged helper requires user approval."
        @unknown default:
            state = .unavailable
            sanitizedMessage = message ?? "Privileged helper status is unavailable."
        }
        return AlanPrivilegedHelperStatus(
            state: state,
            identity: identity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: sanitizedMessage
        )
    }

    private func failure(
        action: AlanPrivilegedHelperLifecycleAction,
        code: AlanPrivilegedHelperErrorCode,
        message: String
    ) -> AlanPrivilegedHelperLifecycleResult {
        let status = AlanPrivilegedHelperStatus(
            state: .unavailable,
            identity: identity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: message
        )
        return AlanPrivilegedHelperLifecycleResult(
            action: action,
            status: status,
            diagnostic: AlanPrivilegedHelperDiagnostic(
                operationID: UUID().uuidString,
                channelID: identity.channelID,
                accountName: nil,
                operation: .helperStatus,
                code: code,
                sanitizedMessage: message
            )
        )
    }
}
#endif

final class AlanPrivilegedHelperFakeLifecycleManager: AlanPrivilegedHelperLifecycleManaging {
    let identity: AlanPrivilegedHelperIdentity
    var currentStatus: AlanPrivilegedHelperStatus
    var deniedActions: Set<AlanPrivilegedHelperLifecycleAction>
    private(set) var performedActions: [AlanPrivilegedHelperLifecycleAction] = []

    init(
        channel: AlanInstallChannel = .current(),
        state: AlanPrivilegedHelperStatusState = .notInstalled,
        deniedActions: Set<AlanPrivilegedHelperLifecycleAction> = []
    ) {
        self.identity = channel.privilegedHelperIdentity
        self.currentStatus = AlanPrivilegedHelperStatus(
            state: state,
            identity: channel.privilegedHelperIdentity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: nil
        )
        self.deniedActions = deniedActions
    }

    func status() -> AlanPrivilegedHelperStatus {
        currentStatus
    }

    func installOrUpdate() -> AlanPrivilegedHelperLifecycleResult {
        let action: AlanPrivilegedHelperLifecycleAction = currentStatus.state == .healthy
            ? .update
            : .install
        return perform(action: action, nextState: .healthy)
    }

    func uninstall() -> AlanPrivilegedHelperLifecycleResult {
        perform(action: .uninstall, nextState: .notInstalled)
    }

    func validateSignature() -> AlanPrivilegedHelperStatus {
        performedActions.append(.validateSignature)
        return currentStatus
    }

    private func perform(
        action: AlanPrivilegedHelperLifecycleAction,
        nextState: AlanPrivilegedHelperStatusState
    ) -> AlanPrivilegedHelperLifecycleResult {
        performedActions.append(action)
        if deniedActions.contains(action) {
            let message = "Privileged helper \(action.rawValue) was denied."
            let status = AlanPrivilegedHelperStatus(
                state: .unavailable,
                identity: identity,
                installedVersion: currentStatus.installedVersion,
                expectedVersion: currentStatus.expectedVersion,
                sanitizedMessage: message
            )
            return AlanPrivilegedHelperLifecycleResult(
                action: action,
                status: status,
                diagnostic: AlanPrivilegedHelperDiagnostic(
                    operationID: UUID().uuidString,
                    channelID: identity.channelID,
                    accountName: nil,
                    operation: .helperStatus,
                    code: .helperUnavailable,
                    sanitizedMessage: message
                )
            )
        }
        currentStatus = AlanPrivilegedHelperStatus(
            state: nextState,
            identity: identity,
            installedVersion: currentStatus.installedVersion,
            expectedVersion: currentStatus.expectedVersion,
            sanitizedMessage: nil
        )
        return AlanPrivilegedHelperLifecycleResult(
            action: action,
            status: currentStatus,
            diagnostic: nil
        )
    }
}
