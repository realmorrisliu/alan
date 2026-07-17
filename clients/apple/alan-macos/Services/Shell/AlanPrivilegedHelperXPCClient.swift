import Foundation

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
