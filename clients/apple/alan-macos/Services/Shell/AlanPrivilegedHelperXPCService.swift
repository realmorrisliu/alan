import Foundation

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

private func encodePayload<T: Encodable>(_ payload: T) -> Data? {
    try? JSONEncoder().encode(payload)
}
