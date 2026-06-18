import Foundation

#if os(macOS)
enum AlanShellPublishedStateMerger {
    static func merge(
        authoritative: ShellStateSnapshot?,
        incoming: ShellStateSnapshot
    ) -> ShellStateSnapshot {
        guard let authoritative else { return incoming }

        // Preserve richer metadata for panes that still exist, but never
        // resurrect panes or tabs that the incoming snapshot removed.
        let authoritativePanesByID = Dictionary(
            uniqueKeysWithValues: authoritative.panes.map { ($0.paneID, $0) }
        )
        let mergedPanes = incoming.panes.map { pane in
            merge(authoritativePane: authoritativePanesByID[pane.paneID], incomingPane: pane)
        }
        let focusedPaneID = incoming.focusedPaneID ?? authoritative.focusedPaneID
        let focusedPane = focusedPaneID.flatMap { candidate in
            mergedPanes.first(where: { $0.paneID == candidate })
        }
        let authoritativeSpacesByID = Dictionary(
            uniqueKeysWithValues: authoritative.spaces.map { ($0.spaceID, $0) }
        )
        let mergedSpaces = incoming.spaces.map { space in
            let authoritativeSpace = authoritativeSpacesByID[space.spaceID]
            return ShellSpace(
                spaceID: space.spaceID,
                title: space.title,
                attention: strongestAttention(in: mergedPanes.filter { $0.spaceID == space.spaceID }),
                tabs: space.tabs,
                selectedTabID: space.selectedTabID ?? authoritativeSpace?.selectedTabID,
                terminalProfileID: space.terminalProfileID,
                presentationIconSystemName: space.presentationIconSystemName
            )
            .repairingSelectedTabID(
                preferredTabID: focusedPane?.spaceID == space.spaceID ? focusedPane?.tabID : nil
            )
        }

        return ShellStateSnapshot(
            contractVersion: incoming.contractVersion,
            windowID: incoming.windowID,
            focusedSpaceID: focusedPane?.spaceID ?? incoming.focusedSpaceID ?? authoritative.focusedSpaceID,
            focusedTabID: focusedPane?.tabID ?? incoming.focusedTabID ?? authoritative.focusedTabID,
            focusedPaneID: focusedPane?.paneID ?? focusedPaneID,
            spaces: mergedSpaces,
            panes: mergedPanes,
            paneSlots: incoming.paneSlots,
            contents: incoming.contents
        )
    }

    private static func merge(
        authoritativePane: ShellPane?,
        incomingPane: ShellPane
    ) -> ShellPane {
        guard let authoritativePane else { return incomingPane }

        return ShellPane(
            paneID: incomingPane.paneID,
            tabID: incomingPane.tabID,
            spaceID: incomingPane.spaceID,
            launchTarget: incomingPane.launchTarget ?? authoritativePane.launchTarget,
            cwd: incomingPane.cwd ?? authoritativePane.cwd,
            process: incomingPane.process ?? authoritativePane.process,
            attention: incomingPane.attention,
            context: merge(authoritativeContext: authoritativePane.context, incomingContext: incomingPane.context),
            viewport: merge(authoritativeViewport: authoritativePane.viewport, incomingViewport: incomingPane.viewport),
            activity: incomingPane.activity,
            alanBinding: incomingPane.alanBinding ?? authoritativePane.alanBinding,
            terminalProfileID: incomingPane.terminalProfileID ?? authoritativePane.terminalProfileID
        )
    }

    private static func merge(
        authoritativeContext: ShellContextSnapshot?,
        incomingContext: ShellContextSnapshot?
    ) -> ShellContextSnapshot? {
        guard authoritativeContext != nil || incomingContext != nil else { return nil }
        let workingDirectoryName =
            incomingContext?.workingDirectoryName ?? authoritativeContext?.workingDirectoryName
        let repositoryRoot =
            incomingContext?.repositoryRoot ?? authoritativeContext?.repositoryRoot
        let gitBranch = incomingContext?.gitBranch ?? authoritativeContext?.gitBranch
        let controlPath = incomingContext?.controlPath ?? authoritativeContext?.controlPath
        let socketPath = incomingContext?.socketPath ?? authoritativeContext?.socketPath
        let alanBindingFile =
            incomingContext?.alanBindingFile ?? authoritativeContext?.alanBindingFile
        let launchCommand =
            incomingContext?.launchCommand ?? authoritativeContext?.launchCommand
        let launchStrategy =
            incomingContext?.launchStrategy ?? authoritativeContext?.launchStrategy
        let incomingTerminalProfileResolution = incomingContext?.terminalProfileState != nil
        let terminalProfileState =
            incomingContext?.terminalProfileState ?? authoritativeContext?.terminalProfileState
        let terminalProfileRequestedID = incomingTerminalProfileResolution
            ? incomingContext?.terminalProfileRequestedID
            : authoritativeContext?.terminalProfileRequestedID
        let terminalProfileID = incomingTerminalProfileResolution
            ? incomingContext?.terminalProfileID
            : authoritativeContext?.terminalProfileID
        let terminalProfileKind = incomingTerminalProfileResolution
            ? incomingContext?.terminalProfileKind
            : authoritativeContext?.terminalProfileKind
        let terminalProfileTitle = incomingTerminalProfileResolution
            ? incomingContext?.terminalProfileTitle
            : authoritativeContext?.terminalProfileTitle
        let shellIntegrationSource =
            incomingContext?.shellIntegrationSource ?? authoritativeContext?.shellIntegrationSource
        let processState = incomingContext?.processState ?? authoritativeContext?.processState
        let rendererPhase = incomingContext?.rendererPhase ?? authoritativeContext?.rendererPhase
        let rendererHealth =
            incomingContext?.rendererHealth ?? authoritativeContext?.rendererHealth
        let surfaceReadiness =
            incomingContext?.surfaceReadiness ?? authoritativeContext?.surfaceReadiness
        let inputReady = incomingContext?.inputReady ?? authoritativeContext?.inputReady
        let readonly = incomingContext?.readonly ?? authoritativeContext?.readonly
        let terminalMode = incomingContext?.terminalMode ?? authoritativeContext?.terminalMode
        let terminalGridDiagnostics =
            incomingContext?.terminalGridDiagnostics
            ?? authoritativeContext?.terminalGridDiagnostics
        let displayName = incomingContext?.displayName ?? authoritativeContext?.displayName
        let displayID = incomingContext?.displayID ?? authoritativeContext?.displayID
        let windowTitle = incomingContext?.windowTitle ?? authoritativeContext?.windowTitle
        let lastMetadataAt =
            incomingContext?.lastMetadataAt ?? authoritativeContext?.lastMetadataAt
        let lastCommandExitCode =
            incomingContext?.lastCommandExitCode ?? authoritativeContext?.lastCommandExitCode

        return ShellContextSnapshot(
            workingDirectoryName: workingDirectoryName,
            repositoryRoot: repositoryRoot,
            gitBranch: gitBranch,
            controlPath: controlPath,
            socketPath: socketPath,
            alanBindingFile: alanBindingFile,
            launchCommand: launchCommand,
            launchStrategy: launchStrategy,
            terminalProfileState: terminalProfileState,
            terminalProfileRequestedID: terminalProfileRequestedID,
            terminalProfileID: terminalProfileID,
            terminalProfileKind: terminalProfileKind,
            terminalProfileTitle: terminalProfileTitle,
            shellIntegrationSource: shellIntegrationSource,
            processState: processState,
            rendererPhase: rendererPhase,
            rendererHealth: rendererHealth,
            surfaceReadiness: surfaceReadiness,
            inputReady: inputReady,
            readonly: readonly,
            terminalMode: terminalMode,
            terminalGridDiagnostics: terminalGridDiagnostics,
            displayName: displayName,
            displayID: displayID,
            windowTitle: windowTitle,
            lastMetadataAt: lastMetadataAt,
            lastCommandExitCode: lastCommandExitCode
        )
    }

    private static func merge(
        authoritativeViewport: ShellViewportSnapshot?,
        incomingViewport: ShellViewportSnapshot?
    ) -> ShellViewportSnapshot? {
        guard authoritativeViewport != nil || incomingViewport != nil else { return nil }
        return ShellViewportSnapshot(
            title: incomingViewport?.title ?? authoritativeViewport?.title,
            summary: incomingViewport?.summary ?? authoritativeViewport?.summary,
            visibleExcerpt: incomingViewport?.visibleExcerpt ?? authoritativeViewport?.visibleExcerpt,
            lastActivityAt: incomingViewport?.lastActivityAt ?? authoritativeViewport?.lastActivityAt
        )
    }

    private static func strongestAttention(in panes: [ShellPane]) -> ShellAttentionState {
        panes
            .map(\.attention)
            .max(by: { attentionRank(for: $0) < attentionRank(for: $1) })
            ?? .idle
    }

    private static func attentionRank(for attention: ShellAttentionState) -> Int {
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
#endif
