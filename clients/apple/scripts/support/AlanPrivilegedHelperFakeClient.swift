// Script/test support only. The Alan macOS app target must use the real helper client.
import Foundation

final class AlanPrivilegedHelperFakeClient: AlanPrivilegedHelperClienting {
    var helperStatus: AlanPrivilegedHelperStatus
    var diagnosesByAccount: [String: AlanManagedUserDiagnosis]
    private let ptyStateLock = NSLock()
    private var storedDeniedOperation: AlanPrivilegedHelperOperation?
    var deniedOperation: AlanPrivilegedHelperOperation? {
        get {
            ptyStateLock.lock()
            defer { ptyStateLock.unlock() }
            return storedDeniedOperation
        }
        set {
            ptyStateLock.lock()
            storedDeniedOperation = newValue
            ptyStateLock.unlock()
        }
    }
    var appliedPlans: [AlanManagedUserHelperPlan] = []
    var startedPTYRequests: [AlanManagedUserPTYStartRequest] = []
    private var storedReadPTYRequests: [AlanManagedUserPTYReadRequest] = []
    var readPTYRequests: [AlanManagedUserPTYReadRequest] {
        ptyStateLock.lock()
        defer { ptyStateLock.unlock() }
        return storedReadPTYRequests
    }
    private var storedWrittenPTYInputRequests: [AlanManagedUserPTYInputRequest] = []
    var writtenPTYInputRequests: [AlanManagedUserPTYInputRequest] {
        ptyStateLock.lock()
        defer { ptyStateLock.unlock() }
        return storedWrittenPTYInputRequests
    }
    var resizedPTYRequests: [AlanManagedUserPTYResizeRequest] = []
    var closedPTYInputSessionIDs: [String] = []
    var signaledPTYRequests: [AlanManagedUserPTYSignalRequest] = []
    var terminatedPTYSessionIDs: [String] = []
    private var storedExitObservationsBySessionID:
        [String: AlanManagedUserPTYExitObservation] = [:]
    var exitObservationsBySessionID: [String: AlanManagedUserPTYExitObservation] {
        get {
            ptyStateLock.lock()
            defer { ptyStateLock.unlock() }
            return storedExitObservationsBySessionID
        }
        set {
            ptyStateLock.lock()
            storedExitObservationsBySessionID = newValue
            ptyStateLock.unlock()
        }
    }
    private var storedOutputChunksBySessionID: [String: [Data]] = [:]
    var outputChunksBySessionID: [String: [Data]] {
        get {
            ptyStateLock.lock()
            defer { ptyStateLock.unlock() }
            return storedOutputChunksBySessionID
        }
        set {
            ptyStateLock.lock()
            storedOutputChunksBySessionID = newValue
            ptyStateLock.unlock()
        }
    }

    func enqueueOutputChunks(_ chunks: [Data], sessionID: String) {
        ptyStateLock.lock()
        storedOutputChunksBySessionID[sessionID, default: []].append(contentsOf: chunks)
        ptyStateLock.unlock()
    }

    private var foregroundProcessGroupStatesBySessionID:
        [String: [AlanManagedUserPTYForegroundProcessGroupState]] = [:]
    private let foregroundProcessGroupStateLock = NSLock()
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
        ptyStateLock.lock()
        storedReadPTYRequests.append(request)
        ptyStateLock.unlock()
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
        let data = nextOutputChunk(sessionID: request.sessionID)
        let foregroundProcessGroupState = nextForegroundProcessGroupState(
            sessionID: request.sessionID
        )
        let final = data.isEmpty && exitObservationsBySessionID[request.sessionID]?.final == true
        return .success(
            AlanManagedUserPTYOutputChunk(
                sessionID: request.sessionID,
                data: data,
                final: final,
                foregroundProcessGroupState: foregroundProcessGroupState,
                sanitizedMessage: data.isEmpty ? nil : "Privileged helper returned PTY output."
            )
        )
    }

    func enqueueForegroundProcessGroupStates(
        _ states: [AlanManagedUserPTYForegroundProcessGroupState],
        sessionID: String
    ) {
        foregroundProcessGroupStateLock.lock()
        foregroundProcessGroupStatesBySessionID[sessionID, default: []]
            .append(contentsOf: states)
        foregroundProcessGroupStateLock.unlock()
    }

    func writeManagedUserPTY(_ request: AlanManagedUserPTYInputRequest) -> AlanManagedUserPTYControlResult {
        let result = controlResult(
            operation: .writeManagedUserPTY,
            sessionID: request.sessionID,
            successMessage: "Privileged helper accepted PTY input."
        )
        if result.accepted {
            ptyStateLock.lock()
            storedWrittenPTYInputRequests.append(request)
            ptyStateLock.unlock()
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

    private func nextForegroundProcessGroupState(
        sessionID: String
    ) -> AlanManagedUserPTYForegroundProcessGroupState {
        foregroundProcessGroupStateLock.lock()
        defer { foregroundProcessGroupStateLock.unlock() }
        guard var states = foregroundProcessGroupStatesBySessionID[sessionID],
              !states.isEmpty
        else {
            return .unavailable
        }
        let state = states.removeFirst()
        foregroundProcessGroupStatesBySessionID[sessionID] = states
        return state
    }

    private func nextOutputChunk(sessionID: String) -> Data {
        ptyStateLock.lock()
        defer { ptyStateLock.unlock() }
        guard var chunks = storedOutputChunksBySessionID[sessionID],
              !chunks.isEmpty
        else {
            return Data()
        }
        let chunk = chunks.removeFirst()
        storedOutputChunksBySessionID[sessionID] = chunks
        return chunk
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
