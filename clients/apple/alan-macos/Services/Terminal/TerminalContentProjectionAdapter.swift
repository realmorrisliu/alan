import Foundation

#if os(macOS)
struct TerminalContentProjection {
    let pane: ShellPane
    let processExited: Bool
    let activity: TerminalActivitySnapshot?
}

struct TerminalContentProjectionAdapter {
    private let paneProjection: ShellPaneProjectionService

    init(paneProjection: ShellPaneProjectionService) {
        self.paneProjection = paneProjection
    }

    func projectRuntime(
        _ runtime: TerminalHostRuntimeSnapshot,
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile
    ) -> TerminalContentProjection {
        projectTerminalMetadata(
            runtime.paneMetadata,
            runtime: runtime,
            for: pane,
            bootProfile: bootProfile,
            workingDirectory: runtime.paneMetadata.workingDirectory ?? pane.cwd
        )
    }

    func projectMetadata(
        _ metadata: TerminalPaneMetadataSnapshot,
        runtime: TerminalHostRuntimeSnapshot,
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile
    ) -> TerminalContentProjection {
        projectTerminalMetadata(
            metadata,
            runtime: runtime,
            for: pane,
            bootProfile: bootProfile,
            workingDirectory: metadata.workingDirectory ?? pane.cwd
        )
    }

    func projectAlanBinding(
        _ binding: ShellAlanBinding?,
        runtime: TerminalHostRuntimeSnapshot,
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile
    ) -> TerminalContentProjection {
        let processExited = paneProjection.projectedProcessExited(
            metadataProcessExited: runtime.paneMetadata.processExited,
            surfaceState: runtime.surfaceState
        ) ?? runtime.paneMetadata.processExited
        let projectedBinding = paneProjection.projectedAlanBinding(
            for: pane,
            binding: binding,
            processExited: processExited
        )
        let projectedContext = paneProjection.projectedContext(
            for: pane,
            bootProfile: bootProfile,
            workingDirectory: pane.cwd,
            processExited: nil,
            lastCommandExitCode: pane.context?.lastCommandExitCode,
            lastMetadataAt: nil,
            activeTaskState: runtime.paneMetadata.activeTaskState,
            existing: pane.context,
            runtime: runtime
        )

        let bindingSummary: String?
        if let projectedBinding {
            bindingSummary = projectedBinding.pendingYield
                ? "alan is waiting for user input"
                : "alan run status: \(projectedBinding.runStatus)"
        } else {
            bindingSummary = nil
        }

        let viewport = ShellViewportSnapshot(
            title: pane.viewport?.title,
            summary: bindingSummary ?? pane.viewport?.summary,
            visibleExcerpt: pane.viewport?.visibleExcerpt,
            lastActivityAt: binding?.lastProjectedAt ?? pane.viewport?.lastActivityAt
        )

        return TerminalContentProjection(
            pane: ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: projectedPaneWorkingDirectory(
                    for: pane,
                    workingDirectory: pane.cwd,
                    bootProfile: bootProfile
                ),
                process: pane.process,
                attention: projectedBinding?.pendingYield == true ? .awaitingUser : pane.attention,
                context: projectedContext,
                viewport: viewport,
                activity: pane.activity,
                alanBinding: projectedBinding,
                terminalProfileID: pane.terminalProfileID
            ),
            processExited: processExited,
            activity: pane.activity
        )
    }

    func projectBootContext(
        runtime: TerminalHostRuntimeSnapshot,
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile
    ) -> TerminalContentProjection {
        let processExited = paneProjection.projectedProcessExited(
            metadataProcessExited: nil,
            surfaceState: runtime.surfaceState
        ) ?? false
        let projectedContext = paneProjection.projectedContext(
            for: pane,
            bootProfile: bootProfile,
            workingDirectory: pane.cwd ?? bootProfile.workingDirectory,
            processExited: nil,
            lastCommandExitCode: pane.context?.lastCommandExitCode,
            lastMetadataAt: nil,
            activeTaskState: runtime.paneMetadata.activeTaskState,
            existing: pane.context,
            runtime: runtime
        )
        let projectedBinding = paneProjection.projectedAlanBinding(
            for: pane,
            binding: pane.alanBinding,
            processExited: processExited
        )

        return TerminalContentProjection(
            pane: ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: projectedPaneWorkingDirectory(
                    for: pane,
                    workingDirectory: pane.cwd,
                    bootProfile: bootProfile
                ),
                process: pane.process,
                attention: pane.attention,
                context: projectedContext,
                viewport: pane.viewport,
                activity: pane.activity,
                alanBinding: projectedBinding,
                terminalProfileID: pane.terminalProfileID
            ),
            processExited: processExited,
            activity: pane.activity
        )
    }

    private func projectTerminalMetadata(
        _ metadata: TerminalPaneMetadataSnapshot,
        runtime: TerminalHostRuntimeSnapshot,
        for pane: ShellPane,
        bootProfile: AlanShellBootProfile,
        workingDirectory: String?
    ) -> TerminalContentProjection {
        let processExited = paneProjection.projectedProcessExited(
            metadataProcessExited: metadata.processExited,
            surfaceState: runtime.surfaceState
        ) ?? metadata.processExited
        let projectedBinding = paneProjection.projectedAlanBinding(
            for: pane,
            binding: pane.alanBinding,
            processExited: processExited
        )
        let projectedContext = paneProjection.projectedContext(
            for: pane,
            bootProfile: bootProfile,
            workingDirectory: workingDirectory,
            processExited: metadata.processExited,
            lastCommandExitCode: metadata.lastCommandExitCode,
            lastMetadataAt: metadata.lastUpdatedAt,
            activeTaskState: metadata.activeTaskState,
            existing: pane.context,
            runtime: runtime
        )
        let viewport = paneProjection.projectedViewport(
            current: pane,
            metadata: metadata,
            runtime: runtime
        )
        let projectedActivity = metadata.clearsActivity ? nil : (metadata.activity ?? pane.activity)

        return TerminalContentProjection(
            pane: ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: projectedPaneWorkingDirectory(
                    for: pane,
                    workingDirectory: workingDirectory,
                    bootProfile: bootProfile
                ),
                process: pane.process,
                attention: paneProjection.projectedAttention(
                    metadataAttention: metadata.attention,
                    processExited: processExited,
                    binding: projectedBinding,
                    surfaceState: runtime.surfaceState
                ),
                context: projectedContext,
                viewport: viewport,
                activity: projectedActivity,
                alanBinding: projectedBinding,
                terminalProfileID: pane.terminalProfileID
            ),
            processExited: processExited,
            activity: projectedActivity
        )
    }

    private func projectedPaneWorkingDirectory(
        for pane: ShellPane,
        workingDirectory: String?,
        bootProfile: AlanShellBootProfile
    ) -> String? {
        if pane.terminalProfileID != nil {
            return pane.cwd
        }
        return workingDirectory ?? bootProfile.workingDirectory
    }
}
#endif
