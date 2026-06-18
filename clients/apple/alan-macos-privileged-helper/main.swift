import Foundation
import os

let subsystem = Bundle.main.bundleIdentifier ?? "app.alanworks.macos.privileged-helper"
let logger = Logger(subsystem: subsystem, category: "privileged-helper")
let identity = AlanPrivilegedHelperXPCIdentity.current(bundleIdentifier: Bundle.main.bundleIdentifier)
let delegate = AlanPrivilegedHelperXPCListenerDelegate(identity: identity) {
    AlanPrivilegedHelperXPCService(identity: identity)
}
let listener = NSXPCListener(machServiceName: identity.machServiceName)

listener.delegate = delegate
listener.resume()

logger.info("Alan privileged helper started on \(identity.machServiceName, privacy: .public).")
RunLoop.current.run()
