import Foundation

@MainActor
struct ShellPlatformMetadataPreserver {
    private let paneProjection: ShellPaneProjectionService
    private let bootProfileCache: AlanShellBootProfileCache

    init(
        paneProjection: ShellPaneProjectionService,
        bootProfileCache: AlanShellBootProfileCache
    ) {
        self.paneProjection = paneProjection
        self.bootProfileCache = bootProfileCache
    }

    func preservingPlatformMetadata(
        in state: ShellStateSnapshot,
        runtime: (String) -> TerminalHostRuntimeSnapshot
    ) -> ShellStateSnapshot {
        let contentState = state.contentStateProjection()
        let hydratedPanes = state.panes.map { pane in
            guard paneHasTerminalContent(pane, in: contentState, state: state) else {
                return pane
            }
            guard paneProjection.needsBootContextProjection(pane) else { return pane }
            let bootProfile = bootProfileCache.profile(for: pane, shellState: state)
            let paneRuntime = runtime(pane.paneID)
            let projectedContext = paneProjection.projectedContext(
                for: pane,
                bootProfile: bootProfile,
                workingDirectory: pane.cwd ?? bootProfile.workingDirectory,
                processExited: nil,
                lastCommandExitCode: pane.context?.lastCommandExitCode,
                lastMetadataAt: nil,
                activeTaskState: paneRuntime.paneMetadata.activeTaskState,
                existing: pane.context,
                runtime: paneRuntime
            )
            return ShellPane(
                paneID: pane.paneID,
                tabID: pane.tabID,
                spaceID: pane.spaceID,
                launchTarget: pane.launchTarget,
                cwd: pane.terminalProfileID == nil
                    ? pane.cwd ?? bootProfile.workingDirectory
                    : pane.cwd,
                process: pane.process,
                attention: pane.attention,
                context: projectedContext,
                viewport: pane.viewport,
                activity: pane.activity,
                alanBinding: pane.alanBinding,
                terminalProfileID: pane.terminalProfileID
            )
        }

        let hydratedSpaces = state.spaces.map { space in
            ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(in: hydratedPanes.filter { $0.spaceID == space.spaceID }),
                tabs: space.tabs,
                selectedTabID: space.selectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
        }

        return ShellStateSnapshot(
            contractVersion: state.contractVersion,
            windowID: state.windowID,
            focusedSpaceID: state.focusedSpaceID,
            focusedTabID: state.focusedTabID,
            focusedPaneID: state.focusedPaneID,
            spaces: hydratedSpaces,
            panes: hydratedPanes,
            paneSlots: state.paneSlots,
            contents: state.contents,
            quickTerminal: state.quickTerminal
        )
    }

    private func paneHasTerminalContent(
        _ pane: ShellPane,
        in contentState: ShellContentStateSnapshot,
        state: ShellStateSnapshot
    ) -> Bool {
        if let content = contentState.contentMounted(in: pane.paneID) {
            return content.kind == .terminal
        }

        return pane.isQuickTerminalPane
            && state.quickTerminal?.paneID == pane.paneID
    }

    private func strongestAttention(in panes: [ShellPane]) -> ShellAttentionState {
        let now = Date()
        return panes
            .map { shellEffectiveAttention(for: $0, now: now) }
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private func attentionRank(for attention: ShellAttentionState) -> Int {
        switch attention {
        case .idle:
            return 0
        case .active:
            return 1
        case .notable:
            return 2
        case .awaitingUser:
            return 3
        }
    }
}
