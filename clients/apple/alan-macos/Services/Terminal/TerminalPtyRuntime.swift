#if os(macOS)
import Darwin
import Foundation

@MainActor
final class AlanDarwinTerminalPtyRuntime: AlanTerminalPtyRuntime {
    private var handlesByContentID: [String: AlanTerminalPtyHandle] = [:]
    private let managedUserPtyProvider: AlanManagedUserPtyProviding

    init(
        managedUserPtyProvider: AlanManagedUserPtyProviding? = nil
    ) {
        self.managedUserPtyProvider = managedUserPtyProvider ?? AlanUnavailableManagedUserPtyProvider()
    }

    var registeredContentIDs: Set<String> {
        Set(handlesByContentID.keys)
    }

    func handle(
        forTerminalContentID contentID: String,
        bootRequest: AlanTerminalBootRequest
    ) -> AlanTerminalPtyHandle {
        if let existing = handlesByContentID[contentID] {
            return existing
        }
        let handle: AlanTerminalPtyHandle
        if bootRequest.strategy == .terminalProfileManagedUser {
            handle = managedUserPtyProvider.handle(
                forTerminalContentID: contentID,
                bootRequest: bootRequest
            )
        } else {
            handle = AlanDarwinTerminalPtyHandle(
                contentID: contentID,
                bootRequest: bootRequest
            )
        }
        handlesByContentID[contentID] = handle
        return handle
    }

    func existingHandle(forTerminalContentID contentID: String) -> AlanTerminalPtyHandle? {
        handlesByContentID[contentID]
    }

    func snapshot(forTerminalContentID contentID: String) -> AlanTerminalPtyRuntimeSnapshot? {
        handlesByContentID[contentID]?.snapshot
    }

    func unregisterHandle(forTerminalContentID contentID: String) {
        handlesByContentID.removeValue(forKey: contentID)
    }
}

func setNoSigpipeSocketOption(_ fileDescriptor: Int32) {
    var enabled: Int32 = 1
    _ = setsockopt(
        fileDescriptor,
        SOL_SOCKET,
        SO_NOSIGPIPE,
        &enabled,
        socklen_t(MemoryLayout<Int32>.size)
    )
}

#endif
