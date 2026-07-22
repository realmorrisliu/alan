import Foundation

@MainActor
struct ShellWorkspacePersistenceStartup {
    let shellState: ShellStateSnapshot
    let coordinator: ShellWorkspacePersistenceCoordinator
    let diagnostics: [String]
}

@MainActor
extension ShellWorkspacePersistenceCoordinator {
    private static var unpinnedTabRetentionTTL: TimeInterval { 12 * 60 * 60 }

    static func prepare(
        windowContext: ShellWindowContext,
        workspaceManifestURL: URL?,
        defaultWorkingDirectory: String?,
        now: Date,
        fileManager: FileManager = .default
    ) -> ShellWorkspacePersistenceStartup {
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
                manifest: loadedManifest,
                now: now,
                ttl: unpinnedTabRetentionTTL
            )
            let retiredTabCount = max(
                loadedManifest.spaces.reduce(0) { $0 + $1.tabs.count }
                    - retainedManifest.spaces.reduce(0) { $0 + $1.tabs.count },
                0
            )
            var diagnostics = startupDiagnostics(for: loadResult.recovery)
            if retainedManifest != loadedManifest {
                do {
                    try store.save(retainedManifest)
                } catch {
                    diagnostics.append(
                        "workspace manifest save failed after shell-core pruning: \(error)"
                    )
                }
            }
            if retiredTabCount > 0 {
                diagnostics.append(
                    "workspace manifest retired \(retiredTabCount) inactive unpinned tab(s)"
                )
            }

            let shellState = try ShellCoreFFIAdapter.shared.materializeContentWorkspaceManifest(
                manifest: retainedManifest,
                defaultWorkingDirectory: workingDirectory,
                now: now
            )
            return ShellWorkspacePersistenceStartup(
                shellState: shellState,
                coordinator: ShellWorkspacePersistenceCoordinator(
                    manifestStore: store,
                    workspaceManifest: retainedManifest
                ),
                diagnostics: diagnostics
            )
        } catch {
            // Shell-core authority failures must leave any decoded valid manifest untouched.
            // Disable manifest persistence for this recovery controller so bootstrap state
            // cannot overwrite the saved manifest before the core dependency is repaired.
            return ShellWorkspacePersistenceStartup(
                shellState: .bootstrapDefault(
                    windowID: windowContext.windowID,
                    workingDirectory: workingDirectory
                ),
                coordinator: ShellWorkspacePersistenceCoordinator(
                    manifestStore: nil,
                    workspaceManifest: nil
                ),
                diagnostics: ["workspace manifest shell-core startup failed: \(error)"]
            )
        }
    }

    private static func startupDiagnostics(
        for recovery: ShellWorkspaceManifestRecovery
    ) -> [String] {
        switch recovery {
        case .loadedExisting:
            return []
        case .createdDefault:
            return ["workspace manifest created default"]
        case .quarantinedCorruptFile(let url):
            return ["workspace manifest corrupt file quarantined: \(url.path)"]
        }
    }
}
