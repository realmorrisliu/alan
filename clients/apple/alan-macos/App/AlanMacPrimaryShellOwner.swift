import Foundation
import SwiftUI

#if os(macOS)

@MainActor
final class AlanMacPrimaryShellOwner: ObservableObject {
    let host: ShellHostController
    let alanOSAttachment: AlanOSAttachmentController
    let alanOSNativeCapabilities: AlanOSNativeCapabilityAdapter

    init(fileManager: FileManager = .default) {
        let windowContext = ShellWindowContext.make(
            fileManager: fileManager,
            windowID: "window_main"
        )
        let resolvedHost = ShellHostController.live(
            fileManager: fileManager,
            windowContext: windowContext
        )
        let resolvedAttachment = AlanOSAttachmentController.shared
        resolvedAttachment.attach()
        let nativeCapabilities = AlanOSNativeCapabilityAdapter.shared
        nativeCapabilities.start(attachment: resolvedAttachment)
        #if canImport(AppIntents)
        ShellAutomationEntityStore.install(snapshotProvider: { [weak resolvedHost] in
            resolvedHost?.shellState
        })
        ShellAutomationIntentStore.install(
            commandHandler: resolvedHost
        )
        #endif
        host = resolvedHost
        alanOSAttachment = resolvedAttachment
        alanOSNativeCapabilities = nativeCapabilities
    }

    deinit {
        let host = host
        Task { @MainActor in
            host.shutdownTerminalRuntimes()
            AlanOSAttachmentController.shared.detach()
            AlanOSNativeCapabilityAdapter.shared.stop()
        }
    }
}
#endif
