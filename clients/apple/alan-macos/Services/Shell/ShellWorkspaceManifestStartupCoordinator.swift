import Foundation

enum ShellWorkspaceStartupMode {
    case fresh
    case restorePrevious
    case workspaceManifest
}

@MainActor
struct ShellWorkspaceManifestStartupResult {
    let shellState: ShellStateSnapshot
    let manifestStore: ShellWorkspaceManifestStore?
    let workspaceManifest: ShellContentWorkspaceManifest?
    let manifestRecovery: ShellWorkspaceManifestRecovery?
    let retiredTabCount: Int
    let diagnostics: [String]
    let shouldPersistInitialShellState: Bool
}

@MainActor
struct ShellWorkspaceManifestStartupCoordinator {
    private static let unpinnedTabRetentionTTL: TimeInterval = 12 * 60 * 60

    private let fileManager: FileManager

    init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
    }

    func prepare(
        mode: ShellWorkspaceStartupMode,
        windowContext: ShellWindowContext,
        workspaceManifestURL: URL?,
        defaultWorkingDirectory: String?,
        now: Date
    ) -> ShellWorkspaceManifestStartupResult {
        switch mode {
        case .fresh:
            return ShellWorkspaceManifestStartupResult(
                shellState: .bootstrapDefault(windowID: windowContext.windowID),
                manifestStore: nil,
                workspaceManifest: nil,
                manifestRecovery: nil,
                retiredTabCount: 0,
                diagnostics: [],
                shouldPersistInitialShellState: true
            )
        case .restorePrevious:
            let shellState =
                ShellStatePersistenceStore.restoreShellState(
                    fileManager: fileManager,
                    persistenceURL: windowContext.persistenceURL,
                    channel: windowContext.installChannel
                )
                ?? .bootstrapDefault(windowID: windowContext.windowID)
            return ShellWorkspaceManifestStartupResult(
                shellState: shellState,
                manifestStore: nil,
                workspaceManifest: nil,
                manifestRecovery: nil,
                retiredTabCount: 0,
                diagnostics: [],
                shouldPersistInitialShellState: false
            )
        case .workspaceManifest:
            return prepareWorkspaceManifestStartup(
                windowContext: windowContext,
                workspaceManifestURL: workspaceManifestURL,
                defaultWorkingDirectory: defaultWorkingDirectory,
                now: now
            )
        }
    }

    private func prepareWorkspaceManifestStartup(
        windowContext: ShellWindowContext,
        workspaceManifestURL: URL?,
        defaultWorkingDirectory: String?,
        now: Date
    ) -> ShellWorkspaceManifestStartupResult {
        let workingDirectory = defaultWorkingDirectory
            ?? fileManager.homeDirectoryForCurrentUser.path
        let store = ShellWorkspaceManifestStore(
            fileManager: fileManager,
            manifestURL: workspaceManifestURL
                ?? ShellWorkspaceManifestStore.defaultManifestURL(
                    windowID: windowContext.windowID,
                    fileManager: fileManager,
                    channel: windowContext.installChannel
                )
        )
        do {
            let loadResult = try store.loadOrCreateDefault(
                windowID: windowContext.windowID,
                defaultWorkingDirectory: workingDirectory,
                now: now
            )
            let loadedManifest = loadResult.manifest
            let retainedManifest = try ShellCoreFFIAdapter.shared.pruningExpiredTabs(
                manifest: loadResult.manifest,
                now: now,
                ttl: Self.unpinnedTabRetentionTTL
            )
            let prunedRetiredTabCount = max(
                loadedManifest.spaces.reduce(0) { $0 + $1.tabs.count }
                    - retainedManifest.spaces.reduce(0) { $0 + $1.tabs.count },
                0
            )
            var diagnostics: [String] = []
            if retainedManifest != loadedManifest {
                do {
                    try store.save(retainedManifest)
                } catch {
                    diagnostics.append(
                        "workspace manifest save failed after shell-core pruning: \(error)"
                    )
                }
            }
            let materializedState = try ShellCoreFFIAdapter.shared.materializeContentWorkspaceManifest(
                manifest: retainedManifest,
                defaultWorkingDirectory: workingDirectory,
                now: now
            )
            return ShellWorkspaceManifestStartupResult(
                shellState: materializedState,
                manifestStore: store,
                workspaceManifest: retainedManifest,
                manifestRecovery: loadResult.recovery,
                retiredTabCount: prunedRetiredTabCount,
                diagnostics: diagnostics,
                shouldPersistInitialShellState: false
            )
        } catch {
            // Shell-core authority failures must leave any decoded valid manifest untouched.
            // Disable manifest persistence for this recovery controller so bootstrap state
            // cannot overwrite the saved manifest before the core dependency is repaired.
            return ShellWorkspaceManifestStartupResult(
                shellState: .bootstrapDefault(
                    windowID: windowContext.windowID,
                    workingDirectory: workingDirectory
                ),
                manifestStore: nil,
                workspaceManifest: nil,
                manifestRecovery: nil,
                retiredTabCount: 0,
                diagnostics: ["workspace manifest shell-core startup failed: \(error)"],
                shouldPersistInitialShellState: false
            )
        }
    }
}
