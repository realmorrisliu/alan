import Foundation
import SwiftUI

#if os(macOS)

@MainActor
final class AlanMacPrimaryShellOwner: ObservableObject {
    let host: ShellHostController

    init(fileManager: FileManager = .default) {
        let windowContext = ShellWindowContext.make(
            fileManager: fileManager,
            windowID: "window_main"
        )
        let resolvedHost = ShellHostController.live(
            fileManager: fileManager,
            windowContext: windowContext
        )
        #if canImport(AppIntents)
        ShellAutomationEntityStore.install(snapshotProvider: { [weak resolvedHost] in
            resolvedHost?.shellState
        })
        ShellAutomationIntentStore.install(
            commandHandler: resolvedHost
        )
        #endif
        host = resolvedHost
    }

    deinit {
        let host = host
        Task { @MainActor in
            host.shutdownTerminalRuntimes()
        }
    }
}
#endif
