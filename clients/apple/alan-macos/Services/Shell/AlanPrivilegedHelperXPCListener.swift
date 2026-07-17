import Foundation

protocol AlanPrivilegedHelperConnectionCleanup {
    func cleanupConnectionSessions()
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
