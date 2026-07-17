// Script/test support only. The Alan macOS app target must use the real helper client.
import Foundation

final class AlanPrivilegedHelperFakeClient: AlanPrivilegedHelperClienting {
    var helperStatus: AlanPrivilegedHelperStatus
    var diagnosesByAccount: [String: AlanManagedUserDiagnosis]
    var deniedOperation: AlanPrivilegedHelperOperation?
    var appliedPlans: [AlanManagedUserHelperPlan] = []
    var startedPTYRequests: [AlanManagedUserPTYStartRequest] = []
    var readPTYRequests: [AlanManagedUserPTYReadRequest] = []
    var writtenPTYInputRequests: [AlanManagedUserPTYInputRequest] = []
    var resizedPTYRequests: [AlanManagedUserPTYResizeRequest] = []
    var closedPTYInputSessionIDs: [String] = []
    var signaledPTYRequests: [AlanManagedUserPTYSignalRequest] = []
    var terminatedPTYSessionIDs: [String] = []
    var exitObservationsBySessionID: [String: AlanManagedUserPTYExitObservation] = [:]
    var outputChunksBySessionID: [String: [Data]] = [:]
    private var startedPTYSessionAccounts: [String: String] = [:]

    init(
        channel: AlanInstallChannel = .current(),
        statusState: AlanPrivilegedHelperStatusState = .healthy,
        diagnosesByAccount: [String: AlanManagedUserDiagnosis] = [:]
    ) {
        helperStatus = AlanPrivilegedHelperStatus(
            state: statusState,
            identity: channel.privilegedHelperIdentity,
            installedVersion: nil,
            expectedVersion: nil,
            sanitizedMessage: nil
        )
        self.diagnosesByAccount = diagnosesByAccount
    }

    func status() -> AlanPrivilegedHelperStatus {
        helperStatus
    }

    func diagnoseManagedUser(_ request: ManagedTerminalAccountRequest) -> AlanManagedUserDiagnosis {
        if let diagnosis = diagnosesByAccount[request.accountName] {
            return diagnosis
        }
        return AlanManagedUserDiagnosis(
            request: request,
            ownershipState: .missing,
            readinessState: helperStatus.isHealthy ? .accountMissing : .helperUnavailable,
            accountExists: false,
            isAdmin: false,
            homeDirectoryExists: false,
            homeDirectoryMatches: false,
            shellMatches: false,
            hiddenFromLoginWindow: false,
            terminalProfileID: nil,
            ptySmokeVerified: false,
            diagnostic: helperStatus.isHealthy ? nil : diagnostic(
                operation: .diagnoseManagedUser,
                accountName: request.accountName,
                code: .helperUnavailable,
                message: "Privileged helper is unavailable."
            )
        )
    }

    func applyManagedUserPlan(_ plan: AlanManagedUserHelperPlan) -> ManagedTerminalAccountApplyResult {
        if deniedOperation == .applyManagedUserPlan || !helperStatus.isHealthy {
            return ManagedTerminalAccountApplyResult(
                completedSteps: [],
                failedStep: plan.steps.first.map { .helperStep($0.kind) },
                cancelled: false,
                visibleDiagnostics: ["Privileged helper rejected the Managed User plan. Credentials redacted."]
            )
        }
        appliedPlans.append(plan)
        return ManagedTerminalAccountApplyResult(
            completedSteps: plan.steps.map { ManagedTerminalAccountPlanStepKind.helperStep($0.kind) },
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper applied the Managed User plan. Credentials redacted."]
        )
    }

    func startManagedUserPTY(
        _ request: AlanManagedUserPTYStartRequest
    ) -> Result<AlanManagedUserPTYSession, AlanPrivilegedHelperDiagnostic> {
        guard helperStatus.isHealthy, deniedOperation != .startManagedUserPTY else {
            return .failure(
                diagnostic(
                    operation: .startManagedUserPTY,
                    accountName: request.accountName,
                    code: .ptySpawnFailed,
                    message: "Privileged helper could not start the managed-user PTY."
                )
            )
        }
        startedPTYRequests.append(request)
        let session = AlanManagedUserPTYSession(
            sessionID: "fake-\(request.contentID)",
            accountName: request.accountName,
            contentID: request.contentID,
            helperOwnsChildProcess: true,
            sanitizedMessage: "Fake helper PTY session started."
        )
        startedPTYSessionAccounts[session.sessionID] = session.accountName
        return .success(session)
    }

    func readManagedUserPTY(
        _ request: AlanManagedUserPTYReadRequest
    ) -> Result<AlanManagedUserPTYOutputChunk, AlanPrivilegedHelperDiagnostic> {
        readPTYRequests.append(request)
        guard helperStatus.isHealthy, deniedOperation != .readManagedUserPTY else {
            return .failure(
                diagnostic(
                    operation: .readManagedUserPTY,
                    accountName: startedPTYSessionAccounts[request.sessionID],
                    code: helperStatus.isHealthy ? .unsupportedOperation : .helperUnavailable,
                    message: "Privileged helper rejected the managed-user PTY read request."
                )
            )
        }
        var queued = outputChunksBySessionID[request.sessionID] ?? []
        let data = queued.isEmpty ? Data() : queued.removeFirst()
        outputChunksBySessionID[request.sessionID] = queued
        let final = data.isEmpty && exitObservationsBySessionID[request.sessionID]?.final == true
        return .success(
            AlanManagedUserPTYOutputChunk(
                sessionID: request.sessionID,
                data: data,
                final: final,
                sanitizedMessage: data.isEmpty ? nil : "Privileged helper returned PTY output."
            )
        )
    }

    func writeManagedUserPTY(_ request: AlanManagedUserPTYInputRequest) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .writeManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper accepted PTY input."
        )
        if result.accepted {
            writtenPTYInputRequests.append(request)
        }
        return result
    }

    func resizeManagedUserPTY(_ request: AlanManagedUserPTYResizeRequest) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .resizeManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper resized PTY session."
        )
        if result.accepted {
            resizedPTYRequests.append(request)
        }
        return result
    }

    func closeManagedUserPTYInput(sessionID: String) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .closeManagedUserPTYInput,
            sessionID: sessionID,
            successMessage: "Privileged helper closed PTY input."
        )
        if result.accepted {
            closedPTYInputSessionIDs.append(sessionID)
        }
        return result
    }

    func signalManagedUserPTY(
        _ request: AlanManagedUserPTYSignalRequest
    ) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .signalManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper signaled PTY session."
        )
        if result.accepted {
            signaledPTYRequests.append(request)
        }
        return result
    }

    func observeManagedUserPTYExit(sessionID: String) -> AlanManagedUserPTYExitObservation? {
        exitObservationsBySessionID[sessionID]
    }

    func terminatePTY(sessionID: String) -> AlanPrivilegedHelperDiagnostic {
        terminatedPTYSessionIDs.append(sessionID)
        exitObservationsBySessionID[sessionID] = AlanManagedUserPTYExitObservation(
            sessionID: sessionID,
            final: true,
            exitCode: nil,
            terminatingSignal: nil,
            sanitizedMessage: "Privileged helper terminated PTY session."
        )
        return diagnostic(
            operation: .terminatePTY,
            accountName: startedPTYSessionAccounts[sessionID],
            code: nil,
            message: "Privileged helper terminated PTY session \(sessionID)."
        )
    }

    func removeManagedUserIntegration(
        _ request: ManagedTerminalAccountRequest
    ) -> ManagedTerminalAccountApplyResult {
        ManagedTerminalAccountApplyResult(
            completedSteps: [.removeManagedTerminalProfile],
            failedStep: nil,
            cancelled: false,
            visibleDiagnostics: ["Privileged helper removed Managed User integration. Credentials redacted."]
        )
    }

    private func controlResult(
        operation: AlanPrivilegedHelperOperation,
        sessionID: String,
        successMessage: String
    ) -> AlanManagedUserPTYControlResult {
        let accountName = startedPTYSessionAccounts[sessionID]
        guard helperStatus.isHealthy, deniedOperation != operation else {
            return .rejected(
                operation: operation,
                channelID: helperStatus.identity.channelID,
                accountName: accountName,
                code: helperStatus.isHealthy ? .unsupportedOperation : .helperUnavailable,
                message: "Privileged helper rejected the managed-user PTY request."
            )
        }
        return .accepted(
            operation: operation,
            channelID: helperStatus.identity.channelID,
            accountName: accountName,
            message: successMessage
        )
    }

    private func diagnostic(
        operation: AlanPrivilegedHelperOperation,
        accountName: String?,
        code: AlanPrivilegedHelperErrorCode?,
        message: String
    ) -> AlanPrivilegedHelperDiagnostic {
        AlanPrivilegedHelperDiagnostic(
            operationID: UUID().uuidString,
            channelID: helperStatus.identity.channelID,
            accountName: accountName,
            operation: operation,
            code: code,
            sanitizedMessage: message
        )
    }
}
