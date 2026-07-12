import Foundation

#if os(macOS)
final class AlanPrivilegedHelperAppClient: AlanPrivilegedHelperClienting {
    private let helperIdentity: AlanPrivilegedHelperIdentity
    private let xpcClient: AlanPrivilegedHelperXPCClient
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()

    init(channel: AlanInstallChannel = .current()) {
        helperIdentity = channel.privilegedHelperIdentity
        xpcClient = AlanPrivilegedHelperXPCClient(identity: helperIdentity.xpcIdentity)
    }

    func status() -> AlanPrivilegedHelperStatus {
        AlanPrivilegedHelperStatus.fromXPCStatus(
            xpcClient.helperStatus(),
            identity: helperIdentity
        )
    }

    func diagnoseManagedUser(_ request: ManagedTerminalAccountRequest) -> AlanManagedUserDiagnosis {
        let response = perform(.diagnoseManagedUser, payload: request)
        if let diagnosis: AlanManagedUserDiagnosis = decodedPayload(response) {
            return diagnosis
        }
        return .helperUnavailable(
            request: request,
            status: unavailableStatus(from: response)
        )
    }

    func applyManagedUserPlan(
        _ plan: AlanManagedUserHelperPlan
    ) -> ManagedTerminalAccountApplyResult {
        let response = perform(.applyManagedUserPlan, payload: plan)
        guard let payload: AlanPrivilegedHelperXPCApplyResultPayload = decodedPayload(response) else {
            return helperApplyFailure(
                firstStep: plan.steps.first?.kind,
                message: response.sanitizedMessage
            )
        }
        return ManagedTerminalAccountApplyResult(
            completedSteps: payload.completedHelperSteps.compactMap(helperPlanStepKind),
            failedStep: payload.failedHelperStep.flatMap(helperPlanStepKind),
            cancelled: payload.cancelled,
            visibleDiagnostics: payload.visibleDiagnostics
        )
    }

    func startManagedUserPTY(
        _ request: AlanManagedUserPTYStartRequest
    ) -> Result<AlanManagedUserPTYSession, AlanPrivilegedHelperDiagnostic> {
        let response = perform(.startManagedUserPTY, payload: request)
        if let session: AlanManagedUserPTYSession = decodedPayload(response), response.accepted {
            return .success(session)
        }
        if let diagnostic: AlanPrivilegedHelperDiagnostic = decodedPayload(response) {
            return .failure(diagnostic)
        }
        return .failure(
            diagnostic(from: response,
                operation: .startManagedUserPTY,
                accountName: request.accountName,
                fallbackCode: .ptySpawnFailed
            )
        )
    }

    func writeManagedUserPTY(
        _ request: AlanManagedUserPTYInputRequest
    ) -> AlanManagedUserPTYControlResult {
        controlResult(
            perform(.writeManagedUserPTY, payload: request),
            operation: .writeManagedUserPTY,
            sessionID: request.sessionID
        )
    }

    func readManagedUserPTY(
        _ request: AlanManagedUserPTYReadRequest
    ) -> Result<AlanManagedUserPTYOutputChunk, AlanPrivilegedHelperDiagnostic> {
        let response = perform(.readManagedUserPTY, payload: request)
        if let chunk: AlanManagedUserPTYOutputChunk = decodedPayload(response), response.accepted {
            return .success(chunk)
        }
        if let diagnostic: AlanPrivilegedHelperDiagnostic = decodedPayload(response) {
            return .failure(diagnostic)
        }
        return .failure(
            diagnostic(
                from: response,
                operation: .readManagedUserPTY,
                accountName: nil,
                fallbackCode: .helperUnavailable
            )
        )
    }

    func resizeManagedUserPTY(
        _ request: AlanManagedUserPTYResizeRequest
    ) -> AlanManagedUserPTYControlResult {
        controlResult(
            perform(.resizeManagedUserPTY, payload: request),
            operation: .resizeManagedUserPTY,
            sessionID: request.sessionID
        )
    }

    func closeManagedUserPTYInput(sessionID: String) -> AlanManagedUserPTYControlResult {
        controlResult(
            perform(
                .closeManagedUserPTYInput,
                payload: AlanPrivilegedHelperXPCSessionPayload(sessionID: sessionID)
            ),
            operation: .closeManagedUserPTYInput,
            sessionID: sessionID
        )
    }

    func signalManagedUserPTY(
        _ request: AlanManagedUserPTYSignalRequest
    ) -> AlanManagedUserPTYControlResult {
        controlResult(
            perform(.signalManagedUserPTY, payload: request),
            operation: .signalManagedUserPTY,
            sessionID: request.sessionID
        )
    }

    func observeManagedUserPTYExit(sessionID: String) -> AlanManagedUserPTYExitObservation? {
        let response = perform(
            .observeManagedUserPTYExit,
            payload: AlanPrivilegedHelperXPCSessionPayload(sessionID: sessionID)
        )
        return decodedPayload(response)
    }

    func terminatePTY(sessionID: String) -> AlanPrivilegedHelperDiagnostic {
        let response = perform(
            .terminatePTY,
            payload: AlanPrivilegedHelperXPCSessionPayload(sessionID: sessionID)
        )
        if let diagnostic: AlanPrivilegedHelperDiagnostic = decodedPayload(response) {
            return diagnostic
        }
        return diagnostic(from: response, operation: .terminatePTY, accountName: nil)
    }

    func removeManagedUserIntegration(
        _ request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountApplyResult {
        let response = perform(.removeManagedUserIntegration, payload: request)
        guard let payload: AlanPrivilegedHelperXPCApplyResultPayload = decodedPayload(response) else {
            return helperApplyFailure(
                firstStep: .removeManagedUserIntegration,
                message: response.sanitizedMessage
            )
        }
        return ManagedTerminalAccountApplyResult(
            completedSteps: payload.completedHelperSteps.compactMap(helperPlanStepKind),
            failedStep: payload.failedHelperStep.flatMap(helperPlanStepKind),
            cancelled: payload.cancelled,
            visibleDiagnostics: payload.visibleDiagnostics
        )
    }

    private func perform<T: Encodable>(
        _ operation: AlanPrivilegedHelperXPCOperation,
        payload: T
    ) -> AlanPrivilegedHelperXPCResponse {
        let payloadData = try? encoder.encode(payload)
        return xpcClient.perform(operation: operation, payload: payloadData)
    }

    private func decodedPayload<T: Decodable>(_ response: AlanPrivilegedHelperXPCResponse) -> T? {
        guard let payload = response.payload else { return nil }
        return try? decoder.decode(T.self, from: payload)
    }

    private func unavailableStatus(
        from response: AlanPrivilegedHelperXPCResponse
    ) -> AlanPrivilegedHelperStatus {
        AlanPrivilegedHelperStatus(
            state: .unavailable,
            identity: helperIdentity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: response.sanitizedMessage
        )
    }

    private func controlResult(
        _ response: AlanPrivilegedHelperXPCResponse,
        operation: AlanPrivilegedHelperOperation,
        sessionID: String
    ) -> AlanManagedUserPTYControlResult {
        if let result: AlanManagedUserPTYControlResult = decodedPayload(response) {
            return result
        }
        return .rejected(
            operation: operation,
            channelID: helperIdentity.channelID,
            accountName: nil,
            code: mappedErrorCode(response.errorCode) ?? .helperUnavailable,
            message: response.sanitizedMessage.isEmpty
                ? "Privileged helper PTY request failed for session \(sessionID)."
                : response.sanitizedMessage
        )
    }

    private func helperApplyFailure(
        firstStep: AlanManagedUserHelperPlanStepKind?,
        message: String
    ) -> ManagedTerminalAccountApplyResult {
        ManagedTerminalAccountApplyResult(
            completedSteps: [],
            failedStep: firstStep.map { .helperStep($0) },
            cancelled: false,
            visibleDiagnostics: [
                message.isEmpty
                    ? "Privileged helper operation failed. Credentials redacted."
                    : message,
            ]
        )
    }

    private func helperPlanStepKind(_ rawValue: String) -> ManagedTerminalAccountPlanStepKind? {
        AlanManagedUserHelperPlanStepKind(rawValue: rawValue).map {
            ManagedTerminalAccountPlanStepKind.helperStep($0)
        }
    }

    private func diagnostic(
        from response: AlanPrivilegedHelperXPCResponse,
        operation: AlanPrivilegedHelperOperation,
        accountName: String?,
        fallbackCode: AlanPrivilegedHelperErrorCode? = nil
    ) -> AlanPrivilegedHelperDiagnostic {
        diagnostic(
            operation: operation,
            accountName: accountName,
            code: mappedErrorCode(response.errorCode) ?? fallbackCode,
            message: response.sanitizedMessage
        )
    }

    private func mappedErrorCode(
        _ code: AlanPrivilegedHelperXPCErrorCode?
    ) -> AlanPrivilegedHelperErrorCode? {
        guard let code else { return nil }
        switch code {
        case .invalidRequest, .invalidAccountIdentifier:
            return .invalidAccountIdentifier
        case .unsupportedOperation:
            return .unsupportedOperation
        case .channelMismatch:
            return .channelMismatch
        case .clientRequirementFailed:
            return .clientRequirementFailed
        case .connectionFailed, .helperUnavailable, .timeout:
            return .helperUnavailable
        case .invalidHomePath:
            return .invalidHomePath
        case .shellNotAllowed:
            return .shellNotAllowed
        case .accountNotAlanManaged:
            return .accountNotAlanManaged
        case .ptySpawnFailed:
            return .ptySpawnFailed
        }
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperOperation,
        accountName: String?,
        code: AlanPrivilegedHelperErrorCode?,
        message: String
    ) -> AlanPrivilegedHelperDiagnostic {
        AlanPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: helperIdentity.channelID,
            accountName: accountName,
            operation: operation,
            code: code,
            sanitizedMessage: AlanPrivilegedHelperSanitizer.sanitizedMessage(message)
        )
    }
}

private struct AlanPrivilegedHelperXPCSessionPayload: Codable, Equatable {
    let sessionID: String
}
#endif
